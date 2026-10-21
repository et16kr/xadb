"""Check actual storage effects against the pre-implementation r6 corpus.

Only the stated prefix-loss alternatives are explored. No recovery is executed
and a process kill is never treated as a power-loss or durability proof.
"""
import hashlib
import itertools
import json
from pathlib import Path

FIXTURES = 'tests/fixtures/dev3-storage-session'
LITERALS = 'crates/engine/tests/fixtures/dev3-storage-session/literal'
HASHES = {
    'manifest.json': '2e35b50c42b21f12448e98c8e2825d1bfaa73dc4bc1f72dca0c5cafb79aa3d5f',
    'effects.json': 'c6ff7445cc963eda23cdd88135ed20fbb9200ae3a47858d1585457f54cbe83b8',
    'durable-images.json': 'ac79632d406c9487e0fe44d24c9c3a13879bfceac27dd6ef5e9a1ba3c684c86e',
    'checker-negatives.json': '7d6c24f3920bffba99f918b4defd71b2b40f72a89aea5a5843c5910210413b47',
}
DATA = 'd3-data-0000000100000001'
WAL = 'd3-wal-00000001'
TARGETS = {'d3-writer-stage': 'WRITER', 'd3-filemap-select-stage': 'FILEMAP'}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def inputs(root):
    result = {}
    for name, expected in HASHES.items():
        data = (Path(root) / FIXTURES / name).read_bytes()
        if sha(data) != expected:
            raise ValueError('FAIL_FROZEN_INPUT')
        result[name] = json.loads(data)
    for name, expected in result['manifest.json']['literal_files'].items():
        raw = (Path(root) / LITERALS / name).read_bytes()
        if len(raw) != expected['bytes'] or sha(raw) != expected['sha256']:
            raise ValueError('FAIL_FROZEN_LITERAL')
    return result


def objects(root, profile):
    def literal(name):
        return (Path(root) / LITERALS / f'profile{profile}-{name}.bin').read_bytes()
    files = {name: literal(name) for name in ('d3-profile', WAL, 'WRITER', 'd3-bootstrap', 'FILEMAP')}
    files[DATA] = literal('allocation') + bytes(1048576 - 16384)
    files['d3-filemap-0'] = literal('root') + bytes(16384 - 320) + literal('chunk') + bytes(2113536 - 16384 - 256)
    files['d3-filemap-1'] = bytes(2113536)
    return files


def expected_trace(plan, files, profile):
    events = []
    for event in plan:
        offset, length = event['offset'], event['length']
        data = files[TARGETS.get(event['name'], event['name'])][offset:offset + length] if event['op'] == 'write' else b''
        events.append({'profile': profile, 'index': event['index'],
                       'op': event['op'], 'name': event['name'],
                       'offset': offset, 'length': length, 'sha256': sha(data)})
    return events


def parse_trace(output):
    lines = [line for line in output.splitlines() if line.startswith('STORAGE_EFFECT ')]
    if len(lines) != 1050 or output.count('STORAGE_EFFECT ') != 1050:
        raise ValueError('FAIL_EFFECT_CENSUS')
    rows = []
    for line in lines:
        try:
            marker, profile, index, op, name, offset, length, digest = line.split()
            rows.append({'profile': int(profile), 'index': int(index),
                         'op': 'directory_sync' if op == 'dirsync' else op,
                         'name': '' if name == '-' else name,
                         'offset': int(offset), 'length': int(length), 'sha256': digest})
        except ValueError as error:
            raise ValueError('FAIL_EFFECT_ORDER') from error
    return rows


def check_trace(rows, expected):
    if len(rows) != len(expected):
        raise ValueError('FAIL_EFFECT_CENSUS')
    for actual, want in zip(rows, expected, strict=True):
        if {k: v for k, v in actual.items() if k != 'sha256'} != {k: v for k, v in want.items() if k != 'sha256'}:
            raise ValueError('FAIL_EFFECT_ORDER')
        if actual['sha256'] != want['sha256']:
            raise ValueError('FAIL_EFFECT_BYTES')


def image(files):
    return [{'name': name, 'length': len(raw), 'sha256': sha(raw)} for name, raw in sorted(files.items())]


def durable_images(plan, files):
    """Replay independently from actual, literal-checked successful writes.

    A file has confirmed bytes, current bytes, and its latest write length.
    Name publication is distinct from file sync. This implementation deliberately
    does not import the generator that froze the expected image corpus.
    """
    current, synced, names, confirmed, renamed = {}, {}, {}, {}, {}
    latest, rows, hashes = {}, [], {}

    def observe(index, side):
        choices = []
        for identity in sorted(current):
            live_name = next(name for name, target in names.items() if target == identity)
            stable_names = [name for name, target in confirmed.items() if target == identity]
            alternatives = stable_names or ([None, identity, live_name] if identity in renamed else [None, live_name])
            lengths = {len(synced[identity])}
            if len(current[identity]) > len(synced[identity]):
                lengths.update((len(current[identity]), len(current[identity]) - latest[identity] // 2))
            values = []
            for name in alternatives:
                if name is None:
                    values.append(None)
                else:
                    for length in sorted(lengths):
                        key = identity, length
                        if key not in hashes:
                            hashes[key] = sha(current[identity][:length])
                        values.append({'name': name, 'length': length, 'sha256': hashes[key]})
            choices.append(values)
        images = [sorted((value for value in choice if value is not None), key=lambda v: v['name'])
                  for choice in itertools.product(*choices)]
        rows.append({'index': index, 'side': side, 'images': images})

    for event in plan:
        op, name, index = event['op'], event['name'], event['index']
        if index >= 13:
            observe(index, 'before')
        if op == 'create':
            current[name], synced[name], latest[name], names[name] = b'', b'', 0, name
        elif op == 'write':
            target = names[name]
            offset, length = event['offset'], event['length']
            if len(current[target]) != offset:
                raise ValueError('FAIL_EFFECT_ORDER')
            current[target] += files[TARGETS.get(name, name)][offset:offset + length]
            latest[target] = length
        elif op == 'sync':
            synced[names[name]] = current[names[name]]
        elif op == 'publish':
            target = names.pop(name)
            names[event['to']] = target
            renamed[target] = event['to']
        elif op == 'directory_sync':
            confirmed = names.copy()
            renamed.clear()
        else:
            raise ValueError('FAIL_EFFECT_ORDER')
        if index >= 13:
            observe(index, 'after')
    return rows


def canonical_images(images):
    return sorted(json.dumps(sorted(value, key=lambda row: row['name']), sort_keys=True) for value in images)


def check_images(actual, expected):
    if [(r['index'], r['side']) for r in actual] != [(r['index'], r['side']) for r in expected]:
        raise ValueError('FAIL_CUT_CENSUS')
    for row, want in zip(actual, expected, strict=True):
        if canonical_images(row['images']) != canonical_images(want['images']):
            raise ValueError('FAIL_DURABLE_IMAGE')


def evaluate(output, root):
    frozen = inputs(root)
    plan = frozen['effects.json']
    expected = []
    for profile in (1, 2, 3):
        files = objects(root, profile)
        manifest = {name: {'bytes': len(raw), 'sha256': sha(raw)} for name, raw in files.items()}
        if manifest != frozen['manifest.json']['installed_files'][str(profile)]:
            raise ValueError('FAIL_FROZEN_LITERAL')
        expected += expected_trace(plan, files, profile)
    check_trace(parse_trace(output), expected)
    actual = durable_images(plan, objects(root, 1))
    check_images(actual, frozen['durable-images.json'])
    if len(actual) != 676 or sum(len(r['images']) for r in actual) != 6804 or max(len(r['images']) for r in actual) != 16:
        raise ValueError('FAIL_CUT_CENSUS')
    return {'status': 'PASS', 'trace_events': 1050, 'cuts': 676, 'images': 6804,
            'max_images_per_cut': 16, 'profile_for_durable_model': 1,
            'frozen_inputs': HASHES, 'scope': 'r6 bounded prefix-loss corpus; no recovery or power-loss execution'}
