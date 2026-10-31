#!/usr/bin/env python3
"""Independent Profile3 native CREATE expectations; never executes product code.

A copy of the pinned v2 native writer (dev3-catalog-create/native) whose only
change is the transition serializer: serializer3.load() replaces the version2
module (Profile3 envelope, Job3 free cells, Root3 with row_high 0 and the Job3
census aggregate). Only SHA-pinned standalone serializers are imported; their
output directories and main functions are never used. serializer3.row_job is
never called: these are Catalog-only histories. No native or Rust output is
available to this writer, and no base image is written.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys
import time
import traceback

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
ADOPTED_PLAN = 'f6e0dd554a91931cc8e048e775a666dacd777f0c1c3285abed6773ec15f21eeb'
PAGE = 16384


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def need(condition, message):
    if not condition:
        raise ValueError(message)


def load_input(path, expected, name):
    source = REPO / path
    need(sha(source.read_bytes()) == expected, ('serializer pin', path))
    spec = importlib.util.spec_from_file_location(name, source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def native_actor(local=1, generation=2):
    need((local, generation) in ((1, 2), (2, 3)), ('closed native actor', local, generation))
    epoch = 1 if local == 1 else 2
    return bytes(range(1, 17)) + struct.pack('<QI4xQQ', 1, epoch, 1, generation)


def outer_epoch(raw, epoch):
    raw = bytearray(raw)
    struct.pack_into('<I', raw, 48, epoch)
    raw[128:160] = hashlib.sha256(raw[:128]).digest()
    raw[-32:] = hashlib.sha256(raw[:-32]).digest()
    return bytes(raw)


class Inputs:
    def __init__(self, plan, transition, system):
        self.plan, self.t, self.system = plan, transition, system
        self.blobs, self.rows, self.checkpoints = {}, [], []
        self.profile = self.t.profile()
        self.profile_sha = self.blob(self.profile)
        self.zero = self.blob(bytes(PAGE))

    def blob(self, raw):
        if raw is None:
            return None
        raw = bytes(raw)
        need(len(raw) <= self.plan['limits']['raw_file_bytes'], ('raw file cap', len(raw)))
        digest = sha(raw)
        prior = self.blobs.setdefault(digest, raw)
        need(prior == raw, 'hash collision')
        return digest

    def images(self, images):
        return {str(role): self.blob(raw) for role, raw in sorted(images.items())}

    def epoch(self, scenario, lsn):
        matches = [p['epoch'] for p in self.plan['scenarios'][scenario]['phases']
                   if p['lsns'][0] <= lsn <= p['lsns'][1]]
        need(len(matches) == 1, ('exact epoch', scenario, lsn))
        return matches[0]

    def row(self, scenario, lsn, kind, operation, raw, changed, before, after,
            plan=None, pure_id=None, before_system=None, after_system=None):
        epoch = self.epoch(scenario, lsn)
        raw = outer_epoch(raw, epoch)
        prior = [r for r in self.rows if r['scenario'] == scenario and r['epoch'] == epoch]
        start = prior[-1]['wal_end'] if prior else 4096
        row = dict(id=f'{scenario}-L{lsn:02}', scenario=scenario, lsn=lsn, epoch=epoch,
                   kind=kind, operation=operation, frame=self.blob(raw), bytes=len(raw),
                   wal_start=start, wal_end=start + len(raw), pure_reference=pure_id,
                   plan={k: v.hex() if isinstance(v, bytes) else v for k, v in (plan or {}).items()},
                   before=self.images(before['images']), after=self.images(changed),
                   jobs_before=self.blob(b''.join(before['jobs'])),
                   jobs_after=self.blob(b''.join(after['jobs'])),
                   transaction_before=self.blob(before.get('tx')),
                   transaction_after=self.blob(after.get('tx')),
                   aging_before=self.blob(before.get('aging')),
                   aging_after=self.blob(after.get('aging')),
                   system_before=self.blob(before_system), system_after=self.blob(after_system),
                   allocated_pages=[n for n, e in sorted(after['entries'].items()) if e[0] == 2])
        hashes = {row['frame'], row['jobs_before'], row['jobs_after'],
                  row['transaction_before'], row['transaction_after'], row['aging_before'],
                  row['aging_after'], row['system_before'], row['system_after'],
                  *row['before'].values(), *row['after'].values()}
        need(sum(len(self.blobs[h]) for h in hashes if h) <= self.plan['limits']['transition_input_bytes'],
             ('transition byte cap', row['id']))
        self.rows.append(row)
        return row

    def setup(self, scenario):
        t = self.t
        before = t.fresh(self.profile)
        full = {int.from_bytes(raw[32:40], 'little'): raw for raw in before['images'].values()}
        full[7] = before['anchor']
        for index in range(1, 5):
            start = index * 15
            full[2 + index] = t.page(2 + index, 13, 4, 0, before['jobs'][start:start + 15])
        after, ordinal = t.successor(before, {}, 16, 'aging', 1)
        raw, roles = t.frame(before, after, {}, 16, ordinal)
        self.row(scenario, 1, 'semantic', 16, raw, {r: after['images'][r] for r in roles},
                 before, after, pure_id='G01')
        full[1] = after['images'][1]
        state = after
        allocator = state['images'][0]
        previous_system = None
        for number, work, birth, role in ((9, 1, 2, 'status'), (10, 2, 4, 'undo')):
            reserve_descriptor = None
            for terminal in ('reserve', role):
                raw, allocator_after, descriptor, payload = self.system.operation(
                    self.profile, allocator, number, 2, work, birth, terminal, 1,
                    old=reserve_descriptor or b'')
                lsn = birth + int(terminal != 'reserve')
                next_state = copy.deepcopy(state)
                next_state['images'][0] = allocator_after
                next_state['last_lsn'] = lsn
                changed = {0: allocator_after}
                if terminal == 'reserve':
                    next_state['entries'][number] = (1, 2, 0, 0, self.system.work(work))
                    reserve_descriptor = descriptor
                else:
                    image_role = 4 if role == 'status' else 5
                    next_state['images'][image_role] = payload
                    next_state['entries'][number] = (2, 2, 9 if role == 'status' else 8,
                                                     2 if role == 'status' else 3, b'')
                    changed[image_role] = payload
                    full[number] = payload
                self.row(scenario, lsn, 'system', 8 if terminal == 'reserve' else 10,
                         raw, changed, state, next_state,
                         before_system=previous_system, after_system=descriptor)
                full[0] = allocator_after
                state, allocator, previous_system = next_state, allocator_after, descriptor
        pure_setup = t.setup(after)
        need(state['images'] == pure_setup['images'], 'system outputs vs independent transition setup')
        state['system_descriptor'] = previous_system
        return state, full

    def checkpoint(self, scenario, state, full):
        t = self.t
        root, _ = t.cell_at(state['images'][1], 0)
        current = {str(n): self.blob(full[n]) if state['entries'].get(n, (0,))[0] == 2 else self.zero
                   for n in range(64)}
        scalar = {name: state[key] for name, key in (
            ('lsn', 'last_lsn'), ('revision', 'revision'), ('issued', 'issued'),
            ('completed', 'completed'), ('canceled', 'canceled'), ('cursor', 'cursor'),
            ('undo_high', 'uhigh'), ('object_high', 'ohigh'), ('C', 'commit'),
            ('aging_consumed', 'aging_cost'))}
        scalar['Q'] = scalar['C']
        scalar['aging_groups'] = state['aging_chain'][3]
        scalar['aging_remaining'] = t.get(state['aging'], 640 + 3 * 128 + 16)
        record = dict(scenario=scenario, lsn=state['last_lsn'], scalar=scalar,
                      pages=current, jobs=self.blob(b''.join(state['jobs'])),
                      aging=self.blob(state['aging']), system=self.blob(state['system_descriptor']),
                      active_transaction=self.blob(state.get('tx'))
                      if state.get('status_phase') in ('active', 'aborting') else None,
                      root_masks=[t.get(root, at) for at in (96, 104, 112)],
                      live_tables=sorted(state['tables']))
        self.checkpoints.append(record)
        return record

    def scenario(self, scenario):
        t = self.t
        state, full = self.setup(scenario)
        cycles = 2 if scenario == 'N01' else 1
        phases = t.COMMIT if scenario == 'N01' else t.ROLLBACK
        counter = 0
        for cycle in range(1, cycles + 1):
            state = copy.deepcopy(state)
            state['tx'] = None
            p = t.plan(cycle)
            if scenario == 'N02':
                p['outcome'] = 'aborted'
            for operation, phase in phases:
                after, ordinal = t.successor(state, p, operation, phase, state['last_lsn'] + 1)
                raw, roles = t.frame(state, after, p, operation, ordinal)
                counter += 1
                self.row(scenario, after['last_lsn'], 'semantic', operation, raw,
                         {r: after['images'][r] for r in roles}, state, after, p,
                         f'{"C" if scenario == "N01" else "R"}{counter:02}')
                for role in roles:
                    image = after['images'][role]
                    full[int.from_bytes(image[32:40], 'little')] = image
                state = after
                if state['last_lsn'] in (8, 9, 10, 15, 16, 19, 25):
                    self.checkpoint(scenario, state, full)
        final = next(c for c in reversed(self.checkpoints) if c['scenario'] == scenario)
        need(final['scalar'] == self.plan['scenarios'][scenario]['final'],
             ('external final scalar', scenario, final['scalar']))

    def build(self):
        self.scenario('N01')
        self.scenario('N02')
        for row in list(self.rows):
            if row['scenario'] == 'N01' and row['lsn'] <= 15:
                alias = copy.deepcopy(row)
                alias.update(id=f'N03-L{row["lsn"]:02}', scenario='N03', source_row=row['id'])
                self.rows.append(alias)
        for check in list(self.checkpoints):
            if check['scenario'] == 'N01' and check['lsn'] <= 15:
                alias = copy.deepcopy(check)
                alias.update(scenario='N03', source_checkpoint=f'N01-L{check["lsn"]:02}')
                self.checkpoints.append(alias)
        final = next(c for c in reversed(self.checkpoints) if c['scenario'] == 'N03')
        need(final['scalar'] == self.plan['scenarios']['N03']['final'], 'N03 external final')
        need(len(self.rows) == 56, ('fixed step inventory', len(self.rows)))
        need(sum(map(len, self.blobs.values())) <= self.plan['limits']['raw_corpus_bytes'], 'corpus cap')

    def write(self):
        index = 'sha256\tbytes\tfile\n' + ''.join(
            f'{h}\t{len(raw)}\traw/{h}.bin\n' for h, raw in sorted(self.blobs.items()))
        fields = ['id', 'scenario', 'lsn', 'epoch', 'kind', 'operation', 'bytes',
                  'wal_start', 'wal_end', 'frame', 'pure_reference', 'jobs_before', 'jobs_after',
                  'transaction_before', 'transaction_after', 'aging_before', 'aging_after',
                  'system_before', 'system_after', 'before', 'after']
        tsv = '\t'.join(fields) + '\n'
        for row in self.rows:
            values = []
            for field in fields:
                v = row[field]
                if isinstance(v, dict):
                    v = ','.join(f'{r}:{h}' for r, h in sorted(v.items(), key=lambda item: int(item[0])))
                values.append('-' if v is None else str(v))
            tsv += '\t'.join(values) + '\n'
        manifest = dict(generator=self.plan['id'], authority='NONE',
                        status='GENERATED_INPUT_NOT_PRODUCT_EXECUTION', plan_sha256=ADOPTED_PLAN,
                        generator_sha256=sha(Path(__file__).read_bytes()), source_inputs=self.plan['source_inputs'],
                        serializer_parent=self.plan['serializer_parent'],
                        profile=self.profile_sha, raw_count=len(self.blobs), raw_bytes=sum(map(len, self.blobs.values())),
                        raw_index_sha256=sha(index.encode()), steps_sha256=sha(tsv.encode()),
                        rows=self.rows, checkpoints=self.checkpoints)
        (HERE / 'raw').mkdir()
        for h, raw in sorted(self.blobs.items()):
            (HERE / 'raw' / f'{h}.bin').write_bytes(raw)
        for name, raw in [('raw-index.tsv', index.encode()), ('steps.tsv', tsv.encode()),
                          ('manifest.json', (json.dumps(manifest, indent=2) + '\n').encode())]:
            with (HERE / name).open('xb') as f:
                f.write(raw)
        return dict(status='GENERATED_INPUT_NOT_PRODUCT_EXECUTION', rows=len(self.rows),
                    raw_count=len(self.blobs), raw_bytes=manifest['raw_bytes'],
                    manifest_sha256=sha((HERE / 'manifest.json').read_bytes()))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--adopted-plan-sha256', required=True)
    args = parser.parse_args()
    need(args.adopted_plan_sha256 == ADOPTED_PLAN, 'explicit adopted plan')
    need(not any((HERE / p).exists() for p in ('raw', 'manifest.json', 'generation-result.json')),
         'write-once output already exists')
    started = time.monotonic()
    result = {'status': 'GENERATION_INCOMPLETE_NOT_PRODUCT_EXECUTION'}
    try:
        raw = (HERE / 'input-plan.json').read_bytes()
        need(sha(raw) == ADOPTED_PLAN, 'plan hash')
        plan = json.loads(raw)
        sources = plan['source_inputs']
        for path, expected in {**plan['serializer_parent'], **plan['immutable_existing']}.items():
            need(sha((REPO / path).read_bytes()) == expected, ('pinned existing input', path))
        serializer = next(p for p in sources if p.endswith('/serializer3.py'))
        system = next(p for p in sources if 'dev3-plan4-free-undo/' in p)
        t = load_input(serializer, sources[serializer], '_native3_serializer3').load()
        s = load_input(system, sources[system], '_native3_system_input')
        need(t.profile()[4:6] == b'\x03\x00', 'Profile3 envelope')
        t.actor = native_actor
        inputs = Inputs(plan, t, s)
        inputs.build()
        need(time.monotonic() - started < plan['limits']['generation_seconds'], 'generation deadline')
        result = inputs.write()
    except Exception as error:
        result = dict(status='GENERATION_FAILED_NOT_PRODUCT_EXECUTION', error=repr(error),
                      traceback=traceback.format_exc())
        raise
    finally:
        result.update(wall_seconds=time.monotonic() - started, generator_sha256=sha(Path(__file__).read_bytes()))
        with (HERE / 'generation-result.json').open('x') as f:
            json.dump(result, f, indent=2); f.write('\n')
        print(json.dumps(result))


if __name__ == '__main__':
    main()
