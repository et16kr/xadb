#!/usr/bin/env python3
"""Three same-ELF workers with exact externally delivered SIGKILL cuts.

The frozen admission binds source/ELF/input and static evidence before effects.
Every attempt has a fresh output directory; failures and killed process traces
remain in place. This controller does not rerun a completed chain.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

SELECTOR = ('storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
            'plan4_recovery::tests::free_undo::free_undo_three_process_worker')
PHASES = ('produce', 'recover', 'reentry')
CUTS = ('A_FU_BEFORE_UNDO_HOME', 'A_FU_AFTER_UNDO_DURABLE', None)
BUDGET = {'wall_seconds': 30, 'worker_seconds': 29, 'audit_seconds': 1,
          'output_bytes': 8 << 20, 'trace_bytes': 4 << 20,
          'database_bytes': 32 << 20, 'snapshot_bytes': 128 << 20,
          'artifact_bytes': 1 << 30}
HOME = 'd3-data-0000000100000001'


def require(condition, detail):
    if not condition:
        raise ValueError(detail)


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def size(root):
    total = 0
    for path in root.rglob('*'):
        require(not path.is_symlink(), 'symlink in owned run')
        if path.is_file():
            total += path.stat().st_size
    return total


def save(path, data):
    with path.open('x') as stream:
        json.dump(data, stream, indent=2, sort_keys=True)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def trace_rows(path):
    rows = [line.split('\t') for line in path.read_text().splitlines()]
    require(all(len(row) == 6 and int(row[0]) == i for i, row in enumerate(rows)),
            'trace sequence/shape')
    return rows


def audit_cut(rows, cut):
    reached = [i for i, row in enumerate(rows)
               if row[1:3] == ['barrier', cut] and row[5] == 'REACHED']
    require(len(reached) == 1, 'exact barrier occurrence')
    at = reached[0]
    if cut == CUTS[0]:
        armed = [i for i, row in enumerate(rows)
                 if row[1:3] == ['arm', cut] and row[3:] == ['180224', '16384', 'ARMED:1']]
        triggers = [i for i, row in enumerate(rows)
                    if row[1:3] == ['cut', cut] and row[3:] == ['180224', '16384', 'REACHED:1']]
        require(len(armed) == 1 and len(triggers) == 1 and armed[0] < triggers[0] < at,
                'exact armed home cut')
        publications = [i for i, row in enumerate(rows[:at])
                        if row[1:3] == ['publish', 'd3-dwb-select-stage']
                        and row[5] == 'DWB:Ok(())']
        require(publications, 'Pending selector was not published')
        require(any(row[1:3] == ['dirsync', 'namespace'] and row[5] == 'Ok(())'
                    for row in rows[publications[-1] + 1:triggers[0]]),
                'Pending publication did not reach successful directory sync')
        # Fresh-file zero filling precedes arming this INIT-image cut. The
        # producer's explicit arm event starts its occurrence/order interval.
        require(not any(row[1:3] == ['write', HOME] and row[3] == '180224'
                        and row[5].startswith('OK:') for row in rows[armed[0] + 1:]),
                'cut happened after page11 home write')
    else:
        removed = [i for i, row in enumerate(rows[:at])
                   if row[1:3] == ['remove', 'd3-wal-00000001'] and row[5] == 'Ok(())']
        require(len(removed) == 1, 'old WAL actual unlink missing/duplicated')
        require(any(row[1:3] == ['dirsync', 'namespace'] and row[5] == 'Ok(())'
                    for row in rows[removed[0] + 1:at]), 'old WAL unlink not dirsynced')
        require(not any(row[1:3] == ['write', HOME] and row[3] in (str(12 * 16384), str(13 * 16384))
                        for row in rows), 'second cut forced a new data home')
    require(at == len(rows) - 1, 'effects after crash barrier')


def audit_pending_database(db, source):
    # Independent fixed-layout observation of the actual selected Pending;
    # a previous Empty selector publication cannot satisfy this crash cut.
    number = lambda raw, at, width=8: int.from_bytes(raw[at:at + width], 'little')
    digest = lambda raw: hashlib.sha256(raw).digest()
    selector = (db / 'DWB').read_bytes()
    require(len(selector) == 4096 and selector[:7] == b'XCS1\x01\x00\x02'
            and digest(selector[:-32]) == selector[-32:], 'actual DWB selector')
    bank, length = selector[81], number(selector, 96, 4)
    require(bank in (0, 1) and length == 248, 'exact one-image selected control')
    control = (db / f'd3-dwb-{bank}').read_bytes()[:length]
    require(digest(control) == selector[112:144] and digest(control[:-32]) == control[-32:],
            'selected Pending digest binding')
    require(control[:7] == b'XDW1\x01\x00\x01' and number(control, 48, 2) == 1
            and number(control, 56) == 6 and number(control, 32) == number(selector, 88),
            'actual selected state is not Pending at L6')
    page = (source / 'tests/fixtures/dev3-plan4-free-undo/literals/native-page11-init.page').read_bytes()
    entry = control[128:216]
    require(entry[:4] == b'\x00\x00\x08\x00' and entry[8:56] == page[24:72]
            and entry[56:88] == digest(page), 'actual Pending page11/inc2/Undo3/L6')
    slot = (db / 'd3-dwb-images').read_bytes()[:16544]
    require(slot[:8] == b'XDS1\x01\x00\x00\x00' and slot[128:-32] == page
            and digest(slot[:-32]) == slot[-32:] and slot[40:88] == page[24:72]
            and number(slot, 32) == number(control, 40), 'actual DWB image slot')
    home = (db / HOME).read_bytes()
    require(home[11 * 16384:12 * 16384] != page, 'home already contains cut image')


def enforce_bounds(output, artifact_root, extra_bytes=0):
    logs = [p for name in ('stdout.log', 'stderr.log') for p in output.glob('*/' + name)]
    require(sum(p.stat().st_size for p in logs) <= BUDGET['output_bytes'], 'cumulative output bound')
    require(all(p.stat().st_size <= BUDGET['trace_bytes'] for p in output.glob('*/effects.tsv')),
            'per-phase trace bound')
    require(size(output / 'database') <= BUDGET['database_bytes'], 'live database bound')
    snapshots = sum(size(p) for p in output.glob('*/database-snapshot'))
    require(snapshots <= BUDGET['snapshot_bytes'], 'retained snapshots bound')
    require(size(artifact_root) + extra_bytes <= BUDGET['artifact_bytes'], 'total artifact bound')
    return snapshots


def verify_source(source, manifest):
    actual = {str(p.relative_to(source)) for p in source.rglob('*') if p.is_file()}
    require(actual == set(manifest['files']), 'source file inventory drift')
    for relative, spec in manifest['files'].items():
        path = source / relative
        require(not path.is_symlink() and path.resolve().is_relative_to(source), 'unsafe source member')
        require(path.stat().st_size == spec['bytes'] and sha(path) == spec['sha256'], 'source member drift')


def run(revision, admission_path, output):
    revision, admission_path, output = map(lambda p: Path(p).resolve(),
                                         (revision, admission_path, output))
    admission = json.loads(admission_path.read_text())
    require(admission.get('status') == 'ACCEPTED' and admission.get('independent') is True,
            'missing independent admission')
    require(admission.get('selector') == SELECTOR and admission.get('budget') == BUDGET,
            'admission inventory/budget mismatch')
    binary = revision / 'xadb-engine-tests'
    source = revision / 'source'
    input_path = source / 'tests/fixtures/dev3-plan4-free-undo/input-plan.json'
    manifest = json.loads((revision / 'source-files.json').read_text())
    require(manifest['source_fingerprint'] == admission['source_fingerprint'], 'source identity mismatch')
    require(sha(binary) == admission['binary_sha256'] and sha(input_path) == admission['input_sha256'],
            'binary/input identity mismatch')
    require(sha(revision / 'source-files.json') == admission['source_manifest_sha256'], 'source manifest drift')
    verify_source(source, manifest)
    require(sha(Path(__file__)) == admission['controller_sha256'], 'controller drift')
    for relative, digest in admission['evidence'].items():
        path = revision / relative
        require(path.resolve().is_relative_to(revision) and sha(path) == digest, 'admission evidence drift')
    require(admission['evidence'], 'missing source admission evidence')
    require(not output.exists() and output.is_relative_to(revision), 'fresh revision-owned output required')
    for old in revision.parent.glob('*/native*/result.json'):
        require(json.loads(old.read_text()).get('status') != 'PASS', 'completed native chain already retained')
    output.mkdir()
    db = output / 'database'
    db.mkdir()
    save(output / 'planned.json', {'admission_sha256': sha(admission_path), 'admission': admission,
                                 'phases': PHASES, 'cuts': CUTS})
    started = time.monotonic()
    deadline = started + BUDGET['worker_seconds']
    records = []
    active = None
    result = {'status': 'FAIL', 'workers': records}
    try:
        for phase, cut in zip(PHASES, CUTS):
            require(time.monotonic() < deadline, 'worker chain deadline')
            phase_dir = output / phase
            phase_dir.mkdir()
            env = os.environ.copy()
            env.update(XADB_FREE_UNDO_NATIVE_PHASE=phase, XADB_FREE_UNDO_NATIVE_DB=str(db),
                       XADB_FREE_UNDO_NATIVE_OUTPUT=str(phase_dir))
            # Reserve the entire remaining output/trace/DB/snapshot envelope
            # before spawning a producer, not after it has consumed the disk.
            reserve = BUDGET['output_bytes'] + 3 * BUDGET['trace_bytes'] + BUDGET['database_bytes'] + BUDGET['snapshot_bytes']
            enforce_bounds(output, revision.parent, reserve)
            command = [str(binary), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
            with (phase_dir / 'stdout.log').open('xb') as stdout, (phase_dir / 'stderr.log').open('xb') as stderr:
                active = subprocess.Popen(command, cwd=source, env=env, stdout=stdout, stderr=stderr,
                                          start_new_session=True)
                record = {'phase': phase, 'pid': active.pid, 'command': command, 'cut': cut}
                records.append(record)
                marker = phase_dir / 'cut-reached.tsv'
                killed = False
                while active.poll() is None:
                    require(time.monotonic() < deadline, 'worker chain timeout')
                    enforce_bounds(output, revision.parent)
                    if cut and not killed and marker.exists():
                        fields = marker.read_text().strip().split('\t')
                        # A partial marker is not permission to kill before its flush.
                        if len(fields) == 6 and fields[:3] == [cut, str(active.pid), '1']:
                            handles, peak, fd_peak = map(int, fields[3:])
                            require(0 <= handles <= peak <= 3 and 0 <= fd_peak <= 6, 'killed worker File/FD peaks')
                            record['cut_resources'] = {'handles': handles, 'peak': peak, 'fd_peak': fd_peak}
                            audit_cut(trace_rows(phase_dir / 'effects.tsv'), cut)
                            os.killpg(active.pid, signal.SIGKILL)
                            killed = True
                    time.sleep(0.01)
                record['exit_code'] = active.wait()
                record['externally_killed'] = killed
                require(record['exit_code'] == (-signal.SIGKILL if cut else 0), 'unexpected worker exit')
                require(not cut or killed, 'worker died without controller cut')
                active = None
            # A fast exiting child must meet the same bounds as a running one.
            enforce_bounds(output, revision.parent)
            rows = trace_rows(phase_dir / 'effects.tsv')
            require((phase_dir / 'effects.tsv').stat().st_size <= BUDGET['trace_bytes'], 'trace bound')
            if cut:
                audit_cut(rows, cut)
                if phase == 'produce':
                    audit_pending_database(db, source)
                    page10_reads = [row for row in rows if row[1:3] == ['read', HOME]
                                    and row[3:5] == ['163840', '16384'] and row[5].startswith('OK:16384:')]
                    require(len(page10_reads) >= 2, 'page10 physical double-read reload missing')
            else:
                require((phase_dir / 'worker.tsv').is_file(), 'normal cleanup receipt missing')
                fields = dict(line.split('\t') for line in (phase_dir / 'worker.tsv').read_text().splitlines())
                require(int(fields['pid']) == record['pid'], 'normal receipt process mismatch')
                require(int(fields['handles']) == 0 and int(fields['peak']) <= 3 and int(fields['fd_peak']) <= 6,
                        'normal phase File/FD bounds')
                stdout = (phase_dir / 'stdout.log').read_text()
                require(stdout.count(f"FREE_UNDO_NATIVE_COMPLETE\t{record['pid']}") == 1
                        and 'running 1 test' in stdout
                        and 'test result: ok. 1 passed; 0 failed; 0 ignored;' in stdout,
                        'exact one-worker completion proof missing')
            if phase != 'produce':
                require(not (db / 'd3-wal-00000001').exists(), 'old WAL survived')
            require(time.monotonic() < deadline, 'snapshot worker budget')
            db_bytes = size(db)
            snapshots = enforce_bounds(output, revision.parent, db_bytes)
            require(snapshots + db_bytes <= BUDGET['snapshot_bytes'], 'snapshot admission bound')
            shutil.copytree(db, phase_dir / 'database-snapshot')
            enforce_bounds(output, revision.parent)
            require(sum(size(output / p / 'database-snapshot') for p in PHASES
                        if (output / p / 'database-snapshot').exists()) <= BUDGET['snapshot_bytes'], 'snapshot bound')
            record['trace_sha256'] = sha(phase_dir / 'effects.tsv')
        require(time.monotonic() < deadline, '29-second worker/snapshot phase budget')
        audit_started = time.monotonic()
        require(len({r['pid'] for r in records}) == 3, 'process identity reused')
        require(sha(revision / 'source-files.json') == admission['source_manifest_sha256'], 'source manifest changed')
        verify_source(source, manifest)
        require(sha(binary) == admission['binary_sha256'] and sha(input_path) == admission['input_sha256'],
                'identity drift after workers')
        enforce_bounds(output, revision.parent)
        result['audit_seconds'] = time.monotonic() - audit_started
        require(result['audit_seconds'] <= BUDGET['audit_seconds'], 'one-second final audit budget')
        require(time.monotonic() - started <= BUDGET['wall_seconds'], 'total wall budget')
        result['status'] = 'PASS'
    except BaseException as error:
        result['error'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        try:
            if active is not None:
                if active.poll() is None:
                    os.killpg(active.pid, signal.SIGKILL)
                    records[-1]['cleanup_signal'] = 'SIGKILL'
                records[-1]['exit_code'] = active.wait(timeout=2)
                records[-1]['cleanup_reaped'] = True
        except BaseException as cleanup_error:
            result['cleanup_error'] = f'{type(cleanup_error).__name__}: {cleanup_error}'
        finally:
            result['seconds'] = time.monotonic() - started
            save(output / 'result.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--admission', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.revision, args.admission, args.output), indent=2))
