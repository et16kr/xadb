use foundation::codec::content_digest;
use std::{collections::BTreeMap, fs, path::Path};

pub(super) const GOLDEN: &[u8] =
    include_bytes!("../../../../engine/tests/fixtures/dev3-page-v1-cells.bin");
pub(super) const DATA: &str = "d3-data-0000000100000002";

pub(super) fn prepare(directory: &Path) -> Vec<u8> {
    fs::write(
        directory.join("d3-profile"),
        include_bytes!("../../../../engine/tests/fixtures/dev3-io-profile.bin"),
    )
    .unwrap();
    fs::write(
        directory.join("FILEMAP"),
        include_bytes!("../../../../engine/tests/fixtures/dev3-io-selector.bin"),
    )
    .unwrap();
    let mut container = vec![0; 2_113_536];
    for (offset, bytes) in [
        (
            0,
            include_bytes!("../../../../engine/tests/fixtures/dev3-io-root.bin").as_slice(),
        ),
        (
            16_384,
            include_bytes!("../../../../engine/tests/fixtures/dev3-io-chunk0.bin").as_slice(),
        ),
        (
            32_768,
            include_bytes!("../../../../engine/tests/fixtures/dev3-io-chunk1.bin").as_slice(),
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
        include_bytes!("../../../../engine/tests/fixtures/dev3-io-selector.bin").to_vec();
    selected_one[81] = 1;
    selected_one[112..144].copy_from_slice(&content_digest(&container[..416]));
    seal(&mut selected_one);
    let mut data = vec![0; 1 << 20];
    data[3 * 16_384..4 * 16_384].copy_from_slice(GOLDEN);
    fs::write(directory.join(DATA), data).unwrap();
    selected_one
}

pub(super) fn inventory(directory: &Path) -> BTreeMap<String, (usize, [u8; 32])> {
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
