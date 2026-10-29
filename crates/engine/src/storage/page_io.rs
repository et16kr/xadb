//! Selected, bounded FILEMAP reads. No mutation, repair, buffer pin or DB READY.
//! Exclusive callers or the owning fresh session freeze namespace/page reuse.
#![allow(dead_code)] // Internal S2 leaf; transaction/buffer integration follows.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use super::{
    control::{
        self, FileMapCursor, FileMapEntry, FileMapRoot, Profile, SelectedControl, SelectorRole,
    },
    page::{FileId, PAGE_BYTES, PageError, PageIdentity, PageKind, PageView, decode_page},
    session_fence::SessionFence,
};
use crate::api::Namespace;
use foundation::{
    io::read_exact_at,
    namespace::{FileMetadata, FileName, NamespaceError, OpenMode, WriterNamespace},
};

/// A failed read never changes database bytes; partially filled output is invalid.
#[derive(Debug)]
pub(crate) enum PageIoError {
    Namespace(NamespaceError),
    Read(foundation::Error),
    Control(control::ControlError),
    Page(PageError),
    InvalidRequest,
    IdentityMismatch,
    LengthMismatch,
    MissingMapping,
    OutOfBounds,
    Busy,
    Fenced,
    Poisoned,
}

impl From<NamespaceError> for PageIoError {
    fn from(error: NamespaceError) -> Self {
        Self::Namespace(error)
    }
}
impl From<foundation::Error> for PageIoError {
    fn from(error: foundation::Error) -> Self {
        Self::Read(error)
    }
}
impl From<control::ControlError> for PageIoError {
    fn from(error: control::ControlError) -> Self {
        Self::Control(error)
    }
}
impl From<PageError> for PageIoError {
    fn from(error: PageError) -> Self {
        Self::Page(error)
    }
}

type Result<T> = std::result::Result<T, PageIoError>;

/// The lease stores no mutex guard: reader/buffer may move between scoped threads.
struct ReaderLease<'a, W: WriterNamespace> {
    writer: &'a Mutex<W>,
    fence: &'a SessionFence,
    active: &'a AtomicBool,
}
impl<W: WriterNamespace> Drop for ReaderLease<'_, W> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.fence.fence();
        }
        self.active.store(false, Ordering::Release);
    }
}

enum NamespaceAccess<'a, W: WriterNamespace> {
    Exclusive(&'a mut W),
    Shared(ReaderLease<'a, W>),
}
impl<'a, W: WriterNamespace> NamespaceAccess<'a, W> {
    fn named(&self, name: &FileName) -> Result<FileMetadata> {
        self.live()?;
        match self {
            Self::Exclusive(writer) => writer.metadata(name)?.ok_or(PageIoError::IdentityMismatch),
            Self::Shared(lease) => {
                let writer = lease.writer.lock().map_err(|_| {
                    lease.fence.fence();
                    PageIoError::Poisoned
                })?;
                if lease.fence.is_fenced() {
                    return Err(PageIoError::Fenced);
                }
                writer.metadata(name)?.ok_or(PageIoError::IdentityMismatch)
            }
        }
    }

    fn open_bound(&mut self, name: &FileName, length: u64) -> Result<(W::File, FileMetadata)> {
        self.live()?;
        let (file, opened) = match self {
            Self::Exclusive(writer) => {
                writer.open_existing_with_metadata(name, OpenMode::ReadOnly)?
            }
            Self::Shared(lease) => {
                let mut writer = lease.writer.lock().map_err(|_| {
                    lease.fence.fence();
                    PageIoError::Poisoned
                })?;
                if lease.fence.is_fenced() {
                    return Err(PageIoError::Fenced);
                }
                writer.open_existing_with_metadata(name, OpenMode::ReadOnly)?
            }
        };
        if opened.length != length {
            return Err(PageIoError::LengthMismatch);
        }
        if self.named(name)? != opened {
            return Err(PageIoError::IdentityMismatch);
        }
        Ok((file, opened))
    }

    fn fence(&self) -> Option<&'a SessionFence> {
        match self {
            Self::Exclusive(_) => None,
            Self::Shared(lease) => Some(lease.fence),
        }
    }

    fn live(&self) -> Result<()> {
        if let Self::Shared(lease) = self {
            if lease.writer.is_poisoned() {
                lease.fence.fence();
                return Err(PageIoError::Poisoned);
            }
            if lease.fence.is_fenced() {
                return Err(PageIoError::Fenced);
            }
        }
        Ok(())
    }

    fn open_exact(&mut self, name: &FileName, length: u64) -> Result<W::File> {
        self.live()?;
        let (file, metadata) = match self {
            Self::Exclusive(writer) => {
                writer.open_existing_with_metadata(name, OpenMode::ReadOnly)?
            }
            Self::Shared(lease) => {
                let mut writer = lease.writer.lock().map_err(|_| {
                    lease.fence.fence();
                    PageIoError::Poisoned
                })?;
                if lease.fence.is_fenced() {
                    return Err(PageIoError::Fenced);
                }
                writer.open_existing_with_metadata(name, OpenMode::ReadOnly)?
            }
        };
        // Namespace lock is released before any data/control file read.
        if metadata.length != length {
            return Err(PageIoError::LengthMismatch);
        }
        Ok(file)
    }
}

/// Which profile grammar an open uses. `Bootstrap` is the original legacy
/// discovery entry (Profile1/2, optional namespace); `Installed` re-reads the
/// caller's bound Profile through `control::decode_installed_profile`.
#[derive(Clone, Copy)]
enum ProfileGrammar {
    Bootstrap(Option<Namespace>),
    Installed(Profile),
}

/// Borrows one fixed selection and caller workspace, retaining at most two files.
/// The caller must not resize/reuse pages or mutate the selected controls through
/// other outstanding handles. This is not a transaction snapshot or frame latch.
pub(crate) struct PageFileReader<'a, W: WriterNamespace> {
    // Rust drops fields in declaration order. Close the retained handle before
    // publishing the lease as available to another shared constructor.
    container: W::File,
    writer: NamespaceAccess<'a, W>,
    root: FileMapRoot<'a>,
    profile: Profile,
    selected: SelectedControl,
    scratch: &'a mut [u8; PAGE_BYTES],
}

/// New raw-home reader closure; caller root/scratch/output pages are loans.
/// Includes the retained container, second data file, named/opened metadata,
/// mapping/codec carriers and return temporaries without optimizer assumptions.
pub(in crate::storage) fn retention_home_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    size_of::<PageFileReader<'_, W>>()
        + 2 * size_of::<Result<PageFileReader<'_, W>>>()
        + 2 * size_of::<W::File>()
        + 2 * size_of::<Result<(W::File, FileMetadata)>>()
        + 4 * size_of::<FileMetadata>()
        + 3 * size_of::<Result<FileMetadata>>()
        + 3 * size_of::<PageIdentity>()
        + 2 * size_of::<FileMapEntry>()
        + 2 * size_of::<Result<FileMapEntry>>()
        + 2 * size_of::<control::FileMapRoot<'_>>()
        + 2 * size_of::<control::FileMapChunk<'_>>()
        + 2 * size_of::<control::FileMapCursor<'_>>()
        + 4 * size_of::<FileName>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<std::sync::MutexGuard<'_, W>>()
        + 4 * size_of::<foundation::codec::ContentHasher>()
        + 4096 // Original open/profile/mapping nested scalar and codec carriers.
        + installed_open_source_bytes()
}

/// Growth of an installed-grammar open over the original legacy open whose
/// profile decode the 4096B allowance above covers: the
/// `decode_installed_profile` wrapper over the Row decoder bound (which must
/// itself fit that allowance) and the wider `ProfileGrammar` argument.
pub(in crate::storage) fn installed_open_source_bytes() -> usize {
    use std::mem::size_of;
    debug_assert!(control::row_profile::source_bytes() <= 4096);
    (control::installed_profile_source_bytes() - control::row_profile::source_bytes())
        + size_of::<ProfileGrammar>().saturating_sub(size_of::<Option<Namespace>>())
}

impl<'a, W: WriterNamespace> PageFileReader<'a, W> {
    /// Validate profile, selected root, and every declared chunk before returning.
    /// Open/read failures close all acquired handles. Only selected logical
    /// prefixes are read; padding and the other bank cannot supply a fallback.
    pub(crate) fn open(
        writer: &'a mut W,
        expected: Option<Namespace>,
        root_bytes: &'a mut [u8; PAGE_BYTES],
        scratch: &'a mut [u8; PAGE_BYTES],
    ) -> Result<Self> {
        Self::open_inner(
            NamespaceAccess::Exclusive(writer),
            ProfileGrammar::Bootstrap(expected),
            None,
            root_bytes,
            scratch,
        )
    }

    /// Same exclusive open, but the caller's bound Profile selects the profile
    /// grammar (`control::decode_installed_profile`) and must equal the file.
    /// The plan4 owners call it; `open` stays the legacy discovery entry for
    /// its DEV2 Profile1 owners.
    pub(crate) fn open_profile(
        writer: &'a mut W,
        profile: Profile,
        root_bytes: &'a mut [u8; PAGE_BYTES],
        scratch: &'a mut [u8; PAGE_BYTES],
    ) -> Result<Self> {
        Self::open_inner(
            NamespaceAccess::Exclusive(writer),
            ProfileGrammar::Installed(profile),
            None,
            root_bytes,
            scratch,
        )
    }

    /// Borrow the immutable installed selection without holding the namespace lock.
    /// The bound profile selects the profile grammar and must equal the file
    /// (`Control(IdentityMismatch)` right after `d3-profile`); every
    /// construction also compares the complete selected root identity.
    pub(super) fn open_shared(
        writer: &'a Mutex<W>,
        fence: &'a SessionFence,
        active: &'a AtomicBool,
        expected_profile: Profile,
        expected_selected: SelectedControl,
        root_bytes: &'a mut [u8; PAGE_BYTES],
        scratch: &'a mut [u8; PAGE_BYTES],
    ) -> Result<Self> {
        let _unwind = fence.unwind_guard();
        if writer.is_poisoned() {
            fence.fence();
            return Err(PageIoError::Poisoned);
        }
        if fence.is_fenced() {
            return Err(PageIoError::Fenced);
        }
        active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| PageIoError::Busy)?;
        let access = NamespaceAccess::Shared(ReaderLease {
            writer,
            fence,
            active,
        });
        Self::open_inner(
            access,
            ProfileGrammar::Installed(expected_profile),
            Some((expected_profile, expected_selected)),
            root_bytes,
            scratch,
        )
    }

    fn open_inner(
        mut writer: NamespaceAccess<'a, W>,
        grammar: ProfileGrammar,
        installed: Option<(Profile, SelectedControl)>,
        root_bytes: &'a mut [u8; PAGE_BYTES],
        scratch: &'a mut [u8; PAGE_BYTES],
    ) -> Result<Self> {
        let _unwind = writer.fence().map(SessionFence::unwind_guard);
        writer.live()?;
        let profile = {
            let mut file =
                writer.open_exact(&FileName::new("d3-profile")?, control::PROFILE_BYTES as u64)?;
            let bytes = &mut scratch[..control::PROFILE_BYTES];
            read_exact_at(&mut file, bytes, 0)?;
            match grammar {
                ProfileGrammar::Bootstrap(expected) => {
                    control::decode_bootstrap_profile(bytes, expected)?
                }
                ProfileGrammar::Installed(expected) => {
                    control::decode_installed_profile(bytes, expected)?
                }
            }
        };
        let selected = {
            let mut file =
                writer.open_exact(&FileName::new("FILEMAP")?, control::SELECTOR_BYTES as u64)?;
            let bytes = &mut scratch[..control::SELECTOR_BYTES];
            read_exact_at(&mut file, bytes, 0)?;
            control::decode_selector(bytes, SelectorRole::FileMap, profile)?
        };
        let mut container = writer.open_exact(
            &control::control_file_name(selected.reference)?,
            control::CONTAINER_BYTES,
        )?;
        let root_length = selected.reference.encoded_len as usize;
        read_exact_at(&mut container, &mut root_bytes[..root_length], 0)?;
        let root =
            control::decode_filemap_root(&root_bytes[..root_length], selected.reference, profile)?;
        let mut cursor = FileMapCursor::new(root);
        while let Some(location) = cursor.next() {
            let bytes = &mut scratch[..location.reference.encoded_len as usize];
            read_exact_at(&mut container, bytes, location.offset)?;
            cursor.accept(bytes)?;
        }
        cursor.finish()?;
        if installed.is_some_and(|expected| expected != (profile, selected)) {
            return Err(PageIoError::IdentityMismatch);
        }
        writer.live()?;
        Ok(Self {
            writer,
            container,
            root,
            profile,
            selected,
            scratch,
        })
    }

    pub(super) fn profile(&self) -> Profile {
        self.profile
    }

    pub(super) fn session_fence(&self) -> Option<&'a SessionFence> {
        self.writer.fence()
    }

    pub(super) fn selected(&self) -> SelectedControl {
        self.selected
    }

    pub(crate) fn selector_generation(&self) -> u64 {
        self.selected.generation
    }

    fn mapping(&mut self, file: FileId) -> Result<FileMapEntry> {
        let descriptor = self
            .root
            .find_chunk(file)
            .ok_or(PageIoError::MissingMapping)?;
        // The validated root bounds index to 0..128 and encoded_len to scratch.
        let offset = (u64::from(descriptor.reference.index) + 1) * PAGE_BYTES as u64;
        let bytes = &mut self.scratch[..descriptor.reference.encoded_len as usize];
        read_exact_at(&mut self.container, bytes, offset)?;
        let chunk = control::decode_filemap_chunk(bytes, descriptor.reference, self.profile)?;
        let summary = chunk.summary();
        if summary.files != u32::from(descriptor.count)
            || summary.first != Some(descriptor.first)
            || summary.last != Some(descriptor.last)
        {
            return Err(PageIoError::Control(control::ControlError::Corrupt));
        }
        for index in 0..summary.files as usize {
            let entry = chunk.entry(index).ok_or(PageIoError::MissingMapping)?;
            if entry.file == file {
                return Ok(entry);
            }
        }
        Err(PageIoError::MissingMapping)
    }

    /// Read one envelope into caller output, binding it to the verified profile.
    /// The result borrows only output. It does not certify cell semantics or
    /// visibility, and error output must be discarded. Selection stays reusable.
    pub(crate) fn read_page<'b>(
        &mut self,
        expected: PageIdentity,
        kind: PageKind,
        level: u8,
        output: &'b mut [u8; PAGE_BYTES],
    ) -> Result<PageView<'b>> {
        let _unwind = self.writer.fence().map(SessionFence::unwind_guard);
        self.writer.live()?;
        validate_page_request(self.profile, expected, kind, level)?;
        let entry = self.mapping(expected.page.file)?;
        let offset = expected
            .page
            .page_no
            .checked_mul(PAGE_BYTES as u64)
            .filter(|offset| {
                offset
                    .checked_add(PAGE_BYTES as u64)
                    .is_some_and(|end| end <= entry.mapped_bytes)
            })
            .ok_or(PageIoError::OutOfBounds)?;
        if kind == PageKind::Allocation
            && entry.metadata_identity(self.profile.spec().namespace, expected.page.page_no)?
                != expected
        {
            return Err(PageIoError::IdentityMismatch);
        }
        let mut file = self
            .writer
            .open_exact(&control::data_file_name(entry.file), entry.mapped_bytes)?;
        read_exact_at(&mut file, output, offset)?;
        let view = decode_page(output, expected, kind)?;
        if view.header().level != level {
            return Err(PageIoError::IdentityMismatch);
        }
        self.writer.live()?;
        Ok(view)
    }

    /// Physical home observation for the closed live history consumer. Zero
    /// and old images are deliberately not interpreted here. Only the upper
    /// replay proof may authorize using these bytes for a DWB write or load.
    pub(in crate::storage) fn read_retention_home(
        &mut self,
        expected: PageIdentity,
        kind: PageKind,
        level: u8,
        allocator: PageIdentity,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<()> {
        let _unwind = self.writer.fence().map(SessionFence::unwind_guard);
        self.writer.live()?;
        validate_page_request(self.profile, expected, kind, level)?;
        let entry = self.mapping(expected.page.file)?;
        if allocator.page.page_no != 0
            || expected.database != allocator.database
            || expected.page.file != allocator.page.file
            || entry.metadata_identity(self.profile.spec().namespace, 0)? != allocator
        {
            return Err(PageIoError::IdentityMismatch);
        }
        let offset = expected
            .page
            .page_no
            .checked_mul(PAGE_BYTES as u64)
            .filter(|offset| {
                offset
                    .checked_add(PAGE_BYTES as u64)
                    .is_some_and(|end| end <= entry.mapped_bytes)
            })
            .ok_or(PageIoError::OutOfBounds)?;
        let name = control::data_file_name(entry.file);
        let (mut file, opened) = self.writer.open_bound(&name, entry.mapped_bytes)?;
        read_exact_at(&mut file, output, offset)?;
        // The same opened handle supplies all bytes. The named observation
        // detects replacement; this does not invent a FileIo metadata method.
        if self.writer.named(&name)? != opened {
            return Err(PageIoError::IdentityMismatch);
        }
        self.writer.live()?;
        Ok(())
    }
}

pub(super) fn validate_page_request(
    profile: Profile,
    expected: PageIdentity,
    kind: PageKind,
    level: u8,
) -> Result<()> {
    if expected.database != profile.spec().namespace.database {
        return Err(PageIoError::IdentityMismatch);
    }
    if !kind.accepts_level(level)
        || expected.page.page_no.is_multiple_of(64) != (kind == PageKind::Allocation)
    {
        return Err(PageIoError::InvalidRequest);
    }
    Ok(())
}

#[cfg(test)]
#[path = "page_io_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "page_io_test_support.rs"]
pub(super) mod test_support;
