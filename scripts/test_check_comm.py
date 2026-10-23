"""Negative tests for implemented COMM evidence checks, not the full C-V16 matrix."""
import unittest
from check_comm import (
    FOUNDATION_REQUIRED_TESTS, INSTRUMENTED_REQUIRED_TESTS, REQUIRED_TESTS,
    check_closed, check_inventory, check_ready, inspect_wire, wire,
)


class CommCheckerTests(unittest.TestCase):
    def test_checksum_endian_oversize_and_truncated_mutants_are_rejected(self):
        valid = wire(b'hello')
        self.assertEqual(inspect_wire(valid), b'hello')
        for data in (valid[:-1], valid+b'0', valid[:10]+bytes([valid[10]^1])+valid[11:], valid[:6]+(5).to_bytes(4,'big')+valid[10:], valid[:6]+(1048577).to_bytes(4,'little')+valid[10:]):
            with self.assertRaises(ValueError):
                inspect_wire(data)

    def test_missing_test_and_ignored_test_cannot_pass_inventory(self):
        valid = '\n'.join(f'test {name} ... ok' for name in sorted(REQUIRED_TESTS))
        check_inventory(valid)
        with self.assertRaises(ValueError):
            check_inventory(valid.replace(' ... ok', ' ... ignored', 1))
        with self.assertRaises(ValueError):
            check_inventory(valid+'\ntest result: FAILED')

    def test_all_features_and_foundation_peak_inventories_are_separate_and_required(self):
        instrumented = '\n'.join(f'test {name} ... ok' for name in sorted(INSTRUMENTED_REQUIRED_TESTS))
        check_inventory(instrumented, INSTRUMENTED_REQUIRED_TESTS)
        with self.assertRaises(ValueError):
            check_inventory(instrumented, INSTRUMENTED_REQUIRED_TESTS | {'two_connections_two_slots_depth_eight_exhaustive_model'})
        foundation = '\n'.join(f'test {name} ... ok' for name in sorted(FOUNDATION_REQUIRED_TESTS))
        check_inventory(foundation, FOUNDATION_REQUIRED_TESTS)

    def test_model_terminal_precedence_regressions_are_required(self):
        names = {
            'exploration::oracle_regression::open_full_ticket_rejection_retains_capacity_witness',
            'exploration::oracle_regression::cancelled_and_draining_full_tickets_preserve_gate_errors',
            'exploration::oracle_regression::terminal_full_tickets_keep_causes_and_later_cancel_precedence',
        }
        self.assertTrue(names <= INSTRUMENTED_REQUIRED_TESTS)
        self.assertTrue(names.isdisjoint(REQUIRED_TESTS))
        rows = [f'test {name} ... ok' for name in sorted(INSTRUMENTED_REQUIRED_TESTS)]
        check_inventory('\n'.join(rows), INSTRUMENTED_REQUIRED_TESTS)
        for name in names:
            with self.assertRaises(ValueError):
                check_inventory('\n'.join(row for row in rows if row != f'test {name} ... ok'),
                                INSTRUMENTED_REQUIRED_TESTS)
            with self.assertRaises(ValueError):
                check_inventory('\n'.join(rows).replace(f'test {name} ... ok',
                                                       f'test {name} ... ignored'),
                                INSTRUMENTED_REQUIRED_TESTS)

    def test_ordered_model_dedup_regression_is_required(self):
        name = 'ordered_layer_dedup_preserves_exact_hash_reference'
        self.assertIn(name, INSTRUMENTED_REQUIRED_TESTS)
        self.assertNotIn(name, REQUIRED_TESTS)
        rows = [f'test {item} ... ok' for item in sorted(INSTRUMENTED_REQUIRED_TESTS)]
        check_inventory('\n'.join(rows), INSTRUMENTED_REQUIRED_TESTS)
        with self.assertRaises(ValueError):
            check_inventory('\n'.join(row for row in rows if row != f'test {name} ... ok'),
                            INSTRUMENTED_REQUIRED_TESTS)
        with self.assertRaises(ValueError):
            check_inventory('\n'.join(rows).replace(f'test {name} ... ok',
                                                   f'test {name} ... ignored'),
                            INSTRUMENTED_REQUIRED_TESTS)
    def test_wrong_binary_identity_missing_ready_and_peer_history_fail(self):
        valid = {'event':'ready','source_sha256':'abc','pid':42,'profile':'debug'}
        check_ready(valid, 'abc', 42)
        for patch in ({'event':'closed'},{'source_sha256':'bad'},{'pid':43},{'profile':'release'}):
            with self.assertRaises(ValueError):
                check_ready(valid | patch, 'abc', 42)
        closed = {'event':'closed','reason':'Closed','payload_bytes':0,'connections':0,'frames':7,'flushed':7,'failed':0}
        check_closed(closed, 'echo', 7)
        for patch in ({'payload_bytes':1},{'connections':1},{'frames':0},{'flushed':6},{'event':'missing'},{'reason':'Cancelled'}):
            with self.assertRaises(ValueError):
                check_closed(closed | patch, 'echo', 7)


if __name__ == '__main__':
    unittest.main()
