//! Additional live mixed-load boundaries over unchanged valid home images.
//! This child reuses only the parent memory harness, never another test case.
use super::super::{history::HistoryError, load::LoadCause};
use super::*;
use crate::storage::page_io::PageIoError;

fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}

fn home_eof(action: port::Action) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b, _) = three_dirty_history!(bound, owner, flusher);
        bound.split().2.try_flush(&mut flusher, b.target).unwrap();
        assert_work(&bound, &state, 1, 3);
        let expected_home = literal(1, "sequence/work1-payload.after");
        assert_eq!(
            &state.lock().unwrap().files[HOME].bytes[PAGE_BYTES..2 * PAGE_BYTES],
            expected_home
        );
        let before = snapshot(&bound.inner.inner.pool);
        let prefix = storage.wal().append.lock().unwrap().prefix;
        let descriptor = bound.inner.inner.control.base.descriptor.to_vec();
        let files = file_inventory(&state);
        let (writes, reads) = io_counts(&state);
        state.lock().unwrap().named_read_plan = Some((HOME, PAGE_BYTES as u64, PAGE_BYTES, action));
        let error = bound.load_into(2, a.target).unwrap_err();
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(matches!(
            &error.cause,
            LoadCause::PageIo(PageIoError::Read(error))
                if error.io_kind == Some(std::io::ErrorKind::UnexpectedEof)
        ));
        assert!(storage.session_fence().is_fenced());
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(storage.wal().append.lock().unwrap().prefix, prefix);
        assert_eq!(
            bound.inner.inner.control.base.descriptor.as_slice(),
            descriptor
        );
        assert_eq!(file_inventory(&state), files);
        let state = state.lock().unwrap();
        assert_eq!(state.events.len(), writes);
        assert_eq!(state.configured_read_hits, 1);
        assert!(state.named_read_plan.is_none());
        let home_reads: Vec<_> = state.read_events[reads..]
            .iter()
            .filter(|event| event.kind == "read" && event.name == HOME)
            .collect();
        if action == port::Action::Zero {
            assert_eq!(home_reads.len(), 1);
            assert_eq!(
                (
                    home_reads[0].offset,
                    home_reads[0].length,
                    home_reads[0].returned
                ),
                (PAGE_BYTES as u64, PAGE_BYTES, Some(0))
            );
            assert_eq!(state.continuation_read_hits, 0);
        } else {
            assert_eq!(action, port::Action::PrefixEof);
            assert_eq!(home_reads.len(), 2);
            assert_eq!(
                (
                    home_reads[0].offset,
                    home_reads[0].length,
                    home_reads[0].returned
                ),
                (PAGE_BYTES as u64, PAGE_BYTES, Some(PAGE_BYTES / 2))
            );
            assert_eq!(home_reads[0].data, expected_home[..PAGE_BYTES / 2]);
            assert_eq!(
                (
                    home_reads[1].offset,
                    home_reads[1].length,
                    home_reads[1].returned
                ),
                (
                    (PAGE_BYTES + PAGE_BYTES / 2) as u64,
                    PAGE_BYTES / 2,
                    Some(0)
                )
            );
            assert_eq!(state.continuation_read_hits, 1);
        }
    });
}

scenario!(
    mixed_owner_load_home_zero_eof_fences_without_install,
    "MO-LOAD-023",
    home_eof(port::Action::Zero)
);
scenario!(
    mixed_owner_load_home_partial_eof_fences_without_install,
    "MO-LOAD-024",
    home_eof(port::Action::PrefixEof)
);

fn unsupported_history(variant: u8) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b, _) = three_dirty_history!(bound, owner, flusher);
        bound.split().2.try_flush(&mut flusher, b.target).unwrap();
        assert_work(&bound, &state, 1, 3);
        let expected_home = literal(1, "sequence/work1-payload.after");
        assert_eq!(
            &state.lock().unwrap().files[HOME].bytes[PAGE_BYTES..2 * PAGE_BYTES],
            expected_home
        );
        // Every frame begins with independent raw input. Mutations target later
        // Undo work although the selected valid home is the earlier Status A:
        // the complete retained prefix remains necessary to authorize a load.
        let mut parts: Vec<_> = (1..=3)
            .flat_map(|work| {
                [
                    literal(1, &format!("sequence/work{work}-reserve.wal")),
                    literal(1, &format!("sequence/work{work}-terminal.wal")),
                ]
            })
            .collect();
        let changed = if variant < 2 { 3 } else { 5 };
        let offset = 4096 + parts[..changed].iter().map(Vec::len).sum::<usize>();
        let frame = &mut parts[changed];
        match variant {
            0 => {
                assert_eq!(frame[14688], 3);
                frame[14688] = 4; // Unsupported Undo chunk native kind.
            }
            1 => {
                assert_eq!(&frame[296..300], &13104u32.to_le_bytes());
                frame[296..300].copy_from_slice(&12896u32.to_le_bytes());
            }
            2 => {
                let descriptor = &mut frame[352..1888];
                assert_eq!(&descriptor[608..616], &59408u64.to_le_bytes());
                // This is a locally well-formed Finished descriptor claiming
                // the Status terminal's smaller consumption for an Undo INIT.
                descriptor[608..616].copy_from_slice(&59200u64.to_le_bytes());
                seal(descriptor);
                assert_eq!(content_digest(&descriptor[..1504]), descriptor[1504..]);
            }
            3 => {
                // The preceding completed work is real and SHA-valid, but its
                // digest is not the current work3 reserve descriptor witness.
                let wrong_previous = literal(1, "sequence/work2-terminal.descriptor");
                let digest = content_digest(&wrong_previous);
                assert_ne!(&frame[248..280], &digest);
                frame[248..280].copy_from_slice(&digest);
            }
            _ => unreachable!(),
        }
        seal(frame);
        assert_eq!(content_digest(&frame[..128]), frame[128..160]);
        assert_eq!(
            content_digest(&frame[..frame.len() - 32]),
            frame[frame.len() - 32..]
        );
        let changed_body = frame[160..].to_vec();
        let wrong_frames = parts.concat();
        {
            let mut state = state.lock().unwrap();
            let wal = state.files.get_mut(wal::WAL_NAME).unwrap();
            assert_eq!(wal.bytes.len(), 4096 + wrong_frames.len());
            wal.bytes[4096..].copy_from_slice(&wrong_frames);
        }
        let before = snapshot(&bound.inner.inner.pool);
        let prefix = storage.wal().append.lock().unwrap().prefix;
        let descriptor = bound.inner.inner.control.base.descriptor.to_vec();
        let files = file_inventory(&state);
        let (writes, reads) = io_counts(&state);
        let error = bound.load_into(2, a.target).unwrap_err();
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(matches!(
            error.cause,
            LoadCause::History(HistoryError::Invalid)
        ));
        assert!(storage.session_fence().is_fenced());
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(storage.wal().append.lock().unwrap().prefix, prefix);
        assert_eq!(
            bound.inner.inner.control.base.descriptor.as_slice(),
            descriptor
        );
        assert_eq!(file_inventory(&state), files);
        let state = state.lock().unwrap();
        assert_eq!(state.events.len(), writes);
        let home_reads: Vec<_> = state.read_events[reads..]
            .iter()
            .filter(|event| event.kind == "read" && event.name == HOME)
            .collect();
        assert_eq!(home_reads.len(), 1);
        assert_eq!(home_reads[0].data, expected_home);
        let changed_reads: Vec<_> = state.read_events[reads..]
            .iter()
            .filter(|event| {
                event.kind == "read"
                    && event.name == wal::WAL_NAME
                    && event.offset == (offset + 160) as u64
            })
            .collect();
        assert_eq!(changed_reads.len(), 1);
        assert_eq!(changed_reads[0].length, changed_body.len());
        assert_eq!(changed_reads[0].returned, Some(changed_body.len()));
        assert_eq!(changed_reads[0].data, changed_body);
    });
}

scenario!(
    mixed_owner_load_rejects_sha_valid_later_undo_class,
    "MO-LOAD-025",
    unsupported_history(0)
);
scenario!(
    mixed_owner_load_rejects_sha_valid_later_undo_geometry,
    "MO-LOAD-026",
    unsupported_history(1)
);
scenario!(
    mixed_owner_load_rejects_sha_valid_terminal_descriptor_budget,
    "MO-LOAD-027",
    unsupported_history(2)
);
scenario!(
    mixed_owner_load_rejects_sha_valid_wrong_descriptor_predecessor,
    "MO-LOAD-028",
    unsupported_history(3)
);
