#!/usr/bin/env python3
"""Read-only Profile3 transition auditor; imports no generator, serializer or product.

Checks the raw inventory, the version3 Root/Job wire everywhere it appears in a
page or Job bank, the absence of any version2 Root/Job magic in any blob, the
Profile3 profile blob, schedule equivalence with the frozen version2
transition corpus, and frame identity with native3 where version2 has it.
"""
import hashlib
import json
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
PLAN_SHA = '4bf20c59207890d66b5b51d06d7778a478f1226e26ce38b7cc803856fbe12181'
PROFILE3_ID1 = 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e'
PAGE = 16384


def check(value, explanation):
    if not value:
        raise ValueError(explanation)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def number(raw, at, width=8):
    return int.from_bytes(raw[at:at + width], 'little')


def cells(page):
    count = number(page, 72, 2)
    check(128 + 4 * count <= PAGE - 32, 'slot directory')
    for ordinal in range(count):
        at = 128 + ordinal * 4
        offset, length = number(page, at, 2), number(page, at + 2, 2)
        check(128 + 4 * count <= offset <= offset + length <= PAGE - 32, 'cell range')
        yield page[offset:offset + length]


def root3(cell):
    check(cell[:6] == b'XQR1\x03\x00' and len(cell) == 512, 'Root3 wire')
    check(number(cell, 432) == 0 and not any(cell[440:512]), 'Root3 row_high0 and zero tail')


def job3(cell):
    check(cell[:6] == b'XQJ1\x03\x00' and len(cell) == 1024 and cell[7] != 3, 'Job3 wire, no Row kind')


def table(path, fields):
    rows = {}
    for line in (REPO / path).read_text().splitlines():
        f = line.split('\t')
        check(len(f) == fields, ('fields', path))
        check(f[0] not in rows, ('duplicate', f[0]))
        rows[f[0]] = f
    return rows


def run():
    plan_raw = (HERE / 'input-plan.json').read_bytes()
    check(digest(plan_raw) == PLAN_SHA, 'frozen plan')
    plan = json.loads(plan_raw)
    for group in ('derived_from', 'source_inputs', 'immutable_existing'):
        for path, h in plan[group].items():
            check(digest((REPO / path).read_bytes()) == h, ('pinned input', path))
    manifest = json.loads((HERE / 'manifest.json').read_text())
    index_raw = (HERE / 'raw-index.tsv').read_bytes()
    cases_raw = (HERE / 'cases.tsv').read_bytes()
    check(manifest['input_plan_sha256'] == PLAN_SHA, 'manifest plan')
    check(manifest['raw_index_sha256'] == digest(index_raw), 'index hash')
    check(manifest['cases_tsv_sha256'] == digest(cases_raw), 'cases hash')
    check(manifest['generator_sha256'] == digest((HERE / 'generate_literals.py').read_bytes()), 'generator hash')
    sizes = {}
    lines = index_raw.decode().splitlines()
    check(lines[0] == 'sha256\tbytes\tfile', 'index header')
    for line in lines[1:]:
        h, n, path = line.split('\t')
        check(path == f'raw/{h}.bin', 'raw path')
        raw = (HERE / path).read_bytes()
        check(len(raw) == int(n) and digest(raw) == h, ('raw hash', h))
        sizes[h] = len(raw)
    on_disk = {p.name for p in (HERE / 'raw').iterdir()}
    check(on_disk == {f'{h}.bin' for h in sizes}, 'raw inventory')
    check((len(sizes), sum(sizes.values())) == (manifest['raw_count'], manifest['raw_bytes']), 'raw totals')
    check(manifest['profile'] == PROFILE3_ID1 and PROFILE3_ID1 in sizes, 'Profile3 profile blob')

    roots = jobs = 0
    for h in sizes:
        raw = (HERE / 'raw' / f'{h}.bin').read_bytes()
        check(b'XQR1\x02\x00' not in raw and b'XQJ1\x02\x00' not in raw, ('version2 queue magic', h))
        if len(raw) == PAGE and raw[:4] == b'XDP1':
            for c in cells(raw):
                if c[:4] == b'XQR1':
                    root3(c); roots += 1
                elif c[:4] == b'XQJ1':
                    job3(c); jobs += 1
        elif len(raw) == 64 * 1024 and raw[:4] == b'XQJ1':
            for i in range(64):
                job3(raw[i * 1024:(i + 1) * 1024]); jobs += 1

    v3 = table('tests/fixtures/dev3-catalog-create/transition3/cases.tsv', 37)
    v2 = table('tests/fixtures/dev3-catalog-create/transition/cases.tsv', 37)
    v2_sizes = {}
    for line in (REPO / 'tests/fixtures/dev3-catalog-create/transition/raw-index.tsv').read_text().splitlines()[1:]:
        h, n, _ = line.split('\t')
        v2_sizes[h] = int(n)
    expected = plan['expected']
    check(len(v3) == expected['cases'] == manifest['count'], 'case count')
    check(sum(r[1] == 'ACCEPT' for r in v3.values()) == expected['positive'], 'positive count')
    check(set(v3) == set(v2), 'case ids')
    differ = 0
    for key, a in v3.items():
        b = v2[key]
        check(a[:17] == b[:17] and a[22] == b[22], ('schedule scalars', key))
        for i in range(17, 37):
            if i != 22:
                check((a[i] == '-') == (b[i] == '-'), ('hash presence', key, i))
                if a[i] != '-':
                    check(a[i] in sizes and sizes[a[i]] == v2_sizes[b[i]], ('blob length', key, i))
        if a[1] == 'ACCEPT':
            check(a[21] != b[21], ('positive frame differs from version2', key))
            differ += 1

    def steps(path):
        out = {}
        rows = (REPO / path).read_text().splitlines()
        header = rows[0].split('\t')
        for line in rows[1:]:
            f = dict(zip(header, line.split('\t')))
            out[f['id']] = f
        return out
    n3 = steps('tests/fixtures/dev3-catalog-create/native3/steps.tsv')
    n2 = steps('tests/fixtures/dev3-catalog-create/native/steps.tsv')
    check(set(n3) == set(n2), 'native step ids')
    equal3 = {k for k, f in n3.items() if f['pure_reference'] in v3 and f['frame'] == v3[f['pure_reference']][21]}
    equal2 = {k for k, f in n2.items() if f['pure_reference'] in v2 and f['frame'] == v2[f['pure_reference']][21]}
    epoch1 = {k for k, f in n3.items() if f['pure_reference'] in v3 and f['epoch'] == '1'}
    check(epoch1 <= equal3, ('epoch1 native3 frame identity', sorted(epoch1 - equal3)))
    check(equal3 == equal2, ('native/transition equal set', sorted(equal3 ^ equal2)))
    return dict(status='INPUT_AUDIT_PASS_NOT_PRODUCT_EXECUTION', cases=len(v3), raw_files=len(sizes),
                raw_bytes=sum(sizes.values()), root3_cells=roots, job3_cells=jobs, positive_frames_differ=differ,
                native3_equal_frames=len(equal3))


def main():
    started = time.monotonic()
    result = run()
    result['wall_seconds'] = round(time.monotonic() - started, 3)
    result['auditor_sha256'] = digest(Path(__file__).read_bytes())
    print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
    main()
