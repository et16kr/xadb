"""Shared capture/build plumbing; COMM correctness oracles remain in their owners."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

from check_dev0 import SOURCE_PATHS, digest, source_fingerprint


def capture_source(root, destination):
    """Retain a coherent source export while other tasks may edit the workspace."""
    root, destination = Path(root), Path(destination)
    for _ in range(3):
        before = source_fingerprint(root)
        if destination.exists():
            shutil.rmtree(destination)
        destination.mkdir(parents=True)
        for name in (*SOURCE_PATHS, 'devdoc'):
            source = root / name
            if source.is_dir():
                shutil.copytree(source, destination / name)
            elif source.is_file():
                shutil.copy2(source, destination / name)
        if source_fingerprint(destination) == before == source_fingerprint(root):
            return before
    raise RuntimeError('source changed during all capture attempts')


def identity(root):
    root = Path(root)
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    dirty = subprocess.check_output(['git', 'diff', '--binary'], cwd=root)
    return {'revision': revision, 'tracked_diff_sha256': hashlib.sha256(dirty).hexdigest(),
            'source_sha256': source_fingerprint(root), 'lockfile_sha256': digest(root / 'Cargo.lock')}


def write_json(path, value, *, replace=False):
    # Manifests/final reports are immutable receipts. Only live progress is
    # replaceable; an external --report cannot overwrite prior evidence.
    with Path(path).open('w' if replace else 'x') as output:
        output.write(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def build_test(source, suite, directory, features=(), release=False):
    """Compile first; a compiler error is never evidence of a killed mutant."""
    source, directory = Path(source), Path(directory)
    command = ['cargo', 'test', '-p', 'xadb-transport', '--test', suite,
               '--no-run', '--locked', '--offline', '--message-format=json']
    if features:
        command += ['--features', ','.join(features)]
    if release:
        command += ['--release']
    result = subprocess.run(command, cwd=source, capture_output=True, timeout=180)
    (directory / 'build.jsonl').write_bytes(result.stdout)
    (directory / 'build.stderr').write_bytes(result.stderr)
    if result.returncode:
        raise RuntimeError(f'test binary build failed: {suite}')
    artifacts = [json.loads(line) for line in result.stdout.decode().splitlines() if line.startswith('{')]
    executables = [a['executable'] for a in artifacts if a.get('reason') == 'compiler-artifact'
                   and a.get('target', {}).get('name') == suite and a.get('executable')]
    if len(executables) != 1 or not any(a.get('reason') == 'build-finished' and a.get('success') is True for a in artifacts):
        raise RuntimeError('missing unique compiled test binary')
    binary = directory / 'test-binary'
    shutil.copy2(executables[0], binary)
    return binary, {'command': command, 'binary_sha256': digest(binary),
                    'build_stdout_sha256': digest(directory / 'build.jsonl'),
                    'build_stderr_sha256': digest(directory / 'build.stderr')}
