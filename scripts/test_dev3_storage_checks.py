"""Execute the frozen checker counterexamples, never database qualification."""
from copy import deepcopy
from pathlib import Path
import unittest

from dev3_storage_model import (inputs, objects, expected_trace, check_trace,
                               durable_images, check_images, sha, DATA)
from dev3_storage_native import check_receipt, check_events

ROOT = Path(__file__).resolve().parent.parent


class StorageCheckerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.frozen = inputs(ROOT)

    def test_seven_trace_mutations(self):
        plan = self.frozen['effects.json']
        expected = expected_trace(plan, objects(ROOT, 1), 1)
        check_trace(expected, expected)
        for case in self.frozen['checker-negatives.json']['trace_mutants']:
            value = deepcopy(expected)
            key = case['id']
            if key == 'T01':
                del value[81]
            elif key == 'T02':
                value[81]['op'] = 'directory_sync'
            elif key == 'T03':
                value[15].update(op='sync', name='d3-bootstrap')
            elif key == 'T04':
                del value[342]
            elif key == 'T05':
                value.insert(85, deepcopy(value[84]))
            elif key == 'T06':
                raw = bytearray(objects(ROOT, 1)[DATA][:16384])
                raw[0] ^= 1
                value[17]['sha256'] = sha(raw)
            elif key == 'T07':
                alternate = ROOT / 'crates/engine/tests/fixtures/dev3-storage-session/alternate/FILEMAP.bin'
                value[346]['sha256'] = sha(alternate.read_bytes())
            else:
                self.fail('unimplemented frozen trace mutation')
            with self.subTest(case=key), self.assertRaisesRegex(ValueError, '^' + case['expected'] + '$'):
                check_trace(value, expected)

    def test_seven_image_mutations(self):
        expected = self.frozen['durable-images.json']
        check_images(durable_images(self.frozen['effects.json'], objects(ROOT, 1)), expected)
        final = next(r for r in expected if r['index'] == 350 and r['side'] == 'after')['images'][0]
        selector = next(f for f in final if f['name'] == 'FILEMAP')
        for case in self.frozen['checker-negatives.json']['image_mutants']:
            value = deepcopy(expected)
            cut = next(r for r in value if r['index'] == case['cut'] and r['side'] == case['side'])
            key = case['id']
            if key == 'I01':
                image = next(v for v in cut['images'] if any(f['name'] == DATA for f in v))
                image[:] = [f for f in image if f['name'] != 'd3-bootstrap']
            elif key == 'I02':
                cut['images'][0].append(deepcopy(selector))
            elif key == 'I03':
                cut['images'][0][:] = [f for f in cut['images'][0] if f['name'] != 'd3-filemap-1']
            elif key == 'I04':
                cut['images'] = [next(v for v in cut['images'] if any(f['name'] == 'FILEMAP' for f in v))]
            elif key == 'I05':
                cut['images'][0].append({**selector, 'name': 'd3-filemap-select-stage'})
            elif key == 'I06':
                file = next(f for f in cut['images'][0] if f['name'] == DATA)
                file['sha256'] = format((int(file['sha256'][0], 16) + 1) % 16, 'x') + file['sha256'][1:]
            elif key == 'I07':
                value.remove(cut)
            else:
                self.fail('unimplemented frozen image mutation')
            with self.subTest(case=key), self.assertRaisesRegex(ValueError, '^' + case['expected'] + '$'):
                check_images(value, expected)

    def test_eleven_native_receipt_mutations(self):
        events = [
            {'at': 0, 'kind': 'create', 'name': 'd3-profile', 'offset': 0, 'length': 0,
             'completed': True, 'injected': None, 'completed_bytes': None},
            {'at': 1, 'kind': 'write', 'name': 'd3-profile', 'offset': 0, 'length': 4096,
             'completed': True, 'injected': None, 'completed_bytes': 4096},
            {'at': 2, 'kind': 'sync', 'name': 'd3-profile', 'offset': 0, 'length': 0,
             'completed': True, 'injected': None, 'completed_bytes': None},
        ]
        check_events(events, self.frozen['effects.json'], 3, True, 'profiles')
        bad = deepcopy(events)
        bad[0]['completed_bytes'] = 0
        with self.assertRaisesRegex(ValueError, '^FAIL_EFFECT_ORDER$'):
            check_events(bad, self.frozen['effects.json'], 3, True, 'profiles')
        files = [{'name': 'd3-filemap-1', 'length': 2113536, 'sha256': 'a' * 64,
                  'device': 1, 'inode': 2, 'links': 1}]
        lock = {'name': 'LOCK', 'regular': True, 'length': 0, 'device': 1, 'inode': 3, 'links': 1}
        value = {'timed_out': False, 'hook_elapsed_ms': 10, 'exit_elapsed_ms': 10,
                 'hook': 15, 'hook_count': 1, 'markers': ['XADB_STORAGE_CUT|descriptor_sync'],
                 'binary_sha256': 'b' * 64, 'source_sha256': 'c' * 64,
                 'fresh_retry_mutations': 0, 'lock': lock, 'files': files}
        expected = {'hook': 15, 'marker': 'XADB_STORAGE_CUT|descriptor_sync', 'mode': 'kill',
                    'binary_sha256': 'b' * 64, 'source_sha256': 'c' * 64,
                    'files': [{k: files[0][k] for k in ('name', 'length', 'sha256')}],
                    'actual_files': files, 'actual_lock': lock}
        check_receipt(value, expected)
        count = 0
        for case in self.frozen['checker-negatives.json']['native_receipt_mutants']:
            for variant in (('absent', 'length') if case['id'] == 'N10' else (None,)):
                candidate, want = deepcopy(value), deepcopy(expected)
                key = case['id']
                if key == 'N01':
                    candidate['hook'] = 14
                elif key == 'N02':
                    candidate['markers'] *= 2
                    candidate['hook_count'] = 2
                elif key == 'N03':
                    candidate.update(timed_out=True, markers=[], hook_count=0)
                elif key == 'N04':
                    candidate['binary_sha256'] = '0' + candidate['binary_sha256'][1:]
                elif key == 'N05':
                    candidate['source_sha256'] = '0' + candidate['source_sha256'][1:]
                elif key == 'N06':
                    candidate.update(hook=82, markers=['XADB_STORAGE_DONE'], status='ERROR_OBSERVED', hit=1, effect='CallerMustResolve')
                    want.update(hook=82, marker='XADB_STORAGE_DONE', mode='fault')
                    check_receipt(candidate, want)
                    candidate['status'] = 'PASS'
                elif key == 'N07':
                    candidate['files'] = []
                elif key == 'N08':
                    candidate['files'].append({'name': 'other', 'length': 0, 'sha256': sha(b''), 'device': 1, 'inode': 4, 'links': 1})
                elif key == 'N09':
                    candidate['ready'] = candidate['resumable'] = True
                elif key == 'N10':
                    if variant == 'absent':
                        candidate['lock'] = None
                    else:
                        candidate['lock']['length'] = 1
                else:
                    self.fail('unimplemented frozen native mutation')
                with self.subTest(case=key, variant=variant), self.assertRaisesRegex(ValueError, '^' + case['expected'] + '$'):
                    check_receipt(candidate, want)
                count += 1
        self.assertEqual(count, 11)


if __name__ == '__main__':
    unittest.main()
