"""The D2-frozen 31 checker counterexamples, separate from product evidence."""
from copy import deepcopy
from pathlib import Path
import unittest

from dev3_doublewrite_model import (inputs, objects, expected_trace, check_trace,
                                   durable_images, check_images, check_observations,
                                   storage_objects)
from dev3_doublewrite_native import check_census, check_receipt

ROOT = Path(__file__).resolve().parent.parent


class DoublewriteCheckerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.frozen = inputs(ROOT)

    def test_seven_trace_mutations(self):
        expected = expected_trace(self.frozen['effects.json'], objects(ROOT, 1), 1)
        check_trace(expected, expected)
        cases = self.frozen['checker-negatives.json']['trace']
        self.assertEqual(len(cases), 7)
        for key in cases:
            value = deepcopy(expected)
            error = 'FAIL_EFFECT_ORDER'
            if key == 'missing_event':
                value.pop(0)
                error = 'FAIL_EFFECT_CENSUS'
            elif key == 'duplicate_event':
                value.insert(0, deepcopy(value[0]))
                error = 'FAIL_EFFECT_CENSUS'
            elif key == 'wrong_order':
                value[0], value[1] = value[1], value[0]
            elif key == 'wrong_name':
                value[1]['name'] = 'd3-profile'
            elif key == 'wrong_offset':
                value[1]['offset'] += 1
            elif key == 'wrong_length':
                value[1]['length'] -= 1
            elif key == 'wrong_digest':
                value[1]['sha256'] = '0' * 64
                error = 'FAIL_EFFECT_BYTES'
            else:
                self.fail(key)
            with self.subTest(case=key), self.assertRaisesRegex(ValueError, '^' + error + '$'):
                check_trace(value, expected)

    def test_seven_image_mutations(self):
        expected = self.frozen['durable-images.json']
        check_images(durable_images(self.frozen['effects.json'], objects(ROOT, 1),
                                    storage_objects(ROOT, 1)), expected)
        cases = self.frozen['checker-negatives.json']['images']
        self.assertEqual(len(cases), 7)
        for key in cases:
            value = deepcopy(expected)
            error = 'FAIL_DURABLE_IMAGE'
            if key == 'missing_cut':
                value.pop(0)
                error = 'FAIL_CUT_CENSUS'
            elif key == 'duplicate_cut':
                value.insert(0, deepcopy(value[0]))
                error = 'FAIL_CUT_CENSUS'
            elif key == 'missing_image':
                value[1]['images'].pop()
            elif key == 'extra_image':
                value[1]['images'].append(deepcopy(value[1]['images'][0]))
            elif key == 'wrong_name':
                value[-1]['images'][0][0]['name'] = 'other'
            elif key == 'wrong_length':
                value[-1]['images'][0][0]['length'] += 1
            elif key == 'wrong_digest':
                value[-1]['images'][0][0]['sha256'] = '0' * 64
            else:
                self.fail(key)
            with self.subTest(case=key), self.assertRaisesRegex(ValueError, '^' + error + '$'):
                check_images(value, expected)

    def test_five_observation_mutations(self):
        expected = self.frozen['observations.json']
        check_observations(expected, expected)
        cases = self.frozen['checker-negatives.json']['observations']
        self.assertEqual(len(cases), 5)
        for key in cases:
            value = deepcopy(expected)
            error = 'FAIL_OBSERVATION_ORDER'
            if key == 'missing_observation':
                value.pop(0)
                error = 'FAIL_OBSERVATION_CENSUS'
            elif key == 'wrong_phase':
                value[0]['phase'] = 'readback'
            elif key == 'wrong_name':
                value[0]['name'] = 'other'
            elif key == 'wrong_offset':
                value[-2]['offset'] += 1
            elif key == 'wrong_length':
                value[-2]['length'] += 1
            else:
                self.fail(key)
            with self.subTest(case=key), self.assertRaisesRegex(ValueError, '^' + error + '$'):
                check_observations(value, expected)

    def test_twelve_native_mutations(self):
        old = {'name': 'LOCK', 'length': 0, 'sha256': 'a' * 64, 'device': 1, 'inode': 2, 'links': 1}
        new = {'name': 'd3-dwb-stage', 'length': 16384, 'sha256': 'b' * 64, 'device': 1, 'inode': 3, 'links': 1}
        files = [old, new]
        content = lambda rows: [{k: row[k] for k in ('name', 'length', 'sha256')} for row in rows]
        baseline = {'source_sha256': 'c' * 64, 'binary_sha256': 'd' * 64,
                    'timed_out': False, 'hook_elapsed_ms': 1, 'exit_elapsed_ms': 1,
                    'hook': 4, 'after': True, 'markers': ['XADB_STORAGE_CUT|effect-4'], 'hook_count': 1,
                    'files': files, 'before': [deepcopy(old)]}
        expected = {k: baseline[k] for k in ('source_sha256', 'binary_sha256', 'hook', 'after')}
        expected.update(marker=baseline['markers'][0], files=content(files), initial=content([old]), actual_files=files)
        check_receipt(baseline, expected)
        required = {(i, side) for i in range(1, 26) for side in (False, True)}
        check_census(list(required), required)
        cases = self.frozen['checker-negatives.json']['native']
        self.assertEqual(len(cases), 12)
        for key in cases:
            value, want = deepcopy(baseline), deepcopy(expected)
            keys, error = list(required), None
            if key == 'wrong_source':
                value['source_sha256'] = '0' * 64
                error = 'FAIL_SOURCE_IDENTITY'
            elif key == 'wrong_binary':
                value['binary_sha256'] = '0' * 64
                error = 'FAIL_BINARY_IDENTITY'
            elif key == 'missing_child':
                keys.pop()
                error = 'FAIL_NATIVE_CENSUS'
            elif key == 'duplicate_child':
                keys.append(keys[0])
                error = 'FAIL_NATIVE_CENSUS'
            elif key == 'wrong_hook_index':
                value['hook'] = 3
                error = 'FAIL_HOOK'
            elif key == 'wrong_side':
                value['after'] = False
                error = 'FAIL_HOOK'
            elif key == 'missing_hook':
                value.update(markers=[], hook_count=0)
                error = 'FAIL_HOOK'
            elif key == 'missed_deadline':
                value['hook_elapsed_ms'] = 60001
                error = 'FAIL_TIMEOUT'
            elif key in ('wrong_file_hash', 'wrong_inode', 'wrong_length'):
                field = {'wrong_file_hash': 'sha256', 'wrong_inode': 'inode', 'wrong_length': 'length'}[key]
                value['files'][1][field] = '0' * 64 if field == 'sha256' else 99
                error = 'FAIL_EXTERNAL_IMAGE'
            elif key == 'old_files_changed':
                # Content may be identical, but replacement of the old inode is forbidden.
                value['before'][0]['inode'] = 99
                error = 'FAIL_OLD_FILES'
            else:
                self.fail(key)
            with self.subTest(case=key), self.assertRaisesRegex(ValueError, '^' + error + '$'):
                check_census(keys, required)
                check_receipt(value, want)


if __name__ == '__main__':
    unittest.main()
