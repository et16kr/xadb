#!/usr/bin/env python3
"""Actual producer -> closed effects -> fresh PID inspection, independent full bytes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
sys.dont_write_bytecode=True
import dev3_recovery_effects_oracle as oracle
import dev3_recovery_effects_vectors as vectors
from check_dev0 import digest,source_fingerprint
from dev3_recovery_prefix_native import check_binary_format,check_build_receipt,BUILD_COMMAND,wait_child,kill_and_reap

ROOT=Path(__file__).resolve().parent.parent
INPUT=Path('crates/engine/tests/fixtures/recovery_effects/native/manifest.json')
INPUT_SHA256='4e55f17078581b45f4dbe382ed23457abe5753ed870f0509516d2f8af53c71f8'
SELECTOR='storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::effect::native::worker_closed_recovery_native'
CASES=oracle.CASES
CASE_SECONDS,TOTAL_SECONDS,DB_LIMIT,TOTAL_LIMIT,FD_LIMIT=30,300,24<<20,768<<20,5
SNAPSHOT_LIMIT,LOG_LIMIT=32<<20,64<<20
require=oracle.require


def selected(names):
    values=list(CASES) if names is None else names
    require(values and len(set(values))==len(values) and not set(values)-set(oracle.ALL_CASES),'FAIL_NATIVE_SELECTION')
    require(not any(a in values and b in values for a,b in oracle.ALIASES.items()),'FAIL_NATIVE_ALIAS_DUPLICATE')
    require(len({c.startswith('NS5') for c in values})==1,'FAIL_MIXED_NATIVE_BUDGET')
    return [c for c in oracle.ALL_CASES if c in values]


def inventory(db):
    result={}
    for path in sorted(db.iterdir()):
        m=path.lstat();require(path.is_file() and not path.is_symlink() and m.st_nlink==1,'FAIL_FILE_TYPE')
        result[path.name]=dict(bytes=m.st_size,sha256=digest(path),device=m.st_dev,inode=m.st_ino,links=m.st_nlink,blocks=m.st_blocks,allocated_bytes=m.st_blocks*512)
    require(sum(v['bytes'] for v in result.values())<=DB_LIMIT,'FAIL_DATABASE_BOUND')
    return result


def wire_identity(files):return {n:dict(bytes=len(v),sha256=oracle.sha(v)) for n,v in files.items()}


def compare_files(db,expected):
    actual=inventory(db);require({n:dict(bytes=r['bytes'],sha256=r['sha256']) for n,r in actual.items()}==wire_identity(expected),'FAIL_INDEPENDENT_FILE_BYTES')
    return actual


def preserved_inodes(before,after):
    for name in set(before)&set(after)-{'DWB','d3-dwb-select-stage'}:
        require(all(before[name][key]==after[name][key] for key in ('device','inode','links')),'FAIL_UNEXPECTED_FILE_REPLACEMENT')


def check_stdout(text,mode,pid,outcome):
    require(text.count('test result:')==1,'FAIL_SUMMARY_CENSUS')
    require(re.findall(r'^test ([\w:]+) \.\.\.',text,re.M)==[SELECTOR],'FAIL_TEST_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;',text,re.M)==[('1','0','0','0')],'FAIL_TEST_CENSUS')
    require(re.findall(r'RECOVERY_EFFECTS_NATIVE_[^\r\n]+',text)==[f'RECOVERY_EFFECTS_NATIVE_BEGIN mode={mode} pid={pid}',f'RECOVERY_EFFECTS_NATIVE_CASE mode={mode} pid={pid} outcome={outcome} PASS','RECOVERY_EFFECTS_NATIVE_COMPLETE count=1'],'FAIL_NATIVE_PROTOCOL')


def check_process(row):require(not row['timed_out'] and row['exit_code']==0 and 0<=row['elapsed_seconds']<=CASE_SECONDS,'FAIL_PROCESS_TIMEOUT_OR_EXIT')


def finish_child(child,remaining,item):
    """Every exceptional wait path reaps before the active job can be removed."""
    try:wait_child(child,remaining,item)
    except BaseException:
        kill_and_reap(child)
        raise


def check_final_budget(started,total_seconds,logs_bytes):
    require(logs_bytes<=LOG_LIMIT and 0<=time.monotonic()-started<=total_seconds,'FAIL_TOTAL_TIME_OR_LOG_SPACE')


def events(folder):
    rows=[line.split('\t') for line in (folder/'events.tsv').read_text().splitlines()]
    require(all(len(r)==5 for r in rows),'FAIL_EVENT_COLUMNS')
    return rows


def check_resources(folder,mode):
    rows=[line.split('\t') for line in (folder/'resources.tsv').read_text().splitlines()]
    require(len(rows)==len({r[0] for r in rows})==6,'FAIL_RESOURCE_FIELDS');values={k:int(v) for k,v in rows}
    require(values['handles_final']==values['fd_final']==0 and 0<values['fd_peak']<=FD_LIMIT,'FAIL_RESOURCE_BOUND')
    trace=[line.split('\t') for line in (folder/'fd.tsv').read_text().splitlines()]
    require(trace and trace[-1]==['final','0'] and max(int(r[1]) for r in trace)==values['fd_peak'],'FAIL_FD_WITNESS')
    if mode!='produce':require(values['handles_peak']<=2,'FAIL_TEMPORARY_OVERLAP')
    return values


def check_producer_cut(folder,definition,resources):
    context=definition['context']
    cut=16 if context=='stage0' else 18 if context=='stage4096' else 19 if context in ('historical','a_pending_torn','s_pending_torn','future_pending','repair_partial','slot_corrupt','retirement_unknown') else None
    if cut is not None:
        require(resources['cut_hits']==1 and resources['cut_ordinal']==cut,'FAIL_PRODUCER_CUT_ORDINAL')
        text=(folder/'producer-error.txt').read_text()
        if cut==18:
            lower='cause: Io(Error { code: IoWriteUnknown, context: "sync outcome unknown", io_kind: Some(Other) })'
            require('CallerMustResolve' in text and 'phase: Selection { index: 1, step: SelectorSync }' in text and lower in text,'FAIL_PRODUCER_TYPED_CUT')
            require(events(folder)[-1]==['sync','d3-dwb-select-stage','0','0','OK'],'FAIL_PRODUCER_CUT_SYSCALL')
        else:
            require('CallerMustResolve' in text and 'closed recovery native after-effect unknown' in text,'FAIL_PRODUCER_TYPED_CUT')
    elif context=='tail':
        require(resources['cut_hits']==1 and any(row[0]=='write' and row[1]==oracle.WAL and row[3]=='38000' and row[4].startswith('ERR_PARTIAL:19000:') for row in events(folder)),'FAIL_ACTUAL_INIT_TAIL_CUT')
    else:require(resources['cut_hits']==0,'FAIL_UNREQUESTED_PRODUCER_CUT')


def observation(folder):
    rows=[line.split('\t') for line in (folder/'observation.tsv').read_text().splitlines()]
    require(len(rows)==len({r[0] for r in rows}) and all(len(r)==2 for r in rows),'FAIL_OBSERVATION_CENSUS');return dict(rows)


def check_census(folder):
    require((folder/'source-census.tsv').is_file(),'FAIL_SOURCE_CENSUS_MISSING')
    rows=[line.split('\t') for line in (folder/'source-census.tsv').read_text().splitlines()]
    require(len(rows)==len({r[0] for r in rows})==9,'FAIL_SOURCE_CENSUS');c={k:int(v) for k,v in rows}
    require(set(c)=={'outer','prefix_nested','inventory','work_nested','peak','writer','file','maintenance','flush'},'FAIL_SOURCE_CENSUS')
    require(0<c['peak']<=65536 and c['peak']==c['outer']+max(c['prefix_nested'],c['inventory'],c['work_nested']) and 0<c['writer']<=16384 and 0<c['file']<=16384 and c['maintenance']==12582912 and c['flush']==4194304,'FAIL_SOURCE_CAPACITY')
    return c


def check_failed_accounting(folder,before,o,fault,context):
    actual=observation(folder);expected=oracle.failed_facts(before,o,fault,context)
    require(all(actual.get(k)==str(v) for k,v in expected.items()),'FAIL_FAILED_OWNER_ACCOUNTING')
    require(int(actual['actual_owned'])+int(actual['outstanding'])==int(actual['owned_peak']),'FAIL_FAILED_CREDIT_BALANCE')
    return expected


def expected_error(case,fault):
    partial='Io(Error { code: IoWriteUnknown, context: "write outcome requires caller resolution", io_kind: Some(Other) })'
    ns='Namespace(NamespaceError { kind: Io, effect: CallerMustResolve, io_kind: None, context: "closed recovery native after-effect unknown" })'
    sync='Io(Error { code: IoWriteUnknown, context: "sync outcome unknown", io_kind: Some(Other) })'
    definitions={
        'boot0_partial':('BootstrapProvision',partial,('Write','d3-rebase-0',0,65536)),
        'boot1_partial':('BootstrapProvision',partial,('Write','d3-rebase-1',131072,65536)),
        'boot_sync_after':('BootstrapProvision',sync,('FileSync','d3-rebase-1',0,0)),
        'boot_dir_after':('BootstrapProvision',ns,('DirectorySync','.',0,0)),
        'repair_partial':('PendingHome',partial,('Write',oracle.DATA,16384,16384)),
        'retirement_publish_after':('Retirement',ns,('Publish','DWB',0,4096)),
        'redo_second_partial':('Flush',partial,('Write',oracle.DATA,16384,16384)),
    }
    if fault!='none':phase,cause,last=definitions[fault]
    else:
        phase,cause,last={
            'NR18':('Redo','PriorMismatch',('Read',oracle.DATA,0,16384)),
            'NR19':('PendingValidate','PriorMismatch',('Read',oracle.DATA,16384,16384)),
            'NR25':('PendingValidate','Doublewrite(Corrupt)',('Read','d3-dwb-images',0,16544)),
            'NR24':('Prefix','Prefix(RecoveryPrefixError { phase: Scan, effect: Unchanged, cause: Wal(Corrupt) })',None),
            'NB06':('Inventory','Unsupported',('Read','d3-rebase-1',983040,65536)),
        }[case]
    call='None' if last is None else 'Some(RecoveryCall { phase: '+phase+', kind: '+last[0]+', name: "'+last[1]+'", offset: '+str(last[2])+', length: '+str(last[3])+' })'
    return dict(phase=phase,cause=cause,effect='CallerMustResolve',lease_effect='MayHaveChanged',last_call=call)


def check_error(folder,case,fault):
    rows=[line.split('\t') for line in (folder/'error.tsv').read_text().splitlines()]
    require(len(rows)==len({r[0] for r in rows})==5,'FAIL_ERROR_FIELDS')
    require(dict(rows)==expected_error(case,fault),'FAIL_EXACT_ERROR_PHASE_CAUSE_OR_CALL')
    return dict(rows)


def check_success(folder,db,files,o,mode):
    facts=observation(folder);expected=oracle.facts(files,o)
    require(all(facts.get(k)==str(v) for k,v in expected.items()),'FAIL_INDEPENDENT_LEDGER_OR_PREFIX')
    require(facts['source_stabilized']==facts['slots_provisioned']=='true' and facts['lease_effect']=='MayHaveChanged' and facts['state']=='Empty' and facts['guard_fd_peak']=='5','FAIL_CLOSED_SUCCESS_RECEIPTS')
    meta=(db/oracle.WAL).stat();require(facts['source_device']==str(meta.st_dev) and facts['source_inode']==str(meta.st_ino),'FAIL_SOURCE_IDENTITY')
    require((folder/'allocator.bin').read_bytes()==o['allocator'] and (folder/'descriptor.bin').read_bytes()==o['descriptor'],'FAIL_PHYSICAL_OR_DESCRIPTOR_BYTES')
    path=folder/'status.bin';require(path.exists()==(o['status'] is not None) and (o['status'] is None or path.read_bytes()==o['status']),'FAIL_STATUS_IMAGE')
    require(facts['canonical_passes']=='1','FAIL_REDO_PASS_COUNT')
    if mode=='verify':require(facts['flushed_mask']=='0' and facts['dirty_mask']=='0','FAIL_SECOND_ENTRY_HOME_WRITES')
    return facts


def check_effects(folder,mode,fault,definition,before,o):
    rows=events(folder);writes=[r for r in rows if r[0]=='write'];effects=(folder/'effects.txt').read_text()
    expected=vectors.mutation_vector(before,o,mode,fault)
    vectors.check_raw_mutations(rows,expected)
    read_count=vectors.check_read_bytes(rows,before,expected)
    call_count=None
    if mode!='reject_acquire':
        call_count=vectors.check_call_order(rows,vectors.effect_calls(before,o,mode,fault,definition['context']))
    image_reads=[r for r in rows if r[0]=='read' and r[1]=='d3-dwb-images']
    selected=before['DWB'];pending=before[f'd3-dwb-{selected[81]}'][6]==1
    if mode!='reject_acquire':require(len(image_reads)==int(pending),'FAIL_RAW_RETIRED_SLOT_READ')
    if mode=='reject_acquire':
        require(not writes and not any(r[0] in ('create','sync','dirsync','publish') for r in rows),'FAIL_ACQUIRE_PROVISIONING')
        text=(folder/'error.txt').read_text();require('CallerMustResolve' in text and 'MayHaveChanged' in text,'FAIL_LEASE_EFFECT')
    if mode!='produce':
        require(all(r[1]!=oracle.WAL for r in writes),'FAIL_SOURCE_WRITE')
        for r in writes:
            if r[1] in oracle.SLOTS:require(int(r[3])==65536 and int(r[2])%65536==0 and r[4].split(':')[-1]==oracle.sha(bytes(65536)),'FAIL_BOOT_ZERO_CHUNK')
        if mode in ('recover','verify','reject_recover'):
            for name in oracle.SLOTS:
                require([int(r[2]) for r in writes if r[1]==name]==list(range(0,1048576,65536)),'FAIL_BOOT_FULL_REWRITE')
            require('slot_chunks_done: [\n        16,\n        16,' in effects and 'boot_directory_synced: true' in effects,'FAIL_BOOT_RECEIPT')
        if mode in ('verify','reject_recover','reject_acquire'):
            require(all(r[1] in oracle.SLOTS for r in writes) and not any(r[0]=='publish' for r in rows),'FAIL_NO_HOME_CHANGE')
        if mode in ('fail','reject_recover'):
            text=(folder/'error.txt').read_text();require('CallerMustResolve' in text and 'MayHaveChanged' in text,'FAIL_CUMULATIVE_EFFECT')
        if fault!='none':require(any('ERR_PARTIAL' in r[4] for r in rows) or mode=='fail','FAIL_FAULT_NOT_REACHED')
    return dict(raw_calls=len(rows),new_literal_calls_verified=call_count,read_hashes_verified=read_count,writes=len(writes),home_writes=sum(r[1]==oracle.DATA for r in writes),boot_writes=sum(r[1] in oracle.SLOTS for r in writes))


def adversarial(db,case,expected):
    x=oracle.ALL_CASES[case]['context'];patches=[]
    if x in ('raw_corrupt','slot_corrupt','checksum','s_pending_torn','s_torn','a_pending_torn','a_torn','future_pending'):
        name='d3-dwb-images' if x in ('raw_corrupt','slot_corrupt') else oracle.WAL if x=='checksum' else oracle.DATA
        old=(db/name).read_bytes();new=expected[name]
        if x=='future_pending':offset=16384;payload=new[offset:offset+16384]
        else:
            offset=1000 if name=='d3-dwb-images' else len(old)-1 if name==oracle.WAL else (16384 if x.startswith('s_') else 0)+8000
            payload=new[offset:offset+1]
        with (db/name).open('r+b') as stream:stream.seek(offset);stream.write(payload)
        patches.append(dict(name=name,offset=offset,bytes=len(payload),before_sha256=oracle.sha(old),after_sha256=digest(db/name)))
    if x in ('boot5','boot6'):
        for name in oracle.SLOTS:
            with (db/name).open('xb') as stream:
                if x=='boot5':stream.truncate(1048576)
                else:stream.write(expected[name])
            m=(db/name).stat();patches.append(dict(name=name,sparse=x=='boot5',bytes=m.st_size,blocks=m.st_blocks,device=m.st_dev,inode=m.st_ino))
            if x=='boot5':require(m.st_blocks*512<m.st_size,'FAIL_SPARSE_FIXTURE')
    return patches


def preserve_failure(db,phase):
    dest=phase/'failed-snapshot';dest.mkdir();total=0
    for name in ('DWB','d3-dwb-stage','d3-dwb-0','d3-dwb-1','d3-dwb-select-stage',*oracle.SLOTS,'d3-dwb-images',oracle.DATA):
        path=db/name
        if not path.exists():continue
        limit=16544 if name=='d3-dwb-images' else 32768 if name==oracle.DATA else path.stat().st_size
        with path.open('rb') as stream:raw=stream.read(limit)
        (dest/name).write_bytes(raw);total+=len(raw)
    return total


def validate_inputs(root):
    require(digest(root/INPUT)==INPUT_SHA256,'FAIL_NATIVE_MANIFEST_IDENTITY');spec=json.loads((root/INPUT).read_text())
    require(spec['cases']==oracle.ALL_CASES and spec['selector']==SELECTOR and spec['budgets']==dict(case_seconds=CASE_SECONDS,total_seconds=TOTAL_SECONDS,database_bytes=DB_LIMIT,retained_database_bytes=TOTAL_LIMIT,fd_limit=FD_LIMIT,failed_snapshots_bytes=SNAPSHOT_LIMIT,logs_bytes=LOG_LIMIT),'FAIL_NATIVE_MANIFEST_CONTRACT')
    for row in spec['dependencies']:
        path=root/row['path'];require(path.stat().st_size==row['bytes'] and digest(path)==row['sha256'],'FAIL_INPUT_DEPENDENCY')
    require(digest(root/spec['generator']['path'])==spec['generator']['sha256'],'FAIL_GENERATOR_IDENTITY');return spec


def checker_identities(root):
    paths={Path(m.__file__).resolve() for m in sys.modules.values() if getattr(m,'__file__',None) and Path(m.__file__).suffix=='.py' and Path(m.__file__).resolve().parent==ROOT/'scripts'}
    paths.add(Path(__file__).resolve());paths.add(ROOT/'scripts/test_dev3_recovery_effects_native.py')
    values={str(p.relative_to(ROOT)):digest(p) for p in paths};require(all(digest(root/p)==v for p,v in values.items()),'FAIL_CHECKER_IDENTITY');return values


def run(root,binary,receipt,output,names=None):
    root,binary,receipt,output=(p.resolve() for p in (root,binary,receipt,output));chosen=selected(names);spec=validate_inputs(root)
    source,binary_hash,receipt_hash=source_fingerprint(root),digest(binary),digest(receipt);check_build_receipt(json.loads(receipt.read_text()),root,source,binary_hash);check_binary_format(binary);checkers=checker_identities(root)
    output.mkdir(parents=True,exist_ok=False);retained=output/'native-binary';shutil.copyfile(binary,retained);retained.chmod(0o500)
    report=dict(status='RUNNING',unit=oracle.UNIT,source_root=str(root),source_sha256=source,binary_sha256=binary_hash,build_receipt_sha256=receipt_hash,input_sha256=INPUT_SHA256,checker_sha256=checkers,selected=chosen,cases=[],failed_snapshot_bytes=0,logs_bytes=0,binary_bytes=binary.stat().st_size)
    extended=chosen[0].startswith('NS5');total_seconds=1200 if extended else TOTAL_SECONDS;retained_limit=1536<<20 if extended else TOTAL_LIMIT
    report.update(budget_group='s5' if extended else 'mandatory',total_seconds=total_seconds,retained_database_limit=retained_limit)
    started=time.monotonic();active=[]
    try:
        for case in chosen:
            definition=oracle.ALL_CASES[case];folder=output/case;folder.mkdir();db=folder/'database';db.mkdir();began=time.monotonic();row=dict(case=case,status='RUNNING',definition=definition,processes=[]);report['cases'].append(row)
            expected,o=oracle.snapshots(root,case);producer,_=oracle.prepared(root,case,False)
            require(spec['expected'][case]['snapshots']==[wire_identity(f) for f in expected] and spec['expected'][case]['producer']==wire_identity(producer),'FAIL_EXPECTED_IDENTITY')
            steps=[('produce','none'),*oracle.histories(case)]
            for index,(mode,fault) in enumerate(steps):
                phase=folder/f'{index:02}-{mode}';phase.mkdir();env={k:v for k,v in os.environ.items() if not k.startswith(('XADB_','XADBTEST_','RUST_TEST_'))}
                env.update({f'XADB_RECOVERY_EFFECTS_NATIVE_{k}':str(v) for k,v in dict(DIR=phase,DB=db,MODE=mode,FAULT=fault,PROFILE=definition['profile'],KIND=definition['kind'],COUNT=definition['count'],CYCLES=definition['cycles'],CONTEXT=definition['context']).items()})
                command=[str(retained),'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1'];stdout=(phase/'stdout.log').open('wb');stderr=(phase/'stderr.log').open('wb')
                child=subprocess.Popen(command,cwd=root,env=env,stdout=stdout,stderr=stderr,start_new_session=True);item=dict(mode=mode,fault=fault,pid=child.pid,command=command,timed_out=False,exit_code=None);row['processes'].append(item);job=(child,phase,item,stdout,stderr);active.append(job);now=time.monotonic()
                def remaining():
                    left=min(CASE_SECONDS-(time.monotonic()-began),total_seconds-(time.monotonic()-started));require(left>0,'FAIL_NATIVE_DEADLINE');return left
                try:finish_child(child,remaining,item)
                finally:
                    stdout.close();stderr.close();item.update(exit_code=child.poll(),elapsed_seconds=time.monotonic()-now);(phase/'process.json').write_text(json.dumps(item,indent=2)+'\n')
                    if child.poll() is not None:active.remove(job)
                check_process(item);outcome='produced' if mode=='produce' else 'rejected_acquire' if mode=='reject_acquire' else 'failed_recover' if mode in ('fail','reject_recover') else 'recovered'
                check_stdout((phase/'stdout.log').read_text(),mode,child.pid,outcome);item['resources']=check_resources(phase,mode)
                if mode=='fail':require(item['resources']['cut_hits']==1,'FAIL_NATIVE_CUT_MISSED')
                if index==0:
                    check_producer_cut(phase,definition,item['resources'])
                    compare_files(db,producer);row['adversarial_input']=adversarial(db,case,expected[0]);row['prepared']=compare_files(db,expected[0]);last=row['prepared'];continue
                current=compare_files(db,expected[index]);preserved_inodes(last,current);require(all(current[oracle.WAL][k]==row['prepared'][oracle.WAL][k] for k in ('bytes','sha256','device','inode')),'FAIL_SOURCE_CHANGED')
                item['effect_witness']=check_effects(phase,mode,fault,definition,expected[index-1],o);item['files']=current
                if mode!='reject_acquire':item['source_census']=check_census(phase)
                if mode in ('fail','reject_acquire','reject_recover'):item['error']=check_error(phase,case,fault)
                if mode in ('fail','reject_recover'):item['failed_accounting']=check_failed_accounting(phase,expected[index-1],o,fault,definition['context'])
                if mode in ('recover','verify'):item['observation']=check_success(phase,db,expected[index],o,mode)
                if mode in ('fail','reject_acquire','reject_recover'):report['failed_snapshot_bytes']+=preserve_failure(db,phase);require(report['failed_snapshot_bytes']<=SNAPSHOT_LIMIT,'FAIL_FAILED_SNAPSHOT_SPACE')
                last=current
            pids=[p['pid'] for p in row['processes']];require(len(pids)==len(set(pids)),'FAIL_FRESH_PID')
            row.update(database_bytes=sum(v['bytes'] for v in last.values()),elapsed_seconds=time.monotonic()-began)
            require(row['elapsed_seconds']<=CASE_SECONDS and sum(r.get('database_bytes',0) for r in report['cases'])<=retained_limit,'FAIL_HISTORY_BUDGET');row['status']='PASS'
        require(source_fingerprint(root)==source and digest(binary)==digest(retained)==binary_hash and digest(receipt)==receipt_hash and validate_inputs(root)==spec and checker_identities(root)==checkers,'FAIL_POST_IDENTITY')
        report['logs_bytes']=sum(p.stat().st_size for p in output.rglob('*') if p.is_file() and p!=retained and 'database' not in p.relative_to(output).parts and 'failed-snapshot' not in p.relative_to(output).parts)
        report['evidence_scope']='all retained worker streams/physical dumps/metadata/receipts, excluding database files, separate failed snapshots and ELF' 
        check_final_budget(started,total_seconds,report['logs_bytes']);report['status']='PASS'
    except BaseException as e:
        report.update(status='FAIL',error=str(e))
        for r in report['cases']:
            if r['status']=='RUNNING':r['status']='FAIL'
        raise
    finally:
        for child,phase,item,stdout,stderr in active:
            kill_and_reap(child);stdout.close();stderr.close();item.update(exit_code=child.returncode,aborted_by_controller=True);(phase/'process.json').write_text(json.dumps(item,indent=2)+'\n')
        report['elapsed_seconds']=time.monotonic()-started;report['not_run']=[c for c in chosen if c not in {r['case'] for r in report['cases']}];(output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,default=ROOT)
    for name in ('binary','build-receipt','output'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--case',action='append');p.add_argument('--select',choices=('mandatory','s5','s5_matrix_all'));a=p.parse_args()
    require(not(a.case and a.select),'FAIL_SELECTION_OVERLAP')
    names=a.case if a.select is None else list(CASES) if a.select=='mandatory' else [c for c in oracle.S5_CASES if a.select=='s5_matrix_all' or c not in oracle.ALIASES]
    r=run(a.root,a.binary,a.build_receipt,a.output,names);print(json.dumps(dict(status=r['status'],completed=len(r['cases']))))


if __name__=='__main__':main()
