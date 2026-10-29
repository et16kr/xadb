#!/usr/bin/env python3
"""D3-CREATE-TRANSITION3-G1: Profile3 transition CREATE input literals.

The version2 transition corpus (../transition) with one change: the serializer
module is module-row-format/catalog3/serializer3.py load() (Profile3 ID1
envelope, Job3 free cells, Root3 with row_high 0 and the Job3 census
aggregate). canonical_cases and mutants are the unchanged version2 phase code
inside that module; emit below is the version2 emit with only the output
directory, generator id and status text changed. serializer3.row_job is never
called. No Rust, product encoder/decoder, test binary or product output is
imported or executed. These bytes convey no runtime authority.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import time
import traceback

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
ADOPTED_PLAN = '4bf20c59207890d66b5b51d06d7778a478f1226e26ce38b7cc803856fbe12181'
SERIALIZER3 = 'tests/fixtures/module-row-format/catalog3/serializer3.py'
SERIALIZER3_SHA = '0b23d146ef7fa5a1ba712baee37c60c882011f36832ef11b5f1d2b49dea70de1'
PROFILE3_SHA = 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e'


def sha(raw):
    return hashlib.sha256(raw).digest()


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def load_serializer3():
    source = REPO / SERIALIZER3
    need(sha(source.read_bytes()).hex() == SERIALIZER3_SHA, 'serializer3 pin')
    spec = importlib.util.spec_from_file_location('xadb_transition3_serializer3', source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.load()


def emit(cases, profile_raw):
    blobs = {}; records = []
    def blob(raw):
        if raw is None:
            return '-'
        need(len(raw) <= 163840, 'raw file bound')
        h = sha(raw).hex(); blobs[h] = raw; return h
    profile_id = blob(profile_raw)
    for identifier, case in sorted(cases.items()):
        b, a, p, op = case['before'], case['after'], case['plan'], case['op']
        row = dict(id=identifier, base=case.get('base', '-'), expected=case['expected'], operation=op,
                   lsn=a['last_lsn'], ordinal=case['ordinal'], plan={k: v.hex() if isinstance(v, bytes) else v for k, v in p.items()},
                   before={str(r): blob(image) for r, image in b['images'].items() if r < 6},
                   jobs=blob(b''.join(b['jobs'])), anchor=blob(b['anchor']), transaction=blob(b['tx']), aging=blob(b['aging']),
                   frame=blob(case['frame']), order=case['roles'], after={str(r): blob(a['images'][r]) for r in case['roles']},
                   external_gap=blob(b.get('external_gap')))
        # Runtime harness uses this bounded, explicitly ordered scalar record;
        # JSON is the human/auditor view, not an implicit test parser dependency.
        fields = [identifier, row['expected'], str(op), str(row['lsn']), str(case['ordinal'] if case['ordinal'] is not None else -1),
                  str(p.get('local', 0)), str(p.get('generation', 0)), str(p.get('status', 0)), str(p.get('undo', 0)),
                  str(p.get('ug', 0)), str(p.get('pair', (0, 0))[0]), str(p.get('pair', (0, 0))[1]), str(p.get('inc', 0)),
                  str(p.get('c', 0)), p.get('projection', bytes(32)).hex(), str(int(p.get('maximum', False))), p.get('name', '-'),
                  row['jobs'], row['anchor'], row['transaction'], row['aging'], row['frame'],
                  ','.join(map(str, row['order'])), *[row['before'].get(str(r), '-') for r in range(6)],
                  *[row['after'].get(str(r), '-') for r in range(8)]]
        row['tsv'] = '\t'.join(fields); records.append(row)
    total = sum(map(len, blobs.values())); need(total <= 20 * 1024 * 1024, ('corpus cap', total))
    need(len(records) == 72 and sum(r['expected'] == 'ACCEPT' for r in records) == 46, 'manifest count')
    index = 'sha256\tbytes\tfile\n' + ''.join(f'{h}\t{len(raw)}\traw/{h}.bin\n' for h, raw in sorted(blobs.items()))
    selectors = '\n'.join(r.pop('tsv') for r in records) + '\n'
    manifest = dict(generator='D3-CREATE-TRANSITION3-G1', authority='NONE', status='GENERATED_INPUT_NOT_PRODUCT_EXECUTION',
                    count=72, positive=46, negative=26, raw_count=len(blobs), raw_bytes=total, profile=profile_id,
                    generator_sha256=sha(Path(__file__).read_bytes()).hex(),
                    input_plan_sha256=sha((HERE / 'input-plan.json').read_bytes()).hex(),
                    raw_index_sha256=sha(index.encode()).hex(), cases_tsv_sha256=sha(selectors.encode()).hex(), cases=records)
    # All arithmetic and corpus bounds close before the first raw write.
    (HERE / 'raw').mkdir()
    for h, raw in sorted(blobs.items()):
        with (HERE / 'raw' / (h + '.bin')).open('xb') as f:
            f.write(raw)
    for name, data in (('raw-index.tsv', index.encode()), ('cases.tsv', selectors.encode()),
                       ('manifest.json', (json.dumps(manifest, indent=2, ensure_ascii=False) + '\n').encode())):
        with (HERE / name).open('xb') as f:
            f.write(data)
    return dict(status='GENERATED_INPUT_NOT_PRODUCT_EXECUTION', cases=72, raw_files=len(blobs), raw_bytes=total,
                manifest_sha256=sha((HERE / 'manifest.json').read_bytes()).hex())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--adopted-plan-sha256', required=True)
    args = parser.parse_args()
    need(args.adopted_plan_sha256 == ADOPTED_PLAN, 'explicit frozen plan')
    need(not any((HERE / p).exists() for p in ('raw', 'manifest.json', 'generation-result.json')), 'write-once output already exists')
    started = time.monotonic()
    result = {}
    try:
        raw = (HERE / 'input-plan.json').read_bytes()
        need(sha(raw).hex() == ADOPTED_PLAN, 'plan hash')
        plan = json.loads(raw)
        for group in ('derived_from', 'source_inputs', 'immutable_existing'):
            for path, h in plan[group].items():
                need(sha((REPO / path).read_bytes()).hex() == h, ('pinned input', path))
        t = load_serializer3()
        prof = t.profile()
        need(sha(prof).hex() == PROFILE3_SHA == plan['profile3_id1_sha256'], 'Profile3 ID1 literal')
        cases = t.canonical_cases(prof)
        t.mutants(cases)
        result = emit(cases, prof)
    except Exception as error:
        result = dict(status='GENERATION_FAILED_NOT_PRODUCT_RUN', error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        result['wall_seconds'] = time.monotonic() - started
        result['generator_sha256'] = sha(Path(__file__).read_bytes()).hex()
        result['plan_sha256'] = ADOPTED_PLAN
        with (HERE / 'generation-result.json').open('x') as f:
            json.dump(result, f, indent=2); f.write('\n')
        print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
    main()
