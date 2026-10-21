"""Apply actual product port bytes to the frozen bounded durable-image corpus.

The caller owns binary/source/log provenance. This module checks a finite
old/mid/full-prefix fault model; it neither executes recovery nor simulates all
device failures. Expected images were frozen independently before implementation.
"""
from copy import deepcopy
import hashlib
import itertools
import json
from pathlib import Path
import struct

FIXTURE = 'tests/fixtures/dev3-genesis-wal-durable.json'
FIXTURE_SHA256 = 'eaaa2bd5ccdd6db4846e73c8e641be34ded3dc9fa2a0b91f60803fc46efcf81a'
WAL = 'd3-wal-00000001'
ACTIONS = (
    ('profile_create', 'create', 'd3-profile'), ('profile_write', 'write', 'd3-profile'),
    ('profile_sync', 'sync', 'd3-profile'), ('epoch_create', 'create', WAL),
    ('epoch_write', 'write', WAL), ('epoch_sync', 'sync', WAL),
    ('initial_directory_sync', 'dirsync', ''), ('writer_stage_create', 'create', 'd3-writer-stage'),
    ('writer_stage_write', 'write', 'd3-writer-stage'), ('writer_stage_sync', 'sync', 'd3-writer-stage'),
    ('writer_publish', 'publish', 'd3-writer-stage'), ('selected_directory_sync', 'dirsync', ''),
    ('first_frame_write', 'write', WAL), ('first_frame_sync', 'sync', WAL),
)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def input_bytes(root):
    base = Path(root) / 'crates/engine/tests/fixtures'
    values = {name: (base / f'dev3-genesis-{name}.bin').read_bytes() for name in ('profile', 'epoch', 'writer')}
    expected = {'profile': 'c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4',
                'epoch': '2acd1b8f1a6199eb2f97f0b7e1a710ca8fa27e0570375ac7b660656ce0798a0c',
                'writer': '266b75b0428795c4ce01100377555f66477b58170112470d5be624ee7882aa34'}
    if any(len(data) != 4096 or sha(data) != expected[name] for name, data in values.items()):
        raise ValueError('independent genesis literal drift')
    frame = bytearray(192)
    frame[:4] = b'XWF1'
    struct.pack_into('<HBBHHI', frame, 4, 1, 3, 0, 160, 0, 192)
    frame[24:40] = bytes(range(1, 17))
    struct.pack_into('<QI', frame, 40, 1, 1)
    struct.pack_into('<Q', frame, 56, 1)
    frame[128:160] = hashlib.sha256(frame[:128]).digest()
    frame[160:] = hashlib.sha256(frame[:160]).digest()
    if sha(frame) != '1843c357f512c44522cc7a99dfe2b263145c6a70179325b3a934c64997677991':
        raise ValueError('independent first frame drift')
    return {1: values['profile'], 4: values['epoch'], 8: values['writer'], 12: bytes(frame)}


def parse_trace(output, root):
    lines = [line for line in output.splitlines() if line.startswith('WAL_IO_TRACE|')]
    if len(lines) != 14 or output.count('WAL_IO_TRACE|') != 14:
        raise ValueError('missing/duplicate/malformed actual port trace')
    payloads = input_bytes(root)
    events = []
    for at, (line, (_, kind, name)) in enumerate(zip(lines, ACTIONS, strict=True)):
        fields = line.split('|')
        if len(fields) != 7:
            raise ValueError('malformed port trace fields')
        _, index, actual_kind, actual_name, offset, length, data = fields
        event = {'at': int(index), 'kind': actual_kind, 'name': actual_name,
                 'offset': int(offset), 'length': int(length), 'data': bytes.fromhex(data)}
        expected = {'at': at, 'kind': kind, 'name': name, 'offset': 4096 if at == 12 else 0,
                    'length': len(payloads.get(at, b'')), 'data': payloads.get(at, b'')}
        if event != expected:
            raise ValueError(f'product port order/offset/bytes mismatch at {at}')
        events.append(event)
    return events


def images(events):
    # Object identity is its original exclusive-create name. Directory sync fixes
    # the name map; file sync fixes bytes. Rename's unresolved namespace permits
    # absent/stage/WRITER independently of the already synced selector bytes.
    files, names, published, renames = {}, {}, {}, {}
    rows = []

    def snapshot(label):
        alternatives = []
        for identity, obj in sorted(files.items()):
            confirmed = [name for name, target in published.items() if target == identity]
            if confirmed:
                candidates = confirmed
            elif identity in renames:
                candidates = [None, identity, renames[identity]]
            else:
                candidates = [None, identity]
            full, durable = obj['written'], obj['synced']
            lengths = sorted({len(durable), (len(durable) + len(full)) // 2, len(full)})
            alternatives.append([(name, full[:length]) for name in candidates for length in lengths])
        seen = {}
        for cells in itertools.product(*alternatives):
            image = {'LOCK': {'length': 0, 'sha256': sha(b'')}}
            for name, data in cells:
                if name is not None:
                    if name in image:
                        raise ValueError('modeled duplicate physical name')
                    image[name] = {'length': len(data), 'sha256': sha(data)}
            key = json.dumps(image, sort_keys=True)
            seen[key] = {'files': image, 'fresh_install_eligible': len(image) == 1,
                         'selected_genesis': 'WRITER' in image, 'db_ready': False}
        rows.append({'cut': label, 'image_count': len(seen), 'images': list(seen.values())})

    for event, (label, _, _) in zip(events, ACTIONS, strict=True):
        snapshot(label + ':before')
        kind, name = event['kind'], event['name']
        if kind == 'create':
            if name in names or name in files:
                raise ValueError('duplicate exclusive create')
            names[name] = name
            files[name] = {'written': b'', 'synced': b''}
        elif kind == 'write':
            obj = files[names[name]]
            if event['offset'] != len(obj['written']):
                raise ValueError('unexpected nonappend physical offset')
            obj['written'] += event['data']
        elif kind == 'sync':
            obj = files[names[name]]
            obj['synced'] = obj['written']
        elif kind == 'dirsync':
            published = dict(names)
            renames.clear()
        elif kind == 'publish':
            identity = names.pop(name)
            names['WRITER'] = identity
            renames[identity] = 'WRITER'
        else:
            raise ValueError('unknown effect')
        snapshot(label + ':after')
    return rows


def canonical(rows):
    result = deepcopy(rows)
    for row in result:
        row['images'].sort(key=lambda value: json.dumps(value, sort_keys=True))
    return result


def evaluate(output, root):
    root = Path(root)
    fixture = (root / FIXTURE).read_bytes()
    if sha(fixture) != FIXTURE_SHA256:
        raise ValueError('frozen independent durable corpus drift')
    reference = json.loads(fixture)
    events = parse_trace(output, root)
    rows = images(events)
    if len(rows) != 28 or sum(row['image_count'] for row in rows) != 82:
        raise ValueError('durable cut/image census mismatch')
    if canonical(rows) != canonical(reference['rows']):
        raise ValueError('actual port bytes produce different bounded durable images')
    return {'status': 'PASS_BOUNDED_ACTUAL_TRACE_MODEL', 'scope': __doc__,
            'fixture': FIXTURE, 'fixture_sha256': sha(fixture), 'cuts': 28, 'image_count': 82,
            'events': [{**event, 'data': event['data'].hex()} for event in events], 'rows': rows}
