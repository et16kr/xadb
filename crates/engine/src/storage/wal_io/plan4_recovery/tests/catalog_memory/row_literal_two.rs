//! S9 C+B two-root literal ingress (consumers §8.3, boundaries
//! `#row-mi03-s9`): one Row actor, two roots in one statement, written
//! without A. C's actual Begin and two RowReserves issue the rows; the
//! independent literal `tests/fixtures/dev3-module-c2/row-read-two/` (the N1
//! History model, never an XADB encoder) proposes both canonical rows and
//! both Forwards. The second Forward has the first root's U as its actual
//! predecessor H. C checks every byte and installs it, C's Commit publishes
//! q, and B's point and cursor drivers read both rows. Nothing here links
//! A: the module is in the C+B single-source export.
//!
//! The live state before the first RowReserve is C's actual state (the S2
//! fixture plus one more Undo INIT, complete LSN 16), pinned by the author
//! and checked here against the pool and control before the reserve. Its
//! RowReserves (LSN 17, 19) are the model's frames; the Forwards (LSN 18,
//! 20) are the literal's, and the pages C installs are its after images.
//!
//! The known-bad cases change only the second Forward: K07 (a CountChange
//! of H with before 0 and after 0), and the original forms of K04 (the
//! first root's mutation consumed again), K05 (a different H of the same
//! size) and K01 (the U target's table generation only). C's prepare
//! refuses each Before on the same ReservedRow with no effect; its explicit
//! cancel is the model's Cancel of that reservation, and the transaction
//! then commits root 1 alone.
use super::row_literal::{effects, int_of, patched, wal};
use super::row_preflight::{create_table, system_init, t_frames};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RetentionCause, RetentionPhase, RowInputPlan, RowOperation,
        RowOwnedError, RowPageRole, RowPreflightStep, RowProducerLane, RowProposal,
        RowValidationLane, TableStep, TransactionOptions,
    },
    row::read::driver::{
        CursorStep, PointRead,
        live::{LiveCall as ReadCall, next_live, open_cursor_live, read_point_live},
    },
};
use foundation::{identity::RowId, runtime::Cancellation};
use std::collections::BTreeMap;

/// The literal directory, from the repository root.
const AUTHOR: &str = "tests/fixtures/dev3-module-c2/row-read-two";
/// The WAL complete LSN before the first RowReserve (expected.json
/// `previous_lsn`).
const PREVIOUS_LSN: u64 = 16;
/// (RowId, value) of the two roots (expected.json `rows`).
const ROWS: [(u64, i64); 2] = [(1, 7), (2, 8)];
/// The model's RowReserve frames C must write (expected.json `reserve`):
/// bytes and SHA-256, and the Free Undo each reserves.
const RESERVE: [(usize, &str, u64); 2] = [
    (
        57_336,
        "adf9b6ac1da88b3186f92e224910fc9d1583758a49e6832be4ebedc2043d8589",
        14,
    ),
    (
        57_336,
        "264da56f4f49c5198991da2f97a1a2d1df9889eaa2aca94144690218f82a2e9e",
        15,
    ),
];
/// The model's Cancel of the second reservation (expected.json `cancel2`).
const CANCEL2: (usize, &str) = (
    40_000,
    "0774407fcd9d50bbcd9f888213d64334a4aab1778ffd049c0284864aa26c2f0b",
);
/// expected.json `files`: (name, bytes, SHA-256).
const FILES: [(&str, usize, &str); 14] = [
    (
        "after1-jobs.bin",
        PAGE_BYTES,
        "7134573aef54d6d48c02ed692a2a3718e3813bae58e6829e6511aff362fd5232",
    ),
    (
        "after1-leaf.bin",
        PAGE_BYTES,
        "4b0174c0b2cc17dccd19e59a0786a9c616af3693215a01d27d5d2bd086115e50",
    ),
    (
        "after1-root.bin",
        PAGE_BYTES,
        "ea64fe3597f7b14a6ad2a50044232988d28c19fe4b0e30423f711e81af096da2",
    ),
    (
        "after1-status.bin",
        PAGE_BYTES,
        "87269fd4f0e7310c8ff4156f02d5374b455b8f3877ca8b2aa501dece33a62f93",
    ),
    (
        "after1-undo.bin",
        PAGE_BYTES,
        "029b3a7f628da543dae571d4a3306ad1d93d98f1086162bc2ef847f25f421f3c",
    ),
    (
        "after2-jobs.bin",
        PAGE_BYTES,
        "22332593d722dabcb0e8a5ceea300a9b9b8c6e85e243597a1f4e4bf44243da07",
    ),
    (
        "after2-leaf.bin",
        PAGE_BYTES,
        "48c3cf30feb2584e2c581dbe5392678dd6cfe5653ec6e22ef46edf419ebd3d39",
    ),
    (
        "after2-root.bin",
        PAGE_BYTES,
        "be9126f0f799cb3cde5ad4c21c91138787bac77918560151258a0b9504b294e5",
    ),
    (
        "after2-status.bin",
        PAGE_BYTES,
        "5296ae9abe9983033e2d0f469634f39b164aaf505257a2afe321beef18452038",
    ),
    (
        "after2-undo.bin",
        PAGE_BYTES,
        "247ff79388d4e67e865447cf879c4dcfd441e56db498b721e96b2297e2810431",
    ),
    (
        "canonical1.bin",
        13,
        "fae2df78d78985674f9e83253255b062c3bbee012bc876f26e4342520ae04b38",
    ),
    (
        "canonical2.bin",
        13,
        "d8a84af06a90e26233dc44a33240eaac7c9dbeefb3d0ddc2a5effa13549110dd",
    ),
    (
        "forward1-frame.bin",
        91_497,
        "c9e8c028cc5cf2c42aff78dc6f07eb8f62587192fa48caf6743df9009feba347",
    ),
    (
        "forward2-frame.bin",
        91_625,
        "89cce26aeba4ca1c6fca0fab45e416625b2974a1c5c27396a6473dd0a21cbddc",
    ),
];
/// The live inputs the author re-encodes (its `LIVE` pins): Root 1, Jobs
/// 2, Status 11, RowLeaf 13 and the Free Undo 14 and 15 from the pool, the
/// 64 Job cells, the Row actor's ATT row, the Aging row and the profile.
const LIVE: [(&str, &str); 10] = [
    (
        "root1.bin",
        "1a036fc2744150d2219461abd250728fc9111cc5f758ee8025888743ab1533ae",
    ),
    (
        "jobs2.bin",
        "20c699c3ecf493a253f8719c4d9db3392323d25d1e29bb6a7f612d691b08a3ea",
    ),
    (
        "status11.bin",
        "dbdffac2932ae3f46ae046217f36ff7656d17a88f710652017092f21f9076860",
    ),
    (
        "leaf13.bin",
        "30b625850232dac524dda502796d7e3b0c2679cfc55346cab807b1579b465396",
    ),
    (
        "undo14.bin",
        "7fe328b11b1d505fa2f35c0297e5cfcc369143ea7e8272298d564fe751774eab",
    ),
    (
        "undo15.bin",
        "d826cb72764ce486fcc4f9346b6ce6e93a7d967bc1e24d285dd6b4f80acac31d",
    ),
    (
        "bank.bin",
        "90a616fc3dace26ea94ff705ec46e28d846292b54d7bbb3cb6a6d85b9ec07efe",
    ),
    (
        "tx.bin",
        "3d1838767a9da54b61a3c86b46dfb8153c645c5504e2d44cb438237e9403fd51",
    ),
    (
        "aging.bin",
        "a35e819253d8a82d64cfc087a24c68bae014cec2874fd7b6dfefb1f11cecd8af",
    ),
    (
        "profile.bin",
        "f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e",
    ),
];
/// The Aging row's control slot.
const AGING: usize = 32 * 1664;
/// The page each after image of root `r` replaces: (role, file stem, page
/// of root 1, page of root 2).
const AFTER: [(RowPageRole, &str, u64, u64); 5] = [
    (RowPageRole::Root, "root", 1, 1),
    (RowPageRole::RootJob, "jobs", 2, 2),
    (RowPageRole::Status, "status", 11, 11),
    (RowPageRole::Undo, "undo", 14, 15),
    (RowPageRole::RowLeaf, "leaf", 13, 13),
];

/// The author's directory.
fn author() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(AUTHOR)
}

/// The author's expected.json.
fn expected() -> String {
    String::from_utf8(read(&author().join("expected.json"), 1 << 16)).unwrap()
}

/// The literal's files by name, each checked against its pin.
#[derive(Clone)]
struct Literal(BTreeMap<String, Vec<u8>>);

impl Literal {
    fn load() -> Self {
        let root = author();
        let expected = expected();
        for fact in [
            "\"case\": \"MCB05-L02\"".to_string(),
            format!("\"previous_lsn\": {PREVIOUS_LSN}"),
            format!(
                "\"rows\": [\n    [\n      {},\n      {}\n    ],\n    [\n      {},\n      {}\n    ]\n  ]",
                ROWS[0].0, ROWS[0].1, ROWS[1].0, ROWS[1].1
            ),
            format!(
                "\"bytes\": {},\n    \"lsn\": 20,\n    \"sha256\": \"{}\"",
                CANCEL2.0, CANCEL2.1
            ),
        ] {
            assert!(expected.contains(&fact), "expected.json lacks {fact}");
        }
        for (index, (bytes, digest, undo)) in RESERVE.iter().enumerate() {
            let fact = format!(
                "\"bytes\": {bytes},\n      \"lsn\": {},\n      \"sha256\": \"{digest}\",\n      \"undo\": {undo}",
                PREVIOUS_LSN + 1 + 2 * index as u64
            );
            assert!(expected.contains(&fact), "expected.json reserve {index}");
        }
        for (name, digest) in LIVE {
            assert_eq!(
                hex(&read(&root.join("live").join(name), 1 << 17)),
                digest,
                "live {name}"
            );
            assert!(
                expected.contains(&format!(
                    "\"dev3-module-c2/row-read-two/live/{name}\": \"{digest}\""
                )),
                "expected.json pin of live {name}"
            );
        }
        Self(
            FILES
                .iter()
                .map(|(name, bytes, digest)| {
                    let data = read(&root.join("literal").join(name), 1 << 17);
                    assert_eq!(
                        (data.len(), hex(&data).as_str()),
                        (*bytes, *digest),
                        "{name}"
                    );
                    assert!(
                        expected.contains(&format!(
                            "\"{name}\": {{\n      \"bytes\": {bytes},\n      \"sha256\": \"{digest}\""
                        )),
                        "expected.json pin of {name}"
                    );
                    (name.to_string(), data)
                })
                .collect(),
        )
    }

    /// The known-bad variant `case` applied to this literal's files.
    fn known_bad(&self, case: &str, digest: &str) -> Self {
        let mut out = self.clone();
        for (name, data) in patched(&author(), &expected(), case, digest, &|name| {
            self.0
                .get(name)
                .unwrap_or_else(|| panic!("patched file {name}"))
                .clone()
        }) {
            out.0.insert(name, data);
        }
        out
    }

    fn file(&self, name: &str) -> &[u8] {
        &self.0[name]
    }

    /// Root `r`'s canonical row in the validation lane; the untrusted plan
    /// names its length, SHA-256 and the lane's SchemaEpoch.
    fn plan(&self, r: usize, lane: RowValidationLane<'_>) -> RowInputPlan {
        let canonical = self.file(&format!("canonical{r}.bin"));
        let (context, _schema, _leaf, candidate) = lane.parts();
        candidate[..canonical.len()].copy_from_slice(canonical);
        RowInputPlan::new(
            canonical.len(),
            context.schema_epoch(),
            foundation::codec::content_digest(canonical),
        )
    }

    /// Root `r`'s Forward frame and after images in the producer lane, the
    /// images in C's declared physical order.
    fn forward(&self, r: usize, lane: RowProducerLane<'_>) -> RowProposal {
        let (context, _control, _before, _schema, frame, after, _candidate, _scratch) =
            lane.parts();
        assert_eq!(context.operation(), RowOperation::Forward);
        let bytes = self.file(&format!("forward{r}-frame.bin"));
        frame[..bytes.len()].copy_from_slice(bytes);
        let order = context.after_order();
        assert_eq!(order.len(), 5, "a Forward changes five pages");
        let mut roles = [RowPageRole::Root; 5];
        for (index, role) in order.iter().enumerate() {
            let (_, stem, ..) = AFTER.iter().find(|(r, ..)| r == role).unwrap();
            after[index].copy_from_slice(self.file(&format!("after{r}-{stem}.bin")));
            roles[index] = *role;
        }
        RowProposal::new(RowOperation::Forward, bytes.len(), 5, roles)
    }
}

/// One pool page's bytes, through the diagnostic probe.
fn page(pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>, number: u64) -> String {
    let index = probe::registered_index(pool, number).unwrap();
    let mut bytes = boxed::<PAGE_BYTES>();
    probe::diagnose(pool, index, &mut bytes);
    hex(&bytes[..])
}

/// MCB05-L02, or with `bad` the MCB05 known-bad case `(name, digest)` on
/// the second Forward. The live state equals the author's pins; root 1's
/// RowReserve is the model's, its literal Forward lands with its five after
/// images; root 2's RowReserve is the model's. Without `bad`, root 2's
/// literal Forward lands likewise, B reads both rows in the writer's own
/// statement, C's Commit returns q 2, and a new read-only transaction reads
/// both by point and cursor. With `bad`, C's prepare refuses it Before
/// (Prepare, Unchanged, RowSemantic) on the same ReservedRow with no WAL,
/// file or T page change; its cancel is the model's Cancel; the statement
/// ends and the Commit (q 2) publishes root 1 alone.
fn two_roots(case_id: &str, bad: Option<(&str, &str)>) {
    case(case_id, || {
        let literal = Literal::load();
        let offered = match bad {
            Some((name, digest)) => literal.known_bad(name, digest),
            None => literal.clone(),
        };
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, first) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let second = system_init(live, owner, flusher, true);
                assert_eq!(
                    (first.unwrap().identity().page, second.identity().page),
                    (page_ref(RESERVE[0].2, 2), page_ref(RESERVE[1].2, 2))
                );
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let pool = reader.catalog_test_pool();
                let inputs = {
                    let bank = writer.recovery_test_bank();
                    [
                        page(pool, 1),
                        page(pool, 2),
                        page(pool, 11),
                        page(pool, 13),
                        page(pool, 14),
                        page(pool, 15),
                        hex(writer.catalog_test_jobs().as_flattened()),
                        hex(&bank[..1536]),
                        hex(&bank[AGING..AGING + 1536]),
                        hex(&state.lock().unwrap().files["d3-profile"].bytes),
                    ]
                };
                for ((name, pin), live) in LIVE.iter().zip(&inputs) {
                    assert_eq!(live, pin, "live input {name}");
                }
                assert_eq!(
                    writer.free_undo_test_state().unwrap().0.complete_lsn,
                    PREVIOUS_LSN
                );
                for r in 1..=2usize {
                    let from = wal(state).len();
                    // Settled (every victim flushed): the next preflight is Ready.
                    let _ = settle!(writer, owner, view, table, private, flush, flusher);
                    let mut preflight = match writer
                        .preflight_row(owner, &view, &table, private, &mut flush)
                        .unwrap()
                    {
                        RowPreflightStep::Ready(preflight) => preflight,
                        RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
                    };
                    let plan = literal.plan(r, preflight.validation_lane().unwrap());
                    let mut reserved = match preflight.reserve(plan) {
                        Ok(reserved) => reserved,
                        Err(RowOwnedError::Before { error, .. })
                        | Err(RowOwnedError::Fatal(error)) => panic!("reserve {r} {error:?}"),
                    };
                    let (_, _, row_id, undo_ref, lsn) = reserved.test_issued();
                    assert_eq!(
                        (row_id.get(), undo_ref.page().page_no, lsn),
                        (
                            ROWS[r - 1].0,
                            RESERVE[r - 1].2,
                            PREVIOUS_LSN - 1 + 2 * r as u64
                        )
                    );
                    let reserve = wal(state)[from..].to_vec();
                    assert_eq!(
                        (reserve.len(), hex(&reserve).as_str()),
                        (RESERVE[r - 1].0, RESERVE[r - 1].1),
                        "the model's RowReserve {r}"
                    );
                    if r == 2
                        && let Some((name, _)) = bad
                    {
                        let issued = reserved.test_issued();
                        let draft = reserved.test_draft();
                        let before = effects(state);
                        let pages = t_frames(pool);
                        let proposal = offered.forward(2, reserved.producer_lane().unwrap());
                        let (error, reserved) = match reserved.prepare(proposal) {
                            Err(RowOwnedError::Before { error, owned }) => (error, owned),
                            Err(RowOwnedError::Fatal(error)) => panic!("{name} Fatal {error:?}"),
                            Ok(_) => panic!("{name} prepared"),
                        };
                        println!("MCB05_{name}_TWO prepare={error:?}");
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Prepare, Effect::Unchanged)
                        );
                        assert!(matches!(error.cause, RetentionCause::RowSemantic(_)));
                        assert_eq!(reserved.test_issued(), issued);
                        assert_eq!(reserved.test_draft(), draft);
                        assert_eq!(effects(state), before, "no WAL or file effect");
                        for (old, new) in pages.iter().zip(t_frames(pool)) {
                            assert_eq!(*old, new, "page {}", old.0);
                        }
                        assert!(!fence.is_fenced());
                        if reserved.cancel().is_err() {
                            panic!("{name} cancel refused");
                        }
                        let cancel = wal(state)[before.0..].to_vec();
                        assert_eq!(
                            (cancel.len(), hex(&cancel).as_str()),
                            CANCEL2,
                            "the model's Cancel of reservation 2"
                        );
                        break;
                    }
                    let proposal = offered.forward(r, reserved.producer_lane().unwrap());
                    let prepared = match reserved.prepare(proposal) {
                        Ok(prepared) => prepared,
                        Err(RowOwnedError::Before { error, .. })
                        | Err(RowOwnedError::Fatal(error)) => panic!("prepare {r} {error:?}"),
                    };
                    let applied = match prepared.apply() {
                        Ok(applied) => applied,
                        Err(RowOwnedError::Before { error, .. })
                        | Err(RowOwnedError::Fatal(error)) => panic!("apply {r} {error:?}"),
                    };
                    let (observation, _) = applied.into_parts();
                    assert_eq!(observation.operation(), RowOperation::Forward);
                    assert_eq!((observation.row(), observation.undo()), (row_id, undo_ref));
                    let gained = wal(state)[from + RESERVE[r - 1].0..].to_vec();
                    assert_eq!(
                        gained,
                        literal.file(&format!("forward{r}-frame.bin")),
                        "the WAL holds the literal Forward {r}"
                    );
                    for (_, stem, page1, page2) in AFTER {
                        let number = if r == 1 { page1 } else { page2 };
                        assert_eq!(
                            page(pool, number),
                            hex(literal.file(&format!("after{r}-{stem}.bin"))),
                            "after{r}-{stem}"
                        );
                    }
                }
                let written: Vec<(u64, i64)> = ROWS[..if bad.is_some() { 1 } else { 2 }].to_vec();
                for (row_id, value) in ROWS {
                    let found = written.contains(&(row_id, value)).then_some(value);
                    assert_eq!(
                        point!(reader, owner, reader_private, &view, &table, row_id),
                        found
                    );
                }
                assert_eq!(
                    scan!(reader, owner, reader_private, &view, &table, flush, flusher),
                    written
                );
                assert!(writer.finish_statement(owner, view).is_ok());
                let q = match writer.commit_row(owner, row, private) {
                    Ok(committed) => committed.q(),
                    Err(_) => panic!("C's Commit of the literal rows refused"),
                };
                assert_eq!(q, 2);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b_owner = &mut b_owner;
                let c = read_only!(
                    writer,
                    b_owner,
                    IsolationLevel::Serializable,
                    reader_private
                );
                let new = writer.begin_statement(b_owner, &c).unwrap();
                assert_eq!(new.q().unwrap(), 2);
                for (row_id, value) in ROWS {
                    let found = written.contains(&(row_id, value)).then_some(value);
                    assert_eq!(
                        point!(reader, b_owner, reader_private, &new, &table, row_id),
                        found
                    );
                }
                assert_eq!(
                    scan!(
                        reader,
                        b_owner,
                        reader_private,
                        &new,
                        &table,
                        flush,
                        flusher
                    ),
                    written
                );
                assert!(writer.finish_statement(b_owner, new).is_ok());
                assert!(writer.finish_read_only(b_owner, c).is_ok());
                b_owner.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                println!("MCB05_TWO {case_id} q={q} rows={written:?}");
            }
        );
    });
}

/// MCB05-L02: two roots of one actor, the second with an actual H.
#[test]
fn mcb05_l02_two_literal_roots_with_actual_h_are_read_by_b() {
    two_roots("MCB05-L02", None);
}

/// MCB05-K07: a CountChange of the actual H with before 0 and after 0.
#[test]
fn mcb05_k07_h_count_before_zero_net_zero_is_before_then_cancel() {
    two_roots(
        "MCB05-K07",
        Some((
            "K07",
            "7dc776a6a77d58f1c7ad146d07ebfbb82f699584592869cc81d7c2c1d1f8fc96",
        )),
    );
}

/// MCB05-K04 (original form): the second Forward consumes the first
/// root's mutation 1 again.
#[test]
fn mcb05_k04_previous_root_mutation_consumed_again_is_before_then_cancel() {
    two_roots(
        "MCB05-K04-ROOT",
        Some((
            "K04",
            "8f47fbe5c58688cb716d7f9dbc81cc0ceca131df3b2b5c634e0203d0489df916",
        )),
    );
}

/// MCB05-K05 (original form): a different H of the same size (the actual
/// H's generation + 1).
#[test]
fn mcb05_k05_wrong_predecessor_same_size_is_before_then_cancel() {
    two_roots(
        "MCB05-K05-H",
        Some((
            "K05",
            "25ec5eba08eca315e75e0014c454e11bfab718c5095e3cc6b0905ce27eaa2ddb",
        )),
    );
}

/// MCB05-K01 (target only): the U target's table generation 2, the Job,
/// record and projection keep T generation 1.
#[test]
fn mcb05_k01_undo_target_table_generation_only_is_before_then_cancel() {
    two_roots(
        "MCB05-K01-TARGET",
        Some((
            "K01",
            "01990fd08b1752d124c4f696df60a32f7dd0310db68f5a91e9c16e015843e9a3",
        )),
    );
}
