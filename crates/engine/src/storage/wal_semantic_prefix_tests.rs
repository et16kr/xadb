//! New semantic-prefix cases only; the unchanged legacy scanner is not rerun.
use super::*;

fn expected() -> FrameExpectation {
    FrameExpectation {
        stream: WalStreamIdentity {
            database: DatabaseId::new([0x31; 16]).unwrap(),
            timeline: 1,
            writer_epoch: 1,
        },
        previous_lsn: 1,
    }
}
// Independent wire construction. No production WAL encoder supplies expected
// header fields, length or payload. The shared hash is only a byte primitive.
fn literal() -> Vec<u8> {
    let mut bytes = vec![0; 244];
    bytes[..4].copy_from_slice(b"XWF1");
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[6] = 3;
    bytes[7] = 2;
    bytes[8..10].copy_from_slice(&160u16.to_le_bytes());
    bytes[12..16].copy_from_slice(&244u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&52u32.to_le_bytes());
    bytes[20..22].copy_from_slice(&1u16.to_le_bytes());
    bytes[24..40].fill(0x31);
    bytes[40..48].copy_from_slice(&1u64.to_le_bytes());
    bytes[48..52].copy_from_slice(&1u32.to_le_bytes());
    bytes[56..64].copy_from_slice(&2u64.to_le_bytes());
    bytes[64..72].copy_from_slice(&1u64.to_le_bytes());
    bytes[160..212].fill(0xa5); // Intentionally not a semantic group.
    rehash(&mut bytes);
    bytes
}
fn rehash(bytes: &mut [u8]) {
    let digest = content_digest(&bytes[..128]);
    bytes[128..160].copy_from_slice(&digest);
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
fn run(id: &str, count: &mut usize, check: impl FnOnce()) {
    if let Ok(selection) = std::env::var("XADB_RECOVERY_PREFIX_CASES") {
        let selected: Vec<_> = selection.split(',').collect();
        assert!(!selected.is_empty() && selected.iter().all(|s| !s.is_empty()));
        let unique: std::collections::BTreeSet<_> = selected.iter().collect();
        assert_eq!(unique.len(), selected.len(), "duplicate case selection");
        if !selected.contains(&id) {
            return;
        }
    }
    println!("RECOVERY_PREFIX_CASE_BEGIN\t{id}");
    check();
    println!("RECOVERY_PREFIX_CASE\t{id}\tPASS");
    *count += 1;
}
fn finish(count: usize) {
    assert!(count > 0, "empty case selection");
    println!("RECOVERY_PREFIX_COMPLETE count={count}");
}
fn incomplete(bytes: &[u8]) {
    assert!(matches!(
        scan_semantic_prefix(bytes, expected(), TailPolicy::ActiveUnrequiredEof),
        Ok(SemanticFrameScan::Incomplete)
    ));
}
fn rejection(bytes: &[u8], error: WalError) {
    for policy in [
        TailPolicy::ActiveUnrequiredEof,
        TailPolicy::ForbidIncomplete,
    ] {
        assert_eq!(
            scan_semantic_prefix(bytes, expected(), policy).err(),
            Some(error)
        );
    }
}

#[test]
fn semantic_prefix_observes_only_envelope() {
    let mut count = 0;
    run("WP001", &mut count, || {
        for policy in [
            TailPolicy::ForbidIncomplete,
            TailPolicy::ActiveUnrequiredEof,
        ] {
            assert!(matches!(
                scan_semantic_prefix(&[], expected(), policy),
                Ok(SemanticFrameScan::End)
            ));
        }
    });
    run("WP002", &mut count, || {
        let bytes = literal();
        let SemanticFrameScan::Complete { envelope, consumed } =
            scan_semantic_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete).unwrap()
        else {
            panic!("complete envelope required");
        };
        assert_eq!(consumed, 244);
        assert_eq!(envelope.header.lsn, 2);
        assert_eq!(envelope.header.previous_lsn, 1);
        assert_eq!(envelope.header.kind, FrameKind::System);
        assert_eq!(envelope.payload, &[0xa5; 52]);
        assert_eq!(envelope.record_count, 1);
    });
    run("WP003", &mut count, || {
        let mut bytes = literal();
        bytes.extend_from_slice(b"bad next header");
        assert!(matches!(
            scan_semantic_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete),
            Ok(SemanticFrameScan::Complete { consumed: 244, .. })
        ));
    });
    run("WP004", &mut count, || {
        let mut bytes = literal();
        bytes[20..22].copy_from_slice(&u16::MAX.to_le_bytes());
        rehash(&mut bytes);
        let SemanticFrameScan::Complete { envelope, .. } =
            scan_semantic_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete).unwrap()
        else {
            panic!("envelope does not validate semantic count");
        };
        assert_eq!(envelope.record_count, u16::MAX);
    });
    run("WP005", &mut count, || {
        assert_eq!(
            semantic_frame_length(&literal()[..160], expected()),
            Ok(244)
        );
    });
    finish(count);
}

#[test]
fn semantic_prefix_incomplete_boundaries() {
    let mut count = 0;
    for (id, cut) in [
        ("WP006", 1),
        ("WP007", 5),
        ("WP008", 159),
        ("WP009", 160),
        ("WP010", 211),
        ("WP011", 243),
    ] {
        run(id, &mut count, || incomplete(&literal()[..cut]));
    }
    for (id, cut) in [("WP012", 161), ("WP013", 243)] {
        run(id, &mut count, || {
            assert_eq!(
                scan_semantic_prefix(&literal()[..cut], expected(), TailPolicy::ForbidIncomplete)
                    .err(),
                Some(WalError::Truncated)
            );
        });
    }
    finish(count);
}

#[test]
fn semantic_prefix_visible_rejections() {
    let mut count = 0;
    let cases = [
        ("WP014", 0, 1, WalError::Corrupt),
        ("WP015", 4, 5, WalError::UnsupportedFormat),
        ("WP016", 7, 8, WalError::UnsupportedFormat),
        ("WP017", 8, 9, WalError::Corrupt),
        ("WP018", 10, 11, WalError::Corrupt),
        ("WP021", 24, 25, WalError::IdentityMismatch),
        ("WP022", 40, 41, WalError::IdentityMismatch),
        ("WP023", 48, 49, WalError::IdentityMismatch),
        ("WP024", 64, 65, WalError::OrderMismatch),
        ("WP026", 72, 73, WalError::Corrupt),
        ("WP027", 112, 113, WalError::Corrupt),
        ("WP028", 128, 129, WalError::Corrupt),
        ("WP031", 120, 121, WalError::Corrupt),
    ];
    for (id, offset, cut, error) in cases {
        run(id, &mut count, || {
            let mut bytes = literal();
            bytes[offset] ^= 0x80;
            rejection(&bytes[..cut], error);
        });
    }
    run("WP019", &mut count, || {
        let mut bytes = literal();
        bytes[12..16].copy_from_slice(&191u32.to_le_bytes());
        rejection(&bytes[..16], WalError::Corrupt);
    });
    run("WP020", &mut count, || {
        let mut bytes = literal();
        bytes[16..20].copy_from_slice(&51u32.to_le_bytes());
        rejection(&bytes[..20], WalError::Corrupt);
    });
    run("WP025", &mut count, || {
        let mut bytes = literal();
        bytes[56..64].fill(0);
        rejection(&bytes[..64], WalError::OrderMismatch);
    });
    run("WP029", &mut count, || {
        let mut bytes = literal();
        bytes[6] = 255;
        rejection(&bytes[..7], WalError::UnsupportedFormat);
    });
    run("WP030", &mut count, || {
        let mut bytes = literal();
        bytes[12..16].copy_from_slice(&(MAX_FRAME_BYTES as u32 + 1).to_le_bytes());
        rejection(&bytes[..16], WalError::Corrupt);
    });
    finish(count);
}

#[test]
fn semantic_prefix_complete_checksum_and_order() {
    let mut count = 0;
    run("WP032", &mut count, || {
        let mut bytes = literal();
        bytes[56..64].copy_from_slice(&3u64.to_le_bytes());
        rehash(&mut bytes);
        // Contiguous LSN issuance belongs to the producer-language consumer.
        assert!(matches!(
            scan_semantic_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete),
            Ok(SemanticFrameScan::Complete { .. })
        ));
    });
    for (id, offset) in [("WP033", 160), ("WP034", 243)] {
        run(id, &mut count, || {
            let mut bytes = literal();
            bytes[offset] ^= 1;
            rejection(&bytes, WalError::Corrupt);
        });
    }
    run("WP035", &mut count, || {
        let mut bytes = literal();
        bytes[56..64].copy_from_slice(&3u64.to_le_bytes());
        let digest = content_digest(&bytes[..128]);
        bytes[128..160].copy_from_slice(&digest);
        rejection(&bytes, WalError::Corrupt);
    });
    finish(count);
}

#[test]
fn semantic_prefix_preserves_input_and_borrow() {
    let mut count = 0;
    run("WP036", &mut count, || {
        let bytes = literal();
        let before = bytes.clone();
        let SemanticFrameScan::Complete { envelope, .. } =
            scan_semantic_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete).unwrap()
        else {
            panic!("complete required");
        };
        assert_eq!(envelope.payload.as_ptr(), bytes[160..].as_ptr());
        assert_eq!(bytes, before);
    });
    run("WP037", &mut count, || {
        let mut bytes = literal();
        bytes[200] ^= 1;
        let before = bytes.clone();
        rejection(&bytes, WalError::Corrupt);
        assert_eq!(bytes, before);
    });
    finish(count);
}

#[test]
fn semantic_prefix_header_length_bounds() {
    let mut count = 0;
    for (id, length) in [("WP038", MAX_FRAME_BYTES), ("WP039", FRAME_OVERHEAD_BYTES)] {
        run(id, &mut count, || {
            let mut bytes = literal();
            bytes[12..16].copy_from_slice(&(length as u32).to_le_bytes());
            bytes[16..20].copy_from_slice(&((length - FRAME_OVERHEAD_BYTES) as u32).to_le_bytes());
            rehash(&mut bytes);
            assert_eq!(semantic_frame_length(&bytes[..160], expected()), Ok(length));
        });
    }
    run("WP040", &mut count, || {
        assert_eq!(
            semantic_frame_length(&literal()[..159], expected()),
            Err(WalError::Truncated)
        );
    });
    run("WP041", &mut count, || {
        let mut invalid = expected();
        invalid.stream.timeline = 0;
        assert_eq!(
            scan_semantic_prefix(&[], invalid, TailPolicy::ActiveUnrequiredEof).err(),
            Some(WalError::InvalidInput)
        );
    });
    run("WP042", &mut count, || {
        let mut invalid = expected();
        invalid.stream.writer_epoch = 0;
        assert_eq!(
            semantic_frame_length(&[], invalid),
            Err(WalError::InvalidInput)
        );
    });
    finish(count);
}

#[test]
#[ignore = "S5 full EOF matrix; mandatory aliases have distinct canonical IDs"]
fn semantic_prefix_s5_every_eof() {
    let mut count = 0;
    let bytes = literal();
    for cut in 0..244 {
        run(&format!("WPS{cut:03}"), &mut count, || {
            if cut == 0 {
                assert!(matches!(
                    scan_semantic_prefix(&[], expected(), TailPolicy::ActiveUnrequiredEof),
                    Ok(SemanticFrameScan::End)
                ));
            } else {
                incomplete(&bytes[..cut]);
            }
        });
    }
    finish(count);
}
