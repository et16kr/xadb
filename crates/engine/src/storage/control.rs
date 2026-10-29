//! Bounded profile, selected reference and FILEMAP byte representations.
//! No file discovery, I/O, selection, resource reservation or recovery effects.
//! See `devdoc/contracts/dev3-control-format.md`.
// Remove when the first namespace/control effect consumer is connected.
#![allow(dead_code)]

#[path = "control/row_profile.rs"]
pub(crate) mod row_profile;

use super::{
    doublewrite_control::{self, ControlExpectation, ControlView},
    page::{FileId, PAGE_BYTES, PageIdentity, PageRef},
};
use crate::api::{DatabaseId, Namespace};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
    namespace::FileName,
};

pub(crate) const PROFILE_BYTES: usize = 4096;
pub(crate) const SELECTOR_BYTES: usize = 4096;
pub(crate) const REF_BYTES: usize = 64;
pub(crate) const MAX_FILES: usize = 8192;
pub(crate) const CHUNK_ENTRIES: usize = 64;
pub(crate) const MAX_CHUNKS: usize = MAX_FILES / CHUNK_ENTRIES;
pub(crate) const ENTRY_BYTES: usize = 96;
pub(crate) const DESCRIPTOR_BYTES: usize = 96;
pub(crate) const MAX_ROOT_BYTES: usize = 224 + DESCRIPTOR_BYTES * MAX_CHUNKS;
pub(crate) const MAX_CHUNK_BYTES: usize = 160 + ENTRY_BYTES * CHUNK_ENTRIES;
pub(crate) const CONTAINER_BYTES: u64 = 16384 * (1 + MAX_CHUNKS as u64);
pub(crate) const EXTENT_BYTES: u64 = 64 * PAGE_BYTES as u64;
pub(crate) const OWNED_BYTES: u64 = 8 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlError {
    InvalidInput,
    UnsupportedFormat,
    UnsupportedProfile,
    Corrupt,
    IdentityMismatch,
    Incomplete,
    NoSpace,
    Exhausted,
}
type Result<T> = std::result::Result<T, ControlError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ControlKind {
    Profile = 1,
    FileMapRoot = 2,
    FileMapChunk = 3,
    Doublewrite = 4,
    OwnershipRoot = 5,
    OwnershipChunk = 6,
}
impl ControlKind {
    fn decode(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Profile),
            2 => Ok(Self::FileMapRoot),
            3 => Ok(Self::FileMapChunk),
            4 => Ok(Self::Doublewrite),
            5 => Ok(Self::OwnershipRoot),
            6 => Ok(Self::OwnershipChunk),
            _ => Err(ControlError::UnsupportedFormat),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ControlRef {
    pub(crate) kind: ControlKind,
    pub(crate) slot: u8,
    pub(crate) index: u32,
    pub(crate) generation: u64,
    pub(crate) encoded_len: u32,
    pub(crate) digest: [u8; 32],
}
impl ControlRef {
    fn validate(self) -> Result<()> {
        let length = self.encoded_len as usize;
        let length_valid = match self.kind {
            ControlKind::Profile => length == PROFILE_BYTES,
            ControlKind::FileMapRoot => {
                (224..=MAX_ROOT_BYTES).contains(&length) && (length - 224).is_multiple_of(96)
            }
            ControlKind::FileMapChunk => {
                (256..=MAX_CHUNK_BYTES).contains(&length) && (length - 160).is_multiple_of(96)
            }
            ControlKind::Doublewrite => {
                (160..=doublewrite_control::MAX_CONTROL_BYTES).contains(&length)
                    && (length - 160).is_multiple_of(doublewrite_control::ENTRY_BYTES)
            }
            ControlKind::OwnershipRoot => {
                (160..=2176).contains(&length) && (length - 160).is_multiple_of(56)
            }
            ControlKind::OwnershipChunk => {
                (256..=16384).contains(&length) && (length - 160).is_multiple_of(96)
            }
        };
        let ownership = matches!(
            self.kind,
            ControlKind::OwnershipRoot | ControlKind::OwnershipChunk
        );
        if self.generation == 0
            || self.slot > if ownership { 2 } else { 1 }
            || (self.kind == ControlKind::Profile && self.slot != 0)
            || (self.kind == ControlKind::FileMapChunk && self.index >= MAX_CHUNKS as u32)
            || (!matches!(
                self.kind,
                ControlKind::FileMapChunk | ControlKind::OwnershipChunk
            ) && self.index != 0)
            || !length_valid
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }

    /// The digest alone does not establish that a target exists or is selected.
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<&[u8]> {
        self.validate()?;
        if output.len() < REF_BYTES {
            return Err(ControlError::NoSpace);
        }
        output[..REF_BYTES].copy_from_slice(&self.bytes());
        Ok(&output[..REF_BYTES])
    }

    fn bytes(self) -> [u8; REF_BYTES] {
        let mut output = [0; REF_BYTES];
        output[0] = self.kind as u8;
        output[1] = self.slot;
        put32(&mut output, 4, self.index);
        put64(&mut output, 8, self.generation);
        put32(&mut output, 16, self.encoded_len);
        output[32..64].copy_from_slice(&self.digest);
        output
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self> {
        if input.len() != REF_BYTES || !zero(&input[2..4]) || !zero(&input[20..32]) {
            return Err(ControlError::Corrupt);
        }
        let reference = Self {
            kind: ControlKind::decode(input[0])?,
            slot: input[1],
            index: get32(input, 4),
            generation: get64(input, 8),
            encoded_len: get32(input, 16),
            digest: input[32..64].try_into().expect("checked digest length"),
        };
        reference.validate().map_err(|_| ControlError::Corrupt)?;
        Ok(reference)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProfileSpec {
    pub(crate) namespace: Namespace,
    pub(crate) generation: u64,
    pub(crate) id: u32,
}
/// Created only by complete fixed-root validation; not a reservation capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Profile {
    // Flatten the validated spec so format occupies its existing padding.
    // A version2 profile is never reduced to an indistinguishable v1 value.
    namespace: Namespace,
    generation: u64,
    id: u32,
    format: u16,
    digest: [u8; 32],
}
impl Profile {
    pub(crate) fn spec(self) -> ProfileSpec {
        ProfileSpec {
            namespace: self.namespace,
            generation: self.generation,
            id: self.id,
        }
    }
    pub(crate) fn format(self) -> u16 {
        self.format
    }
    pub(crate) fn base_bytes(self) -> usize {
        match self.format {
            1 => 1 << 20,
            2 | 3 => 2 << 20,
            _ => unreachable!("validated Profile format"),
        }
    }
    pub(crate) fn digest(self) -> [u8; 32] {
        self.digest
    }
    pub(crate) fn owned_bytes(self) -> u64 {
        OWNED_BYTES
    }
    pub(crate) fn buffer_bytes(self) -> u64 {
        [4, 16, 64][self.id as usize - 1] << 20
    }
}
#[cfg(test)]
impl Profile {
    /// Test expectation assembled field by field from pinned literals, so a
    /// test never derives its expected value from the decoder it checks.
    pub(crate) const fn literal(
        namespace: Namespace,
        generation: u64,
        id: u32,
        format: u16,
        digest: [u8; 32],
    ) -> Self {
        Self {
            namespace,
            generation,
            id,
            format,
            digest,
        }
    }
}

fn profile_prefix(id: u32, format: u16) -> Result<[u8; 188]> {
    if !matches!(format, 1..=3) {
        return Err(ControlError::UnsupportedFormat);
    }
    if !(1..=3).contains(&id) {
        return Err(ControlError::UnsupportedProfile);
    }
    let mut output = [0; 188];
    let qwords = [
        (8, OWNED_BYTES),
        (16, 1 << 30),
        (24, 256 << 20),
        (32, 16 << 20),
        (40, [4, 16, 64][id as usize - 1] << 20),
        (48, 8 << 20),
        (56, 1 << 20),
        (64, 8 << 20),
        (72, 64 << 20),
        (80, 16 << 20),
        (88, 4 << 20),
        (96, 12 << 20),
        (104, 4 << 20),
        (160, CONTAINER_BYTES),
        (168, if format == 1 { 1 << 20 } else { 2 << 20 }),
    ];
    for (offset, value) in qwords {
        put64(&mut output, offset, value);
    }
    for (offset, value) in [
        (0, id),
        (112, 32),
        (116, 256),
        (120, 1 << 20),
        (124, 32),
        (128, 128),
        (136, 16384),
        (148, 8192),
        (156, 16384),
        (176, 2),
        (180, 4096),
        (184, 1 << 20),
    ] {
        put32(&mut output, offset, value);
    }
    for (offset, value) in [
        (132, 16u16),
        (134, 8),
        (140, 64),
        (142, 64),
        (144, 1010),
        (152, 64),
    ] {
        put16(&mut output, offset, value);
    }
    Ok(output)
}

pub(crate) fn encode_profile(spec: ProfileSpec, output: &mut [u8]) -> Result<&[u8]> {
    encode_profile_format(spec, 1, output)
}

/// Current Catalog CREATE profile. All other control envelopes stay version1.
pub(crate) fn encode_catalog_profile(spec: ProfileSpec, output: &mut [u8]) -> Result<&[u8]> {
    encode_profile_format(spec, 2, output)
}

fn encode_profile_format(spec: ProfileSpec, format: u16, output: &mut [u8]) -> Result<&[u8]> {
    validate_namespace(spec.namespace)?;
    if spec.generation == 0 {
        return Err(ControlError::InvalidInput);
    }
    let prefix = profile_prefix(spec.id, format)?;
    if output.len() < PROFILE_BYTES {
        return Err(ControlError::NoSpace);
    }
    output[..PROFILE_BYTES].fill(0);
    write_header(
        output,
        ControlKind::Profile,
        MapIdentity {
            namespace: spec.namespace,
            generation: spec.generation,
            slot: 0,
        },
        0,
        0,
        PROFILE_BYTES,
    );
    put16(output, 4, format);
    output[128..316].copy_from_slice(&prefix);
    seal(&mut output[..PROFILE_BYTES]);
    Ok(&output[..PROFILE_BYTES])
}

/// The exact fixed profile root may discover a namespace. Descendants must use
/// this verified namespace, never their own header as their expected identity.
pub(crate) fn decode_bootstrap_profile(
    input: &[u8],
    expected: Option<Namespace>,
) -> Result<Profile> {
    if let Some(namespace) = expected {
        validate_namespace(namespace)?;
    }
    let header = read_header_for(input, EnvelopeVersion::LegacyProfile)?;
    if header.kind != ControlKind::Profile {
        return Err(ControlError::Corrupt);
    }
    if expected.is_some_and(|value| value != header.identity.namespace) {
        return Err(ControlError::IdentityMismatch);
    }
    let id = get32(input, 128);
    let format = get16(input, 4);
    if input[128..316] != profile_prefix(id, format)? || !zero(&input[316..4064]) {
        return Err(ControlError::UnsupportedProfile);
    }
    Ok(Profile {
        namespace: header.identity.namespace,
        generation: header.identity.generation,
        id,
        format,
        digest: content_digest(input),
    })
}

/// Re-read an already installed profile under the grammar its caller bound.
/// `expected.format()` selects exactly one decoder (1|2 legacy, 3 Row); no
/// header version is inspected first and a decoder failure is final. The
/// decoded value must then equal `expected` in every field, digest included.
/// Uncalled before the single current-format switch; legacy owners keep
/// `decode_bootstrap_profile`.
pub(crate) fn decode_installed_profile(input: &[u8], expected: Profile) -> Result<Profile> {
    let namespace = Some(expected.namespace);
    let profile = match expected.format {
        1 | 2 => decode_bootstrap_profile(input, namespace)?,
        3 => row_profile::decode_profile(input, namespace)?,
        _ => return Err(ControlError::UnsupportedFormat),
    };
    if profile != expected {
        return Err(ControlError::IdentityMismatch);
    }
    Ok(profile)
}

/// Bound over `decode_installed_profile`: its own argument/result/compare
/// values plus one selected decoder closure. The legacy decoder has the Row
/// decoder's frame shape (same header, prefix and SHA closure; one extra u16
/// local inside that bound's scalar allowance), so the Row bound covers both.
/// The wrapper charge (this bound minus the Row bound) is not yet in any real
/// census; the C4c callers charge it with the decode they replace.
pub(crate) fn installed_profile_source_bytes() -> usize {
    use std::mem::size_of;
    let wrapper = size_of::<(&[u8], Profile)>()
        + size_of::<Option<Namespace>>()
        + 2 * size_of::<Result<Profile>>()
        + 2 * size_of::<Profile>()
        + 64; // format tag, comparison and enum carriers
    wrapper + row_profile::source_bytes()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SelectorRole {
    FileMap = 1,
    Doublewrite = 2,
}
impl SelectorRole {
    fn target_kind(self) -> ControlKind {
        match self {
            Self::FileMap => ControlKind::FileMapRoot,
            Self::Doublewrite => ControlKind::Doublewrite,
        }
    }
    pub(crate) fn file_name(self) -> &'static str {
        match self {
            Self::FileMap => "FILEMAP",
            Self::Doublewrite => "DWB",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelectedControl {
    pub(crate) generation: u64,
    pub(crate) reference: ControlRef,
}
pub(crate) fn encode_selector(
    role: SelectorRole,
    selected: SelectedControl,
    profile: Profile,
    output: &mut [u8],
) -> Result<&[u8]> {
    selected.reference.validate()?;
    if selected.generation == 0 || selected.reference.kind != role.target_kind() {
        return Err(ControlError::InvalidInput);
    }
    if output.len() < SELECTOR_BYTES {
        return Err(ControlError::NoSpace);
    }
    output[..SELECTOR_BYTES].fill(0);
    output[..4].copy_from_slice(b"XCS1");
    put16(output, 4, 1);
    output[6] = role as u8;
    output[8..24].copy_from_slice(&profile.namespace.database.bytes());
    put64(output, 24, profile.namespace.timeline);
    put64(output, 32, selected.generation);
    put64(output, 40, profile.generation);
    output[48..80].copy_from_slice(&profile.digest);
    output[80..144].copy_from_slice(&selected.reference.bytes());
    seal(&mut output[..SELECTOR_BYTES]);
    Ok(&output[..SELECTOR_BYTES])
}
pub(crate) fn decode_selector(
    input: &[u8],
    role: SelectorRole,
    profile: Profile,
) -> Result<SelectedControl> {
    if input.len() != SELECTOR_BYTES || input[..4] != *b"XCS1" {
        return Err(ControlError::Corrupt);
    }
    if get16(input, 4) != 1 || !(1..=2).contains(&input[6]) {
        return Err(ControlError::UnsupportedFormat);
    }
    if input[7] != 0 || !zero(&input[144..4064]) || !checksum_valid(input) || get64(input, 32) == 0
    {
        return Err(ControlError::Corrupt);
    }
    let namespace = decode_namespace(input, 8, 24)?;
    if input[6] != role as u8
        || namespace != profile.namespace
        || get64(input, 40) != profile.generation
        || input[48..80] != profile.digest
    {
        return Err(ControlError::IdentityMismatch);
    }
    let reference = ControlRef::decode(&input[80..144])?;
    if reference.kind != role.target_kind() {
        return Err(ControlError::Corrupt);
    }
    Ok(SelectedControl {
        generation: get64(input, 32),
        reference,
    })
}

/// Checks the selected XDW1 bytes with their original decoder; the outer ref's
/// slot chooses a filename, whereas XDW1 image slot numbers choose image storage.
pub(crate) fn decode_selected_doublewrite(
    input: &[u8],
    reference: ControlRef,
    profile: Profile,
) -> Result<ControlView<'_>> {
    reference.validate()?;
    if reference.kind != ControlKind::Doublewrite {
        return Err(ControlError::InvalidInput);
    }
    bind_digest(input, reference)?;
    doublewrite_control::decode_control(
        input,
        ControlExpectation {
            namespace: profile.namespace,
            generation: reference.generation,
        },
    )
    .map_err(|error| match error {
        doublewrite_control::DoublewriteError::IdentityMismatch => ControlError::IdentityMismatch,
        doublewrite_control::DoublewriteError::UnsupportedFormat => ControlError::UnsupportedFormat,
        _ => ControlError::Corrupt,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapIdentity {
    pub(crate) namespace: Namespace,
    pub(crate) generation: u64,
    pub(crate) slot: u8,
}
impl MapIdentity {
    fn validate(self, profile: Profile) -> Result<()> {
        validate_namespace(self.namespace)?;
        if self.generation == 0 || self.slot > 1 || self.namespace != profile.namespace {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum FileState {
    Live = 1,
    Retained = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileMapEntry {
    pub(crate) file: FileId,
    pub(crate) mapped_bytes: u64,
    pub(crate) allocator: ObjectId,
    pub(crate) allocator_generation: Generation,
    pub(crate) state: FileState,
}
impl FileMapEntry {
    fn validate(self, quota: u64) -> Result<()> {
        if self.mapped_bytes == 0
            || !self.mapped_bytes.is_multiple_of(EXTENT_BYTES)
            || self.mapped_bytes > quota
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
    pub(crate) fn metadata_identity(
        self,
        namespace: Namespace,
        extent_base: u64,
    ) -> Result<PageIdentity> {
        validate_namespace(namespace)?;
        self.validate(OWNED_BYTES)?;
        let end = extent_base
            .checked_add(64)
            .and_then(|pages| pages.checked_mul(PAGE_BYTES as u64));
        if !extent_base.is_multiple_of(64) || end.is_none_or(|value| value > self.mapped_bytes) {
            return Err(ControlError::InvalidInput);
        }
        Ok(PageIdentity {
            database: namespace.database,
            page: PageRef {
                file: self.file,
                page_no: extent_base,
                incarnation: generation_one(),
            },
            owner: self.allocator,
            owner_generation: self.allocator_generation,
        })
    }
    fn bytes(self) -> [u8; ENTRY_BYTES] {
        let mut output = [0; ENTRY_BYTES];
        let file = file_number(self.file);
        put64(&mut output, 0, file);
        put64(&mut output, 8, self.mapped_bytes);
        put64(&mut output, 16, self.allocator.get());
        put64(&mut output, 24, self.allocator_generation.get());
        put64(&mut output, 32, file);
        put64(&mut output, 48, 1);
        output[56..80].copy_from_slice(&data_name_bytes(self.file));
        output[88] = self.state as u8;
        output
    }
    fn decode(input: &[u8], quota: u64) -> Result<Self> {
        if input.len() != ENTRY_BYTES || !zero(&input[80..88]) || !zero(&input[89..96]) {
            return Err(ControlError::Corrupt);
        }
        let entry = Self {
            file: FileId::new(get64(input, 0)).map_err(|_| ControlError::Corrupt)?,
            mapped_bytes: get64(input, 8),
            allocator: ObjectId::new(get64(input, 16)).map_err(|_| ControlError::Corrupt)?,
            allocator_generation: Generation::new(get64(input, 24))
                .map_err(|_| ControlError::Corrupt)?,
            state: match input[88] {
                1 => FileState::Live,
                2 => FileState::Retained,
                _ => return Err(ControlError::Corrupt),
            },
        };
        if input[56..80] != data_name_bytes(entry.file)
            || get64(input, 32) != file_number(entry.file)
            || get64(input, 40) != 0
            || get64(input, 48) != 1
        {
            return Err(ControlError::Corrupt);
        }
        entry.validate(quota).map_err(|_| ControlError::Corrupt)?;
        Ok(entry)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapSummary {
    pub(crate) files: u32,
    pub(crate) mapped_bytes: u64,
    pub(crate) first: Option<FileId>,
    pub(crate) last: Option<FileId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChunkDescriptor {
    pub(crate) reference: ControlRef,
    pub(crate) count: u16,
    pub(crate) first: FileId,
    pub(crate) last: FileId,
}
impl ChunkDescriptor {
    fn bytes(self) -> [u8; DESCRIPTOR_BYTES] {
        let mut output = [0; DESCRIPTOR_BYTES];
        output[..64].copy_from_slice(&self.reference.bytes());
        put16(&mut output, 64, self.count);
        put64(&mut output, 72, file_number(self.first));
        put64(&mut output, 80, file_number(self.last));
        output
    }
    fn decode(input: &[u8]) -> Result<Self> {
        if input.len() != DESCRIPTOR_BYTES || !zero(&input[66..72]) || !zero(&input[88..96]) {
            return Err(ControlError::Corrupt);
        }
        Ok(Self {
            reference: ControlRef::decode(&input[..64])?,
            count: get16(input, 64),
            first: FileId::new(get64(input, 72)).map_err(|_| ControlError::Corrupt)?,
            last: FileId::new(get64(input, 80)).map_err(|_| ControlError::Corrupt)?,
        })
    }
}

fn validate_root(
    identity: MapIdentity,
    summary: MapSummary,
    count: usize,
    descriptors: impl Iterator<Item = Result<ChunkDescriptor>>,
    profile: Profile,
) -> Result<()> {
    identity.validate(profile)?;
    if summary.files as usize > MAX_FILES
        || count != (summary.files as usize).div_ceil(CHUNK_ENTRIES)
        || summary.mapped_bytes > profile.owned_bytes()
        || (summary.files == 0
            && (summary.mapped_bytes != 0 || summary.first.is_some() || summary.last.is_some()))
        || (summary.files > 0 && summary.mapped_bytes < u64::from(summary.files) * EXTENT_BYTES)
        || !summary.mapped_bytes.is_multiple_of(EXTENT_BYTES)
    {
        return Err(ControlError::InvalidInput);
    }
    if summary.files > 0 {
        validate_range(
            summary.first.ok_or(ControlError::InvalidInput)?,
            summary.last.ok_or(ControlError::InvalidInput)?,
            summary.files as usize,
        )?;
    }
    let mut previous = None;
    let mut actual = 0;
    for (index, item) in descriptors.enumerate() {
        let descriptor = item?;
        descriptor.reference.validate()?;
        let expected_count = (summary.files as usize)
            .saturating_sub(index * CHUNK_ENTRIES)
            .min(CHUNK_ENTRIES);
        if index >= count
            || usize::from(descriptor.count) != expected_count
            || descriptor.reference.kind != ControlKind::FileMapChunk
            || descriptor.reference.slot != identity.slot
            || descriptor.reference.generation != identity.generation
            || descriptor.reference.index != index as u32
            || descriptor.reference.encoded_len as usize != 160 + ENTRY_BYTES * expected_count
            || previous.is_some_and(|last| last >= file_number(descriptor.first))
            || (index == 0 && summary.first != Some(descriptor.first))
        {
            return Err(ControlError::InvalidInput);
        }
        validate_range(descriptor.first, descriptor.last, expected_count)?;
        previous = Some(file_number(descriptor.last));
        actual += 1;
    }
    if actual != count || previous != summary.last.map(file_number) {
        return Err(ControlError::InvalidInput);
    }
    Ok(())
}

pub(crate) fn encode_filemap_root<'a>(
    identity: MapIdentity,
    summary: MapSummary,
    descriptors: &[ChunkDescriptor],
    profile: Profile,
    output: &'a mut [u8],
) -> Result<&'a [u8]> {
    validate_root(
        identity,
        summary,
        descriptors.len(),
        descriptors.iter().copied().map(Ok),
        profile,
    )?;
    let length = 224 + DESCRIPTOR_BYTES * descriptors.len();
    if output.len() < length {
        return Err(ControlError::NoSpace);
    }
    output[..length].fill(0);
    write_header(
        output,
        ControlKind::FileMapRoot,
        identity,
        0,
        descriptors.len() as u32,
        length,
    );
    put32(output, 128, summary.files);
    put32(output, 132, descriptors.len() as u32);
    put64(output, 136, summary.mapped_bytes);
    put64(output, 144, summary.first.map_or(0, file_number));
    put64(output, 152, summary.last.map_or(0, file_number));
    put16(output, 160, CHUNK_ENTRIES as u16);
    put16(output, 162, ENTRY_BYTES as u16);
    for (index, descriptor) in descriptors.iter().enumerate() {
        let start = 192 + DESCRIPTOR_BYTES * index;
        output[start..start + DESCRIPTOR_BYTES].copy_from_slice(&descriptor.bytes());
    }
    seal(&mut output[..length]);
    Ok(&output[..length])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileMapRoot<'a> {
    bytes: &'a [u8],
    identity: MapIdentity,
    summary: MapSummary,
    chunks: usize,
    profile: Profile,
}
impl FileMapRoot<'_> {
    pub(crate) fn identity(self) -> MapIdentity {
        self.identity
    }
    pub(crate) fn summary(self) -> MapSummary {
        self.summary
    }
    pub(crate) fn len(self) -> usize {
        self.chunks
    }
    pub(crate) fn descriptor(self, index: usize) -> Option<ChunkDescriptor> {
        if index >= self.chunks {
            return None;
        }
        let start = 192 + index * DESCRIPTOR_BYTES;
        Some(
            ChunkDescriptor::decode(&self.bytes[start..start + DESCRIPTOR_BYTES])
                .expect("immutable checked descriptor"),
        )
    }
    /// Returns only the selected range; the caller still must validate its bytes.
    pub(crate) fn find_chunk(self, file: FileId) -> Option<ChunkDescriptor> {
        let target = file_number(file);
        let (mut low, mut high) = (0, self.chunks);
        while low < high {
            let mid = low + (high - low) / 2;
            let descriptor = self.descriptor(mid).expect("bounded descriptor index");
            if target < file_number(descriptor.first) {
                high = mid;
            } else if target > file_number(descriptor.last) {
                low = mid + 1;
            } else {
                return Some(descriptor);
            }
        }
        None
    }
}

pub(crate) fn decode_filemap_root(
    input: &[u8],
    reference: ControlRef,
    profile: Profile,
) -> Result<FileMapRoot<'_>> {
    let header = bind_header(input, reference, profile, ControlKind::FileMapRoot)?;
    if get32(input, 132) != header.count
        || get16(input, 160) != 64
        || get16(input, 162) != 96
        || !zero(&input[164..192])
    {
        return Err(ControlError::Corrupt);
    }
    let summary = MapSummary {
        files: get32(input, 128),
        mapped_bytes: get64(input, 136),
        first: optional_file(get64(input, 144))?,
        last: optional_file(get64(input, 152))?,
    };
    let descriptors = input[192..input.len() - 32]
        .chunks_exact(DESCRIPTOR_BYTES)
        .map(ChunkDescriptor::decode);
    validate_root(
        header.identity,
        summary,
        header.count as usize,
        descriptors,
        profile,
    )
    .map_err(|_| ControlError::Corrupt)?;
    Ok(FileMapRoot {
        bytes: input,
        identity: header.identity,
        summary,
        chunks: header.count as usize,
        profile,
    })
}

fn validate_entries(
    count: usize,
    entries: impl Iterator<Item = Result<FileMapEntry>>,
    quota: u64,
) -> Result<MapSummary> {
    if count == 0 || count > CHUNK_ENTRIES {
        return Err(ControlError::InvalidInput);
    }
    let mut summary = MapSummary {
        files: 0,
        mapped_bytes: 0,
        first: None,
        last: None,
    };
    for item in entries {
        let entry = item?;
        entry.validate(quota)?;
        if summary
            .last
            .is_some_and(|last| file_number(last) >= file_number(entry.file))
        {
            return Err(ControlError::InvalidInput);
        }
        summary.mapped_bytes = summary
            .mapped_bytes
            .checked_add(entry.mapped_bytes)
            .filter(|sum| *sum <= quota)
            .ok_or(ControlError::InvalidInput)?;
        summary.first = summary.first.or(Some(entry.file));
        summary.last = Some(entry.file);
        summary.files += 1;
    }
    if summary.files as usize != count {
        return Err(ControlError::InvalidInput);
    }
    Ok(summary)
}

pub(crate) fn encode_filemap_chunk<'a>(
    identity: MapIdentity,
    index: u32,
    entries: &[FileMapEntry],
    profile: Profile,
    output: &'a mut [u8],
) -> Result<&'a [u8]> {
    identity.validate(profile)?;
    if index >= MAX_CHUNKS as u32 {
        return Err(ControlError::InvalidInput);
    }
    validate_entries(
        entries.len(),
        entries.iter().copied().map(Ok),
        profile.owned_bytes(),
    )?;
    let length = 160 + ENTRY_BYTES * entries.len();
    if output.len() < length {
        return Err(ControlError::NoSpace);
    }
    output[..length].fill(0);
    write_header(
        output,
        ControlKind::FileMapChunk,
        identity,
        index,
        entries.len() as u32,
        length,
    );
    for (index, entry) in entries.iter().enumerate() {
        let start = 128 + ENTRY_BYTES * index;
        output[start..start + ENTRY_BYTES].copy_from_slice(&entry.bytes());
    }
    seal(&mut output[..length]);
    Ok(&output[..length])
}

#[derive(Debug)]
pub(crate) struct FileMapChunk<'a> {
    bytes: &'a [u8],
    identity: MapIdentity,
    index: u32,
    summary: MapSummary,
}
impl FileMapChunk<'_> {
    pub(crate) fn identity(&self) -> MapIdentity {
        self.identity
    }
    pub(crate) fn index(&self) -> u32 {
        self.index
    }
    pub(crate) fn summary(&self) -> MapSummary {
        self.summary
    }
    pub(crate) fn entry(&self, index: usize) -> Option<FileMapEntry> {
        if index >= self.summary.files as usize {
            return None;
        }
        let start = 128 + ENTRY_BYTES * index;
        Some(
            FileMapEntry::decode(&self.bytes[start..start + ENTRY_BYTES], OWNED_BYTES)
                .expect("immutable checked entry"),
        )
    }
}
pub(crate) fn decode_filemap_chunk(
    input: &[u8],
    reference: ControlRef,
    profile: Profile,
) -> Result<FileMapChunk<'_>> {
    let header = bind_header(input, reference, profile, ControlKind::FileMapChunk)?;
    let entries = input[128..input.len() - 32]
        .chunks_exact(ENTRY_BYTES)
        .map(|bytes| FileMapEntry::decode(bytes, profile.owned_bytes()));
    let summary = validate_entries(header.count as usize, entries, profile.owned_bytes())
        .map_err(|_| ControlError::Corrupt)?;
    Ok(FileMapChunk {
        bytes: input,
        identity: header.identity,
        index: header.index,
        summary,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChunkLocation {
    pub(crate) reference: ControlRef,
    pub(crate) offset: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileMapCursor<'a> {
    root: FileMapRoot<'a>,
    next: usize,
    files: u32,
    mapped_bytes: u64,
    last: Option<FileId>,
}
impl<'a> FileMapCursor<'a> {
    pub(crate) fn new(root: FileMapRoot<'a>) -> Self {
        Self {
            root,
            next: 0,
            files: 0,
            mapped_bytes: 0,
            last: None,
        }
    }
    pub(crate) fn next(&self) -> Option<ChunkLocation> {
        self.root
            .descriptor(self.next)
            .map(|descriptor| ChunkLocation {
                reference: descriptor.reference,
                offset: PAGE_BYTES as u64 * (1 + self.next as u64),
            })
    }
    /// The full incoming chunk and tentative aggregate are checked before mutation.
    pub(crate) fn accept<'b>(&mut self, input: &'b [u8]) -> Result<FileMapChunk<'b>> {
        let descriptor = self
            .root
            .descriptor(self.next)
            .ok_or(ControlError::Corrupt)?;
        let chunk = decode_filemap_chunk(input, descriptor.reference, self.root.profile)?;
        if chunk.summary.files != u32::from(descriptor.count)
            || chunk.summary.first != Some(descriptor.first)
            || chunk.summary.last != Some(descriptor.last)
            || self
                .last
                .is_some_and(|last| file_number(last) >= file_number(descriptor.first))
        {
            return Err(ControlError::Corrupt);
        }
        let files = self
            .files
            .checked_add(chunk.summary.files)
            .filter(|files| *files <= self.root.summary.files)
            .ok_or(ControlError::Corrupt)?;
        let mapped_bytes = self
            .mapped_bytes
            .checked_add(chunk.summary.mapped_bytes)
            .filter(|sum| *sum <= self.root.summary.mapped_bytes)
            .ok_or(ControlError::Corrupt)?;
        if self.next + 1 == self.root.chunks
            && (files != self.root.summary.files
                || mapped_bytes != self.root.summary.mapped_bytes
                || chunk.summary.last != self.root.summary.last)
        {
            return Err(ControlError::Corrupt);
        }
        self.next += 1;
        self.files = files;
        self.mapped_bytes = mapped_bytes;
        self.last = chunk.summary.last;
        Ok(chunk)
    }
    pub(crate) fn finish(&self) -> Result<()> {
        if self.next != self.root.chunks
            || self.files != self.root.summary.files
            || self.mapped_bytes != self.root.summary.mapped_bytes
            || self.last != self.root.summary.last
        {
            return Err(ControlError::Incomplete);
        }
        Ok(())
    }
}

pub(crate) fn data_file_name(file: FileId) -> FileName {
    let bytes = data_name_bytes(file);
    FileName::new(std::str::from_utf8(&bytes).expect("constant prefix and hex ASCII"))
        .expect("fixed canonical filename")
}
pub(crate) fn control_file_name(reference: ControlRef) -> Result<FileName> {
    reference.validate()?;
    let name = match (reference.kind, reference.slot) {
        (ControlKind::Profile, _) => "d3-profile",
        (ControlKind::FileMapRoot | ControlKind::FileMapChunk, 0) => "d3-filemap-0",
        (ControlKind::FileMapRoot | ControlKind::FileMapChunk, _) => "d3-filemap-1",
        (ControlKind::Doublewrite, 0) => "d3-dwb-0",
        (ControlKind::Doublewrite, _) => "d3-dwb-1",
        (ControlKind::OwnershipRoot | ControlKind::OwnershipChunk, _) => {
            return Err(ControlError::UnsupportedFormat);
        }
    };
    Ok(FileName::new(name).expect("fixed canonical filename"))
}
pub(crate) fn next_generation(current: u64) -> Result<u64> {
    if current == 0 {
        return Err(ControlError::InvalidInput);
    }
    current.checked_add(1).ok_or(ControlError::Exhausted)
}

#[derive(Clone, Copy)]
struct Envelope {
    kind: ControlKind,
    identity: MapIdentity,
    index: u32,
    count: u32,
}
fn read_header(input: &[u8]) -> Result<Envelope> {
    read_header_for(input, EnvelopeVersion::V1)
}
/// One byte, replacing the original boolean in the same decoder frame.
/// This is an input grammar selection, not current-profile authority.
#[derive(Clone, Copy)]
#[repr(u8)]
enum EnvelopeVersion {
    V1,
    LegacyProfile,
    RowProfile,
}

fn read_header_for(input: &[u8], version: EnvelopeVersion) -> Result<Envelope> {
    if input.len() < 160 || input[..4] != *b"XDC1" {
        return Err(ControlError::Corrupt);
    }
    if !match version {
        EnvelopeVersion::V1 => get16(input, 4) == 1,
        EnvelopeVersion::LegacyProfile => {
            get16(input, 4) == 1 || get16(input, 4) == 2 && input[6] == ControlKind::Profile as u8
        }
        EnvelopeVersion::RowProfile => {
            get16(input, 4) == 3 && input[6] == ControlKind::Profile as u8
        }
    } {
        return Err(ControlError::UnsupportedFormat);
    }
    let kind = ControlKind::decode(input[6])?;
    if kind == ControlKind::Doublewrite {
        return Err(ControlError::UnsupportedFormat);
    }
    let count = get32(input, 52);
    let length = match kind {
        ControlKind::Profile => PROFILE_BYTES,
        ControlKind::FileMapRoot if count <= MAX_CHUNKS as u32 => {
            224 + DESCRIPTOR_BYTES * count as usize
        }
        ControlKind::FileMapChunk if (1..=64).contains(&count) => {
            160 + ENTRY_BYTES * count as usize
        }
        _ => return Err(ControlError::Corrupt),
    };
    let index = get32(input, 48);
    if input.len() != length
        || get32(input, 12) as usize != length
        || get32(input, 56) as usize != length - 160
        || get16(input, 8) != 128
        || get16(input, 10) != 0
        || !zero(&input[60..128])
        || input[7] > 1
        || get64(input, 40) == 0
        || (kind == ControlKind::Profile && (input[7] != 0 || count != 0))
        || (kind == ControlKind::FileMapChunk && index >= MAX_CHUNKS as u32)
        || (kind != ControlKind::FileMapChunk && index != 0)
        || !checksum_valid(input)
    {
        return Err(ControlError::Corrupt);
    }
    Ok(Envelope {
        kind,
        identity: MapIdentity {
            namespace: decode_namespace(input, 16, 32)?,
            generation: get64(input, 40),
            slot: input[7],
        },
        index,
        count,
    })
}
fn bind_header(
    input: &[u8],
    reference: ControlRef,
    profile: Profile,
    kind: ControlKind,
) -> Result<Envelope> {
    reference.validate()?;
    if reference.kind != kind {
        return Err(ControlError::InvalidInput);
    }
    bind_digest(input, reference)?;
    let header = read_header(input)?;
    if header.kind != kind
        || header.identity.namespace != profile.namespace
        || header.identity.slot != reference.slot
        || header.identity.generation != reference.generation
        || header.index != reference.index
    {
        return Err(ControlError::IdentityMismatch);
    }
    Ok(header)
}
fn bind_digest(input: &[u8], reference: ControlRef) -> Result<()> {
    if input.len() != reference.encoded_len as usize || content_digest(input) != reference.digest {
        return Err(ControlError::IdentityMismatch);
    }
    Ok(())
}
fn write_header(
    output: &mut [u8],
    kind: ControlKind,
    identity: MapIdentity,
    index: u32,
    count: u32,
    length: usize,
) {
    output[..4].copy_from_slice(b"XDC1");
    put16(output, 4, 1);
    output[6] = kind as u8;
    output[7] = identity.slot;
    put16(output, 8, 128);
    put32(output, 12, length as u32);
    output[16..32].copy_from_slice(&identity.namespace.database.bytes());
    put64(output, 32, identity.namespace.timeline);
    put64(output, 40, identity.generation);
    put32(output, 48, index);
    put32(output, 52, count);
    put32(output, 56, (length - 160) as u32);
}
fn validate_namespace(namespace: Namespace) -> Result<()> {
    if namespace.timeline == 0 {
        Err(ControlError::InvalidInput)
    } else {
        Ok(())
    }
}
fn decode_namespace(input: &[u8], db_offset: usize, timeline_offset: usize) -> Result<Namespace> {
    let namespace = Namespace {
        database: DatabaseId::new(
            input[db_offset..db_offset + 16]
                .try_into()
                .expect("checked header"),
        )
        .map_err(|_| ControlError::Corrupt)?,
        timeline: get64(input, timeline_offset),
    };
    validate_namespace(namespace).map_err(|_| ControlError::Corrupt)?;
    Ok(namespace)
}
fn validate_range(first: FileId, last: FileId, count: usize) -> Result<()> {
    if count == 0
        || (count == 1 && first != last)
        || (count > 1 && file_number(first) >= file_number(last))
    {
        Err(ControlError::InvalidInput)
    } else {
        Ok(())
    }
}
fn optional_file(value: u64) -> Result<Option<FileId>> {
    if value == 0 {
        Ok(None)
    } else {
        FileId::new(value)
            .map(Some)
            .map_err(|_| ControlError::Corrupt)
    }
}
fn generation_one() -> Generation {
    Generation::new(1).expect("nonzero constant")
}
fn file_number(file: FileId) -> u64 {
    let mut bytes = [0; 24];
    PageRef {
        file,
        page_no: 0,
        incarnation: generation_one(),
    }
    .encode(&mut bytes)
    .expect("fixed reference width");
    get64(&bytes, 0)
}
fn data_name_bytes(file: FileId) -> [u8; 24] {
    let mut output = [0; 24];
    output[..8].copy_from_slice(b"d3-data-");
    let number = file_number(file);
    for index in 0..16 {
        output[8 + index] = b"0123456789abcdef"[((number >> (4 * (15 - index))) & 15) as usize];
    }
    output
}
fn zero(input: &[u8]) -> bool {
    input.iter().all(|byte| *byte == 0)
}
fn get16(input: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        input[offset..offset + 2]
            .try_into()
            .expect("checked length"),
    )
}
fn get32(input: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        input[offset..offset + 4]
            .try_into()
            .expect("checked length"),
    )
}
fn get64(input: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        input[offset..offset + 8]
            .try_into()
            .expect("checked length"),
    )
}
fn put16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn checksum_valid(input: &[u8]) -> bool {
    input[input.len() - 32..] == content_digest(&input[..input.len() - 32])
}
fn seal(output: &mut [u8]) {
    let end = output.len() - 32;
    let digest = content_digest(&output[..end]);
    output[end..].copy_from_slice(&digest);
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "control/installed_profile_tests.rs"]
mod installed_profile_tests;
