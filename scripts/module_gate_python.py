#!/usr/bin/env python3
"""Unprivileged, exact unittest execution for F1; never imported by control.

The worker is a fresh isolated interpreter so candidate imports cannot reuse a
trusted planner's sys.modules. Receipts detect mistakes, not malicious test code.
"""
import argparse
import inspect
import json
import os
import signal
import subprocess
import sys
import unittest
import warnings
from pathlib import Path


class Result(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.rows = []

    def startTest(self, test):
        super().startTest(test)
        self.rows.append({'id': test.id(), 'status': 'INCOMPLETE'})

    def addSuccess(self, test):
        super().addSuccess(test)
        self.rows[-1]['status'] = 'PASS'


def worker(source, names, output):
    sys.path.insert(0, str(source / 'scripts'))
    suite = unittest.defaultTestLoader.loadTestsFromNames(names)

    def synchronous(tests):
        for test in tests:
            if isinstance(test, unittest.TestSuite):
                if not synchronous(test):
                    return False
            else:
                method = getattr(test, test._testMethodName)
                if (inspect.iscoroutinefunction(method) or inspect.isgeneratorfunction(method)
                        or inspect.isasyncgenfunction(method)):
                    return False
        return True

    if not synchronous(suite):
        output.write_text(json.dumps({'status': 'FAIL', 'tests': [],
                                     'error': 'F1 requires synchronous unittest methods'}))
        return 1
    # A decorated ordinary method can still return a coroutine/generator. The
    # unittest warning about a non-None return must not become a false PASS.
    with warnings.catch_warnings():
        warnings.simplefilter('error', DeprecationWarning)
        result = unittest.TextTestRunner(verbosity=2, resultclass=Result).run(suite)
    ok = (result.wasSuccessful() and not result.skipped and not result.expectedFailures
          and not result.unexpectedSuccesses and result.testsRun == len(names)
          and sorted(r['id'] for r in result.rows) == sorted(names)
          and all(r['status'] == 'PASS' for r in result.rows) and bool(names))
    output.write_text(json.dumps({'status': 'PASS' if ok else 'FAIL', 'tests': result.rows}))
    return 0 if ok else 1


def execute(source, plan, logs):
    """Bounded subprocess; omit tokens, Python injection and Git config env."""
    names = plan['tests']
    if not names or len(set(names)) != len(names):
        return {'status': 'FAIL', 'exit_code': None, 'tests': []}
    log, output = logs / 'python-tests.log', logs / 'python-tests.json'
    output.unlink(missing_ok=True)
    keep = ('PATH', 'HOME', 'TMPDIR', 'LANG', 'LC_ALL', 'CARGO_HOME', 'RUSTUP_HOME',
            'RUSTUP_TOOLCHAIN', 'HTTP_PROXY', 'HTTPS_PROXY', 'NO_PROXY',
            'SSL_CERT_FILE', 'SSL_CERT_DIR')
    env = {k: os.environ[k] for k in keep if k in os.environ}
    command = [sys.executable, '-I', '-B', str(Path(__file__).resolve()),
               '--source', str(source), '--out', str(output), *names]
    with log.open('w') as stream:
        child = subprocess.Popen(command, cwd=source, env=env, stdout=stream,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = child.wait(timeout=plan['timeout_seconds'])
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
            return {'status': 'TIMEOUT', 'exit_code': None, 'tests': []}
    try:
        result = json.loads(output.read_text())
        if not isinstance(result, dict):
            raise ValueError('worker result is not an object')
    except (OSError, ValueError):
        result = {'status': 'ERROR', 'tests': []}
    result['exit_code'] = code
    rows = result.get('tests')
    if (code or result.get('status') != 'PASS' or not names
            or not isinstance(rows, list) or not all(isinstance(r, dict) for r in rows)
            or sorted(str(r.get('id')) for r in rows) != names
            or any(r.get('status') != 'PASS' for r in rows)):
        result['status'] = 'FAIL'
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('tests', nargs='+')
    args = parser.parse_args()
    raise SystemExit(worker(args.source.resolve(), args.tests, args.out))
