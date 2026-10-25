#![allow(dead_code)]
use crate::storage::wal_io::OperationalStatus;
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut operational: OperationalStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
) {
    let (mut writer, _, _) = operational.split_with_flush();
    let reservation = writer.reserve_one(owner).unwrap();
    let _ = operational.prepare_next_status(); // expected diagnostic
    std::hint::black_box(reservation);
}
