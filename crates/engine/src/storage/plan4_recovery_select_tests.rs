//! Independent immutable inputs for the new owning genesis selection boundary.
use super::*;
use crate::storage::{
    catalog_record::{self, CatalogExpectation},
    page::{PageIdentity, PageKind},
    retention_genesis::tests as q,
    tree_page::{self, TreeExpectation},
    wal_frame::{self, FrameExpectation, WalStreamIdentity},
    wal_io::plan4_test_support::{Action, Directory, Fault, Key},
};
use std::collections::BTreeMap;
use xadb_foundation::{ErrorCode, identity::ObjectId, namespace::NamespaceErrorKind};

const DATA: &str = "d3-data-0000000100000001";
fn backing<const N: usize>() -> Box<[u8; N]> {
    vec![0xa7; N].into_boxed_slice().try_into().unwrap()
}
fn acquire(
    directory: &Directory,
) -> SelectionResult<Plan4GenesisSelection<wal_io::plan4_test_support::Writer>> {
    Plan4GenesisSelection::acquire(directory, Some(q::namespace()), &mut backing())
}
fn observation(owner: &Plan4GenesisSelection<wal_io::plan4_test_support::Writer>, profile: u32) {
    let o = owner.observation();
    assert_eq!(o.namespace(), q::namespace());
    assert_eq!(o.profile_id(), profile);
    assert_eq!(
        (
            o.bootstrap_plan(),
            o.filemap_generation(),
            o.doublewrite_generation()
        ),
        (4, 1, 1)
    );
    assert_eq!(
        (
            o.issued_object_high_water(),
            o.allocated_pages(),
            o.free_pages(),
            o.free_jobs(),
            o.catalog_entries()
        ),
        (5, 9, 55, 64, 0)
    );
    assert_eq!(
        (o.catalog_anchor().page_no, o.catalog_root().page_no),
        (7, 8)
    );
    assert_eq!(
        o.epoch_digest(),
        &content_digest(&q::reference(profile, wal_io::WAL_NAME))
    );
}
fn after_lease(error: &Plan4SelectError) {
    assert_eq!(error.effect, Effect::CallerMustResolve);
    assert_eq!(error.database_effect, Effect::Unchanged);
    assert_eq!(error.lease_effect, Plan4LeaseEffect::AcquiredMayHaveChanged);
}
fn unchanged(directory: &Directory, before: &BTreeMap<String, Vec<u8>>) {
    let s = directory.0.lock().unwrap();
    assert_eq!(&s.images(), before);
    assert_eq!(
        (s.mutations(), s.entropy_calls, s.writers, s.handles),
        (0, 0, 0, 0)
    );
    assert!(s.peak <= 2);
}
fn reject(directory: &Directory) -> Plan4SelectError {
    let before = directory.0.lock().unwrap().images();
    let error = match acquire(directory) {
        Err(e) => e,
        Ok(_) => panic!("non-genesis input accepted"),
    };
    after_lease(&error);
    unchanged(directory, &before);
    error
}
fn edit(map: &mut BTreeMap<String, Vec<u8>>, name: &str, f: impl FnOnce(&mut Vec<u8>)) {
    f(map.get_mut(name).unwrap());
}
fn page_edit(map: &mut BTreeMap<String, Vec<u8>>, index: usize, f: impl FnOnce(&mut [u8])) {
    edit(map, DATA, |data| {
        let page = &mut data[index * PAGE_BYTES..(index + 1) * PAGE_BYTES];
        f(page);
        q::resign(page);
    });
}
fn allocator_free_from(page: &mut [u8], first: usize) {
    for index in first..9 {
        let at = 10_208 + index * 96;
        page[at..at + 96].fill(0);
        q::put64(page, at + 8, 1);
    }
    q::resign(page);
}
fn old_files(profile: u32, plan: u64) -> BTreeMap<String, Vec<u8>> {
    assert!((1..=3).contains(&plan));
    let mut map = q::files(profile);
    edit(&mut map, DATA, |data| {
        allocator_free_from(&mut data[..PAGE_BYTES], 1);
        data[PAGE_BYTES..].fill(0);
    });
    let digest = content_digest(&map[DATA][..PAGE_BYTES]);
    edit(&mut map, "d3-bootstrap", |raw| {
        q::put64(raw, 120, plan);
        q::put64(raw, 280, plan);
        raw[208..240].copy_from_slice(&digest);
        q::resign(raw);
    });
    map
}
fn system_frame() -> Vec<u8> {
    let mut raw = vec![0; 192];
    raw[..4].copy_from_slice(b"XWF1");
    q::put16(&mut raw, 4, 1);
    raw[6] = 3;
    q::put16(&mut raw, 8, 160);
    raw[12..16].copy_from_slice(&192_u32.to_le_bytes());
    raw[24..40].copy_from_slice(&q::namespace().database.bytes());
    q::put64(&mut raw, 40, 1);
    raw[48..52].copy_from_slice(&1_u32.to_le_bytes());
    q::put64(&mut raw, 56, 1);
    let digest = content_digest(&raw[..128]);
    raw[128..160].copy_from_slice(&digest);
    q::resign(&mut raw);
    let view = wal_frame::decode_frame(
        &raw,
        FrameExpectation {
            stream: WalStreamIdentity {
                database: q::namespace().database,
                timeline: 1,
                writer_epoch: 1,
            },
            previous_lsn: 0,
        },
    )
    .unwrap();
    assert_eq!(view.header().lsn, 1);
    raw
}
fn selected_control(map: &mut BTreeMap<String, Vec<u8>>, raw: &[u8], generation: u64) {
    edit(map, "d3-dwb-0", |bank| {
        bank.fill(0);
        bank[..raw.len()].copy_from_slice(raw);
    });
    edit(map, "DWB", |selector| {
        q::put64(selector, 32, generation);
        q::put64(selector, 88, generation);
        selector[96..100].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        selector[112..144].copy_from_slice(&content_digest(raw));
        q::resign(selector);
    });
}
fn pending(profile: u32) -> Vec<u8> {
    let empty = q::reference(profile, "empty");
    let mut raw = vec![0; 248];
    raw[..128].copy_from_slice(&empty[..128]);
    raw[6] = 1;
    q::put64(&mut raw, 40, 1);
    q::put16(&mut raw, 48, 1);
    q::put64(&mut raw, 56, 1);
    raw[130] = 10;
    q::put64(&mut raw, 136, 0x1_0000_0001);
    q::put64(&mut raw, 152, 1);
    q::put64(&mut raw, 160, 1);
    q::put64(&mut raw, 168, 1);
    q::put64(&mut raw, 176, 1);
    raw[184..216].copy_from_slice(&content_digest(&q::raw("P00")));
    q::resign(&mut raw);
    crate::storage::doublewrite_control::decode_control(
        &raw,
        crate::storage::doublewrite_control::ControlExpectation {
            namespace: q::namespace(),
            generation: 1,
        },
    )
    .unwrap();
    raw
}
fn catalog_leaf() -> Vec<u8> {
    // Independent historical XCT1 table literal, original SHA below. Only its
    // namespace fields are substituted by documented offsets before decoding.
    const HEX: &str = "584354310100010001000000130000000500000000000000070000000000000001000000000000001f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000001d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000013000000030000000000000004000000000000000000000000000000000000000000000000000000000000000a0000000000000001000100540100020069";
    let mut value: Vec<u8> = HEX
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    assert_eq!(
        q::hash_hex(&value),
        "433eae5477405a1cc8b561c13da2975eb58912a97f5da4e74d6928814b9616af"
    );
    value[88..104].copy_from_slice(&q::namespace().database.bytes());
    q::put64(&mut value, 104, 1);
    let object = ObjectId::new(0x13_0000_0001).unwrap();
    catalog_record::decode_catalog_record(
        &value,
        CatalogExpectation {
            namespace: q::namespace(),
            object,
        },
    )
    .unwrap();
    let mut raw = q::raw("P08");
    let length = 4 + 8 + value.len();
    let at = 16_320 - length;
    q::put16(&mut raw, at, 8);
    q::put16(&mut raw, at + 2, value.len() as u16);
    raw[at + 4..at + 12].copy_from_slice(&object.get().to_be_bytes());
    raw[at + 12..16_320].copy_from_slice(&value);
    q::put16(&mut raw, 72, 2);
    q::put16(&mut raw, 74, 136);
    q::put16(&mut raw, 76, at as u16);
    q::put16(&mut raw, 132, at as u16);
    q::put16(&mut raw, 134, length as u16);
    q::resign(&mut raw);
    let view = tree_page::decode_tree_page(
        &raw,
        TreeExpectation {
            identity: PageIdentity {
                database: q::namespace().database,
                page: catalog_page(8),
                owner: ObjectId::new(5).unwrap(),
                owner_generation: Generation::new(1).unwrap(),
            },
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        },
    )
    .unwrap();
    assert_eq!(view.len(), 1);
    raw
}
fn control_mutation(profile: u32, case: usize) -> BTreeMap<String, Vec<u8>> {
    let mut map = q::files(profile);
    match case {
        1 => edit(&mut map, wal_io::WAL_NAME, |raw| {
            raw.pop();
        }),
        2 => edit(&mut map, wal_io::WAL_NAME, |raw| raw.push(0)),
        3 => edit(&mut map, wal_io::WAL_NAME, |raw| raw.extend(system_frame())),
        4 => selected_control(&mut map, &pending(profile), 1),
        5 | 6 => {
            let mut raw = q::reference(profile, "empty");
            q::put64(
                &mut raw,
                if case == 5 { 32 } else { 40 },
                if case == 5 { 2 } else { 1 },
            );
            q::resign(&mut raw);
            selected_control(&mut map, &raw, if case == 5 { 2 } else { 1 });
        }
        7 => edit(&mut map, "d3-dwb-stage", |raw| {
            raw[232] ^= 1;
            q::resign(raw);
        }),
        8 => {
            map.remove("DWB").unwrap();
        }
        9..=15 => {
            map.insert(
                [
                    "CURRENT",
                    "d3-rebase-0",
                    "d3-rebase-1",
                    "d3-current-stage",
                    "d3-writer-stage",
                    "d3-filemap-select-stage",
                    "d3-dwb-select-stage",
                ][case - 9]
                    .into(),
                vec![0],
            );
        }
        16 => edit(&mut map, "d3-bootstrap", |raw| {
            q::put64(raw, 120, 4);
            q::resign(raw);
        }),
        17 => {
            let mut allocator = q::raw("P00");
            allocator_free_from(&mut allocator, 1);
            edit(&mut map, "d3-bootstrap", |raw| {
                raw[208..240].copy_from_slice(&content_digest(&allocator));
                q::resign(raw);
            });
        }
        18 => {
            edit(&mut map, DATA, |data| {
                allocator_free_from(&mut data[..PAGE_BYTES], 7);
                data[7 * PAGE_BYTES..9 * PAGE_BYTES].fill(0);
            });
            let digest = content_digest(&map[DATA][..PAGE_BYTES]);
            edit(&mut map, "d3-bootstrap", |raw| {
                q::put64(raw, 120, 4);
                raw[208..240].copy_from_slice(&digest);
                q::resign(raw);
            });
        }
        19 => edit(&mut map, "d3-bootstrap", |raw| {
            raw[288] = 1;
            q::resign(raw);
        }),
        20 => edit(&mut map, "d3-profile", |raw| {
            raw[16] ^= 0x80;
            q::resign(raw);
        }),
        21 => edit(&mut map, "FILEMAP", |raw| {
            raw[112] ^= 1;
            q::resign(raw);
        }),
        22 => edit(&mut map, "FILEMAP", |raw| {
            q::put64(raw, 32, 2);
            q::resign(raw);
        }),
        23 => edit(&mut map, "WRITER", |raw| {
            raw[96] ^= 1;
            q::resign(raw);
        }),
        24..=26 => edit(
            &mut map,
            [wal_io::WAL_NAME, "d3-bootstrap", "DWB"][case - 24],
            |raw| {
                raw[if case == 26 { 8 } else { 16 }] ^= 0x80;
                q::resign(raw);
            },
        ),
        27 => edit(&mut map, DATA, |raw| {
            raw.pop();
        }),
        28 => edit(&mut map, DATA, |raw| raw.push(0)),
        29 => edit(&mut map, DATA, |raw| {
            raw[3 * PAGE_BYTES..4 * PAGE_BYTES].fill(0)
        }),
        30 => page_edit(&mut map, 8, |raw| q::put64(raw, 64, 1)),
        31 => page_edit(&mut map, 1, |raw| raw[15_840 + 6] = 1),
        32 => edit(&mut map, DATA, |raw| {
            raw[8 * PAGE_BYTES..9 * PAGE_BYTES].copy_from_slice(&catalog_leaf())
        }),
        35 | 36 => edit(
            &mut map,
            if case == 35 {
                "d3-dwb-1"
            } else {
                "d3-dwb-images"
            },
            |raw| raw[0] = 1,
        ),
        _ => panic!("control mutation ID {case}"),
    }
    map
}
fn control_error(case: usize, error: &Plan4SelectError) {
    use Plan4SelectPhase as P;
    let phase = match case {
        1..=3 | 8..=15 | 27..=28 => P::Preflight,
        4..=7 | 26 => P::Doublewrite,
        16..=19 | 21 | 25 => P::Bootstrap,
        20 => P::Profile,
        22 => P::FileMap,
        23 => P::Writer,
        24 => P::Epoch,
        29..=32 => P::Data,
        _ => panic!("error case"),
    };
    assert_eq!(error.phase, phase, "B{case:02}");
    match case {
        1..=3 | 27..=28 => assert!(
            matches!(&error.cause, Plan4SelectCause::Existing(e) if matches!(e.cause, Cause::LengthMismatch))
        ),
        8 => assert!(
            matches!(&error.cause, Plan4SelectCause::Existing(e) if matches!(&e.cause, Cause::Namespace(n) if n.kind == NamespaceErrorKind::NotFound))
        ),
        6 | 9..=15 => assert!(matches!(error.cause, Plan4SelectCause::UnsupportedClosure)),
        4 | 5 | 22 => assert!(
            matches!(&error.cause, Plan4SelectCause::Existing(e) if matches!(e.cause, Cause::UnsupportedClosure))
        ),
        7 => assert!(
            matches!(&error.cause, Plan4SelectCause::Existing(e) if matches!(e.cause, Cause::Control(ControlError::Corrupt)))
        ),
        16..=19 => assert!(matches!(
            error.cause,
            Plan4SelectCause::Genesis(wal_io::Plan4GenesisCause::Control(ControlError::Corrupt))
        )),
        20 | 23 | 24 | 26 => assert!(
            matches!(&error.cause, Plan4SelectCause::Existing(e) if matches!(e.cause, Cause::Control(ControlError::IdentityMismatch)))
        ),
        21 | 30 | 32 => assert!(matches!(
            error.cause,
            Plan4SelectCause::Genesis(wal_io::Plan4GenesisCause::IdentityMismatch)
        )),
        25 => assert!(matches!(
            error.cause,
            Plan4SelectCause::Genesis(wal_io::Plan4GenesisCause::Control(
                ControlError::IdentityMismatch
            ))
        )),
        29 => assert!(matches!(
            error.cause,
            Plan4SelectCause::Genesis(wal_io::Plan4GenesisCause::Retention(
                crate::storage::retention_genesis::RetentionGenesisError::Page(
                    crate::storage::page::PageError::Corrupt
                )
            ))
        )),
        31 => assert!(matches!(
            error.cause,
            Plan4SelectCause::Genesis(wal_io::Plan4GenesisCause::Retention(
                crate::storage::retention_genesis::RetentionGenesisError::Corrupt
            ))
        )),
        _ => panic!("control error case"),
    }
}

#[test]
fn plan4_control_and_closure_inputs() {
    let mut h = q::Harness::new(q::ids("B", 1, 36));
    for case in 1..=36 {
        h.case(&format!("P4-B{case:02}"), || {
            if matches!(case, 33 | 34) {
                let mut map = q::files(1);
                if case == 34 { map.remove("LOCK"); }
                let directory = Directory::from_files(map);
                {
                    let mut s = directory.0.lock().unwrap();
                    s.acquisition_error = Some(NamespaceError::new(NamespaceErrorKind::Io, if case == 33 { Effect::Unchanged } else { Effect::CallerMustResolve }, "plan4 injected acquire"));
                    s.acquisition_error_after_lock = case == 34;
                }
                let before = directory.0.lock().unwrap().images();
                let error = match acquire(&directory) { Err(e) => e, Ok(_) => panic!("acquire fault accepted") };
                assert_eq!(error.phase, Plan4SelectPhase::Lease);
                assert_eq!(error.database_effect, Effect::Unchanged);
                assert_eq!(error.lease_effect, Plan4LeaseEffect::AcquisitionFailed);
                assert_eq!(error.effect, if case == 33 { Effect::Unchanged } else { Effect::CallerMustResolve });
                assert!(matches!(error.cause, Plan4SelectCause::Namespace(n) if n.kind == NamespaceErrorKind::Io && n.context == "plan4 injected acquire"));
                let s = directory.0.lock().unwrap();
                let mut expected = before;
                if case == 34 { expected.insert("LOCK".into(), vec![]); }
                assert_eq!(s.images(), expected);
                assert_eq!((s.lock_creates, s.writers, s.handles), (usize::from(case == 34), 0, 0));
            } else {
                let map = control_mutation(1, case);
                let directory = Directory::from_files(map.clone());
                if case >= 35 { let owner = acquire(&directory).unwrap(); observation(&owner, 1); drop(owner); unchanged(&directory, &map); }
                else { control_error(case, &reject(&directory)); }
            }
        });
    }
    h.finish();
}

#[test]
fn plan4_selection_page_boundaries() {
    let ids = (0..9)
        .flat_map(|page| ['S', 'Z', 'C', 'E'].map(move |mode| format!("P4-R-S{page:02}-{mode}")))
        .collect();
    let mut h = q::Harness::new(ids);
    for page in 0..9 {
        for mode in ['S', 'Z', 'C', 'E'] {
            h.case(&format!("P4-R-S{page:02}-{mode}"), || {
                let map = q::files(1);
                let directory = Directory::from_files(map.clone());
                directory.0.lock().unwrap().fault = Some(Fault {
                    key: Key::new(
                        "read",
                        DATA,
                        (page * PAGE_BYTES) as u64,
                        if page == 0 { 2 } else { 1 },
                    ),
                    action: match mode {
                        'S' => Action::Short(17),
                        'Z' => Action::Zero,
                        'C' => Action::InvalidCount,
                        'E' => Action::PrefixError,
                        _ => unreachable!(),
                    },
                });
                if mode == 'S' {
                    let owner = acquire(&directory).unwrap();
                    observation(&owner, 1);
                    drop(owner);
                    unchanged(&directory, &map);
                } else {
                    let error = reject(&directory);
                    assert_eq!(error.phase, Plan4SelectPhase::Data);
                    match error.cause {
                        Plan4SelectCause::Genesis(wal_io::Plan4GenesisCause::Io(inner)) => {
                            assert_eq!(inner.code, ErrorCode::IoRead);
                            assert_eq!(inner.effect(), Effect::Unchanged);
                        }
                        other => panic!("wrong read boundary {other:?}"),
                    }
                }
                let s = directory.0.lock().unwrap();
                assert_eq!(s.fault_hits, 1);
                if mode == 'E' {
                    assert!(
                        s.events
                            .iter()
                            .any(|e| e.name == DATA && e.error_prefix == 17)
                    );
                }
            });
        }
    }
    h.finish();
}

struct OversizedDirectory;
impl DirectoryPort for OversizedDirectory {
    type Writer = wal_io::plan4_test_support::SizedWriter<65_537>;
    fn acquire_writer(&self) -> xadb_foundation::namespace::NamespaceResult<Self::Writer> {
        panic!("oversized selector reached namespace acquisition")
    }
}

fn profile_page_error(case: usize, cause: &Plan4SelectCause) {
    use crate::storage::retention_genesis::RetentionGenesisError as R;
    use crate::storage::{allocation_page::AllocationError, page::PageError, tree_page::TreeError};
    use wal_io::Plan4GenesisCause as G;
    let Plan4SelectCause::Genesis(cause) = cause else {
        panic!("wrong profile cause {cause:?}")
    };
    match case {
        16 | 18..=22 | 24 | 43..=44 => assert!(matches!(cause, G::IdentityMismatch)),
        23 => assert!(matches!(cause, G::Allocation(AllocationError::Corrupt))),
        45 => assert!(matches!(cause, G::Page(PageError::Corrupt))),
        46..=48 => assert!(matches!(cause, G::Tree(TreeError::Corrupt))),
        _ => {
            let wanted = match case {
                1 | 3..=7 => R::IdentityMismatch,
                8 => R::Page(PageError::KindMismatch),
                9 | 11..=15 => R::Page(PageError::Corrupt),
                25 => R::UnsupportedFormat,
                _ => R::Corrupt,
            };
            assert!(matches!(cause, G::Retention(actual) if *actual == wanted));
        }
    }
}
#[test]
fn plan4_owner_and_old_dispatch_boundaries() {
    let mut h = q::Harness::new(q::ids("O", 1, 21));
    for case in 1..=21 {
        h.case(&format!("P4-O{case:02}"), || {
            match case {
                1..=3 => {
                    let directory = Directory::from_files(old_files(1, case as u64));
                    let error = reject(&directory);
                    assert_eq!(error.phase, Plan4SelectPhase::Bootstrap);
                    assert!(matches!(error.cause, Plan4SelectCause::Genesis(wal_io::Plan4GenesisCause::UnsupportedPlan)));
                }
                4..=7 => {
                    let map = q::files(1);
                    let directory = Directory::from_files(map.clone());
                    if case == 4 {
                        let mut scratch: Box<[[u8; PAGE_BYTES]; 3]> = vec![[0; PAGE_BYTES]; 3].into_boxed_slice().try_into().unwrap();
                        let error = match RecoverySelection::acquire(&directory, Some(q::namespace()), &mut scratch) { Err(e) => e, Ok(_) => panic!("old selection accepted plan4") };
                        assert_eq!(error.phase, Phase::Bootstrap);
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(matches!(error.cause, Cause::UnsupportedBootstrapPlan));
                    } else if case == 5 {
                        let mut work = backing::<262_144>();
                        let error = match wal_io::RecoveryPrefix::acquire(&directory, Some(q::namespace()), &mut work) { Err(e) => e, Ok(_) => panic!("old prefix accepted plan4") };
                        assert_eq!(error.phase, wal_io::RecoveryPrefixPhase::Bootstrap);
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(matches!(error.cause, wal_io::RecoveryPrefixCause::UnsupportedBootstrapPlan));
                    } else {
                        let mut maintenance = backing::<MAINTENANCE_BYTES>();
                        let mut flush = backing::<4_194_304>();
                        let result = if case == 6 { wal_io::ClosedRecovery::acquire(&directory, Some(q::namespace()), &mut maintenance, &mut flush) } else { wal_io::ClosedRecovery::acquire_mixed(&directory, Some(q::namespace()), &mut maintenance, &mut flush) };
                        let error = match result { Err(e) => e, Ok(_) => panic!("old recovery accepted plan4") };
                        assert_eq!(error.error().phase, wal_io::RecoveryPhase::Prefix);
                        assert_eq!(error.error().effect, Effect::CallerMustResolve);
                        assert!(matches!(&error.error().cause, wal_io::RecoveryCause::Prefix(p) if matches!(p.cause, wal_io::RecoveryPrefixCause::UnsupportedBootstrapPlan)));
                        assert_eq!(error.effects().mutation_attempted, 0);
                    }
                    unchanged(&directory, &map);
                }
                8..=14 | 17..=18 | 20 => wal_io::plan4_test_owner_boundary(case),
                15 => {
                    let map = q::files(1);
                    let directory = Directory::from_files(map.clone());
                    let owner = acquire(&directory).unwrap();
                    observation(&owner, 1);
                    let error = match directory.acquire_writer() { Err(e) => e, Ok(_) => panic!("second writer admitted") };
                    assert_eq!((error.kind, error.effect), (NamespaceErrorKind::Busy, Effect::Unchanged));
                    drop(owner);
                    drop(directory.acquire_writer().unwrap());
                    unchanged(&directory, &map);
                    assert_eq!(directory.0.lock().unwrap().acquire_calls, 3);
                }
                16 => {
                    let expected = q::files(1);
                    let mut map = expected.clone();
                    map.remove("LOCK").unwrap();
                    let directory = Directory::from_files(map);
                    let owner = acquire(&directory).unwrap();
                    observation(&owner, 1);
                    drop(owner);
                    let s = directory.0.lock().unwrap();
                    assert_eq!(s.images(), expected);
                    assert_eq!((s.lock_creates, s.writers, s.handles, s.entropy_calls), (1, 0, 0, 0));
                    assert_eq!(s.events.iter().filter(|e| e.name != "LOCK" && matches!(e.op, "create" | "write" | "sync" | "dirsync" | "publish" | "remove")).count(), 0);
                }
                19 => {
                    let directory = Directory::from_files(q::files(1));
                    let mut namespace = q::namespace(); namespace.timeline = 0;
                    let error = match Plan4GenesisSelection::acquire(&directory, Some(namespace), &mut backing()) { Err(e) => e, Ok(_) => panic!("zero timeline accepted") };
                    assert_eq!((error.phase, error.effect, error.database_effect, error.lease_effect), (Plan4SelectPhase::Preflight, Effect::Unchanged, Effect::Unchanged, Plan4LeaseEffect::NotAttempted));
                    assert!(matches!(error.cause, Plan4SelectCause::InvalidInput));
                    let s = directory.0.lock().unwrap();
                    assert_eq!((s.acquire_calls, s.events.len(), s.entropy_calls), (0, 0, 0));
                }
                21 => {
                    let error = match Plan4GenesisSelection::acquire(&OversizedDirectory, Some(q::namespace()), &mut backing()) { Err(e) => e, Ok(_) => panic!("oversized owner accepted") };
                    assert_eq!((error.phase, error.effect, error.database_effect, error.lease_effect), (Plan4SelectPhase::Preflight, Effect::Unchanged, Effect::Unchanged, Plan4LeaseEffect::NotAttempted));
                    assert!(matches!(error.cause, Plan4SelectCause::Capacity));
                }
                _ => unreachable!(),
            }
        });
    }
    h.finish();
}

#[test]
#[ignore = "DEV3 final S5: profile closure expansion"]
fn plan4_s5_profile_closures() {
    let ids = [2, 3]
        .into_iter()
        .flat_map(|profile| {
            ['N', 'B'].into_iter().flat_map(move |kind| {
                (1..=if kind == 'N' { 48 } else { 32 })
                    .map(move |case| format!("P4-SP{profile}-{kind}{case:02}"))
            })
        })
        .collect();
    let mut h = q::Harness::new(ids);
    for profile in [2, 3] {
        for kind in ['N', 'B'] {
            for case in 1..=if kind == 'N' { 48 } else { 32 } {
                h.case(&format!("P4-SP{profile}-{kind}{case:02}"), || {
                    let map = if kind == 'B' {
                        control_mutation(profile, case)
                    } else {
                        let mut map = q::files(profile);
                        let (index, page) = wal_io::plan4_test_mutate_page(case);
                        edit(&mut map, DATA, |raw| {
                            raw[index * PAGE_BYTES..(index + 1) * PAGE_BYTES].copy_from_slice(&page)
                        });
                        map
                    };
                    let error = reject(&Directory::from_files(map));
                    if kind == 'B' {
                        control_error(case, &error);
                    } else {
                        assert_eq!(error.phase, Plan4SelectPhase::Data);
                        profile_page_error(case, &error.cause);
                    }
                });
            }
        }
    }
    h.finish();
}
