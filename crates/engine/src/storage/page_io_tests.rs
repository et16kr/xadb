use super::*;
use crate::api::DatabaseId;
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};
use std::sync::Arc;

use super::test_support::*;

fn attempt(writer: &mut Writer, id: PageIdentity, kind: PageKind, level: u8) -> Result<Vec<u8>> {
    let mut root = [0; PAGE_BYTES];
    let mut scratch = [0; PAGE_BYTES];
    let mut output = [0; PAGE_BYTES];
    let mut reader = PageFileReader::open(writer, Some(ns()), &mut root, &mut scratch)?;
    assert_eq!(reader.selector_generation(), 29);
    let page = reader.read_page(id, kind, level, &mut output)?;
    assert_eq!(page.len(), 3);
    Ok(output.to_vec())
}
fn clean(writer: &Writer) {
    let s = writer.0.lock().unwrap();
    assert_eq!(s.handles, 0);
    assert!(s.peak <= 2);
    assert_eq!(s.mutations, 0);
}

#[test]
fn selected_map_reads_independent_page_and_all_declared_chunks() {
    for count in [0usize, 1, 64, 65, 8192] {
        let mut writer = fixture(count, false);
        let result = attempt(&mut writer, expected(), PageKind::RowLeaf, 0);
        if count == 0 {
            assert!(matches!(result, Err(PageIoError::MissingMapping)));
        } else {
            assert_eq!(result.unwrap(), GOLDEN);
        }
        clean(&writer);
        let s = writer.0.lock().unwrap();
        let chunks = count.div_ceil(64);
        let expected_reads = 3 + chunks + if count == 0 { 0 } else { 2 };
        assert_eq!(s.reads.len(), expected_reads);
        assert_eq!(
            (s.reads[0].name.as_str(), s.reads[0].bytes),
            ("d3-profile", 4096)
        );
        assert_eq!(
            (s.reads[1].name.as_str(), s.reads[1].bytes),
            ("FILEMAP", 4096)
        );
        assert_eq!(
            (s.reads[2].offset, s.reads[2].bytes),
            (0, 224 + 96 * chunks)
        );
        for index in 0..chunks {
            let read = &s.reads[3 + index];
            assert_eq!(read.name, MAP);
            assert_eq!(
                (read.offset, read.bytes),
                (
                    (1 + index) as u64 * 16_384,
                    160 + 96 * (count - index * 64).min(64)
                )
            );
        }
        if count > 0 {
            let lookup = &s.reads[3 + chunks];
            assert_eq!(
                (lookup.offset, lookup.bytes),
                (16_384, 160 + 96 * count.min(64))
            );
        }
    }
}

#[test]
fn selected_map_rejects_late_damage_and_never_scans_fallback() {
    // Independently build a real bank1, including its transitive chunk hashes.
    // A copied bank0 at the bank1 pathname would itself be an invalid fallback.
    fn install_bank_one(writer: &Writer) -> Vec<u8> {
        let mut s = writer.0.lock().unwrap();
        let mut container = s.files[MAP].clone();
        for (index, length) in [6304, 256].into_iter().enumerate() {
            let offset = (index + 1) * PAGE_BYTES;
            let chunk = &mut container[offset..offset + length];
            chunk[7] = 1;
            seal(chunk);
            let digest = content_digest(chunk);
            let reference = 192 + index * 96;
            container[reference + 1] = 1;
            container[reference + 32..reference + 64].copy_from_slice(&digest);
        }
        container[7] = 1;
        seal(&mut container[..416]);
        let mut selector = s.files["FILEMAP"].clone();
        selector[81] = 1;
        selector[112..144].copy_from_slice(&content_digest(&container[..416]));
        seal(&mut selector);
        s.files.insert("d3-filemap-1".into(), container);
        selector
    }
    let mut positive = fixture(65, false);
    let selected_one = install_bank_one(&positive);
    positive
        .0
        .lock()
        .unwrap()
        .files
        .insert("FILEMAP".into(), selected_one);
    assert_eq!(
        attempt(&mut positive, expected(), PageKind::RowLeaf, 0).unwrap(),
        GOLDEN
    );
    clean(&positive);
    for case in 0..8 {
        let mut writer = fixture(65, false);
        install_bank_one(&writer);
        {
            let mut s = writer.0.lock().unwrap();
            match case {
                0 => {
                    s.files.remove("FILEMAP");
                }
                1 => s.files.get_mut("FILEMAP").unwrap()[100] ^= 1,
                2 => {
                    let b = s.files.get_mut("d3-profile").unwrap();
                    b[16..32].fill(0x22);
                    seal(b);
                }
                3 => s.files.get_mut(MAP).unwrap()[128] ^= 1,
                4 => s.files.get_mut(MAP).unwrap()[16_384 + 128] ^= 1,
                5 => s.files.get_mut(MAP).unwrap()[32_768 + 128] ^= 1,
                6 => {
                    s.plan = Some(Plan {
                        phase: 4,
                        action: Action::PrefixEof,
                    })
                }
                7 => {
                    s.files.remove(MAP);
                }
                _ => unreachable!(),
            }
        }
        assert!(
            attempt(&mut writer, expected(), PageKind::RowLeaf, 0).is_err(),
            "{case}"
        );
        clean(&writer);
        let s = writer.0.lock().unwrap();
        assert!(
            s.reads
                .iter()
                .all(|r| r.name != "d3-filemap-1" && r.name != DATA)
        );
        if case == 6 {
            assert!(s.hit);
        }
    }
}

#[test]
fn selected_map_and_data_require_exact_handle_lengths() {
    for file in ["d3-profile", "FILEMAP", MAP, DATA] {
        for delta in [-1i64, 0, 1] {
            let mut writer = fixture(65, false);
            {
                let mut s = writer.0.lock().unwrap();
                let b = s.files.get_mut(file).unwrap();
                b.resize((b.len() as i64 + delta) as usize, 0);
            }
            let result = attempt(&mut writer, expected(), PageKind::RowLeaf, 0);
            if delta == 0 {
                assert_eq!(result.unwrap(), GOLDEN);
            } else {
                assert!(
                    matches!(result, Err(PageIoError::LengthMismatch)),
                    "{file}/{delta}"
                );
            }
            clean(&writer);
        }
    }
}

fn install_page(writer: &Writer, file: &str, page: u64, bytes: &[u8]) {
    let mut s = writer.0.lock().unwrap();
    let data = s
        .files
        .entry(file.into())
        .or_insert_with(|| vec![0; 1 << 20]);
    let offset = page as usize * PAGE_BYTES;
    data[offset..offset + PAGE_BYTES].copy_from_slice(bytes);
}

#[test]
fn page_request_binds_profile_owner_generation_kind_and_bounds() {
    for case in 0..16 {
        let mut writer = fixture(65, false);
        let mut id = expected();
        let mut kind = PageKind::RowLeaf;
        let mut level = 0;
        match case {
            0 => id.database = DatabaseId::new([0x22; 16]).unwrap(),
            1 => {
                id.page.file = FileId::new(0x100000003).unwrap();
                install_page(&writer, "d3-data-0000000100000003", 3, GOLDEN);
            }
            2 => {
                id.page.page_no = 4;
                install_page(&writer, DATA, 4, GOLDEN);
            }
            3 => id.page.incarnation = Generation::new(5).unwrap(),
            4 => id.owner = ObjectId::new(7).unwrap(),
            5 => id.owner_generation = Generation::new(7).unwrap(),
            6 => kind = PageKind::IndexLeaf,
            7 => level = 1,
            8 => id.page.file = FileId::new(0x200000002).unwrap(),
            9 => id.page.page_no = 65,
            10 => id.page.page_no = u64::MAX,
            11 => kind = PageKind::Allocation,
            12 => {
                id.page.page_no = 0;
                id.page.incarnation = Generation::new(1).unwrap();
                id.owner = ObjectId::new(9).unwrap();
                kind = PageKind::Allocation;
            }
            13 => id.page.page_no = 0,
            14 => {
                let mut b = GOLDEN.to_vec();
                b[200] ^= 1;
                install_page(&writer, DATA, 3, &b);
            }
            15 => {
                writer.0.lock().unwrap().plan = Some(Plan {
                    phase: 6,
                    action: Action::PrefixEof,
                })
            }
            _ => unreachable!(),
        }
        let error = attempt(&mut writer, id, kind, level).unwrap_err();
        if case == 0 {
            assert!(
                writer
                    .0
                    .lock()
                    .unwrap()
                    .reads
                    .iter()
                    .all(|read| !read.name.starts_with("d3-data-"))
            );
        }
        match case {
            0 | 12 => assert!(matches!(error, PageIoError::IdentityMismatch)),
            1..=5 => assert!(matches!(
                error,
                PageIoError::Page(PageError::IdentityMismatch)
            )),
            6 => assert!(matches!(error, PageIoError::Page(PageError::KindMismatch))),
            7 | 11 | 13 => assert!(matches!(error, PageIoError::InvalidRequest)),
            8 => assert!(matches!(error, PageIoError::MissingMapping)),
            9 | 10 => assert!(matches!(error, PageIoError::OutOfBounds)),
            14 => assert!(matches!(error, PageIoError::Page(PageError::Corrupt))),
            15 => assert!(matches!(error, PageIoError::Read(_))),
            _ => unreachable!(),
        }
        if matches!(case, 1..=6) {
            assert!(
                writer
                    .0
                    .lock()
                    .unwrap()
                    .reads
                    .iter()
                    .any(|r| r.name.starts_with("d3-data-"))
            );
        }
        clean(&writer);
    }
    for (page, kind, level, incarnation) in [
        (0, PageKind::Allocation, 0, 1),
        (3, PageKind::RowInternal, 1, 4),
        (3, PageKind::RowInternal, 15, 4),
        (63, PageKind::RowLeaf, 0, 4),
    ] {
        let mut writer = fixture(65, false);
        let mut bytes = GOLDEN.to_vec();
        bytes[6] = kind as u8;
        bytes[7] = level;
        put64(&mut bytes, 32, page);
        put64(&mut bytes, 40, incarnation);
        seal(&mut bytes);
        install_page(&writer, DATA, page, &bytes);
        let mut id = expected();
        id.page.page_no = page;
        id.page.incarnation = Generation::new(incarnation).unwrap();
        assert_eq!(attempt(&mut writer, id, kind, level).unwrap(), bytes);
        clean(&writer);
        if kind == PageKind::RowInternal && level == 1 {
            assert!(matches!(
                attempt(&mut writer, id, kind, 2),
                Err(PageIoError::IdentityMismatch)
            ));
            for bad in [0, 16] {
                assert!(matches!(
                    attempt(&mut writer, id, kind, bad),
                    Err(PageIoError::InvalidRequest)
                ));
            }
            clean(&writer);
        }
    }
    let mut retained = fixture(65, true);
    assert_eq!(
        attempt(&mut retained, expected(), PageKind::RowLeaf, 0).unwrap(),
        GOLDEN
    );
    clean(&retained);
}

#[test]
fn every_read_fault_preserves_selection_and_bounded_handles() {
    let actions = [
        Action::BeforeError,
        Action::Short(1),
        Action::Short(17),
        Action::Short(4096),
        Action::Interrupted,
        Action::PrefixEof,
        Action::PrefixError,
        Action::InvalidCount,
    ];
    let (mut succeeded, mut rejected) = (0, 0);
    for phase in 0..7 {
        for action in actions {
            let mut writer = fixture(65, false);
            let state = Arc::clone(&writer.0);
            state.lock().unwrap().plan = Some(Plan { phase, action });
            let mut root = [0; PAGE_BYTES];
            let mut scratch = [0; PAGE_BYTES];
            let mut output = [0; PAGE_BYTES];
            let expected_success = matches!(action, Action::Short(_) | Action::Interrupted);
            {
                let opened = PageFileReader::open(&mut writer, Some(ns()), &mut root, &mut scratch);
                match opened {
                    Err(error) => {
                        assert!(
                            !expected_success && phase < 5,
                            "{phase}/{action:?}: {error:?}"
                        );
                        rejected += 1;
                    }
                    Ok(mut reader) => {
                        let result =
                            reader.read_page(expected(), PageKind::RowLeaf, 0, &mut output);
                        if expected_success {
                            assert_eq!(result.unwrap().len(), 3);
                            assert_eq!(&output, GOLDEN);
                            succeeded += 1;
                        } else {
                            assert!(phase >= 5 && result.is_err(), "{phase}/{action:?}");
                            rejected += 1;
                            state.lock().unwrap().plan = None;
                            assert_eq!(
                                reader
                                    .read_page(expected(), PageKind::RowLeaf, 0, &mut output)
                                    .unwrap()
                                    .len(),
                                3
                            );
                            assert_eq!(&output, GOLDEN);
                            assert_eq!(reader.selector_generation(), 29);
                        }
                    }
                }
            }
            clean(&writer);
            let s = state.lock().unwrap();
            assert!(s.hit, "{phase}/{action:?}");
            if matches!(action, Action::Short(_)) {
                assert!(s.actual_short);
            }
        }
    }
    assert_eq!((succeeded, rejected), (28, 28));
}

/// U2-C4 D1: `open_profile` re-reads the installed profile in the grammar of
/// the caller's bound Profile; `open` keeps the legacy grammar. The Profile3
/// envelope is the literal profile with its published version (4) and base
/// (296) fields changed and resealed; the selector rebinds its digest (48).
#[test]
fn open_profile_binds_installed_grammar() {
    fn relabel(version: u16, base: u64) -> Vec<u8> {
        let mut bytes = PROFILE.to_vec();
        put16(&mut bytes, 4, version);
        put64(&mut bytes, 296, base);
        seal(&mut bytes);
        bytes
    }
    fn install(writer: &Writer, profile: &[u8], bound: &[u8]) {
        let mut s = writer.0.lock().unwrap();
        s.files.insert("d3-profile".into(), profile.to_vec());
        let selector = s.files.get_mut("FILEMAP").unwrap();
        selector[48..80].copy_from_slice(&content_digest(bound));
        seal(selector);
    }
    fn read(writer: &mut Writer, profile: Profile) -> Result<Vec<u8>> {
        let mut root = [0; PAGE_BYTES];
        let mut scratch = [0; PAGE_BYTES];
        let mut output = [0; PAGE_BYTES];
        let mut reader = PageFileReader::open_profile(writer, profile, &mut root, &mut scratch)?;
        assert_eq!(reader.profile(), profile);
        assert_eq!(reader.selector_generation(), 29);
        reader.read_page(expected(), PageKind::RowLeaf, 0, &mut output)?;
        Ok(output.to_vec())
    }
    // A refusal reads only the profile: no selector, root, chunk or data.
    fn refused(writer: &mut Writer, profile: Profile, cause: control::ControlError) {
        let before = writer.0.lock().unwrap().reads.len();
        let result = read(writer, profile);
        assert!(
            matches!(result, Err(PageIoError::Control(c)) if c == cause),
            "{cause:?}"
        );
        let s = writer.0.lock().unwrap();
        assert_eq!(s.reads.len(), before + 1);
        assert_eq!(s.reads[before].name, "d3-profile");
    }
    let profile3 = relabel(3, 2 << 20);
    let profile2 = relabel(2, 2 << 20);
    let p1 = control::decode_bootstrap_profile(PROFILE, Some(ns())).unwrap();
    let p3 = control::row_profile::decode_profile(&profile3, Some(ns())).unwrap();
    assert_eq!((p1.format(), p3.format()), (1, 3));
    let other3 = {
        // Same namespace, Profile3 ID2 (id 128, buffer 168).
        let mut bytes = profile3.clone();
        put32(&mut bytes, 128, 2);
        put64(&mut bytes, 168, 16 << 20);
        seal(&mut bytes);
        control::row_profile::decode_profile(&bytes, Some(ns())).unwrap()
    };

    // Profile1 namespace: both entries read the same page; Profile3 bound refused.
    let mut writer = fixture(65, false);
    assert_eq!(read(&mut writer, p1).unwrap(), GOLDEN);
    assert_eq!(
        attempt(&mut writer, expected(), PageKind::RowLeaf, 0).unwrap(),
        GOLDEN
    );
    refused(&mut writer, p3, control::ControlError::UnsupportedFormat);
    clean(&writer);

    // Profile3 namespace: only the bound Profile3 value opens it.
    let mut writer = fixture(65, false);
    install(&writer, &profile3, &profile3);
    assert_eq!(read(&mut writer, p3).unwrap(), GOLDEN);
    assert!(matches!(
        attempt(&mut writer, expected(), PageKind::RowLeaf, 0),
        Err(PageIoError::Control(
            control::ControlError::UnsupportedFormat
        ))
    ));
    refused(&mut writer, p1, control::ControlError::UnsupportedFormat);
    refused(&mut writer, other3, control::ControlError::IdentityMismatch);
    clean(&writer);

    // A Profile2 envelope offered under the Profile3 expectation, with the
    // selector bound to either digest, never reaches the selector.
    for bound in [&profile2, &profile3] {
        let mut writer = fixture(65, false);
        install(&writer, &profile2, bound);
        refused(&mut writer, p3, control::ControlError::UnsupportedFormat);
        clean(&writer);
    }
    // Header version 0/4/65535 is refused under every bound grammar.
    for version in [0u16, 4, u16::MAX] {
        let mut writer = fixture(65, false);
        let bytes = relabel(version, 2 << 20);
        install(&writer, &bytes, &bytes);
        for bound in [p1, p3] {
            refused(&mut writer, bound, control::ControlError::UnsupportedFormat);
        }
        clean(&writer);
    }
}
