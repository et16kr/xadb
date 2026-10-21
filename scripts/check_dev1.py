#!/usr/bin/env python3
"""Verify DEV1 foundation/platform boundaries and DEV0 regression; retain local evidence."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

# Verification imports must not mutate the source fingerprint with bytecode caches.
sys.dont_write_bytecode = True
from check_dev0 import check_report_paths, run_logged_command, source_fingerprint, verify_source_identity, write_report

ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, help='new DEV1 report path; never overwritten')
    parser.add_argument('--dev0-report', type=Path, help='distinct new DEV0 report path; never overwritten')
    args = parser.parse_args()
    try:
        check_report_paths([args.report, args.dev0_report])
    except (OSError, ValueError) as error:
        parser.error(str(error))
    parent = ROOT / 'artifacts/dev1'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='check-', dir=parent))
    environment = os.environ.copy()
    environment['CARGO_TARGET_DIR'] = str(ROOT / 'target')
    checks = []
    report = {
        'schema_version': 1, 'date': datetime.now(timezone.utc).isoformat(),
        'scope': 'DEV1 foundation and Linux platform F1 local verification; not a database or release qualification',
        'status': 'RUNNING', 'artifact_directory': str(session.relative_to(ROOT)),
        'checks': checks, 'engine_tests': 'VOLATILE_SUBSTRATE_ONLY_NO_DURABILITY',
        'official_release': False, 'protected_ci_execution': 'NOT_RUN',
        'cross_version_compatibility': 'NOT_REQUIRED_INTERNAL_FRESH_BUILD',
    }

    def command(name, argv, expected=0, display_output=True, timeout_seconds=600):
        print(f'{name}: {" ".join(argv)}', flush=True)
        log = session / (name + '.log')
        record = {'id': name, 'command': argv, 'expected_exit_code': expected,
                  'log': str(log.relative_to(ROOT))}
        checks.append(record)
        result = run_logged_command(argv, cwd=ROOT, environment=environment, log=log,
                                    record=record, timeout_seconds=timeout_seconds)
        record['status'] = 'PASS' if result.returncode == expected else 'FAIL'
        if display_output or result.returncode != expected:
            print(result.stdout.decode(errors='replace'), end='', flush=True)
        if result.returncode != expected:
            raise RuntimeError(f'{name}: expected {expected}, got {result.returncode}')
        return result.stdout

    exit_code = 0
    try:
        source_before = source_fingerprint()
        report['source_sha256_before'] = source_before
        # Includes fmt, clippy, all feature workspace tests, clean runtime and fresh source build.
        regression_path = session / 'dev0-regression.json'
        command('dev0-regression', ['python3', 'scripts/check_dev0.py', '--report', str(regression_path)],
                timeout_seconds=1800)
        regression = json.loads(regression_path.read_text())
        if regression['status'] != 'PASS':
            raise RuntimeError('DEV0 regression did not pass')
        if args.dev0_report:
            write_report(args.dev0_report, regression)
        report['source_sha256'] = regression['source_sha256']
        verify_source_identity(source_before, report['source_sha256'])
        report['toolchain'] = regression['toolchain']
        report['target'] = regression['target']
        report['os_release'] = regression['os_release']
        report['dev0_regression'] = str(regression_path.relative_to(ROOT))
        # Inspect the actual normal dependency closure and perform a fresh isolated package build.
        metadata = json.loads(command('dependency-boundary', ['cargo', 'metadata', '--format-version', '1', '--locked', '--offline',
                                                             '--filter-platform', regression['target']], display_output=False))
        packages = {p['id']: p for p in metadata['packages']}
        nodes = {n['id']: n for n in metadata['resolve']['nodes']}
        for name, allowed, report_key, build_check in (
            ('xadb-foundation', {'xadb-foundation'}, 'normal_dependency_closure', 'independent-build'),
            ('xadb-platform', {'xadb-platform', 'xadb-foundation'},
             'platform_normal_dependency_closure', 'platform-independent-build'),
        ):
            root = next(p for p in packages.values() if p['name'] == name)
            pending, closure = [root['id']], set()
            while pending:
                package = pending.pop()
                if package in closure:
                    continue
                closure.add(package)
                pending.extend(dep['pkg'] for dep in nodes[package]['deps']
                               if any(kind['kind'] is None for kind in dep['dep_kinds']))
            names = sorted(packages[p]['name'] for p in closure)
            if any(packages[p]['source'] is None and packages[p]['name'] not in allowed for p in closure):
                raise RuntimeError(f'{name} imports a forbidden workspace layer')
            if name == 'xadb-foundation' and 'nix' in names:
                raise RuntimeError('foundation imports the Linux syscall adapter dependency')
            report[report_key] = names
            with tempfile.TemporaryDirectory(prefix=name + '-build-') as temporary:
                command(build_check, ['cargo', 'build', '-p', name, '--no-default-features',
                                      '--locked', '--offline', '--target-dir', temporary])
        command('foundation-default-tests', ['cargo', 'test', '-p', 'xadb-foundation', '--no-default-features', '--locked'])
        command('platform-default-tests', ['cargo', 'test', '-p', 'xadb-platform', '--no-default-features', '--locked'])
        controller = ROOT / 'target/debug/xadbtest'
        worker = ROOT / 'target/debug/xadbtest-foundation-worker'
        case = ROOT / 'tests/cases/dev1-foundation.json'
        binaries = session / 'binaries'
        binaries.mkdir()
        for path in (controller, worker):
            shutil.copy2(path, binaries / path.name)
        shutil.copy2(case, session / 'case.json')
        controller = binaries / controller.name
        worker = binaries / worker.name
        case = session / 'case.json'

        def run_case(name, case_path, expected):
            output = command(name, [str(controller), 'run', '--case', str(case_path),
                                    '--case-sha256', digest(case_path), '--worker', str(worker),
                                    '--worker-sha256', digest(worker), '--artifacts', str(session / name)], expected)
            directory = Path(json.loads(output)['run_directory'])
            verdict = json.loads((directory / 'verdict.json').read_text())
            manifest = json.loads((directory / 'manifest.json').read_text())
            for filename, expected_hash in json.loads((directory / 'artifacts.json').read_text()).items():
                if digest(directory / filename) != expected_hash:
                    raise RuntimeError(f'corrupt evidence: {directory / filename}')
            verify_source_identity(source_before, manifest['source_sha256'])
            return directory, verdict

        directory, verdict = run_case('foundation-worker', case, 0)
        if verdict['case_verdict'] != 'PASS' or verdict['assertions_checked'] != 7:
            raise RuntimeError('foundation worker did not validate all vectors')
        report['smoke'] = {'run_directory': str(directory.relative_to(ROOT)), 'verdict': verdict,
                           'worker_sha256': digest(worker), 'case_sha256': digest(case)}
        # Literal expected value is deliberately wrong: production worker must not dictate PASS.
        mutated = json.loads(case.read_text())
        mutated['oracle']['assertions'][0]['expected']['value'] = 43
        wrong_case = session / 'wrong-expectation.json'
        wrong_case.write_text(json.dumps(mutated, indent=2) + '\n')
        _, bad_verdict = run_case('wrong-expectation-rejected', wrong_case, 1)
        if bad_verdict['case_verdict'] != 'FAIL':
            raise RuntimeError('wrong expected value was not rejected as FAIL')
        report['source_sha256_after'] = verify_source_identity(source_before, report['source_sha256'])
        report['status'] = 'PASS'
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError, KeyError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        exit_code = 1
    write_report(session / 'validation.json', report)
    try:
        if args.report:
            write_report(args.report, report)
    except OSError as error:
        print(f'DEV1 report export failed: {error}; per-run evidence retained in {session}', flush=True)
        return 1
    print(f'DEV1 {report["status"]}: {session / "validation.json"}')
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
