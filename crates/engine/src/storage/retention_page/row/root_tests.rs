//! Independent cell literals only. No page/census/current DB evidence.
use super::*;
use crate::api::DatabaseId;

const INITIAL: &[u8; 512] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/root3/initial-root.bin"
));
const ACTIVE: &[u8; 512] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/root3/active-root.bin"
));
const FREE: &[u8; 1024] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/root3/free-job.bin"
));
fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn rejected(bytes: &[u8], expected: RetentionError) {
    assert_eq!(
        decode_root(bytes, ns())
            .err()
            .expect("closed Root3 rejection"),
        expected
    );
}

#[test]
fn root3_initial_literals_and_borrowed_view() {
    let mut job = [0xa5; JOB_BYTES];
    free_job(Generation::new(1).unwrap(), &mut job);
    assert_eq!(job.as_slice(), FREE);
    let mut encoded = [0xa5; ROOT_BYTES];
    initial_root(ns(), &mut encoded).unwrap();
    assert_eq!(&encoded, INITIAL);
    let root = decode_root(INITIAL, ns()).unwrap();
    assert_eq!(root.bytes().as_ptr(), INITIAL.as_ptr());
    assert_eq!(root.row_high(), 0);
    assert_eq!(root.undo_high(), 0);
    assert_eq!(root.object_high(), 5);
    assert_eq!(root.commit_c(), 0);
    assert_eq!(root.masks(), [u64::MAX, 0, 0]);
    assert_eq!(root.aging(), None);
    assert_eq!((root.revision(), root.apply(), root.issued()), (1, 0, 0));
    let mut out = [0xa5; ROOT_BYTES];
    encode_root(INITIAL, ns(), &mut out).unwrap();
    assert_eq!(&out, INITIAL);
    // The exact version3 genesis commitment must not admit another Job recipe.
    for at in [128, 159, 160, 191, 432] {
        let mut bad = *INITIAL;
        bad[at] ^= 1;
        rejected(&bad, RetentionError::Corrupt);
    }
}

#[test]
fn root3_active_high_water_and_accounting() {
    let root = decode_root(ACTIVE, ns()).unwrap();
    assert_eq!(root.bytes().as_ptr(), ACTIVE.as_ptr());
    assert_eq!(
        (root.revision(), root.cursor(), root.apply(), root.issued()),
        (42, 17, 99, 8)
    );
    assert_eq!(
        (root.completed(), root.canceled(), root.non_free()),
        (3, 4, 1)
    );
    assert_eq!(
        (
            root.undo_high(),
            root.object_high(),
            root.commit_c(),
            root.row_high()
        ),
        (7, 6, 2, 7)
    );
    assert_eq!(
        (root.consumed_wal(), root.consumed_control()),
        (123456, 16384)
    );
    assert_eq!(
        (root.aging().unwrap().kind(), root.aging().unwrap().id()),
        (SystemWorkKind::Aging, 1)
    );
    // These opaque active commitments require the future full Job3 census.
    assert_eq!(root.job_digest(), &[0x33; 32]);
    assert_eq!(root.budget_digest(), &[0x44; 32]);
    for high in [0, 1, u64::MAX] {
        let mut bytes = *ACTIVE;
        put(&mut bytes, 432, high);
        let root = decode_root(&bytes, ns()).unwrap();
        assert_eq!(root.row_high(), high);
        let mut encoded = [0xa5; ROOT_BYTES];
        encode_root(&bytes, ns(), &mut encoded).unwrap();
        assert_eq!(encoded, bytes);
    }
    // Source-cell accounting is checked arithmetic, including overflow.
    for (at, value) in [
        (64, 0),
        (72, 64),
        (80, 0),
        (88, 7),
        (376, u64::MAX),
        (400, 4),
    ] {
        let mut bad = *ACTIVE;
        put(&mut bad, at, value);
        rejected(&bad, RetentionError::OrderMismatch);
    }
    let mut maximum = *ACTIVE;
    put(&mut maximum, 96, 0xff80_0000_0000_0000);
    put(&mut maximum, 104, 0x007f_ffff_ffff_ffff);
    put(&mut maximum, 88, 62);
    assert_eq!(decode_root(&maximum, ns()).unwrap().non_free(), 55);
    put(&mut maximum, 96, 0xff00_0000_0000_0000);
    put(&mut maximum, 104, 0x00ff_ffff_ffff_ffff);
    put(&mut maximum, 88, 63);
    rejected(&maximum, RetentionError::Corrupt);
}

#[test]
fn root3_closed_shape_and_identity_rejections() {
    for length in 0..ROOT_BYTES {
        rejected(&INITIAL[..length], RetentionError::Corrupt);
    }
    rejected(&[0; ROOT_BYTES + 1], RetentionError::Corrupt);
    for version in [0u16, 1, 2, 4, u16::MAX] {
        let mut bad = *ACTIVE;
        bad[4..6].copy_from_slice(&version.to_le_bytes());
        rejected(&bad, RetentionError::UnsupportedFormat);
    }
    let mut bad = *ACTIVE;
    bad[6] = 2;
    rejected(&bad, RetentionError::UnsupportedState);
    for at in [0, 7, 8, 12, 120, 127, 440, 511] {
        let mut bad = *ACTIVE;
        bad[at] ^= 1;
        rejected(&bad, RetentionError::Corrupt);
    }
    for (at, value) in [(96, u64::MAX), (96, u64::MAX - 3), (112, 1)] {
        let mut bad = *ACTIVE;
        put(&mut bad, at, value);
        rejected(&bad, RetentionError::Corrupt);
    }
    for at in [192, 208, 216, 224, 240, 264, 288, 312, 336, 360] {
        let mut bad = *ACTIVE;
        bad[at] ^= 1;
        rejected(&bad, RetentionError::IdentityMismatch);
    }
    // Malformed FileId/zero incarnation fail PageRef shape before identity.
    for at in [232, 248, 256, 272, 280, 296, 304, 320, 328, 344, 352, 368] {
        let mut bad = *ACTIVE;
        bad[at] ^= 1;
        rejected(
            &bad,
            RetentionError::Page(crate::storage::page::PageError::Corrupt),
        );
    }
    for at in [16, 32, 40, 48, 56] {
        let mut bad = *ACTIVE;
        bad[at] ^= 1;
        rejected(&bad, RetentionError::IdentityMismatch);
    }
    let mut namespace = ns();
    namespace.database = DatabaseId::new([0x42; 16]).unwrap();
    assert_eq!(
        decode_root(ACTIVE, namespace).err(),
        Some(RetentionError::IdentityMismatch)
    );
    namespace = ns();
    namespace.timeline = 2;
    assert_eq!(
        decode_root(ACTIVE, namespace).err(),
        Some(RetentionError::InvalidInput)
    );
}

#[test]
fn root3_encode_failures_preserve_output() {
    let mut output = [0xa5; ROOT_BYTES];
    let mut bad = *ACTIVE;
    bad[511] = 1;
    assert_eq!(
        encode_root(&bad, ns(), &mut output),
        Err(RetentionError::Corrupt)
    );
    assert_eq!(output, [0xa5; ROOT_BYTES]);
    assert_eq!(
        encode_root(ACTIVE, ns(), &mut output[..511]),
        Err(RetentionError::InvalidInput)
    );
    assert_eq!(output, [0xa5; ROOT_BYTES]);
    let mut namespace = ns();
    namespace.timeline = 2;
    assert_eq!(
        initial_root(namespace, &mut output),
        Err(RetentionError::InvalidInput)
    );
    assert_eq!(output, [0xa5; ROOT_BYTES]);
}
