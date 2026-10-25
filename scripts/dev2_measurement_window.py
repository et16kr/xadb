#!/usr/bin/env python3
"""One bounded diagnostic window; never an instrumentation-cost acceptance run.

PMU includes the control bracket around the worker window. Counts are unscaled,
non-inherited, and never converted to GHz. SIGKILL of this controller cannot run
finally cleanup; run under a supervising process if that failure must be covered.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import secrets
import select
import shutil
import signal
import socket
import stat
import struct
import subprocess
import tempfile
import time

EVENTS = ('cycles:u', 'instructions:u', 'ref-cycles:u', 'context-switches',
          'cpu-migrations', 'page-faults')
EVENT_PROFILES = {
    'user-v1': EVENTS,
    'user-kernel-v1': ('cycles:u', 'cycles:k', 'instructions:u', 'instructions:k',
                       'ref-cycles:u', 'context-switches', 'cpu-migrations', 'page-faults'),
}
CAP = 1024 * 1024
LINE_CAP = 512
SUFFIX = '.diagnostic_window.v1'
PERFMON = 1 << 38
PRIVILEGES = ('none', 'sudo-cap-perfmon')


class DiagnosticError(Exception):
    pass


def require(condition, message):
    if not condition:
        raise DiagnosticError(message)


def digest(path):
    with open(path, 'rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def starttime(pid):
    # comm may contain spaces and parentheses; fields after its final ')' start at 3.
    return int(Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[19])


def trusted_tool(path):
    """Privilege mode uses fixed, root-controlled installed tools only."""
    resolved = Path(path).resolve(strict=True)
    for item in (resolved, *resolved.parents):
        info = item.stat()
        require(info.st_uid == 0 and not info.st_mode & 0o022,
                f'privilege tool/path is not root controlled: {item}')
    require(resolved.is_file() and os.access(resolved, os.X_OK), 'privilege tool is not executable')
    return resolved


def perf_invocation(command, privilege, work):
    require(privilege in PRIVILEGES, 'unsupported perf privilege mode')
    if privilege == 'none':
        return command, {'mode': privilege}
    uid, gid = os.getuid(), os.getgid()
    require(uid != 0 and os.geteuid() == uid and os.getegid() == gid,
            'privileged perf requires an ordinary caller')
    caller = process_identity(os.getpid())
    require(all(caller['caps'][k] == 0 for k in ('CapPrm', 'CapEff', 'CapAmb')),
            'privileged perf requires a caller without usable capabilities')
    paths = [trusted_tool(p) for p in ('/usr/bin/sudo', '/usr/bin/setpriv', '/usr/bin/perf')]
    sudo, setpriv, perf = paths
    require(Path(command[0]).resolve() == perf, 'privileged mode only supports installed /usr/bin/perf')
    command = [str(perf), *command[1:]]
    environment = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'HOME': str(work), 'PERF_CONFIG': '/dev/null'}
    wrapped = [str(sudo), '-n', '--preserve-env=' + ','.join(environment), '--',
               str(setpriv), f'--reuid={uid}', f'--regid={gid}',
               '--clear-groups', '--inh-caps=-all,+perfmon', '--ambient-caps=-all,+perfmon',
               '--bounding-set=-all,+perfmon', '--no-new-privs', '--pdeathsig=KILL', '--', *command]
    return wrapped, {'mode': privilege, 'uid': uid, 'gid': gid, 'perf_command': command,
                     'caller': caller,
                     'environment': environment,
                     'tools': [{'path': str(p), 'sha256': digest(p)} for p in paths]}


def process_identity(pid):
    proc = Path(f'/proc/{pid}')
    status = dict(line.split(':', 1) for line in (proc / 'status').read_text().splitlines())
    return {'pid': pid, 'starttime': starttime(pid), 'pgid': os.getpgid(pid), 'sid': os.getsid(pid),
            'ppid': int(status['PPid']), 'uids': list(map(int, status['Uid'].split())),
            'gids': list(map(int, status['Gid'].split())), 'groups': list(map(int, status['Groups'].split())),
            'caps': {k: int(status[k], 16) for k in ('CapInh', 'CapPrm', 'CapEff', 'CapBnd', 'CapAmb')},
            'no_new_privs': int(status['NoNewPrivs']),
            'command': (proc / 'cmdline').read_bytes().rstrip(b'\0').decode().split('\0')}


def validate_worker_identity(worker, policy):
    uid, gid = policy['uid'], policy['gid']
    require(worker['uids'] == [uid] * 4 and worker['gids'] == [gid] * 4 and
            worker['caps'] == policy['caller']['caps'] and worker['groups'] == policy['caller']['groups'],
            'worker must remain unprivileged')


def validate_perf_identity(launcher, collector, worker, policy):
    uid, gid = policy['uid'], policy['gid']
    validate_worker_identity(worker, policy)
    require(launcher['uids'][0] == uid and launcher['pgid'] == launcher['pid'] == launcher['sid'],
            'perf launcher must remain signalable in its owned session')
    require(collector['pgid'] == collector['sid'] == launcher['pid'] and
            (collector['pid'] == launcher['pid'] or collector['ppid'] == launcher['pid']),
            'perf escaped owned process group')
    require(collector['uids'] == [uid] * 4 and collector['gids'] == [gid] * 4 and
            collector['groups'] == [] and collector['no_new_privs'] == 1 and
            all(v == PERFMON for v in collector['caps'].values()), 'perf capability/identity mismatch')
    require(collector['command'] == policy['perf_command'], 'perf command mismatch')


def profile_events(profile):
    require(isinstance(profile, str) and profile in EVENT_PROFILES, 'unsupported perf event profile')
    return EVENT_PROFILES[profile]


def parse_perf(raw, profile='user-v1'):
    """Parse literal perf -x ';' --no-scale rows, retaining availability and coverage."""
    events = profile_events(profile)
    rows = {}
    for line in raw.splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        fields = [v.strip() for v in line.split(';')]
        require(len(fields) >= 5 and fields[2] in events, 'unexpected PMU row')
        count, unit, event, runtime, coverage = fields[:5]
        require(event not in rows, 'duplicate PMU event')
        item = {'raw': line, 'unit': unit, 'count': None, 'runtime_ns': None,
                'coverage_percent': None, 'status': 'UNAVAILABLE'}
        if count not in ('<not supported>', '<not counted>'):
            require(count.isascii() and count.isdecimal(), 'invalid unscaled PMU count')
            require(runtime.isascii() and runtime.isdecimal(), 'invalid PMU runtime')
            value = float(coverage.rstrip('%'))
            require(math.isfinite(value) and 0 <= value <= 100, 'invalid PMU coverage')
            item.update(count=int(count), runtime_ns=int(runtime), coverage_percent=value,
                        status='REPORTED_FULL_COVERAGE' if value == 100 and int(runtime) > 0 else 'PARTIAL_OR_EMPTY')
        rows[event] = item
    require(bool(rows), 'PMU finalization emitted no counter rows')
    for event in events:
        rows.setdefault(event, {'status': 'MISSING', 'count': None})
    eligible = all(rows[event]['status'] == 'REPORTED_FULL_COVERAGE' for event in events)
    return {'profile': profile, 'events': rows, 'all_events_reported_full_coverage': eligible,
            'ratios': None, 'raw_unscaled': True, 'includes_control_bracket': True}


def validate_hello(line, peer_pid, pid, expected_start, source, workload, token):
    require(line.split() == ['HELLO', '1', token, str(pid), str(pid), source, workload],
            'HELLO identity/version/token mismatch')
    require(peer_pid == pid and starttime(pid) == expected_start, 'peer PID/starttime mismatch')


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, 'duplicate JSON field')
        value[key] = item
    return value


def validate_stop(line, token, go_ns, received_ns):
    try:
        value = json.loads(line, object_pairs_hook=unique_object)
    except (ValueError, UnicodeError) as error:
        raise DiagnosticError('invalid STOP JSON') from error
    require(isinstance(value, dict) and set(value) == {
        'phase', 'token', 'start_ns', 'end_ns', 'elapsed_ns', 'before', 'after'}, 'STOP fields')
    require(value['phase'] == 'STOP' and value['token'] == token, 'STOP phase/token')
    for key in ('start_ns', 'end_ns', 'elapsed_ns'):
        require(type(value[key]) is int and 0 <= value[key] < 2**64, 'STOP timestamp type/range')
    require(go_ns <= value['start_ns'] <= value['end_ns'] <= received_ns, 'STOP bracket order')
    require(0 < value['elapsed_ns'] <= value['end_ns'] - value['start_ns'], 'STOP elapsed bounds')
    for key in ('before', 'after'):
        usage = value[key]
        require(usage is None or isinstance(usage, list) and len(usage) == 6 and
                all(type(v) is int and 0 <= v < 2**64 for v in usage), 'STOP rusage shape')
    before, after = value['before'], value['after']
    require(before is None or after is None or all(b <= a for b, a in zip(before, after)),
            'STOP decreasing rusage')
    value['usage_delta'] = [a - b for b, a in zip(before, after)] if before is not None and after is not None else None
    value['boundary_excess_ns'] = value['end_ns'] - value['start_ns'] - value['elapsed_ns']
    return value


class Capture:
    def __init__(self, pipe, path, limit):
        self.pipe, self.path, self.limit = pipe, path, limit
        self.saved = 0
        self.overflow = False
        self.eof = False
        self.file = path.open('xb')
        os.set_blocking(pipe.fileno(), False)

    def drain(self):
        # One bounded chunk per tick prevents a chatty child starving deadlines.
        try:
            data = os.read(self.pipe.fileno(), 65536)
        except BlockingIOError:
            return
        if not data:
            self.eof = True
            return
        size = min(len(data), self.limit - self.saved)
        self.file.write(data[:size])
        self.file.flush()
        self.saved += size
        self.overflow |= size < len(data)

    def close(self):
        self.pipe.close()
        self.file.close()


class Owned:
    def __init__(self, command, work, env, stdin, prefix, output_cap):
        self.process = subprocess.Popen(command, cwd=work, env=env, stdin=stdin,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        start_new_session=True)
        self.pid = self.process.pid
        self.start = None
        self.command = command
        self.captures = []
        self.cleanup = None

    def exited(self):
        # WNOWAIT reserves the PID/group ID until cleanup; do not call Popen.poll.
        return os.waitid(os.P_PID, self.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is not None

    def finish(self):
        signals = []
        capture_errors = []
        def drain(capture):
            try:
                if not capture.eof:
                    capture.drain()
            except Exception as error:
                capture_errors.append(repr(error))
                capture.eof = True
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(self.pid, sig)
                signals.append(sig.name)
            except ProcessLookupError:
                pass
            end = time.monotonic() + 2
            while not self.exited() and time.monotonic() < end:
                for capture in self.captures:
                    drain(capture)
                time.sleep(.005)
            if self.exited():
                # The unreaped leader keeps this group ID reserved. Eliminate
                # any remaining descendants before releasing that reservation.
                try:
                    os.killpg(self.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                break
        if self.exited():
            code = self.process.wait(timeout=.1)
        else:
            code = None
        for _ in range(32):
            for capture in self.captures:
                if not capture.eof:
                    drain(capture)
        for capture in self.captures:
            try:
                capture.close()
            except Exception as error:
                capture_errors.append(repr(error))
        for pipe in (self.process.stdout, self.process.stderr):
            pipe.close()
        try:
            os.killpg(self.pid, 0)
            group_gone = False
        except ProcessLookupError:
            group_gone = True
        self.cleanup = {'pid': self.pid, 'starttime': self.start, 'returncode': code,
                        'signals': signals, 'group_gone': group_gone,
                        'output_overflow': any(c.overflow for c in self.captures), 'capture_errors': capture_errors}
        return self.cleanup


class PrivilegedPerf(Owned):
    """Pin the actual perf child separately from its sudo supervisor."""
    def __init__(self, *args):
        super().__init__(*args)
        self.member = None

    def pin_collector(self, identity):
        if identity['pid'] == self.pid:
            return
        fd = os.pidfd_open(identity['pid'])
        try:
            current = process_identity(identity['pid'])
            require(current['starttime'] == identity['starttime'] and current['ppid'] == self.pid,
                    'perf child identity changed before pinning')
        except Exception:
            os.close(fd)
            raise
        self.member = (fd, identity['pid'], identity['starttime'])

    def finish(self):
        errors = []
        result = None
        try:
            try:
                result = super().finish()
            except Exception as error:
                errors.append(f'launcher cleanup: {error!r}')
            result = result or self.cleanup or {'pid': self.pid, 'starttime': self.start,
                'returncode': None, 'signals': [], 'group_gone': False,
                'output_overflow': any(c.overflow for c in self.captures), 'capture_errors': []}
            if self.member:
                fd, pid, epoch = self.member
                # A verified direct collector may have an unsupported group.
                # pidfd never signals a reused PID, even after sudo reaps it.
                exited = bool(select.select([fd], [], [], 0)[0])
                try:
                    if not exited:
                        signal.pidfd_send_signal(fd, signal.SIGKILL)
                        exited = bool(select.select([fd], [], [], 2)[0])
                except OSError as error:
                    errors.append(f'collector cleanup: {error!r}')
                result['collector'] = {'pid': pid, 'starttime': epoch, 'exited': exited}
                result['group_gone'] &= exited
            result['capture_errors'].extend(errors)
            return result
        finally:
            if self.member:
                os.close(self.member[0])


class PerfAck:
    """One outstanding FIFO command; tolerate a split optional NUL terminator.

    perf's protocol has no sequence ID. Already queued unsolicited bytes are
    rejected; a malicious late reply indistinguishable from the current ACK
    cannot be authenticated by this protocol.
    """
    def __init__(self):
        self.optional_nul = False
        self.pending = bytearray()

    def strip_previous_nul(self, data):
        if self.optional_nul and data:
            self.optional_nul = False
            if data.startswith(b'\0'):
                return data[1:]
        return data

    def unsolicited(self, data):
        require(not self.strip_previous_nul(data), 'unsolicited perf ACK bytes')

    def feed(self, data):
        self.pending.extend(self.strip_previous_nul(data))
        require(len(self.pending) <= LINE_CAP, 'perf ACK bound')
        if b'\n' not in self.pending:
            return False
        require(self.pending in (b'ack\n', b'ack\n\0'), 'invalid perf ACK')
        self.optional_nul = self.pending == b'ack\n'
        self.pending.clear()
        return True


class Runner:
    def __init__(self, directory, deadline):
        self.directory, self.deadline = directory, time.monotonic() + deadline
        self.cancel_signal = None
        self.ack_parser = PerfAck()
        self.children = []
        self.record = {'schema_version': 1, 'status': 'FAILED',
                       'performance_acceptance': 'NOT_APPLICABLE', 'phases': [],
                       'pmu': {'status': 'NOT_STARTED'}, 'cleanup': []}

    def tick(self, end=None):
        require(self.cancel_signal is None, f'runner interrupted by signal {self.cancel_signal}')
        require(time.monotonic() < min(self.deadline, end or self.deadline), 'deadline exceeded')
        for child in self.children:
            for capture in child.captures:
                if not capture.eof:
                    capture.drain()
                require(not capture.overflow, f'output bound exceeded: {capture.path.name}')

    def wait(self, predicate, end=None):
        while True:
            self.tick(end)
            value = predicate()
            if value:
                return value
            time.sleep(.001)

    def spawn(self, command, work, env, stdin, name, cap, owned_type=Owned):
        child = owned_type(command, work, env, stdin, self.directory / name, cap)
        self.children.append(child)
        child.start = starttime(child.pid)
        for pipe, suffix, limit in ((child.process.stdout, '.stdout', cap), (child.process.stderr, '.stderr', CAP)):
            child.captures.append(Capture(pipe, (self.directory / name).with_suffix(suffix), limit))
        self.record.setdefault('commands', {})[name] = command
        return child

    def receive(self, peer, end):
        result = bytearray()
        while True:
            self.tick(end)
            try:
                byte = peer.recv(1)
                require(bool(byte), 'peer EOF')
                if byte == b'\n':
                    return result.decode('ascii')
                result.extend(byte)
                require(len(result) < LINE_CAP, 'line bound exceeded')
            except BlockingIOError:
                time.sleep(.001)

    def send(self, peer, message, end):
        data = (message + '\n').encode('ascii')
        require(len(data) <= LINE_CAP, 'outgoing line bound')
        while data:
            self.tick(end)
            try:
                count = peer.send(data)
                require(count > 0, 'zero socket write')
                data = data[count:]
            except BlockingIOError:
                time.sleep(.001)

    def perf_command(self, command, ctl, ack, child, timeout=2):
        end = time.monotonic() + timeout
        sent = False
        self.ack_parser.pending.clear()
        try:
            self.ack_parser.unsolicited(os.read(ack, LINE_CAP + 1))
        except BlockingIOError:
            pass
        while True:
            self.tick(end)
            require(not child.exited(), 'perf exited during control exchange')
            try:
                if not sent:
                    sent = os.write(ctl, (command + '\n').encode()) == len(command) + 1
                    require(sent, 'partial perf control write')
                value = os.read(ack, LINE_CAP)
                if self.ack_parser.feed(value):
                    return time.monotonic_ns()
            except BlockingIOError:
                pass
            time.sleep(.001)

    def prepare_perf(self, executable, work, pid, control_dir, privilege='none', profile='user-v1'):
        events = profile_events(profile)
        require(privilege in PRIVILEGES, 'unsupported perf privilege mode')
        if executable is None:
            require(privilege == 'none', 'privileged mode requires installed perf')
            self.record['pmu'] = {'status': 'UNAVAILABLE', 'reason': 'no perf executable selected or available'}
            return None
        ctl_path, ack_path = control_dir / 'ctl', control_dir / 'ack'
        for path in (ctl_path, ack_path):
            os.mkfifo(path, 0o600)
        # RDWR/nonblocking avoids waiting indefinitely for the other FIFO opener.
        ctl = os.open(ctl_path, os.O_RDWR | os.O_NONBLOCK)
        self.fifos = (ctl,)
        ack = os.open(ack_path, os.O_RDWR | os.O_NONBLOCK)
        self.fifos = (ctl, ack)
        command = [str(executable), 'stat', '--no-inherit', '--no-scale', '--no-big-num',
                   '-x', ';', '--log-fd', '1', '-e', ','.join(events), '--delay=-1',
                   '--control', f'fifo:{ctl_path},{ack_path}', '--tid', str(pid)]
        self.record['perf_executable'] = {'path': str(executable), 'sha256': digest(executable)}
        command, policy = perf_invocation(command, privilege, work)
        self.record['perf_privilege'] = policy
        if privilege != 'none':
            worker = process_identity(pid)
            policy['worker_before_attach'] = worker
            validate_worker_identity(worker, policy)
        env = policy.get('environment', os.environ.copy())
        perf = self.spawn(command, work, env, subprocess.DEVNULL, 'perf', CAP,
                          PrivilegedPerf if privilege != 'none' else Owned)
        try:
            self.perf_command('disable', ctl, ack, perf, timeout=1)
        except DiagnosticError:
            self.tick()
            text = (self.directory / 'perf.stderr').read_text(errors='replace')
            recognized = any(v in text.lower() for v in (
                'no permission', 'permission denied', 'operation not permitted',
                'not supported', 'unknown tracepoint', 'event syntax error',
                'no supported events found', 'access to performance monitoring and observability operations is limited'))
            if perf.exited() and recognized:
                self.record['pmu'] = {'status': 'UNAVAILABLE', 'reason': text}
                return None
            raise
        if privilege != 'none':
            launcher = process_identity(perf.pid)
            collector = launcher
            if launcher['command'] != policy['perf_command']:
                children = Path(f'/proc/{perf.pid}/task/{perf.pid}/children').read_text().split()
                require(len(children) == 1, 'unexpected sudo process topology')
                collector = process_identity(int(children[0]))
            perf.pin_collector(collector)
            worker = process_identity(pid)
            # Save even a rejected identity, before refusing to enable counters.
            policy['observed'] = {'launcher': launcher, 'collector': collector, 'worker': worker}
            require(launcher['starttime'] == perf.start, 'perf launcher starttime changed')
            validate_perf_identity(launcher, collector, worker, policy)
        self.record['pmu'] = {'status': 'ATTACHED', 'tid': pid}
        return perf

    def observe(self, worker, input_file, source, workload, perf_executable, affinity=None,
                perf_privilege='none', perf_profile='user-v1'):
        profile_events(perf_profile)
        self.record['perf_profile'] = perf_profile
        self.fifos = ()
        token = secrets.token_hex(16)
        work = self.directory / 'work'
        work.mkdir(mode=0o700)
        self.record.update(source_sha256=source, workload=workload, token=token,
                           worker_sha256=digest(worker), input_sha256=digest(input_file),
                           runner_sha256=digest(Path(__file__)), affinity=affinity)
        with tempfile.TemporaryDirectory(prefix='xadb-window-') as local:
            local = Path(local)
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server, input_file.open('rb') as stdin:
                server.bind(str(local / 'socket'))
                server.listen(1)
                server.setblocking(False)
                env = os.environ.copy()
                env.update(XADBTEST_WINDOW_SOCKET=str(local / 'socket'), XADBTEST_WINDOW_TOKEN=token)
                command = [str(worker)]
                if affinity is not None:
                    command = ['taskset', '-c', str(affinity), *command]
                self.record['invoke_ns'] = time.monotonic_ns()
                child = self.spawn(command, work, env, stdin, 'worker', 4 * CAP)
                def accept():
                    try:
                        return server.accept()[0]
                    except BlockingIOError:
                        require(not child.exited(), 'worker exited before HELLO')
                        return None
                # A worker may spend setup time before connecting, bounded by total deadline.
                with self.wait(accept) as peer:
                    peer.setblocking(False)
                    end = time.monotonic() + 2
                    hello = self.receive(peer, end)
                    self.record['raw_hello'] = hello
                    peer_pid = struct.unpack('3i', peer.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))[0]
                    validate_hello(hello, peer_pid, child.pid, child.start, source, workload, token)
                    self.record['identity'] = {'hello': hello, 'peer_pid': peer_pid, 'starttime': child.start}
                    perf = self.prepare_perf(perf_executable, work, child.pid, local, perf_privilege, perf_profile)
                    mode = 'PMU' if perf else 'RUSAGE_ONLY'
                    self.record['mode'] = mode
                    self.send(peer, f'READY {token} {mode}', end)
                    # Warmup itself is bounded by the overall run, not an idle 2s read.
                    self.wait(lambda: bool(select.select([peer], [], [], 0)[0]))
                    end = time.monotonic() + 2
                    require(self.receive(peer, end) == f'START {token}', 'START phase/token')
                    enable_start = time.monotonic_ns()
                    if perf:
                        self.perf_command('enable', *self.fifos, perf)
                    go = time.monotonic_ns()
                    self.send(peer, f'GO {token} {mode}', end)
                    self.wait(lambda: bool(select.select([peer], [], [], 0)[0]))
                    end = time.monotonic() + 2
                    stop = self.receive(peer, end)
                    received = time.monotonic_ns()
                    self.record['raw_stop'] = stop
                    self.record['window'] = validate_stop(stop, token, go, received)
                    if perf:
                        self.perf_command('disable', *self.fifos, perf)
                    disabled = time.monotonic_ns()
                    self.record['phases'] = {'enable_sent_ns': enable_start, 'go_ns': go,
                                             'stop_received_ns': received, 'disable_ack_ns': disabled}
                    if perf:
                        # The unreaped worker reserves its PID but stays visible
                        # in /proc. Explicitly flush perf after disable instead
                        # of waiting for target disappearance (which deadlocks).
                        require(not perf.exited(), 'perf exited before requested finalization')
                        self.record['perf_finalization'] = {'signal': 'SIGINT', 'requested_ns': time.monotonic_ns()}
                        os.killpg(perf.pid, signal.SIGINT)
                    self.send(peer, f'DONE {token} {mode}', end)
                    self.wait(child.exited)
                    if perf:
                        self.wait(perf.exited, time.monotonic() + 2)
                    for owned in self.children:
                        for capture in owned.captures:
                            self.wait(lambda c=capture: c.eof)
                    status = os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOWAIT)
                    require(status.si_code == os.CLD_EXITED and status.si_status == 0, 'worker failed')
                    if perf:
                        status = os.waitid(os.P_PID, perf.pid, os.WEXITED | os.WNOWAIT)
                        self.record['perf_finalization'].update(si_code=status.si_code, si_status=status.si_status)
                        require((status.si_code == os.CLD_EXITED and status.si_status in (0, 130)) or
                                (status.si_code == os.CLD_KILLED and status.si_status == signal.SIGINT), 'perf failed')
                        self.record['pmu'] = {'status': 'COLLECTED', **parse_perf(
                            (self.directory / 'perf.stdout').read_text(), perf_profile)}
                    self.record['observed_ns'] = time.monotonic_ns()
                    require(digest(worker) == self.record['worker_sha256'], 'worker changed during invocation')
                    self.record['status'] = 'COLLECTED_DIAGNOSTIC_ONLY'
        # No acceptance or product correctness assertion is made by this collector.

    def cleanup(self):
        errors = []
        for child in reversed(self.children):
            try:
                self.record['cleanup'].append(child.finish())
            except Exception as error:
                errors.append(repr(error))
        for fd in getattr(self, 'fifos', ()):
            try:
                os.close(fd)
            except OSError as error:
                errors.append(repr(error))
        if errors or any(not r['group_gone'] or r['output_overflow'] or r['capture_errors'] for r in self.record['cleanup']):
            self.record['status'] = 'FAILED'
            self.record['cleanup_errors'] = errors


def bounded_input(path):
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        require(stat.S_ISREG(os.fstat(fd).st_mode), 'input must be a regular file')
        data = bytearray()
        while len(data) <= 65536:
            part = os.read(fd, 65537 - len(data))
            if not part:
                break
            data.extend(part)
        require(len(data) <= 65536, 'input exceeds 64KiB')
        return bytes(data)
    finally:
        os.close(fd)


def collect(worker, input_file, source, workload, output, *, deadline=30, perf=None, affinity=None,
            perf_privilege='none', perf_profile='user-v1'):
    profile_events(perf_profile)
    require(0 < deadline <= 60, 'deadline must be in (0, 60]')
    require(len(source) == 64 and all(v in '0123456789abcdef' for v in source), 'source SHA256 required')
    require(workload in ('k0', 'durable', 'synthetic'), 'unsupported workload')
    require(perf_privilege in PRIVILEGES, 'unsupported perf privilege mode')
    input_bytes = bounded_input(input_file)
    output = output.resolve()
    output.mkdir(mode=0o700)  # Never overwrite an existing result, even on retry.
    (output / 'input.bin').write_bytes(input_bytes)
    run = Runner(output, deadline)
    old_handlers = {}
    def interrupt(signum, _frame):
        run.cancel_signal = signum
    try:
        for sig in (signal.SIGINT, signal.SIGTERM):
            old_handlers[sig] = signal.signal(sig, interrupt)
        run.observe(worker.resolve(), output / 'input.bin', source, workload, perf, affinity,
                    perf_privilege, perf_profile)
    except Exception as error:
        run.record['error'] = f'{type(error).__name__}: {error}'
    finally:
        # A second cooperative signal must not interrupt owned-child cleanup.
        for sig in old_handlers:
            signal.signal(sig, signal.SIG_IGN)
        try:
            run.cleanup()
        finally:
            for sig, handler in old_handlers.items():
                signal.signal(sig, handler)
        files = {}
        for name in ('input.bin', 'worker.stdout', 'worker.stderr', 'perf.stdout', 'perf.stderr'):
            path = output / name
            if path.exists():
                files[name] = {'bytes': path.stat().st_size, 'sha256': digest(path)}
        run.record['files'] = files
        (output / 'result.json').write_text(json.dumps(run.record, indent=2) + '\n')
    return run.record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--workload', choices=('k0', 'durable', 'synthetic'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--deadline', type=float, default=30)
    parser.add_argument('--affinity', type=int)
    parser.add_argument('--perf', type=Path, default=shutil.which('perf'))
    parser.add_argument('--perf-profile', choices=EVENT_PROFILES, default='user-v1',
                        help='fixed event set; kernel counts cover only the target thread')
    parser.add_argument('--perf-privilege', choices=PRIVILEGES, default='none',
                        help='explicit opt-in: sudo drops to caller with only CAP_PERFMON for installed perf')
    args = parser.parse_args()
    result = collect(args.worker, args.input, args.source, args.workload, args.output.resolve(),
                     deadline=args.deadline, perf=args.perf, affinity=args.affinity,
                     perf_privilege=args.perf_privilege, perf_profile=args.perf_profile)
    print(json.dumps({'status': result['status'], 'mode': result.get('mode'), 'output': str(args.output)}))
    return 0 if result['status'] == 'COLLECTED_DIAGNOSTIC_ONLY' else 1


if __name__ == '__main__':
    raise SystemExit(main())
