//! Fresh native files connect the internal reader to Linux; no DB READY claim.
use std::{collections::BTreeMap, fs, path::Path};
use xadb_engine::testing::{PageReadFailure, PageReadRequest, read_selected_page};
use xadb_foundation::{codec::content_digest, namespace::DirectoryPort};
use xadb_platform::namespace::LinuxDirectory;

const GOLDEN: &[u8] = include_bytes!("../../../engine/tests/fixtures/dev3-page-v1-cells.bin");
const DATA: &str = "d3-data-0000000100000002";

fn prepare(directory: &Path) -> Vec<u8> {
    fs::write(
        directory.join("d3-profile"),
        include_bytes!("../../../engine/tests/fixtures/dev3-io-profile.bin"),
    )
    .unwrap();
    fs::write(
        directory.join("FILEMAP"),
        include_bytes!("../../../engine/tests/fixtures/dev3-io-selector.bin"),
    )
    .unwrap();
    let mut container = vec![0; 2_113_536];
    for (offset, bytes) in [
        (
            0,
            include_bytes!("../../../engine/tests/fixtures/dev3-io-root.bin").as_slice(),
        ),
        (
            16_384,
            include_bytes!("../../../engine/tests/fixtures/dev3-io-chunk0.bin").as_slice(),
        ),
        (
            32_768,
            include_bytes!("../../../engine/tests/fixtures/dev3-io-chunk1.bin").as_slice(),
        ),
    ] {
        container[offset..offset + bytes.len()].copy_from_slice(bytes);
    }
    fs::write(directory.join("d3-filemap-0"), &container).unwrap();
    // A valid but unselected bank must never hide selected corruption.
    fn seal(bytes: &mut [u8]) {
        let end = bytes.len() - 32;
        let digest = content_digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&digest);
    }
    for (index, length) in [6304, 256].into_iter().enumerate() {
        let offset = (index + 1) * 16_384;
        let chunk = &mut container[offset..offset + length];
        chunk[7] = 1;
        seal(chunk);
        let digest = content_digest(chunk);
        let reference = 192 + index * 96;
        container[reference + 1] = 1;
        container[reference + 32..reference + 64].copy_from_slice(&digest);
    }
    container[7] = 1;
    seal(&mut container[..416]);
    fs::write(directory.join("d3-filemap-1"), &container).unwrap();
    let mut selected_one =
        include_bytes!("../../../engine/tests/fixtures/dev3-io-selector.bin").to_vec();
    selected_one[81] = 1;
    selected_one[112..144].copy_from_slice(&content_digest(&container[..416]));
    seal(&mut selected_one);
    let mut data = vec![0; 1 << 20];
    data[3 * 16_384..4 * 16_384].copy_from_slice(GOLDEN);
    fs::write(directory.join(DATA), data).unwrap();
    selected_one
}

fn inventory(directory: &Path) -> BTreeMap<String, (usize, [u8; 32])> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let bytes = fs::read(entry.path()).unwrap();
            (
                entry.file_name().into_string().unwrap(),
                (bytes.len(), content_digest(&bytes)),
            )
        })
        .collect()
}

#[test]
fn native_selected_map_reads_and_rejects_selected_damage() {
    for case in 0..6 {
        let directory = tempfile::tempdir().unwrap();
        let selected_one = prepare(directory.path());
        match case {
            1 => {
                let path = directory.path().join("d3-filemap-0");
                let mut bytes = fs::read(&path).unwrap();
                bytes[32_768 + 128] ^= 1;
                fs::write(path, bytes).unwrap();
            }
            2 | 3 => fs::OpenOptions::new()
                .write(true)
                .open(directory.path().join(DATA))
                .unwrap()
                .set_len(if case == 2 {
                    (1 << 20) - 1
                } else {
                    (1 << 20) + 1
                })
                .unwrap(),
            5 => fs::write(directory.path().join("FILEMAP"), selected_one).unwrap(),
            _ => {}
        }
        let port = LinuxDirectory::open_existing(directory.path()).unwrap();
        let mut writer = port.acquire_writer().unwrap();
        let before = inventory(directory.path());
        let mut output = [0; 16_384];
        let request = PageReadRequest {
            database: if case == 4 { [0x22; 16] } else { [0x11; 16] },
            file: 4_294_967_298,
            page: 3,
            incarnation: 4,
            owner: 5,
            owner_generation: 6,
            kind: 1,
            level: 0,
        };
        let result = read_selected_page(&mut writer, ([0x11; 16], 17), request, &mut output);
        match case {
            0 | 5 => {
                assert_eq!(result.unwrap(), (29, 3));
                assert_eq!(&output, GOLDEN);
            }
            1 => assert_eq!(result, Err(PageReadFailure::Control)),
            2 | 3 => assert_eq!(result, Err(PageReadFailure::LengthMismatch)),
            4 => assert_eq!(result, Err(PageReadFailure::IdentityMismatch)),
            _ => unreachable!(),
        }
        assert_eq!(inventory(directory.path()), before);
        drop(writer);
        drop(port.acquire_writer().unwrap()); // Reader released every file/lease.
    }
}
