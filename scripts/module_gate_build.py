#!/usr/bin/env python3
"""Unprivileged CI build/test runner of the module gate (XADB-MODULE-GATE-003).

The build workflow copies this file, `check_module_gate.py` and
`module_single_source.py` from the default branch and runs them on the pull
request head. The plan comes from that trusted checker; the head's own copies
of these files are never executed. The receipt is data for the control
workflow's final judgement (`check_module_gate.py final`), which recomputes
the plan and rejects any mismatch, missing, skipped or partial result.
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import check_module_gate as gate  # noqa: E402  (the trusted copy next to this file)
import module_single_source  # noqa: E402

RESULT = re.compile(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; '
                    r'(\d+) measured; (\d+) filtered out')


def run(command, cwd, log, env):
    started = time.monotonic()
    with open(log, 'ab') as stream:
        stream.write(('$ ' + ' '.join(command) + '\n').encode())
        stream.flush()
        result = subprocess.run(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT)
        stream.write(result.stdout)
    return result.returncode, result.stdout.decode(errors='replace'), time.monotonic() - started


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def selector_result(name, source, plan, selector, logs, env, optional=False):
    """Run one selector; every listed case passes or is `#[ignore]`d in source.
    A required selector must list at least one case; an optional one (its
    module's cfg is not known on the runner) may list none."""
    log = logs / (re.sub(r'[^\w.-]+', '_', name) + '.log')
    filters = [selector] + [x for s in plan['skip'] for x in ('--skip', s)]
    code, out, _ = run(plan['test_command'] + ['--', *filters, '--list'], source, log, env)
    listed = sum(1 for line in out.splitlines() if line.endswith(': test'))
    if code:
        return dict(name=name, status='ERROR', listed=listed, log=log.name,
                    log_sha256=digest(log))
    code, out, seconds = run(plan['test_command'] + ['--', *filters, *plan['test_args']],
                             source, log, env)
    totals = [tuple(map(int, m.groups()[1:])) for m in RESULT.finditer(out)]
    passed = sum(t[0] for t in totals)
    failed = sum(t[1] for t in totals)
    ignored = sum(t[2] for t in totals)
    ok = (code == 0 and (optional or passed >= 1) and failed == 0
          and passed + ignored == listed)
    return dict(name=name, status='PASS' if ok else 'FAIL', listed=listed, passed=passed,
                failed=failed, ignored=ignored, seconds=round(seconds, 1), log=log.name,
                log_sha256=digest(log))


def cases_result(name, source, plan, cases, logs, env, chunk=100, optional=False):
    """Run the exact planned cases; every one must be listed once and pass.
    Optional cases (cfg not known on the runner) pass when every listed one
    passes, whether or not it is listed."""
    log = logs / (re.sub(r'[^\w.-]+', '_', name) + '.log')
    listed = passed = failed = ignored = 0
    seconds, ok = 0.0, True
    for start in range(0, len(cases), chunk):
        names = cases[start:start + chunk]
        code, out, _ = run(plan['test_command'] + ['--', '--exact', *names, '--list'],
                           source, log, env)
        listed += sum(1 for line in out.splitlines() if line.endswith(': test'))
        ok = ok and code == 0
        code, out, spent = run(plan['test_command'] + ['--', '--exact', *names,
                                                        *plan['test_args']], source, log, env)
        totals = [tuple(map(int, m.groups()[1:])) for m in RESULT.finditer(out)]
        passed += sum(t[0] for t in totals)
        failed += sum(t[1] for t in totals)
        ignored += sum(t[2] for t in totals)
        seconds += spent
        ok = ok and code == 0
    ok = (ok and (listed <= len(cases) if optional else listed == len(cases))
          and failed == 0 and passed == listed)
    return dict(name=name, status='PASS' if ok else 'FAIL', planned=len(cases), listed=listed,
                passed=passed, failed=failed, ignored=ignored, seconds=round(seconds, 1),
                log=log.name, log_sha256=digest(log))


def optional_results(prefix, source, plan, part, logs, env):
    """The selectors and cases whose cfg is not known on the runner."""
    results = [selector_result(f'{prefix}?:{selector}', source, plan, selector, logs, env,
                               optional=True)
               for selector in part.get('optional_selectors', [])]
    if part.get('optional_cases'):
        results.append(cases_result(f'{prefix}-optional-cases', source, plan,
                                    part['optional_cases'], logs, env, optional=True))
    return results


def extract(tree, directory):
    """Write the head's files from Git objects (not `git archive`, whose
    output a `.gitattributes` in the change could alter)."""
    files = sorted(tree.files())
    for start in range(0, len(files), 2000):
        tree.preload(files[start:start + 2000])
        for path in files[start:start + 2000]:
            target = directory / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(tree.read(path))
            tree.cache.pop(path, None)


def build_result(name, source, command, logs, env):
    log = logs / f'{name}.log'
    code, _out, seconds = run(command, source, log, env)
    return dict(name=name, status='PASS' if code == 0 else 'FAIL', seconds=round(seconds, 1),
                log=log.name, log_sha256=digest(log))


def identity(plan, source, env, tree_sha256):
    """The identity environment the catalog_memory cases require: the source
    tree, the lib test executable and the plan they run (S-series runners)."""
    out = subprocess.run(plan['test_command'] + ['--no-run', '--message-format=json'],
                         cwd=source, env=env, capture_output=True, text=True)
    executables = [json.loads(line).get('executable') for line in out.stdout.splitlines()
                   if line.startswith('{')]
    executables = [e for e in executables if e]
    values = {'source': tree_sha256,
              'elf': digest(executables[-1]) if executables else '0' * 64,
              'input': hashlib.sha256(json.dumps(plan, sort_keys=True).encode()).hexdigest()}
    result = dict(env)
    for key, names in plan.get('identity_env', {}).items():
        for name in names:
            result[name] = values[key]
    return result, values


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo', default='.')
    parser.add_argument('--base', required=True)
    parser.add_argument('--head', required=True)
    parser.add_argument('--policy', required=True)
    parser.add_argument('--workflow', required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    repo = str(Path(args.repo).resolve())
    args.out.mkdir(parents=True, exist_ok=True)
    logs = args.out
    report = gate.judge(repo, args.base, args.head, args.policy, mode='push', exports=False)
    plan = report['selection']
    receipt = {'schema': gate.RECEIPT_SCHEMA, 'workflow': args.workflow,
               'head': report['head'], 'base': report['base'], 'policy': report['policy'],
               'manifest_blob': report.get('manifest_blob'),
               'checker_blob': report.get('checker_blob'),
               'checker_sha256': digest(HERE / 'check_module_gate.py'),
               'run': {k: os.environ.get(k) for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT',
                                                      'GITHUB_SHA', 'RUNNER_OS')},
               'selection': plan, 'results': []}
    results = receipt['results']
    env = dict(os.environ, **plan.get('env', {})) if plan['rust'] else dict(os.environ)
    head_tree = gate.Tree(repo, report['head'])
    with tempfile.TemporaryDirectory(prefix='xadb-module-build-') as scratch:
        env['CARGO_TARGET_DIR'] = str(Path(scratch) / 'target')
        if plan['rust']:
            head_dir = Path(scratch) / 'head'
            extract(head_tree, head_dir)
            results.append(build_result('head-build', head_dir, plan['head_command'], logs, env))
            run_env, receipt['identity_head'] = identity(
                plan, head_dir, env, hashlib.sha256(''.join(
                    f'{p}\0{head_tree.blob(p)}\n' for p in sorted(head_tree.files())
                ).encode()).hexdigest())
            for selector in plan['selectors']:
                results.append(selector_result(f'head:{selector}', head_dir, plan, selector,
                                               logs, run_env))
            if plan.get('cases'):
                results.append(cases_result('head-cases', head_dir, plan, plan['cases'],
                                            logs, run_env))
            results += optional_results('head', head_dir, plan, plan, logs, run_env)
            shutil.rmtree(head_dir)
            for export in plan['exports']:
                side = export['side']
                out = Path(scratch) / f'export-{side}'
                try:
                    inventory, _ = module_single_source.export(report['head'], out, side,
                                                               repo=repo)
                    receipt[f'export_{side}'] = inventory['source_manifest_sha256']
                    results.append(build_result(f'export-{side}-build', out, export['command'],
                                                logs, env))
                    run_env, receipt[f'identity_{side}'] = identity(
                        plan, out, env, inventory['source_manifest_sha256'])
                    for selector in export['selectors']:
                        results.append(selector_result(f'export-{side}:{selector}', out, plan,
                                                       selector, logs, run_env))
                    if export.get('cases'):
                        results.append(cases_result(f'export-{side}-cases', out, plan,
                                                    export['cases'], logs, run_env))
                    results += optional_results(f'export-{side}', out, plan, export, logs,
                                                run_env)
                except SystemExit as error:
                    results.append(dict(name=f'export-{side}-build', status='ERROR',
                                        error=str(error)))
                shutil.rmtree(out, ignore_errors=True)
    (args.out / 'receipt.json').write_text(json.dumps(receipt, indent=1) + '\n')
    failed = [r['name'] for r in results if r.get('status') != 'PASS']
    print(json.dumps({'results': len(results), 'failed': failed}, indent=1))
    return 1 if failed else 0


if __name__ == '__main__':
    raise SystemExit(main())
