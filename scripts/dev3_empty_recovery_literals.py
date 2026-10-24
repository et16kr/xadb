#!/usr/bin/env python3
"""Raw empty-transaction recovery fixtures using documented wire offsets.

No product imports, encoders, execution, or observed outputs. Existing independent
System histories are inputs; transaction descriptors/pages/frames use raw bytes.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / 'scripts/dev3_empty_abort_literals.py'
RAW_SHA = 'cc6926518a05d12d5596a9b0a30ae9d265ecdf825ffef026ba1f5b8fa16bde6b'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def generate(out):
    assert not out.exists(), 'immutable fixture destination already exists'
    assert digest(RAW.read_bytes()) == RAW_SHA
    spec = importlib.util.spec_from_file_location('independent_empty_raw', RAW)
    raw = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(raw)
    raw.TIMELINE = 1
    raw.WRITER_EPOCH = 1
    provenance = {}

    def source(path):
        data = (ROOT / path).read_bytes()
        provenance[path] = {'path': path, 'bytes': len(data), 'sha256': digest(data)}
        return data

    profile = source('crates/engine/tests/fixtures/dev3-storage-session/literal/profile1-d3-profile.bin')
    records = []

    def emit(path, data):
        target = out / path
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open('xb') as file:
            file.write(data)
        records.append({'path': path, 'bytes': len(data), 'sha256': digest(data)})

    def descriptor(actor, cell, phase, lsn, previous, birth):
        b = bytearray(1536)
        b[:4] = b'XOD1'
        raw.put(b, 4, 1, 2)
        b[6] = (1, 2, 4)[phase]
        raw.put(b, 8, 1536, 4)
        b[16] = 1
        b[24:72] = actor
        for at, value in [(72, lsn), (80, previous), (88, birth), (104, raw.at(profile, 40)),
                          (544, birth), (608, 3040 * (phase + 1)), (624, phase + 1), (1336, birth)]:
            raw.put(b, at, value)
        b[96] = 1
        raw.put(b, 112, 4096, 4)
        b[128:160] = raw.sha(profile)
        b[160:352] = cell[32:]
        b[560:592] = raw.sha(b'')
        active_rows = ({2, 3}, {2}, set())[phase]
        for row in range(5):
            at = 640 + row * 128
            b[at] = row + 1
            if row in active_rows:
                for i, value in enumerate((1, 3040, 0, 0, 0, 0, 0, *raw.PEAK, 3040)):
                    raw.put(b, at + 8 + i * 8, value)
        if active_rows:
            for i, value in enumerate(raw.PEAK):
                raw.put(b, 1280 + i * 8, value)
        raw.put(b, 1328, len(active_rows) * 3040)
        return raw.seal(b)

    scenarios = []
    for name, base, tip, pages, steps in [
        ('interleaved', 'tests/fixtures/dev3-operational-status/p1', 6,
         {1: 'work1-status.after', 2: 'work3-status.after'},
         [(1, 2, 0), (2, 5, 0), (1, 2, 1), (1, 2, 2), (2, 5, 1)]),
        ('capacity', 'tests/fixtures/dev3-operational-recovery/p1-extent', 250,
         {i: f'status-{i}.after' for i in range(1, 64)},
         [(i, i, 0) for i in range(1, 33)] + [(1, 1, 1), (1, 1, 2), (63, 34, 0)]),
    ]:
        base_wal = source(f'{base}/wal.frames-only')
        current = {i: source(f'{base}/{path}') for i, path in pages.items()}
        owners, frames, transitions = {}, [], []
        for n, (page_no, local, phase) in enumerate(steps, 1):
            before = current[page_no]
            raw.DB = before[8:24]
            raw.FILE, raw.PAGE_NO, raw.INCARNATION, raw.OBJECT, raw.OBJECT_GENERATION = struct.unpack_from('<5Q', before, 24)
            assert raw.PAGE_NO == page_no and raw.at(before, 72, 2) == 1
            actor = raw.actor_bytes(2, local=local, epoch=1)
            cell = raw.home((1, 2, 4)[phase], 2, local=local, epoch=1)
            lsn = tip + n
            previous, birth, old = owners.get(local, (0, lsn, None))
            desc = descriptor(actor, cell, phase, lsn, previous, birth)
            frame = raw.frame((1, 3, 7)[phase], (1, 2, 6)[phase], lsn, lsn - 1,
                              previous, actor, 0, before, cell, desc, old, writer_epoch=1)
            page = bytearray(before)
            page[16128:16352] = cell
            raw.put(page, 64, lsn)
            page = raw.seal(page)
            frames.append(frame)
            owners[local] = (lsn, birth, desc)
            current[page_no] = page
            stem = f'{name}/step{n}'
            for suffix, value in [('wal', frame), ('page', page), ('descriptor', desc)]:
                emit(stem + '.' + suffix, value)
            transitions.append({'step': n, 'lsn': lsn, 'page': page_no, 'local': local, 'phase': phase,
                                'prior_page_lsn': raw.at(before, 64), 'previous_owner_lsn': previous,
                                'remaining_wal': (2 - phase) * 3040})
            if name == 'capacity' and n == 32:
                # A separately authored 33rd simultaneous Begin. The accepted
                # continuation instead completes actor1 before local34 begins.
                p63 = current[63]
                raw.FILE, raw.PAGE_NO, raw.INCARNATION, raw.OBJECT, raw.OBJECT_GENERATION = struct.unpack_from('<5Q', p63, 24)
                a33 = raw.actor_bytes(2, local=33, epoch=1)
                c33 = raw.home(1, 2, local=33, epoch=1)
                d33 = descriptor(a33, c33, 0, lsn + 1, 0, lsn + 1)
                emit('capacity/rejected33.wal', raw.frame(1, 1, lsn + 1, lsn, 0, a33, 0, p63, c33, d33, None, writer_epoch=1))
        emit(f'{name}/transaction.frames-only', b''.join(frames))
        scenarios.append({'name': name, 'base': base, 'base_lsn': tip, 'base_bytes': len(base_wal),
                          'transitions': transitions, 'final_lsn': tip + len(steps)})
    emit('manifest.json', (json.dumps({'status': 'RAW_INPUTS_ONLY_NOT_EXECUTION_EVIDENCE',
         'generator_sha256': digest(Path(__file__).read_bytes()), 'raw_generator_sha256': RAW_SHA,
         'provenance': list(provenance.values()), 'scenarios': scenarios, 'outputs': records}, indent=2) + '\n').encode())
    print(json.dumps({'files': len(records), 'bytes': sum(r['bytes'] for r in records)}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    generate(args.output)
