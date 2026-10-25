#!/usr/bin/env python3
"""Exact G5 ownership probes on copied overlays; never execute product binaries."""
import argparse
import difflib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import tomllib

sys.dont_write_bytecode = True
import dev3_plan4_clean_cases as case
import dev3_plan4_genesis_cases as common
from check_dev0 import SOURCE_PATHS
from check_dev3_allocation_borrows import preserve_artifacts

ROOT = Path(__file__).resolve().parent.parent
INPUT = Path('tests/fixtures/dev3-plan4-clean-owner/compiler')
ANCHOR = 'crates/engine/src/storage/wal_io/mutable_storage.rs'
PROBE = 'crates/engine/src/storage/wal_io/plan4_clean_compiler_probe.rs'
DECLARATION = '\n#[path = "plan4_clean_compiler_probe.rs"]\nmod plan4_clean_compiler_probe;\n'
SCHEMA = 'XADB_PLAN4_G5_COMPILER_RUN_V1'
INPUT_SCHEMA = 'XADB_PLAN4_G5_COMPILER_INPUT_V1'
IDS = tuple(f'CP{i:02}' for i in range(1, 11))
EXPECTATIONS = {
    'CP01': (None, None, ()), 'CP02': (None, None, ()),
    'CP03': ('E0499', 8, ('&mut *pages',)),
    'CP04': ('E0499', 7, ('&mut *operation',)),
    'CP05': ('E0499', 10, ('&mut *flush',)),
    'CP06': ('E0451', 5, ('request',)),
    'CP07': ('E0616', 5, ('pool',)),
    'CP08': ('E0515', 7, ('storage.bind_plan4_pages(core, frames, &mut operation).unwrap()',)),
    'CP09': ('E0505', 8, ('storage',)),
    'CP10': ('E0451', 5, ('snapshot', 'index', 'allocator', 'bytes', 'request')),
}
SECONDS, AUDIT_SECONDS, TOTAL_SECONDS = 120.0, 30.0, 150.0
PER_PROBE, DIAGNOSTICS = 30.0, 8 * 1024 * 1024
RECEIPT_RESERVE, RECEIPT_SECONDS = 131072, 0.05
require, sha = common.require, common.sha


def fresh_json(path, value):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def remaining_budget(prior_execution, prior_audit, execution, wall):
    require(execution >= 0 and wall >= execution, 'invalid compiler time accounting')
    common.budget_remaining(prior_execution, execution, SECONDS)
    common.budget_remaining(prior_audit, wall - execution, AUDIT_SECONDS)
    return common.budget_remaining(prior_execution + prior_audit, wall, TOTAL_SECONDS)


def finalize_report(session, lock, report, started, prior_execution, prior_audit,
                    prior_bytes, captured):
    """Keep the first receipt immutable; any late failure is an audit HOLD marker.

    The reserved final write is charged to both the audit and aggregate limits.
    An overrun does not retroactively make the first receipt reliable: the sibling
    failure marker takes precedence, and resume must obtain an independent audit.
    """
    failures = []
    try:
        lock.unlink()
    except OSError as error:
        failures.append('compiler cache lock release: ' + str(error))
    measured = time.monotonic() - started
    reserved_wall = measured + RECEIPT_SECONDS
    run_bytes = (session / 'run.json').stat().st_size
    budgeted_bytes = captured + run_bytes + RECEIPT_RESERVE
    report.update(finalized=True, measured_before_receipt_seconds=measured,
                  aggregate_elapsed_seconds=reserved_wall,
                  audit_elapsed_seconds=reserved_wall - report['compiler_elapsed_seconds'],
                  captured_diagnostic_bytes=captured, budgeted_output_bytes=budgeted_bytes,
                  receipt_reserve_bytes=RECEIPT_RESERVE, receipt_reserve_seconds=RECEIPT_SECONDS,
                  passed=[row['id'] for row in report['cases'] if row['status'] == 'PASS'],
                  not_run=[i for i in IDS if i not in {row['id'] for row in report['cases']}])
    try:
        remaining_budget(prior_execution, prior_audit, report['compiler_elapsed_seconds'],
                         reserved_wall)
        require(prior_bytes + budgeted_bytes <= DIAGNOSTICS, 'final receipt byte budget exceeded')
    except Exception as error:
        failures.append(str(error))
    if failures:
        report.update(status='FAIL', audit_hold=True, finalization_errors=failures.copy())
    result = session / 'result.json'
    fresh_json(result, report)
    # This sample is after fsync and lock release, including the actual result write.
    actual_wall = time.monotonic() - started
    try:
        require(actual_wall <= reserved_wall, 'final receipt time reserve exceeded')
        require(result.stat().st_size <= RECEIPT_RESERVE, 'final receipt byte reserve exceeded')
        remaining_budget(prior_execution, prior_audit, report['compiler_elapsed_seconds'], actual_wall)
        require(prior_bytes + captured + run_bytes + result.stat().st_size <= DIAGNOSTICS,
                'actual final receipt byte budget exceeded')
    except Exception as error:
        failures.append(str(error))
        report.update(status='FAIL', audit_hold=True, finalization_errors=failures.copy())
        marker = session / 'finalization-failure.json'
        fresh_json(marker, {'schema': SCHEMA, 'status': 'HOLD', 'run_sha256': report['run_sha256'],
                           'result_sha256': sha(result), 'errors': failures,
                           'observed_wall_seconds': actual_wall,
                           'scope': 'independent audit required; never automatically replay probes'})
        report['finalization_failure'] = str(marker)
    return report


def diagnostic_spans(program, identifier):
    code, line, tokens = EXPECTATIONS[identifier]
    lines = program.splitlines()
    markers = [i for i, value in enumerate(lines, 1) if '// expected diagnostic' in value]
    require(markers == ([] if line is None else [line]), 'diagnostic marker drift')
    result = []
    for token in tokens:
        text = lines[line - 1].split('// expected diagnostic', 1)[0]
        require(text.count(token) == 1, 'ambiguous diagnostic token')
        start = text.index(token) + 1
        result.append({'line_start': line, 'line_end': line,
                       'column_start': start, 'column_end': start + len(token)})
    return code, result


def inputs(source):
    directory = Path(source) / INPUT
    path = directory / 'manifest.json'
    require(path.is_file() and not path.is_symlink(), 'unsafe compiler manifest')
    manifest = json.loads(path.read_text())
    require(manifest['schema'] == INPUT_SCHEMA, 'foreign compiler input')
    require(manifest['anchor_relative_path'] == ANCHOR and
            manifest['probe_relative_path'] == PROBE and
            manifest['declaration'] == DECLARATION, 'injection scope drift')
    require((manifest['compiler_budget_seconds'], manifest['audit_budget_seconds'],
             manifest['aggregate_budget_seconds'], manifest['probe_budget_seconds'],
             manifest['diagnostic_budget_bytes']) ==
            (SECONDS, AUDIT_SECONDS, TOTAL_SECONDS, PER_PROBE, DIAGNOSTICS),
            'compiler budget drift')
    rows = manifest['cases']
    require([row['id'] for row in rows] == list(IDS), 'compiler inventory drift')
    require({p.name for p in directory.iterdir()} ==
            {'manifest.json', *(identifier + '.rs' for identifier in IDS)},
            'missing or unregistered compiler input')
    for row in rows:
        require(row['path'] == row['id'] + '.rs', 'probe path drift')
        program = directory / row['path']
        require(program.is_file() and not program.is_symlink(), 'unsafe compiler input')
        require((program.stat().st_size, sha(program)) == (row['bytes'], row['sha256']),
                'compiler program drift')
        code, spans = diagnostic_spans(program.read_text(), row['id'])
        require(row['expected_code'] == code and row['primary_spans'] == spans,
                'exact diagnostic expectation drift')
    return manifest


def verify(output, returncode, frozen):
    messages = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
    finished = [row for row in messages if row.get('reason') == 'build-finished']
    errors = [row['message'] for row in messages if row.get('reason') == 'compiler-message'
              and row.get('message', {}).get('level') == 'error']
    require(len(finished) == 1, 'missing or duplicate Cargo completion')
    if frozen['expected_code'] is None:
        require(returncode == 0 and finished[0].get('success') is True and not errors,
                'positive code did not compile')
        return []
    require(returncode == 101 and finished[0].get('success') is False and len(errors) == 1,
            'negative requires one completed compiler rejection')
    error = errors[0]
    require((error.get('code') or {}).get('code') == frozen['expected_code'],
            'unexpected diagnostic code')
    primary = [span for span in error.get('spans', []) if span.get('is_primary') is True]
    actual = [{key: span.get(key) for key in
               ('line_start', 'line_end', 'column_start', 'column_end')} for span in primary]
    require(primary and all(span.get('file_name') == PROBE for span in primary) and
            actual == frozen['primary_spans'], 'unexpected primary file/line/column span')
    return [{'code': frozen['expected_code'], 'primary_spans': primary}]


def checker_bindings(source):
    # Include the actual imported implementations, not merely expected filenames.
    paths = {Path(__file__).resolve()}
    paths.update(Path(module.__file__).resolve() for module in tuple(sys.modules.values())
                 if getattr(module, '__file__', None) and
                 Path(module.__file__).resolve().parent == ROOT / 'scripts')
    result = {}
    for path in sorted(paths):
        relative = path.relative_to(ROOT).as_posix()
        require(sha(path) == sha(source / relative), 'runner differs from frozen source')
        result[relative] = {'executed': str(path), 'sha256': sha(path)}
    return result


def check_bindings(source, bindings):
    for relative, item in bindings.items():
        require(sha(Path(item['executed'])) == item['sha256'] == sha(source / relative),
                'checker changed during run')


def clone_overlay(source, destination, program, evidence):
    destination.mkdir()
    for relative in SOURCE_PATHS:
        original, copied = source / relative, destination / relative
        require(original.exists() and not original.is_symlink(), 'missing/linked source root')
        if original.is_dir():
            require(not any(p.is_symlink() for p in original.rglob('*')), 'linked source input')
            shutil.copytree(original, copied, copy_function=os.link)
        else:
            os.link(original, copied)
    require(common.source_fingerprint(destination) == common.source_fingerprint(source),
            'overlay baseline drift')
    anchor, probe = destination / ANCHOR, destination / PROBE
    require(not probe.exists() and not probe.is_symlink(), 'probe already exists in baseline')
    original = anchor.read_text()
    # All baseline files may be hard links. Unlink before replacing the sole anchor.
    anchor.unlink()
    with anchor.open('x') as stream:
        stream.write(original + DECLARATION)
    with probe.open('x') as stream:
        stream.write(program)
    patch = ''.join(difflib.unified_diff(original.splitlines(True),
                    (original + DECLARATION).splitlines(True), fromfile=ANCHOR, tofile=ANCHOR))
    patch += ''.join(difflib.unified_diff([], program.splitlines(True),
                     fromfile='/dev/null', tofile=PROBE))
    with (evidence / 'probe.patch').open('x') as stream:
        stream.write(patch)
    require(sha(source / ANCHOR) != sha(anchor), 'missing isolated injection')


def load_prior(history):
    passed, references, elapsed, audit_elapsed, log_bytes = set(), [], 0.0, 0.0, 0
    for start_path in sorted(history.glob('*/run.json')):
        start = json.loads(start_path.read_text())
        require(start.get('schema') == SCHEMA, 'foreign run in compiler history')
        require(not (start_path.parent / 'finalization-failure.json').exists(),
                'prior finalization failure requires independent audit HOLD')
        path = start_path.parent / 'result.json'
        require(path.is_file(), 'unfinished compiler run requires audit')
        result = json.loads(path.read_text())
        require(result.get('schema') == SCHEMA and result.get('finalized') is True and
                result['run_sha256'] == sha(start_path), 'unfinalized or changed compiler run')
        require(result['identity'] == start['identity'] and result['selected'] == start['selected'],
                'prior report identity or selection drift')
        require(result.get('audit_hold') is not True, 'prior audit failure requires independent audit HOLD')
        old_source, _, _, _, old_identity = case.validate_build(Path(start['revision']))
        require(tuple(start['identity']) == old_identity and
                start['build_receipt_sha256'] == sha(Path(start['revision']) / 'build-receipt.json'),
                'prior source/ELF/build identity drift')
        original_manifest = inputs(old_source)
        require(start['compiler_manifest_sha256'] == sha(old_source / INPUT / 'manifest.json'),
                'prior compiler manifest drift')
        original_rows = {row['id']: row for row in original_manifest['cases']}
        for relative, binding in start['runner_bindings'].items():
            require(sha(old_source / relative) == binding['sha256'], 'prior checker input drift')
        require(sha(Path(start['budget_path'])) == start['budget_sha256'], 'prior budget drift')
        require(result['compiler_elapsed_seconds'] >= 0 and result['audit_elapsed_seconds'] >= 0,
                'negative elapsed accounting')
        elapsed += result['compiler_elapsed_seconds']
        audit_elapsed += result['audit_elapsed_seconds']
        run_pass = []
        attempted = set()
        actual_logs = 0
        definite_rejection = False
        for row in result['cases']:
            require(row['id'] in IDS and row['id'] in start['selected'] and row['id'] not in attempted,
                    'prior unknown or duplicate attempt')
            attempted.add(row['id'])
            frozen = original_rows[row['id']]
            require(row['expectation'] == frozen and row['probe_sha256'] == frozen['sha256'],
                    'prior expectation or probe differs from original compiler input')
            for key in ('stdout', 'stderr', 'patch'):
                if key in row:
                    require(sha(Path(row[key])) == row[key + '_sha256'], 'prior log/patch drift')
            for item in row.get('metadata', []):
                require(sha(Path(item['retained_path'])) == item['sha256'], 'prior metadata drift')
            actual_logs += sum(Path(row[key]).stat().st_size for key in ('stdout', 'stderr') if key in row)
            require(common.source_fingerprint(Path(row['source'])) == row['source_sha256'],
                    'prior overlay drift')
            require(sha(Path(row['source']) / PROBE) == row['probe_sha256'], 'prior probe drift')
            require(row.get('stop_reason', 'unknown') is None and row.get('truncated') is False and
                    all(key in row for key in ('stdout', 'stderr', 'exit_code')),
                    'uncertain prior compiler capture requires independent audit HOLD')
            output = Path(row['stdout']).read_text()
            messages = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
            finished = [item for item in messages if item.get('reason') == 'build-finished']
            require(len(finished) == 1 and row['exit_code'] in (0, 101) and
                    finished[0].get('success') is (row['exit_code'] == 0),
                    'uncertain prior Cargo completion requires independent audit HOLD')
            try:
                verify(output, row['exit_code'], frozen)
                diagnostic_complete = True
            except (ValueError, KeyError, TypeError, RuntimeError):
                diagnostic_complete = False
            if row['status'] == 'PASS':
                require(row['id'] not in passed, 'completed compiler case was repeated')
                require(diagnostic_complete, 'prior PASS diagnostic drift')
                require('metadata_error' not in row and isinstance(row.get('metadata'), list),
                        'prior PASS metadata receipt missing')
                if frozen['expected_code'] is None:
                    manifest = tomllib.loads((Path(row['source']) / 'crates/engine/Cargo.toml').read_text())
                    target_name = manifest.get('lib', {}).get(
                        'name', manifest['package']['name'].replace('-', '_'))
                    require(any(item['target'] == target_name for item in row['metadata']),
                            'prior positive engine metadata missing')
                passed.add(row['id'])
                run_pass.append(row['id'])
            else:
                require(row['status'] == 'FAIL', 'unfinished prior compiler attempt requires audit HOLD')
                require(not diagnostic_complete and 'metadata_error' not in row,
                        'completed compiler with later audit failure requires independent audit HOLD')
                definite_rejection = True
        require(result['passed'] == run_pass, 'prior PASS list drift')
        require(result['status'] == 'PASS' or definite_rejection,
                'prior run audit failure requires independent audit HOLD')
        require(result['captured_diagnostic_bytes'] == actual_logs and
                result['receipt_reserve_bytes'] == RECEIPT_RESERVE and
                result['receipt_reserve_seconds'] == RECEIPT_SECONDS and
                path.stat().st_size <= RECEIPT_RESERVE and
                result['budgeted_output_bytes'] == actual_logs + start_path.stat().st_size + RECEIPT_RESERVE,
                'prior output/receipt accounting drift')
        require(math.isclose(result['aggregate_elapsed_seconds'],
                result['compiler_elapsed_seconds'] + result['audit_elapsed_seconds'], abs_tol=1e-9),
                'prior aggregate time accounting drift')
        log_bytes += result['budgeted_output_bytes']
        references.append({'path': str(path), 'sha256': sha(path), 'passed': run_pass})
    return passed, references, elapsed, audit_elapsed, log_bytes


class CompilerParser:
    """Common process capture without interpreting compiler output as runtime markers."""
    active_since = error = None
    selected = set()

    def feed(self, line, now=None):
        pass

    def result(self):
        return {'parser_error': None, 'active': None, 'markers': {'PASS': [], 'FAIL': []}}


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('revision', type=Path)
    cli.add_argument('--history', required=True, type=Path)
    cli.add_argument('--run-name', required=True)
    cli.add_argument('--target-dir', required=True, type=Path)
    cli.add_argument('--execution-budget', required=True, type=Path)
    cli.add_argument('--include', help='comma-separated exact CP IDs; defaults to unfinished IDs')
    cli.add_argument('--exclude', help='comma-separated exact CP IDs')
    args = cli.parse_args()
    started = time.monotonic()
    budget = json.loads(args.execution_budget.read_text())
    require(budget['schema'] == 'XADB_PLAN4_CLEAN_EXECUTION_BUDGET_V1' and
            budget['compiler'] == {'execution_seconds': SECONDS, 'audit_seconds': AUDIT_SECONDS,
                                  'aggregate_seconds': TOTAL_SECONDS, 'output_bytes': DIAGNOSTICS},
            'execution budget profile mismatch')
    source, binary, manifest_path, build, identity = case.validate_build(args.revision)
    manifest = inputs(source)
    bindings = checker_bindings(source)
    history, target = args.history.resolve(), args.target_dir.resolve()
    require(args.run_name not in ('', '.', '..') and Path(args.run_name).name == args.run_name,
            'invalid run name')
    require(not history.is_relative_to(source) and not target.is_relative_to(source) and
            not source.is_relative_to(target), 'output/cache aliases frozen source')
    history.mkdir(parents=True, exist_ok=True)
    require(not any(p.is_symlink() for p in history.iterdir()), 'linked compiler history')
    prior, references, prior_elapsed, prior_audit, prior_bytes = load_prior(history)
    included = args.include if args.include is not None else [i for i in IDS if i not in prior]
    selected = common.select_cases(IDS, included, args.exclude, prior)
    require('CP01' in prior or 'CP01' in selected, 'missing positive symbol control')
    require(prior_bytes + RECEIPT_RESERVE < DIAGNOSTICS, 'diagnostic history budget exhausted')
    common.budget_remaining(prior_elapsed, 0, SECONDS)
    common.budget_remaining(prior_audit, time.monotonic() - started, AUDIT_SECONDS)
    session = history / args.run_name
    session.mkdir()
    target.mkdir(parents=True, exist_ok=True)
    lock = target / 'plan4-clean-compiler.lock'
    with lock.open('x') as stream:
        stream.write(str(session) + '\n')
    start = {'schema': SCHEMA, 'identity': identity, 'revision': str(args.revision.resolve()),
             'source': str(source), 'binary': str(binary), 'selected': sorted(selected),
             'prior': references, 'compiler_manifest_sha256': sha(source / INPUT / 'manifest.json'),
             'build_receipt_sha256': sha(args.revision.resolve() / 'build-receipt.json'),
             'budget_path': str(args.execution_budget.resolve()), 'budget_sha256': sha(args.execution_budget),
             'runner_bindings': bindings, 'target_dir': str(target)}
    fresh_json(session / 'run.json', start)
    report = {'schema': SCHEMA, 'run_sha256': sha(session / 'run.json'), 'status': 'FAIL',
              'finalized': False, 'cases': [], 'compiler_elapsed_seconds': 0.0,
              'identity': identity, 'prior': references, 'selected': sorted(selected),
              'scope': 'cargo-check metadata only; no product/runtime execution'}
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADB', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    environment.update(CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0', PYTHONDONTWRITEBYTECODE='1')
    command = ['cargo', 'check', '-p', 'xadb-engine', '--lib', '--all-features',
               '--locked', '--offline', '--message-format=json']
    current = None
    captured = 0
    run_bytes = (session / 'run.json').stat().st_size

    def check_budget():
        elapsed = time.monotonic() - started
        require(prior_bytes + captured + run_bytes + RECEIPT_RESERVE < DIAGNOSTICS,
                'diagnostic and final receipt budget exhausted')
        return remaining_budget(prior_elapsed, prior_audit, report['compiler_elapsed_seconds'],
                                elapsed + RECEIPT_SECONDS)

    try:
        report['toolchain'] = subprocess.check_output(['rustc', '-Vv'], cwd=source,
                                env=environment, text=True, timeout=10)
        sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], cwd=source,
                                env=environment, text=True, timeout=10).strip())
        report['tools'] = {str(sysroot / 'bin' / name): sha(sysroot / 'bin' / name)
                           for name in ('rustc', 'cargo')}
        for frozen in manifest['cases']:
            identifier = frozen['id']
            if identifier not in selected:
                print('P4_SKIP\t' + identifier, flush=True)
                continue
            evidence = session / identifier
            evidence.mkdir()
            overlay = evidence / 'source'
            program = (source / INPUT / frozen['path']).read_text()
            clone_overlay(source, overlay, program, evidence)
            current = {'id': identifier, 'status': 'RUNNING', 'expectation': frozen,
                       'source': str(overlay), 'source_sha256': common.source_fingerprint(overlay),
                       'probe_sha256': sha(overlay / PROBE), 'command': command,
                       'patch': str(evidence / 'probe.patch'), 'patch_sha256': sha(evidence / 'probe.patch')}
            report['cases'].append(current)
            check_budget()
            print('P4_BEGIN\t' + identifier, flush=True)
            began = time.monotonic()
            try:
                timeout = min(PER_PROBE, check_budget(), common.budget_remaining(prior_elapsed,
                              report['compiler_elapsed_seconds'], SECONDS))
                outcome = common._invoke_stream(command, overlay, environment,
                    evidence / 'compiler.jsonl', evidence / 'compiler.stderr', CompilerParser(),
                    began, timeout, timeout,
                    DIAGNOSTICS - prior_bytes - captured - run_bytes - RECEIPT_RESERVE)
                current.update(outcome)
            finally:
                current['elapsed_seconds'] = time.monotonic() - began
                report['compiler_elapsed_seconds'] += current['elapsed_seconds']
                for key, filename in [('stdout', 'compiler.jsonl'), ('stderr', 'compiler.stderr')]:
                    path = evidence / filename
                    if path.exists():
                        current[key], current[key + '_sha256'] = str(path), sha(path)
                        captured += path.stat().st_size
                # Interrupted probes also retain every complete artifact event before
                # a later invocation can reuse this cache. Raw partial logs stay intact.
                if 'stdout' in current:
                    try:
                        lines = []
                        for line in Path(current['stdout']).read_text().splitlines():
                            if line.startswith('{'):
                                try:
                                    json.loads(line)
                                except json.JSONDecodeError:
                                    continue
                                lines.append(line)
                        current['metadata'] = preserve_artifacts('\n'.join(lines), evidence,
                                                                 target, require_engine=False)
                    except Exception as error:
                        current['metadata_error'] = str(error)
            output = Path(current['stdout']).read_text()
            require(current['stop_reason'] is None and not current['truncated'], 'compiler capture interrupted')
            current['diagnostics'] = verify(output, current['exit_code'], frozen)
            # A successful compiler/diagnostic result cannot be replayed merely
            # because a later metadata/hash/budget audit fails.
            current['diagnostic_completed'] = True
            require('metadata_error' not in current, 'compiler metadata preservation failed')
            if frozen['expected_code'] is None:
                require(any(row['target'] == 'engine' for row in current['metadata']),
                        'positive has no preserved engine metadata')
            require(common.source_fingerprint(overlay) == current['source_sha256'], 'overlay changed')
            require(case.validate_build(args.revision)[4] == identity, 'baseline/ELF/input drift')
            check_bindings(source, bindings)
            check_budget()
            current['status'] = 'PASS'
            print('P4_PASS\t' + identifier, flush=True)
            current = None
        require(all(sha(Path(path)) == digest for path, digest in report['tools'].items()), 'tool drift')
        require(sha(args.execution_budget) == start['budget_sha256'], 'budget changed during run')
        check_budget()
        report['status'] = 'PASS'
    except (Exception, KeyboardInterrupt) as error:
        report['error'] = str(error)
        report['audit_hold'] = current is None or current.get('diagnostic_completed') is True
        if current is not None:
            current['status'] = 'FAIL'
            print('P4_FAIL\t' + current['id'], flush=True)
    finally:
        finalize_report(session, lock, report, started, prior_elapsed, prior_audit, prior_bytes, captured)
    print(json.dumps({'status': report['status'], 'passed': report['passed'],
                      'result': str(session / 'result.json'), 'error': report.get('error'),
                      'finalization_failure': report.get('finalization_failure')}))
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
