use super::super::{
    page::{FileId, PAGE_BYTES, PageRef},
    transaction_record::{RollbackState, TransactionState, TransactionStatus},
    undo_ref::UndoRef,
};
use super::*;
use crate::{api::DatabaseId, transaction::TxnId};
use xadb_foundation::{
    codec::content_digest,
    identity::{CommitSeq, Generation, ObjectId},
};

const ACTIVE: &[u8; 224] = include_bytes!("../../tests/fixtures/dev3-status-home-active.bin");
const COMMITTED: &[u8; 224] = include_bytes!("../../tests/fixtures/dev3-status-home-committed.bin");
const FREE: &[u8; 224] = include_bytes!("../../tests/fixtures/dev3-status-home-free.bin");
fn put16(b: &mut [u8], offset: usize, value: u16) {
    b[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put64(b: &mut [u8], offset: usize, value: u64) {
    b[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn gen_value(value: u64) -> Generation {
    Generation::new(value).unwrap()
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
    }
}
fn txn(local: u64, epoch: u32) -> TxnId {
    TxnId {
        database: namespace().database,
        timeline: 17,
        writer_epoch: epoch,
        local,
    }
}
fn reference(local: u64, generation: u64) -> TxnRef {
    TxnRef::new(txn(local, 18), gen_value(generation)).unwrap()
}
fn page_ref(page_no: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x200000001).unwrap(),
        page_no,
        incarnation: gen_value(4),
    }
}
fn identity() -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: page_ref(3),
        owner: ObjectId::new(55).unwrap(),
        owner_generation: gen_value(56),
    }
}
fn occupied(state: TransactionState, count: u64) -> StatusHome {
    StatusHome::new(
        TransactionStatusCell::Occupied(
            TransactionStatus::new(reference(19, 20), state, None).unwrap(),
        ),
        count,
    )
    .unwrap()
}

#[test]
fn status_home_literals_match_core_and_count() {
    for (literal, expected) in [
        (ACTIVE, occupied(TransactionState::Active, 2)),
        (
            COMMITTED,
            occupied(TransactionState::Committed(CommitSeq::new(21).unwrap()), 1),
        ),
        (
            FREE,
            StatusHome::new(TransactionStatusCell::Free(gen_value(20)), 0).unwrap(),
        ),
    ] {
        assert_eq!(StatusHome::decode(literal).unwrap(), expected);
        let mut bytes = [0xa5; 224];
        expected.encode(&mut bytes).unwrap();
        assert_eq!(&bytes, literal);
        assert_eq!(
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            expected.incoming()
        );
    }
    for count in [0, 1, u64::MAX] {
        let mut bytes = *ACTIVE;
        put64(&mut bytes, 8, count);
        let expected = occupied(TransactionState::Active, count);
        assert_eq!(StatusHome::decode(&bytes), Ok(expected));
        let mut output = [0; 224];
        expected.encode(&mut output).unwrap();
        assert_eq!(output, bytes);
    }
}

#[test]
fn status_home_rejects_lengths_headers_and_free_references() {
    for fixture in [ACTIVE, COMMITTED, FREE] {
        for length in 0..224 {
            assert_eq!(
                StatusHome::decode(&fixture[..length]),
                Err(PageError::Corrupt)
            );
        }
        let mut long = fixture.to_vec();
        long.push(0);
        assert_eq!(StatusHome::decode(&long), Err(PageError::Corrupt));
        for offset in (0..4).chain(16..32) {
            let mut b = *fixture;
            b[offset] ^= 1;
            assert_eq!(StatusHome::decode(&b), Err(PageError::Corrupt));
        }
        for version in [0, 2] {
            let mut b = *fixture;
            put16(&mut b, 4, version);
            assert_eq!(StatusHome::decode(&b), Err(PageError::UnsupportedFormat));
        }
        for header in [31, 33] {
            let mut b = *fixture;
            put16(&mut b, 6, header);
            assert_eq!(StatusHome::decode(&b), Err(PageError::Corrupt));
        }
    }
    for count in [1, u64::MAX] {
        let mut b = *FREE;
        put64(&mut b, 8, count);
        assert_eq!(StatusHome::decode(&b), Err(PageError::Corrupt));
    }
    let mut negatives: Vec<([u8; 224], PageError)> = Vec::new();
    for (offset, width, value) in [
        (0, 1, 0),
        (4, 2, 0),
        (4, 2, 2),
        (6, 1, 5),
        (7, 1, 3),
        (8, 16, 0),
        (24, 8, 0),
        (32, 4, 0),
        (36, 1, 1),
        (40, 8, 0),
        (48, 8, 0),
        (56, 8, 1),
        (64, 1, 8),
        (65, 1, 1),
        (72, 1, 1),
        (112, 1, 1),
        (152, 1, 1),
    ] {
        let mut b = *ACTIVE;
        let slot = &mut b[32 + offset..32 + offset + width];
        slot.fill(0);
        slot[0] = value;
        negatives.push((
            b,
            if offset == 4 {
                PageError::UnsupportedFormat
            } else {
                PageError::Corrupt
            },
        ));
    }
    for (fixture, offset, value) in [
        (COMMITTED, 56, 0),
        (COMMITTED, 7, 1),
        (FREE, 8, 1),
        (FREE, 56, 1),
        (FREE, 72, 1),
        (FREE, 7, 1),
    ] {
        let mut b = *fixture;
        b[32 + offset] = value;
        negatives.push((b, PageError::Corrupt));
    }
    assert_eq!(negatives.len(), 23);
    for (bytes, error) in negatives {
        assert_eq!(StatusHome::decode(&bytes), Err(error));
    }
}

#[test]
fn status_home_encoder_errors_preserve_output() {
    let home = occupied(TransactionState::Active, 2);
    for length in (0..224).chain([225]) {
        let mut bytes = vec![0x5a; length];
        let before = bytes.clone();
        assert_eq!(home.encode(&mut bytes), Err(PageError::InvalidInput));
        assert_eq!(bytes, before);
    }
    for count in [1, u64::MAX] {
        assert_eq!(
            StatusHome::new(TransactionStatusCell::Free(gen_value(20)), count),
            Err(PageError::InvalidInput)
        );
    }
    let input = *ACTIVE;
    let decoded = StatusHome::decode(&input).unwrap();
    let mut output = [0; 224];
    decoded.encode(&mut output).unwrap();
    assert_eq!(&input, ACTIVE);
    assert_eq!(output, input);
}

#[test]
fn status_home_count_delta_is_checked_and_atomic() {
    let (mut accepted, mut rejected) = (0, 0);
    for old in [0, 1, u64::MAX - 1, u64::MAX] {
        for removed in [0, 1, 2, u64::MAX] {
            for added in [0, 1, 2, u64::MAX] {
                let expected = if removed > old {
                    None
                } else {
                    let wide = u128::from(old) - u128::from(removed) + u128::from(added);
                    (wide <= u128::from(u64::MAX)).then_some(wide as u64)
                };
                let result = adjust_incoming(old, removed, added);
                if let Some(value) = expected {
                    assert_eq!(result, Ok(value));
                    accepted += 1;
                } else {
                    assert_eq!(result, Err(PageError::InvalidInput));
                    rejected += 1;
                }
            }
        }
    }
    assert_eq!((accepted, rejected), (29, 35));
}

// Raw envelope fixture: direct contract offsets, never a product page encoder.
fn raw_page(cells: &[Vec<u8>]) -> [u8; PAGE_BYTES] {
    let mut b = [0; PAGE_BYTES];
    b[..4].copy_from_slice(b"XDP1");
    put16(&mut b, 4, 1);
    b[6] = 9;
    b[8..24].copy_from_slice(&std::array::from_fn::<_, 16, _>(|i| i as u8 + 1));
    for (offset, value) in [
        (24, 0x200000001),
        (32, 3),
        (40, 4),
        (48, 55),
        (56, 56),
        (64, 30),
    ] {
        put64(&mut b, offset, value);
    }
    put16(&mut b, 72, cells.len() as u16);
    put16(&mut b, 74, (128 + 4 * cells.len()) as u16);
    let mut end = 16_352;
    for (slot, cell) in cells.iter().enumerate() {
        end -= cell.len();
        b[end..end + cell.len()].copy_from_slice(cell);
        put16(&mut b, 128 + 4 * slot, end as u16);
        put16(&mut b, 130 + 4 * slot, cell.len() as u16);
    }
    assert!(end >= 128 + 4 * cells.len());
    put16(&mut b, 76, end as u16);
    reseal(&mut b);
    b
}
fn reseal(b: &mut [u8; PAGE_BYTES]) {
    let hash = content_digest(&b[..16_352]);
    b[16_352..].copy_from_slice(&hash);
}
fn cells(count: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|n| {
            let mut b = ACTIVE.to_vec();
            put64(&mut b, 72, 19 + n as u64);
            b
        })
        .collect()
}

#[test]
fn status_home_page_validates_all_slots_and_namespace() {
    assert_eq!(MAX_HOME_SLOTS, 71);
    for count in [0, 1, 70, 71] {
        let b = raw_page(&cells(count));
        let view = decode_status_home_page(&b, identity(), namespace()).unwrap();
        assert_eq!(view.len(), count);
    }
    for case in 0..7 {
        let mut c = cells(3);
        match case {
            0 => c[2] = ACTIVE[32..].to_vec(),
            1 => c[2][16] = 1,
            2 => c[2][40..56].fill(0x22),
            3 => put64(&mut c[2], 56, 18),
            4 | 5 => {
                c[2] = c[0].clone();
                if case == 5 {
                    put64(&mut c[2], 80, 21);
                }
            }
            6 => {
                c[1] = FREE.to_vec();
                c[2] = FREE.to_vec();
            }
            _ => unreachable!(),
        }
        let b = raw_page(&c);
        let result = decode_status_home_page(&b, identity(), namespace());
        if case == 6 {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.err().unwrap(),
                if matches!(case, 2 | 3) {
                    PageError::IdentityMismatch
                } else {
                    PageError::Corrupt
                }
            );
        }
    }
    let mut c = cells(1);
    c[0][64..68].copy_from_slice(&1_u32.to_le_bytes());
    assert!(decode_status_home_page(&raw_page(&c), identity(), namespace()).is_ok());
    let b = raw_page(&cells(1));
    let mut ns = namespace();
    ns.timeline = 0;
    assert_eq!(
        decode_status_home_page(&b, identity(), ns).err(),
        Some(PageError::InvalidInput)
    );
    ns = namespace();
    ns.database = DatabaseId::new([0x22; 16]).unwrap();
    assert_eq!(
        decode_status_home_page(&b, identity(), ns).err(),
        Some(PageError::IdentityMismatch)
    );
    for (offset, value) in [
        (6, 8),
        (7, 1),
        (24, 2),
        (32, 4),
        (40, 5),
        (48, 54),
        (56, 55),
    ] {
        let mut bad = b;
        bad[offset] = value;
        reseal(&mut bad);
        assert!(decode_status_home_page(&bad, identity(), namespace()).is_err());
    }
    let mut too_many = raw_page(&cells(71));
    put16(&mut too_many, 72, 72);
    reseal(&mut too_many);
    assert!(decode_status_home_page(&too_many, identity(), namespace()).is_err());
}

#[test]
fn status_home_lookup_rejects_stale_aliases() {
    let mut c = cells(71);
    c[1] = FREE.to_vec();
    let b = raw_page(&c);
    let view = decode_status_home_page(&b, identity(), namespace()).unwrap();
    for slot in [0, 70] {
        let location = StatusSlotRef::new(page_ref(3), slot, gen_value(20)).unwrap();
        assert_eq!(
            view.lookup(reference(19 + u64::from(slot), 20), location)
                .unwrap()
                .incoming(),
            2
        );
    }
    for slot in [1, 71, 81] {
        let location = StatusSlotRef::new(page_ref(3), slot, gen_value(20)).unwrap();
        assert_eq!(
            view.lookup(reference(19, 20), location),
            Err(PageError::IdentityMismatch)
        );
    }
    let small = raw_page(&cells(1));
    let small = decode_status_home_page(&small, identity(), namespace()).unwrap();
    assert_eq!(
        small.lookup(
            reference(20, 20),
            StatusSlotRef::new(page_ref(3), 1, gen_value(20)).unwrap()
        ),
        Err(PageError::IdentityMismatch)
    );
    let empty = raw_page(&[]);
    let empty = decode_status_home_page(&empty, identity(), namespace()).unwrap();
    assert_eq!(
        empty.lookup(
            reference(19, 20),
            StatusSlotRef::new(page_ref(3), 0, gen_value(20)).unwrap()
        ),
        Err(PageError::IdentityMismatch)
    );
    for case in 0..10 {
        let mut page = page_ref(3);
        let mut generation = gen_value(20);
        let mut id = txn(19, 18);
        let mut expected_generation = gen_value(20);
        match case {
            0 => page.file = FileId::new(0x200000002).unwrap(),
            1 => page.page_no = 4,
            2 => page.incarnation = gen_value(5),
            3 => generation = gen_value(21),
            4 => id.local = 20,
            5 => id.database = DatabaseId::new([0x22; 16]).unwrap(),
            6 => id.timeline = 18,
            7 => expected_generation = gen_value(21),
            8 => {
                generation = gen_value(21);
                expected_generation = gen_value(21);
            }
            9 => id.writer_epoch = 19,
            _ => unreachable!(),
        }
        let location = StatusSlotRef::new(page, 0, generation).unwrap();
        let expected = TxnRef::new(id, expected_generation).unwrap();
        assert_eq!(
            view.lookup(expected, location),
            Err(PageError::IdentityMismatch)
        );
    }
}

fn raw_undo(page: u64) -> [u8; 40] {
    let mut b = [0; 40];
    put64(&mut b, 0, 0x200000001);
    put64(&mut b, 8, page);
    put64(&mut b, 16, 4);
    put64(&mut b, 32, 20);
    b
}
fn undo(page: u64) -> UndoRef {
    UndoRef::new(page_ref(page), 0, gen_value(20)).unwrap()
}

#[test]
fn status_home_generated_profiles_match_independent_fields() {
    let mut executed = 0;
    for seed in 0..64_u64 {
        for sample in 0..128_u64 {
            let n = 128 * seed + sample;
            let state = (seed + sample) % 6;
            let generation = 20 + seed;
            let count = if state == 0 {
                0
            } else {
                [0, 1, u64::MAX - 1, u64::MAX][n as usize % 4]
            };
            let head = (state != 0 && sample % 2 == 1).then_some(31);
            let target = (state == 2 && head.is_some() && sample % 4 == 3).then_some(29);
            let next = if state == 2 && head.is_some() {
                Some(if sample % 8 == 7 { 29 } else { 30 })
            } else if state == 3 && head.is_some() && sample % 4 == 1 {
                Some(30)
            } else {
                None
            };
            let mut expected = [0; 224];
            expected[..4].copy_from_slice(b"XSH1");
            put16(&mut expected, 4, 1);
            put16(&mut expected, 6, 32);
            put64(&mut expected, 8, count);
            let core = &mut expected[32..];
            core[..4].copy_from_slice(b"XTS1");
            put16(core, 4, 1);
            put64(core, 48, generation);
            let value = if state == 0 {
                TransactionStatusCell::Free(gen_value(generation))
            } else {
                core[8..24].copy_from_slice(&std::array::from_fn::<_, 16, _>(|i| i as u8 + 1));
                put64(core, 24, 17);
                core[32..36].copy_from_slice(&(18 + seed as u32 % 3).to_le_bytes());
                put64(core, 40, 19 + n);
                core[6] = [0, 1, 2, 2, 3, 4][state as usize];
                core[7] = if state == 2 {
                    1
                } else if state == 3 {
                    2
                } else {
                    0
                };
                if state == 4 {
                    put64(core, 56, 21 + sample);
                }
                for (index, page) in [head, target, next].into_iter().enumerate() {
                    if let Some(page) = page {
                        core[64] |= 1 << index;
                        core[72 + 40 * index..112 + 40 * index].copy_from_slice(&raw_undo(page));
                    }
                }
                let typed_state = match state {
                    1 => TransactionState::Active,
                    2 => TransactionState::Aborting(RollbackState::Statement {
                        target: target.map(undo),
                        next: next.map(undo),
                    }),
                    3 => TransactionState::Aborting(RollbackState::Transaction {
                        next: next.map(undo),
                    }),
                    4 => TransactionState::Committed(CommitSeq::new(21 + sample).unwrap()),
                    5 => TransactionState::Aborted,
                    _ => unreachable!(),
                };
                let tx =
                    TxnRef::new(txn(19 + n, 18 + seed as u32 % 3), gen_value(generation)).unwrap();
                TransactionStatusCell::Occupied(
                    TransactionStatus::new(tx, typed_state, head.map(undo)).unwrap(),
                )
            };
            let home = StatusHome::new(value, count).unwrap();
            let mut actual = [0x5a; 224];
            home.encode(&mut actual).unwrap();
            assert_eq!(actual, expected);
            let decoded = StatusHome::decode(&expected).unwrap();
            assert_eq!(decoded.core(), value);
            assert_eq!(decoded.incoming(), count);
            executed += 1;
        }
    }
    assert_eq!(executed, 8192);
}
