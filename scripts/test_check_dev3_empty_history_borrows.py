"""Reject unrelated compiler failures instead of accepting any failed build."""
import copy
import json
import unittest
from check_dev3_empty_history_borrows import PROBE, verify


class Diagnostics(unittest.TestCase):
    def test_load_exact_six_fields(self):
        spans = [{'file_name': str(PROBE), 'line_start': 5, 'line_end': 5,
                  'column_start': start, 'column_end': end, 'is_primary': True}
                 for start, end in [(24, 32), (34, 39), (44, 53), (62, 67), (69, 76), (78, 81)]]
        rows = [{'reason': 'compiler-message', 'message': {'level': 'error',
                 'code': {'code': 'E0451'}, 'spans': spans}},
                {'reason': 'build-finished', 'success': False}]
        encode = lambda value: '\n'.join(json.dumps(row) for row in value)
        verify(encode(rows), 101, 'sealed_load')
        verify(json.dumps({'reason': 'build-finished', 'success': True}), 0, 'load_sequence')
        for index in range(6):
            changed = copy.deepcopy(rows)
            changed[0]['message']['spans'][index]['column_end'] += 1
            with self.assertRaises(ValueError):
                verify(encode(changed), 101, 'sealed_load')
        with self.assertRaises(ValueError):
            verify(encode(rows), 101, 'sealed_history')

    def test_exact_private_fields_and_reject_drift(self):
        spans = [{'file_name': str(PROBE), 'line_start': 4, 'line_end': 4,
                  'column_start': start, 'column_end': end, 'is_primary': True}
                 for start, end in [(15, 22), (38, 43)]]
        base = [{'reason': 'compiler-message', 'message': {'level': 'error',
                 'code': {'code': 'E0451'}, 'spans': spans}},
                {'reason': 'build-finished', 'success': False}]
        encode = lambda rows: '\n'.join(json.dumps(row) for row in rows)
        verify(encode(base), 101, 'sealed_history')
        verify(json.dumps({'reason': 'build-finished', 'success': True}), 0, 'flush_sequence')
        for variant in range(8):
            rows = copy.deepcopy(base)
            error = rows[0]['message']
            if variant == 0:
                error['code']['code'] = 'E0603'
            elif variant == 1:
                error['spans'][0]['file_name'] = 'other.rs'
            elif variant == 2:
                error['spans'][0]['line_start'] = 5
            elif variant == 3:
                error['spans'][0]['column_end'] = 23
            elif variant == 4:
                error['spans'].pop()
            elif variant == 5:
                rows.insert(0, copy.deepcopy(rows[0]))
            elif variant == 6:
                rows.pop()
            else:
                rows[1]['success'] = True
            with self.assertRaises(ValueError, msg=str(variant)):
                verify(encode(rows), 101, 'sealed_history')
        with self.assertRaises(ValueError):
            verify(encode(base), 0, 'sealed_history')
        with self.assertRaises(ValueError):
            verify(encode(base), 101, 'flush_sequence')


if __name__ == '__main__':
    unittest.main()
