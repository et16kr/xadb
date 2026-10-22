#!/usr/bin/env python3
"""S5-only actual 1000x63 producer; independent final RAM state, source WAL unchanged."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
sys.dont_write_bytecode=True
import dev3_recovery_prefix_native as native
from check_dev0 import digest, source_fingerprint

ROOT=native.ROOT
SELECTOR=native.SELECTOR.rsplit('::',1)[0]+'::worker_recovery_prefix_long_native'
INPUT=Path('crates/engine/tests/fixtures/recovery_prefix/native/long_manifest.json')
INPUT_SHA256='f6f632f72d56e1fb5cf73971e3f89412d8a0e1296a2ea7ba7e23abf53d60fbc9'
CASES={'PL01':dict(profile=1,plan=1,kind='EC',context='long63',cycles=1000,pages=63)}
CASE_SECONDS=TOTAL_SECONDS=300
DB_LIMIT=TOTAL_LIMIT=128<<20
WAL_LENGTH=68196096
WAL='d3-wal-00000001'
require=native.require


def selected(values):
    require(values==['PL01'],'FAIL_S5_EXPLICIT_SELECTION')
    return values


def oracle(root):
    """Patch only published final-state fields of independent N63/cancel literals."""
    base=Path('tests/fixtures/dev3-allocation-reservation/p1-n63-cancel')
    allocator=bytearray(native.read(root,Path(str(base)+'.page')))
    descriptor=bytearray(native.read(root,Path(str(base)+'.descriptor')))
    def put(raw,offset,value):raw[offset:offset+8]=value.to_bytes(8,'little')
    put(allocator,64,2000)
    for i in range(1,64):put(allocator,10144+64+96*i+8,1001)
    allocator[-32:]=bytes.fromhex(native.sha(allocator[:-32]))
    for offset,value in ((56,1000),(72,2000),(80,1999),(88,1999),(544,2000)):
        put(descriptor,offset,value)
    descriptor[-32:]=bytes.fromhex(native.sha(descriptor[:-32]))
    files=native.initial_files(root,1,1)
    owned=sum(map(len,files.values()))+WAL_LENGTH-4096
    facts=dict(database=native.DATABASE.hex(),timeline='1',profile='1',plan='1',filemap_generation='1',dwb_generation='1',complete_lsn='2000',complete_offset=str(WAL_LENGTH),source_length=str(WAL_LENGTH),work_high_water='1000',active_id='0',active_mask='0',mode='Some(Exclusive)',inventory_items='0',inventory_digest=native.sha(b''),retained_wal=str(WAL_LENGTH-4096),remaining_wal='0',remaining_pages='0',remaining_memory='0',remaining_frames='0',remaining_files='0',remaining_protected='0',terminal_hold_mask=str((1<<64)-2),source_stabilized='false',replay_page_topup_bytes='16384',owned_bytes=str(owned),protected_bytes=str(21024768+WAL_LENGTH-4096+16384),dwb_batch='0',dwb_state='Empty',tail_present='false',tail_offset='0',tail_bytes='0',tail_declared='0',tail_inspected='0',pending_present='false',pending_slot='0',pending_page='0',pending_incarnation='0',pending_owner='0',pending_owner_generation='0',pending_kind='0',pending_level='0',pending_lsn='0',pending_digest='')
    return files,bytes(allocator),bytes(descriptor),facts


def inventory(db):
    values={}
    for path in sorted(db.iterdir()):
        meta=path.lstat()
        require(path.is_file() and not path.is_symlink() and meta.st_nlink==1,'FAIL_DATABASE_FILE_TYPE')
        values[path.name]=dict(bytes=meta.st_size,sha256=digest(path),device=meta.st_dev,inode=meta.st_ino,links=meta.st_nlink)
    require(sum(r['bytes'] for r in values.values())<=DB_LIMIT,'FAIL_S5_DATABASE_BOUND')
    return values


def check_files(db,files):
    before=inventory(db)
    require(set(before)==set(files),'FAIL_S5_FILE_CENSUS')
    require(before[WAL]['bytes']==WAL_LENGTH,'FAIL_S5_WAL_LENGTH')
    with (db/WAL).open('rb') as stream:
        require(stream.read(4096)==files[WAL],'FAIL_S5_GENESIS_HEADER')
    require(all((db/name).read_bytes()==raw for name,raw in files.items() if name!=WAL),'FAIL_S5_FIXED_FILES')
    return before


def validate_inputs(root):
    require(digest(root/INPUT)==INPUT_SHA256,'FAIL_S5_INPUT_IDENTITY')
    spec=json.loads((root/INPUT).read_text())
    require(spec['cases']==CASES and spec['selector']==SELECTOR and spec['case_seconds']==CASE_SECONDS and spec['total_seconds']==TOTAL_SECONDS and spec['database_bytes']==DB_LIMIT and spec['retained_database_bytes']==TOTAL_LIMIT,'FAIL_S5_MANIFEST_SCHEMA')
    require(digest(root/spec['generator']['path'])==spec['generator']['sha256'],'FAIL_S5_GENERATOR_IDENTITY')
    for row in spec['dependencies']:
        path=root/row['path'];require(path.stat().st_size==row['bytes'] and digest(path)==row['sha256'],'FAIL_S5_TEMPLATE_IDENTITY')
    files,allocator,descriptor,facts=oracle(root)
    expected=dict(fixed_files={name:dict(bytes=len(raw),sha256=native.sha(raw)) for name,raw in files.items()},allocator_sha256=native.sha(allocator),descriptor_sha256=native.sha(descriptor),facts=facts)
    require(spec['expected']==expected,'FAIL_S5_EXPECTED_IDENTITY')
    return spec,files,allocator,descriptor,facts


def check_inspection(folder,db,allocator,descriptor,facts):
    require((folder/'allocator.bin').read_bytes()==allocator and (folder/'descriptor.bin').read_bytes()==descriptor and not (folder/'status.bin').exists(),'FAIL_S5_FINAL_RAM_IMAGES')
    pairs=[line.split('\t') for line in (folder/'observation.tsv').read_text().splitlines()]
    require(len(pairs)==len({p[0] for p in pairs}),'FAIL_S5_OBSERVATION_CENSUS')
    actual=dict(pairs);meta=(db/WAL).stat()
    expected=dict(facts,source_device=str(meta.st_dev),source_inode=str(meta.st_ino))
    require(actual==expected,'FAIL_S5_FINAL_FACTS')
    return actual


def run(root,binary,receipt,output,names):
    selected(names)
    root,binary,receipt,output=(p.resolve() for p in (root,binary,receipt,output))
    spec,files,allocator,descriptor,facts=validate_inputs(root)
    source,binary_hash,receipt_hash=source_fingerprint(root),digest(binary),digest(receipt)
    native.check_build_receipt(json.loads(receipt.read_text()),root,source,binary_hash);native.check_binary_format(binary)
    checkers=native.checker_identities(root)
    output.mkdir(parents=True,exist_ok=False);retained=output/'native-binary';shutil.copyfile(binary,retained);retained.chmod(0o500)
    folder=output/'PL01';folder.mkdir();db=folder/'database';db.mkdir()
    report=dict(status='RUNNING',unit='D3-RECOVERY-PREFIX-001',group='s5_long',selected=['PL01'],scope=spec['oracle_scope'],source_root=str(root),source_sha256=source,binary_sha256=binary_hash,build_receipt_sha256=receipt_hash,input_sha256=INPUT_SHA256,checker_sha256=checkers,processes=[])
    started=time.monotonic();active=[]
    try:
        for mode in ('produce','analyze'):
            phase=folder/mode;phase.mkdir()
            env={k:v for k,v in os.environ.items() if not k.startswith(('XADB_','XADBTEST_','RUST_TEST_'))}
            env.update(XADB_RECOVERY_PREFIX_NATIVE_DIR=str(phase),XADB_RECOVERY_PREFIX_NATIVE_DB=str(db),XADB_RECOVERY_PREFIX_NATIVE_MODE=mode,XADB_RECOVERY_PREFIX_NATIVE_PROFILE='1',XADB_RECOVERY_PREFIX_NATIVE_KIND='EC',XADB_RECOVERY_PREFIX_NATIVE_CONTEXT='long63')
            command=[str(retained),'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1']
            stdout=(phase/'stdout.log').open('wb');stderr=(phase/'stderr.log').open('wb')
            child=subprocess.Popen(command,cwd=root,env=env,stdout=stdout,stderr=stderr,start_new_session=True)
            row=dict(mode=mode,pid=child.pid,command=command,timed_out=False,exit_code=None);report['processes'].append(row)
            job=(child,stdout,stderr);active.append(job);began=time.monotonic()
            def remaining():
                left=TOTAL_SECONDS-(time.monotonic()-started);require(left>0,'FAIL_S5_AGGREGATE_TIMEOUT');return left
            try:native.wait_child(child,remaining,row)
            finally:
                stdout.close();stderr.close();row.update(exit_code=child.poll(),elapsed_seconds=time.monotonic()-began);(phase/'process.json').write_text(json.dumps(row,indent=2)+'\n')
                if child.poll() is not None:active.remove(job)
            require(row['exit_code']==0 and not row['timed_out'] and 0<=row['elapsed_seconds']<=CASE_SECONDS,'FAIL_S5_PROCESS')
            native.check_stdout((phase/'stdout.log').read_text(),mode,child.pid,'produced' if mode=='produce' else 'analyzed',SELECTOR)
            row['resources']=native.check_resources(phase,db,mode=='analyze')
            if mode=='produce':report['before']=check_files(db,files)
            else:report['observation']=check_inspection(phase,db,allocator,descriptor,facts)
        require(len({p['pid'] for p in report['processes']})==2,'FAIL_S5_FRESH_PID')
        report['after']=inventory(db);require(report['after']==report['before'],'FAIL_S5_DATABASE_CHANGED')
        require(source_fingerprint(root)==source and digest(binary)==digest(retained)==binary_hash and digest(receipt)==receipt_hash and validate_inputs(root)[0]==spec and native.checker_identities(root)==checkers,'FAIL_S5_POST_IDENTITY')
        native.check_total_budget(time.monotonic()-started)
        report.update(status='PASS',database_bytes=sum(r['bytes'] for r in report['after'].values()))
    except BaseException as error:
        report.update(status='FAIL',error=str(error));raise
    finally:
        for child,stdout,stderr in active:
            native.kill_and_reap(child);stdout.close();stderr.close()
        report['elapsed_seconds']=time.monotonic()-started;(output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=ROOT)
    for name in ('binary','build-receipt','output'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--case',action='append',required=True);args=parser.parse_args()
    report=run(args.root,args.binary,args.build_receipt,args.output,args.case);print(json.dumps(dict(status=report['status'],completed=1)))


if __name__=='__main__':main()
