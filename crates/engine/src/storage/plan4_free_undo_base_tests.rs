//! New mixed SystemInit base tests against separately generated immutable bytes.
use super::*;
#[allow(
    clippy::duplicate_mod,
    reason = "independent input helper shared by two sibling test consumers"
)]
#[path = "wal_io/plan4_retention/free_undo_literal_tests.rs"]
mod inputs;
use inputs::{BaseFault as Fault, Oracle};

fn expected(oracle: &Oracle) -> Expected {
    let profile =
        super::super::control::decode_bootstrap_profile(oracle.reference("profile.bin"), None)
            .unwrap();
    let filemap = oracle.reference("filemap.bin");
    Expected {
        profile,
        filemap: SelectedControl {
            generation: get(filemap, 32),
            reference: ControlRef::decode(&filemap[80..144]).unwrap(),
        },
        bootstrap_digest: content_digest(oracle.reference("bootstrap.bin")),
    }
}
fn scratch() -> Box<[[u8; PAGE_BYTES]; 4]> {
    vec![[0; PAGE_BYTES]; 4]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
fn original_claim(oracle: &Oracle, claim: InitClaim) -> Result<()> {
    let ordinal = claim.header.identity.page.page_no;
    let Some(lsn) = (match ordinal {
        9 => Some(2),
        10 => Some(4),
        11 => Some(6),
        _ => None,
    }) else {
        return Err(Error::ProvenanceMismatch);
    };
    if claim.header.lsn.0 != lsn || claim.full_digest != content_digest(oracle.native_page(ordinal))
    {
        return Err(Error::ProvenanceMismatch);
    }
    Ok(())
}
#[test]
fn free_undo_base_literal_reconstruction_and_capture() {
    let oracle = Oracle::new();
    let e = expected(&oracle);
    let mut scratch = scratch();
    let view = decode(oracle.base(), e, &mut scratch).unwrap();
    assert_eq!(view.header().format, BaseFormat::V2System);
    assert_eq!(view.header().status_mask, 1 << 9);
    assert_eq!(
        view.header().allocation_terminal,
        AllocationTerminal::InitializedUndo
    );
    assert_eq!(view.census().statuses, 1);
    for index in 9..=11 {
        assert!(view.image(index, &mut scratch[0]).unwrap());
        assert_eq!(&scratch[0], oracle.native_page(index as u64));
        let header = view.certificate(index).unwrap().unwrap().header;
        let mut record = [0xa5; STATUS_BYTES];
        encode_system_record(
            e,
            header,
            header.lsn.0,
            oracle.native_page(index as u64),
            &mut record,
        )
        .unwrap();
        assert_eq!(&record, view.status_record(index).unwrap());
        let mut certificate = [0xa5; CERTIFICATE_BYTES];
        encode_system_certificate(
            e,
            header,
            oracle.native_page(index as u64),
            &mut certificate,
        )
        .unwrap();
        assert_eq!(
            &certificate[..],
            &oracle.base()[CERTIFICATES + index * 96..CERTIFICATES + (index + 1) * 96]
        );
    }
    let mut claims = 0;
    view.check_init_provenance(&mut scratch[0], |claim| {
        claims += 1;
        original_claim(&oracle, claim)
    })
    .unwrap();
    assert_eq!(claims, 3);
    let bytes = oracle.base();
    let mut output: Box<[u8; BASE_BYTES]> = vec![0xa5; BASE_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    encode(
        view.header(),
        e,
        Sections {
            descriptors: bytes[BANK..BANK + DESCRIPTOR_BYTES].try_into().unwrap(),
            allocator: bytes[ALLOCATOR..ALLOCATOR + PAGE_BYTES].try_into().unwrap(),
            queue: bytes[QUEUE..QUEUE + 6 * PAGE_BYTES]
                .as_chunks::<PAGE_BYTES>()
                .0
                .try_into()
                .unwrap(),
            statuses: bytes[STATUS..STATUS + 64 * STATUS_BYTES]
                .as_chunks::<STATUS_BYTES>()
                .0
                .try_into()
                .unwrap(),
            certificates: bytes[CERTIFICATES..CERTIFICATES + 64 * CERTIFICATE_BYTES]
                .as_chunks::<CERTIFICATE_BYTES>()
                .0
                .try_into()
                .unwrap(),
            inventory: bytes[INVENTORY..INVENTORY + PAGE_BYTES].try_into().unwrap(),
        },
        &mut output,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(&*output, oracle.base());
}
#[test]
fn free_undo_base_rejects_role_scope_and_forged_provenance() {
    let oracle = Oracle::new();
    let e = expected(&oracle);
    let mut scratch = scratch();
    for fault in [
        Fault::LegacyVersion,
        Fault::ReservedScope,
        Fault::WrongRoleTag,
        Fault::WrongOrdinal,
    ] {
        assert!(decode(&oracle.bad_base(fault), e, &mut scratch).is_err());
    }
    let forged = oracle.bad_base(Fault::ForgedInit);
    let view = decode(&forged, e, &mut scratch).unwrap();
    assert_eq!(
        view.check_init_provenance(&mut scratch[0], |claim| original_claim(&oracle, claim)),
        Err(Error::ProvenanceMismatch)
    );
    let view = decode(oracle.base(), e, &mut scratch).unwrap();
    assert_eq!(
        view.check_init_provenance(&mut scratch[0], |_| Err(Error::ProvenanceMismatch)),
        Err(Error::ProvenanceMismatch)
    );
}
#[test]
fn free_undo_record_refuses_noncanonical_and_preserves_output() {
    let oracle = Oracle::new();
    let e = expected(&oracle);
    let mut scratch = scratch();
    let view = decode(oracle.base(), e, &mut scratch).unwrap();
    let header = view.certificate(10).unwrap().unwrap().header;
    let mut output = [0xa5; STATUS_BYTES];
    for (at, value) in [(15920 + 6, 1u8), (15920 + 8, 2), (15920 + 16, 1), (78, 1)] {
        let mut bytes = oracle.native_page(10).to_vec();
        bytes[at] = value;
        let digest = content_digest(&bytes[..PAGE_BYTES - 32]);
        bytes[PAGE_BYTES - 32..].copy_from_slice(&digest);
        assert!(
            encode_system_record(
                e,
                header,
                4,
                bytes.as_slice().try_into().unwrap(),
                &mut output
            )
            .is_err()
        );
        assert_eq!(output, [0xa5; STATUS_BYTES]);
    }
    assert!(encode_system_record(e, header, 3, oracle.native_page(10), &mut output).is_err());
    assert_eq!(output, [0xa5; STATUS_BYTES]);
    let mut certificate = [0xa5; CERTIFICATE_BYTES];
    assert!(encode_certificate(e, header, oracle.native_page(10), &mut certificate).is_err());
    assert_eq!(certificate, [0xa5; CERTIFICATE_BYTES]);
}
