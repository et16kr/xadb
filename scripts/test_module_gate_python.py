"""F1 selection, isolated candidate execution and receipt contract negatives."""
import copy
import io
import json
import os
import subprocess
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import check_module_gate as gate
import module_gate_python as runner
from test_check_module_gate import GateFixture

CASE = 'test_candidate.Check.test_value'
SOURCE = 'scripts/candidate.py'
TEST = '''import unittest
import candidate
class Check(unittest.TestCase):
    def test_value(self):
        self.assertEqual(candidate.VALUE, 2)
'''


class PythonFixture(GateFixture):
    def setUp(self):
        super().setUp()
        self.write(SOURCE, 'VALUE = 1\n')
        self.write('scripts/test_candidate.py', TEST)
        for p in gate.PYTHON_RUNNERS:
            self.write(p, '# trusted runner\n')
        self.set_manifest(lambda d: d.update(python_tests={
            SOURCE: ['test_candidate'], 'scripts/test_candidate.py': ['test_candidate']}))
        self.base = self.commit('F1 baseline')

    def candidate(self, tests=None):
        self.write(SOURCE, 'VALUE = 2\n')
        self.write('module-gate/records/python.json', json.dumps({
            'schema': gate.RECORD_SCHEMA, 'python_tests': [CASE] if tests is None else tests}))
        return self.commit('candidate')

    def plan(self, head):
        return self.judge(head, mode='push', exports=False)


class PythonPlanTests(PythonFixture):
    def test_python_change_selects_exact_tests_without_rust(self):
        r = self.plan(self.candidate())
        self.assertFalse(r['selection']['rust'])
        p = r['selection']['python']
        self.assertEqual(p['tests'], [CASE])
        self.assertEqual(p['changed'], {SOURCE: gate.Tree(str(self.repo), r['head']).blob(SOURCE)})
        self.assertTrue(all(p['runners'].values()))
        self.assertFalse(any('Python' in n for n in r['errors']))

    def test_docs_only_does_not_run_python(self):
        self.write('README.md', 'docs change\n')
        self.assertNotIn('python', self.plan(self.commit('docs'))['selection'])

    def test_missing_selection_is_an_obligation_not_success(self):
        r = self.plan(self.candidate([]))
        self.assertTrue(any('no declared Python test' in n for n in r['errors']))
        merged = self.judge(r['head'], exports=False)
        self.assertFinding(merged, 'no declared Python test')

    def test_invalid_missing_and_broad_test_names_fail(self):
        for names in ([CASE.rsplit('.', 1)[0]], ['test_other.Check.test_value'],
                      [CASE + '_missing'], 'not a list'):
            with self.subTest(names=names):
                r = self.plan(self.candidate(names))
                self.assertTrue(any('Python test' in n or 'python_tests' in n for n in r['errors']))

    def test_candidate_syntax_error_is_not_success(self):
        self.candidate()
        self.write('scripts/test_candidate.py', 'def bad(:\n')
        r = self.plan(self.commit('syntax error'))
        self.assertTrue(any('Python test' in n for n in r['errors']))

    def test_async_test_declaration_is_not_an_executable_sync_case(self):
        self.candidate()
        self.write('scripts/test_candidate.py', TEST.replace('def test_value', 'async def test_value'))
        r = self.plan(self.commit('async test'))
        self.assertTrue(any('missing or ambiguous declaration' in n for n in r['errors']))

    def test_mapping_removal_does_not_waive_trusted_obligation(self):
        self.candidate([])
        self.set_manifest(lambda d: d.update(python_tests={}))
        r = self.plan(self.commit('drop mapping'))
        self.assertTrue(any('no declared Python test' in n for n in r['errors']))
        self.assertTrue(any('python_tests changed' in n for n in r['notes']))

    def test_rename_keeps_old_path_obligation(self):
        (self.repo / SOURCE).rename(self.repo / 'scripts/moved.py')
        r = self.plan(self.commit('rename'))
        self.assertIn(SOURCE, r['selection']['python']['changed'])
        self.assertTrue(any('no declared Python test' in n for n in r['errors']))

    def test_declarations_deduplicate_across_records(self):
        self.candidate()
        self.write('module-gate/records/second.json', json.dumps({
            'schema': gate.RECORD_SCHEMA, 'python_tests': [CASE, CASE]}))
        self.assertEqual(self.plan(self.commit('duplicate'))['selection']['python']['tests'], [CASE])

    def receipt(self):
        report = self.plan(self.candidate())
        receipt = {k: report[k] for k in ('head', 'base', 'merge_base', 'manifest_blob',
                                         'checker_blob', 'selection')}
        receipt.update(schema=gate.RECEIPT_SCHEMA, workflow='build.yml', checker_sha256='checker',
                       runner_blobs=report['selection']['python']['runners'], results=[],
                       python_results={'status': 'PASS', 'exit_code': 0,
                                       'tests': [{'id': CASE, 'status': 'PASS'}]})
        run = dict(head_sha=report['head'], event='pull_request', path='build.yml',
                   head_repository={'full_name': 'owner/repo'})
        return report, receipt, run

    def test_receipt_requires_exact_successful_results_and_identity(self):
        r, receipt, run = self.receipt()
        check = lambda value: gate.verify_receipt(r, value, run, 'owner/repo', 'checker')
        self.assertEqual(check(receipt), [])
        for key in ('head', 'base', 'merge_base', 'runner_blobs', 'selection'):
            bad = copy.deepcopy(receipt)
            bad[key] = 'stale'
            with self.subTest(key=key):
                self.assertTrue(check(bad))
        variants = [None, {}, {'status': 'PASS', 'exit_code': 0, 'tests': []}]
        for status in ('SKIP', 'FAIL', 'ERROR', 'INCOMPLETE'):
            bad = copy.deepcopy(receipt['python_results'])
            bad['tests'][0]['status'] = status
            variants.append(bad)
        for rows in ([{'id': CASE, 'status': 'PASS'}] * 2,
                     [{'id': CASE + '_other', 'status': 'PASS'}], ['malformed']):
            variants.append({'status': 'PASS', 'exit_code': 0, 'tests': rows})
        bad = copy.deepcopy(receipt['python_results'])
        bad['exit_code'] = 1
        variants.append(bad)
        for value in variants:
            with self.subTest(value=value):
                self.assertTrue(check(dict(receipt, python_results=value)))

    def test_python_only_final_waits_for_real_build_and_checks_receipt(self):
        report, receipt, run = self.receipt()
        run.update(id=42, run_number=1, status='completed', conclusion='success')
        data = io.BytesIO()
        receipt['checker_sha256'] = gate.hashlib.sha256(Path(gate.__file__).read_bytes()).hexdigest()
        with zipfile.ZipFile(data, 'w') as archive:
            archive.writestr('receipt.json', json.dumps(receipt))
        fetch = mock.Mock(side_effect=[{'workflow_runs': [run]},
                                      {'artifacts': [{'name': 'receipt', 'archive_download_url': 'artifact'}]},
                                      data.getvalue()])
        args = types.SimpleNamespace(repository='owner/repo', build_workflow='build.yml',
                                     artifact='receipt', wait=0)
        with mock.patch.dict(os.environ, GITHUB_TOKEN='test-token'):
            self.assertEqual(gate.final(args, report, fetch=fetch), [])
        self.assertEqual(fetch.call_count, 3)


class PythonWorkerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source, self.logs = self.root / 'source', self.root / 'logs'
        (self.source / 'scripts').mkdir(parents=True)
        self.logs.mkdir()
        (self.source / 'scripts/candidate.py').write_text('VALUE = 2\n')

    def execute(self, text=TEST, names=None, timeout=10):
        (self.source / 'scripts/test_candidate.py').write_text(text)
        return runner.execute(self.source, {'tests': names or [CASE], 'timeout_seconds': timeout},
                              self.logs)

    def test_worker_imports_candidate_not_loaded_trusted_modules(self):
        (self.source / 'scripts/check_module_gate.py').write_text('VALUE = 2\n')
        test = TEST.replace('import candidate', 'import check_module_gate as candidate')
        self.assertEqual(self.execute(test)['status'], 'PASS')

    def test_worker_does_not_receive_tokens_or_python_injection(self):
        test = TEST.replace('import candidate', 'import candidate\nimport os').replace(
            'self.assertEqual(candidate.VALUE, 2)',
            "self.assertNotIn('GITHUB_TOKEN', os.environ)\n"
            "        self.assertNotIn('PYTHONPATH', os.environ)\n"
            "        self.assertNotIn('GH_TOKEN', os.environ)")
        with mock.patch.dict(os.environ, GITHUB_TOKEN='secret', GH_TOKEN='secret', PYTHONPATH='/bad'):
            self.assertEqual(self.execute(test)['status'], 'PASS')

    def test_worker_rejects_syntax_runtime_failure_skip_and_expected_failure(self):
        variants = ['def broken(:', 'raise RuntimeError("import")\n' + TEST,
                    TEST.replace('candidate.VALUE, 2', 'candidate.VALUE, 3'),
                    TEST.replace('    def test_value', '    @unittest.skip("skip")\n    def test_value'),
                    TEST.replace('    def test_value', '    @unittest.expectedFailure\n    def test_value')
                        .replace('candidate.VALUE, 2', 'candidate.VALUE, 3')]
        for text in variants:
            with self.subTest(text=text):
                self.assertNotEqual(self.execute(text)['status'], 'PASS')

    def test_worker_rejects_zero_duplicate_missing_and_partial_execution(self):
        variants = [(TEST + '\n    def run(self, result=None): return result\n', [CASE]),
                    (TEST, [CASE, CASE]), (TEST, [CASE + '_missing']),
                    (TEST + '\nimport os; os._exit(0)\n', [CASE])]
        for text, names in variants:
            with self.subTest(names=names):
                self.assertNotEqual(self.execute(text, names)['status'], 'PASS')

    def test_worker_timeout_is_not_success(self):
        text = TEST.replace('self.assertEqual(candidate.VALUE, 2)', 'import time; time.sleep(10)')
        self.assertEqual(self.execute(text, timeout=0.1)['status'], 'TIMEOUT')

    def test_unexecuted_async_and_generator_bodies_cannot_pass(self):
        bad = TEST.replace('self.assertEqual(candidate.VALUE, 2)', 'self.fail("must execute")')
        variants = [bad.replace('def test_value', 'async def test_value'),
                    bad.replace('self.fail', 'yield\n        self.fail'),
                    bad.replace('def test_value', 'async def test_value')
                       .replace('self.fail', 'yield\n        self.fail'),
                    bad.replace('self.fail("must execute")', 'return (x for x in [])')]
        for text in variants:
            with self.subTest(text=text):
                self.assertNotEqual(self.execute(text)['status'], 'PASS')


class PythonBuildTests(PythonFixture):
    def setUp(self):
        super().setUp()
        self.scripts = Path(__file__).resolve().parent
        for name in ('check_module_gate.py', 'module_single_source.py', 'module_gate_build.py',
                     'module_gate_python.py'):
            self.write(f'scripts/{name}', (self.scripts / name).read_text())
        self.base = self.commit('actual runner baseline')

    def build(self, head, policy=None):
        output = self.repo.parent / 'receipt'
        helpers = self.repo.parent / 'runner'
        helpers.mkdir(exist_ok=True)
        tree = gate.Tree(str(self.repo), policy or self.base)
        for name in ('check_module_gate.py', 'module_single_source.py', 'module_gate_build.py',
                     'module_gate_python.py'):
            (helpers / name).write_bytes(tree.read(f'scripts/{name}'))
        proc = subprocess.run([sys.executable, '-B', str(helpers / 'module_gate_build.py'),
                               '--repo', str(self.repo), '--base', self.base, '--head', head,
                               '--policy', policy or self.base, '--workflow', 'build.yml',
                               '--out', str(output)], capture_output=True, text=True)
        path = output / 'receipt.json'
        return proc, json.loads(path.read_text()) if path.exists() else None

    def test_actual_runner_executes_python_only_and_binds_result(self):
        head = self.candidate()
        proc, receipt = self.build(head)
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
        self.assertEqual(receipt['results'], [])
        self.assertEqual(receipt['python_results']['tests'], [{'id': CASE, 'status': 'PASS'}])
        self.assertEqual(receipt['head'], head)
        self.assertEqual(receipt['merge_base'], self.base)

    def test_actual_runner_failure_is_not_a_successful_receipt(self):
        self.candidate()
        self.write(SOURCE, 'VALUE = 3\n')
        proc, receipt = self.build(self.commit('bad candidate'))
        self.assertNotEqual(proc.returncode, 0)
        self.assertEqual(receipt['python_results']['status'], 'FAIL')

    def test_actual_runner_rejects_missing_declaration_before_execution(self):
        proc, receipt = self.build(self.candidate([]))
        self.assertNotEqual(proc.returncode, 0)
        self.assertIsNone(receipt)
        self.assertIn('no declared Python test', proc.stderr)

    def test_actual_runner_bootstrap_uses_head_policy_without_old_requirement(self):
        self.set_manifest(lambda d: d.pop('python_tests'))
        self.base = self.commit('before F1 adoption')
        self.candidate()
        self.set_manifest(lambda d: d.update(python_tests={SOURCE: ['test_candidate']}))
        head = self.commit('adopting F1')
        proc, receipt = self.build(head, policy=head)
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
        self.assertEqual(receipt['policy'], head)
        self.assertEqual(receipt['python_results']['status'], 'PASS')

    def test_broken_candidate_dependency_and_selection_fail_normal_and_bootstrap(self):
        checker = (self.scripts / 'check_module_gate.py').read_text()
        self.write('scripts/test_check_module_gate.py',
                   (self.scripts / 'test_check_module_gate.py').read_text())
        self.set_manifest(lambda d: d['python_tests'].update({
            'scripts/check_module_gate.py': ['test_check_module_gate']}))
        self.base = self.commit('candidate checker test inputs')
        mutations = [
            ('def dependency_findings(manifest, analysis):',
             'def dependency_findings(manifest, analysis):\n    return []',
             'test_check_module_gate.ModuleGateTests.test_feature_dependency_on_the_other_feature_is_forbidden'),
            ('    if not rust:\n', '    if True:\n',
             'test_check_module_gate.ReviewFindingTests.test_changed_test_cases_are_selected_exactly')]
        for old, new, name in mutations:
            self.write('scripts/check_module_gate.py', checker.replace(old, new, 1))
            self.write('module-gate/records/python.json', json.dumps({
                'schema': gate.RECORD_SCHEMA, 'python_tests': [name]}))
            head = self.commit('known-bad candidate checker')
            for policy in (self.base, head):
                with self.subTest(test=name, bootstrap=policy == head):
                    proc, receipt = self.build(head, policy)
                    self.assertNotEqual(proc.returncode, 0, proc.stdout)
                    self.assertEqual(receipt['python_results']['status'], 'FAIL')
                    self.assertIn(name, (self.repo.parent / 'receipt/python-tests.log').read_text())


class PythonWorkflowTests(unittest.TestCase):
    def test_unprivileged_bootstrap_and_trusted_control_stay_separate(self):
        root = Path(__file__).resolve().parent.parent
        build = (root / '.github/workflows/module-build.yml').read_text()
        control = (root / '.github/workflows/module-gate.yml').read_text()
        self.assertIn('contents: read', build)
        self.assertNotIn('statuses: write', build)
        self.assertNotIn('${{ secrets.', build)
        self.assertIn('persist-credentials: false', build)
        self.assertIn('ref: ${{ github.event.pull_request.head.sha }}', build)
        self.assertIn('F1 adoption:', build)
        self.assertIn('module_gate_python.py', build)
        self.assertIn("env.XADB_RUST == 'true'", build)
        self.assertIn("env.XADB_TOOLCHAIN == 'true'", build)
        self.assertIn('pull_request_target:', control)
        self.assertNotIn('module_gate_python.py', control)
        self.assertNotIn('pull_request.head.sha }}\n          fetch-depth:', control)
        self.assertIn('python3 -B scripts/check_module_gate.py final', control)


if __name__ == '__main__':
    unittest.main()
