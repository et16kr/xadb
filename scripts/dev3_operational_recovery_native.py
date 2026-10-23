#!/usr/bin/env python3
"""Fresh repeated producer -> separate recovery PID -> reentry, full-file oracle.

Four mandatory histories, each <=30s including its workers/verification. Database
logical size <=24MiB each, retained databases <=96MiB; caller arenas, snapshots,
source/ELF and logs are separate resources, not part of a claimed RSS bound.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import sys
import subprocess
import struct
import time

from check_dev0 import digest, source_fingerprint
import dev3_recovery_effects_native as prior
import dev3_recovery_effects_oracle as model

SELECTOR = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::effect::native::operational::worker_operational_recovery_native'
CASES = ('plain', 'tail', 'partial_b', 'retirement_b')
require = model.require


def literal(root, name):
    return (root / 'tests/fixtures/dev3-operational-status/p1' / name).read_bytes()


def inspect_target(page):
    if page[6]==10:
        model.wire.inspect_page(page)
        return
    require(len(page)==model.PAGE and page[6:8]==bytes([9,0]) and page[8:24]==model.wire.DATABASE,
            'FAIL_STATUS_ENVELOPE')
    require(model.number(page,24)==0x100000001 and 1<=model.number(page,32)<64
            and model.number(page,40)>=2 and model.number(page,48)==2 and model.number(page,56)==1
            and model.number(page,64)>0 and model.sha(page[:-32])==page[-32:].hex(),'FAIL_STATUS_IDENTITY')


def wire_control(generation,batch,page=None):
    require(0<generation<2**64 and 0<=batch<2**64 and (page is None or batch>0),'FAIL_CONTROL_COUNTERS')
    raw=bytearray(128+(88 if page is not None else 0))
    struct.pack_into('<4sHBB16sQQQH',raw,0,b'XDW1',1,int(page is not None),0,model.wire.DATABASE,1,generation,batch,int(page is not None))
    if page is not None:
        inspect_target(page)
        struct.pack_into('<Q',raw,56,model.number(page,64))
        struct.pack_into('<HBB',raw,128,0,page[6],page[7])
        raw[136:176]=page[24:64];raw[176:184]=page[64:72]
        raw[184:216]=bytes.fromhex(model.sha(page))
    return model.wire.seal(raw)


def wire_slot(batch,page):
    inspect_target(page);require(0<batch<2**64,'FAIL_SLOT_SEQUENCE')
    raw=bytearray(128)
    struct.pack_into('<4sHH16sQQ',raw,0,b'XDS1',1,0,model.wire.DATABASE,1,batch)
    raw[40:80]=page[24:64];raw[80:88]=page[64:72];raw[88:90]=page[6:8]
    raw[96:128]=bytes.fromhex(model.sha(page))
    return model.wire.seal(raw+page)


def select(files,generation,batch,page=None,cut=9):
    logical=wire_control(generation,batch,page);bank=(generation-1)%2
    full=logical+bytes(model.PAGE-len(logical))
    selector=model.wire.selector(files['d3-profile'],generation,bank,logical)
    if cut>=1:files['d3-dwb-stage']=full
    if cut>=3:files[f'd3-dwb-{bank}']=full
    if cut>=5:files['d3-dwb-select-stage']=b''
    if cut>=6:files['d3-dwb-select-stage']=selector
    if cut>=8:files['DWB']=files.pop('d3-dwb-select-stage')


def flush(files,image,batch,cut=31,partial=None):
    select(files,3*batch-1,batch,cut=min(cut,9))
    if cut>=10:
        raw=wire_slot(batch,image);files['d3-dwb-images']=raw+files['d3-dwb-images'][len(raw):]
    if cut>=12:select(files,3*batch,batch,image,cut=min(cut-11,9))
    if cut>=21:
        if partial is None:model.put_home(files,image)
        else:
            offset=model.number(image,32)*model.PAGE;old=files[model.DATA]
            files[model.DATA]=old[:offset]+image[:partial]+old[offset+partial:]
    if cut>=23:select(files,3*batch+1,batch,cut=min(cut-22,9))


def producer_files(root, tail):
    files = model.prefix.initial_files(root, 1, 2)
    files['LOCK'] = b''
    frames = literal(root, 'wal.frames-only')
    files[model.WAL] += frames[:-19000] if tail else frames
    flush(files, literal(root, 'work1-status.after'), 1)
    flush(files, literal(root, 'work1-allocator.after'), 2)
    return files


def recover_files(files, root, case, failed=False):
    """State transitions are authored from the fixed history, not observed traces."""
    result = dict(files)
    for name in model.SLOTS:
        result[name] = bytes(model.Q)
    bank = result[f'd3-dwb-{result["DWB"][81]}']
    batch = model.number(bank, 40)
    if bank[6] == 1:
        image = result['d3-dwb-images'][128:128+model.PAGE]
        model.put_home(result, image)
        select(result, 3 * batch + 1, batch)
    targets = [literal(root, 'work3-allocator.reserved' if case == 'tail' else 'work3-allocator.after'), literal(root, 'work1-status.after')]
    if case != 'tail':
        targets.append(literal(root, 'work3-status.after'))
    for image in targets:
        at = model.number(image, 32) * model.PAGE
        if result[model.DATA][at:at+model.PAGE] == image:
            continue
        batch += 1
        if failed and case == 'partial_b' and at == 2 * model.PAGE:
            flush(result, image, batch, cut=21, partial=8192)
            return result
        if failed and case == 'retirement_b' and at == 2 * model.PAGE:
            flush(result, image, batch, cut=30)
            return result
        flush(result, image, batch)
    require(not failed, 'FAIL_EXPECTED_FAULT_TARGET')
    return result


def check_worker(text, mode, pid):
    require(re.findall(r'^test ([\w:]+) \.\.\.', text, re.M) == [SELECTOR], 'FAIL_EXACT_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', text, re.M) == [('1','0','0','0')], 'FAIL_EXACT_WORKER_CENSUS')
    require(re.findall(r'OPERATIONAL_RECOVERY_NATIVE_[^\r\n]+', text) == [
        f'OPERATIONAL_RECOVERY_NATIVE_BEGIN mode={mode} pid={pid}',
        f'OPERATIONAL_RECOVERY_NATIVE_PASS mode={mode} pid={pid}'], 'FAIL_NATIVE_PROTOCOL')


def check_fault(folder, case):
    text = (folder / 'error.tsv').read_text()
    require('phase\tFlush' in text and 'CallerMustResolve' in text, 'FAIL_TYPED_FAULT')
    rows = prior.events(folder)
    if case == 'partial_b':
        require(rows[-1][:4] == ['write', model.DATA, '32768', '16384'] and rows[-1][4].startswith('ERR_PARTIAL:8192:'), 'FAIL_B_PARTIAL_WRITE')
    else:
        require(rows[-1] == ['publish', 'd3-dwb-select-stage', '0', '0', 'OK'] and 'Namespace' in text, 'FAIL_B_RETIREMENT_UNKNOWN')


def checker_identities(root):
    paths={Path(module.__file__).resolve() for module in list(sys.modules.values())
           if getattr(module,'__file__',None) and Path(module.__file__).suffix=='.py'
           and Path(module.__file__).parent.name=='scripts'}
    paths.add(Path(__file__).resolve())
    require(all(path.parent==root/'scripts' for path in paths),'FAIL_IMPORTED_CHECKER_SOURCE')
    return {str(path.relative_to(root)):digest(path) for path in sorted(paths)}


def option_array(raw):
    return json.loads(re.sub(r'Some\((\d+)\)', r'\1', raw).replace('None','null'))


def check_inspection(folder,root,case,before,after):
    active=case=='tail'
    allocator=literal(root,'work3-allocator.reserved' if active else 'work3-allocator.after')
    descriptor=literal(root,'work3-reserve.descriptor' if active else 'work3-terminal.descriptor')
    require((folder/'allocator.bin').read_bytes()==allocator and (folder/'descriptor.bin').read_bytes()==descriptor,'FAIL_PHYSICAL_OR_DESCRIPTOR')
    require((folder/'status-2.bin').exists()==(not active),'FAIL_STATUS2_PRESENCE')
    rows=dict(line.split('=',1) for line in (folder/'all-targets.txt').read_text().splitlines())
    require(set(rows)=={'mask','initial','final','applied','skipped'},'FAIL_TARGET_FIELD_CENSUS')
    require(int(rows['mask'])==(2 if active else 6),'FAIL_TARGET_MASK')
    final=[None]*64; initial=[None]*64; effective=[None]*64; applied=[0]*64; skipped=[0]*64
    for index in range(2 if active else 3):
        old=before[model.DATA][index*model.PAGE:(index+1)*model.PAGE]
        new=after[model.DATA][index*model.PAGE:(index+1)*model.PAGE]
        final[index]=model.number(new,64)
        if model.sha(old[:-32])==old[-32:].hex(): initial[index]=model.number(old,64)
        effective[index]=initial[index]
    bank=before[f'd3-dwb-{before["DWB"][81]}']
    if bank[6]==1:
        pending=before['d3-dwb-images'][128:128+model.PAGE]
        effective[model.number(pending,32)]=model.number(pending,64)
    applied[0]=final[0]-effective[0];skipped[0]=effective[0]
    for index in range(1,2 if active else 3):
        applied[index]=int(effective[index]!=final[index]);skipped[index]=1-applied[index]
    require(option_array(rows['initial'])==initial and option_array(rows['final'])==final,'FAIL_ALL_TARGET_LSNS')
    require(json.loads(rows['applied'])==applied and json.loads(rows['skipped'])==skipped,'FAIL_ALL_TARGET_REDO_COUNTS')


def wait_worker(child, item, history_start, aggregate_start):
    started = time.monotonic()
    def remaining():
        value = min(30 - (time.monotonic() - history_start),
                    120 - (time.monotonic() - aggregate_start))
        require(value > 0, 'FAIL_WORKER_BUDGET')
        return value
    try:
        prior.finish_child(child, remaining, item)
    finally:
        item.update(exit_code=child.poll(), elapsed_seconds=time.monotonic() - started)


def run(root, binary, receipt, output, selected):
    root, binary, receipt, output = root.resolve(), binary.resolve(), receipt.resolve(), output.resolve()
    require(selected and len(set(selected)) == len(selected) and set(selected) <= set(CASES), 'FAIL_CASE_SELECTION')
    require(not output.exists(), 'FAIL_OUTPUT_EXISTS')
    source, binary_sha = source_fingerprint(root), digest(binary)
    build = json.loads(receipt.read_text())
    frozen = json.loads((receipt.parent/'source.json').read_text())
    require(root == (receipt.parent/'source').resolve() and frozen['source_sha256'] == source
            and build['status']=='PASS' and build['exit_code']==0 and build['source_sha256']==source
            and Path(build['source_root']).resolve()==root and build['binary_sha256'] == binary_sha and build['command'] == ['cargo','test','-p','xadb-engine','--lib','--no-run','--locked','--offline','--message-format=json'], 'FAIL_BUILD_BINDING')
    require(Path(__file__).resolve() == root/'scripts/dev3_operational_recovery_native.py', 'FAIL_CHECKER_SOURCE')
    compile_rows=[json.loads(line) for line in (receipt.parent/'build.jsonl').read_text().splitlines() if line.startswith('{')]
    require([row.get('success') for row in compile_rows if row.get('reason')=='build-finished']==[True]
            and not any(row.get('reason')=='compiler-message' and row.get('message',{}).get('level')=='error' for row in compile_rows), 'FAIL_COMPILE_RECEIPT')
    checkers=checker_identities(root)
    receipt_sha = digest(receipt)
    output.mkdir(parents=True)
    retained = output / 'engine-tests'; shutil.copy2(binary, retained)
    report = dict(status='RUNNING', source_sha256=source, binary_sha256=binary_sha, build_receipt_sha256=receipt_sha, checker_sha256=checkers, cases=[], not_run=list(selected))
    total = time.monotonic()
    try:
        for case in selected:
            report['not_run'].remove(case)
            began = time.monotonic(); folder = output / case; folder.mkdir(); db = folder / 'database'; db.mkdir()
            row = dict(case=case, status='RUNNING', workers=[]); report['cases'].append(row)
            def worker(mode, label, fault=''):
                stage = folder / label; stage.mkdir()
                env = {k:v for k,v in os.environ.items() if not k.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_'))}
                env.update(XADB_OPERATIONAL_RECOVERY_DB=str(db), XADB_OPERATIONAL_RECOVERY_OUT=str(stage), XADB_OPERATIONAL_RECOVERY_MODE=mode, XADB_OPERATIONAL_RECOVERY_FAULT=fault)
                command = [str(retained), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
                item = dict(mode=mode, fault=fault, command=command, environment={k:v for k,v in env.items() if k.startswith('XADB_')}, status='RUNNING', timed_out=False)
                row['workers'].append(item)
                start = time.monotonic()
                with (stage/'stdout.log').open('x') as stdout, (stage/'stderr.log').open('x') as stderr:
                    with subprocess.Popen(command, cwd=root, env=env, stdout=stdout, stderr=stderr, start_new_session=True) as child:
                        item['pid'] = child.pid
                        try:
                            wait_worker(child, item, began, total)
                        finally:
                            stdout.flush(); stderr.flush()
                            item.update(stdout_sha256=digest(stage/'stdout.log'), stderr_sha256=digest(stage/'stderr.log'))
                item['elapsed_seconds'] = time.monotonic()-start
                require(item['exit_code'] == 0 and not item['timed_out'], 'FAIL_WORKER_OR_TIMEOUT')
                check_worker((stage/'stdout.log').read_text(), mode, item['pid'])
                item['resources'] = prior.check_resources(stage, 'produce' if mode.startswith('produce') else mode)
                require(item['resources']['cut_hits']==int(mode in ('produce_tail','fail')), 'FAIL_CUT_CENSUS')
                if mode=='produce_tail':
                    tail_errors=[r for r in prior.events(stage) if r[0]=='write' and r[1]==model.WAL and r[4].startswith('ERR_PARTIAL:')]
                    require(len(tail_errors)==1 and tail_errors[0][2:4]==['126896','38000'] and tail_errors[0][4].startswith('ERR_PARTIAL:19000:'), 'FAIL_PRODUCER_TAIL_CUT')
                    require('IoWriteUnknown' in (folder/'producer-tail-error.txt').read_text() and 'CallerMustResolve' in (folder/'producer-tail-error.txt').read_text(), 'FAIL_PRODUCER_TAIL_CAUSE')
                item.update(status='PASS', stdout_sha256=digest(stage/'stdout.log'), stderr_sha256=digest(stage/'stderr.log'))
                return stage
            expected = producer_files(root, case == 'tail')
            worker('produce_tail' if case == 'tail' else 'produce', 'producer')
            before = prior.compare_files(db, expected)
            (folder/'producer-files.json').write_text(json.dumps(before,indent=2)+'\n')
            if case in ('partial_b','retirement_b'):
                stage = worker('fail', 'failure', 'operational_b_partial' if case == 'partial_b' else 'operational_b_retirement_after')
                check_fault(stage,case)
                expected = recover_files(expected,root,case,True)
                failed = prior.compare_files(db,expected)
                (folder/'failed-files.json').write_text(json.dumps(failed,indent=2)+'\n')
                # Retain the failure boundary bytes without copying the whole DB.
                for name in (model.DATA,'DWB','d3-dwb-0','d3-dwb-1','d3-dwb-images'):
                    raw=(db/name).read_bytes()
                    (stage/(name+'.snapshot')).write_bytes(raw[:16544] if name=='d3-dwb-images' else raw)
            before_recovery=dict(expected)
            expected = recover_files(expected,root,case)
            recovered = worker('recover','recovery')
            after = prior.compare_files(db,expected); prior.preserved_inodes(before,after)
            facts = prior.observation(recovered)
            require(facts['complete_lsn'] == ('5' if case=='tail' else '6') and facts['highwater']=='3', 'FAIL_PREFIX_FACTS')
            require(facts['replay_topup'] == ('16384' if case=='tail' else '49152'), 'FAIL_TARGET_TOPUP')
            require((recovered/'status-1.bin').read_bytes()==literal(root,'work1-status.after'), 'FAIL_OLDER_STATUS')
            if case!='tail': require((recovered/'status-2.bin').read_bytes()==literal(root,'work3-status.after'), 'FAIL_NEW_STATUS')
            check_inspection(recovered,root,case,before_recovery,expected)
            prior.check_census(recovered)
            reentry=worker('recover','reentry')
            final=prior.compare_files(db,expected);prior.preserved_inodes(after,final)
            check_inspection(reentry,root,case,expected,expected)
            require(prior.observation(reentry)['dirty_mask']=='0' and not any(r[0]=='write' and r[1]==model.DATA for r in prior.events(reentry)), 'FAIL_REENTRY_REDO_WRITE')
            require(len({w['pid'] for w in row['workers']})==len(row['workers']), 'FAIL_SAME_PROCESS_REOPEN')
            row.update(elapsed_seconds=time.monotonic()-began, final_files=after)
            require(row['elapsed_seconds']<=30, 'FAIL_HISTORY_BUDGET')
            row['status']='PASS'
        require(source_fingerprint(root)==source and digest(binary)==binary_sha==digest(retained) and digest(receipt)==receipt_sha and checker_identities(root)==checkers,'FAIL_SOURCE_BINARY_CHANGED')
        require(time.monotonic()-total<=120,'FAIL_AGGREGATE_BUDGET')
        report['status']='PASS'
    except BaseException as error:
        report.update(status='FAIL',error=str(error))
        if report['cases']:
            row=report['cases'][-1]
            if row['status']=='RUNNING': row['status']='FAIL'
            for item in row['workers']:
                if item['status']=='RUNNING': item['status']='FAIL'
    finally:
        report['elapsed_seconds']=time.monotonic()-total
        (output/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('root','binary','build-receipt','output'): p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--select',choices=CASES,nargs='+',default=list(CASES))
    args=p.parse_args(); report=run(args.root,args.binary,args.build_receipt,args.output,args.select)
    print(json.dumps({k:report[k] for k in ('status','elapsed_seconds','not_run')}))
    raise SystemExit(0 if report['status']=='PASS' else 1)


if __name__=='__main__': main()
