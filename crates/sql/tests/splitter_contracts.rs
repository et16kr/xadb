use xadb_foundation::runtime::{Cancellation, Clock, Quota};
use xadb_sql::frontend::{
    CallControl, Code, Diagnostic, Module, SplitOutcome, Splitter, StatementSource,
};
use xadbtest_reference::frontend::splitter::{
    ActionFixture, ErrorObservation, EventObservation, SplitCase, StatementObservation,
    StepObservation, TraceObservation, all_partitions, check_trace, edge_literals, edge_partitions,
    generated_case, lifecycle_literals, limit_cases, partition_case, partition_literals,
};

struct FixedClock;
impl Clock for FixedClock {
    fn now_nanos(&self) -> u64 {
        0
    }
}

fn source_observation(source: StatementSource) -> StatementObservation {
    StatementObservation {
        bytes: source.as_bytes().to_vec(),
        start: source.span().start,
        end: source.span().end,
        terminated: source.terminated(),
    }
}

fn error_observation(error: Diagnostic) -> ErrorObservation {
    ErrorObservation {
        code: match error.code {
            Code::Syntax => "Syntax",
            Code::Unsupported => "Unsupported",
            Code::Name => "Name",
            Code::Limit => "Limit",
            Code::Cancelled => "Cancelled",
            Code::Timeout => "Timeout",
        },
        module: match error.module {
            Module::Lex => "Lex",
            Module::Name => "Name",
            Module::Split => "Split",
            Module::Parse => "Parse",
        },
        start: error.span.start,
        end: error.span.end,
    }
}

fn assert_case(case: &SplitCase) {
    let session = Quota::new(8 * 1024 * 1024);
    let database = Quota::new(64 * 1024 * 1024);
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    let mut splitter = Splitter::new(&session, &database);
    let mut observed = TraceObservation {
        steps: Vec::new(),
        error: None,
        complete: false,
    };
    for call in &case.calls {
        let result = match &call.action {
            ActionFixture::Push(bytes) => {
                splitter.push(bytes, &control).map(|step| StepObservation {
                    consumed: step.consumed,
                    event: match step.outcome {
                        SplitOutcome::NeedMore => EventObservation::NeedMore,
                        SplitOutcome::Statement(source) => {
                            EventObservation::Statement(source_observation(source))
                        }
                    },
                })
            }
            ActionFixture::Finish => splitter.finish(&control).map(|source| StepObservation {
                consumed: 0,
                event: source.map_or(EventObservation::Eof, |source| {
                    EventObservation::Statement(source_observation(source))
                }),
            }),
        };
        match result {
            Ok(step) => observed.steps.push(step),
            Err(error) => {
                assert_eq!(
                    splitter.push(b"ignored", &control).err(),
                    Some(error),
                    "{} terminal push",
                    case.id
                );
                assert_eq!(
                    splitter.finish(&control).err(),
                    Some(error),
                    "{} terminal finish",
                    case.id
                );
                observed.error = Some(error_observation(error));
                break;
            }
        }
    }
    observed.complete = true;
    if !check_trace(case, &observed) {
        let expected = expected_trace(case);
        let index = observed
            .steps
            .iter()
            .zip(&expected.steps)
            .position(|(actual, wanted)| actual != wanted)
            .unwrap_or(observed.steps.len().min(expected.steps.len()));
        panic!(
            "case={} first-different-step={index} actual={} expected={} step-counts={}/{} errors={:?}/{:?}",
            case.id,
            step_summary(observed.steps.get(index)),
            step_summary(expected.steps.get(index)),
            observed.steps.len(),
            expected.steps.len(),
            observed.error,
            expected.error
        );
    }
    drop(splitter);
    assert_eq!(session.used(), 0, "{} session cleanup", case.id);
    assert_eq!(database.used(), 0, "{} database cleanup", case.id);
}

fn step_summary(step: Option<&StepObservation>) -> String {
    let Some(step) = step else {
        return "absent".into();
    };
    match &step.event {
        EventObservation::NeedMore => format!("NeedMore consumed={}", step.consumed),
        EventObservation::Eof => format!("Eof consumed={}", step.consumed),
        EventObservation::Statement(value) => format!(
            "Statement consumed={} span={}..{} terminated={} bytes={} prefix={:?}",
            step.consumed,
            value.start,
            value.end,
            value.terminated,
            value.bytes.len(),
            &value.bytes[..value.bytes.len().min(16)]
        ),
    }
}

#[test]
fn every_short_literal_byte_partition() {
    let literals = partition_literals();
    assert_eq!(literals.len(), 12);
    let mut schedules = 0;
    for literal in &literals {
        for case in all_partitions(literal) {
            assert_case(&case);
            schedules += 1;
        }
    }
    assert_eq!(schedules, 892);
}

#[test]
fn edge_literal_chunk_boundaries() {
    let literals = edge_literals();
    assert_eq!(literals.len(), 54);
    let mut schedules = 0;
    for literal in &literals {
        for case in edge_partitions(literal) {
            assert_case(&case);
            schedules += 1;
        }
    }
    assert_eq!(schedules, 256);
}

#[test]
fn source_limit_and_reset_boundaries() {
    let cases = limit_cases();
    assert_eq!(cases.len(), 13);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn generated_fragment_streams() {
    for seed in 0..64 {
        assert_case(&generated_case(seed, 128));
    }
}

#[test]
fn empty_push_and_finish_lifecycle() {
    let cases = lifecycle_literals();
    assert_eq!(cases.len(), 1);
    for case in cases {
        assert_case(&case);
    }
}

fn expected_trace(case: &SplitCase) -> TraceObservation {
    let mut trace = TraceObservation {
        steps: Vec::new(),
        error: None,
        complete: true,
    };
    for call in &case.calls {
        match &call.expected {
            Ok(step) => trace.steps.push(step.clone()),
            Err(error) => {
                trace.error = Some(error.clone());
                break;
            }
        }
    }
    trace
}

fn first_statement(trace: &mut TraceObservation) -> &mut StatementObservation {
    trace
        .steps
        .iter_mut()
        .find_map(|step| match &mut step.event {
            EventObservation::Statement(source) => Some(source),
            _ => None,
        })
        .expect("fixed fixture contains a statement")
}

#[test]
fn known_bad_splitter_observations_are_rejected() {
    let short = partition_literals();
    let edges = edge_literals();
    let quote = short
        .iter()
        .find(|item| item.id == "text-semicolon")
        .unwrap();
    let quote_case = partition_case(quote, &[1, quote.source.len()], "mutant-control");
    let cr = short.iter().find(|item| item.id == "line-crlf").unwrap();
    let cr_case = partition_case(cr, &[cr.source.len()], "mutant-control");
    let utf8 = short.iter().find(|item| item.id == "utf8-three").unwrap();
    let utf8_case = partition_case(utf8, &[2, utf8.source.len()], "mutant-control");
    let two = short
        .iter()
        .find(|item| item.id == "two-statements")
        .unwrap();
    let two_case = partition_case(two, &[two.source.len()], "mutant-control");
    let error = edges
        .iter()
        .find(|item| item.id == "prefix-before-invalid")
        .unwrap();
    let error_case = partition_case(error, &[error.source.len()], "mutant-control");
    for case in [&quote_case, &cr_case, &utf8_case, &two_case, &error_case] {
        assert!(check_trace(case, &expected_trace(case)));
    }
    for mutation in 0..16 {
        let case = match mutation {
            0 | 1 => &quote_case,
            2 => &cr_case,
            3 => &utf8_case,
            12..=15 => &error_case,
            _ => &two_case,
        };
        let mut trace = expected_trace(case);
        match mutation {
            0 => {
                let value = first_statement(&mut trace);
                value.bytes = b"'a;".to_vec();
                value.end = 3;
            }
            1 => trace.steps[0].event = EventObservation::Eof,
            2 => first_statement(&mut trace).bytes = b"--;\na;".to_vec(),
            3 => {
                first_statement(&mut trace).bytes.remove(2);
            }
            4 => first_statement(&mut trace).start += 1,
            5 => first_statement(&mut trace).end -= 1,
            6 => trace.steps[0].consumed -= 1,
            7 => first_statement(&mut trace).terminated = false,
            8 => {
                trace.steps.remove(0);
            }
            9 => trace.steps.insert(0, trace.steps[0].clone()),
            10 => trace.steps.swap(0, 2),
            11 => trace.complete = false,
            12 => trace.error.as_mut().unwrap().code = "Limit",
            13 => trace.error.as_mut().unwrap().module = "Lex",
            14 => trace.error.as_mut().unwrap().start += 1,
            15 => trace.steps.clear(),
            _ => unreachable!(),
        }
        assert!(!check_trace(case, &trace), "splitter mutant {mutation}");
    }
}

#[test]
#[ignore = "expanded 256 seeds x1024 fragments; select for relevant expanded acceptance"]
fn extended_generated_fragment_streams() {
    for seed in 0..256 {
        assert_case(&generated_case(seed, 1024));
    }
}

#[test]
#[ignore = "expanded splitter malformed-byte profile; minimum30minutes when selected"]
fn extended_splitter_fuzz_30_minutes() {
    use std::time::{Duration, Instant};
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    let literals = partition_literals();
    let mut state = 0x5841_4442_5350_4c54u64;
    let mut rounds = 0u64;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30 * 60) {
        let source = if rounds.is_multiple_of(2) {
            let mut source = literals[(rounds as usize / 2) % literals.len()]
                .source
                .clone();
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let index = (state as usize) % source.len();
            source[index] = (state >> 32) as u8;
            source
        } else {
            let length = if rounds % 64 == 1 {
                1_048_577
            } else {
                ((state >> 32) as usize) % 4097
            };
            let mut source = vec![0; length];
            for byte in &mut source {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                *byte = (state >> 32) as u8;
            }
            source
        };
        let session = Quota::new(8 * 1024 * 1024);
        let database = Quota::new(64 * 1024 * 1024);
        let mut splitter = Splitter::new(&session, &database);
        let mut position = 0;
        let mut failed = false;
        let mut outputs = 0;
        while position < source.len() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let end = (position + 1 + ((state >> 32) as usize) % 257).min(source.len());
            match splitter.push(&source[position..end], &control) {
                Ok(step) => {
                    assert!(
                        (1..=end - position).contains(&step.consumed),
                        "no progress round{rounds}"
                    );
                    position += step.consumed;
                    if let SplitOutcome::Statement(value) = step.outcome {
                        assert!(value.terminated());
                        assert!(
                            value.span().start <= value.span().end && value.span().end <= position
                        );
                        assert_eq!(
                            value.as_bytes(),
                            &source[value.span().start..value.span().end]
                        );
                        outputs += 1;
                    }
                }
                Err(error) => {
                    assert!(error.span.start <= error.span.end && error.span.end <= source.len());
                    failed = true;
                    break;
                }
            }
            assert!(outputs <= source.len());
        }
        if !failed {
            match splitter.finish(&control) {
                Ok(Some(value)) => {
                    assert!(!value.terminated());
                    assert!(
                        value.span().start <= value.span().end && value.span().end == source.len()
                    );
                    assert_eq!(
                        value.as_bytes(),
                        &source[value.span().start..value.span().end]
                    );
                }
                Ok(None) => {}
                Err(error) => {
                    assert!(error.span.start <= error.span.end && error.span.end <= source.len())
                }
            }
        }
        drop(splitter);
        assert_eq!(session.used(), 0);
        assert_eq!(database.used(), 0);
        rounds += 1;
        if rounds.is_multiple_of(128) {
            assert_case(&generated_case((rounds / 128) % 64, 128));
        }
    }
    assert!(rounds > 0);
    eprintln!(
        "splitterfuzz rounds={rounds} elapsed={:?} seed=5841444253504c54",
        start.elapsed()
    );
}
