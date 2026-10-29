//! OperationCore256 and the Edge/Count/InventoryDelta graph of one group,
//! derived from the stable Job/Status observations and the physical native
//! order, never copied from a candidate (obligation.md §6.2-6.5).
use super::{
    super::{Error, Operation, Result, decode_core},
    cells::{self, put, put16},
};
use crate::storage::{transaction_record::TxnRef, undo_record::UndoLink, undo_ref::UndoRef};

/// Edge roles of the Missing-only Row graph.
const ROW_UNDO: u8 = 1;
const UNDO_PREVIOUS_TRANSACTION: u8 = 4;
const ROW_STAMP: u8 = 7;
const TRANSACTION_HEAD: u8 = 10;
pub(super) const AUX_BYTES: usize = 4 * 128 + 2 * 96 + 208;

#[derive(Clone, Copy, Default)]
pub(super) struct Core {
    pub(super) statement: u64,
    pub(super) mutation: u64,
    pub(super) sequence: u64,
    pub(super) current: Option<UndoRef>,
    pub(super) next: Option<UndoRef>,
    pub(super) target: Option<UndoRef>,
    pub(super) q: u64,
    pub(super) cursors: (u64, u64),
    pub(super) digests: Option<([u8; 32], [u8; 32])>,
}

pub(super) fn encode_core(operation: Operation, core: &Core, output: &mut [u8; 256]) -> Result<()> {
    output.fill(0);
    put16(output, 0, operation as u16);
    let mut flags = 0u16;
    for (bit, at, reference) in [
        (0, 32, core.current),
        (1, 72, core.next),
        (2, 112, core.target),
    ] {
        if let Some(reference) = reference {
            flags |= 1 << bit;
            reference.encode(&mut output[at..at + 40])?;
        }
    }
    if let Some((before, after)) = core.digests {
        flags |= 24;
        output[176..208].copy_from_slice(&before);
        output[208..240].copy_from_slice(&after);
    }
    put16(output, 2, flags);
    cells::put32(output, 4, 256);
    put(output, 8, core.statement);
    put(output, 16, core.mutation);
    put(output, 24, core.sequence);
    put(output, 152, core.q);
    put(output, 160, core.cursors.0);
    put(output, 168, core.cursors.1);
    decode_core(output, operation)?;
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) struct Aux {
    pub(super) bytes: [u8; AUX_BYTES],
    pub(super) len: usize,
}
impl Aux {
    pub(super) fn new() -> Self {
        Self {
            bytes: [0; AUX_BYTES],
            len: 0,
        }
    }
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Undo(UndoLink),
    Transaction,
}
fn encode_target(actor: TxnRef, target: Option<Target>, output: &mut [u8]) -> Result<()> {
    match target {
        None => Ok(()),
        Some(Target::Undo(link)) => cells::encode_target_undo(link, output),
        Some(Target::Transaction) => cells::encode_target_txn(actor, output),
    }
}

/// Native indexes of the reference sources/targets in physical order.
#[derive(Clone, Copy)]
pub(super) struct Indices {
    pub(super) status: u16,
    pub(super) undo: u16,
    pub(super) leaf: u16,
}

/// Forward adds RowUndo/RowStamp/TxnHead(H→U)/U.prevTxn(→H); CLR and Prune
/// remove them (TxnHead U→H). U counts 0↔2 and T counts incoming±1. H is
/// net0 and gets no CountChange; its home proof is checked by the caller.
pub(super) fn references(
    operation: Operation,
    actor: TxnRef,
    undo: UndoLink,
    previous: Option<UndoLink>,
    incoming: u64,
    indices: Indices,
    aux: &mut Aux,
) -> Result<()> {
    let adding = match operation {
        Operation::Forward => true,
        Operation::Clr | Operation::Prune => false,
        _ => return Err(Error::InvalidInput),
    };
    let u = Some(Target::Undo(undo));
    let h = previous.map(Target::Undo);
    let toward = |value: Option<Target>| if adding { (None, value) } else { (value, None) };
    let mut edges = [(0u16, 0u8, None, None); 4];
    edges[0] = (
        indices.status,
        TRANSACTION_HEAD,
        if adding { h } else { u },
        if adding { u } else { h },
    );
    let (old, new) = toward(u);
    edges[1] = (indices.leaf, ROW_UNDO, old, new);
    let (old, new) = toward(Some(Target::Transaction));
    edges[2] = (indices.leaf, ROW_STAMP, old, new);
    let count = if h.is_some() {
        let (old, new) = toward(h);
        edges[3] = (indices.undo, UNDO_PREVIOUS_TRANSACTION, old, new);
        4
    } else {
        3
    };
    edges[..count].sort_unstable_by_key(|edge| (edge.0, edge.1));
    let mut at = aux.len;
    for (index, role, old, new) in edges[..count].iter().copied() {
        let edge = &mut aux.bytes[at..at + 128];
        edge.fill(0);
        put16(edge, 0, index);
        edge[2] = role;
        encode_target(actor, old, &mut edge[8..64])?;
        encode_target(actor, new, &mut edge[64..120])?;
        at += 128;
    }
    let after = if adding {
        incoming.checked_add(1)
    } else {
        incoming.checked_sub(1)
    }
    .ok_or(Error::OrderMismatch)?;
    let counts = [
        (
            Target::Undo(undo),
            if adding { (0, 2) } else { (2, 0) },
            indices.undo,
        ),
        (Target::Transaction, (incoming, after), indices.status),
    ];
    for (target, (before, after), index) in counts {
        let count = &mut aux.bytes[at..at + 96];
        count.fill(0);
        encode_target(actor, Some(target), &mut count[..56])?;
        put(count, 56, before);
        put(count, 64, after);
        put16(count, 72, index);
        at += 96;
    }
    aux.len = at;
    Ok(())
}

/// Reserve absent→item, Forward item→absent Consume, Cancel item→absent Release.
pub(super) fn inventory(operation: Operation, item: &[u8; 96], aux: &mut Aux) -> Result<()> {
    let action = match operation {
        Operation::Reserve => 1,
        Operation::Forward => 2,
        Operation::Cancel => 3,
        _ => return Err(Error::InvalidInput),
    };
    let at = aux.len;
    let delta = &mut aux.bytes[at..at + 208];
    delta.fill(0);
    delta[0] = action;
    delta[1] = if action == 1 { 2 } else { 1 };
    let item_at = if action == 1 { 112 } else { 16 };
    delta[item_at..item_at + 96].copy_from_slice(item);
    aux.len = at + 208;
    Ok(())
}
