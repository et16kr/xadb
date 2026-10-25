#!/usr/bin/env python3
"""Strict retained-event reconstruction; shared-disk overlap is not WAL ownership."""
from collections import Counter
import re

KINDS = ('FS_IN', 'FS_OUT', 'WAIT_IN', 'WAIT_OUT', 'PHASE', 'ISSUE', 'COMPLETE', 'REQUEUE', 'SEQ')
WIDTHS = (5, 6, 5, 6, 6, 6, 6, 3)
WIDTHS_V2 = (5, 6, 5, 6, 6, 8, 9, 3, 11)
PHASES = {f'tracepoint:jbd2:jbd2_{s}' for s in ('start_commit', 'commit_locking', 'commit_flushing', 'commit_logging', 'end_commit')}


def require(value, message):
    if not value:
        raise ValueError(message)


def union(intervals):
    result = []
    for start, end in sorted(intervals):
        require(start < end, 'empty/reversed interval')
        if result and start <= result[-1][1]:
            result[-1] = (result[-1][0], max(end, result[-1][1]))
        else:
            result.append((start, end))
    return result


def overlap(left, right):
    left, right = union(left), union(right)
    total = 0
    j = 0
    for start, end in left:
        while j < len(right) and right[j][1] <= start:
            j += 1
        k = j
        while k < len(right) and right[k][0] < end:
            total += max(0, min(end, right[k][1]) - max(start, right[k][0]))
            k += 1
    return total


def parse(raw):
    require(len(raw.encode()) <= 64 * 1024 * 1024, 'raw bound')
    counts = {}
    rows = []
    disabled = []
    headers = [line for line in raw.splitlines() if line.startswith("SCHEMA")]
    require(not headers or headers == ["SCHEMA 2"], "schema header")
    schema = 2 if headers else None
    for line in raw.splitlines():
        if not line.strip():
            continue
        if line.startswith('SCHEMA'):
            continue
        match = re.fullmatch(r'@counts\[([0-8])\]: (\d+)', line)
        if match:
            key, count = map(int, match.groups())
            require(key not in counts and count > 0, 'duplicate/empty count')
            counts[key] = count
            continue
        parts = line.split()
        if parts[0] == 'DISABLED':
            require(len(parts) == 2 and parts[1].isdigit(), 'disabled syntax')
            disabled.append(int(parts[1])); continue
        require(parts[0] in KINDS, 'unknown record/loss/map warning')
        kind = KINDS.index(parts[0])
        widths = WIDTHS_V2 if schema == 2 else WIDTHS
        require(kind < len(widths) and len(parts) == widths[kind], 'record shape')
        values = []
        for index, value in enumerate(parts[1:], 1):
            if kind == 4 and index == 3:
                require(value in PHASES, 'unknown journal phase')
                values.append(value)
            else:
                require(re.fullmatch(r'-?\d+', value), 'noninteger record')
                number = int(value)
                require(0 <= number < 2**64 or kind in (1, 3) and index == 5 and -4095 <= number < 0, 'integer bound')
                values.append(number)
        rows.append((kind, *values))
        require(len(rows) <= 200000, 'event bound')
    require(len(disabled) == 1, 'disable ACK missing/duplicate')
    observed = Counter(row[0] for row in rows)
    require(dict(observed) == counts, 'kernel count/output mismatch')
    require(all(row[1] <= disabled[0] for row in rows), 'event after disable (no barrier assumed)')
    return sorted(rows, key=lambda r: r[1]), counts


def request_provenance(rows, end):
    """Kernel 7.0 flush sequence: DONE restores the request then completes it.

    A marker is evidence of that call path, not a heuristic based on zero bytes.
    Keep every COMPLETE in a census, including partial/invalid physical events.
    """
    active, pending, tainted = {}, {}, {}
    requests, unknown = [], []
    seen = set()
    census = dict(physical=0, logical=0, unknown=0, total=0)
    for kind, stamp, *fields in rows:
        if kind < 5:
            continue
        pointer = fields[0]
        require((pointer, stamp) not in seen, 'ambiguous request timestamp')
        seen.add((pointer, stamp))
        marker = pending.pop(pointer, None)
        if marker is not None:
            when, flags, command, length, restored = marker
            if kind == 6:
                _, error, completed, remaining, cmd, rq_flags, end_io = fields
                census['total'] += 1
                if (not error and completed == remaining == length == 0
                        and cmd == command and rq_flags == flags & ~2
                        and end_io == restored and pointer not in active):
                    census['logical'] += 1
                else:
                    census['unknown'] += 1
                    unknown.append(('logical_completion_mismatch', when, stamp))
                    tainted[pointer] = when
                    if pointer in active:
                        unknown.append(('logical_with_open_physical', active.pop(pointer)[0], stamp))
                continue
            unknown.append(('interrupted_logical_completion', when, stamp))
            tainted[pointer] = when
        if kind == 5:
            conflict = tainted.pop(pointer, None)  # Explicit new physical generation.
            if conflict is not None:
                unknown.append(("abandoned_tainted_generation", conflict, stamp))
        elif pointer in tainted:
            if kind == 6:
                census['total'] += 1
                census['unknown'] += 1
                unknown.append(('tainted_logical_generation', tainted[pointer], stamp))
                if pointer in active:
                    unknown.append(('tainted_open_physical', active.pop(pointer)[0], stamp))
            continue
        if kind == 8:
            _, before, action, error, flags, command, length, end_io, saved = fields
            if before > 7 or action > 7 or before & action or error or not flags & 2:
                unknown.append(('invalid_sequence_marker', 0, stamp))
                tainted[pointer] = stamp
            elif before | action == 7:
                # A second marker invalidates the first. Do not let it rescue
                # the same request's next completion as a new valid pair.
                if marker is None:
                    pending[pointer] = (stamp, flags, command, length, saved)
            continue
        if kind == 5:
            if pointer in active:
                unknown.append(('duplicate_issue', active.pop(pointer)[0], stamp))
            _, command, length, sector, flags, end_io = fields
            active[pointer] = [stamp, command, length, length, False, end_io]
        elif kind == 7:
            previous = active.pop(pointer, None)
            unknown.append(('requeue' if previous else 'orphan_requeue', previous[0] if previous else 0, stamp))
        else:
            census['total'] += 1
            previous = active.get(pointer)
            _, error, completed, remaining, command, flags, end_io = fields
            if previous is None:
                census['unknown'] += 1
                unknown.append(('orphan_completion', 0, stamp))
            elif error or completed > remaining or remaining != previous[3] or command != previous[1] or end_io != previous[5]:
                census['unknown'] += 1
                unknown.append(('error/partial_mismatch', previous[0], stamp))
                del active[pointer]
            elif completed != remaining:
                census['unknown'] += 1
                unknown.append(('partial_completion', previous[0], stamp))
                previous[3] -= completed
                previous[4] = True
            else:
                begin, cmd, length, _, partial, _ = active.pop(pointer)
                require(begin < stamp, 'nonpositive request')
                if partial:
                    census['unknown'] += 1
                    unknown.append(('partial_generation', begin, stamp))
                else:
                    census['physical'] += 1
                    requests.append((begin, stamp, cmd, length))
    unknown.extend(('open_request', value[0], end) for value in active.values())
    unknown.extend(('open_logical_completion', value[0], end) for value in pending.values())
    unknown.extend(('open_tainted_logical', value, end) for value in tainted.values())
    require(sum(census[k] for k in ('physical', 'logical', 'unknown')) == census['total']
            == sum(row[0] == 6 for row in rows), 'completion census')
    return requests, unknown, census


def journal_phases(waits, rows):
    """Five trace points delimit four intervals; end is after sequence publish."""
    order = ['start_commit', 'commit_locking', 'commit_flushing', 'commit_logging', 'end_commit']
    names = ['start_to_locking', 'locking_to_flushing', 'flushing_to_logging', 'logging_to_end']
    groups = {}
    for _, stamp, pid, name, dev, transaction in rows:
        groups.setdefault(transaction, []).append((stamp, name.rsplit(':jbd2_', 1)[1]))
    connected, unknown = [], []
    for begin, finish, transaction in waits:
        phases = groups.get(transaction, [])
        stamps = [stamp for stamp, _ in phases]
        if ([name for _, name in phases] != order or len(set(stamps)) != 5
                or stamps != sorted(stamps) or finish <= stamps[0]):
            unknown.append(dict(transaction=transaction, begin=begin, end=finish, reason='missing/order/reused/tied/late_transaction'))
            continue
        values = {name: overlap([(begin, finish)], [(a, b)])
                  for name, a, b in zip(names, stamps, stamps[1:])}
        values['before_start'] = max(0, min(finish, stamps[0])-begin)
        values['after_end'] = max(0, finish-max(begin, stamps[-1]))
        require(sum(values.values()) == finish-begin, 'journal wait partition')
        connected.append(dict(transaction=transaction, begin=begin, end=finish, overlap_ns=values,
                              return_minus_end_ns=finish-stamps[-1]))
    totals = {name: sum(item['overlap_ns'][name] for item in connected)
              for name in [*names, 'before_start', 'after_end']}
    return dict(status='INCONCLUSIVE' if unknown else 'CONNECTED_DIAGNOSTIC_ONLY',
                overlap_ns=None if unknown else totals, connected=connected, unknown=unknown,
                interpretation='Shared transaction intervals; signed return-minus-end is not pure scheduling delay.')


def analyze(raw, *, start, end, worker, device, inode, expected=512):
    require(0 <= start < end and expected > 0, 'measurement bounds')
    rows, counts = parse(raw)
    v2 = any(line == "SCHEMA 2" for line in raw.splitlines())
    active = {}
    wait_details, all_phases = [], []
    fs, waits, requests, phases, unknown = [], [], [], [], []
    seen = set()
    for row in rows:
        kind, stamp, *fields = row
        if kind <= 3:
            tid, dev, key = fields[:3]
            require(tid == worker, 'foreign worker')
            ident = ('fs' if kind < 2 else 'wait', tid, dev, key)
            require((ident, stamp) not in seen, 'ambiguous equal timestamp')
            seen.add((ident, stamp))
            if kind % 2 == 0:
                require(ident not in active, 'duplicate interval start')
                active[ident] = stamp
            else:
                require(ident in active, 'orphan interval end')
                begin = active.pop(ident)
                require(fields[3] == 0 and begin < stamp, 'failed/reversed interval')
                if begin < end and stamp > start:
                    require(start <= begin < stamp <= end, 'measurement cuts interval')
                    if kind == 1:
                        require(dev == device and key == inode, 'wrong measured WAL inode/device')
                        fs.append((begin, stamp))
                    else:
                        require(dev == device, 'wrong journal device')
                        waits.append((begin, stamp))
                        wait_details.append((begin, stamp, key))
        elif kind == 4:
            require(fields[2] == device, 'wrong phase device')
            all_phases.append(row)
            if start <= stamp <= end:
                phases.append(row)
        elif not v2:
            pointer = fields[0]
            ident = ('rq', pointer)
            require((ident, stamp) not in seen, 'ambiguous request timestamp')
            seen.add((ident, stamp))
            if kind == 5:
                if ident in active:
                    unknown.append(('duplicate_issue', active.pop(ident)[0], stamp))
                active[ident] = [stamp, fields[1], fields[2], fields[2], False]
            elif ident not in active:
                unknown.append(('orphan_completion/requeue', 0, stamp))
            elif kind == 7:
                previous = active.pop(ident)
                unknown.append(('requeue', previous[0], stamp))
            else:
                previous = active[ident]
                error, completed, remaining = fields[1:]
                if error or completed > remaining or remaining != previous[3]:
                    unknown.append(('error/partial_mismatch', previous[0], stamp))
                    del active[ident]
                elif completed == remaining:
                    begin, flags, length, _, tainted = active.pop(ident)
                    require(begin < stamp, 'nonpositive request')
                    if tainted:
                        unknown.append(('partial_generation', begin, stamp))
                    else:
                        requests.append((begin, stamp, flags, length))
                else:
                    unknown.append(('partial_completion', previous[0], stamp))
                    previous[3] -= completed
                    previous[4] = True
    extra = {}
    if v2:
        requests, unknown, census = request_provenance(rows, end)
        extra = dict(schema_version=2, completion_census=census, journal_phases=journal_phases(wait_details, all_phases))
    for ident, value in active.items():
        if ident[0] == 'rq':
            unknown.append(('open_request', value[0], end))
        else:
            require(value >= end, 'open fsync/journal interval')
    require(len(fs) == expected, 'measured fsync pair count')
    require(sum(b-a for a,b in fs) == sum(b-a for a,b in union(fs)), 'overlapping worker fsync')
    relevant_unknown = [item for item in unknown if item[1] <= end and item[2] >= start]
    clipped = [(max(a,start), min(b,end), flags, length) for a,b,flags,length in requests if a < end and b > start]
    groups = {
        'all_requests': [(a,b) for a,b,_,_ in clipped],
        'flush': [(a,b) for a,b,f,_ in clipped if f & 255 == 2],
        'fua': [(a,b) for a,b,f,_ in clipped if f & (1 << 17)],
        'preflush': [(a,b) for a,b,f,_ in clipped if f & (1 << 18)],
    }
    return dict(status='INCONCLUSIVE_REQUEST_COVERAGE' if relevant_unknown else 'CONNECTED_DIAGNOSTIC_ONLY',
                fsync_count=len(fs), fsync_ns=sum(b-a for a,b in fs), journal_wait_ns=sum(b-a for a,b in union(waits)),
                fsync_journal_overlap_ns=overlap(fs, waits), request_count=len(clipped),
                zero_byte_flush_count=sum((f & 255 == 2) and length == 0 for _,_,f,length in clipped),
                request_overlap_ns=None if relevant_unknown else {key: overlap(fs, intervals) for key,intervals in groups.items()},
                unknown=relevant_unknown, boundary_unknown_count=len(unknown)-len(relevant_unknown),
                counts=counts, phase_count=len(phases),
                attribution='Shared journal/device temporal overlap only; categories may overlap, not additive; not pure device service time.', **extra)
