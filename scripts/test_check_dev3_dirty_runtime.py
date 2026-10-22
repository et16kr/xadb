"""New dirty-runtime receipt checker inputs; never run product selectors here."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

import check_dev3_dirty_runtime as checker


def fixture():
    return 'id\tgroup\tname\tkind\texpected\n' + ''.join(
        f'{group}-{index:02}\t{group}\tcase_{group}_{index}\tACTUAL\tOK\n'
        for group, count in checker.COUNTS.items() for index in range(1, count + 1))


def transcript(group, ids):
    return ('running 1 test\ntest ' + checker.PREFIX + checker.SELECTORS[group]
            + ' ... DIRTY_CASE\t' + ids[0] + '\n'
            + ''.join('DIRTY_CASE\t' + identifier + '\n' for identifier in ids[1:])
            + ('DIRTY_ORACLE_NEGATIVES 55\n' if group == 'DA07' else '')
            + f'DIRTY_COMPLETE {group} {len(ids)}\nok\n\n'
            + 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; '
              '777 filtered out; finished in 0.01s\n')


class DirtyRuntimeCheckerTests(unittest.TestCase):
    def test_selection_is_only_new_mandatory_groups(self):
        self.assertEqual(checker.select(None), list(checker.SELECTORS))
        self.assertEqual(checker.select(['DA06']), ['DA06'])
        for bad in ([], ['DA01', 'DA01'], ['GA01'], ['DA08'], ['dirty_s5_expanded_history']):
            with self.assertRaises(ValueError):
                checker.select(bad)

    def test_fixture_requires_exact_53_ids_and_expected_columns(self):
        text = fixture()
        parsed = checker.cases(text)
        self.assertEqual(sum(map(len, parsed.values())), 53)
        rows = text.splitlines()
        for bad in ('\n'.join(rows[:-1]), text + rows[1] + '\n',
                    text.replace('DA01-01', 'DA01-00'), text.replace('id\tgroup', 'group\tid'),
                    text.replace('case_DA02_1', 'case_DA01_1'), text.replace('\tACTUAL\tOK', '\t\tOK', 1),
                    text.replace('DA01-01\tDA01', 'DA01-01\tDA02')):
            with self.assertRaises(ValueError):
                checker.cases(bad)

    def test_exact_success_rejects_missing_duplicate_wrong_or_unfinished_cases(self):
        inventory = checker.cases(fixture())
        process = {'exit_code': 0, 'timed_out': False}
        for group, ids in inventory.items():
            text = transcript(group, ids)
            self.assertEqual(checker.evaluate(text, process, group, ids)['case_count'], len(ids))
            for bad in (text.replace(ids[-1], ids[0]), text.replace('DIRTY_CASE\t' + ids[-1] + '\n', ''),
                        text + 'DIRTY_CASE\t' + ids[0] + '\n', text.replace('1 passed', '0 passed'),
                        text.replace('0 failed', '1 failed'), text.replace('0 ignored', '1 ignored'),
                        text.replace(checker.SELECTORS[group], 'old_unselected_test'),
                        text.replace(f'DIRTY_COMPLETE {group} {len(ids)}', ''),
                        text + f'DIRTY_COMPLETE {group} {len(ids)}\n'):
                with self.assertRaises(ValueError):
                    checker.evaluate(bad, process, group, ids)
            for bad in ({'exit_code': 1}, {'exit_code': None}, {'exit_code': 0, 'timed_out': True}):
                with self.assertRaises(ValueError):
                    checker.evaluate(text, bad, group, ids)

    def test_independent_negative_55_requires_unique_exact_marker(self):
        inventory = checker.cases(fixture())
        group = 'DA07'; ids = inventory[group]; text = transcript(group, ids)
        process = {'exit_code': 0, 'timed_out': False}
        self.assertEqual(checker.evaluate(text, process, group, ids)['independent_negatives'], 55)
        for bad in (text.replace('DIRTY_ORACLE_NEGATIVES 55\n', ''),
                    text.replace('NEGATIVES 55', 'NEGATIVES 54'), text + 'DIRTY_ORACLE_NEGATIVES 55\n'):
            with self.assertRaises(ValueError):
                checker.evaluate(bad, process, group, ids)
        with self.assertRaises(ValueError):
            checker.evaluate(transcript('DA01', inventory['DA01']) + 'DIRTY_ORACLE_NEGATIVES 55\n',
                             process, 'DA01', inventory['DA01'])

    def test_manifest_binds_exact_inputs_source_and_selection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            paths = ['crates/engine/src/storage/wal_io/dirty_allocation_tests.rs',
                     'crates/engine/src/storage/wal_io/dirty_allocation_test_support.rs']
            for name in paths:
                path = root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(name)
            manifest = {
                'schema': 'xadb-dev3-dirty-runtime-input-v1', 'mandatory_cases': 53,
                'group_counts': checker.COUNTS, 'normal_fixture': checker.FIXTURE,
                'normal_sha256': 'normal',
                'selectors': {key: checker.PREFIX + value for key, value in checker.SELECTORS.items()},
                'independent_checker': {'named_rejections': 55},
                'implementation_files': {name: checker.digest(root / name) for name in paths},
            }
            checker.verify_manifest(manifest, root, 'normal')
            for key, value in [('mandatory_cases', 52), ('normal_sha256', 'wrong'),
                               ('selectors', {}), ('implementation_files', {}),
                               ('independent_checker', {'named_rejections': 54})]:
                with self.assertRaises(ValueError):
                    checker.verify_manifest({**manifest, key: value}, root, 'normal')
            bad = deepcopy(manifest); bad['implementation_files'][paths[0]] = 'wrong'
            with self.assertRaises(ValueError):
                checker.verify_manifest(bad, root, 'normal')
            bad = deepcopy(manifest); bad['implementation_files']['../outside'] = 'wrong'
            with self.assertRaises(ValueError):
                checker.verify_manifest(bad, root, 'normal')

    def test_build_receipt_requires_original_source_binary_and_no_run_build(self):
        root, binary = Path('/frozen/source'), Path('/frozen/test-binary')
        receipt = {'status': 'PASS', 'source_root': str(root), 'binary': str(binary),
                   'source_sha256': 'source', 'binary_sha256': 'binary',
                   'command': checker.BUILD_COMMAND, 'exit_code': 0}
        checker.verify_build(receipt, root, binary, 'source', 'binary')
        for key, value in [('source_sha256', 'wrong'), ('binary_sha256', 'wrong'),
                           ('source_root', '/wrong'), ('binary', '/wrong'),
                           ('command', ['cargo', 'test']), ('exit_code', 1), ('status', 'FAIL')]:
            with self.assertRaises(ValueError):
                checker.verify_build({**receipt, key: value}, root, binary, 'source', 'binary')


if __name__ == '__main__':
    unittest.main()
