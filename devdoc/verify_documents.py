#!/usr/bin/env python3
"""Check XADB contracts and evidence identity; do not execute product tests."""
import json
import re
import sys
import tomllib
from datetime import datetime, timezone, timedelta
from pathlib import Path
sys.dont_write_bytecode = True
from document_tools import DocumentIndex, validate_document_layout, validate_markdown
ROOT = Path(__file__).resolve().parent


# Accepted minimum closures. Update deliberately with owner contracts, never from a candidate report.
REQUIRED_TESTS = {'DEV0': {'dev0-validation.json': ['evidence-self-tests',
                                   'format',
                                   'lint',
                                   'self-tests',
                                   'default-build',
                                   'real-fixture-smoke',
                                   'clean-runtime',
                                   'fresh-source-build']},
 'DEV1': {'dev1-validation.json': ['dev0-regression',
                                   'dependency-boundary',
                                   'independent-build',
                                   'foundation-default-tests',
                                   'foundation-worker',
                                   'wrong-expectation-rejected',
                                   'platform-independent-build',
                                   'platform-default-tests']},
 'DEV2': {'stage-completion.json': ['SC01'],
          'kernel-test-matrix.json': ['KU02', 'KU03', 'KU04', 'KU05', 'KU06', 'KU07', 'KU08'],
          'pilot-acceptance.json': ['PA02', 'PA03', 'PA05', 'PA06', 'PA07'],
          'recovery-test-matrix.json': ['RC-T01',
                                        'RC-T02',
                                        'RC-T03',
                                        'RC-T04',
                                        'RC-T05',
                                        'RC-T06',
                                        'RC-T07',
                                        'RC-T08',
                                        'RC-T09',
                                        'RC-T10',
                                        'RC-T11',
                                        'RC-T26'],
          'ai-performance-engineering.json': ['PF01',
                                              'PF02',
                                              'PF03',
                                              'PF04',
                                              'PF05',
                                              'PF06',
                                              'PF07',
                                              'PF12',
                                              'PF13']},
 'DEV3': {'kernel-test-matrix.json': ['KU09', 'KU10'],
          'recovery-test-matrix.json': ['RC-T12', 'RC-T13', 'RC-T14'],
          'ai-performance-engineering.json': ['PF09'],
          'disk-test-matrix.json': ['DS01', 'DS02', 'DS03', 'DS04', 'DS05', 'DS06']},
 'DEV4': {'stage-completion.json': ['SC02'],
          'kernel-test-matrix.json': ['KU13'],
          'ai-performance-engineering.json': ['PF10']},
 'DEV5': {'stage-completion.json': ['SC03'],
          'kernel-test-matrix.json': ['KU14'],
          'pilot-acceptance.json': ['PA08'],
          'ai-performance-engineering.json': ['PF11']},
 'DEV6': {'stage-completion.json': ['SC04', 'SC05', 'SC06']},
 'DEV7': {'stage-completion.json': ['SC07', 'SC08', 'SC09', 'SC10', 'SC11']},
 'DEV8': {'stage-completion.json': ['SC12', 'SC13', 'SC14', 'SC15'],
          'recovery-test-matrix.json': ['RC-T24', 'RC-T25'],
          'ai-performance-engineering.json': ['PF08', 'PF14']},
 'DEV9': {'stage-completion.json': ['SC16', 'SC17']},
 'COMM': {'kernel-test-matrix.json': ['KU12']},
 'PART': {'kernel-test-matrix.json': ['KU11'], 'disk-test-matrix.json': ['DS07']},
 'NG': {'disk-test-matrix.json': ['DS08']}}

REQUIRED_CHECKS = {'dev0': {'evidence-self-tests': 0,
          'format': 0,
          'lint': 0,
          'self-tests': 0,
          'default-build': 0,
          'real-fixture-smoke': 0,
          'clean-runtime': 0,
          'fresh-source-build': 0},
 'dev1': {'dev0-regression': 0,
          'dependency-boundary': 0,
          'independent-build': 0,
          'platform-independent-build': 0,
          'foundation-default-tests': 0,
          'platform-default-tests': 0,
          'foundation-worker': 0,
          'wrong-expectation-rejected': 1},
 'dev2': {'dev1-regression': 0,
          'dependency-boundary': 0,
          'xadbtest-reference-independent-build': 0,
          'literal-v2-maximum': 0,
          'v3-extended': 0,
          'xadb-engine-independent-build': 0,
          'substrate-xadb_engine': 0,
          'substrate-substrate': 0,
          'substrate-model': 0,
          'substrate-v3-extended': 0,
          'xadbtest-protocol-independent-build': 0,
          'frame-literal-v2-maximum': 0,
          'frame-v3-extended': 0,
          'xadbtest-engine-worker-independent-build': 0,
          'worker-adapter': 0,
          'worker-process': 0,
          'worker-v3-extended': 0,
          'catalog-reference-literal-v2-maximum': 0,
          'catalog-reference-v3-extended': 0,
          'worker-native': 0,
          'worker-catalog': 0,
          'worker-controller-v3-extended': 0,
          'worker-catalog-v3-extended': 0,
          'xadbtest-independent-build': 0,
          'controller-adversarial': 0,
          'xadbtest-default-command-independent-build': 0,
          'catalog-command-run': 0,
          'catalog-command-verify': 0}}

STAGE_PREDECESSORS = {'DEV0': [],
 'DEV1': ['DEV0'],
 'DEV2': ['DEV1'],
 'DEV3': ['DEV2'],
 'DEV4': ['DEV3'],
 'DEV5': ['DEV4', 'COMM'],
 'DEV6': ['DEV5'],
 'DEV7': ['DEV4'],
 'DEV8': ['DEV3', 'DEV5'],
 'DEV9': ['DEV6', 'DEV7', 'DEV8', 'PART'],
 'COMM': ['DEV1'],
 'PART': ['DEV3'],
 'NG': ['PART']}


def all_ids(value):
    result = set()
    if isinstance(value, dict):
        if isinstance(value.get('id'), str):
            result.add(value['id'])
        for child in value.values():
            result.update(all_ids(child))
    elif isinstance(value, list):
        for child in value:
            result.update(all_ids(child))
    return result


def check_graph(graph, label):
    errors, visiting, done = [], set(), set()
    def visit(node):
        if node in visiting:
            errors.append(label + ': cycle at ' + node)
            return
        if node in done:
            return
        visiting.add(node)
        for dependency in graph[node]:
            if dependency not in graph:
                errors.append(label + ': unknown dependency ' + dependency)
            else:
                visit(dependency)
        visiting.remove(node)
        done.add(node)
    for node in graph:
        visit(node)
    return errors


def validate_stage_graph(development, completion, scope, documents):
    stages = {s['id']: s for s in development['stages']}
    closes = {s['id']: s for s in completion['stages']}
    errors = check_graph({k: v['depends_on'] for k, v in stages.items()}, 'stages')
    errors += check_graph(scope['task_dependencies'], 'tasks')
    if {k: v.get('depends_on') for k, v in stages.items()} != STAGE_PREDECESSORS:
        errors.append('stages: accepted prerequisite graph changed')
    for case in completion['cases']:
        if case.get('execution_status') != 'NOT_RUN':
            errors.append('stages: planned acceptance case cannot claim execution ' + case['id'])
    for name, document in documents.items():
        if name in {'kernel-test-matrix.json', 'pilot-acceptance.json', 'recovery-test-matrix.json',
                    'ai-performance-engineering.json', 'disk-test-matrix.json', 'replication-undo-plan.json'}:
            for case in document.get('cases', []):
                if case.get('execution_status') != 'NOT_RUN':
                    errors.append('stages: unexecuted contract case claims execution ' + name + ':' + case['id'])
    if len(stages) != len(development['stages']) or set(stages) != set(closes):
        errors.append('stages: duplicate or mismatched inventory')
    for name, stage in stages.items():
        other = closes.get(name, {})
        if other.get('entry_stage_refs') != stage.get('depends_on'):
            errors.append('stages: entry prerequisite mismatch ' + name)
        expected_status = ('LOCAL_VERIFIED' if name in ('DEV0', 'DEV1') else
                           'PARTIAL_IMPLEMENTATION' if name in ('DEV2', 'COMM') else 'NOT_RUN')
        if stage.get('execution_status') != expected_status or other.get('execution_status') != expected_status:
            errors.append('stages: unsupported execution status ' + name)
        if other.get('official_distribution_status') != 'NOT_RUN':
            errors.append('stages: local evidence cannot qualify distribution ' + name)
        refs = other.get('required_test_refs', [])
        closure = {r.get('file'): r.get('ids') for r in refs}
        if len(closure) != len(refs) or closure != REQUIRED_TESTS.get(name):
            errors.append('stages: required test closure changed or duplicated ' + name)
        if not other.get('pass_criteria') or not all(other['pass_criteria']):
            errors.append('stages: missing pass criteria ' + name)
        if not stage.get('gate_refs') or not stage.get('task_refs'):
            errors.append('stages: missing gate/task closure ' + name)
        for field in ('gate_refs', 'task_refs'):
            if stage.get(field) != other.get(field):
                errors.append('stages: ' + field + ' mismatch ' + name)
        for task in stage.get('task_refs', []):
            if task not in scope['task_dependencies']:
                errors.append('stages: unknown task ' + task)
        for test in other.get('required_test_refs', []):
            target = documents.get(test.get('file'))
            if target is None:
                errors.append('stages: missing test document ' + str(test))
                continue
            for case in test.get('ids', []):
                if case not in all_ids(target):
                    errors.append(f'stages: unknown case {name}:{test["file"]}:{case}')
    if scope['user_confirmed'].get('storage_engine') != 'DISK_ONLY':
        errors.append('scope: disk-only invariant lost')
    for contract in (scope['initial_sql_baseline'], development['initial_sql_scope']):
        if contract.get('storage') != ['DISK']:
            errors.append('scope: initial SQL must use disk storage only')
    return errors


def validate_packages(plan, repo):
    errors = []
    workspace = tomllib.loads((repo / 'Cargo.toml').read_text())['workspace']
    packages = {p['path']: p for p in plan['packages'] if p['status'] == 'IMPLEMENTED'}
    if set(packages) != set(workspace['members']):
        errors.append('repository: implemented package/member mismatch')
    names = {p['name'] for p in packages.values()}
    for path, expected in packages.items():
        manifest = tomllib.loads((repo / path / 'Cargo.toml').read_text())
        if manifest['package']['name'] != expected['name']:
            errors.append('repository: name mismatch ' + path)
        actual = []
        for alias, dependency in manifest.get('dependencies', {}).items():
            if isinstance(dependency, dict) and dependency.get('workspace') is True:
                inherited = workspace.get('dependencies', {})
                if alias not in inherited:
                    errors.append('repository: missing inherited dependency ' + alias)
                    continue
                dependency = inherited[alias]
            name = dependency.get('package', alias) if isinstance(dependency, dict) else alias
            if name in names:
                actual.append(name)
        actual.sort()
        if actual != sorted(expected['normal_workspace_dependencies']):
            errors.append('repository: dependency mismatch ' + path)
    roots = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tests', 'scripts', '.github']
    for field in ('source_fingerprint_roots', 'clean_export_roots'):
        if plan.get(field) != roots:
            errors.append('repository: source inventory mismatch ' + field)
    return errors


def validate_capabilities(capabilities, inventory):
    errors = []
    if capabilities.get('current_external_release_eligible') is not False:
        errors.append('capabilities: unimplemented backlog cannot qualify release')
    if capabilities.get('baseline_frozen') is not False or capabilities.get('inventory_complete') is not False:
        errors.append('capabilities: draft inventory cannot claim a frozen complete baseline')
    for group in capabilities['families']:
        for requirement in group['leaf_requirements']:
            if requirement.get('execution_status') != 'NOT_RUN':
                errors.append('capabilities: planned requirement cannot claim execution ' + requirement['id'])
    requirements = [r['id'] for g in capabilities['families'] for r in g['leaf_requirements']]
    items = [i['id'] for g in inventory['groups'] for i in g['items']]
    if len(requirements) != len(set(requirements)) or len(requirements) != capabilities['requirement_count']:
        errors.append('capabilities: duplicate or incorrect count')
    if len(items) != len(set(items)) or len(items) != inventory['summary']['item_count']:
        errors.append('inventory: duplicate or incorrect count')
    if set(requirements) != {m['requirement_id'] for m in inventory['requirement_mappings']}:
        errors.append('inventory: requirement mapping mismatch')
    for mapping in inventory['requirement_mappings']:
        if not set(mapping['item_refs']) <= set(items):
            errors.append('inventory: unknown topic in ' + mapping['requirement_id'])
    for group in inventory['groups']:
        for item in group['items']:
            if not set(item['requirement_refs']) <= set(requirements):
                errors.append('inventory: unknown requirement in ' + item['id'])
            if not item.get('spec_scope') or not item.get('design_scope'):
                errors.append('inventory: missing scope in ' + item['id'])
    return errors


def validate_evidence(report, fingerprint):
    errors = []
    kind = ('dev2' if 'dev2_stage_status' in report else
            'dev1' if 'dev0_regression' in report else 'dev0')
    if report.get('status') != 'PASS':
        errors.append('evidence: current execution is not PASS')
    if any(report.get(k) != fingerprint for k in ('source_sha256', 'source_sha256_before', 'source_sha256_after')):
        errors.append('evidence: current source fingerprint mismatch')
    checks = report.get('checks', [])
    ids = [c.get('id') for c in checks]
    if len(ids) != len(set(ids)) or set(ids) != set(REQUIRED_CHECKS[kind]):
        errors.append('evidence: missing/duplicate/unexpected fixed check inventory')
    if not checks or any(c.get('status') != 'PASS' for c in checks):
        errors.append('evidence: missing or failed checks')
    for check in checks:
        expected = REQUIRED_CHECKS[kind].get(check.get('id'))
        if expected is None or check.get('exit_code') != expected or check.get('expected_exit_code', 0) != expected:
            errors.append('evidence: incorrect check exit outcome ' + str(check.get('id')))
        if not check.get('command') or not check.get('log') or not re.fullmatch(r'[a-f0-9]{64}', check.get('log_sha256', '')):
            errors.append('evidence: missing command/log identity')
    if report.get('official_release', False) or report.get('protected_ci_execution', 'NOT_RUN') != 'NOT_RUN' or report.get('protected_ci', 'NOT_RUN') != 'NOT_RUN':
        errors.append('evidence: local run cannot qualify protected release')
    if kind == 'dev2':
        limits = {key: 'NOT_RUN' for key in ('product_durability', 'bounded_concurrent_exploration',
                  'performance_acceptance', 'v3_30_minute_thread_stress',
                  'v3_30_minute_codec_fuzz')}
        limits.update(dev2_stage_status='INCOMPLETE', k0_stage_status='INCOMPLETE',
                      streaming_controller='ADVERSARIAL_SELF_TEST_AND_ACTUAL_WORKER',
                      catalog_oracle='BOUNDED_SEQUENTIAL_RECIPE_AND_EXTERNAL_HISTORY')
        for key, value in limits.items():
            if report.get(key) != value:
                errors.append('evidence: unimplemented/unexecuted DEV2 limit changed ' + key)
    return errors


def validate_implementation_status(status, documents, fingerprint):
    """Separate source-bound current claims from unchanged historical receipts.

    Only the existing DEV0/DEV1/P0-K0 report profiles are accepted here. A future
    kernel leaf needs its own declared validator; implementation is not a PASS.
    """
    errors = []
    if status.get('durability') not in {'NOT_IMPLEMENTED', 'IMPLEMENTED_VALIDATION_PENDING'}:
        errors.append('implementation: unsupported durability state')
    if status.get('dev2') != 'PARTIAL_IMPLEMENTATION':
        errors.append('implementation: partial kernel work cannot qualify DEV2')
    if status.get('product_qualification') != 'NOT_RUN' or status.get('public_release') != 'NOT_RUN':
        errors.append('implementation: pending kernel validation cannot qualify product or release')
    references = {}
    for field in ('evidence_refs', 'historical_evidence_refs'):
        names = status.get(field, [])
        if not isinstance(names, list) or any(not isinstance(name, str) for name in names):
            errors.append('evidence: invalid reference list ' + field)
            names = []
        if len(names) != len(set(names)):
            errors.append('evidence: duplicate references in ' + field)
        references[field] = set(names)
    if references['evidence_refs'] & references['historical_evidence_refs']:
        errors.append('evidence: current and historical references overlap')
    legacy = {'dev0-validation.json', 'dev1-validation.json', 'dev2-validation.json'}
    for field, names in references.items():
        for name in names:
            if name not in legacy:
                errors.append('evidence: undeclared report profile ' + name)
                continue
            report = documents.get(name)
            if not isinstance(report, dict):
                errors.append('evidence: missing report ' + name)
                continue
            recorded = report.get('source_sha256')
            if not isinstance(recorded, str) or not re.fullmatch(r'[a-f0-9]{64}', recorded):
                errors.append('evidence: missing or invalid recorded source identity ' + name)
                continue
            expected = fingerprint if field == 'evidence_refs' else recorded
            errors += [name + ': ' + error for error in validate_evidence(report, expected)]
    for stage in ('dev0', 'dev1'):
        state = status.get(stage)
        if state not in {'LOCAL_VERIFIED', 'PREVIOUS_SOURCE_VERIFIED', 'NOT_RUN'}:
            errors.append('implementation: unsupported source verification state ' + stage)
        field = {'LOCAL_VERIFIED': 'evidence_refs',
                 'PREVIOUS_SOURCE_VERIFIED': 'historical_evidence_refs'}.get(state)
        if field and stage + '-validation.json' not in references[field]:
            errors.append('evidence: verified stage has no matching source-scope report ' + stage)
    return errors


def validate_policy_limits(documents):
    errors = []
    required = {
        'dev0-contract.json': {'limits': {'max_input_bytes': 65536}},
        'dev1-contract.json': {'values': {'max_variable_bytes': 65536}, 'frame': {'max_payload_bytes': 1048576}},
        'development-policy.json': {'numeric_defaults': {'property_merge_seeds': 64,
            'property_merge_max_operations': 128, 'property_extended_seeds': 256,
            'property_extended_max_operations': 1024, 'pilot_other_metric_max_regression_percent': 10}},
    }
    for name, sections in required.items():
        for section, fields in sections.items():
            for key, expected in fields.items():
                if documents[name].get(section, {}).get(key) != expected:
                    errors.append('policy: required bounded input/profile changed ' + name + ':' + key)
    return errors


def verify(root=ROOT):
    errors, markdown = validate_markdown(root)
    errors += validate_document_layout(root)
    index = DocumentIndex(root)
    docs = {name: json.loads((root / entry['path']).read_text())
            for name, entry in index.entries.items() if entry['path'].endswith('.json')}
    errors += validate_stage_graph(docs['development-stages.json'], docs['stage-completion.json'], docs['phase1-scope.json'], docs)
    errors += validate_packages(docs['repository-build-plan.json'], root.parent)
    errors += validate_policy_limits(docs)
    errors += validate_capabilities(docs['capability-inventory.json'], docs['feature-design-inventory.json'])
    for path in root.rglob('*.json'):
        json.loads(path.read_text())
    for path in list(root.rglob('*.py')) + list((root.parent / 'scripts').glob('*.py')):
        compile(path.read_text(), str(path), 'exec')
    status = docs['implementation-status.json']
    sys.path.insert(0, str(root.parent / 'scripts'))
    from check_dev0 import source_fingerprint
    fingerprint = source_fingerprint(root.parent)
    errors += validate_implementation_status(status, docs, fingerprint)
    for entry in index.entries.values():
        path = root / entry['path']
        if path.parent != root and path.name != 'README.md':
            parent = path.parent / 'README.md'
            if not parent.is_file() or path.name not in parent.read_text():
                errors.append('navigation: missing directory README entry ' + entry['path'])
    return errors, markdown


if __name__ == '__main__':
    errors, markdown = verify()
    result = {'schema_version': 2, 'product': 'XADB',
              'date': datetime.now(timezone(timedelta(hours=9))).isoformat(),
              'status': 'PASS' if not errors else 'FAIL', 'errors': errors,
              'markdown_documents': markdown,
              'scope': 'Markdown/JSON/navigation, stage/task/case/package contracts and current source evidence identity; no product tests or historical measurements executed'}
    (ROOT / 'document-validation.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({'status': result['status'], 'errors': errors, 'markdown_count': len(markdown)}, ensure_ascii=False, indent=2))
    raise SystemExit(bool(errors))
