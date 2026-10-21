"""Verify actual DWB install traces and the frozen bounded prefix-loss corpus.

Fresh files are an already-synced initial state. No recovery, arbitrary torn
sector, device power-loss or complete DEV3 claim is made by this checker.
"""
import itertools
import json
from pathlib import Path

from dev3_storage_model import (sha, check_trace, check_images,
                               inputs as storage_inputs, objects as storage_objects)

from dev3_doublewrite_checks import trace_mutations, image_mutations, observation_mutations

FIXTURES = 'tests/fixtures/dev3-doublewrite-install'
HASHES = {'manifest.json': 'b794e062669d4e7442d1398a484dadfaafd193ab5ff9de70788a6a8177139811', 'effects.json': '772282407afdc00bd95309bb6bac38023bacc9376c43ae6ce0c8ec3b15e04a01', 'observations.json': '93350869a7a0226fa091571379e9a203c1b74158c9234228b3b950983f980dad', 'durable-images.json': '3a8dbc86b9bc3d62e3a61acb0985583481d70fd3bb836bef3c3148e81bbc4362', 'checker-negatives.json': '8a04184424648a966a9c97d6c82b3b3fa2a41e04abab3983e3724ceb8c0cb192', 'semantic-expectations.json': 'ed2fc5c585465113b5409bfc27c12566145b423c3f9eae148f5bbc08e5ea8314'}
TARGETS = {'d3-dwb-select-stage': 'DWB'}


def inputs(root):
    storage_inputs(root)
    result = {}
    for name, expected in HASHES.items():
        data = (Path(root) / FIXTURES / name).read_bytes()
        if sha(data) != expected:
            raise ValueError('FAIL_FROZEN_INPUT')
        result[name] = json.loads(data)
    for name, expected in result['manifest.json']['literal_files'].items():
        raw = (Path(root) / FIXTURES / name).read_bytes()
        if len(raw) != expected['bytes'] or sha(raw) != expected['sha256']:
            raise ValueError('FAIL_FROZEN_LITERAL')
    return result


def objects(root, profile):
    files = storage_objects(root, profile)
    def literal(name):
        return (Path(root) / FIXTURES / 'literal' / f'profile{profile}-{name}.bin').read_bytes()
    files.update({'DWB': literal('DWB'), 'd3-dwb-stage': literal('d3-dwb-stage'),
                  'd3-dwb-0': literal('empty') + bytes(16384 - 160),
                  'd3-dwb-1': bytes(16384), 'd3-dwb-reserved': bytes(8192),
                  'd3-dwb-images': bytes(16711680)})
    return files


def expected_trace(plan, files, profile):
    rows = []
    for event in plan:
        start, length = event['offset'], event['length']
        raw = files[TARGETS.get(event['name'], event['name'])][start:start + length] if event['op'] == 'write' else b''
        rows.append({key: event[key] for key in ('index', 'op', 'name', 'offset', 'length')} |
                    {'profile': profile, 'sha256': sha(raw)})
    return rows


def parse_rows(output, marker, count, numeric):
    lines = [line for line in output.splitlines() if line.startswith(marker)]
    if len(lines) != count or output.count(marker) != count:
        raise ValueError('FAIL_OBSERVATION_CENSUS' if 'OBSERVATION' in marker else 'FAIL_EFFECT_CENSUS')
    rows = []
    for line in lines:
        try:
            pairs = [part.split('=', 1) for part in line.removeprefix(marker).split()]
            row = dict(pairs)
            if len(row) != len(pairs):
                raise ValueError('duplicate key')
            for key in numeric:
                row[key] = int(row[key])
            if row['name'] == '-':
                row['name'] = ''
            if row['op'] == 'dirsync':
                row['op'] = 'directory_sync'
        except (ValueError, KeyError) as error:
            raise ValueError('FAIL_TRACE_PARSE') from error
        rows.append(row)
    return rows


def check_observations(actual, expected):
    if len(actual) != len(expected):
        raise ValueError('FAIL_OBSERVATION_CENSUS')
    if actual != expected:
        raise ValueError('FAIL_OBSERVATION_ORDER')


def durable_images(plan, files, initial):
    # File contents and namespace durability are distinct. This replay starts
    # from the observed, already-validated plan, not the frozen image generator.
    current, synced = dict(initial), dict(initial)
    names = {name: name for name in initial}
    confirmed = dict(names)
    latest = {name: 0 for name in initial}
    renamed, hashes, rows = {}, {}, []

    def observe(index, side):
        choices, bound = [], 1
        for identity in sorted(current):
            live = next(name for name, target in names.items() if target == identity)
            stable = [name for name, target in confirmed.items() if target == identity]
            alternatives = stable or ([None, identity, live] if identity in renamed else [None, live])
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
            bound *= len(values)
            if bound > 256:
                raise ValueError('FAIL_MODEL_BOUND')
            choices.append(values)
        images = [sorted((value for value in choice if value is not None), key=lambda row: row['name'])
                  for choice in itertools.product(*choices)]
        rows.append({'index': index, 'side': side, 'images': images})

    for event in plan:
        name, op = event['name'], event['op']
        observe(event['index'], 'before')
        if op == 'create':
            if name in names:
                raise ValueError('FAIL_EFFECT_ORDER')
            current[name], synced[name], latest[name], names[name] = b'', b'', 0, name
        elif op == 'write':
            target = names[name]
            start, length = event['offset'], event['length']
            if len(current[target]) != start:
                raise ValueError('FAIL_EFFECT_ORDER')
            current[target] += files[TARGETS.get(name, name)][start:start + length]
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
        observe(event['index'], 'after')
    return rows


def evaluate(output, root):
    frozen = inputs(root)
    expected, observed_expected = [], []
    for profile in (1, 2, 3):
        files = objects(root, profile)
        manifest = {name: {'bytes': len(raw), 'sha256': sha(raw)} for name, raw in files.items()}
        if manifest != frozen['manifest.json']['installed_files'][str(profile)]:
            raise ValueError('FAIL_FROZEN_LITERAL')
        expected += expected_trace(frozen['effects.json'], files, profile)
        observed_expected += [dict(row, profile=profile) for row in frozen['observations.json']]
    numeric = ('profile', 'index', 'offset', 'length')
    actual = parse_rows(output, 'DWB_INSTALL_EFFECT ', 75, numeric)
    check_trace(actual, expected)
    observations = parse_rows(output, 'DWB_INSTALL_OBSERVATION ', 96, numeric)
    check_observations(observations, observed_expected)
    actual_plan = [{key: row[key] for key in ('index', 'op', 'name', 'offset', 'length')} |
                   {'to': TARGETS[row['name']] if row['op'] == 'publish' else ''}
                   for row in actual if row['profile'] == 1]
    images = durable_images(actual_plan, objects(root, 1), storage_objects(root, 1))
    check_images(images, frozen['durable-images.json'])
    if len(images) != 50 or sum(len(row['images']) for row in images) != 308 or max(len(row['images']) for row in images) != 32:
        raise ValueError('FAIL_CUT_CENSUS')
    negatives = frozen['checker-negatives.json']
    mutants = {'trace': trace_mutations(actual, expected, negatives['trace']),
               'images': image_mutations(images, frozen['durable-images.json'], negatives['images']),
               'observations': observation_mutations(observations, observed_expected, negatives['observations'], check_observations)}
    if [len(mutants[key]) for key in ('trace', 'images', 'observations')] != [7, 7, 5]:
        raise ValueError('FAIL_CHECKER_CENSUS')
    return {'status': 'PASS', 'checker_counterexamples': mutants, 'trace_events': 75, 'observations': 96, 'cuts': 50,
            'images': 308, 'max_images_per_cut': 32, 'bound_per_cut': 256,
            'frozen_inputs': HASHES, 'scope': 'fixed prefix-loss model; no recovery or power-loss execution'}
