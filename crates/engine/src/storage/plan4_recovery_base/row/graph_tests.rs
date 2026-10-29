use super::super::{ARENA, BANK, CERTIFICATES, INVENTORY, PROVENANCE};
use super::*;
use crate::storage::{
    control,
    plan4_recovery_base::{ControlRef, Expected, SelectedControl},
    retention_page::RetentionError,
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

#[test]
fn base4_closure_accepts_genesis_and_aging_start() {
    let empty = InventoryObservation {
        owners: 0,
        items: 0,
        chunks: 0,
        read_calls: 0,
    };
    let genesis = RowCensus {
        transactions: 0,
        systems: 0,
        statuses: 0,
        actors: 0,
        retained: 0,
        releasing: 0,
        tables: 0,
        rows: 0,
        inventory: empty,
    };
    let aging = RowCensus {
        systems: 1,
        inventory: InventoryObservation { owners: 1, ..empty },
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
fn base4_closure_live_actor_reaches_allocation_boundary() {
    // Every relation before the System boundary closes on the live literal.
    let layout = decode_layout(LIVE, expected()).unwrap();
    let graph = Graph::new(layout).unwrap();
    let mut scratch = scratch();
    graph.check_queue(&mut scratch).unwrap();
    assert_eq!(graph.issuer(), Ok(3));
    assert_eq!(graph.actors(&mut scratch), Ok([1, 2, 0]));
    assert_eq!(graph.tables(), Ok([1, 3]));
    assert_eq!(graph.payloads(3), Ok(2));
    graph.provenance(&mut scratch[0]).unwrap();
    assert_eq!(ledger::validate(&graph, &mut scratch), Ok([1, 1]));
    let inventory = ledger::inventory(&graph, 2, &mut scratch).unwrap();
    assert_eq!((inventory.owners, inventory.items), (2, 0));
    assert_eq!(
        ledger::system_allocation(&graph),
        Err(RowBaseError::AllocationUnwired)
    );
    assert_eq!(
        decode(LIVE, expected(), &mut scratch).err(),
        Some(RowBaseError::AllocationUnwired)
    );
    let (output, encoded) = encoded(LIVE);
    assert_eq!(*output, *LIVE);
    assert_eq!(encoded, Err(RowBaseError::AllocationUnwired));
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
    assert_eq!(
        decode(NO_INVENTORY, expected(), &mut scratch).err(),
        Some(RowBaseError::Corrupt)
    );
}

#[test]
fn base4_closure_relation_variants() {
    use crate::storage::semantic_row::Error as Ledger;
    let mut scratch = scratch();
    let mut seen = 0;
    for (name, bytes) in &variants() {
        let expected_error = match name.as_str() {
            "terminal-without-system" => RowBaseError::Identity,
            "aging-header-budget" => RowBaseError::Budget,
            "issuer-undo-high" => RowBaseError::Order,
            "row-high-global" => RowBaseError::Order,
            "committed-q-collision" => RowBaseError::Order,
            "catalog-row-mixing" => RowBaseError::Identity,
            "free-undo-generation" => RowBaseError::Order,
            "init-initial-digest" => RowBaseError::Provenance,
            "descriptor-last-mutation" => RowBaseError::Ledger(Ledger::OrderMismatch),
            "att-missing" => RowBaseError::Identity,
            "inventory-generation" => RowBaseError::Corrupt,
            "aging-descriptor-missing" => RowBaseError::Identity,
            "catalog-scope-terminal" => RowBaseError::Unsupported,
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
    assert_eq!(seen, 14);
}
