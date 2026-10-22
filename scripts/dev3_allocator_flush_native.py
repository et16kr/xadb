#!/usr/bin/env python3
"""Run only three new Linux allocator-flush DBs; retain exact two-batch evidence.

No product encoder/decoder generates expected bytes. Initial selected objects
come from the pinned installer manifest; new home/slot/control/selector bytes
come from the independently generated flush literals, WAL from dirty literals.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode=True
from check_dev0 import digest,source_fingerprint
from check_dev3_allocation_stack import wait_process_group
from check_dev3_allocator_flush_runtime import verify_build,verify_inputs

FIXTURES=Path('tests/fixtures/dev3-allocator-flush-one')
DIRTY=Path('tests/fixtures/dev3-dirty-allocation')
INSTALL=Path('tests/fixtures/dev3-doublewrite-install')
SELECTOR=('storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
          'allocator_flush::native_tests::flush_native_worker')
PROFILES=(1,2,3)
CASE_SECONDS=30
DB_LIMIT=24<<20
TOTAL_LIMIT=72<<20
FD_LIMIT=5
FIXED_OWNED=22073344
WAL_HEADER=4096
SELECTOR_STAGE=4096
WAL='d3-wal-00000001'
HOME='d3-data-0000000100000001'
CHECKER_NAMES=('dev3_allocator_flush_native.py','test_dev3_allocator_flush_native.py',
               'check_dev3_allocator_flush_runtime.py','check_dev0.py','check_dev3_allocation_stack.py','comm_campaign.py')


def require(ok,reason):
    if not ok:raise ValueError(reason)


def sha(data):return hashlib.sha256(data).hexdigest()


def zero_tail_hash(prefix,tail):
    require(tail>=0,'FAIL_LITERAL_SIZE')
    h=hashlib.sha256(prefix)
    chunk=bytes(65536)
    while tail:
        count=min(tail,len(chunk));h.update(chunk[:count]);tail-=count
    return h.hexdigest()


def select(profiles=None):
    profiles=list(PROFILES) if profiles is None else profiles
    require(profiles and len(profiles)==len(set(profiles)) and not set(profiles)-set(PROFILES),'FAIL_NATIVE_CENSUS')
    return [p for p in PROFILES if p in profiles]


def parse_manifest(path):
    rows={}
    for line in path.read_text().splitlines():
        fields=line.split('\t');require(len(fields)==6,'FAIL_MANIFEST_SHAPE')
        name,size,device,inode,links,checksum=fields
        require(name and name not in ('.','..') and Path(name).name==name and name not in rows and re.fullmatch(r'[0-9a-f]{64}',checksum),'FAIL_FILE_CENSUS')
        numbers=[int(v) for v in (size,device,inode,links)]
        require(numbers[0]>=0 and numbers[1]>=0 and numbers[2]>0 and numbers[3]==1,'FAIL_FILE_IDENTITY')
        rows[name]=numbers+[checksum]
    return rows


def inventory(database):
    require(database.is_dir() and not database.is_symlink(),'FAIL_DATABASE_PATH')
    rows={}
    for path in sorted(database.iterdir()):
        meta=path.lstat()
        require(stat.S_ISREG(meta.st_mode) and meta.st_nlink==1,'FAIL_FILE_TYPE')
        rows[path.name]=[meta.st_size,meta.st_dev,meta.st_ino,meta.st_nlink,digest(path)]
    return rows


def frozen_inputs(root):
    manifest=json.loads((root/FIXTURES/'runtime-manifest.json').read_text())
    identities=verify_inputs(root,manifest,digest(root/FIXTURES/'mandatory-cases.tsv'),digest(root/FIXTURES/'expanded-cases.tsv'))
    oracle=json.loads((root/FIXTURES/'manifest.json').read_text())
    installed_path=INSTALL/'manifest.json'
    require(digest(root/installed_path)==oracle['provenance_manifests'][str(installed_path)],'FAIL_INITIAL_MANIFEST_IDENTITY')
    identities[str(installed_path)]=digest(root/installed_path)
    dirty_path=DIRTY/'manifest.json'
    require(digest(root/dirty_path)==oracle['provenance_manifests'][str(dirty_path)],'FAIL_DIRTY_MANIFEST_IDENTITY')
    identities[str(dirty_path)]=digest(root/dirty_path)
    dirty=json.loads((root/dirty_path).read_text())
    rows={r['path']:r for r in dirty['literals']}
    for profile in PROFILES:
        for part in ('reserve','cancel'):
            name=f'p{profile}-n1-{part}.frame';r=rows[name];path=root/DIRTY/name
            require(path.stat().st_size==r['bytes'] and digest(path)==r['sha256'],'FAIL_WAL_LITERAL_IDENTITY')
            identities[str(DIRTY/name)]=r['sha256']
    plan_path=FIXTURES/'native-manifest.json';plan=json.loads((root/plan_path).read_text())
    require(plan==native_plan(),'FAIL_NATIVE_PLAN')
    identities[str(plan_path)]=digest(root/plan_path)
    return identities


def native_plan():
    return {'schema':'xadb-dev3-allocator-flush-native-v1','status':'PREEXECUTION_NOT_RUN',
            'profiles':list(PROFILES),'selector':SELECTOR,'batches_per_database':2,'logical_effects_per_database':62,
            'case_timeout_seconds':CASE_SECONDS,'database_limit_bytes':DB_LIMIT,'total_limit_bytes':TOTAL_LIMIT,
            'fixed_owned_budget_bytes':FIXED_OWNED,'database_fd_limit':FD_LIMIT,'tracked_file_handles_limit':2,
            'fixed_owned_charge':'database_bytes - wal_bytes + initial_wal_header_bytes + selector_stage_bytes',
            'initial_wal_header_bytes':WAL_HEADER,'selector_stage_bytes':SELECTOR_STAGE,
            'namespace_handles':2,'caller_directory_dropped_before_setup':True,
            'fd_baseline':3,'fd_final':0,'fd_evidence':['fd.tsv','fd-events.tsv'],
            'database_subdirectory':'database','batch_inventories':['reserve.after.tsv','cancel.after.tsv'],
            'environment':['XADB_FLUSH_NATIVE_DIR','XADB_FLUSH_NATIVE_PROFILE'],
            'recovery':'NOT_IMPLEMENTED_OR_CLAIMED','power_loss':'NOT_TESTED'}


def expected_events(batch):
    require(batch in (1,2),'FAIL_BATCH')
    rows=[]
    for phase in range(3):
        if phase==1:rows.extend([['write','d3-dwb-images','0','16544','16544'],['sync','d3-dwb-images','0','0','0']])
        if phase==2:rows.extend([['write',HOME,'0','16384','16384'],['sync',HOME,'0','0','0']])
        generation=3*(batch-1)+2+phase;bank=f'd3-dwb-{(generation-1)%2}'
        rows.extend([['write','d3-dwb-stage','0','16384','16384'],['sync','d3-dwb-stage','0','0','0'],
                     ['write',bank,'0','16384','16384'],['sync',bank,'0','0','0'],
                     ['create','d3-dwb-select-stage','0','0','0'],['write','d3-dwb-select-stage','0','4096','4096'],
                     ['sync','d3-dwb-select-stage','0','0','0'],['publish','d3-dwb-select-stage','0','0','0'],
                     ['dirsync','','0','0','0']])
    return rows


def check_events(rows,batch):
    require(rows==expected_events(batch),'FAIL_FLUSH_EFFECT_SEQUENCE')


def check_clean(values,batch):
    require(values==[str(batch),str(batch),'true','true','true',str(batch),str(3*batch+1)],'FAIL_CLEAN_FINALIZE')


def initial_expected(root,profile):
    m=json.loads((root/INSTALL/'manifest.json').read_text())
    result={name:(r['bytes'],r['sha256']) for name,r in m['installed_files'][str(profile)].items()}
    result['LOCK']=(0,sha(b''))
    return result


def expected_after(root,profile,batch,header):
    result=initial_expected(root,profile)
    part='reserve' if batch==1 else 'cancel';stem=f'p{profile}-n1-{part}'
    literal=lambda suffix:(root/FIXTURES/(stem+'-'+suffix)).read_bytes()
    wal=header+(root/DIRTY/f'p{profile}-n1-reserve.frame').read_bytes()
    if batch==2:wal+=(root/DIRTY/f'p{profile}-n1-cancel.frame').read_bytes()
    result[WAL]=(len(wal),sha(wal))
    home=literal('home.after');image=literal('slot.bin')
    result[HOME]=(1048576,zero_tail_hash(home,1048576-len(home)))
    result['d3-dwb-images']=(16711680,zero_tail_hash(image,16711680-len(image)))
    retired_bank=(3*batch)%2
    for name,suffix in [('d3-dwb-stage','retirement.bank'),(f'd3-dwb-{retired_bank}','retirement.bank'),
                        (f'd3-dwb-{1-retired_bank}','pending.bank'),('DWB','retirement.selector')]:
        data=literal(suffix);result[name]=(len(data),sha(data))
    return result


def check_inventory(before,current,expected,initial):
    require(set(before)==set(current)==set(expected)==set(initial),'FAIL_FILE_CENSUS')
    require(len({tuple(row[1:3]) for row in current.values()})==len(current),'FAIL_FILE_ALIAS')
    for name,(length,checksum) in initial.items():
        require((before[name][0],before[name][4])==(length,checksum),'FAIL_INITIAL_IMAGE')
        require(before[name][3]==1 and current[name][3]==1,'FAIL_LINK_COUNT')
        if name=='DWB':
            # Multiple replacements may legitimately recycle an old inode.
            require(current[name][1]==before[name][1],'FAIL_SELECTOR_DEVICE')
        else:require(current[name][1:4]==before[name][1:4],'FAIL_UNEXPECTED_REPLACEMENT')
        require((current[name][0],current[name][4])==expected[name],'FAIL_EXPECTED_IMAGE_'+name)
    require(sum(r[0] for r in current.values())<=DB_LIMIT,'FAIL_DATABASE_BUDGET')


def check_stdout(text,profile,db_bytes):
    summaries=re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; \d+ filtered out; finished in [\d.]+s$',text,re.M)
    require(summaries==[('1','0','0','0')],'FAIL_TEST_CENSUS')
    require(re.findall(r'^test ([\w:]+) \.\.\.',text,re.M)==[SELECTOR],'FAIL_TEST_SELECTOR')
    markers=re.findall(r'^(?:test [\w:]+ \.\.\. )?FLUSH_NATIVE_COMPLETE profile=(\d+) batches=(\d+) effects=(\d+) bytes=(\d+) peak_files=(\d+)$',text,re.M)
    protocol=[re.sub(r'^test [\w:]+ \.\.\. ','',line) for line in text.splitlines()]
    require(len(markers)==1 and sum(line.startswith('FLUSH_NATIVE') for line in protocol)==1,'FAIL_NATIVE_MARKER')
    p,b,e,n,peak=map(int,markers[0]);require((p,b,e,n)==(profile,2,62,db_bytes),'FAIL_NATIVE_MARKER')
    require(1<=peak<=2,'FAIL_TRACKED_FILE_CENSUS')
    return {'profile':p,'batches':b,'logical_effects':e,'database_bytes':n,'tracked_file_peak':peak,
            'fixed_namespace_handles':2,'tracked_wrapper_and_namespace_bound':2+peak}


def check_resources(db_bytes,wal_bytes):
    require(WAL_HEADER<=wal_bytes<=db_bytes,'FAIL_WAL_RESOURCE_SHAPE')
    # The fixed reservation includes the initial WAL header, not append growth.
    # Actual final/selector-replacement bytes remain independently bounded.
    fixed=db_bytes-wal_bytes+WAL_HEADER+SELECTOR_STAGE
    replacement=db_bytes+SELECTOR_STAGE
    require(fixed<=FIXED_OWNED,'FAIL_FIXED_OWNED_BOUND')
    require(db_bytes<=DB_LIMIT and replacement<=DB_LIMIT,'FAIL_DATABASE_BUDGET')
    return {'fixed_owned_charge_bytes':fixed,'fixed_owned_budget_bytes':FIXED_OWNED,
            'wal_bytes':wal_bytes,'wal_append_bytes':wal_bytes-WAL_HEADER,
            'replacement_peak_bound_bytes':replacement}


def check_fd(summary,rows):
    require(re.fullmatch(r'\d+\t\d+\t\d+\n',summary) is not None,'FAIL_FD_SHAPE')
    baseline,peak,final=map(int,summary.split())
    require(baseline==3 and final==0 and baseline<=peak<=FD_LIMIT,'FAIL_FD_CENSUS')
    allowed={'baseline','open','read','write','sync','create','publish','dirsync','close','final'}
    parsed=[]
    for row in rows:
        require(len(row)==2 and row[0] in allowed and re.fullmatch(r'\d+',row[1]) is not None,'FAIL_FD_EVENT_SHAPE')
        count=int(row[1]);require(count<=FD_LIMIT,'FAIL_FD_CENSUS');parsed.append((row[0],count))
    require(len(parsed)>=2 and parsed[0]==('baseline',baseline) and parsed[-1]==('final',final),'FAIL_FD_ENDPOINTS')
    require(sum(tag=='baseline' for tag,_ in parsed)==1 and sum(tag=='final' for tag,_ in parsed)==1,'FAIL_FD_ENDPOINTS')
    require(max(count for _,count in parsed)==peak,'FAIL_FD_PEAK')
    actual=[tag for tag,_ in parsed if tag in {'write','sync','create','publish','dirsync'}]
    require(actual==[row[0] for batch in (1,2) for row in expected_events(batch)],'FAIL_FD_EFFECT_CENSUS')
    require(all(any(tag==needed for tag,_ in parsed) for needed in ('open','read','close')),'FAIL_FD_OBSERVATION_CENSUS')
    return {'baseline':baseline,'observed_boundary_peak':peak,'final':final,'boundary_observations':len(parsed),
            'database_fd_limit':FD_LIMIT,'includes_direct_inventory_sampling':False}


def evaluate_case(case,root,profile):
    expected_names={'database','before.tsv','reserve.after.tsv','cancel.after.tsv','after.tsv','header.bin',
                    'reserve.events.tsv','cancel.events.tsv','reserve.clean.tsv','cancel.clean.tsv',
                    'fd.tsv','fd-events.tsv','stdout.log','stderr.log','process.json'}
    require({p.name for p in case.iterdir()}==expected_names,'FAIL_ARTIFACT_CENSUS')
    initial=initial_expected(root,profile);header=(case/'header.bin').read_bytes()
    require((len(header),sha(header))==initial[WAL],'FAIL_INITIAL_WAL')
    before=parse_manifest(case/'before.tsv');reserve=parse_manifest(case/'reserve.after.tsv');after=parse_manifest(case/'after.tsv')
    cancel=parse_manifest(case/'cancel.after.tsv')
    actual=inventory(case/'database');require(after==actual==cancel,'FAIL_EXTERNAL_IMAGE')
    check_inventory(before,reserve,expected_after(root,profile,1,header),initial)
    check_inventory(before,after,expected_after(root,profile,2,header),initial)
    for batch,part in ((1,'reserve'),(2,'cancel')):
        check_events([line.split('\t') for line in (case/f'{part}.events.tsv').read_text().splitlines()],batch)
        check_clean((case/f'{part}.clean.tsv').read_text().rstrip('\n').split('\t'),batch)
    db_bytes=sum(r[0] for r in actual.values());worker=check_stdout((case/'stdout.log').read_text(),profile,db_bytes)
    resources=check_resources(db_bytes,actual[WAL][0])
    fd=check_fd((case/'fd.tsv').read_text(),[line.split('\t') for line in (case/'fd-events.tsv').read_text().splitlines()])
    # Full final content and exact intermediate hashes are independent of Rust's assertions.
    return {'status':'PASS','profile':profile,'database':str(case/'database'),'database_bytes':db_bytes,
            **resources,'files':actual,'intermediate_files':reserve,'worker':worker,'fd':fd,
            'evidence':{p.name:digest(p) for p in sorted(case.iterdir()) if p.is_file()}}


def terminal_proc_unavailable(error,child,stop,observed):
    """Record unavailable /proc observation overlapping confirmed child exit.

    This does not diagnose why access was denied. Stop alone is insufficient;
    the same Popen used by the watchdog must confirm a terminal return code.
    """
    prefix=f'/proc/{child.pid}/fd'
    if not isinstance(error,PermissionError) or not isinstance(error.filename,str):return False
    if error.filename!=prefix and re.fullmatch(re.escape(prefix)+r'/\d+',error.filename) is None:return False
    before=child.poll();stop_before=stop.is_set();after=before
    if before is None:
        stop.wait(0.001)
        after=child.poll()
    if after is None:return False
    observed['proc_terminal_unavailable'].append({
        'classification':'PROC_OBSERVATION_UNAVAILABLE_WITH_TERMINAL_CHILD',
        'errno':error.errno,'path':error.filename,'message':str(error),
        'poll_before':before,'poll_after':after,'stop_before':stop_before,'stop_after':stop.is_set()})
    return True


def sample_process(child,database,stop,observed):
    """Supplemental /proc samples; never claim sampling proves an exact peak."""
    anchor=str(database)
    while not stop.is_set():
        if child.poll() is not None:return
        try:
            fds=Path(f'/proc/{child.pid}/fd')
            count=0
            for entry in fds.iterdir():
                try:target=os.readlink(entry)
                except FileNotFoundError:continue
                if target==anchor or target.startswith(anchor+'/'):count+=1
            observed['fd_samples']+=1;observed['sampled_database_fd_peak']=max(observed['sampled_database_fd_peak'],count)
            if database.exists():
                size=sum(p.stat().st_size for p in database.iterdir() if p.is_file())
                observed['sampled_database_bytes_peak']=max(observed['sampled_database_bytes_peak'],size)
        except FileNotFoundError:pass
        except PermissionError as error:
            if terminal_proc_unavailable(error,child,stop,observed):return
            observed['sample_errors'].append(str(error));return
        except OSError as error:observed['sample_errors'].append(str(error));return
        stop.wait(0.001)


def run(root,binary,output,source,build_receipt,profiles=None):
    root,binary,output=root.resolve(),binary.resolve(),output.resolve();selected=select(profiles)
    require(sys.platform.startswith('linux'),'FAIL_PLATFORM')
    require(source_fingerprint(root)==source,'FAIL_SOURCE_IDENTITY')
    identities=frozen_inputs(root);binary_hash=digest(binary)
    build_receipt=build_receipt.resolve();build_hash=digest(build_receipt)
    verify_build(json.loads(build_receipt.read_text()),root,binary,source,binary_hash)
    scripts=Path(__file__).resolve().parent;checkers={name:digest(scripts/name) for name in CHECKER_NAMES}
    require(all(digest(root/'scripts'/name)==value for name,value in checkers.items()),'FAIL_CHECKER_IDENTITY')
    output.mkdir(parents=True,exist_ok=False)
    retained=output/f'flush-native-worker-{binary_hash}';shutil.copyfile(binary,retained);retained.chmod(0o500)
    with retained.open('rb') as stream:require(stream.read(4)==b'\x7fELF','FAIL_BINARY_FORMAT')
    report={'status':'RUNNING','scope':'three new Linux batch-one allocator flush DBs; no recovery/power-loss claim',
            'source_root':str(root),'source_sha256':source,'binary':str(retained),'binary_sha256':binary_hash,
            'build_receipt':str(build_receipt),'build_receipt_sha256':build_hash,'checker_sha256':checkers,
            'fixture_identities':identities,'selected_profiles':selected,'not_selected':[p for p in PROFILES if p not in selected],
            'case_timeout_seconds':CASE_SECONDS,'database_limit_bytes':DB_LIMIT,'total_limit_bytes':TOTAL_LIMIT,
            'native_cases':[],'not_run':selected.copy()}
    try:
        for profile in selected:
            case=output/f'p{profile}';case.mkdir()
            command=[str(retained),'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1']
            environment=os.environ.copy()
            for key in list(environment):
                if key.startswith(('XADB_','XADBTEST_','RUST_TEST_')):environment.pop(key)
            environment.update(XADB_FLUSH_NATIVE_DIR=str(case),XADB_FLUSH_NATIVE_PROFILE=str(profile))
            process={'command':command,'binary_sha256':binary_hash,'database_root':str(case/'database'),'timeout_seconds':CASE_SECONDS,
                     'timed_out':False,'exit_code':None,'fd_samples':0,'sampled_database_fd_peak':0,'sampled_database_bytes_peak':0,
                     'sample_errors':[],'proc_terminal_unavailable':[]}
            row={'profile':profile,'status':'RUNNING','process':process};report['native_cases'].append(row)
            started=time.monotonic()
            stop=threading.Event();sampler=None
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
                    if sampler.is_alive():process['sample_errors'].append('FD sampler did not terminate')
                process['elapsed_seconds']=time.monotonic()-started
                (case/'process.json').write_text(json.dumps(process,indent=2)+'\n')
            require(not process['timed_out'] and process['elapsed_seconds']<=CASE_SECONDS,'FAIL_TIMEOUT')
            require(process['exit_code']==0,'FAIL_WORKER_EXIT')
            require(not process['sample_errors'],'FAIL_PROC_OBSERVATION')
            require(process['fd_samples']>0,'FAIL_PROC_OBSERVATION_EMPTY')
            require(process['sampled_database_fd_peak']<=FD_LIMIT,'FAIL_SAMPLED_FD_BOUND')
            require(process['sampled_database_bytes_peak']<=DB_LIMIT,'FAIL_SAMPLED_DATABASE_BOUND')
            row.update(evaluate_case(case,root,profile))
            require(sum(r.get('database_bytes',0) for r in report['native_cases'])<=TOTAL_LIMIT,'FAIL_TOTAL_BUDGET')
        require(source_fingerprint(root)==source and digest(binary)==digest(retained)==binary_hash and digest(build_receipt)==build_hash,'FAIL_SOURCE_BINARY_BUILD_DRIFT')
        require(all(digest(root/name)==value for name,value in identities.items()),'FAIL_FIXTURE_DRIFT')
        require(all(digest(scripts/name)==value for name,value in checkers.items()),'FAIL_CHECKER_DRIFT')
        report['status']='PASS'
    except BaseException as error:
        report.update(status='FAIL',failure=str(error))
        for row in report['native_cases']:
            if row['status']=='RUNNING':row['status']='FAIL'
        raise
    finally:
        attempted={r['profile'] for r in report['native_cases']}
        report['not_run']=[p for p in selected if p not in attempted]
        report['databases_completed']=sum(r['status']=='PASS' for r in report['native_cases'])
        report['runtime_elapsed_seconds']=sum(r['process'].get('elapsed_seconds',0) for r in report['native_cases'])
        (output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent)
    for name in ('binary','output','build-receipt'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--source-sha256',required=True)
    parser.add_argument('--profile',type=int,action='append')
    args=parser.parse_args();result=run(args.root,args.binary,args.output,args.source_sha256,args.build_receipt,args.profile)
    print(json.dumps({'status':result['status'],'databases_completed':result['databases_completed']}))


if __name__=='__main__':main()
