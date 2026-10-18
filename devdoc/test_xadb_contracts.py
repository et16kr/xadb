"""Negative controls for XADB documentation contract verification."""
import copy
import json
from pathlib import Path
import unittest
from document_tools import DocumentIndex
from verify_documents import validate_capabilities, validate_evidence, validate_policy_limits, validate_stage_graph

ROOT = Path(__file__).resolve().parent


class XadbContractChecks(unittest.TestCase):
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
