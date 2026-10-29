#!/usr/bin/env python3
"""Bounded, same-ELF Catalog CREATE crash/recovery consumers.

Only a frozen independent admission enables spawning. Expected bytes come
from the separately generated and audited input corpus, never product output.
Each selected case has one fresh attempt; all failures and snapshots survive.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import time

SELECTOR = ('storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
            'plan4_recovery::tests::catalog_create::catalog_create_native_worker')
INPUT = 'f6e0dd554a91931cc8e048e775a666dacd777f0c1c3285abed6773ec15f21eeb'
MANIFEST = 'e648b0bc4fb8ccf5f258cec6d521128daef6e59f3f9a20825d19eb81d68555df'
HOME = 'd3-data-0000000100000001'
INPUT_DIR = 'tests/fixtures/dev3-catalog-create/native3'
PHASES = ('produce', 'recover', 'reentry')
SCHEDULE = {
    'N01': ((9, None, 'A_CREATE_ACK_T'), (19, 9, 'A_CREATE_ACK_U'), (25, 19, None)),
    'N02': ((8, None, 'A_CREATE_BEFORE_CATALOG_HOME'), (10, 8, 'A_CREATE_AFTER_CLR_DURABLE'), (16, 10, None)),
    'N03': ((9, None, 'A_CREATE_UNKNOWN_AFTER_SYNC'), (15, 9, None)),
}
# Original finite limits; HOME is one 64-page extent. The independently checked
# namespace bound, including two bases and the largest epoch, is 27,001,322 B.
BUDGET = {'wall_seconds': 30, 'worker_seconds': 29, 'audit_seconds': 1,
          'output_bytes': 8 << 20, 'trace_bytes': 4 << 20,
          'database_bytes': 32 << 20, 'snapshot_bytes': 128 << 20,
          'artifact_bytes': 1 << 30, 'address_space_bytes': 1 << 30,
          'free_floor_bytes': 60 << 30, 'file_peak': 3, 'fd_peak': 6}


def require(condition, detail):
    if not condition:
        raise ValueError(detail)


def digest(raw):
    return hashlib.sha256(raw).digest()


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def number(raw, at, width=8):
    return int.from_bytes(raw[at:at + width], 'little')


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
    fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def fields(path):
    rows = [line.split('\t') for line in path.read_text().splitlines()]
    require(all(len(row) == 2 for row in rows), 'receipt shape')
    result = dict(rows)
    require(len(result) == len(rows), 'duplicate receipt field')
    return result


def trace_rows(path):
    rows = [line.split('\t') for line in path.read_text().splitlines()]
    require(all(len(row) == 6 and row[0] == str(i) for i, row in enumerate(rows)),
            'trace sequence/shape')
    return rows


def marker_resources(text, cut, pid):
    values = text.strip().split('\t')
    require(len(values) == 6 and values[:3] == [cut, str(pid), '1'], 'marker identity/occurrence')
    handles, peak, fd_peak = map(int, values[3:])
    require(0 <= handles <= peak <= BUDGET['file_peak'] and 0 <= fd_peak <= BUDGET['fd_peak'],
            'killed worker File/FD peaks')
    return {'handles': handles, 'peak': peak, 'fd_peak': fd_peak}


def audit_unlink(rows, epoch):
    name = f'd3-wal-{epoch:08x}'
    removed = [i for i, row in enumerate(rows) if row[1:3] == ['remove', name] and row[5] == 'Ok(())']
    require(len(removed) == 1, 'old WAL actual unlink missing/duplicated')
    published = [i for i, row in enumerate(rows[:removed[0]])
                 if row[1:3] == ['publish', 'd3-current-stage'] and row[5] == 'CURRENT:Ok(())']
    require(published and any(row[1:3] == ['dirsync', 'namespace'] and row[5] == 'Ok(())'
                for row in rows[published[-1] + 1:removed[0]]), 'selected CURRENT not dirsynced before retirement')
    created = [i for i, row in enumerate(rows)
               if row[1:3] == ['create', f'd3-wal-{epoch + 1:08x}'] and row[5] == 'OK']
    require(len(created) == 1 and removed[0] < created[0]
            and any(row[1:3] == ['dirsync', 'namespace'] and row[5] == 'Ok(())'
                    for row in rows[removed[0] + 1:created[0]]), 'old WAL unlink not dirsynced before new epoch')


def audit_cut(rows, cut, wal_end):
    reached = [i for i, row in enumerate(rows) if row[1:3] == ['barrier', cut] and row[5] == 'REACHED']
    require(len(reached) == 1 and reached[0] == len(rows) - 1, 'exact final crash barrier')
    if cut == 'A_CREATE_BEFORE_CATALOG_HOME':
        armed = [i for i, row in enumerate(rows) if row[1:3] == ['arm', cut]
                 and row[3:] == ['131072', '16384', 'ARMED:1']]
        triggered = [i for i, row in enumerate(rows) if row[1:3] == ['cut', cut]
                     and row[3:] == ['131072', '16384', 'REACHED:1']]
        require(len(armed) == len(triggered) == 1 and armed[0] < triggered[0] < reached[0], 'exact armed home cut')
        published = [i for i, row in enumerate(rows[:triggered[0]])
                     if row[1:3] == ['publish', 'd3-dwb-select-stage'] and row[5] == 'DWB:Ok(())']
        require(published and any(row[1:3] == ['dirsync', 'namespace'] and row[5] == 'Ok(())'
                for row in rows[published[-1] + 1:triggered[0]]), 'Pending publication directory sync')
        require(not any(row[1:3] == ['write', HOME] and row[3] == '131072'
                        and row[5].startswith('OK:') for row in rows[armed[0] + 1:]), 'Catalog home preceded cut')
    if cut == 'A_CREATE_UNKNOWN_AFTER_SYNC':
        name = 'A_CREATE_COMMIT_SYNC_AFTER_ERROR'
        armed = [i for i, row in enumerate(rows) if row[1:] == ['arm', name, str(wal_end), '0', 'ARMED:1']]
        failed = [i for i, row in enumerate(rows) if row[1:] == ['fault', name, str(wal_end), '0', 'AFTER_SUCCESS:1']]
        require(len(armed) == len(failed) == 1 and armed[0] < failed[0] < reached[0], 'exact sync AfterError')
        require(any(row[1:3] == ['sync', 'd3-wal-00000001'] and row[5] == 'Ok(())'
                    for row in rows[armed[0] + 1:failed[0]]), 'AfterError lacks actual successful WAL sync')
    require(not any(row[1] == 'barrier' and row[2].startswith('A_CREATE_ACK')
                    for row in rows[:-1]), 'unexpected earlier ACK')


def audit_inventory(raw, generation, capture):
    require(len(raw) == 16384 and raw[:8] == b'XOI1\x01\x00\x01\x02', 'Base4 inventory root envelope')
    owners, items, chunks, chunk_len = number(raw, 56, 2), number(raw, 72), number(raw, 64), number(raw, 80)
    root_len = 160 + 56 * owners
    require(owners <= 36 and items in (0, 1, 3) and chunks == int(items != 0)
            and chunk_len == (160 + 96 * items if items else 0)
            and number(raw, 12, 4) == root_len and number(raw, 8, 2) == 128
            and number(raw, 10, 2) == 96, 'Base4 inventory counts/lengths')
    require(raw[16:32] == bytes(range(1, 17)) and number(raw, 32) == 1
            and number(raw, 40) == generation and number(raw, 48) == capture
            and digest(raw[:root_len - 32]) == raw[root_len - 32:root_len], 'Base4 inventory root binding/seal')
    chunk = raw[2176:2176 + chunk_len]
    require(raw[88:120] == digest(chunk) and not any(raw[root_len:2176])
            and not any(raw[2176 + chunk_len:]), 'Base4 inventory chunk digest/padding')
    if items:
        require(chunk[:8] == b'XOI1\x01\x00\x02\x02' and chunk[16:56] == raw[16:56]
                and number(chunk, 12, 4) == chunk_len and number(chunk, 80, 4) == items
                and chunk[88:120] == digest(raw[128:128 + 56 * owners])
                and digest(chunk[:-32]) == chunk[-32:], 'Base4 inventory chunk binding/seal')


def audit_inventory_owners(inventory, expected_owners):
    require(len(expected_owners) % 56 == 0
            and number(inventory, 56, 2) == len(expected_owners) // 56
            and inventory[128:128 + len(expected_owners)] == expected_owners
            and number(inventory, 72) == 0 and number(inventory, 64) == 0,
            'Base4 exact current owner rows and empty Reserved inventory')


class Oracle:
    def __init__(self, source):
        root = source / INPUT_DIR
        require(sha(root / 'input-plan.json') == INPUT and sha(root / 'manifest.json') == MANIFEST, 'oracle input identity')
        self.manifest = json.loads((root / 'manifest.json').read_text())
        self.rows = {(r['scenario'], r['lsn']): r for r in self.manifest['rows']}
        self.points = {(r['scenario'], r['lsn']): r for r in self.manifest['checkpoints']}
        self.raw = {}
        for line in (root / 'raw-index.tsv').read_text().splitlines()[1:]:
            hashed, length, relative = line.split('\t')
            require(relative == f'raw/{hashed}.bin' and len(hashed) == 64, 'oracle raw path')
            raw = (root / relative).read_bytes()
            require(len(raw) == int(length) and digest(raw).hex() == hashed, 'oracle raw digest')
            require(hashed not in self.raw, 'duplicate oracle raw')
            self.raw[hashed] = raw
        require(len(self.rows) == 56 and len(self.points) == 16 and len(self.raw) == 221
                and sum(map(len, self.raw.values())) == 5794810, 'oracle inventory')
        require({p.name for p in (root / 'raw').iterdir()} == {h + '.bin' for h in self.raw}, 'oracle raw inventory')

    def page(self, case, lsn, page):
        return self.raw[self.points[case, lsn]['pages'][str(page)]]

    def actor(self, row):
        # plan.local is the serializer's model cycle. The native input maps
        # cycle 2 to epoch 2/local 1 while retaining cycle-2 slot generations.
        # Read the frozen independent descriptor, never a product result.
        descriptor = self.raw[row['transaction_after']]
        require(len(descriptor) == 1536 and descriptor[16:24] == b'\x01' + bytes(7),
                'native transaction descriptor owner')
        actor = descriptor[24:72]
        require(actor[:16] == bytes(range(1, 17)) and number(actor, 16) == 1
                and number(actor, 24, 4) == row['plan']['local']
                and not any(actor[28:32]) and number(actor, 32) == 1
                and number(actor, 40) == row['plan']['generation'],
                'native model cycle to actor mapping')
        return actor

    def wal(self, db, case, lsn):
        row = self.rows[case, lsn]
        raw = (db / f"d3-wal-{row['epoch']:08x}").read_bytes()
        require(len(raw) == row['wal_end'], 'actual WAL exact end')
        for item in self.rows.values():
            if item['scenario'] == case and item['epoch'] == row['epoch'] and item['lsn'] <= lsn:
                require(raw[item['wal_start']:item['wal_end']] == self.raw[item['frame']], 'actual WAL frame ' + item['id'])
        return raw

    def home(self, db, case, lsn):
        with (db / HOME).open('rb') as stream:
            require(os.fstat(stream.fileno()).st_size == 64 * 16384, 'home physical length')
            for page in range(64):
                expected = self.page(case, lsn, page)
                if any(expected):
                    stream.seek(page * 16384)
                    require(stream.read(16384) == expected, f'allocated final home {page}')

    def pending(self, db, case):
        selector = (db / 'DWB').read_bytes()
        require(len(selector) == 4096 and selector[:7] == b'XCS1\x01\x00\x02'
                and digest(selector[:-32]) == selector[-32:], 'actual DWB selector')
        bank, length = selector[81], number(selector, 96, 4)
        require(bank in (0, 1) and length == 248, 'one-image selected control')
        control = (db / f'd3-dwb-{bank}').read_bytes()[:length]
        require(digest(control) == selector[112:144] and digest(control[:-32]) == control[-32:], 'selected Pending digest')
        require(control[:7] == b'XDW1\x01\x00\x01' and number(control, 48, 2) == 1
                and number(control, 56) == 8 and number(control, 32) == number(selector, 88), 'selected Pending at L8')
        page = self.page(case, 8, 8)
        entry = control[128:216]
        require(entry[:4] == bytes((0, 0, page[6], 0)) and entry[8:56] == page[24:72]
                and entry[56:88] == digest(page), 'Pending Catalog page identity')
        slot = (db / 'd3-dwb-images').read_bytes()[:16544]
        require(slot[:8] == b'XDS1\x01\x00\x00\x00' and slot[128:-32] == page
                and digest(slot[:-32]) == slot[-32:] and slot[40:88] == page[24:72]
                and number(slot, 32) == number(control, 40), 'actual DWB image slot')
        with (db / HOME).open('rb') as stream:
            stream.seek(8 * 16384)
            require(stream.read(16384) != page, 'Catalog home already contains cut image')

    def provenance(self, case, capture, page_number):
        expected = bytearray(352)
        if page_number < 9 or not any(self.page(case, capture, page_number)):
            return expected
        init = None
        role = None
        for row in self.rows.values():
            if row['scenario'] != case or row['lsn'] > capture:
                continue
            for candidate in ('4', '5', '6', '7'):
                if candidate not in row['after']:
                    continue
                image = self.raw[row['after'][candidate]]
                if number(image, 32) != page_number:
                    continue
                if page_number in (9, 10) and row['lsn'] not in (3, 5):
                    continue
                if page_number > 10 and candidate not in ('6', '7'):
                    continue
                require(init is None, 'duplicate INIT provenance input')
                init, role = row, candidate
        require(init is not None, 'missing INIT provenance input')
        image = self.raw[init['after'][role]]
        system = page_number in (9, 10)
        expected[:5] = bytes((1, (1 if page_number == 9 else 2) if system else (3 if role == '6' else 4), image[6], 0, 1 if system else 2))
        expected[8:32] = image[24:48]
        expected[32:48] = image[48:64]
        expected[48:56] = init['lsn'].to_bytes(8, 'little')
        expected[56:60] = init['epoch'].to_bytes(4, 'little')
        expected[64:96] = bytes.fromhex(init['frame'])
        expected[96:128] = digest(image)
        owner = bytearray(48)
        owner[:16] = bytes(range(1, 17))
        owner[16:24] = (1).to_bytes(8, 'little')
        if system:
            owner[24] = 1
            owner[32:40] = (1 if page_number == 9 else 2).to_bytes(8, 'little')
            owner[40:48] = (1).to_bytes(8, 'little')
        else:
            owner[:] = self.actor(init)
            for at, value in ((184, init['plan']['object']), (192, 1), (200, 1)):
                expected[at:at + 8] = value.to_bytes(8, 'little')
            for at, pair in zip((208, 232), init['plan']['pair']):
                expected[at:at + 24] = (0x100000001).to_bytes(8, 'little') + pair.to_bytes(8, 'little') + init['plan']['inc'].to_bytes(8, 'little')
        expected[128:176] = owner
        expected[176:184] = (init['lsn'] - 1).to_bytes(8, 'little')
        return expected

    def base(self, db, previous, case, capture, epoch):
        current = (db / 'CURRENT').read_bytes()
        require(len(current) == 4096 and current[:4] == b'XRC1' and digest(current[:-32]) == current[-32:], 'actual CURRENT envelope')
        require(current[16:32] == bytes(range(1, 17)) and number(current, 32) == 1, 'CURRENT namespace')
        closure = current[256:512]
        require(closure[:4] in (b'\x02\x01\x00\x01', b'\x02\x01\x01\x01')
                and number(closure, 4, 4) == epoch and number(closure, 16) == capture
                and number(closure, 24, 4) == 2097152 and number(closure, 96) == 4096, 'CURRENT selected closure')
        raw = (db / f'd3-rebase-{closure[2]}').read_bytes()
        require(len(raw) == 2097152 and raw[:8] == b'XRB1\x04\x00\x01\x00'
                and number(raw, 8, 4) == len(raw) and number(raw, 12, 4) == 4096
                and digest(raw[:-32]) == raw[-32:] and digest(raw) == closure[32:64], 'selected Base4 envelope')
        require(raw[16:32] == bytes(range(1, 17)) and number(raw, 32) == 1
                and raw[48:80] == current[56:88] and number(raw, 88) == number(closure, 8)
                and number(raw, 96) == capture and number(raw, 248) == capture, 'Base4 namespace/capture')
        expected_generation = epoch - 1
        previous_generation, previous_digest = 0, bytes(32)
        if epoch > 2:
            prior = (previous / 'CURRENT').read_bytes()
            require(len(prior) == 4096 and prior[:4] == b'XRC1' and digest(prior[:-32]) == prior[-32:], 'previous CURRENT envelope')
            previous_generation = number(prior, 264)
            previous_digest = prior[288:320]
            require(previous_generation == expected_generation - 1, 'previous selected generation')
        require(number(raw, 88) == expected_generation and number(raw, 104) == previous_generation
                and raw[112:144] == previous_digest, 'Base4 predecessor chain')
        old = self.wal(previous, case, capture)
        require(number(raw, 144, 4) == epoch - 1 and number(raw, 152) == len(old)
                and raw[160:192] == digest(old) and raw[1192:1224] == digest(old[:4096]), 'Base4 original source provenance')
        require(not (db / f'd3-wal-{epoch - 1:08x}').exists(), 'old epoch WAL survived')
        specs = ((4096, 55296, 36, 1536), (65536, 6144, 64, 96),
                 (73728, 22528, 64, 352), (98304, 16384, 1, 16384), (262144, 1048576, 64, 16384))
        sections = []
        for index, (offset, length, count, width) in enumerate(specs):
            entry = raw[1280 + index * 64:1344 + index * 64]
            require(number(entry, 0, 2) == index + 1 and not any(entry[2:4])
                    and tuple(number(entry, at, 4) for at in (4, 8, 12, 16)) == (offset, length, count, width)
                    and not any(entry[20:32]) and entry[32:64] == digest(raw[offset:offset + length]), 'Base4 section directory')
            sections.append(raw[offset:offset + length])
        bank, certificates, provenance, inventory, arena = sections
        point = self.points[case, capture]
        require(tuple(number(raw, 208 + index * 8) for index in range(4)) == (2, 0, 1, 0)
                and number(raw, 256, 4) == int(bool(point['active_transaction']))
                and number(raw, 260, 4) == 1 and number(raw, 264) == 1 << 9
                and number(raw, 272) == point['root_masks'][1] and number(raw, 280) == point['root_masks'][2]
                and number(raw, 288) == point['scalar']['revision']
                and number(raw, 296) == point['scalar']['aging_groups'], 'Base4 current census/header')
        adopted_local = max((number(self.actor(row), 32) for row in self.rows.values()
                             if row['scenario'] == case and row['epoch'] == epoch - 1
                             and row['lsn'] <= capture and row['operation'] == 1), default=0)
        require(number(raw, 192) == adopted_local, 'Base4 source adopted local')
        for index, key in ((0, 'active_transaction'), (32, 'system'), (34, 'aging')):
            expected = self.raw[point[key]] if point[key] else bytes(1536)
            require(bank[index * 1536:(index + 1) * 1536] == expected, 'Base4 descriptor bank ' + key)
        require(not any(bank[1536:32 * 1536]) and not any(bank[33 * 1536:34 * 1536])
                and not any(bank[35 * 1536:]), 'Base4 inactive descriptor rows')
        audit_inventory(inventory, number(raw, 88), capture)
        # Every selected cut in this native inventory follows Forward/CLR or
        # terminal Commit, never a Reserved cut. Original System INIT is
        # Finished; active Tx (when present) precedes the sole Aging owner.
        expected_owners = b''.join(self.raw[point[key]][16:72]
                                   for key in ('active_transaction', 'aging') if point[key])
        audit_inventory_owners(inventory, expected_owners)
        for index in range(64):
            page = self.page(case, capture, index)
            require(arena[index * 16384:(index + 1) * 16384] == page, f'Base4 arena {index}')
            certificate = bytearray(96)
            if any(page):
                certificate[:3] = bytes((1, page[6], 0))
                certificate[8:32] = page[24:48]
                certificate[32:56] = page[48:72]
                certificate[56:88] = digest(page)
            require(certificates[index * 96:(index + 1) * 96] == certificate, f'Base4 certificate {index}')
            require(provenance[index * 352:(index + 1) * 352] == self.provenance(case, capture, index), f'Base4 provenance {index}')
        return {'base_sha256': digest(raw).hex(), 'current_sha256': digest(current).hex(), 'capture': capture, 'epoch': epoch}


def verify_source(source, manifest):
    actual = {str(p.relative_to(source)) for p in source.rglob('*') if p.is_file()}
    require(actual == set(manifest['files']), 'source file inventory drift')
    for relative, spec in manifest['files'].items():
        path = source / relative
        require(not path.is_symlink() and path.resolve().is_relative_to(source), 'unsafe source member')
        require(path.stat().st_size == spec['bytes'] and sha(path) == spec['sha256'], 'source member drift')


def enforce_bounds(output, aggregate, reserve=0):
    logs = [p for name in ('stdout.log', 'stderr.log') for p in output.glob('*/' + name)]
    require(sum(p.stat().st_size for p in logs) <= BUDGET['output_bytes'], 'cumulative output bound')
    require(all(p.stat().st_size <= BUDGET['trace_bytes'] for p in output.glob('*/effects.tsv')), 'per-phase trace bound')
    require(size(output / 'database') <= BUDGET['database_bytes'], 'live database bound')
    snapshots = sum(size(p) for p in output.glob('*/database-snapshot'))
    require(snapshots <= BUDGET['snapshot_bytes'], 'retained snapshots bound')
    require(size(aggregate) + reserve <= BUDGET['artifact_bytes'], 'native aggregate bound')
    require(shutil.disk_usage(output).free >= BUDGET['free_floor_bytes'] + reserve, 'free disk floor')
    return snapshots


def validate_identity(data, expected):
    require(all(data.get(key) == value for key, value in expected.items()), 'worker identity mismatch')


def audit_ack(phase_dir, cut, identity, lsn):
    path = phase_dir / 'ack-observed.json'
    if cut and cut.startswith('A_CREATE_ACK_'):
        require(path.is_file(), 'missing controller AckObserved')
        actual = json.loads(path.read_text())
        validate_identity(actual, {**identity, 'kind': 'AckObserved', 'cut': cut, 'lsn': lsn})
    else:
        require(not path.exists(), 'non-ACK phase has AckObserved')


def abort_and_reap(active, records, result):
    try:
        if active.poll() is None:
            try:
                os.killpg(active.pid, signal.SIGKILL)
            except ProcessLookupError:
                # A racing exit must still be reaped below.
                pass
        code = active.wait(timeout=2)
        if records:
            records[-1].update(exit_code=code, aborted_and_reaped=True)
    except BaseException as cleanup_error:
        result['status'] = 'FAIL'
        result['cleanup_error'] = repr(cleanup_error)


def run(revision, admission_path, aggregate, case):
    revision, admission_path, aggregate = map(lambda p: Path(p).resolve(), (revision, admission_path, aggregate))
    admission = json.loads(admission_path.read_text())
    require(admission.get('status') == 'ACCEPTED' and admission.get('independent') is True, 'missing independent admission')
    require(admission.get('selector') == SELECTOR and admission.get('budget') == BUDGET and case in admission.get('cases', []), 'admission case/budget mismatch')
    binary, source = revision / 'xadb-engine-tests', revision / 'source'
    manifest = json.loads((revision / 'source-files.json').read_text())
    require(manifest['source_fingerprint'] == admission['source_fingerprint'], 'source identity mismatch')
    require(sha(binary) == admission['binary_sha256'] and sha(source / INPUT_DIR / 'input-plan.json') == admission['input_sha256'] == INPUT, 'binary/input identity mismatch')
    require(sha(revision / 'source-files.json') == admission['source_manifest_sha256'], 'source manifest drift')
    require(sha(Path(__file__)) == admission['controller_sha256'], 'controller drift')
    verify_source(source, manifest)
    require(admission['evidence'], 'missing admission evidence')
    for relative, hashed in admission['evidence'].items():
        path = revision / relative
        require(path.resolve().is_relative_to(revision) and sha(path) == hashed, 'admission evidence drift')
    static = json.loads((revision / 'catalog-source-admission/result.json').read_text())
    require(static['status'] == 'PASS' and static['source'] == admission['source_fingerprint'] and static['elf'] == admission['binary_sha256'], 'source admission not passed for actual ELF')
    require('catalog-source-admission/result.json' in admission['evidence'], 'source admission not pinned')
    oracle = Oracle(source)
    require(case in SCHEDULE and aggregate.is_relative_to(revision), 'revision-owned native aggregate required')
    for old in revision.parent.glob(f'*/catalog-native*/{case}/result.json'):
        require(json.loads(old.read_text()).get('status') != 'PASS', 'completed native case already retained')
    aggregate.mkdir(exist_ok=True)
    output = aggregate / case
    require(not output.exists(), 'fresh case output required')
    output.mkdir()
    db = output / 'database'
    db.mkdir()
    save(output / 'planned.json', {'admission_sha256': sha(admission_path), 'case': case, 'schedule': SCHEDULE[case], 'budget': BUDGET})
    started = time.monotonic()
    deadline = started + BUDGET['worker_seconds']
    records = []
    active = None
    result = {'status': 'FAIL', 'case': case, 'workers': records}
    try:
        for phase_index, (lsn, capture, cut) in enumerate(SCHEDULE[case]):
            phase = PHASES[phase_index]
            require(time.monotonic() < deadline, 'worker chain deadline')
            phase_dir = output / phase
            phase_dir.mkdir()
            identity = {'case': case, 'phase': phase, 'source_sha256': admission['source_fingerprint'],
                        'elf_sha256': admission['binary_sha256'], 'input_sha256': INPUT}
            env = {key: value for key, value in os.environ.items() if not key.startswith('XADB')}
            env.update({f'XADB_CATALOG_NATIVE_{key.upper()}': value for key, value in identity.items()})
            env.update(XADB_CATALOG_NATIVE_DB=str(db), XADB_CATALOG_NATIVE_OUTPUT=str(phase_dir))
            reserve = BUDGET['output_bytes'] + 3 * BUDGET['trace_bytes'] + BUDGET['database_bytes'] + BUDGET['snapshot_bytes']
            enforce_bounds(output, aggregate, reserve)
            command = [str(binary), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
            def constrain():
                resource.setrlimit(resource.RLIMIT_AS, (BUDGET['address_space_bytes'],) * 2)
                resource.setrlimit(resource.RLIMIT_FSIZE, (BUDGET['database_bytes'],) * 2)
            with (phase_dir / 'stdout.log').open('xb') as stdout, (phase_dir / 'stderr.log').open('xb') as stderr:
                active = subprocess.Popen(command, cwd=source, env=env, stdout=stdout, stderr=stderr,
                                          start_new_session=True, preexec_fn=constrain)
                identity['pid'] = str(active.pid)
                require(active.pid not in [record['pid'] for record in records], 'distinct worker PIDs required')
                record = {'phase': phase, 'pid': active.pid, 'command': command, 'cut': cut, 'lsn': lsn}
                records.append(record)
                killed = False
                marker = phase_dir / 'cut-reached.tsv'
                while active.poll() is None:
                    require(time.monotonic() < deadline, 'worker chain timeout')
                    enforce_bounds(output, aggregate)
                    if marker.exists() and not killed:
                        require(cut is not None, 'unexpected normal-phase marker')
                        text = marker.read_text()
                        if text.endswith('\n'):
                            record['cut_resources'] = marker_resources(text, cut, active.pid)
                            validate_identity(fields(phase_dir / 'identity.tsv'), {**identity, 'manifest_sha256': MANIFEST})
                            audit_cut(trace_rows(phase_dir / 'effects.tsv'), cut, oracle.rows[case, lsn]['wal_end'])
                            if cut.startswith('A_CREATE_ACK_'):
                                save(phase_dir / 'ack-observed.json', {'kind': 'AckObserved', 'pid': active.pid, 'cut': cut, 'case': case, 'lsn': lsn, **identity})
                            os.killpg(active.pid, signal.SIGKILL)
                            killed = True
                    time.sleep(0.01)
                record['exit_code'] = active.wait()
                record['externally_killed'] = killed
                require(record['exit_code'] == (-signal.SIGKILL if cut else 0) and (not cut or killed), 'unexpected worker exit/cut')
                active = None
            enforce_bounds(output, aggregate)
            validate_identity(fields(phase_dir / 'identity.tsv'), {**identity, 'manifest_sha256': MANIFEST})
            rows = trace_rows(phase_dir / 'effects.tsv')
            audit_ack(phase_dir, cut, identity, lsn)
            oracle.wal(db, case, lsn)
            if cut:
                audit_cut(rows, cut, oracle.rows[case, lsn]['wal_end'])
                if cut == 'A_CREATE_BEFORE_CATALOG_HOME':
                    oracle.pending(db, case)
                if cut == 'A_CREATE_UNKNOWN_AFTER_SYNC':
                    require(not (phase_dir / 'ack-observed.json').exists()
                            and (phase_dir / 'stdout.log').read_text().count('CATALOG_NATIVE_UNKNOWN\t') == 1, 'UNKNOWN must not ACK')
            else:
                require(not marker.exists(), 'normal phase crash marker')
                receipt = fields(phase_dir / 'worker.tsv')
                validate_identity(receipt, identity)
                require(int(receipt['handles']) == 0 and 0 <= int(receipt['peak']) <= BUDGET['file_peak']
                        and 0 <= int(receipt['fd_peak']) <= BUDGET['fd_peak']
                        and int(receipt['log_bytes']) <= BUDGET['trace_bytes'], 'normal worker resources')
                stdout = (phase_dir / 'stdout.log').read_text()
                require(stdout.count(f'CATALOG_NATIVE_COMPLETE\t{case}\t{phase}\t{record["pid"]}') == 1
                        and 'running 1 test' in stdout and 'test result: ok. 1 passed; 0 failed; 0 ignored;' in stdout, 'exact normal completion')
                oracle.home(db, case, lsn)
            if capture is not None:
                audit_unlink(rows, phase_index)
                record['base'] = oracle.base(db, output / PHASES[phase_index - 1] / 'database-snapshot', case, capture, phase_index + 1)
            require(time.monotonic() < deadline, 'snapshot worker budget')
            db_bytes = size(db)
            snapshots = enforce_bounds(output, aggregate, db_bytes)
            require(snapshots + db_bytes <= BUDGET['snapshot_bytes'], 'snapshot reservation bound')
            shutil.copytree(db, phase_dir / 'database-snapshot')
            save(phase_dir / 'reaped.json', record)
            enforce_bounds(output, aggregate)
        audit_start = time.monotonic()
        require(audit_start < deadline, 'worker and snapshot budget')
        # Source was immutable at spawn; verify its complete inventory again.
        verify_source(source, manifest)
        require(sha(binary) == admission['binary_sha256'], 'post-run ELF drift')
        require(sha(Path(__file__)) == admission['controller_sha256'], 'post-run controller drift')
        require(time.monotonic() - audit_start <= BUDGET['audit_seconds'], 'final audit budget')
        require(time.monotonic() - started <= BUDGET['wall_seconds'], 'whole case budget')
        result['status'] = 'PASS'
    except BaseException as error:
        result['error'] = repr(error)
    finally:
        if active is not None:
            abort_and_reap(active, records, result)
        result.update(seconds=time.monotonic() - started, source=admission['source_fingerprint'],
                      elf=admission['binary_sha256'], input=INPUT, controller=sha(Path(__file__)),
                      admission_sha256=sha(admission_path), free_bytes_after=shutil.disk_usage(output).free)
        save(output / 'result.json', result)
    print(json.dumps(result), flush=True)
    return result['status'] == 'PASS'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--revision', required=True)
    parser.add_argument('--admission', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--case', required=True, choices=tuple(SCHEDULE))
    args = parser.parse_args()
    raise SystemExit(0 if run(args.revision, args.admission, args.output, args.case) else 1)
