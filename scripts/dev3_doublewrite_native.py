"""Independently check retained native DWB workers, file bytes and identities."""
import json
from pathlib import Path
import stat

from check_dev0 import digest, source_fingerprint
from dev3_storage_model import image
from dev3_storage_native import (native_objects as storage_objects, check_events,
                                 content_manifest, seal)
from dev3_doublewrite_model import FIXTURES, TARGETS, inputs, sha

from dev3_doublewrite_checks import native_mutations

CUTS = {f'effect-{index}': index for index in (4, 6, 7, 8, 9, 10, 13, 16, 19, 20, 23, 24, 25)}
PHASES = ('descriptor_create descriptor_write descriptor_sync descriptor_directory images_create '
          'images_write images_write images_write images_write images_sync bank0_create bank0_write bank0_sync '
          'bank1_create bank1_write bank1_sync reserved_create reserved_write reserved_sync files_directory '
          'selector_create selector_write selector_sync selector_publish selector_directory').split()


def native_objects(root, profile, database):
    files = storage_objects(root, profile, database)
    def literal(name):
        return bytearray((Path(root) / FIXTURES / 'literal' / f'profile{profile}-{name}.bin').read_bytes())
    profile_hash = bytes.fromhex(sha(files['d3-profile']))
    empty = literal('empty')
    empty[8:24] = database
    empty = seal(empty)
    empty_hash = bytes.fromhex(sha(empty))
    selector = literal('DWB')
    selector[8:24], selector[48:80], selector[112:144] = database, profile_hash, empty_hash
    descriptor = literal('d3-dwb-stage')
    descriptor[16:32], descriptor[48:80] = database, profile_hash
    descriptor[160:192], descriptor[232:264] = files['FILEMAP'][112:144], empty_hash
    files.update({'DWB': seal(selector), 'd3-dwb-stage': seal(descriptor),
                  'd3-dwb-0': empty + bytes(16384 - 160), 'd3-dwb-1': bytes(16384),
                  'd3-dwb-images': bytes(16711680), 'd3-dwb-reserved': bytes(8192), 'LOCK': b''})
    return files


def prefix(plan, files, initial, count):
    result = dict(initial)
    for row in plan[:count]:
        name, op = row['name'], row['op']
        if op == 'create':
            if name in result:
                raise ValueError('FAIL_EFFECT_ORDER')
            result[name] = b''
        elif op == 'write':
            start, length = row['offset'], row['length']
            if len(result[name]) != start:
                raise ValueError('FAIL_EFFECT_ORDER')
            result[name] += files[TARGETS.get(name, name)][start:start + length]
        elif op == 'publish':
            result[row['to']] = result.pop(name)
        elif op not in ('sync', 'directory_sync'):
            raise ValueError('FAIL_EFFECT_ORDER')
    return result


def actual_inventory(path):
    rows = []
    for file in sorted(path.iterdir()):
        meta = file.lstat()
        if not stat.S_ISREG(meta.st_mode) or meta.st_nlink != 1:
            raise ValueError('FAIL_EXTERNAL_IMAGE')
        rows.append({'name': file.name, 'length': meta.st_size, 'sha256': digest(file),
                     'device': meta.st_dev, 'inode': meta.st_ino, 'links': meta.st_nlink})
    return rows


def check_files(files, before, expected):
    if files != expected['actual_files'] or content_manifest(files) != expected['files']:
        raise ValueError('FAIL_EXTERNAL_IMAGE')
    if content_manifest(before) != expected['initial']:
        raise ValueError('FAIL_OLD_FILES')
    after = {row['name']: row for row in files}
    if any(after.get(row['name']) != row for row in before):
        raise ValueError('FAIL_OLD_FILES')


def check_census(keys, required):
    if len(set(keys)) != len(keys) or set(keys) != required:
        raise ValueError('FAIL_NATIVE_CENSUS')


def check_receipt(value, expected):
    if value['source_sha256'] != expected['source_sha256']:
        raise ValueError('FAIL_SOURCE_IDENTITY')
    if value['binary_sha256'] != expected['binary_sha256']:
        raise ValueError('FAIL_BINARY_IDENTITY')
    if value['timed_out'] or not 0 <= value['hook_elapsed_ms'] <= 60000 or not 0 <= value['exit_elapsed_ms'] <= 10000:
        raise ValueError('FAIL_TIMEOUT')
    if (value['hook'] != expected['hook'] or value['after'] is not expected['after']
            or value['markers'] != [expected['marker']] or value['hook_count'] != 1):
        raise ValueError('FAIL_HOOK')
    check_files(value['files'], value['before'], expected)


def evaluate(output, root, artifact_parent, binary, source_sha256):
    root, parent, binary = Path(root), Path(artifact_parent).resolve(), Path(binary)
    if source_fingerprint(root) != source_sha256:
        raise ValueError('FAIL_SOURCE_IDENTITY')
    binary_sha = digest(binary)
    frozen = inputs(root)
    plan = frozen['effects.json']
    marker = 'XADB_STORAGE_NATIVE_ARTIFACT '
    roots = [Path(line.removeprefix(marker)) for line in output.splitlines() if line.startswith(marker)]
    if len(roots) != 3 or len(set(roots)) != 3 or output.count(marker) != 3:
        raise ValueError('FAIL_NATIVE_CENSUS')
    scopes, databases, reports = set(), set(), []
    mutants = []
    for path in roots:
        if path.resolve().parent != parent or path.is_symlink():
            raise ValueError('FAIL_ARTIFACT_PATH')
        report_path = path / 'result.json'
        report = json.loads(report_path.read_text())
        scope = report['scope']
        if (scope in scopes or scope not in ('doublewrite_native_profiles', 'doublewrite_native_faults', 'doublewrite_native_kills')
                or report['status'] != 'PASS' or report['filesystem'] != 'ext4'):
            raise ValueError('FAIL_NATIVE_CENSUS')
        scopes.add(scope)
        if report['source_sha256'] != source_sha256:
            raise ValueError('FAIL_SOURCE_IDENTITY')
        canonical = Path(report['binary'])
        if (canonical.parent != parent or canonical.is_symlink() or canonical.name != f'doublewrite-install-worker-{binary_sha}'
                or report['binary_sha256'] != binary_sha or digest(canonical) != binary_sha):
            raise ValueError('FAIL_BINARY_IDENTITY')
        keys, receipts = [], []
        mutation_baseline = None
        for row in report['cases']:
            if row['status'] != 'PASS':
                raise ValueError('FAIL_SCOPE')
            if scope == 'doublewrite_native_profiles':
                key, database = row['profile'], Path(row['database'])
                if database.parent != path:
                    raise ValueError('FAIL_ARTIFACT_PATH')
                entropy = bytes(row['entropy'])
                files = native_objects(root, key, entropy)
                initial = storage_objects(root, key, entropy) | {'LOCK': b''}
                expected = {'files': image(files), 'actual_files': actual_inventory(database), 'initial': image(initial)}
                check_files(row['files'], row['files_before'], expected)
                if row['clean_allocation_read'] is not True:
                    raise ValueError('FAIL_SCOPE')
                check_events(row['events'], plan, 25, True, 'profiles')
            else:
                case = Path(row['case'])
                if (case.parent != path or case.is_symlink() or row['worker_receipt'] != 'worker.json'
                        or row['before_receipt'] != 'before.json'):
                    raise ValueError('FAIL_ARTIFACT_PATH')
                database = case / 'database'
                worker = json.loads((case / 'worker.json').read_text())
                process = json.loads((case / 'process.json').read_text())
                before = json.loads((case / 'before.json').read_text())
                if before['source_sha256'] != source_sha256 or before['entropy'] != worker['entropy']:
                    raise ValueError('FAIL_SOURCE_IDENTITY')
                entropy = bytes(worker['entropy'])
                index = row['at'] + 1
                mode = 'fault' if scope == 'doublewrite_native_faults' else 'kill'
                after = row['after'] if mode == 'fault' else True
                if not isinstance(after, bool):
                    raise ValueError('FAIL_HOOK')
                key = (index, after) if mode == 'fault' else (row['cut'], row['repetition'])
                files = native_objects(root, 1, entropy)
                initial = storage_objects(root, 1, entropy) | {'LOCK': b''}
                expected_files = image(prefix(plan, files, initial, index - int(not after)))
                lines = (case / 'stdout.log').read_text().splitlines()
                markers = [line[line.index('XADB_STORAGE_'):] for line in lines if 'XADB_STORAGE_' in line]
                events = worker['events']
                actual_side = events[-1]['injected'] == 'after' if mode == 'fault' else True
                value = {**row, **process, 'markers': markers, 'before': before['files'],
                         'hook': worker['attempted'] if mode == 'fault' else worker['after'] + 1,
                         'after': actual_side, 'binary_sha256': report['binary_sha256'],
                         'source_sha256': worker['source_sha256']}
                expected = {'hook': index if mode == 'fault' else CUTS[row['cut']], 'after': after,
                            'marker': 'XADB_STORAGE_DONE' if mode == 'fault' else 'XADB_STORAGE_CUT|' + row['cut'],
                            'source_sha256': source_sha256, 'binary_sha256': binary_sha,
                            'files': expected_files, 'actual_files': actual_inventory(database), 'initial': image(initial)}
                check_receipt(value, expected)
                if mode == 'fault' and mutation_baseline is None:
                    mutation_baseline = (value, expected)
                if (process['binary'] != str(canonical) or process['hook_timeout_seconds'] != 60
                        or process['exit_timeout_seconds'] != 10
                        or (process['signal'], process['exit_code']) != ((None, 0) if mode == 'fault' else (9, None))):
                    raise ValueError('FAIL_PROCESS')
                if mode == 'fault':
                    cause = 'io' if plan[index - 1]['op'] in ('write', 'sync') else 'namespace'
                    lower = {'code': 'IoWriteUnknown' if cause == 'io' else None,
                             'kind': 'Io' if cause == 'namespace' else None,
                             'effect': 'CallerMustResolve' if cause == 'io' or after else 'Unchanged',
                             'io_kind': 'Other'}
                    if worker['lower'] != lower:
                        raise ValueError('FAIL_ORIGINAL_CAUSE')
                    if (worker['status'] != 'ERROR_OBSERVED' or worker['effect'] != 'CallerMustResolve'
                            or worker['hit'] != 1 or worker['completed'] != index - 1
                            or worker['phase'] != PHASES[index - 1] or worker['cause'] != cause):
                        raise ValueError('FAIL_EXPECTED_FAILURE')
                elif worker['cut'] != row['cut'] or index != expected['hook']:
                    raise ValueError('FAIL_HOOK')
                check_events(events, plan, index, after, mode)
                receipts.append({'case': str(case), 'files': {name: digest(case / name) for name in
                                 ('worker.json', 'before.json', 'process.json', 'stdout.log', 'stderr.log')}})
            if database.is_symlink() or database.resolve() in databases:
                raise ValueError('FAIL_NATIVE_CENSUS')
            keys.append(key)
            databases.add(database.resolve())
        required = (set(range(1, 4)) if scope == 'doublewrite_native_profiles' else
                    {(i, side) for i in range(1, 26) for side in (False, True)} if scope == 'doublewrite_native_faults' else
                    {(cut, repetition) for cut in CUTS for repetition in range(3)})
        check_census(keys, required)
        if mutation_baseline is not None:
            mutants = native_mutations(*mutation_baseline, keys, required,
                                       frozen['checker-negatives.json']['native'], check_receipt, check_census)
        reports.append({'scope': scope, 'cases': len(keys), 'report': str(report_path),
                        'sha256': digest(report_path), 'worker_evidence': receipts})
    if source_fingerprint(root) != source_sha256 or digest(binary) != binary_sha:
        raise ValueError('FAIL_SOURCE_OR_BINARY_DRIFT')
    if len(mutants) != 12:
        raise ValueError('FAIL_CHECKER_CENSUS')
    return {'status': 'PASS', 'checker_counterexamples': mutants, 'source_sha256': source_sha256, 'binary': str(binary),
            'binary_sha256': binary_sha, 'profiles': 3, 'faults': 50, 'kills': 39,
            'databases': len(databases), 'results': reports,
            'scope': 'retained ext4 process effects and unchanged Fresh files; no power-loss recovery'}
