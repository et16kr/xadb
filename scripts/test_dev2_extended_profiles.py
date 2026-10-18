"""Reject incomplete or forged fixed kernel/P0 evidence before long execution."""
import copy
import json
import unittest

import check_dev2_extended as gate
import dev2_extended_profiles as profiles


def receipt(spec):
    target = spec['target']
    profile = {'generator': spec['generator']}
    result = {'generator': spec['generator'], 'elapsed_ms': 1800000}
    if 'seed' in spec:
        profile['seed'] = result['seed'] = spec['seed']
    if target != 'p0_bounded_exhaustive':
        profile.update(seed_count=256, min_seconds=1800)
    if target == 'durable_and_fault_codecs_fuzz':
        rounds = 262144
        profile.update(target=target, operations_per_seed=1024, round_cap=1000000000,
                       input_cap=8388612, lanes=copy.deepcopy(profiles.DURABLE_LANES),
                       operators=profiles.DURABLE_OPERATORS, coverage_guided=False,
                       independent_oracle='literal_acceptance_only', fault_transport_caps_included=False)
        operators = dict.fromkeys(profiles.DURABLE_OPERATORS, 0)
        for index in range(rounds):
            operators[profiles.DURABLE_OPERATORS[index % 17]] += 1
        lanes = {}
        for fixed in profiles.DURABLE_LANES:
            valid = len(fixed['valid'])
            known_valid = valid + 3 + operators['valid_identity'] + operators['concatenate']
            known_invalid = fixed['strict_count'] - 3 + sum(operators[name] for name in profiles.DURABLE_OPERATORS[7:])
            cases = valid + fixed['strict_count'] + rounds
            lanes[fixed['lane']] = {'cases': cases, 'accepted': known_valid, 'rejected': cases - known_valid,
                'known_valid': known_valid, 'known_invalid': known_invalid, 'mutations': rounds,
                'variant_visits': dict.fromkeys(fixed['valid'], 1),
                'family_visits': {**fixed['strict_families'], 'valid': valid, 'mutation': rounds},
                'operator_visits': dict(operators), 'seed_visits': [1024] * 256,
                'seed_operator_pairs': 4352, 'seed_operator_min_visits': 60}
        result.update(target=target, seed_count=256, operations_per_seed=1024, rounds=rounds,
                      mutations=1048576, missing_lanes=0, lane_counts=lanes)
    elif target == 'disk_record_fuzz':
        profile.update(operations_per_seed=1024, input_cap=1048618, coverage_guided=False,
                       variants=16, operators=11)
        operators = [0] * 11
        for seed in range(256):
            for case in range(1024):
                operators[(case // 16 + seed) % 11] += 1
        result.update(cases=262144, accepted=1024, rejected=261120, seed_visits=[1024] * 256,
                      variant_visits=[16384] * 16, operator_visits=operators)
    elif target == 'durable_thread_stress':
        profile.update(writer_attempts_per_seed=1024, readers=4, writers=2, power_loss=False)
        result.update(rounds=256, commits=131072, conflicts=131072, reader_row_checks=1573376,
                      sampler_checks=256, sampler_overlap_begin=20, sampler_overlap_end=15,
                      sampler_overlap_both=10, seed_visits=[1] * 256)
    else:
        excluded = 'Insert,Delete,multi-mutation,rollback,UNKNOWN,crash,catalog'
        scope = 'all raw merges classified; checker runs semantic representatives'
        profile.update(profile='exhaustive', initial_states=256, programs_per_actor=13,
            raw_event_schedules=12870, interval_classes=19, linear_orders=70, critical_orders=6,
            expected_cases=9604608, expected_accepted=7003648, expected_rejected=2600960,
            literal_cases=13, max_seconds=7200, excluded=excluded, schedule_scope=scope,
            grammar='12 BeginRW-Replace-Read-Commit;1 BeginRO-Scan1-Scan2-Commit')
        result.update(status='PASS', profile='exhaustive', excluded=excluded, schedule_scope=scope,
            raw_event_schedules=12870, interval_classes=19, critical_orders=6, program_pairs=43264,
            serial_cases=3028480, signature_cases=4932096, mutant_cases=1644032,
            scalar_reduction_checks=3028480, expected_cases=9604608, completed_cases=9604608,
            accepted=7003648, rejected=2600960, invalid=0, incomplete=0, literal_cases=13,
            missing_cases=0, max_states=100, max_transitions=100)
    return profile, result


def output(spec, profile, result):
    prefix = {'durable_and_fault_codecs_fuzz': 'XADB_V3', 'disk_record_fuzz': 'XADB_DISK_FUZZ',
              'durable_thread_stress': 'XADB_KERNEL_THREAD', 'p0_bounded_exhaustive': 'XADB_BOUNDED'}[spec['target']]
    return (f'running 1 test\ntest {spec["test"]} ... {prefix}_PROFILE {json.dumps(profile)}\n'
            f'{prefix}_RESULT {json.dumps(result)}\nok\n'
            'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1800.00s\n')


class ExtendedKernelProfiles(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fixtures = {spec['target']: receipt(spec) for spec in profiles.TARGETS}

    def validate(self, spec, profile, result):
        return gate.validate_receipts(spec, output(spec, profile, result), 1801)

    def test_all_fixed_profiles_pass(self):
        for spec in profiles.TARGETS:
            profile, result = self.fixtures[spec['target']]
            self.assertEqual(self.validate(spec, profile, result)['result'], result)

    def test_short_foreign_duplicate_missing_and_zero_test_receipts_fail(self):
        for spec in profiles.TARGETS:
            profile, result = self.fixtures[spec['target']]
            for changed in ({**result, 'generator': 'foreign'}, {**result, 'elapsed_ms': 1802000}):
                with self.assertRaises(ValueError):
                    self.validate(spec, profile, changed)
            valid = output(spec, profile, result)
            for bad in (valid.replace('running 1 test', 'running 0 tests'),
                        valid.replace('1 passed; 0 failed', '0 passed; 1 failed'),
                        valid.replace('"elapsed_ms": 1800000', '"elapsed_ms": 0, "elapsed_ms": 1800000')):
                with self.assertRaises(ValueError):
                    gate.validate_receipts(spec, bad, 1801)
            if spec['target'] != 'p0_bounded_exhaustive':
                with self.assertRaises(ValueError):
                    self.validate(spec, profile, {**result, 'elapsed_ms': 1799999})

    def test_missing_seed_lane_work_and_known_outcomes_are_rejected(self):
        for spec in profiles.TARGETS:
            profile, result = self.fixtures[spec['target']]
            mutations = []
            if spec['target'] == 'durable_and_fault_codecs_fuzz':
                for field in ('cases', 'mutations', 'known_valid', 'known_invalid', 'seed_operator_pairs', 'seed_operator_min_visits'):
                    changed = copy.deepcopy(result)
                    changed['lane_counts']['durable_request'][field] -= 1
                    mutations.append(changed)
                changed = copy.deepcopy(result)
                changed['lane_counts'].pop('fault_release')
                mutations.append(changed)
                changed = copy.deepcopy(profile)
                changed['lanes'][0]['strict_count'] = 214.0
                with self.assertRaises(ValueError):
                    self.validate(spec, changed, result)
            elif spec['target'] == 'disk_record_fuzz':
                for field in ('seed_visits', 'variant_visits', 'operator_visits'):
                    changed = copy.deepcopy(result)
                    changed[field][0] -= 1
                    changed[field][1] += 1
                    mutations.append(changed)
            elif spec['target'] == 'durable_thread_stress':
                for field, value in (('commits', 131071), ('conflicts', 131071), ('sampler_overlap_both', 0),
                                     ('reader_row_checks', 1), ('seed_visits', [1] * 255)):
                    mutations.append({**result, field: value})
            else:
                for field, value in (('completed_cases', 9604607), ('missing_cases', 1), ('incomplete', 1),
                                     ('program_pairs', 43263), ('accepted', 7003649), ('profile', 'smoke')):
                    mutations.append({**result, field: value})
            for changed in mutations:
                with self.assertRaises(ValueError):
                    self.validate(spec, profile, changed)


if __name__ == '__main__':
    unittest.main()
