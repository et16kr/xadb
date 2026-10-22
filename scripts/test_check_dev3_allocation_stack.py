"""Known-bad observer receipts; these tests never launch GDB or the product."""
from copy import deepcopy
import json
from pathlib import Path
import unittest
from unittest.mock import patch
import subprocess
import sys
import tempfile
import time

from check_dev3_allocation_stack import PREFIX, TEST, evaluate, wait_process_group


class StackObserverTests(unittest.TestCase):
    def fixture(self):
        records = [
            {'kind': 'pthread_attr_getstack', 'tid': 7, 'rc': 0, 'values': [1048576, 65536]},
            {'kind': 'pthread_attr_getguardsize', 'tid': 7, 'rc': 0, 'values': [4096]},
            {'kind': 'exit', 'code': 0, 'binary': '/retained/elf'},
        ]
        marker = 'RESOURCE_STACK_THREAD tid=7 name=allocation-64k marker=1100000 mapped_bytes=77824 compiled_source=source'
        return records, marker

    def transcript(self, records, marker):
        return ('running 1 test\n' + 'test ' + TEST + ' ... \n' + marker + '\n'
                + '\n'.join(PREFIX + json.dumps(row) for row in records) + '\n'
                + 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 258 filtered out; finished in 0.1s\n')

    def check(self, records, marker, process=None):
        process = process or {'exit_code': 0, 'timed_out': False, 'seconds': 1}
        return evaluate(self.transcript(records, marker), process, Path('/retained/elf'), 'source')

    def test_uses_pthread_attributes_instead_of_vma_length(self):
        records, marker = self.fixture()
        result = self.check(records, marker)
        self.assertEqual(result['pthread_stack_bytes'], 65536)
        self.assertEqual(result['maps_vma_bytes'], 77824)
        self.assertFalse(result['maps_vma_is_stack_size'])

    def test_signal_backtrace_never_becomes_a_passing_receipt(self):
        records, marker = self.fixture()
        diagnostic = {'kind': 'signal', 'name': 'SIGSEGV', 'tid': 7,
                      'frames': [{'index': 0, 'name': 'sha2::sha256::x86::digest_blocks',
                                  'pc': 1234, 'sp': 1044480, 'file': 'src/sha256/x86.rs', 'line': 42}],
                      'frame_limit_reached': False}
        # Even a fabricated successful libtest summary and exit cannot hide a signal.
        for exit_code in (None, 0, 101):
            with self.subTest(exit_code=exit_code):
                changed = deepcopy(records)
                changed[-1]['code'] = exit_code
                changed.insert(-1, diagnostic)
                with self.assertRaisesRegex(ValueError, 'FAIL_INFERIOR_SIGNAL'):
                    self.check(changed, marker)

    def test_rejects_attribute_marker_and_identity_mutations(self):
        records, marker = self.fixture()
        mutations = []
        for index in (0, 1):
            bad = deepcopy(records); del bad[index]
            mutations.append((bad, marker, 'FAIL_ATTRIBUTE_CENSUS'))
            mutations.append((records + [records[index]], marker, 'FAIL_ATTRIBUTE_CENSUS'))
        for index, key, value, code in [
                (0, 'rc', 22, 'FAIL_ATTRIBUTE_ERROR'),
                (0, 'values', [1048576, 131072], 'FAIL_STACK_BUDGET'),
                (0, 'values', [1200000, 65536], 'FAIL_MARKER_INTERVAL'),
                (1, 'values', [0], 'FAIL_STACK_BUDGET'),
                (2, 'binary', '/wrong/elf', 'FAIL_BINARY_IDENTITY'),
                (2, 'code', 101, 'FAIL_INFERIOR_EXIT')]:
            bad = deepcopy(records); bad[index][key] = value
            mutations.append((bad, marker, code))
        mutations.extend([
            (records, marker.replace('compiled_source=source', 'compiled_source=wrong'), 'FAIL_COMPILED_SOURCE'),
            (records, marker.replace('tid=7', 'tid=99'), 'FAIL_ATTRIBUTE_CENSUS'),
            (records, marker + '\n' + marker, 'FAIL_MARKER_CENSUS'),
            (records, '', 'FAIL_MARKER_CENSUS'),
            (records + [{'kind': 'observer_error'}], marker, 'FAIL_OBSERVER_ERROR'),
        ])
        for bad, line, code in mutations:
            with self.subTest(code=code, line=line):
                with self.assertRaisesRegex(ValueError, code):
                    self.check(bad, line)

    def test_interrupt_and_timeout_kill_and_reap_owned_group(self):
        # Real stand-in debugger plus child. No GDB or product test is launched.
        for mode in ('interrupt', 'timeout'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                pid_path = Path(temporary) / 'grandchild.pid'
                code = ("import pathlib,subprocess,sys,time; "
                        "p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']); "
                        "pathlib.Path(sys.argv[1]).write_text(str(p.pid)); time.sleep(30)")
                with subprocess.Popen([sys.executable, '-c', code, str(pid_path)], start_new_session=True) as child:
                    deadline = time.monotonic() + 3
                    while not pid_path.exists() and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertTrue(pid_path.exists())
                    grandchild = int(pid_path.read_text())
                    process = {'timed_out': False}
                    if mode == 'interrupt':
                        original_wait = child.wait
                        calls = 0
                        def interrupted_wait(*args, **kwargs):
                            nonlocal calls
                            calls += 1
                            if calls == 1:
                                raise KeyboardInterrupt()
                            return original_wait(*args, **kwargs)
                        with patch.object(child, 'wait', side_effect=interrupted_wait):
                            with self.assertRaises(KeyboardInterrupt):
                                wait_process_group(child, process, timeout=0.01)
                        self.assertEqual(process['interruption'], 'KeyboardInterrupt')
                    else:
                        wait_process_group(child, process, timeout=0.01)
                        self.assertTrue(process['timed_out'])
                    self.assertEqual(child.returncode, -9)
                    self.assertEqual(process['exit_code'], -9)
                    self.assertTrue(process['process_group_killed'])
                    # A killed reparented descendant may briefly remain a zombie.
                    status = Path(f'/proc/{grandchild}/status')
                    deadline = time.monotonic() + 2
                    while status.exists() and time.monotonic() < deadline:
                        if 'State:\tZ' in status.read_text():
                            break
                        time.sleep(0.01)
                    self.assertTrue(not status.exists() or 'State:\tZ' in status.read_text())

    def test_rejects_missed_test_nonzero_and_timeout(self):
        records, marker = self.fixture()
        for process, code in [
                ({'exit_code': 1, 'timed_out': False, 'seconds': 1}, 'FAIL_GDB_EXIT'),
                ({'exit_code': 0, 'timed_out': True, 'seconds': 30}, 'FAIL_TIMEOUT'),
                ({'exit_code': 0, 'timed_out': False, 'seconds': 31}, 'FAIL_TIMEOUT')]:
            with self.assertRaisesRegex(ValueError, code):
                self.check(records, marker, process)
        for text, code in [
                (self.transcript(records, marker).replace('running 1 test', 'running 0 tests'), 'FAIL_TEST_SELECTION'),
                (self.transcript(records, marker).replace(TEST, 'wrong_test'), 'FAIL_TEST_SELECTION'),
                (self.transcript(records, marker).replace('1 passed;', '0 passed;'), 'FAIL_TEST_RESULT')]:
            with self.assertRaisesRegex(ValueError, code):
                evaluate(text, {'exit_code': 0, 'timed_out': False, 'seconds': 1}, Path('/retained/elf'), 'source')


if __name__ == '__main__':
    unittest.main()
