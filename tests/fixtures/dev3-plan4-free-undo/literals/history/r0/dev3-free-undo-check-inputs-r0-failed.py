"""Read-only independent literal consistency checks; no product imports."""
from pathlib import Path
import hashlib
import json
import resource
import struct

resource.setrlimit(resource.RLIMIT_AS, (64 << 20, 64 << 20))
repo = Path('/home/et16/work/xadb')
fixture = repo / 'tests/fixtures/dev3-plan4-free-undo'
sha = lambda raw: hashlib.sha256(raw).digest()
u64 = lambda raw, at: struct.unpack_from('<Q', raw, at)[0]
u32 = lambda raw, at: struct.unpack_from('<I', raw, at)[0]
u16 = lambda raw, at: struct.unpack_from('<H', raw, at)[0]
index = (fixture / 'raw-index.tsv').read_bytes()
assert sha(index).hex() == 'e48c132b3473e9f160d77903774fcfedb66cbae9e410481307ef15a42de23239'
raw = {}
for row in index.decode().splitlines()[1:]:
    key, path, length, digest = row.split('\t')
    value = (repo / path).read_bytes()
    assert len(value) == int(length) and sha(value).hex() == digest
    assert key not in raw
    raw[key] = value
assert len(raw) == 48
frames = {}
for epoch, first_lsn, lengths in [(1, 1, [21200,38000,21200,38208,21200,38208]),
                                  (2, 7, [21200,38000,21200,38208]),
                                  (3, 11, [21200,38000,21200,38208])]:
    suffix = raw[f'native-wal-e{epoch}-suffix.bin']
    assert len(suffix) == sum(lengths)
    offset = 0
    for i, size in enumerate(lengths):
        frames[first_lsn+i] = (suffix[offset:offset+size], epoch)
        offset += size
for lsn in range(8, 14):
    frames[lsn+100] = (raw[f'memory/{lsn:02}.wal'], 1)
frames[202] = (raw['pure/undo-02.wal'], 1)
frames[302] = (raw['pure/cancel-02.wal'], 1)
for key, (frame, epoch) in frames.items():
    lsn = key % 100
    assert frame[:8] == b'XWF1\x01\x00\x03\x02'
    assert u32(frame,12) == len(frame) and u32(frame,16) == len(frame)-192
    assert u32(frame,48) == epoch and u64(frame,56) == lsn and u64(frame,64) == lsn-1
    assert frame[128:160] == sha(frame[:128]) and frame[-32:] == sha(frame[:-32])
    assert frame[160:164] == b'XSG1' and u16(frame,164) == 2 and u16(frame,170) == 1
    assert frame[1856:1888] == sha(frame[352:1856])
    assert frame[2168:2200] == sha(frame[2272:8480])
    assert frame[2200:2232] == sha(frame[8480:14688])
    if lsn % 2:
        assert len(frame) == 21200 and u16(frame,166) == 8
        assert u64(frame,352+912) == 38208 and u64(frame,352+1008) == 70976
        assert u64(frame,352+1328) == 70976 and u64(frame,352+608) == 21200
        assert frame[248:280] == bytes(32)
    else:
        assert u16(frame,166) == 10
        assert u64(frame,352+608) == 21200 + len(frame)
        assert all(frame[352+640+i*128] == i+1 for i in range(5))
        assert all(frame[352+641+i*128:352+768+i*128] == bytes(127) for i in range(5))
    if len(frame) in (38000,38208):
        page = frame[-16416:-32]
        assert page[-32:] == sha(page[:-32])
        assert u64(page,64) == lsn and u16(page,72) == 1 and u16(page,74) == 132
        cell_size = 432 if len(frame) == 38208 else 224
        assert u16(page,76) == 16352-cell_size and u16(page,130) == cell_size
        body = page[16352-cell_size:16352]
        assert frame[14816:14816+cell_size] == body
        assert frame[14744:14776] == sha(body)
        if cell_size == 432:
            assert body[:8] == b'XUR1\x01\x00\x00\x00'
            assert u64(body,8) == 1 and body[16:] == bytes(416)
            assert page[6] == 8 and u64(page,48) == 3
        else:
            assert body[:4] == b'XSH1' and body[32:39] == b'XTS1\x01\x00\x00'
            assert u64(body,80) == 1 and page[6] == 9 and u64(page,48) == 2
before = raw['reference/P00.bin']
for work in range(1,8):
    reserve, terminal = frames[2*work-1][0], frames[2*work][0]
    assert reserve[2272:8480] == before[10144:16352]
    assert terminal[2272:8480] == reserve[8480:14688]
    assert terminal[248:280] == sha(reserve[352:1888])
    before = raw[f'native-allocator-l{2*work}.page']
    assert before[-32:] == sha(before[:-32]) and u64(before,64) == 2*work
    assert before[10144:16352] == terminal[8480:14688]
    assert raw[f'native-page{8+work}-init.page'] == terminal[-16416:-32]
assert u64(raw['memory/08.wal'], 64) == 7
assert u64(raw['memory/08.wal'], 14688+208+48) == 2  # allocator Patch prior pageLSN
assert u64(raw['memory/12.wal'],8480+64+11*96+8) == 3
base = raw['base/mixed-l6.xrb']
assert len(base) == 1048576 and u16(base,4) == 2 and base[-32:] == sha(base[:-32])
assert u64(base,96) == 6 and u64(base,208) == 3 and base[304] == 3
assert u64(base,264) == 1<<9
prefix = raw['reference/wal-header.bin'] + raw['native-wal-e1-suffix.bin']
assert u64(base,152) == len(prefix) and base[160:192] == sha(prefix)
for i,(offset,length) in enumerate([(4096,55296),(65536,16384),(81920,98304),
                                    (180224,22528),(202752,6144),(208896,16384)]):
    assert base[1280+i*64+32:1280+i*64+64] == sha(base[offset:offset+length])
assert base[65536:81920] == raw['native-allocator-l6.page']
assert base[4096+32*1536:4096+33*1536] == frames[6][0][352:1888]
for number in range(64):
    record = base[180224+number*352:180224+(number+1)*352]
    cert = base[202752+number*96:202752+(number+1)*96]
    if number >= 12:
        assert record == bytes(352) and cert == bytes(96)
    elif number >= 9:
        page = raw[f'native-page{number}-init.page']
        assert record[:4] == bytes([1 if number == 9 else 2,page[6],0,1])
        assert record[8:56] == page[24:72] and u64(record,56) == u64(page,64)
        assert record[64:96] == sha(page) and cert[56:88] == sha(page)
        assert cert[8:56] == page[24:72]
        if number > 9:
            assert record[96:] == bytes(256)
    else:
        assert record == bytes(352)
report = {'schema':'XADB_PLAN4_FREE_UNDO_INPUT_CHECK_V1',
          'status':'INDEPENDENT_INPUT_CONSISTENCY_CHECKED_NOT_PRODUCT_VERIFICATION',
          'checker_sha256':sha(Path(__file__).read_bytes()).hex(),
          'raw_index_sha256':sha(index).hex(),'indexed_files':48,
          'complete_frames_checked':len(frames),'native_pages_checked':7,
          'native_allocators_checked':7,'base_sections_checked':6,
          'product_imports':0,'product_tests':0}
out = fixture/'literals/independent-check.json'
with out.open('x') as stream:
    json.dump(report,stream,indent=2,sort_keys=True);stream.write('\n')
print(json.dumps(report,sort_keys=True))
