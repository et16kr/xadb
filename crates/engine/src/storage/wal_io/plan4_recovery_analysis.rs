//! Canonical complete-prefix analysis. The actual home is used solely as a
//! physical before-image to classify; it never seeds a semantic before state.
use super::super::plan4_pages::{
    observe_recovery_frame, retention_replay_bank, retention_replay_image, seed_recovery_replay,
};
use super::*;
use crate::storage::{
    allocation_page::{self, AllocationEntry},
    dirty_frame::TargetRequest,
    obligation::{self, DescriptorSpec, OwnerState, RemainingBudget, WorkPhase},
    page::{self, PageIdentity, PageKind, PageLsn, PageRef},
    retention_obligation,
    wal_frame::{self, FrameExpectation, SemanticFrameScan, TailPolicy},
};
use xadb_foundation::identity::{Generation, ObjectId};

const INIT_LSNS: usize = STATUS_START + 64 * base::STATUS_BYTES;
const TARGETS: usize = CERT_START + 64 * base::CERTIFICATE_BYTES;
const TARGET_BYTES: usize = 48;
const BANK_BYTES: usize = 65_536;
const RAM_ROW: usize = 1664;
const RAM_AGING: usize = 32;
const RAM_ALLOCATION: usize = 33;

pub(super) fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(
        bytes[at..at + 8]
            .try_into()
            .expect("bounded recovery scalar"),
    )
}
pub(super) fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn kind(index: usize) -> PageKind {
    match index {
        0 => PageKind::Allocation,
        1 => PageKind::RetentionRoot,
        2..=6 => PageKind::RetentionJobs,
        7 => PageKind::TreeAnchor,
        8 => PageKind::CatalogLeaf,
        _ => PageKind::TransactionStatus,
    }
}
fn object(index: usize) -> u64 {
    match index {
        0 => 1,
        1..=6 => 4,
        7..=8 => 5,
        _ => 2,
    }
}
pub(super) fn request(profile: Profile, index: usize, incarnation: Generation) -> TargetRequest {
    TargetRequest {
        identity: PageIdentity {
            database: profile.spec().namespace.database,
            page: PageRef {
                file: crate::storage::page::FileId::new(0x0000_0001_0000_0001)
                    .expect("fixed plan4 file"),
                page_no: index as u64,
                incarnation,
            },
            owner: ObjectId::new(object(index)).expect("fixed role"),
            owner_generation: Generation::new(1).expect("role generation"),
        },
        kind: kind(index),
        level: 0,
    }
}
pub(super) fn target_at(
    bytes: &[u8],
    profile: Profile,
    index: usize,
) -> Result<(TargetRequest, u64, [u8; 32])> {
    let at = TARGETS + index * TARGET_BYTES;
    let incarnation = Generation::new(u64_at(bytes, at + 8))
        .map_err(|_| error(Plan4RecoveryPhase::Verify, Plan4RecoveryCause::Invariant))?;
    Ok((
        request(profile, index, incarnation),
        u64_at(bytes, at),
        bytes[at + 16..at + 48].try_into().expect("target hash"),
    ))
}
fn note_target(table: &mut [u8], request: TargetRequest, image: &[u8; PAGE_BYTES]) -> Result<()> {
    let view = page::decode_page(image, request.identity, request.kind)
        .map_err(|e| error(Plan4RecoveryPhase::Analysis, Plan4RecoveryCause::Page(e)))?;
    let at = request.identity.page.page_no as usize * TARGET_BYTES;
    put64(table, at, view.header().lsn.0);
    put64(table, at + 8, request.identity.page.incarnation.get());
    table[at + 16..at + 48].copy_from_slice(&content_digest(image));
    Ok(())
}
fn scan_prefix(
    input: &[u8],
    expected: FrameExpectation,
    policy: TailPolicy,
) -> std::result::Result<SemanticFrameScan<'_>, wal_frame::WalError> {
    if input.get(7) == Some(&3) {
        wal_frame::scan_retention_prefix(input, expected, policy)
    } else {
        wal_frame::scan_semantic_prefix(input, expected, policy)
    }
}
pub(super) fn prefix_hash<F: FileIo>(
    source: &mut HeldSource<F>,
    o: &mut Plan4RecoveryObservation,
    end: u64,
    scratch: &mut [u8],
) -> Result<[u8; 32]> {
    let mut hash = ContentHasher::new();
    let mut offset = 0;
    while offset < end {
        let n = (end - offset).min(scratch.len() as u64) as usize;
        selection::read(&mut source.file, o, source.name, offset, &mut scratch[..n])?;
        hash.update(&scratch[..n]);
        offset += n as u64;
    }
    Ok(hash.digest())
}

/// Check a canonical image against the captured physical home. A match may
/// occur at an earlier adopted group than the final group; later images cannot
/// erase that historical proof. Snapshot repair has a separate bit.
fn observe_home(
    profile: Profile,
    index: usize,
    image: &[u8; PAGE_BYTES],
    home: &[u8; PAGE_BYTES],
    at_base: bool,
    initialized: bool,
    matched: &mut u64,
    repair: &mut u64,
) -> Result<()> {
    let p = Plan4RecoveryPhase::Analysis;
    let incarnation =
        Generation::new(u64_at(image, 40)).map_err(|_| error(p, Plan4RecoveryCause::Invariant))?;
    let req = request(profile, index, incarnation);
    let canonical = page::decode_page(image, req.identity, req.kind)
        .map_err(|e| error(p, Plan4RecoveryCause::Page(e)))?;
    if home == image {
        *matched |= 1u64 << index;
        return Ok(());
    }
    match page::decode_page(home, req.identity, req.kind) {
        Ok(view) => {
            if view.header().level != 0 {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            if at_base {
                if view.header().lsn == canonical.header().lsn {
                    return Err(error(
                        p,
                        if canonical.header().lsn.0 == 0 {
                            Plan4RecoveryCause::Unsupported
                        } else {
                            Plan4RecoveryCause::ContentMismatch
                        },
                    ));
                }
                if view.header().lsn.0 < canonical.header().lsn.0 {
                    *repair |= 1u64 << index;
                }
            }
        }
        Err(
            PageError::IdentityMismatch | PageError::KindMismatch | PageError::UnsupportedFormat,
        ) => {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        Err(_) if at_base || (initialized && index >= 9) => {
            *repair |= 1u64 << index;
        }
        Err(_) => {}
    }
    Ok(())
}

impl<W: WriterNamespace> RecoveryState<'_, '_, '_, W> {
    pub(super) fn preinspect(&mut self) -> Result<()> {
        let p = Plan4RecoveryPhase::Analysis;
        self.observation.phase = p;
        let profile = self.profile();
        let expected = self.base_expected();
        // Read all64 physical pages, preserving their exact original bytes for
        // Pending/snapshot/redo compare-before-write. This is caller backing.
        selection::read_named(
            &mut self.writer,
            &mut self.observation,
            name(HOME_NAME),
            &mut self.maintenance[PHYSICAL_START..PHYSICAL_END],
        )?;
        self.maintenance[INIT_LSNS..INIT_LSNS + 512].fill(0);
        self.maintenance[TARGETS..TARGETS + 64 * TARGET_BYTES].fill(0);
        let (before, replay_tail) = self.maintenance.split_at_mut(REPLAY_START);
        let (replay, tail) = replay_tail.split_at_mut(REPLAY_END - REPLAY_START);
        let replay: &mut [u8; 1_572_864] = replay.try_into().expect("original replay loan");
        let (mut cursor, inherited_facts) = {
            let seed = if self.selected_base.is_some() {
                let scratch_at = BASE_SCRATCH_START - REPLAY_END;
                let scratch: &mut [[u8; PAGE_BYTES]; 4] = tail
                    [scratch_at..scratch_at + 4 * PAGE_BYTES]
                    .as_chunks_mut::<PAGE_BYTES>()
                    .0
                    .try_into()
                    .expect("base scratch");
                let view = base::decode(&before[..BASE_BYTES], expected, scratch)
                    .map_err(|e| error(p, Plan4RecoveryCause::Base(e)))?;
                Some(VerifiedPlan4Base {
                    digest: view.digest(),
                    view,
                })
            } else {
                None
            };
            if let Some(seed) = &seed {
                for i in 9..64 {
                    let record = seed
                        .view
                        .status_record(i)
                        .map_err(|e| error(p, Plan4RecoveryCause::Base(e)))?;
                    put64(tail, INIT_LSNS - REPLAY_END + 8 * i, u64_at(record, 56));
                }
            }
            let inherited = seed.as_ref().map(|seed| {
                let h = seed.view.header();
                SourceFacts {
                    epoch: h.source_epoch,
                    header_digest: h.source_header_digest,
                    prefix_digest: h.source_prefix_digest,
                    lsn: h.capture_lsn,
                    end: h.source_offset,
                    adopted_local: h.source_local,
                }
            });
            if let Some(seed) = &seed {
                let selected = self.doublewrite_header.expect("selected DWB");
                if selected.batch_id < seed.view.header().retired_batch
                    || (selected.state == dwb::ControlState::Pending
                        && selected.batch_id <= seed.view.header().retired_batch)
                {
                    return Err(error(p, Plan4RecoveryCause::OrderMismatch));
                }
            }
            (
                seed_recovery_replay(profile, seed.as_ref(), replay)
                    .map_err(|e| error(p, Plan4RecoveryCause::Retention(e)))?,
                inherited,
            )
        };
        let mut matched = 0u64;
        let mut repair = 0u64;
        let mut targets = 0u64;
        let mut pending_matched = 0u64;
        // The seed's entire current images are checked, with no dependence on
        // a source WAL already retired by its selected base.
        for index in 0..64 {
            let image = retention_replay_image(replay, index);
            if image.iter().all(|b| *b == 0) {
                continue;
            }
            let home: &[u8; PAGE_BYTES] = tail[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                .try_into()
                .expect("physical page");
            observe_home(
                profile,
                index,
                image,
                home,
                self.selected_base.is_some(),
                false,
                &mut matched,
                &mut repair,
            )?;
            let req = request(
                profile,
                index,
                Generation::new(u64_at(image, 40))
                    .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?,
            );
            note_target(
                &mut tail[TARGETS - REPLAY_END..TARGETS - REPLAY_END + 64 * TARGET_BYTES],
                req,
                image,
            )?;
            targets |= 1u64 << index;
            physical::observe_pending(
                profile,
                self.doublewrite.expect("selected DWB"),
                &self.flush[..PAGE_BYTES],
                index,
                image,
                &mut pending_matched,
            )?;
        }
        let anchor = if self.selected_base.is_some() {
            u64_at(&before[..BASE_BYTES], 96)
        } else {
            0
        };
        let mut previous = anchor;
        let mut offset = CONTROL_BYTES as u64;
        let mut first_digest = None;
        if let Some(source) = self.source.as_mut() {
            if source.anchor_lsn != anchor {
                return Err(error(p, Plan4RecoveryCause::OrderMismatch));
            }
            let limit = source.sealed.map_or(source.metadata.length, |v| v.0);
            if limit > source.metadata.length {
                return Err(error(p, Plan4RecoveryCause::LengthMismatch));
            }
            let stream = WalStreamIdentity {
                database: profile.spec().namespace.database,
                timeline: profile.spec().namespace.timeline,
                writer_epoch: source.epoch,
            };
            let policy = if source.sealed.is_some() {
                TailPolicy::ForbidIncomplete
            } else {
                TailPolicy::ActiveUnrequiredEof
            };
            // Bind the replay result to the exact bytes it consumes. Two
            // hashes begun only after replay could agree on a changed earlier
            // frame without detecting that our canonical state used old bytes.
            selection::read(
                &mut source.file,
                &mut self.observation,
                source.name,
                0,
                &mut self.slot[..CONTROL_BYTES],
            )?;
            if content_digest(&self.slot[..CONTROL_BYTES]) != source.header_digest {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
            let mut consumed = ContentHasher::new();
            consumed.update(&self.slot[..CONTROL_BYTES]);
            while offset < limit {
                let available = (limit - offset) as usize;
                let prefix = available.min(160);
                selection::read(
                    &mut source.file,
                    &mut self.observation,
                    source.name,
                    offset,
                    &mut self.slot[..prefix],
                )?;
                let frame_expected = FrameExpectation {
                    stream,
                    previous_lsn: previous,
                };
                if prefix < 160 {
                    match scan_prefix(&self.slot[..prefix], frame_expected, policy)
                        .map_err(|e| error(p, Plan4RecoveryCause::Frame(e)))?
                    {
                        SemanticFrameScan::Incomplete => break,
                        _ => return Err(error(p, Plan4RecoveryCause::Invariant)),
                    }
                }
                let length = if self.slot[7] == 3 {
                    wal_frame::retention_frame_length(&self.slot[..160], frame_expected)
                } else {
                    wal_frame::semantic_frame_length(&self.slot[..160], frame_expected)
                }
                .map_err(|e| error(p, Plan4RecoveryCause::Frame(e)))?;
                if length > available {
                    match scan_prefix(&self.slot[..160], frame_expected, policy)
                        .map_err(|e| error(p, Plan4RecoveryCause::Frame(e)))?
                    {
                        SemanticFrameScan::Incomplete => break,
                        _ => return Err(error(p, Plan4RecoveryCause::Invariant)),
                    }
                }
                selection::read(
                    &mut source.file,
                    &mut self.observation,
                    source.name,
                    offset + 160,
                    &mut self.slot[160..length],
                )?;
                let envelope = match scan_prefix(
                    &self.slot[..length],
                    frame_expected,
                    TailPolicy::ForbidIncomplete,
                )
                .map_err(|e| error(p, Plan4RecoveryCause::Frame(e)))?
                {
                    SemanticFrameScan::Complete { envelope, consumed } if consumed == length => {
                        envelope
                    }
                    _ => return Err(error(p, Plan4RecoveryCause::Invariant)),
                };
                // Only complete selected frames contribute. A permitted
                // incomplete physical tail is outside the accepted prefix.
                consumed.update(&self.slot[..length]);
                let observed = observe_recovery_frame(
                    profile,
                    frame_expected,
                    &self.slot[..length],
                    replay,
                    &mut cursor,
                )
                .map_err(|e| error(p, Plan4RecoveryCause::Retention(e)))?;
                if observed.header != envelope.header {
                    return Err(error(p, Plan4RecoveryCause::Invariant));
                }
                if previous.checked_add(1) != Some(observed.header.lsn) {
                    return Err(error(p, Plan4RecoveryCause::OrderMismatch));
                }
                if let Some(init) = observed.initialized {
                    let index = init.identity.page.page_no as usize;
                    let at = INIT_LSNS - REPLAY_END + 8 * index;
                    if u64_at(tail, at) != 0 {
                        return Err(error(p, Plan4RecoveryCause::OrderMismatch));
                    }
                    put64(tail, at, observed.header.lsn);
                }
                for req in observed.changed.into_iter().flatten() {
                    let index = req.identity.page.page_no as usize;
                    if index >= 64 || req != request(profile, index, req.identity.page.incarnation)
                    {
                        return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
                    }
                    let image = retention_replay_image(replay, index);
                    let home: &[u8; PAGE_BYTES] = tail
                        [index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                        .try_into()
                        .expect("physical page");
                    observe_home(
                        profile,
                        index,
                        image,
                        home,
                        false,
                        observed.initialized == Some(req),
                        &mut matched,
                        &mut repair,
                    )?;
                    note_target(
                        &mut tail[TARGETS - REPLAY_END..TARGETS - REPLAY_END + 64 * TARGET_BYTES],
                        req,
                        image,
                    )?;
                    physical::observe_pending(
                        profile,
                        self.doublewrite.expect("selected DWB"),
                        &self.flush[..PAGE_BYTES],
                        index,
                        image,
                        &mut pending_matched,
                    )?;
                    targets |= 1u64 << index;
                }
                previous = observed.header.lsn;
                offset += length as u64;
            }
            let replay_digest = consumed.digest();
            if let Some((end, lsn, digest)) = source.sealed {
                if offset != end || previous != lsn || replay_digest != digest {
                    return Err(error(p, Plan4RecoveryCause::OrderMismatch));
                }
                let actual = prefix_hash(
                    source,
                    &mut self.observation,
                    offset,
                    &mut self.slot[..65_536],
                )?;
                if actual != digest || actual != replay_digest {
                    return Err(error(p, Plan4RecoveryCause::ContentMismatch));
                }
                first_digest = Some(actual);
            }
            if first_digest.is_none() {
                let actual = prefix_hash(
                    source,
                    &mut self.observation,
                    offset,
                    &mut self.slot[..65_536],
                )?;
                if actual != replay_digest {
                    return Err(error(p, Plan4RecoveryCause::ContentMismatch));
                }
                first_digest = Some(actual);
            }
            // Explicit same-handle second content pass before any effect.
            if prefix_hash(
                source,
                &mut self.observation,
                offset,
                &mut self.slot[..65_536],
            )? != first_digest.expect("hashed prefix")
            {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
            if selection::metadata(&self.writer, &mut self.observation, source.name)?
                != Some(source.metadata)
            {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
        }
        if previous == 0 {
            return Err(error(p, Plan4RecoveryCause::Unsupported));
        }
        let bank = retention_replay_bank(replay);
        let (remaining_wal, remaining_steps) = remaining(profile, bank)?;
        let state_digest = content_digest(&replay[..1_179_648]);
        let status_mask = targets & !511;
        // Pending may supply the exact authoritative image for an otherwise
        // unparseable genesis allocator; physical repair validates it first.
        let supported = matched | repair | pending_matched;
        if targets & !supported != 0 {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        for index in 0..64 {
            let bit = 1u64 << index;
            if targets & bit == 0 || (matched | repair) & bit != 0 {
                continue;
            }
            let image = retention_replay_image(replay, index);
            let req = request(
                profile,
                index,
                Generation::new(u64_at(image, 40))
                    .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?,
            );
            match page::decode_page(
                &tail[index * PAGE_BYTES..(index + 1) * PAGE_BYTES],
                req.identity,
                req.kind,
            ) {
                // Pending may repair an unparseable before-image, but cannot
                // excuse a valid unknown/future home or foreign identity.
                Err(
                    PageError::IdentityMismatch
                    | PageError::KindMismatch
                    | PageError::UnsupportedFormat,
                )
                | Ok(_) => return Err(error(p, Plan4RecoveryCause::ContentMismatch)),
                Err(_) => {}
            }
        }
        // A later positive suffix image may legitimately supersede a zero-LSN
        // base image. Only the final target decides whether physical repair
        // needs an unsupported zero-LSN Pending image.
        for index in 0..64 {
            if targets & (1u64 << index) == 0 {
                continue;
            }
            let image = retention_replay_image(replay, index);
            if u64_at(image, 64) == 0
                && image.as_slice() != &tail[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
            {
                return Err(error(p, Plan4RecoveryCause::Unsupported));
            }
        }
        let facts = if let Some(source) = &self.source {
            SourceFacts {
                epoch: source.epoch,
                header_digest: source.header_digest,
                prefix_digest: first_digest.expect("source digest"),
                lsn: previous,
                end: offset,
                adopted_local: cursor.adopted_local(),
            }
        } else {
            // BaseOnly carries predecessor provenance but has no suffix file.
            inherited_facts.ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?
        };
        self.observation.source_lsn = previous;
        self.observation.source_end = offset;
        self.source_facts = Some(facts);
        self.analysis = Some(AnalysisSeal {
            state_digest,
            target_mask: targets,
            status_mask,
            remaining_wal,
            remaining_steps,
            home_matched: matched,
            snapshot_repair: repair,
            pending_matched,
        });
        Ok(())
    }
}

fn remaining(profile: Profile, bank: &[u8; BANK_BYTES]) -> Result<(u64, u64)> {
    let p = Plan4RecoveryPhase::Analysis;
    let mut wal = 0u64;
    let mut steps = 0u64;
    for slot in 0..36 {
        let raw = &bank[slot * RAM_ROW..slot * RAM_ROW + 1536];
        if raw.iter().all(|b| *b == 0) {
            continue;
        }
        let descriptor = decode_ram_descriptor(profile, slot, raw, bank)?;
        if matches!(descriptor.owner,OwnerState::System(s) if s.phase==WorkPhase::Finished) {
            continue;
        }
        for r in descriptor.remaining {
            wal = wal
                .checked_add(r.wal)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
            steps = steps
                .checked_add(r.steps)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        }
    }
    Ok((wal, steps))
}

fn decode_ram_descriptor(
    profile: Profile,
    slot: usize,
    raw: &[u8],
    bank: &[u8],
) -> Result<DescriptorSpec> {
    use crate::storage::{
        allocation_page::{ReservationOwner, SystemWorkRef},
        transaction_record::TxnRef,
    };
    let p = Plan4RecoveryPhase::Analysis;
    let reference = retention_obligation::profile_reference(profile);
    let owner = if slot < 32 {
        ReservationOwner::Transaction(
            TxnRef::decode(&bank[slot * RAM_ROW + 1576..slot * RAM_ROW + 1624])
                .map_err(|e| error(p, Plan4RecoveryCause::Page(e)))?,
        )
    } else if slot == RAM_ALLOCATION {
        ReservationOwner::System(
            SystemWorkRef::decode(&raw[24..72])
                .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?,
        )
    } else if slot == RAM_AGING {
        let owner = ReservationOwner::System(
            SystemWorkRef::decode(&raw[24..72])
                .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?,
        );
        return obligation::decode_aging_descriptor(raw, owner, reference)
            .map_err(|_| error(p, Plan4RecoveryCause::Invariant));
    } else {
        return Err(error(p, Plan4RecoveryCause::Unsupported));
    };
    obligation::decode_descriptor(raw, owner, reference)
        .map_err(|_| error(p, Plan4RecoveryCause::Invariant))
}

fn add_remaining(total: &mut [RemainingBudget; 5], next: &[RemainingBudget; 5]) -> Result<()> {
    let p = Plan4RecoveryPhase::Rebase;
    for (a, b) in total.iter_mut().zip(next) {
        macro_rules! add {
            ($field:ident) => {
                a.$field = a
                    .$field
                    .checked_add(b.$field)
                    .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
            };
        }
        add!(steps);
        add!(wal);
        add!(control);
        add!(growth);
        add!(pages);
        add!(undo_slots);
        add!(status_slots);
        add!(protected);
        macro_rules! peak {
            ($field:ident) => {
                a.peak.$field = a.peak.$field.max(b.peak.$field);
            };
        }
        peak!(frames);
        peak!(memory);
        peak!(locks);
        peak!(registry);
        peak!(files);
        peak!(doublewrite);
    }
    Ok(())
}

/// Bounded selection sort over the immutable RAM bank. Only indices and one
/// decoded value are retained, never a second bank of DescriptorSpec values.
fn pack_bank(
    profile: Profile,
    bank: &[u8; BANK_BYTES],
    out: &mut [u8; base::DESCRIPTOR_BYTES],
) -> Result<(u32, u32, [RemainingBudget; 5])> {
    use crate::storage::transaction_record::TxnRef;
    let p = Plan4RecoveryPhase::Rebase;
    out.fill(0);
    let mut used = 0u32;
    let mut transactions = 0u32;
    let mut remaining = [RemainingBudget::default(); 5];
    loop {
        let mut best: Option<(usize, [u64; 3])> = None;
        for slot in 0..32 {
            if used & (1u32 << slot) != 0
                || bank[slot * RAM_ROW..slot * RAM_ROW + 1536]
                    .iter()
                    .all(|v| *v == 0)
            {
                continue;
            }
            let actor = TxnRef::decode(&bank[slot * RAM_ROW + 1576..slot * RAM_ROW + 1624])
                .map_err(|e| error(p, Plan4RecoveryCause::Page(e)))?;
            let key = [
                u64::from(actor.id().writer_epoch),
                actor.id().local,
                actor.generation().get(),
            ];
            if best.is_none_or(|(_, prior)| key < prior) {
                best = Some((slot, key));
            }
        }
        let Some((slot, _)) = best else { break };
        let raw = &bank[slot * RAM_ROW..slot * RAM_ROW + 1536];
        let descriptor = decode_ram_descriptor(profile, slot, raw, bank)?;
        add_remaining(&mut remaining, &descriptor.remaining)?;
        let at = transactions as usize * 1536;
        out[at..at + 1536].copy_from_slice(raw);
        used |= 1u32 << slot;
        transactions += 1;
    }
    let mut systems = 0u32;
    for (ram, wire) in [(RAM_ALLOCATION, 32), (RAM_AGING, 34)] {
        let raw = &bank[ram * RAM_ROW..ram * RAM_ROW + 1536];
        if raw.iter().all(|v| *v == 0) {
            continue;
        }
        let descriptor = decode_ram_descriptor(profile, ram, raw, bank)?;
        out[wire * 1536..(wire + 1) * 1536].copy_from_slice(raw);
        if !matches!(descriptor.owner,OwnerState::System(s) if s.phase==WorkPhase::Finished) {
            systems += 1;
            add_remaining(&mut remaining, &descriptor.remaining)?;
        }
    }
    Ok((transactions, systems, remaining))
}

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn seal(bytes: &mut [u8]) {
    let n = bytes.len() - 32;
    let digest = content_digest(&bytes[..n]);
    bytes[n..].copy_from_slice(&digest);
}

/// Encodes the closed zero/one-item embedded XOI1 form. The original borrowed
/// ownership verifier inside base::encode remains the authority on its grammar
/// and descriptor/census equality; this producer adds no alternate parser.
fn pack_inventory(
    profile: Profile,
    generation: u64,
    capture: u64,
    transactions: u32,
    bank: &[u8; base::DESCRIPTOR_BYTES],
    allocator: &[u8; PAGE_BYTES],
    output: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let p = Plan4RecoveryPhase::Rebase;
    output.fill(0);
    let mut owners = transactions as usize;
    for i in 0..transactions as usize {
        output[128 + i * 56..128 + (i + 1) * 56]
            .copy_from_slice(&bank[i * 1536 + 16..i * 1536 + 72]);
    }
    let mut allocation_owner = None;
    for wire in [32, 34] {
        let raw = &bank[wire * 1536..(wire + 1) * 1536];
        if raw.iter().all(|v| *v == 0) {
            continue;
        }
        let owner = crate::storage::allocation_page::SystemWorkRef::decode(&raw[24..72])
            .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?;
        let d = if wire == 34 {
            obligation::decode_aging_descriptor(
                raw,
                crate::storage::allocation_page::ReservationOwner::System(owner),
                retention_obligation::profile_reference(profile),
            )
        } else {
            obligation::decode_descriptor(
                raw,
                crate::storage::allocation_page::ReservationOwner::System(owner),
                retention_obligation::profile_reference(profile),
            )
        }
        .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?;
        if matches!(d.owner,OwnerState::System(s) if s.phase==WorkPhase::Finished) {
            continue;
        }
        if wire == 32 {
            allocation_owner = Some(owners);
        }
        output[128 + owners * 56..128 + (owners + 1) * 56].copy_from_slice(&raw[16..72]);
        owners += 1;
    }
    let req = request(profile, 0, Generation::new(1).expect("fixed incarnation"));
    let allocation =
        allocation_page::decode_allocation_page(allocator, req.identity, profile.spec().namespace)
            .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?;
    let mut reserved = None;
    for i in 0..64 {
        if let AllocationEntry::Reserved { generation, .. } = allocation
            .entry(i)
            .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?
        {
            if reserved.is_some() {
                return Err(error(p, Plan4RecoveryCause::Invariant));
            }
            reserved = Some((i, generation));
        }
    }
    let chunk_len = if let Some((index, incarnation)) = reserved {
        let owner = allocation_owner.ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        let table_hash = content_digest(&output[128..128 + owners * 56]);
        let chunk = &mut output[2176..2176 + 256];
        chunk[..4].copy_from_slice(b"XOI1");
        put16(chunk, 4, 1);
        chunk[6] = 2;
        chunk[7] = 2;
        put16(chunk, 8, 128);
        put16(chunk, 10, 96);
        put32(chunk, 12, 256);
        chunk[16..32].copy_from_slice(&profile.spec().namespace.database.bytes());
        put64(chunk, 32, profile.spec().namespace.timeline);
        put64(chunk, 40, generation);
        put64(chunk, 48, capture);
        put64(chunk, 64, 1);
        put32(chunk, 80, 1);
        chunk[88..120].copy_from_slice(&table_hash);
        chunk[128] = 1;
        chunk[129] = 3;
        put16(chunk, 130, owner as u16);
        request(profile, index, incarnation)
            .identity
            .page
            .encode(&mut chunk[136..160])
            .map_err(|e| error(p, Plan4RecoveryCause::Page(e)))?;
        put64(chunk, 192, 1);
        put64(chunk, 200, u64_at(allocator, 64));
        seal(chunk);
        256usize
    } else {
        0
    };
    let chunks_hash = content_digest(&output[2176..2176 + chunk_len]);
    let root_len = 160 + owners * 56;
    let root = &mut output[..root_len];
    root[..4].copy_from_slice(b"XOI1");
    put16(root, 4, 1);
    root[6] = 1;
    root[7] = 2;
    put16(root, 8, 128);
    put16(root, 10, 96);
    put32(root, 12, root_len as u32);
    root[16..32].copy_from_slice(&profile.spec().namespace.database.bytes());
    put64(root, 32, profile.spec().namespace.timeline);
    put64(root, 40, generation);
    put64(root, 48, capture);
    put16(root, 56, owners as u16);
    put64(root, 64, u64::from(reserved.is_some()));
    put64(root, 72, u64::from(reserved.is_some()));
    put64(root, 80, chunk_len as u64);
    root[88..120].copy_from_slice(&chunks_hash);
    seal(root);
    Ok(())
}

impl<W: WriterNamespace> RecoveryState<'_, '_, '_, W> {
    pub(super) fn capture_base(&mut self, generation: u64, issued_epoch: u32) -> Result<[u8; 32]> {
        let p = Plan4RecoveryPhase::Rebase;
        self.observation.phase = p;
        if self.source.is_none() {
            self.maintenance.copy_within(0..BASE_BYTES, BASE_BYTES);
            return Ok(content_digest(
                &self.maintenance[BASE_BYTES..2 * BASE_BYTES],
            ));
        }
        // Packing descriptors/certificates/inventory completes before the
        // full codec runs. Only its scalar Header crosses this boundary.
        let header = self.pack_capture(generation, issued_epoch)?;
        let expected = self.base_expected();
        let (_, from_new) = self.maintenance.split_at_mut(BASE_BYTES);
        let (output, from_replay) = from_new.split_at_mut(BASE_BYTES);
        let (replay, tail) = from_replay.split_at_mut(REPLAY_END - REPLAY_START);
        let (_, packing) = tail.split_at_mut(PACK_START - REPLAY_END);
        let (pack, packing) = packing.split_at_mut(PACK_END - PACK_START);
        let (status, packing) = packing.split_at_mut(STATUS_END - STATUS_START);
        let (certs, packing) = packing.split_at_mut(CERT_END - CERT_START);
        let (inventory, packing) = packing.split_at_mut(PAGE_BYTES);
        let scratch: &mut [[u8; PAGE_BYTES]; 4] = packing[..4 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("base scratch");
        let sections = base::Sections {
            descriptors: (&pack[..base::DESCRIPTOR_BYTES])
                .try_into()
                .expect("packed bank"),
            allocator: (&replay[..PAGE_BYTES]).try_into().expect("allocator"),
            queue: replay[PAGE_BYTES..7 * PAGE_BYTES]
                .as_chunks::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("queue pages"),
            statuses: status[..64 * 352]
                .as_chunks::<352>()
                .0
                .try_into()
                .expect("status records"),
            certificates: certs[..64 * 96]
                .as_chunks::<96>()
                .0
                .try_into()
                .expect("certificates"),
            inventory: (&*inventory).try_into().expect("inventory"),
        };
        let view = base::encode(
            header,
            expected,
            sections,
            output.try_into().expect("new base"),
            scratch,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Base(e)))?;
        Ok(view.digest())
    }

    fn pack_capture(&mut self, generation: u64, issued_epoch: u32) -> Result<base::Header> {
        let p = Plan4RecoveryPhase::Rebase;
        let expected = self.base_expected();
        let profile = self.profile();
        let previous = self.selected_closure()?;
        let (previous_generation, previous_digest) = previous
            .filter(|c| c.anchor == record::AnchorKind::Base)
            .map_or((0, [0; 32]), |c| (c.generation, c.digest));
        let facts = self
            .source_facts
            .as_ref()
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        let analyzed = self
            .analysis
            .as_ref()
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        let selected = self.doublewrite_header.expect("verified DWB");
        let (_, from_new) = self.maintenance.split_at_mut(BASE_BYTES);
        let (_, from_replay) = from_new.split_at_mut(BASE_BYTES);
        let (replay, tail) = from_replay.split_at_mut(REPLAY_END - REPLAY_START);
        if content_digest(&replay[..1_179_648]) != analyzed.state_digest {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        let replay: &[u8; 1_572_864] = (&*replay).try_into().expect("replay");
        let bank = retention_replay_bank(replay);
        let allocator = retention_replay_image(replay, 0);
        let (before_pack, packing) = tail.split_at_mut(PACK_START - REPLAY_END);
        let _ = before_pack;
        let (pack, packing) = packing.split_at_mut(PACK_END - PACK_START);
        let packed: &mut [u8; base::DESCRIPTOR_BYTES] = (&mut pack[..base::DESCRIPTOR_BYTES])
            .try_into()
            .expect("packed bank");
        let (transactions, systems, remaining) = pack_bank(profile, bank, packed)?;
        let (status, packing) = packing.split_at_mut(STATUS_END - STATUS_START);
        let (certs, packing) = packing.split_at_mut(CERT_END - CERT_START);
        let (inventory, _) = packing.split_at_mut(PAGE_BYTES);
        status[..64 * base::STATUS_BYTES].fill(0);
        certs[..64 * base::CERTIFICATE_BYTES].fill(0);
        for index in 0..64 {
            if analyzed.target_mask & (1u64 << index) == 0 {
                continue;
            }
            let image = retention_replay_image(replay, index);
            let req = request(
                profile,
                index,
                Generation::new(u64_at(image, 40))
                    .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?,
            );
            let header = page::decode_page(image, req.identity, req.kind)
                .map_err(|e| error(p, Plan4RecoveryCause::Page(e)))?
                .header();
            base::encode_certificate(
                expected,
                header,
                image,
                (&mut certs[index * 96..(index + 1) * 96])
                    .try_into()
                    .expect("certificate"),
            )
            .map_err(|e| error(p, Plan4RecoveryCause::Base(e)))?;
            if index >= 9 {
                let init = u64_at(status, 64 * base::STATUS_BYTES + index * 8);
                base::encode_status_record(
                    expected,
                    header,
                    init,
                    image,
                    (&mut status[index * 352..(index + 1) * 352])
                        .try_into()
                        .expect("status record"),
                )
                .map_err(|e| error(p, Plan4RecoveryCause::Base(e)))?;
            }
        }
        let inventory: &mut [u8; PAGE_BYTES] = inventory.try_into().expect("inventory");
        pack_inventory(
            profile,
            generation,
            facts.lsn,
            transactions,
            packed,
            allocator,
            inventory,
        )?;
        let root = crate::storage::retention_page::decode_root_page(
            retention_replay_image(replay, 1),
            profile.spec().namespace,
        )
        .map_err(|_| error(p, Plan4RecoveryCause::Invariant))?
        .1
        .spec();
        let terminal = match bank[RAM_ALLOCATION * RAM_ROW + 1544] {
            0 => base::AllocationTerminal::None,
            1 => base::AllocationTerminal::Cancelled,
            2 => base::AllocationTerminal::InitializedStatus,
            _ => return Err(error(p, Plan4RecoveryCause::Invariant)),
        };
        let header = base::Header {
            generation,
            capture_lsn: facts.lsn,
            previous_generation,
            previous_digest,
            source_epoch: facts.epoch,
            source_offset: facts.end,
            source_prefix_digest: facts.prefix_digest,
            source_local: facts.adopted_local,
            system_high_water: [
                u64_at(bank, RAM_ALLOCATION * RAM_ROW + 1536),
                0,
                u64::from(root.aging.is_some()),
                0,
            ],
            active_transactions: transactions,
            active_systems: systems,
            status_mask: analyzed.status_mask,
            leased: root.leased,
            ready: root.ready,
            root_revision: root.revision,
            aging_groups: root.revision - 1,
            allocation_terminal: terminal,
            retired_batch: selected.batch_id,
            doublewrite_generation: selected.generation,
            remaining,
            physical_bytes: self.observation.actual_owned,
            issued_epoch,
            source_header_digest: facts.header_digest,
        };
        // The original base INITs and the accepted unique full INIT frames
        // supplied every compressed record above. No home bytes fed this table.
        Ok(header)
    }
}
