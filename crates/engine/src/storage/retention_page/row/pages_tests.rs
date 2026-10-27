//! New envelope entry points over independently authored Root/Job cells.
use super::*;
use crate::{
    api::DatabaseId,
    storage::page::{self, PAGE_BYTES, PageError, PageKind, PageLsn},
};
const PAGES: &[u8; 12 * PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/pages.bin"
));
fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn literal(i: usize) -> [u8; PAGE_BYTES] {
    PAGES[i * PAGE_BYTES..(i + 1) * PAGE_BYTES]
        .try_into()
        .unwrap()
}
fn start(page: &[u8], slot: usize) -> usize {
    usize::from(u16::from_le_bytes(
        page[128 + 4 * slot..130 + 4 * slot].try_into().unwrap(),
    ))
}
fn seal(page: &mut [u8]) {
    let end = page.len() - 32;
    let digest = content_digest(&page[..end]);
    page[end..].copy_from_slice(&digest);
}

#[test]
fn row_pages_literal_envelopes_and_census() {
    for group in 0..2 {
        let root_bytes = literal(group * 6);
        let (root_header, root) = decode_root_page(&root_bytes, ns()).unwrap();
        assert_eq!(root_header.lsn.0, if group == 0 { 0 } else { 99 });
        assert_eq!(
            root.bytes().as_ptr(),
            root_bytes[start(&root_bytes, 0)..].as_ptr()
        );
        let mut encoded = [0xa5; PAGE_BYTES];
        encode_root_page(root.bytes(), ns(), &mut encoded).unwrap();
        assert_eq!(encoded, root_bytes);
        let mut jobs = [[0; JOB_BYTES]; JOB_COUNT];
        for index in 0..5 {
            let input = literal(group * 6 + 1 + index);
            let view = decode_jobs_page(&input, ns(), index as u8).unwrap();
            assert_eq!(view.len(), if index == 4 { 4 } else { 15 });
            assert_eq!(view.header().lsn.0, root_header.lsn.0);
            let mut cells: [&[u8]; 15] = [&[]; 15];
            for slot in 0..view.len() {
                cells[slot] = view.cell(slot).unwrap();
                assert_eq!(cells[slot].as_ptr(), input[start(&input, slot)..].as_ptr());
                jobs[index * 15 + slot].copy_from_slice(cells[slot]);
            }
            encode_jobs_page(
                ns(),
                index as u8,
                root_header.lsn.0,
                &cells[..view.len()],
                &mut encoded,
            )
            .unwrap();
            assert_eq!(encoded, input);
        }
        // This explicit extra call, not the page wrappers themselves, checks
        // complete64-cell relationships and commitments, including genesis.
        verify_census(root, ns(), &jobs, &mut [0; 640]).unwrap();
    }
}

#[test]
fn row_pages_closed_identity_cardinality_and_versions() {
    for size in [0, PAGE_BYTES - 1, PAGE_BYTES + 1] {
        let bytes = vec![0; size];
        assert_eq!(
            decode_root_page(&bytes, ns()).err(),
            Some(RetentionError::Page(PageError::Corrupt))
        );
        assert_eq!(
            decode_jobs_page(&bytes, ns(), 0).err(),
            Some(RetentionError::Page(PageError::Corrupt))
        );
    }
    for index in 0..6 {
        let original = literal(index);
        for at in [8, 32, 40, 48, 56] {
            let mut bad = original;
            // Keep a structurally valid nonzero generation/identity.
            if at == 8 {
                bad[at] ^= 1;
            } else {
                put(&mut bad, at, 2);
            }
            if at == 32 {
                put(&mut bad, at, 20);
            }
            seal(&mut bad);
            let error = if index == 0 {
                decode_root_page(&bad, ns()).err()
            } else {
                decode_jobs_page(&bad, ns(), (index - 1) as u8).err()
            };
            assert_eq!(
                error,
                Some(RetentionError::Page(PageError::IdentityMismatch))
            );
        }
        let mut bad = original;
        bad[4] = 2;
        seal(&mut bad);
        let error = if index == 0 {
            decode_root_page(&bad, ns()).err()
        } else {
            decode_jobs_page(&bad, ns(), (index - 1) as u8).err()
        };
        assert_eq!(
            error,
            Some(RetentionError::Page(PageError::UnsupportedFormat))
        );
    }
    for index in [5u8, u8::MAX] {
        assert_eq!(
            decode_jobs_page(&literal(1), ns(), index).err(),
            Some(RetentionError::InvalidInput)
        );
    }
    let mut bad = literal(0);
    bad[6] = 13;
    seal(&mut bad);
    assert_eq!(
        decode_root_page(&bad, ns()).err(),
        Some(RetentionError::Page(PageError::KindMismatch))
    );
    let mut bad = literal(1);
    bad[7] = 1;
    seal(&mut bad);
    assert_eq!(
        decode_jobs_page(&bad, ns(), 0).err(),
        Some(RetentionError::Page(PageError::Corrupt))
    );
    // Valid lower envelopes with wrong queue cardinality isolate the wrapper.
    for (kind, number, count) in [
        (PageKind::RetentionRoot, 0, 0),
        (PageKind::RetentionRoot, 0, 2),
        (PageKind::RetentionJobs, 1, 14),
        (PageKind::RetentionJobs, 5, 5),
    ] {
        let original = literal(number);
        let expected = header(
            ns(),
            if number == 0 {
                RetentionGenesisPage::Root
            } else {
                RetentionGenesisPage::Jobs {
                    index: (number - 1) as u8,
                }
            },
            0,
        )
        .unwrap();
        let size = if number == 0 { ROOT_BYTES } else { JOB_BYTES };
        let pos = start(&original, 0);
        let cell = &original[pos..pos + size];
        let cells = vec![cell; count];
        let mut bad = [0; PAGE_BYTES];
        page::encode_page(expected, &cells, &mut bad).unwrap();
        assert_eq!(
            page::decode_page(&bad, expected.identity, kind)
                .unwrap()
                .len(),
            count
        );
        let error = if number == 0 {
            decode_root_page(&bad, ns()).err()
        } else {
            decode_jobs_page(&bad, ns(), (number - 1) as u8).err()
        };
        assert_eq!(error, Some(RetentionError::Corrupt));
    }
}

#[test]
fn row_pages_lsn_and_nested_job_rejections() {
    let mut bad = literal(6);
    put(&mut bad, 64, 98);
    seal(&mut bad);
    assert_eq!(
        decode_root_page(&bad, ns()).err(),
        Some(RetentionError::OrderMismatch)
    );
    let mut bad = literal(0);
    let at = start(&bad, 0);
    bad[at + 4] = 2;
    seal(&mut bad);
    assert_eq!(
        decode_root_page(&bad, ns()).err(),
        Some(RetentionError::UnsupportedFormat)
    );
    for index in 0..5 {
        let mut bad = literal(index + 1);
        let at = start(&bad, if index == 4 { 3 } else { 14 });
        bad[at + 4] = 2;
        seal(&mut bad);
        assert_eq!(
            decode_jobs_page(&bad, ns(), index as u8).err(),
            Some(RetentionError::UnsupportedFormat)
        );
        let mut bad = literal(index + 1);
        let at = start(&bad, 0);
        put(&mut bad, at + 8, 2);
        seal(&mut bad);
        assert_eq!(
            decode_jobs_page(&bad, ns(), index as u8).err(),
            Some(RetentionError::OrderMismatch)
        );
    }
    // A valid occupied bank cannot be installed at a preexisting smaller LSN.
    let mut bad = literal(7);
    put(&mut bad, 64, 0);
    seal(&mut bad);
    assert_eq!(
        decode_jobs_page(&bad, ns(), 0).err(),
        Some(RetentionError::OrderMismatch)
    );
    let mut bad = literal(7);
    put(&mut bad, 64, 1);
    seal(&mut bad);
    assert_eq!(
        decode_jobs_page(&bad, ns(), 0).err(),
        Some(RetentionError::OrderMismatch)
    );
}

#[test]
fn row_pages_encode_errors_preserve_output() {
    let bytes = literal(6);
    let (_, root) = decode_root_page(&bytes, ns()).unwrap();
    let mut bad = *root.bytes();
    bad[4] = 2;
    let mut output = [0xa5; PAGE_BYTES];
    assert_eq!(
        encode_root_page(&bad, ns(), &mut output),
        Err(RetentionError::UnsupportedFormat)
    );
    assert_eq!(output, [0xa5; PAGE_BYTES]);
    for size in [0, 1, PAGE_BYTES - 1, PAGE_BYTES + 1] {
        let mut out = vec![0xa5; size];
        assert_eq!(
            encode_root_page(root.bytes(), ns(), &mut out),
            Err(RetentionError::Page(PageError::InvalidInput))
        );
        assert!(out.iter().all(|b| *b == 0xa5));
    }
    let bytes = literal(7);
    let view = decode_jobs_page(&bytes, ns(), 0).unwrap();
    let cells: Vec<_> = (0..view.len()).map(|i| view.cell(i).unwrap()).collect();
    let mut bad_cell: [u8; JOB_BYTES] = cells[14].try_into().unwrap();
    bad_cell[4] = 2;
    let mut bad_cells = cells.clone();
    bad_cells[14] = &bad_cell;
    assert_eq!(
        encode_jobs_page(ns(), 0, 99, &bad_cells, &mut output),
        Err(RetentionError::UnsupportedFormat)
    );
    assert_eq!(output, [0xa5; PAGE_BYTES]);
    for (index, lsn, count) in [
        (0, 0, 15),
        (0, 1, 15),
        (0, 99, 14),
        (4, 99, 15),
        (5, 99, 15),
    ] {
        output.fill(0xa5);
        assert!(encode_jobs_page(ns(), index, lsn, &cells[..count], &mut output).is_err());
        assert_eq!(output, [0xa5; PAGE_BYTES]);
    }
    for size in [0, PAGE_BYTES - 1, PAGE_BYTES + 1] {
        let mut out = vec![0xa5; size];
        assert_eq!(
            encode_jobs_page(ns(), 0, 99, &cells, &mut out),
            Err(RetentionError::InvalidInput)
        );
        assert!(out.iter().all(|b| *b == 0xa5));
    }
    // Header construction is ordinary pure input, never an installed page.
    assert_eq!(view.header().lsn, PageLsn(99));
}
