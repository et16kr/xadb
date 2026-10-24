#!/usr/bin/env python3
"""One actual native G5 consumer; retain its first setup and every failed attempt."""
import argparse
import datetime
import json
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
import dev3_plan4_genesis_cases as common
import dev3_plan4_genesis_native as oracle
import dev3_plan4_clean_cases as clean

SCHEMA = 'XADB_PLAN4_G5_NATIVE_V1'
SELECTOR = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::native::worker_g5_clean'
require, sha = common.require, common.sha
RECEIPT_RESERVE = 65536


def index_tsv(path):
    rows = Path(path).read_text().splitlines()
    require(rows and rows[0] == 'name\tbytes\tdevice\tinode\tsha256', 'file index header')
    result = {}
    for line in rows[1:]:
        name, size, device, inode, digest = line.split('\t')
        require(name not in result, 'duplicate file index')
        result[name] = {'bytes': int(size), 'device': int(device), 'inode': int(inode),
                        'sha256': digest, 'links': 1}
    require(len(result) == 15, 'file index census')
    return result


def validate_worker(output, profile, identity, process, final_files):
    output = Path(output)
    case = f'P4-G5-NATIVE-P{profile}'
    execution = oracle.tsv(output / 'execution.tsv')
    require(execution == {'id': case, 'pid': str(process['pid']), 'profile': str(profile),
                         **dict(zip(('source_sha256', 'elf_sha256', 'input_sha256'), identity))},
            'native execution identity')
    stdout = (output / 'stdout.log').read_text()
    require(process['exact'] and process['exit_code'] == 0 and not process['truncated'], 'incomplete worker')
    completions = [line.removeprefix('test ' + SELECTOR + ' ... ') for line in stdout.splitlines() if 'P4_NATIVE_COMPLETE\t' in line]
    require(completions == [f'P4_NATIVE_COMPLETE\t{case}\t{process["pid"]}'], 'missing or foreign completion')
    require('running 1 test' in stdout and '1 passed; 0 failed;' in stdout, 'non-exact native selector')
    oracle.validate_entropy((output / 'entropy.tsv').read_text(), 'produce', process['pid'])
    effects = (output / 'effects.tsv').read_text()
    oracle.validate_effects(effects, 'produce')
    rows = [line.split('\t') for line in effects.splitlines()]
    begin = [int(row[0]) for row in rows if row[1] == 'g5_begin']
    require(len(begin) == 1, 'missing or duplicate G5 boundary')
    live = rows[begin[0] + 1:]
    require(live and all(row[1] not in oracle.MUTATIONS | {'entropy', 'acquire'} for row in live),
            'G5 attempted a persistent mutation or second authority')
    require(any(row[1] == 'read' for row in live) and any(row[1] == 'drop_lease' for row in live),
            'missing actual G5 reads or cleanup')
    resources = oracle.tsv(output / 'resources.tsv')
    require(int(resources['g5_event_begin']) == begin[0] + 1, 'trace boundary mismatch')
    require(resources['g5_mutations'] == resources['g5_entropy'] == resources['handles_final'] == resources['fd_final'] == '0', 'G5 side effect or handle residue')
    require(resources['setup_entropy'] == '1' and int(resources['setup_mutations']) > 0, 'no actual fresh setup')
    require(int(resources['handles_peak']) <= 3 and int(resources['fd_peak']) <= 5, 'native handle bound')
    require(int(resources['events']) == len(rows), 'event census')
    before, after = index_tsv(output / 'before.tsv'), index_tsv(output / 'after.tsv')
    require(before == after == final_files['files'], 'G5 file bytes or identity changed')
    return {'execution': execution, 'resources': resources, 'g5_events': len(live),
            'before': before, 'after': after, 'whole_files': final_files}


def history(unit, case):
    refs, elapsed, retained, logs = [], 0.0, 0, 0
    for path in sorted(Path(unit).rglob('result.json')):
        record = json.loads(path.read_text())
        if record.get('schema') != SCHEMA:
            continue
        require(record.get('finalized') is True, 'unfinished native attempt requires audit')
        for relative, value in record['retained_index'].items():
            file = path.parent / relative
            require(file.is_file() and file.stat().st_size == value['bytes'] and sha(file) == value['sha256'],
                    'prior native attempt changed')
        require({str(p.relative_to(path.parent)) for p in path.parent.rglob('*') if p.is_file()} == set(record['retained_index']) | {'result.json'}, 'unindexed prior native output')
        elapsed += record['elapsed_seconds']
        retained += sum(p.stat().st_size for p in path.parent.rglob('*') if p.is_file())
        logs += sum(p.stat().st_size for p in path.parent.rglob('*') if p.is_file() and p.suffix in {'.json', '.log', '.tsv', '.txt'})
        refs.append({'path': str(path), 'sha256': sha(path), 'index': record['retained_index']})
        if record['case'] != case:
            continue
        process = record.get('process')
        require(process is not None and record.get('attempt_state') == 'FINISHED', 'uncertain prior native attempt requires audit')
        parsed = common.parse_markers((path.parent / 'worker/stdout.log').read_text(), SELECTOR, [case], record['identity'])
        require(parsed == process['parsed'] and not process.get('truncated') and parsed['parser_error'] is None,
                'uncertain prior native output requires audit')
        require(not parsed['markers']['PASS'] and 'P4_NATIVE_COMPLETE\t' not in (path.parent / 'worker/stdout.log').read_text(),
                'completed native consumer cannot be replayed')
    return refs, elapsed, retained, logs


def retained_index(output):
    return {str(path.relative_to(output)): {'bytes': path.stat().st_size, 'sha256': sha(path)}
            for path in sorted(output.rglob('*')) if path.is_file() and path.name != 'result.json'}


def main():
    cli = argparse.ArgumentParser()
    cli.add_argument('revision', type=Path)
    cli.add_argument('--output', type=Path, required=True)
    cli.add_argument('--profile', type=int, choices=(1, 2, 3), default=1)
    cli.add_argument('--s5', action='store_true')
    args = cli.parse_args()
    started = time.monotonic()
    require(args.profile == 1 or args.s5, 'expanded profile requires S5 selection')
    revision = args.revision.resolve()
    source, binary, manifest_path, build, identity = clean.validate_build(revision)
    bindings = clean.runner_bindings(source, [__file__])
    case = f'P4-G5-NATIVE-P{args.profile}'
    manifest = json.loads(manifest_path.read_text())
    require(manifest['cases'][case]['selector'] == SELECTOR, 'worker selector drift')
    refs, prior_elapsed, prior_retained, prior_logs = history(revision.parent, case)
    prior_process = sum(json.loads(Path(ref['path']).read_text())['process_wall_seconds'] for ref in refs)
    limit = 30.0 if args.profile == 1 else 90.0
    common.budget_remaining(prior_elapsed, time.monotonic() - started, limit)
    cap = (64 if args.profile == 1 else 128) << 20
    require(prior_retained + (24 << 20) + (8 << 20) + RECEIPT_RESERVE <= cap, 'native retained budget cannot admit another fresh database')
    require(prior_logs + (4 << 20) + RECEIPT_RESERVE < 8 << 20, 'native log budget cannot admit trace reserve')
    raw, raw_inputs = oracle.fixture(source)
    expected = oracle.expected_files(raw, args.profile)
    output = args.output.resolve()
    require(output.parent == revision, 'output must be a new revision child')
    output.mkdir()
    database, worker = output / 'database', output / 'worker'
    database.mkdir(); worker.mkdir()
    report = {'schema': SCHEMA, 'case': case, 'identity': identity, 'prior_receipts': refs,
              'prior_elapsed_seconds': prior_elapsed, 'raw_inputs': raw_inputs,
              'runner_sha256': sha(Path(__file__)), 'build_receipt_sha256': sha(revision / 'build-receipt.json'),
              'loaded_runner_bindings': bindings,
              'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'attempt_state': 'PLANNED', 'status': 'NOT_RUN', 'finalized': False,
              'elapsed_seconds': 0.0, 'retained_index': {}}
    common.save(output / 'result.json', report)
    try:
        clean.check_bindings(source, bindings)
        allowance = min(29.0, common.budget_remaining(prior_elapsed, time.monotonic() - started, limit) - 1.0)
        env = os.environ.copy()
        env.pop('XADB_PLAN4_EXCLUDE', None)
        env.update(XADB_PLAN4_INCLUDE=case, XADB_PLAN4_CLEAN_PROFILE=str(args.profile),
                   XADB_PLAN4_CLEAN_DB=str(database), XADB_PLAN4_CLEAN_OUT=str(worker))
        for key, value in zip(('SOURCE', 'ELF', 'INPUT'), identity):
            env[f'XADB_PLAN4_GENESIS_{key}_SHA256'] = value
        command = [str(binary), SELECTOR, '--exact', '--ignored', '--nocapture', '--test-threads=1']
        report.update(attempt_state='INVOKING', command=command)
        common.save(output / 'result.json', report)
        process = common.invoke(command, source, env, worker / 'stdout.log', worker / 'stderr.log', SELECTOR, [case], identity, allowance, allowance, (8 << 20) - prior_logs - (4 << 20) - RECEIPT_RESERVE)
        report.update(process=process, attempt_state='FINISHED')
        common.save(output / 'result.json', report)
        require(process['exact'], 'native worker failed or incomplete')
        files = oracle.compare_files(database, expected)
        report['oracle'] = validate_worker(worker, args.profile, identity, process, files)
        clean.validate_build(revision)
        require(raw_inputs == oracle.fixture(source)[1], 'raw fixture drift')
        for ref in refs:
            require(sha(ref['path']) == ref['sha256'], 'prior result changed')
            prior_root = Path(ref['path']).parent
            require({str(path.relative_to(prior_root)) for path in prior_root.rglob('*') if path.is_file()} ==
                    set(ref['index']) | {'result.json'}, 'prior retained pathname inventory changed')
            for name, value in ref['index'].items():
                require(sha(Path(ref['path']).parent / name) == value['sha256'], 'prior retained file changed')
        report['status'] = 'PASS'
    except (ValueError, OSError, KeyError) as error:
        report.update(status='PARTIAL_OR_FAIL', error=str(error))
    finally:
        try:
            clean.check_bindings(source, bindings)
        except (ValueError, OSError) as error:
            report.update(status='PARTIAL_OR_FAIL', error=str(error))
        report['retained_index'] = retained_index(output)
        report['finalized'] = report['attempt_state'] in ('PLANNED', 'FINISHED')
        report['measured_before_final_receipt_seconds'] = time.monotonic() - started
        # A finite 50 ms reserve includes the final bounded JSON write/stat.
        # This is a conservative accounting bound, not a performance sample.
        report['elapsed_seconds'] = report['measured_before_final_receipt_seconds'] + 0.05
        report['process_wall_seconds'] = report.get('process', {}).get('elapsed_seconds', 0.0)
        report['audit_wall_seconds'] = clean.audit_seconds(report['elapsed_seconds'], report['process_wall_seconds'])
        report['aggregate_audit_wall_seconds'] = clean.audit_seconds(prior_elapsed, prior_process) + report['audit_wall_seconds']
        report['aggregate_elapsed_seconds'] = prior_elapsed + report['elapsed_seconds']
        report['retained_bytes_with_receipt_reserve'] = prior_retained + sum(v['bytes'] for v in report['retained_index'].values()) + RECEIPT_RESERVE
        report['log_bytes_with_receipt_reserve'] = prior_logs + sum(v['bytes'] for name, v in report['retained_index'].items() if Path(name).suffix in {'.json', '.log', '.tsv', '.txt'}) + RECEIPT_RESERVE
        cap = (64 if args.profile == 1 else 128) << 20
        report['database_bytes'] = sum(value['bytes'] for name, value in report['retained_index'].items() if name.startswith('database/'))
        if report['database_bytes'] > 24 << 20 or report['retained_bytes_with_receipt_reserve'] > cap or report['log_bytes_with_receipt_reserve'] > 8 << 20 or report['aggregate_elapsed_seconds'] > limit:
            report.update(status='PARTIAL_OR_FAIL', error='aggregate native audit/time/bytes bound')
        common.save(output / 'result.json', report)
        # Final receipt size and the save/audit duration are within the same cap.
        require((output / 'result.json').stat().st_size <= RECEIPT_RESERVE, 'final receipt reserve exceeded')
        if time.monotonic() - started > report['elapsed_seconds']:
            report.update(status='PARTIAL_OR_FAIL', error='final receipt exceeded reserved audit time')
            report['elapsed_seconds'] = time.monotonic() - started + 0.05
            report['aggregate_elapsed_seconds'] = prior_elapsed + report['elapsed_seconds']
            report['audit_wall_seconds'] = clean.audit_seconds(report['elapsed_seconds'], report['process_wall_seconds'])
            report['aggregate_audit_wall_seconds'] = clean.audit_seconds(prior_elapsed, prior_process) + report['audit_wall_seconds']
            common.save(output / 'result.json', report)
    require(report['status'] == 'PASS', report.get('error', 'native run incomplete'))


if __name__ == '__main__':
    main()
