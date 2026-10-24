"""Reject unrelated compiler failures instead of accepting any failed build."""
import copy
import json
import unittest
from check_dev3_empty_history_borrows import PROBE, verify


class Diagnostics(unittest.TestCase):
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
