//! Recovery-only checked tuples; this never creates a live flusher capability.
use super::*;
use crate::storage::page::PageLsn;
const BANKS: [&str; 2] = ["d3-dwb-0", "d3-dwb-1"];
impl<W: WriterNamespace> ClosedRecovery<'_, '_, W> {
    pub(super) fn read_selected(&mut self, p: RecoveryPhase) -> Result<()> {
        let raw = &mut self.flush[SELECTOR_BYTES_AT..SELECTOR_BYTES_AT + 4096];
        port_io::with_file(
            &mut self.prefix.writer,
            &mut self.allowance,
            &mut self.observation,
            p,
            "DWB",
            OpenMode::ReadOnly,
            Some(4096),
            None,
            |f, _, o| port_io::read(f, o, p, "DWB", 0, raw),
        )?;
        let selected = control::decode_selector(
            raw,
            SelectorRole::Doublewrite,
            self.prefix.observation.profile,
        )
        .map_err(|e| fail(p, RecoveryCause::Control(e)))?;
        if selected != self.observation.known {
            return Err(fail(p, RecoveryCause::IdentityMismatch));
        }
        let path = BANKS[selected.reference.slot as usize];
        let len = selected.reference.encoded_len as usize;
        let raw = &mut self.flush[CONTROL..CONTROL + len];
        port_io::with_file(
            &mut self.prefix.writer,
            &mut self.allowance,
            &mut self.observation,
            p,
            path,
            OpenMode::ReadOnly,
            Some(PAGE_BYTES as u64),
            None,
            |f, _, o| port_io::read(f, o, p, path, 0, raw),
        )?;
        let view = control::decode_selected_doublewrite(
            raw,
            selected.reference,
            self.prefix.observation.profile,
        )
        .map_err(|e| fail(p, RecoveryCause::Control(e)))?;
        if view.header().batch_id != self.observation.batch
            || view.header().state != self.observation.state
        {
            return Err(fail(p, RecoveryCause::IdentityMismatch));
        }
        Ok(())
    }
    pub(super) fn select_control(
        &mut self,
        batch: u64,
        state: ControlState,
        pending: Option<PendingImage>,
        p: RecoveryPhase,
    ) -> Result<()> {
        if self.source_seal.is_none() || self.slots.is_none() {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        if self
            .reservation
            .as_ref()
            .is_none_or(|g| batch > g.last_batch)
        {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        let generation = batch
            .checked_mul(3)
            .and_then(|v| match state {
                ControlState::Pending => Some(v),
                ControlState::Empty if batch > self.observation.batch => v.checked_sub(1),
                ControlState::Empty => v.checked_add(1),
            })
            .ok_or_else(|| overflow(p))?;
        if generation <= self.observation.known.generation {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        let slot = ((generation - 1) % 2) as u8;
        let header = ControlHeader {
            namespace: self.prefix.observation.namespace(),
            generation,
            batch_id: batch,
            state,
            max_lsn: pending.map_or(PageLsn(0), |v| v.lsn),
        };
        let raw = &mut self.flush[CONTROL..CONTROL + PAGE_BYTES];
        raw.fill(0);
        let len = dwb::encode_control(header, pending.as_slice(), raw)
            .map_err(|e| fail(p, RecoveryCause::Doublewrite(e)))?
            .len();
        let selected = SelectedControl {
            generation,
            reference: ControlRef {
                kind: ControlKind::Doublewrite,
                slot,
                index: 0,
                generation,
                encoded_len: len as u32,
                digest: content_digest(&raw[..len]),
            },
        };
        control::encode_selector(
            SelectorRole::Doublewrite,
            selected,
            self.prefix.observation.profile,
            &mut self.flush[SELECTOR_BYTES_AT..SELECTOR_BYTES_AT + 4096],
        )
        .map_err(|e| fail(p, RecoveryCause::Control(e)))?;
        for path in ["d3-dwb-stage", BANKS[slot as usize]] {
            let raw = &self.flush[CONTROL..CONTROL + PAGE_BYTES];
            port_io::with_file(
                &mut self.prefix.writer,
                &mut self.allowance,
                &mut self.observation,
                p,
                path,
                OpenMode::ReadWrite,
                Some(PAGE_BYTES as u64),
                None,
                |f, _, o| {
                    port_io::write(f, o, p, path, 0, raw)?;
                    port_io::file_sync(f, o, p, path)
                },
            )?;
        }
        let stage = port_io::metadata(
            &self.prefix.writer,
            &mut self.observation,
            p,
            "d3-dwb-select-stage",
        )?;
        if stage.is_some_and(|s| s.length > 4096) {
            return Err(fail(p, RecoveryCause::LengthMismatch));
        }
        if stage.is_none() {
            port_io::create_closed(
                &mut self.prefix.writer,
                &mut self.allowance,
                &mut self.observation,
                p,
                "d3-dwb-select-stage",
            )?;
        }
        let raw = &self.flush[SELECTOR_BYTES_AT..SELECTOR_BYTES_AT + 4096];
        let stamp = port_io::with_file(
            &mut self.prefix.writer,
            &mut self.allowance,
            &mut self.observation,
            p,
            "d3-dwb-select-stage",
            OpenMode::ReadWrite,
            Some(stage.map_or(0, |s| s.length)),
            stage,
            |f, opened, o| {
                port_io::write(f, o, p, "d3-dwb-select-stage", 0, raw)?;
                port_io::file_sync(f, o, p, "d3-dwb-select-stage")?;
                Ok(FileMetadata {
                    length: 4096,
                    ..opened
                })
            },
        )?;
        let named = port_io::metadata(
            &self.prefix.writer,
            &mut self.observation,
            p,
            "d3-dwb-select-stage",
        )?
        .ok_or_else(|| fail(p, RecoveryCause::IdentityMismatch))?;
        port_io::same(named, stamp, p)?;
        // Known credit moves only after successful exact metadata. On uncertain
        // publish both selections remain possible and stage credit stays charged.
        let credit = self.observation.ledger.dwb_stage_credit;
        self.observation.ledger.actual_owned += credit;
        self.observation.ledger.outstanding -= credit;
        self.observation.ledger.dwb_stage_actual = 4096;
        self.observation.ledger.dwb_stage_credit = 0;
        self.observation.possible = [Some(self.observation.known), Some(selected)];
        self.observation.effects.dwb_publish_attempted += 1;
        port_io::begin(&mut self.observation, p, CallKind::Publish, "DWB", 0, 4096)?;
        self.prefix
            .writer
            .publish_selector(&filename("d3-dwb-select-stage"), Selector::Doublewrite)
            .map_err(|e| fail(p, RecoveryCause::Namespace(e)))?;
        port_io::done(&mut self.observation, CallKind::Publish);
        self.observation.effects.dwb_publish_completed += 1;
        port_io::directory_sync(&mut self.prefix.writer, &mut self.observation, p)?;
        self.observation.known = selected;
        self.observation.possible = [Some(selected), None];
        self.observation.batch = batch;
        self.observation.state = state;
        self.observation.ledger.actual_owned -= 4096;
        self.observation.ledger.outstanding += 4096;
        self.observation.ledger.dwb_stage_actual = 0;
        self.observation.ledger.dwb_stage_credit = 4096;
        Ok(())
    }
    pub(super) fn encode_and_write_slot(
        &mut self,
        batch: u64,
        pending: PendingImage,
        index: usize,
    ) -> Result<()> {
        let p = RecoveryPhase::Flush;
        let generation = batch.checked_mul(3).ok_or_else(|| overflow(p))?;
        let (left, right) = self.flush.split_at_mut(CONTROL);
        let len = dwb::encode_control(
            ControlHeader {
                namespace: self.prefix.observation.namespace(),
                generation,
                batch_id: batch,
                state: ControlState::Pending,
                max_lsn: pending.lsn,
            },
            &[pending],
            &mut right[..PAGE_BYTES],
        )
        .map_err(|e| fail(p, RecoveryCause::Doublewrite(e)))?
        .len();
        let view = dwb::decode_control(
            &right[..len],
            dwb::ControlExpectation {
                namespace: self.prefix.observation.namespace(),
                generation,
            },
        )
        .map_err(|e| fail(p, RecoveryCause::Doublewrite(e)))?;
        let image = &self.maintenance_tail[index * PAGE_BYTES..(index + 1) * PAGE_BYTES];
        let raw = dwb::encode_slot(&view, 0, image, &mut left[SLOT..SLOT + dwb::SLOT_BYTES])
            .map_err(|e| fail(p, RecoveryCause::Doublewrite(e)))?;
        port_io::with_file(
            &mut self.prefix.writer,
            &mut self.allowance,
            &mut self.observation,
            p,
            "d3-dwb-images",
            OpenMode::ReadWrite,
            Some(16_711_680),
            None,
            |f, _, o| {
                port_io::write(f, o, p, "d3-dwb-images", 0, raw)?;
                port_io::file_sync(f, o, p, "d3-dwb-images")
            },
        )
    }
}
