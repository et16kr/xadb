#!/usr/bin/env python3
"""Prepare plan3 inputs from independent bootstrap literals, never product output."""
import hashlib
import json
from pathlib import Path
import struct
import sys

sys.dont_write_bytecode = True
from dev3_storage_model import objects, expected_trace
from dev3_storage_native import prefix

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / 'tests/fixtures/dev3-undo-bootstrap'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def main():
    OUTPUT.mkdir(exist_ok=False)
    sources = ['scripts/dev3_storage_model.py', 'scripts/dev3_storage_native.py',
               'tests/fixtures/dev3-storage-session/effects.json']
    manifest = {'unit': 'D3-UNDO-BOOTSTRAP-001', 'status': 'PREPARED_NOT_EXECUTED',
                'files': {}, 'inputs': {}}
    for profile in (1, 2, 3):
        source = f'crates/engine/tests/fixtures/dev3-storage-session/literal/profile{profile}-d3-bootstrap.bin'
        sources.append(source)
        raw = bytearray((ROOT / source).read_bytes())
        assert len(raw) == 4096 and raw[4064:] == hashlib.sha256(raw[:4064]).digest()
        assert struct.unpack_from('<Q', raw, 120)[0] == struct.unpack_from('<Q', raw, 280)[0] == 1
        struct.pack_into('<Q', raw, 120, 3)
        struct.pack_into('<Q', raw, 280, 3)
        raw[4064:] = hashlib.sha256(raw[:4064]).digest()
        (OUTPUT / f'profile{profile}-d3-bootstrap.bin').write_bytes(raw)
    effects = json.loads((ROOT / sources[2]).read_text())
    files = objects(ROOT, 1)
    files['d3-bootstrap'] = (OUTPUT / 'profile1-d3-bootstrap.bin').read_bytes()
    events = expected_trace(effects, files, 1)
    assert len(events) == 350
    (OUTPUT / 'effects.tsv').write_text(''.join(
        f'{e["op"]}\t{e["name"] or "-"}\t{e["offset"]}\t{e["length"]}\t{e["sha256"]}\n' for e in events))
    lines = []
    for case, completed in [(1, 0), (2, 350), (3, 350), (4, 14)]:
        state = {'LOCK': b'', **prefix(effects, files, completed)}
        if case == 2:
            source = 'tests/fixtures/dev3-status-bootstrap/profile1-d3-bootstrap.bin'
            sources.append(source)
            state['d3-bootstrap'] = (ROOT / source).read_bytes()
        for name, raw in sorted(state.items()):
            lines.append(f'UB04-{case:02}\t{name}\t{len(raw)}\t{sha(raw)}\n')
    (OUTPUT / 'partial-files.tsv').write_text(''.join(lines))
    matrix = []
    for profile in (1, 2, 3):
        for wire in (0, 1, 2, 3, 4, 2**64 - 1):
            for high in (0, 1, 2, 3, 2**64 - 1):
                alias = wire == high == 3 or (profile == 1 and
                    ((wire != 3 and high == 3) or (wire == 3 and high != 3)))
                matrix.append({'id': f'UB-S5-P{profile}-W{wire}-H{high}', 'profile': profile,
                               'wire': wire, 'high': high, 'mandatory_alias': alias})
    assert len(matrix) == 90 and sum(not row['mandatory_alias'] for row in matrix) == 78
    (OUTPUT / 's5-matrix.json').write_text(json.dumps(matrix, indent=2) + '\n')
    # The independent model consumes these raw files as well as its own source.
    sources.extend(str(p.relative_to(ROOT)) for p in sorted(
        (ROOT / 'crates/engine/tests/fixtures/dev3-storage-session/literal').iterdir()) if p.is_file())
    manifest['inputs'] = {p: sha((ROOT / p).read_bytes()) for p in sorted(set(sources))}
    manifest['files'] = {p.name: {'bytes': p.stat().st_size, 'sha256': sha(p.read_bytes())}
                         for p in sorted(OUTPUT.iterdir())}
    (OUTPUT / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'files': len(manifest['files']), 's5_total': 90, 's5_not_run': 78}))


if __name__ == '__main__':
    main()
