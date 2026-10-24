#!/usr/bin/env python3
"""Independent static boundary literals. No product/model/codec/test execution.

Only named original JSON literals and explicit deltas are expanded. Expected
inputs are copied from frozen independent literals and changed at named fields;
no arbitrary-state assembler or namespace-selection algorithm supplies them.
"""
from pathlib import Path
import copy
import hashlib
import json

HERE = Path(__file__).resolve().parent
ART = HERE.parent
ROOT = ART.parents[1]
ORIG_PATH = ROOT / 'tests/fixtures/dev4-lifecycle-model/literals.json'
RAW_PATH = ROOT / 'tests/fixtures/dev4-lifecycle-raw-selection/raw-descriptors.json'
SOURCE = json.loads(ORIG_PATH.read_text())
RAW = json.loads(RAW_PATH.read_text())['cases']
NORMAL = ART / 'assembler-independent-inputs-v1'
INITIAL = {c['raw_index']: c['expected_input'] for c in json.loads((NORMAL/'initial.json').read_text())['cases']}
ORIGINAL = {c['id'].removeprefix('asm/original/'): c['expected_input'] for c in json.loads((NORMAL/'original-links.json').read_text())['cases']}
STATES, CASES = [], []
CLASSIFICATION = 'Exact conditional read-set workspace only; no global invariant or model reachability claim.'


def put(value, edits):
    for pointer, item in edits.items():
        parts = pointer.strip('/').split('/')
        parent = value
        for part in parts[:-1]:
            parent = parent[part]
        parent[parts[-1]] = copy.deepcopy(item)
    return value


def expand(value):
    if isinstance(value, str) and value.startswith('@'):
        return copy.deepcopy(SOURCE['identities'][value[1:]])
    if isinstance(value, list):
        return [expand(x) for x in value]
    if isinstance(value, dict):
        if '$value' in value:
            return expand(SOURCE['values'][value['$value']])
        if 'envelope_ref' in value:
            return named('envelopes', value['envelope_ref'])
        return {k: expand(v) for k, v in value.items()}
    return value


def named(section, name):
    row = SOURCE[section][name]
    if 'base' not in row:
        return expand(row)
    result = named(section, row['base'])
    for pointer in row.get('remove', []):
        parts = pointer.strip('/').split('/')
        parent = result
        for part in parts[:-1]:
            parent = parent[part]
        del parent[parts[-1]]
    return expand(put(result, row.get('put', {})))


def pre(label, base='initial', edits=None):
    identity = 'boundary:' + label
    STATES.append({'id': identity, 'base': 'original:' + base, 'put': edits or {}, 'classification': CLASSIFICATION})
    return identity


def add(label, state, index, purpose, error=None, expected=None, edits=None, action=None):
    row = {'id': 'asm/boundary/' + label, 'pre_state': state, 'raw_index': index,
           'raw_action': copy.deepcopy(action if action is not None else RAW[index]['expected_action']),
           'purpose': purpose}
    assert (error is None) != (expected is None)
    if error is not None:
        row['expected_error'] = {'kind': error} if isinstance(error, str) else error
    else:
        row['expected_input'] = put(copy.deepcopy(expected), edits or {})
    CASES.append(row)


def ident(kind, nonce, actor=0, session=1, engine=1, slot=255, gen=0):
    return [kind, actor, slot, engine, session, nonce, gen]


def completed(n):
    return {'id': ident(5, n), 'issue_ordinal': n, 'status': 'Completed'}


INIT = named('states', 'initial')
OPEN = named('states', 'sel.open')
PARKED = named('states', 'sel.parked')
PENDING = named('states', 'rw.recovery_pending')
P1 = ident(3, 1, slot=0, gen=1)
T1 = ident(6, 1)
R1 = ident(4, 1, slot=0, gen=1)

# Two NEW S0 ordering witnesses. The earlier 123 raw helper cases are not replayed.
bad_prepare = copy.deepcopy(RAW[0]['expected_action'])
bad_prepare['actor'] = 2
add('s0-before-missing-caller', pre('missing-all-callers', edits={'/actors': {}}), None,
    'InvalidAction before any missing caller lookup; Err out invalid and State unchanged.',
    error={'kind': 'InvalidAction', 'inner': 'InvalidFields'}, action=bad_prepare)
bad_execute = copy.deepcopy(RAW[400]['expected_action'])
bad_execute['prepare'] = {'slot': 0, 'shape': 'Select', 'parameter_count': 0}
add('s0-before-used-context-poison', pre('context-poison', edits={'/next_nonce/Context': 0}), None,
    'Invalid unused field before a newly used Context namespace guard.',
    error={'kind': 'InvalidAction', 'inner': 'InvalidFields'}, action=bad_execute)

# The existing selector errors are sampled ONLY at new phase crossings.
s = pre('selector-before-context-cap', 'sel.prepared', {
    '/issued_statements/1/issue_ordinal': 0,
    '/contexts/6': completed(6), '/next_nonce/Context': 7,
    '/prepared_slots/0:0/statement': ident(3, 6, slot=0, gen=1)})
add('selector-before-relation-and-cap', s, 400, 'S1 selected namespace error precedes S3 link and S4 cap.',
    error={'kind': 'Selector', 'inner': 'InvalidIssuedRecord'})
s = pre('result-selector-before-context-cap', edits={
    '/prepared_slots': {}, '/contexts/6': completed(6), '/next_nonce/Context': 7})
add('u6-selector-before-context-cap', s, 872, 'Result U6 requires slot before fresh-context cap.',
    error={'kind': 'Selector', 'inner': 'MissingSlot'})

# Non-target caller guards belong to this new API rather than selector replay.
add('nonselector-missing-caller', pre('missing-a0', edits={'/actors': {'1': INIT['actors']['1']}}), 1354,
    'InspectSession needs one actor row despite no context or environment.', error='InvalidState')
add('nonselector-duplicate-caller', pre('duplicate-a0', edits={'/actors': {'0': INIT['actors']['0'], '1': INIT['actors']['0']}}), 1264,
    'Fresh Begin requires caller uniqueness.', error='InvalidState')
add('nonselector-caller-scope', pre('invalid-attached-engine', edits={'/actors/0/engine': ident(1, 8, actor=255, session=0, engine=8)}), 1318,
    'Reopen prefix validates attached scalar range without transition logic.', error='InvalidState')

# Required relations precede valid-but-exhausted candidate namespaces.
bad_link = {'/prepared_slots/0:0/statement': ident(3, 6, slot=0, gen=1)}
s = pre('p-backlink-and-context-cap', 'sel.prepared', {**bad_link, '/contexts/6': completed(6), '/next_nonce/Context': 7})
add('p-backlink-before-context-cap', s, 400, 'Authenticated current P has missing slot backlink before cap.', error='InvalidState')
s = pre('descriptor-missing-and-context-cap', 'sel.open', {'/private_descriptors': {}, '/contexts/6': completed(6), '/next_nonce/Context': 7})
add('descriptor-before-context-cap', s, 752, 'Required current R descriptor missing before Context cap.', error='InvalidState')
s = pre('owner-holder-missing-and-context-cap', 'sel.open', {'/owners/3/current_holder': ident(4, 6, slot=0, gen=1), '/contexts/6': completed(6), '/next_nonce/Context': 7})
add('selected-owner-before-context-cap', s, 1072, 'Selected C owner has dangling before holder before cap.', error='InvalidState')
s = pre('o-backlink-and-context-cap', 'rw.recovery_pending', {'/transactions/1/obligation': ident(10, 6), '/contexts/6': completed(6), '/next_nonce/Context': 7})
add('selected-o-before-context-cap', s, 1310, 'Fenced CloseSession handoff must preflight exact O before cap.', error='InvalidState')
s = pre('context-cap-and-bad-fact-counter', edits={'/contexts/6': completed(6), '/next_nonce/Context': 7, '/next_nonce/Fact': 2})
add('later-used-structure-before-earlier-cap', s, 752, 'Context cap cannot hide later Fact max-next inconsistency.', error='InvalidState')

# Conditional-read controls pair poison with the exact new path that skips it.
s = pre('bad-p-link-unused-by-entry-fixed-bind', 'sel.prepared', {**bad_link, '/actors/0/transaction': ident(6, 6)})
add('entry-execute-skips-auth-and-t', s, 401, 'Entry Execute ends before P authentication or T lookup.', expected=INITIAL[401],
    edits={'/target': P1, '/context': ident(5, 2)})
add('fixed-bind-skips-auth-and-t', s, 51, 'Fixed Type payload needs selector but not P metadata or T.', expected=INITIAL[51],
    edits={'/target': P1, '/context': ident(5, 2)})
add('bind-success-reads-p-link', s, 48, 'Success Bind PN contrasts with fixed Bind poison control.', error='InvalidState')
add('inspect-p-skips-auth', s, 1322, 'Inspect copies exact selector bytes without current-auth lookup.', expected=INITIAL[1322], edits={'/target': P1})
broken_pf = copy.deepcopy(PARKED['contexts']['3'])
broken_pf['statement'] = ident(3, 6, slot=0, gen=1)
broken_pf['transaction'] = ident(6, 6)
s = pre('fresh-ignores-parked-payload', 'sel.parked', {'/contexts/3': broken_pf})
add('fresh-context-does-not-read-payload', s, 0, 'Fresh Context guard reads identities/ordinals only.', expected=INITIAL[0], edits={'/context': ident(5, 4)})
add('matching-context-reads-private-links', s, 1484, 'Matching PF private links cannot silently become absent.', error='InvalidState')
s = pre('rows-ignore-owner-payload', 'sel.open', {'/owners/3/current_holder': ident(4, 6, slot=0, gen=1)})
add('fetch-rows-skips-owner-lookup', s, 752, 'Rows has owner0 and does not inspect unrelated release payload.', expected=ORIGINAL['sel.rows'])
add('fetch-eof-reads-selected-owner', s, 753, 'EOF selects cursor owners unlike Rows.', error='InvalidState')

# Pending absence is covered by initial; these add the broken-vs-wrong-kind distinction.
for label, edits in [
    ('dangling', {'/actors/0/parked_context': ident(5, 6)}),
    ('duplicate', {'/contexts/4': PARKED['contexts']['3']}),
    ('foreign-scope', {'/contexts/3/id': ident(5, 3, actor=1, session=2)}),
    ('completed', {'/contexts/3': completed(3)}),
    ('t-backlink', {'/transactions/1/parked_context': None}),
    ('private-descriptor', {'/private_descriptors': {}}),
]:
    add('pending-' + label, pre('pending-' + label, 'sel.parked', edits), 1484,
        'New continuation relation guard: ' + label, error='InvalidState')
wrong = copy.deepcopy(broken_pf)
wrong['status'] = 'ParkedCommit'
wrong['api'] = 'BeginCommit'
s = pre('wrong-kind-opaque-payload', 'sel.parked', {'/contexts/3': wrong})
add('wrong-kind-does-not-copy-or-read-payload', s, 1484,
    'Valid pending kind mismatch ignores its malformed target/T payload and control.', expected=INITIAL[1484],
    edits={'/environment/fact': ident(11, 2, actor=255, session=0)})

# New prospective namespace guards: sparse retained definitions are intentional.
# Each L-1/L/L+1 input satisfies the needed maximum+1 rule; they are NOT graph states.
proto_fact = copy.deepcopy(OPEN['facts']['1'])
proto_txn = copy.deepcopy(OPEN['transactions']['1'])
proto_result = copy.deepcopy(OPEN['issued_results']['1'])
proto_owner = copy.deepcopy(OPEN['owners']['1'])
proto_o = copy.deepcopy(PENDING['obligations']['1'])
proto_receipt = {'id': ident(8, 1), 'transaction': T1, 'engine': ident(1, 1, actor=255, session=0),
                 'kind': 'NoWriteCompletion', 'delivery': 'NotApplicable', 'anchor': None,
                 'source_fact': None, 'delivery_context': None}


def namespace_case(kind, last, label):
    cap = 7 if kind == 'Engine' else 6
    maximum = last - 1
    edits, out_edits = {}, {}
    if kind == 'Context':
        index, base, expected = 0, 'initial', INITIAL[0]
        edits = {'/contexts': {str(maximum): completed(maximum)}}
        out_edits = {'/context': ident(5, last)}
    elif kind == 'Fact':
        index, base, expected = 752, 'initial', INITIAL[752]
        row = copy.deepcopy(proto_fact); row['id'] = ident(11, maximum, actor=255, session=0)
        edits = {'/facts': {str(maximum): row}}
        out_edits = {'/environment/fact': ident(11, last, actor=255, session=0)}
    elif kind == 'Transaction':
        index, base, expected = 400, 'sel.prepared', ORIGINAL['sel.execute']
        row = copy.deepcopy(proto_txn); row['id'] = ident(6, maximum)
        row['life'], row['parked_context'] = 'Ended', None
        edits = {'/transactions': {str(maximum): row}}
        out_edits = {'/environment/transaction': ident(6, last)}
    elif kind == 'Result':
        index, base, expected = 400, 'sel.prepared', ORIGINAL['sel.execute']
        row = copy.deepcopy(proto_result); row['id'] = ident(4, maximum, slot=1, gen=1)
        row['issue_ordinal'], row['public_generation_live'] = maximum, False
        edits = {'/issued_results': {str(maximum): row}}
        out = copy.deepcopy(expected)
        for owner in out['environment']['owners']:
            owner['after_holder'] = ident(4, last, slot=0, gen=1)
        expected = out
    elif kind == 'Receipt':
        index, base, expected = 1282, 'initial', INITIAL[1282]
        row = copy.deepcopy(proto_receipt); row['id'] = ident(8, maximum)
        edits = {'/receipts': {str(maximum): row}}
        out_edits = {'/environment/receipt': ident(8, last)}
    elif kind == 'Anchor':
        index, base, expected = 1289, 'initial', INITIAL[1289]
        row = copy.deepcopy(proto_receipt)
        row['kind'], row['anchor'] = 'DurableWrite', ident(12, maximum, actor=255, session=0)
        edits = {'/receipts': {'1': row}, '/next_nonce/Receipt': 2}
        out_edits = {'/environment/receipt': ident(8, 2), '/environment/anchor': ident(12, last, actor=255, session=0)}
    elif kind == 'Obligation':
        index, base, expected = 1303, 'rw.written', ORIGINAL['rw.rollback_incomplete']
        row = copy.deepcopy(proto_o)
        row.update(id=ident(10, maximum, actor=1, session=3, engine=2),
                   engine=ident(1, 2, actor=255, session=0, engine=2),
                   transaction=ident(6, 5, actor=1, session=3, engine=2),
                   origin_session=ident(2, 3, actor=1, session=3, engine=2))
        edits = {'/obligations': {str(maximum): row}}
        out = copy.deepcopy(expected)
        out['environment']['obligations'][0]['obligation'] = ident(10, last)
        for owner in out['environment']['owners']:
            owner['obligation'] = ident(10, last)
        expected = out
    elif kind == 'Engine':
        index, base, expected = 1528, 'initial', INITIAL[1528]
        row = copy.deepcopy(INIT['engines']['1'])
        row['id'] = ident(1, maximum, actor=255, session=0, engine=maximum)
        edits = {'/engines/' + str(maximum): row}
        out_edits = {'/environment/fresh_engine': ident(1, last, actor=255, session=0, engine=last)}
    else:
        raise ValueError(kind)
    edits['/next_nonce/' + kind] = last
    state = pre(kind.lower() + '-' + label, base, edits)
    add(kind.lower() + '-' + label, state, index, f'New {kind} candidate {last}, cap {cap}; retained max is {maximum}.',
        error='ProfileLimit' if last > cap else None, expected=None if last > cap else expected, edits=out_edits)
    if label == 'at-cap':
        malformed = copy.deepcopy(edits); malformed['/next_nonce/' + kind] = maximum
        add(kind.lower() + '-bad-next', pre(kind.lower() + '-bad-next', base, malformed), index,
            f'Used {kind} new-namespace max-next structure before prospective cap.', error='InvalidState')
    if label == 'above-cap':
        add(kind.lower() + '-unused-exhausted', pre(kind.lower() + '-unused-exhausted', base, edits), 1354,
            f'InspectSession does not use {kind}; max={cap}, stored next={cap + 1} is valid and exhausted.',
            expected=INITIAL[1354])


for kind in ('Engine', 'Result', 'Context', 'Transaction', 'Receipt', 'Obligation', 'Fact', 'Anchor'):
    limit = 7 if kind == 'Engine' else 6
    for last, label in ((limit - 1, 'below-cap'), (limit, 'at-cap'), (limit + 1, 'above-cap')):
        namespace_case(kind, last, label)

# Resource block6: OFF first SELECT supplies RW3 followed by cursor3.
for last, label in ((53, 'below-cap'), (54, 'at-cap'), (55, 'above-cap')):
    first = last - 5
    old = copy.deepcopy(proto_owner)
    old.update(id=ident(9, first - 1), disposition='Released', current_holder=None)
    s = pre('resource6-' + label, 'sel.prepared', {
        '/actors/0/autocommit': 'Off', '/owners/' + str(first - 1): old, '/next_nonce/Resource': first})
    expected = copy.deepcopy(ORIGINAL['sel.execute'])
    rw = []
    for nonce, resource_kind in zip(range(first, first + 3), ('Undo', 'Locks', 'AbortReserve')):
        rw.append({'resource': ident(9, nonce), 'resource_kind': resource_kind, 'after_disposition': 'Held',
                   'obligation': None, 'before_holder': None, 'after_holder': T1})
    for owner, nonce in zip(expected['environment']['owners'], range(first + 3, first + 6)):
        owner['resource'] = ident(9, nonce)
    expected['environment']['owners'] = rw + expected['environment']['owners']
    add('resource6-' + label, s, 400, 'Used Resource block is checked at final nonce, RW3 then cursor3.',
        error='ProfileLimit' if last > 54 else None, expected=None if last > 54 else expected)

# Result candidate generation includes future retirement, without doing a transition.
for n, live, outcome in ((11, True, 12), (12, True, None), (12, False, 12), (13, False, None)):
    state = pre(f'generation-{n}-live-{str(live).lower()}', 'sel.eof', {
        '/prepared_slots/0:0/next_result_generation': n,
        '/issued_results/1/public_generation_live': live,
        '/private_descriptors/1/public_generation_live': live})
    expected = copy.deepcopy(ORIGINAL['sel.reexecute'])
    if outcome is not None:
        for owner in expected['environment']['owners']:
            owner['after_holder'] = ident(4, 2, slot=0, gen=outcome)
    add(f'generation-{n}-live-{str(live).lower()}', state, 400,
        'S3 old result linkage then stored generation + retirement offset cap; no counter mutation.',
        error='ProfileLimit' if outcome is None else None, expected=None if outcome is None else expected)

s = pre('generation-structure-before-cap', 'sel.eof', {'/prepared_slots/0:0/next_result_generation': 14})
add('generation-stored-invalid', s, 400, 'Stored generation outside1..13 is structure error, not ProfileLimit.', error='InvalidState')
s = pre('generation-link-before-cap', 'sel.eof', {'/prepared_slots/0:0/next_result_generation': 12,
    '/issued_results/1/public_generation_live': False, '/private_descriptors/1/public_generation_live': True})
add('generation-link-before-cap', s, 400, 'Old result/descriptor public-live disagreement precedes candidate cap.', error='InvalidState')

# Typed decoding is preparation, not a product invocation. All IDs and exact
# inputs are finalized here; final D2 may discover a candidate authoring error.
assert len({x['id'] for x in STATES}) == len(STATES)
assert len({x['id'] for x in CASES}) == len(CASES)
result = {'status': 'INDEPENDENT_READSET_LITERAL_CANDIDATE_NOT_D2_FROZEN_NOT_RUN',
          'authorship': 'root explicit read-set deltas and literal expected fields, no product/model/codec output',
          'prestates': STATES, 'cases': CASES,
          'not_claimed': ['full-state validity', 'model reachability', 'Capacity attainability', 'runtime PASS']}
HERE.mkdir(parents=True, exist_ok=True)
def write(name, data):
    (HERE/name).write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')
write('cases.json', result)
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
sources = [ORIG_PATH, RAW_PATH, NORMAL/'initial.json', NORMAL/'original-links.json',
           ART/'assembler-owner-scope-v2.md', ART/'assembler-readset-v2.md', ART/'assembler-readset-v3.md']
write('manifest.json', {'status': result['status'], 'baseline_revision': '1dd7bed1fc03bead2158fba439a06673dff29158',
    'counts': {'cases': len(CASES), 'prestates': len(STATES), 'success_expected': sum('expected_input' in c for c in CASES),
               'error_expected': sum('expected_error' in c for c in CASES), 'product_calls': 0},
    'files': [{'path': p.name, 'sha256': sha(p), 'bytes': p.stat().st_size} for p in [HERE/'cases.json', HERE/'author-errors.py']],
    'source_pins': [{'path': str(p.relative_to(ROOT)), 'sha256': sha(p)} for p in sources],
    'pending': ['independent full static review and final D2', 'additional required relation/receipt/namespace witnesses after coverage review'],
    'excluded_old_proposals': '123 old invalid-action/helper cases and12 mere selector wrappers remain NOT_RUN; only new crossing witnesses retained'})
print(json.dumps({'static_authoring_only': True, 'cases': len(CASES), 'prestates': len(STATES), 'manifest_sha256': sha(HERE/'manifest.json')}))
