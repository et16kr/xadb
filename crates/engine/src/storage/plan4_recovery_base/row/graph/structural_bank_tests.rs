use super::*;
const ROOT: &[u8; BASE_BYTES] = fixture!("structural-bank/root.xrb");
const CHILD: &[u8; BASE_BYTES] = fixture!("structural-bank/child.xrb");
const LIVE_CHILD: &[u8; BASE_BYTES] = fixture!("structural-bank/live-child.xrb");
const LIVE_ROOT: &[u8; BASE_BYTES] = fixture!("structural-bank/live-root.xrb");

fn put(raw: &mut [u8], at: usize, value: u64) {
    raw[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn seal(raw: &mut [u8]) {
    let end = raw.len() - 32;
    let digest = content_digest(&raw[..end]);
    raw[end..].copy_from_slice(&digest);
}
fn reseal(raw: &mut [u8; BASE_BYTES]) {
    for (index, (offset, len)) in [
        (BANK, DESCRIPTOR_BYTES),
        (CERTIFICATES, 64 * 96),
        (PROVENANCE, 64 * 352),
        (INVENTORY, PAGE_BYTES),
        (ARENA, 64 * PAGE_BYTES),
    ]
    .into_iter()
    .enumerate()
    {
        let digest = content_digest(&raw[offset..offset + len]);
        raw[1280 + index * 64 + 32..1280 + (index + 1) * 64].copy_from_slice(&digest);
    }
    seal(raw);
}
fn descriptor_id(bytes: &mut [u8; BASE_BYTES], id: u64) {
    put(bytes, 216, id);
    let d = &mut bytes[BANK + 33 * 1536..BANK + 34 * 1536];
    for at in [56, 72, 88, 544] {
        put(d, at, id);
    }
    seal(d);
}
/// Add a separately allocated Structural leaf63 while preserving the original
/// CREATE pair10/11. This is an observed layout, not a historical split proof.
fn structural_claim(bytes: &mut [u8; BASE_BYTES], birth: u64, epoch: u32) {
    bytes.copy_within(
        ARENA + 11 * PAGE_BYTES..ARENA + 12 * PAGE_BYTES,
        ARENA + 63 * PAGE_BYTES,
    );
    let p = &mut bytes[ARENA + 63 * PAGE_BYTES..ARENA + 64 * PAGE_BYTES];
    put(p, 32, 63);
    put(p, 64, 70);
    seal(p);
    let digest = content_digest(p);
    let c = CERTIFICATES + 63 * 96;
    bytes.copy_within(CERTIFICATES + 11 * 96..CERTIFICATES + 12 * 96, c);
    put(bytes, c + 16, 63);
    put(bytes, c + 48, 70);
    bytes[c + 56..c + 88].copy_from_slice(&digest);
    let a = &mut bytes[ARENA..ARENA + PAGE_BYTES];
    let entries = usize::from(u16::from_le_bytes(a[128..130].try_into().unwrap())) + 64;
    a.copy_within(entries + 11 * 96..entries + 12 * 96, entries + 63 * 96);
    seal(a);
    let digest = content_digest(a);
    bytes[CERTIFICATES + 56..CERTIFICATES + 88].copy_from_slice(&digest);
    let mut claim = [0; 352];
    claim[0] = 1;
    claim[1] = 5;
    claim[2] = PageKind::RowLeaf as u8;
    claim[4] = 1;
    claim[8..48].copy_from_slice(&bytes[c + 8..c + 48]);
    put(&mut claim, 48, birth);
    claim[56..60].copy_from_slice(&epoch.to_le_bytes());
    claim[64..128].fill(0x51);
    claim[128..176].copy_from_slice(&fixture!("structural-bank/child.xod")[24..72]);
    put(&mut claim, 160, birth);
    put(&mut claim, 168, u64::from(epoch));
    put(&mut claim, 176, birth);
    for (at, value) in [(184, 6), (192, 1), (200, 1), (280, 2), (288, 1), (296, 2)] {
        put(&mut claim, at, value);
    }
    let old = &bytes[PROVENANCE + 11 * 352..PROVENANCE + 12 * 352];
    claim[208..232].copy_from_slice(&old[208..232]); // actual CREATE anchor10
    claim[232..256].copy_from_slice(&old[232..256]);
    put(&mut claim, 240, 11); // historical left; original CREATE leaf
    claim[304..336].fill(0x73);
    bytes[PROVENANCE + 63 * 352..PROVENANCE + 64 * 352].copy_from_slice(&claim);
    reseal(bytes);
}
fn ledger(bytes: &[u8; BASE_BYTES]) -> BaseResult<(usize, usize)> {
    let graph = Graph::new(decode_layout(bytes, expected())?).unwrap();
    seam::validate_ledger(&graph, ReservedInventory::None)
}

#[test]
fn structural_bank_base_accepts_retired_claims_without_active_inventory_owner() {
    for bytes in [ROOT, CHILD, LIVE_ROOT, LIVE_CHILD] {
        let view = decode(bytes, expected(), &mut scratch()).unwrap();
        assert_eq!(view.header().system_high_water[1], 70);
        assert!(view.descriptor(33).unwrap().is_some());
        assert_eq!(
            view.census().systems,
            u32::from(std::ptr::eq(bytes, LIVE_ROOT) || std::ptr::eq(bytes, LIVE_CHILD))
        );
        assert_eq!(
            view.census().inventory.owners,
            view.census().systems as usize + view.census().transactions as usize
        );
    }
}

#[test]
fn structural_bank_base_rejects_missing_high_water_future_cut_and_wrong_epoch() {
    for mutation in 0..5 {
        let mut bytes = backing(ROOT);
        match mutation {
            0 => bytes[BANK + 33 * 1536..BANK + 34 * 1536].fill(0),
            1 => put(&mut bytes[..], 216, 0),
            2 => put(&mut bytes[..], 216, 71),
            3 => {
                let d = &mut bytes[BANK + 33 * 1536..BANK + 34 * 1536];
                put(d, 64, 2);
                seal(d);
            }
            _ => {
                let d = &mut bytes[BANK + 33 * 1536..BANK + 34 * 1536];
                put(d, 608, 1);
                seal(d);
            }
        }
        reseal(&mut bytes);
        assert!(
            decode(&*bytes, expected(), &mut scratch()).is_err(),
            "mutation {mutation}"
        );
    }
    // The shared header does not open Structural high-water to old Base3.
    let mut header = decode_layout(ROOT, expected()).unwrap().header();
    header.format = crate::storage::plan4_recovery_base::BaseFormat::V3Catalog;
    let mut p2 = [0; 4096];
    control::encode_catalog_profile(expected().profile.spec(), &mut p2).unwrap();
    let old = Expected {
        profile: control::decode_bootstrap_profile(&p2, None).unwrap(),
        ..expected()
    };
    assert!(crate::storage::plan4_recovery_base::validate_header(header, old).is_err());
}

#[test]
fn structural_bank_latest_claim_binds_operation_owner_and_monotonic_epoch() {
    let mut bytes = backing(LIVE_CHILD);
    structural_claim(&mut bytes, 70, 1);
    assert_eq!(ledger(&bytes).unwrap(), (1, 1));
    let mut root = backing(LIVE_ROOT);
    structural_claim(&mut root, 70, 1);
    assert_eq!(ledger(&root), Err(BaseError::ProvenanceMismatch));
    // An earlier split in the same epoch may have a different operation.
    structural_claim(&mut root, 69, 1);
    assert_eq!(ledger(&root).unwrap(), (1, 1));
    // A claim newer than the last descriptor cannot be inherited.
    descriptor_id(&mut bytes, 69);
    reseal(&mut bytes);
    assert_eq!(ledger(&bytes), Err(BaseError::ProvenanceMismatch));
    let mut later = backing(LIVE_CHILD);
    later[144..148].copy_from_slice(&2u32.to_le_bytes());
    later[1184..1188].copy_from_slice(&2u32.to_le_bytes());
    structural_claim(&mut later, 69, 2);
    assert_eq!(ledger(&later), Err(BaseError::ProvenanceMismatch));
    // Same id and monotonic epoch are insufficient: the complete latest owner
    // must agree, including its epoch/generation.
    let d = &mut later[BANK + 33 * 1536..BANK + 34 * 1536];
    put(d, 64, 2);
    seal(d);
    structural_claim(&mut later, 70, 1);
    assert_eq!(ledger(&later), Err(BaseError::ProvenanceMismatch));
}

#[test]
fn structural_bank_does_not_open_unimplemented_structural_tree_closure() {
    assert!(decode(LIVE_CHILD, expected(), &mut scratch()).is_ok());
    let mut bytes = backing(LIVE_CHILD);
    structural_claim(&mut bytes, 70, 1);
    assert!(decode_layout(&*bytes, expected()).is_ok());
    assert!(ledger(&bytes).is_ok());
    assert_eq!(
        decode(&*bytes, expected(), &mut scratch()).err(),
        Some(RowBaseError::Base(BaseError::ProvenanceMismatch))
    );
}
