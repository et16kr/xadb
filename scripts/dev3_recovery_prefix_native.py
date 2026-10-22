#!/usr/bin/env python3
"""Actual producer files -> separate read-only prefix owners, with literal oracles."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from dev3_recovery_select_native import initial_files, inventory, check_binary_format
from dev3_status_home_flush import BUILD_COMMAND, check_build_receipt

ROOT = Path(__file__).resolve().parent.parent
SELECTOR = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::native::worker_recovery_prefix_native'
INPUT = Path('crates/engine/tests/fixtures/recovery_prefix/native/manifest.json')
INPUT_SHA256 = '88ac335c98edafb6a1f887434a96bbcd6a16967c50f993287221c60b437b30b9'
CASE_SECONDS,TOTAL_SECONDS,DB_LIMIT,TOTAL_LIMIT,FD_LIMIT=30,300,24<<20,768<<20,5
DATABASE=bytes(range(1,17))
KINDS=('EA','EC','DA','DC','SA','SC','SI')
CASES={f'PN{7*(profile-1)+i+1:02}':dict(profile=profile,kind=kind,context='plain') for profile in (1,2,3) for i,kind in enumerate(KINDS)}
for n,(profile,kind,context) in enumerate(((1,'SI','a_flushed'),(2,'SI','s_flushed'),(3,'SI','both_flushed'),(1,'DC','dirty_flushed'),(1,'SI','pending'),(1,'SI','raw_corrupt'),(1,'SI','stage'),(1,'SI','tail'),(1,'SI','checksum'),(1,'SI','unknown')),1):
    CASES[f'PC{n:02}']=dict(profile=profile,kind=kind,context=context)
DEPENDENCIES=set()


def require(ok,reason):
    if not ok: raise ValueError(reason)


def sha(raw):return hashlib.sha256(raw).hexdigest()


def read(root,path):
    DEPENDENCIES.add(str(path))
    return (root/path).read_bytes()


def selected(names):
    values=list(CASES) if names is None else names
    require(values and len(values)==len(set(values)) and not set(values)-set(CASES),'FAIL_NATIVE_SELECTION')
    return [name for name in CASES if name in values]


def literal(root,logical):
    base=Path('tests/fixtures/dev3-status-home-flush')
    rows=[r for r in csv.DictReader(read(root,base/'literals.tsv').decode().splitlines(),delimiter='\t') if r['logical']==logical]
    require(len(rows)==1,'FAIL_LITERAL_CENSUS');row=rows[0]
    value=read(root,base/row['blob'])
    require(len(value)==int(row['bytes']) and sha(value)==row['sha256'],'FAIL_LITERAL_IDENTITY')
    return value


def state_files(root,files,key):
    base=Path('tests/fixtures/dev3-status-home-flush')
    rows=[r for r in csv.DictReader(read(root,base/'states.tsv').decode().splitlines(),delimiter='\t') if r['state']==key]
    require(len(rows) in (7,8),'FAIL_STATE_CENSUS')
    for row in rows:
        region=row['region'];raw=literal(root,row['logical'])
        if region.startswith('home:page'):
            offset=int(region[-1])*16384;old=files['d3-data-0000000100000001'];files['d3-data-0000000100000001']=old[:offset]+raw+old[offset+16384:]
        elif region=='d3-dwb-images:slot0':files['d3-dwb-images']=raw+files['d3-dwb-images'][16544:]
        else:files[region]=raw


def subtype_oracle(root,profile,kind,context):
    status=kind.startswith('S');part='cancel' if kind.endswith('C') else 'reserve'
    if not status:
        base=Path('tests/fixtures/dev3-allocation-reservation' if kind.startswith('E') else 'tests/fixtures/dev3-dirty-allocation')
        stem=f'p{profile}-n1-'
        frames=[read(root,base/(stem+'reserve.frame'))]
        if part=='cancel':frames.append(read(root,base/(stem+'cancel.frame')))
        allocator=read(root,base/(stem+part+'.page'));descriptor=read(root,base/(stem+part+'.descriptor'));image=None
    else:
        stem=f'p{profile}-c{1 if kind=="SC" else 0}/work1-'
        frames=[literal(root,stem+'reserve.wal')]
        if kind=='SC':frames.append(literal(root,stem+'cancel.wal'))
        if kind=='SI':frames.append(literal(root,stem+'init.wal'))
        allocator=literal(root,stem+('allocator.reserved' if kind=='SA' or context=='tail' else 'allocator.after'))
        descriptor=literal(root,stem+('reserve.descriptor' if kind=='SA' or context=='tail' else 'cancel.descriptor' if kind=='SC' else 'init.descriptor'))
        image=literal(root,stem+'status.after') if kind=='SI' and context!='tail' else None
    if context=='tail':frames[-1]=frames[-1][:19000]
    adopted=1 if part=='reserve' and kind!='SI' or context=='tail' else 2
    active=kind in ('EA','DA','SA') or context=='tail'
    return dict(frames=frames,allocator=allocator,status=image,descriptor=descriptor,lsn=adopted,active=active)


def expected_files(root,case):
    row=CASES[case];profile,kind,context=(row[k] for k in ('profile','kind','context'))
    files=initial_files(root,profile,2 if kind.startswith('S') else 1)
    oracle=subtype_oracle(root,profile,kind,context)
    files['d3-wal-00000001']+=b''.join(oracle['frames'])
    if context in ('a_flushed','s_flushed','both_flushed','raw_corrupt','pending','stage'):
        key={'a_flushed':f'p{profile}-c0-AS-b1/after','s_flushed':f'p{profile}-c0-SA-b1/after','both_flushed':f'p{profile}-c0-AS-b2/after','raw_corrupt':f'p{profile}-c0-AS-b2/after','pending':'SF-P1-SA-AFTER-19','stage':'SF-P1-SA-AFTER-18'}[context]
        state_files(root,files,key)
    if context=='dirty_flushed':
        base=Path('tests/fixtures/dev3-allocator-flush-one');stem=f'p{profile}-n1-cancel-'
        raw=read(root,base/(stem+'home.after'));files['d3-data-0000000100000001']=raw+bytes(1048576-len(raw))
        raw=read(root,base/(stem+'slot.bin'));files['d3-dwb-images']=raw+bytes(16711680-len(raw))
        files['d3-dwb-stage']=read(root,base/(stem+'retirement.bank'))
        files['d3-dwb-0']=files['d3-dwb-stage'];files['d3-dwb-1']=read(root,base/(stem+'pending.bank'))
        files['DWB']=read(root,base/(stem+'retirement.selector'))
    if context=='raw_corrupt':
        raw=bytearray(files['d3-dwb-images']);raw[1000]^=1;files['d3-dwb-images']=bytes(raw)
    if context in ('checksum','unknown'):
        raw=bytearray(files['d3-wal-00000001'])
        if context=='checksum':raw[-1]^=1
        else:
            for off in (166,1888):raw[25296+off:25296+off+2]=(65534).to_bytes(2,'little')
            raw[-32:]=hashlib.sha256(raw[25296:-32]).digest()
        files['d3-wal-00000001']=bytes(raw)
    return files,oracle


def number(raw,offset):return int.from_bytes(raw[offset:offset+8],'little')


def expected_observation(files,oracle,case):
    row=CASES[case];kind,context=row['kind'],row['context'];desc=oracle['descriptor'];active=oracle['active']
    wal=files['d3-wal-00000001'];selector=files['DWB'];generation=number(selector,32)
    base=dict(database=DATABASE.hex(),timeline='1',profile=str(row['profile']),plan=str(2 if kind.startswith('S') else 1),filemap_generation='1',dwb_generation=str(generation),complete_lsn=str(oracle['lsn']),complete_offset=str(25296 if context=='tail' else len(wal)),source_length=str(len(wal)),work_high_water='1',active_id=str(int(active)),active_mask=str(2 if active else 0),mode='Some('+('Exclusive' if kind.startswith('E') else 'Dirty' if kind.startswith('D') else 'Status')+')',inventory_items=str(number(desc,552)),inventory_digest=desc[560:592].hex(),retained_wal=str(len(wal)-4096),remaining_wal=str(number(desc,912)),remaining_pages=str(number(desc,936)),remaining_memory=str(number(desc,968)),remaining_frames=str(number(desc,960)),remaining_files=str(number(desc,992)),remaining_protected=str(number(desc,1008)),terminal_hold_mask=str(0 if active else 2),source_stabilized='false')
    tail=context=='tail';pending=context=='pending'
    bank=files[f'd3-dwb-{selector[81]}'];base.update(dwb_batch=str(number(bank,40)),dwb_state='Pending' if pending else 'Empty',tail_present=str(tail).lower(),tail_offset=str(25296 if tail else 0),tail_bytes=str(19000 if tail else 0),tail_declared=str(38000 if tail else 0),tail_inspected=str(160 if tail else 0),pending_present=str(pending).lower(),pending_slot='0',pending_page=str(int(pending)),pending_incarnation=str(2 if pending else 0),pending_owner=str(2 if pending else 0),pending_owner_generation=str(int(pending)),pending_kind=str(9 if pending else 0),pending_level='0',pending_lsn=str(2 if pending else 0),pending_digest=sha(oracle['status']) if pending else '')
    owned=sum(len(raw) for raw in files.values());stage=len(files.get('d3-dwb-select-stage',b''))
    replay_pages=1+int(oracle['status'] is not None);inventory=int(base['inventory_items']);remaining_protected=int(base['remaining_protected']);remaining_wal=int(base['remaining_wal'])
    require(remaining_protected>=remaining_wal and (remaining_protected-remaining_wal)%16384==0,'FAIL_ORACLE_PROTECTED_TUPLE')
    page_credit=(remaining_protected-remaining_wal)//16384;metadata_topup=max(replay_pages+inventory-page_credit,0)*16384
    base.update(replay_page_topup_bytes=str(metadata_topup),owned_bytes=str(owned),protected_bytes=str(owned-1048576+(4096-stage)+remaining_protected+metadata_topup))
    return base


def check_stdout(text,mode,pid,outcome,selector=SELECTOR):
    require(text.count('test result:')==1,'FAIL_SUMMARY_CENSUS')
    require(re.findall(r'^test ([\w:]+) \.\.\.',text,re.M)==[selector],'FAIL_NATIVE_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;',text,re.M)==[('1','0','0','0')],'FAIL_NATIVE_CENSUS')
    lines=re.findall(r'RECOVERY_PREFIX_NATIVE_[^\r\n]+',text)
    require(lines==[f'RECOVERY_PREFIX_NATIVE_BEGIN mode={mode} pid={pid}',f'RECOVERY_PREFIX_NATIVE_CASE mode={mode} pid={pid} outcome={outcome} PASS','RECOVERY_PREFIX_NATIVE_COMPLETE count=1'],'FAIL_NATIVE_PROTOCOL')


def check_process(row):
    require(row['timed_out'] is False and row['exit_code']==0 and 0<=row['elapsed_seconds']<=CASE_SECONDS,'FAIL_NATIVE_TIMEOUT_OR_EXIT')


def kill_and_reap(child):
    if child.poll() is None:
        try: os.killpg(child.pid,9)
        except ProcessLookupError: pass
    child.wait()


def wait_child(child,remaining,process):
    try:
        child.wait(timeout=remaining())
    except subprocess.TimeoutExpired:
        process['timed_out']=True
        kill_and_reap(child)
    except BaseException:
        process['aborted_by_controller']=True
        kill_and_reap(child)
        raise


def check_total_budget(elapsed):
    require(0<=elapsed<=TOTAL_SECONDS,'FAIL_NATIVE_TOTAL_TIMEOUT')


def validate_inputs(root):
    require(digest(root/INPUT)==INPUT_SHA256,'FAIL_NATIVE_MANIFEST_IDENTITY')
    spec=json.loads((root/INPUT).read_text())
    require(spec['cases']==CASES and spec['selector']==SELECTOR and spec['case_seconds']==CASE_SECONDS and spec['total_seconds']==TOTAL_SECONDS and spec['database_bytes']==DB_LIMIT and spec['total_bytes']==TOTAL_LIMIT and spec['fd_limit']==FD_LIMIT,'FAIL_NATIVE_MANIFEST_SCHEMA')
    require(digest(root/spec['generator']['path'])==spec['generator']['sha256'],'FAIL_GENERATOR_IDENTITY')
    for row in spec['dependencies']:
        path=root/row['path'];require(path.stat().st_size==row['bytes'] and digest(path)==row['sha256'],'FAIL_NATIVE_DEPENDENCY')
    return spec


def check_resources(folder,db,closed):
    rows=[line.split('\t') for line in (folder/'resources.tsv').read_text().splitlines()]
    require(len(rows)==len({r[0] for r in rows})==7,'FAIL_RESOURCE_CENSUS');values={k:int(v) for k,v in rows}
    require(values['handles_final']==values['fd_final']==0 and values['fd_peak']<=FD_LIMIT,'FAIL_RESOURCE_BOUND')
    trace=[line.split('\t') for line in (folder/'fd.tsv').read_text().splitlines()]
    require(trace and trace[-1]==['final','0'] and max(int(r[1]) for r in trace)==values['fd_peak'],'FAIL_FD_TRACE')
    rows=[line.split('\t') for line in (folder/'source-census.tsv').read_text().splitlines()]
    require(len(rows)==len({r[0] for r in rows})==8,'FAIL_SOURCE_CENSUS_FIELDS')
    census={k:int(v) for k,v in rows}
    require(set(census)=={'outer','bootstrap','replay','stream','peak','writer','file','caller'},'FAIL_SOURCE_CENSUS_FIELDS')
    require(all(census[k]>0 for k in ('outer','bootstrap','replay','stream')) and census['peak']==census['outer']+max(census['bootstrap'],census['replay'],census['stream']) and census['peak']<=65536 and 0<census['writer']<=16384 and 0<census['file']<=16384 and census['caller']==262144,'FAIL_SOURCE_CENSUS_BOUND')
    values['source_census']=census
    if closed:
        require(values['handles_peak']<=2 and values['mutations']==0 and values['cut_hits']==0,'FAIL_CLOSED_EFFECT')
        allowed={'d3-profile','FILEMAP','d3-filemap-0','d3-bootstrap','DWB','d3-dwb-0','d3-dwb-1','d3-wal-00000001','WRITER'}
        reads=[line.split('\t') for line in (folder/'reads.tsv').read_text().splitlines()]
        for parts in reads:
            require(len(parts)==5 and parts[0] in allowed,'FAIL_CLOSED_READ_SET')
            name,offset,requested,returned,hash_value=parts;offset,requested,returned=map(int,(offset,requested,returned))
            require(0<=returned<=requested and offset+returned<=(db/name).stat().st_size,'FAIL_READ_RANGE')
            with (db/name).open('rb') as stream:stream.seek(offset);raw=stream.read(returned)
            require(sha(raw)==hash_value,'FAIL_READ_CONTENT_IDENTITY')
        selected_bank=(db/'DWB').read_bytes()[81]
        require(all(r[0]!=f'd3-dwb-{1-selected_bank}' for r in reads),'FAIL_UNSELECTED_BANK_READ')
    return values


def check_producer_cut(folder,context,resources):
    require(resources['cut_hits']==1 and resources['cut_ordinal']==(19 if context=='pending' else 18 if context=='stage' else 0),'FAIL_PRODUCER_CUT_NOT_REACHED')
    text=(folder/'producer-error.txt').read_text();events=(folder/'events.tsv').read_text().splitlines()
    require('CallerMustResolve' in text and events,'FAIL_PRODUCER_FAILURE_EVIDENCE')
    if context=='pending':
        require('Publish' in text and 'Namespace' in text and events[-1]=='publish\t19','FAIL_PUBLISH_UNKNOWN_CAUSE')
    elif context=='stage':
        require('SelectorSync' in text and 'IoWriteUnknown' in text and events[-1]=='sync\td3-dwb-select-stage','FAIL_SELECTOR_SYNC_UNKNOWN_CAUSE')
    else:
        require('phase: Write' in text and 'IoWriteUnknown' in text and events[-1]=='wal-prefix-error\t25296\t38000\t19000','FAIL_INIT_PARTIAL_UNKNOWN_CAUSE')


def check_inspection(folder,root,case,files,oracle):
    require((folder/'allocator.bin').read_bytes()==oracle['allocator'],'FAIL_RAM_ALLOCATOR')
    for name,key in (('status.bin','status'),('descriptor.bin','descriptor')):
        path=folder/name;value=oracle[key]
        require(path.exists()==(value is not None) and (value is None or path.read_bytes()==value),'FAIL_RAM_'+key.upper())
    pairs=[line.split('\t') for line in (folder/'observation.tsv').read_text().splitlines()]
    require(len(pairs)==len({p[0] for p in pairs}),'FAIL_OBSERVATION_CENSUS');facts=dict(pairs)
    expected=expected_observation(files,oracle,case)
    require(set(facts)==set(expected)|{'source_device','source_inode'} and all(facts.get(k)==v for k,v in expected.items()),'FAIL_OBSERVATION_FACTS')
    if CASES[case]['context']=='tail':
        reads=[line.split('\t') for line in (folder/'reads.tsv').read_text().splitlines()]
        require(all(int(r[1])+int(r[3])<=25456 for r in reads if r[0]=='d3-wal-00000001'),'FAIL_UNINSPECTED_TAIL_BODY_READ')
    meta=(folder.parent/'database'/'d3-wal-00000001').stat()
    require(facts.get('source_device')==str(meta.st_dev) and facts.get('source_inode')==str(meta.st_ino),'FAIL_SOURCE_HANDLE_IDENTITY')
    return facts


def checker_identities(root):
    paths={Path(m.__file__).resolve() for m in list(sys.modules.values()) if getattr(m,'__file__',None) and Path(m.__file__).suffix=='.py' and Path(m.__file__).resolve().parent==Path(__file__).resolve().parent}
    paths.add(Path(__file__).resolve());paths.add(Path(__file__).with_name('test_dev3_recovery_prefix_native.py'))
    values={str(p.relative_to(ROOT)):digest(p) for p in paths};require(all(digest(root/p)==value for p,value in values.items()),'FAIL_CHECKER_IDENTITY');return values


def run(root,binary,receipt,output,names=None):
    root,binary,receipt,output=(p.resolve() for p in (root,binary,receipt,output));chosen=selected(names);spec=validate_inputs(root)
    source,binary_hash,receipt_hash=source_fingerprint(root),digest(binary),digest(receipt)
    check_build_receipt(json.loads(receipt.read_text()),root,source,binary_hash);check_binary_format(binary);checkers=checker_identities(root)
    output.mkdir(parents=True,exist_ok=False);retained=output/'native-binary';shutil.copyfile(binary,retained);retained.chmod(0o500)
    report=dict(status='RUNNING',unit='D3-RECOVERY-PREFIX-001',scope='RAM prefix inspection only; home read/mutation/sync/repair0',source_root=str(root),source_sha256=source,binary_sha256=binary_hash,build_receipt_sha256=receipt_hash,input_sha256=INPUT_SHA256,checker_sha256=checkers,selected=chosen,not_selected=[c for c in CASES if c not in chosen],cases=[])
    started=time.monotonic();active_jobs=[]
    try:
        for case in chosen:
            folder=output/case;folder.mkdir();db=folder/'database';db.mkdir();definition=CASES[case];began=time.monotonic()
            row=dict(case=case,**definition,status='RUNNING',processes=[]);report['cases'].append(row)
            def remaining():
                left=min(CASE_SECONDS-(time.monotonic()-began),TOTAL_SECONDS-(time.monotonic()-started));require(left>0,'FAIL_NATIVE_AGGREGATE_TIMEOUT');return left
            def start(mode):
                phase=folder/(mode+'-'+str(len(row['processes'])));phase.mkdir()
                env={k:v for k,v in os.environ.items() if not k.startswith(('XADB_','XADBTEST_','RUST_TEST_'))}
                env.update(XADB_RECOVERY_PREFIX_NATIVE_DIR=str(phase),XADB_RECOVERY_PREFIX_NATIVE_DB=str(db),XADB_RECOVERY_PREFIX_NATIVE_MODE=mode,XADB_RECOVERY_PREFIX_NATIVE_PROFILE=str(definition['profile']),XADB_RECOVERY_PREFIX_NATIVE_KIND=definition['kind'],XADB_RECOVERY_PREFIX_NATIVE_CONTEXT=definition['context'])
                command=[str(retained),'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1']
                stdout=(phase/'stdout.log').open('wb');stderr=(phase/'stderr.log').open('wb');child=subprocess.Popen(command,cwd=root,env=env,stdout=stdout,stderr=stderr,start_new_session=True)
                item=dict(mode=mode,pid=child.pid,command=command,source_sha256=source,binary_sha256=binary_hash,timed_out=False,exit_code=None,started=time.monotonic());row['processes'].append(item)
                job=(child,phase,item,stdout,stderr);active_jobs.append(job);return job
            def finish(job):
                child,phase,item,stdout,stderr=job
                try:
                    wait_child(child,remaining,item)
                finally:
                    stdout.close();stderr.close();item.update(exit_code=child.poll(),elapsed_seconds=time.monotonic()-item.pop('started'));(phase/'process.json').write_text(json.dumps(item,indent=2)+'\n')
                    # An unexpected cleanup failure must leave the child registered
                    # for outer cleanup rather than discard a live process.
                    if child.poll() is not None:active_jobs.remove(job)
                check_process(item);outcome='produced' if item['mode']=='produce' else 'rejected' if item['mode'] in ('reject','contend') else 'analyzed'
                check_stdout((phase/'stdout.log').read_text(),item['mode'],child.pid,outcome);item['resources']=check_resources(phase,db,item['mode']!='produce');return phase
            produced=finish(start('produce'))
            context=definition['context']
            if context in ('raw_corrupt','checksum','unknown'):
                path=db/('d3-dwb-images' if context=='raw_corrupt' else 'd3-wal-00000001');before_hash=digest(path);raw=bytearray(path.read_bytes())
                if context=='raw_corrupt':raw[1000]^=1
                elif context=='checksum':raw[-1]^=1
                else:
                    for off in (166,1888):raw[25296+off:25296+off+2]=(65534).to_bytes(2,'little')
                    raw[-32:]=hashlib.sha256(raw[25296:-32]).digest()
                path.write_bytes(raw);row['harness_input_mutation']=dict(path=path.name,before_sha256=before_hash,after_sha256=digest(path))
            files,oracle=expected_files(root,case)
            require({n:dict(bytes=len(v),sha256=sha(v)) for n,v in files.items()}==spec['expected'][case]['files'] and expected_observation(files,oracle,case)==spec['expected'][case]['facts'],'FAIL_EXPECTED_IDENTITY')
            before=inventory(db);require(sum(r['bytes'] for r in before.values())<=DB_LIMIT,'FAIL_DATABASE_BOUND');require(set(before)==set(files) and all((db/n).read_bytes()==v for n,v in files.items()),'FAIL_PRODUCER_BYTES');row['before']=before
            if context in ('pending','stage','tail'):
                check_producer_cut(produced,context,row['processes'][0]['resources'])
            if context in ('checksum','unknown'):
                rejected=finish(start('reject'));text=(rejected/'error.txt').read_text()
                require('Unchanged' in text and ('Wal(Corrupt)' in text if context=='checksum' else 'UnsupportedOperation' in text),'FAIL_PREFIX_REJECTION')
            elif case=='PN01':
                hold=start('hold')
                try:
                    while not (hold[1]/'holding').exists():require(hold[0].poll() is None,'FAIL_HOLD_NOT_REACHED');remaining();time.sleep(.005)
                    contender=finish(start('contend'));require('Busy' in (contender/'error.txt').read_text(),'FAIL_LEASE_BUSY')
                finally:(hold[1]/'release').write_text('release\n')
                phase=finish(hold);row['observation']=check_inspection(phase,root,case,files,oracle)
                reopened=finish(start('analyze'));check_inspection(reopened,root,case,files,oracle)
            else:
                phase=finish(start('analyze'));row['observation']=check_inspection(phase,root,case,files,oracle)
            after=inventory(db);require(after==before,'FAIL_DATABASE_CHANGED');row['after']=after
            pids=[p['pid'] for p in row['processes']];require(len(pids)==len(set(pids)),'FAIL_FRESH_PID')
            row.update(database_bytes=sum(v['bytes'] for v in after.values()),elapsed_seconds=time.monotonic()-began)
            require(row['elapsed_seconds']<=CASE_SECONDS and sum(r.get('database_bytes',0) for r in report['cases'])<=TOTAL_LIMIT,'FAIL_NATIVE_RESOURCE_BUDGET')
            row['status']='PASS'
        require(source_fingerprint(root)==source and digest(binary)==digest(retained)==binary_hash and digest(receipt)==receipt_hash and validate_inputs(root)==spec and checker_identities(root)==checkers,'FAIL_POST_IDENTITY')
        check_total_budget(time.monotonic()-started)
        report['status']='PASS'
    except BaseException as error:
        report.update(status='FAIL',error=str(error))
        for row in report['cases']:
            if row['status']=='RUNNING':row['status']='FAIL'
        raise
    finally:
        for child,phase,item,stdout,stderr in active_jobs:
            kill_and_reap(child)
            stdout.close();stderr.close();item.update(exit_code=child.returncode,aborted_by_controller=True,elapsed_seconds=item.get('elapsed_seconds',time.monotonic()-item.get('started',time.monotonic())));item.pop('started',None);(phase/'process.json').write_text(json.dumps(item,indent=2)+'\n')
        report['not_run']=[c for c in chosen if c not in {r['case'] for r in report['cases']}];report['elapsed_seconds']=time.monotonic()-started;(output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=ROOT)
    for name in ('binary','build-receipt','output'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--case',action='append');args=parser.parse_args();report=run(args.root,args.binary,args.build_receipt,args.output,args.case);print(json.dumps(dict(status=report['status'],completed=len(report['cases']))))


if __name__=='__main__':main()
