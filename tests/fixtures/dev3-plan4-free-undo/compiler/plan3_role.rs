#![allow(dead_code)]
use super::*;
fn probe<W: WriterNamespace>(
    mut reserved: ReservedRetentionSystem<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
    old_role: crate::storage::wal_io::UndoRole<'_>,
) {
    reserved.role = old_role; // expected diagnostic
}
