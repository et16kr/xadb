//! Ordered fixed-selector publication and exact epoch retirement. A successful
//! write alone never advances selection; uncertain publication retains both.
use super::*;

pub(super) fn write<F: FileIo>(
    file: &mut F,
    o: &mut Plan4RecoveryObservation,
    path: FileName,
    offset: u64,
    bytes: &[u8],
) -> Result<()> {
    o.begin(RecoveryCallKind::Write, path, offset, bytes.len() as u64)?;
    write_all_at(file, bytes, offset).map_err(|e| error(o.phase, Plan4RecoveryCause::Io(e)))?;
    o.done(RecoveryCallKind::Write);
    Ok(())
}
pub(super) fn file_sync<F: FileIo>(
    file: &mut F,
    o: &mut Plan4RecoveryObservation,
    path: FileName,
) -> Result<()> {
    o.begin(RecoveryCallKind::Sync, path, 0, 0)?;
    sync(file).map_err(|e| error(o.phase, Plan4RecoveryCause::Io(e)))?;
    o.done(RecoveryCallKind::Sync);
    Ok(())
}
pub(super) fn directory_sync<W: WriterNamespace>(
    w: &mut W,
    o: &mut Plan4RecoveryObservation,
) -> Result<()> {
    o.begin(RecoveryCallKind::DirectorySync, name("directory"), 0, 0)?;
    w.sync_directory()
        .map_err(|e| error(o.phase, Plan4RecoveryCause::Namespace(e)))?;
    o.done(RecoveryCallKind::DirectorySync);
    Ok(())
}
pub(super) fn create<W: WriterNamespace>(
    w: &mut W,
    o: &mut Plan4RecoveryObservation,
    path: FileName,
) -> Result<()> {
    o.begin(RecoveryCallKind::Create, path, 0, 0)?;
    drop(
        w.create_new(&path)
            .map_err(|e| error(o.phase, Plan4RecoveryCause::Namespace(e)))?,
    );
    o.done(RecoveryCallKind::Create);
    Ok(())
}
pub(super) fn write_existing<W: WriterNamespace>(
    w: &mut W,
    o: &mut Plan4RecoveryObservation,
    path: FileName,
    expected_length: u64,
    offset: u64,
    bytes: &[u8],
) -> Result<()> {
    let (mut file, opened) =
        selection::open(w, o, path, OpenMode::ReadWrite, Some(expected_length))?;
    write(&mut file, o, path, offset, bytes)?;
    file_sync(&mut file, o, path)?;
    let length = expected_length.max(
        offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| error(o.phase, Plan4RecoveryCause::Exhausted))?,
    );
    if selection::metadata(w, o, path)? != Some(FileMetadata { length, ..opened }) {
        return Err(error(o.phase, Plan4RecoveryCause::IdentityMismatch));
    }
    Ok(())
}

/// The caller records the known/possible union before entering this function.
/// A prior bounded stage may be overwritten in place; its identity is checked
/// before/after and its full exact image is synced before publication.
pub(super) fn publish<W: WriterNamespace>(
    w: &mut W,
    o: &mut Plan4RecoveryObservation,
    selector: Selector,
    stage: FileName,
    bytes: &[u8; CONTROL_BYTES],
    readback: &mut [u8; CONTROL_BYTES],
) -> Result<()> {
    let prior = selection::metadata(w, o, stage)?;
    if prior.is_some_and(|s| s.length > CONTROL_BYTES as u64) {
        return Err(error(o.phase, Plan4RecoveryCause::LengthMismatch));
    }
    if prior.is_none() {
        create(w, o, stage)?;
    }
    write_existing(w, o, stage, prior.map_or(0, |s| s.length), 0, bytes)?;
    let stage_stamp = selection::read_named(w, o, stage, readback)?;
    if readback != bytes {
        return Err(error(o.phase, Plan4RecoveryCause::ContentMismatch));
    }
    o.begin(
        RecoveryCallKind::Publish,
        name(selector.as_str()),
        0,
        CONTROL_BYTES as u64,
    )?;
    w.publish_selector(&stage, selector)
        .map_err(|e| error(o.phase, Plan4RecoveryCause::Namespace(e)))?;
    o.done(RecoveryCallKind::Publish);
    directory_sync(w, o)?;
    if selection::read_named(w, o, name(selector.as_str()), readback)? != stage_stamp
        || readback != bytes
        || selection::metadata(w, o, stage)?.is_some()
    {
        return Err(error(o.phase, Plan4RecoveryCause::IdentityMismatch));
    }
    Ok(())
}

impl<W: WriterNamespace> RecoveryState<'_, '_, '_, W> {
    pub(super) fn publish_current(&mut self, value: record::Current) -> Result<()> {
        let profile = self.profile();
        let p = self.observation.phase;
        let (prior, rest) = self.maintenance.split_at_mut(POSSIBLE_CURRENT_AT);
        let (candidate, rest) = rest.split_at_mut(CONTROL_BYTES);
        let candidate: &mut [u8; CONTROL_BYTES] = candidate.try_into().expect("possible CURRENT");
        let readback: &mut [u8; CONTROL_BYTES] = (&mut rest[..CONTROL_BYTES])
            .try_into()
            .expect("CURRENT readback");
        record::encode_current(value, profile, candidate)
            .map_err(|e| error(p, Plan4RecoveryCause::Record(e)))?;
        self.possible_current = true;
        publish(
            &mut self.writer,
            &mut self.observation,
            Selector::Current,
            name("d3-current-stage"),
            candidate,
            readback,
        )?;
        prior[CURRENT_AT..CURRENT_AT + CONTROL_BYTES].copy_from_slice(candidate);
        self.current_present = true;
        self.possible_current = false;
        self.selected_previous = false;
        Ok(())
    }
    /// A recorded intent, including a partial header, authorizes only this exact
    /// unselected name. Burn remains in CURRENT after deletion and directory sync.
    pub(super) fn retire_intent(&mut self) -> Result<()> {
        let Some(mut current) = self.current()? else {
            return Ok(());
        };
        let Some(intent) = current.intent else {
            return Ok(());
        };
        if current.current.mode != record::Mode::BaseOnly
            || current.previous.mode != record::Mode::BaseOnly
        {
            return Err(error(
                self.observation.phase,
                Plan4RecoveryCause::OrderMismatch,
            ));
        }
        self.observation.phase = Plan4RecoveryPhase::Reclaim;
        let path = selection::epoch_name(intent.epoch);
        if selection::metadata(&self.writer, &mut self.observation, path)?.is_some() {
            self.observation
                .begin(RecoveryCallKind::Remove, path, 0, 0)?;
            self.writer
                .remove_retired(&path)
                .map_err(|e| error(self.observation.phase, Plan4RecoveryCause::Namespace(e)))?;
            self.observation.done(RecoveryCallKind::Remove);
        }
        directory_sync(&mut self.writer, &mut self.observation)?;
        if selection::metadata(&self.writer, &mut self.observation, path)?.is_some() {
            return Err(error(
                self.observation.phase,
                Plan4RecoveryCause::IdentityMismatch,
            ));
        }
        current.generation = current
            .generation
            .checked_add(1)
            .ok_or_else(|| error(self.observation.phase, Plan4RecoveryCause::Exhausted))?;
        current.intent = None; // issued_epoch is intentionally unchanged.
        self.publish_current(current)?;
        self.inventory()
    }

    fn write_base_copy(&mut self, slot: u8) -> Result<()> {
        let p = Plan4RecoveryPhase::Rebase;
        if self.profile().base_bytes() != BASE_BYTES {
            return Err(error(p, Plan4RecoveryCause::LengthMismatch));
        }
        let path = name(BASE_NAMES[slot as usize]);
        let stamp = selection::metadata(&self.writer, &mut self.observation, path)?;
        if stamp.is_some_and(|s| s.length > BASE_BYTES as u64) {
            return Err(error(p, Plan4RecoveryCause::LengthMismatch));
        }
        if stamp.is_none() {
            create(&mut self.writer, &mut self.observation, path)?;
        }
        write_existing(
            &mut self.writer,
            &mut self.observation,
            path,
            stamp.map_or(0, |s| s.length),
            0,
            &self.maintenance[BASE_BYTES..2 * BASE_BYTES],
        )?;
        directory_sync(&mut self.writer, &mut self.observation)?;
        let (mut file, opened) = selection::open(
            &mut self.writer,
            &mut self.observation,
            path,
            OpenMode::ReadOnly,
            Some(BASE_BYTES as u64),
        )?;
        let mut offset = 0usize;
        while offset < BASE_BYTES {
            let end = offset + PAGE_BYTES;
            selection::read(
                &mut file,
                &mut self.observation,
                path,
                offset as u64,
                &mut self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES],
            )?;
            if self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES]
                != self.maintenance[BASE_BYTES + offset..BASE_BYTES + end]
            {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
            offset = end;
        }
        if selection::metadata(&self.writer, &mut self.observation, path)? != Some(opened) {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        Ok(())
    }

    /// First seal the exact consumed source. A new base never uses the old
    /// physical slot while any selected fallback still requires that slot.
    pub(super) fn rebase(&mut self) -> Result<()> {
        let p = Plan4RecoveryPhase::Rebase;
        self.observation.phase = p;
        let source_present = self.source.is_some();
        if source_present {
            let mut closure = if let Some(closure) = self.selected_closure()? {
                closure
            } else {
                selection::read_named(
                    &mut self.writer,
                    &mut self.observation,
                    name("WRITER"),
                    &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
                )?;
                record::Closure {
                    anchor: record::AnchorKind::Genesis,
                    mode: record::Mode::WalSealed,
                    slot: 0,
                    epoch: 1,
                    generation: 1,
                    lsn: 0,
                    digest: self.bootstrap_digest,
                    epoch_digest: self.source.as_ref().expect("source").header_digest,
                    replay_offset: CONTROL_BYTES as u64,
                    sealed_offset: 0,
                    sealed_lsn: 0,
                    sealed_digest: [0; 32],
                    writer_digest: content_digest(
                        &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
                    ),
                    writer_generation: 1,
                }
            };
            let facts = self.source_facts.as_ref().expect("analyzed source");
            closure.mode = record::Mode::WalSealed;
            closure.sealed_offset = facts.end;
            closure.sealed_lsn = facts.lsn;
            closure.sealed_digest = facts.prefix_digest;
            let old = self.current()?;
            let sealed = record::Current {
                generation: old
                    .map_or(Some(1), |c| c.generation.checked_add(1))
                    .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?,
                issued_epoch: old.map_or(facts.epoch, |c| c.issued_epoch),
                retired_through_epoch: old.map_or(0, |c| c.retired_through_epoch),
                writer_high_water: old.map_or(1, |c| c.writer_high_water),
                base_high_water: old.map_or(0, |c| c.base_high_water),
                intent: None,
                current: closure,
                previous: closure,
            };
            self.publish_current(sealed)?;
        }
        let mut current = self
            .current()?
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        // A failed new-epoch creation is retired from its exact durable intent
        // before a different header can be prepared.
        if current.intent.is_some() {
            self.retire_intent()?;
            current = self.current()?.expect("CURRENT");
        }
        let generation = if source_present {
            current
                .base_high_water
                .checked_add(1)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?
        } else {
            current.current.generation
        };
        let digest = self.capture_base(generation, current.issued_epoch)?;
        let lsn = self.source_facts.as_ref().expect("analysis facts").lsn;
        let first = if source_present && current.current.anchor == record::AnchorKind::Base {
            1 - current.current.slot
        } else if !source_present {
            self.selected_base
                .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?
        } else {
            0
        };
        if source_present {
            self.write_base_copy(first)?;
        } else {
            // The selected existing full snapshot was already read and decoded;
            // confirm the selected bytes immediately before replacing closure.
            let path = name(BASE_NAMES[first as usize]);
            selection::read_named(
                &mut self.writer,
                &mut self.observation,
                path,
                &mut self.maintenance[..BASE_BYTES],
            )?;
            if content_digest(&self.maintenance[..BASE_BYTES]) != digest {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
        }
        let closure = record::Closure {
            anchor: record::AnchorKind::Base,
            mode: record::Mode::BaseOnly,
            slot: first,
            epoch: 0,
            generation,
            lsn,
            digest,
            epoch_digest: [0; 32],
            replay_offset: 0,
            sealed_offset: 0,
            sealed_lsn: 0,
            sealed_digest: [0; 32],
            writer_digest: [0; 32],
            writer_generation: 0,
        };
        current.generation = current
            .generation
            .checked_add(1)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        current.base_high_water = current.base_high_water.max(generation);
        current.current = closure;
        current.intent = None;
        if source_present {
            self.publish_current(current)?;
        }
        // Both references now protect the new first copy. The old second slot
        // is no longer reachable and can be replaced without a torn fallback.
        current.generation = current
            .generation
            .checked_add(1)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        current.previous = closure;
        self.publish_current(current)?;
        self.write_base_copy(1 - first)?;
        current.generation = current
            .generation
            .checked_add(1)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        current.previous.slot = 1 - first;
        self.publish_current(current)?;
        self.maintenance.copy_within(BASE_BYTES..2 * BASE_BYTES, 0);
        self.selected_base = Some(first);
        self.retire_source()?;
        self.inventory()
    }

    fn retire_source(&mut self) -> Result<()> {
        let p = Plan4RecoveryPhase::Reclaim;
        self.observation.phase = p;
        let mut current = self
            .current()?
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        if current.current.mode != record::Mode::BaseOnly
            || current.previous.mode != record::Mode::BaseOnly
            || current.intent.is_some()
        {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        }
        let (epoch, digest) = {
            let view = self.selected_base_view()?;
            (
                view.header().source_epoch,
                view.header().source_header_digest,
            )
        };
        let path = selection::epoch_name(epoch);
        if let Some(stamp) = selection::metadata(&self.writer, &mut self.observation, path)? {
            if stamp.length < CONTROL_BYTES as u64 {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            {
                let (mut file, opened) = selection::open(
                    &mut self.writer,
                    &mut self.observation,
                    path,
                    OpenMode::ReadOnly,
                    Some(stamp.length),
                )?;
                selection::read(
                    &mut file,
                    &mut self.observation,
                    path,
                    0,
                    &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
                )?;
                if content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES]) != digest
                    || selection::metadata(&self.writer, &mut self.observation, path)?
                        != Some(opened)
                {
                    return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
                }
            }
            if self
                .source
                .as_ref()
                .is_some_and(|s| s.name != path || s.metadata != stamp || s.header_digest != digest)
            {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            drop(self.source.take());
            self.observation
                .begin(RecoveryCallKind::Remove, path, 0, 0)?;
            self.writer
                .remove_retired(&path)
                .map_err(|e| error(p, Plan4RecoveryCause::Namespace(e)))?;
            self.observation.done(RecoveryCallKind::Remove);
        } else {
            drop(self.source.take());
        }
        directory_sync(&mut self.writer, &mut self.observation)?;
        if selection::metadata(&self.writer, &mut self.observation, path)?.is_some() {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        current.retired_through_epoch = current.retired_through_epoch.max(epoch);
        current.generation = current
            .generation
            .checked_add(1)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        self.publish_current(current)
    }

    /// Durable intent selects the exact proposed header before create_new.
    /// Both final WalOpen closures share one continuation and differ only slot.
    pub(super) fn log_open(&mut self) -> Result<()> {
        let p = Plan4RecoveryPhase::LogOpen;
        self.observation.phase = p;
        let profile = self.profile();
        self.inventory()?;
        let mut current = self
            .current()?
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        if current.intent.is_some()
            || current.current.mode != record::Mode::BaseOnly
            || current.previous.mode != record::Mode::BaseOnly
            || self.source.is_some()
        {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        }
        let (previous_epoch, previous_header_digest) = {
            let base = self.selected_base_view()?;
            (
                base.header().source_epoch,
                base.header().source_header_digest,
            )
        };
        let epoch = current
            .issued_epoch
            .checked_add(1)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        let active_generation = current
            .generation
            .checked_add(2)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        let writer_generation = current
            .writer_high_water
            .checked_add(1)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        let header = record::EpochHeader {
            epoch,
            base_lsn: current.current.lsn,
            base_generation: current.current.generation,
            base_digest: current.current.digest,
            previous_epoch,
            previous_header_digest,
        };
        record::encode_epoch(
            header,
            profile,
            (&mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES])
                .try_into()
                .expect("epoch header loan"),
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Record(e)))?;
        let header_digest = content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES]);
        let writer = record::WriterSelection {
            generation: writer_generation,
            epoch,
            epoch_digest: header_digest,
            base_generation: header.base_generation,
            base_lsn: header.base_lsn,
            base_digest: header.base_digest,
            active_generation,
        };
        record::encode_writer(
            writer,
            profile,
            (&mut self.maintenance[IO_WORK + CONTROL_BYTES..IO_WORK + 2 * CONTROL_BYTES])
                .try_into()
                .expect("writer selection loan"),
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Record(e)))?;
        let writer_digest =
            content_digest(&self.maintenance[IO_WORK + CONTROL_BYTES..IO_WORK + 2 * CONTROL_BYTES]);
        current.generation += 1;
        current.issued_epoch = epoch;
        current.writer_high_water = writer_generation;
        current.intent = Some(record::EpochIntent {
            epoch,
            base_generation: header.base_generation,
            base_lsn: header.base_lsn,
            base_digest: header.base_digest,
            active_generation,
            epoch_digest: header_digest,
            writer_digest,
        });
        self.publish_current(current)?;
        let path = selection::epoch_name(epoch);
        create(&mut self.writer, &mut self.observation, path)?;
        write_existing(
            &mut self.writer,
            &mut self.observation,
            path,
            0,
            0,
            &self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        directory_sync(&mut self.writer, &mut self.observation)?;
        selection::read_named(
            &mut self.writer,
            &mut self.observation,
            path,
            &mut self.maintenance[SELECTOR_AT..SELECTOR_AT + CONTROL_BYTES],
        )?;
        if content_digest(&self.maintenance[SELECTOR_AT..SELECTOR_AT + CONTROL_BYTES])
            != header_digest
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        let (prior, after) = self.maintenance.split_at_mut(IO_WORK + CONTROL_BYTES);
        let readback: &mut [u8; CONTROL_BYTES] = (&mut prior
            [SELECTOR_AT..SELECTOR_AT + CONTROL_BYTES])
            .try_into()
            .expect("selector readback");
        let bytes: &[u8; CONTROL_BYTES] =
            (&after[..CONTROL_BYTES]).try_into().expect("writer bytes");
        publish(
            &mut self.writer,
            &mut self.observation,
            Selector::Writer,
            name("d3-writer-stage"),
            bytes,
            readback,
        )?;
        current.generation = active_generation;
        current.intent = None;
        for closure in [&mut current.current, &mut current.previous] {
            closure.mode = record::Mode::WalOpen;
            closure.epoch = epoch;
            closure.epoch_digest = header_digest;
            closure.replay_offset = CONTROL_BYTES as u64;
            closure.writer_generation = writer_generation;
            closure.writer_digest = writer_digest;
        }
        self.publish_current(current)?;
        let (mut file, metadata) = selection::open(
            &mut self.writer,
            &mut self.observation,
            path,
            OpenMode::ReadWrite,
            Some(CONTROL_BYTES as u64),
        )?;
        selection::read(
            &mut file,
            &mut self.observation,
            path,
            0,
            &mut self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES],
        )?;
        if content_digest(&self.maintenance[IO_WORK..IO_WORK + CONTROL_BYTES]) != header_digest {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        self.selected_epoch = Some(SelectedNewEpoch {
            name: path,
            metadata,
            header,
            header_digest,
            writer_digest,
            current_digest: content_digest(
                &self.maintenance[CURRENT_AT..CURRENT_AT + CONTROL_BYTES],
            ),
            current_generation: active_generation,
        });
        self.source = Some(HeldSource {
            file,
            name: path,
            metadata,
            epoch,
            header_digest,
            anchor_lsn: header.base_lsn,
            sealed: None,
        });
        self.inventory()
    }
}

impl<W: WriterNamespace> Plan4RecoveryRuntime<'_, '_, '_, W> {
    pub(super) fn stabilize_live(
        &self,
        o: &mut Plan4RecoveryObservation,
        expected: SelectedControl,
        last_issued: u64,
    ) -> Result<()> {
        let p = Plan4RecoveryPhase::Stabilize;
        o.phase = p;
        let wal = self.storage.wal();
        let prefix = wal.prefix().map_err(|e| {
            let effect = e.effect;
            self.bound_error(p, Plan4RecoveryCause::Wal(e), effect)
        })?;
        if prefix.complete_lsn != prefix.synced_lsn
            || prefix.complete_offset != prefix.synced_offset
            || prefix.complete_lsn < self.base.capture_lsn()
            || prefix.file != self.selected.metadata.identity
            || prefix.stream.writer_epoch != self.selected.header.epoch
        {
            return Err(self.bound_error(
                p,
                Plan4RecoveryCause::Invariant,
                Effect::CallerMustResolve,
            ));
        }
        {
            let state = wal.state(wal::Phase::Observe).map_err(|e| {
                let effect = e.effect;
                self.bound_error(p, Plan4RecoveryCause::Wal(e), effect)
            })?;
            if !matches!(state.state, wal::SlotState::Idle) || state.slot.is_none() {
                return Err(self.bound_error(
                    p,
                    Plan4RecoveryCause::Invariant,
                    Effect::CallerMustResolve,
                ));
            }
        }
        // No namespace/append/pool guard is held over the original File sync.
        {
            let mut file = wal
                .file
                .try_lock()
                .map_err(|_| self.bound_error(p, Plan4RecoveryCause::Busy, Effect::Unchanged))?;
            file_sync(&mut *file, o, self.selected.name)?;
        }
        let mut scratch = self
            .remainder
            .try_lock()
            .map_err(|_| self.bound_error(p, Plan4RecoveryCause::Busy, Effect::Unchanged))?;
        let mut writer = wal
            ._writer
            .try_lock()
            .map_err(|_| self.bound_error(p, Plan4RecoveryCause::Busy, Effect::Unchanged))?;
        directory_sync(&mut *writer, o)?;
        let bytes = &mut scratch.packing_and_scratch[..PAGE_BYTES];
        for (path, digest) in [
            (name("CURRENT"), self.selected.current_digest),
            (name("WRITER"), self.selected.writer_digest),
        ] {
            selection::read_named(&mut *writer, o, path, &mut bytes[..CONTROL_BYTES])?;
            if content_digest(&bytes[..CONTROL_BYTES]) != digest {
                return Err(self.bound_error(
                    p,
                    Plan4RecoveryCause::ContentMismatch,
                    Effect::CallerMustResolve,
                ));
            }
        }
        {
            let (mut file, opened) = selection::open(
                &mut *writer,
                o,
                self.selected.name,
                OpenMode::ReadOnly,
                None,
            )?;
            if opened.identity != prefix.file || opened.length != prefix.complete_offset {
                return Err(self.bound_error(
                    p,
                    Plan4RecoveryCause::IdentityMismatch,
                    Effect::CallerMustResolve,
                ));
            }
            selection::read(
                &mut file,
                o,
                self.selected.name,
                0,
                &mut bytes[..CONTROL_BYTES],
            )?;
            if content_digest(&bytes[..CONTROL_BYTES]) != self.selected.header_digest {
                return Err(self.bound_error(
                    p,
                    Plan4RecoveryCause::ContentMismatch,
                    Effect::CallerMustResolve,
                ));
            }
        }
        selection::read_named(&mut *writer, o, name("DWB"), &mut bytes[..CONTROL_BYTES])?;
        let selected = control::decode_selector(
            &bytes[..CONTROL_BYTES],
            crate::storage::control::SelectorRole::Doublewrite,
            self.storage.profile(),
        )
        .map_err(|e| {
            self.bound_error(p, Plan4RecoveryCause::Control(e), Effect::CallerMustResolve)
        })?;
        if selected != expected || last_issued < self.doublewrite.last_issued {
            return Err(self.bound_error(
                p,
                Plan4RecoveryCause::Invariant,
                Effect::CallerMustResolve,
            ));
        }
        selection::read_named(
            &mut *writer,
            o,
            name(if selected.reference.slot == 0 {
                "d3-dwb-0"
            } else {
                "d3-dwb-1"
            }),
            bytes,
        )?;
        let view = control::decode_selected_doublewrite(
            &bytes[..selected.reference.encoded_len as usize],
            selected.reference,
            self.storage.profile(),
        )
        .map_err(|e| {
            self.bound_error(p, Plan4RecoveryCause::Control(e), Effect::CallerMustResolve)
        })?;
        if view.header().state != dwb::ControlState::Empty
            || view.len() != 0
            || view.header().batch_id != last_issued
        {
            return Err(self.bound_error(
                p,
                Plan4RecoveryCause::Invariant,
                Effect::CallerMustResolve,
            ));
        }
        for path in BASE_NAMES {
            selection::read_named(&mut *writer, o, name(path), &mut scratch.old_base[..])?;
            if scratch.old_base.as_slice() != self.base.view.bytes().as_slice() {
                return Err(self.bound_error(
                    p,
                    Plan4RecoveryCause::ContentMismatch,
                    Effect::CallerMustResolve,
                ));
            }
        }
        selection::stable_inventory(&*writer, o, self.selected.name, self.storage.profile())?;
        o.source_lsn = prefix.complete_lsn;
        o.source_end = prefix.complete_offset;
        Ok(())
    }
}
