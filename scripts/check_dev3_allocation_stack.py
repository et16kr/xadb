#!/usr/bin/env python3
"""Observe one new allocation stack test through existing pthread getter calls.

GDB only reads registers and returned output memory. It never calls an inferior
function or evaluates code in the inferior. This is intrusive correctness and
resource diagnostics, never a performance measurement. The safe Rust test logs
its Linux TID and marker; /proc/maps length is retained but is not stack size.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import time

from check_dev0 import digest, source_fingerprint

TEST = ('storage::wal_io::bootstrap::doublewrite_install::allocation_reservation::'
        'resource_tests::allocation_component_runs_with_observed_64k_stack')
LIMIT = 65536
TIMEOUT = 30
PREFIX = 'XADB_STACK_OBSERVER '
GDB_SCRIPT = r'''set pagination off
set confirm off
set breakpoint pending on
set disable-randomization off
set debuginfod enabled off
set auto-load off
python
import gdb, json
PREFIX = 'XADB_STACK_OBSERVER '
def emit(record):
    print(PREFIX + json.dumps(record, sort_keys=True), flush=True)
def error(message):
    emit({'kind': 'observer_error', 'message': str(message)})
class Returned(gdb.FinishBreakpoint):
    def __init__(self, name, pointers):
        super().__init__(gdb.newest_frame(), internal=True)
        self.name, self.pointers = name, pointers
        self.tid = gdb.selected_thread().ptid[1]
    def stop(self):
        try:
            inferior = gdb.selected_inferior()
            values = [int.from_bytes(inferior.read_memory(p, 8).tobytes(), 'little') for p in self.pointers]
            emit({'kind': self.name, 'tid': self.tid, 'rc': int(self.return_value), 'values': values})
        except Exception as exception:
            error(exception)
        return False
    def out_of_scope(self):
        error('getter did not return: ' + self.name)
class Getter(gdb.Breakpoint):
    def __init__(self, name, registers):
        super().__init__(name, internal=True)
        self.name, self.registers = name, registers
    def stop(self):
        try:
            pointers = [int(gdb.parse_and_eval(register)) for register in self.registers]
            Returned(self.name, pointers)
        except Exception as exception:
            error(exception)
        return False
def exited(event):
    emit({'kind': 'exit', 'code': getattr(event, 'exit_code', None),
          'binary': gdb.current_progspace().filename})
def stopped(event):
    if not isinstance(event, gdb.SignalEvent):
        return
    try:
        frames = []
        frame = gdb.newest_frame()
        while frame is not None and len(frames) < 128:
            location = frame.find_sal()
            frames.append({'index': len(frames), 'name': frame.name(), 'pc': frame.pc(),
                           'sp': int(frame.read_register('sp')),
                           'file': location.symtab.fullname() if location.symtab else None,
                           'line': location.line})
            frame = frame.older()
        emit({'kind': 'signal', 'name': event.stop_signal,
              'tid': gdb.selected_thread().ptid[1], 'frames': frames,
              'frame_limit_reached': frame is not None})
    except Exception as exception:
        error('signal backtrace: ' + str(exception))
Getter('pthread_attr_getstack', ['$rsi', '$rdx'])
Getter('pthread_attr_getguardsize', ['$rsi'])
gdb.events.exited.connect(exited)
gdb.events.stop.connect(stopped)
end
run
'''


def require(condition, code):
    if not condition:
        raise ValueError(code)


def check_identity(root, source, binary, expected_binary):
    require(source_fingerprint(root) == source, 'FAIL_SOURCE_IDENTITY')
    require(digest(binary) == expected_binary, 'FAIL_BINARY_IDENTITY')
    header = binary.read_bytes()[:20]
    require(header[:6] == b'\x7fELF\x02\x01' and header[18:20] == b'\x3e\x00',
            'FAIL_OBSERVER_ABI')


def evaluate(stdout, process, binary, source):
    require(not process['timed_out'] and 0 <= process['seconds'] <= TIMEOUT, 'FAIL_TIMEOUT')
    require(process['exit_code'] == 0, 'FAIL_GDB_EXIT')
    records = []
    for line in stdout.splitlines():
        if PREFIX in line:
            require(line.startswith(PREFIX), 'FAIL_OBSERVER_RECORD')
            records.append(json.loads(line[len(PREFIX):]))
    require(all(row.get('kind') in ('pthread_attr_getstack', 'pthread_attr_getguardsize', 'exit', 'signal')
                for row in records), 'FAIL_OBSERVER_ERROR')
    require(not any(row['kind'] == 'signal' for row in records), 'FAIL_INFERIOR_SIGNAL')
    exits = [row for row in records if row['kind'] == 'exit']
    require(len(exits) == 1 and exits[0]['code'] == 0, 'FAIL_INFERIOR_EXIT')
    require(exits[0]['binary'] == str(binary), 'FAIL_BINARY_IDENTITY')
    require(stdout.count('running 1 test\n') == 1 and stdout.count(TEST) == 1, 'FAIL_TEST_SELECTION')
    summaries = re.findall(r'test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out;', stdout)
    require(len(summaries) == 1 and stdout.count('test result:') == 1, 'FAIL_TEST_RESULT')
    lines = [line[line.index('RESOURCE_STACK_THREAD '):] for line in stdout.splitlines()
             if 'RESOURCE_STACK_THREAD ' in line]
    require(len(lines) == 1, 'FAIL_MARKER_CENSUS')
    pairs = [part.split('=', 1) for part in lines[0].split()[1:]]
    marker = dict(pairs)
    require(len(pairs) == len(marker) and set(marker) == {'tid', 'name', 'marker', 'mapped_bytes', 'compiled_source'},
            'FAIL_MARKER_FIELDS')
    require(marker['name'] == 'allocation-64k' and marker['compiled_source'] == source, 'FAIL_COMPILED_SOURCE')
    tid, address, mapped = (int(marker[key]) for key in ('tid', 'marker', 'mapped_bytes'))
    require(tid > 0 and address > 0 and mapped > 0, 'FAIL_MARKER_FIELDS')
    targeted = [row for row in records if row.get('tid') == tid]
    stack = [row for row in targeted if row['kind'] == 'pthread_attr_getstack']
    guard = [row for row in targeted if row['kind'] == 'pthread_attr_getguardsize']
    require(len(stack) == len(guard) == 1 and len(targeted) == 2, 'FAIL_ATTRIBUTE_CENSUS')
    require(stack[0]['rc'] == guard[0]['rc'] == 0, 'FAIL_ATTRIBUTE_ERROR')
    require(len(stack[0]['values']) == 2 and len(guard[0]['values']) == 1, 'FAIL_ATTRIBUTE_FIELDS')
    base, size = stack[0]['values']
    guard_bytes = guard[0]['values'][0]
    require(isinstance(base, int) and isinstance(size, int) and isinstance(guard_bytes, int), 'FAIL_ATTRIBUTE_FIELDS')
    require(base > 0 and 0 < size <= LIMIT and guard_bytes >= 4096, 'FAIL_STACK_BUDGET')
    require(base <= address < base + size, 'FAIL_MARKER_INTERVAL')
    return {'status': 'PASS', 'test': TEST, 'thread_id': tid, 'thread_name': marker['name'],
            'pthread_stack_base': base, 'pthread_stack_bytes': size, 'pthread_guard_bytes': guard_bytes,
            'marker_address': address, 'maps_vma_bytes': mapped,
            'maps_vma_is_stack_size': False, 'compiled_source_sha256': marker['compiled_source'],
            'observations': records}


def wait_process_group(child, process, timeout=TIMEOUT):
    """Always terminate and reap the owned debugger when waiting is interrupted."""
    try:
        process['exit_code'] = child.wait(timeout=timeout)
    except BaseException as exception:
        if isinstance(exception, subprocess.TimeoutExpired):
            process['timed_out'] = True
        else:
            process['interruption'] = type(exception).__name__
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process['process_group_killed'] = True
        process['exit_code'] = child.wait(timeout=5)
        if not isinstance(exception, subprocess.TimeoutExpired):
            raise


def run(root, binary, output, source, expected_binary):
    root, binary, output = root.resolve(), binary.resolve(), output.resolve()
    check_identity(root, source, binary, expected_binary)
    output.mkdir(parents=True, exist_ok=False)
    retained = output / f'allocation-stack-worker-{expected_binary}'
    shutil.copyfile(binary, retained)
    retained.chmod(0o500)
    script = output / 'observer.gdb'
    script.write_text(GDB_SCRIPT)
    gdb = Path(shutil.which('gdb') or '').resolve()
    require(gdb.is_file(), 'FAIL_GDB_UNAVAILABLE')
    gdb_hash = digest(gdb)
    version = subprocess.run([str(gdb), '--version'], text=True, capture_output=True, timeout=5, check=True)
    (output / 'gdb-version.log').write_text(version.stdout + version.stderr)
    command = [str(gdb), '--nx', '--batch', '-x', str(script), '--args', str(retained), '--exact', TEST,
               '--ignored', '--nocapture', '--test-threads=1']
    environment = dict(os.environ, XADB_ALLOCATION_STACK_OBSERVER='1', PYTHONDONTWRITEBYTECODE='1')
    process = {'command': command, 'exit_code': None, 'timed_out': False, 'timeout_seconds': TIMEOUT}
    report = {'status': 'RUNNING', 'scope': 'intrusive resource correctness; no performance claim',
              'source_root': str(root), 'source_sha256': source,
              'binary': str(retained), 'binary_sha256': expected_binary,
              'gdb_script_sha256': digest(script), 'gdb_binary': str(gdb), 'gdb_binary_sha256': gdb_hash,
              'checker_files': {str(path.resolve()): digest(path) for path in
                                (Path(__file__), Path(__file__).with_name('test_check_dev3_allocation_stack.py'),
                                 Path(__file__).with_name('check_dev0.py'))}, 'process': process}
    started = time.monotonic()
    try:
        with (output / 'stdout.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
            with subprocess.Popen(command, cwd=root, env=environment, stdout=stdout, stderr=stderr,
                                  start_new_session=True) as child:
                process['pid'] = child.pid
                wait_process_group(child, process)
        process['seconds'] = time.monotonic() - started
        result = evaluate((output / 'stdout.log').read_text(), process, retained, source)
        check_identity(root, source, binary, expected_binary)
        require(digest(retained) == expected_binary, 'FAIL_BINARY_IDENTITY')
        require(digest(gdb) == gdb_hash, 'FAIL_GDB_IDENTITY')
        report.update(result)
    except BaseException as exception:
        process.setdefault('seconds', time.monotonic() - started)
        report['status'] = 'FAIL'
        report['failure'] = str(exception)
        raise
    finally:
        report['artifacts'] = {path.name: digest(path) for path in sorted(output.iterdir()) if path.is_file()}
        (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--binary-sha256', required=True)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = run(args.root, args.binary, args.output, args.source_sha256, args.binary_sha256)
    print(json.dumps({'status': result['status'], 'stack_bytes': result['pthread_stack_bytes'],
                      'report': str(args.output / 'result.json')}))


if __name__ == '__main__':
    main()
