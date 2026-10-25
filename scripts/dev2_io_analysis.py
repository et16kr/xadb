#!/usr/bin/env python3
"""Strict retained-event reconstruction; shared-disk overlap is not WAL ownership."""
from collections import Counter
import re

KINDS = ('FS_IN', 'FS_OUT', 'WAIT_IN', 'WAIT_OUT', 'PHASE', 'ISSUE', 'COMPLETE', 'REQUEUE')
WIDTHS = (5, 6, 5, 6, 6, 6, 6, 3)
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
    for line in raw.splitlines():
        if not line.strip():
            continue
        match = re.fullmatch(r'@counts\[([0-7])\]: (\d+)', line)
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
        require(len(parts) == WIDTHS[kind], 'record shape')
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


def analyze(raw, *, start, end, worker, device, inode, expected=512):
    require(0 <= start < end and expected > 0, 'measurement bounds')
    rows, counts = parse(raw)
    active = {}
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
        elif kind == 4:
            require(fields[2] == device, 'wrong phase device')
            if start <= stamp <= end:
                phases.append(row)
        else:
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
                attribution='Shared journal/device temporal overlap only; categories may overlap, not additive; not pure device service time.')
