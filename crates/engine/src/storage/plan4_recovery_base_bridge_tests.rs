//! New bridge boundaries only. Parent helpers supply frozen independent raw,
//! explicit selection and original-output markers; no old test is called.
use super::*;
use crate::storage::wal_io::{self, Plan4TestSemanticError as SemanticError};
use std::cell::Cell;

#[test]
fn xrb_catalog_bridge_literals() {
    cases("BC", 6, |i| {
        let f = Fixture::new();
        let mut out = boxed::<PAGE_BYTES>();
        out.fill(0xa5);
        let mut ns = namespace();
        let ordinal = match i {
            1 => 7,
            2 => 8,
            3 => 6,
            4 => 9,
            5 | 6 => {
                ns.timeline = if i == 5 { 0 } else { 2 };
                7
            }
            _ => unreachable!(),
        };
        let result = wal_io::encode_plan4_catalog_genesis(ns, ordinal, &mut out);
        if i <= 2 {
            result.unwrap();
            assert_eq!(
                out.as_slice(),
                f.read(&format!("references/P{ordinal:02}.bin"))
            );
        } else {
            assert!(matches!(
                result,
                Err(wal_io::Plan4GenesisCause::InvalidInput)
            ));
            assert!(out.iter().all(|b| *b == 0xa5));
        }
    });
}

#[test]
fn xrb_allocation_bridge_states() {
    cases("BA", 24, |i| {
        let f = Fixture::new();
        let name = match i {
            2 | 16 | 17 | 20 | 23 => "reserved",
            3 | 18 => "cancelled",
            4 | 19 => "init",
            5 => "reserved32",
            _ => "reserved", // Initial-page cases replace the allocator below.
        };
        let raw = f.raw(name);
        let initial = matches!(i, 1 | 6..=15 | 21 | 22 | 24);
        let mut page = if initial {
            f.read("references/P00.bin")
        } else {
            raw[65536..81920].to_vec()
        };
        let mut desc: [u8; 1536] = raw[53248..54784].try_into().unwrap();
        let mut high = if initial { 0 } else { number(&raw, 208) };
        let mut terminal = if initial { 0 } else { raw[304] };
        let mut has_descriptor = !initial;
        let entry = |ordinal: usize| 10208 + ordinal * 96;
        let want = match i {
            1..=5 => None,
            6 => {
                set(&mut page, entry(9) + 8, 2);
                Some(SemanticError::StateMismatch)
            }
            7..=15 => {
                set(&mut page, entry(i - 7) + 8, 2);
                Some(SemanticError::PageMismatch)
            }
            16 => {
                page.copy_within(entry(9)..entry(10), entry(10));
                Some(SemanticError::PageMismatch)
            }
            17 => {
                terminal = 1;
                Some(SemanticError::StateMismatch)
            }
            18 => {
                terminal = 0;
                Some(SemanticError::StateMismatch)
            }
            19 => {
                terminal = 1;
                Some(SemanticError::StateMismatch)
            }
            20 => {
                high = 2;
                Some(SemanticError::PageMismatch)
            }
            21 => {
                desc.fill(0);
                has_descriptor = true;
                Some(SemanticError::StateMismatch)
            }
            22 => {
                set(&mut page, 64, 1);
                Some(SemanticError::StateMismatch)
            }
            23 => {
                set(&mut desc, 608, 21201);
                seal(&mut desc);
                Some(SemanticError::StateMismatch)
            }
            24 => {
                let at = entry(9);
                page[at] = 2;
                page[at + 1] = 8; // Undo: valid common kind, outside plan4 Status scope.
                set(&mut page, at + 8, 2);
                set(&mut page, at + 16, 3);
                set(&mut page, at + 24, 1);
                Some(SemanticError::PageMismatch)
            }
            _ => unreachable!(),
        };
        seal(&mut page);
        let original = content_digest(&page);
        let result = wal_io::validate_plan4_base_allocation(
            page.as_slice().try_into().unwrap(),
            f.expected.profile,
            has_descriptor.then_some(&desc),
            high,
            terminal,
        );
        assert_eq!(result.err(), want);
        assert_eq!(content_digest(&page), original);
    });
}

struct ChunkSource<'a> {
    bytes: &'a [u8],
    calls: usize,
    fault: usize,
}

#[test]
fn xrb_reserved_terminal_adjacency() {
    cases("BB", 1, |_| {
        let f = Fixture::new();
        let mut raw = f.raw("reserved");
        assert_eq!(number(&raw, 96), 1);
        assert_eq!(number(&raw, 248), 1);
        // Update both header copies so the new cross-section guard is reached
        // before the unchanged inventory capture check. No late WAL group may
        // intervene while this closed owner holds a linear reservation.
        set(&mut raw, 96, 2);
        set(&mut raw, 248, 2);
        base_seal(&mut raw);
        rejected(&raw, f.expected, Error::OrderMismatch);
    });
}
impl InventorySource for ChunkSource<'_> {
    fn read_chunk(
        &mut self,
        index: u64,
        output: &mut [u8; PAGE_BYTES],
    ) -> std::result::Result<usize, InventoryError> {
        self.calls += 1;
        assert_eq!(index, 0);
        if self.fault == 7 && self.calls == 5 {
            return Err(InventoryError::Source);
        }
        output[..256].copy_from_slice(&self.bytes[211072..211328]);
        if self.fault == 8 && self.calls == 5 {
            output[120] = 1;
            seal(&mut output[..256]);
        }
        Ok(256)
    }
}

#[test]
fn xrb_borrowed_inventory_bridge() {
    cases("BI", 8, |i| {
        let f = Fixture::new();
        let mut raw = f.raw(if i == 1 {
            "init"
        } else if i == 3 {
            "reserved32"
        } else {
            "reserved"
        });
        let owners = if i == 1 {
            0
        } else if i == 3 {
            34
        } else {
            1
        };
        if i == 6 {
            set16(&mut raw, 211202, 1);
            seal_inventory(&mut raw);
        }
        let root_len = 160 + owners * 56;
        let expected = InventoryExpected {
            namespace: namespace(),
            reference: ControlRef {
                kind: ControlKind::OwnershipRoot,
                slot: 2,
                index: 0,
                generation: number(&raw, 88),
                encoded_len: root_len as u32,
                digest: content_digest(&raw[208896..208896 + root_len]),
            },
            capture_lsn: number(&raw, 96),
            profile: retention_obligation::profile_reference(f.expected.profile),
            item_limit: 1,
        };
        let bank: &[u8; BASE_BYTES] = raw.as_slice().try_into().unwrap();
        let calls = Cell::new(0);
        let callback = |ordinal| {
            calls.set(calls.get() + 1);
            if i == 4 {
                return Err(InventoryError::Source);
            }
            active_descriptor(bank, f.expected, ordinal).map_err(|_| InventoryError::Corrupt)
        };
        let mut source = ChunkSource {
            bytes: &raw,
            calls: 0,
            fault: i,
        };
        let mut workspace = scratch();
        let result = ownership::verify_borrowed_inventory(
            &raw[208896..208896 + root_len],
            expected,
            if i == 5 { 37 } else { owners },
            callback,
            &mut source,
            &mut workspace,
        );
        match i {
            1..=3 => {
                let observed = result.unwrap();
                assert_eq!(observed.owners, owners);
                assert_eq!(observed.items, u64::from(i != 1));
                assert_eq!(observed.read_calls, if i == 1 { 0 } else { 5 });
                assert_eq!(source.calls, observed.read_calls as usize);
                if i == 1 {
                    assert_eq!(calls.get(), 0);
                }
            }
            4 | 7 => assert_eq!(result, Err(InventoryError::Source)),
            5 => {
                assert_eq!(result, Err(InventoryError::InvalidInput));
                assert_eq!(calls.get(), 0);
                assert_eq!(source.calls, 0);
            }
            6 => {
                assert_eq!(result, Err(InventoryError::Corrupt));
                assert_eq!(calls.get(), 2); // expected + root; invalid owner never reaches callback.
                assert_eq!(source.calls, 1);
            }
            8 => assert_eq!(result, Err(InventoryError::Corrupt)),
            _ => unreachable!(),
        }
        if matches!(i, 7 | 8) {
            assert_eq!(source.calls, 5);
        }
    });
}

#[test]
fn xrb_bridge_source_layout() {
    cases("BST", 1, |_| {
        let inventory = ownership::borrowed_inventory_source_bytes();
        let allocation = wal_io::plan4_base_allocation_source_bytes();
        let catalog = wal_io::plan4_catalog_genesis_source_bytes();
        let combined = source_bytes();
        println!(
            "XB_BRIDGE_SOURCE\tinventory={inventory}\tallocation={allocation}\tcatalog={catalog}\tbase={combined}\tcaller_base={}\tcaller_scratch={}\tstatus=MEASUREMENT_NOT_F05",
            BASE_BYTES,
            4 * PAGE_BYTES
        );
        assert!(combined >= inventory.max(allocation).max(catalog));
    });
}
