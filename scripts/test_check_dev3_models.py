"""Synthetic receipt rejection checks, not finite-model execution evidence."""
import copy
import json
import unittest

from check_dev3_models import MODELS, check_model_output


def cases_for(model):
    cases = []
    for fixture, required in model.normal.items():
        cases.append({
            'scope': model.scope, 'fixture': fixture, 'mutant': 'none', 'status': 'PASS',
            'states': 2, 'transitions': 3, 'max_depth': 1, 'bound_depth': 16,
            'elapsed_ms': 1, 'unexplored': 0, 'error': None, 'violation': None,
            'witnesses': {name: ([] if model.empty_witness else [sorted(model.actions)[0]])
                          for name in required}, 'witnesses_replayed': True,
            'counterexample': [], 'counterexample_replayed': False,
        })
    cases[0]['witnesses'].update({name: [] for name in model.aggregate})
    for (fixture, mutant), violation in model.negative.items():
        cases.append({
            'scope': model.scope, 'fixture': fixture, 'mutant': mutant,
            'status': 'EXPECTED_COUNTEREXAMPLE', 'states': 2, 'transitions': 3,
            'max_depth': 1, 'bound_depth': 16, 'elapsed_ms': 1, 'unexplored': 1,
            'error': None, 'violation': violation, 'witnesses': {},
            'witnesses_replayed': False, 'counterexample': [sorted(model.actions)[0]],
            'counterexample_replayed': True,
        })
    if model.preludes is not None:
        for case in cases:
            case['prelude'] = model.preludes[case['fixture']].copy()
    return cases


def output_for(model, cases):
    lines = [model.prefix + ' ' + json.dumps(case) for case in cases]
    if model.completion:
        lines.append(model.completion + ' ' + json.dumps({
            'scope': model.scope, 'status': 'PASS', 'normal_cases': len(model.normal),
            'negative_cases': len(model.negative)}))
    lines += [f'test {model.test} ... ok',
              'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;']
    return '\n'.join(lines)


class ModelReceiptTests(unittest.TestCase):
    def test_explicit_inventories_accept_complete_synthetic_receipts(self):
        self.assertEqual((len(MODELS['M01'].normal), len(MODELS['M01'].negative)), (16, 12))
        self.assertEqual((len(MODELS['M02'].normal), len(MODELS['M02'].negative)), (13, 23))
        self.assertEqual((len(MODELS['M04'].normal), len(MODELS['M04'].negative)), (5, 10))
        self.assertEqual((len(MODELS['M05'].normal), len(MODELS['M05'].negative)), (3, 9))
        for model in MODELS.values():
            cases = cases_for(model)
            self.assertEqual(check_model_output(output_for(model, cases), 0, model), cases)

    def test_missing_duplicate_unexpected_and_malformed_cases_fail(self):
        for model in MODELS.values():
            cases = cases_for(model)
            for rows in (cases[:-1], cases + [cases[0]], [None] + cases[1:],
                         [['not an object']] + cases[1:]):
                with self.subTest(scope=model.scope, rows=rows[:1]):
                    with self.assertRaises(ValueError):
                        check_model_output(output_for(model, rows), 0, model)
            for field, value in [('fixture', 'unknown'), ('mutant', []), ('scope', 'other'),
                                 ('fixture', {}), ('mutant', 'unexpected')]:
                changed = copy.deepcopy(cases)
                changed[0][field] = value
                with self.subTest(scope=model.scope, field=field):
                    with self.assertRaises(ValueError):
                        check_model_output(output_for(model, changed), 0, model)

    def test_inconclusive_wrong_types_and_invalid_witnesses_fail(self):
        changes = [
            ('status', 'INCONCLUSIVE'), ('states', True), ('states', 0),
            ('states', 1_000_001), ('transitions', 0), ('transitions', 8_000_001),
            ('elapsed_ms', 60_000), ('elapsed_ms', -1), ('max_depth', 17),
            ('unexplored', 1), ('unexplored', False), ('witnesses', {}),
            ('witnesses', []), ('counterexample', None), ('error', 'state_cap'),
            ('violation', 'failure'),
            ('missing_witnesses', ['missing']), ('missing_witnesses', None),
        ]
        for model in MODELS.values():
            for field, value in changes:
                changed = cases_for(model)
                changed[0][field] = value
                with self.subTest(scope=model.scope, field=field, value=value):
                    with self.assertRaises(ValueError):
                        check_model_output(output_for(model, changed), 0, model)
            for invalid in (['unknown_action'], ['cut:23'], ['capture'] * 17, 'not a trace'):
                changed = cases_for(model)
                name = next(iter(changed[0]['witnesses']))
                changed[0]['witnesses'][name] = invalid
                with self.assertRaises(ValueError):
                    check_model_output(output_for(model, changed), 0, model)
        changed = cases_for(MODELS['M01'])
        changed[0]['bound_depth'] = 15
        with self.assertRaises(ValueError):
            check_model_output(output_for(MODELS['M01'], changed), 0, MODELS['M01'])
        model = MODELS['M02']
        for prelude in (None, ['copy_root'], ['copy_root'] * 19):
            changed = cases_for(model)
            changed[0]['prelude'] = prelude
            with self.assertRaises(ValueError):
                check_model_output(output_for(model, changed), 0, model)

    def test_counterexample_and_replay_are_required(self):
        for model in MODELS.values():
            for field, value in [('counterexample', []), ('counterexample', ['bad']),
                                 ('violation', 'wrong_error'), ('status', 'PASS')]:
                changed = cases_for(model)
                changed[-1][field] = value
                with self.subTest(scope=model.scope, field=field):
                    with self.assertRaises(ValueError):
                        check_model_output(output_for(model, changed), 0, model)
        model = MODELS['M05']
        for index, field in ((0, 'witnesses_replayed'), (-1, 'counterexample_replayed')):
            for invalid in (False, None, 1, 'true'):
                changed = cases_for(model)
                changed[index][field] = invalid
                with self.assertRaises(ValueError):
                    check_model_output(output_for(model, changed), 0, model)

    def test_execution_summary_completion_and_json_duplicates_fail(self):
        for model in MODELS.values():
            output = output_for(model, cases_for(model))
            test = f'test {model.test} ... ok'
            for changed, code in (
                    (output, 1), (output.replace(test, ''), 0),
                    (output + '\n' + test, 0),
                    (output + '\ntest result: ok. 1 passed; 0 failed; 0 ignored;', 0),
                    (output.replace('0 ignored;', '1 ignored;'), 0),
                    (output.replace('"states": 2', '"states": 2, "states": 2', 1), 0)):
                with self.assertRaises(ValueError):
                    check_model_output(changed, code, model)
            if model.completion:
                with self.assertRaises(ValueError):
                    check_model_output(output.replace(model.completion, 'missing'), 0, model)

    def test_campaign_coverage_is_not_inferred_from_case_counts(self):
        model = MODELS['M01']
        changed = cases_for(model)
        del changed[0]['witnesses']['undo_reused_3']
        with self.assertRaises(ValueError):
            check_model_output(output_for(model, changed), 0, model)


if __name__ == '__main__':
    unittest.main()
