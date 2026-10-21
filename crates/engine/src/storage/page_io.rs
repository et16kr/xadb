//! Selected, bounded FILEMAP reads. No mutation, repair, buffer pin or DB READY.
//! The caller freezes namespace/page reuse while this exclusive reader exists.
#![allow(dead_code)] // Internal S2 leaf; transaction/buffer integration follows.

use super::{
    control::{
        self, FileMapCursor, FileMapEntry, FileMapRoot, Profile, SelectedControl, SelectorRole,
    },
    page::{FileId, PAGE_BYTES, PageError, PageIdentity, PageKind, PageView, decode_page},
};
use crate::api::Namespace;
use xadb_foundation::{
    io::read_exact_at,
    namespace::{FileName, NamespaceError, OpenMode, WriterNamespace},
};

/// A failed read never changes database bytes; partially filled output is invalid.
#[derive(Debug)]
pub(crate) enum PageIoError {
    Namespace(NamespaceError),
    Read(xadb_foundation::Error),
    Control(control::ControlError),
    Page(PageError),
    InvalidRequest,
    IdentityMismatch,
    LengthMismatch,
    MissingMapping,
    OutOfBounds,
}

impl From<NamespaceError> for PageIoError {
    fn from(error: NamespaceError) -> Self {
        Self::Namespace(error)
    }
}
impl From<xadb_foundation::Error> for PageIoError {
    fn from(error: xadb_foundation::Error) -> Self {
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

fn open_exact<W: WriterNamespace>(writer: &mut W, name: &FileName, length: u64) -> Result<W::File> {
    let (file, metadata) = writer.open_existing_with_metadata(name, OpenMode::ReadOnly)?;
    if metadata.length != length {
        return Err(PageIoError::LengthMismatch);
    }
    Ok(file)
}

/// Borrows one fixed selection and caller workspace, retaining at most two files.
/// The caller must not resize/reuse pages or mutate the selected controls through
/// other outstanding handles. This is not a transaction snapshot or frame latch.
pub(crate) struct PageFileReader<'a, W: WriterNamespace> {
    writer: &'a mut W,
    container: W::File,
    root: FileMapRoot<'a>,
    profile: Profile,
    selected: SelectedControl,
    scratch: &'a mut [u8; PAGE_BYTES],
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
        let profile = {
            let mut file = open_exact(
                writer,
                &FileName::new("d3-profile")?,
                control::PROFILE_BYTES as u64,
            )?;
            let bytes = &mut scratch[..control::PROFILE_BYTES];
            read_exact_at(&mut file, bytes, 0)?;
            control::decode_bootstrap_profile(bytes, expected)?
        };
        let selected = {
            let mut file = open_exact(
                writer,
                &FileName::new("FILEMAP")?,
                control::SELECTOR_BYTES as u64,
            )?;
            let bytes = &mut scratch[..control::SELECTOR_BYTES];
            read_exact_at(&mut file, bytes, 0)?;
            control::decode_selector(bytes, SelectorRole::FileMap, profile)?
        };
        let mut container = open_exact(
            writer,
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
        let mut file = open_exact(
            self.writer,
            &control::data_file_name(entry.file),
            entry.mapped_bytes,
        )?;
        read_exact_at(&mut file, output, offset)?;
        let view = decode_page(output, expected, kind)?;
        if view.header().level != level {
            return Err(PageIoError::IdentityMismatch);
        }
        Ok(view)
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
