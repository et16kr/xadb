//! Exact finite namespace and explicit CURRENT closure selection. No directory
//! generation search, implicit fallback, or deletion permission is produced.
use super::*;
use crate::storage::{
    control::{ControlKind, SelectorRole},
    page_io::PageFileReader,
};
use foundation::namespace::{DirectoryEntry, NamespaceErrorKind, NamespaceResult};

const FIXED: [(&str, u64); 14] = [
    ("LOCK", 0),
    ("d3-profile", 4096),
    ("WRITER", 4096),
    ("FILEMAP", 4096),
    ("d3-bootstrap", 4096),
    ("d3-filemap-0", control::CONTAINER_BYTES),
    ("d3-filemap-1", control::CONTAINER_BYTES),
    (HOME_NAME, control::EXTENT_BYTES),
    ("DWB", 4096),
    ("d3-dwb-0", PAGE_BYTES as u64),
    ("d3-dwb-1", PAGE_BYTES as u64),
    ("d3-dwb-stage", PAGE_BYTES as u64),
    ("d3-dwb-images", 16_711_680),
    ("d3-dwb-reserved", 8192),
];
const STAGES: [&str; 3] = ["d3-current-stage", "d3-writer-stage", "d3-dwb-select-stage"];

pub(super) fn epoch_name(epoch: u32) -> FileName {
    let mut bytes = *b"d3-wal-00000000";
    let hex = b"0123456789abcdef";
    for i in 0..8 {
        bytes[7 + i] = hex[((epoch >> (4 * (7 - i))) & 15) as usize];
    }
    name(std::str::from_utf8(&bytes).expect("fixed ASCII epoch"))
}
fn parse_epoch(value: &str) -> Option<u32> {
    let bytes = value.as_bytes();
    if bytes.len() != 15 || &bytes[..7] != b"d3-wal-" {
        return None;
    }
    let mut epoch = 0u32;
    for byte in &bytes[7..] {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => return None,
        };
        epoch = (epoch << 4) | u32::from(digit);
    }
    (epoch != 0 && epoch_name(epoch).as_str() == value).then_some(epoch)
}
pub(super) fn metadata<W: WriterNamespace>(
    w: &W,
    o: &mut Plan4RecoveryObservation,
    path: FileName,
) -> Result<Option<FileMetadata>> {
    o.begin(RecoveryCallKind::Metadata, path, 0, 0)?;
    let found = w
        .metadata(&path)
        .map_err(|e| error(o.phase, Plan4RecoveryCause::Namespace(e)))?;
    o.done(RecoveryCallKind::Metadata);
    Ok(found)
}
pub(super) fn open<W: WriterNamespace>(
    w: &mut W,
    o: &mut Plan4RecoveryObservation,
    path: FileName,
    mode: OpenMode,
    length: Option<u64>,
) -> Result<(W::File, FileMetadata)> {
    o.begin(RecoveryCallKind::Open, path, 0, length.unwrap_or(0))?;
    let (file, opened) = w
        .open_existing_with_metadata(&path, mode)
        .map_err(|e| error(o.phase, Plan4RecoveryCause::Namespace(e)))?;
    o.done(RecoveryCallKind::Open);
    if length.is_some_and(|n| n != opened.length) {
        return Err(error(o.phase, Plan4RecoveryCause::LengthMismatch));
    }
    if metadata(w, o, path)? != Some(opened) {
        return Err(error(o.phase, Plan4RecoveryCause::IdentityMismatch));
    }
    Ok((file, opened))
}
pub(super) fn read<F: FileIo>(
    file: &mut F,
    o: &mut Plan4RecoveryObservation,
    path: FileName,
    offset: u64,
    out: &mut [u8],
) -> Result<()> {
    o.begin(RecoveryCallKind::Read, path, offset, out.len() as u64)?;
    read_exact_at(file, out, offset).map_err(|e| error(o.phase, Plan4RecoveryCause::Io(e)))?;
    o.done(RecoveryCallKind::Read);
    Ok(())
}
pub(super) fn read_named<W: WriterNamespace>(
    w: &mut W,
    o: &mut Plan4RecoveryObservation,
    path: FileName,
    out: &mut [u8],
) -> Result<FileMetadata> {
    let (mut file, opened) = open(w, o, path, OpenMode::ReadOnly, Some(out.len() as u64))?;
    read(&mut file, o, path, 0, out)?;
    if metadata(w, o, path)? != Some(opened) {
        return Err(error(o.phase, Plan4RecoveryCause::IdentityMismatch));
    }
    Ok(opened)
}

/// Order-independent complete named identity observation with a bounded22
/// entry namespace result. No generation/name grants deletion authority.
fn inventory_digest<W: WriterNamespace>(
    w: &W,
    o: &mut Plan4RecoveryObservation,
) -> Result<[u8; 32]> {
    o.begin(RecoveryCallKind::Inventory, name("source-seal"), 0, 22)?;
    let entries = w
        .list_owned(22)
        .map_err(|e| error(o.phase, Plan4RecoveryCause::Namespace(e)))?;
    o.done(RecoveryCallKind::Inventory);
    if entries.len() > 22 || entries.capacity() > 22 {
        return Err(error(o.phase, Plan4RecoveryCause::Capacity));
    }
    for (i, entry) in entries.iter().enumerate() {
        if entries[..i].iter().any(|old| old.name == entry.name) {
            return Err(error(o.phase, Plan4RecoveryCause::IdentityMismatch));
        }
    }
    let mut hash = ContentHasher::new();
    hash.update(&(entries.len() as u64).to_le_bytes());
    let mut previous: Option<FileName> = None;
    // At most22² comparisons; avoid a sorting implementation's extra stack or
    // another retained array merely to canonicalize this small observation.
    for _ in 0..entries.len() {
        let entry = entries
            .iter()
            .filter(|e| previous.is_none_or(|p| e.name > p))
            .min_by_key(|e| e.name)
            .ok_or_else(|| error(o.phase, Plan4RecoveryCause::Invariant))?;
        hash.update(&(entry.name.as_str().len() as u64).to_le_bytes());
        hash.update(entry.name.as_str().as_bytes());
        hash.update(&entry.metadata.identity.device.to_le_bytes());
        hash.update(&entry.metadata.identity.inode.to_le_bytes());
        hash.update(&entry.metadata.length.to_le_bytes());
        previous = Some(entry.name);
    }
    Ok(hash.digest())
}

pub(super) fn stable_inventory<W: WriterNamespace>(
    w: &W,
    o: &mut Plan4RecoveryObservation,
    epoch: FileName,
    profile: Profile,
) -> Result<()> {
    let p = o.phase;
    o.begin(RecoveryCallKind::Inventory, name("inventory"), 0, 22)?;
    let entries = w
        .list_owned(22)
        .map_err(|e| error(p, Plan4RecoveryCause::Namespace(e)))?;
    o.done(RecoveryCallKind::Inventory);
    if entries.len() > 22 || entries.capacity() > 22 {
        return Err(error(p, Plan4RecoveryCause::Capacity));
    }
    let mut bytes = 0u64;
    let mut wal = 0u64;
    for (index, entry) in entries.iter().enumerate() {
        if entries[..index].iter().any(|old| old.name == entry.name) {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        let path = entry.name.as_str();
        let valid = if let Some((_, length)) = FIXED.iter().find(|(fixed, _)| *fixed == path) {
            entry.metadata.length == *length
        } else if BASE_NAMES.contains(&path) {
            entry.metadata.length == profile.base_bytes() as u64
        } else if path == "CURRENT" {
            entry.metadata.length == CONTROL_BYTES as u64
        } else if STAGES.contains(&path) {
            entry.metadata.length <= CONTROL_BYTES as u64
        } else if entry.name == epoch {
            wal = entry.metadata.length;
            (CONTROL_BYTES as u64..=WAL_CAP).contains(&wal)
        } else {
            return Err(error(p, Plan4RecoveryCause::Unsupported));
        };
        if !valid {
            return Err(error(p, Plan4RecoveryCause::LengthMismatch));
        }
        bytes = bytes
            .checked_add(entry.metadata.length)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
    }
    for (path, _) in FIXED {
        if !entries.iter().any(|e| e.name.as_str() == path) {
            return Err(error(p, Plan4RecoveryCause::Missing));
        }
    }
    for path in [BASE_NAMES[0], BASE_NAMES[1], "CURRENT", epoch.as_str()] {
        if !entries.iter().any(|e| e.name.as_str() == path) {
            return Err(error(p, Plan4RecoveryCause::Missing));
        }
    }
    if bytes > profile.owned_bytes() || wal == 0 {
        return Err(error(p, Plan4RecoveryCause::Capacity));
    }
    o.actual_owned = bytes;
    o.actual_wal = wal;
    o.inventory_capacity = entries.capacity();
    Ok(())
}

/// Whether a failed CURRENT closure may fall back to its previous closure:
/// only a missing or content-mismatched input and a corrupt base, whether the
/// base codec's own `Base(Corrupt)` or the base4 Row graph's
/// `RowBase(Corrupt)`. Unsupported semantics, mismatched valid identity and IO
/// uncertainty are never hidden by an older fallback.
pub(super) fn current_may_fall_back(cause: &Plan4RecoveryCause) -> bool {
    matches!(
        cause,
        Plan4RecoveryCause::Missing
            | Plan4RecoveryCause::ContentMismatch
            | Plan4RecoveryCause::Base(base::Error::Corrupt)
            | Plan4RecoveryCause::RowBase(base::row::RowBaseError::Corrupt)
    )
}

impl<W: WriterNamespace> RecoveryState<'_, '_, '_, W> {
    pub(super) fn current(&self) -> Result<Option<record::Current>> {
        if !self.current_present {
            return Ok(None);
        }
        record::decode_current(
            &self.maintenance[CURRENT_AT..CURRENT_AT + CONTROL_BYTES],
            self.profile(),
        )
        .map(Some)
        .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::Record(e)))
    }
    pub(super) fn selected_closure(&self) -> Result<Option<record::Closure>> {
        Ok(self.current()?.map(|c| {
            if self.selected_previous {
                c.previous
            } else {
                c.current
            }
        }))
    }
    pub(super) fn selected_base_view(&mut self) -> Result<base::row::RowBaseView<'_>> {
        if self.selected_base.is_none() {
            return Err(error(self.observation.phase, Plan4RecoveryCause::Invariant));
        }
        let expected = self.base_expected();
        let phase = self.observation.phase;
        let (prior, tail) = self.maintenance.split_at_mut(BASE_SCRATCH_START);
        let scratch: &mut [[u8; PAGE_BYTES]; 4] = tail[..4 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("base scratch");
        let view = base::row::decode(&prior[..BASE_BYTES], expected, scratch)
            .map_err(|e| error(phase, row_base(e)))?;
        if view.header().format != base::BaseFormat::V4RowMissing {
            return Err(error(phase, Plan4RecoveryCause::Unsupported));
        }
        Ok(view)
    }
    pub(super) fn select(&mut self) -> Result<()> {
        self.observation.phase = Plan4RecoveryPhase::Selection;
        let stamp = inventory_digest(&self.writer, &mut self.observation)?;
        self.maintenance[SOURCE_STAMP_AT..SOURCE_STAMP_AT + 32].copy_from_slice(&stamp);
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("d3-profile"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        // The current closure is exactly Profile3: every other profile version
        // is the Row decoder's UnsupportedFormat, never a legacy retry.
        let profile = control::row_profile::decode_profile(
            &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            self.expected_namespace,
        )
        .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::Control(e)))?;
        if profile.format() != 3
            || profile.spec().generation != 1
            || profile.spec().namespace.timeline != 1
        {
            return Err(error(
                self.observation.phase,
                Plan4RecoveryCause::Unsupported,
            ));
        }
        self.profile = Some(profile);
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("FILEMAP"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        let filemap = control::decode_selector(
            &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            SelectorRole::FileMap,
            profile,
        )
        .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::Control(e)))?;
        self.filemap = Some(filemap);
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("d3-bootstrap"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        self.bootstrap_digest = content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES]);
        {
            let scratch: &mut [[u8; PAGE_BYTES]; 3] = self.maintenance
                [IO_WORK..IO_WORK + 3 * PAGE_BYTES]
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("bootstrap scratch");
            super::super::super::super::plan4_genesis::inspect_bootstrap(profile, filemap, scratch)
                .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::Genesis(e)))?;
            // Map validation must not interpret an allocator home before repair.
            let plan = BootstrapPlan::prepare_for_kind(
                profile,
                scratch,
                BootstrapPlanKind::RetentionCatalogBootstrap,
            )
            .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::Bootstrap(e)))?;
            let allocator = plan
                .allocation_identity()
                .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::Control(e)))?;
            let [root, work, home] = scratch;
            let mut checked = CheckedRead(&mut self.writer);
            let mut reader = PageFileReader::open_profile(&mut checked, profile, root, work)
                .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::PageIo(e)))?;
            if reader.profile() != profile || reader.selected() != filemap {
                return Err(error(
                    self.observation.phase,
                    Plan4RecoveryCause::IdentityMismatch,
                ));
            }
            reader
                .read_retention_home(
                    allocator,
                    crate::storage::page::PageKind::Allocation,
                    0,
                    allocator,
                    home,
                )
                .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::PageIo(e)))?;
        }
        self.select_doublewrite()?;
        if metadata(&self.writer, &mut self.observation, name("CURRENT"))?.is_some() {
            read_named(
                &mut self.writer,
                &mut self.observation,
                name("CURRENT"),
                &mut self.maintenance[CURRENT_AT..CURRENT_AT + CONTROL_BYTES],
            )?;
            self.current_present = true;
            let current = self.current()?.expect("present CURRENT");
            match self.try_closure(current.current) {
                Ok(()) => {}
                Err(first) => {
                    if !current_may_fall_back(&first.cause) || current.current == current.previous {
                        return Err(first);
                    }
                    self.try_closure(current.previous)?;
                    self.selected_previous = true;
                }
            }
        } else {
            self.select_genesis_source()?;
        }
        self.inventory()?;
        self.verify_pre_effect_controls()
    }

    /// The initial selected bytes and full named identity set must remain the
    /// same through analysis. Called after selection and before the first sync,
    /// write, publication or removal. Home repair has its separate before check.
    pub(super) fn verify_pre_effect_controls(&mut self) -> Result<()> {
        let p = self.observation.phase;
        let profile = self.profile();
        let stamp = inventory_digest(&self.writer, &mut self.observation)?;
        if self.maintenance[SOURCE_STAMP_AT..SOURCE_STAMP_AT + 32] != stamp {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("d3-profile"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        if control::row_profile::decode_profile(
            &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            Some(profile.spec().namespace),
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?
            != profile
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("d3-bootstrap"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        if content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES])
            != self.bootstrap_digest
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("FILEMAP"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        if Some(
            control::decode_selector(
                &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
                SelectorRole::FileMap,
                profile,
            )
            .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?,
        ) != self.filemap
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        for (path, at) in [("DWB", SELECTOR_AT), ("CURRENT", CURRENT_AT)] {
            if path == "CURRENT" && !self.current_present {
                continue;
            }
            read_named(
                &mut self.writer,
                &mut self.observation,
                name(path),
                &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            )?;
            if self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES]
                != self.maintenance[at..at + CONTROL_BYTES]
            {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
        }
        let selected = self.doublewrite.expect("selected DWB");
        read_named(
            &mut self.writer,
            &mut self.observation,
            name(if selected.reference.slot == 0 {
                "d3-dwb-0"
            } else {
                "d3-dwb-1"
            }),
            &mut self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES],
        )?;
        if self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES] != self.flush[..PAGE_BYTES] {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        if let Some(slot) = self.selected_base {
            read_named(
                &mut self.writer,
                &mut self.observation,
                name(BASE_NAMES[slot as usize]),
                &mut self.maintenance[BASE_BYTES..2 * BASE_BYTES],
            )?;
            if self.maintenance[..BASE_BYTES] != self.maintenance[BASE_BYTES..2 * BASE_BYTES] {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
        }
        if let Some(source) = self.source.as_ref() {
            let source_epoch = source.epoch;
            let header_digest = source.header_digest;
            let closure = self.selected_closure()?;
            if source_epoch == 1 || closure.is_some_and(|c| c.mode == record::Mode::WalOpen) {
                read_named(
                    &mut self.writer,
                    &mut self.observation,
                    name("WRITER"),
                    &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
                )?;
                if source_epoch == 1 {
                    wal::decode_writer(
                        &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
                        profile,
                        header_digest,
                    )
                    .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
                } else if content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES])
                    != closure.expect("WalOpen closure").writer_digest
                {
                    return Err(error(p, Plan4RecoveryCause::ContentMismatch));
                }
            }
        }
        {
            let scratch: &mut [[u8; PAGE_BYTES]; 3] = self.maintenance
                [IO_WORK..IO_WORK + 3 * PAGE_BYTES]
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("mapping scratch");
            let allocator = BootstrapPlan::prepare_for_kind(
                profile,
                scratch,
                BootstrapPlanKind::RetentionCatalogBootstrap,
            )
            .map_err(|e| error(p, Plan4RecoveryCause::Bootstrap(e)))?
            .allocation_identity()
            .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
            let [root, work, home] = scratch;
            let mut checked = CheckedRead(&mut self.writer);
            let mut reader = PageFileReader::open_profile(&mut checked, profile, root, work)
                .map_err(|e| error(p, Plan4RecoveryCause::PageIo(e)))?;
            if reader.profile() != profile || Some(reader.selected()) != self.filemap {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
            reader
                .read_retention_home(
                    allocator,
                    crate::storage::page::PageKind::Allocation,
                    0,
                    allocator,
                    home,
                )
                .map_err(|e| error(p, Plan4RecoveryCause::PageIo(e)))?;
        }
        let stamp = inventory_digest(&self.writer, &mut self.observation)?;
        if self.maintenance[SOURCE_STAMP_AT..SOURCE_STAMP_AT + 32] != stamp {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        Ok(())
    }
    fn select_doublewrite(&mut self) -> Result<()> {
        let profile = self.profile();
        let p = self.observation.phase;
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("DWB"),
            &mut self.maintenance[SELECTOR_AT..SELECTOR_AT + CONTROL_BYTES],
        )?;
        let selected = control::decode_selector(
            &self.maintenance[SELECTOR_AT..SELECTOR_AT + CONTROL_BYTES],
            SelectorRole::Doublewrite,
            profile,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        let r = selected.reference;
        if r.kind != ControlKind::Doublewrite
            || r.slot > 1
            || r.index != 0
            || r.encoded_len as usize > PAGE_BYTES
        {
            return Err(error(p, Plan4RecoveryCause::Unsupported));
        }
        let path = name(if r.slot == 0 { "d3-dwb-0" } else { "d3-dwb-1" });
        read_named(
            &mut self.writer,
            &mut self.observation,
            path,
            &mut self.flush[..PAGE_BYTES],
        )?;
        let view =
            control::decode_selected_doublewrite(&self.flush[..r.encoded_len as usize], r, profile)
                .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        let header = view.header();
        let middle = header
            .batch_id
            .checked_mul(3)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        let valid = match header.state {
            dwb::ControlState::Empty => {
                view.len() == 0
                    && (header.generation == middle.saturating_sub(1)
                        || middle.checked_add(1) == Some(header.generation))
            }
            dwb::ControlState::Pending => {
                view.len() == 1 && header.batch_id != 0 && header.generation == middle
            }
        };
        if !valid || r.slot != ((header.generation - 1) % 2) as u8 {
            return Err(error(p, Plan4RecoveryCause::Unsupported));
        }
        self.doublewrite = Some(selected);
        self.doublewrite_header = Some(view.header());
        Ok(())
    }
    fn try_closure(&mut self, closure: record::Closure) -> Result<()> {
        let p = self.observation.phase;
        let profile = self.profile();
        let mut predecessor = None;
        if closure.anchor == record::AnchorKind::Genesis {
            if closure.digest != self.bootstrap_digest {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            self.selected_base = None;
        } else {
            let path = name(BASE_NAMES[closure.slot as usize]);
            if metadata(&self.writer, &mut self.observation, path)?.is_none() {
                return Err(error(p, Plan4RecoveryCause::Missing));
            }
            read_named(
                &mut self.writer,
                &mut self.observation,
                path,
                &mut self.maintenance[..BASE_BYTES],
            )?;
            if content_digest(&self.maintenance[..BASE_BYTES]) != closure.digest {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
            self.selected_base = Some(closure.slot);
            let view = self.selected_base_view()?;
            if view.header().generation != closure.generation
                || view.header().capture_lsn != closure.lsn
            {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            predecessor = Some((
                view.header().source_epoch,
                view.header().source_header_digest,
            ));
        }
        if closure.mode == record::Mode::BaseOnly {
            if self.doublewrite_header.expect("selected DWB").state != dwb::ControlState::Empty {
                return Err(error(p, Plan4RecoveryCause::OrderMismatch));
            }
            self.source = None;
            return Ok(());
        }
        let path = epoch_name(closure.epoch);
        let (mut file, opened) = open(
            &mut self.writer,
            &mut self.observation,
            path,
            OpenMode::ReadOnly,
            None,
        )?;
        if !(CONTROL_BYTES as u64..=WAL_CAP).contains(&opened.length) {
            return Err(error(p, Plan4RecoveryCause::LengthMismatch));
        }
        read(
            &mut file,
            &mut self.observation,
            path,
            0,
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        let raw = &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES];
        let header_digest = content_digest(raw);
        if header_digest != closure.epoch_digest {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        if closure.epoch == 1 {
            wal::decode_epoch(raw, profile)
                .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        } else {
            let h = record::decode_epoch(raw, profile)
                .map_err(|e| error(p, Plan4RecoveryCause::Record(e)))?;
            if h.epoch != closure.epoch
                || h.base_generation != closure.generation
                || h.base_lsn != closure.lsn
                || h.base_digest != closure.digest
            {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            if predecessor != Some((h.previous_epoch, h.previous_header_digest))
                || h.epoch <= h.previous_epoch
            {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
        }
        if closure.mode == record::Mode::WalOpen {
            read_named(
                &mut self.writer,
                &mut self.observation,
                name("WRITER"),
                &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            )?;
            let raw = &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES];
            if content_digest(raw) != closure.writer_digest {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            let w = record::decode_writer(raw, profile)
                .map_err(|e| error(p, Plan4RecoveryCause::Record(e)))?;
            let current = self.current()?.expect("WalOpen has CURRENT");
            if w.generation != closure.writer_generation
                || w.epoch != closure.epoch
                || w.epoch_digest != header_digest
                || w.base_generation != closure.generation
                || w.base_lsn != closure.lsn
                || w.base_digest != closure.digest
                || w.active_generation != current.generation
            {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
        }
        self.source = Some(HeldSource {
            file,
            name: path,
            metadata: opened,
            epoch: closure.epoch,
            header_digest,
            anchor_lsn: closure.lsn,
            sealed: (closure.mode == record::Mode::WalSealed).then_some((
                closure.sealed_offset,
                closure.sealed_lsn,
                closure.sealed_digest,
            )),
        });
        Ok(())
    }
    fn select_genesis_source(&mut self) -> Result<()> {
        let p = self.observation.phase;
        let profile = self.profile();
        let path = name(wal::WAL_NAME);
        let (mut file, opened) = open(
            &mut self.writer,
            &mut self.observation,
            path,
            OpenMode::ReadOnly,
            None,
        )?;
        if opened.length <= CONTROL_BYTES as u64 || opened.length > WAL_CAP {
            return Err(error(p, Plan4RecoveryCause::Unsupported));
        }
        read(
            &mut file,
            &mut self.observation,
            path,
            0,
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        let digest =
            wal::decode_epoch(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES], profile)
                .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("WRITER"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        wal::decode_writer(
            &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            profile,
            digest,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        self.source = Some(HeldSource {
            file,
            name: path,
            metadata: opened,
            epoch: 1,
            header_digest: digest,
            anchor_lsn: 0,
            sealed: None,
        });
        Ok(())
    }
    pub(super) fn inventory(&mut self) -> Result<()> {
        let p = self.observation.phase;
        let current = self.current()?;
        let selected_epoch = self.source.as_ref().map(|s| s.epoch);
        let inherited_epoch = if self.selected_base.is_some() {
            Some(self.selected_base_view()?.header().source_epoch)
        } else {
            None
        };
        let intent_epoch = current.and_then(|c| c.intent.map(|i| i.epoch));
        self.observation
            .begin(RecoveryCallKind::Inventory, name("inventory"), 0, 22)?;
        let entries = self
            .writer
            .list_owned(22)
            .map_err(|e| error(p, Plan4RecoveryCause::Namespace(e)))?;
        self.observation.done(RecoveryCallKind::Inventory);
        if entries.len() > 22 || entries.capacity() > 22 {
            return Err(error(p, Plan4RecoveryCause::Capacity));
        }
        let mut owned = 0u64;
        let mut wal_bytes = 0u64;
        let mut wal_count = 0;
        for (i, entry) in entries.iter().enumerate() {
            if entries[..i].iter().any(|e| e.name == entry.name) {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            let path = entry.name.as_str();
            if let Some((_, length)) = FIXED.iter().find(|(n, _)| *n == path) {
                if entry.metadata.length != *length {
                    return Err(error(p, Plan4RecoveryCause::LengthMismatch));
                }
            } else if BASE_NAMES.contains(&path) {
                if entry.metadata.length > self.profile().base_bytes() as u64 {
                    return Err(error(p, Plan4RecoveryCause::LengthMismatch));
                }
            } else if path == "CURRENT" {
                if entry.metadata.length != CONTROL_BYTES as u64 {
                    return Err(error(p, Plan4RecoveryCause::LengthMismatch));
                }
            } else if STAGES.contains(&path) {
                if entry.metadata.length > CONTROL_BYTES as u64 {
                    return Err(error(p, Plan4RecoveryCause::LengthMismatch));
                }
            } else if let Some(epoch) = parse_epoch(path) {
                if !(Some(epoch) == selected_epoch
                    || Some(epoch) == inherited_epoch
                    || Some(epoch) == intent_epoch)
                    || (current.is_none() && epoch != 1)
                {
                    return Err(error(p, Plan4RecoveryCause::Unsupported));
                }
                // LOG_OPEN is reachable only after predecessor retirement. An
                // extra predecessor beside an attached continuation is HOLD,
                // not a second deletion capability inferred from numbering.
                if selected_epoch.is_some()
                    && Some(epoch) == inherited_epoch
                    && Some(epoch) != selected_epoch
                {
                    return Err(error(p, Plan4RecoveryCause::Unsupported));
                }
                wal_count += 1;
                wal_bytes = wal_bytes
                    .checked_add(entry.metadata.length)
                    .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
            } else {
                return Err(error(p, Plan4RecoveryCause::Unsupported));
            }
            owned = owned
                .checked_add(entry.metadata.length)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        }
        for (path, _) in FIXED {
            if path == "WRITER" && current.is_some_and(|c| c.current.mode != record::Mode::WalOpen)
            {
                continue;
            }
            if !entries.iter().any(|e| e.name.as_str() == path) {
                return Err(error(p, Plan4RecoveryCause::Missing));
            }
        }
        if wal_count > 2 || wal_bytes > WAL_CAP || owned > self.profile().owned_bytes() {
            return Err(error(p, Plan4RecoveryCause::Capacity));
        }
        self.observation.actual_owned = owned;
        self.observation.actual_wal = wal_bytes;
        self.observation.inventory_capacity = entries.capacity();
        Ok(())
    }
    pub(super) fn verify_selected_controls(&mut self) -> Result<()> {
        let p = Plan4RecoveryPhase::Verify;
        self.observation.phase = p;
        let profile = self.profile();
        if self.possible_current || self.possible_doublewrite {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        }
        let selected = self
            .selected_epoch
            .as_ref()
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        let current_digest = selected.current_digest;
        let writer_digest = selected.writer_digest;
        let epoch_digest = selected.header_digest;
        let epoch_name = selected.name;
        let epoch_stamp = selected.metadata;
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("d3-profile"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        if control::row_profile::decode_profile(
            &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            Some(profile.spec().namespace),
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?
            != profile
        {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        for (path, digest) in [
            ("d3-bootstrap", self.bootstrap_digest),
            ("CURRENT", current_digest),
            ("WRITER", writer_digest),
        ] {
            read_named(
                &mut self.writer,
                &mut self.observation,
                name(path),
                &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
            )?;
            if content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES]) != digest {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
        }
        read_named(
            &mut self.writer,
            &mut self.observation,
            name("FILEMAP"),
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        if Some(
            control::decode_selector(
                &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
                SelectorRole::FileMap,
                profile,
            )
            .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?,
        ) != self.filemap
        {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        {
            let scratch: &mut [[u8; PAGE_BYTES]; 3] = self.maintenance
                [IO_WORK..IO_WORK + 3 * PAGE_BYTES]
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("mapping scratch");
            let [root, work, _] = scratch;
            let mut checked = CheckedRead(&mut self.writer);
            let reader = PageFileReader::open_profile(&mut checked, profile, root, work)
                .map_err(|e| error(p, Plan4RecoveryCause::PageIo(e)))?;
            if reader.profile() != profile || Some(reader.selected()) != self.filemap {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
        }
        if read_named(
            &mut self.writer,
            &mut self.observation,
            epoch_name,
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )? != epoch_stamp
            || content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES]) != epoch_digest
        {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        let current = self.current()?.expect("selected CURRENT");
        for closure in [current.current, current.previous] {
            read_named(
                &mut self.writer,
                &mut self.observation,
                name(BASE_NAMES[closure.slot as usize]),
                &mut self.maintenance[..BASE_BYTES],
            )?;
            if content_digest(&self.maintenance[..BASE_BYTES]) != closure.digest
                || self.maintenance[..BASE_BYTES] != self.maintenance[BASE_BYTES..2 * BASE_BYTES]
            {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
        }
        self.inventory()
    }
}

/// PageFileReader's exclusive mode validates mapping but does not itself bind
/// every opened File to its named stamp. This closed read-only port does.
struct CheckedRead<'a, W: WriterNamespace>(&'a mut W);
fn denied() -> NamespaceError {
    NamespaceError::new(
        NamespaceErrorKind::InvalidName,
        Effect::Unchanged,
        "closed plan4 recovery mapping read",
    )
}
fn mapped_length(path: &FileName) -> NamespaceResult<u64> {
    match path.as_str() {
        "d3-profile" | "FILEMAP" => Ok(4096),
        "d3-filemap-0" | "d3-filemap-1" => Ok(control::CONTAINER_BYTES),
        HOME_NAME => Ok(control::EXTENT_BYTES),
        _ => Err(denied()),
    }
}
impl<W: WriterNamespace> WriterNamespace for CheckedRead<'_, W> {
    type File = W::File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        Err(denied())
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<Self::File> {
        Err(denied())
    }
    fn open_existing(&mut self, p: &FileName, m: OpenMode) -> NamespaceResult<Self::File> {
        self.open_existing_with_metadata(p, m).map(|v| v.0)
    }
    fn open_existing_with_metadata(
        &mut self,
        p: &FileName,
        m: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        if m != OpenMode::ReadOnly {
            return Err(denied());
        }
        let n = mapped_length(p)?;
        let (f, stamp) = self.0.open_existing_with_metadata(p, m)?;
        if stamp.length != n || self.0.metadata(p)? != Some(stamp) {
            return Err(NamespaceError::new(
                NamespaceErrorKind::InvalidType,
                Effect::Unchanged,
                "opened/named recovery mapping mismatch",
            ));
        }
        Ok((f, stamp))
    }
    fn metadata(&self, p: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        mapped_length(p)?;
        self.0.metadata(p)
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        Err(denied())
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        Err(denied())
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        Err(denied())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        Err(denied())
    }
}
