#!/usr/bin/env python3
"""P3 collector joining a root BPF owner to the unchanged P1 measurement runner."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

import dev2_measurement_window as window


class Trace:
    def __init__(self, worker, device, major, minor):
        self.tools = {name: dict(path=str(window.trusted_tool('/usr/bin/'+name)),
                                 sha256=window.digest(window.trusted_tool('/usr/bin/'+name)))
                      for name in ('sudo','python3','setpriv','bpftrace')}
        self.spool = Path(tempfile.mkdtemp(prefix='xadb-io-', dir='/dev/shm'))
        self.messages = []
        self.pending = bytearray()
        self.closed = False
        command = ['/usr/bin/sudo', '-n', '--', '/usr/bin/python3', '-I',
                   str(Path(__file__).with_name('dev2_io_supervisor.py').resolve()),
                   '--spool', str(self.spool), '--worker', str(worker),
                   '--starttime', str(window.starttime(worker)), '--device', str(device),
                   '--major', str(major), '--minor', str(minor)]
        self.command = command
        self.err = (self.spool / 'launcher.stderr').open('xb')
        self.process = None
        try:
            self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                            stderr=self.err, start_new_session=True,
                                            env={'PATH':'/usr/bin:/bin', 'LC_ALL':'C'})
            os.set_blocking(self.process.stdout.fileno(), False)
        except Exception as error:
            errors = [repr(error)]
            if self.process is not None:
                for action in (self.process.stdin.close, lambda: self.process.wait(timeout=8), self.process.stdout.close):
                    try:
                        action()
                    except Exception as cleanup_error:
                        errors.append(repr(cleanup_error))
            self.err.close()
            (self.spool / 'constructor-error.json').write_text(json.dumps(errors))
            raise window.DiagnosticError(f'collector construction failed, retained {self.spool}: {errors}') from error
        self.started = time.monotonic()

    def tick(self):
        try:
            chunk = os.read(self.process.stdout.fileno(), 65536)
        except BlockingIOError:
            chunk = b''
        self.pending.extend(chunk)
        window.require(len(self.pending) < window.CAP, 'supervisor control bound')
        while b'\n' in self.pending:
            row, _, self.pending = self.pending.partition(b'\n')
            self.messages.append(json.loads(row))
            window.require(len(self.messages) <= 3, 'supervisor message bound')
        if os.waitid(os.P_PID, self.process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT):
            window.require(any('done' in m for m in self.messages), 'supervisor exited without receipt')
        return any('ready' in m for m in self.messages)

    def stop(self, tick):
        self.process.stdin.write(b'STOP\n'); self.process.stdin.flush()
        end = time.monotonic() + 5
        while not any('done' in m for m in self.messages):
            tick(); self.tick()
            window.require(time.monotonic() < end, 'supervisor STOP deadline')
            time.sleep(.005)
        window.require(self.messages[-1]['done']['status'] == 'STOPPED', 'BPF stop failed')

    def finish(self, destination):
        if self.closed:
            return
        self.closed = True
        errors = []
        def attempt(action):
            try:
                return action()
            except Exception as error:
                errors.append(repr(error))
                return None
        attempt(self.process.stdin.close)  # EOF reaches root even if buffered close fails.
        attempt(lambda: self.process.wait(timeout=8))
        attempt(self.process.stdout.close)
        attempt(self.err.close)
        destination.mkdir()
        for path in self.spool.iterdir():
            if path.is_file():
                attempt(lambda p=path: shutil.copy2(p, destination / p.name))
        (destination / 'launcher.json').write_text(json.dumps(dict(command=self.command, tools=self.tools,
             messages=self.messages, cleanup_errors=errors), indent=2)+'\n')
        receipt = json.loads((destination / 'supervisor.json').read_text())
        window.require(not errors and receipt.get('collector_gone') is True and receipt.get('group_gone') is True,
                       f'collector cleanup unconfirmed; retained {self.spool}: {errors}')
        shutil.rmtree(self.spool)


def send_payload(child, payload, tick):
    fd = child.process.stdin.fileno()
    os.set_blocking(fd, False)
    remaining = memoryview(payload)
    try:
        while remaining:
            tick()
            window.require(not child.exited(), 'worker exited before payload')
            try:
                size = os.write(fd, remaining)
                window.require(size > 0, 'zero payload write')
                remaining = remaining[size:]
            except BlockingIOError:
                time.sleep(.001)
    finally:
        child.process.stdin.close()


class IoRunner(window.Runner):
    def __init__(self, directory, trace, device, major, minor):
        super().__init__(directory, 60)
        self.trace_enabled, self.device, self.major, self.minor = trace, device, major, minor
        self.trace = None

    def spawn(self, command, work, env, stdin, name, cap, owned_type=window.Owned):
        if name != 'worker' or not self.trace_enabled:
            return super().spawn(command, work, env, stdin, name, cap, owned_type)
        payload = stdin.read()
        child = super().spawn(command, work, env, subprocess.PIPE, name, cap, owned_type)
        self.trace = Trace(child.pid, self.device, self.major, self.minor)
        self.wait(self.trace.tick)
        send_payload(child, payload, self.tick)
        return child

    def send(self, peer, message, end):
        super().send(peer, message, end)
        if message.startswith('DONE ') and self.trace:
            self.trace.stop(self.tick)

    def cleanup(self):
        try:
            if self.trace:
                self.trace.finish(self.directory / 'trace')
        except Exception as error:
            self.record['status'] = 'FAILED'
            self.record['trace_cleanup_error'] = repr(error)
        super().cleanup()


def collect(worker, input_file, source, output, *, trace, device, major, minor):
    data = window.bounded_input(input_file)
    output.mkdir(mode=0o700)
    (output / 'input.bin').write_bytes(data)
    run = IoRunner(output, trace, device, major, minor)
    run.record.update(trace_enabled=trace, boot_id=Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
                      clock='CLOCK_MONOTONIC', device=device, disk=[major, minor])
    handlers = {}
    def cancel(signum, _frame):
        run.cancel_signal = signum
    try:
        for sig in (signal.SIGINT, signal.SIGTERM):
            handlers[sig] = signal.signal(sig, cancel)
        run.observe(worker.resolve(), output / 'input.bin', source, 'durable', '/usr/bin/perf', 2, 'sudo-cap-perfmon')
    except Exception as error:
        run.record['error'] = repr(error)
    finally:
        for sig in handlers:
            signal.signal(sig, signal.SIG_IGN)
        try:
            run.cleanup()
        finally:
            for sig, handler in handlers.items():
                signal.signal(sig, handler)
        run.record['files'] = {str(p.relative_to(output)): dict(sha256=window.digest(p), bytes=p.stat().st_size)
                               for p in output.rglob('*') if p.is_file() and 'work' not in p.relative_to(output).parts}
        (output / 'result.json').write_text(json.dumps(run.record, indent=2)+'\n')
    return run.record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('worker', 'input', 'output'):
        parser.add_argument('--'+name, required=True, type=Path)
    parser.add_argument('--source', required=True)
    parser.add_argument('--trace', action='store_true')
    for name in ('device', 'major', 'minor'):
        parser.add_argument('--'+name, required=True, type=int)
    args = parser.parse_args()
    record = collect(args.worker, args.input, args.source, args.output, trace=args.trace,
                     device=args.device, major=args.major, minor=args.minor)
    print(json.dumps(record))
    return 0 if record['status'] == 'COLLECTED_DIAGNOSTIC_ONLY' else 1


if __name__ == '__main__':
    raise SystemExit(main())
