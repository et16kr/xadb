//! Independently instantiated Row types at every original Commit window.
//! These observations assert the fixed charges; they never enlarge them.
use super::super::catalog_live::{
    self,
    lifecycle::row::{PreparedRowGroup, RowCommitImage, RowCommitSummary},
};
use super::*;
use std::mem::size_of;

#[test]
fn s3_c01_row_commit_windows_and_actual_a_caller_fit_fixed_charges() {
    inspect::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
    #[cfg(target_os = "linux")]
    inspect::<super::super::super::super::native::Writer>("native-type");
}

fn inspect<W: WriterNamespace + 'static>(kind: &str) {
    let [shared, stage, pending, install, publish] =
        source::CatalogCommitCharges::new::<W>().observations();
    let a = crate::storage::row::insert::driver::live::commit_caller_source::<W>();
    let prepare = catalog_live::row_commit_prepare_source_bytes::<W>();
    let phases = borrowed_source_phases_for::<PreparedRowGroup>(
        catalog_live::row_commit_control_source_bytes(),
        visibility::row_commit_source_bytes(),
    );
    let [parent, broad_wait, broad_drain] = pending_window_for::<PreparedRowGroup>();
    let wait = terminal::catalog_wait_source_observation::<W>()
        + 2 * size_of::<crate::storage::wal_io::Result<DurableCommit<'_, '_>>>()
        + size_of::<Result<DurableCommit<'_, '_>>>()
        + size_of::<crate::storage::wal_io::WalIoError>()
        + size_of::<RetentionCause>()
        + size_of::<RetentionError>()
        + 4 * size_of::<&()>();
    assert!(
        wait <= broad_wait,
        "actual backend wait must fit the original closure"
    );
    let drain = size_of::<DurableCommit<'_, '_>>()
        + dirty_frame::borrowed_drain_source_observation::<TARGETS>()
        + size_of::<Result<dirty_frame::DrainedCommitBorrow<'_, '_, '_, '_, '_, '_, '_, TARGETS>>>(
        )
        + size_of::<RetentionCause>()
        + size_of::<RetentionError>()
        + 4 * size_of::<&()>();
    assert!(
        drain <= broad_drain,
        "actual Drain must fit original core envelope"
    );
    let handoff = size_of::<Result<CommitPublication<'_, '_, RowCommitSummary<'_>>>>();
    // The handoff Result owns its packet, moved once through publication;
    // destructuring and forwarding its frontier do not construct another.
    let publication_parent = handoff
        + size_of::<visibility::CommitEpochs>()
        + 8 * size_of::<&()>()
        + 2 * size_of::<Result<RowCommitSummary<'_>>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + size_of::<visibility::VisibilityError>()
        + size_of::<RetentionCause>()
        + size_of::<RetentionError>()
        + 4 * size_of::<&()>();
    let leaf = source::commit_leaf_source_observation();
    let retained = source::commit_retained_source_observation();
    assert_eq!(retained + leaf, source::commit_carrier_bytes());
    let publication = publication_parent
        + visibility::commit_publication_source_bytes().max(source::leaf_source_bytes());
    let carriers = retained
        + size_of::<crate::storage::transaction_record::TxnRef>()
        + size_of::<&mut [u8; 425_984]>()
        + transaction::row_commit::terminal_caller_bytes()
        + a.1;
    let actual = [
        prepare.max(phases.preflight).max(phases.stage).max(leaf),
        parent + wait.max(drain).max(leaf) + handoff,
        phases.install.max(parent + leaf) + handoff,
        publication,
    ]
    .map(|v| v + carriers);
    let fixed = [stage, pending, install, publish];
    let catalog = borrowed_source_phases_for::<PreparedCatalog>(
        catalog_live::catalog_control_source_bytes(),
        visibility::epoch_transition_source_bytes(),
    );
    let catalog_install = catalog.install
        + size_of::<Result<CommitPublication<'_, '_, Observation>>>()
        + source::commit_carrier_bytes()
        + transaction::catalog::terminal_caller_bytes()
        + size_of::<crate::storage::transaction_record::TxnRef>()
        + size_of::<&mut [u8; 425_984]>();
    println!("S3_C01_CATALOG borrowed_install={catalog_install} fixed={install}");
    assert!(catalog_install <= install);

    println!(
        "S3_C01_PARTS parent={parent} wait={wait} broad_wait={broad_wait} drain={drain} broad_drain={broad_drain} handoff={handoff} publication={publication} retained={retained} leaf={leaf} carriers={carriers}"
    );
    let checks = transaction::row_commit::check_source_bytes() + a.1;
    let progress = source::progress_charge_bytes::<W>();
    type Pre<W> = crate::storage::disk::PreflightRow<
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        'static,
        W,
    >;
    let finish = catalog_live::row_write_bodies::<W>().finish
        + size_of::<Pre<W>>()
        + 2 * size_of::<Result<&mut [u8; 425_984]>>()
        + 3 * size_of::<Result<()>>();
    let statement = transaction::catalog::check_source_bytes();
    assert!(finish + 11_520 <= progress && statement <= progress);
    println!(
        "S3_C01_END preflight_finish={finish} caller_bound=11520 statement_end={statement} fixed_progress={progress}"
    );

    let lifecycle = catalog_live::catalog_live_source_bytes::<W>().row_lifecycle;
    assert!(
        lifecycle <= progress,
        "shared Row control helper {lifecycle} exceeds fixed {progress}"
    );
    let (transaction_handle, statement_handle) = transaction::handle_census();
    assert!(transaction_handle <= 512 && statement_handle <= 512);
    println!(
        "S3_C01_FINISH row_lifecycle={lifecycle} progress={progress} transaction_handle={transaction_handle} statement_handle={statement_handle}"
    );
    println!(
        "S3_C01 {kind} shared={shared} prepare={prepare} phases={phases:?} row={actual:?} fixed={fixed:?} progress_actual={checks} progress_fixed={progress} a={a:?} capsule={:?} image={} prepared={} epoch={} catalog_caller={} row_caller={}",
        transaction::handle_census(),
        size_of::<RowCommitImage>(),
        size_of::<PreparedRowGroup>(),
        size_of::<visibility::CommitEpochs>(),
        transaction::catalog::terminal_caller_bytes(),
        transaction::row_commit::terminal_caller_bytes()
    );
    assert!(a.0 <= a.1 && a.1 <= 11_520);
    for (index, (actual, fixed)) in actual.into_iter().zip(fixed).enumerate() {
        assert!(
            actual <= fixed,
            "window {index}: Row {actual} exceeds fixed {fixed}"
        );
        assert!(shared + fixed <= 65_536);
    }
    assert!(
        checks <= progress,
        "Row Progress {checks} exceeds {progress}"
    );
    assert!(shared + progress <= 65_536);
    assert!(size_of::<RowCommitImage>() + 512 <= size_of::<PreparedRowGroup>());
    gate_waits::<W>(kind, a.1);
}

/// WRITER-PROGRESS-001 (S9, boundaries `#row-m4-s9`): the Row capsule's gate
/// waits under A's Commit call and A's statement end. During each wait the
/// writer holds no charge; shared and two resident readers are admitted
/// beside it. Each floor is an upper bound through slack like S7's (iv):
/// A's share at the call (`a.1` for `transaction.commit`, the S2-e caller's
/// frame-static share at `insert.finish_statement`), then the carriers the
/// class charge counts in every window (`commit_carrier_bytes` with the
/// gate's close/wait closure, the inputs and the consuming caller for the
/// Commit gate; the check entry for RowProgress). With A's Write bound
/// (11520 B) instead of the actual share no RowProgress floor could fit:
/// 11520 + shared + two residents alone exceeds the cap.
fn gate_waits<W: WriterNamespace + 'static>(kind: &str, a: usize) {
    let [shared, _, resident] = source::CatalogSourceCharges::new::<W>().observations();
    let readers = shared + 2 * resident;
    let gate = source::commit_carrier_bytes();
    let entry =
        transaction::row_commit::check_entry_bytes().max(transaction::catalog::check_entry_bytes());
    let end = crate::storage::row::insert::driver::live::statement_end_caller_source::<W>();
    let floors = [
        (
            "transaction.commit/commit_gate",
            a + gate
                + size_of::<crate::storage::transaction_record::TxnRef>()
                + size_of::<&mut [u8; 425_984]>()
                + transaction::row_commit::terminal_caller_bytes(),
        ),
        ("transaction.commit/row_progress_check", a + entry),
        ("insert.finish_statement/row_progress_check", end + entry),
    ];
    for (site, floor) in floors {
        let waiting = floor + readers;
        println!(
            "S3_C01_GATE {kind} site={site} floor={floor} a_commit={a} a_statement_end={end} \
             check_entry={entry} gate={gate} shared_plus_two_resident={readers} \
             floor_plus_shared_plus_two_resident={waiting} cap_margin={}",
            65_536 - waiting as i64
        );
        assert!(waiting <= 65_536, "{kind} {site}: {waiting}");
    }
    assert_eq!(
        size_of::<visibility::CommitEpochs>(),
        48,
        "Row discriminator uses existing padding"
    );
}
