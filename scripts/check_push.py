#!/usr/bin/env python3
"""Check the committed tips in Git's pre-push input, without editing the checkout."""
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def pushed_commits(repo, lines):
    """Resolve branch/annotated-tag tips; deletions require no snapshot check."""
    commits = []
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
        if commit not in commits:
            commits.append(commit)
    return commits


def check_commit(repo, commit):
    """Run the snapshot's verifier; generated reports stay in the temporary export."""
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
        result = subprocess.run([sys.executable, '-B', str(verifier)], cwd=snapshot)
        if result.returncode:
            raise ValueError(f'{commit}: document verification failed; push blocked')


def main():
    try:
        repo = Path(subprocess.check_output(
            ['git', 'rev-parse', '--show-toplevel'], text=True).strip())
        for commit in pushed_commits(repo, sys.stdin):
            check_commit(repo, commit)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'XADB pre-push: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
