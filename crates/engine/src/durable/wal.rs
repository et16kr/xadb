//! Single-stream L0 append, sync and writer selection. Publication belongs to K1.
use super::{
    codec::{
        self, Anchor, Batch, Header, Identity, Link, MAX_FILES, Origin, Record, SEGMENT_BYTES,
        WAL_BYTES,
    },
    error::{DurableError as Error, DurableErrorCode, Result, TransactionOutcome},
    io::{self, OwnedName},
    observation::{DurabilityOperation as Operation, Observations},
};
use std::sync::Arc;
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    io::{FileIo, write_all_at},
    namespace::{Selector, WriterNamespace},
    runtime::{Cancellation, Quota},
};

const SEAL_BYTES: u64 = (codec::COMMON_BYTES + 48 + codec::FRAME_OVERHEAD) as u64;
// Normal writes leave space for one subsequent recovery epoch. A linked header
// contains segment/start_q, origin/option tags, Link and resume digest. Recovery
// itself uses the hard limits; repeated opens without maintenance remain finite.
const LINKED_HEADER_BYTES: u64 =
    (codec::COMMON_BYTES + 16 + 2 + 68 + 32 + codec::FRAME_OVERHEAD) as u64;
const RECOVERY_FILE_SLOTS: usize = 4;

pub(crate) struct Wal<W: WriterNamespace> {
    pub writer: W,
    pub identity: Identity,
    pub header: Header,
    pub header_digest: [u8; 32],
    file: W::File,
    pub offset: u64,
    pub position: u64,
    pub hasher: ContentHasher,
    pub selector_serial: u64,
    pub retained_wal_bytes: u64,
    pub inventory_count: usize,
    /// Slots promised to a checkpoint builder/selector, unavailable to rotation.
    pub reserved_files: usize,
    pub scratch: Quota,
    pub written_bytes: u64,
    pub sync_calls: u64,
    pub observations: Arc<Observations>,
}

impl<W: WriterNamespace> Wal<W> {
    /// Caller already validated/stabilized any predecessor and durably selected
    /// fallback when applicable. No IDs or normal readers may escape before this
    /// method and the corresponding CURRENT setup both succeed.
    // These explicit inputs bind one durable epoch installation to its already
    // validated predecessor, resource owners and selector serial.
    #[allow(clippy::too_many_arguments)]
    pub fn install_epoch(
        mut writer: W,
        identity: Identity,
        start_q: u64,
        previous: Option<Link>,
        restored: bool,
        scratch: Quota,
        initial_selector_serial: u64,
        observations: Arc<Observations>,
    ) -> Result<Self> {
        let timer = observations.start(Operation::WalEpochInstall, 0, 1);
        if identity.epoch == 0 || identity.timeline == 0 || (restored && previous.is_some()) {
            return Err(Error::new(
                DurableErrorCode::InvalidInput,
                "writer epoch origin",
            ));
        }
        let _workspace = scratch
            .reserve(4 * 1024 * 1024)
            .map_err(|_| Error::allocation())?;
        let inventory = writer.list_owned(MAX_FILES).map_err(io::namespace)?;
        let mut retained_wal_bytes = 0_u64;
        for entry in &inventory {
            if matches!(io::parse_name(&entry.name), Some(OwnedName::Wal { .. })) {
                retained_wal_bytes = retained_wal_bytes
                    .checked_add(entry.metadata.length)
                    .ok_or(Error::limit("WAL byte accounting overflow"))?;
            }
        }
        // Header + optional resume + a temporary selector; the selected WRITER
        // is new only for initial create/restore. Reserve conservatively.
        if inventory
            .len()
            .checked_add(RECOVERY_FILE_SLOTS)
            .is_none_or(|n| n > MAX_FILES)
        {
            return Err(Error::limit("writer metadata file budget"));
        }
        let segment = 1;
        let resume = match &previous {
            Some(previous) => Some(codec::encode(
                identity,
                &Record::Resume {
                    segment,
                    previous: previous.clone(),
                },
            )?),
            None => None,
        };
        let header = Header {
            segment,
            start_q,
            origin: if restored {
                Origin::Restore
            } else if previous.is_some() {
                Origin::Resume
            } else {
                Origin::Genesis
            },
            previous,
            resume_digest: resume
                .as_ref()
                .map_or([0; 32], |bytes| content_digest(bytes)),
        };
        let header_bytes = codec::encode(identity, &Record::Header(header.clone()))?;
        let header_digest = content_digest(&header_bytes);
        let selection = codec::encode(
            identity,
            &Record::Writer {
                segment,
                header_digest,
            },
        )?;
        retained_wal_bytes = retained_wal_bytes
            .checked_add(header_bytes.len() as u64)
            .ok_or(Error::limit("WAL byte accounting overflow"))?;
        if retained_wal_bytes > WAL_BYTES {
            return Err(Error::limit("retained WAL budget"));
        }
        if let Some(resume) = &resume {
            drop(io::write_new(
                &mut writer,
                &io::resume_name(identity.epoch)?,
                resume,
            )?);
        }
        let file = io::write_new(
            &mut writer,
            &io::wal_name(identity.epoch, segment)?,
            &header_bytes,
        )?;
        let sync = observations.start(Operation::DirectorySync, 0, 1);
        let result = writer
            .sync_directory()
            .map_err(io::namespace)
            .map_err(io::metadata_failure);
        sync.finish(result.is_ok(), 0);
        result?;
        let mut selector_serial = initial_selector_serial;
        io::select(
            &mut writer,
            identity.epoch,
            &mut selector_serial,
            Selector::Writer,
            &selection,
        )?;
        let inventory_count = writer
            .list_owned(MAX_FILES)
            .map_err(io::namespace)
            .map_err(io::metadata_failure)?
            .len();
        let mut hasher = ContentHasher::new();
        hasher.update(&header_bytes);
        let wal = Self {
            writer,
            identity,
            header,
            header_digest,
            file,
            offset: header_bytes.len() as u64,
            position: start_q,
            hasher,
            selector_serial,
            retained_wal_bytes,
            inventory_count,
            reserved_files: 0,
            scratch,
            written_bytes: header_bytes.len() as u64,
            sync_calls: 1,
            observations: Arc::clone(&observations),
        };
        timer.finish(true, header_bytes.len() as u64);
        Ok(wal)
    }

    pub fn anchor(&self) -> Anchor {
        Anchor {
            epoch: self.identity.epoch,
            segment: self.header.segment,
            header_digest: self.header_digest,
            offset: self.offset,
            prefix_digest: self.hasher.digest(),
            q: self.position,
        }
    }

    pub fn check_files(&self, additional: usize) -> Result<()> {
        if self
            .inventory_count
            .checked_add(self.reserved_files)
            .and_then(|n| n.checked_add(additional))
            .and_then(|n| n.checked_add(RECOVERY_FILE_SLOTS))
            .is_none_or(|n| n > MAX_FILES)
        {
            return Err(Error::limit("internal file inventory budget"));
        }
        Ok(())
    }

    fn check_wal_bytes(&self, additional: u64) -> Result<()> {
        if self
            .retained_wal_bytes
            .checked_add(additional)
            .and_then(|n| n.checked_add(LINKED_HEADER_BYTES))
            .is_none_or(|n| n > WAL_BYTES)
        {
            return Err(Error::limit(
                "retained WAL budget including recovery header",
            ));
        }
        Ok(())
    }

    /// Refresh only after coordinated metadata changes/retirement, never as a
    /// substitute for retaining a required predecessor or a high-water marker.
    pub fn refresh_inventory(&mut self) -> Result<()> {
        let requested =
            MAX_FILES * std::mem::size_of::<xadb_foundation::namespace::DirectoryEntry>();
        let _inventory = self
            .scratch
            .reserve(requested as u64)
            .map_err(Error::foundation)?;
        let entries = self.writer.list_owned(MAX_FILES).map_err(io::namespace)?;
        let extra = entries
            .capacity()
            .saturating_sub(MAX_FILES)
            .checked_mul(std::mem::size_of::<
                xadb_foundation::namespace::DirectoryEntry,
            >())
            .ok_or(Error::limit("WAL inventory capacity"))?;
        let _extra = self
            .scratch
            .reserve(extra as u64)
            .map_err(Error::foundation)?;
        let inventory_count = entries.len();
        let mut retained_wal_bytes = 0_u64;
        for entry in &entries {
            if matches!(io::parse_name(&entry.name), Some(OwnedName::Wal { .. })) {
                if entry.metadata.length > SEGMENT_BYTES {
                    return Err(Error::limit("WAL segment byte budget"));
                }
                retained_wal_bytes = retained_wal_bytes
                    .checked_add(entry.metadata.length)
                    .ok_or(Error::limit("WAL byte accounting overflow"))?;
            }
        }
        if retained_wal_bytes > WAL_BYTES {
            return Err(Error::limit("retained WAL byte budget"));
        }
        self.inventory_count = inventory_count;
        self.retained_wal_bytes = retained_wal_bytes;
        Ok(())
    }

    fn rotate(&mut self, pending_batch_bytes: u64) -> Result<()> {
        let observations = Arc::clone(&self.observations);
        let timer = observations.start(Operation::WalRotate, 0, 1);
        self.check_files(2)?;
        let next_segment = self
            .header
            .segment
            .checked_add(1)
            .ok_or(Error::limit("segment identity exhausted"))?;
        let seal = codec::encode(
            self.identity,
            &Record::Seal {
                end_offset: self.offset,
                q: self.position,
                prefix_digest: self.hasher.digest(),
            },
        )?;
        let sealed_end = self
            .offset
            .checked_add(seal.len() as u64)
            .ok_or(Error::limit("sealed segment length"))?;
        if sealed_end > SEGMENT_BYTES {
            return Err(Error::invalid("no reserved seal space"));
        }
        let mut sealed_hasher = self.hasher.clone();
        sealed_hasher.update(&seal);
        let header = Header {
            segment: next_segment,
            start_q: self.position,
            origin: Origin::Rotation,
            previous: Some(Link {
                epoch: self.identity.epoch,
                segment: self.header.segment,
                end_offset: sealed_end,
                file_size: sealed_end,
                q: self.position,
                prefix_digest: sealed_hasher.digest(),
            }),
            resume_digest: [0; 32],
        };
        let bytes = codec::encode(self.identity, &Record::Header(header.clone()))?;
        let header_digest = content_digest(&bytes);
        let selection = codec::encode(
            self.identity,
            &Record::Writer {
                segment: next_segment,
                header_digest,
            },
        )?;
        let extra = seal.len() as u64 + bytes.len() as u64;
        // Admit rotation and the pending batch together, before the first seal
        // write. A rejected batch must not consume its recovery headroom through
        // otherwise successful metadata writes.
        self.check_wal_bytes(
            extra
                .checked_add(pending_batch_bytes)
                .ok_or(Error::limit("WAL rotation byte accounting overflow"))?,
        )?;
        // Rotation uncertainty fences the engine, but the next transaction's
        // batch has not been attempted and therefore remains definitely aborted.
        write_all_at(&mut self.file, &seal, self.offset)
            .map_err(io::file_error)
            .map_err(io::metadata_failure)?;
        let sync = observations.start(Operation::WalSync, 0, 1);
        let result = self.file.sync_all().map_err(|e| {
            io::metadata_failure(Error {
                io_kind: Some(e.kind()),
                ..Error::new(DurableErrorCode::Io, "segment seal sync")
            })
        });
        sync.finish(result.is_ok(), 0);
        result?;
        let file = io::write_new(
            &mut self.writer,
            &io::wal_name(self.identity.epoch, next_segment)?,
            &bytes,
        )?;
        let sync = observations.start(Operation::DirectorySync, 0, 1);
        let result = self
            .writer
            .sync_directory()
            .map_err(io::namespace)
            .map_err(io::metadata_failure);
        sync.finish(result.is_ok(), 0);
        result?;
        io::select(
            &mut self.writer,
            self.identity.epoch,
            &mut self.selector_serial,
            Selector::Writer,
            &selection,
        )?;
        self.file = file;
        self.header = header;
        self.header_digest = header_digest;
        self.offset = bytes.len() as u64;
        self.hasher = ContentHasher::new();
        self.hasher.update(&bytes);
        self.retained_wal_bytes += extra;
        self.inventory_count += 1;
        self.written_bytes = self.written_bytes.saturating_add(extra);
        self.sync_calls = self.sync_calls.saturating_add(2);
        timer.finish(true, extra);
        Ok(())
    }

    /// Returns only after batch bytes are synced. It never publishes row state.
    /// The caller maps this success to Published, or fences after a later failure.
    pub fn append(&mut self, batch: &Batch, cancellation: &Cancellation) -> Result<()> {
        if batch.previous_q != self.position
            || batch.q
                != self
                    .position
                    .checked_add(1)
                    .ok_or(Error::limit("commit sequence exhausted"))?
        {
            return Err(Error::invalid("WAL/current publication sequence mismatch"));
        }
        let _workspace = self
            .scratch
            .reserve(3 * 1024 * 1024)
            .map_err(|_| Error::allocation())?;
        let observations = Arc::clone(&self.observations);
        let encode = observations.start(Operation::WalEncode, 0, batch.changes.len() as u64);
        let encoded = codec::encode_batch(self.identity, batch);
        encode.finish(
            encoded.is_ok(),
            encoded.as_ref().map_or(0, |frame| frame.len() as u64),
        );
        let frame = encoded?;
        // A prior open may have consumed reserved metadata space. Even a batch
        // that does not rotate must leave the next open possible after its ACK.
        self.check_files(0)?;
        self.check_wal_bytes(frame.len() as u64)?;
        if self.offset + frame.len() as u64 + SEAL_BYTES > SEGMENT_BYTES {
            self.rotate(frame.len() as u64)?;
        }
        cancellation.check().map_err(|_| Error {
            outcome: TransactionOutcome::Aborted,
            ..Error::new(
                DurableErrorCode::Transaction,
                "cancelled before commit attempt",
            )
        })?;
        // CommitAttempted starts immediately before the first write. Every error
        // from this point can leave a complete batch and must fence the engine.
        let append = observations.start(Operation::WalAppend, frame.len() as u64, 1);
        let written = write_all_at(&mut self.file, &frame, self.offset).map_err(|e| Error {
            outcome: TransactionOutcome::Unknown,
            fenced: true,
            ..io::file_error(e)
        });
        append.finish(written.is_ok(), frame.len() as u64);
        written?;
        let sync = observations.start(Operation::WalSync, 0, 1);
        let synced = self.file.sync_all().map_err(|e| Error {
            io_kind: Some(e.kind()),
            outcome: TransactionOutcome::Unknown,
            fenced: true,
            ..Error::new(DurableErrorCode::Io, "commit batch sync")
        });
        sync.finish(synced.is_ok(), 0);
        synced?;
        self.hasher.update(&frame);
        self.offset += frame.len() as u64;
        self.retained_wal_bytes += frame.len() as u64;
        self.position = batch.q;
        self.written_bytes = self.written_bytes.saturating_add(frame.len() as u64);
        self.sync_calls = self.sync_calls.saturating_add(1);
        Ok(())
    }
}
