"""Audit native bootstrap workers against retained source, binaries and files."""
import json
from pathlib import Path
import stat
import struct

from check_dev0 import digest, source_fingerprint
from dev3_storage_model import DATA, WAL, TARGETS, inputs, objects, image, sha

CUTS = {'descriptor_sync': 15, 'descriptor_directory': 16, 'allocation_write': 18,
        'data_sync': 82, 'root_write': 84, 'chunk_write': 85, 'bank0_sync': 213,
        'bank1_first_write': 215, 'bank1_sync': 344, 'files_directory': 345,
        'selector_sync': 348, 'selector_publish': 349, 'selector_directory': 350}


def seal(data):
    data[-32:] = bytes.fromhex(sha(data[:-32]))
    return bytes(data)


def native_objects(root, profile, database):
    """Substitute actual entropy into frozen literals using published offsets."""
    if len(database) != 16 or not any(database):
        raise ValueError('FAIL_ENTROPY')
    files = {name: bytearray(raw) for name, raw in objects(root, profile).items()}
    for name, offset in [('d3-profile', 16), (WAL, 16), ('WRITER', 16),
                         ('d3-bootstrap', 16), ('FILEMAP', 8)]:
        files[name][offset:offset + 16] = database
    profile_raw = seal(files['d3-profile'])
    files[WAL][56:88] = bytes.fromhex(sha(profile_raw))
    epoch = seal(files[WAL])
    files['WRITER'][64:96] = bytes.fromhex(sha(profile_raw))
    files['WRITER'][96:128] = bytes.fromhex(sha(epoch))
    allocation = files[DATA][:16384]
    allocation[8:24] = database
    allocation = seal(allocation)
    files[DATA][:16384] = allocation
    chunk = files['d3-filemap-0'][16384:16640]
    chunk[16:32] = database
    chunk = seal(chunk)
    root_page = files['d3-filemap-0'][:320]
    root_page[16:32] = database
    root_page[224:256] = bytes.fromhex(sha(chunk))
    root_page = seal(root_page)
    files['d3-filemap-0'][:320] = root_page
    files['d3-filemap-0'][16384:16640] = chunk
    files['FILEMAP'][48:80] = bytes.fromhex(sha(profile_raw))
    files['FILEMAP'][112:144] = bytes.fromhex(sha(root_page))
    files['d3-bootstrap'][48:80] = bytes.fromhex(sha(profile_raw))
    files['d3-bootstrap'][176:208] = bytes.fromhex(sha(root_page))
    files['d3-bootstrap'][208:240] = bytes.fromhex(sha(allocation))
    for name in ('d3-profile', WAL, 'WRITER', 'd3-bootstrap', 'FILEMAP'):
        files[name] = seal(files[name])
    return {name: bytes(raw) for name, raw in files.items()}


def frame(database):
    data = bytearray(192)
    struct.pack_into('<4sHBBHHI', data, 0, b'XWF1', 1, 3, 0, 160, 0, 192)
    data[24:40] = database
    struct.pack_into('<QI', data, 40, 1, 1)
    struct.pack_into('<Q', data, 56, 1)
    data[128:160] = bytes.fromhex(sha(data[:128]))
    return seal(data)


def prefix(plan, files, completed):
    result = {}
    for event in plan[:completed]:
        name, op = event['name'], event['op']
        if op == 'create':
            result[name] = b''
        elif op == 'write':
            offset, length = event['offset'], event['length']
            if len(result[name]) != offset:
                raise ValueError('FAIL_EFFECT_ORDER')
            result[name] += files[TARGETS.get(name, name)][offset:offset + length]
        elif op == 'publish':
            result[event['to']] = result.pop(name)
    return result


def phase(index):
    ranges = [(13, 13, 'descriptor_create'), (14, 14, 'descriptor_write'),
              (15, 15, 'descriptor_sync'), (16, 16, 'descriptor_directory'),
              (17, 17, 'data_create'), (18, 81, 'data_write'), (82, 82, 'data_sync'),
              (83, 83, 'bank0_create'), (84, 212, 'bank0_write'), (213, 213, 'bank0_sync'),
              (214, 214, 'bank1_create'), (215, 343, 'bank1_write'), (344, 344, 'bank1_sync'),
              (345, 345, 'files_directory'), (346, 346, 'selector_create'),
              (347, 347, 'selector_write'), (348, 348, 'selector_sync'),
              (349, 349, 'selector_publish'), (350, 350, 'selector_directory')]
    return next(name for low, high, name in ranges if low <= index <= high)


def actual_inventory(path):
    rows, lock = [], None
    for file in sorted(path.iterdir()):
        meta = file.lstat()
        if not stat.S_ISREG(meta.st_mode):
            raise ValueError('FAIL_EXTERNAL_IMAGE')
        identity = {'name': file.name, 'length': meta.st_size, 'device': meta.st_dev,
                    'inode': meta.st_ino, 'links': meta.st_nlink}
        if file.name == 'LOCK':
            lock = {**identity, 'regular': True}
        else:
            rows.append({**identity, 'sha256': digest(file)})
    return rows, lock


def content_manifest(rows):
    return [{key: row[key] for key in ('name', 'length', 'sha256')} for row in rows]


def check_receipt(value, expected):
    """Fail-closed comparison also used by the frozen N01–N10 mutations."""
    if value['timed_out'] or not 0 <= value['hook_elapsed_ms'] <= 60000 or not 0 <= value['exit_elapsed_ms'] <= 10000:
        raise ValueError('FAIL_TIMEOUT')
    if value['hook'] != expected['hook'] or value['markers'] != [expected['marker']] or value['hook_count'] != 1:
        raise ValueError('FAIL_HOOK')
    if value['binary_sha256'] != expected['binary_sha256']:
        raise ValueError('FAIL_BINARY_IDENTITY')
    if value['source_sha256'] != expected['source_sha256']:
        raise ValueError('FAIL_SOURCE_IDENTITY')
    if expected['mode'] == 'fault' and (value['status'] != 'ERROR_OBSERVED' or value['hit'] != 1 or value['effect'] != 'CallerMustResolve'):
        raise ValueError('FAIL_EXPECTED_FAILURE')
    if value.get('ready', False) or value.get('resumable', False) or value['fresh_retry_mutations'] != 0:
        raise ValueError('FAIL_SCOPE')
    if (value['lock'] is None or value['lock'].get('regular') is not True or value['lock'].get('length') != 0
            or value['lock'] != expected['actual_lock']):
        raise ValueError('FAIL_LOCK')
    if (content_manifest(value['files']) != expected['files']
            or value['files'] != expected['actual_files']):
        raise ValueError('FAIL_EXTERNAL_IMAGE')


def check_events(events, plan, index, after, mode):
    if len(events) != index:
        raise ValueError('FAIL_EFFECT_CENSUS')
    for at, (event, want) in enumerate(zip(events, plan, strict=False)):
        completed = at + 1 < index or after
        expected_injected = ('after' if after else 'before') if mode == 'fault' and at + 1 == index else None
        if (event['at'] != at or event['kind'] != ('dirsync' if want['op'] == 'directory_sync' else want['op'])
                or any(event[key] != want[key] for key in ('name', 'offset', 'length'))
                or event['completed'] is not completed or event['injected'] != expected_injected
                or (completed and event['completed_bytes'] != (want['length'] if want['op'] == 'write' else None))):
            raise ValueError('FAIL_EFFECT_ORDER')


def evaluate(output, root, artifact_parent, binary, source_sha256):
    root, artifact_parent, binary = Path(root), Path(artifact_parent).resolve(), Path(binary)
    if source_fingerprint(root) != source_sha256:
        raise ValueError('FAIL_SOURCE_IDENTITY')
    binary_sha256 = digest(binary)
    frozen = inputs(root)
    plan = frozen['effects.json']
    marker = 'XADB_STORAGE_NATIVE_ARTIFACT '
    paths = [Path(line.removeprefix(marker)) for line in output.splitlines() if line.startswith(marker)]
    if len(paths) != 3 or len(set(paths)) != 3 or output.count(marker) != 3:
        raise ValueError('FAIL_NATIVE_CENSUS')
    results, scopes, databases = [], set(), set()
    for path in paths:
        if path.resolve().parent != artifact_parent or path.is_symlink():
            raise ValueError('FAIL_ARTIFACT_PATH')
        report_path = path / 'result.json'
        report = json.loads(report_path.read_text())
        scope = report['scope']
        if scope in scopes or scope not in ('native_profiles', 'native_faults', 'native_kills') or report['status'] != 'PASS' or report['filesystem'] != 'ext4':
            raise ValueError('FAIL_NATIVE_CENSUS')
        scopes.add(scope)
        if report['source_sha256'] != source_sha256:
            raise ValueError('FAIL_SOURCE_IDENTITY')
        canonical = Path(report['binary'])
        if (canonical.parent != artifact_parent or canonical.is_symlink() or canonical.name != f'storage-session-worker-{binary_sha256}'
                or report['binary_sha256'] != binary_sha256 or digest(canonical) != binary_sha256):
            raise ValueError('FAIL_BINARY_IDENTITY')
        seen, evidence = set(), []
        for row in report['cases']:
            if row['status'] != 'PASS' or row['fresh_retry_mutations'] != 0:
                raise ValueError('FAIL_SCOPE')
            if scope == 'native_profiles':
                key = row['profile']
                database = Path(row['database'])
                db = bytes(row['entropy'])
                files = native_objects(root, key, db)
                if content_manifest(row['files_before_append']) != image(files):
                    raise ValueError('FAIL_EXTERNAL_IMAGE')
                for before, after in zip(row['files_before_append'], row['files'], strict=True):
                    if any(before[k] != after[k] for k in ('name', 'device', 'inode', 'links')):
                        raise ValueError('FAIL_EXTERNAL_IMAGE')
                files[WAL] += frame(db)
                actual, lock = actual_inventory(database)
                if content_manifest(row['files']) != image(files) or row['files'] != actual or row['lock'] != lock or lock['length'] != 0:
                    raise ValueError('FAIL_EXTERNAL_IMAGE')
                if row['shared_read_buffer_and_wal'] is not True:
                    raise ValueError('FAIL_SCOPE')
                append = [dict(index=351, op='write', name=WAL, offset=4096, length=192),
                          dict(index=352, op='sync', name=WAL, offset=0, length=0)]
                check_events(row['events'], plan + append, 352, True, 'profiles')
            else:
                case = Path(row['case'])
                if case.parent != path or case.is_symlink() or row['worker_receipt'] != 'worker.json':
                    raise ValueError('FAIL_ARTIFACT_PATH')
                database = case / 'database'
                worker = json.loads((case / 'worker.json').read_text())
                process = json.loads((case / 'process.json').read_text())
                db = bytes(worker['entropy'])
                index = row['at'] + 1
                mode = 'fault' if scope == 'native_faults' else 'kill'
                after = row['after'] if mode == 'fault' else True
                key = (index, after) if mode == 'fault' else (row['cut'], row['repetition'])
                actual, lock = actual_inventory(database)
                expected_files = image(prefix(plan, native_objects(root, 1, db), index - int(not after)))
                lines = (case / 'stdout.log').read_text().splitlines()
                markers = [line[line.index('XADB_STORAGE_'):] for line in lines if 'XADB_STORAGE_' in line]
                hook = index if mode == 'fault' else worker['after'] + 1
                expected_hook = index if mode == 'fault' else CUTS[row['cut']]
                expected_marker = 'XADB_STORAGE_DONE' if mode == 'fault' else 'XADB_STORAGE_CUT|' + row['cut']
                value = {**row, **worker, **process, 'markers': markers, 'hook': hook,
                         'binary_sha256': report['binary_sha256'], 'source_sha256': worker['source_sha256']}
                expected = {'mode': mode, 'hook': expected_hook, 'marker': expected_marker,
                            'binary_sha256': binary_sha256, 'source_sha256': source_sha256,
                            'files': expected_files, 'actual_files': actual, 'actual_lock': lock}
                check_receipt(value, expected)
                if (process['binary'] != str(canonical) or process['hook_timeout_seconds'] != 60
                        or process['exit_timeout_seconds'] != 10
                        or (process['signal'], process['exit_code']) != ((None, 0) if mode == 'fault' else (9, None))):
                    raise ValueError('FAIL_PROCESS')
                if mode == 'fault':
                    cause = 'io' if plan[index - 1]['op'] in ('write', 'sync') else 'namespace'
                    if worker['phase'] != phase(index) or worker['cause'] != cause:
                        raise ValueError('FAIL_EXPECTED_FAILURE')
                elif worker['cut'] != row['cut'] or index != expected_hook:
                    raise ValueError('FAIL_HOOK')
                check_events(worker['events'], plan, index, after, mode)
                evidence.append({'case': str(case), 'files': {name: digest(case / name) for name in
                                 ('worker.json', 'process.json', 'stdout.log', 'stderr.log')}})
            if database.parent != path and (scope == 'native_profiles' or database.parent.parent != path):
                raise ValueError('FAIL_ARTIFACT_PATH')
            if database.is_symlink() or database.resolve() in databases or key in seen:
                raise ValueError('FAIL_NATIVE_CENSUS')
            databases.add(database.resolve())
            seen.add(key)
        required = (set(range(1, 4)) if scope == 'native_profiles' else
                    {(i, side) for i in range(13, 351) for side in (False, True)} if scope == 'native_faults' else
                    {(cut, repetition) for cut in CUTS for repetition in range(3)})
        if seen != required:
            raise ValueError('FAIL_NATIVE_CENSUS')
        results.append({'scope': scope, 'cases': len(seen), 'report': str(report_path),
                        'sha256': digest(report_path), 'worker_evidence': evidence})
    if source_fingerprint(root) != source_sha256 or digest(binary) != binary_sha256:
        raise ValueError('FAIL_SOURCE_OR_BINARY_DRIFT')
    return {'status': 'PASS', 'source_sha256': source_sha256, 'binary': str(binary),
            'binary_sha256': binary_sha256, 'profiles': 3, 'faults': 676, 'kills': 39,
            'results': results, 'scope': 'retained ext4 files and process effects; no power-loss recovery'}
