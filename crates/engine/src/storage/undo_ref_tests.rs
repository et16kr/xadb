use super::*;
use crate::storage::page::FileId;

fn hex(input: &str) -> Vec<u8> {
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}

fn page() -> PageRef {
    PageRef {
        file: FileId::new(0x0000_0001_0000_0002).unwrap(),
        page_no: 3,
        incarnation: Generation::new(4).unwrap(),
    }
}

#[test]
fn page_and_undo_references_match_independent_literals_and_exact_lengths() {
    let page_expected = hex("020000000100000003000000000000000400000000000000");
    let undo_expected =
        hex("02000000010000000300000000000000040000000000000005000000000000000600000000000000");
    let undo = UndoRef::new(page(), 5, Generation::new(6).unwrap()).unwrap();
    assert_eq!(undo.page(), page());
    assert_eq!(undo.slot(), 5);
    assert_eq!(undo.generation().get(), 6);
    let mut bytes = [0xa5; UNDO_REF_BYTES];
    undo.encode(&mut bytes).unwrap();
    assert_eq!(bytes.as_slice(), undo_expected);
    assert_eq!(UndoRef::decode(&undo_expected).unwrap(), undo);
    page().encode(&mut bytes[..24]).unwrap();
    assert_eq!(&bytes[..24], page_expected);
    assert_eq!(PageRef::decode(&page_expected).unwrap(), page());
    for size in 0..=41 {
        if size != 40 {
            let mut output = vec![0xa5; size];
            assert_eq!(undo.encode(&mut output), Err(PageError::InvalidInput));
            assert_eq!(output, vec![0xa5; size]);
            let mut input = undo_expected.clone();
            input.resize(size, 0);
            assert!(UndoRef::decode(&input).is_err());
        }
        if size != 24 {
            let mut output = vec![0xa5; size];
            assert_eq!(page().encode(&mut output), Err(PageError::InvalidInput));
            assert_eq!(output, vec![0xa5; size]);
            let mut input = page_expected.clone();
            input.resize(size, 0);
            assert!(PageRef::decode(&input).is_err());
        }
    }
}

#[test]
fn reference_decoders_reject_zero_identity_reserved_and_out_of_range_slots() {
    let mut bytes = [0; UNDO_REF_BYTES];
    let undo = UndoRef::new(page(), 0, Generation::new(u64::MAX).unwrap()).unwrap();
    undo.encode(&mut bytes).unwrap();
    for (start, end) in [(0, 4), (4, 8), (16, 24), (32, 40)] {
        let mut damaged = bytes;
        damaged[start..end].fill(0);
        assert_eq!(UndoRef::decode(&damaged), Err(PageError::Corrupt));
    }
    for offset in 26..32 {
        let mut damaged = bytes;
        damaged[offset] = 1;
        assert_eq!(UndoRef::decode(&damaged), Err(PageError::Corrupt));
    }
    assert_eq!(MAX_PAGE_SLOTS, 3_244);
    for slot in [0, 3_243] {
        let reference = UndoRef::new(page(), slot, Generation::new(1).unwrap()).unwrap();
        reference.encode(&mut bytes).unwrap();
        assert_eq!(UndoRef::decode(&bytes).unwrap(), reference);
    }
    for slot in [3_244_u16, u16::MAX] {
        assert!(UndoRef::new(page(), slot, Generation::new(1).unwrap()).is_err());
        bytes[24..26].copy_from_slice(&slot.to_le_bytes());
        assert_eq!(UndoRef::decode(&bytes), Err(PageError::Corrupt));
    }
    for page_no in [0, u64::MAX] {
        let page = PageRef { page_no, ..page() };
        let mut wire = [0; PAGE_REF_BYTES];
        page.encode(&mut wire).unwrap();
        assert_eq!(PageRef::decode(&wire).unwrap(), page);
    }
}
