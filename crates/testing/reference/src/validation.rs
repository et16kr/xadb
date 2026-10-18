//! Validate controller structure before allocating a search frontier.
use std::collections::{BTreeMap, BTreeSet};
use xadbtest_protocol::history::{Action, Cell, Event, History, Key, Mutation, Observation, Row};

const MAX_BYTES: usize = 8 * 1024 * 1024;

fn key_valid(key: &Key) -> bool {
    key.table != 0 && key.row != 0
}

fn row_bytes(row: &Row) -> Option<usize> {
    if row.is_empty() || row.len() > 32 {
        return None;
    }
    let mut bytes = 0usize;
    for cell in row {
        let payload = match cell {
            Cell::Null | Cell::Bool(_) => 1,
            Cell::Int64(_) => 8,
            Cell::Utf8(value) => value.len(),
            Cell::Binary(value) => value.len(),
        };
        if payload > 65_536 {
            return None;
        }
        bytes = bytes.checked_add(payload + 32)?;
    }
    Some(bytes)
}

fn action_bytes(action: &Action) -> Option<usize> {
    match action {
        Action::Read(key) if !key_valid(key) => None,
        Action::Scan(0) => None,
        Action::Statement(mutations) => {
            if mutations.len() > 256 {
                return None;
            }
            let mut bytes = 0usize;
            for mutation in mutations {
                let (key, payload) = match mutation {
                    Mutation::Insert(key, row) | Mutation::Replace(key, row) => {
                        (key, row_bytes(row)?)
                    }
                    Mutation::Delete(key) => (key, 0),
                };
                if !key_valid(key) {
                    return None;
                }
                bytes = bytes.checked_add(payload + 32)?;
            }
            Some(bytes)
        }
        _ => Some(0),
    }
}

fn observation_bytes(observation: &Observation) -> Option<usize> {
    match observation {
        Observation::Row(Some(row)) => row_bytes(row),
        Observation::Scan(rows) => {
            let mut bytes = 0usize;
            for (key, row) in rows {
                if !key_valid(key) {
                    return None;
                }
                bytes = bytes.checked_add(row_bytes(row)? + 32)?;
                if bytes > MAX_BYTES {
                    return None;
                }
            }
            Some(bytes)
        }
        _ => Some(0),
    }
}

pub(super) fn validate(history: &History) -> Result<(), &'static str> {
    if history.events.is_empty() || history.events.len() > 4096 || history.initial.len() > 128 {
        return Err("history event/initial row limit");
    }
    let mut bytes = 0usize;
    for (key, row) in &history.initial {
        if !key_valid(key) {
            return Err("zero row/table identity");
        }
        bytes += row_bytes(row).ok_or("invalid initial row")? + 32;
    }
    let mut pending = BTreeMap::new();
    let mut requests = BTreeSet::new();
    let mut responses = 0;
    for event in &history.events {
        let size = match event {
            Event::Invoke {
                actor,
                request,
                action,
            } => {
                if *actor >= 8 || *request == 0 || !requests.insert(*request) {
                    return Err("actor/request identity invalid or reused");
                }
                if pending.insert(*actor, *request).is_some() {
                    return Err("actor has overlapping requests");
                }
                action_bytes(action).ok_or("invalid operation or input bound")?
            }
            Event::Return {
                actor,
                request,
                observation,
            } => {
                if pending.remove(actor) != Some(*request) {
                    return Err("unmatched/late/duplicate response");
                }
                responses += 1;
                observation_bytes(observation).ok_or("invalid observation or input bound")?
            }
            Event::Crash => {
                pending.clear();
                0
            }
        };
        bytes = bytes
            .checked_add(size + 64)
            .ok_or("history size overflow")?;
        if bytes > MAX_BYTES {
            return Err("history logical input cost exceeds 8 MiB");
        }
    }
    if responses == 0 || !pending.is_empty() {
        return Err("no observations or unclosed request without crash");
    }
    Ok(())
}

// Account owned capacities independently of the input-format budget. BTree node
// allowance deliberately charges a whole 16-slot node plus 1 KiB per entry;
// this is conservative for the pinned toolchain, not a process RSS measurement.
fn row_allocation(row: &Row) -> usize {
    row.capacity() * std::mem::size_of::<Cell>()
        + row
            .iter()
            .map(|cell| match cell {
                Cell::Utf8(value) => value.capacity(),
                Cell::Binary(value) => value.capacity(),
                _ => 0,
            })
            .sum::<usize>()
}

pub(super) fn tree_entry_bytes<T>() -> usize {
    1024 + 16 * std::mem::size_of::<T>()
}

pub(super) fn rows_bytes(rows: &xadbtest_protocol::history::Rows) -> usize {
    rows.len() * tree_entry_bytes::<(Key, Row)>() + rows.values().map(row_allocation).sum::<usize>()
}

pub(super) fn pending_bytes(action: &Action, result: Option<&Observation>) -> usize {
    let input = match action {
        Action::Statement(mutations) => {
            mutations.capacity() * std::mem::size_of::<Mutation>()
                + mutations
                    .iter()
                    .map(|mutation| match mutation {
                        Mutation::Insert(_, row) | Mutation::Replace(_, row) => row_allocation(row),
                        Mutation::Delete(_) => 0,
                    })
                    .sum::<usize>()
        }
        _ => 0,
    };
    let output = match result {
        Some(Observation::Row(Some(row))) => row_allocation(row),
        Some(Observation::Scan(rows)) => {
            rows.capacity() * std::mem::size_of::<(Key, Row)>()
                + rows
                    .iter()
                    .map(|(_, row)| row_allocation(row))
                    .sum::<usize>()
        }
        _ => 0,
    };
    input + output
}
