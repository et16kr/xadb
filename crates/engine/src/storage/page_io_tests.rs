use super::*;
use crate::api::DatabaseId;
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    identity::{Generation, ObjectId},
    io::FileIo,
    namespace::{
        DirectoryEntry, FileIdentity, FileMetadata, NamespaceErrorKind, NamespaceResult, Selector,
    },
};

const PROFILE: &[u8] = include_bytes!("../../tests/fixtures/dev3-io-profile.bin");
const SELECTOR: &[u8] = include_bytes!("../../tests/fixtures/dev3-io-selector.bin");
const GOLDEN: &[u8] = include_bytes!("../../tests/fixtures/dev3-page-v1-cells.bin");
const DATA: &str = "d3-data-0000000100000002";
const MAP: &str = "d3-filemap-0";
fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 17,
    }
}
fn expected() -> PageIdentity {
    PageIdentity {
        database: ns().database,
        page: super::super::page::PageRef {
            file: FileId::new(0x100000002).unwrap(),
            page_no: 3,
            incarnation: Generation::new(4).unwrap(),
        },
        owner: ObjectId::new(5).unwrap(),
        owner_generation: Generation::new(6).unwrap(),
    }
}
fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn seal(b: &mut [u8]) {
    let end = b.len() - 32;
    let hash = content_digest(&b[..end]);
    b[end..].copy_from_slice(&hash);
}
// Independent raw contract offsets; fixture setup never calls product encoders.
fn raw_control(kind: u8, index: u32, count: u32, body: &[u8]) -> Vec<u8> {
    let mut b = vec![0; 160 + body.len()];
    let len = b.len() as u32;
    b[..4].copy_from_slice(b"XDC1");
    put16(&mut b, 4, 1);
    b[6] = kind;
    put16(&mut b, 8, 128);
    put32(&mut b, 12, len);
    b[16..32].fill(0x11);
    put64(&mut b, 32, 17);
    put64(&mut b, 40, 23);
    put32(&mut b, 48, index);
    put32(&mut b, 52, count);
    put32(&mut b, 56, body.len() as u32);
    b[128..len as usize - 32].copy_from_slice(body);
    seal(&mut b);
    b
}
fn raw_ref(kind: u8, index: u32, bytes: &[u8]) -> [u8; 64] {
    let mut b = [0; 64];
    b[0] = kind;
    put32(&mut b, 4, index);
    put64(&mut b, 8, 23);
    put32(&mut b, 16, bytes.len() as u32);
    b[32..].copy_from_slice(&content_digest(bytes));
    b
}

#[derive(Clone, Copy, Debug)]
enum Action {
    BeforeError,
    Short(usize),
    Interrupted,
    PrefixEof,
    PrefixError,
    InvalidCount,
}
#[derive(Clone, Copy, Debug)]
struct Plan {
    phase: usize,
    action: Action,
}
#[derive(Debug)]
struct Read {
    name: String,
    offset: u64,
    bytes: usize,
    calls: usize,
}
struct Active {
    phase: usize,
    next: u64,
    end: u64,
}
#[derive(Default)]
struct State {
    files: BTreeMap<String, Vec<u8>>,
    handles: usize,
    peak: usize,
    mutations: usize,
    reads: Vec<Read>,
    active: Option<Active>,
    plan: Option<Plan>,
    hit: bool,
    actual_short: bool,
}
struct Writer(Arc<Mutex<State>>);
struct File {
    state: Arc<Mutex<State>>,
    name: String,
}
impl Drop for File {
    fn drop(&mut self) {
        self.state.lock().unwrap().handles -= 1;
    }
}
fn io_failure() -> io::Error {
    io::Error::other("fixed read fault")
}
fn unsupported() -> NamespaceError {
    NamespaceError::new(
        NamespaceErrorKind::Io,
        Effect::Unchanged,
        "unexpected mutation",
    )
}
impl FileIo for File {
    fn read_at(&mut self, output: &mut [u8], offset: u64) -> io::Result<usize> {
        let mut s = self.state.lock().unwrap();
        if s.active.is_none() {
            let phase = s.reads.len();
            s.reads.push(Read {
                name: self.name.clone(),
                offset,
                bytes: output.len(),
                calls: 0,
            });
            s.active = Some(Active {
                phase,
                next: offset,
                end: offset + output.len() as u64,
            });
        }
        let a = s.active.as_ref().unwrap();
        assert_eq!(a.next, offset);
        assert_eq!(a.end - offset, output.len() as u64);
        let phase = a.phase;
        assert_eq!(s.reads[phase].name, self.name);
        s.reads[phase].calls += 1;
        let call = s.reads[phase].calls;
        let action = s.plan.filter(|p| p.phase == phase).map(|p| p.action);
        if action.is_some() {
            s.hit = true;
        }
        match action {
            Some(Action::BeforeError) => {
                s.active = None;
                return Err(io_failure());
            }
            Some(Action::Interrupted) if call == 1 => {
                return Err(io::ErrorKind::Interrupted.into());
            }
            Some(Action::PrefixEof) if call == 2 => {
                s.active = None;
                return Ok(0);
            }
            Some(Action::PrefixError) if call == 2 => {
                s.active = None;
                return Err(io_failure());
            }
            Some(Action::InvalidCount) => {
                s.active = None;
                return Ok(output.len() + 1);
            }
            _ => {}
        }
        let cap = match action {
            Some(Action::Short(cap)) if call == 1 => cap.min(output.len() - 1),
            Some(Action::Short(cap)) => cap,
            Some(Action::PrefixEof | Action::PrefixError) if call == 1 => 1,
            _ => output.len(),
        };
        let bytes = s.files.get(&self.name).unwrap();
        let start = usize::try_from(offset).unwrap();
        let count = bytes.len().saturating_sub(start).min(output.len()).min(cap);
        if count > 0 {
            output[..count].copy_from_slice(&bytes[start..start + count]);
        }
        if matches!(action, Some(Action::Short(_))) && call == 1 {
            s.actual_short = count > 0 && count < output.len();
        }
        if count == 0 || count == output.len() {
            s.active = None;
        } else {
            s.active.as_mut().unwrap().next += count as u64;
        }
        Ok(count)
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> io::Result<usize> {
        self.state.lock().unwrap().mutations += 1;
        Err(io_failure())
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.state.lock().unwrap().mutations += 1;
        Err(io_failure())
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        self.state.lock().unwrap().mutations += 1;
        Err(io_failure())
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<File> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
    fn open_existing(&mut self, _: &FileName, _: OpenMode) -> NamespaceResult<File> {
        panic!("must bind returned handle metadata")
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        assert_eq!(mode, OpenMode::ReadOnly);
        let mut s = self.0.lock().unwrap();
        let length = s
            .files
            .get(name.as_str())
            .ok_or_else(|| {
                NamespaceError::new(
                    NamespaceErrorKind::NotFound,
                    Effect::Unchanged,
                    "absent fixture",
                )
            })?
            .len() as u64;
        s.handles += 1;
        s.peak = s.peak.max(s.handles);
        Ok((
            File {
                state: Arc::clone(&self.0),
                name: name.as_str().into(),
            },
            FileMetadata {
                identity: FileIdentity {
                    device: 1,
                    inode: 2,
                },
                length,
            },
        ))
    }
    fn metadata(&self, _: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        panic!("path stat is not handle metadata")
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        panic!("no directory scan or fallback")
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        self.0.lock().unwrap().mutations += 1;
        Err(unsupported())
    }
}
fn fixture(count: usize, retained: bool) -> Writer {
    let chunks = count.div_ceil(64);
    let mut body = vec![0; 64 + 96 * chunks];
    put32(&mut body, 0, count as u32);
    put32(&mut body, 4, chunks as u32);
    put64(&mut body, 8, (count as u64) << 20);
    if count > 0 {
        put64(&mut body, 16, 0x100000002);
        put64(&mut body, 24, 0x100000001 + count as u64);
    }
    put16(&mut body, 32, 64);
    put16(&mut body, 34, 96);
    let mut container = vec![0; 2_113_536];
    for index in 0..chunks {
        let first = index * 64;
        let n = (count - first).min(64);
        let mut entries = vec![0; n * 96];
        for entry in 0..n {
            let file = 0x100000002 + (first + entry) as u64;
            let b = &mut entries[entry * 96..(entry + 1) * 96];
            put64(b, 0, file);
            put64(b, 8, 1 << 20);
            put64(b, 16, 5);
            put64(b, 24, 6);
            put64(b, 32, file);
            put64(b, 48, 1);
            b[56..80].copy_from_slice(format!("d3-data-{file:016x}").as_bytes());
            b[88] = if retained { 2 } else { 1 };
        }
        let chunk = raw_control(3, index as u32, n as u32, &entries);
        let offset = 16_384 * (1 + index);
        container[offset..offset + chunk.len()].copy_from_slice(&chunk);
        let b = &mut body[64 + index * 96..64 + (index + 1) * 96];
        b[..64].copy_from_slice(&raw_ref(3, index as u32, &chunk));
        put16(b, 64, n as u16);
        put64(b, 72, 0x100000002 + first as u64);
        put64(b, 80, 0x100000001 + (first + n) as u64);
    }
    let root = raw_control(2, 0, chunks as u32, &body);
    container[..root.len()].copy_from_slice(&root);
    let mut selector = SELECTOR.to_vec();
    selector[80..144].copy_from_slice(&raw_ref(2, 0, &root));
    seal(&mut selector);
    if count == 65 && !retained {
        assert_eq!(
            &root,
            include_bytes!("../../tests/fixtures/dev3-io-root.bin")
        );
        assert_eq!(&selector, SELECTOR);
        assert_eq!(
            &container[16_384..16_384 + 6304],
            include_bytes!("../../tests/fixtures/dev3-io-chunk0.bin")
        );
        assert_eq!(
            &container[32_768..32_768 + 256],
            include_bytes!("../../tests/fixtures/dev3-io-chunk1.bin")
        );
    }
    let mut data = vec![0; 1 << 20];
    data[3 * 16_384..4 * 16_384].copy_from_slice(GOLDEN);
    let files = BTreeMap::from([
        ("d3-profile".into(), PROFILE.to_vec()),
        ("FILEMAP".into(), selector),
        (MAP.into(), container),
        (DATA.into(), data),
    ]);
    Writer(Arc::new(Mutex::new(State {
        files,
        ..State::default()
    })))
}
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
