//! Linux selected-page buffer integration; all data files are fresh fixtures.
use std::{
    fs, io,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use xadb_engine::testing::{
    BufferTraceFailure, BufferTraceOperation as Op, BufferTracePage, PageReadFailure,
    PageReadRequest, run_selected_buffer_trace, selected_buffer_pool_bytes,
};
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceError, NamespaceErrorKind,
        NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};
use xadb_platform::namespace::LinuxDirectory;
#[path = "support/selected_pages.rs"]
mod selected_pages;
use selected_pages::*;
const PAGE: usize = 16_384;

fn page_bytes(page: u64) -> Vec<u8> {
    let mut bytes = GOLDEN.to_vec();
    bytes[32..40].copy_from_slice(&page.to_le_bytes());
    let hash = content_digest(&bytes[..PAGE - 32]);
    bytes[PAGE - 32..].copy_from_slice(&hash);
    bytes
}
fn prepare_buffer(directory: &Path) {
    prepare(directory);
    let mut bytes = vec![0; 1 << 20];
    for page in 1..=3 {
        bytes[page * PAGE..(page + 1) * PAGE].copy_from_slice(&page_bytes(page as u64));
    }
    fs::write(directory.join(DATA), bytes).unwrap();
}
fn request(page: u64) -> PageReadRequest {
    PageReadRequest {
        database: [0x11; 16],
        file: 0x100000002,
        page,
        incarnation: 4,
        owner: 5,
        owner_generation: 6,
        kind: 1,
        level: 0,
    }
}
fn verify(observed: &BufferTracePage, page: u64) {
    assert_eq!(observed.digest, content_digest(&page_bytes(page)));
    assert_eq!(observed.identity, ([0x11; 16], 0x100000002, page, 4, 5, 6));
    assert_eq!(observed.kind_level, (1, 0));
    assert_eq!(
        observed.lsn_cells,
        (u64::from_le_bytes(GOLDEN[64..72].try_into().unwrap()), 3)
    );
}
#[derive(Default)]
struct Counts {
    data_reads: AtomicUsize,
    handles: AtomicUsize,
    peak: AtomicUsize,
    mutations: AtomicUsize,
}
struct Counted<W> {
    inner: W,
    counts: Arc<Counts>,
}
struct CountedFile<F> {
    inner: F,
    data: bool,
    counts: Arc<Counts>,
}
impl<F> Drop for CountedFile<F> {
    fn drop(&mut self) {
        self.counts.handles.fetch_sub(1, Ordering::SeqCst);
    }
}
impl<F: FileIo> FileIo for CountedFile<F> {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        if self.data {
            self.counts.data_reads.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.read_at(bytes, offset)
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> io::Result<usize> {
        self.counts.mutations.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::other("unexpected buffer write"))
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        self.counts.mutations.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::other("unexpected buffer resize"))
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.counts.mutations.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::other("unexpected buffer sync"))
    }
}
impl<W: WriterNamespace> Counted<W> {
    fn wrap(&self, inner: W::File, name: &FileName) -> CountedFile<W::File> {
        let handles = self.counts.handles.fetch_add(1, Ordering::SeqCst) + 1;
        self.counts.peak.fetch_max(handles, Ordering::SeqCst);
        CountedFile {
            inner,
            data: name.as_str() == DATA,
            counts: Arc::clone(&self.counts),
        }
    }
    fn forbidden(&self) -> NamespaceError {
        self.counts.mutations.fetch_add(1, Ordering::SeqCst);
        NamespaceError::new(
            NamespaceErrorKind::Io,
            Effect::Unchanged,
            "unexpected namespace mutation",
        )
    }
}
impl<W: WriterNamespace> WriterNamespace for Counted<W> {
    type File = CountedFile<W::File>;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        Err(self.forbidden())
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<Self::File> {
        Err(self.forbidden())
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<Self::File> {
        let f = self.inner.open_existing(name, mode)?;
        Ok(self.wrap(f, name))
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        assert_eq!(mode, OpenMode::ReadOnly);
        let (f, m) = self.inner.open_existing_with_metadata(name, mode)?;
        Ok((self.wrap(f, name), m))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        Err(self.forbidden())
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        Err(self.forbidden())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        Err(self.forbidden())
    }
}
fn unchanged(counts: &Counts) {
    assert_eq!(counts.handles.load(Ordering::SeqCst), 0);
    assert_eq!(counts.mutations.load(Ordering::SeqCst), 0);
    assert!(counts.peak.load(Ordering::SeqCst) <= 2);
}

#[test]
fn native_read_buffer_hits_evictions_and_reopen() {
    assert!(selected_buffer_pool_bytes::<<LinuxDirectory as DirectoryPort>::Writer>() <= 4096);
    assert!(
        selected_buffer_pool_bytes::<Counted<<LinuxDirectory as DirectoryPort>::Writer>>() <= 4096
    );
    let directory = tempfile::tempdir().unwrap();
    prepare_buffer(directory.path());
    let port = LinuxDirectory::open_existing(directory.path()).unwrap();
    let operations = [
        Op::Pin {
            slot: 0,
            request: 0,
        },
        Op::Pin {
            slot: 1,
            request: 0,
        },
        Op::Pin {
            slot: 2,
            request: 1,
        },
        Op::Pin {
            slot: 3,
            request: 2,
        },
        Op::Release { slot: 0 },
        Op::Pin {
            slot: 3,
            request: 2,
        },
        Op::Read { slot: 1 },
        Op::Release { slot: 1 },
        Op::Pin {
            slot: 3,
            request: 2,
        },
        Op::Read { slot: 2 },
        Op::Release { slot: 2 },
        Op::Pin {
            slot: 0,
            request: 0,
        },
        Op::Read { slot: 3 },
        Op::Release { slot: 0 },
        Op::Release { slot: 3 },
    ];
    let requests = [request(1), request(2), request(3)];
    for _ in 0..2 {
        let mut writer = Counted {
            inner: port.acquire_writer().unwrap(),
            counts: Arc::new(Counts::default()),
        };
        let before = inventory(directory.path());
        let trace =
            run_selected_buffer_trace(&mut writer, ([0x11; 16], 17), &requests, &operations)
                .unwrap();
        assert_eq!(trace.len(), 15);
        for (i, result) in trace.iter().enumerate() {
            match i {
                3 | 5 => assert_eq!(*result, Err(BufferTraceFailure::Busy)),
                4 | 7 | 10 | 13 | 14 => assert_eq!(*result, Ok(None)),
                _ => verify(
                    result.as_ref().unwrap().as_ref().unwrap(),
                    match i {
                        2 | 9 => 2,
                        8 | 12 => 3,
                        _ => 1,
                    },
                ),
            }
        }
        assert_eq!(writer.counts.data_reads.load(Ordering::SeqCst), 4);
        unchanged(&writer.counts);
        assert_eq!(inventory(directory.path()), before);
        // Script misuse remains a separately identifiable error; bounds reject before I/O.
        let bad = [
            Op::Pin {
                slot: 0,
                request: 0,
            },
            Op::Pin {
                slot: 0,
                request: 1,
            },
            Op::Read { slot: 4 },
            Op::Release { slot: 3 },
            Op::Pin {
                slot: 1,
                request: 99,
            },
        ];
        let trace =
            run_selected_buffer_trace(&mut writer, ([0x11; 16], 17), &requests, &bad).unwrap();
        verify(trace[0].as_ref().unwrap().as_ref().unwrap(), 1);
        assert!(
            trace[1..]
                .iter()
                .all(|r| *r == Err(BufferTraceFailure::InvalidInput))
        );
        let before_reads = writer.counts.data_reads.load(Ordering::SeqCst);
        assert_eq!(
            run_selected_buffer_trace(
                &mut writer,
                ([0x11; 16], 17),
                &requests,
                &[Op::Read { slot: 0 }; 129]
            ),
            Err(BufferTraceFailure::InvalidInput)
        );
        let excessive: Vec<_> = (0..129).map(|_| request(1)).collect();
        assert_eq!(
            run_selected_buffer_trace(&mut writer, ([0x11; 16], 17), &excessive, &[]),
            Err(BufferTraceFailure::InvalidInput)
        );
        assert_eq!(
            writer.counts.data_reads.load(Ordering::SeqCst),
            before_reads
        );
        unchanged(&writer.counts);
        assert_eq!(inventory(directory.path()), before);
        drop(writer);
        drop(port.acquire_writer().unwrap());
    }
}
#[test]
fn native_read_buffer_rejects_bad_load_and_recovers() {
    for damage in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        prepare_buffer(directory.path());
        match damage {
            0 => {
                let path = directory.path().join(DATA);
                let mut bytes = fs::read(&path).unwrap();
                bytes[PAGE + 256] ^= 1;
                fs::write(path, bytes).unwrap();
            }
            1 => fs::OpenOptions::new()
                .write(true)
                .open(directory.path().join(DATA))
                .unwrap()
                .set_len((1 << 20) - 1)
                .unwrap(),
            _ => {
                let path = directory.path().join("d3-filemap-0");
                let mut bytes = fs::read(&path).unwrap();
                bytes[PAGE + 128] ^= 1;
                fs::write(path, bytes).unwrap();
            }
        }
        let port = LinuxDirectory::open_existing(directory.path()).unwrap();
        {
            let mut writer = Counted {
                inner: port.acquire_writer().unwrap(),
                counts: Arc::new(Counts::default()),
            };
            let before = inventory(directory.path());
            let result = run_selected_buffer_trace(
                &mut writer,
                ([0x11; 16], 17),
                &[request(1)],
                &[Op::Pin {
                    slot: 0,
                    request: 0,
                }],
            );
            if damage == 2 {
                assert_eq!(
                    result,
                    Err(BufferTraceFailure::Source(PageReadFailure::Control))
                );
            } else {
                assert_eq!(
                    result.unwrap(),
                    vec![Err(BufferTraceFailure::Source(if damage == 0 {
                        PageReadFailure::Page
                    } else {
                        PageReadFailure::LengthMismatch
                    }))]
                );
            }
            assert_eq!(
                writer.counts.data_reads.load(Ordering::SeqCst),
                usize::from(damage == 0)
            );
            unchanged(&writer.counts);
            assert_eq!(inventory(directory.path()), before);
        }
        prepare_buffer(directory.path()); // Only after complete pool/reader/writer teardown.
        let mut writer = Counted {
            inner: port.acquire_writer().unwrap(),
            counts: Arc::new(Counts::default()),
        };
        let before = inventory(directory.path());
        let trace = run_selected_buffer_trace(
            &mut writer,
            ([0x11; 16], 17),
            &[request(1)],
            &[Op::Pin {
                slot: 0,
                request: 0,
            }],
        )
        .unwrap();
        verify(trace[0].as_ref().unwrap().as_ref().unwrap(), 1);
        assert_eq!(writer.counts.data_reads.load(Ordering::SeqCst), 1);
        unchanged(&writer.counts);
        assert_eq!(inventory(directory.path()), before);
    }
}
