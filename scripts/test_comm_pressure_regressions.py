"""Controls for the COMM slow-peer prefix, sustained pressure, and failure receipts."""
import copy
import json
from pathlib import Path
import selectors
import socket
import tempfile
import unittest
from unittest.mock import Mock, patch

import check_comm_performance as perf


class SlowPressureTests(unittest.TestCase):
    def test_prefix_probe_uses_remaining_unix_capacity_without_writable_hint(self):
        sender, receiver = socket.socketpair()
        sender.setblocking(False)
        receiver.setblocking(False)
        sender.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 65536)
        selector = selectors.DefaultSelector()
        peer = perf.Peer(sender, 65536, 'ECHO', 0, 30, 0, True, True)
        selector.register(sender, selectors.EVENT_WRITE, peer)
        try:
            # AF_UNIX withdraws POLLOUT before all short-write space is consumed.
            while selector.select(0):
                sender.send(b'x' * 4096)
            self.assertEqual(selector.select(0), [])
            events = perf.select_peer_events(selector, 0, peer)
            self.assertEqual(len(events), 1)
            self.assertIs(events[0][0].data, peer)
            self.assertEqual(events[0][1], selectors.EVENT_WRITE)
            self.assertGreater(sender.send(b'x' * 4096), 0)
        finally:
            selector.close()
            sender.close()
            receiver.close()

    def test_probe_is_bounded_and_never_duplicates_real_readiness(self):
        peer = perf.Peer(None, 0, 'ECHO', 0, 30, 0, True, True)
        key = Mock(data=peer)
        selector = Mock()
        selector.get_key.return_value = key
        selector.select.side_effect = [[], [], [], [(key, selectors.EVENT_WRITE)], []]
        with patch.object(perf.time, 'monotonic', side_effect=[1.0, 1.019, 1.021]):
            self.assertEqual(len(perf.select_peer_events(selector, 0, peer)), 1)
            self.assertEqual(perf.select_peer_events(selector, 0, peer), [])
            self.assertEqual(len(perf.select_peer_events(selector, 0, peer)), 1)
            self.assertEqual(len(perf.select_peer_events(selector, 0, peer)), 1)
            self.assertEqual(perf.select_peer_events(selector, 0, None), [])
        self.assertEqual(selector.get_key.call_count, 2)

    def test_pressure_requires_prefix_backlog_queue_and_continuous_native_stall(self):
        peer = {'socket_frames_fully_written_by_peer': [8], 'slow_peer_burst_stop_reason': 'socket_backpressure'}
        detail = {'pending_echo_backlog_peak': 2, 'ticket_cap_saturations': 1,
                  'peak_tx_queue_bytes': 168, 'peak_stalled_tx_ns': 500_000_000,
                  'would_block': 0}
        perf.validate_slow_pressure(peer, detail, 'unix', 65536)
        for field in ('pending_echo_backlog_peak', 'ticket_cap_saturations', 'peak_tx_queue_bytes', 'peak_stalled_tx_ns'):
            bad = dict(detail, **{field: 0, 'would_block': 100})
            with self.subTest(field=field), self.assertRaises(ValueError):
                perf.validate_slow_pressure(peer, bad, 'unix', 65536)
        with self.assertRaises(ValueError):
            perf.validate_slow_pressure(peer, dict(detail, peak_stalled_tx_ns=499_999_999), 'unix', 65536)
        for bad in (dict(peer, socket_frames_fully_written_by_peer=[7]), dict(peer, slow_peer_burst_stop_reason=None)):
            with self.assertRaises(ValueError):
                perf.validate_slow_pressure(bad, detail, 'unix', 65536)
        # Missing new evidence from a historical worker cannot qualify.
        old = dict(detail)
        del old['peak_stalled_tx_ns']
        with self.assertRaises(ValueError):
            perf.validate_slow_pressure(peer, old, 'unix', 65536)

    def test_failure_preserves_closed_receipt_and_partial_peer_state(self):
        cell = {'family': 'tcp4', 'connections': 1, 'payload_bytes': 0, 'workload': 'ECHO', 'slow_peer': True}
        ready = {'event': 'ready', 'source_sha256': 'source', 'profile': 'release', 'pid': 123,
                 'family': 'tcp4', 'workload': 'ECHO', 'instrumentation': 'ALWAYS',
                 'expected_connections': 2, 'payload_bytes': 0, 'slow_peer': True}
        closed = {'event': 'closed', 'source_sha256': 'source', 'connections': 2,
                  'cleanup': {'payload_bytes': 0, 'connection_count': 0},
                  'totals': {'failed_tickets': 0, 'received_frames': 8, 'received_payload_bytes': 0},
                  'connection_details': [
                      {'connection_index': i, 'received_frames': 8 if i == 0 else 0,
                       'sent_frames': 8 if i == 0 else 0, 'flushed_frames': 8 if i == 0 else 0,
                       'failed_tickets': 0, 'dropped_echo_responses': 0} for i in range(2)]}
        peer_result = {'cancel_target_peer_index': 1, 'socket_frames_fully_written_by_peer': [8, 0],
                       'socket_frames_fully_written': 8, 'socket_payload_bytes_fully_written': 0,
                       'slow_peer_burst_stop_reason': 'frame_limit'}
        peers = [perf.Peer(None, 0, 'ECHO', 0, 30, i, True, i == 0) for i in range(2)]
        peers[0].slow_frames_sent = 7
        process = Mock(pid=123, returncode=0, _comm_line_buffer=b'partial stdout')
        process.communicate.return_value = (b' tail', b'worker diagnostics')
        process.poll.return_value = 0
        cancel_ready = {'event': 'cancel-ready', 'connection_index': 1, 'phase_before': 'Open', 'terminal_error_before': None, 'received_frames': 0}
        cancelled = {'event': 'cancelled', 'connection_index': 1, 'phase_before': 'Open', 'phase_after': 'Closed', 'terminal_error_kind': 'Cancelled'}
        for partial in (False, True):
            with self.subTest(partial=partial), tempfile.TemporaryDirectory() as scratch:
                with (patch.object(perf.subprocess, 'Popen', return_value=process),
                      patch.object(perf, 'connect_peers', return_value=[Mock(), Mock()]),
                      patch.object(perf, 'prepare_peer_window', return_value=(Mock(), peers, 1, [{'send_buffer_bytes': 212992}])),
                      patch.object(perf, 'read_line', side_effect=[ready, {'event': 'armed'}, {'event': 'started'}, cancel_ready, cancelled, closed]),
                      patch.object(perf, 'run_peer_window', side_effect=TimeoutError('prefix stalled') if partial else None, return_value=copy.deepcopy(peer_result)),
                      patch.object(perf, 'wait_for_cancel_eof', return_value={'latency_seconds': .01})):
                    with self.assertRaises((ValueError, TimeoutError)):
                        perf.run_case(Path('/fixture'), Path(scratch)/'case', cell, 'ALWAYS', 'source')
                result = json.loads((Path(scratch)/'case/validation.json').read_text())
                self.assertEqual(result['status'], 'FAIL')
                self.assertEqual(result['worker_pid'], 123)
                self.assertTrue(result['worker_reaped'])
                self.assertEqual(result['worker_ready'], ready)
                self.assertEqual(result['worker'], None if partial else closed)
                self.assertEqual(result['peer_states_at_failure'][0]['slow_frames_sent'], 7)
                self.assertEqual(result['worker_stdout_tail'], 'partial stdout tail')
                self.assertEqual(result['socket_settings'], [{'send_buffer_bytes': 212992}])

    def test_tcp_slow_offer_resumes_after_transient_write_pressure(self):
        from types import SimpleNamespace

        class Clock:
            now = 0.0
            def monotonic(self):
                self.now += 0.05
                return self.now

        for resume in (True, False):
            with self.subTest(resume=resume):
                clock = Clock()
                slow_socket = Mock(family=socket.AF_INET)
                slow = perf.Peer(slow_socket, 0, 'ECHO', 0, 30, 0, True, True)
                helper = perf.Peer(Mock(family=socket.AF_INET), 0, 'ECHO', 0, 30, 1, False, False)
                helper.cancel_target = True
                selector = Mock()
                registered = {}
                pause_at = None
                def register(sock, events, peer):
                    registered[id(sock)] = SimpleNamespace(data=peer)
                def key(sock):
                    if id(sock) not in registered:
                        raise KeyError(sock)
                    return registered[id(sock)]
                def select(timeout):
                    nonlocal pause_at
                    clock.now += timeout
                    if id(slow_socket) not in registered:
                        return []
                    if slow.slow_frames_sent >= 8:
                        if pause_at is None:
                            pause_at = clock.now
                        if not resume or clock.now - pause_at < 1.0:
                            return []
                    return [(key(slow_socket), selectors.EVENT_WRITE)]
                selector.register.side_effect = register
                selector.get_key.side_effect = key
                selector.unregister.side_effect = lambda sock: registered.pop(id(sock))
                selector.select.side_effect = select
                slow_socket.send.side_effect = len
                snapshot = {'cpu_ticks': 0, 'rss_high_water_kib': 1}
                with (patch.object(perf.time, 'monotonic', side_effect=clock.monotonic),
                      patch.object(perf.time, 'monotonic_ns', side_effect=lambda: int(clock.now*1e9)),
                      patch.object(perf, 'proc_snapshot', return_value=snapshot),
                      patch.object(perf, 'slow_peer_frame_limit', return_value=16)):
                    result = perf.run_peer_window((selector, [slow, helper], 1, []), 'ECHO', 0, 123, 0.0, True)
                self.assertEqual(result['slow_peer_offered_frames'], 16 if resume else 8)
                self.assertEqual(result['slow_peer_burst_stop_reason'], 'frame_limit' if resume else 'socket_backpressure')
                self.assertTrue(slow.closed)
                self.assertFalse(helper.closed)

    def test_only_slow_tcp_peer_negotiates_bounded_segments(self):
        for family, host, name in ((socket.AF_INET, '127.0.0.1', 'tcp4'),
                                   (socket.AF_INET6, '::1', 'tcp6')):
            with self.subTest(family=name), socket.socket(family) as listener:
                listener.bind((host, 0))
                listener.listen(4)
                port = listener.getsockname()[1]
                ready = {'tcp': f'[{host}]:{port}' if family == socket.AF_INET6 else f'{host}:{port}'}
                native_socket = socket.socket
                def make_socket(*args, **kwargs):
                    return Mock(wraps=native_socket(*args, **kwargs))
                with patch.object(perf.socket, 'socket', side_effect=make_socket):
                    peers = perf.connect_peers(name, ready, 2, slow_peer=True)
                accepted = [listener.accept()[0] for _ in peers]
                try:
                    # Checking both ends proves MSS was advertised before connect.
                    for endpoint in (peers[0], accepted[0]):
                        segment = endpoint.getsockopt(socket.IPPROTO_TCP, socket.TCP_MAXSEG)
                        self.assertGreater(segment, 0)
                        self.assertLessEqual(segment, 1460)
                    for endpoint in (peers[1], accepted[1]):
                        self.assertGreater(endpoint.getsockopt(socket.IPPROTO_TCP, socket.TCP_MAXSEG), 1460)
                    for peer in peers:
                        # MSS also affects Linux's default buffer sizing; prove
                        # no override rather than assuming a fixed automatic size.
                        self.assertGreater(peer.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF), 0)
                        self.assertFalse(any(call.args[:2] == (socket.SOL_SOCKET, socket.SO_SNDBUF)
                                             for call in peer.setsockopt.call_args_list))
                        self.assertEqual(peer.getsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY), 0)
                finally:
                    for peer in peers + accepted:
                        peer.close()
