//! Independent physical cursors. Canonical validation always starts at genesis.
use super::*;
use crate::storage::page::PageLsn;

fn image_lsn(
    bytes: &[u8; PAGE_BYTES],
    identity: PageIdentity,
    kind: PageKind,
    p: RecoveryPhase,
) -> Result<Option<u64>> {
    // Format decoding precedes SHA in the general codec. Establish SHA ourselves
    // so unsupported but intact data is never treated as a repairable torn page.
    if content_digest(&bytes[..page::DIGEST_OFFSET]) != bytes[page::DIGEST_OFFSET..] {
        return Ok(None);
    }
    let view = page::decode_page(bytes, identity, kind)
        .map_err(|e| fail(p, RecoveryCause::PageIo(PageIoError::Page(e))))?;
    if kind == PageKind::Allocation {
        allocation_page::decode_allocation_page(
            bytes,
            identity,
            Namespace {
                database: identity.database,
                timeline: 1,
            },
        )
        .map_err(|e| fail(p, RecoveryCause::Allocation(e)))?;
    }
    if kind == PageKind::TransactionStatus {
        transactions::empty_home(
            bytes,
            identity,
            Namespace {
                database: identity.database,
                timeline: 1,
            },
        )
        .map_err(|e| fail(p, RecoveryCause::Prefix(e)))?;
    }
    Ok(Some(view.header().lsn.0))
}
pub(super) fn identity_of(
    bytes: &[u8; PAGE_BYTES],
    database: crate::api::DatabaseId,
) -> Result<PageIdentity> {
    let p = RecoveryPhase::Redo;
    Ok(PageIdentity {
        database,
        page: PageRef::decode(&bytes[24..48])
            .map_err(|e| fail(p, RecoveryCause::PageIo(PageIoError::Page(e))))?,
        owner: ObjectId::new(u64::from_le_bytes(bytes[48..56].try_into().expect("owner")))
            .map_err(|_| fail(p, RecoveryCause::Invariant))?,
        owner_generation: Generation::new(u64::from_le_bytes(
            bytes[56..64].try_into().expect("generation"),
        ))
        .map_err(|_| fail(p, RecoveryCause::Invariant))?,
    })
}
fn observe(o: &mut PhysicalObservation, index: usize, lsn: Option<u64>) {
    if o.observed_mask & (1 << index) == 0 {
        o.initial_lsn[index] = lsn;
        o.observed_mask |= 1 << index;
    }
    o.final_lsn[index] = lsn;
}
impl<W: WriterNamespace> ClosedRecovery<'_, '_, W> {
    fn canonical_target(&self, index: usize) -> Result<RecoveryTarget> {
        let p = RecoveryPhase::Redo;
        let allocator = identity_of(
            self.prefix.allocator_image(),
            self.prefix.observation.namespace().database,
        )?;
        if index == 0 {
            return Ok(RecoveryTarget {
                identity: allocator,
                lsn: self.prefix.observation.allocator_lsn,
                digest: self.final_allocator_hash,
            });
        }
        if index >= TARGET_COUNT || self.prefix.observation.status_mask & (1u64 << index) == 0 {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        let at = SAVED_TARGETS + index * TARGET_BYTES;
        let raw = &self.maintenance_tail[at..at + TARGET_BYTES];
        let incarnation = Generation::new(u64::from_le_bytes(
            raw[8..16].try_into().expect("census generation"),
        ))
        .map_err(|_| fail(p, RecoveryCause::Invariant))?;
        Ok(RecoveryTarget {
            identity: PageIdentity {
                page: PageRef {
                    page_no: index as u64,
                    incarnation,
                    ..allocator.page
                },
                owner: ObjectId::new(2).expect("Status object"),
                owner_generation: Generation::new(1).expect("Status generation"),
                ..allocator
            },
            lsn: u64::from_le_bytes(raw[..8].try_into().expect("census LSN")),
            digest: raw[16..].try_into().expect("census digest"),
        })
    }
    fn read_home(
        &mut self,
        index: usize,
        identity: PageIdentity,
        kind: PageKind,
        p: RecoveryPhase,
    ) -> Result<Option<u64>> {
        let at = index * PAGE_BYTES;
        let out = &mut self.maintenance_tail[at..at + PAGE_BYTES];
        port_io::with_file(
            &mut self.prefix.writer,
            &mut self.allowance,
            &mut self.observation,
            p,
            HOME_NAME,
            OpenMode::ReadOnly,
            Some(control::EXTENT_BYTES),
            None,
            |f, _, o| {
                port_io::read(
                    f,
                    o,
                    p,
                    HOME_NAME,
                    identity.page.page_no * PAGE_BYTES as u64,
                    out,
                )
            },
        )?;
        let lsn = image_lsn(
            (&*out).try_into().expect("physical page"),
            identity,
            kind,
            p,
        )?;
        observe(&mut self.observation.physical, index, lsn);
        Ok(lsn)
    }
    fn write_home(
        &mut self,
        index: usize,
        identity: PageIdentity,
        p: RecoveryPhase,
        write: bool,
    ) -> Result<()> {
        if self.source_seal.is_none() || self.slots.is_none() {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        let bytes = &self.maintenance_tail[index * PAGE_BYTES..(index + 1) * PAGE_BYTES];
        port_io::with_file(
            &mut self.prefix.writer,
            &mut self.allowance,
            &mut self.observation,
            p,
            HOME_NAME,
            OpenMode::ReadWrite,
            Some(control::EXTENT_BYTES),
            None,
            |f, _, o| {
                if write {
                    o.effects.home_write_attempted |= 1 << index;
                    port_io::write(
                        f,
                        o,
                        p,
                        HOME_NAME,
                        identity.page.page_no * PAGE_BYTES as u64,
                        bytes,
                    )?;
                    o.effects.home_written |= 1 << index;
                }
                o.effects.home_sync_attempted |= 1 << index;
                port_io::file_sync(f, o, p, HOME_NAME)?;
                o.effects.home_synced |= 1 << index;
                Ok(())
            },
        )
    }
    pub(super) fn repair_pending(&mut self) -> Result<()> {
        let Some(pending) = self.prefix.observation.pending else {
            return Ok(());
        };
        let p = RecoveryPhase::PendingValidate;
        self.read_selected(p)?;
        let len = self.observation.known.reference.encoded_len as usize;
        let slot = &mut self.flush[SLOT..SLOT + dwb::SLOT_BYTES];
        port_io::with_file(
            &mut self.prefix.writer,
            &mut self.allowance,
            &mut self.observation,
            p,
            "d3-dwb-images",
            OpenMode::ReadOnly,
            Some(16_711_680),
            None,
            |f, _, o| port_io::read(f, o, p, "d3-dwb-images", 0, slot),
        )?;
        let (left, right) = self.flush.split_at_mut(SLOT);
        let view = dwb::decode_control(
            &right[CONTROL - SLOT..CONTROL - SLOT + len],
            dwb::ControlExpectation {
                namespace: self.prefix.observation.namespace(),
                generation: self.observation.known.generation,
            },
        )
        .map_err(|e| fail(p, RecoveryCause::Doublewrite(e)))?;
        let image = dwb::decode_slot(&view, 0, &right[..dwb::SLOT_BYTES])
            .map_err(|e| fail(p, RecoveryCause::Doublewrite(e)))?;
        left[..PAGE_BYTES].copy_from_slice(image);
        let identity = PageIdentity {
            database: self.prefix.observation.namespace().database,
            page: pending.page,
            owner: pending.owner,
            owner_generation: pending.owner_generation,
        };
        let index =
            usize::try_from(pending.page.page_no).map_err(|_| fail(p, RecoveryCause::Invariant))?;
        let target = self.canonical_target(index)?;
        if target.identity != identity || pending.lsn.0 > target.lsn {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        let lsn = self.read_home(index, identity, pending.kind, p)?;
        let image = &self.flush[..PAGE_BYTES];
        let home = &mut self.maintenance_tail[index * PAGE_BYTES..(index + 1) * PAGE_BYTES];
        let write = match lsn {
            Some(lsn) if lsn > pending.lsn.0 => return Err(fail(p, RecoveryCause::PriorMismatch)),
            Some(lsn) if lsn == pending.lsn.0 => {
                if home != image {
                    return Err(fail(p, RecoveryCause::Invariant));
                }
                false
            }
            _ => true,
        };
        if write {
            home.copy_from_slice(image);
        }
        self.write_home(index, identity, RecoveryPhase::PendingHome, write)?;
        self.observation.physical.pending_repaired = write;
        self.observation.physical.pending_home_skipped = !write;
        self.select_control(
            self.observation.batch,
            ControlState::Empty,
            None,
            RecoveryPhase::Retirement,
        )?;
        self.observation.physical.pending_retired = true;
        Ok(())
    }
    pub(super) fn redo(&mut self) -> Result<()> {
        let p = RecoveryPhase::Redo;
        let f = &self.prefix.observation;
        let database = f.namespace().database;
        let allocator = identity_of(self.prefix.allocator_image(), database)?;
        let adopted = f.complete_lsn;
        let a = self.read_home(0, allocator, PageKind::Allocation, p)?;
        if a.is_none() {
            return Err(fail(p, RecoveryCause::PriorMismatch));
        }
        if a.is_some_and(|v| v > self.prefix.observation.allocator_lsn) {
            return Err(fail(p, RecoveryCause::PriorMismatch));
        }
        for index in 1..TARGET_COUNT {
            if self.prefix.observation.status_mask & (1u64 << index) == 0 {
                continue;
            }
            let target = self.canonical_target(index)?;
            let lsn = self.read_home(index, target.identity, PageKind::TransactionStatus, p)?;
            if lsn.is_some_and(|v| v > target.lsn) {
                return Err(fail(p, RecoveryCause::PriorMismatch));
            }
        }
        let f = &self.prefix.observation;
        let mut replay = RecoveryPrefixObservation {
            profile: f.profile,
            plan: f.plan,
            filemap: f.filemap,
            doublewrite: f.doublewrite,
            dwb_header: f.dwb_header,
            pending: None,
            source: f.source,
            complete_offset: 4096,
            complete_lsn: 0,
            allocator_lsn: 0,
            transactions: transactions::Transactions::default(),
            tail: None,
            mode: None,
            active: None,
            high_water: 0,
            inventory_items: 0,
            inventory_digest: content_digest(&[]),
            remaining: RemainingBudget::default(),
            retained: f.retained,
            owned: 0,
            protected: 0,
            terminal_hold: 0,
            scope: f.scope,
            terminal: TerminalOutcome::Cancelled,
            status_mask: 0,
            last_status_page: 0,
            pending_matched: true,
            stage_bytes: f.stage_bytes,
            replay_topup: 0,
        };
        {
            let (pages, rest) =
                self.prefix.workspace[BOOT..BOOT + 3 * PAGE_BYTES].as_chunks_mut::<PAGE_BYTES>();
            if !rest.is_empty() {
                return Err(fail(p, RecoveryCause::Invariant));
            }
            let scratch: &mut [[u8; PAGE_BYTES]; 3] =
                pages.try_into().expect("three bootstrap pages");
            let kind = if replay.plan == 1 {
                bootstrap::BootstrapPlanKind::AllocatorOnly
            } else {
                bootstrap::BootstrapPlanKind::StatusBootstrap
            };
            bootstrap::BootstrapPlan::prepare_for_kind(replay.profile, scratch, kind).map_err(
                |e| {
                    fail(
                        p,
                        RecoveryCause::Prefix(super::super::error(
                            super::super::Phase::Bootstrap,
                            super::super::Cause::Bootstrap(e),
                        )),
                    )
                },
            )?;
        }
        self.prefix
            .workspace
            .copy_within(BOOT..BOOT + PAGE_BYTES, ALLOC);
        let physical = &mut self.maintenance_tail[..PHYSICAL_BYTES];
        let progress = &mut self.observation.physical;
        let mut first = None;
        let mut source = port_io::SourceReads {
            file: &mut self.prefix.source,
            effects: &mut self.observation.effects,
        };
        let scan = super::super::analyze_with(
            &mut source,
            self.prefix.workspace,
            self.prefix.transaction_arena.as_deref_mut(),
            &mut replay,
            allocator,
            |header, frame, change| {
                if first.is_some() {
                    return false;
                }
                let apply = (|| {
                    let (after, status_after) = match change {
                        CanonicalChange::System { allocator, status } => (allocator, status),
                        CanonicalChange::Transaction { before, after } => {
                            let redo = wal_frame::decode_redo_record(header, &frame[2720..3008])
                                .map_err(|e| fail(p, RecoveryCause::Wal(e)))?;
                            let index = usize::try_from(redo.target.page.page_no)
                                .map_err(|_| fail(p, RecoveryCause::Invariant))?;
                            if !(1..TARGET_COUNT).contains(&index) {
                                return Err(fail(p, RecoveryCause::Invariant));
                            }
                            let current = progress.final_lsn[index]
                                .ok_or_else(|| fail(p, RecoveryCause::PriorMismatch))?;
                            if current >= header.lsn {
                                progress.skipped[index] += 1;
                            } else {
                                let image =
                                    &mut physical[index * PAGE_BYTES..(index + 1) * PAGE_BYTES];
                                if current != redo.prior_lsn || image != before {
                                    return Err(fail(p, RecoveryCause::PriorMismatch));
                                }
                                image.copy_from_slice(after);
                                progress.final_lsn[index] = Some(header.lsn);
                                progress.applied[index] += 1;
                                progress.dirty_mask |= 1u64 << index;
                            }
                            return Ok(());
                        }
                    };
                    let raw = if frame.len() == init::FRAME_BYTES {
                        &frame[15248..21520]
                    } else {
                        let n = get16(frame, 290) as usize;
                        let at = 14688 + 208 * n;
                        &frame[at..at + 6272]
                    };
                    let redo = wal_frame::decode_redo_record(header, raw)
                        .map_err(|e| fail(p, RecoveryCause::Wal(e)))?;
                    let current = progress.final_lsn[0]
                        .ok_or_else(|| fail(p, RecoveryCause::PriorMismatch))?;
                    if current >= header.lsn {
                        progress.skipped[0] += 1;
                    } else {
                        if current != redo.prior_lsn
                            || physical[10144..16352] != frame[2272..8480]
                            || physical[..64] != after[..64]
                            || physical[72..10144] != after[72..10144]
                        {
                            return Err(fail(p, RecoveryCause::PriorMismatch));
                        }
                        physical[..PAGE_BYTES].copy_from_slice(after);
                        progress.final_lsn[0] = Some(header.lsn);
                        progress.applied[0] += 1;
                        progress.dirty_mask |= 1;
                    }
                    if let Some(after) = status_after {
                        let init_record =
                            wal_frame::decode_redo_record(header, &frame[21520..37968])
                                .map_err(|e| fail(p, RecoveryCause::Wal(e)))?;
                        let identity = identity_of(after, database)?;
                        let index = usize::try_from(identity.page.page_no)
                            .map_err(|_| fail(p, RecoveryCause::Invariant))?;
                        if !(1..TARGET_COUNT).contains(&index) {
                            return Err(fail(p, RecoveryCause::Invariant));
                        }
                        if progress.final_lsn[index].is_some_and(|v| v >= header.lsn) {
                            progress.skipped[index] += 1;
                        } else {
                            if progress.final_lsn[index].is_some_and(|v| v != init_record.prior_lsn)
                            {
                                return Err(fail(p, RecoveryCause::PriorMismatch));
                            }
                            physical[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                                .copy_from_slice(after);
                            progress.final_lsn[index] = Some(header.lsn);
                            progress.applied[index] += 1;
                            progress.dirty_mask |= 1u64 << index;
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = apply {
                    first = Some(e);
                    false
                } else {
                    true
                }
            },
        );
        if let Some(e) = first {
            return Err(e);
        }
        scan.map_err(|e| fail(p, RecoveryCause::Prefix(e)))?;
        self.observation.physical.canonical_passes = 1;
        if replay.complete_offset != self.prefix.observation.complete_offset
            || replay.complete_lsn != adopted
            || replay.allocator_lsn != self.prefix.observation.allocator_lsn
            || replay.transactions != self.prefix.observation.transactions
            || transactions::bank_digest(
                self.prefix
                    .transaction_arena
                    .as_deref()
                    .ok_or_else(|| fail(p, RecoveryCause::Invariant))?,
            ) != self.final_transaction_hash
            || replay.high_water != self.prefix.observation.high_water
            || replay.remaining != self.prefix.observation.remaining
            || replay.terminal_hold != self.prefix.observation.terminal_hold
            || replay.mode != self.prefix.observation.mode
            || replay.active != self.prefix.observation.active
            || replay.inventory_items != self.prefix.observation.inventory_items
            || replay.inventory_digest != self.prefix.observation.inventory_digest
            || replay.terminal != self.prefix.observation.terminal
            || replay.status_mask != self.prefix.observation.status_mask
            || replay.last_status_page != self.prefix.observation.last_status_page
            || content_digest(self.prefix.allocator_image()) != self.final_allocator_hash
            || self.prefix.target_table()
                != &self.maintenance_tail[SAVED_TARGETS..SAVED_TARGETS + TARGET_TABLE_BYTES]
            || self.prefix.descriptor_bytes().map(|b| content_digest(b))
                != self.final_descriptor_hash
        {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        for index in 0..TARGET_COUNT {
            if index != 0 && self.prefix.observation.status_mask & (1u64 << index) == 0 {
                continue;
            }
            let target = self.canonical_target(index)?;
            if self.observation.physical.final_lsn[index] != Some(target.lsn)
                || content_digest(
                    &self.maintenance_tail[index * PAGE_BYTES..(index + 1) * PAGE_BYTES],
                ) != target.digest
            {
                return Err(fail(p, RecoveryCause::Invariant));
            }
        }
        // No new token is adopted: replay is a temporary canonical cursor only.
        Ok(())
    }
    pub(super) fn flush_dirty(&mut self) -> Result<()> {
        let p = RecoveryPhase::Flush;
        if self.source_seal.is_none() || self.restored.is_none() {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        self.source_snapshot(p)?;
        for index in 0..TARGET_COUNT {
            if self.observation.physical.dirty_mask & (1 << index) == 0 {
                continue;
            }
            let image: &[u8; PAGE_BYTES] = self.maintenance_tail
                [index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
                .try_into()
                .expect("physical page");
            let identity = self.canonical_target(index)?.identity;
            let kind = if index == 0 {
                PageKind::Allocation
            } else {
                PageKind::TransactionStatus
            };
            let pending = PendingImage {
                slot: 0,
                page: identity.page,
                owner: identity.owner,
                owner_generation: identity.owner_generation,
                kind,
                level: 0,
                lsn: PageLsn(
                    self.observation.physical.final_lsn[index]
                        .ok_or_else(|| fail(p, RecoveryCause::Invariant))?,
                ),
                image_digest: content_digest(image),
            };
            let batch = self
                .observation
                .batch
                .checked_add(1)
                .ok_or_else(|| overflow(p))?;
            self.select_control(batch, ControlState::Empty, None, p)?;
            self.encode_and_write_slot(batch, pending, index)?;
            self.select_control(batch, ControlState::Pending, Some(pending), p)?;
            self.write_home(index, identity, p, true)?;
            self.select_control(batch, ControlState::Empty, None, p)?;
            self.observation.physical.flushed_mask |= 1 << index;
        }
        Ok(())
    }
    pub(super) fn verify(&mut self) -> Result<()> {
        let p = RecoveryPhase::Verify;
        for index in 0..TARGET_COUNT {
            if index != 0 && self.prefix.observation.status_mask & (1u64 << index) == 0 {
                continue;
            }
            let target = self.canonical_target(index)?;
            let identity = target.identity;
            let kind = if index == 0 {
                PageKind::Allocation
            } else {
                PageKind::TransactionStatus
            };
            let lsn = self.read_home(index, identity, kind, p)?;
            let bytes = &self.maintenance_tail[index * PAGE_BYTES..(index + 1) * PAGE_BYTES];
            if lsn != Some(target.lsn) || content_digest(bytes) != target.digest {
                return Err(fail(p, RecoveryCause::Invariant));
            }
        }
        self.read_selected(p)?;
        self.source_snapshot(RecoveryPhase::Snapshot)?;
        if !self.observation.ledger.restored_once
            || self.observation.ledger.replay_topup != self.prefix.observation.replay_topup
        {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        let restored = self
            .restored
            .as_ref()
            .ok_or_else(|| fail(p, RecoveryCause::Invariant))?;
        let reservation = self
            .reservation
            .as_ref()
            .ok_or_else(|| fail(p, RecoveryCause::Invariant))?;
        if restored.lsn != self.prefix.observation.complete_lsn
            || restored.descriptor != self.final_descriptor_hash
            || restored.transactions != self.final_transaction_hash
            || transactions::bank_digest(
                self.prefix
                    .transaction_arena
                    .as_deref()
                    .ok_or_else(|| fail(p, RecoveryCause::Invariant))?,
            ) != restored.transactions
            || restored.remaining != self.prefix.observation.remaining
            || restored.topup != self.observation.ledger.replay_topup
            || reservation.remaining.is_some()
            || reservation.owned_peak != self.observation.ledger.owned_peak
            || reservation.protected != self.observation.ledger.protected
        {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        self.observation.phase = RecoveryPhase::Snapshot;
        Ok(())
    }
}
