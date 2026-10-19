import unittest
import subprocess
import socket
import sys
import time

from check_comm_performance import (
    FAMILIES,
    MAX_RECEIPT_BYTES,
    Peer,
    PAYLOAD_SIZES,
    add_measurement,
    active_peer_count,
    payload_for_sequence,
    WORKLOADS,
    matrix,
    slow_peer_matrix,
    slow_peer_frame_limit,
    slow_peer_pressure_prefix_minimum,
    percentile,
    read_line,
    summarize_pair_noise,
    summarize_pairs,
    wait_for_cancel_eof,
    wire,
)


def _record(mode, workload="BULK", payload=0, frame_rate=100, byte_rate=0, p99=None, peak=1024):
    cell = {"family": "tcp4", "connections": 1, "payload_bytes": payload, "workload": workload, "slow_peer": False}
    totals = {
        "measured_received_frames_per_second": [frame_rate] * 30,
        "measured_received_payload_bytes_per_second": [byte_rate] * 30,
    }
    return {
        "status": "PASS",
        "pair": 1,
        "cell": cell,
        "instrumentation": mode,
        "worker": {"totals": totals, "diagnostics": {"peak_payload_bytes": peak}},
        "peer": {
            "latency_p99_ns": p99,
            "measured_frames_per_second": [frame_rate] * 30,
            "measured_payload_bytes_per_second": [byte_rate] * 30,
        },
    }


class PerformanceCheckerTests(unittest.TestCase):
    def test_matrix_covers_all_ninety_workload_cells(self):
        cases = matrix()
        self.assertEqual(len(cases), 90)
        self.assertEqual({case["family"] for case in cases}, set(FAMILIES))
        self.assertEqual({case["payload_bytes"] for case in cases}, set(PAYLOAD_SIZES))
        self.assertEqual({case["workload"] for case in cases}, set(WORKLOADS))
        self.assertEqual(len({(case["family"], case["connections"], case["payload_bytes"], case["workload"], case["slow_peer"]) for case in cases}), 90)

    def test_slow_peer_matrix_covers_all_forty_five_echo_cells(self):
        cases = slow_peer_matrix()
        self.assertEqual(len(cases), 45)
        self.assertTrue(all(case["workload"] == "ECHO" and case["slow_peer"] for case in cases))
        self.assertEqual(len({(case["family"], case["connections"], case["payload_bytes"]) for case in cases}), 45)

    def test_slow_peer_burst_exceeds_small_socket_buffers(self):
        self.assertEqual(slow_peer_frame_limit(0), 65_536)
        self.assertGreater(slow_peer_frame_limit(64) * 106, 4 * 1024 * 1024)
        self.assertEqual(slow_peer_frame_limit(1_048_576), 8)
        self.assertEqual(slow_peer_pressure_prefix_minimum("tcp4", 1_048_576), 8)
        self.assertEqual(slow_peer_pressure_prefix_minimum("tcp6", 1_048_576), 8)
        self.assertEqual(slow_peer_pressure_prefix_minimum("unix", 1_048_576), 6)

    def test_independent_frame_oracle(self):
        payload = payload_for_sequence(0, 16)
        encoded = wire(payload)
        self.assertEqual(encoded[:10], b"AXF1\x01\x00\x10\x00\x00\x00")
        self.assertEqual(encoded[10:-32], payload)
        self.assertEqual(len(encoded), len(payload) + 42)

    def test_nearest_rank_percentiles(self):
        self.assertEqual(percentile([9, 1, 5, 7], 0.5), 5)
        self.assertEqual(percentile([9, 1, 5, 7], 0.99), 9)
        self.assertIsNone(percentile([], 0.99))

    def test_warmup_frame_does_not_leak_into_measurement_bucket_zero(self):
        peer = Peer(None, 0, "BULK", 10.0, 40.0, 0, True, False)
        add_measurement(peer, 9.75)
        self.assertEqual(sum(peer.frames_per_second), 0)
        add_measurement(peer, 10.0)
        self.assertEqual(peer.frames_per_second[0], 1)

    def test_largest_payload_respects_global_in_flight_budget(self):
        self.assertEqual(active_peer_count(128, 1_048_576, "ECHO"), 11)
        self.assertEqual(active_peer_count(128, 1_048_576, "BULK"), 55)
        self.assertEqual(active_peer_count(128, 1_048_576, "ECHO", slow_peer=True), 3)
        self.assertEqual(active_peer_count(128, 0, "ECHO"), 128)

    def test_receipt_reader_handles_bounded_large_json_and_read_ahead(self):
        source = (
            "import json; "
            "print(json.dumps({'event':'large','data':'x'*20000})); "
            "print(json.dumps({'event':'next'}))"
        )
        process = subprocess.Popen([sys.executable, "-c", source], stdout=subprocess.PIPE)
        try:
            first = read_line(process, 5)
            second = read_line(process, 5)
            self.assertEqual(len(first["data"]), 20000)
            self.assertEqual(second, {"event": "next"})
        finally:
            process.communicate(timeout=5)
        self.assertGreater(MAX_RECEIPT_BYTES, 20000)

    def test_cancel_eof_drains_and_verifies_complete_buffered_echo_frames(self):
        controller, peer = socket.socketpair()
        payload = payload_for_sequence(0, 16)
        peer.sendall(wire(payload))
        peer.close()
        try:
            result = wait_for_cancel_eof(
                controller, time.monotonic(), 1.0, "ECHO", len(payload), True
            )
        finally:
            controller.close()
        self.assertEqual(result["full_echo_frames"], 1)
        self.assertEqual(result["partial_frame_bytes_at_eof"], 0)

    def test_zero_byte_bulk_still_enforces_frame_throughput(self):
        comparisons = summarize_pairs(
            [_record("ALWAYS", frame_rate=100), _record("DETAILED", frame_rate=91)],
            expected_cells=1,
            expected_pairs=1,
        )
        self.assertTrue(comparisons[0]["throughput_guardrail_pass"])
        failed = summarize_pairs(
            [_record("ALWAYS", frame_rate=100), _record("DETAILED", frame_rate=88)],
            expected_cells=1,
            expected_pairs=1,
        )
        self.assertFalse(failed[0]["throughput_guardrail_pass"])

    def test_echo_p99_and_global_quota_guardrails(self):
        records = [
            _record("ALWAYS", workload="ECHO", payload=64, frame_rate=100, byte_rate=6400, p99=100, peak=1024),
            _record("DETAILED", workload="ECHO", payload=64, frame_rate=100, byte_rate=6400, p99=116, peak=2048),
        ]
        comparison = summarize_pairs(records, expected_cells=1, expected_pairs=1)[0]
        self.assertFalse(comparison["p99_guardrail_pass"])
        self.assertTrue(comparison["quota_guardrail_pass"])
        records[1]["worker"]["diagnostics"]["peak_payload_bytes"] = 64 * 1024 * 1024 + 1
        comparison = summarize_pairs(records, expected_cells=1, expected_pairs=1)[0]
        self.assertFalse(comparison["quota_guardrail_pass"])

    def test_missing_or_duplicate_mode_receipts_are_rejected(self):
        with self.assertRaises(ValueError):
            summarize_pairs([_record("ALWAYS")], expected_cells=1, expected_pairs=1)
        with self.assertRaises(ValueError):
            summarize_pairs(
                [_record("ALWAYS"), _record("ALWAYS"), _record("DETAILED")],
                expected_cells=1,
                expected_pairs=1,
            )

    def test_frozen_profile_rejects_a_replaced_cell(self):
        expected = [{"family": "tcp4", "connections": 1, "payload_bytes": 0, "workload": "BULK", "slow_peer": False}]
        unexpected = [_record("ALWAYS"), _record("DETAILED")]
        unexpected[0]["cell"]["family"] = "tcp6"
        unexpected[1]["cell"]["family"] = "tcp6"
        with self.assertRaises(ValueError):
            summarize_pairs(unexpected, 1, 1, expected)

    def test_ten_pair_noise_summary_has_reproducible_confidence_intervals(self):
        records = []
        for pair in range(1, 11):
            always = _record("ALWAYS", workload="ECHO", payload=64, frame_rate=100, byte_rate=6400, p99=100)
            detailed = _record("DETAILED", workload="ECHO", payload=64, frame_rate=99, byte_rate=6336, p99=110)
            always["pair"] = detailed["pair"] = pair
            records.extend((always, detailed))
        comparisons = summarize_pairs(records, 1, 10)
        first = summarize_pair_noise(comparisons, 10)
        second = summarize_pair_noise(comparisons, 10)
        self.assertEqual(first, second)
        self.assertEqual(first[0]["pair_count"], 10)
        self.assertIsNotNone(first[0]["completed_throughput_drop_95_ci"])
        self.assertIsNotNone(first[0]["p99_rise_95_ci"])

    def test_one_connection_slow_peer_marks_performance_guardrails_not_applicable(self):
        records = []
        for pair in range(1, 11):
            always = _record("ALWAYS", workload="ECHO", payload=64, frame_rate=0, byte_rate=0, p99=None)
            detailed = _record("DETAILED", workload="ECHO", payload=64, frame_rate=0, byte_rate=0, p99=None)
            always["cell"]["slow_peer"] = detailed["cell"]["slow_peer"] = True
            always["pair"] = detailed["pair"] = pair
            records.extend((always, detailed))
        comparisons = summarize_pairs(records, 1, 10)
        summary = summarize_pair_noise(comparisons, 10)[0]
        self.assertFalse(summary["guardrails_applicable"])
        self.assertIsNone(summary["completed_throughput_drop_95_ci"])
        self.assertTrue(summary["quota_guardrail_pass"])


if __name__ == "__main__":
    unittest.main()
