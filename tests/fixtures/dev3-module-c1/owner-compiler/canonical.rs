#![allow(dead_code)]
use crate::storage::disk::{
    DiskSession, EmptyRetentionControlLoan, IntoRetentionError, MaintenancePort, MutationPort,
    Plan4Recovered, ReadPort,
};
use crate::storage::wal_io::Plan4Pages;
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;

fn transfer<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    old: Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
    loan: EmptyRetentionControlLoan<'a>,
) -> Result<
    DiskSession<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    IntoRetentionError<Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>>,
> {
    old.into_disk_session(owner, loan)
}
fn recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    owner: Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) -> Result<
    DiskSession<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
> {
    owner.into_live()
}
fn split<W: WriterNamespace>(session: &mut DiskSession<'_, '_, '_, '_, '_, '_, '_, '_, W>) {
    let (mutation, read, maintenance) = session.split();
    let _: MutationPort<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> = mutation;
    let _: ReadPort<'_, '_, '_, '_, '_, '_, '_, '_, W> = read;
    let _: MaintenancePort<'_, '_, '_, '_, '_, '_, '_, W> = maintenance;
}
