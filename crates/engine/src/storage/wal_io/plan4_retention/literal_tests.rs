//! Independent field placement transcribed from the preserved Status literal
//! recipe; no product encoder, decoder, descriptor, or transition is called.
use super::*;

const CELL: usize = 10_144;
const CELL_BYTES: usize = 6_208;
const HOME: usize = 16_128;
fn put(out: &mut [u8], at: usize, value: u64, width: usize) {
    out[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
}
fn seal(out: &mut [u8]) {
    let at = out.len() - 32;
    let digest = content_digest(&out[..at]);
    out[at..].copy_from_slice(&digest);
}
fn work(id: u64) -> [u8; 48] {
    let mut out = [0; 48];
    for (i, byte) in out[..16].iter_mut().enumerate() {
        *byte = i as u8 + 1;
    }
    put(&mut out, 16, 1, 8);
    out[24] = 1;
    put(&mut out, 32, id, 8);
    put(&mut out, 40, 1, 8);
    out
}
fn token(generation: u64, birth: u64) -> [u8; 96] {
    let mut out = [0; 96];
    out[..2].copy_from_slice(&[1, 3]);
    put(&mut out, 8, 0x100000001, 8);
    put(&mut out, 16, 9, 8);
    put(&mut out, 24, generation, 8);
    put(&mut out, 64, 1, 8);
    put(&mut out, 72, birth, 8);
    out
}
fn allocator(before: &[u8], id: u64, lsn: u64, terminal: u8) -> Vec<u8> {
    let mut out = before.to_vec();
    let at = CELL + 64 + 9 * 96;
    let generation = u64::from_le_bytes(before[at + 8..at + 16].try_into().unwrap());
    let entry = &mut out[at..at + 96];
    entry.fill(0);
    if terminal == 0 {
        assert_eq!(before[at], 0);
        entry[..3].copy_from_slice(&[1, 0, 2]);
        put(entry, 8, generation + 1, 8);
        entry[32..80].copy_from_slice(&work(id));
    } else {
        assert_eq!(before[at], 1);
        put(entry, 8, generation, 8);
        if terminal == 2 {
            entry[..3].copy_from_slice(&[2, 9, 0]);
            put(entry, 16, 2, 8);
            put(entry, 24, 1, 8);
        }
    }
    put(&mut out, 64, lsn, 8);
    seal(&mut out);
    out
}
fn descriptor(profile: &[u8], id: u64, birth: u64, terminal: u8, item: &[u8; 96]) -> Vec<u8> {
    let finished = terminal != 0;
    let lsn = birth + u64::from(finished);
    let mut out = vec![0; 1536];
    out[..4].copy_from_slice(b"XOD1");
    put(&mut out, 4, 1, 2);
    out[6] = if finished { 6 } else { 5 };
    put(&mut out, 8, 1536, 4);
    out[16] = 2;
    out[24..72].copy_from_slice(&work(id));
    put(&mut out, 72, lsn, 8);
    put(&mut out, 80, if finished { birth } else { 0 }, 8);
    put(&mut out, 88, birth, 8);
    out[96] = 1;
    put(&mut out, 104, 1, 8);
    put(&mut out, 112, 4096, 4);
    out[128..160].copy_from_slice(&content_digest(profile));
    out[352..354].copy_from_slice(&[1, if finished { 4 } else { 1 }]);
    put(&mut out, 360, 1, 8);
    put(&mut out, 368, u64::from(finished), 8);
    put(&mut out, 376, 1, 8);
    out[448..480].copy_from_slice(&content_digest(&[]));
    put(&mut out, 544, lsn, 8);
    put(&mut out, 552, u64::from(!finished), 8);
    out[560..592].copy_from_slice(&content_digest(if finished { &[] } else { item }));
    put(
        &mut out,
        608,
        if terminal == 0 {
            21_200
        } else if terminal == 1 {
            42_400
        } else {
            59_200
        },
        8,
    );
    put(&mut out, 624, if finished { 2 } else { 1 }, 8);
    for category in 0..5 {
        out[640 + category * 128] = category as u8 + 1;
    }
    if !finished {
        for (at, value) in [
            (904, 1),
            (912, 38_000),
            (936, 1),
            (960, 2),
            (968, 114_688),
            (992, 1),
            (1008, 70_768),
            (1280, 2),
            (1288, 114_688),
            (1312, 1),
            (1328, 70_768),
            (1336, birth),
        ] {
            put(&mut out, at, value, 8);
        }
    }
    seal(&mut out);
    out
}

pub(super) struct Population {
    pub(super) frame: Vec<u8>,
    pub(super) allocator: Vec<u8>,
    pub(super) descriptor: Vec<u8>,
}
pub(super) fn population(
    oracle: &Oracle,
    before: &[u8],
    id: u64,
    birth: u64,
    terminal: u8,
    old_descriptor: &[u8],
) -> Population {
    assert!(terminal <= 2);
    let after = allocator(before, id, birth + u64::from(terminal != 0), terminal);
    let entry = CELL + 64 + 9 * 96;
    let generation = u64::from_le_bytes(after[entry + 8..entry + 16].try_into().unwrap());
    let item = token(generation, birth);
    let descriptor = descriptor(
        &oracle.files["references/REF-P1-d3-profile.bin"],
        id,
        birth,
        terminal,
        &item,
    );
    let init = terminal == 2;
    let finished = terminal != 0;
    let length = if init { 38_000 } else { 21_200 };
    let lsn = birth + u64::from(finished);
    let opcode = if finished { 10 } else { 8 };
    let mut frame = vec![0; length];
    frame[..8].copy_from_slice(b"XWF1\x01\x00\x03\x02");
    put(&mut frame, 8, 160, 2);
    put(&mut frame, 12, length as u64, 4);
    put(&mut frame, 16, length as u64 - 192, 4);
    put(&mut frame, 20, if init { 2 } else { 1 }, 2);
    frame[24..40].copy_from_slice(&work(id)[..16]);
    put(&mut frame, 40, 1, 8);
    put(&mut frame, 48, 1, 4);
    put(&mut frame, 56, lsn, 8);
    put(&mut frame, 64, lsn - 1, 8);
    let header_sha = content_digest(&frame[..128]);
    frame[128..160].copy_from_slice(&header_sha);
    let group = &mut frame[160..352];
    group[..4].copy_from_slice(b"XSG1");
    put(group, 4, 1, 2);
    put(group, 6, opcode, 2);
    put(group, 8, 192, 2);
    put(group, 12, length as u64 - 192, 4);
    put(group, 16, lsn, 8);
    put(group, 24, if finished { birth } else { 0 }, 8);
    group[32] = 2;
    group[40..88].copy_from_slice(&work(id));
    if finished {
        group[88..120].copy_from_slice(&content_digest(old_descriptor));
    }
    put(group, 120, 1536, 4);
    put(group, 124, if init { 2 } else { 1 }, 2);
    put(group, 130, 1, 2);
    put(group, 132, if init { 2 } else { 1 }, 2);
    put(group, 136, if init { 12_896 } else { 12_544 }, 4);
    put(group, 148, 208, 4);
    put(group, 152, if init { 22_720 } else { 6272 }, 4);
    frame[352..1888].copy_from_slice(&descriptor);
    put(&mut frame, 1888, opcode, 2);
    put(&mut frame, 1892, 256, 4);
    put(&mut frame, 2056, u64::from(finished), 8);
    let native = &mut frame[2144..14_688];
    native[..2].copy_from_slice(&[6, 3]);
    put(native, 8, CELL as u64, 2);
    put(native, 10, CELL as u64, 2);
    put(native, 12, CELL_BYTES as u64, 4);
    put(native, 16, CELL_BYTES as u64, 4);
    put(native, 20, 12_544, 4);
    native[24..56].copy_from_slice(&content_digest(&before[CELL..16_352]));
    native[56..88].copy_from_slice(&content_digest(&after[CELL..16_352]));
    native[128..6336].copy_from_slice(&before[CELL..16_352]);
    native[6336..].copy_from_slice(&after[CELL..16_352]);
    let status = oracle.page("status-free.page");
    if init {
        let native = &mut frame[14_688..15_040];
        native[..8].copy_from_slice(&[5, 2, 1, 0, 255, 255, 0, 0]);
        put(native, 10, HOME as u64, 2);
        put(native, 16, 224, 4);
        put(native, 20, 352, 4);
        native[56..88].copy_from_slice(&content_digest(&status[HOME..16_352]));
        native[128..].copy_from_slice(&status[HOME..16_352]);
    }
    let delta = if init { 15_040 } else { 14_688 };
    frame[delta] = if terminal == 0 {
        1
    } else if terminal == 2 {
        2
    } else {
        3
    };
    frame[delta + 1] = if finished { 1 } else { 2 };
    let token_at = delta + if finished { 16 } else { 112 };
    frame[token_at..token_at + 96].copy_from_slice(&item);
    let patch = &mut frame[delta + 208..delta + 6480];
    patch[..2].copy_from_slice(&[1, 10]);
    put(patch, 4, 6272, 4);
    patch[8..48].copy_from_slice(&before[24..64]);
    patch[48..56].copy_from_slice(&before[64..72]);
    put(patch, 56, CELL as u64, 2);
    put(patch, 58, CELL_BYTES as u64, 2);
    patch[64..].copy_from_slice(&after[CELL..16_352]);
    if init {
        let redo = &mut frame[21_520..37_968];
        redo[..2].copy_from_slice(&[2, 9]);
        put(redo, 4, 16_448, 4);
        redo[8..48].copy_from_slice(&status[24..64]);
        put(redo, 58, 16_384, 2);
        redo[64..].copy_from_slice(status);
    }
    seal(&mut frame);
    Population {
        frame,
        allocator: after,
        descriptor,
    }
}
