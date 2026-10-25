use std::time::Duration;
use test_protocol::history::{
    Action, Cell, CommitFault, Event, History, Key, Mode, Mutation, Observation, Rejection, Row,
    Rows,
};
use test_reference::{SearchBudget, Verdict, check};

fn key(row: u64) -> Key {
    Key { table: 1, row }
}
fn row(value: i64) -> Row {
    vec![Cell::Int64(value)]
}
fn read(value: i64) -> Observation {
    Observation::Row(Some(row(value)))
}

struct Trace {
    history: History,
    request: u64,
}
impl Trace {
    fn new(initial: &[(u64, i64)]) -> Self {
        Self {
            history: History {
                initial: initial
                    .iter()
                    .map(|(id, value)| (key(*id), row(*value)))
                    .collect(),
                events: Vec::new(),
            },
            request: 0,
        }
    }
    fn invoke(&mut self, actor: u8, action: Action) -> u64 {
        self.request += 1;
        self.history.events.push(Event::Invoke {
            actor,
            request: self.request,
            action,
        });
        self.request
    }
    fn reply(&mut self, actor: u8, request: u64, observation: Observation) {
        self.history.events.push(Event::Return {
            actor,
            request,
            observation,
        });
    }
    fn call(&mut self, actor: u8, action: Action, observation: Observation) {
        let request = self.invoke(actor, action);
        self.reply(actor, request, observation);
    }
    fn begin(&mut self, actor: u8) {
        self.call(actor, Action::Begin(Mode::ReadWrite), Observation::Begun);
    }
    fn replace(&mut self, actor: u8, id: u64, value: i64) {
        self.call(
            actor,
            Action::Statement(vec![Mutation::Replace(key(id), row(value))]),
            Observation::StatementApplied,
        );
    }
    fn commit(&mut self, actor: u8) {
        self.call(
            actor,
            Action::Commit(CommitFault::None),
            Observation::Committed,
        );
    }
    fn crash(&mut self) {
        self.history.events.push(Event::Crash);
    }
    fn verdict(&self, expected: Verdict) {
        let result = check(&self.history, SearchBudget::default());
        assert_eq!(result.verdict, expected, "{result:?}");
    }
}

#[test]
fn literal_private_changes_statement_rollback_null_and_absence() {
    let mut t = Trace::new(&[(1, 0)]);
    t.begin(0);
    t.replace(0, 1, 1);
    t.call(
        0,
        Action::Statement(vec![
            Mutation::Replace(key(1), row(2)),
            Mutation::Delete(key(99)),
        ]),
        Observation::Rejected(Rejection::RowNotFound),
    );
    t.call(0, Action::Read(key(1)), read(1));
    t.call(0, Action::Read(key(99)), Observation::Row(None));
    t.call(
        0,
        Action::Statement(vec![Mutation::Insert(key(2), vec![Cell::Null])]),
        Observation::StatementApplied,
    );
    t.call(
        0,
        Action::Read(key(2)),
        Observation::Row(Some(vec![Cell::Null])),
    );
    t.begin(1);
    t.call(1, Action::Read(key(1)), read(0));
    t.call(0, Action::Rollback, Observation::RolledBack);
    t.call(1, Action::Read(key(2)), Observation::Row(None));
    t.verdict(Verdict::Accepted);
}

#[test]
fn input_order_and_duplicate_insert_do_not_partially_change_statement() {
    let mut t = Trace::new(&[]);
    t.begin(0);
    t.call(
        0,
        Action::Statement(vec![
            Mutation::Insert(key(1), row(1)),
            Mutation::Replace(key(1), row(2)),
        ]),
        Observation::StatementApplied,
    );
    t.call(
        0,
        Action::Statement(vec![
            Mutation::Insert(key(2), row(3)),
            Mutation::Insert(key(1), row(4)),
        ]),
        Observation::Rejected(Rejection::DuplicateRow),
    );
    t.call(
        0,
        Action::Scan(1),
        Observation::Scan(vec![(key(1), row(2))]),
    );
    t.commit(0);
    t.verdict(Verdict::Accepted);
}

#[test]
fn lost_update_and_write_skew_known_bad_are_rejected() {
    for skew in [false, true] {
        let mut t = Trace::new(&[(1, 0), (2, 0)]);
        t.begin(0);
        t.begin(1);
        t.call(0, Action::Read(key(if skew { 2 } else { 1 })), read(0));
        t.call(1, Action::Read(key(1)), read(0));
        t.replace(0, 1, 1);
        t.replace(1, if skew { 2 } else { 1 }, 1);
        t.commit(0);
        t.commit(1);
        t.verdict(Verdict::Rejected);
        if let Some(Event::Return { observation, .. }) = t.history.events.last_mut() {
            *observation = Observation::Rejected(Rejection::SerializationConflict);
        }
        t.verdict(Verdict::Accepted);
    }
}

#[test]
fn phantom_and_mixed_snapshot_known_bad_are_rejected() {
    let mut t = Trace::new(&[]);
    t.call(0, Action::Begin(Mode::ReadOnly), Observation::Begun);
    t.call(0, Action::Scan(1), Observation::Scan(vec![]));
    t.begin(1);
    t.call(
        1,
        Action::Statement(vec![Mutation::Insert(key(1), row(1))]),
        Observation::StatementApplied,
    );
    t.commit(1);
    t.call(
        0,
        Action::Scan(1),
        Observation::Scan(vec![(key(1), row(1))]),
    );
    t.verdict(Verdict::Rejected);
    if let Some(Event::Return { observation, .. }) = t.history.events.last_mut() {
        *observation = Observation::Scan(vec![]);
    }
    t.commit(0);
    t.verdict(Verdict::Accepted);
}

#[test]
fn partial_batch_and_ack_loss_known_bad_are_rejected_after_crash() {
    for actual in [
        vec![(key(1), row(1)), (key(2), row(0))],
        vec![(key(1), row(0)), (key(2), row(0))],
    ] {
        let mut t = Trace::new(&[(1, 0), (2, 0)]);
        t.begin(0);
        t.call(
            0,
            Action::Statement(vec![
                Mutation::Replace(key(1), row(1)),
                Mutation::Replace(key(2), row(1)),
            ]),
            Observation::StatementApplied,
        );
        t.commit(0);
        t.crash();
        t.begin(0);
        t.call(0, Action::Scan(1), Observation::Scan(actual));
        t.verdict(Verdict::Rejected);
        if let Some(Event::Return { observation, .. }) = t.history.events.last_mut() {
            *observation = Observation::Scan(vec![(key(1), row(1)), (key(2), row(1))]);
        }
        t.verdict(Verdict::Accepted);
    }
}

#[test]
fn unjustified_unknown_is_not_a_wildcard_for_wrong_results() {
    for (mode, dirty, fault) in [
        (Mode::ReadWrite, true, CommitFault::None),
        (Mode::ReadOnly, false, CommitFault::Uncertain),
        (Mode::ReadWrite, false, CommitFault::Uncertain),
    ] {
        let mut t = Trace::new(&[(1, 0)]);
        t.call(0, Action::Begin(mode), Observation::Begun);
        if dirty {
            t.replace(0, 1, 1);
        }
        t.call(0, Action::Commit(fault), Observation::Unknown);
        t.verdict(Verdict::Rejected);
    }
}

#[test]
fn uncertain_commit_has_one_whole_outcome_across_repeated_crashes() {
    for included in [false, true] {
        let mut t = Trace::new(&[(1, 0), (2, 0)]);
        t.begin(0);
        t.call(
            0,
            Action::Statement(vec![
                Mutation::Replace(key(1), row(1)),
                Mutation::Replace(key(2), row(1)),
            ]),
            Observation::StatementApplied,
        );
        t.call(
            0,
            Action::Commit(CommitFault::Uncertain),
            Observation::Unknown,
        );
        t.call(
            1,
            Action::Begin(Mode::ReadOnly),
            Observation::Rejected(Rejection::Fenced),
        );
        t.crash();
        t.begin(1);
        let value = i64::from(included);
        t.call(
            1,
            Action::Scan(1),
            Observation::Scan(vec![(key(1), row(value)), (key(2), row(value))]),
        );
        t.crash();
        t.begin(1);
        t.call(1, Action::Read(key(1)), read(value));
        t.verdict(Verdict::Accepted);
        if let Some(Event::Return { observation, .. }) = t.history.events.last_mut() {
            *observation = read(1 - value);
        }
        t.verdict(Verdict::Rejected);
    }
}

#[test]
fn pending_unacknowledged_commit_may_be_included_or_absent() {
    for included in [false, true] {
        let mut t = Trace::new(&[(1, 0)]);
        t.begin(0);
        t.replace(0, 1, 1);
        t.invoke(0, Action::Commit(CommitFault::None));
        t.crash();
        t.begin(0);
        t.call(0, Action::Read(key(1)), read(i64::from(included)));
        t.verdict(Verdict::Accepted);
    }
}

#[test]
fn reversed_responses_and_begin_interval_use_legal_linearizations() {
    let mut t = Trace::new(&[(1, 0)]);
    t.begin(0);
    t.replace(0, 1, 1);
    let commit = t.invoke(0, Action::Commit(CommitFault::None));
    let begin = t.invoke(1, Action::Begin(Mode::ReadOnly));
    t.reply(1, begin, Observation::Begun);
    t.call(1, Action::Read(key(1)), read(1));
    t.reply(0, commit, Observation::Committed);
    t.verdict(Verdict::Accepted);
    // Begin completed before commit invocation: observing the new row is impossible.
    let mut t = Trace::new(&[(1, 0)]);
    t.begin(0);
    t.replace(0, 1, 1);
    t.call(1, Action::Begin(Mode::ReadOnly), Observation::Begun);
    t.commit(0);
    t.call(1, Action::Read(key(1)), read(1));
    t.verdict(Verdict::Rejected);
}

#[test]
fn fenced_existing_readers_can_read_and_rollback_but_cannot_write_or_commit() {
    let mut t = Trace::new(&[(1, 0)]);
    t.begin(0);
    t.begin(1);
    t.replace(0, 1, 1);
    t.call(
        0,
        Action::Commit(CommitFault::Uncertain),
        Observation::Unknown,
    );
    t.call(1, Action::Read(key(1)), read(0));
    t.call(
        1,
        Action::Statement(vec![Mutation::Replace(key(1), row(2))]),
        Observation::Rejected(Rejection::Fenced),
    );
    t.call(
        1,
        Action::Commit(CommitFault::None),
        Observation::Rejected(Rejection::Fenced),
    );
    t.call(1, Action::Rollback, Observation::RolledBack);
    t.verdict(Verdict::Accepted);
}

#[test]
fn before_attempt_abort_and_stale_noop_writer_cannot_publish() {
    let mut t = Trace::new(&[(1, 0)]);
    t.begin(0);
    t.begin(1);
    t.replace(0, 1, 1);
    t.call(
        0,
        Action::Commit(CommitFault::BeforeAttempt),
        Observation::Aborted,
    );
    t.call(1, Action::Read(key(1)), read(0));
    t.commit(1);
    t.begin(0);
    t.begin(1);
    t.replace(0, 1, 2);
    t.commit(0);
    t.call(
        1,
        Action::Commit(CommitFault::None),
        Observation::Rejected(Rejection::SerializationConflict),
    );
    t.verdict(Verdict::Accepted);
}

fn net_zero_statements(trace: &mut Trace, form: &str) {
    match form {
        "same_value" => trace.replace(0, 1, 0),
        "empty" => trace.call(0, Action::Statement(vec![]), Observation::StatementApplied),
        "insert_delete" => {
            trace.call(
                0,
                Action::Statement(vec![Mutation::Insert(key(3), row(9))]),
                Observation::StatementApplied,
            );
            trace.call(
                0,
                Action::Statement(vec![Mutation::Delete(key(3))]),
                Observation::StatementApplied,
            );
        }
        "revert" => {
            trace.replace(0, 1, 9);
            trace.replace(0, 1, 0);
        }
        _ => panic!("unknown literal net-zero form"),
    }
}

#[test]
fn logical_net_zero_commit_does_not_conflict_with_an_already_begun_writer() {
    for form in ["same_value", "empty", "insert_delete", "revert"] {
        for fault in [CommitFault::None, CommitFault::Uncertain] {
            let mut t = Trace::new(&[(1, 0), (2, 0)]);
            t.begin(0);
            t.begin(1);
            net_zero_statements(&mut t, form);
            let noop_return = t.history.events.len() + 1;
            t.call(0, Action::Commit(fault), Observation::Committed);
            t.replace(1, 2, 1);
            let writer_return = t.history.events.len() + 1;
            t.commit(1);
            t.crash();
            t.begin(2);
            t.call(
                2,
                Action::Scan(1),
                Observation::Scan(vec![(key(1), row(0)), (key(2), row(1))]),
            );
            t.verdict(Verdict::Accepted);

            // A fictitious version bump rejects a writer whose real starting
            // snapshot is still current. That rejection is observably wrong.
            let original = t.history.events[writer_return].clone();
            if let Event::Return { observation, .. } = &mut t.history.events[writer_return] {
                *observation = Observation::Rejected(Rejection::SerializationConflict);
            } else {
                panic!("literal writer return index changed");
            }
            t.verdict(Verdict::Rejected);
            t.history.events[writer_return] = original;
            if let Event::Return { observation, .. } = &mut t.history.events[noop_return] {
                *observation = Observation::Unknown;
            } else {
                panic!("literal no-op return index changed");
            }
            t.verdict(Verdict::Rejected);
        }
    }
}

#[test]
fn stale_rw_validation_precedes_net_zero_equality_and_fault_selection() {
    for private_value in [0, 1] {
        for fault in [
            CommitFault::None,
            CommitFault::BeforeAttempt,
            CommitFault::Uncertain,
        ] {
            let mut t = Trace::new(&[(1, 0)]);
            t.begin(0);
            t.begin(1);
            t.replace(0, 1, private_value);
            t.replace(1, 1, 1);
            t.commit(1);
            // private_value=0 is unchanged from Begin; =1 equals the newly
            // visible state. Both remain stale RW transactions.
            t.call(
                0,
                Action::Commit(fault),
                Observation::Rejected(Rejection::SerializationConflict),
            );
            t.verdict(Verdict::Accepted);
            for wrong in [
                Observation::Committed,
                Observation::Aborted,
                Observation::Unknown,
            ] {
                if let Event::Return { observation, .. } = t.history.events.last_mut().unwrap() {
                    *observation = wrong;
                } else {
                    panic!("literal final return changed");
                }
                t.verdict(Verdict::Rejected);
            }
        }
    }
}

#[test]
fn failed_statement_preserves_prior_change_and_the_net_zero_commit_decision() {
    for prior in [0, 1] {
        let mut t = Trace::new(&[(1, 0), (2, 0)]);
        t.begin(0);
        t.begin(1);
        t.replace(0, 1, prior);
        t.call(
            0,
            Action::Statement(vec![
                Mutation::Replace(key(1), row(9)),
                Mutation::Delete(key(99)),
            ]),
            Observation::Rejected(Rejection::RowNotFound),
        );
        t.call(0, Action::Read(key(1)), read(prior));
        t.commit(0);
        t.call(1, Action::Read(key(1)), read(0));
        t.replace(1, 2, 1);
        t.call(
            1,
            Action::Commit(CommitFault::None),
            if prior == 0 {
                Observation::Committed
            } else {
                Observation::Rejected(Rejection::SerializationConflict)
            },
        );
        t.crash();
        t.begin(2);
        t.call(2, Action::Read(key(1)), read(prior));
        t.call(2, Action::Read(key(2)), read(1 - prior));
        t.verdict(Verdict::Accepted);
        if let Event::Return { observation, .. } = t.history.events.last_mut().unwrap() {
            *observation = read(prior);
        } else {
            panic!("literal final read changed");
        }
        t.verdict(Verdict::Rejected);
    }
}

#[test]
fn explicit_before_attempt_abort_still_precedes_the_net_zero_shortcut() {
    let mut t = Trace::new(&[(1, 0)]);
    t.begin(0);
    t.begin(1);
    t.replace(0, 1, 0);
    t.call(
        0,
        Action::Commit(CommitFault::BeforeAttempt),
        Observation::Aborted,
    );
    t.replace(1, 1, 1);
    t.commit(1);
    t.crash();
    t.begin(2);
    t.call(2, Action::Read(key(1)), read(1));
    t.verdict(Verdict::Accepted);
}

#[test]
fn malformed_trace_and_empty_assertions_never_pass() {
    let empty = History {
        initial: Rows::new(),
        events: Vec::new(),
    };
    assert_eq!(
        check(&empty, SearchBudget::default()).verdict,
        Verdict::Invalid
    );
    let mut t = Trace::new(&[]);
    let request = t.invoke(0, Action::Begin(Mode::ReadOnly));
    t.verdict(Verdict::Invalid);
    t.reply(1, request, Observation::Begun);
    t.verdict(Verdict::Invalid);
    t.history.events.pop();
    t.crash();
    t.reply(0, request, Observation::Begun);
    t.verdict(Verdict::Invalid);
    let mut t = Trace::new(&[]);
    t.begin(0);
    t.request = 0;
    t.invoke(1, Action::Begin(Mode::ReadWrite));
    t.verdict(Verdict::Invalid);
}

#[test]
fn every_search_budget_is_incomplete_instead_of_false_pass_or_rejection() {
    let mut t = Trace::new(&[(1, 0)]);
    t.begin(0);
    for budget in [
        SearchBudget {
            states: 0,
            ..SearchBudget::default()
        },
        SearchBudget {
            transitions: 0,
            ..SearchBudget::default()
        },
        SearchBudget {
            frontier_states: 0,
            ..SearchBudget::default()
        },
        SearchBudget {
            frontier_bytes: 0,
            ..SearchBudget::default()
        },
        SearchBudget {
            timeout: Duration::ZERO,
            ..SearchBudget::default()
        },
    ] {
        assert_eq!(check(&t.history, budget).verdict, Verdict::Incomplete);
    }
}

#[test]
fn input_limits_are_invalid_and_not_silently_truncated() {
    let mut t = Trace::new(&[]);
    t.begin(0);
    t.call(
        0,
        Action::Statement(vec![Mutation::Insert(
            key(1),
            vec![Cell::Binary(vec![0; 65_537])],
        )]),
        Observation::StatementApplied,
    );
    t.verdict(Verdict::Invalid);
    let mut t = Trace::new(&[]);
    t.begin(8);
    t.verdict(Verdict::Invalid);
    let mut t = Trace::new(&[]);
    t.begin(0);
    t.call(0, Action::Read(key(0)), Observation::Row(None));
    t.verdict(Verdict::Invalid);
}

#[test]
fn retained_mutation_and_scan_buffers_respect_the_frontier_budget() {
    let mut t = Trace::new(&[]);
    t.call(
        0,
        Action::Statement(vec![Mutation::Delete(key(1)); 256]),
        Observation::Rejected(Rejection::NoTransaction),
    );
    // Two pending request variants alone retain 2*256*size_of::<Mutation>().
    assert!(2 * 256 * std::mem::size_of::<Mutation>() > 20_000);
    assert_eq!(
        check(
            &t.history,
            SearchBudget {
                frontier_bytes: 20_000,
                ..SearchBudget::default()
            }
        )
        .verdict,
        Verdict::Incomplete
    );
    t.verdict(Verdict::Accepted);

    // A scan owns a separate vector and separate rows; do not count only keys.
    let mut t = Trace::new(&(1..=100).map(|id| (id, 0)).collect::<Vec<_>>());
    t.begin(0);
    t.call(
        0,
        Action::Scan(1),
        Observation::Scan((1..=100).map(|id| (key(id), row(0))).collect()),
    );
    t.verdict(Verdict::Accepted);
    assert_eq!(
        check(
            &t.history,
            SearchBudget {
                frontier_bytes: 20_000,
                ..SearchBudget::default()
            }
        )
        .verdict,
        Verdict::Incomplete
    );
}

#[test]
fn logical_input_budget_includes_cell_key_and_event_costs() {
    let mut t = Trace::new(&[]);
    // 128*(payload + cell 32 + key 32) + two events*64 = exactly 8 MiB.
    t.history.initial = (1..=128)
        .map(|id| {
            let bytes = if id == 128 { 65_536 - 8_320 } else { 65_536 };
            (key(id), vec![Cell::Binary(vec![0; bytes])])
        })
        .collect();
    t.call(0, Action::Begin(Mode::ReadOnly), Observation::Begun);
    t.verdict(Verdict::Accepted);
    if let Cell::Binary(bytes) = &mut t.history.initial.get_mut(&key(128)).unwrap()[0] {
        bytes.push(0);
    }
    t.verdict(Verdict::Invalid);
}

// Independent deterministic trace generator v1. It emits one serial transaction
// at a time and computes expected scalar values with ordinary integer bookkeeping,
// never by calling the model. Every generated history has exactly `operations` calls.
fn generated(seed: u64, operations: usize) -> Trace {
    let mut t = Trace::new(&[(1, 0)]);
    let mut random = seed;
    let mut committed = 0;
    for _ in 0..operations / 8 {
        random = random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let next = (random >> 1) as i64;
        t.begin(0);
        t.call(0, Action::Read(key(1)), read(committed));
        t.replace(0, 1, next);
        t.call(0, Action::Read(key(1)), read(next));
        if random & 4 == 0 {
            t.call(0, Action::Rollback, Observation::RolledBack);
        } else {
            committed = next;
            t.commit(0);
        }
        t.call(0, Action::Begin(Mode::ReadOnly), Observation::Begun);
        t.call(0, Action::Read(key(1)), read(committed));
        t.commit(0);
    }
    // The final observed value is deliberately not read from the production/model state.
    assert_eq!(t.history.events.len(), operations * 2);
    t
}

#[test]
fn v2_fixed_64_seeds_each_reach_128_operations_and_reject_mutated_observation() {
    for seed in 0..64 {
        let mut t = generated(seed, 128);
        t.verdict(Verdict::Accepted);
        if let Event::Return { observation, .. } = &mut t.history.events[3] {
            *observation = Observation::Row(None);
        }
        t.verdict(Verdict::Rejected);
    }
}

#[test]
fn maximum_length_1024_operations_is_supported() {
    generated(u64::MAX, 1024).verdict(Verdict::Accepted);
}

#[test]
#[ignore = "V3 explicit extended profile; run with --ignored"]
fn v3_fixed_256_seeds_each_reach_1024_operations() {
    for seed in 0..256 {
        generated(seed, 1024).verdict(Verdict::Accepted);
    }
}
