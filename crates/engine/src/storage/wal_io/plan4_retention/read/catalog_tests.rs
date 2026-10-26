//! New typed catalog consumers only. Nonempty pages below are pure borrowed
//! input, never injected into the original owner's canonical N=0 population.
use super::*;
use crate::{
    api::DatabaseId,
    storage::{
        catalog_record::{CatalogDefinitionView, CatalogError},
        dirty_frame::{DirtyFrame, init_test_support as probe},
        page::{FileId, PageRef},
        transaction_record::VersionStamp,
        wal_frame::MAX_FRAME_BYTES,
        wal_io::{FreshStorageSession, plan4_test_support as port},
    },
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use foundation::{codec::content_digest, identity::Generation, namespace::DirectoryPort};

fn object(value: u64) -> ObjectId {
    ObjectId::new(value).unwrap()
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 17,
    }
}
fn identity() -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: PageRef {
            file: FileId::new(0x1_0000_0001).unwrap(),
            page_no: 8,
            incarnation: Generation::new(1).unwrap(),
        },
        owner: object(5),
        owner_generation: Generation::new(1).unwrap(),
    }
}
fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

// Direct XCT1 scalar recipe: minimal table T(i Int64 NOT NULL). It consumes no
// candidate/production encoder or lookup to calculate expected record fields.
fn value(object: u64, retired: bool, uncommitted: bool) -> Vec<u8> {
    let mut bytes = vec![0; 242];
    bytes[..6].copy_from_slice(b"XCT1\x01\0");
    bytes[6] = 1;
    bytes[7] = u8::from(retired);
    for (offset, number) in [
        (8, object),
        (16, 5),
        (24, 7),
        (104, 17),
        (112, 19),
        (120, 23),
        (128, 29),
        (176, 0x1_0000_0001),
        (184, 19),
        (192, 4),
    ] {
        put64(&mut bytes, offset, number);
    }
    bytes[88..104].fill(0x11);
    if uncommitted {
        bytes[32] = 2;
        bytes[40..56].fill(0x11);
        for (offset, number) in [(56, 17), (64, 19), (72, 23), (80, 37)] {
            put64(&mut bytes, offset, number);
        }
    } else {
        bytes[32] = 1;
        put64(&mut bytes, 40, 31);
    }
    bytes[224..228].copy_from_slice(&10u32.to_le_bytes());
    bytes[232..].copy_from_slice(b"\x01\0\x01\0T\x01\0\x02\0i");
    bytes
}
fn page(records: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = vec![0; PAGE_BYTES];
    bytes[..6].copy_from_slice(b"XDP1\x01\0");
    bytes[6] = 5;
    bytes[8..24].fill(0x11);
    for (offset, number) in [
        (24, 0x1_0000_0001),
        (32, 8),
        (40, 1),
        (48, 5),
        (56, 1),
        (64, 93),
    ] {
        put64(&mut bytes, offset, number);
    }
    let count = records.len() + 1;
    bytes[72..74].copy_from_slice(&(count as u16).to_le_bytes());
    bytes[74..76].copy_from_slice(&((128 + 4 * count) as u16).to_le_bytes());
    let mut metadata = vec![0; 32];
    metadata[..6].copy_from_slice(b"XBT1\x01\0");
    let cells = std::iter::once(metadata).chain(records.iter().map(|(object, value)| {
        let mut cell = vec![8, 0];
        cell.extend((value.len() as u16).to_le_bytes());
        cell.extend(object.to_be_bytes());
        cell.extend(value);
        cell
    }));
    let mut end = 16_352;
    for (slot, cell) in cells.enumerate() {
        let start = end - cell.len();
        bytes[128 + slot * 4..130 + slot * 4].copy_from_slice(&(start as u16).to_le_bytes());
        bytes[130 + slot * 4..132 + slot * 4].copy_from_slice(&(cell.len() as u16).to_le_bytes());
        bytes[start..end].copy_from_slice(&cell);
        end = start;
    }
    bytes[76..78].copy_from_slice(&(end as u16).to_le_bytes());
    let digest = content_digest(&bytes[..16_352]);
    bytes[16_352..].copy_from_slice(&digest);
    bytes
}

#[test]
fn catalog_read_nonempty_physical_versions_and_absence() {
    let bytes = page(&[(6, value(6, false, false)), (8, value(8, true, true))]);
    let before = bytes.clone();
    let view = leaf_view(&bytes, identity(), namespace()).unwrap();
    assert_eq!(view.header().lsn, PageLsn(93));
    for number in [1, 5, 7, 9, u64::MAX] {
        assert_eq!(lookup(&view, namespace(), object(number)).unwrap(), None);
    }
    for (number, retired, transaction) in [(6, false, false), (8, true, true)] {
        let record = lookup(&view, namespace(), object(number)).unwrap().unwrap();
        assert_eq!(record.header().identity.object, object(number));
        assert_eq!(record.header().identity.generation.get(), 5);
        assert_eq!(record.header().identity.schema_epoch.get(), 7);
        assert_eq!(record.header().retired, retired);
        assert_eq!(
            matches!(record.header().stamp, VersionStamp::Transaction(_)),
            transaction
        );
        let CatalogDefinitionView::Table(table) = record.definition() else {
            panic!("table")
        };
        assert_eq!((table.name(), table.len()), ("T", 1));
        let column = table.column(0).unwrap();
        assert_eq!((column.name, column.nullable), ("i", false));
        let start = bytes.as_ptr() as usize;
        assert!((start..start + bytes.len()).contains(&(table.name().as_ptr() as usize)));
    }
    assert_eq!(bytes, before);
}

#[test]
fn catalog_read_rejects_corrupt_unselected_values_and_identity() {
    for (offset, byte, expected) in [
        (8, 9, CatalogError::IdentityMismatch),
        (88, 0x12, CatalogError::IdentityMismatch),
        (4, 2, CatalogError::UnsupportedFormat),
        (228, 1, CatalogError::Corrupt),
        (239, 7, CatalogError::Corrupt),
    ] {
        let mut bad = value(8, false, false);
        bad[offset] = byte;
        let bytes = page(&[(6, value(6, false, false)), (8, bad)]);
        let before = bytes.clone();
        let error = match leaf_view(&bytes, identity(), namespace()) {
            Ok(_) => panic!("bad value outside first lookup"),
            Err(e) => e,
        };
        assert!(
            matches!(error, RetentionCause::Catalog(e) if e == expected),
            "{error:?}"
        );
        assert_eq!(bytes, before);
    }
    let bytes = page(&[(6, value(6, false, false))]);
    let mut expected = identity();
    expected.owner_generation = Generation::new(2).unwrap();
    assert!(matches!(
        leaf_view(&bytes, expected, namespace()),
        Err(RetentionCause::Tree(tree_page::TreeError::Page(
            page::PageError::IdentityMismatch
        )))
    ));
    let mut wrong_namespace = namespace();
    wrong_namespace.timeline += 1;
    assert!(matches!(
        leaf_view(&bytes, identity(), wrong_namespace),
        Err(RetentionCause::Catalog(CatalogError::IdentityMismatch))
    ));
}

fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().ok().unwrap()
}
macro_rules! original_owner {
    (|$bound:ident, $owner:ident, $core:ident, $state:ident, $storage:ident| $body:block) => {{
        let directory = port::Directory::fresh();
        let $state = directory.state();
        let mut slot = boxed::<MAX_FRAME_BYTES>();
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let fresh = FreshStorageSession::create_retention_catalog(directory.acquire_writer().unwrap(), 1, &mut slot, &mut maintenance).unwrap();
        let $storage = fresh.install_doublewrite(&mut dwb).unwrap().into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        let mut $owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let mut backing = vec![[0; PAGE_BYTES]; 4];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let mut control = boxed::<{ super::super::super::RETENTION_CONTROL_BYTES }>();
        let mut replay = boxed::<{ super::super::super::RETENTION_REPLAY_BYTES }>();
        let old = $storage.bind_plan4_pages(&$core, &mut frames, &mut operation).unwrap();
        let mut $bound = match old.into_empty_retention(&mut $owner, super::super::super::EmptyRetentionControlLoan { control: &mut control, replay: &mut replay }) {
            Ok(v) => v,
            Err(super::super::super::IntoRetentionError::Before { error, .. }) | Err(super::super::super::IntoRetentionError::Fatal(error)) => panic!("activation: {error:?}"),
        };
        let mut cursor = Plan4ResidencyCursor::new();
        assert!($bound.prepare_catalog(&$owner, &mut cursor).unwrap().is_none());
        $state.lock().unwrap().events.clear();
        $body
        drop($bound);
        drop($owner);
        drop($storage);
        let state = $state.lock().unwrap();
        assert_eq!((state.writers, state.handles), (0, 0));
    }};
}

#[test]
fn catalog_read_original_owner_whole_set_and_release() {
    original_owner!(|bound, owner, core, state, storage| {
        let (_, reader, _) = bound.split();
        let guard = reader.read_catalog(&mut owner).unwrap();
        let (counts, total, init, _) = probe::pin_state(reader.pool);
        assert_eq!(
            (counts.iter().filter(|&&n| n == 1).count(), total, init),
            (3, 3, false)
        );
        assert!(guard.lookup(object(6)).unwrap().is_none());
        assert!(guard.lookup(object(u64::MAX)).unwrap().is_none());
        assert!(state.lock().unwrap().events.is_empty());
        guard.release_checked().unwrap();
        assert_eq!(probe::pin_state(reader.pool).1, 0);
        owner.check_idle().unwrap();
        assert!(!storage.session_fence().is_fenced());
    });
}

#[test]
fn catalog_read_original_owner_binding_and_fence_cleanup() {
    original_owner!(|bound, owner, core, state, storage| {
        let foreign = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut foreign_owner = foreign.try_owner(OwnerRole::Foreground).unwrap();
        let (_, reader, _) = bound.split();
        let error = match reader.read_catalog(&mut foreign_owner) {
            Ok(_) => panic!("foreign core"),
            Err(e) => e,
        };
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(
            matches!(error.cause, RetentionCause::Frame(e) if e.cause == dirty_frame::Cause::BindingMismatch)
        );
        assert_eq!(probe::pin_state(reader.pool).1, 0);
        foreign_owner.check_idle().unwrap();
        let guard = reader.read_catalog(&mut owner).unwrap();
        storage.session_fence().fence();
        let error = guard.lookup(object(6)).unwrap_err();
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(
            matches!(error.cause, RetentionCause::Frame(e) if e.cause == dirty_frame::Cause::Fenced)
        );
        let _ = guard.release_checked();
        assert_eq!(probe::pin_state(reader.pool).1, 0);
        assert!(state.lock().unwrap().events.is_empty());
        drop(foreign_owner);
    });
}

#[test]
fn catalog_read_source_reservation_fits_existing_peak() {
    fn check<W: WriterNamespace>(label: &str) {
        let read = catalog_source_bytes::<W>();
        let live = super::super::super::resources::source_census::<W>();
        let old = super::super::super::super::resources::source_census::<W>();
        println!(
            "CATALOG_READ_SOURCE {label} actual={read} reserved={CATALOG_READ_SOURCE_BYTES} old_catalog={} retained={} peak={} metadata={} whole={}",
            old.catalog_nested, live.retained, live.peak, live.metadata, live.whole
        );
        assert!(read <= CATALOG_READ_SOURCE_BYTES);
        assert!(CATALOG_READ_SOURCE_BYTES <= old.catalog_nested);
        assert!(super::super::super::resources::fits::<W>());
        assert!(live.peak <= 65_536 && live.metadata <= 196_608 && live.whole <= 262_144);
    }
    check::<port::Writer>("memory");
    #[cfg(target_os = "linux")]
    check::<super::super::super::super::native::Writer>("native");
}
