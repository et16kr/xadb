"""Negative and positive cases of the module gate on disposable repositories.

Each case builds a small crate laid out like the real engine (row/insert,
row/read, disk, codec, C) with its own manifest, commits a base and a head,
and judges the head with `check_module_gate.judge`. Export cases derive
removals from the fixture's explicitly selected policy manifest.
"""
import copy
import hashlib
import http.server
import io
import json
import os
import subprocess
import tempfile
import threading
import types
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import check_module_gate as gate
import module_gate_build
import module_single_source

E = 'crates/engine/src/storage'
MEM = f'{E}/wal_io/plan4_recovery/tests/catalog_memory'
MEM_PATH = 'crate::storage::wal_io::plan4_recovery::tests::catalog_memory'

FILES = {
    'Cargo.toml': '[workspace]\n',
    'README.md': 'fixture\n',
    'crates/engine/Cargo.toml': '[package]\nname = "xadb-engine"\n',
    'crates/engine/src/lib.rs': 'mod storage;\npub mod api;\n',
    'crates/engine/src/api.rs': 'pub enum Namespace { User }\n',
    f'{E}/mod.rs': 'pub(crate) mod codec;\npub(crate) mod disk;\npub(crate) mod row;\n'
                   'pub(crate) mod c;\nmod wal_io;\n',
    f'{E}/codec.rs': 'use crate::api::Namespace;\npub(crate) struct Page(pub Namespace);\n',
    f'{E}/disk.rs': 'pub(crate) use super::c::Port;\n',
    f'{E}/c.rs': ('pub(crate) struct Port;\n#[cfg(test)]\n#[path = "c_tests.rs"]\nmod tests;\n'),
    f'{E}/c_tests.rs': 'use super::Port;\n// "row::insert" in a comment is not a reference\n',
    f'{E}/row.rs': 'pub(crate) mod insert;\npub(crate) mod read;\n',
    f'{E}/row/insert.rs': ('use crate::storage::{codec::Page, disk::Port};\n'
                           'pub(crate) fn insert(_p: &Page, _q: Port) {}\n'
                           '#[cfg(test)]\nmod tests;\n'),
    f'{E}/row/insert/tests.rs': 'use super::insert;\nconst S: &str = "a // b";\n',
    f'{E}/row/read.rs': ('use crate::storage::disk::Port;\n'
                         'pub(crate) fn read(_q: Port) {}\n'),
    f'{E}/row/read/driver.rs': '',
    f'{E}/row/insert/driver.rs': '',
    f'{E}/wal_io.rs': 'mod plan4_recovery;\n',
    f'{E}/wal_io/plan4_recovery.rs': '#[cfg(test)]\nmod tests;\n',
    f'{E}/wal_io/plan4_recovery/tests.rs': 'mod catalog_memory;\n',
    f'{E}/wal_io/plan4_recovery/tests/catalog_memory.rs': (
        '#[path = "catalog_memory/row_a.rs"]\nmod row_a;\n'
        '#[path = "catalog_memory/row_b.rs"]\nmod row_b;\n'),
    f'{MEM}/row_a.rs': 'use crate::storage::row::insert::insert;\n',
    f'{MEM}/row_b.rs': 'use crate::storage::row::read::read;\n',
    'tests/fixtures/module-row-insert/a.json': '{}\n',
    'tests/fixtures/module-row-read/b.json': '{}\n',
    'devdoc/contract.md': '<a id="ex"></a>\n',
}
# The real engine has these two files; the fixture's module tree must not
# reach them (they would be outside the tree), so they are created empty and
# then deleted before the base commit in setUp.
FILES.pop(f'{E}/row/read/driver.rs')
FILES.pop(f'{E}/row/insert/driver.rs')


def manifest():
    return {
        'schema': gate.MANIFEST_SCHEMA,
        'crates': [{'root': 'crates/engine/src/lib.rs', 'package': 'xadb-engine'}],
        'modules': [
            {'id': 'A', 'kind': 'feature',
             'logical': ['crate::storage::row::insert', f'{MEM_PATH}::row_a'],
             'files': [f'{E}/row/insert.rs', f'{E}/row/insert/**',
                       'tests/fixtures/module-row-insert/**', f'{MEM}/row_a.rs'],
             'fixture_tokens': ['module-row-insert'], 'allow': ['disk', 'codec', 'values'],
             'allow_test': ['*'], 'ci_selectors': ['storage::row::insert::']},
            {'id': 'B', 'kind': 'feature',
             'logical': ['crate::storage::row::read', f'{MEM_PATH}::row_b'],
             'files': [f'{E}/row/read.rs', f'{E}/row/read/**',
                       'tests/fixtures/module-row-read/**', f'{MEM}/row_b.rs'],
             'fixture_tokens': ['module-row-read'], 'allow': ['disk', 'codec', 'values'],
             'allow_test': ['*'], 'ci_selectors': ['storage::row::read::']},
            {'id': 'REG', 'kind': 'registration', 'files': [f'{E}/row.rs'],
             'logical': ['crate::storage::row'], 'deny': []},
            {'id': 'disk', 'kind': 'common-api', 'files': [f'{E}/disk.rs'],
             'logical': ['crate::storage::disk'], 'deny': []},
            {'id': 'codec', 'kind': 'common', 'files': [f'{E}/codec.rs'],
             'logical': ['crate::storage::codec'], 'allow': ['values'], 'allow_test': ['*']},
            {'id': 'values', 'kind': 'common', 'files': ['crates/engine/src/api.rs'],
             'logical': ['crate::api'], 'deny': []},
            {'id': 'C', 'kind': 'common', 'files': [f'{E}/**'], 'logical': ['crate::storage'],
             'deny': []},
            {'id': 'engine', 'kind': 'common', 'files': ['crates/engine/**'],
             'logical': ['crate'], 'deny': []},
        ],
        'unscoped': [{'files': ['crates/**', 'tests/**', 'scripts/**', 'devdoc/**', '.github/**',
                                'module-gate/**', 'Cargo.toml', 'README.md'],
                      'reason': 'fixture'}],
        'policy_files': [gate.MANIFEST, 'scripts/check_module_gate.py', '.github/workflows/**'],
        'review_required': {'D1': ['crates/**', 'tests/**', 'scripts/**', 'Cargo.toml']},
        'records': 'module-gate/records/*.json',
        'exceptions': [],
        'exports': [],
        'attested': {'allowed_selectors': ['native::big::*'], 'input_roots': ['crates', 'tests']},
        'evidence': {'build_workflow': '.github/workflows/module-build.yml',
                     'artifact': 'module-build-receipt',
                     'head_command': ['cargo', 'test', '--no-run'],
                     'test_command': ['cargo', 'test', '--lib'], 'skip': ['_native_worker']},
    }


def exception(eid, source, target, scope, paths):
    return dict(id=eid, source=source, target=target, scope=scope, paths=paths,
                baseline='0' * 40, contract='contract.md#ex', reason='y', owner='A',
                review_when='z')


EXPORTS = [{'side': 'a', 'keeps': 'A', 'removes': 'B', 'command': ['cargo', 'test', '--no-run']},
           {'side': 'b', 'keeps': 'B', 'removes': 'A', 'command': ['cargo', 'test', '--no-run']}]


class GateFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='xadb-gate-test-')
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name) / 'repo'
        self.repo.mkdir()
        self.git('init', '-q', '-b', 'main')
        self.git('config', 'user.name', 'gate fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'commit.gpgsign', 'false')
        for path, text in FILES.items():
            self.write(path, text)
        self.write(gate.MANIFEST, json.dumps(manifest(), indent=1))
        self.base = self.commit('base')

    def git(self, *args):
        return subprocess.run(['git', *args], cwd=self.repo, check=True, capture_output=True,
                              text=True).stdout.strip()

    def write(self, path, text):
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def edit(self, path, old, new):
        target = self.repo / path
        text = target.read_text()
        self.assertIn(old, text)
        target.write_text(text.replace(old, new, 1))

    def commit(self, message):
        self.git('add', '-A')
        self.git('commit', '-q', '--allow-empty', '-m', message)
        return self.git('rev-parse', 'HEAD')

    def judge(self, head, mode='merge', policy=None, exports=True):
        return gate.judge(str(self.repo), self.base, head, policy, mode, exports)

    def set_manifest(self, change):
        data = json.loads((self.repo / gate.MANIFEST).read_text())
        change(data)
        self.write(gate.MANIFEST, json.dumps(data, indent=1))

    def review(self, head, cls='D1', scopes=(), reviewer='/root/review', author='/root',
               paths=None, name='r1', **extra):
        """Add a record covering base..head as reviewed at `head`."""
        files = gate.review_files(str(self.repo), self.base, head)
        if paths is not None:
            files = {p: b for p, b in files.items() if p in paths}
        record = {'schema': gate.RECORD_SCHEMA, 'change': 'fixture',
                  'reviews': [{'id': name, 'class': cls, 'scopes': list(scopes),
                               'author': author, 'reviewer': reviewer, 'verdict': 'ACCEPT',
                               'required_open': 0, 'files': files, 'log': 'review.md'}]}
        record.update(extra)
        self.write(f'module-gate/records/{name}.json', json.dumps(record, indent=1))
        return self.commit(f'record {name}')

    def assertFinding(self, report, text):
        self.assertEqual(report['status'], 'FAIL', report)
        self.assertTrue(any(text in e for e in report['errors']),
                        f'{text!r} not in {report["errors"]}')


class SourceParsing(unittest.TestCase):
    def test_comments_strings_chars_and_lifetimes(self):
        text = ('let a = "x // y"; // crate::hidden\n'
                "let b = 'c'; fn f<'a>(x: &'a u8) {}\n"
                'let r = r#"crate::raw"#; /* crate::block /* nested */ */ crate::seen\n')
        keep, code = gate.strip_rust(text)
        self.assertNotIn('hidden', keep)
        self.assertIn('x // y', keep)
        self.assertNotIn('raw', code)
        self.assertIn("&'a u8", code)
        self.assertIn('crate::seen', code)
        self.assertNotIn('nested', keep)
        self.assertEqual(len(code), len(text))

    def test_use_tree_expansion(self):
        code = 'use crate::storage::{a::{self, B as C}, d::*, e as _};'
        paths, _ = gate.use_paths(code, 4)
        self.assertEqual(paths, [['crate', 'storage', 'a'], ['crate', 'storage', 'a', 'B'],
                                 ['crate', 'storage', 'd', '*'], ['crate', 'storage', 'e']])


class ModuleGateTests(GateFixture):
    def test_normal_change_with_review_passes(self):
        self.edit(f'{E}/row/insert.rs', 'fn insert(', 'fn insert_row(')
        self.edit(f'{E}/row/insert/tests.rs', 'super::insert', 'super::insert_row')
        self.edit(f'{MEM}/row_a.rs', 'insert::insert', 'insert::insert_row')
        head = self.commit('A change')
        pending = self.judge(head, mode='push', exports=False)
        self.assertEqual(pending['status'], 'PASS', pending)
        self.assertTrue(pending['pending'])  # review is pending at push
        self.assertEqual(sorted(pending['touched']), ['A'])
        self.assertEqual(pending['selection']['selectors'], ['storage::row::insert::'])
        head = self.review(head)
        report = self.judge(head, exports=False)
        self.assertEqual(report['status'], 'PASS', report)

    def test_feature_dependency_on_the_other_feature_is_forbidden(self):
        self.edit(f'{E}/row/insert.rs', 'pub(crate) fn insert',
                  'pub(crate) fn peek() { crate::storage::row::read::read(Port) }\n'
                  'pub(crate) fn insert')
        report = self.judge(self.review(self.commit('A reads B')), exports=False)
        self.assertFinding(report, 'forbidden dependency A->B (production)')

    def test_relative_and_ident_paths_resolve(self):
        self.write(f'{E}/row/read.rs', 'use super::insert;\npub(crate) fn read() {}\n')
        report = self.judge(self.commit('B uses super::insert'), exports=False)
        self.assertFinding(report, 'forbidden dependency B->A (production)')

    def test_lower_layer_back_reference_is_forbidden(self):
        self.edit(f'{E}/codec.rs', 'pub(crate) struct',
                  'pub(crate) fn port() -> crate::storage::c::Port { todo!() }\npub(crate) struct')
        report = self.judge(self.commit('codec reaches C'), mode='push', exports=False)
        self.assertFinding(report, 'forbidden dependency codec->C (production)')

    def test_common_code_must_not_reach_features(self):
        self.edit(f'{E}/c.rs', 'pub(crate) struct Port;',
                  'pub(crate) struct Port;\npub(crate) use super::row::insert::insert;')
        report = self.judge(self.commit('C re-exports A'), mode='push', exports=False)
        self.assertFinding(report, 'forbidden dependency C->A (production)')

    def test_opposite_fixture_mixed_into_a_test(self):
        self.edit(f'{E}/row/insert/tests.rs', '"a // b"', '"tests/fixtures/module-row-read/b.json"')
        report = self.judge(self.commit('A test reads B fixture'), mode='push', exports=False)
        self.assertFinding(report, 'forbidden dependency A->B (test)')

    def test_unregistered_path_and_file_outside_the_module_tree(self):
        self.write('newroot/x.txt', 'x\n')
        self.write(f'{E}/row/insert/orphan.rs', 'pub fn f() {}\n')
        report = self.judge(self.commit('unregistered'), mode='push', exports=False)
        self.assertFinding(report, 'unregistered path newroot/x.txt')
        self.assertFinding(report, 'crate file outside the module tree: '
                                   f'{E}/row/insert/orphan.rs')

    def test_missing_review_blocks_merge_and_review_must_match_head_blob(self):
        self.edit(f'{E}/codec.rs', 'struct Page', 'struct Page /* changed */')
        head = self.commit('codec change')
        self.assertFinding(self.judge(head, exports=False),
                           f'review missing: {E}/codec.rs needs D1')
        reviewed = self.review(head, na=[{'module': 'codec', 'reason': 'comment only'}])
        report = self.judge(reviewed, exports=False)
        self.assertEqual(report['status'], 'PASS', report)
        self.edit(f'{E}/codec.rs', '/* changed */', '/* changed after review */')
        moved = self.commit('change after review')
        self.assertFinding(self.judge(moved, exports=False), f'review missing: {E}/codec.rs')

    def test_file_defining_a_common_api_reexport_needs_d2_common(self):
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port; // changed')
        head = self.commit('C change behind disk')
        report = self.judge(self.review(head, ci_selectors=['storage::c::']), exports=False)
        self.assertFinding(report, f'review missing: {E}/c.rs needs D2 common')
        self.assertTrue(any('c.rs defines what disk re-exports' in n for n in report['notes']))
        report = self.judge(self.review(head, cls='D2', scopes=('common',), name='r2',
                                        ci_selectors=['storage::c::']), exports=False)
        self.assertEqual(report['status'], 'PASS', report)
        self.assertIn('storage::c::', report['selection']['selectors'])

    def test_self_review_and_rejected_verdict_do_not_count(self):
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port; // x')
        head = self.review(self.commit('C change'), reviewer='/root', author='/root')
        report = self.judge(head, exports=False)
        self.assertFinding(report, 'reviewer missing or the author')
        self.assertFinding(report, 'review missing')

    def test_joint_and_common_changes_need_d2_scopes(self):
        self.edit(f'{E}/row/insert.rs', 'fn insert(', 'fn insert2(')
        self.edit(f'{E}/row/insert/tests.rs', 'super::insert', 'super::insert2')
        self.edit(f'{MEM}/row_a.rs', 'insert::insert', 'insert::insert2')
        self.edit(f'{E}/row/read.rs', 'fn read(', 'fn read2(')
        self.edit(f'{MEM}/row_b.rs', 'read::read', 'read::read2')
        self.edit(f'{E}/disk.rs', 'Port;', 'Port; // api')
        head = self.commit('joint')
        self.assertFinding(self.judge(self.review(head), exports=False),
                           f'{E}/row/read.rs needs D2 joint')
        report = self.judge(self.review(head, cls='D2', scopes=('joint', 'common'), name='r2',
                                        na=[{'module': 'disk', 'reason': 'comment only'}]),
                            exports=False)
        self.assertEqual(report['status'], 'PASS', report)

    def test_rule_only_relaxation_needs_a_d2_policy_review(self):
        self.edit(f'{E}/row/insert.rs', 'pub(crate) fn insert',
                  'pub(crate) fn peek() { crate::storage::row::read::read(Port) }\n'
                  'pub(crate) fn insert')
        self.set_manifest(lambda m: m['exceptions'].append(exception(
            'EX1', f'{E}/row/insert.rs', 'B', 'production', ['crate::storage::row::read'])))
        head = self.commit('relax and violate')
        report = self.judge(self.review(head), exports=False)
        self.assertFinding(report, f'review missing: {gate.MANIFEST} needs D2 policy')
        self.assertTrue(any('exception EX1 added' in n for n in report['notes']))
        self.assertTrue(any('only under the trusted manifest' in n for n in report['notes']))
        report = self.judge(self.review(head, cls='D2', scopes=('policy',), name='r2'),
                            exports=False)
        self.assertEqual(report['status'], 'PASS', report)

    def test_widening_allow_and_stale_exception(self):
        self.set_manifest(lambda m: m['modules'][4]['allow'].append('C'))
        report = self.judge(self.commit('widen codec'), mode='push', exports=False)
        self.assertTrue(any('codec->C (production) newly allowed (relaxation)' in n
                            for n in report['notes']), report['notes'])
        self.set_manifest(lambda m: m['exceptions'].append(exception(
            'EX2', f'{E}/c.rs', 'A', 'test', ['crate::storage::row::insert'])))
        report = self.judge(self.commit('unused exception'), mode='push', exports=False)
        self.assertFinding(report, 'stale exception EX2')

    def test_gate_policy_files_need_d2(self):
        self.write('.github/workflows/module-build.yml', 'name: fake success\n')
        report = self.judge(self.review(self.commit('fake workflow')), exports=False)
        self.assertFinding(report, '.github/workflows/module-build.yml needs D2 policy')

    def test_invalid_manifests_are_checker_errors(self):
        for change in (lambda m: m['modules'][0].update(deny=[]),
                       lambda m: m['modules'][0]['allow'].append('nope'),
                       lambda m: m.update(exports=[dict(side='a', keeps='A', removes='B',
                                                       command=['sh', '-c', 'true'])])):
            self.git('reset', '-q', '--hard', self.base)
            self.set_manifest(change)
            head = self.commit('bad manifest')
            self.assertFinding(self.judge(head, mode='push', exports=False),
                               'head manifest invalid')
            with self.assertRaises(gate.GateError):
                self.judge(head, mode='push', policy=head, exports=False)
        self.write(gate.MANIFEST, '{not json')
        head = self.commit('corrupt')
        self.assertEqual(gate.main(['check', '--repo', str(self.repo), '--base', self.base,
                                    '--head', head, '--policy', head]), 1)

    def test_not_adopted_without_manifest_at_policy(self):
        (self.repo / gate.MANIFEST).unlink()
        head = self.commit('drop manifest')
        report = self.judge(head, policy=head, exports=False)
        self.assertEqual(report['status'], 'NOT_ADOPTED')
        self.assertFinding(self.judge(head, mode='push', exports=False),
                           f'head deletes {gate.MANIFEST}')

    def test_attested_results_only_for_allowed_selectors_and_current_inputs(self):
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port; // n')
        head = self.commit('C change')
        identity = gate.input_identity(gate.Tree(str(self.repo), head), ['crates', 'tests'])
        good = dict(selector='native::big::case', input_identity=identity, result='PASS',
                    log='artifacts/x.log', log_sha256='0' * 64, reviewed_by='/root/r')
        record = self.review(head, attested=[good, dict(good, selector='storage::small')])
        report = self.judge(record, exports=False)
        self.assertFinding(report, 'storage::small may not be attested')
        self.assertIn('native::big::case', report['attested'])
        self.edit(f'{E}/c.rs', '// n', '// m')
        report = self.judge(self.commit('inputs change'), exports=False)
        self.assertFinding(report, 'native::big::case ran on inputs')


class ExportTests(GateFixture):
    def setUp(self):
        super().setUp()
        self.set_manifest(lambda m: m.update(exports=EXPORTS))
        self.base = self.commit('exports declared')

    def test_declared_exports_pass(self):
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port; // e')
        report = self.judge(self.commit('C change'), mode='push')
        self.assertEqual(report['status'], 'PASS', report)
        self.assertEqual(sorted(report['exports']), ['a', 'b'])
        self.assertEqual(sorted(report['exports']['a']['removed']),
                         sorted([f'{E}/row/read.rs', 'tests/fixtures/module-row-read/b.json',
                                 f'{MEM}/row_b.rs']))

    def test_excessive_or_short_export_removal(self):
        original = module_single_source.source_spec

        def bad_export(data, side):
            spec = original(data, side)
            if side == 'a':
                spec['removed'].append(f'{E}/row/insert/tests.rs')
            else:
                spec['removed'].remove('tests/fixtures/module-row-insert')
            return spec
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port; // e')
        head = self.commit('C change')
        with mock.patch.object(module_single_source, 'source_spec', side_effect=bad_export):
            report = self.judge(head, mode='push')
        self.assertFinding(report, f'export a removed {E}/row/insert/tests.rs of A')
        self.assertFinding(report, 'export b left A file tests/fixtures/module-row-insert/a.json')



class FinalTests(GateFixture):
    def report(self):
        self.set_manifest(lambda m: m.update(exports=EXPORTS))
        self.base = self.commit('exports declared')
        self.edit(f'{E}/row/insert.rs', 'fn insert(', 'fn insert_x(')
        self.edit(f'{E}/row/insert/tests.rs', 'super::insert', 'super::insert_x')
        self.edit(f'{MEM}/row_a.rs', 'insert::insert', 'insert::insert_x')
        head = self.commit('A change')
        return self.judge(head, mode='push')

    def receipt(self, report, **change):
        sel = report['selection']
        results = [dict(name='head-build', status='PASS')]
        results += [dict(name=f'head:{s}', status='PASS', listed=3, passed=3, failed=0)
                    for s in sel['selectors']]
        for export in sel['exports']:
            results.append(dict(name=f'export-{export["side"]}-build', status='PASS'))
            results += [dict(name=f'export-{export["side"]}:{s}', status='PASS', listed=2,
                             passed=2, failed=0) for s in export['selectors']]
        receipt = dict(schema=gate.RECEIPT_SCHEMA, workflow='.github/workflows/module-build.yml',
                       head=report['head'], policy=report['policy'], checker_sha256='c' * 64,
                       manifest_blob=report['manifest_blob'],
                       checker_blob=report['checker_blob'], selection=sel, results=results)
        receipt.update(change)
        return receipt

    def run_for(self, report, **change):
        run = dict(id=7, run_number=3, head_sha=report['head'], event='pull_request',
                   status='completed', conclusion='success',
                   path='.github/workflows/module-build.yml', head_repository={'full_name': 'o/r'})
        run.update(change)
        return run

    def verify(self, report, receipt=None, run=None):
        return gate.verify_receipt(report, receipt or self.receipt(report),
                                   run or self.run_for(report), 'o/r', 'c' * 64)

    def test_matching_receipt_is_accepted(self):
        report = self.report()
        self.assertEqual(self.verify(report), [])

    def test_cases_counted_from_a_selector_must_have_passed_there(self):
        report = self.report()
        case = 'storage::row::insert::tests::newly_added'
        report['selection'] = dict(report['selection'], cases=[case, 'storage::c::tests::z'])

        def receipt(passed_names, covered):
            out = self.receipt(report)
            for r in out['results']:
                if r['name'] == 'head:storage::row::insert::':
                    r.update(passed_names=passed_names, listed_names=passed_names,
                             listed=len(passed_names), passed=len(passed_names))
            out['results'].append(dict(name='head-cases', status='PASS', listed=2, passed=2,
                                       failed=0, ignored=0, covered=covered,
                                       listed_names=[case, 'storage::c::tests::z'],
                                       passed_names=[case, 'storage::c::tests::z']))
            return out
        self.assertEqual(self.verify(report, receipt([case], [case])), [])
        for passed_names, covered in (([], [case]), ([case, 'x::y'], [case, 'x::y']),
                                      ([case], 'all')):
            with self.subTest(covered=covered):
                errors = self.verify(report, receipt(passed_names, covered))
                self.assertTrue(any('earlier runs that did not pass' in e for e in errors),
                                errors)

    def test_exports_run_only_the_changed_feature(self):
        """An A change builds both exports but runs no B selector (AGENTS.md:
        unchanged code is not tested); the receipt needs only export-b-build."""
        report = self.report()
        exports = {e['side']: e for e in report['selection']['exports']}
        self.assertEqual(exports['a']['selectors'], ['storage::row::insert::'])
        self.assertEqual(exports['b']['selectors'], [])
        self.assertEqual(exports['b']['cases'], [])
        receipt = self.receipt(report)
        self.assertIn('export-b-build', {r['name'] for r in receipt['results']})
        self.assertEqual(self.verify(report, receipt), [])
        receipt['results'] = [r for r in receipt['results'] if r['name'] != 'export-b-build']
        self.assertTrue(any('lacks export-b-build' in e for e in self.verify(report, receipt)))

    def test_optional_results_may_list_nothing(self):  # RC1
        report = self.report()
        report['selection'] = dict(report['selection'], optional_cases=['c::x', 'c::y'],
                                   optional_selectors=['c::m::'])

        def receipt(cases, selector):
            out = self.receipt(report)
            out['results'] += [dict(name='head-optional-cases', status='PASS',
                                   listed_names=['c::x', 'c::y'][:cases['listed']],
                                   passed_names=['c::x', 'c::y'][:cases['passed']], **cases),
                               dict(name='head?:c::m::', status='PASS', **selector)]
            return out
        none = dict(listed=0, passed=0, failed=0, ignored=0)
        self.assertEqual(self.verify(report, receipt(none, none)), [])
        self.assertEqual(self.verify(report, receipt(dict(none, listed=2, passed=2),
                                                     dict(none, listed=1, passed=1))), [])
        for cases, selector in ((dict(none, listed=3, passed=3), none),
                                (dict(none, listed=1, passed=0, failed=1), none),
                                (none, dict(none, listed=2, passed=1))):
            self.assertTrue(any('every planned case' in e
                                for e in self.verify(report, receipt(cases, selector))))
        missing = self.receipt(report)
        self.assertTrue(any('lacks head-optional-cases' in e
                            for e in self.verify(report, missing)))
        required = self.receipt(report)
        required['results'] = [dict(r, listed=0, passed=0) if r['name'].startswith('head:')
                               else r for r in required['results']]
        report['selection'] = dict(report['selection'], optional_cases=[],
                                   optional_selectors=[])
        self.assertTrue(any('every planned case' in e
                            for e in self.verify(report, required)))

    def test_stale_forged_skipped_and_partial_results_are_rejected(self):
        report = self.report()
        stale = self.verify(report, self.receipt(report, head='f' * 40),
                            self.run_for(report, head_sha='f' * 40))
        self.assertTrue(any('stale result' in e for e in stale))
        other = self.verify(report, run=self.run_for(report, event='push',
                                                     path='.github/workflows/fake.yml'))
        self.assertTrue(any('event/path' in e for e in other))
        fork = self.verify(report, run=self.run_for(report, head_repository={'full_name': 'x/r'}))
        self.assertTrue(any('another repository' in e for e in fork))
        self.assertTrue(any('another checker' in e for e in
                            self.verify(report, self.receipt(report, checker_sha256='d' * 64))))
        receipt = self.receipt(report)
        receipt['selection'] = dict(receipt['selection'], selectors=[])
        self.assertTrue(any('selection differs' in e for e in self.verify(report, receipt)))
        receipt = self.receipt(report)
        receipt['results'] = [r for r in receipt['results'] if r['name'] != 'export-b-build']
        self.assertTrue(any('lacks export-b-build' in e for e in self.verify(report, receipt)))
        receipt = self.receipt(report)
        receipt['results'][1].update(listed=3, passed=2)  # one case skipped, not #[ignore]
        self.assertTrue(any('every planned case' in e for e in self.verify(report, receipt)))
        receipt = self.receipt(report)
        receipt['results'][1].update(listed=0, passed=0)  # selector matched nothing
        self.assertTrue(any('every planned case' in e for e in self.verify(report, receipt)))
        receipt = self.receipt(report)
        receipt['results'][1].update(listed=1, passed=0, ignored=1)  # only #[ignore] cases
        self.assertTrue(any('every planned case' in e for e in self.verify(report, receipt)))
        receipt = self.receipt(report)
        receipt['results'][1].update(listed=3, passed=2, ignored=1)  # an #[ignore] case
        self.assertEqual(self.verify(report, receipt), [])
        receipt = self.receipt(report, manifest_blob='0' * 40)
        self.assertTrue(any('manifest_blob' in e for e in self.verify(report, receipt)))
        receipt = self.receipt(report)
        receipt['results'].append(dict(name='head:extra', status='PASS'))
        self.assertTrue(any('undeclared' in e for e in self.verify(report, receipt)))

    def test_final_rejects_cancelled_missing_and_artifactless_runs(self):
        report = self.report()
        receipt = self.receipt(report, checker_sha256=hashlib.sha256(
            Path(gate.__file__).read_bytes()).hexdigest())
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, 'w') as archive:
            archive.writestr('receipt.json', json.dumps(receipt))
        args = types.SimpleNamespace(repository='o/r', build_workflow='module-build.yml',
                                     artifact='module-build-receipt', wait=0)

        def fetcher(run, artifacts=True):
            def fetch(url, token, raw=False):
                if raw:
                    return buffer.getvalue()
                if url.endswith('/artifacts'):
                    return {'artifacts': [{'name': 'module-build-receipt',
                                           'archive_download_url': 'zip'}] if artifacts else []}
                return {'workflow_runs': [run] if run else []}
            return fetch

        import os
        os.environ['GITHUB_TOKEN'] = 'fixture'
        self.addCleanup(os.environ.pop, 'GITHUB_TOKEN', None)
        ok = gate.final(args, dict(report, notes=[]), fetcher(self.run_for(report)), lambda s: None)
        self.assertEqual(ok, [])
        cancelled = gate.final(args, dict(report, notes=[]),
                               fetcher(self.run_for(report, conclusion='cancelled')), lambda s: None)
        self.assertTrue(any('concluded cancelled' in e for e in cancelled))
        missing = gate.final(args, dict(report, notes=[]), fetcher(None), lambda s: None)
        self.assertTrue(any('not completed' in e for e in missing))
        running = gate.final(args, dict(report, notes=[]),
                             fetcher(self.run_for(report, status='in_progress')), lambda s: None)
        self.assertTrue(any('not completed' in e for e in running))
        bare = gate.final(args, dict(report, notes=[]), fetcher(self.run_for(report), False),
                          lambda s: None)
        self.assertTrue(any('0 module-build-receipt artifacts' in e for e in bare))

    def test_final_fails_closed_without_a_trusted_manifest(self):
        (self.repo / gate.MANIFEST).unlink()
        head = self.commit('drop manifest')
        common = ['--repo', str(self.repo), '--base', self.base, '--head', head,
                  '--policy', head]
        self.assertEqual(gate.main(['check', *common]), 0)
        self.assertEqual(gate.main(['final', *common, '--repository', 'o/r',
                                    '--build-workflow', 'module-build.yml',
                                    '--artifact', 'module-build-receipt', '--wait', '0']), 1)

    def test_planned_cases_must_all_pass(self):
        report = self.report()
        report['selection'] = dict(report['selection'], cases=['storage::row::insert::t::a',
                                                               'storage::row::insert::t::b'])
        receipt = self.receipt(report)
        receipt['results'].append(dict(name='head-cases', status='PASS', listed=2, passed=2,
                                       failed=0, listed_names=report['selection']['cases'],
                                       passed_names=report['selection']['cases']))
        self.assertEqual(self.verify(report, receipt), [])
        receipt['results'][-1].update(listed=1, passed=1)
        self.assertTrue(any('head-cases: listed 1' in e for e in self.verify(report, receipt)))


class ReviewFindingTests(GateFixture):
    """Each case reproduces a finding of the independent D2 review of S12."""

    def test_file_declared_into_another_modules_namespace(self):  # E1, E4
        self.write(f'{E}/row/insert/extra.rs', 'pub(crate) fn extra() {}\n')
        self.edit(f'{E}/c.rs', 'pub(crate) struct Port;\n',
                  'pub(crate) struct Port;\n#[path = "row/insert/extra.rs"]\nmod extra;\n')
        self.edit(f'{E}/row/read.rs', 'pub(crate) fn read',
                  '#[path = "insert.rs"]\nmod borrowed;\npub(crate) fn read')
        report = self.judge(self.commit('declare A files elsewhere'), mode='push', exports=False)
        self.assertFinding(report, f'module mismatch: {E}/row/insert/extra.rs is A\'s but is '
                                   'declared as crate::storage::c::extra (C)')
        self.assertFinding(report, f'module mismatch: {E}/row/insert.rs is A\'s but is declared '
                                   'as crate::storage::row::read::borrowed (B)')

    def test_cfg_test_on_a_field_or_arm_does_not_hide_later_code(self):  # E2
        self.edit(f'{E}/row/read.rs', 'pub(crate) fn read(_q: Port) {}\n',
                  'pub(crate) fn read(_q: Port) {}\n'
                  'pub(crate) struct S {\n    #[cfg(test)]\n    probe: u8,\n}\n'
                  'pub(crate) fn sneak() { crate::storage::row::insert::insert; }\n'
                  'pub(crate) fn pick(x: u8) -> u8 {\n    match x {\n        #[cfg(test)]\n'
                  '        0 => 1,\n        _ => 2,\n    }\n}\n'
                  'pub(crate) fn sneak2() { crate::storage::row::insert::insert; }\n')
        report = self.judge(self.commit('cfg(test) field and arm'), mode='push', exports=False)
        found = [e for e in report['errors'] if 'forbidden dependency B->A (production)' in e]
        self.assertEqual(len(found), 2, report['errors'])

    def test_denied_item_named_in_production_code(self):  # E3
        self.set_manifest(lambda m: m['modules'][4].update(deny_items=['Secret']))
        self.base = self.commit('codec denies Secret')
        self.edit(f'{E}/codec.rs', 'pub(crate) struct Page',
                  '#[cfg(test)]\nfn probe() { let _ = Secret; }\n'
                  'pub(crate) fn leak() -> Secret { todo!() }\npub(crate) struct Page')
        report = self.judge(self.commit('codec names Secret'), mode='push', exports=False)
        found = [e for e in report['errors'] if e.startswith('forbidden item Secret in codec')]
        self.assertEqual(found, [f'forbidden item Secret in codec (production) {E}/codec.rs:4'],
                         report['errors'])

    def test_alias_module_import_and_glob_do_not_hide_an_edge(self):  # E3b, E9
        for body in ('use crate::storage::row as r;\nfn f() { r::insert::insert; }\n',
                     'use crate::storage::row;\nfn f() { row::insert::insert; }\n',
                     'use super::*;\nfn f() { insert::insert; }\n',
                     'use crate::storage::row::*;\nfn f() { insert::insert; }\n'):
            with self.subTest(body=body):
                self.git('reset', '-q', '--hard', self.base)
                self.edit(f'{E}/row/read.rs', 'pub(crate) fn read', body + 'pub(crate) fn read')
                report = self.judge(self.commit('hidden path'), mode='push', exports=False)
                self.assertFinding(report, 'forbidden dependency B->A (production)')

    def test_exception_covers_only_its_paths_and_widening_is_a_relaxation(self):  # E5
        self.edit(f'{E}/row/insert.rs', 'pub(crate) fn insert',
                  'pub(crate) fn peek() { crate::storage::row::read::read(Port) }\n'
                  'pub(crate) fn insert')
        self.set_manifest(lambda m: m['exceptions'].append(exception(
            'EX1', f'{E}/row/insert.rs', 'B', 'production', ['crate::storage::row::read::read'])))
        self.base = self.commit('trusted exception for read')
        self.edit(f'{E}/row/insert.rs', 'pub(crate) fn insert',
                  'pub(crate) fn peek2() { crate::storage::row::read::driver() }\n'
                  'pub(crate) fn insert')
        report = self.judge(self.commit('another B item'), mode='push', exports=False)
        self.assertFinding(report, 'forbidden dependency A->B (production) '
                                   f'{E}/row/insert.rs:3: crate::storage::row::read::driver')
        self.assertFalse(any('row::read::read' in e for e in report['errors']), report['errors'])
        self.git('reset', '-q', '--hard', self.base)
        self.set_manifest(lambda m: m['exceptions'][0].update(paths=['crate::storage::row']))
        report = self.judge(self.commit('widen EX1'), mode='push', exports=False)
        self.assertTrue(any('exception EX1 changed (relaxation)' in n for n in report['notes']))
        self.assertIn('POLICY RELAXATION', report['summary'])

    def test_narrowing_selectors_is_a_relaxation(self):
        self.set_manifest(lambda m: m['modules'][0].update(ci_selectors=[]))
        report = self.judge(self.commit('drop A selectors'), mode='push', exports=False)
        self.assertTrue(any('A.ci_selectors narrowed (relaxation)' in n
                            for n in report['notes']), report['notes'])
        self.assertIn('POLICY RELAXATION', report['summary'])

    def test_b_test_importing_an_a_test_module(self):  # E8
        self.edit(f'{MEM}/row_b.rs', 'use crate::storage::row::read::read;\n',
                  'use crate::storage::row::read::read;\nuse super::row_a::*;\n')
        head = self.commit('B test reuses A test helpers')
        self.assertFinding(self.judge(head, mode='push', exports=False),
                           'forbidden dependency B->A (test)')
        # Without the catalog_memory logical path, row_a would sit in C's
        # namespace; the owner/logical reconciliation reports it.
        self.set_manifest(lambda m: m['modules'][0]['logical'].remove(f'{MEM_PATH}::row_a'))
        report = self.judge(self.commit('forget the logical path'), mode='push', exports=False)
        self.assertFinding(report, f'module mismatch: {MEM}/row_a.rs is A\'s but is declared as '
                                   f'{MEM_PATH}::row_a (C)')

    def test_symlink_under_the_crate_source_root(self):  # E10
        os.symlink('../../../../../../README.md', self.repo / f'{E}/row/insert/link.rs')
        report = self.judge(self.commit('symlink'), mode='push', exports=False)
        self.assertFinding(report, 'symlink or submodule (mode 120000) under a crate source '
                                   f'root: {E}/row/insert/link.rs')

    def test_fixture_only_change_and_ci_also_select_ci(self):  # E7
        self.set_manifest(lambda m: m['modules'][0].update(ci_also=['B']))
        self.base = self.commit('A also runs B')
        self.write('tests/fixtures/module-row-insert/a.json', '{"x": 1}\n')
        report = self.judge(self.commit('fixture only'), mode='push', exports=False)
        self.assertTrue(report['selection']['rust'])
        self.assertEqual(report['selection']['selectors'],
                         ['storage::row::insert::', 'storage::row::read::'])

    def test_touched_module_without_ci_evidence(self):  # E7
        self.edit(f'{E}/codec.rs', 'struct Page', 'struct Page /* x */')
        head = self.commit('codec change')
        missing = 'no CI evidence selected for codec'
        self.assertTrue(any(missing in p for p in
                            self.judge(head, mode='push', exports=False)['pending']))
        self.assertFinding(self.judge(self.review(head), exports=False), missing)
        self_na = self.review(head, reviewer='/root', author='/root', name='r2',
                              na=[{'module': 'codec', 'reason': 'comment only'}])
        self.assertFinding(self.judge(self_na, exports=False), missing)
        (self.repo / 'module-gate/records/r2.json').unlink()
        selected = self.review(head, name='r3', ci_selectors=['storage::codec::'])
        report = self.judge(selected, exports=False)
        self.assertEqual(report['status'], 'PASS', report)
        self.assertEqual(report['selection']['selectors'], ['storage::codec::'])

    def test_record_selector_must_be_a_module_path(self):
        self.edit(f'{E}/codec.rs', 'struct Page', 'struct Page /* x */')
        head = self.review(self.commit('codec change'), ci_selectors=['--include-ignored'])
        self.assertFinding(self.judge(head, mode='push', exports=False),
                           "ci_selector '--include-ignored' is not a module path")

    def test_changed_test_cases_are_selected_exactly(self):
        self.edit(f'{E}/c_tests.rs', 'use super::Port;\n',
                  'use super::Port;\n#[test]\nfn port_case() {}\n'
                  '#[test]\n#[ignore]\nfn slow_case() {}\n'
                  '#[cfg(feature = "x")]\n#[test]\nfn feature_case() {}\n'
                  '#[test]\nfn big_native_worker() {}\n'
                  'mod inner {\n    #[test]\n    fn nested() {}\n}\n')
        report = self.judge(self.commit('C cases'), mode='push', exports=False)
        self.assertEqual(report['selection']['cases'],
                         ['storage::c::tests::inner::nested', 'storage::c::tests::port_case'])
        self.assertFalse(any('no CI evidence' in p for p in report['pending']), report)

    def test_attested_result_required_for_a_module(self):
        self.set_manifest(lambda m: m['attested'].update(required_for={'A': ['native::big::*']}))
        self.base = self.commit('A needs a big native')
        self.write('tests/fixtures/module-row-insert/a.json', '{"x": 2}\n')
        head = self.commit('A fixture')
        report = self.judge(head, mode='push', exports=False)
        self.assertTrue(any('A changed: attested native::big::*' in p
                            for p in report['pending']), report['pending'])
        identity = gate.input_identity(gate.Tree(str(self.repo), head), ['crates', 'tests'])
        attested = [dict(selector='native::big::case', input_identity=identity, result='PASS',
                         log='artifacts/x.log', log_sha256='0' * 64, reviewed_by='/root/r')]
        report = self.judge(self.review(head, attested=attested), exports=False)
        self.assertEqual(report['status'], 'PASS', report)

    def test_gitattributes_export_ignore_is_rejected(self):
        self.write('crates/.gitattributes', 'engine/src/storage/row/read.rs export-ignore\n')
        report = self.judge(self.commit('hide a file from git archive'), mode='push',
                            exports=False)
        self.assertFinding(report, 'crates/.gitattributes: export-ignore/export-subst')

    def test_exception_contract_and_source_must_exist(self):
        self.set_manifest(lambda m: m['exceptions'].append(dict(
            exception('EX3', f'{E}/row/gone.rs', 'B', 'test', ['crate::storage::row::read']),
            contract='contract.md#nowhere')))
        report = self.judge(self.commit('dangling exception'), mode='push', exports=False)
        self.assertFinding(report, f'exception EX3: source {E}/row/gone.rs does not exist')
        self.assertFinding(report, 'exception EX3: contract anchor contract.md#nowhere missing')


class ReReviewFindingTests(GateFixture):
    """Each case reproduces a required finding of the S12 re-review (RR1–RR6)."""

    def test_module_absent_from_the_test_binary_plans_no_case(self):  # RR1
        self.edit(f'{E}/c.rs', 'pub(crate) struct Port;\n',
                  'pub(crate) struct Port;\n'
                  '#[cfg(not(test))]\n#[path = "dual.rs"]\nmod product_view;\n'
                  '#[cfg(test)]\n#[path = "dual.rs"]\nmod test_view;\n')
        self.write(f'{E}/dual.rs', '#[cfg(test)]\nmod tests {\n    #[test]\n'
                                   '    fn dual_case() {}\n}\n')
        report = self.judge(self.commit('one file, two declarations'), mode='push',
                            exports=False)
        self.assertEqual(report['status'], 'PASS', report)
        self.assertEqual(report['selection']['cases'],
                         ['storage::c::test_view::tests::dual_case'])

    def test_definition_behind_a_two_level_reexport_needs_d2_common(self):  # RR2
        self.edit(f'{E}/c.rs', 'pub(crate) struct Port;\n',
                  'pub(crate) use inner::Port;\nmod inner;\n')
        self.write(f'{E}/c/inner.rs', 'pub(crate) struct Port;\n')
        self.base = self.commit('Port defined one level down')
        self.edit(f'{E}/c/inner.rs', 'struct Port;', 'struct Port; // changed')
        head = self.commit('change the defining file')
        report = self.judge(self.review(head, ci_selectors=['storage::c::']), exports=False)
        self.assertFinding(report, f'review missing: {E}/c/inner.rs needs D2 common')
        self.assertTrue(any('c/inner.rs defines what disk re-exports' in n
                            for n in report['notes']), report['notes'])

    def test_executor_only_cases_need_a_reviewed_na(self):  # harness
        self.set_manifest(lambda m: m['evidence'].update(executor_only=[
            {'selector': 'storage::c::tests::needs_executor', 'reason': 'executor inputs'}]))
        self.base = self.commit('an executor-only case')
        self.edit(f'{E}/c_tests.rs', 'use super::Port;\n',
                  'use super::Port;\n#[test]\nfn needs_executor() {}\n#[test]\nfn plain() {}\n')
        head = self.commit('change it')
        report = self.judge(self.review(head), exports=False)
        self.assertEqual(report['selection']['cases'], ['storage::c::tests::plain'])
        self.assertFinding(report, f'{E}/c_tests.rs changes 1 executor-only case(s)')
        named = self.review(head, name='r2', na=[{'module': '-', 'paths': [f'{E}/c_tests.rs'],
                                                  'reason': 'executor run, log at x'}])
        report = self.judge(named, exports=False)
        self.assertFalse(any('executor-only' in e for e in report['errors']), report['errors'])
        self.set_manifest(lambda m: m['evidence'].update(executor_only=[{'selector': '-x'}]))
        report = self.judge(self.commit('a malformed entry'), mode='push', exports=False)
        self.assertFinding(report, 'evidence.executor_only')

    def test_glob_and_module_reexports_need_d2_common(self):  # RC2
        cases = {
            'glob of a glob': (
                lambda: (self.edit(f'{E}/disk.rs', 'use super::c::Port;', 'use super::c::*;'),
                         self.edit(f'{E}/c.rs', 'pub(crate) struct Port;\n',
                                   'pub(crate) use inner::*;\nmod inner;\n'),
                         self.write(f'{E}/c/inner.rs', 'pub(crate) struct Port;\n'))),
            'a module': (
                lambda: (self.edit(f'{E}/disk.rs', 'use super::c::Port;\n',
                                   'use super::c::Port;\npub(crate) use super::c::inner;\n'),
                         self.edit(f'{E}/c.rs', 'pub(crate) struct Port;\n',
                                   'pub(crate) struct Port;\npub(crate) mod inner;\n'),
                         self.write(f'{E}/c/inner.rs', 'pub(crate) struct Extra;\n'))),
            'a renamed module': (
                lambda: (self.edit(f'{E}/disk.rs', 'use super::c::Port;\n',
                                   'use super::c::Port;\npub(crate) use super::c::alias;\n'),
                         self.edit(f'{E}/c.rs', 'pub(crate) struct Port;\n',
                                   'pub(crate) struct Port;\nmod inner;\n'
                                   'pub(crate) use self::inner as alias;\n'),
                         self.write(f'{E}/c/inner.rs', 'pub(crate) struct Extra;\n'))),
            'a module re-exported on the way': (
                lambda: (self.edit(f'{E}/disk.rs', 'use super::c::Port;\n',
                                   'use super::c::Port;\npub(crate) use super::c::inner;\n'),
                         self.edit(f'{E}/c.rs', 'pub(crate) struct Port;\n',
                                   'pub(crate) struct Port;\nmod deep;\n'
                                   'pub(crate) use self::deep::inner;\n'),
                         self.write(f'{E}/c/deep.rs', 'pub(crate) mod inner;\n'),
                         self.write(f'{E}/c/deep/inner.rs', 'pub(crate) struct Extra;\n'))),
        }
        start = self.base
        for text, setup in cases.items():
            with self.subTest(text=text):
                self.git('reset', '-q', '--hard', start)
                setup()
                self.base = self.commit(f'disk re-exports {text}')
                inner = f'{E}/c/deep/inner.rs' if 'way' in text else f'{E}/c/inner.rs'
                self.edit(inner, 'struct', 'struct /* changed */')
                head = self.commit('change the file behind it')
                report = self.judge(self.review(head, ci_selectors=['storage::c::']),
                                    exports=False)
                self.assertFinding(report, f'review missing: {inner} needs D2 common')

    def test_unscoped_test_input_needs_evidence(self):  # RR3a
        self.write('tests/fixtures/other/input.json', '{}\n')
        self.base = self.commit('an engine fixture outside A/B')
        self.write('tests/fixtures/other/input.json', '{"x": 1}\n')
        head = self.commit('change it')
        missing = 'no CI evidence selected for unscoped input tests/fixtures/other/input.json'
        self.assertFinding(self.judge(self.review(head), exports=False), missing)
        named = self.review(head, name='r2', na=[{'module': '-', 'paths': ['tests/fixtures/other/**'],
                                                  'reason': 'read by no lib test'}])
        report = self.judge(named, exports=False)
        self.assertFalse(any(missing in e for e in report['errors']), report['errors'])

    def test_macro_generated_tests_select_their_module(self):  # RR3b
        self.edit(f'{E}/c_tests.rs', 'use super::Port;\n',
                  'use super::Port;\n'
                  'macro_rules! cases {\n    ($($name:ident),*) => {$(\n'
                  '        #[test]\n        fn $name() {}\n    )*};\n}\n'
                  'cases!(generated_a, generated_b);\n#[test]\nfn literal_case() {}\n')
        report = self.judge(self.commit('generated cases'), mode='push', exports=False)
        self.assertIn('storage::c::tests::', report['selection']['selectors'])
        self.assertEqual(report['selection']['cases'], ['storage::c::tests::literal_case'])

    def test_cfg_predicates_follow_the_ci_runner(self):  # RR3c
        self.edit(f'{E}/c_tests.rs', 'use super::Port;\n',
                  'use super::Port;\n'
                  '#[cfg(target_os = "linux")]\n#[test]\nfn linux_case() {}\n'
                  '#[cfg(not(target_os = "linux"))]\n#[test]\nfn other_os_case() {}\n'
                  '#[cfg(all(unix, not(windows)))]\n#[test]\nfn unix_case() {}\n'
                  '#[cfg(windows)]\nmod windows_only {\n    #[test]\n    fn absent() {}\n}\n')
        report = self.judge(self.commit('platform cases'), mode='push', exports=False)
        self.assertEqual(sorted(report['selection']['cases']),
                         ['storage::c::tests::linux_case', 'storage::c::tests::unix_case'])
        self.assertNotIn('storage::c::tests::', report['selection']['selectors'])
        self.edit(f'{E}/c_tests.rs', 'fn unix_case() {}\n',
                  'fn unix_case() {}\n#[cfg(feature = "test-support")]\n#[test]\n'
                  'fn feature_case() {}\n')
        report = self.judge(self.commit('a feature-gated case'), mode='push', exports=False)
        self.assertNotIn('storage::c::tests::', report['selection']['selectors'])
        self.assertNotIn('storage::c::tests::feature_case', report['selection']['cases'])
        self.assertEqual(report['selection']['optional_cases'],
                         ['storage::c::tests::feature_case'])

    def test_cases_behind_an_unknown_module_cfg_are_optional(self):  # RC1
        self.edit(f'{E}/c.rs', 'mod tests;\n',
                  'mod tests;\n#[cfg(feature = "test-support")]\n#[path = "c_feature.rs"]\n'
                  'mod feature_tests;\n')
        self.write(f'{E}/c_feature.rs', 'use super::Port;\n')
        self.base = self.commit('a feature-gated module')
        self.edit(f'{E}/c_feature.rs', 'use super::Port;\n',
                  'use super::Port;\n#[test]\nfn in_feature_module() {}\n')
        self.edit(f'{E}/c_tests.rs', 'use super::Port;\n',
                  'use super::Port;\n#[cfg(all(test, feature = "x"))]\nmod ftests {\n'
                  '    #[test]\n    fn in_feature_inline() {}\n}\n#[test]\nfn plain() {}\n')
        report = self.judge(self.commit('cases there'), mode='push', exports=False)
        plan = report['selection']
        self.assertEqual(plan['cases'], ['storage::c::tests::plain'])
        self.assertEqual(plan['optional_cases'],
                         ['storage::c::feature_tests::in_feature_module',
                          'storage::c::tests::ftests::in_feature_inline'])
        self.assertFalse(any(s.startswith('storage::c::') for s in plan['selectors']))

    def test_optional_cases_alone_are_no_evidence(self):  # RC1
        self.edit(f'{E}/c_tests.rs', 'use super::Port;\n',
                  'use super::Port;\n#[cfg(feature = "test-support")]\n#[test]\n'
                  'fn only_with_feature() {}\n')
        head = self.commit('only a feature-gated case')
        report = self.judge(self.review(head), exports=False)
        self.assertEqual(report['selection']['optional_cases'],
                         ['storage::c::tests::only_with_feature'])
        self.assertEqual(report['selection']['selectors'], [])
        self.assertFinding(report, 'no CI evidence selected for C')

    def test_cfg_all_test_module_is_test_only(self):  # RR3c
        self.edit(f'{E}/row/read.rs', 'pub(crate) fn read(_q: Port) {}\n',
                  'pub(crate) fn read(_q: Port) {}\n'
                  '#[cfg(all(test, target_os = "linux"))]\n#[path = "read/native.rs"]\n'
                  'mod native;\n')
        self.write(f'{E}/row/read/native.rs',
                   'use crate::storage::row::insert::insert;\n#[test]\nfn native_case() {}\n')
        report = self.judge(self.commit('a Linux-only test module'), mode='push', exports=False)
        found = [e for e in report['errors'] if e.startswith('forbidden dependency')]
        self.assertTrue(found and all(e.startswith(f'forbidden dependency B->A (test) {E}/row/'
                                                   f'read/native.rs:1:') for e in found), found)
        self.assertIn('storage::row::read::native::native_case', report['selection']['cases'])

    def test_denied_logical_path(self):  # RR4
        self.set_manifest(lambda m: m['modules'][6].update(deny_paths=['crate::storage::wal_io']))
        self.base = self.commit('C may not reach wal_io')
        self.edit(f'{E}/c.rs', 'pub(crate) struct Port;\n',
                  'pub(crate) struct Port;\n'
                  '#[cfg(test)]\nfn probe() { crate::storage::wal_io::plan4_recovery::x(); }\n'
                  'pub(crate) fn leak() { crate::storage::wal_io::plan4_recovery::x(); }\n')
        report = self.judge(self.commit('C reaches wal_io'), mode='push', exports=False)
        found = [e for e in report['errors'] if e.startswith('forbidden path')]
        self.assertEqual(found, [f'forbidden path crate::storage::wal_io in C (production) '
                                 f'{E}/c.rs:4: crate::storage::wal_io::plan4_recovery::x'],
                         report['errors'])
        self.set_manifest(lambda m: m['modules'][6].pop('deny_paths'))
        report = self.judge(self.commit('drop the deny'), mode='push', exports=False)
        self.assertTrue(any('C.deny_paths narrowed (relaxation)' in n for n in report['notes']))

    def test_relaxations_are_flagged_by_effect(self):  # RR6e
        cases = {
            'B->C (production) newly allowed (relaxation)': None,
            'exception EX1 changed (relaxation)': None,
            'policy_files changed (relaxation)': lambda m: m['policy_files'].pop(),
            'evidence changed (relaxation)': lambda m: m['evidence']['skip'].append('row'),
            'review_required changed (relaxation)': lambda m: m['review_required'].pop('D1'),
        }
        self.set_manifest(lambda m: m['exceptions'].append(exception(
            'EX1', f'{E}/row/insert.rs', 'B', 'test', ['crate::storage::row::read'])))
        self.edit(f'{E}/row/insert/tests.rs', 'use super::insert;',
                  'use super::insert;\nuse crate::storage::row::read::read;')
        base = self.base = self.commit('a test exception')
        for text, change in cases.items():
            with self.subTest(text=text):
                self.git('reset', '-q', '--hard', base)
                if text.startswith('B->C'):
                    def change(m):
                        m['modules'][1].pop('allow')
                        m['modules'][1].pop('allow_test')
                        m['modules'][1]['deny'] = []
                elif text.startswith('exception'):
                    def change(m):
                        m['exceptions'][0]['scope'] = 'production'
                self.set_manifest(change)
                report = self.judge(self.commit('relax'), mode='push', exports=False)
                self.assertTrue(any(text in n for n in report['notes']), report['notes'])
                self.assertIn('POLICY RELAXATION', report['summary'])
        self.git('reset', '-q', '--hard', base)
        self.set_manifest(lambda m: m['policy_files'].append('scripts/extra.py'))
        report = self.judge(self.commit('tighten'), mode='push', exports=False)
        self.assertNotIn('POLICY RELAXATION', report['summary'])

    def test_unused_exception_path_is_stale(self):
        self.edit(f'{E}/row/insert.rs', 'pub(crate) fn insert',
                  'pub(crate) fn peek() { crate::storage::row::read::read(Port) }\n'
                  'pub(crate) fn insert')
        self.set_manifest(lambda m: m['exceptions'].append(exception(
            'EX1', f'{E}/row/insert.rs', 'B', 'production',
            ['crate::storage::row::read::read', 'crate::storage::row::read::other'])))
        report = self.judge(self.commit('one path unused'), mode='push', exports=False)
        self.assertFinding(report, 'stale exception path EX1 crate::storage::row::read::other')

    def test_final_does_not_wait_after_a_failed_judgement(self):
        self.edit(f'{E}/row/read.rs', 'pub(crate) fn read',
                  'fn f() { crate::storage::row::insert::insert; }\npub(crate) fn read')
        head = self.commit('B reaches A')
        out = Path(self.temp.name) / 'report.json'
        with mock.patch.dict(os.environ, {'GITHUB_TOKEN': ''}):
            code = gate.main(['final', '--repo', str(self.repo), '--base', self.base,
                              '--head', head, '--repository', 'o/r',
                              '--build-workflow', 'module-build.yml',
                              '--artifact', 'module-build-receipt', '--out', str(out)])
        report = json.loads(out.read_text())
        self.assertEqual(code, 1)
        self.assertFalse(any('GITHUB_TOKEN' in e for e in report['errors']), report['errors'])
        self.assertTrue(any('not awaited' in n for n in report['notes']))


class OwnerReviewTests(GateFixture):
    """The owner's review of PR #18 (604c980e): reproductions that passed."""

    def test_raw_identifiers_are_the_same_names(self):
        for spelled in ('crate::storage::r#c::Port', 'crate::r#storage::c::r#Port'):
            with self.subTest(spelled=spelled):
                self.git('reset', '-q', '--hard', self.base)
                self.edit(f'{E}/codec.rs', 'pub(crate) struct',
                          f'pub(crate) fn port() -> {spelled} {{ todo!() }}\npub(crate) struct')
                report = self.judge(self.commit('codec reaches C, raw'), mode='push',
                                    exports=False)
                self.assertFinding(report, 'forbidden dependency codec->C (production)')
        self.git('reset', '-q', '--hard', self.base)
        self.edit(f'{E}/codec.rs', 'use crate::api::Namespace;',
                  'use crate::storage::{r#c::Port as _P};\nuse crate::api::Namespace;')
        report = self.judge(self.commit('codec use tree, raw'), mode='push', exports=False)
        self.assertFinding(report, 'forbidden dependency codec->C (production)')

    def test_na_needs_a_review_of_this_change(self):
        self.edit(f'{E}/codec.rs', 'struct Page', 'struct Page /* changed */')
        head = self.commit('codec change')
        current = self.review(head, name='r1')
        self.assertFinding(self.judge(current, exports=False),
                           'no CI evidence selected for codec')
        stale = {'schema': gate.RECORD_SCHEMA, 'change': 'old',
                 'na': [{'module': 'codec', 'reason': 'old result copied'}],
                 'reviews': [{'id': 'old', 'class': 'D1', 'scopes': [], 'author': '/root',
                              'reviewer': '/root/review', 'verdict': 'ACCEPT',
                              'required_open': 0, 'files': {'unrelated.rs': '0' * 40},
                              'log': 'old.md'}]}
        self.write('module-gate/records/old.json', json.dumps(stale))
        report = self.judge(self.commit('copied N/A'), exports=False)
        self.assertFinding(report, 'no CI evidence selected for codec')
        # A review of the same record that lists the file at its base blob
        # (an older state of the change) does not bind the N/A either.
        self.git('reset', '-q', '--hard', head)
        base_blob = self.git('rev-parse', f'{self.base}:{E}/codec.rs')
        old = dict(stale, reviews=[dict(stale['reviews'][0],
                                        files={f'{E}/codec.rs': base_blob})])
        self.write('module-gate/records/old.json', json.dumps(old))
        report = self.judge(self.commit('N/A reviewed at the base blob'), exports=False)
        self.assertFinding(report, 'no CI evidence selected for codec')
        # The same N/A inside the review of this change is accepted.
        self.git('reset', '-q', '--hard', head)
        report = self.judge(self.review(head, na=[{'module': 'codec', 'reason': 'comment only'}]),
                            exports=False)
        self.assertEqual(report['status'], 'PASS', report)

    def test_path_na_needs_a_review_of_that_path(self):
        self.write('tests/fixtures/other/x.json', '{}\n')
        head = self.commit('unscoped fixture')
        self.review(head, name='r1', paths=[])
        stale = {'schema': gate.RECORD_SCHEMA, 'change': 'old',
                 'na': [{'module': '-', 'paths': ['tests/fixtures/other/**'], 'reason': 'x'}],
                 'reviews': [{'id': 'old', 'class': 'D1', 'scopes': [], 'author': '/root',
                              'reviewer': '/root/review', 'verdict': 'ACCEPT',
                              'required_open': 0, 'files': {'unrelated.rs': '0' * 40},
                              'log': 'old.md'}]}
        self.write('module-gate/records/old.json', json.dumps(stale))
        report = self.judge(self.commit('copied path N/A'), exports=False)
        self.assertFinding(report, 'no CI evidence selected for unscoped input '
                                   'tests/fixtures/other/x.json')

    def test_impl_of_a_reexported_type_in_another_file_is_common(self):
        self.edit(f'{E}/mod.rs', 'pub(crate) mod c;', 'pub(crate) mod c;\nmod port_impl;')
        self.write(f'{E}/port_impl.rs',
                   'impl super::c::Port {\n    pub(crate) fn read(&self) -> u8 { 1 }\n}\n')
        self.base = self.commit('impl elsewhere')
        self.edit(f'{E}/port_impl.rs', '-> u8 { 1 }', '-> u16 { 1 }')
        head = self.commit('common API signature change')
        report = self.judge(self.review(head, ci_selectors=['storage::c::']), exports=False)
        self.assertFinding(report, f'review missing: {E}/port_impl.rs needs D2 common')
        # Through a `use` and a re-export, and as a trait impl.
        for body in ('use crate::storage::disk::Port;\nimpl Port { pub fn read(&self) {} }\n',
                     'impl Clone for super::disk::Port { fn clone(&self) -> Self { todo!() } }\n'):
            with self.subTest(body=body):
                self.write(f'{E}/port_impl.rs', body)
                head = self.commit('another impl')
                report = self.judge(self.review(head, ci_selectors=['storage::c::']),
                                    exports=False)
                self.assertFinding(report, f'review missing: {E}/port_impl.rs needs D2 common')
        # A private inherent impl, an impl of another type and an impl of a
        # trait the crate defines outside the common API stay D1 (at both
        # the base and the head).
        for body in ('impl super::c::Port { fn read(&self) {} }\n',
                     'struct Other;\nimpl Other { pub fn read(&self) {} }\n',
                     'trait Own { fn own(&self); }\n'
                     'impl Own for super::c::Port { fn own(&self) {} }\n',
                     '#[cfg(test)]\nmod tests {\n'
                     '    impl super::super::c::Port { pub fn helper(&self) {} }\n}\n'):
            with self.subTest(body=body):
                self.write(f'{E}/port_impl.rs', body)
                self.base = self.commit('not common at the base')
                self.write(f'{E}/port_impl.rs', body + '// changed\n')
                head = self.commit('not common')
                report = self.judge(self.review(head, ci_selectors=['storage::c::']),
                                    exports=False)
                self.assertEqual(report['status'], 'PASS', report)


FAKE_LIBTEST = r"""
import json, sys
names = ['storage::row::insert::tests::a', 'storage::row::insert::tests::newly_added',
         'storage::c::tests::z']
args = sys.argv[sys.argv.index('--') + 1:]
exact, listing, filters, skip, k = '--exact' in args, '--list' in args, [], [], 0
while k < len(args):
    if args[k] == '--skip':
        skip.append(args[k + 1]); k += 2; continue
    if not args[k].startswith('--'):
        filters.append(args[k])
    k += 1
chosen = [n for n in names if (not filters or any(n == f if exact else f in n for f in filters))
          and not any(x in n for x in skip)]
if listing:
    print('\n'.join(f'{n}: test' for n in chosen))
else:
    with open(sys.argv[1], 'a') as log:
        log.write(json.dumps(chosen) + '\n')
    for n in chosen:
        print(f'test {n} ... ok')
    print(f'test result: ok. {len(chosen)} passed; 0 failed; 0 ignored; 0 measured; '
          f'0 filtered out')
"""


class RunnerTests(unittest.TestCase):
    """The build runner runs a case once even when a selector and the exact
    case list both name it (owner review of PR #18)."""

    def test_a_case_a_selector_passed_is_not_run_again(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            (temp / 'fake.py').write_text(FAKE_LIBTEST)
            calls = temp / 'calls.jsonl'
            plan = {'test_command': [os.sys.executable, str(temp / 'fake.py'), str(calls)],
                    'skip': [], 'test_args': []}
            logs = temp / 'logs'
            logs.mkdir()
            env = dict(os.environ)
            results = [module_gate_build.selector_result(
                'head:storage::row::insert::', temp, plan, 'storage::row::insert::', logs, env)]
            self.assertEqual(results[0]['status'], 'PASS', results[0])
            cases = ['storage::c::tests::z', 'storage::row::insert::tests::newly_added']
            result = module_gate_build.cases_result(
                'head-cases', temp, plan, cases, logs, env,
                covered=module_gate_build.passed_by_source(results, 'head'))
            self.assertEqual(result['status'], 'PASS', result)
            self.assertEqual(result['covered'], ['storage::row::insert::tests::newly_added'])
            self.assertEqual((result['listed'], result['passed']), (2, 2))
            ran = [n for line in calls.read_text().splitlines() for n in json.loads(line)]
            self.assertEqual(sorted(ran), ['storage::c::tests::z', 'storage::row::insert::tests::a',
                                           'storage::row::insert::tests::newly_added'])


class RedirectTests(unittest.TestCase):
    def test_token_is_not_sent_to_the_redirect_target(self):
        seen = {}

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                seen.setdefault(self.server.server_port, []).append(
                    self.headers.get('Authorization'))
                if self.path == '/start':
                    self.send_response(302)
                    self.send_header('Location', f'http://127.0.0.1:{storage.server_port}/blob')
                    self.send_header('Content-Length', '0')
                    self.end_headers()
                    return
                self.send_response(200)
                self.send_header('Content-Length', '3')
                self.end_headers()
                self.wfile.write(b'zip')

            def log_message(self, *_args):
                pass

        api_server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
        storage = http.server.HTTPServer(('127.0.0.1', 0), Handler)
        for server in (api_server, storage):
            threading.Thread(target=server.serve_forever, daemon=True).start()
            self.addCleanup(server.server_close)
            self.addCleanup(server.shutdown)
        with mock.patch.dict(os.environ, {'no_proxy': '*', 'NO_PROXY': '*'}):
            data = gate.api(f'http://127.0.0.1:{api_server.server_port}/start', 'secret',
                            raw=True)
        self.assertEqual(data, b'zip')
        self.assertEqual(seen[api_server.server_port], ['Bearer secret'])
        self.assertEqual(seen[storage.server_port], [None])


class RealManifest(unittest.TestCase):
    def test_repository_manifest_matches_the_export_tool(self):
        """The manifest's A/B test files are exactly the export tool's link lists."""
        root = Path(gate.__file__).resolve().parent.parent
        data = json.loads((root / gate.MANIFEST).read_text())
        modules = {m['id']: m for m in data['modules']}
        memory = 'crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/'

        def names(mid):
            return {f[len(memory):-3] for f in modules[mid]['files']
                    if f.startswith(memory) and f.endswith('.rs')}

        links_a = set(module_single_source.source_spec(data, 'b')['tests'])
        links_b = set(module_single_source.source_spec(data, 'a')['tests'])
        self.assertEqual(names('A'), links_a - links_b)
        self.assertEqual(names('B'), links_b - links_a)
        self.assertEqual(names('JOIN'), links_a & links_b)
        census = {c[3] for c in module_single_source.source_spec(data, 'b')['tests_c']}
        self.assertEqual(census, {e['source'] for e in data['exceptions'] if e['target'] == 'A'})
        gate.Manifest(data, 'repository')

    def test_protected_refs_are_judged_like_main(self):
        """Every ref the ruleset protects gets the required context (RR5)."""
        root = Path(gate.__file__).resolve().parent.parent
        ruleset = json.loads((root / '.github/rulesets/main-module-gate.json').read_text())
        workflow = (root / '.github/workflows/module-gate.yml').read_text()
        include = ruleset['conditions']['ref_name']['include']
        self.assertEqual(include, ['~DEFAULT_BRANCH', 'refs/heads/gate-test/**'])
        for key in ('XADB_CONTEXT', 'XADB_MODE'):
            line = next(x for x in workflow.splitlines() if x.strip().startswith(key + ':'))
            self.assertIn('github.event.repository.default_branch', line)
            self.assertIn("startsWith(github.event.pull_request.base.ref, 'gate-test/')", line)

    def test_build_workflow_skips_only_before_adoption(self):
        """The adopting PR has no planner on the default branch: the build half
        ends without a receipt, which the control workflow never accepts."""
        root = Path(gate.__file__).resolve().parent.parent
        workflow = (root / '.github/workflows/module-build.yml').read_text()
        self.assertIn('ref="refs/remotes/origin/$DEFAULT_BRANCH"', workflow)
        self.assertIn(f'git cat-file -e "$ref:{gate.MANIFEST}"', workflow)
        self.assertNotIn('"origin/$DEFAULT_BRANCH', workflow)  # a tag could shadow it
        steps = workflow.split('      - ')[1:]
        copy = next(i for i, step in enumerate(steps) if step.startswith('name: Copy'))
        self.assertIn('XADB_ADOPTED=false', steps[copy])
        for step in steps[copy + 1:]:
            guard = next((x for x in step.splitlines() if x.strip().startswith('if:')), '')
            self.assertIn("env.XADB_ADOPTED == 'true'", guard, step.splitlines()[0])




class F2ContractTests(GateFixture):
    def setUp(self):
        super().setUp()
        self.case = 'storage::wal_io::plan4_recovery::tests::catalog_memory::row_a::contract'
        self.write(f'{MEM}/row_a.rs', '#[test]\nfn contract() {}\n')
        self.row = dict(id='F2-A', owner='A', paths=[f'{E}/c.rs'], test_file=f'{MEM}/row_a.rs',
                        inputs=['tests/fixtures/module-row-insert/**'], case=self.case,
                        sources=['export-a'], contract='contract.md#ex', reason='direct C call')
        self.set_manifest(lambda m: m.update(exports=EXPORTS, affected_contracts=[self.row]))
        self.base = self.commit('contract registry')

    def test_common_change_selects_minimal_export_case_and_covers_changed_path(self):
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port { x: u8 }')
        report = self.judge(self.commit('common change'), mode='push', exports=False)
        self.assertEqual(report['errors'], [])
        plan = report['selection']
        self.assertEqual(plan['cases'], [])
        self.assertEqual(plan['selectors'], [])
        self.assertEqual(plan['exports'][0]['cases'], [self.case])
        self.assertEqual(plan['exports'][1]['cases'], [])
        self.assertFalse(any('no CI evidence' in p for p in report['pending']))
        self.assertEqual(plan['contracts'][0]['changed'], [f'{E}/c.rs'])

    def test_input_and_harness_change_trigger_but_unrelated_change_does_not(self):
        self.edit('README.md', 'fixture', 'other')
        report = self.judge(self.commit('unrelated'), mode='push', exports=False)
        self.assertFalse(report['selection']['rust'])
        for path in ('tests/fixtures/module-row-insert/a.json', f'{MEM}/row_a.rs'):
            self.write(path, (self.repo / path).read_text() + '\n')
            report = self.judge(self.commit('input'), mode='push', exports=False)
            self.assertEqual(len(report['selection']['contracts']), 1)
            self.assertIn(path, report['selection']['contracts'][0]['inputs'])

    def test_missing_ignored_unknown_and_wrong_case_are_rejected(self):
        for body in ('', '#[test]\n#[ignore]\nfn contract() {}',
                     '#[test]\n#[cfg(feature="x")]\nfn contract() {}',
                     '#[test]\nfn other() {}'):
            with self.subTest(body=body):
                self.write(f'{MEM}/row_a.rs', body)
                self.assertFinding(self.judge(self.commit('missing'), exports=False),
                                   'required case not found')

    def test_removed_c_owned_test_and_wrong_fixture_export_are_rejected(self):
        for path in (f'{MEM}/row_a.rs', 'tests/fixtures/module-row-insert/a.json'):
            tree = gate.Tree(str(self.repo), self.base)
            analysis = gate.Analysis(tree, ['crates/engine/src/lib.rs'])
            data = json.loads((self.repo / gate.MANIFEST).read_text())
            data['affected_contracts'][0]['sources'] = ['export-b']
            # Even a C-owned test may be physically removed by the export tool.
            if path.endswith('.rs'):
                data['modules'][0]['files'].remove(f'{MEM}/row_a.rs')
                data['affected_contracts'][0]['owner'] = 'C'
                ex = exception('C-A', f'{MEM}/row_a.rs', 'A', 'test', ['crate::storage::row::insert'])
                ex['export_registration'] = dict(file=f'{E}/wal_io/plan4_recovery/tests/catalog_memory.rs',
                    path='catalog_memory/row_a.rs', module='row_a')
                data['exceptions'].append(ex)
                data['affected_contracts'][0]['inputs'] = [f'{E}/c.rs']
            _, errors = gate.contract_selection(gate.Manifest(data, 'fixture'), analysis,
                                                 {f'{E}/c.rs'})
            self.assertTrue(any('export-b removes input' in e for e in errors), errors)

    def test_missing_input_and_deleted_production_path_fail(self):
        for path in ('tests/fixtures/module-row-insert/a.json', f'{E}/c.rs'):
            self.git('rm', path)
            report = self.judge(self.commit('delete'), exports=False)
            self.assertFinding(report, 'missing input')

    def test_duplicate_id_bad_source_and_nonexact_case_rejected(self):
        data = json.loads((self.repo / gate.MANIFEST).read_text())
        for change in ({'sources': ['export-z']}, {'case': 'storage::c::'},
                       {'inputs': ['../elsewhere']}, {'paths': []}):
            bad = copy.deepcopy(data)
            bad['affected_contracts'][0].update(change)
            with self.assertRaises(gate.GateError):
                gate.Manifest(bad, 'fixture')
        data['affected_contracts'] *= 2
        with self.assertRaises(gate.GateError):
            gate.Manifest(data, 'fixture')

    def test_head_cannot_remove_trusted_obligation_and_duplicate_case_is_unioned(self):
        self.set_manifest(lambda m: m.update(affected_contracts=[]))
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port { x: u8 }')
        report = self.judge(self.commit('remove mapping'), mode='push', exports=False)
        self.assertEqual(report['selection']['exports'][0]['cases'], [self.case])
        plan = report['selection']
        gate.apply_contract_selection(plan, [dict(plan['contracts'][0], id='second')])
        self.assertEqual(plan['exports'][0]['cases'], [self.case])


class F2RunnerTests(unittest.TestCase):
    def run_fixture(self, action, body=FAKE_LIBTEST):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            script, calls = root / 'fake.py', root / 'calls.jsonl'
            script.write_text(body)
            plan = dict(test_command=[os.sys.executable, str(script), str(calls)],
                        skip=[], test_args=[])
            action(root, calls, plan)

    def test_overlapping_selectors_and_exact_cases_run_once_per_source(self):
        def check(root, calls, plan):
            results = []
            for sel in ('storage::row::insert::', 'storage::'):
                results.append(module_gate_build.selector_result(
                    f'head:{sel}', root, plan, sel, root, os.environ,
                    earlier=module_gate_build.source_claims(results, 'head')))
            self.assertTrue(all(r['status'] == 'PASS' for r in results), results)
            names = results[-1]['listed_names']
            result = module_gate_build.cases_result('head-cases', root, plan, names, root,
                os.environ, covered=module_gate_build.passed_by_source(results, 'head'))
            self.assertEqual(result['status'], 'PASS')
            ran = [n for line in calls.read_text().splitlines() for n in json.loads(line)]
            self.assertEqual(sorted(ran), names)
            self.assertEqual(module_gate_build.source_claims(results, 'export-a'), [])
            other = module_gate_build.selector_result('export-a:storage::', root, plan,
                'storage::', root, os.environ, earlier=module_gate_build.source_claims(results, 'export-a'))
            self.assertEqual(other['covered'], [])
            self.assertEqual(other['status'], 'PASS')
        self.run_fixture(check)

    def test_optional_lanes_reuse_required_exact_case_without_execution(self):
        def check(root, calls, plan):
            case = 'storage::c::tests::z'
            first = module_gate_build.cases_result('head-cases', root, plan,
                [case], root, os.environ)
            part = dict(optional_selectors=['storage::c::'], optional_cases=[case])
            later = module_gate_build.optional_results('head', root, plan, part, root,
                                                       os.environ, earlier=[first])
            self.assertTrue(all(r['status'] == 'PASS' for r in later), later)
            self.assertEqual([r['covered'] for r in later], [[case], [case]])
            self.assertEqual(len(calls.read_text().splitlines()), 1)
        self.run_fixture(check)

    def test_missing_required_or_wrong_exact_name_fails(self):
        def check(root, calls, plan):
            result = module_gate_build.cases_result('head-cases', root, plan,
                ['missing::case'], root, os.environ)
            self.assertEqual(result['status'], 'FAIL')
            result = module_gate_build.selector_result('head:missing::', root, plan,
                'missing::', root, os.environ)
            self.assertEqual(result['status'], 'FAIL')
        self.run_fixture(check)
        # Equal aggregate counts from a different name must not pass.
        def substituted(root, calls, plan):
            result = module_gate_build.cases_result('head-cases', root, plan,
                ['storage::c::tests::z'], root, os.environ)
            self.assertEqual(result['status'], 'FAIL')
        self.run_fixture(substituted, FAKE_LIBTEST.replace("f'{n}: test'", "f'wrong::{n}: test'"))

    def test_failed_selector_case_is_not_retried_by_overlap_or_exact_lane(self):
        def check(root, calls, plan):
            first = module_gate_build.selector_result('head:storage::', root, plan,
                'storage::', root, os.environ)
            self.assertEqual(first['status'], 'FAIL')
            second = module_gate_build.selector_result('head:storage::c::', root, plan,
                'storage::c::', root, os.environ, earlier=[first])
            self.assertEqual(second['status'], 'FAIL')
            exact = module_gate_build.cases_result('head-cases', root, plan,
                ['storage::c::tests::z'], root, os.environ, unavailable=first['listed_names'])
            self.assertEqual(exact['status'], 'FAIL')
            self.assertEqual(len(calls.read_text().splitlines()), 1)
        self.run_fixture(check, FAKE_LIBTEST.replace(' ... ok', ' ... FAILED')
                         .replace('{len(chosen)} passed; 0 failed', '0 passed; {len(chosen)} failed'))


class F2ReceiptTests(GateFixture):
    receipt = FinalTests.receipt
    run_for = FinalTests.run_for
    verify = FinalTests.verify

    def fixture(self):
        self.edit('README.md', 'fixture', 'changed')
        report = self.judge(self.commit('head'), mode='push', exports=False)
        case = 'storage::c::test'
        report['selection'] = dict(rust=True, selectors=[], cases=[case], exports=[],
                                   contracts=[dict(id='C', source='head', case=case)])
        receipt = self.receipt(report)
        receipt['results'].append(dict(name='head-cases', status='PASS', listed=1,
            passed=1, failed=0, ignored=0, listed_names=[case], passed_names=[case], covered=[]))
        return report, receipt

    def test_missing_selection_result_wrong_names_and_duplicate_rows_fail(self):
        report, receipt = self.fixture()
        self.assertEqual(self.verify(report, receipt), [])
        mutations = [lambda r: r['selection'].update(cases=[]),
                     lambda r: r['results'].pop(),
                     lambda r: r['results'][-1].update(listed_names=['other::case'],
                                                       passed_names=['other::case']),
                     lambda r: r['results'].append(r['results'][-1].copy()),
                     lambda r: r['results'][-1].update(covered=['storage::c::test'])]
        for mutate in mutations:
            bad = copy.deepcopy(receipt)
            mutate(bad)
            self.assertTrue(self.verify(report, bad), bad)

    def test_cross_source_reuse_and_duplicate_execution_fail(self):
        report, receipt = self.fixture()
        case = report['selection']['cases'][0]
        report['selection']['selectors'] = ['storage::c::', 'storage::']
        receipt['selection'] = copy.deepcopy(report['selection'])
        one = dict(name='head:storage::c::', status='PASS', listed=1, passed=1, failed=0,
                   ignored=0, listed_names=[case], passed_names=[case], covered=[])
        two = dict(one, name='head:storage::', covered=[case])
        receipt['results'][1:1] = [one, two]
        receipt['results'][-1]['covered'] = [case]
        self.assertEqual(self.verify(report, receipt), [])
        two['covered'] = []
        self.assertTrue(any('duplicate execution' in e for e in self.verify(report, receipt)))
        two['covered'] = [case]
        # An export's row cannot cover the head's case.
        one['name'] = 'export-a:storage::c::'
        self.assertTrue(self.verify(report, receipt))

    def test_optional_lanes_may_reuse_an_earlier_required_exact_case(self):
        report, receipt = self.fixture()
        case = report['selection']['cases'][0]
        report['selection'].update(optional_selectors=['storage::'], optional_cases=[case])
        receipt['selection'] = copy.deepcopy(report['selection'])
        row = receipt['results'][-1]
        receipt['results'] += [dict(row, name='head?:storage::', covered=[case]),
                               dict(row, name='head-optional-cases', covered=[case])]
        self.assertEqual(self.verify(report, receipt), [])



class F2RealRegistryTests(unittest.TestCase):
    def test_reserved_row_real_paths_cases_inputs_and_exports_are_bound(self):
        root = Path(__file__).resolve().parent.parent
        data = json.loads((root / gate.MANIFEST).read_text())
        manifest = gate.Manifest(data, 'candidate')
        # F1 executes a clean Git-object export without .git. Read actual
        # candidate files as test data; the production checker still uses Git.
        paths = {str(p.relative_to(root)) for directory in ('crates', 'tests')
                 for p in (root / directory).rglob('*') if p.is_file()}
        tree = types.SimpleNamespace(
            files=lambda: paths, exists=lambda p: p in paths,
            text=lambda p: (root / p).read_text(),
            read=lambda p: (root / p).read_bytes(),
            blob=lambda p: hashlib.sha256((root / p).read_bytes()).hexdigest() if p in paths else None,
            preload=lambda paths: None)
        analysis = gate.Analysis(tree, [e['root'] for e in data['crates']])
        path = f'{E}/wal_io/plan4_apply/row/write.rs'
        selected, errors = gate.contract_selection(manifest, analysis, {path})
        self.assertEqual(errors, [])
        self.assertEqual(len(selected), 3)
        self.assertEqual({r['source'] for r in selected}, {'export-a'})
        self.assertEqual({r['id'] for r in selected}, {'S2W-R10-LIVE', 'S2W-R11-LIVE', 'S2W-R21-LIVE'})
        self.assertTrue(all(r['inputs'][path] == tree.blob(path) for r in selected))
        self.assertEqual(gate.contract_selection(manifest, analysis, {f'{E}/codec.rs'}), ([], []))
        for row in data['affected_contracts']:
            row['sources'] = ['export-b']
        _, errors = gate.contract_selection(gate.Manifest(data, 'wrong export'), analysis, {path})
        self.assertEqual(sum('removes input' in e for e in errors), 3)


class F5ExportTests(GateFixture):
    def setUp(self):
        super().setUp()
        self.set_manifest(lambda m: m.update(exports=EXPORTS))
        self.base = self.commit('exports')

    def export(self, head, side, data=None):
        policy = data or module_single_source.policy_data(self.repo, self.base)
        out = self.repo.parent / ('export-' + side)
        inventory, _ = module_single_source.export(head, out, side, repo=self.repo, policy=policy)
        self.addCleanup(lambda: __import__('shutil').rmtree(out, ignore_errors=True))
        return inventory, out

    def test_head_cannot_change_trusted_export_policy(self):
        self.set_manifest(lambda m: m['modules'][1].update(files=[]))
        head = self.commit('head deletes removal policy')
        inventory, out = self.export(head, 'a')
        self.assertIn(f'{E}/row/read.rs', inventory['removed'])
        self.assertFalse((out / f'{MEM}/row_b.rs').exists())
        self.assertTrue((out / f'{MEM}/row_a.rs').exists())

    def test_join_and_exception_registrations_are_derived_from_owners(self):
        data = module_single_source.policy_data(self.repo, self.base)
        data['modules'].insert(0, dict(id='JOIN', kind='join', files=[f'{MEM}/row_join.rs'],
            logical=[f'{MEM_PATH}::row_join'], deny=[]))
        self.write(f'{MEM}/row_join.rs', 'use crate::storage::row::insert::insert;\n')
        registry = f'{E}/wal_io/plan4_recovery/tests/catalog_memory.rs'
        self.write(registry, (self.repo / registry).read_text()
            + '#[path = "catalog_memory/row_join.rs"]\nmod row_join;\n')
        self.write(f'{E}/census.rs', '#[test]\nfn size() {}\n')
        self.write(f'{E}/c.rs', (self.repo / f'{E}/c.rs').read_text()
            + '#[cfg(test)]\n#[path = "census.rs"]\nmod census;\n')
        ex = exception('EX', f'{E}/census.rs', 'A', 'test', ['crate::storage::row::insert'])
        ex['export_registration'] = dict(file=f'{E}/c.rs', path='census.rs', module='census')
        data['exceptions'] = [ex]
        head = self.commit('join and census')
        for side in ('a', 'b'):
            inventory, out = self.export(head, side, data)
            self.assertIn(f'{MEM}/row_join.rs', inventory['removed'])
            self.assertEqual((out / f'{E}/census.rs').exists(), side == 'a')
            self.assertEqual('mod census;' in (out / f'{E}/c.rs').read_text(), side == 'a')

    def test_missing_duplicate_or_wrong_registration_is_rejected(self):
        registry = f'{E}/wal_io/plan4_recovery/tests/catalog_memory.rs'
        original = (self.repo / registry).read_text()
        for text in (original.replace('mod row_b;', 'mod wrong;'),
                     original + '#[path = "catalog_memory/row_b.rs"]\nmod row_b;\n'):
            self.write(registry, text)
            with tempfile.TemporaryDirectory() as directory:
                with self.assertRaisesRegex(SystemExit, 'registration of row_b'):
                    module_single_source.export(self.commit('broken registration'),
                        Path(directory) / 'out', 'a', repo=self.repo,
                        policy=module_single_source.policy_data(self.repo, self.base))

    def test_missing_or_invalid_metadata_fails_closed(self):
        data = module_single_source.policy_data(self.repo, self.base)
        ex = exception('EX', f'{E}/census.rs', 'A', 'test', ['crate::storage::row::insert'])
        data['exceptions'] = [ex]
        with self.assertRaises(gate.GateError):
            gate.Manifest(data, 'missing registration')
        for registration in (dict(file=f'{E}/c.rs', path='../census.rs', module='census'),
                             dict(file=f'{E}/c.rs', path='different.rs', module='census'),
                             dict(file=f'{E}/c.rs', path='census.rs', module='bad-name')):
            ex['export_registration'] = registration
            with self.assertRaises(gate.GateError):
                gate.Manifest(data, 'bad registration')
        ex['export_registration'] = dict(file=f'{E}/c.rs', path='census.rs', module='census')
        data['exceptions'].append(dict(ex, id='second', source=f'{E}/other.rs'))
        with self.assertRaises(gate.GateError):
            gate.Manifest(data, 'duplicate registration')

    def test_empty_owned_subtree_is_allowed_but_missing_literal_is_not(self):
        inventory, _ = self.export(self.base, 'a')
        self.assertIn(f'{E}/row/read.rs', inventory['removed'])
        self.git('rm', f'{E}/row/read.rs')
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(SystemExit, 'declared path missing: .*row/read.rs'):
                module_single_source.export(self.commit('missing literal'),
                    Path(directory) / 'out', 'a', repo=self.repo,
                    policy=module_single_source.policy_data(self.repo, self.base))

    def test_actual_runner_uses_policy_revision_for_each_export(self):
        # A head may change the declarative policy, but this build still owes
        # the old policy's removal of B's fixture. No Rust process is invoked.
        self.set_manifest(lambda m: m['modules'][1]['files'].remove(
            'tests/fixtures/module-row-read/**'))
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port; // changed')
        head = self.commit('candidate weakens fixture ownership')
        output = self.repo.parent / 'receipt'
        original = module_single_source.export
        policies = []

        def export(commit, out, side, repo=None, *, policy):
            policies.append(policy)
            result = original(commit, out, side, repo, policy=policy)
            if side == 'a':
                self.assertFalse((out / 'tests/fixtures/module-row-read').exists())
            return result

        argv = ['module_gate_build.py', '--repo', str(self.repo), '--base', self.base,
                '--head', head, '--policy', self.base, '--workflow', '.github/workflows/module-build.yml',
                '--out', str(output)]
        with mock.patch.object(os.sys, 'argv', argv), \
             mock.patch.object(module_single_source, 'export', side_effect=export), \
             mock.patch.object(module_gate_build, 'build_result',
                               side_effect=lambda name, *args: dict(name=name, status='PASS')), \
             mock.patch.object(module_gate_build, 'identity',
                               side_effect=lambda plan, source, env, sha: (env, {})):
            self.assertEqual(module_gate_build.main(), 0)
        self.assertEqual(len(policies), 2)
        self.assertTrue(all(p == module_single_source.policy_data(self.repo, self.base)
                            for p in policies))
        receipt = json.loads((output / 'receipt.json').read_text())
        self.assertEqual(receipt['policy'], self.base)

    def test_unsupported_join_and_memory_patterns_fail_closed(self):
        original = module_single_source.policy_data(self.repo, self.base)
        for path in (f'{E}/other_join.rs', f'{MEM}/nested/join.rs', f'{MEM}/orphan/**'):
            data = copy.deepcopy(original)
            data['modules'].insert(0, dict(id='JOIN', kind='join', files=[path],
                                          logical=[], deny=[]))
            with self.assertRaisesRegex(gate.GateError, 'unsupported .*ownership pattern'):
                gate.Manifest(data, 'unsupported layout')

    def test_gate_rejects_exporter_leaving_a_join_unit(self):
        data = module_single_source.policy_data(self.repo, self.base)
        data['modules'].insert(0, dict(id='JOIN', kind='join', files=[f'{MEM}/row_join.rs'],
            logical=[f'{MEM_PATH}::row_join'], deny=[]))
        self.write(gate.MANIFEST, json.dumps(data))
        self.write(f'{MEM}/row_join.rs', '// joint test helper\n')
        registry = f'{E}/wal_io/plan4_recovery/tests/catalog_memory.rs'
        self.write(registry, (self.repo / registry).read_text()
            + '#[path = "catalog_memory/row_join.rs"]\nmod row_join;\n')
        self.base = self.commit('joint owner')
        self.edit(f'{E}/c.rs', 'struct Port;', 'struct Port; // changed')
        head = self.commit('changed C')
        original = module_single_source.export

        def incomplete(commit, out, side, repo=None, *, policy):
            incomplete_policy = copy.deepcopy(policy)
            incomplete_policy['modules'][0]['files'] = []
            return original(commit, out, side, repo, policy=incomplete_policy)

        with mock.patch.object(module_single_source, 'export', side_effect=incomplete):
            report = self.judge(head, mode='push')
        self.assertFinding(report, f'left JOIN file {MEM}/row_join.rs')

    def test_explicit_policy_is_required(self):
        with self.assertRaises(TypeError):
            module_single_source.export(self.base, self.repo.parent / 'out', 'a', repo=self.repo)

    def test_contract_owner_must_match_test_file_owner(self):
        data = module_single_source.policy_data(self.repo, self.base)
        row = dict(id='C', owner='B', paths=[f'{E}/c.rs'], test_file=f'{MEM}/row_a.rs',
                   inputs=[f'{MEM}/row_a.rs'], case='storage::test', sources=['head'],
                   contract='contract.md#ex', reason='fixture')
        data['affected_contracts'] = [row]
        with self.assertRaises(gate.GateError):
            gate.Manifest(data, 'wrong owner')
        row['owner'] = 'A'
        gate.Manifest(data, 'correct owner')


class F5RealRegistryTests(unittest.TestCase):
    def test_single_manifest_preserves_baseline_export_layout_and_f2_owners(self):
        root = Path(__file__).resolve().parent.parent
        data = json.loads((root / gate.MANIFEST).read_text())
        baseline = json.loads((root / 'module-gate/evidence/f5/baseline-export-spec.json').read_text())
        for side, expected in baseline['sides'].items():
            spec = module_single_source.source_spec(data, side)
            self.assertEqual(spec['module'], expected['module'])
            for key in ('removed', 'tests', 'tests_c'):
                self.assertEqual(sorted(map(str, json.loads(json.dumps(spec.get(key, []))))),
                                 sorted(map(str, expected.get(key, []))))
        manifest = gate.Manifest(data, 'candidate')
        self.assertEqual({r['owner'] for r in data['affected_contracts']}, {'A'})
        self.assertTrue(all(manifest.owner(r['test_file']) == r['owner']
                            for r in data['affected_contracts']))
        self.assertEqual({e['id'] for e in data['exceptions']},
                         {f'EX{i:02}' for i in range(1, 10)})


if __name__ == '__main__':
    unittest.main()
