#!/usr/bin/env python3
"""Generate only the newly adopted duplicate-native-cell input, once."""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
BASE = HERE.parent
ORIGINAL_MANIFEST_SHA = '84b8aa4c0607d314dc373a956e6d75c7dcdca701209ef18ab0470c23826a6d5d'
ORIGINAL_INDEX_SHA = '728977808b9c1abd66939bce4989c487d7c3ebc6c71182b97222419dd113b35c'


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def main():
    if (HERE / 'manifest.json').exists() or (HERE / 'raw').exists():
        raise SystemExit('write-once addendum; preserve existing input')
    manifest_bytes = (BASE / 'manifest.json').read_bytes()
    index_bytes = (BASE / 'raw-index.tsv').read_bytes()
    assert digest(manifest_bytes) == ORIGINAL_MANIFEST_SHA
    assert digest(index_bytes) == ORIGINAL_INDEX_SHA
    old = json.loads(manifest_bytes)
    base = next(row for row in old['cases'] if row['id'] == 'W02')
    raw = bytearray((BASE / base['file']).read_bytes())
    assert digest(raw) == base['sha256']
    # Independent contract sum: header160 + XHG256 + descriptors3072 + core256
    # + allocator native(128+2*6208) + Root native(128+2*512).
    native_job_start = 160 + 256 + 3072 + 256 + 12544 + 1152
    assert native_job_start == 17440
    # Rewrite redo index=1, before slot=0, after slot=0. Its kind9 stays intact,
    # after Root kind8, so (redo,slot,kind) still strictly increases while the
    # physical cell key (redo1,slot0) duplicates. Native body hashes are unchanged.
    patch = b'\x01\x00\x00\x00\x00\x00'
    offset = 17442
    raw[offset:offset + len(patch)] = patch
    raw[128:160] = hashlib.sha256(raw[:128]).digest()
    raw[-32:] = hashlib.sha256(raw[:-32]).digest()
    (HERE / 'raw').mkdir()
    (HERE / 'raw' / 'NW19.bin').write_bytes(patch)
    # Retain original manifest/index bytes without replacing their original paths.
    history = HERE / 'original-r0'
    history.mkdir()
    (history / 'manifest.json').write_bytes(manifest_bytes)
    (history / 'raw-index.tsv').write_bytes(index_bytes)
    result = {
        'schema': 1, 'input_id': 'D3-CREATE-LEAF-L1-R1',
        'state': 'NEW_INPUT_FROZEN_NOT_EXECUTED', 'base_manifest_sha256': ORIGINAL_MANIFEST_SHA,
        'base_index_sha256': ORIGINAL_INDEX_SHA,
        'generator_sha256': digest(Path(__file__).read_bytes()),
        'plan_sha256': digest((HERE / 'input-plan.json').read_bytes()),
        'raw_files': 1, 'raw_bytes': 6, 'previous_raw_regenerated': 0,
        'product_calls': 0, 'compiler_calls': 0, 'test_runner_calls': 0,
        'cases': [{'id': 'NW19', 'category': 'wire-negative-patch',
                   'file': 'raw/NW19.bin', 'bytes': 6, 'sha256': digest(patch),
                   'expected': 'REJECT_LAYOUT_DUPLICATE_PHYSICAL_CELL',
                   'base_manifest': '../manifest.json', 'base': 'W02', 'offset': offset,
                   'reseal': 'XWF_HEADER_AND_FRAME_SHA256', 'materialized_sha256': digest(raw),
                   'reason': 'same redo1/slot0 with distinct kind8 and kind9 is forbidden'}],
    }
    (HERE / 'manifest.json').write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    index = 'id\tpath\tbytes\tsha256\nNW19\traw/NW19.bin\t6\t' + digest(patch) + '\n'
    (HERE / 'raw-index.tsv').write_text(index)
    print(json.dumps({'state': result['state'], 'raw_files': 1, 'raw_bytes': 6,
                      'manifest_sha256': digest((HERE / 'manifest.json').read_bytes()),
                      'index_sha256': digest(index.encode()), 'product_calls': 0}))


if __name__ == '__main__':
    main()
