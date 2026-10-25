//! Selected Pending repair and one-target original DWB cycles. The executor
//! never issues a live flush receipt or adopts a descriptor/remaining ledger.
use super::*;
use crate::storage::{
    control::{ControlKind, ControlRef, SelectorRole},
    page::PageLsn,
};

const CANDIDATE: usize = PAGE_BYTES;
const SLOT: usize = 2 * PAGE_BYTES;
const SELECTOR: usize = SLOT + dwb::SLOT_BYTES;
const READBACK: usize = SELECTOR + CONTROL_BYTES;
const HOME: usize = READBACK + PAGE_BYTES;
const BANKS: [&str; 2] = ["d3-dwb-0", "d3-dwb-1"];

pub(super) fn observe_pending(
    profile: Profile,
    selected: SelectedControl,
    raw: &[u8],
    req: crate::storage::dirty_frame::TargetRequest,
    image: &[u8; PAGE_BYTES],
    matched: &mut u64,
) -> Result<()> {
    let p = Plan4RecoveryPhase::Analysis;
    let index = req.identity.page.page_no as usize;
    let view = control::decode_selected_doublewrite(
        &raw[..selected.reference.encoded_len as usize],
        selected.reference,
        profile,
    )
    .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
    for n in 0..view.len() {
        let pending = view.entry(n).expect("validated pending index");
        if pending.page.page_no as usize != index {
            continue;
        }
        if pending.page != req.identity.page
            || pending.owner != req.identity.owner
            || pending.owner_generation != req.identity.owner_generation
            || pending.kind != req.kind
            || pending.level != 0
        {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        if analysis::u64_at(image, 64) == pending.lsn.0
            && content_digest(image) == pending.image_digest
        {
            *matched |= 1u64 << index;
        }
    }
    Ok(())
}

impl<W: WriterNamespace> RecoveryState<'_, '_, '_, W> {
    pub(super) fn reserve_physical(&mut self) -> Result<()> {
        let p = Plan4RecoveryPhase::Analysis;
        let analyzed = self
            .analysis
            .as_ref()
            .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
        let current = self.doublewrite_header.expect("selected DWB");
        if let Some(selector) = self.current()? {
            selector
                .generation
                .checked_add(12)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
            selector
                .base_high_water
                .checked_add(1)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
            selector
                .writer_high_water
                .checked_add(1)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
            selector
                .issued_epoch
                .checked_add(1)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        }
        // Every touched target may require one full physical image, independently
        // of its WAL price. All bases/stages/header coexistence is prepaid now.
        let topup = u64::from(analyzed.target_mask.count_ones()) * PAGE_BYTES as u64;
        let growth = 2 * BASE_BYTES as u64 + 4 * CONTROL_BYTES as u64;
        let owned = self
            .observation
            .actual_owned
            .checked_add(growth)
            .and_then(|v| v.checked_add(analyzed.remaining_wal))
            .and_then(|v| v.checked_add(topup))
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        if owned > self.profile().owned_bytes()
            || self
                .observation
                .actual_wal
                .checked_add(analyzed.remaining_wal)
                .and_then(|v| v.checked_add(CONTROL_BYTES as u64))
                .is_none_or(|v| v > WAL_CAP)
        {
            return Err(error(p, Plan4RecoveryCause::Capacity));
        }
        current
            .batch_id
            .checked_add(u64::from(analyzed.target_mask.count_ones()) + 1)
            .and_then(|v| v.checked_mul(3))
            .and_then(|v| v.checked_add(1))
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        if self.restored.is_some() {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        }
        self.restored = Some(RestoredRetention {
            state_digest: analyzed.state_digest,
            remaining_wal: analyzed.remaining_wal,
            remaining_steps: analyzed.remaining_steps,
            physical_topup: topup,
            metadata_growth: growth,
        });
        Ok(())
    }
    pub(super) fn stabilize_source(&mut self) -> Result<()> {
        let p = Plan4RecoveryPhase::Analysis;
        self.observation.phase = p;
        if let Some(source) = self.source.as_mut() {
            epoch::file_sync(&mut source.file, &mut self.observation, source.name)?;
            epoch::directory_sync(&mut self.writer, &mut self.observation)?;
            if selection::metadata(&self.writer, &mut self.observation, source.name)?
                != Some(source.metadata)
            {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
            let facts = self
                .source_facts
                .as_ref()
                .ok_or_else(|| error(p, Plan4RecoveryCause::Invariant))?;
            if analysis::prefix_hash(
                source,
                &mut self.observation,
                facts.end,
                &mut self.slot[..65_536],
            )? != facts.prefix_digest
            {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
        } else {
            epoch::directory_sync(&mut self.writer, &mut self.observation)?;
        }
        Ok(())
    }
    pub(super) fn verify_pending_slot(&mut self) -> Result<()> {
        let p = self.observation.phase;
        let header = self.doublewrite_header.expect("selected DWB");
        if header.state == dwb::ControlState::Empty {
            return Ok(());
        }
        let selected = self.doublewrite.expect("selected DWB");
        let profile = self.profile();
        let pending = {
            let view = control::decode_selected_doublewrite(
                &self.flush[..selected.reference.encoded_len as usize],
                selected.reference,
                profile,
            )
            .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
            if view.len() != 1 {
                return Err(error(p, Plan4RecoveryCause::Unsupported));
            }
            view.entry(0).expect("one selected image")
        };
        let index = pending.page.page_no as usize;
        if index >= 64
            || self
                .analysis
                .as_ref()
                .expect("complete analysis")
                .pending_matched
                & (1u64 << index)
                == 0
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        let path = name("d3-dwb-images");
        {
            let (mut file, opened) = selection::open(
                &mut self.writer,
                &mut self.observation,
                path,
                OpenMode::ReadOnly,
                Some(16_711_680),
            )?;
            selection::read(
                &mut file,
                &mut self.observation,
                path,
                u64::from(pending.slot) * dwb::SLOT_BYTES as u64,
                &mut self.flush[SLOT..SLOT + dwb::SLOT_BYTES],
            )?;
            if selection::metadata(&self.writer, &mut self.observation, path)? != Some(opened) {
                return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
            }
        }
        let view = control::decode_selected_doublewrite(
            &self.flush[..selected.reference.encoded_len as usize],
            selected.reference,
            profile,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        dwb::decode_slot(&view, 0, &self.flush[SLOT..SLOT + dwb::SLOT_BYTES])
            .map_err(|e| error(p, Plan4RecoveryCause::Doublewrite(e)))?;
        Ok(())
    }
    pub(super) fn physical(&mut self) -> Result<()> {
        if self.restored.is_none() {
            return Err(error(self.observation.phase, Plan4RecoveryCause::Invariant));
        }
        self.repair_pending()?;
        self.observation.phase = Plan4RecoveryPhase::Redo;
        let mask = self.analysis.as_ref().expect("analysis").target_mask;
        for index in 0..64 {
            if mask & (1u64 << index) == 0 {
                continue;
            }
            let image = super::super::plan4_pages::retention_replay_image(
                (&self.maintenance[REPLAY_START..REPLAY_END])
                    .try_into()
                    .expect("replay"),
                index,
            );
            if image
                == &self.maintenance
                    [PHYSICAL_START + index * PAGE_BYTES..PHYSICAL_START + (index + 1) * PAGE_BYTES]
            {
                continue;
            }
            self.flush_final(index)?;
        }
        self.verify_physical()
    }
    fn repair_pending(&mut self) -> Result<()> {
        let p = Plan4RecoveryPhase::Pending;
        self.observation.phase = p;
        let header = self.doublewrite_header.expect("DWB");
        if header.state == dwb::ControlState::Empty {
            return Ok(());
        }
        let selected = self.doublewrite.expect("DWB selected");
        let profile = self.profile();
        let count = control::decode_selected_doublewrite(
            &self.flush[..selected.reference.encoded_len as usize],
            selected.reference,
            profile,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?
        .len();
        // This closed producer uses only one-image selections. Reject complete
        // unsupported geometry before any repair, even if each image looks valid.
        if count != 1 {
            return Err(error(p, Plan4RecoveryCause::Unsupported));
        }
        let pending = control::decode_selected_doublewrite(
            &self.flush[..selected.reference.encoded_len as usize],
            selected.reference,
            profile,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?
        .entry(0)
        .expect("one entry");
        let index = pending.page.page_no as usize;
        if index >= 64
            || self.analysis.as_ref().expect("analysis").pending_matched & (1u64 << index) == 0
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        {
            let (mut file, _) = selection::open(
                &mut self.writer,
                &mut self.observation,
                name("d3-dwb-images"),
                OpenMode::ReadOnly,
                Some(16_711_680),
            )?;
            selection::read(
                &mut file,
                &mut self.observation,
                name("d3-dwb-images"),
                u64::from(pending.slot) * dwb::SLOT_BYTES as u64,
                &mut self.flush[SLOT..SLOT + dwb::SLOT_BYTES],
            )?;
        }
        let (before, rest) = self.flush.split_at_mut(SLOT);
        let control = control::decode_selected_doublewrite(
            &before[..selected.reference.encoded_len as usize],
            selected.reference,
            profile,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        let image = dwb::decode_slot(&control, 0, &rest[..dwb::SLOT_BYTES])
            .map_err(|e| error(p, Plan4RecoveryCause::Doublewrite(e)))?;
        let home = &self.maintenance
            [PHYSICAL_START + index * PAGE_BYTES..PHYSICAL_START + (index + 1) * PAGE_BYTES];
        let exact_final = analysis::target_at(self.maintenance, profile, index)?;
        let home_lsn =
            crate::storage::page::decode_page(home, exact_final.0.identity, exact_final.0.kind)
                .ok()
                .map(|v| v.header().lsn.0);
        if home != image && home_lsn.is_none_or(|lsn| lsn < pending.lsn.0) {
            // Copy into an existing caller page to end the control/slot borrows.
            self.flush.copy_within(
                SLOT + dwb::SLOT_HEADER_BYTES..SLOT + dwb::SLOT_HEADER_BYTES + PAGE_BYTES,
                HOME,
            );
            self.write_home(index, HOME)?;
        } else {
            // Equality or a later canonical home avoids a redundant write,
            // but does not prove the observed bytes have reached durable
            // storage. Keep Pending selected until that same home is synced.
            self.sync_unchanged_home(index)?;
        }
        // All selected homes are synced before this whole-batch retirement.
        self.select_dwb(header.batch_id, dwb::ControlState::Empty, None)?;
        Ok(())
    }
    fn sync_unchanged_home(&mut self, index: usize) -> Result<()> {
        let p = self.observation.phase;
        let path = name(HOME_NAME);
        let (mut file, opened) = selection::open(
            &mut self.writer,
            &mut self.observation,
            path,
            OpenMode::ReadWrite,
            Some(control::EXTENT_BYTES),
        )?;
        let offset = index as u64 * PAGE_BYTES as u64;
        selection::read(
            &mut file,
            &mut self.observation,
            path,
            offset,
            &mut self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES],
        )?;
        let prior = &self.maintenance
            [PHYSICAL_START + index * PAGE_BYTES..PHYSICAL_START + (index + 1) * PAGE_BYTES];
        if prior != &self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES] {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        epoch::file_sync(&mut file, &mut self.observation, path)?;
        selection::read(
            &mut file,
            &mut self.observation,
            path,
            offset,
            &mut self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES],
        )?;
        let prior = &self.maintenance
            [PHYSICAL_START + index * PAGE_BYTES..PHYSICAL_START + (index + 1) * PAGE_BYTES];
        if prior != &self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES]
            || selection::metadata(&self.writer, &mut self.observation, path)? != Some(opened)
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        Ok(())
    }
    fn write_home(&mut self, index: usize, image_at: usize) -> Result<()> {
        let p = self.observation.phase;
        let path = name(HOME_NAME);
        let (mut file, opened) = selection::open(
            &mut self.writer,
            &mut self.observation,
            path,
            OpenMode::ReadWrite,
            Some(control::EXTENT_BYTES),
        )?;
        let offset = index as u64 * PAGE_BYTES as u64;
        selection::read(
            &mut file,
            &mut self.observation,
            path,
            offset,
            &mut self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES],
        )?;
        let prior = &self.maintenance
            [PHYSICAL_START + index * PAGE_BYTES..PHYSICAL_START + (index + 1) * PAGE_BYTES];
        if prior != &self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES] {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        epoch::write(
            &mut file,
            &mut self.observation,
            path,
            offset,
            &self.flush[image_at..image_at + PAGE_BYTES],
        )?;
        epoch::file_sync(&mut file, &mut self.observation, path)?;
        selection::read(
            &mut file,
            &mut self.observation,
            path,
            offset,
            &mut self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES],
        )?;
        if self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES]
            != self.flush[image_at..image_at + PAGE_BYTES]
            || selection::metadata(&self.writer, &mut self.observation, path)? != Some(opened)
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        self.maintenance
            [PHYSICAL_START + index * PAGE_BYTES..PHYSICAL_START + (index + 1) * PAGE_BYTES]
            .copy_from_slice(&self.flush[image_at..image_at + PAGE_BYTES]);
        Ok(())
    }
    fn flush_final(&mut self, index: usize) -> Result<()> {
        let p = self.observation.phase;
        let profile = self.profile();
        let (request, lsn, digest) = analysis::target_at(self.maintenance, profile, index)?;
        if lsn == 0 {
            return Err(error(p, Plan4RecoveryCause::Unsupported));
        }
        let batch = self
            .doublewrite_header
            .expect("DWB")
            .batch_id
            .checked_add(1)
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        let pending = dwb::PendingImage {
            slot: 0,
            page: request.identity.page,
            owner: request.identity.owner,
            owner_generation: request.identity.owner_generation,
            kind: request.kind,
            level: 0,
            lsn: PageLsn(lsn),
            image_digest: digest,
        };
        self.select_dwb(batch, dwb::ControlState::Empty, None)?;
        self.flush[HOME..HOME + PAGE_BYTES].copy_from_slice(
            &self.maintenance
                [REPLAY_START + index * PAGE_BYTES..REPLAY_START + (index + 1) * PAGE_BYTES],
        );
        {
            let generation = batch
                .checked_mul(3)
                .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
            let header = dwb::ControlHeader {
                namespace: profile.spec().namespace,
                generation,
                batch_id: batch,
                state: dwb::ControlState::Pending,
                max_lsn: pending.lsn,
            };
            let (prior, rest) = self.flush.split_at_mut(SLOT);
            let n = dwb::encode_control(
                header,
                &[pending],
                &mut prior[CANDIDATE..CANDIDATE + PAGE_BYTES],
            )
            .map_err(|e| error(p, Plan4RecoveryCause::Doublewrite(e)))?
            .len();
            let view = dwb::decode_control(
                &prior[CANDIDATE..CANDIDATE + n],
                dwb::ControlExpectation {
                    namespace: header.namespace,
                    generation,
                },
            )
            .map_err(|e| error(p, Plan4RecoveryCause::Doublewrite(e)))?;
            let (slot, after) = rest.split_at_mut(dwb::SLOT_BYTES);
            let image: &[u8; PAGE_BYTES] = after
                [HOME - SLOT - dwb::SLOT_BYTES..HOME - SLOT - dwb::SLOT_BYTES + PAGE_BYTES]
                .try_into()
                .expect("staged image");
            dwb::encode_slot(&view, 0, image, slot)
                .map_err(|e| error(p, Plan4RecoveryCause::Doublewrite(e)))?;
        }
        epoch::write_existing(
            &mut self.writer,
            &mut self.observation,
            name("d3-dwb-images"),
            16_711_680,
            0,
            &self.flush[SLOT..SLOT + dwb::SLOT_BYTES],
        )?;
        self.select_dwb(batch, dwb::ControlState::Pending, Some(pending))?;
        self.write_home(index, HOME)?;
        self.select_dwb(batch, dwb::ControlState::Empty, None)?;
        Ok(())
    }
    fn select_dwb(
        &mut self,
        batch: u64,
        state: dwb::ControlState,
        pending: Option<dwb::PendingImage>,
    ) -> Result<()> {
        let p = self.observation.phase;
        let profile = self.profile();
        let prior = self.doublewrite_header.expect("DWB");
        let selected = self.doublewrite.expect("DWB");
        let generation = batch
            .checked_mul(3)
            .and_then(|v| match state {
                dwb::ControlState::Pending => Some(v),
                dwb::ControlState::Empty if batch > prior.batch_id => v.checked_sub(1),
                dwb::ControlState::Empty => v.checked_add(1),
            })
            .ok_or_else(|| error(p, Plan4RecoveryCause::Exhausted))?;
        let slot = ((generation - 1) % 2) as u8;
        if generation <= selected.generation || slot == selected.reference.slot {
            return Err(error(p, Plan4RecoveryCause::OrderMismatch));
        }
        let header = dwb::ControlHeader {
            namespace: profile.spec().namespace,
            generation,
            batch_id: batch,
            state,
            max_lsn: pending.map_or(PageLsn(0), |v| v.lsn),
        };
        self.flush[CANDIDATE..CANDIDATE + PAGE_BYTES].fill(0);
        let n = dwb::encode_control(
            header,
            pending.as_slice(),
            &mut self.flush[CANDIDATE..CANDIDATE + PAGE_BYTES],
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Doublewrite(e)))?
        .len();
        let next = SelectedControl {
            generation,
            reference: ControlRef {
                kind: ControlKind::Doublewrite,
                slot,
                index: 0,
                generation,
                encoded_len: n as u32,
                digest: content_digest(&self.flush[CANDIDATE..CANDIDATE + n]),
            },
        };
        control::encode_selector(
            SelectorRole::Doublewrite,
            next,
            profile,
            &mut self.flush[SELECTOR..SELECTOR + CONTROL_BYTES],
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        for path in ["d3-dwb-stage", BANKS[slot as usize]] {
            epoch::write_existing(
                &mut self.writer,
                &mut self.observation,
                name(path),
                PAGE_BYTES as u64,
                0,
                &self.flush[CANDIDATE..CANDIDATE + PAGE_BYTES],
            )?;
        }
        let (before, after) = self.flush.split_at_mut(READBACK);
        let raw: &[u8; CONTROL_BYTES] = before[SELECTOR..SELECTOR + CONTROL_BYTES]
            .try_into()
            .expect("selector");
        let readback: &mut [u8; CONTROL_BYTES] =
            (&mut after[..CONTROL_BYTES]).try_into().expect("readback");
        self.possible_doublewrite = true;
        epoch::publish(
            &mut self.writer,
            &mut self.observation,
            Selector::Doublewrite,
            name("d3-dwb-select-stage"),
            raw,
            readback,
        )?;
        self.flush.copy_within(CANDIDATE..CANDIDATE + PAGE_BYTES, 0);
        self.doublewrite = Some(next);
        self.doublewrite_header = Some(header);
        self.possible_doublewrite = false;
        Ok(())
    }
    pub(super) fn verify_physical(&mut self) -> Result<()> {
        self.observation.phase = Plan4RecoveryPhase::Verify;
        let p = self.observation.phase;
        let profile = self.profile();
        let analyzed = self.analysis.as_ref().expect("analysis");
        if content_digest(&self.maintenance[REPLAY_START..REPLAY_START + 1_179_648])
            != analyzed.state_digest
        {
            return Err(error(p, Plan4RecoveryCause::ContentMismatch));
        }
        let mask = analyzed.target_mask;
        let (mut file, opened) = selection::open(
            &mut self.writer,
            &mut self.observation,
            name(HOME_NAME),
            OpenMode::ReadOnly,
            Some(control::EXTENT_BYTES),
        )?;
        for index in 0..64 {
            if mask & (1u64 << index) == 0 {
                continue;
            }
            let (_, _, digest) = analysis::target_at(self.maintenance, profile, index)?;
            selection::read(
                &mut file,
                &mut self.observation,
                name(HOME_NAME),
                index as u64 * PAGE_BYTES as u64,
                &mut self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES],
            )?;
            if content_digest(&self.maintenance[IO_WORK..IO_WORK + PAGE_BYTES]) != digest {
                return Err(error(p, Plan4RecoveryCause::ContentMismatch));
            }
        }
        if selection::metadata(&self.writer, &mut self.observation, name(HOME_NAME))?
            != Some(opened)
        {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        if self.doublewrite_header.expect("DWB").state != dwb::ControlState::Empty {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        }
        Ok(())
    }
    pub(super) fn seal_doublewrite(&mut self) -> Result<SelectedRecoveryDwb> {
        let p = self.observation.phase;
        let profile = self.profile();
        if self.possible_doublewrite {
            return Err(error(p, Plan4RecoveryCause::Invariant));
        }
        selection::read_named(
            &mut self.writer,
            &mut self.observation,
            name("DWB"),
            &mut self.flush[READBACK..READBACK + CONTROL_BYTES],
        )?;
        let selected = control::decode_selector(
            &self.flush[READBACK..READBACK + CONTROL_BYTES],
            SelectorRole::Doublewrite,
            profile,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        if Some(selected) != self.doublewrite {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        selection::read_named(
            &mut self.writer,
            &mut self.observation,
            name(BANKS[selected.reference.slot as usize]),
            &mut self.flush[READBACK..READBACK + PAGE_BYTES],
        )?;
        let view = control::decode_selected_doublewrite(
            &self.flush[READBACK..READBACK + selected.reference.encoded_len as usize],
            selected.reference,
            profile,
        )
        .map_err(|e| error(p, Plan4RecoveryCause::Control(e)))?;
        if Some(view.header()) != self.doublewrite_header
            || view.header().state != dwb::ControlState::Empty
            || view.len() != 0
        {
            return Err(error(p, Plan4RecoveryCause::IdentityMismatch));
        }
        Ok(SelectedRecoveryDwb {
            selected,
            last_issued: view.header().batch_id,
        })
    }
}
