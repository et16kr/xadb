#!/usr/bin/env python3
"""Write-once independent Profile3 MF14 inputs; no product code or output.

A copy of the pinned v2 memory-retire writer whose only change is the imported
modules: the transition module is serializer3.load() (Profile3 envelope, Job3
cells, Root3 with row_high 0) and the reused Inputs mechanics come from native3.
serializer3.row_job is never called: the history is Catalog scope2 only.
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
ADOPTED = '0a6efde6c8e0c6e39217765af970802dd7a0a184d93e2aa9f98d97dcbf3a8017'


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
                    source_inputs=plan['source_inputs'], serializer_parent=plan['serializer_parent'],
                    immutable_existing=plan['immutable_existing'],
                    profile=inputs.profile_sha, raw_count=len(inputs.blobs),
                    raw_bytes=sum(map(len, inputs.blobs.values())),
                    raw_index_sha256=sha(index.encode()), steps_sha256=sha(steps.encode()),
                    rows=inputs.rows, checkpoints=inputs.checkpoints)
    need(len(inputs.rows) == 16 and len(inputs.checkpoints) == 3, 'exact inventory')
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
        role = 'native' if '/native3/' in path else 'transition' if path.endswith('/serializer3.py') else 'system'
        modules[role] = load(path, expected, '_memory_retire3_' + role)
    for path, expected in {**plan['serializer_parent'], **plan['immutable_existing']}.items():
        need(sha((REPO / path).read_bytes()) == expected, ('old input pin', path))
    t = modules['transition'].load()
    need(t.profile()[4:6] == b'\x03\x00', 'Profile3 envelope')
    # Importing the native writer does not call main or its actor rewrite.
    # Transition.actor directly serializes the explicit epoch1/local argument.
    inputs = modules['native'].Inputs(plan, t, modules['system'])
    for scenario in ('MF14',):
        selected = plan['scenarios'][scenario]
        need(tuple(tuple(item) for item in selected['operations']) == t.ROLLBACK,
             'transition rollback operation order')
        need([(p['epoch'], p['lsns']) for p in selected['phases']] == [(1, [1, 9]), (2, [10, 16])]
             and selected['phases'][1]['selected_capture'] == 9, 'MF14 epoch schedule')
        state, full = inputs.setup(scenario)
        inputs.checkpoint(scenario, state, full)
        p = t.plan(1)
        p['local'] = selected['actor']['local']
        p['q'] = selected['logical']['q']
        p['outcome'] = selected['logical']['outcome']
        p['projection'] = t.sha(t.projection(p['object'], t.catalog_record(p)))
        need(p['generation'] == p['ug'] == 2 and p['sequence'] == 1
             and p['object'] == 6 and p['pair'] == (11, 12)
             and p['terminal'] == 0 and p['root'] == 1, 'first logical CREATE')
        need(p['local'] == 1 and p['inc'] == 2 and p['q'] == 0 and p['outcome'] == 'aborted'
             and p['terminal_gen'] == p['root_gen'] == 2, 'MF14 rollback plan')
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
        durable = [r for r in inputs.rows if r['scenario'] == scenario and r['epoch'] == 1]
        need(durable[-1]['lsn'] == selected['failure']['durable_last_lsn']
             and durable[-1]['wal_end'] == selected['failure']['durable_wal_end'], 'durable epoch1 end')
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
