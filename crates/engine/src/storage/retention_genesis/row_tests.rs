//! Uncalled Profile3 genesis pages against independently authored literals.
//! Cell offsets are read from the literal layout, not from product decoders.
use super::*;
use crate::{
    api::DatabaseId,
    storage::{
        allocation_page::SystemWorkRef,
        retention_page::{
            RetentionError,
            row::{RootView, source_tests as frames},
        },
    },
};

const PAGES: &[u8; 12 * PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/genesis3/pages.bin"
));
const REJECTS: &[u8; 7 * PAGE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/genesis3/rejects.bin"
));
const INITIAL: &[u8; 512] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/root3/initial-root.bin"
));
const FREE: &[u8; 1024] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/root3/free-job.bin"
));
const ROLES: [RetentionGenesisPage; 6] = [
    RetentionGenesisPage::Root,
    RetentionGenesisPage::Jobs { index: 0 },
    RetentionGenesisPage::Jobs { index: 1 },
    RetentionGenesisPage::Jobs { index: 2 },
    RetentionGenesisPage::Jobs { index: 3 },
    RetentionGenesisPage::Jobs { index: 4 },
];
const ROOT_CELL: usize = PAGE_BYTES - 32 - 512;

fn database(group: usize) -> [u8; 16] {
    std::array::from_fn(|i| [1, 0xa1][group] + i as u8)
}
fn ns(group: usize) -> Namespace {
    Namespace {
        database: DatabaseId::new(database(group)).unwrap(),
        timeline: 1,
    }
}
fn literal(set: &[u8], index: usize) -> [u8; PAGE_BYTES] {
    set[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
        .try_into()
        .unwrap()
}
fn job_cell(slot: usize) -> usize {
    PAGE_BYTES - 32 - 1024 * (slot + 1)
}
fn slots(role: RetentionGenesisPage) -> usize {
    match role {
        RetentionGenesisPage::Root => 1,
        RetentionGenesisPage::Jobs { index: 4 } => 4,
        RetentionGenesisPage::Jobs { .. } => 15,
    }
}
fn seal(page: &mut [u8]) {
    let end = page.len() - 32;
    let digest = content_digest(&page[..end]);
    page[end..].copy_from_slice(&digest);
}
fn put64(page: &mut [u8], at: usize, value: u64) {
    page[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
/// Change one byte range of a group-0 literal, reseal and validate it.
fn mutated(number: usize, change: impl FnOnce(&mut [u8; PAGE_BYTES])) -> Result<()> {
    let mut bytes = literal(PAGES, number);
    change(&mut bytes);
    seal(&mut bytes);
    validate_row_current(&bytes, ns(0), ROLES[number])
}

#[test]
fn row_genesis_pages_match_independent_literals() {
    for group in 0..2 {
        for (number, role) in ROLES.into_iter().enumerate() {
            let expected = literal(PAGES, group * 6 + number);
            for fill in [0x00, 0xa5] {
                let mut output = [fill; PAGE_BYTES];
                encode_row_current(ns(group), role, &mut output).unwrap();
                assert_eq!(output, expected, "group {group} page {number}");
            }
            validate_row_current(&expected, ns(group), role).unwrap();
            // The cells are the hash-pinned Root3/Free Job3 cell literals.
            if role == RetentionGenesisPage::Root {
                let mut cell = *INITIAL;
                cell[192..208].copy_from_slice(&database(group));
                assert_eq!(expected[ROOT_CELL..PAGE_BYTES - 32], cell);
                assert_eq!(expected[ROOT_CELL + 432..ROOT_CELL + 440], [0; 8]);
            } else {
                for slot in 0..slots(role) {
                    assert_eq!(&expected[job_cell(slot)..job_cell(slot) + 1024], FREE);
                }
            }
        }
    }
}

#[test]
fn row_genesis_rejects_versions_identity_and_noncanonical() {
    use RetentionGenesisError::{Corrupt, IdentityMismatch, InvalidInput, UnsupportedFormat};
    // Profile2-current Root2/Job2 genesis pages are never accepted as Row.
    for (number, role) in ROLES.into_iter().enumerate() {
        assert_eq!(
            validate_row_current(&literal(REJECTS, number), ns(0), role),
            Err(UnsupportedFormat)
        );
    }
    // A valid active Root3 page at its own LSN is not genesis.
    assert_eq!(
        validate_row_current(&literal(REJECTS, 6), ns(0), RetentionGenesisPage::Root),
        Err(Corrupt)
    );
    for (number, role) in ROLES.into_iter().enumerate() {
        let first = if number == 0 { ROOT_CELL } else { job_cell(0) };
        let last = if number == 0 {
            ROOT_CELL
        } else {
            job_cell(slots(role) - 1)
        };
        for version in [0u16, 1, 2, 4, u16::MAX] {
            for at in [first, last] {
                assert_eq!(
                    mutated(number, |p| p[at + 4..at + 6]
                        .copy_from_slice(&version.to_le_bytes())),
                    Err(UnsupportedFormat)
                );
            }
        }
        assert_eq!(mutated(number, |p| p[4] = 2), Err(UnsupportedFormat));
        // Namespace and fixed page identity/generation.
        assert_eq!(
            validate_row_current(&literal(PAGES, number), ns(1), role),
            Err(IdentityMismatch)
        );
        assert_eq!(
            validate_row_current(&literal(PAGES, 6 + number), ns(0), role),
            Err(IdentityMismatch)
        );
        for (at, value) in [(40, 2), (48, 5), (56, 2)] {
            assert_eq!(
                mutated(number, |p| put64(p, at, value)),
                Err(IdentityMismatch)
            );
        }
        for other in ROLES.into_iter().filter(|other| *other != role) {
            assert_eq!(
                validate_row_current(&literal(PAGES, number), ns(0), other),
                Err(IdentityMismatch)
            );
        }
        for timeline in [0, 2] {
            let namespace = Namespace { timeline, ..ns(0) };
            assert_eq!(
                validate_row_current(&literal(PAGES, number), namespace, role),
                Err(InvalidInput)
            );
        }
        // A genesis page has LSN0; Root.apply0 or the expected header rejects.
        assert_eq!(mutated(number, |p| put64(p, 64, 1)), Err(Corrupt));
        for size in [0, PAGE_BYTES - 1, PAGE_BYTES + 1] {
            let mut bytes = literal(PAGES, number).to_vec();
            bytes.resize(size, 0);
            assert_eq!(
                validate_row_current(&bytes, ns(0), role),
                Err(RetentionGenesisError::Page(PageError::Corrupt))
            );
        }
    }
    for index in [5, u8::MAX] {
        assert_eq!(
            validate_row_current(
                &literal(PAGES, 1),
                ns(0),
                RetentionGenesisPage::Jobs { index }
            ),
            Err(InvalidInput)
        );
    }
    // Root cell: namespace/refs, then non-canonical row_high and commitments.
    let cell = ROOT_CELL;
    assert_eq!(
        mutated(0, |p| p[cell + 192..cell + 208]
            .copy_from_slice(&database(1))),
        Err(IdentityMismatch)
    );
    for at in [208, 232 + 16, 256 + 4 * 24 + 8] {
        assert_eq!(
            mutated(0, |p| put64(p, cell + at, 2)),
            Err(IdentityMismatch)
        );
    }
    for (at, value) in [
        (64, 2),
        (88, 1),
        (400, 6),
        (408, 1),
        (432, 1),
        (432, u64::MAX),
    ] {
        assert_eq!(mutated(0, |p| put64(p, cell + at, value)), Err(Corrupt));
    }
    for at in [7, 120, 128, 159, 160, 191, 440, 511] {
        assert_eq!(mutated(0, |p| p[cell + at] ^= 1), Err(Corrupt));
    }
    // Job cells: only Free generation1 with a zero tail.
    for number in 1..6 {
        let at = job_cell(slots(ROLES[number]) - 1);
        for value in [0, 2] {
            assert_eq!(mutated(number, |p| put64(p, at + 8, value)), Err(Corrupt));
        }
        for offset in [6, 7, 16, 320, 384, 1023] {
            assert_eq!(mutated(number, |p| p[at + offset] = 1), Err(Corrupt));
        }
    }
}

#[test]
fn row_genesis_encode_failures_preserve_output() {
    for role in ROLES {
        for timeline in [0, 2] {
            let mut output = [0xa5; PAGE_BYTES];
            assert_eq!(
                encode_row_current(Namespace { timeline, ..ns(0) }, role, &mut output),
                Err(RetentionGenesisError::InvalidInput)
            );
            assert_eq!(output, [0xa5; PAGE_BYTES]);
        }
        for size in [0, 1, PAGE_BYTES - 1, PAGE_BYTES + 1] {
            let mut output = vec![0xa5; size];
            assert_eq!(
                encode_row_current(ns(0), role, &mut output),
                Err(RetentionGenesisError::InvalidInput)
            );
            assert!(output.iter().all(|byte| *byte == 0xa5));
        }
    }
    for index in [5, u8::MAX] {
        let mut output = [0xa5; PAGE_BYTES];
        assert_eq!(
            encode_row_current(ns(0), RetentionGenesisPage::Jobs { index }, &mut output),
            Err(RetentionGenesisError::InvalidInput)
        );
        assert_eq!(output, [0xa5; PAGE_BYTES]);
    }
}

#[test]
fn row_genesis_source_bound_covers_measured_frames() {
    use std::mem::{size_of, size_of_val};
    let sha = frames::SHA;
    let encode = size_of::<(Namespace, RetentionGenesisPage, &mut [u8])>()
        + size_of::<Result<()>>()
        + size_of::<PageHeader>()
        + size_of::<Result<PageHeader>>()
        + size_of::<std::result::Result<(), PageError>>()
        + size_of::<&[&[u8]]>();
    let encode_root = encode
        + 512
        + size_of::<std::result::Result<(), RetentionError>>()
        + size_of::<Result<()>>()
        + size_of::<[&[u8]; 1]>();
    let encode_jobs = encode
        + 1024
        + size_of_val(&Generation::new(1))
        + size_of::<Generation>()
        + size_of::<[&[u8]; 15]>();
    let validate = size_of::<(&[u8], Namespace, RetentionGenesisPage)>()
        + size_of::<Result<()>>()
        + 2 * size_of::<PageHeader>()
        + size_of::<Result<PageHeader>>();
    let validate_root = validate
        + size_of::<(PageHeader, RootView<'_>)>()
        + size_of::<std::result::Result<(PageHeader, RootView<'_>), RetentionError>>()
        + size_of::<Result<(PageHeader, RootView<'_>)>>()
        + size_of::<Option<SystemWorkRef>>();
    let validate_jobs = validate
        + size_of::<page::PageView<'_>>()
        + size_of::<std::result::Result<page::PageView<'_>, RetentionError>>()
        + size_of::<Result<page::PageView<'_>>>();
    let page_hash = frames::encode_page_frame() + frames::content_digest_frame() + sha;
    let root_decode = frames::root_chains()
        .iter()
        .filter(|(name, _)| name.starts_with("decode"))
        .map(|(_, bytes)| *bytes)
        .max()
        .unwrap();
    let jobs_decode = frames::job_chains()
        .iter()
        .find(|(name, _)| *name == "decode_jobs_page")
        .unwrap()
        .1;
    let chains = [
        (
            "encode_root_initial",
            encode_root
                + frames::initial_root_jobs_frame()
                + frames::initial_jobs_digest_frame()
                + sha,
        ),
        ("encode_root_header", encode_root + frames::header_chain()),
        ("encode_root_page", encode_root + page_hash),
        ("encode_jobs_page", encode_jobs + page_hash),
        ("validate_root", validate_root + root_decode),
        ("validate_jobs", validate_jobs + jobs_decode),
    ];
    let bound = row_current_source_bytes();
    for (name, bytes) in chains {
        println!("ROW_GENESIS_SOURCE_CHAIN {name}={bytes}");
        assert!(bound >= bytes, "{name}");
    }
    let measured = chains.iter().map(|(_, bytes)| *bytes).max().unwrap();
    println!(
        "ROW_GENESIS_SOURCE bound={bound} measured={measured} wrapper={} row_codec={} catalog_current={}",
        source_bytes(),
        crate::storage::retention_page::row::source_bytes(),
        current_source_bytes()
    );
    assert_eq!(
        bound,
        source_bytes() + crate::storage::retention_page::row::source_bytes()
    );
}
