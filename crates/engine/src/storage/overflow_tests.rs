use super::*;
use crate::api::DatabaseId;
use crate::storage::page::{FileId, PageLsn};
use xadb_foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};

fn bytes(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0);
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn address(index: usize) -> PageRef {
    PageRef {
        file: FileId::new(0x0000_0001_0000_0002).unwrap(),
        page_no: 3 + index as u64,
        incarnation: Generation::new(4).unwrap(),
    }
}

fn header(index: usize) -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: DatabaseId::new([11; 16]).unwrap(),
            page: address(index),
            owner: ObjectId::new(5).unwrap(),
            owner_generation: Generation::new(6).unwrap(),
        },
        kind: PageKind::Overflow,
        level: 0,
        lsn: PageLsn(7),
    }
}

fn payload(total: usize) -> Vec<u8> {
    (0..total).map(|i| (i * 17 + 3) as u8).collect()
}

fn head(data: &[u8]) -> OverflowHead {
    OverflowHead::new(address(0), data.len(), content_digest(data)).unwrap()
}

fn make_cell(owner: OverflowHead, index: usize, data: &[u8]) -> Vec<u8> {
    let part =
        &data[index * MAX_OVERFLOW_PAYLOAD..((index + 1) * MAX_OVERFLOW_PAYLOAD).min(data.len())];
    let mut cell = vec![0; OVERFLOW_HEADER_BYTES + part.len()];
    encode_overflow_cell(
        owner,
        index,
        address(index),
        (index + 1 < owner.count()).then(|| address(index + 1)),
        part,
        &mut cell,
    )
    .unwrap();
    cell
}

fn raw_page(header: PageHeader, cell: &[u8]) -> Vec<u8> {
    let mut page = vec![0; PAGE_BYTES];
    encode_page(header, &[cell], &mut page).unwrap();
    page
}

fn make_page(owner: OverflowHead, index: usize, data: &[u8]) -> Vec<u8> {
    let cell = make_cell(owner, index, data);
    let mut page = vec![0; PAGE_BYTES];
    encode_overflow_page(header(index), owner, index, &cell, &mut page).unwrap();
    page
}

#[test]
fn overflow_head_literal_and_limits() {
    let data = payload(2049);
    let expected = bytes(
        "0200000001000000030000000000000004000000000000000108000001000000\
         22f0a97231b1c2f1c8e4390f637461035e336295817ba70929f902e11b5e9809",
    );
    let mut encoded = [0; OVERFLOW_HEAD_BYTES];
    head(&data).encode(&mut encoded).unwrap();
    assert_eq!(encoded.as_slice(), expected);
    assert_eq!(OverflowHead::decode(&expected).unwrap(), head(&data));
    for (total, count) in [(2049, 1), (16092, 1), (16093, 2), (131072, 9)] {
        assert_eq!(
            OverflowHead::new(address(0), total, [0; 32])
                .unwrap()
                .count(),
            count
        );
    }
    for total in [0, 2048, 131073, usize::MAX] {
        assert_eq!(
            OverflowHead::new(address(0), total, [0; 32]),
            Err(OverflowError::InvalidInput)
        );
    }
    for len in 0..OVERFLOW_HEAD_BYTES {
        assert!(OverflowHead::decode(&expected[..len]).is_err());
    }
    let mut long = expected.clone();
    long.push(0);
    assert!(OverflowHead::decode(&long).is_err());
    for (offset, size, value) in [(0, 4, 0), (4, 4, 0), (16, 8, 0), (28, 2, 0), (30, 2, 1)] {
        let mut invalid = expected.clone();
        invalid[offset..offset + size].fill(value);
        assert!(OverflowHead::decode(&invalid).is_err());
    }
}

#[test]
fn overflow_page_literal() {
    let data = payload(2049);
    let owner = head(&data);
    let page = make_page(owner, 0, &data);
    let cell_header = bytes(
        "584f56310100000001000000010800000108000000000000\
         020000000100000003000000000000000400000000000000\
         000000000000000000000000000000000000000000000000\
         22f0a97231b1c2f1c8e4390f637461035e336295817ba70929f902e11b5e9809\
         000000000000000000000000000000000000000000000000",
    );
    assert_eq!(cell_header.len(), OVERFLOW_HEADER_BYTES);
    let cell = make_cell(owner, 0, &data);
    assert_eq!(&cell[..OVERFLOW_HEADER_BYTES], cell_header);
    assert_eq!(&cell[OVERFLOW_HEADER_BYTES..], data);
    assert_eq!(
        &page[DIGEST_OFFSET..],
        bytes("1f97d1d77b1d98246096de2d96fc2675fe08524a5d377fae0534a26538c02a3b")
    );
    assert_eq!(
        content_digest(&page).as_slice(),
        bytes("cd8499269433bc243136e100edb13f7328d0563d2a74f1098f23ec1cfc4634d5")
    );
    let view = decode_overflow_page(&page, header(0).identity, owner, 0).unwrap();
    assert_eq!(view.payload(), data);
    assert_eq!(view.next(), None);
}

fn verify_chain(data: &[u8], digest: [u8; 32]) -> usize {
    let owner = OverflowHead::new(address(0), data.len(), digest).unwrap();
    let mut verifier = OverflowChainVerifier::new(header(0).identity, owner).unwrap();
    for i in 0..owner.count() {
        let page = make_page(owner, i, data);
        let view = decode_overflow_page(&page, header(i).identity, owner, i).unwrap();
        assert_eq!(view.index(), i);
        assert_eq!(view.head(), owner);
        assert_eq!(view.header(), header(i));
        assert_eq!(
            view.payload(),
            &data[i * MAX_OVERFLOW_PAYLOAD..((i + 1) * MAX_OVERFLOW_PAYLOAD).min(data.len())]
        );
        assert_eq!(view.next(), (i + 1 < owner.count()).then(|| address(i + 1)));
        verifier.push(&view).unwrap();
    }
    verifier.finish().unwrap();
    owner.count()
}

#[test]
fn overflow_chain_fixed_boundaries() {
    let mut count = 0;
    for total in [2049, 16092, 16093, 32184, 32185, 131072] {
        let data = payload(total);
        count += verify_chain(&data, content_digest(&data));
    }
    assert_eq!(count, 18);
    assert_eq!((MAX_OVERFLOW_PAYLOAD, MAX_OVERFLOW_CHUNKS), (16092, 9));
}

#[test]
fn overflow_rejects_checksummed_damage() {
    let data = payload(32185);
    let owner = head(&data);
    let mut rejected = 0;
    for i in 0..3 {
        let original = make_cell(owner, i, &data);
        for offset in [0, 4, 6, 8, 10, 12, 16, 20, 32, 72, 104] {
            let mut cell = original.clone();
            cell[offset] ^= 0x40;
            let page = raw_page(header(i), &cell);
            assert!(decode_overflow_page(&page, header(i).identity, owner, i).is_err());
            rejected += 1;
        }
        let page = raw_page(header(i), &original[..original.len() - 1]);
        assert!(decode_overflow_page(&page, header(i).identity, owner, i).is_err());
        rejected += 1;
    }
    assert_eq!(rejected, 36);
    let mut last = make_cell(owner, 2, &data);
    last.push(0);
    let page = raw_page(header(2), &last);
    assert!(decode_overflow_page(&page, header(2).identity, owner, 2).is_err());
    let last = make_cell(owner, 2, &data);
    for cells in [vec![], vec![last.as_slice(), last.as_slice()]] {
        let mut page = vec![0; PAGE_BYTES];
        encode_page(header(2), &cells, &mut page).unwrap();
        assert!(decode_overflow_page(&page, header(2).identity, owner, 2).is_err());
    }
}

#[test]
fn overflow_rejects_links_and_identity() {
    let data = payload(32185);
    let owner = head(&data);
    let first = make_cell(owner, 0, &data);
    let page = raw_page(header(1), &first);
    assert!(decode_overflow_page(&page, header(1).identity, owner, 0).is_err());
    let middle = make_cell(owner, 1, &data);
    let page = raw_page(header(0), &middle);
    assert!(decode_overflow_page(&page, header(0).identity, owner, 1).is_err());
    for next in [address(0), address(1)] {
        for incarnation in [4, 5] {
            let mut cell = middle.clone();
            PageRef {
                incarnation: Generation::new(incarnation).unwrap(),
                ..next
            }
            .encode(&mut cell[48..72])
            .unwrap();
            let page = raw_page(header(1), &cell);
            assert!(decode_overflow_page(&page, header(1).identity, owner, 1).is_err());
        }
    }
    for i in [1, 2] {
        let mut cell = make_cell(owner, i, &data);
        if i == 1 {
            cell[48..72].fill(0);
        } else {
            address(3).encode(&mut cell[48..72]).unwrap();
        }
        let page = raw_page(header(i), &cell);
        assert!(decode_overflow_page(&page, header(i).identity, owner, i).is_err());
    }
    let mut headers = [header(0); 7];
    headers[0].identity.database = DatabaseId::new([12; 16]).unwrap();
    headers[1].identity.page.file = FileId::new(0x0000_0001_0000_0003).unwrap();
    headers[2].identity.page.page_no += 1;
    headers[3].identity.page.incarnation = Generation::new(5).unwrap();
    headers[4].identity.owner = ObjectId::new(6).unwrap();
    headers[5].identity.owner_generation = Generation::new(7).unwrap();
    headers[6].kind = PageKind::Undo;
    for changed in headers {
        let page = raw_page(changed, &first);
        assert!(decode_overflow_page(&page, header(0).identity, owner, 0).is_err());
    }
}

#[test]
fn overflow_verifier_requires_complete_order_and_digest() {
    let data = payload(32185);
    let owner = head(&data);
    assert_eq!(
        OverflowChainVerifier::new(header(0).identity, owner)
            .unwrap()
            .finish(),
        Err(OverflowError::Incomplete)
    );
    let pages: Vec<_> = (0..3).map(|i| make_page(owner, i, &data)).collect();
    let views: Vec<_> = pages
        .iter()
        .enumerate()
        .map(|(i, page)| decode_overflow_page(page, header(i).identity, owner, i).unwrap())
        .collect();
    let mut verifier = OverflowChainVerifier::new(header(0).identity, owner).unwrap();
    assert_eq!(verifier.finish(), Err(OverflowError::Incomplete));
    assert_eq!(verifier.push(&views[1]), Err(OverflowError::OrderMismatch));
    verifier.push(&views[0]).unwrap();
    assert_eq!(verifier.push(&views[0]), Err(OverflowError::OrderMismatch));
    let mut foreign_header = header(1);
    foreign_header.identity.owner = ObjectId::new(8).unwrap();
    let foreign_page = raw_page(foreign_header, &make_cell(owner, 1, &data));
    let foreign_view =
        decode_overflow_page(&foreign_page, foreign_header.identity, owner, 1).unwrap();
    assert_eq!(
        verifier.push(&foreign_view),
        Err(OverflowError::OrderMismatch)
    );
    let other = OverflowHead::new(address(0), data.len(), [9; 32]).unwrap();
    let other_page = make_page(other, 1, &data);
    let other_view = decode_overflow_page(&other_page, header(1).identity, other, 1).unwrap();
    assert_eq!(verifier.push(&other_view), Err(OverflowError::HeadMismatch));
    verifier.push(&views[1]).unwrap();
    let mut last = make_cell(owner, 2, &data);
    last[OVERFLOW_HEADER_BYTES] ^= 1;
    let bad_page = raw_page(header(2), &last);
    let bad_view = decode_overflow_page(&bad_page, header(2).identity, owner, 2).unwrap();
    assert_eq!(verifier.push(&bad_view), Err(OverflowError::Corrupt));
    verifier.push(&views[2]).unwrap();
    assert_eq!(verifier.push(&views[2]), Err(OverflowError::InvalidInput));
    verifier.finish().unwrap();
    // Each view has a valid local index/link/header. Only the bounded visited
    // address set catches reusing page 4 for the final chunk, even after reuse.
    let data = payload(48277);
    let owner = head(&data);
    assert_eq!(owner.count(), 4);
    for generation in [4, 5] {
        let repeated = PageRef {
            incarnation: Generation::new(generation).unwrap(),
            ..address(1)
        };
        let current = [address(0), address(1), address(2), repeated];
        let mut verifier = OverflowChainVerifier::new(header(0).identity, owner).unwrap();
        for i in 0..4 {
            let mut h = header(i);
            h.identity.page = current[i];
            let part =
                &data[i * MAX_OVERFLOW_PAYLOAD..((i + 1) * MAX_OVERFLOW_PAYLOAD).min(data.len())];
            let mut cell = vec![0; OVERFLOW_HEADER_BYTES + part.len()];
            encode_overflow_cell(
                owner,
                i,
                current[i],
                current.get(i + 1).copied(),
                part,
                &mut cell,
            )
            .unwrap();
            let page = raw_page(h, &cell);
            let view = decode_overflow_page(&page, h.identity, owner, i).unwrap();
            if i < 3 {
                verifier.push(&view).unwrap();
            } else {
                assert_eq!(verifier.push(&view), Err(OverflowError::OrderMismatch));
                assert_eq!(verifier.finish(), Err(OverflowError::Incomplete));
            }
        }
    }
}

#[test]
fn overflow_streaming_profiles() {
    // Independent hashlib outputs from the contract formulas, before codec execution.
    let expected = [
        "e262cd6233b197a742f18bedb4ae068613ea8e6e7bf03aecb507c945de14fac1",
        "549bbf7ab90ff1f94757be62486f19d48cb134eae398bb39f05e773d1d7f84a4",
        "e7fc3d82a80bc0acb67fe9e30a3d22da685c65fd377aed6daaec1869a8d946d4",
        "2fb4d169026998a0ed634efa8175b767df39f7a7fe147351c85bcb817760bcc8",
        "56078cef952e69042f9a0aaef29fadf60bbb1247c411124c9049d5dfd8ac1b88",
        "9a5ae5bc581039d915308e3f05b21ec46e32575faaaabe2a8e235b7e514dde45",
        "9d288c819dedcf9017ef364c128e4cf24b30c24d327ec623a7cc094350674583",
        "e58994404f4e99fa079b1b5bc0a7a8d295ecd5f7c07f063b2eb1d39ba2b6c549",
        "178ff62da90d002d44c595617530df6f7732d6a58c92f3f6bc28e3400efd9c1b",
        "598f3ee5a665b43b9ae5c609d7795b2dcc5f83d9f81220aebfedf4529a8ed3cb",
        "0adf22755f4097657910bf3247d23d4a31107c7d3391067bc0f9a48fa227a3f9",
        "d9eeaa3cef22d2fdca1b74f19023c60260352999b53fdc8631cefcbac8f49329",
        "96efe63012567dd833b326fd655331dab265cc87a4cd6d54d27db0076274226f",
        "800fe3d455f9b4b6cafbbfaaa3b689e3dff31f529639ef7f577ce4b36043226c",
        "368e9a4620f55e0b0ba0753d513136e5cf9c050da4b10b3a1e993ab3ba598479",
        "654e42bc8e61c822100533f44439084531e86002921570e35d9de256d34c0d6f",
    ];
    let mut chunks = 0;
    for seed in 1..=16 {
        let total = 2049 + seed * 7919 % 129024;
        let data: Vec<_> = (0..total).map(|i| (i * 31 + seed * 7) as u8).collect();
        let digest: [u8; 32] = bytes(expected[seed - 1]).try_into().unwrap();
        chunks += verify_chain(&data, digest);
    }
    assert_eq!(chunks, 80);
}

#[test]
fn overflow_encoders_preserve_outputs() {
    let data = payload(2049);
    let owner = head(&data);
    for len in [0, OVERFLOW_HEAD_BYTES - 1, OVERFLOW_HEAD_BYTES + 1] {
        let mut out = vec![0xa5; len];
        assert!(owner.encode(&mut out).is_err());
        assert_eq!(out, vec![0xa5; len]);
    }
    for (index, current, next, len) in [
        (1, address(0), None, data.len()),
        (0, address(1), None, data.len()),
        (0, address(0), Some(address(1)), data.len()),
        (0, address(0), None, data.len() - 1),
    ] {
        let mut out = vec![0xa5; OVERFLOW_HEADER_BYTES + data.len() + 4];
        assert!(encode_overflow_cell(owner, index, current, next, &data[..len], &mut out).is_err());
        assert!(out.iter().all(|b| *b == 0xa5));
    }
    let mut small = vec![0xa5; OVERFLOW_HEADER_BYTES + data.len() - 1];
    assert!(encode_overflow_cell(owner, 0, address(0), None, &data, &mut small).is_err());
    assert!(small.iter().all(|b| *b == 0xa5));
    let mut out = vec![0xa5; OVERFLOW_HEADER_BYTES + data.len() + 4];
    let used = encode_overflow_cell(owner, 0, address(0), None, &data, &mut out).unwrap();
    assert_eq!(used, OVERFLOW_HEADER_BYTES + data.len());
    assert_eq!(&out[used..], &[0xa5; 4]);
    let cell = make_cell(owner, 0, &data);
    let mut invalid_cell = cell.clone();
    invalid_cell[104] = 1;
    let mut wrong = header(0);
    wrong.kind = PageKind::Undo;
    for (h, cell, len) in [
        (wrong, cell.as_slice(), PAGE_BYTES),
        (header(0), invalid_cell.as_slice(), PAGE_BYTES),
        (header(0), cell.as_slice(), PAGE_BYTES - 1),
    ] {
        let mut out = vec![0xa5; len];
        assert!(encode_overflow_page(h, owner, 0, cell, &mut out).is_err());
        assert!(out.iter().all(|b| *b == 0xa5));
    }
}
