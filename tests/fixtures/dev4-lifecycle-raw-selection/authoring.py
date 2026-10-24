"""Static, independent input authoring; no model/product/test execution.

This emits JSON data only. Ordered literal axes come from owner/wire contracts,
not the research expanded inventory or a production index/inverse algorithm.
Selector expectations are literal tuples, never resolved from a model state.
"""
from pathlib import Path
from itertools import product
import copy
import hashlib
import json

HERE = Path(__file__).resolve().parent
SOURCE = json.loads((HERE / 'recipe-source.json').read_text())


def save(name, value):
    (HERE / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def target(number):
    # This maps a literal axis label, not an input index or live target history.
    return {'kind': 'Issued', 'index': number} if number < 6 else {'kind': 'FrozenUnissued', 'index': number - 6}


def raw(api, actor, outcome, selector=None, prepare=None, mode=None):
    return {'api': api, 'actor': actor, 'selector': selector, 'prepare': prepare,
            'autocommit': mode, 'outcome': outcome}


# The authoring order is a direct Cartesian traversal of written contract axes.
# The future product uses index decoding; it does not share this authoring code.
rows = []


def add_raw(action):
    index = len(rows)
    rows.append({'id': f'raw/{index:04}', 'group': 'raw1533',
                 'index': index, 'expected_action': action, 'expected_inverse': index})


for actor, slot, shape, n, outcome in product([0, 1], [0, 1], ['Select', 'Dml'], [0, 1], [0, 1, 2]):
    add_raw(raw('Prepare', actor, outcome, prepare={'slot': slot, 'shape': shape, 'parameter_count': n}))
for api, outcomes in [
    ('BindAll', list(range(11))), ('Execute', list(range(11))),
    ('Fetch', list(range(10))), ('CloseResult', [0, 1, 2]), ('CloseStatement', [0, 1, 2]),
]:
    for actor, selector, outcome in product([0, 1], list(range(16)), outcomes):
        add_raw(raw(api, actor, outcome, selector=target(selector)))
for actor, outcome in product([0, 1], [0, 1, 2]):
    add_raw(raw('Begin', actor, outcome))
for actor, mode, outcome in product([0, 1], ['On', 'Off'], [0, 1, 2]):
    add_raw(raw('SetAutocommit', actor, outcome, mode=mode))
for api, outcomes in [
    ('CommitAtomic', list(range(10))), ('Rollback', [0, 1, 2, 3]),
    ('CloseSession', [0, 1, 2, 3]), ('ReopenSession', [0, 1]),
]:
    for actor, outcome in product([0, 1], outcomes):
        add_raw(raw(api, actor, outcome))
for actor, selector in product([0, 1], list(range(16))):
    add_raw(raw('InspectStatement', actor, 0, selector=target(selector)))
for actor in [0, 1]:
    add_raw(raw('InspectSession', actor, 0))
for api in ['SignalContext', 'ParkFetch']:
    for actor, selector, outcome in product([0, 1], list(range(16)), [0, 1]):
        add_raw(raw(api, actor, outcome, selector=target(selector)))
for api, outcomes in [
    ('ResumeFetch', list(range(10))), ('BeginCommit', [0, 1]),
    ('CommitFact', [0, 1, 2, 3, 4]), ('FinishCommit', [0, 1, 2, 3, 4]),
]:
    for actor, outcome in product([0, 1], outcomes):
        add_raw(raw(api, actor, outcome))
for outcome in [0, 1, 2, 3, 4]:
    add_raw(raw('RecoveryConfirmedFreshEngine', 255, outcome))

# These are static authoring consistency checks, not execution of the resolver.
assert len(rows) == 1533
for api, first, last, count, _axes in SOURCE['families']:
    assert rows[first]['expected_action']['api'] == api
    assert rows[last]['expected_action']['api'] == api
    assert last - first + 1 == count
save('raw-descriptors.json', {'status': 'INDEPENDENT_STATIC_EXPECTED_NOT_RUNTIME', 'cases': rows})

invalid_indices = [{'id': f'raw-index/{n}', 'group': 'invalid_index', 'index': n,
                    'expected_error': {'OutOfRange': {'index': n}}} for n in [1533, 1534, 65535]]
invalid_fields = []


def invalid(api, label, action):
    invalid_fields.append({'id': f'raw-fields/{api}/{label}', 'group': 'invalid_fields',
                           'action': action, 'expected_error': 'InvalidFields'})


for api, first, last, _count, _axes in SOURCE['families']:
    base = rows[first]['expected_action']
    changed = copy.deepcopy(base)
    changed['actor'] = 0 if api == 'RecoveryConfirmedFreshEngine' else 2
    invalid(api, 'actor-domain', changed)
    changed = copy.deepcopy(base)
    changed['outcome'] = rows[last]['expected_action']['outcome'] + 1
    invalid(api, 'outcome-domain', changed)
    if base['selector'] is not None:
        changed = copy.deepcopy(base)
        changed['selector'] = None
        invalid(api, 'missing-selector', changed)
        for kind, index in [('Issued', 6), ('FrozenUnissued', 10)]:
            changed = copy.deepcopy(base)
            changed['selector'] = {'kind': kind, 'index': index}
            invalid(api, f'{kind}-domain', changed)
    else:
        changed = copy.deepcopy(base)
        changed['selector'] = {'kind': 'Issued', 'index': 0}
        invalid(api, 'unused-selector-present', changed)
    if api == 'Prepare':
        changed = copy.deepcopy(base)
        changed['prepare'] = None
        invalid(api, 'missing-prepare', changed)
        for field in ['slot', 'parameter_count']:
            changed = copy.deepcopy(base)
            changed['prepare'][field] = 2
            invalid(api, f'prepare-{field}-domain', changed)
    else:
        changed = copy.deepcopy(base)
        changed['prepare'] = {'slot': 0, 'shape': 'Select', 'parameter_count': 0}
        invalid(api, 'unused-prepare-present', changed)
    changed = copy.deepcopy(base)
    if api == 'SetAutocommit':
        changed['autocommit'] = None
        invalid(api, 'missing-autocommit', changed)
    else:
        changed['autocommit'] = 'On'
        invalid(api, 'unused-autocommit-present', changed)
assert len(invalid_fields) == 123
save('raw-invalid.json', {'invalid_index': invalid_indices, 'invalid_fields': invalid_fields})


def empty_state(actor=0, engine=1, session=1):
    return {'current_engine': 1, 'actors': [{'actor': actor, 'engine': engine, 'session': session}],
            'prepared_slots': [], 'issued_statements': [], 'issued_results': [], 'contexts': [],
            'next_nonce': {'Statement': 1, 'Result': 1, 'Context': 1}, 'poison_default_record_tables': []}


initial = empty_state()
initial['actors'].append({'actor': 1, 'engine': 1, 'session': 2})
for actor, slot in product([0, 1], [0, 1]):
    initial['prepared_slots'].append({'actor': actor, 'slot': slot,
                                     'next_statement_generation': 1, 'next_result_generation': 1})


def hex_identity(parts):
    # Hand-written wire positions only. No existing packing/codec helper import.
    kind, actor, slot, engine, session, nonce, generation = parts
    return (bytes([kind, actor, slot, 0]) + engine.to_bytes(2, 'little')
            + session.to_bytes(2, 'little') + nonce.to_bytes(2, 'little')
            + generation.to_bytes(2, 'little') + b'\0\0\0\0').hex()


selectors = []
TABLE = {'Statement': 'issued_statements', 'Result': 'issued_results', 'Context': 'contexts'}
KIND = {'Statement': 3, 'Result': 4, 'Context': 5}


def selector_case(id_, group, state, kind, selector, expected=None, error=None, actor=0, full_literal=None):
    assert (expected is None) != (error is None)
    row = {'id': id_, 'group': group, 'input': {'actor': actor, 'kind': kind, 'selector': selector},
           'expected_raw_tuple': expected, 'expected_raw_hex': None if expected is None else hex_identity(expected),
           'expected_error': error, 'state_must_be_unchanged': True}
    if full_literal is None:
        row['read_set'] = copy.deepcopy(state)
    else:
        row['full_literal'] = full_literal
    selectors.append(row)


for kind, actor in product(['Statement', 'Result', 'Context'], [0, 1]):
    for ordinal in range(6):
        selector_case(f'selector/baseline/{kind}/A{actor}/Issued{ordinal}', 'selector_baseline', initial,
                      kind, {'Issued': ordinal}, expected=[0, 0, 0, 0, 0, 0, 0], actor=actor, full_literal='initial')
    for u, parts in enumerate(SOURCE['baseline_selector_matrix']['U0_to9_exact_tuples'][f'{kind}/A{actor}']):
        selector_case(f'selector/baseline/{kind}/A{actor}/U{u}', 'selector_baseline', initial,
                      kind, {'FrozenUnissued': u}, expected=parts, actor=actor, full_literal='initial')


def issued_record(kind, identity, ordinal, history=False):
    row = {'id': identity, 'issue_ordinal': ordinal}
    if kind == 'Statement':
        row.update(life='Retired' if history else 'Current',
                   retirement='SessionClose' if history else 'None')
    elif kind == 'Result':
        row.update(retirement='EngineFence' if history else 'None', public_generation_live=not history)
    else:
        # Deliberately default parked payload: selector must read no payload.
        row['context_data'] = 'ParkedFetchDefault' if history and ordinal == 1 else 'Completed'
    return row


for control in SOURCE['supplemental_history_readset']['namespace_controls']:
    kind = control['kind']
    state = empty_state(engine=2, session=3)
    state['current_engine'] = 7
    state['next_nonce'][kind] = 3
    state[TABLE[kind]] = [issued_record(kind, r['identity'], r['ordinal'], history=True)
                          for r in control['canonical_records']]
    for ordinal, expected in enumerate(control['expected_issued_0_1_2']):
        selector_case(f'selector/history/{kind}/Issued{ordinal}', 'selector_history', state,
                      kind, {'Issued': ordinal}, expected=expected)

namespace = empty_state(engine=2, session=3)
namespace['current_engine'] = 7
namespace['prepared_slots'] = [
    {'actor': 0, 'slot': 0, 'next_statement_generation': 2, 'next_result_generation': 5},
    {'actor': 0, 'slot': 1, 'next_statement_generation': 4, 'next_result_generation': 7},
]
for kind in ['Statement', 'Result', 'Context']:
    for u, expected in zip([2, 3, 6, 7], SOURCE['supplemental_namespace_readset']['expected_tuples'][kind]):
        selector_case(f'selector/namespace/{kind}/U{u}', 'selector_namespace', namespace,
                      kind, {'FrozenUnissued': u}, expected=expected)


def single(kind):
    state = empty_state()
    identity = [KIND[kind], 0, 255 if kind == 'Context' else 0, 1, 1, 1, 0 if kind == 'Context' else 1]
    state[TABLE[kind]] = [issued_record(kind, identity, 1)]
    state['next_nonce'][kind] = 2
    return state


def err(label, state, error, kind='Statement', selector=None, actor=0):
    selector_case(f'selector/error/{label}', 'local_errors', state, kind,
                  {'Issued': 0} if selector is None else selector, error=error, actor=actor)


def control(label, state, expected, kind='Statement', selector=None):
    selector_case(f'selector/control/{label}', 'readset_control', state, kind,
                  {'Issued': 0} if selector is None else selector, expected=expected)


for actor in [2, 255]:
    err(f'SC01/actor{actor}', initial, 'InvalidActor', actor=actor)
for kind, ordinal in [('Issued', 6), ('Issued', 255), ('FrozenUnissued', 10), ('FrozenUnissued', 255)]:
    err(f'SC01/{kind}{ordinal}', initial, 'InvalidSelector', selector={kind: ordinal})
err('SC01/actor2-and-Issued6', initial, 'InvalidActor', actor=2, selector={'Issued': 6})
err('SC01/FrozenU0-actor2', initial, 'InvalidActor', actor=2, selector={'FrozenUnissued': 0})
state = single('Statement')
state['actors'] = []
err('SC02/FrozenU0-missing-actor', state, 'MissingActor', selector={'FrozenUnissued': 0})
state = single('Statement')
state['actors'][0]['engine'] = 0
err('SC03/FrozenU0-caller-engine0', state, 'InvalidCallerScope', selector={'FrozenUnissued': 0})
state = single('Statement')
state['actors'] = []
err('SC02/missing-actor', state, 'MissingActor')
state = single('Statement')
state['actors'].append(copy.deepcopy(state['actors'][0]))
err('SC02/duplicate-actor', state, 'DuplicateActor')
state['actors'][0]['engine'] = 0
err('SC02/duplicate-before-bad-scope', state, 'DuplicateActor')
for field, value in [('engine', 0), ('engine', 8), ('session', 0), ('session', 9)]:
    state = single('Statement')
    state['actors'][0][field] = value
    err(f'SC03/{field}{value}', state, 'InvalidCallerScope')
for engine, session in [(1, 1), (7, 8)]:
    state = single('Statement')
    state['actors'][0].update(engine=engine, session=session)
    control(f'SC03/valid-E{engine}-S{session}', state, [3, 0, 0, 1, 1, 1, 1])

for kind in ['Statement', 'Result', 'Context']:
    table = TABLE[kind]
    mutations = [('ordinal', -1, 0), ('ordinal', -1, 7), ('kind', 0, 0),
                 ('actor', 1, 2), ('actor', 1, 255),
                 ('slot', 2, 0 if kind == 'Context' else 2), ('slot', 2, 2 if kind == 'Context' else 255),
                 ('engine', 3, 0), ('engine', 3, 8), ('session', 4, 0), ('session', 4, 9),
                 ('nonce', 5, 0), ('nonce', 5, 7),
                 ('generation', 6, 1 if kind == 'Context' else 0), ('generation', 6, 255 if kind == 'Context' else 13)]
    for label, position, value in mutations:
        state = single(kind)
        if position < 0:
            state[table][0]['issue_ordinal'] = value
        else:
            state[table][0]['id'][position] = value
        err(f'SC04/{kind}/{label}{value}', state, 'InvalidIssuedRecord', kind)
    state = single(kind)
    other = copy.deepcopy(state[table][0])
    other['issue_ordinal'] = 2
    other['id'][5] = 2
    other['id'][3] = 0
    state[table].append(other)
    state['next_nonce'][kind] = 3
    err(f'SC04/{kind}/nonrequested-row-malformed', state, 'InvalidIssuedRecord', kind)
    state = single(kind)
    id_ = [KIND[kind], 1, 255 if kind == 'Context' else 1, 7, 8, 6, 0 if kind == 'Context' else 12]
    state[table] = [issued_record(kind, id_, 6)]
    state['next_nonce'][kind] = 7
    control(f'SC04/{kind}/maximum-primitive', state, id_, kind, {'Issued': 5})
    state = single(kind)
    second = copy.deepcopy(state[table][0])
    second['id'][5] = 2
    third = copy.deepcopy(state[table][0])
    third['issue_ordinal'] = 3
    third['id'][5] = 3
    third['id'][3] = 0
    state[table] += [second, third]
    state['next_nonce'][kind] = 4
    err(f'SC05/{kind}/late-primitive-before-early-duplicate', state, 'InvalidIssuedRecord', kind)
    for label, ordinal, nonce, expected in [
        ('ordinal-only', 1, 2, 'DuplicateOrdinal'), ('nonce-only', 2, 1, 'DuplicateNonce'),
        ('both', 1, 1, 'DuplicateOrdinal')]:
        state = single(kind)
        second = copy.deepcopy(state[table][0])
        second['issue_ordinal'] = ordinal
        second['id'][5] = nonce
        state[table].append(second)
        state['next_nonce'][kind] = 3 if nonce == 2 else 2
        err(f'SC06/{kind}/{label}', state, expected, kind)
    state = single(kind)
    second = copy.deepcopy(state[table][0])
    second['issue_ordinal'] = 2
    third = copy.deepcopy(second)
    third['id'][5] = 2
    state[table] += [second, third]
    state['next_nonce'][kind] = 3
    err(f'SC06/{kind}/later-ordinal-before-earlier-nonce', state, 'DuplicateOrdinal', kind)
    for value in [0, 1, 3]:
        state = single(kind)
        state['next_nonce'][kind] = value
        err(f'SC07/{kind}/next{value}', state, 'InvalidNextNonce', kind)
    state = single(kind)
    state['next_nonce'][kind] = 0
    err(f'SC07/{kind}/bad-next-before-missing-ordinal', state, 'InvalidNextNonce', kind, {'Issued': 1})
    state = empty_state()
    state['next_nonce'][kind] = 0
    err(f'SC07/{kind}/empty-with-next0', state, 'InvalidNextNonce', kind)

for kind, slot in product(['Statement', 'Result'], [0, 1]):
    generation_field = 'next_statement_generation' if kind == 'Statement' else 'next_result_generation'
    other_field = 'next_result_generation' if kind == 'Statement' else 'next_statement_generation'
    u = 6 + slot
    selector = {'FrozenUnissued': u}
    expected = SOURCE['supplemental_namespace_readset']['expected_tuples'][kind][2 + slot]
    state = copy.deepcopy(namespace)
    state['prepared_slots'] = [r for r in state['prepared_slots'] if r['slot'] != slot]
    err(f'SC08/{kind}/slot{slot}-missing', state, 'MissingSlot', kind, selector)
    state = copy.deepcopy(namespace)
    state['prepared_slots'].append(copy.deepcopy(state['prepared_slots'][slot]))
    err(f'SC08/{kind}/slot{slot}-duplicate', state, 'DuplicateSlot', kind, selector)
    for value in [0, 14]:
        state = copy.deepcopy(namespace)
        state['prepared_slots'][slot][generation_field] = value
        err(f'SC08/{kind}/slot{slot}-generation{value}', state, 'InvalidGeneration', kind, selector)
    for value in [1, 13]:
        state = copy.deepcopy(namespace)
        state['prepared_slots'][slot][generation_field] = value
        parts = copy.deepcopy(expected)
        parts[6] = value
        control(f'SC08/{kind}/slot{slot}-generation{value}', state, parts, kind, selector)
    state = copy.deepcopy(namespace)
    state['prepared_slots'].append(copy.deepcopy(state['prepared_slots'][slot]))
    state['prepared_slots'][slot][generation_field] = 0
    err(f'SC09/{kind}/slot{slot}-duplicate-before-generation', state, 'DuplicateSlot', kind, selector)
    state = copy.deepcopy(namespace)
    state['prepared_slots'][slot][other_field] = 0
    control(f'SC09/{kind}/slot{slot}-other-kind-generation-poison', state, expected, kind, selector)
    state = copy.deepcopy(namespace)
    state['prepared_slots'][1 - slot][generation_field] = 0
    state['prepared_slots'].append(copy.deepcopy(state['prepared_slots'][1 - slot]))
    control(f'SC09/{kind}/slot{slot}-other-slot-duplicate-poison', state, expected, kind, selector)

expected = [3, 0, 0, 1, 1, 1, 1]
state = single('Statement')
for kind in ['Result', 'Context']:
    state[TABLE[kind]] = [issued_record(kind, [0, 255, 2, 0, 0, 0, 255], 0)]
    state['next_nonce'][kind] = 65535
control('SC10/unselected-namespaces', state, expected)
state = single('Statement')
state['actors'].append({'actor': 1, 'engine': 0, 'session': 0})
control('SC10/unrequested-actor-scope', state, expected)
state = single('Statement')
state['prepared_slots'] = [{'actor': 0, 'slot': 0, 'next_statement_generation': 0, 'next_result_generation': 0}] * 4
control('SC10/unused-slot-table', state, expected)
state = single('Statement')
state['poison_default_record_tables'] = ['engines', 'session_incarnations']
control('SC10/unused-engine-session-tables', state, expected)
state = single('Statement')
state['poison_default_record_tables'] = ['owners', 'facts', 'receipts']
control('SC10/unused-owner-fact-receipt-tables', state, expected)
for kind in ['Statement', 'Result', 'Context']:
    for u, expected in enumerate(SOURCE['baseline_selector_matrix']['U0_to9_exact_tuples'][f'{kind}/A0']):
        state = copy.deepcopy(initial)
        state[TABLE[kind]] = [issued_record(kind, [0, 255, 2, 0, 0, 0, 255], 0)]
        state['next_nonce'][kind] = 65535
        control(f'SC11/{kind}/U{u}-ignored-history-and-next', state, expected, kind, {'FrozenUnissued': u})
for u in [6, 7]:
    expected = SOURCE['baseline_selector_matrix']['U0_to9_exact_tuples']['Context/A0'][u]
    for label in ['empty', 'poisoned']:
        state = copy.deepcopy(initial)
        state['prepared_slots'] = [] if label == 'empty' else [
            {'actor': 0, 'slot': 0, 'next_statement_generation': 0, 'next_result_generation': 65535}] * 4
        control(f'SC12/U{u}-{label}-slot-table', state, expected, 'Context', {'FrozenUnissued': u})

groups = {}
for case in selectors:
    groups[case['group']] = groups.get(case['group'], 0) + 1
assert groups == {'selector_baseline': 96, 'selector_history': 9, 'selector_namespace': 12,
                  'local_errors': 115, 'readset_control': 60}, groups
save('selectors.json', {
    'status': 'STATIC_SELECTOR_EXPECTED_NOT_FULL_STATE_OR_MODEL_EVIDENCE',
    'readset_expansion': 'Start from State::default in an existing heap workspace; replace only explicitly listed scalar/table fields. Set specified kind next_nonce entries. Each row starts from its schema record default, then exact stated fields are assigned. Context data names mean Completed or ParkedFetch(ParkedContext::default()). Poison table names mean insert one unchanged Default record. No loader/model validation is called. Unspecified fields are intentionally outside the selector read-set, not alleged valid model history.',
    'complete_literal_source': {'path': 'tests/fixtures/dev4-lifecycle-model/literals.json',
                                'sha256': 'ec4a7940a457882c155eb84e2d9e5602eaf880563bab4f6f04d0b643bcec42d2'},
    'group_counts': groups, 'cases': selectors,
})
all_cases = rows + invalid_indices + invalid_fields + selectors
ids = [c['id'] for c in all_cases]
assert len(ids) == len(set(ids)) == 1951
save('selection-inventory.json', {
    'ordered_case_ids': ids,
    'groups': {'raw1533': 1533, 'invalid_index': 3, 'invalid_fields': 123, **groups},
    'logical_cases': 1951,
    'planned_api_calls': {'action_at': 1536, 'index_of': 1656, 'resolve_selector': 292, 'total': 3484},
    'execution_contract': 'Run each selected case once; emit START/COMPLETED id and exact direction. On first failure retain FAIL and leave suffix NOT_RUN. Resume by explicit IDs or half-open group range. A raw case has two directions, which need separate completion markers to avoid repeating a completed direction after failure.',
    'stack_plan': 'Future new-group body can run initially on a 256KiB thread. Parse JSON outside it. No second stack-test replay of completed cases. No allocator measurement claim; heap0 starts as static source evidence.',
    'not_run': ['all product API calls', 'full ResolvedInput assembly', 'model/checker', 'BFS', 'mandatory51action assembly'],
})
files = ['recipe-source.json', 'authoring.py', 'raw-descriptors.json', 'raw-invalid.json', 'selectors.json', 'selection-inventory.json']
save('manifest.json', {
    'id': 'DEV4-LM-INDEPENDENT-RAW-SELECTOR-INPUTS-V3',
    'status': 'MATERIALIZED_STATIC_INPUTS_PENDING_FINAL_D2',
    'derivation': 'Literal owner axes and separately written raw tuples/read-set errors only; no product/research expanded-inventory/model/codec calls or imports.',
    'logical_cases': 1951, 'planned_api_calls': 3484,
    'files': [{'path': f, 'bytes': (HERE / f).stat().st_size, 'sha256': hashlib.sha256((HERE / f).read_bytes()).hexdigest()} for f in files],
    'execution': {'static_authoring_only': True, 'product_calls': 0, 'compile': False, 'tests': False, 'runtime_PASS': False},
})
print(json.dumps({'static_authoring_only': True, 'logical_cases': 1951, 'planned_api_calls_not_executed': 3484, 'selector_groups': groups}))
