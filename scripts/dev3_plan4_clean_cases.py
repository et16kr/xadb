#!/usr/bin/env python3
"""G5 input binding for the existing exact-case runner; no genesis case replay."""
import argparse
import collections
import datetime
import json
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
import dev3_plan4_genesis_cases as common
import dev3_plan4_genesis_native as raw_oracle

INPUT = Path('tests/fixtures/dev3-plan4-clean-owner')
RAW = Path('tests/fixtures/dev3-plan4-genesis')
SCHEMA = common.SCHEMA
require, sha = common.require, common.sha
BUDGETS = {'static': (10.0, 15.0, 5.0, 1 << 20),
           'memory': (180.0, 210.0, 30.0, 16 << 20)}
PADDING = [256, 512, 1024, 2048, 4096]
RECEIPT_RESERVE = 65536


def runner_bindings(source, extra=()):
    paths = {Path(__file__).resolve(), Path(common.__file__).resolve(),
             Path(raw_oracle.__file__).resolve(), *(Path(path).resolve() for path in extra)}
    result = {}
    for path in sorted(paths):
        relative = 'scripts/' + path.name
        digest = sha(path)
        require(digest == sha(Path(source) / relative), 'loaded runner/helper differs from frozen source')
        result[relative] = {'executed': str(path), 'sha256': digest}
    return result


def check_bindings(source, bindings):
    for relative, row in bindings.items():
        require(sha(row['executed']) == row['sha256'] == sha(Path(source) / relative),
                'loaded runner/helper changed during execution')


def audit_seconds(wall, process):
    require(wall >= 0 and process >= 0, 'invalid audit accounting')
    return max(0.0, wall - process)


def require_audit(prior_wall, prior_process, wall, process, limit):
    used = audit_seconds(prior_wall, prior_process) + audit_seconds(wall, process)
    require(used <= limit, 'aggregate audit-only budget exceeded')
    return used


def validate_build(revision):
    revision = Path(revision).resolve()
    build = json.loads((revision / 'build-receipt.json').read_text())
    source, binary = revision / 'source', revision / 'xadb-engine-tests'
    manifest_path = source / INPUT / 'manifest.json'
    require(build['exit_code'] == 0, 'unsuccessful G5 build')
    require(build['command'] == ['cargo', 'test', '-p', 'xadb-engine', '--lib',
                                '--all-features', '--locked', '--no-run',
                                '--message-format=json'], 'unexpected build command')
    require(build['source_sha256'] == common.source_fingerprint(source), 'source drift')
    for field, path in [('toolchain_file_sha256', source / 'rust-toolchain.toml'),
                        ('cargo_lock_sha256', source / 'Cargo.lock'),
                        ('build_log_sha256', revision / 'build.log'),
                        ('binary_sha256', binary),
                        ('manifest_sha256', manifest_path),
                        ('raw_manifest_sha256', source / RAW / 'manifest.json'),
                        ('recipes_sha256', source / INPUT / 'execution-recipes.json')]:
        require(build[field] == sha(path), f'{field} drift')
    with binary.open('rb') as stream:
        require(stream.read(4) == b'\x7fELF', 'not an ELF')
    require(build['owned_files'], 'missing owned source inventory')
    for relative, digest in build['owned_files'].items():
        path = source / relative
        require(path.resolve().is_relative_to(source) and sha(path) == digest,
                'owned source mismatch')
    manifest = json.loads(manifest_path.read_text())
    require(manifest['schema'] == 'XADB_PLAN4_G5_INPUT_V1', 'not a G5 input manifest')
    require(manifest['counts'] == {'mandatory': 78, 'mandatory_memory': 75,
                                  'mandatory_static': 2, 'mandatory_native': 1,
                                  's5_additional': 128, 's5_memory': 126,
                                  's5_native': 2, 'total': 206, 'compiler': 10},
            'G5 inventory drift')
    require(len(manifest['cases']) == 206 and all(case.startswith('P4-G5-')
            for case in manifest['cases']), 'foreign runtime inventory')
    _, inputs = raw_oracle.fixture(source)
    require(inputs['manifest.json'] == manifest['raw_manifest_sha256'] and
            inputs['raw-index.tsv'] == manifest['raw_index_sha256'], 'raw lineage drift')
    identity = (build['source_sha256'], build['binary_sha256'], build['manifest_sha256'])
    return source, binary, manifest_path, build, identity


def capacity_gate(revision, identity):
    path = Path(revision) / 'gate-execution-input.json'
    gate = json.loads(path.read_text())
    require(gate['schema'] == 'XADB_PLAN4_G5_GATE_INPUT_V1', 'unknown static gate schema')
    require(tuple(gate['identity']) == tuple(identity), 'static gate identity mismatch')
    receipt = Path(gate['static_receipt_path']).resolve()
    require(receipt.is_relative_to(Path(revision).resolve()) and sha(receipt) == gate['static_receipt_sha256'],
            'static gate receipt changed or is not in the frozen revision')
    record = json.loads(receipt.read_text())
    require(tuple(record['identity']) == tuple(identity) and record['finalized'] and
            record['cases'].get('P4-G5-ST01') == 'PASS', 'missing completed actual-memory census')
    rows = []
    for job in record['jobs']:
        if 'P4-G5-ST01' not in job['selected_ids']:
            continue
        stdout = receipt.parent / job['stdout']
        require(sha(stdout) == job['stdout_sha256'], 'static census stdout drift')
        parsed = common.parse_markers(stdout.read_text(), job['selector'], job['selected_ids'], identity)
        require(parsed == job['parsed'] and 'P4-G5-ST01' in parsed['markers']['PASS'] and
                parsed['parser_error'] is None and not job['truncated'], 'uncertain static census')
        for line in stdout.read_text().splitlines():
            marker = 'P4_CENSUS\tB08-candidate\t'
            if marker in line:
                fields = dict(item.split('=', 1) for item in line.split(marker, 1)[1].split('\t'))
                rows.append(fields)
    require([int(row['padding']) for row in rows] == PADDING, 'missing or duplicate padding census')
    eligible = [int(row['padding']) for row in rows if row['fresh_fits'] == 'true' and
                row['setup_fits'] == 'true' and int(row['doublewrite_owner']) <= 16384 and
                int(row['doublewrite_plan']) <= 4096 and row['g5_fits'] == 'false']
    require(eligible, 'no legitimate B08 type: input review required before effects')
    require(gate['capacity_padding'] == eligible[0], 'B08 is not the first eligible finite candidate')
    return gate['capacity_padding'], {'path': str(path), 'sha256': sha(path), **gate}


def main():
    cli = argparse.ArgumentParser()
    cli.add_argument('revision', type=Path)
    cli.add_argument('category', choices=BUDGETS)
    cli.add_argument('--output', required=True, type=Path)
    cli.add_argument('--include')
    cli.add_argument('--exclude')
    cli.add_argument('--s5', action='store_true')
    args = cli.parse_args()
    started = time.monotonic()
    revision = args.revision.resolve()
    source, binary, manifest_path, _, identity = validate_build(revision)
    bindings = runner_bindings(source)
    all_cases = json.loads(manifest_path.read_text())['cases']
    phase = 's5' if args.s5 else 'mandatory'
    require(not args.s5 or args.category == 'memory', 'no expanded static campaign')
    available = [case for case, row in all_cases.items()
                 if row['phase'] == phase and row['category'] == args.category]
    prior, references, prior_elapsed, prior_bytes = common.load_prior(revision.parent, args.category)
    prior_execution = sum(json.loads(Path(ref['path']).read_text())['process_wall_seconds'] for ref in references)
    selected = common.select_cases(available, args.include, args.exclude, prior)
    execution_limit, total_limit, function_limit, log_limit = BUDGETS[args.category]
    audit_limit = 5.0 if args.category == 'static' else 30.0
    if args.s5:
        execution_limit, total_limit, function_limit, log_limit = 600.0, 630.0, 180.0, 32 << 20
    common.budget_remaining(prior_elapsed, time.monotonic() - started, total_limit)
    common.budget_remaining(prior_execution, 0, execution_limit)
    require_audit(prior_elapsed, prior_execution, time.monotonic() - started, 0, audit_limit)
    require(prior_bytes + RECEIPT_RESERVE < log_limit, 'aggregate log budget exhausted')
    padding, gate = None, None
    if 'P4-G5-B08' in selected:
        padding, gate = capacity_gate(revision, identity)
    output = args.output.resolve()
    require(output.parent == revision, 'output must be a new revision child')
    output.mkdir()
    report = {'schema': SCHEMA, 'category': args.category, 'phase': phase, 'identity': identity,
              'build_receipt_sha256': sha(revision / 'build-receipt.json'), 'runner_sha256': sha(Path(__file__)),
              'selected_ids': sorted(selected), 'prior_receipts': references,
              'loaded_runner_bindings': bindings,
              'prior_elapsed_seconds': prior_elapsed, 'prior_process_seconds': prior_execution,
              'prior_bytes': prior_bytes, 'gate_execution_input': gate,
              'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'jobs': [], 'cases': {case: 'NOT_RUN' for case in sorted(selected)},
              'execution_wall_seconds': 0.0, 'process_wall_seconds': 0.0,
              'finalized': False, 'status': 'NOT_RUN'}
    common.save(output / 'result.json', report)
    groups = collections.defaultdict(list)
    for case in selected:
        groups[all_cases[case]['selector']].append(case)
    try:
        for ordinal, (selector, cases) in enumerate(sorted(groups.items()), 1):
            check_bindings(source, bindings)
            validate_build(revision)
            require_audit(prior_elapsed, prior_execution, time.monotonic() - started,
                          report['process_wall_seconds'], audit_limit)
            allowance = min(function_limit,
                common.budget_remaining(prior_execution, report['process_wall_seconds'], execution_limit),
                common.budget_remaining(prior_elapsed, time.monotonic() - started, total_limit) - 0.05)
            used = prior_bytes + sum(p.stat().st_size for p in output.iterdir() if p.is_file())
            capacity = log_limit - used - RECEIPT_RESERVE
            require(capacity > 0 and allowance > 0, 'aggregate budget exhausted')
            command = [str(binary), selector, '--exact', '--nocapture', '--test-threads=1']
            if args.s5:
                command.append('--ignored')
            env = os.environ.copy()
            env.pop('XADB_PLAN4_EXCLUDE', None)
            env.pop('XADB_PLAN4_G5_CAPACITY_PADDING', None)
            env['XADB_PLAN4_INCLUDE'] = ','.join(sorted(cases))
            if 'P4-G5-B08' in cases:
                require(capacity_gate(revision, identity)[1] == gate, 'static gate changed')
                env['XADB_PLAN4_G5_CAPACITY_PADDING'] = str(padding)
            for key, value in zip(('SOURCE', 'ELF', 'INPUT'), identity):
                env['XADB_PLAN4_GENESIS_' + key + '_SHA256'] = value
            stdout, stderr = f'{ordinal:02}.stdout', f'{ordinal:02}.stderr'
            planned = {'selector': selector, 'selected_ids': sorted(cases), 'command': command,
                       'cwd': str(source), 'stdout': stdout, 'stderr': stderr, 'state': 'INVOKING'}
            report['jobs'].append(planned)
            common.save(output / 'result.json', report)
            job = common.invoke(command, source, env, output / stdout, output / stderr,
                                selector, cases, identity, allowance, 5.0, capacity)
            job.update(selector=selector, selected_ids=sorted(cases), command=command, cwd=str(source),
                       include=env['XADB_PLAN4_INCLUDE'], exclude=None, capacity_padding=padding,
                       timeout_seconds=allowance, stdout=stdout, stderr=stderr,
                       stdout_sha256=sha(output / stdout), stderr_sha256=sha(output / stderr))
            planned.update(job, state='FINISHED')
            report['process_wall_seconds'] += job['elapsed_seconds']
            for word in ('PASS', 'FAIL'):
                for case in job['parsed']['markers'][word]:
                    report['cases'][case] = word
            common.save(output / 'result.json', report)
            require(job['exact'], 'incomplete case execution')
            validate_build(revision)
        for ref in references:
            require(sha(ref['path']) == ref['sha256'], 'prior receipt changed')
            old = json.loads(Path(ref['path']).read_text())
            expected_names = {'result.json'} | {job[key] for job in old['jobs'] for key in ('stdout', 'stderr')}
            require({path.name for path in Path(ref['path']).parent.iterdir() if path.is_file()} == expected_names,
                    'prior output inventory changed')
            for job in old['jobs']:
                require(sha(Path(ref['path']).parent / job['stdout']) == job['stdout_sha256'] and
                        sha(Path(ref['path']).parent / job['stderr']) == job['stderr_sha256'], 'prior output changed')
        report['status'] = 'PASS'
    except (ValueError, OSError, KeyError) as error:
        report.update(status='PARTIAL_OR_FAIL', error=str(error))
    finally:
        try:
            check_bindings(source, bindings)
        except (ValueError, OSError) as error:
            report.update(status='PARTIAL_OR_FAIL', error=str(error))
        report['finalized'] = all(job.get('state') == 'FINISHED' for job in report['jobs'])
        # Includes preflight hashes, prior audits, invocation, and final receipt reserve.
        report['measured_before_receipt_seconds'] = time.monotonic() - started
        report['execution_wall_seconds'] = report['measured_before_receipt_seconds'] + 0.05
        report['audit_wall_seconds'] = audit_seconds(report['execution_wall_seconds'], report['process_wall_seconds'])
        report['aggregate_audit_wall_seconds'] = audit_seconds(prior_elapsed, prior_execution) + report['audit_wall_seconds']
        report['log_bytes_with_receipt_reserve'] = prior_bytes + sum(p.stat().st_size for p in output.iterdir()
            if p.is_file() and p.name != 'result.json') + RECEIPT_RESERVE
        if (prior_elapsed + report['execution_wall_seconds'] > total_limit or
                prior_execution + report['process_wall_seconds'] > execution_limit or
                report['aggregate_audit_wall_seconds'] > audit_limit or
                report['log_bytes_with_receipt_reserve'] > log_limit):
            report.update(status='PARTIAL_OR_FAIL', error='aggregate audit/process/log budget exceeded')
        common.save(output / 'result.json', report)
        require((output / 'result.json').stat().st_size <= RECEIPT_RESERVE, 'receipt reserve exceeded')
        if time.monotonic() - started > report['execution_wall_seconds']:
            report.update(status='PARTIAL_OR_FAIL', error='receipt exceeded reserved audit time')
            report['execution_wall_seconds'] = time.monotonic() - started + 0.05
            report['audit_wall_seconds'] = audit_seconds(report['execution_wall_seconds'], report['process_wall_seconds'])
            report['aggregate_audit_wall_seconds'] = audit_seconds(prior_elapsed, prior_execution) + report['audit_wall_seconds']
            common.save(output / 'result.json', report)
    require(report['status'] == 'PASS', report.get('error', 'case run incomplete'))


if __name__ == '__main__':
    main()
