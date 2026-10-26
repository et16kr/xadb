#!/usr/bin/env python3
"""Measurement profile B supervisor and environment judge (DEV2 PF04, PF04-PB-001).

Owns the slot gate FIFOs, the B1 pre-pair idle gate, B2 per-slot host observation
and the slot-env-v1 CPU PMU collector. The environment verdict is computed only
from these observations. Worker output and bundle evidence are read only after
the verdict digest has been sent to the controller. Controller verdict axes are
never rewritten; this tool adds environment_validity, a campaign classification
and pf04_decision. It never changes host configuration.
"""
import argparse
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
import dev2_measurement_window as window

ROOT = Path(__file__).resolve().parent.parent
PROFILE = ROOT / 'scripts/fixtures/dev2_profile_b_v1.json'
REQUEST_FIFO, REPLY_FIFO = 'controller.fifo', 'supervisor.fifo'
LINE_BYTES = 128
FIXTURE_SEED = 20261018
WORKER_CPU, SIBLING_CPU, SUPERVISOR_CPU = 2, 18, 4
MEASURED = (WORKER_CPU, SIBLING_CPU)
EVENTS = ('cycles', 'ref-cycles')
SLOT_SECONDS = 60
CAMPAIGN_SECONDS = 1800
BUDGET_SECONDS = 3 * 3600
IDLE_WINDOW_SECONDS = 10
IDLE_WAIT_SECONDS = 600
REPLACEMENT_OFFSETS = (0, 1_000_000, 2_000_000)
# Pre-GO stops caused by the environment rather than by a tool.
ENVIRONMENT_ABORTS = ('IDLE_WAIT_EXCEEDED', 'CAMPAIGN_CAP')
# (kind, mode, part, base order seed). Replays follow their original's order.
ROLES = {
    'always-control': ('acceptance', 'always', 'control', 2026102571),
    'always-audit': ('acceptance', 'always', 'audit', 2026102572),
    'always-replay': ('acceptance', 'always', 'replay', None),
    'detailed-control': ('acceptance', 'detailed', 'control', 2026102573),
    'detailed-audit': ('acceptance', 'detailed', 'audit', 2026102574),
    'detailed-replay': ('acceptance', 'detailed', 'replay', None),
    'c3-exploration': ('c3', 'detailed', 'exploration', 2026102561),
    'c3-confirmation': ('c3', 'detailed', 'confirmation', 2026102562),
    'c3-guard-off': ('c3', 'off', 'guard', 2026102563),
    'c3-guard-always': ('c3', 'always', 'guard', 2026102564),
    # Tool connection only: never a PF04 or C3 decision input.
    'connection': ('connection', 'off', 'control', 2026102600),
}


class ProfileError(Exception):
    pass


class GateError(ProfileError):
    pass


def require(condition, message, error=ProfileError):
    if not condition:
        raise error(message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def fraction(value):
    return Fraction(*value)


def packed(value):
    return None if value is None else [value.numerator, value.denominator]


def save_new(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def load_profile(path=PROFILE):
    data = Path(path).read_bytes()
    profile = json.loads(data)
    require(profile.get('protocol') == 'xadb-slot-gate-v1' and
            profile.get('environment', {}).get('profile') == 'pf04-profile-b-v1',
            'unsupported profile B file')
    return profile['environment'], hashlib.sha256(data).hexdigest()


# ---------------------------------------------------------------- host observation

def parse_stat(text):
    """Per-CPU tick counters: user nice system idle iowait irq softirq steal."""
    cpus = {}
    for line in text.splitlines():
        name, *values = line.split()
        if name.startswith('cpu') and name[3:].isdecimal():
            require(len(values) >= 8 and all(v.isdecimal() for v in values[:8]), 'malformed /proc/stat')
            number = int(name[3:])
            require(number not in cpus, 'duplicate /proc/stat CPU')
            cpus[number] = [int(v) for v in values[:8]]
    require(all(n in cpus for n in MEASURED), 'measured CPU missing from /proc/stat')
    return cpus


def delta(before, after):
    values = [b - a for a, b in zip(before, after, strict=True)]
    require(all(v >= 0 for v in values), 'CPU ticks regressed')
    return values


def busy(before, after):
    values = delta(before, after)
    require(sum(values) > 0, 'empty CPU tick window')
    return Fraction(sum(values) - values[3] - values[4], sum(values))


def idle_assessment(before, after, rules):
    """B1: measured CPUs, other-CPU mean and fixture device io_ticks over one window."""
    elapsed = after['monotonic_ns'] - before['monotonic_ns']
    require(elapsed >= rules['window_seconds'] * 10**9, 'idle window shorter than profile')
    require(set(before['cpus']) == set(after['cpus']), 'CPU set changed')
    measured = {n: busy(before['cpus'][n], after['cpus'][n]) for n in MEASURED}
    others = [busy(before['cpus'][n], after['cpus'][n]) for n in sorted(before['cpus']) if n not in MEASURED]
    require(others, 'no unmeasured CPU')
    mean = sum(others, Fraction(0)) / len(others)
    io_delta = after['io_ticks'] - before['io_ticks']
    require(io_delta >= 0, 'device io_ticks regressed')
    io = Fraction(io_delta * 10**6, elapsed)  # io_ticks are milliseconds
    passed = (all(v <= fraction(rules['maximum_measured_cpu_busy']) for v in measured.values())
              and mean <= fraction(rules['maximum_other_cpu_mean_busy'])
              and io <= fraction(rules['maximum_device_io_ticks']))
    return {'passed': passed, 'elapsed_ns': elapsed,
            'measured_busy': {f'cpu{n}': packed(v) for n, v in measured.items()},
            'other_cpu_mean_busy': packed(mean), 'device_io_ticks_fraction': packed(io)}


def tick_rule(before, after, rules):
    """B2 contention: CPU18 non-idle ticks and CPU2 nice+steal ticks over GO..DONE."""
    sibling = delta(before[SIBLING_CPU], after[SIBLING_CPU])
    worker = delta(before[WORKER_CPU], after[WORKER_CPU])
    nonidle = sibling[0] + sibling[1] + sibling[2] + sibling[5] + sibling[6] + sibling[7]
    nice_steal = worker[1] + worker[7]
    valid = (nonidle <= rules['cpu18_nonidle_ticks_maximum'] and
             nice_steal <= rules['cpu2_nice_steal_ticks_maximum'])
    return {'cpu18_nonidle_ticks': nonidle, 'cpu2_nice_steal_ticks': nice_steal,
            'status': 'VALID' if valid else 'INVALID'}


def pmu_rule(pmu, rules):
    """B2 PMU contention and the CPU2 cycles/ref ratio; quality failure is UNKNOWN."""
    if pmu is None:
        return {'status': 'UNAVAILABLE', 'cpu2_cycles_per_ref': None}
    if not pmu['all_rows_full_coverage']:
        return {'status': 'UNKNOWN', 'reason': 'PMU coverage/scale/availability', 'cpu2_cycles_per_ref': None}
    rows = pmu['rows']
    cycles2, cycles18 = rows['cpu2/cycles']['count'], rows['cpu18/cycles']['count']
    ref2 = rows['cpu2/ref-cycles']['count']
    if cycles2 == 0 or ref2 == 0:
        return {'status': 'UNKNOWN', 'reason': 'zero CPU2 denominator', 'cpu2_cycles_per_ref': None}
    share = Fraction(cycles18, cycles2)
    status = 'VALID' if share <= fraction(rules['cpu18_cycles_maximum_of_cpu2']) else 'INVALID'
    return {'status': status, 'cpu18_cycles_share': packed(share),
            'cpu2_cycles_per_ref': packed(Fraction(cycles2, ref2))}


def median(values):
    """Exact median; even counts average the two middle values."""
    ordered = sorted(values)
    middle = len(ordered) // 2
    return ordered[middle] if len(ordered) % 2 else (ordered[middle - 1] + ordered[middle]) / 2


def running_tasks(cpus=MEASURED, limit=4096):
    """Runnable tasks whose last CPU is measured, sampled once at DONE for later attribution."""
    found = []
    for process in Path('/proc').iterdir():
        if not process.name.isdecimal():
            continue
        try:
            tasks = list((process / 'task').iterdir())
        except OSError:
            continue
        for task in tasks:
            try:
                stat = (task / 'stat').read_text()
            except OSError:
                continue
            fields = stat[stat.rindex(')') + 2:].split()
            # Fields after comm start at 3: state is 3, processor is 39.
            if fields[0] == 'R' and int(fields[36]) in cpus:
                found.append({'pid': int(process.name), 'tid': int(task.name), 'processor': int(fields[36]),
                              'comm': stat[stat.index('(') + 1:stat.rindex(')')]})
                if len(found) >= limit:
                    return found
    return found


class Host:
    """Real host reads; tests substitute a scripted host."""
    def __init__(self, device):
        self.device = device

    def stat(self):
        return parse_stat(Path('/proc/stat').read_text())

    def io_ticks(self):
        fields = (Path('/sys/block') / self.device / 'stat').read_text().split()
        return int(fields[9])

    def now_ns(self):
        return time.monotonic_ns()

    def sleep(self, seconds):
        time.sleep(seconds)

    def tasks(self):
        return running_tasks()


def device_identity(path, rules):
    """Resolve the fixture disk from the mounted UUID and serial; its kernel name is not stable."""
    mount = json.loads(subprocess.check_output(
        ['findmnt', '--json', '-T', str(path), '-o', 'SOURCE,FSTYPE,UUID,MAJ:MIN'], text=True))['filesystems'][0]
    block = (Path('/sys/dev/block') / mount['maj:min']).resolve()
    disk = block.parent.name if (block / 'partition').exists() else block.name
    serial = (Path('/sys/block') / disk / 'device/serial').read_text().strip()
    identity = {'mount': mount, 'disk': disk, 'serial': serial}
    require(mount['fstype'] == 'ext4' and mount['uuid'] == rules['filesystem_uuid'] and
            serial == rules['serial'], f'fixture device differs: {identity}')
    return identity


# ---------------------------------------------------------------- slot-env-v1 PMU

def slot_env_command(perf, ctl, ack):
    return [str(perf), 'stat', '-C', '2,18', '-A', '--no-scale', '--no-big-num', '-x', ';',
            '--log-fd', '1', '-e', ','.join(EVENTS), '--delay=-1', '--control', f'fifo:{ctl},{ack}']


def validate_slot_env_command(command):
    """The fixed profile: CPUs 2 and 18 only, cycles/ref-cycles only, counting only."""
    require(command[1:3] == ['stat', '-C'] and command[3] == '2,18' and
            command[command.index('-e') + 1] == 'cycles,ref-cycles' and
            not any(a in command for a in ('-a', 'record', '--tid', '-p', '-I')),
            'slot-env-v1 accepts only CPUs 2,18 and cycles,ref-cycles counting')


def parse_slot_env(raw):
    """Literal `CPUn;count;unit;event;runtime;coverage` rows; scaled or partial rows stay visible."""
    rows = {}
    for line in raw.splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        fields = [v.strip() for v in line.split(';')]
        require(len(fields) >= 6 and fields[0] in ('CPU2', 'CPU18') and fields[3] in EVENTS,
                'unexpected slot-env-v1 row')
        key = f'{fields[0].lower()}/{fields[3]}'
        require(key not in rows, 'duplicate slot-env-v1 row')
        item = {'raw': line, 'count': None, 'runtime_ns': None, 'coverage_percent': None, 'status': 'UNAVAILABLE'}
        if fields[1] not in ('<not supported>', '<not counted>'):
            require(fields[1].isascii() and fields[1].isdecimal(), 'invalid unscaled slot-env-v1 count')
            require(fields[4].isascii() and fields[4].isdecimal(), 'invalid slot-env-v1 runtime')
            coverage = float(fields[5].rstrip('%'))
            require(0 <= coverage <= 100, 'invalid slot-env-v1 coverage')
            full = coverage == 100 and int(fields[4]) > 0
            item.update(count=int(fields[1]), runtime_ns=int(fields[4]), coverage_percent=coverage,
                        status='REPORTED_FULL_COVERAGE' if full else 'PARTIAL_OR_EMPTY')
        rows[key] = item
    require(bool(rows), 'slot-env-v1 finalization emitted no rows')
    for key in ('cpu2/cycles', 'cpu18/cycles', 'cpu2/ref-cycles', 'cpu18/ref-cycles'):
        rows.setdefault(key, {'status': 'MISSING', 'count': None})
    return {'profile': 'slot-env-v1', 'rows': rows, 'raw_unscaled': True,
            'all_rows_full_coverage': all(r['status'] == 'REPORTED_FULL_COVERAGE' for r in rows.values())}


def validate_collector_identity(launcher, collector, policy):
    """Same privilege boundary as the worker-TID collector, without a worker target."""
    uid, gid = policy['uid'], policy['gid']
    require(launcher['uids'][0] == uid and launcher['pgid'] == launcher['pid'] == launcher['sid'],
            'perf launcher must remain signalable in its owned session')
    require(collector['pgid'] == collector['sid'] == launcher['pid'] and
            (collector['pid'] == launcher['pid'] or collector['ppid'] == launcher['pid']),
            'perf escaped owned process group')
    require(collector['uids'] == [uid] * 4 and collector['gids'] == [gid] * 4 and collector['groups'] == [] and
            collector['no_new_privs'] == 1 and all(v == window.PERFMON for v in collector['caps'].values()),
            'perf capability/identity mismatch')
    require(collector['command'] == policy['perf_command'], 'perf command mismatch')


class SlotEnvCollector:
    """One disabled-at-start CPU-scope perf per slot; enabled at GO, disabled at DONE."""
    def __init__(self, directory, perf, privilege, deadline=SLOT_SECONDS + 60):
        self.directory = Path(directory)
        self.directory.mkdir(mode=0o700)
        self.runner = window.Runner(self.directory, deadline)
        self.perf, self.privilege = perf, privilege
        self.child = None
        self.record = {'profile': 'slot-env-v1', 'status': 'NOT_STARTED'}

    def start(self):
        control = self.directory / 'control'
        control.mkdir(mode=0o700)
        for name in ('ctl', 'ack'):
            os.mkfifo(control / name, 0o600)
        ctl = os.open(control / 'ctl', os.O_RDWR | os.O_NONBLOCK)
        self.runner.fifos = (ctl,)
        ack = os.open(control / 'ack', os.O_RDWR | os.O_NONBLOCK)
        self.runner.fifos = (ctl, ack)
        command = slot_env_command(self.perf, control / 'ctl', control / 'ack')
        validate_slot_env_command(command)
        command, policy = window.perf_invocation(command, self.privilege, self.directory)
        self.record.update(perf_executable={'path': str(self.perf), 'sha256': digest(self.perf)},
                           privilege=policy)
        environment = policy.get('environment', os.environ.copy())
        owned = window.PrivilegedPerf if self.privilege != 'none' else window.Owned
        self.child = self.runner.spawn(command, self.directory, environment, subprocess.DEVNULL,
                                       'perf', window.CAP, owned)
        # The first ACK proves the disabled counters are prepared before any GO.
        self.runner.perf_command('disable', ctl, ack, self.child, timeout=5)
        if self.privilege != 'none':
            launcher = window.process_identity(self.child.pid)
            collector = launcher
            if launcher['command'] != policy['perf_command']:
                children = Path(f'/proc/{self.child.pid}/task/{self.child.pid}/children').read_text().split()
                require(len(children) == 1, 'unexpected sudo process topology')
                collector = window.process_identity(int(children[0]))
            self.child.pin_collector(collector)
            policy['observed'] = {'launcher': launcher, 'collector': collector}
            require(launcher['starttime'] == self.child.start, 'perf launcher starttime changed')
            validate_collector_identity(launcher, collector, policy)
        self.record['status'] = 'PREPARED'

    def enable(self):
        self.runner.perf_command('enable', *self.runner.fifos, self.child)
        self.record['status'] = 'ENABLED'

    def disable(self):
        self.runner.perf_command('disable', *self.runner.fifos, self.child)
        self.record['status'] = 'DISABLED'

    def finish(self):
        require(not self.child.exited(), 'slot-env-v1 perf exited before finalization')
        os.killpg(self.child.pid, signal.SIGINT)
        self.runner.wait(self.child.exited, time.monotonic() + 5)
        for capture in self.child.captures:
            self.runner.wait(lambda c=capture: c.eof, time.monotonic() + 5)
        status = os.waitid(os.P_PID, self.child.pid, os.WEXITED | os.WNOWAIT)
        require((status.si_code == os.CLD_EXITED and status.si_status in (0, 130)) or
                (status.si_code == os.CLD_KILLED and status.si_status == signal.SIGINT),
                'slot-env-v1 perf failed')
        parsed = parse_slot_env((self.directory / 'perf.stdout').read_text())
        self.record.update(status='COLLECTED', **parsed)
        return parsed

    def cleanup(self):
        self.runner.cleanup()
        self.record['cleanup'] = self.runner.record['cleanup']
        if self.runner.record['status'] == 'FAILED' and self.runner.record.get('cleanup_errors'):
            self.record['cleanup_errors'] = self.runner.record['cleanup_errors']
        require(all(r['group_gone'] and not r['output_overflow'] and not r['capture_errors']
                    for r in self.record['cleanup']), 'slot-env-v1 collector cleanup failed')


# ---------------------------------------------------------------- slot gate server

class GateServer:
    """Supervisor-owned FIFOs. RDWR opens never block and keep both ends present."""
    def __init__(self, directory):
        self.directory = Path(directory)
        self.directory.mkdir(mode=0o700)
        for name in (REQUEST_FIFO, REPLY_FIFO):
            os.mkfifo(self.directory / name, 0o600)
        self.request = os.open(self.directory / REQUEST_FIFO, os.O_RDWR | os.O_NONBLOCK)
        self.reply = os.open(self.directory / REPLY_FIFO, os.O_RDWR | os.O_NONBLOCK)
        self.pending = bytearray()
        self.lines = []
        self.began = time.monotonic_ns()

    def receive(self, seconds, alive, cancelled=lambda: False):
        end = time.monotonic() + seconds
        while True:
            try:
                data = os.read(self.request, LINE_BYTES + 1)
                self.pending.extend(data)
            except BlockingIOError:
                pass
            require(len(self.pending) <= LINE_BYTES, 'gate request exceeds line bound', GateError)
            if b'\n' in self.pending:
                end_of_line = self.pending.index(b'\n')
                require(end_of_line + 1 == len(self.pending), 'unsolicited gate request bytes', GateError)
                line = self.pending[:end_of_line].decode('ascii', errors='strict')
                require(all(c.isprintable() for c in line), 'gate request is not printable ASCII', GateError)
                self.pending.clear()
                self.lines.append({'from': 'controller', 'ns': time.monotonic_ns() - self.began, 'text': line})
                return line
            require(not cancelled(), 'supervisor cancelled', GateError)
            require(alive(), 'controller exited during gate exchange', GateError)
            require(time.monotonic() < end, 'gate request deadline', GateError)
            time.sleep(.005)

    def send(self, text):
        data = (text + '\n').encode('ascii')
        require(len(data) <= LINE_BYTES, 'gate reply exceeds line bound', GateError)
        require(os.write(self.reply, data) == len(data), 'partial gate reply', GateError)
        self.lines.append({'from': 'supervisor', 'ns': time.monotonic_ns() - self.began, 'text': text})

    def close(self):
        for fd in (self.request, self.reply):
            try:
                os.close(fd)
            except OSError:
                pass


# ---------------------------------------------------------------- environment verdict

def environment_verdict(slots, rules, *, pmu_enabled, abort, termination):
    """Campaign environment validity from supervisor observations only."""
    reasons = []
    statuses = [s['status'] for s in slots]
    if pmu_enabled and len(slots) == 20 and all(s['pmu']['cpu2_cycles_per_ref'] for s in slots):
        ratios = [fraction(s['pmu']['cpu2_cycles_per_ref']) for s in slots]
        center = median(ratios)
        tolerance = fraction(rules['cpu2_cycles_per_ref_tolerance_of_campaign_median'])
        for index, ratio in enumerate(ratios):
            inside = abs(ratio - center) <= tolerance * center
            slots[index]['frequency'] = 'VALID' if inside else 'INVALID'
            if not inside:
                statuses[index] = 'INVALID'
        pairs = []
        for number in range(10):
            by_side = {slots[i]['side']: ratios[i] for i in (number * 2, number * 2 + 1)}
            # The plan's direction: baseline over candidate. The interval is not inversion-symmetric.
            pairs.append(by_side['baseline'] / by_side['candidate'])
        center_pair = median(pairs)
        low, high = (fraction(v) for v in rules['covariate_median_pair_ratio_interval'])
        covariate = {'median_pair_ratio': packed(center_pair), 'frequency_median': packed(center),
                     'status': 'VALID' if low <= center_pair <= high else 'UNKNOWN'}
    elif pmu_enabled:
        covariate = {'status': 'UNKNOWN', 'reason': 'not all twenty slots have a quality CPU2 ratio'}
    else:
        covariate = {'status': 'UNAVAILABLE'}
    for slot, status in zip(slots, statuses):
        slot['status'] = status
    if termination:
        validity = 'UNKNOWN'
        reasons.append(f'terminated: {termination}')
    elif abort:
        validity = 'INVALID'
        reasons.append(f'aborted before GO: {abort}')
    elif len(slots) != 20:
        validity = 'UNKNOWN'
        reasons.append('not all twenty slots observed')
    elif 'INVALID' in statuses:
        validity = 'INVALID'
        reasons.append('slot environment INVALID')
    elif 'UNKNOWN' in statuses or covariate['status'] == 'UNKNOWN':
        validity = 'UNKNOWN'
        reasons.append('slot PMU quality or covariate independence UNKNOWN')
    else:
        validity = 'VALID'
    sides = {side: sum(1 for s in slots if s['side'] == side and s['status'] != 'VALID')
             for side in ('baseline', 'candidate')}
    return {'environment_validity': validity, 'reasons': reasons, 'covariate': covariate,
            'side_non_valid_slots': sides, 'abort': abort, 'termination': termination}


def slot_status(ticks, pmu):
    if ticks['status'] == 'INVALID' or pmu['status'] == 'INVALID':
        return 'INVALID'
    return 'UNKNOWN' if pmu['status'] == 'UNKNOWN' else 'VALID'


# ---------------------------------------------------------------- campaign service

class Plan:
    """Execution-order slots derived from the frozen recipe, never from controller replies."""
    def __init__(self, recipe):
        self.runs, self.sides = [], []
        for pair in recipe['pairs']:
            order = [('baseline', pair['baseline_run']), ('candidate', pair['candidate_run'])]
            if pair['order'] == 'BA':
                order.reverse()
            for side, run in order:
                self.runs.append(bytes(run).hex())
                self.sides.append(side)
        require(len(self.runs) == 20 and len(set(self.runs)) == 20, 'recipe must have twenty unique slots')


def serve(gate, process, plan, rules, host, *, pmu_factory=None, stop_after=None,
          cancelled=lambda: False, verdict_path=None, record=None, ready_seconds=120):
    """Run the gate for one controller process. Returns the record with the verdict digest.

    Never stops for a B2 INVALID slot. Ends early only by ABORT before a GO
    (idle wait, campaign cap, connection stop, or a collector failure).
    """
    b1, b2 = rules['b1_pre_pair_idle'], rules['b2_slot']
    record = record if record is not None else {}
    record.update(slots=[], idle=[], go_sent=False, abort=None, termination=None, pre_go_failure=None)
    started = host.now_ns()
    alive = lambda: process.poll() is None
    pending_termination = None

    def remaining():
        return CAMPAIGN_SECONDS - (host.now_ns() - started) / 1e9

    def finish(ordinal, run):
        verdict = environment_verdict(record['slots'], b2, pmu_enabled=pmu_factory is not None,
                                      abort=record['abort'], termination=record['termination'])
        verdict.update(tool=record.get('tool'), slots=record['slots'], idle=record['idle'], go_sent=record['go_sent'],
                       pre_go_failure=record['pre_go_failure'], plan={'runs': plan.runs, 'sides': plan.sides})
        save_new(verdict_path, verdict)
        record['environment_sha256'] = digest(verdict_path)
        record['environment_validity'] = verdict['environment_validity']
        gate.send(f'ACK {ordinal} {run} {record["environment_sha256"]}')

    for ordinal in range(20):
        run = plan.runs[ordinal]
        line = gate.receive(ready_seconds if ordinal == 0 else SLOT_SECONDS, alive, cancelled)
        require(line == f'READY {ordinal} {run}', f'expected READY {ordinal}; got {line!r}', GateError)
        abort = None
        if pending_termination:
            abort, record['termination'] = 'COLLECTOR_FAILURE', pending_termination
        elif stop_after is not None and ordinal >= stop_after:
            abort = 'CONNECTION_STOP'
        elif remaining() < SLOT_SECONDS:
            abort = 'CAMPAIGN_CAP'
        elif ordinal % 2 == 0:
            abort = idle_gate(host, b1, record, remaining, cancelled)
        collector = None
        if abort is None and pmu_factory is not None:
            try:
                collector = pmu_factory(ordinal)
                collector.start()
            except Exception as error:
                reason = f'slot-env-v1 start failed: {error}'
                if record['go_sent']:
                    record['termination'] = reason
                else:
                    record['pre_go_failure'] = reason
                abort = 'COLLECTOR_FAILURE'
                if collector is not None:
                    try:
                        collector.cleanup()
                    except Exception as cleanup:
                        record['termination'] = f'{reason}; cleanup: {cleanup}'
        if abort:
            record['abort'] = abort
            gate.send(f'ABORT {ordinal} {run}')
            line = gate.receive(SLOT_SECONDS, alive, cancelled)
            require(line == f'DONE {ordinal} {run}', f'expected DONE {ordinal}; got {line!r}', GateError)
            finish(ordinal, run)
            return record
        slot = {'ordinal': ordinal, 'run': run, 'side': plan.sides[ordinal], 'position': ordinal % 2}
        try:
            # A controller lost during the idle wait must not receive a GO that marks the run started.
            require(alive(), 'controller exited before GO', GateError)
            before = host.stat()
            if collector:
                collector.enable()
            slot['go_ns'] = host.now_ns()
            gate.send(f'GO {ordinal} {run}')
            record['go_sent'] = True
            line = gate.receive(SLOT_SECONDS, alive, cancelled)
            require(line == f'DONE {ordinal} {run}', f'expected DONE {ordinal}; got {line!r}', GateError)
            slot['done_ns'] = host.now_ns()
            pmu = None
            try:
                if collector:
                    collector.disable()
            except Exception as error:
                pending_termination = f'slot-env-v1 failed at slot {ordinal}: {error}'
            after = host.stat()
            slot['tasks_at_done'] = host.tasks()
            if collector and not pending_termination:
                try:
                    pmu = collector.finish()
                except Exception as error:
                    pending_termination = f'slot-env-v1 failed at slot {ordinal}: {error}'
        finally:
            if collector:
                try:
                    collector.cleanup()
                except Exception as error:
                    pending_termination = pending_termination or f'slot-env-v1 cleanup failed: {error}'
                slot['collector'] = collector.record
        slot['ticks'] = {'before': {f'cpu{n}': before[n] for n in MEASURED},
                         'after': {f'cpu{n}': after[n] for n in MEASURED}}
        slot['tick_rule'] = tick_rule(before, after, b2)
        if pmu_factory and pmu is None:
            slot['pmu'] = {'status': 'UNKNOWN', 'reason': 'collector failed', 'cpu2_cycles_per_ref': None}
        else:
            slot['pmu'] = pmu_rule(pmu, b2)
        slot['status'] = slot_status(slot['tick_rule'], slot['pmu'])
        record['slots'].append(slot)
        if ordinal == 19:
            record['termination'] = pending_termination
            finish(ordinal, run)
            return record
        gate.send(f'ACK {ordinal} {run}')
    raise GateError('unreachable gate state')


def idle_gate(host, rules, record, remaining, cancelled):
    """B1 windows until idle, the 10-minute wait limit, or the campaign cap."""
    began = host.now_ns()
    windows = []
    record['idle'].append(windows)
    while True:
        require(not cancelled(), 'supervisor cancelled', GateError)
        waited = (host.now_ns() - began) / 1e9
        # A window that could end after the limit is not started, so GO never follows a longer wait.
        if waited + rules['window_seconds'] > rules['maximum_wait_seconds']:
            return 'IDLE_WAIT_EXCEEDED'
        if remaining() < rules['window_seconds'] + SLOT_SECONDS:
            return 'CAMPAIGN_CAP'
        before = {'monotonic_ns': host.now_ns(), 'cpus': host.stat(), 'io_ticks': host.io_ticks()}
        host.sleep(rules['window_seconds'])
        after = {'monotonic_ns': host.now_ns(), 'cpus': host.stat(), 'io_ticks': host.io_ticks()}
        assessment = idle_assessment(before, after, rules)
        windows.append(assessment)
        if assessment['passed']:
            return None


# ---------------------------------------------------------------- post-verdict classification

def frames(data):
    """Native stdout frames: u32 big-endian length then JSON response."""
    responses = []
    while data:
        require(len(data) >= 4, 'partial native frame header')
        length = int.from_bytes(data[:4], 'big')
        require(len(data) >= 4 + length, 'partial native frame')
        responses.append(json.loads(data[4:4 + length]))
        data = data[4 + length:]
    return responses


def native_facts(bundle, index):
    """Read only after the verdict digest: product defects versus tool terminations."""
    product, terminated = [], []
    for run in index['runs']:
        failure = run['failure']
        if failure and failure.startswith('NOT_RUN'):
            continue
        side = 'candidate' if run['candidate'] else 'baseline'
        if run['directory'] is None:
            terminated.append({'ordinal': run['ordinal'], 'side': side, 'reason': failure})
            continue
        native = bundle / run['directory']
        stderr = (native / 'stderr.bin').read_bytes() if (native / 'stderr.bin').is_file() else b''
        if b'panicked at' in stderr:
            product.append({'ordinal': run['ordinal'], 'side': side, 'reason': 'worker panic'})
            continue
        try:
            responses = frames((native / 'stdout.bin').read_bytes())
        except (OSError, ValueError, ProfileError) as error:
            responses = []
            failure = failure or f'unreadable native stdout: {error}'
        results = [r.get('observation', {}).get('result') for r in responses]
        errors = [r['observation']['output'].get('error') for r in responses
                  if r.get('observation', {}).get('result') == 'performance']
        if any(result not in ('ready', 'performance') for result in results):
            product.append({'ordinal': run['ordinal'], 'side': side, 'reason': f'worker error response {results}'})
        elif any(error is not None for error in errors):
            product.append({'ordinal': run['ordinal'], 'side': side, 'reason': 'worker Output.error'})
        elif failure:
            # Nice mismatch, transport, timeout or a signalled worker without a panic report.
            terminated.append({'ordinal': run['ordinal'], 'side': side, 'reason': failure})
    return {'product_defects': product, 'terminated_runs': terminated}


def classify(kind, facts):
    """Campaign termination table. Product defects and correctness FAIL take precedence."""
    comparison = facts.get('comparison') or {}
    product = facts.get('product_defects') or []
    if kind == 'connection':
        connected = (facts['go_sent'] and facts.get('abort') == 'CONNECTION_STOP' and facts.get('acknowledged')
                     and not (facts.get('termination') or facts.get('terminated_runs') or product))
        return {'status': 'TOOL_CONNECTED' if connected else 'TOOL_FAILED',
                'reason': 'CONNECTION_ONLY_NOT_A_PERFORMANCE_VERDICT', 'replaceable': False}
    if kind == 'c3':
        if not facts['go_sent']:
            return {'status': 'INCOMPLETE', 'reason': 'NOT_RUN', 'replaceable': False}
        # Unattributed correctness FAIL rejects the candidate: rejection is the safe direction.
        if any(d['side'] == 'candidate' for d in product) or (
                comparison.get('correctness') == 'FAIL' and not any(d['side'] == 'baseline' for d in product)):
            return {'status': 'REJECTED', 'reason': 'CANDIDATE_PRODUCT_DEFECT', 'replaceable': False}
        if product:
            return {'status': 'INCOMPLETE', 'reason': 'BASELINE_FAILURE', 'replaceable': False}
        if facts.get('termination') or facts.get('terminated_runs') or facts.get('abort') or facts.get('slots', 20) != 20:
            return {'status': 'INCOMPLETE', 'reason': 'TERMINATED', 'replaceable': False}
        # Environment is descriptive here; the controller comparison decides C3.
        return {'status': 'COMPLETED', 'reason': 'CONTROLLER_VERDICT', 'replaceable': False}
    if not facts['go_sent'] and facts.get('abort') not in ENVIRONMENT_ABORTS:
        reason = facts.get('pre_go_failure') or facts.get('termination') or 'TOOL_FAILURE_BEFORE_GO'
        return {'status': 'NOT_RUN', 'reason': reason, 'replaceable': True}
    if product:
        return {'status': 'FAIL', 'reason': 'PRODUCT_DEFECT', 'replaceable': False}
    if comparison.get('correctness') == 'FAIL':
        return {'status': 'FAIL', 'reason': 'CORRECTNESS_FAIL', 'replaceable': False}
    if facts.get('termination') or facts.get('terminated_runs'):
        return {'status': 'INCOMPLETE', 'reason': 'TERMINATED', 'replaceable': False}
    if facts.get('abort') in ENVIRONMENT_ABORTS:
        return {'status': 'ENVIRONMENT_INVALID', 'reason': facts['abort'], 'replaceable': True}
    require(facts.get('abort') is None, f'unclassified abort {facts.get("abort")}')
    if facts['environment_validity'] != 'VALID':
        return {'status': 'ENVIRONMENT_INVALID', 'reason': f'ENVIRONMENT_{facts["environment_validity"]}',
                'replaceable': True}
    if comparison.get('validity') != 'VALID' or comparison.get('guardrails') == 'UNKNOWN':
        return {'status': 'INCOMPLETE', 'reason': 'INCONCLUSIVE_COMPARISON', 'replaceable': False}
    if comparison.get('guardrails') == 'FAIL':
        return {'status': 'FAIL', 'reason': 'GUARDRAIL_FAIL', 'replaceable': False}
    return {'status': 'PASS', 'reason': 'GUARDRAIL_PASS', 'replaceable': False}


# ---------------------------------------------------------------- ledger and decision

def new_ledger(profile_sha256, predecessors=()):
    """The three-hour cap spans S1, S3 and S5: a new session starts from closed predecessors' use."""
    carried = sum(p['used_seconds'] for p in predecessors)
    return {'schema_version': 1, 'profile_sha256': profile_sha256, 'budget_seconds': BUDGET_SECONDS,
            'used_seconds': carried, 'carried_seconds': carried, 'predecessors': list(predecessors),
            'closed': False, 'roles': {role: {'attempts': [], 'final': None} for role in ROLES}}


def predecessor(path, expected_sha256):
    """A closed earlier ledger, bound by hash, whose total use carries into the next session."""
    require(digest(path) == expected_sha256, 'predecessor ledger identity mismatch')
    ledger = json.loads(Path(path).read_text())
    require(ledger.get('closed') is True, 'predecessor ledger must be closed before its budget is carried')
    require(ledger['budget_seconds'] == BUDGET_SECONDS, 'predecessor ledger budget differs')
    return {'path': str(Path(path).resolve()), 'sha256': expected_sha256, 'used_seconds': ledger['used_seconds']}


def check_total(ledger):
    """Used time must equal the carried total plus this ledger's own recorded attempts."""
    own = sum(a['seconds'] for entry in ledger['roles'].values() for a in entry['attempts'])
    require(abs(ledger['used_seconds'] - ledger.get('carried_seconds', 0) - own) < 1e-6,
            'ledger budget total differs from its attempts')


def admit(ledger, role):
    """Refuse finalized roles, unmet prerequisites, exhausted replacements and budget."""
    require(role in ROLES, f'unknown role {role}')
    check_total(ledger)
    kind, mode, part, seed = ROLES[role]
    entry = ledger['roles'][role]
    require(not ledger.get('closed'), 'ledger is closed; its budget was carried to a later session')
    require(entry['final'] is None, f'{role} is final and is never rerun')
    require(ledger['used_seconds'] < ledger['budget_seconds'], 'BUDGET_EXHAUSTED: three-hour measurement budget')
    if kind == 'acceptance' and part in ('audit', 'replay'):
        prior = f'{mode}-control' if part == 'audit' else f'{mode}-audit'
        final = ledger['roles'][prior]['final']
        require(final is not None and final['status'] == 'PASS', f'{role} requires {prior} final PASS')
    if kind == 'c3' and role != 'c3-exploration':
        # S3 judges each step's controller axes; the ledger only keeps the frozen order.
        prior = 'c3-exploration' if role == 'c3-confirmation' else 'c3-confirmation'
        final = ledger['roles'][prior]['final']
        require(final is not None and final['status'] == 'COMPLETED', f'{role} requires {prior} COMPLETED')
    attempt = len(entry['attempts'])
    # Acceptance roles may be replaced twice; C3 and the tool connection run once.
    maximum = len(REPLACEMENT_OFFSETS) if kind == 'acceptance' else 1
    require(attempt < maximum, f'{role} has no attempt left')
    return {'attempt': attempt, 'order_seed': None if seed is None else seed + REPLACEMENT_OFFSETS[attempt]}


def settle(ledger, role, classification, seconds, summary):
    kind = ROLES[role][0]
    entry = ledger['roles'][role]
    require(not ledger.get('closed'), 'ledger is closed; its budget was carried to a later session')
    entry['attempts'].append({'classification': classification, 'seconds': seconds, **summary})
    ledger['used_seconds'] += seconds
    if kind == 'connection':
        return None
    if not classification['replaceable'] or kind != 'acceptance':
        entry['final'] = {'status': classification['status'], 'reason': classification['reason']}
    elif len(entry['attempts']) == len(REPLACEMENT_OFFSETS):
        entry['final'] = {'status': 'INCOMPLETE', 'reason': 'ENVIRONMENT'}
    return entry['final']


def mode_decision(ledger, mode):
    """PF04 K0 table for one mode; FAIL takes precedence over INCOMPLETE."""
    roles = ledger['roles']
    control, audit, replay = (roles[f'{mode}-{p}']['final'] for p in ('control', 'audit', 'replay'))
    if control is None:
        return {'decision': 'INCOMPLETE', 'reason': 'CONTROL_NOT_RUN'}
    if control['status'] != 'PASS':
        if control['status'] == 'FAIL' and control['reason'] != 'GUARDRAIL_FAIL':
            return {'decision': 'FAIL', 'reason': f'CONTROL_{control["reason"]}'}
        reason = 'CONTROL_GUARDRAIL_FAIL' if control['status'] == 'FAIL' else f'CONTROL_{control["reason"]}'
        return {'decision': 'INCOMPLETE', 'reason': reason}
    for name, final in (('AUDIT', audit), ('REPLAY', replay)):
        if final is None:
            return {'decision': 'INCOMPLETE', 'reason': f'{name}_NOT_RUN'}
        if final['status'] == 'FAIL':
            return {'decision': 'FAIL', 'reason': f'{name}_{final["reason"]}'}
        if final['status'] != 'PASS':
            return {'decision': 'INCOMPLETE', 'reason': f'{name}_{final["reason"]}'}
    return {'decision': 'PASS', 'reason': 'CONTROL_AUDIT_REPLAY_PASS'}


def pf04_decision(ledger):
    modes = {mode: mode_decision(ledger, mode) for mode in ('always', 'detailed')}
    decisions = [m['decision'] for m in modes.values()]
    overall = 'PASS' if decisions == ['PASS', 'PASS'] else 'FAIL' if 'FAIL' in decisions else 'INCOMPLETE'
    return {'pf04_k0': overall, 'modes': modes,
            'scope': 'new source only; the original e3 PF04 FAILs are unchanged'}


# ---------------------------------------------------------------- recipes

def paired_orders(seed):
    """Port of the controller's frozen Fisher-Yates over five AB and five BA blocks."""
    orders = ['AB'] * 5 + ['BA'] * 5
    for index in range(9, 0, -1):
        seed = (seed * 6364136223846793005 + 1442695040888963407) % 2**64
        other = seed % (index + 1)
        orders[index], orders[other] = orders[other], orders[index]
    return orders


def pairs(campaign_id, order_seed):
    ids = [list(hashlib.sha256(campaign_id + bytes([slot])).digest()[:16]) for slot in range(20)]
    return [{'baseline_run': ids[i * 2], 'candidate_run': ids[i * 2 + 1], 'order': order}
            for i, order in enumerate(paired_orders(order_seed))]


def build_recipe(template, part, mode, order_seed, campaign_id):
    """Control is an Off/Off Exploration of one build; audit is Off/mode. No new recipe kind."""
    recipe = json.loads(json.dumps(template))
    require(recipe['fixture']['seed'] == FIXTURE_SEED and recipe['kind'] == 'instrumentation_audit',
            'template is not the frozen instrumentation fixture')
    recipe['campaign_id'] = list(campaign_id)
    recipe['order_seed'] = order_seed
    recipe['pairs'] = pairs(campaign_id, order_seed)
    if part == 'control':
        recipe['kind'] = 'exploration'
        recipe['candidate']['mode'] = 'off'
        recipe['hypothesis'] = ('Profile B Off/Off control of one release build: detects only >15% disturbance and '
                                'whole-procedure A/A behavior; never provenance for any confirmation.')
    else:
        recipe['candidate']['mode'] = mode
    return recipe


# ---------------------------------------------------------------- session CLI

C3_RECIPES = {'c3-exploration': ('exploration', 'detailed'), 'c3-confirmation': ('confirmation', 'detailed'),
              'c3-guard-off': ('exploration', 'off'), 'c3-guard-always': ('exploration', 'always')}


def check_c3_recipe(role, recipe):
    """A C3 recipe compares two builds in the role's one mode; the controller checks the rest."""
    kind, mode = C3_RECIPES[role]
    require(recipe['kind'] == kind and recipe['baseline']['mode'] == recipe['candidate']['mode'] == mode,
            f'{role} requires a {kind} recipe with both sides in {mode} mode')
    require(recipe['baseline']['worker_sha256'] != recipe['candidate']['worker_sha256'],
            'C3 recipe must compare two different builds')


class Session:
    def __init__(self, directory):
        self.directory = Path(directory).resolve()
        self.inputs = json.loads((self.directory / 'inputs.json').read_text())
        self.ledger_path = self.directory / 'ledger.json'

    def ledger(self):
        return json.loads(self.ledger_path.read_text())

    def write_ledger(self, ledger):
        temporary = self.ledger_path.with_suffix('.tmp')
        temporary.write_text(json.dumps(ledger, indent=2) + '\n')
        os.replace(temporary, self.ledger_path)

    def verify_inputs(self):
        for name, item in self.inputs['files'].items():
            require(digest(item['path']) == item['sha256'], f'changed frozen input: {name}')
        _, profile_sha256 = load_profile()
        require(profile_sha256 == self.inputs['profile_sha256'], 'profile B file changed')


def initialize(directory, controller, worker, receipt, config, perf_privilege, *, budget_from=(), new_budget=False):
    """A fresh three-hour budget is an explicit choice; later sessions carry closed ledgers."""
    require(bool(budget_from) != bool(new_budget), 'give either --new-budget or --budget-from')
    predecessors = [predecessor(path, sha256) for path, sha256 in budget_from]
    environment, profile_sha256 = load_profile()
    directory = Path(directory).resolve()
    identity = device_identity(directory.parent, environment['device'])
    directory.mkdir(mode=0o700)
    files = {name: {'path': str(Path(path).resolve()), 'sha256': digest(path)}
             for name, path in (('controller', controller), ('worker', worker), ('receipt', receipt),
                                ('config', config))}
    receipt_value = json.loads(Path(receipt).read_text())
    require(receipt_value['worker_sha256'] == files['worker']['sha256'] and
            receipt_value['config_sha256'] == files['config']['sha256'], 'receipt does not bind worker/config')
    save_new(directory / 'inputs.json', {'schema_version': 1, 'profile_sha256': profile_sha256, 'files': files,
                                          'perf_privilege': perf_privilege, 'device': identity,
                                          'created_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())})
    save_new(directory / 'ledger.json', new_ledger(profile_sha256, predecessors))
    return directory


def close(directory):
    """Close a session so its total use can carry into the next one; it starts no new campaign."""
    session = Session(directory)
    lock = session.directory / 'lock'
    # The same lock as a campaign: no attempt can settle after the total is carried.
    os.close(os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600))
    try:
        ledger = session.ledger()
        check_total(ledger)
        ledger['closed'] = True
        session.write_ledger(ledger)
    finally:
        lock.unlink()
    return {'ledger': str(session.ledger_path), 'sha256': digest(session.ledger_path),
            'used_seconds': ledger['used_seconds']}


def tool_identity():
    """The supervisor and judge code that produced an environment verdict."""
    return {name: digest(path) for name, path in (('dev2_profile_b.py', Path(__file__)),
                                                   ('dev2_measurement_window.py', Path(window.__file__)))}


def clean_environment():
    environment = {k: v for k, v in os.environ.items() if not k.startswith(('XADBTEST_', 'CARGO_PROFILE_'))}
    environment['PYTHONDONTWRITEBYTECODE'] = '1'
    return environment


def campaign(session_directory, role, *, recipe_path=None, builds=None, host=None, pmu=True):
    """One attempt of one role, from admission to ledger settlement."""
    session = Session(session_directory)
    lock = session.directory / 'lock'
    fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    os.close(fd)
    try:
        return _campaign(session, role, recipe_path, builds, host, pmu)
    finally:
        lock.unlink()


def _campaign(session, role, recipe_path, builds, host, pmu):
    require(os.sched_getaffinity(0) == {SUPERVISOR_CPU}, 'supervisor must run on CPU4 only')
    require(os.getpriority(os.PRIO_PROCESS, 0) == 0, 'supervisor and controller must run at nice 0')
    session.verify_inputs()
    environment, profile_sha256 = load_profile()
    ledger = session.ledger()
    admission = admit(ledger, role)
    kind, mode, part, _ = ROLES[role]
    c3_recipe = None
    if kind == 'c3':
        # Input checks precede the attempt; a wrong argument is not a campaign.
        require(recipe_path is not None and builds is not None, 'C3 roles take a prepared recipe and builds')
        c3_recipe = json.loads(Path(recipe_path).read_text())
        require(c3_recipe['order_seed'] == admission['order_seed'], 'C3 recipe order seed differs from the plan')
        check_c3_recipe(role, c3_recipe)
    name = f'{role}-a{admission["attempt"]}'
    directory = session.directory / 'campaigns' / name
    directory.mkdir(parents=True)
    began = time.monotonic()
    gate = stdout = stderr = None
    # Installed before any process exists, so a signal can only cancel a recorded attempt.
    cancel = {'signal': None}
    handlers = {sig: signal.signal(sig, lambda signum, _f: cancel.update(signal=signum))
                for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        files = session.inputs['files']
        controller = files['controller']['path']
        campaign_id = uuid.uuid4().bytes
        gate_arguments = ['--gate', str(directory / 'gate'), '--gate-profile', str(PROFILE),
                          '--gate-profile-sha256', profile_sha256]
        if part == 'replay':
            audit = ledger['roles'][f'{mode}-audit']['attempts'][-1]
            original = directory / 'original'
            shutil.copytree(audit['bundle'], original, copy_function=os.link)
            recipe = json.loads((original / 'recipe.json').read_text())
            recipe['pairs'] = pairs(campaign_id, recipe['order_seed'])
            recipe_sha256 = audit['recipe_sha256']
            argv = [str(original / 'controller'), 'replay-performance', '--directory', str(original),
                    '--recipe-sha256', recipe_sha256, '--campaign-id', campaign_id.hex(),
                    '--artifacts', str(directory / 'artifacts')]
            verifier = None  # the new bundle's retained controller verifies it
        else:
            if kind == 'c3':
                recipe, sides = c3_recipe, builds
            else:
                template_path = directory / 'template.recipe.json'
                subprocess.run([controller, 'prepare-performance', '--receipt', files['receipt']['path'],
                                '--receipt-sha256', files['receipt']['sha256'], '--campaign-id', campaign_id.hex(),
                                '--seed', str(FIXTURE_SEED), '--mode', 'always', '--output', str(template_path)],
                               check=True, capture_output=True, env=clean_environment())
                template = json.loads(template_path.read_text())
                # The Python port must reproduce the controller's own frozen order.
                require([p['order'] for p in template['pairs']] == paired_orders(FIXTURE_SEED),
                        'paired order port differs from the controller')
                recipe = build_recipe(template, part, mode, admission['order_seed'], campaign_id)
                sides = {side: {'worker': files['worker']['path'], 'receipt': files['receipt']['path'],
                                'config': files['config']['path']} for side in ('baseline', 'candidate')}
            recipe_file = directory / 'recipe.json'
            save_new(recipe_file, recipe)
            recipe_sha256 = digest(recipe_file)
            argv = [controller, 'run-performance', '--recipe', str(recipe_file), '--recipe-sha256', recipe_sha256]
            for side in ('baseline', 'candidate'):
                argv += [f'--{side}-worker', sides[side]['worker'], f'--{side}-receipt', sides[side]['receipt'],
                         f'--{side}-config', sides[side]['config']]
            argv += ['--artifacts', str(directory / 'artifacts')]
            verifier = controller
        argv = ['taskset', '-c', str(WORKER_CPU), *argv, *gate_arguments]
        plan = Plan(recipe)
        # Re-resolved per campaign: a reboot between campaigns may rename the same disk.
        device = device_identity(session.directory, environment['device'])
        host = host or Host(device['disk'])
        privilege = session.inputs['perf_privilege']
        pmu_factory = None
        if pmu and privilege != 'unavailable':
            perf = Path('/usr/bin/perf')
            pmu_factory = lambda ordinal: SlotEnvCollector(directory / 'slots' / f'{ordinal:02}', perf, privilege)
            (directory / 'slots').mkdir()
        record = {'schema_version': 1, 'role': role, 'attempt': admission['attempt'], 'device': device,
              'tool': tool_identity(),
                  'order_seed': recipe['order_seed'],
                  'campaign_id': campaign_id.hex(), 'recipe_sha256': recipe_sha256, 'argv': argv,
                  'profile_sha256': profile_sha256, 'pmu': 'slot-env-v1' if pmu_factory else 'UNAVAILABLE'}
        gate = GateServer(directory / 'gate')
        stdout, stderr = (directory / 'controller.stdout').open('xb'), (directory / 'controller.stderr').open('xb')
        process = subprocess.Popen(argv, cwd=directory, env=clean_environment(), stdout=stdout, stderr=stderr,
                                   start_new_session=True)
    except Exception as error:
        # A supervisor failure before the gate is the plan's NOT_RUN and uses one attempt.
        for sig, handler in handlers.items():
            signal.signal(sig, handler)
        for owned in (gate, stdout, stderr):
            if owned is not None:
                owned.close()
        failure = f'supervisor before gate: {type(error).__name__}: {error}'
        facts = {'go_sent': False, 'pre_go_failure': failure, 'abort': None, 'termination': None, 'slots': 0,
                 'acknowledged': False, 'environment_validity': 'UNKNOWN', 'comparison': None}
        classification = classify(kind, facts)
        seconds = time.monotonic() - began
        summary = {'campaign': name, 'campaign_id': None, 'order_seed': admission['order_seed'], 'bundle': None,
                   'environment_sha256': None, 'environment_validity': 'UNKNOWN', 'controller': None}
        save_new(directory / 'campaign.json', {'schema_version': 1, 'role': role, 'attempt': admission['attempt'],
                                               'tool': tool_identity(), 'facts': facts,
                                               'classification': classification, 'seconds': seconds,
                                               'summary': summary})
        ledger = session.ledger()
        final = settle(ledger, role, classification, seconds, summary)
        session.write_ledger(ledger)
        return {'classification': classification, 'final': final, 'summary': summary}
    try:
        try:
            serve(gate, process, plan, environment, host, pmu_factory=pmu_factory,
                  stop_after=2 if kind == 'connection' else None,
                  cancelled=lambda: cancel['signal'] is not None,
                  verdict_path=directory / 'environment-verdict.json', record=record)
        except Exception as error:  # every supervisor failure is retained, never retried here
            record['termination'] = f'{type(error).__name__}: {error}'
        if not (directory / 'environment-verdict.json').exists():
            # The controller withholds analysis without the final ACK; keep the observations.
            verdict = environment_verdict(record['slots'], environment['b2_slot'],
                                          pmu_enabled=pmu_factory is not None, abort=record['abort'],
                                          termination=record['termination'])
            save_new(directory / 'environment-verdict.json',
                     verdict | {'tool': record['tool'], 'slots': record['slots'], 'idle': record['idle'],
                                'acknowledged': False})
        try:
            process.wait(timeout=300)
        except subprocess.TimeoutExpired:
            record['termination'] = record['termination'] or 'controller analysis deadline'
    finally:
        for sig, handler in handlers.items():
            signal.signal(sig, handler)
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        gate.close()
        stdout.close()
        stderr.close()
    leftovers = owned_leftovers(directory)
    for pid in leftovers:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if leftovers:
        record['termination'] = record['termination'] or f'leftover owned processes {leftovers}'
    save_new(directory / 'gate-supervisor-transcript.json', gate.lines)
    # Only now, after the verdict digest was sent or the gate failed, read controller evidence.
    record['controller_exit'] = process.returncode
    report = None
    if process.returncode in (0, 1, 2):
        report = json.loads((directory / 'controller.stdout').read_text())
        checker = verifier or Path(report['directory']) / 'controller'
        checked = subprocess.run([checker, 'verify-performance', '--directory', report['directory'],
                                  '--recipe-sha256', report['recipe_sha256']], capture_output=True,
                                 env=clean_environment())
        if checked.returncode not in (0, 1, 2) or json.loads(checked.stdout)['comparison'] != report['comparison']:
            record['termination'] = record['termination'] or 'saved campaign reanalysis differs'
    elif record['go_sent'] and not record['termination']:
        record['termination'] = f'controller exit {process.returncode}'
    bundles = sorted((directory / 'artifacts').glob('performance-*')) if (directory / 'artifacts').is_dir() else []
    facts = {'go_sent': record['go_sent'], 'pre_go_failure': record['pre_go_failure'], 'abort': record['abort'],
             'termination': record['termination'], 'slots': len(record['slots']),
             'acknowledged': record.get('environment_sha256') is not None,
             'environment_validity': record.get('environment_validity', 'UNKNOWN'),
             'comparison': report and report['comparison']}
    if len(bundles) > 1:
        facts['termination'] = facts['termination'] or 'more than one bundle in a campaign'
    elif bundles:
        try:
            index = json.loads((bundles[0] / 'index.json').read_text())
            facts.update(native_facts(bundles[0], index))
        except (OSError, ValueError, KeyError, ProfileError) as error:
            # Retained as a termination so the attempt is still settled in the ledger.
            index = {}
            facts['termination'] = facts['termination'] or f'bundle evidence unreadable: {error}'
        if index.get('gate') and index['gate']['environment_sha256'] not in (None, record.get('environment_sha256')):
            facts['termination'] = 'bundle verdict digest differs from the supervisor verdict'
    classification = classify(kind, facts)
    seconds = time.monotonic() - began
    summary = {'campaign': name, 'campaign_id': campaign_id.hex(), 'order_seed': recipe['order_seed'],
               'recipe_sha256': recipe_sha256, 'bundle': str(bundles[0]) if bundles else None,
               'environment_sha256': record.get('environment_sha256'),
               'environment_validity': facts['environment_validity'],
               'controller': report and {k: report['comparison'][k] for k in
                                         ('validity', 'correctness', 'performance_effect', 'guardrails')},
               'position': report and position_effect(recipe, report['comparison'])}
    record.update(facts=facts, classification=classification, seconds=seconds)
    save_new(directory / 'campaign.json', record | {'summary': summary})
    ledger = session.ledger()
    final = settle(ledger, role, classification, seconds, summary)
    session.write_ledger(ledger)
    return {'classification': classification, 'final': final, 'summary': summary}


def owned_leftovers(directory):
    """Processes still executing a binary copied under this campaign directory."""
    found = []
    for item in Path('/proc').iterdir():
        if not item.name.isdecimal():
            continue
        try:
            executable = os.readlink(item / 'exe')
        except OSError:
            continue
        if executable.startswith(str(directory) + '/'):
            found.append(int(item.name))
    return found


def position_effect(recipe, comparison):
    """Descriptive only: median ratio for AB pairs (baseline first) and BA pairs."""
    ratios = comparison.get('ratios') or []
    if len(ratios) != 10:
        return None
    groups = {'AB': [], 'BA': []}
    for pair, ratio in zip(recipe['pairs'], ratios):
        groups[pair['order']].append(Fraction(ratio['numerator'], ratio['denominator']))
    return {order: packed(median(values)) for order, values in groups.items() if values}


def preflight(paths):
    """Re-apply the B2 tick rule to preserved slot host windows. Descriptive; not a measurement."""
    environment, _ = load_profile()
    rows = []
    for path in paths:
        host = json.loads(Path(path).read_text())
        before = {int(k[3:]): v for k, v in host['before']['cpus'].items()}
        after = {int(k[3:]): v for k, v in host['after']['cpus'].items()}
        rows.append({'path': str(path), **tick_rule(before, after, environment['b2_slot'])})
    valid = sum(1 for r in rows if r['status'] == 'VALID')
    return {'scope': 'descriptive B2 tick rule over preserved host windows; no remeasurement',
            'total': len(rows), 'valid': valid, 'invalid': [r for r in rows if r['status'] != 'VALID']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    init = commands.add_parser('init')
    init.add_argument('--session', type=Path, required=True)
    for name in ('controller', 'worker', 'receipt', 'config'):
        init.add_argument(f'--{name}', type=Path, required=True)
    init.add_argument('--perf-privilege', choices=('sudo-cap-perfmon', 'unavailable'), required=True)
    init.add_argument('--budget-from', nargs=2, action='append', default=[], metavar=('LEDGER', 'SHA256'),
                      help='closed earlier ledger whose use counts toward the shared three-hour budget')
    init.add_argument('--new-budget', action='store_true', help='only for the first measurement session')
    run = commands.add_parser('campaign')
    run.add_argument('--session', type=Path, required=True)
    run.add_argument('--role', choices=ROLES, required=True)
    run.add_argument('--recipe', type=Path)
    run.add_argument('--builds', type=Path, help='C3 only: JSON {baseline,candidate: {worker,receipt,config}}')
    finish = commands.add_parser('close')
    finish.add_argument('--session', type=Path, required=True)
    decide = commands.add_parser('decide')
    decide.add_argument('--session', type=Path, required=True)
    check = commands.add_parser('preflight')
    check.add_argument('hosts', type=Path, nargs='+')
    args = parser.parse_args()
    if args.command == 'init':
        print(initialize(args.session, args.controller, args.worker, args.receipt, args.config, args.perf_privilege,
                         budget_from=args.budget_from, new_budget=args.new_budget))
    elif args.command == 'close':
        print(json.dumps(close(args.session), indent=2))
    elif args.command == 'campaign':
        os.sched_setaffinity(0, {SUPERVISOR_CPU})
        builds = json.loads(args.builds.read_text()) if args.builds else None
        print(json.dumps(campaign(args.session, args.role, recipe_path=args.recipe, builds=builds), indent=2))
    elif args.command == 'decide':
        print(json.dumps(pf04_decision(Session(args.session).ledger()), indent=2))
    else:
        print(json.dumps(preflight(args.hosts), indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
