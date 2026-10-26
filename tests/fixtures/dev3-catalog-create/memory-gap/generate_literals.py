#!/usr/bin/env python3
"""Write-once independent MC03/MC04 inputs; no product code or output."""
import argparse
import copy
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
ADOPTED = '3c21253e75cb168a34c7f4d054a27e1bbeb536221a96389208052371f4dd9208'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def load(path, expected, name):
    source = REPO / path
    need(sha(source.read_bytes()) == expected, ('source pin', path))
    spec = importlib.util.spec_from_file_location(name, source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write(inputs, plan):
    index = 'sha256\tbytes\tfile\n' + ''.join(
        f'{h}\t{len(raw)}\traw/{h}.bin\n' for h, raw in sorted(inputs.blobs.items()))
    fields = ['id', 'scenario', 'lsn', 'epoch', 'kind', 'operation', 'bytes',
              'wal_start', 'wal_end', 'frame', 'pure_reference', 'jobs_before', 'jobs_after',
              'transaction_before', 'transaction_after', 'aging_before', 'aging_after',
              'system_before', 'system_after', 'before', 'after']
    lines = ['\t'.join(fields)]
    for row in inputs.rows:
        values = []
        for field in fields:
            value = row[field]
            if isinstance(value, dict):
                value = ','.join(f'{r}:{h}' for r, h in sorted(value.items(), key=lambda x: int(x[0])))
            values.append('-' if value is None else str(value))
        lines.append('\t'.join(values))
    steps = '\n'.join(lines) + '\n'
    manifest = dict(generator=plan['id'], authority='NONE',
                    status='GENERATED_INPUT_NOT_PRODUCT_EXECUTION', plan_sha256=ADOPTED,
                    generator_sha256=sha(Path(__file__).read_bytes()),
                    source_inputs=plan['source_inputs'], immutable_existing=plan['immutable_existing'],
                    profile=inputs.profile_sha, raw_count=len(inputs.blobs),
                    raw_bytes=sum(map(len, inputs.blobs.values())),
                    raw_index_sha256=sha(index.encode()), steps_sha256=sha(steps.encode()),
                    rows=inputs.rows, checkpoints=inputs.checkpoints)
    need(len(inputs.rows) == 26 and len(inputs.checkpoints) == 6, 'exact inventory')
    need(manifest['raw_bytes'] <= plan['limits']['raw_corpus_bytes'], 'raw corpus cap')
    (HERE / 'raw').mkdir()
    for h, raw in sorted(inputs.blobs.items()):
        with (HERE / 'raw' / f'{h}.bin').open('xb') as output:
            output.write(raw)
    for name, raw in [('raw-index.tsv', index.encode()), ('steps.tsv', steps.encode()),
                      ('manifest.json', (json.dumps(manifest, indent=2) + '\n').encode())]:
        need(len(raw) <= plan['limits']['output_bytes'], ('metadata cap', name))
        with (HERE / name).open('xb') as output:
            output.write(raw)
    return dict(status='GENERATED_INPUT_NOT_PRODUCT_EXECUTION', rows=len(inputs.rows),
                checkpoints=len(inputs.checkpoints), raw_count=len(inputs.blobs),
                raw_bytes=manifest['raw_bytes'], manifest_sha256=sha((HERE / 'manifest.json').read_bytes()))


def build(plan):
    modules = {}
    for path, expected in plan['source_inputs'].items():
        role = 'native' if '/native/' in path else 'transition' if '/transition/' in path else 'system'
        modules[role] = load(path, expected, '_memory_gap_' + role)
    for path, expected in plan['immutable_existing'].items():
        need(sha((REPO / path).read_bytes()) == expected, ('old input pin', path))
    t = modules['transition']
    # Importing the native writer does not call main or its actor rewrite.
    # Transition.actor directly serializes the explicit epoch1/local argument.
    inputs = modules['native'].Inputs(plan, t, modules['system'])
    for scenario in ('MC03', 'MC04'):
        selected = plan['scenarios'][scenario]
        state, full = inputs.setup(scenario)
        inputs.checkpoint(scenario, state, full)
        p = t.plan(1, canceled=scenario == 'MC04')
        p['local'] = selected['actor']['local']
        p['q'] = selected['logical']['q']
        p['projection'] = t.sha(t.projection(p['object'], t.catalog_record(p)))
        need(p['generation'] == p['ug'] == 2 and p['sequence'] == 1
             and p['object'] == 6 and p['pair'] == (11, 12)
             and p['terminal'] == 0 and p['root'] == 1, 'first logical CREATE')
        for op, phase in selected['operations']:
            after, ordinal = t.successor(state, p, op, phase, state['last_lsn'] + 1)
            raw, roles = t.frame(state, after, p, op, ordinal)
            inputs.row(scenario, after['last_lsn'], 'semantic', op, raw,
                       {r: after['images'][r] for r in roles}, state, after, p,
                       f'{scenario}-{phase}')
            for role in roles:
                image = after['images'][role]
                full[int.from_bytes(image[32:40], 'little')] = image
            state = after
            if state['last_lsn'] in selected['checkpoints']:
                inputs.checkpoint(scenario, state, full)
        need(inputs.checkpoints[-1]['scalar'] == selected['final'],
             ('external final scalar', scenario, inputs.checkpoints[-1]['scalar']))
    return inputs


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--adopted-plan-sha256', required=True)
    args = parser.parse_args()
    need(args.adopted_plan_sha256 == ADOPTED, 'explicit root adoption')
    need(not any((HERE / p).exists() for p in ('raw', 'manifest.json', 'generation-result.json')),
         'write-once outputs already exist')
    started = time.monotonic()
    result = {'status': 'GENERATION_INCOMPLETE_NOT_PRODUCT_EXECUTION'}
    try:
        raw = (HERE / 'input-plan.json').read_bytes()
        need(sha(raw) == ADOPTED, 'adopted plan hash')
        plan = json.loads(raw)
        inputs = build(plan)
        need(time.monotonic() - started < plan['limits']['generation_seconds'], 'generation deadline')
        result = write(inputs, plan)
    except Exception as error:
        result = dict(status='GENERATION_FAILED_NOT_PRODUCT_EXECUTION', error=repr(error),
                      traceback=traceback.format_exc())
        raise
    finally:
        result.update(wall_seconds=time.monotonic() - started,
                      generator_sha256=sha(Path(__file__).read_bytes()))
        with (HERE / 'generation-result.json').open('x') as output:
            json.dump(result, output, indent=2)
            output.write('\n')
        print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
