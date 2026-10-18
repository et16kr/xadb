//! Immutable pre-open native process-loss images, captured only after owned reap.
//! Reading page-cache-visible bytes does not simulate OS/power loss.
use super::{Control, error, io_error, protocol};
use crate::{
    durable::{self, Session},
    native::{Error, ErrorKind},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use xadbtest_protocol::{
    durable::{Action, Observation},
    fault::Phase,
};
const MAX_FILES: usize = 4096;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

/// Paths and identity of the immutable evidence, separate from the work directory
/// which a later worker may reopen and change.
#[derive(Clone, Debug)]
pub struct CapturedImage {
    pub directory: PathBuf,
    pub receipt: PathBuf,
    pub inventory_sha256: String,
    pub files: usize,
    pub bytes: u64,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Boundary {
    BlockedReachedOwnedKill,
    ReleasedResponseClosedOwnedKill,
}
#[derive(Clone, Serialize)]
struct Entry {
    name: String,
    bytes: u64,
    sha256: String,
}
struct Source {
    name: String,
    metadata: Metadata,
    digest: String,
}
fn signature(metadata: &Metadata) -> (u64, u64, u64, u64, i64, i64, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.nlink(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}
fn inventory(directory: &Path) -> Result<Vec<Source>, Error> {
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| protocol("image filename is not ASCII"))?;
        if name.is_empty()
            || name.len() > 128
            || matches!(name.as_str(), "." | "..")
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(protocol("image filename invalid"));
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if !metadata.is_file() || metadata.nlink() != 1 {
            return Err(protocol("image requires single-link regular files"));
        }
        if files.len() == MAX_FILES || metadata.len() > MAX_FILE_BYTES {
            return Err(error(ErrorKind::Limit, "image file bound"));
        }
        total = total
            .checked_add(metadata.len())
            .filter(|bytes| *bytes <= MAX_IMAGE_BYTES)
            .ok_or_else(|| error(ErrorKind::Limit, "image total byte bound"))?;
        // Open without following a substituted symlink, and bind the open inode.
        let source = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(entry.path())
            .map_err(io_error)?;
        if signature(&source.metadata().map_err(io_error)?) != signature(&metadata) {
            return Err(protocol("image inventory inode changed"));
        }
        let digest = hash_file(source, metadata.len())?;
        files.push(Source {
            name,
            metadata,
            digest,
        });
    }
    files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(files)
}
fn hash_file(mut file: File, length: u64) -> Result<String, Error> {
    use std::io::Read;
    let mut hash = Sha256::new();
    let mut bytes = [0; 16 * 1024];
    let mut total = 0u64;
    loop {
        let count = file.read(&mut bytes).map_err(io_error)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .filter(|total| *total <= length)
            .ok_or_else(|| protocol("image source grew while hashing"))?;
        hash.update(&bytes[..count]);
    }
    if total != length {
        return Err(protocol("image source shrank while hashing"));
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn inventory_hash(entries: &[Entry]) -> String {
    let mut hash = Sha256::new();
    for entry in entries {
        hash.update((entry.name.len() as u64).to_le_bytes());
        hash.update(entry.name.as_bytes());
        hash.update(entry.bytes.to_le_bytes());
        for pair in entry.sha256.as_bytes().chunks_exact(2) {
            let nibble = |byte: u8| {
                if byte <= b'9' {
                    byte - b'0'
                } else {
                    byte - b'a' + 10
                }
            };
            hash.update([nibble(pair[0]) * 16 + nibble(pair[1])]);
        }
    }
    format!("{:x}", hash.finalize())
}
impl Control {
    /// Preserve the bytes after a cut-qualified owned kill, before any reopening.
    ///
    /// # Errors
    /// Owner/kill/witness mismatch, changed source, bounds and existing artifact
    /// paths leave incomplete evidence. Original database files are never changed.
    pub fn capture_after_kill(&mut self, session: &Session) -> Result<CapturedImage, Error> {
        self.owner(session)?;
        if !self.qualified_kill {
            return Err(protocol("image requires qualified blocked-cut kill"));
        }
        self.capture(session, Boundary::BlockedReachedOwnedKill)
    }
    /// After the injected operation's response was consumed, close its database,
    /// perform a plain owned kill/reap, and preserve the pre-open bytes. A failed
    /// Open is checked closed via ConfigureEngine instead of issuing invalid Close.
    /// This receipt explicitly does not claim the process was killed at the cut.
    ///
    /// # Errors
    /// The completed response/closed-state handshake must succeed; active work,
    /// owner/phase mismatch, cleanup or image errors remain incomplete.
    pub fn close_kill_capture(&mut self, session: &mut Session) -> Result<CapturedImage, Error> {
        self.owner(session)?;
        if !self.released || self.qualified_kill {
            return Err(protocol("plain-close image requires a released operation"));
        }
        let witness = self
            .reached
            .as_ref()
            .ok_or_else(|| protocol("image lacks reached witness"))?;
        let action = if witness.phase == Phase::Open {
            Action::ConfigureEngine {
                observations: true,
                clock: true,
            }
        } else {
            Action::CloseDatabase {}
        };
        let response = durable::call(session, 0, action)?;
        if response.observation != (Observation::Done {}) {
            return Err(protocol("image database closed-state check failed"));
        }
        session.kill_worker()?;
        self.capture(session, Boundary::ReleasedResponseClosedOwnedKill)
    }
    fn capture(&mut self, session: &Session, boundary: Boundary) -> Result<CapturedImage, Error> {
        self.owner(session)?;
        if !session.was_killed() {
            return Err(protocol("image requires successful owned kill and reap"));
        }
        let witness = self
            .reached
            .as_ref()
            .ok_or_else(|| protocol("image lacks reached witness"))?;
        if crate::file_sha256(&session.directory().join("fault-case.json"))? != self.case_sha256 {
            return Err(protocol("predeclared fault case changed after arming"));
        }
        let source = session
            .work_directory()
            .join(format!("database-{}", witness.directory));
        let directory = session.directory().join("crash-image");
        fs::create_dir(&directory).map_err(io_error)?;
        let before = inventory(&source)?;
        let mut entries = Vec::with_capacity(before.len());
        let mut total = 0u64;
        for original in &before {
            let path = source.join(&original.name);
            let mut input = OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NOFOLLOW)
                .open(&path)
                .map_err(io_error)?;
            if signature(&input.metadata().map_err(io_error)?) != signature(&original.metadata) {
                return Err(protocol("image source changed before copy"));
            }
            let target = directory.join(&original.name);
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(io_error)?;
            let written = io::copy(
                &mut std::io::Read::take(&mut input, original.metadata.len() + 1),
                &mut output,
            )
            .map_err(io_error)?;
            if written != original.metadata.len() {
                return Err(protocol("image source changed during copy"));
            }
            output
                .flush()
                .and_then(|()| output.sync_all())
                .map_err(io_error)?;
            let digest = crate::file_sha256(&target)?;
            if digest != original.digest {
                return Err(protocol("image destination digest mismatch"));
            }
            entries.push(Entry {
                name: original.name.clone(),
                bytes: written,
                sha256: digest,
            });
            total += written;
        }
        let after = inventory(&source)?;
        if before.len() != after.len()
            || before.iter().zip(&after).any(|(before, after)| {
                before.name != after.name
                    || signature(&before.metadata) != signature(&after.metadata)
                    || before.digest != after.digest
            })
        {
            return Err(protocol("image source inventory changed while copying"));
        }
        File::open(&directory)
            .and_then(|file| file.sync_all())
            .map_err(io_error)?;
        let digest = inventory_hash(&entries);
        let receipt = session.directory().join("crash-image.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&receipt)
            .map_err(io_error)?;
        serde_json::to_writer_pretty(&mut file,&serde_json::json!({"schema_version":1,"scope":"NATIVE_PROCESS_LOSS_IMAGE_NOT_POWER_LOSS","boundary":boundary,"run":self.run,"worker_id":self.worker_id,"case_sha256":self.case_sha256,"source_work_directory":session.work_directory(),"directory_slot":witness.directory,"image":"crash-image","witness":witness,"inventory_hash_format":"name_len_le64+name_ascii+size_le64+sha256_bytes_sorted_names_v1","inventory_sha256":digest,"files":entries,"total_bytes":total})).map_err(|error|protocol(&error.to_string()))?;
        file.write_all(b"\n")
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_all())
            .map_err(io_error)?;
        File::open(session.directory())
            .and_then(|file| file.sync_all())
            .map_err(io_error)?;
        let receipt_sha256 = crate::file_sha256(&receipt)?;
        self.record("image",serde_json::json!({"boundary":boundary,"receipt":"crash-image.json","receipt_sha256":receipt_sha256,"inventory_sha256":digest,"files":entries.len(),"bytes":total}))?;
        Ok(CapturedImage {
            directory,
            receipt,
            inventory_sha256: digest,
            files: entries.len(),
            bytes: total,
        })
    }
}
