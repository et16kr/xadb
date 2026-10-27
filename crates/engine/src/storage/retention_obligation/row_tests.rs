use super::*;
use crate::storage::{control, obligation::RemainingBudget};
const PAGES: &[u8; 6 * PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/aging/pages.bin"
));
const JOBS: &[u8; 5 * 65536] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/aging/jobs.bin"
));
const DESCRIPTORS: &[u8; 5 * 1536] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/aging/descriptors.bin"
));
const AGGREGATES: &[u8; 5 * 640] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/aging/aggregates.bin"
));
const PROFILES: &[u8; 12288] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
fn profile() -> Profile {
    control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap()
}
fn page(n: usize) -> &'static [u8; PAGE_BYTES] {
    PAGES[n * PAGE_BYTES..(n + 1) * PAGE_BYTES]
        .try_into()
        .unwrap()
}
fn descriptor(n: usize) -> &'static [u8; 1536] {
    DESCRIPTORS[n * 1536..(n + 1) * 1536].try_into().unwrap()
}
fn jobs(n: usize) -> &'static [[u8; 1024]; 64] {
    JOBS[n * 65536..(n + 1) * 65536]
        .as_chunks::<1024>()
        .0
        .try_into()
        .unwrap()
}
fn input(n: usize) -> AgingTransition<'static> {
    AgingTransition {
        profile: profile(),
        before_descriptor: if n == 0 {
            None
        } else {
            Some(descriptor(n - 1))
        },
        before_root: page(n),
        after_root: page(n + 1),
        jobs: row::JobCells::original(jobs(n)),
    }
}
fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let hash = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash);
}
fn put(bytes: &mut [u8], at: usize, v: u64) {
    bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

#[test]
fn row_aging_literal_bindings_and_candidates() {
    for i in 0..5 {
        let raw = descriptor(i);
        let view = decode(raw, profile(), page(i + 1)).unwrap();
        assert!(std::ptr::eq(view.bytes(), raw));
        let spec = view.descriptor();
        assert_eq!(spec.remaining[3].peak.registry, [0, 9, 16, 16, 0][i]);
        let mut output = [0xa5; 1536];
        encode(&spec, profile(), page(i + 1), &mut output).unwrap();
        assert_eq!(&output, raw);
        let mut aggregate = [0xa5; 640];
        next(input(i), &mut aggregate, &mut output).unwrap();
        assert_eq!(&output, raw);
        assert_eq!(aggregate, AGGREGATES[i * 640..(i + 1) * 640]);
        assert_eq!(spec.chain.birth, 1);
        assert_eq!(spec.chain.groups, i as u64 + 1);
        assert_eq!(spec.inventory.change_lsn, 1);
        assert_eq!(spec.inventory.items, 0);
        // Generic empty-work decoder remains closed to the Root-bound work body.
        assert_eq!(
            obligation::decode_descriptor(raw, spec.owner.reference(), spec.profile).err(),
            Some(ObligationError::UnsupportedWorkBody)
        );
    }
}

#[test]
fn row_aging_exact_profile_root_and_work_binding() {
    let raw = descriptor(2);
    let mut old_profile = PROFILES[..4096].to_vec();
    old_profile[4] = 2;
    seal(&mut old_profile);
    let old = control::decode_bootstrap_profile(&old_profile, None).unwrap();
    assert!(decode(raw, old, page(3)).is_err());
    assert!(decode(raw, profile(), page(0)).is_err());
    for field in [
        24, 72, 80, 88, 96, 360, 368, 376, 384, 392, 400, 416, 448, 544, 552, 560, 608, 616, 624,
        1336, 1344,
    ] {
        let mut bad = *raw;
        bad[field] ^= 1;
        seal(&mut bad);
        assert!(decode(&bad, profile(), page(3)).is_err(), "field {field}");
    }
    let mut wrong = *page(3);
    wrong[15840 + 4] = 2;
    seal(&mut wrong);
    assert!(decode(raw, profile(), &wrong).is_err());
    for n in [0, 1535, 1537] {
        assert!(decode(&vec![0; n], profile(), page(3)).is_err());
    }
    let mut bad = *raw;
    bad[1504] ^= 1;
    assert!(decode(&bad, profile(), page(3)).is_err());
}

#[test]
fn row_aging_rejects_changed_queue_and_debt() {
    for n in [1, 2, 3] {
        let mut bank = *jobs(n);
        bank[0][64] ^= 8;
        let mut selected = input(n);
        selected.jobs = row::JobCells::original(&bank);
        let mut out = [0xa5; 1536];
        assert!(next(selected, &mut [0; 640], &mut out).is_err());
        assert_eq!(out, [0xa5; 1536]);
    }
    // Correct summary can still hide an invalid actor relationship; full census
    // must reject the candidate even after its commitments are recomputed.
    let mut bank = *jobs(2);
    put(&mut bank[1], 48, 99);
    let mut aggregate = [0; 640];
    let cells = row::JobCells::original(&bank);
    let (_, digest) =
        row::summarize_jobs(cells, profile().spec().namespace, &mut aggregate).unwrap();
    let mut after = *page(3);
    after[15840 + 128..15840 + 160].copy_from_slice(&digest);
    after[15840 + 160..15840 + 192].copy_from_slice(&content_digest(&aggregate));
    seal(&mut after);
    let mut selected = input(2);
    selected.jobs = cells;
    selected.after_root = &after;
    let mut out = [0xa5; 1536];
    assert!(next(selected, &mut aggregate, &mut out).is_err());
    assert_eq!(out, [0xa5; 1536]);
    // The borrowed one-cell replacement is the actual after census, without
    // copying a second bank in the producer.
    let mut selected = input(2);
    selected.jobs = cells.replace(1, &jobs(2)[1]).unwrap();
    next(selected, &mut aggregate, &mut out).unwrap();
    assert_eq!(&out, descriptor(2));
    // Descriptor's debt is root-bound, including every scalar/peak/category.
    let spec = decode(descriptor(2), profile(), page(3))
        .unwrap()
        .descriptor();
    for field in [8, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112] {
        let mut bad = *descriptor(2);
        bad[640 + 384 + field] ^= 1;
        seal(&mut bad);
        assert!(decode(&bad, profile(), page(3)).is_err());
    }
    let mut bad = spec;
    bad.remaining = [RemainingBudget::default(); 5];
    let mut out = [0xa5; 1536];
    assert!(encode(&bad, profile(), page(3), &mut out).is_err());
    assert_eq!(out, [0xa5; 1536]);
}

#[test]
fn row_aging_predecessor_order_and_start_contract() {
    let mut out = [0xa5; 1536];
    let mut wrong = input(0);
    wrong.before_root = page(1);
    assert!(next(wrong, &mut [0; 640], &mut out).is_err());
    let mut wrong = input(2);
    wrong.before_descriptor = None;
    assert!(next(wrong, &mut [0; 640], &mut out).is_err());
    let mut wrong = input(2);
    wrong.before_descriptor = Some(descriptor(2));
    wrong.before_root = page(3);
    assert!(next(wrong, &mut [0; 640], &mut out).is_err());
    for (n, at, value) in [
        (0, 64, 3),
        (0, 416, 19673),
        (0, 424, 1),
        (2, 64, 5),
        (2, 416, 19671),
        (2, 424, 1),
    ] {
        let mut after = *page(n + 1);
        put(&mut after, 15840 + at, value);
        seal(&mut after);
        let mut wrong = input(n);
        wrong.after_root = &after;
        assert!(next(wrong, &mut [0; 640], &mut out).is_err());
        assert_eq!(out, [0xa5; 1536]);
    }
    for (at, value) in [(72, 1), (392, 1), (400, 6), (408, 1), (432, 1)] {
        let mut after = *page(1);
        put(&mut after, 15840 + at, value);
        seal(&mut after);
        let mut wrong = input(0);
        wrong.after_root = &after;
        assert!(next(wrong, &mut [0; 640], &mut out).is_err());
        assert_eq!(out, [0xa5; 1536]);
    }
    let mut after = *page(2);
    put(&mut after, 15840 + 64, 2);
    put(&mut after, 15840 + 416, 19672);
    seal(&mut after);
    let mut wrong = input(0);
    wrong.after_root = &after;
    wrong.jobs = row::JobCells::original(jobs(1));
    assert!(next(wrong, &mut [0; 640], &mut out).is_err());
    assert_eq!(out, [0xa5; 1536]);
    let mut bank = *jobs(0);
    for cell in &mut bank {
        put(cell, 8, 2);
    }
    let mut aggregate = [0; 640];
    let cells = row::JobCells::original(&bank);
    let (_, digest) =
        row::summarize_jobs(cells, profile().spec().namespace, &mut aggregate).unwrap();
    let mut after = *page(1);
    after[15840 + 128..15840 + 160].copy_from_slice(&digest);
    seal(&mut after);
    let mut wrong = input(0);
    wrong.after_root = &after;
    wrong.jobs = cells;
    assert!(next(wrong, &mut aggregate, &mut out).is_err());
    assert_eq!(out, [0xa5; 1536]);
    let mut before = *descriptor(1);
    before[416] ^= 1;
    seal(&mut before);
    let mut wrong = input(2);
    wrong.before_descriptor = Some(&before);
    assert!(next(wrong, &mut [0; 640], &mut out).is_err());
    assert_eq!(out, [0xa5; 1536]);
}

#[test]
fn row_aging_output_failure_and_raw_scope() {
    let spec = decode(descriptor(2), profile(), page(3))
        .unwrap()
        .descriptor();
    for n in [0, 1535, 1537] {
        let mut output = vec![0xa5; n];
        assert!(encode(&spec, profile(), page(3), &mut output).is_err());
        assert_eq!(output, vec![0xa5; n]);
        assert!(next(input(2), &mut [0; 640], &mut output).is_err());
        assert_eq!(output, vec![0xa5; n]);
    }
    // Raw decode checks its Root commitment; the separate next producer is the
    // entry that calls the complete64 Job census, not a current-history owner.
    let mut root = *page(3);
    root[15840 + 128] ^= 1;
    seal(&mut root);
    let mut raw = *descriptor(2);
    raw[416..448].copy_from_slice(&content_digest(&root));
    raw[448..480].copy_from_slice(&content_digest(&root[15840..16352]));
    seal(&mut raw);
    assert!(decode(&raw, profile(), &root).is_ok());
    let mut wrong = input(2);
    wrong.after_root = &root;
    let mut out = [0xa5; 1536];
    assert!(next(wrong, &mut [0; 640], &mut out).is_err());
    assert_eq!(out, [0xa5; 1536]);
}
