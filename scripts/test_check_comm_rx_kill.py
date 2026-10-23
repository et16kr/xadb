"""Negative oracles for the new RX-prefix/process-kill receipt boundary."""
import copy
import io
import json
import os
from pathlib import Path
import signal
import tempfile
import unittest
from unittest.mock import Mock, patch

from check_comm_rx_kill import (
    CELLS, MAX_RECEIPT_BYTES, PAYLOAD, PREFIXES, check_identity, check_prefix,
    check_termination, read_receipt, run_cell,
)


class RxKillCheckerTests(unittest.TestCase):
    def setUp(self):
        self.expected = {'fixture': 'comm-rx-prefix-v1', 'family': 'tcp4',
                         'prefix_bytes': PREFIXES['header_partial'], 'nonce': 'a' * 32,
                         'pid': 42, 'source_sha256': 'source', 'profile': 'debug',
                         'target': 'x86_64-unknown-linux-gnu'}

    def receipt(self, stage):
        expected = self.expected | {'prefix_bytes': PREFIXES[stage]}
        return expected, expected | {
            'event': 'rx_prefix', 'phase': 'Open', 'read_bytes': PREFIXES[stage],
            'written_bytes': 0, 'received_frames': int(stage == 'complete'),
            'overflowed': False, 'payload': list(PAYLOAD) if stage == 'complete' else None,
            'registration': {'slot': 1, 'generation': 1},
        }

    def test_all_partial_prefixes_require_exact_read_and_no_public_frame(self):
        for stage in ('header_partial', 'body_partial', 'digest_partial'):
            expected, receipt = self.receipt(stage)
            check_prefix(receipt, expected, stage)
            for patch in ({'read_bytes': PREFIXES[stage] - 1},
                          {'read_bytes': PREFIXES[stage] + 1}, {'received_frames': 1},
                          {'payload': []}, {'phase': 'Closed'}, {'overflowed': True},
                          {'written_bytes': 1}, {'registration': {'slot': 1, 'generation': 0}}):
                with self.subTest(stage=stage, patch=patch), self.assertRaises(ValueError):
                    check_prefix(receipt | patch, expected, stage)
            for key in receipt:
                missing = receipt.copy()
                del missing[key]
                with self.subTest(stage=stage, missing=key), self.assertRaises(ValueError):
                    check_prefix(missing, expected, stage)

    def test_complete_requires_the_independent_payload_and_one_validated_frame(self):
        expected, receipt = self.receipt('complete')
        check_prefix(receipt, expected, 'complete')
        for patch in ({'payload': None}, {'payload': list(PAYLOAD[:-1])},
                      {'payload': list(reversed(PAYLOAD))}, {'received_frames': 0},
                      {'received_frames': 2}, {'payload': [False] + list(PAYLOAD[1:])}):
            with self.subTest(patch=patch), self.assertRaises(ValueError):
                check_prefix(receipt | patch, expected, 'complete')

    def test_receipt_identity_rejects_stale_wrong_source_pid_and_profile(self):
        ready = self.expected | {'event': 'ready'}
        check_identity(ready, self.expected, 'ready')
        for patch in ({'pid': 43}, {'pid': True}, {'source_sha256': 'old'},
                      {'profile': 'release'}, {'nonce': 'b' * 32}, {'family': 'unix'},
                      {'event': 'rx_prefix'}, {'prefix_bytes': 0}, {'target': 'wrong'}):
            with self.subTest(patch=patch), self.assertRaises(ValueError):
                check_identity(ready | patch, self.expected, 'ready')
        missing = copy.copy(ready)
        del missing['nonce']
        with self.assertRaises(ValueError):
                check_identity(missing, self.expected, 'ready')
        for malformed in (None, [], 'ready'):
            with self.subTest(malformed=malformed), self.assertRaises(ValueError):
                check_identity(malformed, self.expected, 'ready')

    def test_missing_signal_reap_order_or_peer_close_cannot_pass(self):
        receipt = {'exit_code': -signal.SIGKILL, 'reaped': True, 'closure': 'EOF',
                   'unexpected_peer_bytes': '', 'prefix_observed_ns': 10, 'signal_sent_ns': 11}
        check_termination(receipt)
        check_termination(receipt | {'closure': 'ConnectionReset'})
        for patch in ({'exit_code': 0}, {'exit_code': -signal.SIGTERM}, {'reaped': False},
                      {'closure': 'Timeout'}, {'unexpected_peer_bytes': '41'},
                      {'signal_sent_ns': 9}, {'prefix_observed_ns': 0}, {'signal_sent_ns': True}):
            with self.subTest(patch=patch), self.assertRaises(ValueError):
                check_termination(receipt | patch)
        for key in receipt:
            missing = receipt.copy()
            del missing[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                check_termination(missing)

    def test_new_inventory_is_individually_selectable_and_has_three_fresh_repetitions(self):
        self.assertEqual(len(CELLS), 36)
        self.assertEqual(len(set(CELLS)), 36)
        for family in ('tcp4', 'tcp6', 'unix'):
            for stage in PREFIXES:
                self.assertEqual({c for c in CELLS if c.startswith(f'{family}/{stage}/')},
                                 {f'{family}/{stage}/{r}' for r in range(3)})

    def test_wrong_binary_missing_or_duplicate_prefix_never_becomes_a_pass(self):
        for fault in ('wrong_binary', 'missing_prefix', 'duplicate_ready', 'trailing_receipt'):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as scratch:
                process = Mock(pid=42)
                process.stdout = io.BytesIO(b'')
                process.poll.return_value = None
                process.wait.return_value = -signal.SIGKILL
                process._comm_line_buffer = b'{}\n' if fault == 'trailing_receipt' else b''
                expected, prefix = self.receipt('header_partial')
                ready = expected | {'event': 'ready', 'tcp': '127.0.0.1:1234', 'unix': None}
                peer = Mock()
                peer.recv.return_value = b''
                entries = [ready, prefix]
                if fault == 'missing_prefix':
                    entries[1] = ValueError('worker exited before receipt')
                elif fault == 'duplicate_ready':
                    entries[1] = ready
                with (patch('check_comm_rx_kill.secrets.token_hex', return_value='a' * 32),
                      patch('check_comm_rx_kill.subprocess.Popen', return_value=process),
                      patch('check_comm_rx_kill.socket.socket', return_value=peer),
                      patch('check_comm_rx_kill.read_receipt', side_effect=entries),
                      patch('check_comm_rx_kill.digest', return_value='wrong' if fault == 'wrong_binary' else 'binary')):
                    result = run_cell(Path('/unused/fixture'), 'source', 'binary',
                                      'tcp4/header_partial/0', Path(scratch) / 'cell')
                self.assertEqual(result['status'], 'FAIL')
                if fault != 'trailing_receipt':
                    process.send_signal.assert_not_called()
                    self.assertTrue(result['cleanup_kill'])
                else:
                    process.send_signal.assert_called_once_with(signal.SIGKILL)
                    self.assertIn('duplicate', result['error'])

    def test_malformed_tcp_endpoints_are_saved_as_failed_cells_before_signal(self):
        for address in (None, 1, [], {}, '', 'no-port', '127.0.0.1:invalid',
                        '127.0.0.1:0', '127.0.0.1:-1', '127.0.0.1:65536'):
            with self.subTest(address=address), tempfile.TemporaryDirectory() as scratch:
                process = Mock(pid=42)
                process.stdout = io.BytesIO(b'')
                process.poll.return_value = None
                expected, _ = self.receipt('header_partial')
                ready = expected | {'event': 'ready', 'tcp': address}
                with (patch('check_comm_rx_kill.secrets.token_hex', return_value='a' * 32),
                      patch('check_comm_rx_kill.subprocess.Popen', return_value=process),
                      patch('check_comm_rx_kill.digest', return_value='binary'),
                      patch('check_comm_rx_kill.read_receipt', return_value=ready)):
                    directory = Path(scratch) / 'cell'
                    result = run_cell(Path('/unused/fixture'), 'source', 'binary',
                                      'tcp4/header_partial/0', directory)
                self.assertEqual(result['status'], 'FAIL')
                self.assertEqual(json.loads((directory / 'validation.json').read_text())['status'], 'FAIL')
                process.send_signal.assert_not_called()
                self.assertTrue(result['cleanup_kill'])

    def test_receipt_reader_preserves_buffered_and_malformed_raw_bytes(self):
        for raw in (b'{"event":"ready"}\n{"event":"rx_prefix"}\n', b'{invalid}\n', b'{"partial":'):
            with self.subTest(raw=raw), tempfile.TemporaryFile('w+b') as log:
                read_fd, write_fd = os.pipe()
                with os.fdopen(read_fd, 'rb', buffering=0) as stream:
                    os.write(write_fd, raw)
                    os.close(write_fd)
                    process = Mock(stdout=stream)
                    process._comm_line_buffer = bytearray()
                    if raw.startswith(b'{"event"'):
                        self.assertEqual(read_receipt(process, log, 1), {'event': 'ready'})
                        self.assertEqual(read_receipt(process, log, 1), {'event': 'rx_prefix'})
                    else:
                        with self.assertRaises(ValueError):
                            read_receipt(process, log, 1)
                    log.seek(0)
                    self.assertEqual(log.read(), raw)

    def test_receipt_reader_bounds_a_worker_that_never_finishes_a_line(self):
        read_fd, write_fd = os.pipe()
        with (os.fdopen(read_fd, 'rb', buffering=0) as stream,
              tempfile.TemporaryFile('w+b') as log):
            os.close(write_fd)
            process = Mock(stdout=stream)
            process._comm_line_buffer = bytearray()
            # Real readiness is EOF; force bounded chunks as a malformed worker
            # stream without filling a pipe or requiring a writer thread.
            with patch('check_comm_rx_kill.os.read', return_value=b'x' * 4096):
                with self.assertRaisesRegex(ValueError, 'bounded protocol'):
                    read_receipt(process, log, 1)
            self.assertLessEqual(log.tell(), MAX_RECEIPT_BYTES + 4096)


if __name__ == '__main__':
    unittest.main()
