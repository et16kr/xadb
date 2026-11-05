use super::*;
use crate::storage::{doublewrite_control as dwb, page::PageLsn};
macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-format/",
            $name
        ))
    };
}
fn profile() -> Profile {
    crate::storage::control::row_profile::decode_profile(
        &fixture!("envelopes/profiles.bin")[..4096],
        None,
    )
    .unwrap()
}
fn image(level: u8) -> &'static [u8; PAGE_BYTES] {
    let (bytes, n): (&[u8], usize) = match level {
        0 => (fixture!("base4-closure/live.xrb"), 11),
        1 => (fixture!("current-tree/root-split.xrb"), 17),
        _ => (fixture!("current-tree/internal-root-split.xrb"), 21),
    };
    bytes[262144 + n * PAGE_BYTES..262144 + (n + 1) * PAGE_BYTES]
        .try_into()
        .unwrap()
}
fn seal(p: &mut [u8; PAGE_BYTES]) {
    let digest = content_digest(&p[..PAGE_BYTES - 32]);
    p[PAGE_BYTES - 32..].copy_from_slice(&digest);
}

#[test]
fn structural_analysis_target_table_preserves_level_and_rejects_noncanonical_kind_word() {
    for level in [0, 1, 2] {
        let p = image(level);
        let index = u64_at(p, 32) as usize;
        let req = canonical_request(profile(), index, p).unwrap();
        assert_eq!(req.level, level);
        let mut table = [0; 64 * 72];
        note_target(&mut table, req, p).unwrap();
        let word = u64_at(&table, index * 72 + 48);
        assert_eq!(word, req.kind as u64 | (u64::from(level) << 8));
        let (decoded, lsn, digest) = target_record(&table, profile(), index).unwrap();
        assert_eq!(decoded, req);
        assert_eq!(lsn, u64_at(p, 64));
        assert_eq!(digest, content_digest(p));
        for bad in [
            word | (1 << 16),
            word | (1 << 63),
            (req.kind as u64) | (16 << 8),
        ] {
            put64(&mut table, index * 72 + 48, bad);
            assert!(target_record(&table, profile(), index).is_err());
        }
        if level > 0 {
            put64(&mut table, index * 72 + 48, req.kind as u64);
            assert!(target_record(&table, profile(), index).is_err());
        } else {
            put64(&mut table, index * 72 + 48, word | (1 << 8));
            assert!(target_record(&table, profile(), index).is_err());
        }
        let mut wrong = req;
        wrong.level = level + 1;
        let saved = table;
        assert!(note_target(&mut table, wrong, p).is_err());
        assert_eq!(table, saved);
    }
    let p = image(1);
    let mut raw = [0; 4096];
    crate::storage::control::encode_catalog_profile(profile().spec(), &mut raw).unwrap();
    let legacy = crate::storage::control::decode_bootstrap_profile(&raw, None).unwrap();
    assert!(canonical_request(legacy, 17, p).is_err());
    assert!(
        request_for_kind(
            profile(),
            17,
            Generation::new(2).unwrap(),
            PageKind::RowInternal,
            1,
            ObjectId::new(5).unwrap(),
            Generation::new(1).unwrap()
        )
        .is_err()
    );
    assert!(
        request_for_kind(
            profile(),
            17,
            Generation::new(2).unwrap(),
            PageKind::RowInternal,
            1,
            ObjectId::new(6).unwrap(),
            Generation::new(2).unwrap()
        )
        .is_err()
    );
}

#[test]
fn structural_analysis_home_level_checks_do_not_confuse_future_incarnation() {
    let current = image(2);
    let mut home = *current;
    home[7] = 1;
    seal(&mut home);
    let (mut matched, mut repair) = (0, 0);
    assert!(
        observe_home(
            profile(),
            21,
            current,
            &home,
            true,
            false,
            &mut matched,
            &mut repair
        )
        .is_err()
    );
    assert_eq!((matched, repair), (0, 0));
    put64(&mut home, 40, u64_at(current, 40) + 1);
    seal(&mut home);
    observe_home(
        profile(),
        21,
        current,
        &home,
        true,
        false,
        &mut matched,
        &mut repair,
    )
    .unwrap();
    assert_eq!((matched, repair), (0, 0));
    home = *current;
    put64(&mut home, 64, u64_at(current, 64) - 1);
    seal(&mut home);
    observe_home(
        profile(),
        21,
        current,
        &home,
        true,
        false,
        &mut matched,
        &mut repair,
    )
    .unwrap();
    assert_eq!(repair, 1 << 21);
    assert_eq!(matched, 0);
}

fn pending(entry: dwb::PendingImage) -> (SelectedControl, Vec<u8>) {
    let mut raw = vec![0; 4096];
    let len = dwb::encode_control(
        dwb::ControlHeader {
            namespace: profile().spec().namespace,
            generation: 1,
            batch_id: 1,
            state: dwb::ControlState::Pending,
            max_lsn: entry.lsn,
        },
        &[entry],
        &mut raw,
    )
    .unwrap()
    .len();
    raw.truncate(len);
    let reference = crate::storage::control::ControlRef {
        kind: crate::storage::control::ControlKind::Doublewrite,
        slot: 0,
        index: 0,
        generation: 1,
        encoded_len: len as u32,
        digest: content_digest(&raw),
    };
    (
        SelectedControl {
            generation: 1,
            reference,
        },
        raw,
    )
}

#[test]
fn structural_pending_requires_exact_level_for_same_incarnation() {
    let current = image(2);
    let req = canonical_request(profile(), 21, current).unwrap();
    let entry = dwb::PendingImage {
        slot: 0,
        page: req.identity.page,
        owner: req.identity.owner,
        owner_generation: req.identity.owner_generation,
        kind: req.kind,
        level: req.level,
        lsn: PageLsn(u64_at(current, 64)),
        image_digest: content_digest(current),
    };
    let (selected, raw) = pending(entry);
    let mut matched = 0;
    physical::observe_pending(profile(), selected, &raw, req, current, &mut matched).unwrap();
    assert_eq!(matched, 1 << 21);
    let mut table = [0; 64 * 72];
    note_target(&mut table, req, current).unwrap();
    physical::validate_pending_membership(profile(), selected, &raw, 1 << 21, matched, &table)
        .unwrap();
    let (selected, raw) = pending(dwb::PendingImage { level: 1, ..entry });
    matched = 0;
    assert!(
        physical::observe_pending(profile(), selected, &raw, req, current, &mut matched).is_err()
    );
    assert_eq!(matched, 0);
    let (selected, raw) = pending(dwb::PendingImage {
        page: PageRef {
            incarnation: Generation::new(3).unwrap(),
            ..entry.page
        },
        level: 1,
        ..entry
    });
    physical::observe_pending(profile(), selected, &raw, req, current, &mut matched).unwrap();
    assert_eq!(matched, 0);
    assert!(
        physical::validate_pending_membership(profile(), selected, &raw, 1 << 21, matched, &table)
            .is_err()
    );
}
