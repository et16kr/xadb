#!/usr/bin/env python3
"""Run actual DEV0 checks and retain logs/bundles; never qualify an engine/release."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE_PATHS = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tests', 'scripts', '.github')


def source_fingerprint(root=ROOT):
    """Same path/length/content encoding as the controller build script."""
    files = []
    for name in SOURCE_PATHS:
        path = root / name
        files.extend(path.rglob('*') if path.is_dir() else [path])
    value = hashlib.sha256()
    for path in sorted(p for p in files if p.is_file()):
        relative = path.relative_to(root).as_posix().encode()
        data = path.read_bytes()
        value.update(len(relative).to_bytes(8, 'little'))
        value.update(relative)
        value.update(len(data).to_bytes(8, 'little'))
        value.update(data)
    return value.hexdigest()


def verify_source_identity(expected, compiled, root=ROOT):
    current = source_fingerprint(root)
    if current != expected or compiled != expected:
        raise RuntimeError('source identity changed during verification or compiled source is stale')
    return current


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def check_report_paths(paths):
    """Reject occupied or aliased destinations before spending time on checks."""
    resolved = set()
    for path in paths:
        if path is None:
            continue
        if path.exists() or path.is_symlink():
            raise FileExistsError(f'report already exists; choose a fresh run ID: {path}')
        if not path.parent.is_dir():
            raise FileNotFoundError(f'report parent directory does not exist: {path.parent}')
        destination = path.resolve()
        if destination in resolved:
            raise ValueError(f'report destinations must be distinct: {path}')
        resolved.add(destination)


def write_report(path, report):
    """Create one immutable run report; exclusive open also closes the preflight race.

    An interrupted write is incomplete evidence, never a reusable destination.
    """
    contents = json.dumps(report, ensure_ascii=False, indent=2) + '\n'
    with path.open('x', encoding='utf-8') as stream:
        stream.write(contents)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, help='optional new validation JSON; existing paths are rejected')
    args = parser.parse_args()
    try:
        check_report_paths([args.report])
    except (OSError, ValueError) as error:
        parser.error(str(error))
    artifacts = ROOT / 'artifacts' / 'dev0'
    artifacts.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='check-', dir=artifacts))
    environment = os.environ.copy()
    environment['XADBTEST_SELF_TEST_ARTIFACTS'] = str(session / 'self-tests')
    # Pin Cargo's output location; callers may otherwise use a different CARGO_TARGET_DIR.
    environment['CARGO_TARGET_DIR'] = str(ROOT / 'target')
    checks = []
    report = {
        'schema_version': 1,
        'date': datetime.now(timezone.utc).isoformat(),
        'scope': 'DEV0 local harness qualification only; no engine or protected release qualification',
        'status': 'RUNNING',
        'artifact_directory': str(session.relative_to(ROOT)),
        'checks': checks,
        'engine_tests': 'VOLATILE_SUBSTRATE_ONLY_NO_DURABILITY',
        'official_release': False,
        'protected_ci_execution': 'NOT_RUN',
    }

    def command(name, argv, expected=0):
        print(f'{name}: {" ".join(argv)}', flush=True)
        result = subprocess.run(argv, cwd=ROOT, env=environment, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=600)
        log = session / (name + '.log')
        log.write_bytes(result.stdout)
        checks.append({'id': name, 'command': argv, 'exit_code': result.returncode,
                       'expected_exit_code': expected, 'status': 'PASS' if result.returncode == expected else 'FAIL',
                       'log': str(log.relative_to(ROOT)), 'log_sha256': digest(log)})
        print(result.stdout.decode(errors='replace'), end='', flush=True)
        if result.returncode != expected:
            raise RuntimeError(f'{name}: expected {expected}, got {result.returncode}')
        return result.stdout

    exit_code = 0
    try:
        source_before = source_fingerprint()
        report['source_sha256_before'] = source_before
        command('evidence-self-tests', ['python3', '-B', 'scripts/test_check_evidence.py'])
        command('format', ['cargo', 'fmt', '--all', '--check'])
        command('lint', ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings'])
        command('self-tests', ['cargo', 'test', '--workspace', '--all-features', '--locked'])
        command('default-build', ['cargo', 'build', '--workspace', '--locked'])
        controller = ROOT / 'target/debug/xadbtest'
        worker = ROOT / 'target/debug/xadbtest-fixture-worker'
        case = ROOT / 'tests/cases/dev0-smoke.json'
        binary_dir = session / 'binaries'
        binary_dir.mkdir()
        for binary in (controller, worker):
            shutil.copy2(binary, binary_dir / binary.name)
        controller = binary_dir / controller.name
        worker = binary_dir / worker.name
        retained_case = session / 'case.json'
        shutil.copy2(case, retained_case)
        case = retained_case
        base = [str(controller), 'run', '--case', str(case), '--case-sha256', digest(case),
                '--worker', str(worker), '--worker-sha256', digest(worker), '--artifacts', str(session / 'smoke')]
        output = command('real-fixture-smoke', base)
        run_directory = Path(json.loads(output)['run_directory'])
        manifest = json.loads((run_directory / 'manifest.json').read_text())
        verdict = json.loads((run_directory / 'verdict.json').read_text())
        if verdict['case_verdict'] != 'PASS' or verdict['assertions_checked'] != 7:
            raise RuntimeError('smoke verdict/assertion count mismatch')
        report['source_sha256'] = manifest['source_sha256']
        verify_source_identity(source_before, report['source_sha256'])
        report['toolchain'] = manifest['toolchain']
        report['target'] = manifest['target']
        report['os_release'] = manifest['os_release']
        report['smoke'] = {'run_directory': str(run_directory.relative_to(ROOT)),
                           'controller_sha256': manifest['controller_sha256'],
                           'worker_sha256': manifest['worker_sha256'],
                           'case_sha256': manifest['case_sha256'], 'verdict': verdict}
        # Execute precisely these retained binaries and the case in an empty working directory.
        # No source checkout, Cargo, legacy framework, or engine is consulted by the runner.
        with tempfile.TemporaryDirectory(prefix='xadbtest-clean-runtime-') as temporary:
            clean = Path(temporary)
            for binary in binary_dir.iterdir():
                shutil.copy2(binary, clean / binary.name)
            shutil.copy2(case, clean / 'case.json')
            argv = [str(clean / 'xadbtest'), 'run', '--case', str(clean / 'case.json'),
                    '--case-sha256', digest(case), '--worker', str(clean / worker.name),
                    '--worker-sha256', digest(worker), '--artifacts', str(session / 'clean-runtime')]
            # Absolute files plus worker env_clear guarantee independence from cwd/env.
            result = subprocess.run(argv, cwd=clean, env={'PATH': '/usr/bin:/bin'},
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
            log = session / 'clean-runtime.log'
            log.write_bytes(result.stdout)
            checks.append({'id': 'clean-runtime', 'command': ['retained binaries + case in empty cwd'],
                           'exit_code': result.returncode, 'expected_exit_code': 0,
                           'status': 'PASS' if result.returncode == 0 else 'FAIL',
                           'log': str(log.relative_to(ROOT)), 'log_sha256': digest(log)})
            if result.returncode != 0:
                raise RuntimeError('clean runtime failed')
        # A source-only export has no .git, prior build products, or historical trees.
        with tempfile.TemporaryDirectory(prefix='xadbtest-fresh-build-') as temporary:
            fresh = Path(temporary)
            for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tests', 'scripts', '.github']:
                source = ROOT / name
                if source.is_dir():
                    shutil.copytree(source, fresh / name)
                else:
                    shutil.copy2(source, fresh / name)
            argv = ['cargo', 'build', '--workspace', '--locked', '--offline', '--target-dir', str(fresh / 'target')]
            print('fresh-source-build: clean source export, locked dependencies', flush=True)
            result = subprocess.run(argv, cwd=fresh, env=environment, stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT, timeout=600)
            log = session / 'fresh-source-build.log'
            log.write_bytes(result.stdout)
            checks.append({'id': 'fresh-source-build', 'command': argv, 'exit_code': result.returncode,
                           'expected_exit_code': 0, 'status': 'PASS' if result.returncode == 0 else 'FAIL',
                           'log': str(log.relative_to(ROOT)), 'log_sha256': digest(log)})
            if result.returncode != 0:
                raise RuntimeError('fresh source build failed')
        # Final verification checks every retained artifact against the controller's index.
        bundles = sorted(session.glob('*/run-*/artifacts.json'))
        outcomes = {}
        for index in bundles:
            for name, expected in json.loads(index.read_text()).items():
                if digest(index.parent / name) != expected:
                    raise RuntimeError(f'artifact mismatch: {index.parent / name}')
            result = json.loads((index.parent / 'verdict.json').read_text())
            key = result['case_verdict']
            outcomes[key] = outcomes.get(key, 0) + 1
        report['retained_bundles'] = len(bundles)
        report['intentional_self_test_and_smoke_outcomes'] = outcomes
        report['source_sha256_after'] = verify_source_identity(source_before, report['source_sha256'])
        report['status'] = 'PASS'
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        exit_code = 1
    write_report(session / 'validation.json', report)
    try:
        if args.report:
            write_report(args.report, report)
    except OSError as error:
        print(f'DEV0 report export failed: {error}; per-run evidence retained in {session}', flush=True)
        return 1
    print(f'DEV0 {report["status"]}: {session / "validation.json"}')
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
