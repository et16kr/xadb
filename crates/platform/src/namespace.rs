//! Linux effects anchored to one opened database directory and one writer lease.
//!
//! Paths are resolved without symlinks and database operations stay fd-relative.
//! This is not a sandbox against a non-cooperating process changing the directory.
//! No effect here supplies WAL ordering, recovery validation or retirement policy.
use crate::linux::LinuxFile;
use nix::{
    dir::Dir,
    errno::Errno,
    fcntl::{AtFlags, OFlag, open, openat, renameat},
    sys::stat::{Mode, SFlag, fstatat},
    unistd::{UnlinkatFlags, unlinkat},
};
use std::{
    fs::{File, Metadata, TryLockError},
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::{Component, Path},
    sync::Arc,
};
use xadb_foundation::{
    error::Effect,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileIdentity, FileMetadata, FileName, MAX_DIRECTORY_ENTRIES,
        NamespaceError, NamespaceErrorKind, NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};

const LOCK_NAME: &str = "LOCK";

fn os_error(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}

fn failure(error: io::Error, effect: Effect, context: &'static str) -> NamespaceError {
    let kind = if matches!(error.raw_os_error(), Some(code) if code == Errno::ELOOP as i32 || code == Errno::ENOTDIR as i32)
    {
        NamespaceErrorKind::InvalidType
    } else {
        match error.kind() {
            io::ErrorKind::NotFound => NamespaceErrorKind::NotFound,
            io::ErrorKind::AlreadyExists => NamespaceErrorKind::AlreadyExists,
            _ => NamespaceErrorKind::Io,
        }
    };
    NamespaceError::io(kind, effect, context, error)
}

fn changed_effect(mut error: NamespaceError, effect: Effect) -> NamespaceError {
    error.effect = effect;
    error
}

fn invalid_type(context: &'static str) -> NamespaceError {
    NamespaceError::new(NamespaceErrorKind::InvalidType, Effect::Unchanged, context)
}

fn regular_metadata(metadata: &Metadata) -> NamespaceResult<FileMetadata> {
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(invalid_type("regular single-link file required"));
    }
    if metadata.len() > i64::MAX as u64 {
        return Err(NamespaceError::new(
            NamespaceErrorKind::LimitExceeded,
            Effect::Unchanged,
            "file length exceeds signed offset domain",
        ));
    }
    Ok(FileMetadata {
        identity: FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        },
        length: metadata.len(),
    })
}

fn metadata_at(directory: &File, name: &str) -> NamespaceResult<Option<FileMetadata>> {
    let metadata = match fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(metadata) => metadata,
        Err(Errno::ENOENT) => return Ok(None),
        Err(error) => {
            return Err(failure(
                os_error(error),
                Effect::Unchanged,
                "observe directory entry",
            ));
        }
    };
    if metadata.st_mode & SFlag::S_IFMT.bits() != SFlag::S_IFREG.bits() || metadata.st_nlink != 1 {
        return Err(invalid_type("regular single-link directory entry required"));
    }
    let length = u64::try_from(metadata.st_size).map_err(|_| {
        NamespaceError::new(
            NamespaceErrorKind::LimitExceeded,
            Effect::Unchanged,
            "negative file length",
        )
    })?;
    if length > i64::MAX as u64 {
        return Err(NamespaceError::new(
            NamespaceErrorKind::LimitExceeded,
            Effect::Unchanged,
            "file length exceeds signed offset domain",
        ));
    }
    Ok(Some(FileMetadata {
        identity: FileIdentity {
            device: metadata.st_dev,
            inode: metadata.st_ino,
        },
        length,
    }))
}

fn require_file(directory: &File, name: &str) -> NamespaceResult<FileMetadata> {
    metadata_at(directory, name)?.ok_or_else(|| {
        NamespaceError::new(
            NamespaceErrorKind::NotFound,
            Effect::Unchanged,
            "required directory entry is absent",
        )
    })
}

fn check_identity(directory: &File, name: &str, expected: FileIdentity) -> NamespaceResult<()> {
    if require_file(directory, name)?.identity != expected {
        return Err(invalid_type("directory entry identity changed"));
    }
    Ok(())
}

fn reserve_name(name: &FileName, selectors: bool) -> NamespaceResult<()> {
    if name.as_str() == LOCK_NAME
        || (selectors && matches!(name.as_str(), "WRITER" | "CURRENT" | "FILEMAP" | "DWB"))
    {
        return Err(NamespaceError::new(
            NamespaceErrorKind::InvalidName,
            Effect::Unchanged,
            "reserved namespace name",
        ));
    }
    Ok(())
}

fn file_flags() -> OFlag {
    OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK
}

fn directory_flags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW
}

/// An opened caller-owned directory, without writer ownership until acquisition.
/// The directory is never created, removed or renamed by this adapter.
pub struct LinuxDirectory {
    directory: File,
}

impl LinuxDirectory {
    /// Open a directory by walking each component without following symlinks.
    /// Absolute and relative directory paths are accepted, but parent traversal is not.
    ///
    /// # Errors
    /// Empty paths, parent/prefix components, symlinks, missing or non-directory
    /// components and OS errors fail without modifying the directory.
    pub fn open_existing(path: &Path) -> NamespaceResult<Self> {
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(NamespaceError::new(
                NamespaceErrorKind::InvalidName,
                Effect::Unchanged,
                "nonempty directory path without parent traversal required",
            ));
        }
        let beginning = if path.is_absolute() { "/" } else { "." };
        let initial = open(beginning, directory_flags(), Mode::empty()).map_err(|error| {
            failure(
                os_error(error),
                Effect::Unchanged,
                "open directory traversal root",
            )
        })?;
        let mut directory = File::from(initial);
        for component in path.components() {
            if let Component::Normal(name) = component {
                let next = openat(
                    &directory,
                    Path::new(name),
                    directory_flags(),
                    Mode::empty(),
                )
                .map_err(|error| {
                    failure(
                        os_error(error),
                        Effect::Unchanged,
                        "open directory component",
                    )
                })?;
                directory = File::from(next);
            }
        }
        Ok(Self { directory })
    }
}

struct LeaseState {
    directory: File,
    // Private, never cloned or explicitly unlocked. Closing the last Arc releases
    // writer exclusion without unlinking the inode or performing required sync.
    _lock: File,
}

/// Exclusive writer capability. Returned files keep its private lock state alive.
pub struct LinuxWriterLease {
    state: Arc<LeaseState>,
}

/// Owned positional file whose lifetime also retains database writer exclusion.
/// No native handle, cloning or file-status-flag mutation is exposed.
pub struct LinuxNamespaceFile {
    // Field drop order closes the file before releasing its lease ownership.
    file: LinuxFile,
    _lease: Arc<LeaseState>,
}

impl FileIo for LinuxNamespaceFile {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        self.file.read_at(bytes, offset)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        self.file.write_at(bytes, offset)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }
    fn set_len(&mut self, len: u64) -> io::Result<()> {
        self.file.set_len(len)
    }
}

impl DirectoryPort for LinuxDirectory {
    type Writer = LinuxWriterLease;

    fn acquire_writer(&self) -> NamespaceResult<Self::Writer> {
        // O_EXCL distinguishes effects made by this call from an already-existing
        // lock inode. A failed acquisition never deletes that permanent inode.
        let (lock, created) = match openat(
            &self.directory,
            LOCK_NAME,
            file_flags() | OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL,
            Mode::S_IRUSR | Mode::S_IWUSR,
        ) {
            Ok(file) => (File::from(file), true),
            Err(Errno::EEXIST) => {
                require_file(&self.directory, LOCK_NAME)?;
                let file = openat(
                    &self.directory,
                    LOCK_NAME,
                    file_flags() | OFlag::O_RDWR,
                    Mode::empty(),
                )
                .map_err(|error| {
                    failure(
                        os_error(error),
                        Effect::Unchanged,
                        "open existing writer lock",
                    )
                })?;
                (File::from(file), false)
            }
            Err(error) => {
                return Err(failure(
                    os_error(error),
                    Effect::CallerMustResolve,
                    "create writer lock outcome unknown",
                ));
            }
        };
        let effect = if created {
            Effect::CallerMustResolve
        } else {
            Effect::Unchanged
        };
        let metadata = lock
            .metadata()
            .map_err(|error| failure(error, effect, "writer lock metadata"))?;
        let identity = regular_metadata(&metadata)
            .map_err(|error| changed_effect(error, effect))?
            .identity;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(NamespaceError::new(
                    NamespaceErrorKind::Busy,
                    effect,
                    "writer lease is already owned",
                ));
            }
            Err(TryLockError::Error(error)) => {
                return Err(failure(error, effect, "acquire exclusive writer lease"));
            }
        }
        check_identity(&self.directory, LOCK_NAME, identity)
            .map_err(|error| changed_effect(error, effect))?;
        let directory = self
            .directory
            .try_clone()
            .map_err(|error| failure(error, effect, "retain writer directory"))?;
        Ok(LinuxWriterLease {
            state: Arc::new(LeaseState {
                directory,
                _lock: lock,
            }),
        })
    }
}

impl LinuxWriterLease {
    fn retain_file(
        &self,
        file: File,
        name: &FileName,
        effect: Effect,
    ) -> NamespaceResult<LinuxNamespaceFile> {
        let metadata = file
            .metadata()
            .map_err(|error| failure(error, effect, "opened file metadata"))?;
        let identity = regular_metadata(&metadata)
            .map_err(|error| changed_effect(error, effect))?
            .identity;
        check_identity(&self.state.directory, name.as_str(), identity)
            .map_err(|error| changed_effect(error, effect))?;
        Ok(LinuxNamespaceFile {
            file: LinuxFile(file),
            _lease: Arc::clone(&self.state),
        })
    }
}

impl WriterNamespace for LinuxWriterLease {
    type File = LinuxNamespaceFile;

    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        let mut source = File::open("/dev/urandom")
            .map_err(|error| failure(error, Effect::Unchanged, "open OS entropy"))?;
        let mut bytes = [0; 16];
        source
            .read_exact(&mut bytes)
            .map_err(|error| failure(error, Effect::Unchanged, "read OS entropy"))?;
        if bytes == [0; 16] {
            return Err(NamespaceError::new(
                NamespaceErrorKind::Io,
                Effect::Unchanged,
                "OS entropy returned a zero incarnation",
            ));
        }
        Ok(bytes)
    }

    fn create_new(&mut self, name: &FileName) -> NamespaceResult<Self::File> {
        reserve_name(name, true)?;
        let file = openat(
            &self.state.directory,
            name.as_str(),
            file_flags() | OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )
        .map_err(|error| {
            let effect = if error == Errno::EEXIST {
                Effect::Unchanged
            } else {
                Effect::CallerMustResolve
            };
            failure(os_error(error), effect, "exclusive file create")
        })?;
        self.retain_file(File::from(file), name, Effect::CallerMustResolve)
    }

    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<Self::File> {
        reserve_name(name, false)?;
        let expected = require_file(&self.state.directory, name.as_str())?.identity;
        let access = match mode {
            OpenMode::ReadOnly => OFlag::O_RDONLY,
            OpenMode::ReadWrite => OFlag::O_RDWR,
        };
        let file = openat(
            &self.state.directory,
            name.as_str(),
            file_flags() | access,
            Mode::empty(),
        )
        .map_err(|error| failure(os_error(error), Effect::Unchanged, "open existing file"))?;
        let file = File::from(file);
        let metadata = file
            .metadata()
            .map_err(|error| failure(error, Effect::Unchanged, "existing file metadata"))?;
        if regular_metadata(&metadata)?.identity != expected {
            return Err(invalid_type("file identity changed while opening"));
        }
        self.retain_file(file, name, Effect::Unchanged)
    }

    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        let file = self.open_existing(name, mode)?;
        let metadata = file
            .file
            .0
            .metadata()
            .map_err(|error| failure(error, Effect::Unchanged, "opened handle metadata"))?;
        let metadata = regular_metadata(&metadata)?;
        check_identity(&self.state.directory, name.as_str(), metadata.identity)?;
        Ok((file, metadata))
    }

    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        metadata_at(&self.state.directory, name.as_str())
    }

    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        if limit > MAX_DIRECTORY_ENTRIES {
            return Err(NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "directory inventory limit",
            ));
        }
        let mut entries = Vec::new();
        entries.try_reserve_exact(limit).map_err(|_| {
            NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "directory inventory allocation",
            )
        })?;
        // Opening '.' produces a fresh directory stream: dup/from_fd would share
        // offsets and could make a later inventory falsely appear empty.
        let mut directory =
            Dir::openat(&self.state.directory, ".", directory_flags(), Mode::empty()).map_err(
                |error| {
                    failure(
                        os_error(error),
                        Effect::Unchanged,
                        "open directory inventory",
                    )
                },
            )?;
        for entry in directory.iter() {
            let entry = entry.map_err(|error| {
                failure(
                    os_error(error),
                    Effect::Unchanged,
                    "read directory inventory",
                )
            })?;
            let bytes = entry.file_name().to_bytes();
            if matches!(bytes, b"." | b"..") {
                continue;
            }
            if entries.len() == limit {
                return Err(NamespaceError::new(
                    NamespaceErrorKind::LimitExceeded,
                    Effect::Unchanged,
                    "directory inventory exceeds requested limit",
                ));
            }
            let text = std::str::from_utf8(bytes).map_err(|_| {
                NamespaceError::new(
                    NamespaceErrorKind::InvalidName,
                    Effect::Unchanged,
                    "directory entry is not an engine filename",
                )
            })?;
            let name = FileName::new(text)?;
            let metadata = require_file(&self.state.directory, name.as_str())?;
            entries.push(DirectoryEntry { name, metadata });
        }
        entries.sort_unstable_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
        Ok(entries)
    }

    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.state.directory.sync_all().map_err(|error| {
            failure(
                error,
                Effect::CallerMustResolve,
                "directory sync outcome unknown",
            )
        })
    }

    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        reserve_name(temporary, true)?;
        let source = require_file(&self.state.directory, temporary.as_str())?;
        metadata_at(&self.state.directory, selected.as_str())?;
        check_identity(&self.state.directory, temporary.as_str(), source.identity)?;
        renameat(
            &self.state.directory,
            temporary.as_str(),
            &self.state.directory,
            selected.as_str(),
        )
        .map_err(|error| {
            failure(
                os_error(error),
                Effect::CallerMustResolve,
                "selector rename outcome unknown",
            )
        })
    }

    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        reserve_name(name, true)?;
        let expected = require_file(&self.state.directory, name.as_str())?.identity;
        check_identity(&self.state.directory, name.as_str(), expected)?;
        unlinkat(
            &self.state.directory,
            name.as_str(),
            UnlinkatFlags::NoRemoveDir,
        )
        .map_err(|error| {
            failure(
                os_error(error),
                Effect::CallerMustResolve,
                "retired file unlink outcome unknown",
            )
        })
    }
}
