use super::*;
use crate::storage::{
    control,
    plan4_recovery_base::{ControlRef, Expected, SelectedControl},
};
use foundation::codec::content_digest;
const BASE: usize = 2_097_152;
const RECORD: usize = 331_776;
const INPUT: &[u8; 11 * RECORD] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/actor/cases.bin"
));
const INITIAL: &[u8; BASE] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/base4/initial.xrb"
));
const PROFILE: &[u8; 12288] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
fn expected() -> Expected {
    Expected {
        profile: control::row_profile::decode_profile(&PROFILE[..4096], None).unwrap(),
        filemap: SelectedControl {
            generation: 1,
            reference: ControlRef::decode(&INITIAL[384..448]).unwrap(),
        },
        bootstrap_digest: INITIAL[464..496].try_into().unwrap(),
    }
}
fn get(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn put(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn start(b: &[u8], slot: usize) -> usize {
    u16::from_le_bytes(b[128 + 4 * slot..130 + 4 * slot].try_into().unwrap()) as usize
}
fn seal(b: &mut [u8]) {
    let end = b.len() - 32;
    let d = content_digest(&b[..end]);
    b[end..].copy_from_slice(&d);
}
fn reseal(b: &mut [u8; BASE]) {
    for (i, (at, len)) in [
        (4096, 55296),
        (65536, 6144),
        (73728, 22528),
        (98304, 16384),
        (262144, 64 * 16384),
    ]
    .into_iter()
    .enumerate()
    {
        let d = content_digest(&b[at..at + len]);
        b[1312 + i * 64..1344 + i * 64].copy_from_slice(&d);
    }
    seal(b);
}
fn image_mut(b: &mut [u8; BASE], n: usize) -> &mut [u8; PAGE_BYTES] {
    (&mut b[262144 + n * PAGE_BYTES..262144 + (n + 1) * PAGE_BYTES])
        .try_into()
        .unwrap()
}
fn image_changed(b: &mut [u8; BASE], n: usize) {
    let p = image_mut(b, n);
    seal(p);
    let d = content_digest(p);
    let header: [u8; 48] = p[24..72].try_into().unwrap();
    b[65536 + n * 96 + 8..65536 + n * 96 + 56].copy_from_slice(&header);
    b[65536 + n * 96 + 56..65536 + n * 96 + 88].copy_from_slice(&d);
    reseal(b);
}
fn fixture(n: usize) -> Box<[u8; BASE]> {
    let mut b: Box<[u8; BASE]> = INITIAL.to_vec().into_boxed_slice().try_into().unwrap();
    let input = &INPUT[n * RECORD..(n + 1) * RECORD];
    b[..4096].copy_from_slice(&input[..4096]);
    b[65536..98304].copy_from_slice(&input[4096..36864]);
    b[262144..262144 + 18 * PAGE_BYTES].copy_from_slice(&input[36864..]);
    reseal(&mut b);
    b
}
fn actor() -> TxnRef {
    TxnRef::decode(&INPUT[36864 + 9 * PAGE_BYTES + 16128 + 40..36864 + 9 * PAGE_BYTES + 16128 + 88])
        .unwrap()
}
fn check(b: &[u8; BASE]) -> Result<RowActorObservation> {
    let layout = super::super::decode_layout(b, expected()).expect("independent raw layout");
    let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
    observe(layout, actor(), &mut scratch)
}

#[test]
fn actor_literal_physical_graphs() {
    let expected = [
        (0, 0, 0),
        (0, 3, 0),
        (0, 3, 0),
        (0, 1, 1),
        (0, 2, 1),
        (0, 1, 2),
        (0, 0, 1),
        (0, 0, 2),
        (1, 1, 0),
    ];
    for (n, (reserved, retained, releasing)) in expected.into_iter().enumerate() {
        assert_eq!(
            check(&fixture(n)),
            Ok(RowActorObservation {
                reserved,
                retained,
                releasing,
                transaction_rows: retained
            }),
            "literal {n}"
        );
    }
}

#[test]
fn actor_rejects_wrong_chain_counts_identity_and_before() {
    for mutation in 0..9 {
        let mut b = fixture(1);
        let n = if mutation == 3 { 12 } else { 14 };
        let p = image_mut(&mut b, n);
        let at = start(p, 0);
        match mutation {
            0 => {
                put(p, at + 128, 12);
                put(p, at + 160, 1);
            } // skip real predecessor U13
            1 => {
                put(p, at + 128, 14);
                put(p, at + 160, 3);
            } // self cycle
            2 => put(p, at + 160, 1),  // exact reference, wrong sequence
            3 => put(p, at + 80, 0),   // old head net-zero proof still requires incoming2
            4 => put(p, at + 56, 8),   // foreign creator local id
            5 => put(p, at + 64, 3),   // wrong status generation
            6 => put(p, at + 112, 99), // wrong Row target
            7 => put(p, at + 72, 99),  // wrong logical mutation
            8 => p[at + 324] = 1,      // occupied before forbidden
            _ => unreachable!(),
        }
        image_changed(&mut b, n);
        assert!(check(&b).is_err(), "mutation {mutation}");
    }
}

#[test]
fn actor_rejects_uncut_release_and_cursor_mismatch() {
    for mutation in 0..3 {
        let mut b = fixture(3);
        let p = image_mut(&mut b, 13);
        let at = start(p, 0);
        match mutation {
            0 => p[at + 326] = 0,
            1 => p[at + 328] = 1,
            2 => put(p, at + 80, 2),
            _ => unreachable!(),
        }
        image_changed(&mut b, 13);
        assert!(check(&b).is_err(), "release {mutation}");
    }
}

#[test]
fn actor_rejects_row_target_schema_version_and_projection() {
    for mutation in 0..7 {
        let mut b = fixture(1);
        let n = if mutation == 6 { 8 } else { 11 };
        let p = image_mut(&mut b, n);
        let at = start(p, 1) + 12;
        match mutation {
            0 => put(p, at + 8, 2),     // key BE and record LE disagreement
            1 => put(p, at + 112, 2),   // wrong mutation
            2 => put(p, at + 152, 3),   // wrong Undo generation
            3 => put(p, at + 64, 3),    // same actor id wrong stamp generation
            4 => put(p, at + 173, 999), // valid value, stale planned projection
            5 => p[at + 6] = 1,         // deleted/non-inline is outside the profile
            6 => p[at + 232 + 7] = 1,   // schema Int64 -> Bool
            _ => unreachable!(),
        }
        image_changed(&mut b, n);
        assert!(check(&b).is_err(), "row {mutation}");
    }
}

#[test]
fn actor_rejects_orphan_physical_undo() {
    let mut b = fixture(1);
    let copy = *image_mut(&mut b, 14);
    *image_mut(&mut b, 15) = copy;
    put(image_mut(&mut b, 15), 32, 15);
    let cert: [u8; 96] = b[65536 + 14 * 96..65536 + 15 * 96].try_into().unwrap();
    b[65536 + 15 * 96..65536 + 16 * 96].copy_from_slice(&cert);
    let mut provenance: [u8; 352] = b[73728 + 14 * 352..73728 + 15 * 352].try_into().unwrap();
    put(&mut provenance, 16, 15);
    b[73728 + 15 * 352..73728 + 16 * 352].copy_from_slice(&provenance);
    let p = image_mut(&mut b, 0);
    let at = start(p, 0) + 64;
    let item: [u8; 96] = p[at + 14 * 96..at + 15 * 96].try_into().unwrap();
    p[at + 15 * 96..at + 16 * 96].copy_from_slice(&item);
    image_changed(&mut b, 0);
    image_changed(&mut b, 15);
    assert_eq!(check(&b), Err(Error::Identity));
}

#[test]
fn actor_rejects_foreign_actor_and_orphan_row() {
    let b = fixture(1);
    let layout = super::super::decode_layout(&b[..], expected()).unwrap();
    let mut raw = [0; 48];
    actor().encode(&mut raw).unwrap();
    put(&mut raw, 32, 8);
    let foreign = TxnRef::decode(&raw).unwrap();
    let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
    assert_eq!(observe(layout, foreign, &mut scratch), Err(Error::Identity));
    assert_eq!(get(&b[..], 264), 512);
    assert_eq!(check(&fixture(9)), Err(Error::Identity));
}

#[test]
fn actor_root_free_row_high_water_boundary() {
    for id in [20, 21] {
        let mut b = fixture(6);
        let p = image_mut(&mut b, 11);
        let at = start(p, 1);
        p[at + 4..at + 12].copy_from_slice(&u64::to_be_bytes(id));
        put(p, at + 20, id);
        image_changed(&mut b, 11);
        if id == 20 {
            assert!(check(&b).is_ok());
        } else {
            assert_eq!(check(&b), Err(Error::Order));
        }
    }
}

#[test]
fn actor_orphan_cannot_hide_in_noncanonical_free() {
    let mut b = fixture(1);
    let copy = *image_mut(&mut b, 14);
    *image_mut(&mut b, 15) = copy;
    put(image_mut(&mut b, 15), 32, 15);
    let cert: [u8; 96] = b[65536 + 14 * 96..65536 + 15 * 96].try_into().unwrap();
    b[65536 + 15 * 96..65536 + 16 * 96].copy_from_slice(&cert);
    let mut provenance: [u8; 352] = b[73728 + 14 * 352..73728 + 15 * 352].try_into().unwrap();
    put(&mut provenance, 16, 15);
    b[73728 + 15 * 352..73728 + 16 * 352].copy_from_slice(&provenance);
    let p = image_mut(&mut b, 0);
    let at = start(p, 0) + 64;
    let item: [u8; 96] = p[at + 14 * 96..at + 15 * 96].try_into().unwrap();
    p[at + 15 * 96..at + 16 * 96].copy_from_slice(&item);
    image_changed(&mut b, 0);
    let p = image_mut(&mut b, 15);
    let at = start(p, 0);
    p[at + 6] = 0;
    image_changed(&mut b, 15);
    assert!(matches!(check(&b), Err(Error::Undo(_))));
    let p = image_mut(&mut b, 15);
    p[at..at + 432].fill(0);
    p[at..at + 6].copy_from_slice(b"XUR1\x01\0");
    put(p, at + 8, 2);
    image_changed(&mut b, 15);
    assert_eq!(
        check(&b),
        Ok(RowActorObservation {
            reserved: 0,
            retained: 3,
            releasing: 0,
            transaction_rows: 3
        })
    );
}

#[test]
fn actor_root_free_rows_remain_globally_unique() {
    assert!(check(&fixture(10)).is_ok());
    for mutation in 0..2 {
        let mut b = fixture(10);
        let p = image_mut(&mut b, 17);
        let at = start(p, 1);
        if mutation == 0 {
            p[at + 4..at + 12].copy_from_slice(&1u64.to_be_bytes());
            put(p, at + 20, 1);
        } else {
            put(p, at + 12 + 112, 1);
        }
        image_changed(&mut b, 17);
        assert_eq!(check(&b), Err(Error::Identity));
    }
}

#[test]
fn actor_root_free_same_leaf_rejects_duplicate_logical_version() {
    let mut b = fixture(10);
    let layout = super::super::decode_layout(&b[..], expected()).unwrap();
    let first = layout.certificate(11).unwrap().unwrap().header;
    let second = layout.certificate(17).unwrap().unwrap().header;
    let first_page = page::decode_page(
        layout.image(11).unwrap().unwrap(),
        first.identity,
        PageKind::RowLeaf,
    )
    .unwrap();
    let second_page = page::decode_page(
        layout.image(17).unwrap().unwrap(),
        second.identity,
        PageKind::RowLeaf,
    )
    .unwrap();
    let metadata = first_page.cell(0).unwrap().to_vec();
    let row1 = first_page.cell(1).unwrap().to_vec();
    let mut row19 = second_page.cell(1).unwrap().to_vec();
    page::encode_page(first, &[&metadata, &row1, &row19], image_mut(&mut b, 11)).unwrap();
    page::encode_page(second, &[&metadata], image_mut(&mut b, 17)).unwrap();
    image_changed(&mut b, 11);
    image_changed(&mut b, 17);
    assert!(check(&b).is_ok());
    put(&mut row19, 12 + 112, 1);
    page::encode_page(first, &[&metadata, &row1, &row19], image_mut(&mut b, 11)).unwrap();
    image_changed(&mut b, 11);
    assert_eq!(check(&b), Err(Error::Identity));
}
