//! Numerical admission boundaries only. Synthetic counters/LSNs and the
//! codec-generated Root never construct original-source prepared authority.
use super::*;
use crate::storage::{
    allocation_page::{SystemWorkKind, SystemWorkRef},
    control::row_profile,
    retention_page::row as row_queue,
    wal_frame::WalStreamIdentity,
};

#[test]
fn structural_admission_lsn_each_existing_counter_and_root_revision_boundaries() {
    let profile = row_profile::decode_profile(
        &include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
        ))[..4096],
        None,
    )
    .unwrap();
    let namespace = profile.spec().namespace;
    let mut cell = [0; crate::storage::retention_page::ROOT_BYTES];
    row_queue::initial_root(namespace, &mut cell).unwrap();
    cell[6] = 1;
    SystemWorkRef::new(namespace, SystemWorkKind::Aging, 1, one())
        .unwrap()
        .encode(&mut cell[16..64])
        .unwrap();
    cell[80..88].copy_from_slice(&1u64.to_le_bytes());
    let mut root = Box::new([0; PAGE_BYTES]);
    row_queue::encode_root_page(&cell, namespace, root.as_mut()).unwrap();
    let prefix = WalPrefix {
        stream: WalStreamIdentity {
            database: namespace.database,
            timeline: 1,
            writer_epoch: 1,
        },
        file: foundation::namespace::FileIdentity {
            device: 1,
            inode: 1,
        },
        complete_lsn: 40,
        complete_offset: 4096,
        synced_lsn: 40,
        synced_offset: 4096,
    };
    let stamp = FrameStamp {
        identity: queue::decode_root_page(&root[..], namespace)
            .unwrap()
            .0
            .identity,
        generation: 1,
        page_lsn: PageLsn(1),
        modification_counter: 9,
        rec_lsn: None,
    };
    // A nontrivial physical permutation verifies that role4 (Child parent)
    // is charged too, and that missing INIT stamps invent no old counter.
    let roles = [0, 4, 1, 3, 2];
    let stamps = [Some(stamp), Some(stamp), Some(stamp), None, Some(stamp)];
    let future = (8192, 7);
    let check = |lsn, stamps: &[Option<FrameStamp>; 5], root: &[u8; PAGE_BYTES]| {
        check_progress_room(
            prefix,
            (lsn, 84_744),
            future,
            (&roles, stamps),
            &[0, 1, 2, 3, 4],
            root,
            profile,
        )
    };
    check(u64::MAX - 7, &stamps, &root).unwrap();
    assert!(matches!(
        check(u64::MAX - 6, &stamps, &root).unwrap_err().cause,
        RetentionCause::Capacity
    ));
    for at in [0, 1, 2, 4] {
        let mut boundary = stamps;
        boundary[at].as_mut().unwrap().modification_counter = u64::MAX - 8;
        check(41, &boundary, &root).unwrap();
        boundary[at].as_mut().unwrap().modification_counter += 1;
        assert!(matches!(
            check(41, &boundary, &root).unwrap_err().cause,
            RetentionCause::Exhausted
        ));
    }
    cell[64..72].copy_from_slice(&(u64::MAX - 7).to_le_bytes());
    row_queue::encode_root_page(&cell, namespace, root.as_mut()).unwrap();
    check(41, &stamps, &root).unwrap();
    cell[64..72].copy_from_slice(&(u64::MAX - 6).to_le_bytes());
    row_queue::encode_root_page(&cell, namespace, root.as_mut()).unwrap();
    assert!(matches!(
        check(41, &stamps, &root).unwrap_err().cause,
        RetentionCause::Exhausted
    ));
}
