"""Controls for the new COMM five-plus-five policy, without native workloads."""
from contextlib import nullcontext
from copy import deepcopy
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import check_comm_performance as performance
import check_comm_performance_batch as batch
from test_check_comm_performance import _record


def rows(index=37, rates=(1000,) * 5):
    result = []
    cell = batch.CATALOGUE[index]
    for pair, detailed in enumerate(rates, 1):
        for mode in ('ALWAYS', 'DETAILED'):
            rate = 1000 if mode == 'ALWAYS' else detailed
            record = _record(mode, cell['workload'], cell['payload_bytes'], rate,
                             rate * cell['payload_bytes'], p99=100)
            record.update(cell=cell.copy(), cell_index=index, pair=pair,
                          source_sha256='source', binary_sha256='binary',
                          repetition_policy=performance.REPETITION_POLICY)
            result.append(record)
    return result


class AdaptiveRepetitionTests(unittest.TestCase):
    def test_stable_five_stop_and_unselected_extension_is_rejected(self):
        result = batch.batch_summary(rows(), [37], 'source', 'binary')
        self.assertEqual(result['status'], 'BATCH_PASS')
        self.assertEqual(result['required_mode_runs'], 10)
        self.assertEqual(result['repetition_decisions'][0]['required_pairs'], 5)
        with self.assertRaises(ValueError):
            batch.batch_summary(rows(rates=(1000,) * 10), [37], 'source', 'binary')

    def test_wide_interval_requires_all_ten_and_preserves_first_five(self):
        rates = (1200, 1000, 1200, 1000, 1200)
        first = batch.batch_summary(rows(rates=rates), [37], 'source', 'binary')
        self.assertEqual(first['status'], 'INCOMPLETE_VARIATION')
        decision = first['repetition_decisions'][0]
        self.assertIn('throughput_wide_interval', decision['extension_reasons'])
        self.assertEqual(decision['first_five_summary']['pair_count'], 5)
        final = batch.batch_summary(rows(rates=rates + (1000,) * 5), [37], 'source', 'binary')
        self.assertEqual(final['status'], 'BATCH_PASS')
        self.assertEqual(final['required_mode_runs'], 20)
        self.assertEqual(final['repetition_decisions'][0]['first_five_summary'], decision['first_five_summary'])
        for count in (6, 7, 8, 9):
            with self.assertRaises(ValueError):
                batch.batch_summary(rows(rates=(rates + (1000,) * 5)[:count]), [37], 'source', 'binary')

    def test_threshold_uncertainty_then_failure_and_terminal_clear_failure(self):
        rates = (920, 910, 900, 890, 880)
        first = batch.batch_summary(rows(rates=rates), [37], 'source', 'binary')
        self.assertEqual(first['status'], 'INCOMPLETE_VARIATION')
        self.assertIn('throughput_threshold_uncertain', first['repetition_decisions'][0]['extension_reasons'])
        self.assertEqual(batch.batch_summary(rows(rates=rates * 2), [37], 'source', 'binary')['status'], 'FAIL')
        clear = batch.batch_summary(rows(rates=(500,) * 5), [37], 'source', 'binary')
        self.assertEqual(clear['status'], 'FAIL')
        self.assertFalse(clear['repetition_decisions'][0]['additional_pairs_required'])
        with self.assertRaises(ValueError):
            batch.batch_summary(rows(rates=(500,) * 5 + (1000,) * 5), [37], 'source', 'binary')

    def test_quota_failure_has_priority_and_malformed_receipts_are_rejected(self):
        noisy = rows(rates=(1200, 1000, 1200, 1000, 1200))
        noisy[0]['worker']['diagnostics']['peak_payload_bytes'] = performance.PAYLOAD_QUOTA_BYTES + 1
        result = batch.batch_summary(noisy, [37], 'source', 'binary')
        self.assertEqual(result['status'], 'FAIL')
        self.assertFalse(result['repetition_decisions'][0]['additional_pairs_required'])
        for mutate in (lambda r: r.pop(), lambda r: r.__setitem__(0, deepcopy(r[1])),
                       lambda r: r[0].update(pair=True), lambda r: r[0].update(status='FAIL'),
                       lambda r: r[0].update(source_sha256='other'),
                       lambda r: r[0].update(repetition_policy='legacy-ten-pairs')):
            bad = rows()
            mutate(bad)
            with self.assertRaises(ValueError):
                batch.batch_summary(bad, [37], 'source', 'binary')
        for latency in (float('nan'), float('inf'), -1, True):
            bad = rows(38)
            bad[0]['peer']['latency_p99_ns'] = latency
            with self.assertRaises(ValueError):
                batch.batch_summary(bad, [38], 'source', 'binary')

    def test_na_is_derived_from_inventory_and_bulk_keeps_throughput(self):
        indices = [i for i, c in enumerate(batch.CATALOGUE) if c['slow_peer'] and c['connections'] == 1]
        self.assertEqual(len(indices), 15)
        self.assertEqual(performance.adaptive_max_mode_runs(batch.CATALOGUE), 2550)
        for index in indices:
            result = batch.batch_summary(rows(index, (1200, 1000, 1200, 1000, 1200)), [index], 'source', 'binary')
            self.assertEqual(result['status'], 'BATCH_PASS')
            self.assertEqual(result['required_mode_runs'], 10)
        result = batch.batch_summary(rows(rates=(1200, 1000, 1200, 1000, 1200)), [37], 'source', 'binary')
        self.assertIsNone(result['paired_noise_summary'][0]['paired_p99_rise_mean'])
        self.assertEqual(result['status'], 'INCOMPLETE_VARIATION')

    def test_adjusted_bounds_can_require_extension_when_descriptive_upper_passes(self):
        comparisons = performance.summarize_pairs(rows(), 1, 5, [batch.CATALOGUE[37]])
        means = [0.0] * 9800 + [0.2] * 200
        with patch('check_comm_performance.bootstrap_mean_samples', return_value=means):
            result = performance.repetition_noise(comparisons, 5)
        self.assertEqual(result['completed_throughput_drop_95_ci']['upper_95'], 0)
        self.assertEqual(result['decision_intervals']['throughput']['upper'], .2)
        self.assertTrue(result['additional_pairs_required'])

    def test_width_boundary_is_strict_and_echo_p99_can_select_extension(self):
        comparisons = performance.summarize_pairs(rows(), 1, 5, [batch.CATALOGUE[37]])
        for width, expected in ((.05, False), (.050001, True)):
            summary = performance.summarize_pair_noise(comparisons, 5)[0]
            summary['completed_throughput_drop_95_ci'] = {'lower_95': 0, 'upper_95': width}
            with (patch('check_comm_performance.summarize_pair_noise', return_value=[summary]),
                  patch('check_comm_performance.bootstrap_mean_samples', return_value=[0.] * 10_000)):
                self.assertEqual(performance.repetition_noise(comparisons, 5)['additional_pairs_required'], expected)
        echo = rows(38)
        for row in echo:
            if row['instrumentation'] == 'DETAILED':
                row['peer']['latency_p99_ns'] = 70 if row['pair'] % 2 else 100
        result = batch.batch_summary(echo, [38], 'source', 'binary')
        self.assertEqual(result['status'], 'INCOMPLETE_VARIATION')
        self.assertIn('p99_wide_interval', result['repetition_decisions'][0]['extension_reasons'])

    def test_full_runner_failed_eleventh_mode_is_attempted_not_completed(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            destination = root / 'result.json'
            responses = iter(rows(rates=(1200, 1000, 1200, 1000, 1200)))

            def fake_case(_binary, directory, *_args):
                directory.mkdir(parents=True)
                try:
                    return next(responses)
                except StopIteration:
                    raise TimeoutError('additional mode interrupted') from None

            # The controller alternates order; supply the proper mode for each call.
            def ordered_case(binary, directory, cell, mode, source):
                record = fake_case(binary, directory)
                record['instrumentation'] = mode
                if mode == 'ALWAYS':
                    rate = 1000
                else:
                    pair = int(directory.parent.parent.name.split('-')[1])
                    rate = 1200 if pair % 2 else 1000
                for owner in (record['peer'], record['worker']['totals']):
                    frame_key = 'measured_frames_per_second' if owner is record['peer'] else 'measured_received_frames_per_second'
                    byte_key = 'measured_payload_bytes_per_second' if owner is record['peer'] else 'measured_received_payload_bytes_per_second'
                    owner[frame_key] = [rate] * 30
                    owner[byte_key] = [rate * cell['payload_bytes']] * 30
                return record

            with (patch('check_comm_performance.ROOT', root),
                  patch('sys.argv', ['performance', '--report', str(destination)]),
                  patch('check_comm_performance.matrix', return_value=[batch.CATALOGUE[37]]),
                  patch('check_comm_performance.slow_peer_matrix', return_value=[]),
                  patch('check_comm_performance.capture_source', return_value='source'),
                  patch('check_comm_performance.source_fingerprint', return_value='source'),
                  patch('check_comm_performance.digest', return_value='binary'),
                  patch('check_comm_performance.subprocess.run', return_value=Mock(returncode=0, stdout=b'')),
                  patch('check_comm_performance.run_case', side_effect=ordered_case)):
                with self.assertRaises(TimeoutError):
                    performance.main()
            report = json.loads(destination.read_text())
            self.assertEqual(report['status'], 'FAIL')
            self.assertEqual(report['attempted_mode_runs'], 11)
            self.assertEqual(report['completed_mode_runs'], 10)

    def test_mixed_batch_and_interrupted_extension_never_fall_back_to_pass(self):
        for interrupt in (False, True):
            with self.subTest(interrupt=interrupt), tempfile.TemporaryDirectory() as scratch:
                root = Path(scratch)
                destination = root / 'result.json'
                calls = []

                def fake_case(_binary, directory, cell, mode, _source):
                    pair = int(directory.parent.parent.name.split('-')[1])
                    index = int(directory.parent.name.split('-')[1])
                    if interrupt and pair == 6:
                        raise TimeoutError('additional measurement interrupted')
                    calls.append((index, pair, mode))
                    directory.mkdir(parents=True)
                    rates = (1200, 1000, 1200, 1000, 1200) + (1000,) * 5 if index == 39 else (1000,) * 10
                    return next(r for r in rows(index, rates) if r['pair'] == pair and r['instrumentation'] == mode)

                deadline = (datetime.now(timezone.utc) + timedelta(hours=3)).isoformat()
                with (patch('check_comm_performance_batch.ROOT', root),
                      patch('sys.argv', ['batch', '--cell-index', '37', '--cell-index', '39',
                                        '--deadline', deadline, '--batch-seconds', '8000', '--report', str(destination)]),
                      patch('check_comm_performance_batch.capture_source', return_value='source'),
                      patch('check_comm_performance_batch.source_fingerprint', return_value='source'),
                      patch('check_comm_performance_batch.digest', return_value='binary'),
                      patch('check_comm_performance_batch.subprocess.run', return_value=Mock(returncode=0)),
                      patch('check_comm_performance_batch.mode_timeout', return_value=nullcontext()),
                      patch('check_comm_performance_batch.run_case', side_effect=fake_case)):
                    self.assertEqual(batch.main(), int(interrupt))
                report = json.loads(destination.read_text())
                self.assertEqual(report['status'], 'FAIL' if interrupt else 'BATCH_PASS')
                self.assertEqual(report['completed_mode_runs'], 20 if interrupt else 30)
                self.assertEqual(report['first_stage_decision']['status'], 'INCOMPLETE_VARIATION')
                self.assertEqual(sum(i == 37 for i, _, _ in calls), 10)
                self.assertEqual(report['c1_status'], 'INCOMPLETE')

    def test_full_runner_uses_the_same_conditional_rule(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            destination = root / 'result.json'
            calls = []

            def fake_case(_binary, directory, cell, mode, _source):
                pair = int(directory.parent.parent.name.split('-')[1])
                calls.append(pair)
                directory.mkdir(parents=True)
                rates = (1200, 1000, 1200, 1000, 1200) + (1000,) * 5
                return next(r for r in rows(37, rates) if r['pair'] == pair and r['instrumentation'] == mode)

            with (patch('check_comm_performance.ROOT', root),
                  patch('sys.argv', ['performance', '--report', str(destination)]),
                  patch('check_comm_performance.matrix', return_value=[batch.CATALOGUE[37]]),
                  patch('check_comm_performance.slow_peer_matrix', return_value=[]),
                  patch('check_comm_performance.capture_source', return_value='source'),
                  patch('check_comm_performance.source_fingerprint', return_value='source'),
                  patch('check_comm_performance.digest', return_value='binary'),
                  patch('check_comm_performance.subprocess.run', return_value=Mock(returncode=0, stdout=b'')),
                  patch('check_comm_performance.run_case', side_effect=fake_case)):
                self.assertEqual(performance.main(), 0)
            report = json.loads(destination.read_text())
            self.assertEqual(report['status'], 'PASS')
            self.assertEqual(report['required_mode_run_count'], 20)
            self.assertEqual(len(calls), 20)


if __name__ == '__main__':
    unittest.main()
