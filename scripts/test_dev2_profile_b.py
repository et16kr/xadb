"""Profile B supervisor and judge contract tests; fixtures are never measurement evidence."""
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import dev2_profile_b as b

HERE = Path(__file__).resolve().parent
PERF = HERE / 'fixtures/profile_b_perf.py'
RULES, _ = b.load_profile()
B1, B2 = RULES['b1_pre_pair_idle'], RULES['b2_slot']
# Literal order produced by the controller's own prepare-performance for seed 20261018.
CONTROLLER_ORDERS_20261018 = ['BA', 'BA', 'AB', 'AB', 'BA', 'BA', 'BA', 'AB', 'AB', 'AB']


def ticks(**fields):
    names = ('user', 'nice', 'system', 'idle', 'iowait', 'irq', 'softirq', 'steal')
    return [fields.get(name, 0) for name in names]


def full_pmu(cycles2=1000, cycles18=10, ref2=500, ref18=10, coverage=100.0):
    rows = {}
    for key, count in (('cpu2/cycles', cycles2), ('cpu18/cycles', cycles18),
                       ('cpu2/ref-cycles', ref2), ('cpu18/ref-cycles', ref18)):
        full = coverage == 100.0
        rows[key] = {'count': count, 'runtime_ns': 1, 'coverage_percent': coverage,
                     'status': 'REPORTED_FULL_COVERAGE' if full else 'PARTIAL_OR_EMPTY'}
    return {'rows': rows, 'all_rows_full_coverage': coverage == 100.0}


def slot(ordinal, side, ratio=Fraction(2), status='VALID'):
    return {'ordinal': ordinal, 'side': side, 'status': status,
            'pmu': {'status': 'VALID', 'cpu2_cycles_per_ref': b.packed(ratio)}}


def plan_slots(ratios=None):
    sides = ['baseline', 'candidate'] * 10
    return [slot(i, sides[i], (ratios or {}).get(i, Fraction(2))) for i in range(20)]


class JudgeVectors(unittest.TestCase):
    def test_device_identity_uses_uuid_and_serial_not_kernel_name(self):
        rules = RULES['device']
        mount = {'filesystems': [{'source': '/dev/nvme9n1p2', 'fstype': 'ext4', 'uuid': rules['filesystem_uuid'],
                                  'maj:min': '259:8'}]}
        with tempfile.TemporaryDirectory() as root:
            block = Path(root) / 'nvme9n1'
            (block / 'nvme9n1p2').mkdir(parents=True)
            (block / 'nvme9n1p2/partition').write_text('2')

            def resolve(path):
                return block / 'nvme9n1p2' if str(path).endswith('259:8') else path

            def read(path):
                return {'device/serial': rules['serial'] + '\n'}[str(path).split('nvme9n1/', 1)[1]]
            with patch.object(b.subprocess, 'check_output', return_value=json.dumps(mount)), \
                    patch.object(b.Path, 'resolve', resolve), patch.object(b.Path, 'read_text', read):
                self.assertEqual(b.device_identity('/x', rules)['disk'], 'nvme9n1')
                mount['filesystems'][0]['uuid'] = '0'
                with patch.object(b.subprocess, 'check_output', return_value=json.dumps(mount)):
                    with self.assertRaises(b.ProfileError):
                        b.device_identity('/x', rules)

    def test_tick_rule_counts_only_profile_fields_at_exact_bounds(self):
        before = {2: ticks(), 18: ticks()}
        after = {2: ticks(user=50, idle=10, iowait=3), 18: ticks(irq=1, idle=40, iowait=9)}
        self.assertEqual(b.tick_rule(before, after, B2)['status'], 'VALID')
        after[18] = ticks(softirq=1, steal=1, idle=40)
        self.assertEqual(b.tick_rule(before, after, B2)['cpu18_nonidle_ticks'], 2)
        self.assertEqual(b.tick_rule(before, after, B2)['status'], 'INVALID')
        for field in ('nice', 'steal'):
            after = {2: ticks(**{field: 1}), 18: ticks()}
            self.assertEqual(b.tick_rule(before, after, B2)['status'], 'INVALID', field)
        with self.assertRaises(b.ProfileError):
            b.tick_rule({2: ticks(user=2), 18: ticks()}, {2: ticks(user=1), 18: ticks()}, B2)

    def window(self, measured=0, other=0, io=0, seconds=10, cpus=4):
        before = {'monotonic_ns': 0, 'cpus': {n: ticks() for n in (2, 18, *range(cpus))}, 'io_ticks': 0}
        after = {'monotonic_ns': seconds * 10**9, 'io_ticks': io * 100, 'cpus': {}}
        for n in before['cpus']:
            busy = measured if n in (2, 18) else other
            after['cpus'][n] = ticks(user=busy, idle=100 - busy)
        return b.idle_assessment(before, after, B1)

    def test_idle_assessment_exact_profile_boundaries(self):
        self.assertTrue(self.window(measured=5, other=10, io=5)['passed'])
        self.assertFalse(self.window(measured=6)['passed'])
        self.assertFalse(self.window(other=11)['passed'])
        self.assertFalse(self.window(io=6)['passed'])
        with self.assertRaises(b.ProfileError):
            self.window(seconds=9)

    def test_pmu_quality_and_sibling_share_boundaries(self):
        self.assertEqual(b.pmu_rule(None, B2)['status'], 'UNAVAILABLE')
        self.assertEqual(b.pmu_rule(full_pmu(cycles2=1000, cycles18=50), B2)['status'], 'VALID')
        self.assertEqual(b.pmu_rule(full_pmu(cycles2=1000, cycles18=51), B2)['status'], 'INVALID')
        self.assertEqual(b.pmu_rule(full_pmu(coverage=99.5), B2)['status'], 'UNKNOWN')
        self.assertEqual(b.pmu_rule(full_pmu(ref2=0), B2)['status'], 'UNKNOWN')
        self.assertEqual(b.pmu_rule(full_pmu(cycles2=900, ref2=450), B2)['cpu2_cycles_per_ref'], [2, 1])

    def test_frequency_tolerance_and_covariate_interval_are_exact(self):
        median = Fraction(2)
        verdict = b.environment_verdict(plan_slots({0: median * Fraction(103, 100)}), B2,
                                        pmu_enabled=True, abort=None, termination=None)
        self.assertEqual(verdict['environment_validity'], 'VALID')
        verdict = b.environment_verdict(plan_slots({0: median * Fraction(10301, 10000)}), B2,
                                        pmu_enabled=True, abort=None, termination=None)
        self.assertEqual(verdict['environment_validity'], 'INVALID')
        self.assertEqual(verdict['side_non_valid_slots'], {'baseline': 1, 'candidate': 0})
        # Baseline/candidate paired median: both interval ends are inside, values beyond are UNKNOWN.
        for factor, expected in ((Fraction(99, 100), 'VALID'), (Fraction(9899, 10000), 'UNKNOWN'),
                                 (Fraction(101, 100), 'VALID'), (Fraction(10101, 10000), 'UNKNOWN')):
            ratios = {i: Fraction(2) * factor for i in range(0, 20, 2)}
            verdict = b.environment_verdict(plan_slots(ratios), B2, pmu_enabled=True, abort=None,
                                            termination=None)
            self.assertEqual(verdict['covariate']['status'], expected)
            self.assertEqual(verdict['environment_validity'], expected)

    def test_campaign_validity_from_slots_abort_and_termination(self):
        slots = plan_slots()
        slots[7]['status'] = 'INVALID'
        verdict = b.environment_verdict(slots, B2, pmu_enabled=False, abort=None, termination=None)
        self.assertEqual(verdict['environment_validity'], 'INVALID')
        self.assertEqual(verdict['covariate']['status'], 'UNAVAILABLE')
        verdict = b.environment_verdict(plan_slots()[:3], B2, pmu_enabled=False, abort=None, termination=None)
        self.assertEqual(verdict['environment_validity'], 'UNKNOWN')
        verdict = b.environment_verdict([], B2, pmu_enabled=True, abort='IDLE_WAIT_EXCEEDED', termination=None)
        self.assertEqual(verdict['environment_validity'], 'INVALID')
        verdict = b.environment_verdict(plan_slots(), B2, pmu_enabled=True, abort=None, termination='x')
        self.assertEqual(verdict['environment_validity'], 'UNKNOWN')

    def test_slot_env_profile_is_fixed_and_rows_are_literal(self):
        command = b.slot_env_command('/usr/bin/perf', 'c', 'a')
        b.validate_slot_env_command(command)
        for bad in (['-C', '2,18,3'], ['-e', 'cycles,instructions']):
            changed = list(command)
            changed[changed.index(bad[0]) + 1] = bad[1]
            with self.assertRaises(b.ProfileError):
                b.validate_slot_env_command(changed)
        for extra in ('-a', '--tid'):
            with self.assertRaises(b.ProfileError):
                b.validate_slot_env_command(command + [extra])
        raw = ('CPU2;623093;;cycles;101376702;100.00;;\nCPU18;540673;;cycles;101376982;100.00;;\n'
               'CPU2;636450;;ref-cycles;101372894;100.00;;\nCPU18;439125;;ref-cycles;101376972;100.00;;\n')
        parsed = b.parse_slot_env(raw)
        self.assertTrue(parsed['all_rows_full_coverage'])
        self.assertEqual(parsed['rows']['cpu18/cycles']['count'], 540673)
        self.assertFalse(b.parse_slot_env(raw.replace('100.00', '87.50', 1))['all_rows_full_coverage'])
        self.assertEqual(b.parse_slot_env(raw.splitlines()[0])['rows']['cpu2/ref-cycles']['status'], 'MISSING')
        for bad in (raw + raw.splitlines()[0], raw.replace('CPU18', 'CPU3', 1),
                    raw.replace('ref-cycles', 'instructions', 1), raw.replace('623093', '6.2e5', 1), ''):
            with self.assertRaises(b.ProfileError):
                b.parse_slot_env(bad)

    def test_paired_order_port_matches_controller_vector(self):
        self.assertEqual(b.paired_orders(20261018), CONTROLLER_ORDERS_20261018)
        pairs = b.pairs(bytes(range(16)), 2026102571)
        identifiers = [bytes(p[side]) for p in pairs for side in ('baseline_run', 'candidate_run')]
        self.assertEqual(len(set(identifiers)), 20)
        self.assertEqual(identifiers[3], hashlib.sha256(bytes(range(16)) + bytes([3])).digest()[:16])

    def test_preflight_reapplies_tick_rule_without_measurement(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = []
            for index, sibling in enumerate((1, 2)):
                path = Path(directory) / f'{index}.json'
                path.write_text(json.dumps({'before': {'cpus': {'cpu2': ticks(), 'cpu18': ticks()}},
                                            'after': {'cpus': {'cpu2': ticks(user=9), 'cpu18': ticks(user=sibling)}}}))
                paths.append(path)
            result = b.preflight(paths)
            self.assertEqual((result['total'], result['valid']), (2, 1))


def facts(**overrides):
    value = {'go_sent': True, 'pre_go_failure': None, 'abort': None, 'termination': None, 'slots': 20,
             'acknowledged': True, 'environment_validity': 'VALID', 'product_defects': [], 'terminated_runs': [],
             'comparison': {'validity': 'VALID', 'correctness': 'PASS', 'guardrails': 'PASS'}}
    value.update(overrides)
    return value


class TerminationTable(unittest.TestCase):
    def check(self, kind, expected, **overrides):
        result = b.classify(kind, facts(**overrides))
        self.assertEqual((result['status'], result['reason'], result['replaceable']), expected, overrides)

    def test_acceptance_rows(self):
        self.check('acceptance', ('NOT_RUN', 'recipe rejected', True), go_sent=False, comparison=None,
                   pre_go_failure='recipe rejected')
        self.check('acceptance', ('NOT_RUN', 'slot-env-v1 start failed', True), go_sent=False,
                   pre_go_failure='slot-env-v1 start failed', abort='COLLECTOR_FAILURE')
        self.check('acceptance', ('ENVIRONMENT_INVALID', 'IDLE_WAIT_EXCEEDED', True), go_sent=False,
                   abort='IDLE_WAIT_EXCEEDED', comparison=None)
        self.check('acceptance', ('ENVIRONMENT_INVALID', 'CAMPAIGN_CAP', True), abort='CAMPAIGN_CAP')
        self.check('acceptance', ('ENVIRONMENT_INVALID', 'ENVIRONMENT_INVALID', True), environment_validity='INVALID')
        self.check('acceptance', ('ENVIRONMENT_INVALID', 'ENVIRONMENT_UNKNOWN', True), environment_validity='UNKNOWN')
        self.check('acceptance', ('INCOMPLETE', 'TERMINATED', False), abort='COLLECTOR_FAILURE',
                   termination='slot-env-v1 failed at slot 4')
        self.check('acceptance', ('INCOMPLETE', 'TERMINATED', False),
                   terminated_runs=[{'side': 'baseline', 'reason': 'Configuration: worker nice 1 is not 0'}])
        self.check('acceptance', ('FAIL', 'PRODUCT_DEFECT', False), environment_validity='INVALID',
                   product_defects=[{'side': 'candidate', 'reason': 'worker panic'}], termination='x')
        # Correctness FAIL is final regardless of environment or termination.
        self.check('acceptance', ('FAIL', 'CORRECTNESS_FAIL', False), environment_validity='INVALID',
                   termination='x', comparison={'validity': 'INVALID', 'correctness': 'FAIL', 'guardrails': 'FAIL'})
        self.check('acceptance', ('INCOMPLETE', 'INCONCLUSIVE_COMPARISON', False),
                   comparison={'validity': 'INVALID', 'correctness': 'PASS', 'guardrails': 'PASS'})
        self.check('acceptance', ('INCOMPLETE', 'INCONCLUSIVE_COMPARISON', False),
                   comparison={'validity': 'VALID', 'correctness': 'PASS', 'guardrails': 'UNKNOWN'})
        self.check('acceptance', ('FAIL', 'GUARDRAIL_FAIL', False),
                   comparison={'validity': 'VALID', 'correctness': 'PASS', 'guardrails': 'FAIL'})
        self.check('acceptance', ('PASS', 'GUARDRAIL_PASS', False))

    def test_c3_never_replaces_and_ignores_environment(self):
        self.check('c3', ('COMPLETED', 'CONTROLLER_VERDICT', False), environment_validity='INVALID')
        self.check('c3', ('INCOMPLETE', 'TERMINATED', False), abort='IDLE_WAIT_EXCEEDED', slots=6)
        self.check('c3', ('INCOMPLETE', 'NOT_RUN', False), go_sent=False, abort='COLLECTOR_FAILURE')
        self.check('c3', ('REJECTED', 'CANDIDATE_PRODUCT_DEFECT', False),
                   product_defects=[{'side': 'candidate', 'reason': 'worker Output.error'}])
        self.check('c3', ('INCOMPLETE', 'BASELINE_FAILURE', False),
                   product_defects=[{'side': 'baseline', 'reason': 'worker panic'}],
                   comparison={'validity': 'INVALID', 'correctness': 'FAIL', 'guardrails': 'FAIL'})
        self.check('c3', ('REJECTED', 'CANDIDATE_PRODUCT_DEFECT', False),
                   comparison={'validity': 'VALID', 'correctness': 'FAIL', 'guardrails': 'PASS'})
        self.check('c3', ('INCOMPLETE', 'TERMINATED', False), terminated_runs=[{'side': 'baseline'}])

    def test_connection_is_only_a_tool_result(self):
        self.check('connection', ('TOOL_CONNECTED', 'CONNECTION_ONLY_NOT_A_PERFORMANCE_VERDICT', False),
                   abort='CONNECTION_STOP', slots=2)
        self.check('connection', ('TOOL_FAILED', 'CONNECTION_ONLY_NOT_A_PERFORMANCE_VERDICT', False),
                   abort='CONNECTION_STOP', acknowledged=False)


class LedgerAndDecision(unittest.TestCase):
    def finish(self, ledger, role, status, reason='R', replaceable=False, seconds=10):
        return b.settle(ledger, role, {'status': status, 'reason': reason, 'replaceable': replaceable}, seconds, {})

    def test_replacements_seeds_finality_and_budget(self):
        ledger = b.new_ledger('p')
        seeds = []
        for _ in range(3):
            seeds.append(b.admit(ledger, 'always-control')['order_seed'])
            self.finish(ledger, 'always-control', 'ENVIRONMENT_INVALID', replaceable=True)
        self.assertEqual(seeds, [2026102571, 2027102571, 2028102571])
        self.assertEqual(ledger['roles']['always-control']['final'], {'status': 'INCOMPLETE', 'reason': 'ENVIRONMENT'})
        with self.assertRaises(b.ProfileError):
            b.admit(ledger, 'always-control')
        with self.assertRaises(b.ProfileError):
            b.admit(ledger, 'always-audit')
        b.admit(ledger, 'detailed-control')
        self.finish(ledger, 'detailed-control', 'PASS')
        with self.assertRaisesRegex(b.ProfileError, 'final'):
            b.admit(ledger, 'detailed-control')
        with self.assertRaisesRegex(b.ProfileError, 'detailed-audit'):
            b.admit(ledger, 'detailed-replay')
        self.assertEqual(b.admit(ledger, 'detailed-audit')['order_seed'], 2026102574)
        ledger['used_seconds'] += b.BUDGET_SECONDS
        with self.assertRaisesRegex(b.ProfileError, 'total differs'):
            b.admit(ledger, 'detailed-audit')
        ledger['carried_seconds'] = b.BUDGET_SECONDS
        with self.assertRaisesRegex(b.ProfileError, 'BUDGET'):
            b.admit(ledger, 'detailed-audit')

    def test_c3_order_single_attempts_and_connection_limit(self):
        ledger = b.new_ledger('p')
        with self.assertRaisesRegex(b.ProfileError, 'c3-exploration COMPLETED'):
            b.admit(ledger, 'c3-confirmation')
        self.assertEqual(b.admit(ledger, 'c3-exploration')['order_seed'], 2026102561)
        self.finish(ledger, 'c3-exploration', 'INCOMPLETE', 'TERMINATED')
        with self.assertRaises(b.ProfileError):
            b.admit(ledger, 'c3-exploration')
        with self.assertRaisesRegex(b.ProfileError, 'c3-exploration COMPLETED'):
            b.admit(ledger, 'c3-confirmation')
        ledger = b.new_ledger('p')
        self.finish(ledger, 'c3-exploration', 'COMPLETED', 'CONTROLLER_VERDICT')
        self.assertEqual(b.admit(ledger, 'c3-confirmation')['order_seed'], 2026102562)
        with self.assertRaisesRegex(b.ProfileError, 'c3-confirmation COMPLETED'):
            b.admit(ledger, 'c3-guard-off')
        self.finish(ledger, 'c3-confirmation', 'COMPLETED', 'CONTROLLER_VERDICT')
        for role, seed in (('c3-guard-off', 2026102563), ('c3-guard-always', 2026102564)):
            self.assertEqual(b.admit(ledger, role)['order_seed'], seed)
        b.admit(ledger, 'connection')
        self.assertIsNone(self.finish(ledger, 'connection', 'TOOL_FAILED', seconds=100))
        with self.assertRaises(b.ProfileError):
            b.admit(ledger, 'connection')
        self.assertEqual(ledger['used_seconds'], 120)

    def test_budget_carries_only_from_closed_bound_predecessors(self):
        with tempfile.TemporaryDirectory() as root:
            first = Path(root) / 'first.json'
            ledger = b.new_ledger('p')
            ledger['used_seconds'] = ledger['carried_seconds'] = 7000
            first.write_text(json.dumps(ledger))
            with self.assertRaisesRegex(b.ProfileError, 'closed'):
                b.predecessor(first, b.digest(first))
            ledger['closed'] = True
            first.write_text(json.dumps(ledger))
            with self.assertRaisesRegex(b.ProfileError, 'identity'):
                b.predecessor(first, '0' * 64)
            with self.assertRaisesRegex(b.ProfileError, 'closed'):
                b.admit(ledger, 'always-control')
            with self.assertRaisesRegex(b.ProfileError, 'closed'):
                self.finish(ledger, 'always-control', 'PASS')
            carried = b.new_ledger('q', [b.predecessor(first, b.digest(first))])
            self.assertEqual((carried['used_seconds'], carried['carried_seconds']), (7000, 7000))
            b.admit(carried, 'always-control')
            self.finish(carried, 'always-control', 'ENVIRONMENT_INVALID', replaceable=True, seconds=3800)
            with self.assertRaisesRegex(b.ProfileError, 'BUDGET'):
                b.admit(carried, 'always-control')
            with patch.object(b, 'device_identity', return_value={'disk': 'fixture'}):
                for kwargs in ({}, {'new_budget': True, 'budget_from': [(first, b.digest(first))]}):
                    with self.assertRaisesRegex(b.ProfileError, 'new-budget'):
                        b.initialize(Path(root) / 'session', first, first, first, first, 'unavailable', **kwargs)

    def test_c3_recipe_kind_mode_and_two_builds(self):
        recipe = {'kind': 'exploration', 'baseline': {'mode': 'detailed', 'worker_sha256': 'a'},
                  'candidate': {'mode': 'detailed', 'worker_sha256': 'b'}}
        b.check_c3_recipe('c3-exploration', recipe)
        for role, change in (('c3-confirmation', {}), ('c3-guard-off', {}),
                             ('c3-exploration', {'candidate': {'mode': 'always', 'worker_sha256': 'b'}}),
                             ('c3-exploration', {'candidate': {'mode': 'detailed', 'worker_sha256': 'a'}})):
            with self.assertRaises(b.ProfileError, msg=(role, change)):
                b.check_c3_recipe(role, recipe | change)

    def decide(self, **finals):
        ledger = b.new_ledger('p')
        for role, (status, reason) in finals.items():
            ledger['roles'][role.replace('_', '-')]['final'] = {'status': status, 'reason': reason}
        return b.pf04_decision(ledger)

    def test_pf04_table(self):
        passed = ('PASS', 'GUARDRAIL_PASS')
        both = {f'{m}_{p}': passed for m in ('always', 'detailed') for p in ('control', 'audit', 'replay')}
        self.assertEqual(self.decide(**both)['pf04_k0'], 'PASS')
        result = self.decide(**(both | {'always_control': ('FAIL', 'GUARDRAIL_FAIL')}))
        self.assertEqual(result['modes']['always'], {'decision': 'INCOMPLETE', 'reason': 'CONTROL_GUARDRAIL_FAIL'})
        self.assertEqual(result['pf04_k0'], 'INCOMPLETE')
        result = self.decide(**(both | {'always_control': ('FAIL', 'CORRECTNESS_FAIL')}))
        self.assertEqual(result['modes']['always']['decision'], 'FAIL')
        result = self.decide(**(both | {'detailed_audit': ('FAIL', 'GUARDRAIL_FAIL'),
                                        'always_replay': ('INCOMPLETE', 'TERMINATED')}))
        self.assertEqual(result['pf04_k0'], 'FAIL')
        self.assertEqual(result['modes']['always'], {'decision': 'INCOMPLETE', 'reason': 'REPLAY_TERMINATED'})
        result = self.decide(**(both | {'detailed_replay': ('FAIL', 'GUARDRAIL_FAIL')}))
        self.assertEqual(result['modes']['detailed'], {'decision': 'FAIL', 'reason': 'REPLAY_GUARDRAIL_FAIL'})
        self.assertEqual(self.decide()['modes']['always']['reason'], 'CONTROL_NOT_RUN')


def frame(observation):
    body = json.dumps({'version': 1, 'run': [1] * 16, 'request': 1, 'actor': 0, 'observation': observation})
    return len(body).to_bytes(4, 'big') + body.encode()


class NativeEvidence(unittest.TestCase):
    def test_product_defects_versus_tool_terminations(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            runs = []
            cases = {
                'ok': (frame({'result': 'ready'}) + frame({'result': 'performance', 'output': {'error': None}}), b'', None),
                'panic': (b'', b"thread 'main' panicked at src/x.rs", 'Protocol: worker exit was not successful'),
                'error-response': (frame({'result': 'product_error', 'code': 'X'}), b'', None),
                'output-error': (frame({'result': 'performance', 'output': {'error': 'boom'}}), b'', None),
                'signalled': (frame({'result': 'ready'}), b'', 'Protocol: worker exit was not successful'),
                'nice': (b'', b'', 'Configuration: worker nice 1 is not 0'),
            }
            for ordinal, (name, (stdout, stderr, failure)) in enumerate(cases.items()):
                (root / name).mkdir()
                (root / name / 'stdout.bin').write_bytes(stdout)
                (root / name / 'stderr.bin').write_bytes(stderr)
                runs.append({'ordinal': ordinal, 'candidate': ordinal % 2 == 1, 'directory': name, 'failure': failure})
            runs.append({'ordinal': 9, 'candidate': False, 'directory': None, 'failure': 'NOT_RUN: slot gate ended'})
            result = b.native_facts(root, {'runs': runs})
            self.assertEqual([d['reason'] for d in result['product_defects']],
                             ['worker panic', "worker error response ['product_error']", 'worker Output.error'])
            self.assertEqual([t['ordinal'] for t in result['terminated_runs']], [4, 5])


class FakeHost:
    """Scripted /proc counters and clock; every sleep is an idle window unless marked busy."""
    def __init__(self):
        self.clock = 0
        self.counters = {n: ticks() for n in range(8)} | {18: ticks()}
        self.io = 0
        self.busy_windows = 0
        self.lock = threading.Lock()

    def now_ns(self):
        return self.clock

    def sleep(self, seconds):
        if getattr(self, 'before_sleep', None):
            self.before_sleep()
        with self.lock:
            self.clock += int(seconds * 10**9)
            for number, values in self.counters.items():
                values[3] += int(100 * seconds)
                if self.busy_windows and number == 2:
                    values[0] += int(100 * seconds)
            self.busy_windows = max(0, self.busy_windows - 1)

    def stat(self):
        with self.lock:
            return {n: list(v) for n, v in self.counters.items()}

    def io_ticks(self):
        return self.io

    def tasks(self):
        return [{'pid': 1, 'tid': 1, 'processor': 2, 'comm': 'fixture'}]


class FakeController(threading.Thread):
    """Client side of the gate as the controller performs it; `during` runs between GO and DONE."""
    def __init__(self, directory, runs, during=None, script=None):
        super().__init__(daemon=True)
        self.request = os.open(directory / b.REQUEST_FIFO, os.O_WRONLY | os.O_NONBLOCK)
        self.reply = os.open(directory / b.REPLY_FIFO, os.O_RDONLY | os.O_NONBLOCK)
        self.runs, self.during, self.script = runs, during or (lambda k: None), script or {}
        self.received, self.verdict, self.error = [], None, None

    def poll(self):
        return None if self.is_alive() else 0

    def line(self):
        end, data = time.monotonic() + 3, b''
        while not data.endswith(b'\n'):
            try:
                data += os.read(self.reply, 128)
            except BlockingIOError:
                time.sleep(.001)
            if time.monotonic() > end:
                raise TimeoutError('fake controller reply deadline')
        self.received.append(data.decode().rstrip('\n'))
        return self.received[-1]

    def run(self):
        try:
            for k, run in enumerate(self.runs):
                action = self.script.get(k)
                if action == 'exit':
                    return
                # One write keeps a duplicated line atomic, so the supervisor sees both together.
                os.write(self.request, f'READY {k} {run}\n'.encode() * (2 if action == 'duplicate-ready' else 1))
                if action == 'exit-after-ready':
                    return
                reply = self.line()
                if reply.startswith('ABORT'):
                    os.write(self.request, f'DONE {k} {run}\n'.encode())
                    self.verdict = self.line().split()[-1]
                    return
                self.during(k)
                if action == 'exit-after-go':
                    return
                os.write(self.request, f'DONE {k} {run}\n'.encode())
                ack = self.line()
                if k == 19:
                    self.verdict = ack.split()[-1]
        except Exception as error:
            self.error = error
        finally:
            os.close(self.request)
            os.close(self.reply)


class FakeCollector:
    def __init__(self, fail=None, counts=(1000, 10, 500, 10)):
        self.fail, self.counts = fail, counts
        self.record = {'profile': 'slot-env-v1', 'status': 'FAKE'}

    def start(self):
        if self.fail == 'start':
            raise b.ProfileError('fake start failure')

    def enable(self):
        pass

    def disable(self):
        if self.fail == 'disable':
            raise b.ProfileError('fake collector lost')

    def finish(self):
        return full_pmu(*self.counts)

    def cleanup(self):
        pass


class GateService(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix=self._testMethodName + '-'))
        self.addCleanup(shutil.rmtree, self.root)
        self.gate = b.GateServer(self.root / 'gate')
        self.addCleanup(self.gate.close)
        self.host = FakeHost()
        recipe = {'pairs': b.pairs(b'\x01' * 16, 2026102571)}
        self.plan = b.Plan(recipe)

    def serve(self, *, during=None, script=None, pmu_factory=None, stop_after=None):
        controller = FakeController(self.root / 'gate', self.plan.runs, during, script)
        controller.start()
        record = {}
        error = None
        try:
            b.serve(self.gate, controller, self.plan, RULES, self.host, pmu_factory=pmu_factory,
                    stop_after=stop_after, verdict_path=self.root / 'verdict.json', record=record, ready_seconds=5)
        except b.GateError as caught:
            error = caught
        controller.join(10)
        return record, controller, error

    def test_twenty_slots_bind_verdict_digest_in_final_ack(self):
        record, controller, error = self.serve(pmu_factory=lambda k: FakeCollector())
        self.assertIsNone(error)
        self.assertIsNone(controller.error)
        self.assertEqual(len(record['slots']), 20)
        self.assertEqual(controller.verdict, b.digest(self.root / 'verdict.json'))
        self.assertEqual(record['environment_validity'], 'VALID')
        verdict = json.loads((self.root / 'verdict.json').read_text())
        self.assertEqual(verdict['covariate']['status'], 'VALID')
        self.assertEqual(len(record['idle']), 10)  # first slot of each pair only
        self.assertTrue(all(s['tasks_at_done'] for s in record['slots']))
        self.assertEqual(self.gate.lines[-1]['text'], f'ACK 19 {self.plan.runs[19]} {controller.verdict}')

    def test_invalid_slot_never_stops_the_campaign(self):
        def contend(k):
            if k == 3:
                with self.host.lock:
                    self.host.counters[18][0] += 2
        record, controller, error = self.serve(during=contend)
        self.assertIsNone(error)
        self.assertEqual(len(record['slots']), 20)
        self.assertEqual(record['slots'][3]['tick_rule']['status'], 'INVALID')
        self.assertEqual(record['environment_validity'], 'INVALID')
        self.assertIsNotNone(controller.verdict)

    def test_idle_wait_limit_aborts_before_go_with_verdict(self):
        self.host.busy_windows = 10**6
        record, controller, error = self.serve()
        self.assertIsNone(error)
        self.assertEqual(record['abort'], 'IDLE_WAIT_EXCEEDED')
        self.assertFalse(record['go_sent'])
        self.assertEqual(len(record['idle'][0]), 60)
        self.assertEqual(controller.received[0], f'ABORT 0 {self.plan.runs[0]}')
        self.assertEqual(controller.verdict, b.digest(self.root / 'verdict.json'))

    def test_campaign_cap_aborts_at_the_next_ready(self):
        def advance(k):
            if k == 3:
                self.host.clock += (b.CAMPAIGN_SECONDS - 50) * 10**9
        record, controller, error = self.serve(during=advance)
        self.assertIsNone(error)
        self.assertEqual((record['abort'], len(record['slots'])), ('CAMPAIGN_CAP', 4))
        self.assertTrue(controller.received[-1].startswith(f'ACK 4 {self.plan.runs[4]} '))

    def test_duplicate_ready_wrong_order_and_controller_exit_fail_the_gate(self):
        for script, text in (({2: 'duplicate-ready'}, 'unsolicited'), ({1: 'exit'}, 'controller exited'),
                             ({4: 'exit-after-go'}, 'controller exited')):
            self.setUp()
            record, _, error = self.serve(script=script)
            self.assertIsNotNone(error, script)
            self.assertIn(text, str(error))
        self.setUp()
        controller = FakeController(self.root / 'gate', ['00' * 16] + self.plan.runs[1:])
        controller.start()
        with self.assertRaisesRegex(b.GateError, 'expected READY 0'):
            b.serve(self.gate, controller, self.plan, RULES, self.host, verdict_path=self.root / 'v.json',
                    ready_seconds=5)
        controller.join(10)
        with self.assertRaisesRegex(b.GateError, 'deadline'):
            self.gate.receive(.05, lambda: True)

    def test_controller_lost_during_idle_wait_gets_no_go(self):
        controller = FakeController(self.root / 'gate', self.plan.runs, script={0: 'exit-after-ready'})
        self.host.before_sleep = lambda: controller.join(5)
        controller.start()
        record = {}
        with self.assertRaisesRegex(b.GateError, 'before GO'):
            b.serve(self.gate, controller, self.plan, RULES, self.host, verdict_path=self.root / 'v.json',
                    record=record, ready_seconds=5)
        self.assertFalse(record['go_sent'])
        self.assertEqual([line['text'].split()[0] for line in self.gate.lines], ['READY'])

    def test_collector_failures_before_and_after_first_go(self):
        record, controller, error = self.serve(pmu_factory=lambda k: FakeCollector('start'))
        self.assertIsNone(error)
        self.assertFalse(record['go_sent'])
        self.assertIn('start failed', record['pre_go_failure'])
        self.assertEqual(record['abort'], 'COLLECTOR_FAILURE')
        self.setUp()
        record, controller, error = self.serve(
            pmu_factory=lambda k: FakeCollector('disable' if k == 5 else None))
        self.assertIsNone(error)
        self.assertEqual(len(record['slots']), 6)
        self.assertIn('slot 5', record['termination'])
        self.assertEqual(controller.received[-2], f'ABORT 6 {self.plan.runs[6]}')
        self.assertEqual(record['slots'][5]['pmu']['status'], 'UNKNOWN')

    def test_connection_stop_after_two_slots(self):
        record, controller, error = self.serve(stop_after=2)
        self.assertIsNone(error)
        self.assertEqual((record['abort'], len(record['slots'])), ('CONNECTION_STOP', 2))
        self.assertIsNotNone(controller.verdict)


class SlotEnvProcess(unittest.TestCase):
    def collector(self, behavior='normal', counts='1000,40,500,30'):
        root = Path(tempfile.mkdtemp(prefix=self._testMethodName + '-'))
        self.addCleanup(shutil.rmtree, root)
        environment = {'PROFILE_B_FAKE_PERF': behavior, 'PROFILE_B_FAKE_COUNTS': counts}
        with patch.dict(os.environ, environment):
            return b.SlotEnvCollector(root / 'slot', PERF, 'none', deadline=10)

    def test_fake_perf_cycle_collects_literal_rows_and_cleans_up(self):
        collector = self.collector()
        with patch.dict(os.environ, {'PROFILE_B_FAKE_PERF': 'normal', 'PROFILE_B_FAKE_COUNTS': '1000,40,500,30'}):
            collector.start()
        collector.enable()
        collector.disable()
        parsed = collector.finish()
        collector.cleanup()
        self.assertTrue(parsed['all_rows_full_coverage'])
        self.assertEqual(b.pmu_rule(parsed, B2)['status'], 'VALID')
        self.assertTrue(all(r['group_gone'] for r in collector.record['cleanup']))

    def test_scaled_lost_and_unresponsive_collectors_are_not_valid(self):
        collector = self.collector()
        with patch.dict(os.environ, {'PROFILE_B_FAKE_PERF': 'scaled'}):
            collector.start()
        collector.enable()
        collector.disable()
        self.assertEqual(b.pmu_rule(collector.finish(), B2)['status'], 'UNKNOWN')
        collector.cleanup()
        collector = self.collector()
        with patch.dict(os.environ, {'PROFILE_B_FAKE_PERF': 'lost'}):
            collector.start()
        with self.assertRaises(Exception):
            collector.enable()
        collector.cleanup()
        collector = self.collector()
        with patch.dict(os.environ, {'PROFILE_B_FAKE_PERF': 'ignore-sigint'}):
            collector.start()
        collector.enable()
        collector.disable()
        with self.assertRaises(Exception):
            collector.finish()
        collector.cleanup()  # SIGTERM/SIGKILL cleanup still empties the owned group
        self.assertTrue(all(r['group_gone'] for r in collector.record['cleanup']))

    def test_collector_identity_accepts_only_perfmon_in_owned_group(self):
        caps = {k: b.window.PERFMON for k in ('CapInh', 'CapPrm', 'CapEff', 'CapBnd', 'CapAmb')}
        policy = {'uid': 1000, 'gid': 1000, 'perf_command': ['/usr/bin/perf', 'stat']}
        launcher = {'pid': 10, 'pgid': 10, 'sid': 10, 'uids': [1000, 0, 0, 0]}
        collector = {'pid': 11, 'ppid': 10, 'pgid': 10, 'sid': 10, 'uids': [1000] * 4, 'gids': [1000] * 4,
                     'groups': [], 'no_new_privs': 1, 'caps': caps, 'command': ['/usr/bin/perf', 'stat']}
        b.validate_collector_identity(launcher, collector, policy)
        for change in ({'caps': caps | {'CapEff': b.window.PERFMON | 1 << 21}}, {'uids': [0] * 4},
                       {'groups': [27]}, {'no_new_privs': 0}, {'pgid': 99}, {'ppid': 1},
                       {'command': ['/usr/bin/perf', 'stat', '-a']}):
            with self.assertRaises(b.ProfileError, msg=change):
                b.validate_collector_identity(launcher, collector | change, policy)
        with self.assertRaises(b.ProfileError):
            b.validate_collector_identity(launcher | {'sid': 3}, collector, policy)

    def test_privileged_mode_refuses_fixture_before_sudo(self):
        collector = self.collector()
        collector.privilege = 'sudo-cap-perfmon'
        with self.assertRaisesRegex(Exception, 'only supports installed'):
            collector.start()
        collector.cleanup()
        self.assertNotIn('perf', collector.runner.record.get('commands', {}))


class SessionLifecycle(unittest.TestCase):
    """Supervisor failures before the gate settle as NOT_RUN; close shares the campaign lock."""
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix=self._testMethodName + '-'))
        self.addCleanup(shutil.rmtree, self.root)
        files = []
        for name in ('controller', 'worker', 'config'):
            path = self.root / name
            path.write_text(name)  # not executable: prepare-performance cannot start
            files.append(path)
        receipt = self.root / 'receipt.json'
        receipt.write_text(json.dumps({'worker_sha256': b.digest(files[1]), 'config_sha256': b.digest(files[2])}))
        with patch.object(b, 'device_identity', return_value={'disk': 'fixture'}):
            self.session = b.initialize(self.root / 'session', files[0], files[1], receipt, files[2], 'unavailable',
                                        new_budget=True)

    def test_failure_before_gate_uses_one_attempt_as_not_run(self):
        before = {sig: b.signal.getsignal(sig) for sig in (b.signal.SIGINT, b.signal.SIGTERM)}
        with patch.object(b.os, 'sched_getaffinity', return_value={b.SUPERVISOR_CPU}):
            result = b.campaign(self.session, 'always-control', host=FakeHost(), pmu=False)
        self.assertEqual({sig: b.signal.getsignal(sig) for sig in before}, before)
        self.assertEqual((result['classification']['status'], result['classification']['replaceable']),
                         ('NOT_RUN', True))
        self.assertIn('supervisor before gate', result['classification']['reason'])
        record = json.loads((self.session / 'campaigns/always-control-a0/campaign.json').read_text())
        self.assertEqual(record['tool']['dev2_profile_b.py'], b.digest(b.__file__))
        ledger = b.Session(self.session).ledger()
        self.assertEqual(len(ledger['roles']['always-control']['attempts']), 1)
        self.assertEqual(b.admit(ledger, 'always-control')['order_seed'], 2027102571)
        self.assertFalse((self.session / 'lock').exists())

    def test_close_takes_the_campaign_lock_and_checks_the_total(self):
        (self.session / 'lock').write_text('')
        with self.assertRaises(FileExistsError):
            b.close(self.session)
        (self.session / 'lock').unlink()
        session = b.Session(self.session)
        ledger = session.ledger()
        ledger['used_seconds'] = 5
        session.write_ledger(ledger)
        with self.assertRaisesRegex(b.ProfileError, 'total differs'):
            b.close(self.session)
        ledger['used_seconds'] = 0
        session.write_ledger(ledger)
        self.assertEqual(b.close(self.session)['used_seconds'], 0)
        self.assertFalse((self.session / 'lock').exists())


@unittest.skipUnless(os.environ.get('PROFILE_B_TEST_CONTROLLER') and os.environ.get('PROFILE_B_TEST_WORKER'),
                     'needs a self-test controller and native mutant worker')
class CampaignWiring(unittest.TestCase):
    """Real controller process and gate with a mutant worker and scripted host; no PMU."""
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='profile-b-wiring-'))
        self.addCleanup(shutil.rmtree, self.root)
        worker = self.root / 'worker'
        shutil.copy(os.environ['PROFILE_B_TEST_WORKER'], worker)
        config = self.root / 'config.json'
        config.write_text('{"synthetic":"wiring"}')
        receipt = self.root / 'receipt.json'
        receipt.write_text(json.dumps({'schema_version': 1, 'worker_sha256': b.digest(worker), 'build': {
            'source_sha256': 'd' * 64, 'toolchain': 'fixed vector', 'target': 'x86_64-unknown-linux-gnu',
            'profile': 'release', 'self_test': False}, 'command': 'synthetic wiring fixture', 'features': [],
            'config_sha256': b.digest(config)}))
        with patch.object(b, 'device_identity', return_value={'disk': 'fixture'}):
            self.session = b.initialize(self.root / 'session', os.environ['PROFILE_B_TEST_CONTROLLER'], worker,
                                        receipt, config, 'unavailable', new_budget=True)
        self.inputs = (worker, receipt, config)

    def run_role(self, role):
        with patch.object(b.os, 'sched_getaffinity', return_value={b.SUPERVISOR_CPU}), \
                patch.object(b, 'device_identity', return_value={'disk': 'fixture'}):
            return b.campaign(self.session, role, host=FakeHost(), pmu=False)

    def test_control_and_connection_reach_classification(self):
        result = self.run_role('always-control')
        self.assertEqual(result['classification']['status'], 'INCOMPLETE', result)
        self.assertEqual(result['classification']['reason'], 'INCONCLUSIVE_COMPARISON')
        self.assertEqual(result['summary']['environment_validity'], 'VALID')
        bundle = Path(result['summary']['bundle'])
        index = json.loads((bundle / 'index.json').read_text())
        self.assertEqual(index['gate']['environment_sha256'], result['summary']['environment_sha256'])
        recipe = json.loads((bundle / 'recipe.json').read_text())
        self.assertEqual((recipe['kind'], recipe['candidate']['mode'], recipe['order_seed']),
                         ('exploration', 'off', 2026102571))
        decision = b.pf04_decision(b.Session(self.session).ledger())
        self.assertEqual(decision['modes']['always']['reason'], 'CONTROL_INCONCLUSIVE_COMPARISON')
        record = json.loads((self.session / 'campaigns/always-control-a0/campaign.json').read_text())
        verdict = json.loads((self.session / 'campaigns/always-control-a0/environment-verdict.json').read_text())
        self.assertEqual(record['tool'], verdict['tool'])
        connection = self.run_role('connection')
        self.assertEqual(connection['classification']['status'], 'TOOL_CONNECTED', connection)
        index = json.loads((Path(connection['summary']['bundle']) / 'index.json').read_text())
        self.assertEqual(sum(1 for r in index['runs'] if r['directory']), 2)
        with self.assertRaisesRegex(b.ProfileError, 'final'):
            self.run_role('always-control')
        # The next session starts from this session's closed total, not from zero.
        closed = b.close(self.session)
        with self.assertRaisesRegex(b.ProfileError, 'closed'):
            self.run_role('detailed-control')
        with patch.object(b, 'device_identity', return_value={'disk': 'fixture'}):
            later = b.initialize(self.root / 'later', os.environ['PROFILE_B_TEST_CONTROLLER'], *self.inputs,
                                 'unavailable', budget_from=[(closed['ledger'], closed['sha256'])])
        carried = b.Session(later).ledger()
        self.assertEqual(carried['used_seconds'], closed['used_seconds'])
        self.assertGreater(carried['used_seconds'], 0)


if __name__ == '__main__':
    unittest.main()
