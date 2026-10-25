#!/usr/bin/env python3
"""Three new Linux Status flush DBs; frozen literal bytes and exact boundary trace.

No product encoder generates expected bytes. Existing installer literals supply
only the original namespace; new independently generated inputs supply every
Status/allocator page, WAL append, image, control and selector expected below.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_allocation_stack import wait_process_group
from dev3_doublewrite_native import native_objects
from dev3_allocator_flush_native import (inventory, parse_manifest, sample_process,
                                         check_resources, check_inventory)
import dev3_status_home_flush as runtime

ROOT = Path(__file__).resolve().parent.parent
SELECTOR = runtime.PREFIX + 'native::worker_status_home_flush_native'
PROFILES = (1, 2, 3)
CASE_SECONDS, DB_LIMIT, TOTAL_LIMIT, FD_LIMIT = 30, 24 << 20, 72 << 20, 5
WAL, HOME = 'd3-wal-00000001', 'd3-data-0000000100000001'
INPUT_REL = Path('crates/engine/tests/fixtures/dev3-status-home-flush/native/manifest.json')
NATIVE_SHA256 = 'f7d5cf6c7a7d3f458468cb2adcd1d9440faa91065dba7d22e3d2fcbbda4d7998'


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def sha(value):
    return hashlib.sha256(value).hexdigest()


def selected(profiles=None):
    names = list(PROFILES) if profiles is None else profiles
    require(names and len(names) == len(set(names)) and not set(names)-set(PROFILES), 'FAIL_NATIVE_SELECTION')
    return [p for p in PROFILES if p in names]


def native_input(root=ROOT):
    path = root/INPUT_REL
    require(digest(path) == NATIVE_SHA256, 'FAIL_NATIVE_INPUT_IDENTITY')
    spec = json.loads(path.read_text())
    require(spec['selector'] == SELECTOR and spec['profiles'] == [1, 2, 3]
            and spec['case_seconds'] == CASE_SECONDS and spec['database_bytes'] == DB_LIMIT
            and spec['total_bytes'] == TOTAL_LIMIT and spec['fd_limit'] == FD_LIMIT,
            'FAIL_NATIVE_INPUT_SCHEMA')
    runtime.check_inputs(root)
    for row in spec['dependencies']:
        path = root/row['path']
        require(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'], 'FAIL_NATIVE_DEPENDENCY')
    return spec


def literal(root, name):
    matches = [r for r in runtime.read_tsv(root/runtime.FIXTURE_REL/'literals.tsv') if r['logical'] == name]
    require(len(matches) == 1, 'FAIL_LITERAL_LOOKUP')
    row = matches[0]
    value = (root/runtime.FIXTURE_REL/row['blob']).read_bytes()
    require(len(value) == int(row['bytes']) and sha(value) == row['sha256'], 'FAIL_LITERAL_IDENTITY')
    return value


def initial_files(root, profile):
    files = native_objects(root, profile, bytes(range(1, 17)))
    bootstrap = bytearray(files['d3-bootstrap'])
    struct.pack_into('<Q', bootstrap, 120, 2)
    struct.pack_into('<Q', bootstrap, 280, 2)
    bootstrap[4064:] = hashlib.sha256(bootstrap[:4064]).digest()
    files['d3-bootstrap'] = bytes(bootstrap)
    return files


def batch_keys(profile):
    return (['native-P3-b1', 'native-P3-b2', 'native-P3-b3'] if profile == 3 else
            [f'p{profile}-c0-{"AS" if profile == 1 else "SA"}-b{i}' for i in (1, 2)])


def batch_row(root, key):
    rows = [r for r in runtime.read_tsv(root/runtime.FIXTURE_REL/'batches.tsv') if r['id'] == key]
    require(len(rows) == 1, 'FAIL_BATCH_LOOKUP')
    return rows[0]


def expected_files(root, profile, key):
    files = initial_files(root, profile)
    c = int(profile == 3)
    append = literal(root, f'p{profile}-c{c}/wal.frames-only')
    if profile == 3 and key == 'native-P3-b1':
        append = b''.join(literal(root, f'p3-c1/{name}') for name in
                          ('work1-reserve.wal', 'work1-cancel.wal', 'work2-reserve.wal'))
    files[WAL] += append
    rows = [r for r in runtime.read_tsv(root/runtime.FIXTURE_REL/'states.tsv') if r['state'] == key+'/after']
    regions = {r['region']: literal(root, r['logical']) for r in rows}
    require(len(rows) == len(regions) == 7, 'FAIL_STATE_CENSUS')
    files[HOME] = regions.pop('home:page0')+regions.pop('home:page1')+bytes(1048576-32768)
    files['d3-dwb-images'] = regions.pop('d3-dwb-images:slot0')+bytes(16711680-16544)
    files.update(regions)
    return files


def summaries(files):
    return {name: (len(value), sha(value)) for name, value in files.items()}


def flush_events(batch, target):
    rows = []
    def event(op, name, offset=0, size=0):
        rows.append((op, name, offset, size, size))
    for phase in range(3):
        bank = (3*batch-2+phase) % 2
        for name in ('d3-dwb-stage', f'd3-dwb-{bank}'):
            event('write', name, size=16384); event('sync', name)
        event('create', 'd3-dwb-select-stage')
        event('write', 'd3-dwb-select-stage', size=4096)
        event('sync', 'd3-dwb-select-stage')
        event('publish', 'd3-dwb-select-stage'); event('dirsync', '')
        if phase == 0:
            event('write', 'd3-dwb-images', size=16544); event('sync', 'd3-dwb-images')
        if phase == 1:
            event('write', HOME, offset=16384 if target == 'S' else 0, size=16384); event('sync', HOME)
    return rows


def campaign(profile):
    """Action ordering is independent of the Rust worker and literal encoder."""
    if profile == 1:
        return [('wal', 21200), ('wal', 38000), ('flush', (1, 'A')), ('flush', (2, 'S'))]
    if profile == 2:
        return [('wal', 21200), ('wal', 38000), ('flush', (1, 'S')), ('flush', (2, 'A'))]
    require(profile == 3, 'FAIL_NATIVE_SELECTION')
    return [('wal', 21200), ('wal', 21200), ('wal', 21200), ('flush', (1, 'A')),
            ('wal', 38000), ('flush', (2, 'S')), ('flush', (3, 'A'))]


def all_events(profile):
    events, offset = [], 4096
    for kind, value in campaign(profile):
        if kind == 'wal':
            events.extend([('write', WAL, offset, value, value), ('sync', WAL, 0, 0, 0)])
            offset += value
        else:
            events.extend(flush_events(*value))
    return events


def parse_events(text):
    result = []
    for line in text.splitlines():
        fields = line.split('\t')
        require(len(fields) == 5 and all(re.fullmatch(r'\d+', v) for v in fields[2:]), 'FAIL_EVENT_SHAPE')
        result.append((*fields[:2], *map(int, fields[2:])))
    return result


def flush_fd_events():
    # prepare: profile, DWB, bank, FILEMAP each open/read/close;
    # container one open/two reads/close; home one open/read/close.
    rows = [('open', 4), ('read', 4), ('close', 3)]*4
    rows += [('open', 4), ('read', 4), ('read', 4), ('close', 3)]
    rows += [('open', 4), ('read', 4), ('close', 3)]
    io = [('open', 4), ('write', 4), ('sync', 4), ('close', 3)]
    selection = io*2+[('create', 4)]+io+[('publish', 3), ('dirsync', 3)]
    return rows+selection+io+selection+io+selection


def expected_fd_events(profile):
    rows = [('baseline', 3)]
    for kind, _ in campaign(profile):
        rows += [('write', 3), ('sync', 3)] if kind == 'wal' else flush_fd_events()
    return rows+[('close', 0), ('final', 0)]


def check_fd(summary, trace, resource_text, profile):
    require(re.fullmatch(r'\d+\t\d+\t\d+\n', summary) is not None, 'FAIL_FD_SHAPE')
    values = tuple(map(int, summary.split()))
    rows = []
    for line in trace.splitlines():
        fields = line.split('\t')
        require(len(fields) == 2 and re.fullmatch(r'\d+', fields[1]), 'FAIL_FD_EVENT_SHAPE')
        rows.append((fields[0], int(fields[1])))
    require(rows == expected_fd_events(profile), 'FAIL_FD_EXACT_SEQUENCE')
    require(values == (3, max(n for _, n in rows), 0) and values[1] <= FD_LIMIT, 'FAIL_FD_SUMMARY')
    pairs = [line.split('\t') for line in resource_text.splitlines()]
    wanted = [('wrapped_peak', 2), ('wrapped_final', 0), ('observed_fd_peak', values[1]), ('observed_fd_final', 0)]
    require(pairs == [[k, str(v)] for k, v in wanted], 'FAIL_RESOURCE_CENSUS')
    return dict(baseline=3, boundary_peak=values[1], final=0, observations=len(rows),
                scope='exact source boundary observations; direct inventory and continuous peaks excluded')


def check_stdout(text, profile, db_bytes):
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; \d+ filtered out; finished in [\d.]+s$', text, re.M) == [('1','0','0','0')], 'FAIL_TEST_CENSUS')
    require(re.findall(r'^test ([\w:]+) \.\.\.', text, re.M) == [SELECTOR], 'FAIL_TEST_SELECTOR')
    lines = [re.sub(r'^test [\w:]+ \.\.\. ', '', line) for line in text.splitlines()]
    marker = f'STATUS_FLUSH_NATIVE_COMPLETE profile={profile} cancellations={int(profile == 3)} batches={3 if profile == 3 else 2} effects={93 if profile == 3 else 62} bytes={db_bytes}'
    require([line for line in lines if line.startswith('STATUS_FLUSH_')] == [marker], 'FAIL_NATIVE_MARKER')


def check_case(case, root, profile):
    keys = batch_keys(profile)
    names = {'database','before.tsv','after.tsv','wal-header.bin','wal.complete','allocator.after','status.after',
             'owner.descriptor','owner-state.tsv','ram.tsv','events.tsv','fd.tsv','fd-events.tsv','resources.tsv','stdout.log','stderr.log','process.json'}
    names.update(f'batch{i}.{suffix}.tsv' for i in range(1,len(keys)+1) for suffix in ('events','after','clean'))
    require({p.name for p in case.iterdir()} == names and not any(p.is_symlink() for p in case.iterdir()), 'FAIL_ARTIFACT_CENSUS')
    before = parse_manifest(case/'before.tsv')
    initial = initial_files(root, profile)
    final = expected_files(root, profile, keys[-1])
    for batch, key in enumerate(keys, 1):
        current = parse_manifest(case/f'batch{batch}.after.tsv')
        expected = expected_files(root, profile, key)
        check_inventory(before, current, summaries(expected), summaries(initial))
        row = batch_row(root, key)
        target, lsn = row['target'], int(row['lsn'])
        require(parse_events((case/f'batch{batch}.events.tsv').read_text()) == flush_events(batch,target), 'FAIL_FLUSH_EFFECTS')
        counter = lsn if target == 'A' else 1
        require((case/f'batch{batch}.clean.tsv').read_text() == f'{target}\t{lsn}\t{counter}\ttrue\ttrue\ttrue\t{batch}\t{3*batch+1}\t0\n', 'FAIL_CLEAN_FACTS')
    current = inventory(case/'database')
    require(current == parse_manifest(case/'after.tsv') == parse_manifest(case/f'batch{len(keys)}.after.tsv'), 'FAIL_FINAL_INVENTORY')
    check_inventory(before, current, summaries(final), summaries(initial))
    require(all((case/'database'/name).read_bytes() == value for name,value in final.items()), 'FAIL_FINAL_FULL_BYTES')
    c, lsn = int(profile == 3), 4 if profile == 3 else 2
    stem = f'p{profile}-c{c}/work{c+1}'
    dumps = {'wal-header.bin':initial[WAL],'wal.complete':final[WAL],
             'allocator.after':literal(root,stem+'-allocator.after'),'status.after':literal(root,stem+'-status.after'),
             'owner.descriptor':literal(root,stem+'-init.descriptor')}
    require(all((case/name).read_bytes() == value for name,value in dumps.items()), 'FAIL_RAM_WAL_DESCRIPTOR')
    owner_names = ('status_installation_none','base_installation_none','base_batch_none',
                   'terminal_pending_false','flush_capture_none','flush_retired_none',
                   'flush_authority_none','flush_phase_ready','healthy')
    require((case/'owner-state.tsv').read_text() == ''.join(name+'\ttrue\n' for name in owner_names),
            'FAIL_FINAL_OWNER_STATE')
    require((case/'ram.tsv').read_text() == f'{lsn}\t{lsn}\t1\tfalse\tfalse\ttrue\ttrue\t0\ttrue\n', 'FAIL_RAM_FACTS')
    require(parse_events((case/'events.tsv').read_text()) == all_events(profile), 'FAIL_CAMPAIGN_EFFECTS')
    fd = check_fd((case/'fd.tsv').read_text(),(case/'fd-events.tsv').read_text(),(case/'resources.tsv').read_text(),profile)
    db_bytes = sum(r[0] for r in current.values())
    check_stdout((case/'stdout.log').read_text(),profile,db_bytes)
    return dict(status='PASS',profile=profile,batches=len(keys),logical_flush_effects=31*len(keys),
                database_bytes=db_bytes,files=current,fd=fd,**check_resources(db_bytes,len(final[WAL])),
                evidence={p.name:digest(p) for p in sorted(case.iterdir()) if p.is_file()})


def checker_identities(root):
    paths = {Path(m.__file__).resolve() for m in list(sys.modules.values())
             if getattr(m,'__file__',None) and Path(m.__file__).suffix == '.py'
             and Path(m.__file__).resolve().parent == Path(__file__).resolve().parent}
    paths.add(Path(__file__).resolve())
    paths.add(Path(__file__).with_name('test_dev3_status_home_flush_native.py'))
    values = {str(p.relative_to(Path(__file__).resolve().parent.parent)):digest(p) for p in paths}
    require(all(digest(root/name) == value for name,value in values.items()), 'FAIL_CHECKER_IDENTITY')
    return values


def run(root,binary,output,build_receipt,profiles=None):
    root,binary,output,build_receipt = root.resolve(),binary.resolve(),output.resolve(),build_receipt.resolve()
    require(sys.platform.startswith('linux'),'FAIL_PLATFORM')
    source,binary_hash,build_hash = source_fingerprint(root),digest(binary),digest(build_receipt)
    runtime.check_build_receipt(json.loads(build_receipt.read_text()),root,source,binary_hash)
    spec=native_input(root);chosen=selected(profiles);checkers=checker_identities(root)
    output.mkdir(parents=True,exist_ok=False)
    retained=output/f'status-flush-native-{binary_hash}';shutil.copyfile(binary,retained);retained.chmod(0o500)
    with retained.open('rb') as stream:require(stream.read(4)==b'\x7fELF','FAIL_BINARY_FORMAT')
    report=dict(status='RUNNING',scope='new Status home Linux flushes; no reopen/power-loss credit',source_root=str(root),
                source_sha256=source,binary=str(retained),binary_sha256=binary_hash,build_receipt=str(build_receipt),
                build_receipt_sha256=build_hash,native_input_sha256=NATIVE_SHA256,checker_sha256=checkers,
                selected_profiles=chosen,not_selected=[p for p in PROFILES if p not in chosen],native_cases=[],
                case_seconds=CASE_SECONDS,database_limit=DB_LIMIT,total_limit=TOTAL_LIMIT)
    try:
        for profile in chosen:
            case=output/f'p{profile}';case.mkdir()
            command=[str(retained),'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1']
            environment={k:v for k,v in os.environ.items() if not k.startswith(('XADB_','XADBTEST_','RUST_TEST_'))}
            environment.update(XADB_STATUS_FLUSH_NATIVE_DIR=str(case),XADB_STATUS_FLUSH_NATIVE_PROFILE=str(profile))
            process=dict(command=command,binary_sha256=binary_hash,database_root=str(case/'database'),timeout_seconds=CASE_SECONDS,
                         timed_out=False,exit_code=None,fd_samples=0,sampled_database_fd_peak=0,sampled_database_bytes_peak=0,
                         sample_errors=[],proc_terminal_unavailable=[])
            row=dict(profile=profile,status='RUNNING',process=process);report['native_cases'].append(row)
            started=time.monotonic();stop=threading.Event();sampler=None
            try:
                with (case/'stdout.log').open('wb') as stdout,(case/'stderr.log').open('wb') as stderr:
                    with subprocess.Popen(command,cwd=root,env=environment,stdout=stdout,stderr=stderr,start_new_session=True) as child:
                        process['pid']=child.pid
                        sampler=threading.Thread(target=sample_process,args=(child,case/'database',stop,process),daemon=True);sampler.start()
                        wait_process_group(child,process,CASE_SECONDS)
            finally:
                stop.set()
                if sampler:
                    sampler.join(timeout=1)
                    if sampler.is_alive():process['sample_errors'].append('sampler did not terminate')
                process['elapsed_seconds']=time.monotonic()-started
                (case/'process.json').write_text(json.dumps(process,indent=2)+'\n')
            require(not process['timed_out'] and process['elapsed_seconds']<=CASE_SECONDS,'FAIL_TIMEOUT')
            require(process['exit_code']==0,'FAIL_WORKER_EXIT')
            require(not process['sample_errors'] and process['fd_samples']>0,'FAIL_PROC_OBSERVATION')
            require(process['sampled_database_fd_peak']<=FD_LIMIT,'FAIL_SAMPLED_FD_BOUND')
            require(process['sampled_database_bytes_peak']<=DB_LIMIT,'FAIL_SAMPLED_DATABASE_BOUND')
            row.update(check_case(case,root,profile))
            require(sum(r.get('database_bytes',0) for r in report['native_cases'])<=TOTAL_LIMIT,'FAIL_TOTAL_BUDGET')
        require(source_fingerprint(root)==source and digest(binary)==digest(retained)==binary_hash and digest(build_receipt)==build_hash,'FAIL_SOURCE_BINARY_BUILD_DRIFT')
        native_input(root)
        require(checker_identities(root)==checkers,'FAIL_CHECKER_DRIFT')
        report['status']='PASS'
    except BaseException as error:
        report.update(status='FAIL',failure=str(error))
        for row in report['native_cases']:
            if row['status']=='RUNNING':row['status']='FAIL'
        raise
    finally:
        report['not_run']=[p for p in chosen if p not in {r['profile'] for r in report['native_cases']}]
        report['databases_completed']=sum(r['status']=='PASS' for r in report['native_cases'])
        report['runtime_seconds']=sum(r['process'].get('elapsed_seconds',0) for r in report['native_cases'])
        (output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=ROOT)
    for name in ('binary','output','build-receipt'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--profile',type=int,action='append')
    args=parser.parse_args();result=run(args.root,args.binary,args.output,args.build_receipt,args.profile)
    print(json.dumps(dict(status=result['status'],databases_completed=result['databases_completed'])))


if __name__=='__main__':main()
