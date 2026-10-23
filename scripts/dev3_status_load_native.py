#!/usr/bin/env python3
"""Fresh P3 history, selected actual reads and independent final whole-file oracle."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
import dev3_operational_recovery_native as history
import dev3_recovery_effects_native as evidence

SELECTOR = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::status_init::status_flush::tests::native::status_load::worker_status_load_native'
require = history.require


def expected_files(root):
    files = history.producer_files(root, False)
    history.flush(files, history.literal(root, 'work3-status.after'), 3)
    history.flush(files, history.literal(root, 'work3-allocator.after'), 4)
    return files


def check_stdout(output, code):
    require(code == 0 and re.findall(r'^test ([\w:]+) \.\.\.', output, re.M) == [SELECTOR], 'FAIL_EXACT_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', output, re.M) == [('1', '0', '0', '0')], 'FAIL_TEST_CENSUS')


def check_reads(text):
    rows = [line.split('\t') for line in text.splitlines()]
    require(rows and all(len(r) == 5 and r[0] == 'read' and r[3] == r[4] for r in rows), 'FAIL_READ_ONLY')
    homes = [r[2:] for r in rows if r[1] == history.model.DATA]
    require(homes == [[str(offset), '16384', '16384'] for offset in (16384, 32768, 16384)], 'FAIL_LOAD_OFFSETS')
    # Each load selects profile, FILEMAP, root, chunk twice, and exact target.
    names = ['d3-profile', 'FILEMAP', 'd3-filemap-0', 'd3-filemap-0', 'd3-filemap-0', history.model.DATA]
    require([r[1] for r in rows] == names * 3, 'FAIL_SELECTION_READS')


def check_build_rows(rows):
    require([r.get('success') for r in rows if r.get('reason') == 'build-finished'] == [True]
            and not any(r.get('reason') == 'compiler-message' and r.get('message', {}).get('level') == 'error' for r in rows), 'FAIL_COMPILE_RECEIPT')


def check_identity(before, after):
    require(before == after, 'FAIL_SOURCE_BINARY_CHECKER_CHANGED')


def identities(root, binary, receipt):
    return dict(source=source_fingerprint(root), binary=digest(binary), receipt=digest(receipt),
                build=digest(receipt.parent/'build.jsonl'), checkers=history.checker_identities(root))


def run(root, binary, receipt, output):
    root, binary, receipt, output = (p.resolve() for p in (root, binary, receipt, output))
    require(not output.exists(), 'FAIL_OUTPUT_EXISTS')
    build = json.loads(receipt.read_text())
    source = source_fingerprint(root)
    require(root == (receipt.parent / 'source').resolve() and build['status'] == 'PASS'
            and build['exit_code'] == 0 and build['source_sha256'] == source
            and Path(build['source_root']).resolve() == root
            and build['binary_sha256'] == digest(binary), 'FAIL_BUILD_BINDING')
    require(Path(__file__).resolve() == root / 'scripts/dev3_status_load_native.py', 'FAIL_CHECKER_SOURCE')
    rows=[json.loads(line) for line in (receipt.parent/'build.jsonl').read_text().splitlines() if line.startswith('{')]
    check_build_rows(rows)
    require(build['command'] == ['cargo','test','-p','xadb-engine','--lib','--no-run','--locked','--offline','--message-format=json'], 'FAIL_BUILD_COMMAND')
    before=identities(root,binary,receipt)
    output.mkdir(parents=True)
    report = dict(status='RUNNING', source_sha256=source, binary_sha256=digest(binary),
                  receipt_sha256=digest(receipt), checker_sha256=history.checker_identities(root),
                  selector=SELECTOR, timeout_seconds=30, database_limit_bytes=24 << 20,
                  inventory=['native_A_B_A_dirty_allocator'])
    began = time.monotonic()
    try:
        env = {k:v for k,v in os.environ.items() if not k.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_'))}
        env['XADB_STATUS_LOAD_NATIVE_DIR'] = str(output)
        command = [str(binary), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
        report['command'] = command
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True, timeout=30)
        (output/'worker.log').write_text(result.stdout + result.stderr)
        report['exit_code'] = result.returncode
        check_stdout(result.stdout, result.returncode)
        check_reads((output/'load-reads.tsv').read_text())
        report['files'] = evidence.compare_files(output/'database', expected_files(root))
        require(time.monotonic() - began < 30, 'FAIL_HISTORY_BUDGET')
        fields = dict(line.split('=',1) for line in (output/'summary.txt').read_text().splitlines())
        require(fields['actual_loads'] == '3' and int(fields['fds_peak']) <= 5
                and fields['remaining_fds'] == fields['remaining_handles'] == '0', 'FAIL_NATIVE_RESOURCES')
        report.update(status='PASS', actual_loads=3, database_files_compared=len(report['files']), resource=fields)
    except Exception as exc:
        report.update(status='FAIL', error=f'{type(exc).__name__}: {exc}')
        if isinstance(exc, subprocess.TimeoutExpired):
            (output/'worker-timeout.log').write_bytes((exc.stdout or b'')+(exc.stderr or b''))
    finally:
        try:
            report['identities_after']=identities(root,binary,receipt)
            check_identity(before,report['identities_after'])
            # Failures retain their actual database inventory too; no failed
            # partial database is omitted from the declared resource boundary.
            if (output/'database').exists():
                report['retained_files']=evidence.inventory(output/'database')
                report['retained_database_bytes']=sum(r['bytes'] for r in report['retained_files'].values())
            require(time.monotonic()-began < 30,'FAIL_HISTORY_BUDGET')
        except Exception as exc:
            report.update(status='FAIL', final_validation_error=f'{type(exc).__name__}: {exc}')
        report['identities_before']=before
        report['elapsed_seconds'] = time.monotonic() - began
        (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    require(report['status']=='PASS',report.get('error',report.get('final_validation_error','FAIL_NATIVE')))
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('source-root', 'binary', 'receipt', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.source_root,args.binary,args.receipt,args.output),indent=2))
