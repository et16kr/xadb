use super::super::{ARENA, BANK, CERTIFICATES, INVENTORY, PROVENANCE};
use super::*;
use crate::storage::{
    control,
    ownership::InventoryError,
    plan4_recovery_base::{
        ControlRef, Expected, SelectedControl,
        catalog::inventory::{InventoryRows, encode_row_current},
        content_digest,
    },
    retention_page::RetentionError,
    wal_io::CatalogAllocationError,
};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-format/",
            $name
        ))
    };
}
const GENESIS: &[u8; BASE_BYTES] = fixture!("base4-closure/genesis.xrb");
const AGING: &[u8; BASE_BYTES] = fixture!("base4-closure/aging.xrb");
const LIVE: &[u8; BASE_BYTES] = fixture!("base4-closure/live.xrb");
const VARIANTS: &[u8] = fixture!("base4-closure/variants.bin");
const ENVELOPES: &[u8; 5 * 4096] = fixture!("base4-closure/envelopes.bin");
const NO_INVENTORY: &[u8; BASE_BYTES] = fixture!("base4/initial.xrb");
const PROFILES: &[u8; 12288] = fixture!("envelopes/profiles.bin");

fn expected() -> Expected {
    Expected {
        profile: control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap(),
        filemap: SelectedControl {
            generation: 1,
            reference: ControlRef::decode(&GENESIS[384..448]).unwrap(),
        },
        bootstrap_digest: GENESIS[464..496].try_into().unwrap(),
    }
}
fn backing(bytes: &[u8]) -> Box<[u8; BASE_BYTES]> {
    bytes.to_vec().into_boxed_slice().try_into().unwrap()
}
fn scratch() -> Box<[[u8; PAGE_BYTES]; 4]> {
    Box::new([[0; PAGE_BYTES]; 4])
}
fn sections(bytes: &[u8; BASE_BYTES]) -> Sections<'_> {
    Sections {
        descriptors: bytes[BANK..BANK + DESCRIPTOR_BYTES].try_into().unwrap(),
        certificates: bytes[CERTIFICATES..CERTIFICATES + 64 * 96]
            .as_chunks::<96>()
            .0
            .try_into()
            .unwrap(),
        provenance: bytes[PROVENANCE..PROVENANCE + 64 * 352]
            .as_chunks::<352>()
            .0
            .try_into()
            .unwrap(),
        inventory: bytes[INVENTORY..INVENTORY + PAGE_BYTES].try_into().unwrap(),
        images: bytes[ARENA..ARENA + 64 * PAGE_BYTES]
            .as_chunks::<PAGE_BYTES>()
            .0
            .try_into()
            .unwrap(),
    }
}
/// Re-encode exact sections; output must equal the input bytes either way.
fn encoded(bytes: &[u8; BASE_BYTES]) -> (Box<[u8; BASE_BYTES]>, Result<RowCensus>) {
    let header = decode_layout(bytes, expected()).unwrap().header();
    let mut output = backing(&[0; BASE_BYTES]);
    let census = encode(
        header,
        expected(),
        sections(bytes),
        &mut output,
        &mut scratch(),
    )
    .map(RowBaseView::census);
    (output, census)
}

/// Each authored variant is a run list over one of the three base literals.
fn variants() -> Vec<(String, Box<[u8; BASE_BYTES]>)> {
    let bases: [&[u8; BASE_BYTES]; 3] = [GENESIS, AGING, LIVE];
    let u32_at = |at: usize| u32::from_le_bytes(VARIANTS[at..at + 4].try_into().unwrap()) as usize;
    let mut out = Vec::new();
    let mut at = 0;
    while at < VARIANTS.len() {
        let (parent, length) = (VARIANTS[at] as usize, VARIANTS[at + 1] as usize);
        let name = std::str::from_utf8(&VARIANTS[at + 2..at + 2 + length])
            .unwrap()
            .to_owned();
        at += 2 + length;
        let runs = u32_at(at);
        at += 4;
        let mut bytes = backing(bases[parent]);
        for _ in 0..runs {
            let (offset, size) = (u32_at(at), u32_at(at + 4));
            bytes[offset..offset + size].copy_from_slice(&VARIANTS[at + 8..at + 8 + size]);
            at += 8 + size;
        }
        out.push((name, bytes));
    }
    out
}
fn state(name: &str) -> Box<[u8; BASE_BYTES]> {
    variants()
        .into_iter()
        .find_map(|(n, bytes)| (n == name).then_some(bytes))
        .unwrap()
}

const EMPTY: InventoryObservation = InventoryObservation {
    owners: 0,
    items: 0,
    chunks: 0,
    read_calls: 0,
};
/// The live literal: one Row actor with two Retained rows over a committed
/// CREATE and INSERT, after the last System Undo INIT finished.
const LIVE_CENSUS: RowCensus = RowCensus {
    transactions: 1,
    systems: 1,
    statuses: 2,
    actors: 1,
    retained: 2,
    releasing: 0,
    tables: 1,
    rows: 3,
    reserved: ReservedInventory::None,
    inventory: InventoryObservation { owners: 2, ..EMPTY },
};

#[test]
fn base4_closure_accepts_genesis_and_aging_start() {
    let genesis = RowCensus {
        transactions: 0,
        systems: 0,
        statuses: 0,
        actors: 0,
        retained: 0,
        releasing: 0,
        tables: 0,
        rows: 0,
        reserved: ReservedInventory::None,
        inventory: EMPTY,
    };
    let aging = RowCensus {
        systems: 1,
        inventory: InventoryObservation { owners: 1, ..EMPTY },
        ..genesis
    };
    for (bytes, census) in [(GENESIS, genesis), (AGING, aging)] {
        let view = decode(bytes, expected(), &mut scratch()).unwrap();
        assert!(std::ptr::eq(view.bytes(), bytes));
        assert_eq!(view.census(), census);
        assert_eq!(view.digest(), content_digest(bytes));
        assert_eq!(view.descriptor(32), Ok(None));
        let (output, encoded) = encoded(bytes);
        assert_eq!(*output, *bytes);
        assert_eq!(encoded, Ok(census));
    }
    let view = decode(AGING, expected(), &mut scratch()).unwrap();
    let aging = view.descriptor(34).unwrap().unwrap();
    assert_eq!(view.active_descriptor(0), Ok(aging));
    assert_eq!(
        view.active_descriptor(1),
        Err(RowBaseError::Base(BaseError::InvalidInput))
    );
    assert_eq!(
        view.descriptor(36),
        Err(RowBaseError::Base(BaseError::InvalidInput))
    );
    assert_eq!(view.header().system_high_water, [0, 0, 1, 0]);
}

#[test]
fn base4_closure_accepts_live_system_init_done() {
    let mut scratch = scratch();
    let view = decode(LIVE, expected(), &mut scratch).unwrap();
    assert!(std::ptr::eq(view.bytes(), LIVE));
    assert_eq!(view.census(), LIVE_CENSUS);
    // The Finished System row stays in the bank but owns no inventory.
    let finished = view.descriptor(32).unwrap().unwrap();
    assert!(matches!(
        finished.owner,
        crate::storage::obligation::OwnerState::System(p)
            if p.phase == crate::storage::obligation::WorkPhase::Finished
    ));
    assert_eq!(
        view.active_descriptor(1),
        Ok(view.descriptor(34).unwrap().unwrap())
    );
    assert_eq!(
        view.active_descriptor(2),
        Err(RowBaseError::Base(BaseError::InvalidInput))
    );
    assert_eq!(view.header().system_high_water, [5, 0, 1, 0]);
    // The graph pieces the census is built from.
    let graph = Graph::new(decode_layout(LIVE, expected()).unwrap()).unwrap();
    graph.check_queue(&mut scratch).unwrap();
    assert_eq!(
        seam::validate_relations(&graph),
        Ok(ReservedInventory::None)
    );
    assert_eq!(graph.actors(&mut scratch), Ok([1, 2, 0]));
    assert_eq!(graph.tables(), Ok([1, 3]));
    assert!(!graph.catalog_unwired.get());
    let (output, encoded) = encoded(LIVE);
    assert_eq!(*output, *LIVE);
    assert_eq!(encoded, Ok(LIVE_CENSUS));
}

#[test]
fn base4_closure_accepts_system_page_reservation() {
    let bytes = state("state-system-reserved");
    let census = RowCensus {
        systems: 2,
        reserved: ReservedInventory::SystemPage,
        inventory: InventoryObservation {
            owners: 3,
            items: 1,
            chunks: 1,
            read_calls: 5,
        },
        ..LIVE_CENSUS
    };
    let view = decode(&*bytes, expected(), &mut scratch()).unwrap();
    assert_eq!(view.census(), census);
    assert_eq!(view.header().system_high_water, [6, 0, 1, 0]);
    // The Prepared System row is active ordinal 1, between A and Aging.
    assert_eq!(
        view.active_descriptor(1),
        Ok(view.descriptor(32).unwrap().unwrap())
    );
    let (output, encoded) = encoded(&bytes);
    assert_eq!(*output, *bytes);
    assert_eq!(encoded, Ok(census));
}

#[test]
fn base4_closure_catalog_create_fails_closed_after_structure() {
    // Reserved (three CREATE items) and Retained/uncommitted CREATE: every
    // structural relation and inventory check closes, then the semantic
    // Catalog residual fails closed.
    for (name, reserved) in [
        ("state-create-reserved", ReservedInventory::CatalogCreate),
        ("state-create-retained", ReservedInventory::None),
    ] {
        let bytes = state(name);
        let layout = decode_layout(&*bytes, expected()).unwrap();
        let graph = Graph::new(layout).unwrap();
        assert_eq!(seam::validate_relations(&graph), Ok(reserved), "{name}");
        assert!(graph.catalog_unwired.get(), "{name}");
        assert_eq!(
            decode(&*bytes, expected(), &mut scratch()).err(),
            Some(RowBaseError::CatalogUnwired),
            "{name}"
        );
        let (output, encoded) = encoded(&bytes);
        assert_eq!(*output, *bytes, "{name}");
        assert_eq!(encoded, Err(RowBaseError::CatalogUnwired), "{name}");
    }
}

#[test]
fn base4_closure_entry_gates_profile_base_root_job() {
    use RowBaseError::Base;
    let mut scratch = scratch();
    for (index, version) in [0u16, 1, 2, 4, u16::MAX].into_iter().enumerate() {
        let envelope = &ENVELOPES[index * 4096..(index + 1) * 4096];
        assert!(control::row_profile::decode_profile(envelope, None).is_err());
        match control::decode_bootstrap_profile(envelope, None) {
            Ok(profile) => {
                assert!(matches!(version, 1 | 2));
                assert_eq!(profile.format(), version);
                let e = Expected {
                    profile,
                    ..expected()
                };
                assert_eq!(
                    decode(GENESIS, e, &mut scratch).err(),
                    Some(Base(BaseError::Unsupported))
                );
            }
            Err(_) => assert!(matches!(version, 0 | 4 | u16::MAX)),
        }
    }
    let gates = variants();
    let mut seen = 0;
    for (name, bytes) in &gates {
        let expected_error = if name.starts_with("base-version-") {
            Base(BaseError::Unsupported)
        } else if name.starts_with("root-version-") || name.starts_with("job-version-") {
            Base(BaseError::Retention(RetentionError::UnsupportedFormat))
        } else {
            continue;
        };
        seen += 1;
        assert_eq!(
            decode(&**bytes, expected(), &mut scratch).err(),
            Some(expected_error),
            "{name}"
        );
    }
    assert_eq!(seen, 10);
    // The XOI1 shape check runs in the shared Catalog seam.
    assert_eq!(
        decode(NO_INVENTORY, expected(), &mut scratch).err(),
        Some(Base(BaseError::Corrupt))
    );
}

#[test]
fn base4_closure_relation_variants() {
    use crate::storage::semantic_row::Error as Ledger;
    use RowBaseError::Base as B;
    let mut scratch = scratch();
    let mut seen = 0;
    for (name, bytes) in &variants() {
        let expected_error = match name.as_str() {
            "terminal-without-system"
            | "allocation-finished-noncanonical"
            | "system-reserved-terminal" => B(BaseError::CatalogAllocation(
                CatalogAllocationError::StateMismatch,
            )),
            "aging-header-budget" => B(BaseError::BudgetMismatch),
            "issuer-undo-high" | "free-undo-generation" | "create-undo-lsn" => {
                B(BaseError::OrderMismatch)
            }
            "row-high-global" | "committed-q-collision" => RowBaseError::Order,
            "catalog-row-mixing" => RowBaseError::Identity,
            "init-initial-digest" => B(BaseError::ProvenanceMismatch),
            "descriptor-last-mutation" => RowBaseError::Ledger(Ledger::OrderMismatch),
            "att-missing"
            | "aging-descriptor-missing"
            | "catalog-scope-terminal"
            | "retained-projection" => B(BaseError::IdentityMismatch),
            "inventory-generation" => B(BaseError::Corrupt),
            "system-item-owner" | "create-item-order" => {
                B(BaseError::Inventory(InventoryError::Corrupt))
            }
            "reserved-row-root" => RowBaseError::Unsupported,
            _ => continue,
        };
        seen += 1;
        // Each variant passes the raw layout, so the closure owns the rejection.
        decode_layout(&**bytes, expected()).unwrap();
        assert_eq!(
            decode(&**bytes, expected(), &mut scratch).err(),
            Some(expected_error),
            "{name}"
        );
    }
    assert_eq!(seen, 20);
}

/// Borrowed rows of one accepted literal for the Profile3 XOI mode.
struct Rows<'a> {
    bytes: &'a [u8; BASE_BYTES],
    graph: Graph<'a>,
}
impl Rows<'_> {
    fn slot(&self, slot: usize) -> Option<&[u8; 1536]> {
        let bytes: &[u8; 1536] = self.graph.layout.descriptor_bank()[slot * 1536..][..1536]
            .try_into()
            .unwrap();
        bytes.iter().any(|byte| *byte != 0).then_some(bytes)
    }
}
impl InventoryRows for Rows<'_> {
    fn transaction(&self, index: usize) -> Option<&[u8; 1536]> {
        (index < 32).then(|| self.slot(index)).flatten()
    }
    fn allocation(&self) -> Option<&[u8; 1536]> {
        self.slot(32)
    }
    fn aging(&self) -> Option<&[u8; 1536]> {
        self.slot(34)
    }
    fn root(&self) -> &[u8; PAGE_BYTES] {
        image(self.bytes, 1)
    }
    fn allocator(&self) -> &[u8; PAGE_BYTES] {
        image(self.bytes, 0)
    }
    fn job(&self, ordinal: usize) -> BaseResult<&[u8]> {
        self.graph.jobs[ordinal / 15]
            .cell(ordinal % 15)
            .ok_or(BaseError::InvalidInput)
    }
}

#[test]
fn base4_closure_row_xoi_mode_reproduces_inventory() {
    let reserved = state("state-system-reserved");
    let retained = state("state-create-retained");
    let create = state("state-create-reserved");
    let mut seen = 0;
    for (name, bytes) in [
        ("genesis", GENESIS),
        ("aging", AGING),
        ("live", LIVE),
        ("state-system-reserved", &*reserved),
        ("state-create-retained", &*retained),
        ("state-create-reserved", &*create),
    ] {
        let layout = decode_layout(bytes, expected()).unwrap();
        let header = layout.header();
        let rows = Rows {
            bytes,
            graph: Graph::new(layout).unwrap(),
        };
        let mut output = Box::new([0xa5; PAGE_BYTES]);
        let result = encode_row_current(
            expected().profile,
            header.generation,
            header.capture_lsn,
            &rows,
            &mut output,
        );
        if name == "state-create-reserved" {
            // Job3 Catalog CREATE items need the Profile3 projection (U2-C3).
            assert_eq!(result, Err(BaseError::Unsupported), "{name}");
            continue;
        }
        // Current credit is every active owner's residual, as in the header.
        let wal = header.remaining.iter().map(|row| row.wal).sum::<u64>();
        let steps = header.remaining.iter().map(|row| row.steps).sum::<u64>();
        assert_eq!(result, Ok((wal, steps)), "{name}");
        assert_eq!(*output, bytes[INVENTORY..INVENTORY + PAGE_BYTES], "{name}");
        seen += 1;
    }
    assert_eq!(seen, 5);
}
