//! New tag-3 prefix boundaries. Literal bytes follow fixed wire offsets; no
//! product encoder or previous test function supplies the expected result.
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
    bytes[7] = 3;
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
fn selected(id: &str) -> bool {
    let parse = |name: &str| {
        std::env::var(name).ok().map(|text| {
            assert!(!text.is_empty() && text.len() <= 16_384);
            let mut values = std::collections::BTreeSet::new();
            for value in text.split(',') {
                let normal = (1..=43).any(|n| value == format!("P4-RP{n:03}"));
                let extended = (0..244).any(|n| value == format!("P4-RPS{n:03}"));
                assert!(normal || extended, "foreign prefix ID {value}");
                assert!(values.insert(value.to_owned()), "duplicate prefix ID");
            }
            values
        })
    };
    let include = parse("XADB_PLAN4_LIVE_INCLUDE");
    let exclude = parse("XADB_PLAN4_LIVE_EXCLUDE").unwrap_or_default();
    if let Some(values) = &include {
        assert!(values.is_disjoint(&exclude));
    }
    include.as_ref().is_none_or(|values| values.contains(id)) && !exclude.contains(id)
}
fn run(id: &str, count: &mut usize, check: impl FnOnce()) {
    if !selected(id) {
        println!("P4_LIVE_SKIP\t{id}");
        return;
    }
    let identity = ["SOURCE", "ELF", "INPUT"]
        .map(|key| {
            let value = std::env::var(format!("XADB_PLAN4_LIVE_{key}_SHA256")).unwrap();
            assert!(value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()));
            value
        })
        .join("\t");
    println!("P4_LIVE_CONTEXT\t{id}\t{identity}");
    println!("P4_LIVE_BEGIN\t{id}");
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)) {
        Ok(()) => println!("P4_LIVE_PASS\t{id}"),
        Err(e) => {
            println!("P4_LIVE_FAIL\t{id}");
            std::panic::resume_unwind(e);
        }
    }
    *count += 1;
}
fn finish(count: usize) {
    assert!(count > 0, "empty prefix selection");
    if let Ok(include) = std::env::var("XADB_PLAN4_LIVE_INCLUDE") {
        assert_eq!(
            count,
            include.split(',').count(),
            "selector did not reach every requested case"
        );
    }
}
fn incomplete(bytes: &[u8]) {
    assert!(matches!(
        scan_retention_prefix(bytes, expected(), TailPolicy::ActiveUnrequiredEof),
        Ok(SemanticFrameScan::Incomplete)
    ));
}
fn rejection(bytes: &[u8], error: WalError) {
    for policy in [
        TailPolicy::ActiveUnrequiredEof,
        TailPolicy::ForbidIncomplete,
    ] {
        assert_eq!(
            scan_retention_prefix(bytes, expected(), policy).err(),
            Some(error)
        );
    }
}

#[test]
fn retention_prefix_observes_only_envelope() {
    let mut count = 0;
    run("P4-RP001", &mut count, || {
        for policy in [
            TailPolicy::ForbidIncomplete,
            TailPolicy::ActiveUnrequiredEof,
        ] {
            assert!(matches!(
                scan_retention_prefix(&[], expected(), policy),
                Ok(SemanticFrameScan::End)
            ));
        }
    });
    run("P4-RP002", &mut count, || {
        let bytes = literal();
        let SemanticFrameScan::Complete { envelope, consumed } =
            scan_retention_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete).unwrap()
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
    run("P4-RP003", &mut count, || {
        let mut bytes = literal();
        bytes.extend_from_slice(b"bad next header");
        assert!(matches!(
            scan_retention_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete),
            Ok(SemanticFrameScan::Complete { consumed: 244, .. })
        ));
    });
    run("P4-RP004", &mut count, || {
        let mut bytes = literal();
        bytes[20..22].copy_from_slice(&u16::MAX.to_le_bytes());
        rehash(&mut bytes);
        let SemanticFrameScan::Complete { envelope, .. } =
            scan_retention_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete).unwrap()
        else {
            panic!("envelope does not validate semantic count");
        };
        assert_eq!(envelope.record_count, u16::MAX);
    });
    run("P4-RP005", &mut count, || {
        assert_eq!(
            retention_frame_length(&literal()[..160], expected()),
            Ok(244)
        );
    });
    finish(count);
}

#[test]
fn retention_prefix_incomplete_boundaries() {
    let mut count = 0;
    for (id, cut) in [
        ("P4-RP006", 1),
        ("P4-RP007", 5),
        ("P4-RP008", 159),
        ("P4-RP009", 160),
        ("P4-RP010", 211),
        ("P4-RP011", 243),
    ] {
        run(id, &mut count, || incomplete(&literal()[..cut]));
    }
    for (id, cut) in [("P4-RP012", 161), ("P4-RP013", 243)] {
        run(id, &mut count, || {
            assert_eq!(
                scan_retention_prefix(&literal()[..cut], expected(), TailPolicy::ForbidIncomplete)
                    .err(),
                Some(WalError::Truncated)
            );
        });
    }
    finish(count);
}

#[test]
fn retention_prefix_visible_rejections() {
    let mut count = 0;
    let cases = [
        ("P4-RP014", 0, 1, WalError::Corrupt),
        ("P4-RP015", 4, 5, WalError::UnsupportedFormat),
        ("P4-RP016", 7, 8, WalError::UnsupportedFormat),
        ("P4-RP017", 8, 9, WalError::Corrupt),
        ("P4-RP018", 10, 11, WalError::Corrupt),
        ("P4-RP021", 24, 25, WalError::IdentityMismatch),
        ("P4-RP022", 40, 41, WalError::IdentityMismatch),
        ("P4-RP023", 48, 49, WalError::IdentityMismatch),
        ("P4-RP024", 64, 65, WalError::OrderMismatch),
        ("P4-RP026", 72, 73, WalError::Corrupt),
        ("P4-RP027", 112, 113, WalError::Corrupt),
        ("P4-RP028", 128, 129, WalError::Corrupt),
        ("P4-RP031", 120, 121, WalError::Corrupt),
    ];
    for (id, offset, cut, error) in cases {
        run(id, &mut count, || {
            let mut bytes = literal();
            bytes[offset] ^= 0x80;
            rejection(&bytes[..cut], error);
        });
    }
    run("P4-RP019", &mut count, || {
        let mut bytes = literal();
        bytes[12..16].copy_from_slice(&191u32.to_le_bytes());
        rejection(&bytes[..16], WalError::Corrupt);
    });
    run("P4-RP020", &mut count, || {
        let mut bytes = literal();
        bytes[16..20].copy_from_slice(&51u32.to_le_bytes());
        rejection(&bytes[..20], WalError::Corrupt);
    });
    run("P4-RP025", &mut count, || {
        let mut bytes = literal();
        bytes[56..64].fill(0);
        rejection(&bytes[..64], WalError::OrderMismatch);
    });
    run("P4-RP029", &mut count, || {
        let mut bytes = literal();
        bytes[6] = 255;
        rejection(&bytes[..7], WalError::UnsupportedFormat);
    });
    run("P4-RP030", &mut count, || {
        let mut bytes = literal();
        bytes[12..16].copy_from_slice(&(MAX_FRAME_BYTES as u32 + 1).to_le_bytes());
        rejection(&bytes[..16], WalError::Corrupt);
    });
    finish(count);
}

#[test]
fn retention_prefix_complete_checksum_and_order() {
    let mut count = 0;
    run("P4-RP032", &mut count, || {
        let mut bytes = literal();
        bytes[56..64].copy_from_slice(&3u64.to_le_bytes());
        rehash(&mut bytes);
        // Contiguous LSN issuance belongs to the producer-language consumer.
        assert!(matches!(
            scan_retention_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete),
            Ok(SemanticFrameScan::Complete { .. })
        ));
    });
    for (id, offset) in [("P4-RP033", 160), ("P4-RP034", 243)] {
        run(id, &mut count, || {
            let mut bytes = literal();
            bytes[offset] ^= 1;
            rejection(&bytes, WalError::Corrupt);
        });
    }
    run("P4-RP035", &mut count, || {
        let mut bytes = literal();
        bytes[56..64].copy_from_slice(&3u64.to_le_bytes());
        let digest = content_digest(&bytes[..128]);
        bytes[128..160].copy_from_slice(&digest);
        rejection(&bytes, WalError::Corrupt);
    });
    finish(count);
}

#[test]
fn retention_prefix_preserves_input_and_borrow() {
    let mut count = 0;
    run("P4-RP036", &mut count, || {
        let bytes = literal();
        let before = bytes.clone();
        let SemanticFrameScan::Complete { envelope, .. } =
            scan_retention_prefix(&bytes, expected(), TailPolicy::ForbidIncomplete).unwrap()
        else {
            panic!("complete required");
        };
        assert_eq!(envelope.payload.as_ptr(), bytes[160..].as_ptr());
        assert_eq!(bytes, before);
    });
    run("P4-RP037", &mut count, || {
        let mut bytes = literal();
        bytes[200] ^= 1;
        let before = bytes.clone();
        rejection(&bytes, WalError::Corrupt);
        assert_eq!(bytes, before);
    });
    finish(count);
}

#[test]
fn retention_prefix_header_length_bounds() {
    let mut count = 0;
    for (id, length) in [
        ("P4-RP038", MAX_FRAME_BYTES),
        ("P4-RP039", FRAME_OVERHEAD_BYTES),
    ] {
        run(id, &mut count, || {
            let mut bytes = literal();
            bytes[12..16].copy_from_slice(&(length as u32).to_le_bytes());
            bytes[16..20].copy_from_slice(&((length - FRAME_OVERHEAD_BYTES) as u32).to_le_bytes());
            rehash(&mut bytes);
            assert_eq!(
                retention_frame_length(&bytes[..160], expected()),
                Ok(length)
            );
        });
    }
    run("P4-RP040", &mut count, || {
        assert_eq!(
            retention_frame_length(&literal()[..159], expected()),
            Err(WalError::Truncated)
        );
    });
    run("P4-RP041", &mut count, || {
        let mut invalid = expected();
        invalid.stream.timeline = 0;
        assert_eq!(
            scan_retention_prefix(&[], invalid, TailPolicy::ActiveUnrequiredEof).err(),
            Some(WalError::InvalidInput)
        );
    });
    run("P4-RP042", &mut count, || {
        let mut invalid = expected();
        invalid.stream.writer_epoch = 0;
        assert_eq!(
            retention_frame_length(&[], invalid),
            Err(WalError::InvalidInput)
        );
    });
    finish(count);
}

#[test]
#[ignore = "S5 full EOF matrix; mandatory aliases have distinct canonical IDs"]
fn retention_prefix_s5_every_eof() {
    let mut count = 0;
    let bytes = literal();
    for cut in 0..244 {
        run(&format!("P4-RPS{cut:03}"), &mut count, || {
            if cut == 0 {
                assert!(matches!(
                    scan_retention_prefix(&[], expected(), TailPolicy::ActiveUnrequiredEof),
                    Ok(SemanticFrameScan::End)
                ));
            } else {
                incomplete(&bytes[..cut]);
            }
        });
    }
    finish(count);
}

#[test]
fn retention_prefix_rejects_other_tags() {
    let mut count = 0;
    run("P4-RP043", &mut count, || {
        for tag in [0, 1, 2, 4, 255] {
            let mut bytes = literal();
            bytes[7] = tag;
            rehash(&mut bytes);
            rejection(&bytes[..8], WalError::UnsupportedFormat);
            assert_eq!(
                retention_frame_length(&bytes[..160], expected()),
                Err(WalError::UnsupportedFormat)
            );
        }
    });
    finish(count);
}
