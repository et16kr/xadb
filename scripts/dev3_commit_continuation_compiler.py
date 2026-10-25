#!/usr/bin/env python3
"""Seven frozen B1 Commit continuation checks on disposable source overlays.

This checker never executes a product binary or the A compiler inventory.
Prior runs in the dedicated unit directory are audited before any new selection.
"""
import argparse
import difflib
import json
import os
from pathlib import Path
import shutil
import sys
import time

sys.dont_write_bytecode = True
import dev3_plan4_genesis_cases as process
from check_dev0 import SOURCE_PATHS, digest, source_fingerprint
from check_dev3_allocation_borrows import preserve_artifacts

INPUT = Path('tests/fixtures/dev3-commit-continuation/compiler')
ANCHOR = 'crates/engine/src/storage/mod.rs'
PROBE = 'crates/engine/src/storage/commit_continuation_compiler_probe.rs'
DECLARATION = '\n#[path = "commit_continuation_compiler_probe.rs"]\nmod commit_continuation_compiler_probe;\n'
IDS = ('baseline', 'positive', 'double_consume', 'owner_reborrow',
       'pending_double_consume', 'raw_grant', 'physical_durable')
BUDGET = {'worker_seconds': 180, 'audit_seconds': 30, 'total_seconds': 210,
          'case_seconds': 30, 'output_bytes': 8 << 20, 'artifacts_bytes': 1 << 30}
SCHEMA = 'XADB_COMMIT_CONTINUATION_COMPILER_RUN_V1'
MANIFEST_SHA256 = '491b1b663b7edac154b86bc6586fa0c990e86d878c5fe329593371f0a9252581'
RECEIPT_RESERVE = 131072


def require(value, message):
    if not value:
        raise ValueError(message)


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def inputs(source):
    directory = source / INPUT
    require(digest(directory / 'manifest.json') == MANIFEST_SHA256, 'frozen compiler manifest changed')
    manifest = json.loads((directory / 'manifest.json').read_text())
    require(manifest['schema'] == 'XADB_COMMIT_CONTINUATION_COMPILER_INPUT_V1', 'input schema')
    require((manifest['anchor'], manifest['probe'], manifest['declaration']) ==
            (ANCHOR, PROBE, DECLARATION), 'injection scope drift')
    require(manifest['budget'] == BUDGET, 'budget drift')
    require((manifest['baseline_count'], manifest['semantic_count'],
             manifest['positive_count'], manifest['negative_count']) == (1, 6, 1, 5), 'inventory totals')
    rows = manifest['cases']
    require(tuple(row['name'] for row in rows) == IDS, 'exact inventory')
    require({p.name for p in directory.iterdir()} ==
            {'manifest.json', *(name + '.rs' for name in IDS[1:])}, 'extra/missing probe')
    for row in rows[1:]:
        path = directory / row['path']
        require(row['path'] == row['name'] + '.rs' and path.is_file() and not path.is_symlink(), 'probe path')
        require(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'], 'probe hash')
        marks = [i for i, line in enumerate(path.read_text().splitlines(), 1)
                 if '// expected diagnostic' in line]
        require(marks == [s['line_start'] for s in row['primary_spans']], 'diagnostic marker')
    return manifest


def prior_runs(unit):
    """Every previous attempt remains visible, including interrupted/failed work."""
    result = []
    for directory in sorted(unit.iterdir()):
        if not directory.is_dir():
            continue
        path = directory / 'run.json'
        require(path.is_file(), 'unclassified prior compiler directory: audit required')
        initial = json.loads(path.read_text())
        require(initial.get('schema') == SCHEMA, 'foreign compiler unit history')
        receipt = path.with_name('result.json')
        require(receipt.is_file(), 'unfinished prior B1 compiler attempt: audit required')
        validate_prior_summary(initial, json.loads(receipt.read_text()))
        result.append(receipt)
    return result


def validate_prior_summary(initial, receipt):
    """A completed run must account for every originally selected diagnostic."""
    require(initial.get('schema') == SCHEMA and receipt.get('schema') == SCHEMA,
            'foreign prior compiler schema')
    require(initial.get('status') == 'RUNNING' and initial.get('finalized') is False
            and initial.get('cases') == [], 'initial run receipt changed')
    require(receipt.get('status') == 'PASS' and receipt.get('finalized') is True
            and not receipt.get('budget_failure') and receipt.get('not_run') == [],
            'failed or incomplete prior run requires audit')
    for key in ('source_sha256', 'manifest_sha256', 'budget', 'selected', 'prior',
                'tool_bindings', 'unit_root', 'product_execution'):
        require(key in initial and key in receipt and initial[key] == receipt[key],
                'initial/final prior run binding mismatch: ' + key)
    require(receipt['product_execution'] is False, 'prior executed product code')
    selected = receipt['selected']
    require(isinstance(selected, list) and selected and len(set(selected)) == len(selected)
            and set(selected) <= set(IDS), 'invalid prior selection')
    rows = receipt.get('cases')
    require(isinstance(rows, list) and [row.get('name') for row in rows] == selected,
            'prior completion inventory differs from selected input')
    for row in rows:
        require(row.get('status') == 'PASS' and row.get('diagnostic_completed') is True
                and row.get('evidence_validated') is True,
                'prior selected diagnostic is not fully validated')
    require(0 <= receipt['compiler_seconds'] <= receipt['elapsed_seconds']
            and receipt['log_bytes'] >= 0 and receipt['artifact_bytes'] >= 0,
            'invalid prior resource accounting')


def select_cases(requested, completed):
    selected = list(requested) if requested is not None else [name for name in IDS if name not in completed]
    require(selected and len(set(selected)) == len(selected) and set(selected) <= set(IDS)
            and not set(selected) & completed, 'empty/duplicate/completed selection')
    require('baseline' in completed or 'baseline' in selected, 'actual baseline required')
    return sorted(selected, key=IDS.index)


def verify(output, code, expected):
    messages = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
    done = [v for v in messages if v.get('reason') == 'build-finished']
    errors = [v['message'] for v in messages if v.get('reason') == 'compiler-message'
              and v.get('message', {}).get('level') == 'error']
    require(len(done) == 1, 'missing/duplicate Cargo completion')
    if expected['expected_code'] is None:
        require(code == 0 and done[0].get('success') is True and not errors, 'baseline/positive failure')
        return []
    require(code == 101 and done[0].get('success') is False and len(errors) == 1,
            'negative needs exactly one completed rejection')
    error = errors[0]
    require((error.get('code') or {}).get('code') == expected['expected_code'], 'wrong diagnostic code')
    primary = [s for s in error['spans'] if s['is_primary']]
    require(primary and all(s['file_name'] == PROBE for s in primary), 'wrong primary file')
    actual = [{key: s[key] for key in ('line_start', 'line_end', 'column_start', 'column_end')}
              for s in primary]
    require(actual == expected['primary_spans'], 'wrong exact primary code/span')
    return [{'code': expected['expected_code'], 'primary_spans': primary}]


def clone(source, output, program=None):
    output.mkdir()
    for name in SOURCE_PATHS:
        old, new = source / name, output / name
        require(old.exists() and not old.is_symlink(), 'source root missing/symlinked')
        if old.is_dir():
            require(not any(p.is_symlink() for p in old.rglob('*')), 'symlinked source member')
            shutil.copytree(old, new, copy_function=os.link)
        else:
            os.link(old, new)
    require(source_fingerprint(output) == source_fingerprint(source), 'overlay baseline drift')
    if program is not None:
        anchor = output / ANCHOR
        original = anchor.read_text()
        require(not (output / PROBE).exists(), 'probe already exists')
        anchor.unlink()  # break the hardlink before the sole injected edit
        with anchor.open('x') as stream:
            stream.write(original + DECLARATION)
        with (output / PROBE).open('x') as stream:
            stream.write(program)
        return ''.join(difflib.unified_diff(original.splitlines(True),
            (original + DECLARATION).splitlines(True), fromfile=ANCHOR, tofile=ANCHOR)) + ''.join(
            difflib.unified_diff([], program.splitlines(True), fromfile='/dev/null', tofile=PROBE))
    return ''


def artifact_bytes(directory):
    seen, total = set(), 0
    for path in directory.rglob('*'):
        if path.is_file():
            try:
                stat = path.stat()
            except FileNotFoundError:
                continue  # Cargo can remove a temporary artifact during polling.
            identity = (stat.st_dev, stat.st_ino)
            if identity not in seen:
                seen.add(identity)
                total += stat.st_size
    return total


class Parser:
    active_since = None
    selected = set()

    def __init__(self, output, prior_artifacts):
        self.output, self.prior_artifacts = output, prior_artifacts
        self.next_poll, self.failure = 0.0, None

    @property
    def error(self):
        # The shared process loop polls this even when Cargo produces no output.
        # A violation causes the original child process group to be killed.
        now = time.monotonic()
        if now >= self.next_poll and self.failure is None:
            self.next_poll = now + 0.25
            if (self.prior_artifacts + artifact_bytes(self.output) + RECEIPT_RESERVE
                    >= BUDGET['artifacts_bytes']):
                self.failure = 'ARTIFACT_LIMIT'
        return self.failure

    def feed(self, line, now=None):
        pass

    def result(self):
        return {'parser_error': self.error, 'active': None, 'markers': {'PASS': [], 'FAIL': []}}


def tool_bindings(source):
    root = Path(__file__).resolve().parents[1]
    bindings = {}
    for module in tuple(sys.modules.values()):
        filename = getattr(module, '__file__', None)
        if filename and Path(filename).resolve().parent == root / 'scripts':
            path = Path(filename).resolve()
            relative = path.relative_to(root)
            require(digest(path) == digest(source / relative), 'loaded checker/source mismatch')
            bindings[str(relative)] = digest(path)
    return bindings


def preserve_bounded(text, evidence, target, output, prior_artifacts, require_engine):
    additional = 0
    for line in text.splitlines():
        if not line.startswith('{'):
            continue
        row = json.loads(line)
        if row.get('reason') == 'compiler-artifact':
            for filename in row.get('filenames', []):
                path = Path(filename).resolve()
                require(path.is_relative_to(target.resolve()) and path.is_file(), 'artifact outside target')
                additional += path.stat().st_size
    require(prior_artifacts + artifact_bytes(output) + additional + RECEIPT_RESERVE
            < BUDGET['artifacts_bytes'],
            'projected metadata preservation cap')
    return preserve_artifacts(text, evidence, target, require_engine)


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--source-root', type=Path, required=True)
    cli.add_argument('--source-sha256', required=True)
    cli.add_argument('--output', type=Path, required=True)
    cli.add_argument('--unit-root', type=Path, required=True,
                     help='dedicated B1 compiler history; all earlier attempts are audited')
    cli.add_argument('--case', action='append', choices=IDS)
    cli.add_argument('--prior', action='append', type=Path, default=[])
    args = cli.parse_args()
    started = time.monotonic()
    source = args.source_root.resolve(strict=True)
    unit = args.unit_root.resolve(strict=True)
    output = args.output.resolve()
    require(unit.is_dir() and output.parent == unit and not output.exists(),
            'output must be a fresh direct child of the dedicated unit directory')
    require(not unit.is_relative_to(source) and not source.is_relative_to(unit),
            'compiler history and approved source must be disjoint')
    require(source_fingerprint(source) == args.source_sha256, 'unapproved source fingerprint')
    manifest = inputs(source)
    source_bytes = artifact_bytes(source)
    bindings = tool_bindings(source)
    rows = {row['name']: row for row in manifest['cases']}
    completed, prior_seconds, prior_bytes = set(), 0.0, 0
    prior_worker, prior_audit, prior_artifacts = 0.0, 0.0, 0
    prior_refs = []
    prior_paths = sorted({path.resolve() for path in [*prior_runs(unit), *args.prior]})
    for path in prior_paths:
        receipt = json.loads(path.read_text())
        initial = path.with_name('run.json')
        require(initial.is_file(), 'missing original prior run receipt')
        validate_prior_summary(json.loads(initial.read_text()), receipt)
        require(receipt['schema'] == SCHEMA and receipt.get('finalized') is True, 'uncertain prior run: audit required')
        require(receipt['status'] == 'PASS' and not receipt.get('budget_failure'),
                'failed prior run requires explicit audit; do not replay completed diagnostics')
        require(receipt['source_sha256'] == args.source_sha256
                and receipt['manifest_sha256'] == MANIFEST_SHA256
                and receipt['budget'] == BUDGET and receipt['tool_bindings'] == bindings,
                'prior source/manifest/tool/budget mismatch: compatibility audit required')
        for row in receipt['cases']:
            if row.get('diagnostic_completed'):
                require(row['status'] == 'PASS' and row.get('evidence_validated') is True,
                        'completed diagnostic has invalid evidence: audit required, no automatic replay')
                require(row['name'] not in completed, 'prior duplicate completion')
                require(row['expectation'] == rows[row['name']], 'completed probe changed: audit required')
                require(digest(Path(row['stdout'])) == row['stdout_sha256'], 'prior stdout drift')
                require(digest(Path(row['stderr'])) == row['stderr_sha256'], 'prior stderr drift')
                evidence = Path(row['stdout']).parent
                require(source_fingerprint(evidence / 'source') == row['source_sha256'], 'prior overlay drift')
                require(digest(evidence / 'probe.patch') == row['patch_sha256'], 'prior injection drift')
                for item in row['metadata']:
                    retained = Path(item['retained_path'])
                    require(retained.stat().st_size == item['bytes'] and digest(retained) == item['sha256'],
                            'prior metadata drift')
                verify(Path(row['stdout']).read_text(), row['exit_code'], row['expectation'])
                completed.add(row['name'])
        prior_worker += receipt['compiler_seconds']
        prior_audit += receipt['elapsed_seconds'] - receipt['compiler_seconds']
        prior_artifacts += receipt['artifact_bytes']
        prior_seconds += receipt['elapsed_seconds']
        prior_bytes += receipt['log_bytes']
        prior_refs.append({'path': str(path.resolve()), 'sha256': digest(path)})
    selected = select_cases(args.case, completed)
    output.mkdir(parents=False, exist_ok=False)
    report = {'schema': SCHEMA, 'status': 'RUNNING', 'source_sha256': args.source_sha256,
              'manifest_sha256': digest(source / INPUT / 'manifest.json'), 'budget': BUDGET,
              'selected': selected, 'prior': prior_refs, 'cases': [], 'finalized': False,
              'product_execution': False, 'compiler_seconds': 0.0}
    report['unit_root'] = str(unit)
    report['tool_bindings'] = bindings
    save(output / 'run.json', report)
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in (
            'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
            'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    target = output / 'target'
    environment.update(CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0')
    current = None
    try:
        for name in selected:
            elapsed = time.monotonic() - started
            require(prior_seconds + elapsed < BUDGET['total_seconds'] - 1, 'aggregate time budget')
            require(prior_artifacts + artifact_bytes(output) + source_bytes + RECEIPT_RESERVE
                    < BUDGET['artifacts_bytes'], 'projected source overlay artifact cap')
            evidence = output / name
            evidence.mkdir()
            program = None if name == 'baseline' else (source / INPUT / (name + '.rs')).read_text()
            overlay = evidence / 'source'
            patch = clone(source, overlay, program)
            with (evidence / 'probe.patch').open('x') as stream:
                stream.write(patch)
            current = {'name': name, 'expectation': rows[name], 'status': 'RUNNING',
                       'source_sha256': source_fingerprint(overlay), 'patch_sha256': digest(evidence / 'probe.patch')}
            report['cases'].append(current)
            print('B1_COMMIT_COMPILER_BEGIN\t' + name, flush=True)
            command = ['cargo', 'check', '-p', 'xadb-engine', '--lib', '--all-features',
                       '--locked', '--offline', '--message-format=json']
            allowance = min(BUDGET['case_seconds'], BUDGET['worker_seconds'] - prior_worker - report['compiler_seconds'],
                            BUDGET['total_seconds'] - prior_seconds - (time.monotonic() - started) - 1)
            require(allowance > 0, 'compiler time budget')
            logs = sum(p.stat().st_size for p in output.rglob('compiler.*') if p.is_file())
            capacity = BUDGET['output_bytes'] - prior_bytes - logs - RECEIPT_RESERVE
            require(capacity > 0, 'diagnostic output cap')
            begin = time.monotonic()
            try:
                outcome = process._invoke_stream(command, overlay, environment,
                    evidence / 'compiler.jsonl', evidence / 'compiler.stderr', Parser(output, prior_artifacts),
                    begin, allowance, allowance, capacity)
                current.update(outcome)
            finally:
                report['compiler_seconds'] += time.monotonic() - begin
                for key, filename in [('stdout', 'compiler.jsonl'), ('stderr', 'compiler.stderr')]:
                    path = evidence / filename
                    if path.exists():
                        current[key], current[key + '_sha256'] = str(path), digest(path)
            require(current['stop_reason'] is None and not current['truncated'], 'interrupted/truncated capture')
            text = Path(current['stdout']).read_text()
            current['diagnostic_completed'] = any(
                json.loads(line).get('reason') == 'build-finished'
                for line in text.splitlines() if line.startswith('{'))
            current['diagnostics'] = verify(text, current['exit_code'], rows[name])
            current['metadata'] = preserve_bounded(text, evidence, target, output, prior_artifacts,
                                                  require_engine=name in ('baseline', 'positive'))
            require(source_fingerprint(overlay) == current['source_sha256'], 'overlay source changed')
            require(source_fingerprint(source) == args.source_sha256, 'baseline source changed')
            current['evidence_validated'] = True
            current['status'] = 'PASS'
            print('B1_COMMIT_COMPILER_PASS\t' + name, flush=True)
            current = None
        require(tool_bindings(source) == bindings, 'checker drift')
        report['status'] = 'PASS'
    except (Exception, KeyboardInterrupt) as error:
        report['status'], report['error'] = 'FAIL', str(error)
        if current is not None:
            current['status'] = 'FAIL'
            print('B1_COMMIT_COMPILER_FAIL\t' + current['name'], flush=True)
    finally:
        report['log_bytes'] = sum(p.stat().st_size for p in output.rglob('compiler.*') if p.is_file())
        report['artifact_bytes'] = artifact_bytes(output)
        report['elapsed_seconds'] = time.monotonic() - started
        audit = report['elapsed_seconds'] - report['compiler_seconds']
        if (prior_seconds + report['elapsed_seconds'] > BUDGET['total_seconds']
                or prior_audit + audit > BUDGET['audit_seconds']
                or prior_worker + report['compiler_seconds'] > BUDGET['worker_seconds']
                or prior_bytes + report['log_bytes'] + RECEIPT_RESERVE > BUDGET['output_bytes']
                or prior_artifacts + report['artifact_bytes'] + RECEIPT_RESERVE > BUDGET['artifacts_bytes']):
            report['status'], report['budget_failure'] = 'FAIL', True
        report['finalized'] = True
        report['not_run'] = [name for name in selected if name not in {v['name'] for v in report['cases']}]
        save(output / 'result.json', report)
    print(json.dumps({'status': report['status'], 'result': str(output / 'result.json')}, sort_keys=True))
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
