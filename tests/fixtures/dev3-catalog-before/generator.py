#!/usr/bin/env python3
"""Independent frozen catalog-before literals; no product imports or execution."""
import datetime
import hashlib
import json
from pathlib import Path
import struct
import sys
import time

sys.dont_write_bytecode = True
BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]
SPEC = ROOT / 'artifacts/dev3/first-row-transaction-20261024/catalog-before-test-spec.md'
FREEZE = ROOT / 'artifacts/dev3/first-row-transaction-20261024/catalog-before-input-freeze-d2.md'
DB = bytes(range(1, 17))
PREPARATION_HISTORY = [{
    'attempt': 1, 'status': 'FAIL_BEFORE_ANY_OUTPUT_WRITE', 'exit_code': 1,
    'stderr': "IndexError: list index out of range at main raw metadata notes.get default evaluation (former line 322)",
    'elapsed_seconds': None,
    'elapsed_note': 'Script did not reach its timing receipt; no measured script duration is claimed.',
    'correction': 'Use conditional selection instead of eagerly evaluating dict.get default for N keys.',
}]


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def put(raw, offset, value, width=8):
    raw[offset:offset + width] = value.to_bytes(width, 'little')


def page(number, counter=5, incarnation=7):
    return struct.pack('<QQQ', (3 << 32) | counter, number, incarnation)


def ref(number, generation=9):
    return page(number) + bytes(8) + struct.pack('<Q', generation)


def txn(local, epoch=18, timeline=17):
    return DB + struct.pack('<QIIQ', timeline, epoch, 0, local)


def committed(q=77):
    return b'\x01' + bytes(7) + struct.pack('<Q', q) + bytes(40)


def named(prefix, length):
    assert len(prefix) <= length
    return prefix.encode() + b'a' * (length - len(prefix))


def definition(j):
    if j in (10, 11, 12):
        name = named('I', 128 if j == 11 else 1)
        mode = {10: 1, 11: 2, 12: 3}[j]
        columns = [0, 1, 2, 3] if j == 11 else [0]
        return dict(kind=2, name=name.decode(), mode=mode, columns=columns,
                    parent=[50, 7, 8], retired=j == 12)
    if j == 6:
        columns = [dict(name=n, type=i + 1, nullable=i % 2 == 0) for i, n in enumerate('biux')]
        name = b'T'
    elif j in (7, 8, 9, 14, 15):
        maximum = j in (9, 14, 15)
        name = named('T', 128 if maximum else 100 + (j == 8))
        columns = [dict(name=named(f'c{i:02}', 128).decode(), type=1 + i % 4, nullable=bool(i % 2))
                   for i in range(32 if maximum else 14)]
    else:
        name, columns = b'T', [dict(name='i', type=2, nullable=False)]
    return dict(kind=1, name=name.decode(), columns=columns, parent=[0, 0, 0], retired=j == 5)


def payload(j):
    d = definition(j)
    name = d['name'].encode()
    if d['kind'] == 1:
        body = struct.pack('<HBB', len(name), len(d['columns']), 0) + name
        for c in d['columns']:
            n = c['name'].encode()
            body += struct.pack('<HBB', len(n), c['type'], c['nullable']) + n
    else:
        columns = bytes(d['columns']) + bytes(4 - len(d['columns']))
        body = struct.pack('<HBB', len(name), d['mode'], len(d['columns'])) + columns + name
    head = bytearray(96)
    head[:4] = b'XCB1'
    put(head, 4, 1, 2)
    head[6:8] = bytes((d['kind'], d['retired']))
    put(head, 8, 7)
    put(head, 16, 8)
    head[24:48] = page(20000 + j, 6, 5)
    head[48:72] = struct.pack('<QQQ', *d['parent'])
    put(head, 72, len(body), 4)
    return bytes(head) + body


def root(j):
    raw = bytearray(432)
    raw[:4] = b'XUR1'
    put(raw, 4, 1, 2)
    releasing = j in (2, 13, 14, 15)
    raw[6] = 2 if releasing else 1
    put(raw, 8, 9)
    put(raw, 16, 10000 + j)
    raw[24:64] = txn(1000 + j)
    put(raw, 64, 20)
    put(raw, 72, 100)
    put(raw, 80, 0 if releasing else 2)
    raw[88] = 3
    for offset, value in ((96, 9001), (104, 4), (112, 100000 + j)):
        put(raw, offset, value)
    if j in (1, 2):
        return bytes(raw)
    raw[216:272] = (b'\x02' + bytes(7) + txn(1000 + j) + struct.pack('<Q', 20)) if j == 4 else committed()
    raw[272:320] = (txn(1000 + j) + struct.pack('<Q', 99)) if j == 4 else txn(3000 + j, 16) + struct.pack('<Q', 50)
    value = payload(j)
    put(raw, 320, len(value), 4)
    raw[324] = 5
    if len(value) <= 2048:
        raw[325] = 1
        raw.extend(value)
    else:
        raw[325] = 2
        raw[336:360] = page(30000 + j)
        put(raw, 360, len(value), 4)
        put(raw, 364, 1, 2)
        raw[368:400] = hashlib.sha256(value).digest()
    if j in (13, 14, 15):
        raw[326:329] = bytes((3, 3, 3))
        raw[216:272] = bytes(56)
        if j == 14:
            raw[400:424] = page(30000 + j)
        if j == 15:
            put(raw, 332, 1, 2)
    return bytes(raw)


def chunk(j, value=None):
    value = payload(j) if value is None else value
    raw = bytearray(160)
    raw[:4] = b'XUC1'
    put(raw, 4, 1, 2)
    put(raw, 8, 1, 2)
    put(raw, 12, len(value), 4)
    put(raw, 16, len(value), 4)
    raw[24:64] = ref(10000 + j)
    put(raw, 64, 10000 + j)
    raw[72:96] = page(30000 + j)
    raw[120:152] = hashlib.sha256(value).digest()
    put(raw, 152, 17)
    return bytes(raw) + value


def canonical_row():
    return struct.pack('<IBI', 1, 4, 2040) + bytes((17 * i + 3) % 256 for i in range(2040))


def negative(n, values):
    base = 4 if n in (7, 8, 9, 10) else 3
    base = {14: 1, 15: 1, 16: 1, 24: 10, 25: 10, 30: 6, 33: 8, 34: 8, 35: 9, 36: 8,
            41: 14, 45: 13, 46: 15, 47: 14, 48: 15, 49: 8, 50: 8}.get(n, base)
    b = bytearray(values[f'P{base:02}'])
    context = dict(base=f'P{base:02}', operation='decode_root_reject')
    def x(offset, value, width=1):
        put(b, 432 + offset, value, width)
    if n == 1:
        b[88] = 1
        context['target_kind'] = 'Row'
    elif n == 2:
        b[88:92] = bytes((2, 1, 10, 0))
        b[432:432] = b'\x01\x01' + (100003).to_bytes(8, 'big')
        context['target_kind'] = 'Index'
    elif n == 3: b[324] = 2
    elif n == 4: b[24] = 0x21
    elif n == 5: context.update(mutation_kind='context', byte_source='P03', expected_entity=100004)
    elif n == 6: context.update(mutation_kind='context', byte_source='P03', expected_target_generation=5)
    elif n == 7: put(b, 256, 1005)
    elif n == 8: put(b, 264, 21)
    elif n == 9: put(b, 304, 1005)
    elif n == 10: put(b, 312, 100)
    elif n == 11: put(b, 224, 0)
    elif n == 12: b[272] = 0x21
    elif n == 13:
        del b[432:]
        put(b, 320, 0, 4)
        b[325] = 0
    elif n == 14:
        b.extend(payload(3))
        put(b, 320, 106, 4)
        b[325] = 1
    elif n == 15: b[216:272] = b'\x02' + bytes(7) + txn(1001) + struct.pack('<Q', 20)
    elif n == 16:
        b[168:216] = ref(999) + struct.pack('<Q', 1)
        b[327] = 1
    elif n == 17: x(0, ord('Y'))
    elif n == 18: x(4, 2, 2)
    elif n == 19: x(6, 0)
    elif n == 20: x(7, 2)
    elif n == 21: x(8, 0, 8)
    elif n == 22: x(16, 0, 8)
    elif n == 23: x(48, 1, 8)
    elif n == 24: b[480:504] = bytes(24)
    elif n == 25: x(48, 100010, 8)
    elif n == 26: x(72, 11, 4)
    elif n == 27: x(76, 1)
    elif n == 28:
        b.append(0)
        put(b, 320, 107, 4)
    elif n == 29: x(100, 255)
    elif n == 30: x(110, ord('b'))
    elif n == 31: x(103, 5)
    elif n == 32: x(104, 2)
    elif n == 33:
        put(b, 320, 2048, 4)
        put(b, 360, 2048, 4)
    elif n == 34:
        b[325] = 1
        b[336:400] = bytes(64)
        b.extend(payload(8))
    elif n == 35:
        put(b, 320, 4453, 4)
        put(b, 360, 4453, 4)
    elif n == 36: put(b, 364, 2, 2)
    elif n in (37, 38, 39, 40):
        b = bytearray(values['C01'])
        context = dict(base='C01', root='P08', operation='chunk_or_materialization_reject')
        if n == 37: b[96:120] = page(40008)
        elif n == 38:
            b[-1] = ord('b')
            context.update(changed_region='chunk.payload', digest_fields='unchanged',
                           original_payload_sha256=sha(values['C01'][160:]), actual_payload_sha256=sha(b[160:]),
                           operation='streaming_and_materialized_digest_reject')
        elif n == 39: put(b, 56, 10)
        else:
            value = payload(10) + bytes(1944)
            b = bytearray(chunk(8, value))
            context.update(companion_root_digest=sha(value), operation='typed_index_trailing_reject')
    elif n == 41: put(b, 80, 1)
    elif n == 42: b[328] = 1
    elif n == 43:
        b[6] = 2
        put(b, 80, 0)
        b[328] = 1  # Normal base's Released version is faulted back to Never.
    elif n == 44:
        b[6] = 2
        put(b, 80, 0)
        b[327:329] = bytes((3, 2))  # Normal base's Released txn is faulted to Never.
    elif n == 45: b[216:272] = committed()
    elif n == 46: put(b, 332, 2, 2)
    elif n == 47: b[400:424] = bytes(24)
    elif n == 48: b[400:424] = page(30015)
    elif n == 49:
        b[88], b[324] = 1, 2
        context.update(target_kind='Row', operation='catalog_materializer_domain_reject', expected_error='StateMismatch')
    elif n == 50:
        b[368:400] = hashlib.sha256(canonical_row()).digest()
        context.update(operation='row_materializer_domain_reject', expected_error='StateMismatch',
                       materialized_payload_sha256=sha(canonical_row()))
    return bytes(b), context


FUNCTIONS = {
    'p': 'catalog_before_literals_match_independent_raw',
    'ns': 'catalog_before_target_and_stamp_rejections',
    'shape': 'catalog_before_shape_and_storage_rejections',
    'chunk': 'catalog_before_chunk_materialization_is_typed',
    'release': 'catalog_before_release_has_no_live_accessor',
    'loaded': 'catalog_before_loaded_version_identity',
    'output': 'catalog_before_encoder_failure_preserves_output',
    'legacy': 'undo_catalog_legacy_malformed_bodies',
    'bridge': 'catalog_before_xct1_definition_bridge',
    'static': 'catalog_before_static_source_census',
    'generated': 'catalog_before_s5_generated_definitions',
    'truncated': 'catalog_before_s5_all_prefixes_and_trailing',
    'reserved': 'catalog_before_s5_reserved_and_flags',
}


def inventory():
    out = {}
    def add(label, group, operation, expectation, phase='mandatory'):
        prefix = 'storage::undo_record::tests::' if group == 'legacy' else 'storage::catalog_record::before_tests::'
        out['CB-' + label] = dict(selector=prefix + FUNCTIONS[group], phase=phase, operation=operation,
                                 expected=expectation, status='NOT_RUN')
    for i in range(1, 16): add(f'P{i:02}', 'p', 'root_literal_and_declared_inline_accessor', 'exact_raw_and_typed_fields')
    for i in range(1, 4): add(f'C{i:02}', 'chunk', 'release_chunk_and_materializer_rejection' if i == 3 else 'typed_chunk_materialization', 'StateMismatch_after_release_success' if i == 3 else 'exact_fields')
    for i in range(1, 51):
        group = 'ns' if i <= 12 else 'shape' if i <= 36 else 'chunk' if i <= 40 or i >= 49 else 'release'
        add(f'N{i:02}', group, 'raw_negative_or_context', 'reject_at_declared_boundary')
    for i in range(1, 10): add(f'A{i:02}', 'release', 'generic_live_before' if i <= 4 else 'new_inline_accessor', 'StateMismatch')
    for i in range(1, 7): add(f'L{i:02}', 'loaded', 'loaded_previous_version', 'success' if i == 1 else 'reject')
    for i in range(1, 9): add(f'E{i:02}', 'output', 'xcb_encoder' if i <= 4 else 'root_encoder', 'NoSpace_full_sentinel_unchanged')
    for i in range(1, 11): add(f'I{i:02}', 'output', 'root_encoder' if i in (8, 9) else 'xcb_encoder', ('IdentityMismatch' if i == 7 else 'InvalidInput') + '_full_sentinel_unchanged')
    for i in range(1, 15): add(f'H{i:02}', 'shape', 'xcb_decoder', 'InvalidInput' if i == 14 else 'reject')
    for i in range(1, 6): add(f'K{i:02}', 'legacy', 'legacy_catalog_caller_encoded_len', 'StateMismatch' if i == 1 else 'InvalidInput')
    for i in range(1, 3): add(f'B{i:02}', 'bridge', 'xcb_xct1_definition_bridge', 'independent_exact_bytes_fields_tail')
    add('ST01', 'static', 'actual_source_value_census', 'caller<=24576_all_values<=65536')
    for n in range(512): add(f'G{n:03}', 'generated', f'seed={n // 8},profile={n % 8}', 'independent_exact_bytes_fields', 'S5')
    for k, length in enumerate((106, 2048, 2049, 4452, 105, 232), 1):
        for d in range(length): add(f'T{k:02}-{d:04}', 'truncated', 'xcb_prefix', 'reject_no_panic_input_unchanged', 'S5')
        add(f'T{k:02}-TAIL', 'truncated', 'xcb_trailing', 'reject_no_panic_input_unchanged', 'S5')
    for i in range(1, 25): add(f'W{i:02}', 'reserved', 'reserved_byte' if i <= 18 else 'unknown_flag', 'reject', 'S5')
    assert len(out) == 9657 and sum(v['phase'] == 'mandatory' for v in out.values()) == 123
    return out


def main():
    start = time.monotonic()
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    assert sha(SPEC.read_bytes()) == '58d0b314df8477a924587e7f0ec4e62a28be24aeee6fe8dc5a4fbcf22dde6444'
    assert sha(FREEZE.read_bytes()) == 'b9fdf44e1bf636742808089e72015c32929d3c0561e912f1007fbb42163a1fdd'
    values = {f'P{j:02}': root(j) for j in range(1, 16)}
    values.update({f'C{i:02}': chunk(j) for i, j in enumerate((8, 9, 14), 1)})
    notes = {}
    for n in range(1, 51):
        values[f'N{n:02}'], notes[f'N{n:02}'] = negative(n, values)
    assert len(values) == 68 and sum(map(len, values.values())) <= 512 * 1024
    assert values['N05'] == values['P03'] == values['N06']
    assert len(values['P07']) == 2480 and len(values['C01']) == 2209 and len(values['C02']) == 4612
    references, reference_bytes = {}, {}
    for name, expected in [('U04', 'c267a18f8ac192c9e4e9908ba54695c18854abe1a11e2fc9a7f88e589269f8be'),
                           ('U08', '84bea8378fa74d5830e29c3177c778da2acbc4ba54eaf34ab67a57e0a6e04c3d')]:
        path = ROOT / f'artifacts/dev3/undo-format-preimplementation/{name}.bin'
        data = path.read_bytes()
        assert sha(data) == expected
        references[name] = dict(path=f'tests/fixtures/dev3-catalog-before/references/{name}.bin',
                                original_path=str(path.relative_to(ROOT)), bytes=len(data), sha256=expected)
        reference_bytes[name] = data
    raw = {key: dict(path=f'raw/{key}.bin', bytes=len(value), sha256=sha(value),
                    expected=notes[key] if key in notes else ({'root_case': int(key[1:])} if key.startswith('P') else {'root_case': [8, 9, 14][int(key[1:]) - 1]}))
           for key, value in sorted(values.items())}
    cases = inventory()
    for n, note in notes.items(): cases['CB-' + n]['raw_expectation'] = note
    for j in range(3, 16): cases[f'CB-P{j:02}']['definition'] = definition(j)
    targets = ([BASE / row['path'] for row in raw.values()] + [BASE / 'manifest.json', BASE / 'provenance.md']
               + [BASE / f'references/{name}.bin' for name in reference_bytes])
    assert not any(path.exists() for path in targets), 'create_new only: frozen input already exists'
    (BASE / 'raw').mkdir(exist_ok=False)
    (BASE / 'references').mkdir(exist_ok=False)
    for name, data in reference_bytes.items():
        with (BASE / f'references/{name}.bin').open('xb') as stream:
            stream.write(data)
    for key, value in values.items():
        with (BASE / f'raw/{key}.bin').open('xb') as stream:
            stream.write(value)
    receipt = dict(kind='independent_fixture_generation_only', raw_files=68,
                   raw_bytes=sum(map(len, values.values())), mandatory=123, s5=9534,
                   started_utc=started, elapsed_seconds=time.monotonic() - start)
    stdout = json.dumps(receipt, sort_keys=True)
    manifest = dict(schema=1, state='INPUT_PREPARATION_NOT_PRODUCT_EXECUTION',
                    spec_sha256=sha(SPEC.read_bytes()), input_freeze_sha256=sha(FREEZE.read_bytes()),
                    generator_sha256=sha(Path(__file__).read_bytes()), preparation_history=PREPARATION_HISTORY,
                    generation={**receipt, 'stdout': stdout},
                    selectors=FUNCTIONS, raw=raw, references=references, cases=cases)
    with (BASE / 'manifest.json').open('x') as stream:
        json.dump(manifest, stream, indent=2, sort_keys=True)
        stream.write('\n')
    with (BASE / 'provenance.md').open('x') as stream:
        stream.write('# Catalog before independent raw provenance\n\n')
        stream.write('Input preparation only; product/build/tests were not executed.\n\n')
        stream.write('The first generator invocation failed before any output write: `IndexError: list index out of range` '
                     'in eager metadata-default evaluation. Its exit status was 1; no script duration receipt existed. '
                     'The corrected invocation below generated the first raw/manifest files. This failure is retained '
                     'in manifest preparation_history and is not classified as a product failure or PASS.\n\n')
        stream.write('The generator uses only Python struct/hashlib and frozen offset/definition rules. '
                     'No product parser, encoder, or output is imported. All files use create_new; '
                     'existing files are never overwritten. The exact generation output and elapsed time are in manifest.json.\n\n')
        stream.write('N05/N06 deliberately have P03 bytes and different caller expectations. '
                     'N38 changes only payload, leaving both digest fields unchanged; N40/N50 deliberately recompute the digest. '
                     'Referenced U04/U08 remain their original source inputs, not newly executed lower tests.\n\n')
        stream.write('Runtime references are tracked references/U04.bin and U08.bin. Fresh regeneration '
                     'requires the frozen spec/input documents and original historical raw sources; '
                     'all output raw, reference, and manifest paths must be absent.\n\n')
        stream.write(f'Generator SHA256: `{manifest["generator_sha256"]}`.\n\n')
        stream.write(f'Spec SHA256: `{manifest["spec_sha256"]}`.\n\n')
        stream.write(f'Generation: `{stdout}`\n')
    print(stdout)


if __name__ == '__main__':
    main()
