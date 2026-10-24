//! Actual plan3 creator; external checker verifies every byte and effect.
use super::super::native::{Trace, Writer, db_fds, hex};
use super::*;
use std::{fs, path::PathBuf};
use xadb_foundation::namespace::DirectoryPort;
use xadb_platform::namespace::LinuxDirectory;

#[test]
#[ignore = "explicit native plan3 creator with a fresh output directory"]
fn undo_bootstrap_native_p1() {
    let output = PathBuf::from(std::env::var("XADB_UNDO_BOOTSTRAP_NATIVE_DIR").unwrap());
    assert!(output.is_absolute());
    let database = output.join("database");
    fs::create_dir(&database).unwrap();
    let directory = LinuxDirectory::open_existing(&database).unwrap();
    let trace = Arc::new(Mutex::new(Trace::default()));
    let writer = Writer {
        inner: directory.acquire_writer().unwrap(),
        trace: Arc::clone(&trace),
    };
    drop(directory);
    assert_eq!(db_fds(&database), 2);
    let mut slot = port::slot();
    let fresh =
        boot::FreshStorageSession::create_undo(writer, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let identity = {
        let role = fresh.undo_role().unwrap();
        assert_eq!(
            (
                role.object().get(),
                role.generation().get(),
                role.kind(),
                role.level(),
                role.issued_object_high_water()
            ),
            (3, 1, PageKind::Undo, 0, 3)
        );
        assert!(role.uses_fence(&fresh.core.wal.fenced));
        assert_eq!(role.namespace().timeline, 1);
        let status = fresh.status_role().unwrap();
        assert_eq!(
            (
                status.object().get(),
                status.kind(),
                status.issued_object_high_water()
            ),
            (2, PageKind::TransactionStatus, 3)
        );
        assert_eq!(status.namespace(), role.namespace());
        hex(&role.namespace().database.bytes())
    };
    assert_eq!(trace.lock().unwrap().handles, 1);
    let live_fds = db_fds(&database);
    assert_eq!(live_fds, 3);
    drop(fresh);
    assert_eq!(db_fds(&database), 0);
    let trace = trace.lock().unwrap();
    assert_eq!((trace.events.len(), trace.handles, trace.peak), (350, 0, 3));
    fs::write(output.join("effects.tsv"), trace.events.join("\n") + "\n").unwrap();
    fs::write(
        output.join("role.tsv"),
        format!("P1\t1\t{identity}\t1\t3\t1\t8\t0\t3\t350\t3\t0\t{live_fds}\t0\n"),
    )
    .unwrap();
    println!("\nXADB_UNDO_BOOTSTRAP_NATIVE P1");
}
