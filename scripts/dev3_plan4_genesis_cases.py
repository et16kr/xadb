"""Exact plan4 case execution with immutable source identity and prior-PASS exclusion.

This runner never invokes a lower-stage wrapper. Static and memory budgets are
separate; native orchestration imports the same selection and budget helpers.
"""
from pathlib import Path
import argparse
import collections
import datetime
import hashlib
import importlib.util
import json
import os
import selectors
import signal
import subprocess
import time

SCHEMA = 'XADB_PLAN4_CASE_RUN_V1'
BUDGETS = {'static': (10.0, 5.0, 1024 * 1024),
           'memory': (180.0, 30.0, 16 * 1024 * 1024)}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def select_cases(available, include, exclude, prior_pass):
    available = list(available)
    require(available and len(set(available)) == len(available), 'invalid inventory')
    universe = set(available)

    def parse(value):
        if value is None:
            return None
        values = value.split(',') if isinstance(value, str) else list(value)
        require(values and all(values) and len(set(values)) == len(values),
                'empty or duplicate selection')
        require(set(values) <= universe, 'unknown case selection')
        return set(values)

    included = parse(include)
    excluded = parse(exclude) or set()
    require(included is None or not included & excluded, 'include/exclude overlap')
    selected = (universe if included is None else included) - excluded
    require(selected and not selected & set(prior_pass),
            'empty selection or completed case replay')
    return selected


def budget_remaining(history_elapsed, current_elapsed, limit):
    require(history_elapsed >= 0 and current_elapsed >= 0 and limit > 0,
            'invalid elapsed accounting')
    remaining = limit - history_elapsed - current_elapsed
    require(remaining > 0, 'aggregate wall budget exhausted')
    return remaining


class MarkerParser:
    """Retain valid completed prefixes even when a later marker is malformed."""
    def __init__(self, selector, selected, identity=None):
        self.selector = selector
        self.selected = set(selected)
        self.identity = identity
        self.markers = {word: [] for word in ('BEGIN', 'PASS', 'FAIL', 'SKIP')}
        self.active = None
        self.active_since = None
        self.contexts = set()
        self.error = None

    def feed(self, line, now=None):
        if self.error is not None:
            return
        prefix = 'test ' + self.selector + ' ... '
        if line.startswith(prefix):
            line = line[len(prefix):]
        if not line.startswith('P4_'):
            return
        try:
            parts = line.rstrip('\r\n').split('\t')
            word = parts[0][3:]
            if word not in (*self.markers, 'CONTEXT'):
                return  # Separate census/native observations are not case outcomes.
            if word == 'CONTEXT':
                require(len(parts) == 5 and parts[1] in self.selected,
                        'foreign or malformed context')
                require(parts[1] not in self.contexts, 'duplicate context')
                if self.identity is not None:
                    require(parts[2:] == list(self.identity), 'case identity mismatch')
                self.contexts.add(parts[1])
                return
            require(word in self.markers and len(parts) == 2, 'malformed marker')
            case = parts[1]
            if word == 'SKIP':
                require(case not in self.selected, 'selected case skipped')
                require(case not in self.markers['SKIP'], 'duplicate skip')
            else:
                require(case in self.selected, 'foreign execution marker')
                if word == 'BEGIN':
                    require(self.active is None and case not in self.markers['BEGIN'],
                            'overlapping or duplicate begin')
                    if self.identity is not None:
                        require(case in self.contexts, 'missing exact identity context')
                    self.active, self.active_since = case, now
                else:
                    require(self.active == case, 'terminal without active begin')
                    self.active = self.active_since = None
            self.markers[word].append(case)
        except ValueError as error:
            self.error = str(error)

    def result(self):
        return {'markers': self.markers, 'active': self.active,
                'parser_error': self.error, 'contexts': sorted(self.contexts)}


def parse_markers(text, selector, selected, identity=None):
    parser = MarkerParser(selector, selected, identity)
    for line in text.splitlines():
        parser.feed(line)
    return parser.result()


def source_fingerprint(root):
    spec = importlib.util.spec_from_file_location('_plan4_source_identity',
                                                  root / 'scripts/check_dev0.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.source_fingerprint(root)


def validate_build(revision):
    revision = Path(revision).resolve()
    receipt_path = revision / 'build-receipt.json'
    build = json.loads(receipt_path.read_text())
    source = revision / 'source'
    binary = revision / 'xadb-engine-tests'
    manifest = source / 'tests/fixtures/dev3-plan4-genesis/manifest.json'
    require(build['exit_code'] == 0, 'unsuccessful build')
    require(build['command'] == ['cargo', 'test', '-p', 'xadb-engine', '--lib',
                                '--all-features', '--locked', '--no-run',
                                '--message-format=json'], 'unexpected build command')
    require(build['source_sha256'] == source_fingerprint(source), 'source drift')
    require(build['toolchain_file_sha256'] == sha(source / 'rust-toolchain.toml'),
            'toolchain input drift')
    require(build['cargo_lock_sha256'] == sha(source / 'Cargo.lock'), 'lockfile drift')
    require(build['build_log_sha256'] == sha(revision / 'build.log'), 'build log drift')
    require(build['binary_sha256'] == sha(binary), 'ELF drift')
    with binary.open('rb') as elf:
        require(elf.read(4) == b'\x7fELF', 'not an ELF')
    require(build['manifest_sha256'] == sha(manifest), 'manifest drift')
    require(build['owned_files'], 'missing owned source inventory')
    for relative, digest in build['owned_files'].items():
        path = source / relative
        require(path.resolve().is_relative_to(source) and sha(path) == digest,
                'owned diff mismatch')
    identity = (build['source_sha256'], build['binary_sha256'], build['manifest_sha256'])
    return source, binary, manifest, build, identity


def load_prior(unit_root, category):
    completed, references, elapsed, total_bytes = set(), [], 0.0, 0
    for path in sorted(Path(unit_root).rglob('result.json')):
        old = json.loads(path.read_text())
        if old.get('schema') != SCHEMA or old.get('category') != category:
            continue
        require(old.get('finalized') is True, 'unfinished prior run requires audit')
        indexed = {'result.json'} | {job[key] for job in old['jobs'] for key in ('stdout', 'stderr')}
        require({p.name for p in path.parent.iterdir()} == indexed,
                'unindexed prior output requires audit')
        require(old['execution_wall_seconds'] >= 0, 'invalid prior elapsed')
        elapsed += old['execution_wall_seconds']
        ids = []
        for job in old['jobs']:
            stdout, stderr = path.parent / job['stdout'], path.parent / job['stderr']
            require(sha(stdout) == job['stdout_sha256'] and sha(stderr) == job['stderr_sha256'],
                    'prior failure/input log changed')
            parsed = parse_markers(stdout.read_text(), job['selector'], job['selected_ids'],
                                   old['identity'])
            require(parsed == job['parsed'], 'prior marker record changed')
            passed = set(parsed['markers']['PASS'])
            require(not passed & completed, 'prior completed case replayed')
            completed |= passed
            ids.extend(sorted(passed))
            # Output beyond a truncation or malformed marker is unclassified.
            # Keep the recorded PASS prefix, but require an audit before any
            # suffix can be selected again.
            require(not job.get('truncated', False) and parsed['parser_error'] is None,
                    'uncertain prior output requires audit')
        total_bytes += sum(p.stat().st_size for p in path.parent.iterdir() if p.is_file())
        references.append({'path': str(path), 'sha256': sha(path), 'passed': ids})
    return completed, references, elapsed, total_bytes


def invoke(command, cwd, env, stdout, stderr, selector, selected, identity,
           timeout, case_timeout, output_limit):
    """Run one owned process group; enforce elapsed and streamed-output limits."""
    require(timeout > 0 and case_timeout > 0 and output_limit > 0, 'invalid process budget')
    parser = MarkerParser(selector, selected, identity)
    return _invoke_stream(command, cwd, env, stdout, stderr, parser, time.monotonic(),
                          timeout, case_timeout, output_limit)


def _invoke_stream(command, cwd, env, stdout, stderr, parser, started,
                   timeout, case_timeout, output_limit):
    written, observed, pending, stopped, child, finished = 0, 0, b'', None, None, False

    def terminate_group():
        if child is not None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    try:
        with Path(stdout).open('xb') as out, Path(stderr).open('xb') as err:
            child = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, start_new_session=True)
            with selectors.DefaultSelector() as ready:
                ready.register(child.stdout, selectors.EVENT_READ, out)
                ready.register(child.stderr, selectors.EVENT_READ, err)
                while ready.get_map():
                    now = time.monotonic()
                    if stopped is None:
                        if now - started >= timeout:
                            stopped = 'TIMEOUT'
                        elif parser.active_since is not None and now - parser.active_since >= case_timeout:
                            stopped = 'CASE_TIMEOUT'
                        elif parser.error is not None:
                            stopped = 'MARKER_ERROR'
                        if stopped is not None:
                            terminate_group()
                    for key, _ in ready.select(0.05):
                        block = os.read(key.fileobj.fileno(), 65536)
                        if not block:
                            ready.unregister(key.fileobj)
                            continue
                        observed += len(block)
                        available = max(0, output_limit - written)
                        kept = block[:available]
                        key.data.write(kept)
                        written += len(kept)
                        if len(block) > available and stopped is None:
                            stopped = 'OUTPUT_LIMIT'
                            terminate_group()
                        if key.fileobj is child.stdout:
                            pending += kept
                            while b'\n' in pending:
                                line, pending = pending.split(b'\n', 1)
                                parser.feed(line.decode('utf-8', errors='replace'), time.monotonic())
                if pending:
                    parser.feed(pending.decode('utf-8', errors='replace'), time.monotonic())
            code = child.wait()
            finished = True
            child.stdout.close()
            child.stderr.close()
    finally:
        if child is not None:
            if not finished:
                terminate_group()
            child.wait()
            child.stdout.close()
            child.stderr.close()
    parsed = parser.result()
    exact = (code == 0 and stopped is None and parsed['parser_error'] is None
             and parsed['active'] is None and set(parsed['markers']['PASS']) == parser.selected
             and not parsed['markers']['FAIL'])
    return {'pid': child.pid, 'exit_code': code, 'stop_reason': stopped,
            'elapsed_seconds': time.monotonic() - started, 'output_bytes': written,
            'observed_output_bytes': observed, 'captured_output_bytes': written,
            'truncated': observed != written,
            'parsed': parsed, 'exact': exact}


def main():
    cli = argparse.ArgumentParser()
    cli.add_argument('revision', type=Path)
    cli.add_argument('category', choices=BUDGETS)
    cli.add_argument('--output', required=True, type=Path)
    cli.add_argument('--include')
    cli.add_argument('--exclude')
    args = cli.parse_args()
    revision = args.revision.resolve()
    source, binary, manifest_path, build, identity = validate_build(revision)
    manifest = json.loads(manifest_path.read_text())
    all_cases = manifest['cases']
    available = [case for case, row in all_cases.items()
                 if row['phase'] == 'mandatory' and row['category'] == args.category]
    prior, references, prior_elapsed, prior_bytes = load_prior(revision.parent, args.category)
    selected = select_cases(available, args.include, args.exclude, prior)
    limit, function_limit, log_limit = BUDGETS[args.category]
    budget_remaining(prior_elapsed, 0, limit)
    require(prior_bytes < log_limit, 'aggregate output budget exhausted')
    output = args.output.resolve()
    require(output.parent == revision, 'run output must be a new revision child')
    output.mkdir()
    report = {'schema': SCHEMA, 'category': args.category, 'identity': identity,
              'build_receipt_sha256': sha(revision / 'build-receipt.json'),
              'runner_sha256': sha(Path(__file__)), 'selected_ids': sorted(selected),
              'prior_receipts': references, 'prior_elapsed_seconds': prior_elapsed,
              'prior_bytes': prior_bytes, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'jobs': [], 'cases': {case: 'NOT_RUN' for case in sorted(selected)},
              'execution_wall_seconds': 0.0, 'finalized': False, 'status': 'NOT_RUN'}
    save(output / 'result.json', report)
    groups = collections.defaultdict(list)
    for case in selected:
        groups[all_cases[case]['selector']].append(case)
    started = time.monotonic()
    try:
        for ordinal, (selector, cases) in enumerate(sorted(groups.items()), 1):
            validate_build(revision)
            allowance = min(function_limit, budget_remaining(prior_elapsed, time.monotonic() - started, limit))
            used = prior_bytes + sum(p.stat().st_size for p in output.iterdir() if p.is_file())
            # Leave bounded room for the receipt after process output.
            capacity = log_limit - used - 65536
            require(capacity > 0, 'aggregate output budget exhausted')
            command = [str(binary), selector, '--exact', '--nocapture', '--test-threads=1']
            env = os.environ.copy()
            env.pop('XADB_PLAN4_EXCLUDE', None)
            env['XADB_PLAN4_INCLUDE'] = ','.join(sorted(cases))
            for key, value in zip(('SOURCE', 'ELF', 'INPUT'), identity):
                env['XADB_PLAN4_GENESIS_' + key + '_SHA256'] = value
            stdout, stderr = f'{ordinal:02}.stdout', f'{ordinal:02}.stderr'
            planned = {'selector': selector, 'selected_ids': sorted(cases),
                       'command': command, 'cwd': str(source), 'stdout': stdout,
                       'stderr': stderr, 'state': 'INVOKING'}
            report['jobs'].append(planned)
            save(output / 'result.json', report)
            job = invoke(command, source, env, output / stdout, output / stderr,
                         selector, cases, identity, allowance, 5.0, capacity)
            job.update(selector=selector, selected_ids=sorted(cases), command=command,
                       cwd=str(source), include=env['XADB_PLAN4_INCLUDE'], exclude=None,
                       timeout_seconds=allowance, stdout=stdout, stderr=stderr,
                       stdout_sha256=sha(output / stdout), stderr_sha256=sha(output / stderr))
            planned.update(job, state='FINISHED')
            for case in job['parsed']['markers']['PASS']:
                report['cases'][case] = 'PASS'
            for case in job['parsed']['markers']['FAIL']:
                report['cases'][case] = 'FAIL'
            report['execution_wall_seconds'] = time.monotonic() - started
            save(output / 'result.json', report)
            print(json.dumps({'selector': selector, 'cases': len(cases),
                              'exact': job['exact'], 'elapsed_seconds': job['elapsed_seconds']}), flush=True)
            require(job['exact'], 'incomplete case execution')
            validate_build(revision)
        require(prior_elapsed + time.monotonic() - started <= limit, 'aggregate wall budget exceeded')
        report['status'] = 'PASS'
    except (ValueError, OSError, KeyError) as error:
        report['status'] = 'PARTIAL_OR_FAIL'
        report['error'] = str(error)
    finally:
        report['execution_wall_seconds'] = time.monotonic() - started
        report['finalized'] = all(job.get('state') == 'FINISHED' for job in report['jobs'])
        save(output / 'result.json', report)
        total = prior_bytes + sum(p.stat().st_size for p in output.iterdir() if p.is_file())
        if total > log_limit:
            report['status'] = 'PARTIAL_OR_FAIL'
            report['error'] = 'aggregate log/receipt budget exceeded'
            save(output / 'result.json', report)
    require(report['status'] == 'PASS', report.get('error', 'case run failed'))


if __name__ == '__main__':
    main()
