import unittest
from check_comm_soak import TARGETS, check_soak


def thread_receipt(rounds):
    drain_rounds = (rounds + 1) // 3
    non_drain_rounds = rounds - drain_rounds
    attempts = 16 * rounds
    accepted = 5 * rounds
    backpressured = 4 * rounds
    rejected = 7 * rounds
    race_accepted = rounds
    race_rejected = 3 * rounds
    postclose_rejected = 4 * rounds
    leases = 2 * rounds
    flushed = 4 * rounds + drain_rounds
    failed = non_drain_rounds
    return (
        f'test {TARGETS["threads"]["test"]} ... ok\n'
        f'COMM_THREAD_COUNTS target=threads rounds={rounds} attempts={attempts} accepted={accepted} '
        f'backpressured={backpressured} rejected={rejected} race_accepted={race_accepted} '
        f'race_rejected={race_rejected} postclose_rejected={postclose_rejected} leases={leases} '
        f'flushed={flushed} failed={failed}\n'
        f'COMM_SOAK_COMPLETE target=threads elapsed_ms=1800001 iterations={attempts} '
        'seed=20261019 corpus=8 crashes=0\n'
    )


class SoakCheckerTests(unittest.TestCase):
    def test_internal_elapsed_cannot_replace_observed_wall_time_or_work(self):
        test = TARGETS['codec']['test']
        valid = f'test {test} ... ok\nCOMM_SOAK_COMPLETE target=codec elapsed_ms=1800001 iterations=10000 seed=20261019 corpus=13 crashes=0\n'
        check_soak(valid, 0, 'codec', 1800.1)
        for output, code, seconds in [(valid, 0, 1), (valid, -9, 1800.1),
                                      (valid.replace('iterations=10000', 'iterations=0'), 0, 1800.1),
                                      (valid.replace('crashes=0', 'crashes=1'), 0, 1800.1),
                                      (valid.replace('corpus=13', 'corpus=12'), 0, 1800.1),
                                      (valid.replace('elapsed_ms=1800001', 'elapsed_ms=1799999'), 0, 1800.1),
                                      (valid + valid, 0, 1800.1)]:
            with self.assertRaises(ValueError):
                check_soak(output, code, 'codec', seconds)

    def test_event_kind_coverage_must_be_complete_and_match_operations(self):
        counts = [384] * 25 + [400]
        valid = (
            f'test {TARGETS["events"]["test"]} ... ok\n'
            'COMM_EVENT_COUNTS target=events counts=' + ','.join(map(str, counts)) + '\n'
            'COMM_SOAK_COMPLETE target=events elapsed_ms=1800001 iterations=10000 '
            'seed=20261019 corpus=8 crashes=0\n'
        )
        check_soak(valid, 0, 'events', 1800.1)
        missing = valid.replace('COMM_EVENT_COUNTS target=events counts=', 'COMM_EVENT_COUNTS target=events counts=0,')
        repeated = valid + valid
        wrong_sum = valid.replace('iterations=10000', 'iterations=10001')
        for output in [missing, repeated, wrong_sum]:
            with self.assertRaises(ValueError):
                check_soak(output, 0, 'events', 1800.1)

    def test_event_iteration_counter_wrap_is_reconstructed_from_all_kind_totals(self):
        counts = [100_000_000] * 26
        total = sum(counts)
        signed_i32 = total - (1 << 32)
        def output(event_counts, reported):
            return (
                f'test {TARGETS["events"]["test"]} ... ok\n'
                'COMM_EVENT_COUNTS target=events counts=' + ','.join(map(str, event_counts)) + '\n'
                f'COMM_SOAK_COMPLETE target=events elapsed_ms=1800001 iterations={reported} '
                'seed=20261019 corpus=8 crashes=0\n'
            )

        legacy = check_soak(output(counts, signed_i32), 0, 'events', 1800.1)
        self.assertEqual(legacy['iterations'], total)
        self.assertEqual(legacy['iterations_reported'], signed_i32)
        self.assertEqual(legacy['iteration_encoding'], 'legacy_i32_wrapped')
        fixed = check_soak(output(counts, total), 0, 'events', 1800.1)
        self.assertEqual(fixed['iteration_encoding'], 'u64_exact')

        i32_boundary = [1] * 25 + [(1 << 31) - 25]
        boundary_reported = -(1 << 31)
        boundary = check_soak(output(i32_boundary, boundary_reported), 0, 'events', 1800.1)
        self.assertEqual(boundary['iterations'], 1 << 31)

        over_u32 = [200_000_000] * 26
        over_u32_total = sum(over_u32)
        self.assertGreater(over_u32_total, 0xFFFF_FFFF)
        wide_fixed = check_soak(output(over_u32, over_u32_total), 0, 'events', 1800.1)
        self.assertEqual(wide_fixed['iterations'], over_u32_total)
        wrapped_ambiguous = ((over_u32_total + (1 << 31)) % (1 << 32)) - (1 << 31)
        for invalid in [
            output(counts, 123),
            output(over_u32, wrapped_ambiguous),
            output([1] * 26, 1 << 64),
        ]:
            with self.assertRaises(ValueError):
                check_soak(invalid, 0, 'events', 1800.1)

    def test_thread_receipt_requires_real_activity_and_terminal_accounting(self):
        for rounds in (640, 641, 642):
            valid = thread_receipt(rounds)
            counts = check_soak(valid, 0, 'threads', 1800.1)['thread_counts']
            self.assertEqual(counts['flushed'], 4 * rounds + (rounds + 1) // 3)
            self.assertEqual(counts['failed'], rounds - (rounds + 1) // 3)
        valid = thread_receipt(640)
        for output in [
            valid.replace('leases=1280', 'leases=0'),
            valid.replace('failed=427', 'failed=426'),
            valid.replace('flushed=2773 failed=427', 'flushed=2774 failed=426'),
            valid.replace('flushed=2773 failed=427', 'flushed=3200 failed=0'),
            valid.replace('iterations=10240', 'iterations=10241'),
            valid.replace('postclose_rejected=2560', 'postclose_rejected=2559'),
            valid.replace('rounds=640', 'rounds=1'),
            valid.replace('COMM_THREAD_COUNTS', 'COMM_THREAD_COUNTS_DUP'),
        ]:
            with self.assertRaises(ValueError):
                check_soak(output, 0, 'threads', 1800.1)


if __name__ == '__main__':
    unittest.main()
