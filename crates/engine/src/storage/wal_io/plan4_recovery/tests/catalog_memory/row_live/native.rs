//! Actual native Row reads after a committed INSERT. Controller owns paths.
use super::super::super::super::super::plan4_pages::native;
use super::*;
use crate::storage::{
    disk::{ReadStep, TransactionOptions},
    row::insert::driver::live::commit_live,
    row::read::driver::{
        CursorStep, PointRead,
        live::{LiveCall as ReadCall, next_live, open_cursor_live, read_point_live},
    },
};
use foundation::{identity::RowId, runtime::Cancellation};
use std::{fs, path::Path};

#[test]
#[ignore = "bounded S5 native controller supplies isolated paths and identity"]
fn s5r_n01_native_worker() {
    let env = |name| std::env::var(format!("XADB_S5_NATIVE_{name}")).unwrap();
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    let (directory, trace) = native::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    case("S5R-N01", || {
        produce(&directory, &db, || native::native_receipt(&trace).mutations)
    });
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6 && receipt.log_bytes <= 4 * 1024 * 1024);
    fs::write(
        output.join("receipt.tsv"),
        format!(
            "pid\t{}\nhandles\t{}\npeak\t{}\nfd_peak\t{}\nmutations\t{}\nlog_bytes\t{}\n",
            std::process::id(),
            receipt.handles,
            receipt.peak,
            receipt.fd_peak,
            receipt.mutations,
            receipt.log_bytes
        ),
    )
    .unwrap();
}

fn images(db: &Path) -> Vec<(String, u64, [u8; 32])> {
    // Plan4's existing DWB image file is 16,711,680 bytes and the namespace
    // has 15 files. Stream its bytes; do not mistake caller loan size for
    // persistent file size. Bound the controller to 16 files / 32 MiB total.
    let mut result = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(db).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let length = entry.metadata().unwrap().len();
            total += length;
            assert!(length <= 16 * 1024 * 1024 && total <= 32 * 1024 * 1024);
            let mut file = fs::File::open(entry.path()).unwrap();
            let mut buffer = [0u8; 65536];
            let mut digest = foundation::codec::ContentHasher::new();
            let mut observed = 0u64;
            loop {
                let count = std::io::Read::read(&mut file, &mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                observed += count as u64;
                assert!(observed <= length);
                digest.update(&buffer[..count]);
            }
            assert_eq!(observed, length);
            result.push((
                entry.file_name().into_string().unwrap(),
                length,
                digest.digest(),
            ));
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(result.len() <= 16);
    result
}

fn produce<D: DirectoryPort>(directory: &D, db: &Path, mutations: impl Fn() -> usize) {
    let mut slot = boxed::<1_048_576>();
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let storage = FreshStorageSession::create_retention_catalog(
        directory.acquire_writer().unwrap(),
        1,
        &mut slot,
        &mut maintenance,
    )
    .unwrap()
    .install_doublewrite(&mut dwb)
    .unwrap()
    .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut backing = vec![[0xa7; PAGE_BYTES]; 12];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let (prefix, visibility) = maintenance.split_at_mut(12_574_720);
    let (prefix, private) = prefix.split_at_mut(12_148_736);
    let (prefix, isolation) = prefix.split_at_mut(12_144_576);
    let (prefix, epochs) = prefix.split_at_mut(12_142_976);
    let (prefix, control) = prefix.split_at_mut(6_815_744);
    let (reader_private, prefix) = prefix.split_at_mut(PRIVATE_BYTES);
    let replay: &mut [u8; RETENTION_REPLAY_BYTES] = (&mut prefix
        [4_194_304 - PRIVATE_BYTES..5_767_168 - PRIVATE_BYTES])
        .try_into()
        .unwrap();
    let control: &mut [u8; RETENTION_CONTROL_BYTES] = (&mut control[..RETENTION_CONTROL_BYTES])
        .try_into()
        .unwrap();
    let private: &mut [u8; PRIVATE_BYTES] = private.try_into().unwrap();
    let reader_private: &mut [u8; PRIVATE_BYTES] = reader_private.try_into().unwrap();
    let pages = storage
        .bind_plan4_pages(&core, &mut frames, &mut operation)
        .unwrap();
    let mut live = match pages.into_empty_retention(
        &mut owner,
        EmptyRetentionControlLoan {
            control,
            replay,
            isolation: isolation.try_into().unwrap(),
        },
    ) {
        Ok(live) => live,
        Err(IntoRetentionError::Before { error, .. }) | Err(IntoRetentionError::Fatal(error)) => {
            panic!("{error:?}")
        }
    };
    live.enable_catalog_visibility(
        &mut owner,
        VisibilityLoan {
            bytes: visibility.try_into().unwrap(),
            epochs: epochs.try_into().unwrap(),
        },
    )
    .unwrap();
    let root = live.target(Plan4Page::Root).unwrap();
    assert!(matches!(
        live.residency_step(&root, &mut Plan4ResidencyCursor::new())
            .unwrap(),
        RetentionResidencyStep::Resident { .. }
    ));
    assert_eq!(
        live.catalog_aging_start(&mut owner, private)
            .unwrap()
            .header
            .lsn,
        1
    );
    let status = initialize(&mut live, &mut owner, &mut flusher, false);
    let undo = initialize(&mut live, &mut owner, &mut flusher, true);
    let (row_status, _) = create_table(
        &mut live,
        &mut owner,
        &mut flusher,
        &status,
        &undo,
        private,
        true,
    );
    let (mut writer, reader, mut flush) = live.split();
    let tx = writer
        .begin_row_transaction(
            &mut owner,
            IsolationLevel::Serializable,
            &row_status,
            private,
            &mut flush,
        )
        .unwrap();
    let view = writer.begin_statement(&mut owner, &tx).unwrap();
    let owner = &mut owner;
    let flusher = &mut flusher;
    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
    let _ = settle!(writer, owner, view, table, private, flush, flusher);
    let values = [ValueRef::Int64(7)];
    let input: [&[ValueRef<'_>]; 1] = [&values];
    let mut slots = [None];
    {
        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
        assert!(matches!(
            insert_live(&mut call, view, &table, private, &input, &mut slots),
            Ok(1)
        ));
    }
    let row = slots[0].take().unwrap().row();
    {
        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
        let committed = commit_live(&mut call, tx, private)
            .unwrap_or_else(|e| panic!("native commit {:?}", e.failure));
        assert_eq!(committed.q(), 2);
    }
    let before = images(db);
    let mutations_before = mutations();
    let tx = writer
        .begin_transaction(
            owner,
            TransactionOptions {
                read_only: true,
                isolation: IsolationLevel::ReadCommitted,
            },
            private,
        )
        .unwrap();
    let view = writer.begin_statement(owner, &tx).unwrap();
    assert_eq!(view.q().unwrap(), 2);
    let mut residency = Plan4ResidencyCursor::new();
    let cancel = Cancellation::default();
    let mut output = [0xA5; 64];
    let copied = match read_point_live(
        ReadCall {
            port: &reader,
            owner,
            private: reader_private,
            residency: &mut residency,
        },
        &view,
        &table,
        row,
        &mut output,
        &cancel,
    )
    .unwrap()
    {
        ReadStep::Ready(PointRead::Found(copy)) => copy,
        _ => panic!("native committed row"),
    };
    assert_eq!(copied.len(), 13);
    assert_eq!(copied.version().row, row);
    const CANONICAL: [u8; 13] = [1, 0, 0, 0, 2, 7, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(&output[..13], &CANONICAL);
    output.fill(0xA5);
    assert!(matches!(
        read_point_live(
            ReadCall {
                port: &reader,
                owner,
                private: reader_private,
                residency: &mut residency
            },
            &view,
            &table,
            RowId::new(row.get() + 1).unwrap(),
            &mut output,
            &cancel
        )
        .unwrap(),
        ReadStep::Ready(PointRead::Missing)
    ));
    assert_eq!(output, [0xA5; 64]);
    let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
    assert!(
        matches!(next_live(&mut cursor,ReadCall{port:&reader,owner,private:reader_private,residency:&mut residency},&mut output,&cancel).unwrap(),ReadStep::Ready(CursorStep::Row(ref copy)) if copy.version().row==row)
    );
    assert_eq!(&output[..13], &CANONICAL);
    assert!(matches!(
        next_live(
            &mut cursor,
            ReadCall {
                port: &reader,
                owner,
                private: reader_private,
                residency: &mut residency
            },
            &mut output,
            &cancel
        )
        .unwrap(),
        ReadStep::Ready(CursorStep::End)
    ));
    cursor.close().unwrap();
    assert!(writer.finish_statement(owner, view).is_ok());
    assert!(writer.finish_read_only(owner, tx).is_ok());
    assert_eq!(reader.catalog_test_source_used(), 0);
    assert_eq!(images(db), before);
    assert_eq!(mutations(), mutations_before);
    assert!(!storage.session_fence().is_fenced());
    println!(
        "S5_NATIVE_READ q=2 row={} canonical={} mutations_before={} mutations_after={}",
        row.get(),
        hex(&CANONICAL),
        mutations_before,
        mutations()
    );
}
