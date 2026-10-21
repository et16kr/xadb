//! Metadata is observed from the actual opened inode, with the existing lease.
use std::{
    fs,
    os::unix::fs::{MetadataExt, symlink},
};
use xadb_foundation::{
    error::Effect,
    io::{FileIo, read_exact_at, write_all_at},
    namespace::{
        DirectoryPort, FileIdentity, FileName, NamespaceErrorKind, OpenMode, Selector,
        WriterNamespace,
    },
};
use xadb_platform::namespace::LinuxDirectory;

fn name(value: &str) -> FileName {
    FileName::new(value).unwrap()
}

#[test]
fn opened_metadata_binds_exact_handle_and_length() {
    let directory = tempfile::tempdir().unwrap();
    let port = LinuxDirectory::open_existing(directory.path()).unwrap();
    let mut writer = port.acquire_writer().unwrap();
    let path = name("page");
    let mut original = writer.create_new(&path).unwrap();
    for length in [0, 17, 16_384] {
        original.set_len(length).unwrap();
        if length > 0 {
            write_all_at(&mut original, b"v", length - 1).unwrap();
        }
        for mode in [OpenMode::ReadOnly, OpenMode::ReadWrite] {
            let (mut file, metadata) = writer.open_existing_with_metadata(&path, mode).unwrap();
            let native = fs::metadata(directory.path().join("page")).unwrap();
            assert_eq!(metadata.length, length);
            assert_eq!(
                metadata.identity,
                FileIdentity {
                    device: native.dev(),
                    inode: native.ino()
                }
            );
            if length > 0 {
                let mut last = [0];
                read_exact_at(&mut file, &mut last, length - 1).unwrap();
                assert_eq!(last, *b"v");
            }
            let mut eof = [0];
            assert_eq!(file.read_at(&mut eof, length).unwrap(), 0);
        }
    }
}

#[test]
fn replacement_keeps_old_handle_metadata_and_lease() {
    for selector in [Selector::FileMap, Selector::Doublewrite] {
        let directory = tempfile::tempdir().unwrap();
        let port = LinuxDirectory::open_existing(directory.path()).unwrap();
        let mut writer = port.acquire_writer().unwrap();
        let selected = name(selector.as_str());
        let stage = name("stage");
        let mut file = writer.create_new(&stage).unwrap();
        write_all_at(&mut file, b"old", 0).unwrap();
        file.sync_all().unwrap();
        drop(file);
        writer.publish_selector(&stage, selector).unwrap();
        writer.sync_directory().unwrap();
        let (mut old, old_metadata) = writer
            .open_existing_with_metadata(&selected, OpenMode::ReadOnly)
            .unwrap();
        let mut file = writer.create_new(&stage).unwrap();
        write_all_at(&mut file, b"new-value", 0).unwrap();
        file.sync_all().unwrap();
        drop(file);
        writer.publish_selector(&stage, selector).unwrap();
        writer.sync_directory().unwrap();
        let (mut new, new_metadata) = writer
            .open_existing_with_metadata(&selected, OpenMode::ReadOnly)
            .unwrap();
        assert_ne!(old_metadata.identity, new_metadata.identity);
        assert_eq!((old_metadata.length, new_metadata.length), (3, 9));
        let mut old_bytes = [0; 3];
        let mut new_bytes = [0; 9];
        read_exact_at(&mut old, &mut old_bytes, 0).unwrap();
        read_exact_at(&mut new, &mut new_bytes, 0).unwrap();
        assert_eq!(old_bytes, *b"old");
        assert_eq!(new_bytes, *b"new-value");
        drop(writer);
        assert!(
            matches!(port.acquire_writer(), Err(error) if error.kind == NamespaceErrorKind::Busy)
        );
        drop(old);
        assert!(
            matches!(port.acquire_writer(), Err(error) if error.kind == NamespaceErrorKind::Busy)
        );
        drop(new);
        drop(port.acquire_writer().unwrap());
    }
}

#[test]
fn opened_metadata_rejects_invalid_names_and_types_without_effects() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("target"), b"retained").unwrap();
    symlink("target", directory.path().join("symlink")).unwrap();
    fs::hard_link(
        directory.path().join("target"),
        directory.path().join("hardlink"),
    )
    .unwrap();
    fs::create_dir(directory.path().join("directory")).unwrap();
    let port = LinuxDirectory::open_existing(directory.path()).unwrap();
    let mut writer = port.acquire_writer().unwrap();
    let lock_before = fs::read(directory.path().join("LOCK")).unwrap();
    for mode in [OpenMode::ReadOnly, OpenMode::ReadWrite] {
        for (path, expected) in [
            ("missing", NamespaceErrorKind::NotFound),
            ("symlink", NamespaceErrorKind::InvalidType),
            ("hardlink", NamespaceErrorKind::InvalidType),
            ("directory", NamespaceErrorKind::InvalidType),
            ("LOCK", NamespaceErrorKind::InvalidName),
        ] {
            let Err(error) = writer.open_existing_with_metadata(&name(path), mode) else {
                panic!("accepted {path}")
            };
            assert_eq!(error.kind, expected, "{path}");
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(
                fs::read(directory.path().join("target")).unwrap(),
                b"retained"
            );
            assert_eq!(
                fs::read(directory.path().join("LOCK")).unwrap(),
                lock_before
            );
            assert!(!directory.path().join("missing").exists());
        }
    }
}
