use super::*;
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
fn root() -> &'static [u8; PAGE_BYTES] {
    fixture!("base4-closure/genesis.xrb")[262144 + PAGE_BYTES..262144 + 2 * PAGE_BYTES]
        .try_into()
        .unwrap()
}
fn bank(raw: &[u8; 1536]) -> Box<[u8; BANK_BYTES]> {
    let mut bytes = Box::new([0; BANK_BYTES]);
    let at = 34 * RAM_ROW;
    bytes[at..at + 1536].copy_from_slice(raw);
    bytes[at + 1536..at + 1544].copy_from_slice(&70u64.to_le_bytes());
    bytes[at + 1624] = 1;
    bytes
}

#[test]
fn structural_bank_capture_packs_ram34_as_wire33_with_no_active_debt() {
    for raw in [
        fixture!("structural-bank/root.xod"),
        fixture!("structural-bank/child.xod"),
    ] {
        let bank = bank(raw);
        let mut packed = Box::new([0xff; base::DESCRIPTOR_BYTES]);
        assert_eq!(remaining(profile(), &bank, root(), 70, 1).unwrap(), (0, 0));
        assert_eq!(
            pack_bank(profile(), &bank, &mut packed, root(), 70, 1).unwrap(),
            (0, 0, [RemainingBudget::default(); 5])
        );
        assert_eq!(&packed[33 * 1536..34 * 1536], raw);
        assert!(packed[..33 * 1536].iter().all(|byte| *byte == 0));
        assert!(packed[34 * 1536..].iter().all(|byte| *byte == 0));
        assert_eq!(u64_at(&bank[..], RAM_STRUCTURAL * RAM_ROW + 1536), 70);
    }
}

#[test]
fn structural_bank_capture_and_remaining_reject_stale_cut_epoch_and_ram_metadata() {
    let mut bank = bank(fixture!("structural-bank/root.xod"));
    let mut packed = Box::new([0; base::DESCRIPTOR_BYTES]);
    for (cut, epoch) in [(69, 1), (70, 0)] {
        assert!(remaining(profile(), &bank, root(), cut, epoch).is_err());
        assert!(pack_bank(profile(), &bank, &mut packed, root(), cut, epoch).is_err());
    }
    for at in [1536, 1544, 1624, 1663] {
        bank[RAM_STRUCTURAL * RAM_ROW + at] ^= 1;
        assert!(remaining(profile(), &bank, root(), 70, 1).is_err());
        assert!(pack_bank(profile(), &bank, &mut packed, root(), 70, 1).is_err());
        bank[RAM_STRUCTURAL * RAM_ROW + at] ^= 1;
    }
    let mut bytes = [0; 4096];
    crate::storage::control::encode_catalog_profile(profile().spec(), &mut bytes).unwrap();
    let p2 = crate::storage::control::decode_bootstrap_profile(&bytes, None).unwrap();
    assert!(remaining(p2, &bank, root(), 70, 1).is_err());
    assert!(pack_bank(p2, &bank, &mut packed, root(), 70, 1).is_err());
    bank.fill(0);
    assert_eq!(remaining(p2, &bank, root(), 70, 1).unwrap(), (0, 0));
}
