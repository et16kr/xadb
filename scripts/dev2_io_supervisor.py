#!/usr/bin/env python3
"""Fixed privileged BPF child owner. No arbitrary command or program argument.

The ordinary runner owns the control pipe. EOF cancels. This process owns the
unreaped collector, its pidfd and independent deadline. Only the collector gets
PDEATHSIG. This is a trusted local diagnostic, not a multi-user privilege API.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import re
import select
import signal
import stat
import subprocess
import time

LIMIT = 64 * 1024 * 1024
PROBES = 15


def identity(pid):
    proc = Path('/proc') / str(pid)
    status = dict(line.split(':', 1) for line in (proc / 'status').read_text().splitlines())
    return dict(pid=pid, starttime=int((proc / 'stat').read_text().rsplit(')', 1)[1].split()[19]),
                uid=list(map(int, status['Uid'].split())), pgid=os.getpgid(pid), sid=os.getsid(pid))


def render(worker, device, major, minor):
    values = dict(WORKER=worker, DEV=device, MAJOR=major, MINOR=minor)
    if any(type(v) is not int or not 0 <= v < 2**32 for v in values.values()) or worker < 1:
        raise ValueError('invalid fixed probe parameter')
    source = Path(__file__).with_name('dev2_io_trace.bt').read_text()
    for key, value in values.items():
        source = source.replace('__' + key + '__', str(value))
    return source


def collector_command(directory):
    return ['/usr/bin/bpftrace', '-k', '-B', 'line', '-o', str(directory / 'events'), str(directory / 'program.bt')]


def collector_exec(parent, directory):
    # setpriv has already installed PDEATHSIG. If the parent died before that,
    # reject the reparented child; death after this check delivers SIGKILL.
    if os.geteuid() != 0 or os.getppid() != parent:
        raise RuntimeError('collector parent lost before PDEATHSIG installation')
    command = collector_command(directory)
    os.execve(command[0], command, os.environ)


def emit(value):
    try:
        print(json.dumps(value), flush=True)
    except BrokenPipeError:
        pass


def run(args):
    uid = int(os.environ['SUDO_UID'])
    directory = args.spool.resolve()
    info = directory.stat()
    if os.geteuid() != 0 or uid == 0 or directory.parent != Path('/dev/shm') or not directory.name.startswith('xadb-io-'):
        raise ValueError('root supervisor requires private /dev/shm/xadb-io-*')
    if info.st_uid != uid or stat.S_IMODE(info.st_mode) != 0o700 or args.spool.is_symlink():
        raise ValueError('spool owner/mode mismatch')
    target = identity(args.worker)
    if target['uid'] != [uid] * 4 or target['starttime'] != args.starttime:
        raise ValueError('worker identity mismatch')
    record = dict(status='FAILED', supervisor=identity(os.getpid()), target=target,
                  boot_id=Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
                  clock='CLOCK_MONOTONIC', time_ns=os.readlink('/proc/self/ns/time'),
                  initial_time_ns=os.readlink('/proc/1/ns/time'),
                  offsets=Path('/proc/self/timens_offsets').read_text(),
                  deadline_seconds=55, cleanup_seconds=3)
    if record['time_ns'] != record['initial_time_ns'] or any(int(v) != 0 for line in record['offsets'].splitlines() for v in line.split()[1:]):
        raise ValueError('host clock domain required')
    program = directory / 'program.bt'
    with program.open('x') as stream:
        stream.write(render(args.worker, args.device, args.major, args.minor))
    record['program_sha256'] = hashlib.sha256(program.read_bytes()).hexdigest()
    command = ['/usr/bin/setpriv', '--pdeathsig', 'KILL', '/usr/bin/python3', '-I',
               str(Path(__file__).resolve()), '--collector-parent', str(os.getpid()), '--spool', str(directory)]
    record['command'] = command
    deadline = time.monotonic() + 55
    child = None
    pidfd = None
    logs = {}
    cancelled = []
    def cancel(signum, _frame):
        cancelled.append(signum)
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, cancel)
    try:
        # No control-pipe inheritance into BPF. File size is an OS enforced cap.
        def limits():
            resource.setrlimit(resource.RLIMIT_FSIZE, (LIMIT, LIMIT))
        child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, start_new_session=True, preexec_fn=limits,
                                 env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'HOME': '/root',
                                      '__BPFTRACE_NOTIFY_PROBES_ATTACHED': '1'})
        pidfd = os.pidfd_open(child.pid)
        record['collector'] = identity(child.pid)
        emit({'starting': record})
        for pipe, name in ((child.stdout, 'control'), (child.stderr, 'stderr')):
            os.set_blocking(pipe.fileno(), False)
            logs[pipe.fileno()] = [pipe, (directory / name).open('xb'), bytearray()]
        os.set_blocking(0, False)
        control = bytearray()
        ready = False
        stop_at = None
        disabled_at = None
        int_sent = False
        while True:
            now = time.monotonic()
            if cancelled or now >= deadline:
                raise RuntimeError('cancel/deadline')
            # WNOWAIT keeps the PID reserved until all owned group signalling ends.
            exited = os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
            for fd in select.select([0, *logs], [], [], .01)[0]:
                data = os.read(fd, 65536)
                if fd == 0:
                    if not data:
                        raise RuntimeError('parent_eof')
                    control.extend(data)
                    if len(control) > 16 or b'\n' in control and bytes(control) != b'STOP\n':
                        raise RuntimeError('cancel/invalid control')
                    if control == b'STOP\n':
                        if not ready or stop_at is not None:
                            raise RuntimeError('invalid STOP state')
                        signal.pidfd_send_signal(pidfd, signal.SIGUSR1)
                        stop_at = now
                        control.clear()
                else:
                    entry = logs[fd]
                    entry[1].write(data); entry[1].flush(); entry[2].extend(data)
                    if len(entry[2]) > 1024 * 1024:
                        raise RuntimeError('control/stderr bound')
            stdout = bytes(logs[child.stdout.fileno()][2])
            stderr = bytes(logs[child.stderr.fileno()][2])
            if not ready and b'__BPFTRACE_NOTIFY_PROBES_ATTACHED\n' in stdout and b'\n' in stderr:
                if stdout.count(b'__BPFTRACE_NOTIFY_PROBES_ATTACHED\n') != 1 or stderr.strip() != b'Attached 15 probes':
                    raise RuntimeError('attach count/warning mismatch')
                ready = True
                record['collector'] = identity(child.pid)
                if record['collector']['uid'] != [0] * 4 or record['collector']['pgid'] != child.pid:
                    raise RuntimeError('collector identity mismatch')
                emit({'ready': record})
            if stop_at is not None and not int_sent:
                events = directory / 'events'
                with events.open('rb') as stream:
                    stream.seek(max(0, events.stat().st_size - 4096))
                    disabled = any(re.fullmatch(rb'DISABLED [0-9]+', line) for line in stream.read().splitlines())
                if now - stop_at >= 2 and not disabled:
                    raise RuntimeError('disable ACK missing')
                if disabled and disabled_at is None:
                    disabled_at = now
                if disabled_at is not None and now - disabled_at >= 1:
                    signal.pidfd_send_signal(pidfd, signal.SIGINT)
                    int_sent = True
                    deadline = min(deadline, now + 2)
            if exited:
                record['exit'] = dict(code=exited.si_code, status=exited.si_status)
                if not int_sent or exited.si_code != os.CLD_EXITED or exited.si_status != 0:
                    raise RuntimeError('unexpected collector exit')
                record['status'] = 'STOPPED'
                break
    except Exception as error:
        record['error'] = repr(error)
    finally:
        errors = []
        def attempt(name, action):
            try:
                return action()
            except Exception as error:
                errors.append(f'{name}: {error!r}')
                return None
        def group_signal(sig):
            try:
                os.killpg(child.pid, sig)
            except ProcessLookupError:
                pass
        if child is not None:
            # Each step is independent; one failure cannot bypass KILL or reap.
            attempt('TERM', lambda: group_signal(signal.SIGTERM))
            end = time.monotonic() + 1
            while time.monotonic() < end:
                exited = attempt('waitid', lambda: os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT))
                if exited or errors:
                    break
                time.sleep(.01)
            attempt('KILL', lambda: group_signal(signal.SIGKILL))
            record['reaped'] = attempt('reap', lambda: child.wait(timeout=2))
            if pidfd is not None:
                attempt('pidfd close', lambda: os.close(pidfd))
            for pipe, stream, data in logs.values():
                def drain():
                    while True:
                        part = pipe.read(65536)
                        if not part:
                            break
                        if len(data) + len(part) > 1024 * 1024:
                            raise RuntimeError('final output bound')
                        stream.write(part); data.extend(part)
                attempt('final drain', drain)
                attempt('log close', stream.close)
                attempt('pipe close', pipe.close)
            final_stderr = attempt('read final stderr', lambda: (directory / 'stderr').read_bytes())
            if final_stderr is not None and final_stderr.strip() != b'Attached 15 probes':
                errors.append('final BPF stderr warning/error')
            record['collector_gone'] = attempt('collector disappearance', lambda: not Path('/proc', str(child.pid)).exists())
            def group_gone():
                try:
                    os.killpg(child.pid, 0)
                    return False
                except ProcessLookupError:
                    return True
            record['group_gone'] = attempt('group disappearance', group_gone)
            if not record['collector_gone'] or not record['group_gone']:
                errors.append('owned collector/group still present')
        for path in attempt('list raw files', lambda: list(directory.iterdir())) or []:
            def own(p=path):
                if p.is_file() and not p.is_symlink():
                    os.chown(p, uid, -1)
            attempt('raw ownership', own)
        if errors:
            record['status'] = 'FAILED'
            record['cleanup_errors'] = errors
        def save():
            with (directory / 'supervisor.json').open('x') as stream:
                json.dump(record, stream, indent=2)
            os.chown(directory / 'supervisor.json', uid, -1)
        attempt('receipt', save)
        if errors:
            record['status'] = 'FAILED'
            record['cleanup_errors'] = errors
        emit({'done': record})
    return 0 if record['status'] == 'STOPPED' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spool', type=Path, required=True)
    parser.add_argument('--collector-parent', type=int)
    for key in ('worker', 'starttime', 'device', 'major', 'minor'):
        parser.add_argument('--' + key, type=int)
    args = parser.parse_args()
    if args.collector_parent is not None:
        collector_exec(args.collector_parent, args.spool)
        return 1
    if any(getattr(args, key) is None for key in ('worker', 'starttime', 'device', 'major', 'minor')):
        parser.error('fixed collector parameters are required')
    return run(args)


if __name__ == '__main__':
    raise SystemExit(main())
