//! Reserved-only files carry no selector authority. Rewrite both on every entry.
use super::*;
impl<W: WriterNamespace> ClosedRecovery<'_, '_, W> {
    pub(super) fn validate_zero_slots(&mut self) -> Result<()> {
        let p = RecoveryPhase::Inventory;
        for (i, path) in SLOT_NAMES.into_iter().enumerate() {
            if let Some(stamp) = self.observation.boot.initial_stamps[i] {
                if stamp.length > SLOT_CAP {
                    return Err(fail(p, RecoveryCause::LengthMismatch));
                }
                let chunk = &mut self.flush[..ZERO_CHUNK];
                port_io::with_file(
                    &mut self.prefix.writer,
                    &mut self.allowance,
                    &mut self.observation,
                    p,
                    path,
                    OpenMode::ReadOnly,
                    Some(stamp.length),
                    Some(stamp),
                    |file, _, o| {
                        let mut offset = 0;
                        while offset < stamp.length {
                            let count = ZERO_CHUNK.min((stamp.length - offset) as usize);
                            port_io::read(file, o, p, path, offset, &mut chunk[..count])?;
                            if chunk[..count].iter().any(|b| *b != 0) {
                                return Err(fail(p, RecoveryCause::Unsupported));
                            }
                            offset += count as u64;
                        }
                        Ok(())
                    },
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn source_snapshot(&mut self, p: RecoveryPhase) -> Result<()> {
        let current =
            port_io::metadata(&self.prefix.writer, &mut self.observation, p, wal::WAL_NAME)?
                .ok_or_else(|| fail(p, RecoveryCause::IdentityMismatch))?;
        port_io::same(current, self.prefix.observation.source, p)
    }
    pub(super) fn provision(&mut self) -> Result<()> {
        let p = RecoveryPhase::BootstrapProvision;
        self.flush[..ZERO_CHUNK].fill(0);
        for (i, path) in SLOT_NAMES.into_iter().enumerate() {
            let before = self.observation.boot.initial_stamps[i];
            if before.is_none() {
                self.observation.effects.slot_create_attempted |= 1 << i;
                port_io::create_closed(
                    &mut self.prefix.writer,
                    &mut self.allowance,
                    &mut self.observation,
                    p,
                    path,
                )?;
                self.observation.effects.slot_created |= 1 << i;
            }
            let initial_length = before.map_or(0, |m| m.length);
            let zero = &self.flush[..ZERO_CHUNK];
            let stamp = port_io::with_file(
                &mut self.prefix.writer,
                &mut self.allowance,
                &mut self.observation,
                p,
                path,
                OpenMode::ReadWrite,
                Some(initial_length),
                before,
                |file, opened, o| {
                    // For absent files this reopened length0/named identity is the
                    // first stamp: FileIo never exposed created-handle metadata.
                    o.boot.provision_stamps[i] = Some(opened);
                    o.effects.slot_zero_started |= 1 << i;
                    for n in 0..16 {
                        port_io::write(file, o, p, path, n * ZERO_CHUNK as u64, zero)?;
                        o.effects.slot_chunks_done[i] += 1;
                    }
                    o.effects.slot_sync_attempted |= 1 << i;
                    port_io::file_sync(file, o, p, path)?;
                    o.effects.slot_synced |= 1 << i;
                    Ok(FileMetadata {
                        length: SLOT_CAP,
                        ..opened
                    })
                },
            )?;
            port_io::with_file(
                &mut self.prefix.writer,
                &mut self.allowance,
                &mut self.observation,
                p,
                path,
                OpenMode::ReadOnly,
                Some(SLOT_CAP),
                Some(stamp),
                |_, _, _| Ok(()),
            )?;
            let growth = SLOT_CAP - initial_length;
            self.observation.ledger.actual_owned = self
                .observation
                .ledger
                .actual_owned
                .checked_add(growth)
                .ok_or_else(|| overflow(p))?;
            self.observation.ledger.outstanding = self
                .observation
                .ledger
                .outstanding
                .checked_sub(growth)
                .ok_or_else(|| overflow(p))?;
            self.observation.ledger.slot_growth = self
                .observation
                .ledger
                .slot_growth
                .checked_sub(growth)
                .ok_or_else(|| overflow(p))?;
            self.observation.boot.provision_stamps[i] = Some(stamp);
        }
        self.observation.effects.boot_directory_sync_attempted = true;
        port_io::directory_sync(&mut self.prefix.writer, &mut self.observation, p)?;
        self.observation.effects.boot_directory_synced = true;
        let mut stamps = [self.prefix.observation.source; 2];
        for (i, path) in SLOT_NAMES.into_iter().enumerate() {
            let expected = self.observation.boot.provision_stamps[i]
                .ok_or_else(|| fail(p, RecoveryCause::Invariant))?;
            let named = port_io::metadata(&self.prefix.writer, &mut self.observation, p, path)?
                .ok_or_else(|| fail(p, RecoveryCause::IdentityMismatch))?;
            port_io::same(named, expected, p)?;
            stamps[i] = named;
        }
        self.source_snapshot(p)?;
        self.slots = Some(SlotsProvisioned { stamps });
        self.observation.boot.slots_provisioned = true;
        Ok(())
    }
    pub(super) fn stabilize_source(&mut self) -> Result<()> {
        let p = RecoveryPhase::SourceStabilize;
        if self.slots.is_none() {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        self.observation.effects.source_sync_attempted = true;
        port_io::file_sync(
            &mut self.prefix.source,
            &mut self.observation,
            p,
            wal::WAL_NAME,
        )?;
        self.observation.effects.source_synced = true;
        port_io::directory_sync(&mut self.prefix.writer, &mut self.observation, p)?;
        self.observation.effects.source_directory_synced = true;
        self.source_snapshot(p)?;
        let f = &self.prefix.observation;
        self.source_seal = Some(SourceSeal {
            identity: f.source.identity,
            length: f.source.length,
            complete_offset: f.complete_offset,
            lsn: f.complete_lsn,
        });
        self.observation.source_stabilized = true;
        Ok(())
    }
}
