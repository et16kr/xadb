"""New INIT harness checks; synthetic artifacts are not product evidence."""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import dev3_status_init as check


def transcript(ids, diagnostics=()):
    rows = [f'test {check.SELECTORS[ids[0]]} ...'] + list(diagnostics)
    for key in ids:
        if key in check.MANDATORY:
            rows.append(check.CASE_MARKER + key)
            if key.startswith(('WI-', 'WM-')):
                rows.append('STATUS_INIT_PROFILE ' + key + ' 1')
        elif key in check.EXPANDED:
            if key.startswith(('SWI-', 'SWM-')):
                case = key[1:3] + '-' + key[-2:]
                rows.extend([check.CASE_MARKER + case, f'STATUS_INIT_PROFILE {case} {key[5]}'])
            rows.append(check.S5_MARKER + key)
    rows.append('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 999 filtered out')
    return '\n'.join(rows) + '\n'


class StatusInitChecks(unittest.TestCase):
    def test_exact_inventory_case_selection_and_aggregate_budget(self):
        build = dict(status='PASS', source_sha256='a' * 64, binary_sha256='b' * 64,
                     source_root=str(check.ROOT), command=check.BUILD_COMMAND, exit_code=0, errors=0)
        check.check_build_receipt(build, check.ROOT, 'a' * 64, 'b' * 64)
        for key, value in [('status', 'FAIL'), ('source_sha256', 'c' * 64),
                           ('binary_sha256', 'c' * 64), ('source_root', '/foreign-export'),
                           ('command', [item for item in check.BUILD_COMMAND if item != '--no-run']),
                           ('exit_code', 1), ('errors', 1)]:
            with self.assertRaisesRegex(ValueError, 'FAIL_BUILD_BINDING'):
                check.check_build_receipt(dict(build, **{key: value}), check.ROOT, 'a' * 64, 'b' * 64)
        self.assertEqual(len(check.MANDATORY), 93)
        self.assertEqual(len(check.EXPANDED), 116)
        self.assertEqual(check.selections(['CF03-04', 'WI-01']), ['CF03-04', 'WI-01'])
        self.assertEqual(check.jobs(check.selections(['P1', 'P2'])), [['P1'], ['P2']])
        for bad in ([], ['CF01', 'CF01-01'], ['P4']):
            with self.assertRaises(ValueError):
                check.selections(bad)
        self.assertNotIn('SR-P3-C1-INIT', check.EXPANDED)
        ordered = ['CF02-01', 'CF02-03', 'CF02-02']
        check.check_runtime(transcript(ordered), ['CF02-01', 'CF02-02', 'CF02-03'])
        for key in ('CF03-04', 'WI-01', 'SWI-P2-01', 'SWM-P3-08', 'SA-P2-01'):
            good = transcript([key])
            check.check_runtime(good, [key])
            check.check_runtime(good.replace(' ...\n', ' ... ', 1), [key])
            for bad in (good.replace('1 passed', '0 passed'), good.replace(check.SELECTORS[key], 'unrelated'),
                        good + check.CASE_MARKER + 'WI-01\n', good + 'XADB_STATUS_INIT_UNKNOWN yes\n'):
                with self.assertRaises(ValueError):
                    check.check_runtime(bad, [key])
        self.assertEqual(check.remaining_seconds(['CF01-01'], {'mandatory': 119.5}), ('mandatory', .5))
        with self.assertRaises(ValueError):
            check.remaining_seconds(['CF01-01'], {'mandatory': 120})
        self.assertEqual(check.remaining_seconds(['P1'], {'native': 60}), ('native', 30))

    def test_variant_witness_and_frozen_numeric_caps(self):
        variants = [check.VARIANT_MARKER + 'CF11-02 ' + name for name in ('count_mismatch', 'own_pins_flag')]
        good = transcript(['CF11-02'], variants)
        check.check_runtime(good, ['CF11-02'])
        for bad in (good.replace('own_pins_flag', 'count_mismatch'), good.replace(variants[0] + '\n', '')):
            with self.assertRaises(ValueError):
                check.check_runtime(bad, ['CF11-02'])
        closed = [check.VARIANT_MARKER + f'CF12-02 C{i}' for i in range(1, 6)]
        publication = transcript(['CF12-01', 'CF12-02'], closed)
        check.check_runtime(publication, ['CF12-01', 'CF12-02'])
        for bad in (publication + check.VARIANT_MARKER + 'CF12-02 C6\n',
                    publication.replace(closed[-1] + '\n', '')):
            with self.assertRaisesRegex(ValueError, 'FAIL_VARIANT_CENSUS'):
                check.check_runtime(bad, ['CF12-01', 'CF12-02'])
        oracle = transcript(['CF18-01'], [check.ORACLE])
        check.check_runtime(oracle, ['CF18-01'])
        with self.assertRaises(ValueError):
            check.check_runtime(oracle.replace('rejected_known_bad=8', 'rejected_known_bad=7'), ['CF18-01'])
        rows = [f'{category} {name} 1 {cap}' for category, caps in check.runtime_input()['resource_caps'].items()
                if category != 'DIRTY_TYPE' for name, cap in caps.items()]
        resource = transcript(['CF17-01'], rows)
        check.check_runtime(resource, ['CF17-01'])
        for bad in (resource.replace('pool_delta 1 512', 'pool_delta 1 513', 1),
                    resource.replace(rows[0] + '\n', ''), resource + rows[0] + '\n'):
            with self.assertRaises(ValueError):
                check.check_runtime(bad, ['CF17-01'])

    def test_literal_identity_and_input_census(self):
        check.check_inputs()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = check.FIXTURES
            target = root / 'tests/fixtures/dev3-status-init'
            shutil.copytree(source, target)
            manifest = json.loads((target / 'manifest.json').read_text())
            for relative in [row['path'] for row in manifest['input_provenance']] + ['scripts/dev3_status_init_literals.py']:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(check.ROOT / relative, path)
            check.check_inputs(root)
            corrupt = target / 'p1-cancel0-init/work1-init.wal'
            raw = corrupt.read_bytes()
            corrupt.write_bytes(raw[:-1] + bytes([raw[-1] ^ 1]))
            with self.assertRaisesRegex(ValueError, 'FAIL_INPUT_IDENTITY'):
                check.check_inputs(root)
            corrupt.write_bytes(raw)
            (target / 'unregistered').write_bytes(b'')
            with self.assertRaisesRegex(ValueError, 'FAIL_INPUT_CENSUS'):
                check.check_inputs(root)

    def test_native_full_files_ram_events_and_handle_observations(self):
        # Synthetic material from pinned literals exercises external verification;
        # no storage engine function or native worker is executed by this test.
        before, expected, fixture = check.native_expected(3)
        self.assertEqual(len(expected['d3-wal-00000001']), 105696)
        self.assertEqual(expected['d3-bootstrap'][120:128], (2).to_bytes(8, 'little'))
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            db = out / 'database'
            db.mkdir()
            for name, data in expected.items():
                (db / name).write_bytes(data)
            def save(name, values):
                rows = []
                for key in sorted(values):
                    meta = (db / key).stat()
                    rows.append(f'{key}\t{len(values[key])}\t{meta.st_dev}\t{meta.st_ino}\t1\t{hashlib.sha256(values[key]).hexdigest()}')
                (out / name).write_text('\n'.join(rows) + '\n')
            save('before.tsv', before)
            save('after.tsv', expected)
            (out / 'wal-header.bin').write_bytes(before['d3-wal-00000001'])
            (out / 'wal.complete').write_bytes(expected['d3-wal-00000001'])
            for name, source in [('allocator.after', 'work2-allocator.after'), ('status.after', 'work2-status.after'), ('owner.descriptor', 'work2-init.descriptor')]:
                (out / name).write_bytes((fixture / source).read_bytes())
            events = []
            for offset, size in [(4096, 21200), (25296, 21200), (46496, 21200), (67696, 38000)]:
                events.extend([f'write\td3-wal-00000001\t{offset}\t{size}\t{size}', 'sync\td3-wal-00000001\t0\t0\t0'])
            (out / 'events.tsv').write_text('\n'.join(events) + '\n')
            (out / 'resources.tsv').write_text('wrapped_peak\t1\nwrapped_final\t0\nobserved_fd_peak\t3\nobserved_fd_final\t0\n')
            fd_rows = ['baseline\t3'] + [row.split('\t')[0] + '\t3' for row in events] + ['close\t0']
            (out / 'fd-events.tsv').write_text('\n'.join(fd_rows) + '\n')
            self.assertEqual(check.check_native(out, 3)['effects'], 8)
            for malformed in ([fd_rows[0]], fd_rows[:-1], fd_rows + [fd_rows[-1]],
                              [fd_rows[0], fd_rows[2], fd_rows[1], *fd_rows[3:]],
                              [*fd_rows[:-1], 'close\t1']):
                (out / 'fd-events.tsv').write_text('\n'.join(malformed) + '\n')
                with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_FD_SEQUENCE'):
                    check.check_native(out, 3)
            (out / 'fd-events.tsv').write_text('\n'.join(fd_rows) + '\n')
            status = (out / 'status.after').read_bytes()
            (out / 'status.after').write_bytes(bytes(len(status)))
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_RAM_WAL_DESCRIPTOR'):
                check.check_native(out, 3)
            (out / 'status.after').write_bytes(status)
            (out / 'events.tsv').write_text('\n'.join(events[:-1]) + '\n')
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_EFFECTS'):
                check.check_native(out, 3)
            (out / 'events.tsv').write_text('\n'.join(events) + '\n')
            (out / 'fd-events.tsv').write_text('baseline\t3\nopen\t6\n')
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_FD_EVENT'):
                check.check_native(out, 3)
            (db / 'unexpected').write_bytes(b'')
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_FILE_KIND'):
                check.check_native(out, 3)


if __name__ == '__main__':
    unittest.main()
