//! Fresh native files connect the internal reader to Linux; no DB READY claim.
use engine::testing::{PageReadFailure, PageReadRequest, read_selected_page};
use foundation::namespace::DirectoryPort;
use platform::namespace::LinuxDirectory;
use std::fs;

#[path = "support/selected_pages.rs"]
mod selected_pages;
use selected_pages::*;

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
