#!/usr/bin/env python3
"""Check the committed tips in Git's pre-push input, without editing the checkout."""
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def pushed_commits(repo, lines):
    """Resolve branch/annotated-tag tips; deletions require no snapshot check."""
    return [commit for commit, _ref in pushed_updates(repo, lines)]


def pushed_updates(repo, lines):
    """(commit, remote ref) of each pushed tip, each commit once."""
    commits, seen = [], set()
    for line in lines:
        fields = line.split()
        if len(fields) != 4:
            raise ValueError('malformed pre-push input')
        local_ref, local_oid, remote_ref, remote_oid = fields
        if not all(re.fullmatch(r'(?:[0-9a-f]{40}|[0-9a-f]{64})', oid)
                   for oid in (local_oid, remote_oid)):
            raise ValueError('invalid pre-push object ID')
        if set(local_oid) == {'0'}:
            continue
        commit = subprocess.check_output(
            ['git', 'rev-parse', '--verify', local_oid + '^{commit}'],
            cwd=repo, text=True).strip()
        if (commit, remote_ref) not in seen:
            seen.add((commit, remote_ref))
            commits.append((commit, remote_ref))
    return commits


def remote_default(repo, remote):
    """The fetched default branch of the push remote, if Git knows it."""
    if not remote:
        return None
    for ref in (f'refs/remotes/{remote}/HEAD', f'refs/remotes/{remote}/main'):
        found = subprocess.run(['git', 'rev-parse', '--verify', '-q', ref + '^{commit}'],
                               cwd=repo, capture_output=True, text=True)
        if found.returncode == 0:
            return found.stdout.strip()
    return None


def check_module_gate(repo, remote, commit, remote_ref):
    """XADB-MODULE-GATE-003 before transport, with the policy of the merge-base.

    Boundary and registration findings block. Review/evidence obligations stay
    PENDING except for a push to main, which the remote ruleset also rejects.
    """
    base = remote_default(repo, remote)
    if base is None:
        print('XADB pre-push: module gate skipped (no fetched default branch of the remote)',
              flush=True)
        return
    found = subprocess.run(['git', 'merge-base', base, commit], cwd=repo,
                           capture_output=True, text=True)
    if found.returncode:
        raise ValueError(f'{commit}: no merge-base with {base}; module gate cannot judge')
    policy = found.stdout.strip()
    with tempfile.TemporaryDirectory(prefix='xadb-module-gate-') as directory:
        for name in ('check_module_gate.py', 'module_single_source.py'):
            blob = subprocess.run(['git', 'show', f'{policy}:scripts/{name}'], cwd=repo,
                                  capture_output=True)
            if blob.returncode:
                print(f'XADB pre-push: module gate not adopted at {policy}', flush=True)
                return
            (Path(directory) / name).write_bytes(blob.stdout)
        mode = 'merge' if remote_ref == 'refs/heads/main' else 'push'
        print(f'XADB pre-push: module gate {mode} {policy[:12]}..{commit[:12]}', flush=True)
        result = subprocess.run([sys.executable, '-B', str(Path(directory) / 'check_module_gate.py'),
                                 'check', '--repo', str(repo), '--base', base, '--head', commit,
                                 '--policy', policy, '--mode', mode], cwd=repo)
        if result.returncode:
            raise ValueError(f'{commit}: module gate failed; push blocked')


def check_commit(repo, commit):
    """Check Rust format, Clippy and documents in the committed export, not tests."""
    with tempfile.TemporaryDirectory(prefix='xadb-pre-push-') as directory:
        temporary = Path(directory)
        snapshot = temporary / 'snapshot'
        snapshot.mkdir()
        archive = temporary / 'snapshot.tar'
        subprocess.run(['git', 'archive', '--format=tar', '-o', str(archive), commit],
                       cwd=repo, check=True)
        subprocess.run(['tar', '-xf', str(archive), '-C', str(snapshot)], check=True)
        verifier = snapshot / 'devdoc' / 'verify_documents.py'
        if not verifier.is_file() or not verifier.resolve().is_relative_to(snapshot):
            raise ValueError(f'{commit}: missing or external document verifier')
        print(f'XADB pre-push: checking {commit}', flush=True)
        print('XADB pre-push: cargo fmt --all --check', flush=True)
        result = subprocess.run(['cargo', 'fmt', '--all', '--check'], cwd=snapshot)
        if result.returncode:
            raise ValueError(f'{commit}: Rust format verification failed; push blocked')
        # Match the Actions lint scope, including cfg(test) and nondefault features.
        # Clippy compiles/checks test targets but does not execute their test cases.
        environment = dict(os.environ, CARGO_TARGET_DIR=str(temporary / 'target'))
        clippy = ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features',
                  '--locked', '--', '-D', 'warnings']
        print('XADB pre-push: ' + ' '.join(clippy), flush=True)
        result = subprocess.run(clippy, cwd=snapshot, env=environment)
        if result.returncode:
            raise ValueError(f'{commit}: Clippy verification failed; push blocked')
        result = subprocess.run([sys.executable, '-B', str(verifier)], cwd=snapshot)
        if result.returncode:
            raise ValueError(f'{commit}: document verification failed; push blocked')


def main():
    try:
        repo = Path(subprocess.check_output(
            ['git', 'rev-parse', '--show-toplevel'], text=True).strip())
        remote = sys.argv[1] if len(sys.argv) > 1 else None
        updates = pushed_updates(repo, sys.stdin)
        checked = set()
        for commit, remote_ref in updates:
            check_module_gate(repo, remote, commit, remote_ref)
        for commit, _remote_ref in updates:
            if commit not in checked:
                checked.add(commit)
                check_commit(repo, commit)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'XADB pre-push: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
