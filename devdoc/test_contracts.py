"""Negative controls for XADB documentation contract verification."""
import copy
import json
from pathlib import Path
import unittest
import tempfile
from document_tools import DocumentIndex
from verify_documents import validate_capabilities, validate_evidence, validate_implementation_status, validate_packages, validate_policy_limits, validate_stage_graph

ROOT = Path(__file__).resolve().parent


class PackageAliasChecks(unittest.TestCase):
    def test_aliases_preserve_package_dependency_checks(self):
        roots = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tests', 'scripts', '.github']
        plan = {
            'packages': [
                dict(path='crates/base', name='xadb-foundation', status='IMPLEMENTED',
                     normal_workspace_dependencies=[]),
                dict(path='crates/user', name='xadb-engine', status='IMPLEMENTED',
                     normal_workspace_dependencies=['xadb-foundation']),
            ],
            'source_fingerprint_roots': roots, 'clean_export_roots': roots,
        }
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            (repo / 'crates/base').mkdir(parents=True)
            (repo / 'crates/user').mkdir()
            (repo / 'Cargo.toml').write_text(
                '[workspace]\nmembers = ["crates/base", "crates/user"]\n'
                '[workspace.dependencies]\n'
                'foundation = { package = "xadb-foundation", path = "crates/base" }\n')
            (repo / 'crates/base/Cargo.toml').write_text('[package]\nname = "xadb-foundation"\n')
            consumer = repo / 'crates/user/Cargo.toml'
            for declaration in (
                    'xadb-foundation = { path = "../base" }',
                    'foundation = { package = "xadb-foundation", path = "../base" }',
                    'foundation.workspace = true'):
                with self.subTest(declaration=declaration):
                    consumer.write_text('[package]\nname = "xadb-engine"\n[dependencies]\n' + declaration)
                    self.assertEqual(validate_packages(plan, repo), [])
                    wrong = copy.deepcopy(plan)
                    wrong['packages'][1]['normal_workspace_dependencies'] = []
                    self.assertIn('repository: dependency mismatch crates/user',
                                  validate_packages(wrong, repo))
            consumer.write_text('[package]\nname = "xadb-engine"\n[dependencies]\nmissing.workspace = true')
            self.assertIn('repository: missing inherited dependency missing', validate_packages(plan, repo))


class ContractChecks(unittest.TestCase):
    def setUp(self):
        index = DocumentIndex(ROOT)
        self.docs = {name: json.loads((ROOT / entry['path']).read_text())
                     for name, entry in index.entries.items() if entry['path'].endswith('.json')}

    def graph(self, stage=None, completion=None, scope=None):
        return validate_stage_graph(stage or self.docs['development-stages.json'],
                                    completion or self.docs['stage-completion.json'],
                                    scope or self.docs['phase1-scope.json'], self.docs)

    def test_cycle_is_rejected(self):
        stage = copy.deepcopy(self.docs['development-stages.json'])
        stage['stages'][0]['depends_on'] = ['DEV1']
        self.assertTrue(any('cycle' in e for e in self.graph(stage=stage)))

    def test_unknown_case_is_rejected(self):
        completion = copy.deepcopy(self.docs['stage-completion.json'])
        completion['stages'][0]['required_test_refs'][0]['ids'].append('NONEXISTENT')
        self.assertTrue(any('unknown case' in e for e in self.graph(completion=completion)))

    def test_second_storage_is_rejected(self):
        scope = copy.deepcopy(self.docs['phase1-scope.json'])
        scope['initial_sql_baseline']['storage'].append('SECOND_ENGINE')
        self.assertTrue(any('disk storage only' in e for e in self.graph(scope=scope)))

    def test_removed_topic_reference_is_rejected(self):
        inventory = copy.deepcopy(self.docs['feature-design-inventory.json'])
        inventory['requirement_mappings'][0]['item_refs'].append('NONEXISTENT')
        errors = validate_capabilities(self.docs['capability-inventory.json'], inventory)
        self.assertTrue(any('unknown topic' in e for e in errors))

    def test_stale_source_evidence_is_rejected(self):
        report = {'status': 'PASS', 'source_sha256': 'old', 'source_sha256_before': 'old',
                  'source_sha256_after': 'old', 'checks': [{'status': 'PASS'}]}
        self.assertTrue(any('fingerprint mismatch' in e for e in validate_evidence(report, 'new')))

    def test_failed_check_cannot_hide_under_pass(self):
        report = {'status': 'PASS', 'source_sha256': 'same', 'source_sha256_before': 'same',
                  'source_sha256_after': 'same', 'checks': [{'status': 'FAIL'}]}
        self.assertTrue(any('failed' in e for e in validate_evidence(report, 'same')))

    def test_pending_implementation_preserves_historical_source_without_current_claim(self):
        status = self.docs['implementation-status.json']
        self.assertEqual(status['evidence_refs'], [])
        self.assertEqual(validate_implementation_status(status, self.docs, 'f' * 64), [])

    def test_historical_receipt_cannot_qualify_a_changed_current_source(self):
        status = copy.deepcopy(self.docs['implementation-status.json'])
        status.update(dev0='LOCAL_VERIFIED', dev1='LOCAL_VERIFIED',
                      evidence_refs=status.pop('historical_evidence_refs'))
        errors = validate_implementation_status(status, self.docs, 'f' * 64)
        self.assertTrue(any('fingerprint mismatch' in error for error in errors))

    def test_historical_status_keeps_pass_internal_hash_and_check_requirements(self):
        for field, value in (('status', 'FAIL'), ('source_sha256_before', 'e' * 64), ('checks', [])):
            with self.subTest(field=field):
                documents = copy.deepcopy(self.docs)
                documents['dev0-validation.json'][field] = value
                self.assertTrue(validate_implementation_status(documents['implementation-status.json'], documents, 'f' * 64))

    def test_historical_report_needs_a_real_recorded_source_hash(self):
        documents = copy.deepcopy(self.docs)
        report = documents['dev0-validation.json']
        for field in ('source_sha256', 'source_sha256_before', 'source_sha256_after'):
            report.pop(field)
        errors = validate_implementation_status(documents['implementation-status.json'], documents, 'f' * 64)
        self.assertTrue(any('recorded source identity' in error for error in errors))

    def test_reference_roles_cannot_overlap_or_admit_an_undeclared_kernel_report(self):
        status = copy.deepcopy(self.docs['implementation-status.json'])
        status['evidence_refs'] = ['dev0-validation.json']
        errors = validate_implementation_status(status, self.docs, 'f' * 64)
        self.assertTrue(any('overlap' in error for error in errors))
        status['evidence_refs'] = []
        status['historical_evidence_refs'].append('new-kernel-report.json')
        errors = validate_implementation_status(status, self.docs, 'f' * 64)
        self.assertTrue(any('undeclared report profile' in error for error in errors))

    def test_pending_implementation_cannot_claim_stage_product_or_release_completion(self):
        for field, value in (('durability', 'VERIFIED_PASS'), ('dev2', 'COMPLETE'),
                             ('product_qualification', 'PASS'), ('public_release', 'PASS')):
            with self.subTest(field=field):
                status = copy.deepcopy(self.docs['implementation-status.json'])
                status[field] = value
                self.assertTrue(validate_implementation_status(status, self.docs, 'f' * 64))

    def test_real_baseline_is_accepted(self):
        self.assertEqual(self.graph(), [])
        self.assertEqual(validate_policy_limits(self.docs), [])
        for stage in ('dev0', 'dev1', 'dev2'):
            report = self.docs[stage + '-validation.json']
            self.assertEqual(validate_evidence(report, report['source_sha256']), [])

    def test_empty_required_closure_is_rejected(self):
        completion = copy.deepcopy(self.docs['stage-completion.json'])
        next(s for s in completion['stages'] if s['id'] == 'DEV3')['required_test_refs'] = []
        self.assertTrue(any('required test closure' in e for e in self.graph(completion=completion)))

    def test_missing_disk_case_or_performance_case_is_rejected(self):
        for file in ('disk-test-matrix.json', 'ai-performance-engineering.json'):
            with self.subTest(file=file):
                completion = copy.deepcopy(self.docs['stage-completion.json'])
                stage = next(s for s in completion['stages'] if s['id'] == 'DEV3')
                next(r for r in stage['required_test_refs'] if r['file'] == file)['ids'].pop()
                self.assertTrue(any('required test closure' in e for e in self.graph(completion=completion)))

    def test_entry_prerequisite_and_false_completion_are_rejected(self):
        completion = copy.deepcopy(self.docs['stage-completion.json'])
        stage = next(s for s in completion['stages'] if s['id'] == 'DEV3')
        stage.update(entry_stage_refs=[], execution_status='PASS', pass_criteria=[])
        errors = self.graph(completion=completion)
        for fragment in ('entry prerequisite', 'unsupported execution status', 'missing pass criteria'):
            self.assertTrue(any(fragment in e for e in errors))

    def test_planned_acceptance_cannot_be_marked_pass(self):
        completion = copy.deepcopy(self.docs['stage-completion.json'])
        completion['cases'][0]['execution_status'] = 'PASS'
        self.assertTrue(any('planned acceptance' in e for e in self.graph(completion=completion)))

    def test_capability_backlog_cannot_be_marked_release_ready(self):
        capability = copy.deepcopy(self.docs['capability-inventory.json'])
        capability['current_external_release_eligible'] = True
        capability['families'][0]['leaf_requirements'][0]['execution_status'] = 'PASS'
        errors = validate_capabilities(capability, self.docs['feature-design-inventory.json'])
        self.assertTrue(any('qualify release' in e for e in errors))
        self.assertTrue(any('planned requirement' in e for e in errors))

    def test_dev2_leaf_report_cannot_claim_durability_or_protected_ci(self):
        report = copy.deepcopy(self.docs['dev2-validation.json'])
        report.update(product_durability='PASS', protected_ci='PASS')
        errors = validate_evidence(report, report['source_sha256'])
        self.assertTrue(any('protected release' in e for e in errors))
        self.assertTrue(any('product_durability' in e for e in errors))

    def test_missing_fixed_evidence_check_is_rejected(self):
        report = copy.deepcopy(self.docs['dev2-validation.json'])
        report['checks'].pop()
        self.assertTrue(any('fixed check inventory' in e for e in validate_evidence(report, report['source_sha256'])))

    def test_wrong_exit_cannot_hide_under_pass(self):
        report = copy.deepcopy(self.docs['dev1-validation.json'])
        next(c for c in report['checks'] if c['id'] == 'wrong-expectation-rejected')['exit_code'] = 0
        self.assertTrue(any('exit outcome' in e for e in validate_evidence(report, report['source_sha256'])))

    def test_corrupted_limit_key_is_rejected(self):
        docs = copy.deepcopy(self.docs)
        limits = docs['dev0-contract.json']['limits']
        limits['misspelled_input_bytes'] = limits.pop('max_input_bytes')
        self.assertTrue(any('max_input_bytes' in e for e in validate_policy_limits(docs)))


if __name__ == '__main__':
    unittest.main()
