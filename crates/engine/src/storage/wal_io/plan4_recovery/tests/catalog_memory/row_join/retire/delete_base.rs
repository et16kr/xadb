//! RDL-R (dev3-row-delete.md#recovery): C's base4 of the DELETE crash
//! states over actual captured bases. Each case restarts `crash_images` to
//! READY and takes the base its Rebase captured (the `d3-rebase-*` file the
//! restart wrote), decoded under the selected Profile and the base's own
//! filemap and bootstrap digest. The accepted census is a state-machine
//! fact. Each refusal edits the captured base's RowLeaf or Undo page by raw
//! offsets (no product encoder) and reseals the page, its certificate, the
//! section digests and the trailer, as the base4 actor tests do.
use super::super::super::super::super::super::support::namespace;
use super::super::super::super::super::row_recovery::{Drive, PageRead, restart_with};
use super::*;
use crate::storage::{
    control::{ControlRef, SelectedControl, row_profile},
    plan4_recovery_base::{
        Expected,
        row::{RowBaseError, RowCensus, actor::Error as ActorError, decode},
    },
};
use foundation::codec::content_digest;

const BASE: usize = 2_097_152;
const ARENA: usize = 262_144;
const CERTIFICATES: usize = 65_536;
/// Page geometry: the 128-byte header, its slot directory, the digest.
const HEADER: usize = 128;
const DIGEST: usize = PAGE_BYTES - 32;
/// Row record fields (`row_record`): flags, RowId, stamp, logical, undo.
const FLAGS: usize = 6;
const ROW: std::ops::Range<usize> = 8..16;
const STAMP: std::ops::Range<usize> = 16..72;
const LOGICAL: std::ops::Range<usize> = 72..120;
const UNDO: std::ops::Range<usize> = 120..160;
/// The logical version's mutation sequence (`LogicalRowVersion` @40).
const MUTATION: std::ops::Range<usize> = 112..120;
/// A committed one-column row's record and a tombstone's (168, no body).
const ROW_RECORD: usize = 181;
const TOMBSTONE_RECORD: usize = 168;
/// The Live delete U of the replaced row: 432 + L, L = C - 180 = 13.
const LIVE_U: usize = 432 + 13;

fn u16_at(b: &[u8], at: usize) -> usize {
    u16::from_le_bytes([b[at], b[at + 1]]) as usize
}
fn put_u16(b: &mut [u8], at: usize, v: usize) {
    b[at..at + 2].copy_from_slice(&(v as u16).to_le_bytes());
}

/// C's restart of `images` to READY: the base4 its Rebase wrote and the
/// Expected it decodes under.
fn rebased(images: &Images) -> (Box<[u8; BASE]>, Expected) {
    let directory = clone_directory(images);
    join_restart(&directory, RowRecoveryPolicy::WithRowClr, None);
    let files = directory.state().lock().unwrap().images();
    let name = ["d3-rebase-0", "d3-rebase-1"]
        .into_iter()
        .find(|name| files.get(*name) != images.get(*name))
        .expect("the restart's Rebase");
    let bytes: Box<[u8; BASE]> = files[name].clone().into_boxed_slice().try_into().unwrap();
    let expected = Expected {
        profile: row_profile::decode_profile(&files["d3-profile"], Some(namespace())).unwrap(),
        filemap: SelectedControl {
            generation: u64::from_le_bytes(bytes[448..456].try_into().unwrap()),
            reference: ControlRef::decode(&bytes[384..448]).unwrap(),
        },
        bootstrap_digest: bytes[464..496].try_into().unwrap(),
    };
    (bytes, expected)
}

fn census(bytes: &[u8; BASE], expected: Expected) -> std::result::Result<RowCensus, RowBaseError> {
    let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
    decode(&bytes[..], expected, &mut scratch).map(|view| view.census())
}

/// The arena index of the one page of `kind` whose cells `pick` accepts.
fn find(bytes: &[u8; BASE], kind: u8, pick: impl Fn(&[Vec<u8>]) -> bool) -> usize {
    let found: Vec<_> = (0..64)
        .filter(|n| {
            let page = &bytes[ARENA + n * PAGE_BYTES..ARENA + (n + 1) * PAGE_BYTES];
            &page[..4] == b"XDP1" && page[6] == kind && pick(&page_cells(page))
        })
        .collect();
    assert_eq!(found.len(), 1, "pages of kind {kind}: {found:?}");
    found[0]
}
fn page_mut(bytes: &mut [u8; BASE], n: usize) -> &mut [u8] {
    &mut bytes[ARENA + n * PAGE_BYTES..ARENA + (n + 1) * PAGE_BYTES]
}
/// The page's cells in slot order (slot 0 is the tree metadata).
fn page_cells(page: &[u8]) -> Vec<Vec<u8>> {
    (0..u16_at(page, 72))
        .map(|slot| {
            let (start, length) = (
                u16_at(page, HEADER + 4 * slot),
                u16_at(page, HEADER + 4 * slot + 2),
            );
            page[start..start + length].to_vec()
        })
        .collect()
}
/// Lay `cells` out from the digest down in slot order and reseal the page,
/// its certificate, every section digest and the trailer.
fn rewrite(bytes: &mut [u8; BASE], n: usize, cells: &[Vec<u8>]) {
    let page = page_mut(bytes, n);
    let lower = HEADER + 4 * cells.len();
    let mut end = DIGEST;
    for (slot, cell) in cells.iter().enumerate() {
        let start = end - cell.len();
        page[start..end].copy_from_slice(cell);
        put_u16(page, HEADER + 4 * slot, start);
        put_u16(page, HEADER + 4 * slot + 2, cell.len());
        end = start;
    }
    assert!(lower <= end, "the cells fit the page");
    put_u16(page, 72, cells.len());
    put_u16(page, 74, lower);
    put_u16(page, 76, end);
    page[lower..end].fill(0);
    let digest = content_digest(&page[..DIGEST]);
    page[DIGEST..].copy_from_slice(&digest);
    let header: [u8; 48] = page[24..72].try_into().unwrap();
    let digest = content_digest(page);
    let certificate = CERTIFICATES + n * 96;
    bytes[certificate + 8..certificate + 56].copy_from_slice(&header);
    bytes[certificate + 56..certificate + 88].copy_from_slice(&digest);
    for (i, (at, len)) in [
        (4096, 55296),
        (65536, 6144),
        (73728, 22528),
        (98304, 16384),
        (ARENA, 64 * PAGE_BYTES),
    ]
    .into_iter()
    .enumerate()
    {
        let digest = content_digest(&bytes[at..at + len]);
        bytes[1312 + i * 64..1344 + i * 64].copy_from_slice(&digest);
    }
    let digest = content_digest(&bytes[..BASE - 32]);
    bytes[BASE - 32..].copy_from_slice(&digest);
}

/// A RowLeaf cell: key and value lengths, the big-endian RowId, the record.
fn leaf_cell(row: u64, record: &[u8]) -> Vec<u8> {
    let mut cell = Vec::with_capacity(12 + record.len());
    cell.extend_from_slice(&8u16.to_le_bytes());
    cell.extend_from_slice(&(record.len() as u16).to_le_bytes());
    cell.extend_from_slice(&row.to_be_bytes());
    cell.extend_from_slice(record);
    cell
}
fn record(cell: &[u8]) -> &[u8] {
    &cell[12..]
}
/// `record` as RowId `row`.
fn renumbered(record: &[u8], row: u64) -> Vec<u8> {
    let mut out = record.to_vec();
    out[ROW].copy_from_slice(&row.to_le_bytes());
    out
}
/// The `Committed(q)` stamp bytes.
fn committed_stamp(q: u64) -> [u8; 56] {
    let mut stamp = [0; 56];
    stamp[0] = 1;
    stamp[8..16].copy_from_slice(&q.to_le_bytes());
    stamp
}

/// The RowLeaf of a base whose leaf holds the committed row and the
/// actor's tombstone: (arena index, metadata cell, committed record,
/// tombstone record).
fn leaf(bytes: &[u8; BASE]) -> (usize, Vec<u8>, Vec<u8>, Vec<u8>) {
    let n = find(bytes, 1, |_| true);
    let cells = page_cells(&bytes[ARENA + n * PAGE_BYTES..ARENA + (n + 1) * PAGE_BYTES]);
    assert_eq!(cells.len(), 3, "metadata, the tombstone, the kept row");
    let (tombstone, kept) = (record(&cells[1]).to_vec(), record(&cells[2]).to_vec());
    assert_eq!(
        (tombstone.len(), kept.len()),
        (TOMBSTONE_RECORD, ROW_RECORD)
    );
    assert_eq!(tombstone[FLAGS], 0b11, "deleted, with its U");
    (n, cells[0].clone(), kept, tombstone)
}

/// RDL-R01: C's base4 takes every effect-1 crash state. Reserved (the
/// replaced committed row, U Free), Retained beside a Ready or a Leased
/// Terminal (the tombstone, Live RowValue U), ReleasingCompensated (the
/// restored row, U Releasing), and ReleasingCommitted, taken by stopping a
/// restart of the committed files after its first Row cell change (the
/// recovered DeletePrune: no key, U Releasing). Each base's census counts
/// the one Row actor's root as the state machine says.
#[test]
fn rdl_r01_base4_takes_each_delete_state() {
    case_within("RDL-R01", std::time::Duration::from_secs(600), || {
        let committed = crash_images(Crash::Committed).0;
        let pruned = restart_with(
            &committed,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::CrashAfterCells(1),
            0,
            &PageRead,
        );
        let mut seen = Vec::new();
        for (name, images) in [
            ("reserved", crash_images(Crash::Reserved).0),
            ("committed", committed.clone()),
            ("active", crash_images(Crash::Active).0),
            ("rolled-back", crash_images(Crash::RolledBack).0),
            ("pruned", pruned.images),
        ] {
            let (bytes, expected) = rebased(&images);
            let census = census(&bytes, expected).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            println!("RDL_R01 {name} {census:?}");
            seen.push((
                name,
                census.actors,
                census.retained,
                census.releasing,
                census.rows,
            ));
        }
        assert_eq!(
            seen,
            [
                ("reserved", 1, 0, 0, 2),
                ("committed", 1, 1, 0, 2),
                ("active", 1, 1, 0, 2),
                ("rolled-back", 1, 0, 1, 2),
                ("pruned", 1, 0, 1, 1),
            ]
        );
    });
}

/// RDL-R02..R06: refusals over the Active base (Retained root, Leased
/// Terminal), each one edit of its RowLeaf or delete U page.
/// - R02: the actor's tombstone restamped `Committed(3)`: its Retained root
///   no longer finds its own tombstone (actor, Identity).
/// - R03: a `Committed` tombstone with no U in place of the kept row (its
///   writer's logical): a tombstone is only ever `Transaction(T)` with U
///   (actor scan, Identity).
/// - R04: an unpaired tombstone: the actor's tombstone at the kept row's
///   RowId with a new mutation, which no Retained DELETE root names
///   (actor reverse scan, Identity).
/// - R05: a non-canonical delete U: the first byte of its RowValue changed;
///   entry (a) with the table schema refuses it (actor, Undo).
/// - R06: the leaf reservation: the leaf padded with copies of the kept row
///   and of a foreign transaction's tombstone to entries + D one above the
///   capacity (16,188 B) is Corrupt; exactly at the capacity the
///   reservation holds and the padded RowIds above row_high are refused
///   after it (Order).
#[test]
fn rdl_r02_to_r06_base4_refuses_broken_delete_states() {
    case_within("RDL-R02", std::time::Duration::from_secs(600), || {
        let (bytes, expected) = rebased(&crash_images(Crash::Active).0);
        assert!(census(&bytes, expected).is_ok());
        let (n, metadata, kept, tombstone) = leaf(&bytes);
        let base_cells = || {
            vec![
                metadata.clone(),
                leaf_cell(1, &tombstone),
                leaf_cell(2, &kept),
            ]
        };
        let edited = |cells: Vec<Vec<u8>>| {
            let mut copy = bytes.clone();
            rewrite(&mut copy, n, &cells);
            census(&copy, expected)
        };
        // The unedited relayout is the same base.
        assert!(edited(base_cells()).is_ok());

        let mut restamped = tombstone.clone();
        restamped[STAMP].copy_from_slice(&committed_stamp(3));
        let mut cells = base_cells();
        cells[1] = leaf_cell(1, &restamped);
        let r02 = edited(cells);
        println!("RDL_R02 {r02:?}");
        assert_eq!(r02, Err(RowBaseError::Actor(ActorError::Identity)));

        let mut orphan = renumbered(&tombstone, 2);
        orphan[FLAGS] = 0b01;
        orphan[STAMP].copy_from_slice(&committed_stamp(2));
        orphan[LOGICAL].copy_from_slice(&kept[LOGICAL]);
        orphan[UNDO].fill(0);
        let mut cells = base_cells();
        cells[2] = leaf_cell(2, &orphan);
        let r03 = edited(cells);
        println!("RDL_R03 {r03:?}");
        assert_eq!(r03, Err(RowBaseError::Actor(ActorError::Identity)));

        let mut unpaired = renumbered(&tombstone, 2);
        unpaired[MUTATION].copy_from_slice(&2u64.to_le_bytes());
        let mut cells = base_cells();
        cells[2] = leaf_cell(2, &unpaired);
        let r04 = edited(cells);
        println!("RDL_R04 {r04:?}");
        assert_eq!(r04, Err(RowBaseError::Actor(ActorError::Identity)));

        let u = find(&bytes, 8, |cells| {
            cells.len() == 1 && cells[0].len() == LIVE_U
        });
        let mut value = page_cells(&bytes[ARENA + u * PAGE_BYTES..ARENA + (u + 1) * PAGE_BYTES]);
        value[0][432] ^= 0xff;
        let mut copy = bytes.clone();
        rewrite(&mut copy, u, &value);
        let r05 = census(&copy, expected);
        println!("RDL_R05 {r05:?}");
        assert!(
            matches!(r05, Err(RowBaseError::Actor(ActorError::Undo(_)))),
            "{r05:?}"
        );

        // entries = 197 (kept) + 184 (tombstone) + 197a + 184b, D = 13.
        // A foreign transaction's tombstone: the kept row's writer stamps it
        // (`Transaction` names the logical's TxnId).
        let mut foreign = tombstone.clone();
        foreign[LOGICAL].copy_from_slice(&kept[LOGICAL]);
        foreign[STAMP.start + 8..STAMP.start + 48]
            .copy_from_slice(&kept[LOGICAL.start..LOGICAL.start + 40]);
        let padded = |rows: u64, tombstones: u64| {
            let mut cells = base_cells();
            for row in 3..3 + rows {
                cells.push(leaf_cell(row, &renumbered(&kept, row)));
            }
            for row in 3 + rows..3 + rows + tombstones {
                cells.push(leaf_cell(row, &renumbered(&foreign, row)));
            }
            let used: usize = cells[1..].iter().map(|cell| cell.len() + 4).sum();
            (used + 13, edited(cells))
        };
        let (over, r06) = padded(27, 57);
        let (at, held) = padded(26, 58);
        println!("RDL_R06 over={over} {r06:?} at={at} {held:?}");
        assert_eq!((over, at), (16_201, 16_188));
        assert_eq!(r06, Err(RowBaseError::Corrupt));
        assert!(
            held.is_err() && held != Err(RowBaseError::Corrupt),
            "{held:?}"
        );
    });
}
