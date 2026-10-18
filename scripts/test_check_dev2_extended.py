"""Negative evidence tests: an executable exit zero is not V3 qualification."""
import copy
import json
import os
from pathlib import Path
import tempfile
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

import check_dev2_extended as gate


def receipts(spec):
    profile = {key: spec[key] for key in ('target', 'generator', 'seed')}
    profile['min_seconds'] = 1800
    result = {**profile, 'elapsed_ms': 1800000}
    if spec['target'] == 'k0_real_thread_stress':
        profile['seed_count'] = 256
        result.update(rounds=1280, lane_rounds={lane: 256 for lane in gate.LANES},
                      reader_operations=81920, reader_operations_by_thread=[20480] * 4,
                      writer_operations=40960, missing_lanes=0, seed_count=256,
                      visited_seed_lanes=1280)
    else:
        profile.update(coverage_guided=False, input_cap=8388612)
        families = (*gate.FUZZ_FAMILIES, 'product_code') if spec['target'] == 'native_response_fuzz' else gate.FUZZ_FAMILIES
        corpus_cases = 322 if spec['target'] == 'native_request_fuzz' else 223
        cases = corpus_cases + 1024
        family_counts = {key: 1 for key in families}
        family_counts['differential'] = cases - len(families) + 1
        operators = {key: 1 for key in gate.FUZZ_OPERATORS}
        operators['byte_flip'] += 1024 - len(operators)
        result.update(cases=cases, accepted=100, rejected=cases - 100, known_valid=1,
                      known_invalid=len(families) - 2, mutations=1024, corpus_cases=corpus_cases,
                      family_visits=family_counts, operator_visits=operators,
                      variant_visits={key: 1 for key in gate.FUZZ_VARIANTS[spec['target']]})
    return profile, result


def output(spec, profile, result):
    return (f'running 1 test\ntest {spec["test"]} ... XADB_V3_PROFILE {json.dumps(profile)}\n'
            f'XADB_V3_RESULT {json.dumps(result)}\nok\n'
            'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 1800.00s\n')


class ExtendedGateTests(unittest.TestCase):
    def test_binary_changed_after_build_is_rejected_before_spawn(self):
        with tempfile.TemporaryDirectory(prefix='xadb-v3-binary-') as temporary:
            session = Path(temporary)
            spec = gate.BASE_TARGETS[0]
            binary = session / 'changed-worker'
            binary.write_text('#!/usr/bin/env python3\nraise SystemExit(0)\n')
            binary.chmod(0o700)
            key = (spec['package'], spec['binary'])
            with patch.object(gate.subprocess, 'Popen') as spawn:
                results = gate.run_targets([spec], {key: binary}, session, os.environ.copy(),
                                           {key: '0' * 64})
            spawn.assert_not_called()
            self.assertEqual(results[0]['status'], 'FAIL')
            self.assertIn('build receipt', results[0]['error'])
            self.assertIsNone(results[0]['exit_code'])

    def test_complete_frozen_receipts_are_accepted_for_all_targets(self):
        for spec in gate.BASE_TARGETS:
            profile, result = receipts(spec)
            parsed = gate.validate_receipts(spec, output(spec, profile, result), 1801)
            self.assertEqual(parsed['result'], result)

    def test_missing_duplicate_short_foreign_and_zero_test_receipts_fail(self):
        for spec in gate.BASE_TARGETS:
            profile, result = receipts(spec)
            valid = output(spec, profile, result)
            bad_outputs = [valid.replace('running 1 test', 'running 0 tests'),
                           valid.replace('XADB_V3_RESULT ', 'unrecognized '),
                           valid + f'XADB_V3_RESULT {json.dumps(result)}\n',
                           valid.replace('1 passed; 0 failed', '0 passed; 1 failed'),
                           valid.replace('"elapsed_ms": 1800000', '"elapsed_ms": 1, "elapsed_ms": 1800000')]
            for text in bad_outputs:
                with self.assertRaises((ValueError, RuntimeError)):
                    gate.validate_receipts(spec, text, 1801)
            for field, value in [('target', 'foreign'), ('generator', 'future'),
                                 ('seed', True), ('seed', spec['seed'] + 1),
                                 ('elapsed_ms', 1799999), ('elapsed_ms', 1802000),
                                 ('elapsed_ms', '1800000')]:
                changed = {**result, field: value}
                with self.assertRaises(ValueError, msg=field):
                    gate.validate_receipts(spec, output(spec, profile, changed), 1801)
            for seconds in [0, 1799.999]:
                with self.assertRaises(ValueError):
                    gate.validate_receipts(spec, valid, seconds)
            for duration in [None, True, 1799, 1801]:
                changed = {**profile, 'min_seconds': duration}
                with self.assertRaises(ValueError):
                    gate.validate_receipts(spec, output(spec, changed, result), 1801)

    def test_missing_work_and_inconsistent_lane_counts_fail(self):
        spec = gate.TARGETS[0]
        profile, result = receipts(spec)
        changes = [{'rounds': 1279}, {'missing_lanes': 1}, {'reader_operations': 0},
                   {'writer_operations': 0}, {'lane_rounds': {'off': 1280}},
                   {'lane_rounds': {lane: 255 for lane in gate.LANES}},
                   {'rounds': 1281}, {'missing_lanes': False}, {'visited_seed_lanes': 1279},
                   {'reader_operations_by_thread': [81920, 0, 0, 0]}, {'seed_count': 255}]
        for changed in changes:
            with self.assertRaises(ValueError):
                gate.validate_receipts(spec, output(spec, profile, {**result, **changed}), 1801)
        spec = gate.TARGETS[1]
        profile, result = receipts(spec)
        for field in ('cases', 'accepted', 'rejected', 'known_valid', 'known_invalid'):
            with self.assertRaises(ValueError):
                gate.validate_receipts(spec, output(spec, profile, {**result, field: 0}), 1801)
        with self.assertRaises(ValueError):
            gate.validate_receipts(spec, output(spec, profile, {**result, 'cases': 201}), 1801)
        for field in ('variant_visits', 'family_visits', 'operator_visits'):
            changed = copy.deepcopy(result)
            changed[field].pop(next(iter(changed[field])))
            with self.assertRaises(ValueError):
                gate.validate_receipts(spec, output(spec, profile, changed), 1801)
        with self.assertRaises(ValueError):
            gate.validate_receipts(spec, output(spec, profile, {**result, 'known_valid': 101}), 1801)

    def test_actual_short_exit_zero_and_crash_never_pass_and_logs_survive(self):
        with tempfile.TemporaryDirectory(prefix='xadb-v3-gate-') as temporary:
            session = Path(temporary)
            specs = copy.deepcopy(gate.TARGETS[:2])
            binaries = {}
            for index, spec in enumerate(specs):
                binary = session / f'mutant-{index}'
                profile, result = receipts(spec)
                # Shell-free executable fixture, with literal output and no
                # implementation dependency on the gate being tested.
                script = '#!/usr/bin/env python3\n'
                script += f'print({output(spec, profile, result)!r}, flush=True)\n'
                if index == 1:
                    script += 'raise SystemExit(7)\n'
                binary.write_text(script)
                binary.chmod(0o700)
                binaries[(spec['package'], spec['binary'])] = binary
            results = gate.run_targets(specs, binaries, session, os.environ.copy())
            self.assertEqual([entry['status'] for entry in results], ['FAIL', 'FAIL'])
            self.assertEqual({entry['exit_code'] for entry in results}, {0, 7})
            for entry in results:
                self.assertIn('XADB_V3_RESULT', Path(entry['log']).read_text())
                self.assertEqual(entry['log_sha256'], gate.digest(entry['log']))
            self.assertEqual(json.loads((session / 'target-results.json').read_text()), results)

    def test_hung_target_is_killed_reaped_and_recorded(self):
        with tempfile.TemporaryDirectory(prefix='xadb-v3-timeout-') as temporary:
            session = Path(temporary)
            spec = gate.TARGETS[0]
            binary = session / 'hung-worker'
            binary.write_text('#!/usr/bin/env python3\nimport time\nprint("entered", flush=True)\ntime.sleep(60)\n')
            binary.chmod(0o700)
            with patch.object(gate, 'TIMEOUT_SECONDS', 0.1):
                results = gate.run_targets([spec], {(spec['package'], spec['binary']): binary},
                                          session, os.environ.copy())
            self.assertEqual(results[0]['status'], 'FAIL')
            self.assertTrue(results[0]['timed_out'])
            self.assertEqual(results[0]['exit_code'], -9)
            self.assertIn('entered', Path(results[0]['log']).read_text())

    def test_spawn_preexec_failure_becomes_failed_target_receipt(self):
        with tempfile.TemporaryDirectory(prefix='xadb-v3-spawn-') as temporary:
            session = Path(temporary)
            spec = gate.TARGETS[0]
            binary = session / 'binary'
            binary.write_text('never started')
            with patch.object(gate.subprocess, 'Popen', side_effect=subprocess.SubprocessError('preexec failed')):
                result = gate.run_targets([spec], {(spec['package'], spec['binary']): binary},
                                          session, os.environ.copy())
            self.assertEqual(result[0]['status'], 'FAIL')
            self.assertIn('preexec failed', result[0]['error'])
            self.assertIsNone(result[0]['exit_code'])
            self.assertTrue((session / 'target-results.json').is_file())

    def test_command_timeout_kills_descendant_and_keeps_uncaptured_failure_log(self):
        with tempfile.TemporaryDirectory(prefix='xadb-v3-command-') as temporary:
            session = Path(temporary)
            child_pid = session / 'child.pid'
            parent = session / 'parent.py'
            parent.write_text(
                'import subprocess,sys,time\n'
                'child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])\n'
                f'open({str(child_pid)!r}, "w").write(str(child.pid))\n'
                'print("original panic before barrier hang", flush=True)\n'
                'time.sleep(60)\n')
            log = session / 'output.log'
            result = gate.run_command([sys.executable, str(parent)], session, os.environ.copy(), log, 0.3)
            self.assertTrue(result['timed_out'])
            self.assertTrue(result['remaining_group_signalled'])
            self.assertEqual(result['exit_code'], -9)
            self.assertIn('original panic', log.read_text())
            pid = int(child_pid.read_text())
            # Linux may briefly retain an adopted zombie. It must no longer run;
            # waitpid on a non-child would not prove that its group was stopped.
            state = Path(f'/proc/{pid}/stat')
            deadline = time.monotonic() + 2
            while state.exists() and state.read_text().split(') ', 1)[1][0] != 'Z':
                self.assertLess(time.monotonic(), deadline, 'descendant still running')
                time.sleep(0.01)


if __name__ == '__main__':
    unittest.main()
