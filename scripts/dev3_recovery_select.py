#!/usr/bin/env python3
"""Select exact new recovery-selection assertions; preserve failures and reach evidence."""
import argparse
from collections import OrderedDict
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_allocation_stack import wait_process_group
from dev3_status_home_flush import BUILD_COMMAND, check_build_receipt

ROOT = Path(__file__).resolve().parent.parent
INPUT = Path('crates/engine/tests/fixtures/dev3-recovery-select/runtime/manifest.json')
INPUT_SHA256 = '6c9202da43a85b1ef68e7a33ed8f45cd6049ff0bfa1c78710718fa63aea3667f'
BEGIN, CASE, COMPLETE = 'RECOVERY_SELECT_CASE_BEGIN\t', 'RECOVERY_SELECT_CASE\t', 'RECOVERY_SELECT_COMPLETE count='


def require(ok, reason):
    if not ok: raise ValueError(reason)


def runtime_input(root):
    require(digest(root/INPUT) == INPUT_SHA256, 'FAIL_RUNTIME_MANIFEST_IDENTITY')
    spec = json.loads((root/INPUT).read_text())
    require(spec['unit'] == 'DEV3-3.12.1.a-read-only-initial-select' and spec['cases'] and spec['groups']['mandatory'] and spec['groups']['s5'], 'FAIL_RUNTIME_MANIFEST_SCHEMA')
    require(len(spec['groups']['mandatory']) == len(set(spec['groups']['mandatory'])) and len(spec['groups']['s5']) == len(set(spec['groups']['s5'])), 'FAIL_RUNTIME_GROUP_CENSUS')
    return spec


def selections(requested, spec):
    result = []
    for key in requested:
        names = spec['groups'].get(key, [key])
        require(names and all(name in spec['cases'] and name not in result for name in names), 'FAIL_RUNTIME_SELECTION')
        result.extend(names)
    require(result, 'FAIL_EMPTY_RUNTIME_SELECTION')
    require(not any(alias in result and target in result for alias, target in spec.get('aliases', {}).items()), 'FAIL_DUPLICATE_RUNTIME_ALIAS')
    return [name for name in spec['cases'] if name in result]


def jobs(selected, spec):
    result = OrderedDict()
    for name in selected:
        result.setdefault(spec['cases'][name]['selector'], []).append(name)
    return list(result.values())


def protocol(output, selected, selector, complete_required=True):
    prefix = 'test '+selector+' ...'
    lines = [line[len(prefix):].lstrip() if line.startswith(prefix) else line for line in output.splitlines()]
    active, passed, reached, completions = None, [], [], []
    for line in lines:
        if line.startswith(BEGIN):
            name = line[len(BEGIN):]
            require(active is None and not completions and len(passed) < len(selected) and name == selected[len(passed)], 'FAIL_CASE_BEGIN_ORDER')
            active = name; reached.append(name)
        elif line.startswith(CASE):
            parts = line[len(CASE):].split('\t')
            require(len(parts) == 2 and parts == [active, 'PASS'], 'FAIL_CASE_COMPLETION_ORDER')
            passed.append(active); active = None
        elif line.startswith(COMPLETE):
            require(active is None and passed == selected, 'FAIL_COMPLETION_BEFORE_CASES')
            completions.append(line[len(COMPLETE):])
        elif line.startswith('RECOVERY_SELECT_') and not line.startswith(('RECOVERY_SELECT_SOURCE_BUDGET ', 'RECOVERY_SELECT_OPERATIONS ')):
            raise ValueError('FAIL_UNKNOWN_RUNTIME_PROTOCOL')
    if complete_required:
        require(active is None and passed == selected and completions == [str(len(selected))], 'FAIL_FINAL_CASE_CENSUS')
        require(output.count('test result:') == 1, 'FAIL_SUMMARY_CENSUS')
        require(re.findall(r'^test ([\w:]+) \.\.\.', output, re.M) == [selector], 'FAIL_EXACT_RUNTIME_SELECTOR')
        require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', output, re.M) == [('1','0','0','0')], 'FAIL_EXACT_RUNTIME_CENSUS')
    return dict(reached_case_begin=reached, completed_case_assertions=passed, active_case=active)


def remaining(chosen, consumed, spec):
    classes = {'s5' if spec['cases'][name].get('ignored', False) else 'mandatory' for name in chosen}
    require(len(classes) == 1, 'FAIL_MIXED_RUNTIME_BUDGET')
    group = classes.pop(); left = spec['budget_seconds'][group]-consumed.get(group, 0)
    require(left > 0, 'FAIL_RUNTIME_AGGREGATE_TIMEOUT')
    per_job = {spec['cases'][name]['budget'] for name in chosen}
    require(len(per_job) == 1, 'FAIL_MIXED_RUNTIME_JOB_BUDGET')
    return group, min(left, per_job.pop())


def checker_identities(root):
    paths = {Path(m.__file__).resolve() for m in list(sys.modules.values()) if getattr(m, '__file__', None) and Path(m.__file__).suffix == '.py' and Path(m.__file__).resolve().parent == Path(__file__).resolve().parent}
    paths.add(Path(__file__).resolve()); paths.add(Path(__file__).with_name('test_dev3_recovery_select.py'))
    result = {str(p.relative_to(ROOT)): digest(p) for p in paths}
    require(all(digest(root/p) == sha for p, sha in result.items()), 'FAIL_RUNTIME_CHECKER_IDENTITY')
    return result


def run(root, binary, receipt, output, requested):
    root, binary, receipt, output = (p.resolve() for p in (root, binary, receipt, output))
    spec = runtime_input(root); selected = selections(requested, spec)
    source, binary_hash, receipt_hash = source_fingerprint(root), digest(binary), digest(receipt)
    check_build_receipt(json.loads(receipt.read_text()), root, source, binary_hash)
    with binary.open('rb') as stream: require(stream.read(4) == b'\x7fELF', 'FAIL_BINARY_FORMAT')
    checkers = checker_identities(root)
    output.mkdir(parents=True, exist_ok=False)
    retained = output/'runtime-binary'; shutil.copyfile(binary, retained); retained.chmod(0o500)
    report = dict(status='RUNNING', unit='D3-RECOVERY-SELECT-001', selected=selected, source_root=str(root), source_sha256=source, binary_sha256=binary_hash, build_receipt_sha256=receipt_hash, input_sha256=INPUT_SHA256, checker_sha256=checkers, results=[], elapsed_seconds={})
    started = time.monotonic()
    try:
        for chosen in jobs(selected, spec):
            group, timeout = remaining(chosen, report['elapsed_seconds'], spec)
            selector = spec['cases'][chosen[0]]['selector']
            folder = output/chosen[0]; folder.mkdir()
            row = dict(selected=chosen, selector=selector, status='RUNNING', timeout_seconds=timeout, directory=str(folder), timed_out=False)
            report['results'].append(row)
            command = [str(retained), '--exact', selector, '--nocapture', '--test-threads=1']
            ignored = {spec['cases'][name].get('ignored', False) for name in chosen}
            require(len(ignored) == 1, 'FAIL_MIXED_IGNORE_SELECTOR')
            if ignored.pop(): command.append('--ignored')
            environment = {k:v for k,v in os.environ.items() if not k.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_'))}
            environment['XADB_RECOVERY_SELECT_CASES'] = ','.join(chosen)
            row.update(command=command, environment=dict(XADB_RECOVERY_SELECT_CASES=environment['XADB_RECOVERY_SELECT_CASES']))
            began = time.monotonic()
            with (folder/'stdout.log').open('wb') as stdout, (folder/'stderr.log').open('wb') as stderr:
                with subprocess.Popen(command, cwd=root, env=environment, stdout=stdout, stderr=stderr, start_new_session=True) as child:
                    row['pid'] = child.pid; wait_process_group(child, row, timeout)
            elapsed = time.monotonic()-began
            row['elapsed_seconds'] = elapsed
            report['elapsed_seconds'][group] = report['elapsed_seconds'].get(group,0)+elapsed
            text = (folder/'stdout.log').read_text()
            row.update(protocol(text, chosen, selector, False))
            require(row['exit_code'] == 0 and not row['timed_out'], 'FAIL_RUNTIME_WORKER_OR_TIMEOUT')
            require(report['elapsed_seconds'][group] <= spec['budget_seconds'][group], 'FAIL_RUNTIME_AGGREGATE_TIMEOUT')
            row.update(protocol(text, chosen, selector)); row['status'] = 'PASS'
            row.update(stdout_sha256=digest(folder/'stdout.log'), stderr_sha256=digest(folder/'stderr.log'))
        require(source_fingerprint(root) == source and digest(binary) == digest(retained) == binary_hash and digest(receipt) == receipt_hash and runtime_input(root) == spec and checker_identities(root) == checkers, 'FAIL_RUNTIME_POST_IDENTITY')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', error=str(error))
        if report['results'] and report['results'][-1]['status'] == 'RUNNING':
            row = report['results'][-1]; row['status'] = 'FAIL'
            row.setdefault('elapsed_seconds',time.monotonic()-began)
            for stream in ('stdout','stderr'):
                path = Path(row['directory'])/(stream+'.log')
                if path.exists(): row[stream+'_sha256'] = digest(path)
            path = Path(row['directory'])/'stdout.log'
            if path.exists():
                try: row.update(protocol(path.read_text(),row['selected'],row['selector'],False))
                except ValueError as failure: row['partial_protocol_error'] = str(failure)
    finally:
        reached = {name for row in report['results'] for name in row.get('reached_case_begin',[])}
        report['not_run'] = [name for name in selected if name not in reached]
        report['completed_selected'] = [name for row in report['results'] if row['status'] == 'PASS' for name in row['selected']]
        report['total_elapsed_seconds'] = time.monotonic()-started
        (output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=ROOT)
    for name in ('binary','build-receipt','output'): parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--select',action='append',required=True)
    args = parser.parse_args(); report = run(args.root,args.binary,args.build_receipt,args.output,args.select)
    print(json.dumps(dict(status=report['status'],completed=len(report['completed_selected']),error=report.get('error'))))
    return int(report['status'] != 'PASS')


if __name__ == '__main__': raise SystemExit(main())
