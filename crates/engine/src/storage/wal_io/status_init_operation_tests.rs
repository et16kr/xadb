//! WI/WM leaf tests use independently generated literal bytes. The private
//! role seed below is codec setup only, never successful installation evidence.
use super::*;
use crate::storage::{
    allocation_page::SystemWorkRef,
    page::PageRef,
    wal_frame::WalStreamIdentity,
    wal_io::bootstrap::{self as boot, test_support as port},
};
use xadb_foundation::namespace::FileIdentity;

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-init"
);
const GENESIS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/dev3-storage-session/literal"
);

fn literal(profile: u32, cancellations: u64, terminal: &str, work: u64, part: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{INPUT}/p{profile}-cancel{cancellations}-{terminal}/work{work}-{part}"
    ))
    .unwrap()
}
fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
fn page_literal(
    profile: u32,
    cancellations: u64,
    terminal: &str,
    work: u64,
    part: &str,
) -> Box<[u8; PAGE_BYTES]> {
    literal(profile, cancellations, terminal, work, part)
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
fn with_input(
    profile_id: u32,
    cancellations: u64,
    f: impl FnOnce(&InitPlan<'_, '_>, &[u8; PAGE_BYTES], &[u8], &[u8; PAGE_BYTES], &[u8; PAGE_BYTES]),
) {
    let raw = std::fs::read(format!("{GENESIS}/profile{profile_id}-d3-profile.bin")).unwrap();
    let profile =
        crate::storage::control::decode_bootstrap_profile(&raw, Some(port::namespace())).unwrap();
    let mut scratch: Box<[[u8; PAGE_BYTES]; 3]> = vec![[0; PAGE_BYTES]; 3]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    let bootstrap = boot::BootstrapPlan::prepare_for_kind(
        profile,
        &mut scratch,
        boot::BootstrapPlanKind::StatusBootstrap,
    )
    .unwrap();
    let fence = SessionFence::new();
    let role = StatusRole {
        plan: &bootstrap,
        fence: &fence,
    };
    let work = cancellations + 1;
    let birth = cancellations * 2 + 1;
    let allocator = bootstrap.allocation_identity().unwrap();
    let payload = PageIdentity {
        database: allocator.database,
        page: PageRef {
            file: allocator.page.file,
            page_no: 1,
            incarnation: Generation::new(work + 1).unwrap(),
        },
        owner: role.object(),
        owner_generation: role.generation(),
    };
    let before = page_literal(
        profile_id,
        cancellations,
        "init",
        work,
        "allocator.reserved",
    );
    let after = page_literal(profile_id, cancellations, "init", work, "allocator.after");
    let status = page_literal(profile_id, cancellations, "init", work, "status.after");
    let frame = literal(profile_id, cancellations, "init", work, "init.wal");
    let descriptor: Box<[u8; 1536]> = literal(
        profile_id,
        cancellations,
        "init",
        work,
        "reserve.descriptor",
    )
    .into_boxed_slice()
    .try_into()
    .unwrap();
    let offset = 4096 + cancellations * 42400 + 21200;
    let plan = InitPlan {
        profile,
        allocator,
        payload,
        role: &role,
        before_descriptor: &descriptor,
        batch: Batch {
            owner: SystemWorkRef::new(
                profile.spec().namespace,
                SystemWorkKind::AllocationMaintenance,
                work,
                Generation::new(1).unwrap(),
            )
            .unwrap(),
            mask: 2,
            birth,
        },
        prefix: WalPrefix {
            stream: WalStreamIdentity {
                database: allocator.database,
                timeline: 1,
                writer_epoch: 1,
            },
            file: FileIdentity {
                device: 1,
                inode: 1,
            },
            complete_lsn: birth,
            synced_lsn: birth,
            complete_offset: offset,
            synced_offset: offset,
        },
    };
    f(&plan, &before, &frame, &after, &status);
    assert!(
        !fence.is_fenced(),
        "pure codec input never fences a live owner"
    );
}
fn mark(family: &str, id: usize, profile: u32) {
    println!("\nXADB_STATUS_INIT_CASE {family}-{id:02}");
    println!("STATUS_INIT_PROFILE {family}-{id:02} {profile}");
}
fn seal(raw: &mut [u8]) {
    let end = raw.len() - 32;
    let hash = content_digest(&raw[..end]);
    raw[end..].copy_from_slice(&hash);
}
fn replace_allocator(frame: &mut [u8], after: &[u8; PAGE_BYTES]) {
    frame[8480..14688].copy_from_slice(&after[10144..16352]);
    frame[15312..21520].copy_from_slice(&after[10144..16352]);
    frame[2200..2232].copy_from_slice(&content_digest(&after[10144..16352]));
    seal(frame);
}
fn replace_status(frame: &mut [u8], after: &[u8; PAGE_BYTES]) {
    frame[14816..15040].copy_from_slice(&after[16128..16352]);
    frame[14744..14776].copy_from_slice(&content_digest(&after[16128..16352]));
    frame[21584..37968].copy_from_slice(after);
    seal(frame);
}
fn clone_plan<'p, 's>(plan: &InitPlan<'p, 's>) -> InitPlan<'p, 's> {
    InitPlan {
        profile: plan.profile,
        allocator: plan.allocator,
        payload: plan.payload,
        prefix: plan.prefix,
        batch: plan.batch,
        before_descriptor: plan.before_descriptor,
        role: plan.role,
    }
}
fn generated(profile: u32, cancellations: u64) {
    with_input(
        profile,
        cancellations,
        |plan, before, expected, allocator, status| {
            let mut output = boxed::<65536>(0xa5);
            let mut after_a = boxed::<PAGE_BYTES>(0xcc);
            let mut after_s = boxed::<PAGE_BYTES>(0x5a);
            let prepared =
                prepare_init(plan, before, &mut output, &mut after_a, &mut after_s).unwrap();
            assert_eq!(prepared.bytes, expected);
            assert_eq!(prepared.allocator_after(), allocator);
            assert_eq!(prepared.status_after(), status);
            assert_eq!(
                prepared.descriptor_bytes().as_slice(),
                literal(
                    profile,
                    cancellations,
                    "init",
                    cancellations + 1,
                    "init.descriptor"
                )
            );
            assert_eq!(
                observe_initialized(
                    plan.profile,
                    plan.allocator,
                    plan.payload,
                    plan.role,
                    allocator,
                    status
                ),
                Ok(PageLsn(plan.batch.birth + 1))
            );
            assert!(output[38000..].iter().all(|b| *b == 0xa5));
        },
    );
}
fn profile_frame(profile: u32, cancel: bool) {
    with_input(profile, 0, |plan, reserved, _, _, _| {
        let original = page_literal(profile, 0, "init", 1, "allocator.before");
        let expected = literal(
            profile,
            0,
            if cancel { "cancel" } else { "init" },
            1,
            if cancel { "cancel.wal" } else { "reserve.wal" },
        );
        let after = page_literal(
            profile,
            0,
            if cancel { "cancel" } else { "init" },
            1,
            if cancel {
                "allocator.after"
            } else {
                "allocator.reserved"
            },
        );
        let prefix = if cancel {
            plan.prefix
        } else {
            WalPrefix {
                complete_lsn: 0,
                synced_lsn: 0,
                complete_offset: 4096,
                synced_offset: 4096,
                ..plan.prefix
            }
        };
        let op = operation::Plan {
            profile: plan.profile,
            identity: plan.allocator,
            prefix,
            batch: plan.batch,
            cancel,
            before_descriptor: cancel.then_some(plan.before_descriptor.as_slice()),
        };
        let before = if cancel { reserved } else { &original };
        let mut output = boxed::<65536>(0xa5);
        let mut page = boxed::<PAGE_BYTES>(0xcc);
        operation::prepare_with_budget(
            op,
            before,
            &mut output[..],
            &mut page,
            OperationBudget::StatusInit114688,
        )
        .unwrap();
        assert_eq!(&output[..21200], expected);
        assert_eq!(&page[..], &after[..]);
        assert!(output[21200..].iter().all(|b| *b == 0xa5));
        operation::validate_with_budget(
            op,
            before,
            &expected,
            &after,
            OperationBudget::StatusInit114688,
        )
        .unwrap();
        assert_eq!(
            operation::validate_with_budget(
                op,
                before,
                &expected,
                &after,
                OperationBudget::Dirty98304
            ),
            Err(if cancel {
                SemanticError::StateMismatch
            } else {
                SemanticError::BudgetMismatch
            })
        );
    });
}
fn wi(id: usize, profile: u32) {
    use SemanticError::*;
    match id {
        1 => generated(profile, 0),
        2 => profile_frame(profile, false),
        3 => profile_frame(profile, true),
        4 => generated(profile, 1),
        _ => with_input(profile, 0, |plan, before, frame, allocator, status| {
            let mut bytes = frame.to_vec();
            let mut after_a = Box::new(*allocator);
            let mut after_s = Box::new(*status);
            let expected = match id {
                5 => {
                    put16(&mut bytes, 166, 8);
                    UnsupportedOperation
                }
                6 => {
                    bytes[224] = 2;
                    IdentityMismatch
                }
                7 => {
                    put64(&mut bytes, 232, 2);
                    seal(&mut bytes);
                    assert_eq!(
                        validate_init(plan, before, &bytes, allocator, status),
                        Err(IdentityMismatch)
                    );
                    bytes.copy_from_slice(frame);
                    put64(&mut bytes, 184, 2);
                    OrderMismatch
                }
                8 => {
                    put16(&mut bytes, 284, 1);
                    Corrupt
                }
                9 => {
                    bytes[14689] = 3;
                    Corrupt
                }
                10 => {
                    put32(&mut bytes, 14704, 216);
                    seal(&mut bytes);
                    assert_eq!(
                        validate_init(plan, before, &bytes, allocator, status),
                        Err(Corrupt)
                    );
                    bytes.copy_from_slice(frame);
                    put16(&mut bytes, 14698, 16120);
                    Corrupt
                }
                11 => {
                    put16(&mut bytes, 14690, 0);
                    Corrupt
                }
                12 => {
                    bytes[15040] = 3;
                    InventoryMismatch
                }
                13 => {
                    put64(&mut bytes, 15128, 2);
                    InventoryMismatch
                }
                14 => {
                    put64(&mut bytes, 15080, 3);
                    InventoryMismatch
                }
                15..=18 => {
                    let mut desc = *plan.before_descriptor;
                    match id {
                        15 => {
                            put64(&mut desc, 968, 98304);
                            put64(&mut desc, 1288, 98304);
                        }
                        16 => put64(&mut desc, 912, 21200),
                        17 => {
                            put64(&mut desc, 960, 1);
                            put64(&mut desc, 1280, 1);
                        }
                        18 => {
                            put64(&mut desc, 1008, 54384);
                            put64(&mut desc, 1328, 54384);
                        }
                        _ => unreachable!(),
                    }
                    seal(&mut desc);
                    let altered = InitPlan {
                        before_descriptor: &desc,
                        ..clone_plan(plan)
                    };
                    assert_eq!(
                        validate_init(&altered, before, frame, allocator, status),
                        Err(StateMismatch)
                    );
                    return;
                }
                19 => {
                    put64(&mut after_a[..], 10328, 2);
                    seal(&mut after_a[..]);
                    replace_allocator(&mut bytes, &after_a);
                    PageMismatch
                }
                20 => {
                    after_a[10305] = 5;
                    seal(&mut after_a[..]);
                    replace_allocator(&mut bytes, &after_a);
                    PageMismatch
                }
                21 => {
                    after_a.copy_within(10304..10400, 10400);
                    seal(&mut after_a[..]);
                    replace_allocator(&mut bytes, &after_a);
                    PageMismatch
                }
                22 => {
                    put64(&mut after_s[..], 48, 3);
                    seal(&mut after_s[..]);
                    replace_status(&mut bytes, &after_s);
                    Corrupt
                }
                23 => {
                    for (offset, value) in [(16208, 2), (16136, 1)] {
                        after_s.copy_from_slice(status);
                        put64(&mut after_s[..], offset, value);
                        seal(&mut after_s[..]);
                        replace_status(&mut bytes, &after_s);
                        assert_eq!(
                            validate_init(plan, before, &bytes, allocator, &after_s),
                            Err(PageMismatch)
                        );
                    }
                    return;
                }
                24 => {
                    // Every variant retains valid physical and changed inner checksums.
                    bytes[14896] ^= 1;
                    let hash = content_digest(&bytes[14816..15040]);
                    bytes[14744..14776].copy_from_slice(&hash);
                    seal(&mut bytes);
                    assert_eq!(
                        validate_init(plan, before, &bytes, allocator, status),
                        Err(PageMismatch)
                    );
                    bytes.copy_from_slice(frame);
                    put64(&mut bytes, 21552, 3); // redo target ObjectId differs from sealed image.
                    seal(&mut bytes);
                    assert_eq!(
                        validate_init(plan, before, &bytes, allocator, status),
                        Err(Corrupt)
                    );
                    bytes.copy_from_slice(frame);
                    put64(&mut bytes, 352 + 608, 59201);
                    seal(&mut bytes[352..1888]);
                    seal(&mut bytes);
                    assert_eq!(
                        validate_init(plan, before, &bytes, allocator, status),
                        Err(BudgetMismatch)
                    );
                    return;
                }
                _ => unreachable!(),
            };
            seal(&mut bytes);
            assert_eq!(
                validate_init(plan, before, &bytes, &after_a, &after_s),
                Err(expected),
                "WI-{id:02}"
            );
            if (19..=21).contains(&id) {
                assert_eq!(
                    observe_initialized(
                        plan.profile,
                        plan.allocator,
                        plan.payload,
                        plan.role,
                        &after_a,
                        status
                    ),
                    Err(IdentityMismatch),
                    "full role census WI-{id:02}"
                );
            }
        }),
    }
    mark("WI", id, profile);
}

fn wm(id: usize, profile: u32) {
    use SemanticError::*;
    with_input(profile, 0, |plan, before, frame, allocator, status| {
        let before_hash = content_digest(before);
        let frame_hash = content_digest(frame);
        let mut out_a = boxed::<PAGE_BYTES>(if id == 1 { 0 } else { 0xa5 });
        let mut out_s = boxed::<PAGE_BYTES>(if id == 1 { 0 } else { 0x5a });
        if id == 2 {
            out_a[..8192].fill(0xcc);
            out_s[8192..].fill(0x33);
        }
        match id {
            1 | 2 => {
                materialize_init(plan, before, frame, &mut out_a, &mut out_s).unwrap();
                assert_eq!(&out_a[..], allocator);
                assert_eq!(&out_s[..], status);
            }
            3 => assert_eq!(
                materialize_init(plan, before, &frame[..37999], &mut out_a, &mut out_s),
                Err(Corrupt)
            ),
            4 => {
                let mut bad = frame.to_vec();
                bad[21584 + 500] = 1; // Inner page SHA deliberately stale; frame SHA valid.
                seal(&mut bad);
                assert_eq!(
                    materialize_init(plan, before, &bad, &mut out_a, &mut out_s),
                    Err(Corrupt)
                );
            }
            5 => {
                let mut bad = frame.to_vec();
                let a = bad[15256..15296].to_vec();
                let s = bad[21528..21568].to_vec();
                bad[15256..15296].copy_from_slice(&s);
                bad[21528..21568].copy_from_slice(&a);
                seal(&mut bad);
                assert_eq!(
                    materialize_init(plan, before, &bad, &mut out_a, &mut out_s),
                    Err(PageMismatch)
                );
            }
            6 => {
                let mut bad = Box::new(*before);
                put64(&mut bad[..], 10368, 2); // Different adopted reservation work-id.
                seal(&mut bad[..]);
                assert_eq!(
                    materialize_init(plan, &bad, frame, &mut out_a, &mut out_s),
                    Err(PageMismatch)
                );
            }
            7 => assert_eq!(
                materialize_init(plan, allocator, frame, &mut out_a, &mut out_s),
                Err(PageMismatch)
            ),
            8 => {
                let mut bad = Box::new(*before);
                put64(&mut bad[..], 10312, 3);
                put64(&mut bad[..], 64, 3);
                seal(&mut bad[..]);
                // Token digest no longer matches the adopted before descriptor.
                assert_eq!(
                    materialize_init(plan, &bad, frame, &mut out_a, &mut out_s),
                    Err(StateMismatch)
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(content_digest(before), before_hash);
        assert_eq!(content_digest(frame), frame_hash);
    });
    mark("WM", id, profile);
}

macro_rules! wi_test {
    ($name:ident, $id:literal) => {
        #[test]
        fn $name() {
            wi($id, 1);
        }
    };
}
macro_rules! wm_test {
    ($name:ident, $id:literal) => {
        #[test]
        fn $name() {
            wm($id, 1);
        }
    };
}
wi_test!(status_init_wi01_exact_init, 1);
wi_test!(status_init_wi02_reserve_budget, 2);
wi_test!(status_init_wi03_cancel_budget, 3);
wi_test!(status_init_wi04_retry_init, 4);
wi_test!(status_init_wi05_opcode, 5);
wi_test!(status_init_wi06_work_kind, 6);
wi_test!(status_init_wi07_birth_chain, 7);
wi_test!(status_init_wi08_native_count, 8);
wi_test!(status_init_wi09_status_before, 9);
wi_test!(status_init_wi10_status_span, 10);
wi_test!(status_init_wi11_redo_index, 11);
wi_test!(status_init_wi12_consume_action, 12);
wi_test!(status_init_wi13_token_birth, 13);
wi_test!(status_init_wi14_token_incarnation, 14);
wi_test!(status_init_wi15_memory_budget, 15);
wi_test!(status_init_wi16_wal_budget, 16);
wi_test!(status_init_wi17_frame_budget, 17);
wi_test!(status_init_wi18_protected_budget, 18);
wi_test!(status_init_wi19_role_generation, 19);
wi_test!(status_init_wi20_role_kind, 20);
wi_test!(status_init_wi21_role_duplicate, 21);
wi_test!(status_init_wi22_payload_owner, 22);
wi_test!(status_init_wi23_canonical_free, 23);
wi_test!(status_init_wi24_cross_target_mismatch, 24);
wm_test!(status_init_wm01_zero_output, 1);
wm_test!(status_init_wm02_torn_output, 2);
wm_test!(status_init_wm03_truncated, 3);
wm_test!(status_init_wm04_inner_page_checksum, 4);
wm_test!(status_init_wm05_swapped_target, 5);
wm_test!(status_init_wm06_wrong_before_owner, 6);
wm_test!(status_init_wm07_already_allocated, 7);
wm_test!(status_init_wm08_newer_before, 8);

#[test]
#[ignore = "DEV3 S5 additional profiles; no repeated profile1 leaf"]
fn status_init_wi_s5_profiles() {
    for profile in 2..=3 {
        for id in 1..=24 {
            let selected = format!("SWI-P{profile}-{id:02}");
            if std::env::var("XADB_STATUS_INIT_CASES")
                .is_ok_and(|value| !value.split(',').any(|item| item == selected))
            {
                continue;
            }
            wi(id, profile);
            println!("XADB_STATUS_INIT_S5_CASE {selected}");
        }
    }
}
#[test]
#[ignore = "DEV3 S5 additional profiles; no repeated profile1 leaf"]
fn status_init_wm_s5_profiles() {
    for profile in 2..=3 {
        for id in 1..=8 {
            let selected = format!("SWM-P{profile}-{id:02}");
            if std::env::var("XADB_STATUS_INIT_CASES")
                .is_ok_and(|value| !value.split(',').any(|item| item == selected))
            {
                continue;
            }
            wm(id, profile);
            println!("XADB_STATUS_INIT_S5_CASE {selected}");
        }
    }
}
