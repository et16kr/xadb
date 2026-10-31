//! Rollback writer admission after both resident readers return, at −1/0/+1.
use super::*;
use crate::transaction::concurrency::{
    self,
    catalog_test_support::{self as hook, Point, Resume},
};
use std::sync::mpsc::sync_channel;
use std::time::Duration;
const WAIT: Duration = Duration::from_secs(10);
fn admission(over: i64) {
    case("S4-V09", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                // Reserve/Cancel funds the empty failed statement's three
                // control steps while preserving its empty head.
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(p) => p,
                    _ => panic!("resident"),
                };
                let input =
                    validate_insert(&[ValueRef::Int64(7)], preflight.validation_lane().unwrap())
                        .unwrap();
                let reserved = applied!(preflight.reserve(input));
                applied!(reserved.cancel());
                let shared = writer.catalog_test_commit_charges()[0];
                let charge = writer.catalog_test_retirement_charge();
                let resident = reader.catalog_test_fixed_charges()[2];
                let used = reader.catalog_test_source_used();
                let held = ((65536 - shared - charge) as i64 - used as i64 - over) as usize;
                let mut second_private = boxed::<PRIVATE_BYTES>();
                let mut late_private = boxed::<PRIVATE_BYTES>();
                let before = super::super::row_forward::files(state);
                reader.catalog_test_with_held_source(held,||{
                std::thread::scope(|scope|{
                    let reader_ref=&reader;
                    let mut threads=Vec::new();let mut releases=Vec::new();
                    for private in [&mut *reader_private,&mut *second_private] {
                        let(entered,captured)=sync_channel(1);let(resume,release)=sync_channel(1);
                        hook::install(core,Point::LookupCaptured,entered,release);
                        threads.push(scope.spawn(move||{
                            let mut owner=core.try_owner(OwnerRole::Foreground).unwrap();
                            let snapshot=reader_ref.catalog_snapshot(&mut owner).unwrap();
                            assert!(matches!(reader_ref.lookup_catalog_resident(&mut owner,&snapshot,ObjectId::new(7).unwrap(),&mut Plan4ResidencyCursor::new(),private).unwrap(),CatalogLookupStep::Missing));
                            snapshot.release_checked().unwrap();owner.check_idle().unwrap();
                        }));
                        captured.recv_timeout(WAIT).unwrap();releases.push(resume);
                    }
                    let(entered,waiting)=sync_channel(1);let(resume_wait,release)=sync_channel(1);
                    hook::install(core,Point::ReadGateWaiting,entered,release);
                    let controlling=scope.spawn(move||{
                        waiting.recv_timeout(WAIT).unwrap();
                        let at_wait=hook::observe(core);assert_eq!(at_wait.active_read_operations,2);assert!(at_wait.read_gate_closed);
                        assert_eq!(reader_ref.catalog_test_source_used(),used+held as u64+2*resident as u64);
                        let mut late=core.try_owner(OwnerRole::Foreground).unwrap();let snapshot=reader_ref.catalog_snapshot(&mut late).unwrap();
                        let error=reader_ref.lookup_catalog_resident(&mut late,&snapshot,ObjectId::new(7).unwrap(),&mut Plan4ResidencyCursor::new(),&mut late_private).err().unwrap();
                        assert!(matches!(error.cause,RetentionCause::Concurrency(error) if error.cause==concurrency::Cause::InstallationPending));
                        snapshot.release_checked().unwrap();late.check_idle().unwrap();
                        let admitted=(over>=0).then(||{let(entered,observed)=sync_channel(1);let(resume,release)=sync_channel(1);hook::install(core,Point::RowPreflightResident,entered,release);(observed,resume)});
                        resume_wait.send(Resume::Continue).unwrap();
                        let mut releases=releases.into_iter();let mut threads=threads.into_iter();
                        releases.next().unwrap().send(Resume::Continue).unwrap();threads.next().unwrap().join().unwrap();
                        if let Some((observed,_))=&admitted {assert!(observed.recv_timeout(Duration::from_millis(200)).is_err());} else {std::thread::sleep(Duration::from_millis(200));}
                        assert_eq!(hook::observe(core).active_read_operations,1);assert!(hook::observe(core).read_gate_closed);
                        assert_eq!(reader_ref.catalog_test_source_used(),used+held as u64+resident as u64);
                        releases.next().unwrap().send(Resume::Continue).unwrap();threads.next().unwrap().join().unwrap();
                        if let Some((observed,resume))=admitted {observed.recv_timeout(WAIT).unwrap();assert_eq!(hook::observe(core).active_read_operations,0);assert_eq!(reader_ref.catalog_test_source_used(),used+held as u64+charge as u64);resume.send(Resume::Continue).unwrap();}
                    });
                    let result=writer.start_row_rollback(owner,&row,Some(&view),RollbackScope::Statement,private,&mut flush,flusher);
                    controlling.join().unwrap();
                    match result {
                        Err(error)=>{assert_eq!(over,-1);assert_eq!(error.effect,Effect::Unchanged);assert!(matches!(error.cause,RetentionCause::Capacity));assert_eq!(super::super::row_forward::files(state),before);},
                        Ok(mut rollback)=>{assert!(over>=0);assert!(matches!(rollback.next().unwrap(),RollbackStep::Complete));applied!(rollback.finish());},
                    }
                    assert_eq!(reader.catalog_test_source_used(),used+held as u64);assert!(!hook::observe(core).read_gate_closed);
                });
            });
                assert!(writer.finish_statement(owner, view).is_ok());
                let mut rollback = writer
                    .start_row_rollback(
                        owner,
                        &row,
                        None,
                        RollbackScope::Transaction,
                        private,
                        &mut flush,
                        flusher,
                    )
                    .unwrap();
                assert!(matches!(rollback.next().unwrap(), RollbackStep::Complete));
                applied!(rollback.finish());
                assert!(writer.finish_aborted(owner, row).is_ok());
                assert!(!fence.is_fenced());
                assert_eq!(reader.catalog_test_source_used(), 0);
                println!("S4_V09 over={over} charge={charge} resident={resident} two_returns=true");
            }
        );
    });
}
#[test]
fn s4_v09_minus_one() {
    admission(-1);
}
#[test]
fn s4_v09_exact() {
    admission(0);
}
#[test]
fn s4_v09_plus_one() {
    admission(1);
}
