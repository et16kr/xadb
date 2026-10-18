//! Deliberately simple full-copy state; do not share production algorithms here.
use std::collections::BTreeMap;
use xadbtest_protocol::history::{
    Action, CommitFault, Mode, Mutation, Observation, Rejection, Rows,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Transaction {
    mode: Mode,
    version: u64,
    rows: Rows,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Pending {
    pub request: u64,
    pub action: Action,
    pub result: Option<Observation>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Model {
    visible: Rows,
    durable: Rows,
    version: u64,
    fenced: bool,
    active: BTreeMap<u8, Transaction>,
    pub pending: BTreeMap<u8, Pending>,
}

impl Model {
    pub fn retained_bytes(&self) -> usize {
        use crate::validation::{pending_bytes, rows_bytes, tree_entry_bytes};
        tree_entry_bytes::<Self>()
            + rows_bytes(&self.visible)
            + rows_bytes(&self.durable)
            + self
                .active
                .values()
                .map(|transaction| {
                    tree_entry_bytes::<(u8, Transaction)>() + rows_bytes(&transaction.rows)
                })
                .sum::<usize>()
            + self
                .pending
                .values()
                .map(|pending| {
                    tree_entry_bytes::<(u8, Pending)>()
                        + pending_bytes(&pending.action, pending.result.as_ref())
                })
                .sum::<usize>()
    }

    pub fn new(rows: Rows) -> Self {
        Self {
            visible: rows.clone(),
            durable: rows,
            version: 0,
            fenced: false,
            active: BTreeMap::new(),
            pending: BTreeMap::new(),
        }
    }

    pub fn crash(&mut self) {
        self.visible = self.durable.clone();
        self.active.clear();
        self.pending.clear();
        self.fenced = false;
    }

    // Each pending operation linearizes once. Uncertain commit alone branches.
    pub fn linearize(&self, actor: u8) -> Vec<Self> {
        let Some(pending) = self.pending.get(&actor) else {
            return Vec::new();
        };
        if pending.result.is_some() {
            return Vec::new();
        }
        let uncertain = pending.action == Action::Commit(CommitFault::Uncertain);
        let mut outcomes = Vec::new();
        for include_unknown in [false, true]
            .into_iter()
            .take(if uncertain { 2 } else { 1 })
        {
            let mut next = self.clone();
            let observation = next.execute(actor, &pending.action, include_unknown);
            if let Some(request) = next.pending.get_mut(&actor) {
                request.result = Some(observation);
            }
            if !outcomes.contains(&next) {
                outcomes.push(next);
            }
        }
        outcomes
    }

    fn execute(&mut self, actor: u8, action: &Action, include_unknown: bool) -> Observation {
        if self.fenced
            && matches!(
                action,
                Action::Begin(_) | Action::Statement(_) | Action::Commit(_)
            )
        {
            return Observation::Rejected(Rejection::Fenced);
        }
        if let Action::Begin(mode) = action {
            if self.active.contains_key(&actor) {
                return Observation::Rejected(Rejection::AlreadyActive);
            }
            self.active.insert(
                actor,
                Transaction {
                    mode: *mode,
                    version: self.version,
                    rows: self.visible.clone(),
                },
            );
            return Observation::Begun;
        }
        let Some(transaction) = self.active.get_mut(&actor) else {
            return Observation::Rejected(Rejection::NoTransaction);
        };
        match action {
            Action::Read(key) => Observation::Row(transaction.rows.get(key).cloned()),
            Action::Scan(table) => Observation::Scan(
                transaction
                    .rows
                    .iter()
                    .filter(|(key, _)| key.table == *table)
                    .map(|(key, row)| (*key, row.clone()))
                    .collect(),
            ),
            Action::Statement(mutations) => {
                if transaction.mode == Mode::ReadOnly {
                    return Observation::Rejected(Rejection::ReadOnly);
                }
                let mut prepared = transaction.rows.clone();
                for mutation in mutations {
                    match mutation {
                        Mutation::Insert(key, row) => {
                            if prepared.contains_key(key) {
                                return Observation::Rejected(Rejection::DuplicateRow);
                            }
                            prepared.insert(*key, row.clone());
                        }
                        Mutation::Replace(key, row) => {
                            let Some(target) = prepared.get_mut(key) else {
                                return Observation::Rejected(Rejection::RowNotFound);
                            };
                            *target = row.clone();
                        }
                        Mutation::Delete(key) => {
                            if prepared.remove(key).is_none() {
                                return Observation::Rejected(Rejection::RowNotFound);
                            }
                        }
                    }
                }
                transaction.rows = prepared;
                Observation::StatementApplied
            }
            Action::Rollback => {
                self.active.remove(&actor);
                Observation::RolledBack
            }
            Action::Commit(fault) => {
                let Some(transaction) = self.active.remove(&actor) else {
                    return Observation::Rejected(Rejection::NoTransaction);
                };
                if transaction.mode == Mode::ReadOnly {
                    return Observation::Committed;
                }
                if transaction.version != self.version {
                    return Observation::Rejected(Rejection::SerializationConflict);
                }
                if *fault == CommitFault::BeforeAttempt {
                    return Observation::Aborted;
                }
                // Validation must precede equality: a stale RW transaction is
                // rejected even when its private rows have returned to the
                // visible values. Only a logical change advances the version
                // or reaches the uncertain durable-attempt boundary.
                if transaction.rows == self.visible {
                    return Observation::Committed;
                }
                if *fault == CommitFault::Uncertain {
                    self.fenced = true;
                    if include_unknown {
                        self.durable = transaction.rows;
                        self.version += 1;
                    }
                    return Observation::Unknown;
                }
                self.version += 1;
                self.durable = transaction.rows.clone();
                self.visible = transaction.rows;
                Observation::Committed
            }
            // Begin was handled before the active-transaction lookup.
            Action::Begin(_) => Observation::Rejected(Rejection::AlreadyActive),
        }
    }
}
