//! One local File or enumeration stream. Each normal logical call is observable.
use super::*;
fn mutates(kind: CallKind) -> bool {
    matches!(
        kind,
        CallKind::Create
            | CallKind::Write
            | CallKind::FileSync
            | CallKind::DirectorySync
            | CallKind::Publish
    )
}
pub(super) fn begin(
    o: &mut RecoveryObservation,
    phase: RecoveryPhase,
    kind: CallKind,
    path: &'static str,
    offset: u64,
    length: u64,
) -> Result<()> {
    o.phase = phase;
    o.effects.last_call = Some(RecoveryCall {
        phase,
        kind,
        name: path,
        offset,
        length,
    });
    o.effects.calls_attempted = o
        .effects
        .calls_attempted
        .checked_add(1)
        .ok_or_else(|| overflow(phase))?;
    if mutates(kind) {
        o.effects.mutation_attempted = o
            .effects
            .mutation_attempted
            .checked_add(1)
            .ok_or_else(|| overflow(phase))?;
    }
    Ok(())
}
pub(super) fn done(o: &mut RecoveryObservation, kind: CallKind) {
    o.effects.calls_completed += 1;
    if mutates(kind) {
        o.effects.mutation_completed += 1;
    }
}
pub(super) fn metadata<W: WriterNamespace>(
    w: &W,
    o: &mut RecoveryObservation,
    p: RecoveryPhase,
    path: &'static str,
) -> Result<Option<FileMetadata>> {
    begin(o, p, CallKind::Metadata, path, 0, 0)?;
    let result = w
        .metadata(&filename(path))
        .map_err(|e| fail(p, RecoveryCause::Namespace(e)))?;
    done(o, CallKind::Metadata);
    Ok(result)
}
pub(super) fn same(actual: FileMetadata, expected: FileMetadata, p: RecoveryPhase) -> Result<()> {
    if actual.identity != expected.identity {
        return Err(fail(p, RecoveryCause::IdentityMismatch));
    }
    if actual.length != expected.length {
        return Err(fail(p, RecoveryCause::LengthMismatch));
    }
    Ok(())
}
// Keep disjoint owner fields explicit: one local permit, observation and caller
// arena slice may be borrowed independently without an alias to the owner.
#[allow(clippy::too_many_arguments)]
pub(super) fn with_file<W: WriterNamespace, T>(
    w: &mut W,
    a: &mut LocalAllowance,
    o: &mut RecoveryObservation,
    p: RecoveryPhase,
    path: &'static str,
    mode: OpenMode,
    length: Option<u64>,
    stamp: Option<FileMetadata>,
    action: impl FnOnce(&mut W::File, FileMetadata, &mut RecoveryObservation) -> Result<T>,
) -> Result<T> {
    a.enter(p)?;
    let kind = if mode == OpenMode::ReadOnly {
        CallKind::OpenRead
    } else {
        CallKind::OpenWrite
    };
    let opened = (|| {
        begin(o, p, kind, path, 0, 0)?;
        let value = w
            .open_existing_with_metadata(&filename(path), mode)
            .map_err(|e| fail(p, RecoveryCause::Namespace(e)))?;
        done(o, kind);
        Ok(value)
    })();
    let (mut file, opened) = match opened {
        Ok(value) => value,
        Err(e) => {
            a.leave();
            return Err(e);
        }
    };
    let result = (|| {
        if length.is_some_and(|l| l != opened.length) {
            return Err(fail(p, RecoveryCause::LengthMismatch));
        }
        if let Some(stamp) = stamp {
            same(opened, stamp, p)?;
        }
        let named =
            metadata(w, o, p, path)?.ok_or_else(|| fail(p, RecoveryCause::IdentityMismatch))?;
        same(named, opened, p)?;
        action(&mut file, opened, o)
    })();
    drop(file);
    a.leave();
    result
}
pub(super) fn create_closed<W: WriterNamespace>(
    w: &mut W,
    a: &mut LocalAllowance,
    o: &mut RecoveryObservation,
    p: RecoveryPhase,
    path: &'static str,
) -> Result<()> {
    a.enter(p)?;
    let result = (|| {
        begin(o, p, CallKind::Create, path, 0, 0)?;
        let file = w
            .create_new(&filename(path))
            .map_err(|e| fail(p, RecoveryCause::Namespace(e)))?;
        done(o, CallKind::Create);
        drop(file);
        Ok(())
    })();
    a.leave();
    result
}
pub(super) fn read<F: FileIo>(
    f: &mut F,
    o: &mut RecoveryObservation,
    p: RecoveryPhase,
    path: &'static str,
    offset: u64,
    out: &mut [u8],
) -> Result<()> {
    begin(o, p, CallKind::Read, path, offset, out.len() as u64)?;
    read_exact_at(f, out, offset).map_err(|e| fail(p, RecoveryCause::Io(e)))?;
    done(o, CallKind::Read);
    Ok(())
}
pub(super) fn write<F: FileIo>(
    f: &mut F,
    o: &mut RecoveryObservation,
    p: RecoveryPhase,
    path: &'static str,
    offset: u64,
    bytes: &[u8],
) -> Result<()> {
    begin(o, p, CallKind::Write, path, offset, bytes.len() as u64)?;
    write_all_at(f, bytes, offset).map_err(|e| fail(p, RecoveryCause::Io(e)))?;
    done(o, CallKind::Write);
    Ok(())
}
pub(super) fn file_sync<F: FileIo>(
    f: &mut F,
    o: &mut RecoveryObservation,
    p: RecoveryPhase,
    path: &'static str,
) -> Result<()> {
    begin(o, p, CallKind::FileSync, path, 0, 0)?;
    sync(f).map_err(|e| fail(p, RecoveryCause::Io(e)))?;
    done(o, CallKind::FileSync);
    Ok(())
}
pub(super) fn directory_sync<W: WriterNamespace>(
    w: &mut W,
    o: &mut RecoveryObservation,
    p: RecoveryPhase,
) -> Result<()> {
    begin(o, p, CallKind::DirectorySync, ".", 0, 0)?;
    w.sync_directory()
        .map_err(|e| fail(p, RecoveryCause::Namespace(e)))?;
    done(o, CallKind::DirectorySync);
    Ok(())
}
/// Read-only adapter for the second canonical pass. It forwards each source
/// read unchanged while retaining its attempted/completed call in the effect
/// observation. It owns two references, no File or descriptor. No write/sync
/// method forwards effects through this read cursor.
pub(super) struct SourceReads<'a, F> {
    pub(super) file: &'a mut F,
    pub(super) effects: &'a mut RecoveryEffects,
}
impl<F: FileIo> FileIo for SourceReads<'_, F> {
    fn read_at(&mut self, out: &mut [u8], offset: u64) -> std::io::Result<usize> {
        let e = &mut self.effects;
        e.calls_attempted = e
            .calls_attempted
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("source observation counter overflow"))?;
        e.last_call = Some(RecoveryCall {
            phase: RecoveryPhase::Redo,
            kind: CallKind::Read,
            name: wal::WAL_NAME,
            offset,
            length: out.len() as u64,
        });
        let count = self.file.read_at(out, offset)?;
        e.calls_completed = e
            .calls_completed
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("source observation counter overflow"))?;
        Ok(count)
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
    fn sync_all(&mut self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
    fn set_len(&mut self, _: u64) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}
