use super::*;
use crate::storage::page::{FileId, PAGE_BYTES, PageHeader, PageLsn, encode_page};
use xadb_foundation::identity::ObjectId;

// Frozen before implementation from the owner offset table, never from encode.
const LITERALS: [&str; 12] = [
    "0102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000",
    "0102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001400000000000000",
    "0102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001500000000000000",
    "0100000000000000160000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "02000000000000000102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001400000000000000",
    "02000000010000000300000000000000040000000000000005000000000000001400000000000000",
    "58545331010001000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "58545331010002010102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000000000000000000000700000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000200000001000000030000000000000004000000000000000400000000000000050000000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000",
    "58545331010003000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000016000000000000000100000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "58545331010004000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000000000000000000000100000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "58545331010002020102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000000000000000000000100000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "585453310100000000000000000000000000000000000000000000000000000000000000000000000000000000000000140000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
];
const HEAD: &str =
    "02000000010000000300000000000000040000000000000005000000000000000600000000000000";
const TARGET: &str =
    "02000000010000000300000000000000040000000000000004000000000000000500000000000000";

fn hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap())
        .collect()
}

fn literal(index: usize) -> Vec<u8> {
    hex(LITERALS[index])
}

fn id() -> TxnId {
    TxnId {
        database: DatabaseId::new(std::array::from_fn(|index| index as u8 + 1)).unwrap(),
        timeline: 17,
        writer_epoch: 18,
        local: 19,
    }
}

fn generation(value: u64) -> Generation {
    Generation::new(value).unwrap()
}

fn reference() -> TxnRef {
    TxnRef::new(id(), generation(20)).unwrap()
}

fn page_ref() -> PageRef {
    PageRef {
        file: FileId::new(0x0000_0001_0000_0002).unwrap(),
        page_no: 3,
        incarnation: generation(4),
    }
}

fn undo(slot: u16, value: u64) -> UndoRef {
    UndoRef::new(page_ref(), slot, generation(value)).unwrap()
}

fn occupied(state: TransactionState, head: Option<UndoRef>) -> TransactionStatusCell {
    TransactionStatusCell::Occupied(TransactionStatus::new(reference(), state, head).unwrap())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Id(TxnId),
    Ref(TxnRef),
    Logical(LogicalRowVersion),
    Stamp(VersionStamp),
    Location(StatusSlotRef),
    Cell(TransactionStatusCell),
}

impl Value {
    fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        match self {
            Self::Id(value) => encode_txn_id(value, output),
            Self::Ref(value) => value.encode(output),
            Self::Logical(value) => value.encode(output),
            Self::Stamp(value) => value.encode(output),
            Self::Location(value) => value.encode(output),
            Self::Cell(value) => value.encode(output),
        }
    }
}

fn decode(index: usize, bytes: &[u8]) -> Result<Value, PageError> {
    match index {
        0 => decode_txn_id(bytes).map(Value::Id),
        1 => TxnRef::decode(bytes).map(Value::Ref),
        2 => LogicalRowVersion::decode(bytes).map(Value::Logical),
        3 | 4 => VersionStamp::decode(bytes).map(Value::Stamp),
        5 => StatusSlotRef::decode(bytes).map(Value::Location),
        6..12 => TransactionStatusCell::decode(bytes).map(Value::Cell),
        _ => panic!("unregistered literal"),
    }
}

fn values() -> [Value; 12] {
    [
        Value::Id(id()),
        Value::Ref(reference()),
        Value::Logical(LogicalRowVersion::new(id(), 21).unwrap()),
        Value::Stamp(VersionStamp::Committed(CommitSeq::new(22).unwrap())),
        Value::Stamp(VersionStamp::Transaction(reference())),
        Value::Location(StatusSlotRef::new(page_ref(), 5, generation(20)).unwrap()),
        Value::Cell(occupied(TransactionState::Active, None)),
        Value::Cell(occupied(
            TransactionState::Aborting(RollbackState::Statement {
                target: Some(undo(4, 5)),
                next: Some(undo(5, 6)),
            }),
            Some(undo(5, 6)),
        )),
        Value::Cell(occupied(
            TransactionState::Committed(CommitSeq::new(22).unwrap()),
            Some(undo(5, 6)),
        )),
        Value::Cell(occupied(TransactionState::Aborted, Some(undo(5, 6)))),
        Value::Cell(occupied(
            TransactionState::Aborting(RollbackState::Transaction { next: None }),
            Some(undo(5, 6)),
        )),
        Value::Cell(TransactionStatusCell::Free(generation(20))),
    ]
}

fn check_literal(index: usize) {
    let expected = literal(index);
    let value = values()[index];
    assert_eq!(decode(index, &expected), Ok(value), "V{:02}", index + 1);
    let mut output = vec![0xa5; expected.len()];
    value.encode(&mut output).unwrap();
    assert_eq!(output, expected, "V{:02}", index + 1);
}

#[test]
fn identity_literals_and_limits_are_canonical() {
    for index in [0, 1, 5] {
        check_literal(index);
    }
    assert_eq!(MAX_STATUS_SLOTS, 82);
    for value in [1, u64::MAX] {
        let id = TxnId {
            timeline: value,
            writer_epoch: if value == 1 { 1 } else { u32::MAX },
            local: value,
            ..id()
        };
        let reference = TxnRef::new(id, generation(value)).unwrap();
        let mut bytes = [0; TXN_REF_BYTES];
        reference.encode(&mut bytes).unwrap();
        assert_eq!(TxnRef::decode(&bytes), Ok(reference));
        assert_eq!(reference.id(), id);
        assert_eq!(reference.generation().get(), value);
        let location = StatusSlotRef::new(page_ref(), 81, generation(value)).unwrap();
        let mut bytes = [0; STATUS_SLOT_REF_BYTES];
        location.encode(&mut bytes).unwrap();
        assert_eq!(StatusSlotRef::decode(&bytes), Ok(location));
        assert_eq!(location.slot(), 81);
        assert_eq!(location.page(), page_ref());
        assert_eq!(location.generation().get(), value);
    }
    assert!(generation(u64::MAX).next().is_err());
    assert_eq!(
        StatusSlotRef::new(page_ref(), 82, generation(1)),
        Err(PageError::InvalidInput)
    );
    for bad_id in [
        TxnId {
            timeline: 0,
            ..id()
        },
        TxnId {
            writer_epoch: 0,
            ..id()
        },
        TxnId { local: 0, ..id() },
    ] {
        let mut output = [0x8a; 40];
        assert_eq!(
            encode_txn_id(bad_id, &mut output),
            Err(PageError::InvalidInput)
        );
        assert_eq!(output, [0x8a; 40]);
        assert_eq!(
            TxnRef::new(bad_id, generation(1)),
            Err(PageError::InvalidInput)
        );
        assert_eq!(
            LogicalRowVersion::new(bad_id, 1),
            Err(PageError::InvalidInput)
        );
    }
}

#[test]
fn stamp_and_logical_version_identities_are_distinct() {
    for index in [2, 3, 4] {
        check_literal(index);
    }
    assert_eq!(
        LogicalRowVersion::new(id(), 0),
        Err(PageError::InvalidInput)
    );
    let first = LogicalRowVersion::new(id(), 1).unwrap();
    let second = LogicalRowVersion::new(id(), 2).unwrap();
    assert_ne!(first, second);
    let moved = first;
    let before_image = first;
    let mut row = (second, VersionStamp::Transaction(reference()));
    row.1 = VersionStamp::Committed(CommitSeq::new(22).unwrap());
    assert_eq!(row.0, second);
    row.0 = before_image;
    assert_eq!(row.0, moved);
    assert_eq!(first.transaction(), id());
    assert_eq!(first.mutation_sequence(), 1);
    let max = LogicalRowVersion::new(id(), u64::MAX).unwrap();
    let mut bytes = [0; 48];
    max.encode(&mut bytes).unwrap();
    assert_eq!(LogicalRowVersion::decode(&bytes), Ok(max));
    for q in [1, u64::MAX] {
        let value = VersionStamp::Committed(CommitSeq::new(q).unwrap());
        let mut bytes = [0; 56];
        value.encode(&mut bytes).unwrap();
        assert_eq!(VersionStamp::decode(&bytes), Ok(value));
    }
}

#[test]
fn status_literals_preserve_rollback_and_terminal_heads() {
    for index in 6..12 {
        check_literal(index);
    }
    let head = Some(undo(5, 6));
    let target = Some(undo(4, 5));
    // Value representations only: this does not simulate or assert a CLR effect.
    for _ in 0..3 {
        for (state, retained) in [
            (
                TransactionState::Aborting(RollbackState::Statement {
                    target,
                    next: target,
                }),
                head,
            ),
            (TransactionState::Active, head),
            (
                TransactionState::Aborting(RollbackState::Statement {
                    target: None,
                    next: None,
                }),
                None,
            ),
            (TransactionState::Active, None),
        ] {
            let status = TransactionStatus::new(reference(), state, retained).unwrap();
            let mut bytes = [0; 192];
            TransactionStatusCell::Occupied(status)
                .encode(&mut bytes)
                .unwrap();
            assert_eq!(
                TransactionStatusCell::decode(&bytes),
                Ok(TransactionStatusCell::Occupied(status))
            );
            assert_eq!(status.reference(), reference());
            assert_eq!(status.state(), state);
            assert_eq!(status.undo_head(), retained);
        }
    }
    for (head, target, next, statement) in [
        (None, None, Some(undo(5, 6)), true),
        (None, Some(undo(4, 5)), Some(undo(5, 6)), true),
        (head, Some(undo(4, 5)), None, true),
        (None, None, Some(undo(5, 6)), false),
    ] {
        let rollback = if statement {
            RollbackState::Statement { target, next }
        } else {
            RollbackState::Transaction { next }
        };
        assert_eq!(
            TransactionStatus::new(reference(), TransactionState::Aborting(rollback), head),
            Err(PageError::InvalidInput)
        );
    }
    let free = TransactionStatusCell::Free(generation(u64::MAX));
    let mut bytes = [0; 192];
    free.encode(&mut bytes).unwrap();
    assert_eq!(TransactionStatusCell::decode(&bytes), Ok(free));
}

#[test]
fn all_length_boundaries_reject_without_output_changes() {
    let mut decodes = 0;
    let mut encodes = 0;
    let lengths = [40, 48, 48, 56, 56, 40, 192, 192, 192, 192, 192, 192];
    for (index, value) in values().into_iter().enumerate() {
        let good = literal(index);
        assert_eq!(good.len(), lengths[index]);
        for length in (0..good.len()).chain(std::iter::once(good.len() + 1)) {
            let mut input = good.clone();
            input.resize(length, 0);
            assert_eq!(decode(index, &input), Err(PageError::Corrupt));
            decodes += 1;
            let mut output: Vec<u8> = (0..length)
                .map(|offset| (offset as u8).wrapping_add(0x59))
                .collect();
            let before = output.clone();
            assert_eq!(value.encode(&mut output), Err(PageError::InvalidInput));
            assert_eq!(output, before);
            encodes += 1;
        }
    }
    assert_eq!((decodes, encodes), (1452, 1452));
}

#[test]
fn fixed_corrupt_fields_are_rejected() {
    // N01..N40: (zero-based literal, offset, replacement length, first byte).
    let cases = [
        (0, 0, 16, 0),
        (0, 16, 8, 0),
        (0, 24, 4, 0),
        (0, 32, 8, 0),
        (0, 28, 1, 1),
        (1, 40, 8, 0),
        (2, 40, 8, 0),
        (5, 4, 4, 0),
        (5, 0, 4, 0),
        (5, 16, 8, 0),
        (5, 24, 2, 82),
        (5, 26, 1, 1),
        (5, 32, 8, 0),
        (3, 0, 1, 0),
        (3, 0, 1, 3),
        (3, 1, 1, 1),
        (3, 8, 8, 0),
        (3, 16, 1, 1),
        (4, 40, 8, 0),
        (4, 48, 8, 0),
        (6, 0, 1, 0),
        (6, 4, 2, 2),
        (6, 6, 1, 5),
        (6, 7, 1, 3),
        (6, 64, 1, 8),
        (6, 65, 1, 1),
        (6, 72, 1, 1),
        (6, 112, 1, 1),
        (6, 152, 1, 1),
        (7, 72, 40, 0),
        (7, 112, 40, 0),
        (7, 152, 40, 0),
        (6, 40, 8, 0),
        (6, 48, 8, 0),
        (11, 8, 1, 1),
        (11, 48, 8, 0),
        (11, 64, 1, 1),
        (11, 56, 8, 22),
        (11, 7, 1, 1),
        (11, 191, 1, 1),
    ];
    assert_eq!(cases.len(), 40);
    for (ordinal, (index, offset, count, value)) in cases.into_iter().enumerate() {
        let mut bytes = literal(index);
        bytes[offset..offset + count].fill(0);
        bytes[offset] = value;
        let expected = if ordinal == 21 {
            PageError::UnsupportedFormat
        } else {
            PageError::Corrupt
        };
        assert_eq!(decode(index, &bytes), Err(expected), "N{:02}", ordinal + 1);
    }
}

fn header() -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: id().database,
            page: page_ref(),
            owner: ObjectId::new(30).unwrap(),
            owner_generation: generation(31),
        },
        kind: PageKind::TransactionStatus,
        level: 0,
        lsn: PageLsn(32),
    }
}

fn namespace() -> Namespace {
    Namespace {
        database: id().database,
        timeline: id().timeline,
    }
}

fn page_bytes(header: PageHeader, cells: &[Vec<u8>]) -> [u8; PAGE_BYTES] {
    let references: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
    let mut bytes = [0; PAGE_BYTES];
    encode_page(header, &references, &mut bytes).unwrap();
    bytes
}

fn base_cells() -> Vec<Vec<u8>> {
    let mut cells = vec![literal(11); 5];
    cells.push(literal(6));
    cells
}

#[test]
fn status_lookup_binds_logical_and_physical_identity() {
    let base = page_bytes(header(), &base_cells());
    let location = StatusSlotRef::new(page_ref(), 5, generation(20)).unwrap();
    let page = decode_status_page(&base, header().identity, namespace()).unwrap();
    assert_eq!(page.len(), 6);
    assert_eq!(
        page.cell(0),
        Ok(TransactionStatusCell::Free(generation(20)))
    );
    assert_eq!(
        page.lookup(reference(), location).unwrap().reference(),
        reference()
    );
    let other_db = DatabaseId::new([0x77; 16]).unwrap();
    let other_file = FileId::new(0x0000_0002_0000_0002).unwrap();
    for ordinal in 1..=24 {
        let mut expected = header().identity;
        let mut supplied_namespace = namespace();
        let mut supplied_location = location;
        let mut supplied_reference = reference();
        let mut actual_header = header();
        let mut cells = base_cells();
        match ordinal {
            1 => supplied_namespace.database = other_db,
            2 => supplied_namespace.timeline += 1,
            3 => expected.database = other_db,
            4 => expected.page.file = other_file,
            5 => expected.page.page_no += 1,
            6 => expected.page.incarnation = generation(5),
            7 => expected.owner = ObjectId::new(31).unwrap(),
            8 => expected.owner_generation = generation(32),
            9 => actual_header.kind = PageKind::Undo,
            10 => supplied_location.page.file = other_file,
            11 => supplied_location.page.page_no += 1,
            12 => supplied_location.page.incarnation = generation(5),
            13 => supplied_location.slot = 6,
            14 => supplied_location.generation = generation(21),
            15 => supplied_reference.id.database = other_db,
            16 => supplied_reference.id.timeline += 1,
            17 => supplied_reference.id.writer_epoch += 1,
            18 => supplied_reference.id.local += 1,
            19 => supplied_reference.generation = generation(21),
            20 => cells[5][40] = 20,
            21 => cells[5][48] = 21,
            22 => cells[5] = literal(11),
            23 => cells[5].truncate(191),
            24 => cells[0][65] = 1,
            _ => unreachable!(),
        }
        // Encoding a fresh envelope gives semantic damage a valid checksum.
        let input = page_bytes(actual_header, &cells);
        let result = decode_status_page(&input, expected, supplied_namespace)
            .and_then(|page| page.lookup(supplied_reference, supplied_location));
        let error = match ordinal {
            9 => PageError::KindMismatch,
            23 | 24 => PageError::Corrupt,
            _ => PageError::IdentityMismatch,
        };
        assert_eq!(result, Err(error), "L{ordinal:02}");
    }
    for count in [0, 82] {
        let input = page_bytes(header(), &vec![literal(11); count]);
        let page = decode_status_page(&input, header().identity, namespace()).unwrap();
        assert_eq!(page.len(), count);
        assert_eq!(
            page.lookup(reference(), location),
            Err(PageError::IdentityMismatch)
        );
    }
    let refs: Vec<Vec<u8>> = vec![literal(11); 83];
    let slices: Vec<&[u8]> = refs.iter().map(Vec::as_slice).collect();
    let mut output = [0xb7; PAGE_BYTES];
    assert_eq!(
        encode_page(header(), &slices, &mut output),
        Err(PageError::NoSpace)
    );
    assert_eq!(output, [0xb7; PAGE_BYTES]);
    let invalid_namespace = Namespace {
        timeline: 0,
        ..namespace()
    };
    assert_eq!(
        decode_status_page(&base, header().identity, invalid_namespace).err(),
        Some(PageError::InvalidInput)
    );
}

// Raw field construction and the accepted truth table are separate from the
// candidate's enums/constructors/encoder. All 240 shapes are attempted.
#[test]
fn state_and_namespace_matrix_is_exact() {
    let mut accepted = 0;
    let mut rejected = 0;
    for state in 0..5 {
        for rollback in 0..3 {
            for committed in [false, true] {
                for mask in 0..8 {
                    let h = mask & 1 != 0;
                    let t = mask & 2 != 0;
                    let n = mask & 4 != 0;
                    let expected = match state {
                        0 => rollback == 0 && !committed && mask == 0,
                        1 | 4 => rollback == 0 && !committed && !t && !n,
                        2 => {
                            !committed
                                && (!t && !n || h)
                                && ((rollback == 1 && (!t || n)) || (rollback == 2 && !t))
                        }
                        3 => rollback == 0 && committed && !t && !n,
                        _ => unreachable!(),
                    };
                    let mut bytes = literal(if state == 0 { 11 } else { 6 });
                    bytes[6] = state;
                    bytes[7] = rollback;
                    bytes[56] = if committed { 22 } else { 0 };
                    bytes[64] = mask;
                    for (index, present, value) in
                        [(0, h, hex(HEAD)), (1, t, hex(TARGET)), (2, n, hex(HEAD))]
                    {
                        if present {
                            bytes[72 + index * 40..112 + index * 40].copy_from_slice(&value);
                        }
                    }
                    match TransactionStatusCell::decode(&bytes) {
                        Ok(cell) => {
                            assert!(expected, "{state}/{rollback}/{committed}/{mask}");
                            let mut output = [0; 192];
                            cell.encode(&mut output).unwrap();
                            assert_eq!(output.as_slice(), bytes);
                            accepted += 1;
                        }
                        Err(error) => {
                            assert!(!expected, "{state}/{rollback}/{committed}/{mask}");
                            assert_eq!(error, PageError::Corrupt);
                            rejected += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!((accepted, rejected), (14, 226));
    let mut accepted = 0;
    let mut rejected = 0;
    for same_db in [false, true] {
        for timeline in [17_u64, 18] {
            for epoch in [1_u32, 18, u32::MAX] {
                let mut cells = base_cells();
                if !same_db {
                    cells[5][8..24].fill(0x77);
                }
                cells[5][24..32].copy_from_slice(&timeline.to_le_bytes());
                cells[5][32..36].copy_from_slice(&epoch.to_le_bytes());
                let input = page_bytes(header(), &cells);
                let result = decode_status_page(&input, header().identity, namespace());
                if same_db && timeline == 17 {
                    let page = result.unwrap();
                    let expected = TxnRef::new(
                        TxnId {
                            writer_epoch: epoch,
                            ..id()
                        },
                        generation(20),
                    )
                    .unwrap();
                    assert!(
                        page.lookup(
                            expected,
                            StatusSlotRef::new(page_ref(), 5, generation(20)).unwrap()
                        )
                        .is_ok()
                    );
                    accepted += 1;
                } else {
                    assert_eq!(result.err(), Some(PageError::IdentityMismatch));
                    rejected += 1;
                }
            }
        }
    }
    assert_eq!((accepted, rejected), (3, 9));
}

fn allowed_shapes(
    reference: TxnRef,
    head: UndoRef,
    target: UndoRef,
    q: CommitSeq,
) -> [TransactionStatusCell; 14] {
    let cell = |state, head| {
        TransactionStatusCell::Occupied(TransactionStatus::new(reference, state, head).unwrap())
    };
    let statement = |head, target, next| {
        cell(
            TransactionState::Aborting(RollbackState::Statement { target, next }),
            head,
        )
    };
    let transaction = |head, next| {
        cell(
            TransactionState::Aborting(RollbackState::Transaction { next }),
            head,
        )
    };
    [
        TransactionStatusCell::Free(reference.generation()),
        cell(TransactionState::Active, None),
        cell(TransactionState::Active, Some(head)),
        cell(TransactionState::Committed(q), None),
        cell(TransactionState::Committed(q), Some(head)),
        cell(TransactionState::Aborted, None),
        cell(TransactionState::Aborted, Some(head)),
        statement(None, None, None),
        statement(Some(head), None, None),
        statement(Some(head), None, Some(head)),
        statement(Some(head), Some(target), Some(head)),
        transaction(None, None),
        transaction(Some(head), None),
        transaction(Some(head), Some(head)),
    ]
}

fn raw_number(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().unwrap())
}

fn check_raw_id(bytes: &[u8], id: TxnId) {
    assert_eq!(&bytes[..16], &id.database.bytes());
    assert_eq!(raw_number(&bytes[16..24]), id.timeline);
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        id.writer_epoch
    );
    assert_eq!(&bytes[28..32], &[0; 4]);
    assert_eq!(raw_number(&bytes[32..40]), id.local);
}

#[test]
fn generated_transaction_records_roundtrip_without_effects() {
    let mut cases = 0;
    for seed in 0..64_u64 {
        let mut x = seed;
        for iteration in 0..128 {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let nonzero = x.max(1);
            let id = TxnId {
                timeline: nonzero,
                writer_epoch: (x as u32).max(1),
                local: nonzero,
                ..id()
            };
            let generation = generation(nonzero);
            let reference = TxnRef::new(id, generation).unwrap();
            let logical = LogicalRowVersion::new(id, nonzero).unwrap();
            let page = PageRef {
                file: FileId::new((u64::from((x as u32).max(1)) << 32) | (x >> 32).max(1)).unwrap(),
                page_no: x,
                incarnation: generation,
            };
            let location = StatusSlotRef::new(page, (x % 82) as u16, generation).unwrap();
            let q = CommitSeq::new(nonzero).unwrap();
            let stamp = if x & 1 == 0 {
                VersionStamp::Committed(q)
            } else {
                VersionStamp::Transaction(reference)
            };
            let cell = allowed_shapes(
                reference,
                UndoRef::new(page, 5, generation).unwrap(),
                UndoRef::new(page, 4, generation).unwrap(),
                q,
            )[iteration % 14];
            for (index, value, size) in [
                (0, Value::Id(id), 40),
                (1, Value::Ref(reference), 48),
                (2, Value::Logical(logical), 48),
                (3, Value::Stamp(stamp), 56),
                (5, Value::Location(location), 40),
                (6, Value::Cell(cell), 192),
            ] {
                let mut bytes = vec![0x91; size];
                value.encode(&mut bytes).unwrap();
                assert_eq!(decode(index, &bytes), Ok(value));
                match index {
                    0..=2 => {
                        check_raw_id(&bytes, id);
                        if index != 0 {
                            assert_eq!(raw_number(&bytes[40..48]), nonzero);
                        }
                    }
                    3 => match stamp {
                        VersionStamp::Committed(_) => {
                            assert_eq!(bytes[0], 1);
                            assert_eq!(raw_number(&bytes[8..16]), nonzero);
                            assert!(bytes[16..].iter().all(|byte| *byte == 0));
                        }
                        VersionStamp::Transaction(_) => {
                            assert_eq!(bytes[0], 2);
                            check_raw_id(&bytes[8..48], id);
                            assert_eq!(raw_number(&bytes[48..56]), nonzero);
                        }
                    },
                    5 => {
                        assert_eq!(raw_number(&bytes[8..16]), x);
                        assert_eq!(raw_number(&bytes[16..24]), nonzero);
                        assert_eq!(
                            u16::from_le_bytes(bytes[24..26].try_into().unwrap()),
                            (x % 82) as u16
                        );
                        assert_eq!(&bytes[26..32], &[0; 6]);
                        assert_eq!(raw_number(&bytes[32..40]), nonzero);
                    }
                    6 => {
                        // Expected shape arrays are independent from enum serialization.
                        let shape = iteration % 14;
                        assert_eq!(bytes[6], [0, 1, 1, 3, 3, 4, 4, 2, 2, 2, 2, 2, 2, 2][shape]);
                        assert_eq!(bytes[7], [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2][shape]);
                        assert_eq!(bytes[64], [0, 0, 1, 0, 1, 0, 1, 0, 1, 5, 7, 0, 1, 5][shape]);
                        assert_eq!(
                            raw_number(&bytes[56..64]),
                            if matches!(shape, 3 | 4) { nonzero } else { 0 }
                        );
                        assert_eq!(raw_number(&bytes[48..56]), nonzero);
                        if shape == 0 {
                            assert!(bytes[8..48].iter().all(|byte| *byte == 0));
                        } else {
                            check_raw_id(&bytes[8..48], id);
                        }
                        for (part, bit) in [1, 2, 4].into_iter().enumerate() {
                            let part_bytes = &bytes[72 + part * 40..112 + part * 40];
                            if bytes[64] & bit == 0 {
                                assert!(part_bytes.iter().all(|byte| *byte == 0));
                            } else {
                                assert_eq!(raw_number(&part_bytes[8..16]), x);
                                assert_eq!(raw_number(&part_bytes[16..24]), nonzero);
                                assert_eq!(
                                    u16::from_le_bytes(part_bytes[24..26].try_into().unwrap()),
                                    if part == 1 { 4 } else { 5 }
                                );
                                assert_eq!(raw_number(&part_bytes[32..40]), nonzero);
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 8192);
}
