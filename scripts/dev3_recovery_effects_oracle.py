#!/usr/bin/env python3
"""Independent closed-recovery wire/state authoring; no product code is executed."""
from pathlib import Path
import hashlib
import struct
import dev3_recovery_prefix_native as prefix
import dev3_status_home_flush_literals as wire

UNIT='D3-RECOVERY-EFFECTS-001'
DATA='d3-data-0000000100000001'
WAL='d3-wal-00000001'
SLOTS=('d3-rebase-0','d3-rebase-1')
Q=1048576
PAGE=16384
CASES={}
for p in (1,2,3):CASES[f'NR{p:02}']=dict(profile=p,kind='SI',count=1,cycles=0,context='plain')
for n,p,k,c,h,x in ((4,1,'EA',1,0,'plain'),(5,2,'DC',63,0,'plain'),(6,3,'SA',1,0,'plain'),(7,1,'SC',1,1,'plain'),(8,1,'SI',1,0,'a_flushed'),(9,2,'SI',1,0,'s_flushed'),(10,3,'SI',1,0,'both_flushed'),(11,1,'SI',1,0,'s_pending_torn'),(12,2,'DC',1,0,'a_pending_torn'),(13,1,'DC',1,0,'historical'),(14,1,'SI',1,0,'raw_corrupt'),(15,1,'SI',1,0,'stage4096'),(16,1,'SI',1,0,'stage0'),(17,1,'SI',1,0,'s_torn'),(18,1,'EC',1,0,'a_torn'),(19,1,'SI',1,0,'future_pending'),(20,1,'SI',1,0,'repair_partial'),(21,1,'SI',1,0,'retirement_unknown'),(22,1,'SI',1,0,'redo_second_partial'),(23,1,'SI',1,0,'tail'),(24,1,'SI',1,0,'checksum'),(25,1,'SI',1,0,'slot_corrupt')):
    CASES[f'NR{n:02}']=dict(profile=p,kind=k,count=c,cycles=h,context=x)
for i in range(1,7):CASES[f'NB{i:02}']=dict(profile=1,kind='SI',count=1,cycles=0,context='boot'+str(i))
S5_CASES={}
for profile in (1,2,3):
    for kind in ('EA','EC','DA','DC','SA','SC','SI'):
        for count in ((1,8,63) if not kind.startswith('S') else (1,)):
            for cycles in ((0,1,2,8) if kind in ('SC','SI') else (0,)):
                S5_CASES[f'NS5{len(S5_CASES)+1:03}']=dict(profile=profile,kind=kind,count=count,cycles=cycles,context='plain')
ALL_CASES={**CASES,**S5_CASES}
ALIASES={name:next(key for key,mandatory in CASES.items() if mandatory==row) for name,row in S5_CASES.items() if row in CASES.values()}


def require(ok,why):
    if not ok:raise ValueError(why)


def sha(raw):return hashlib.sha256(raw).hexdigest()


def number(raw,at):return int.from_bytes(raw[at:at+8],'little')


def material(root,row):
    p,k,n,c,x=(row[key] for key in ('profile','kind','count','cycles','context'))
    files=prefix.initial_files(root,p,2 if k.startswith('S') else 1)
    files['LOCK']=b''
    if k.startswith('S'):
        profile=files['d3-profile'];genesis=files[DATA][:PAGE]
        terminal='cancel' if k=='SC' else 'init'
        values,_=wire.candidate_history(profile,genesis,c,terminal)
        work=c+1;stem=f'work{work}-'
        active=k=='SA' or x=='tail'
        frames=values['wal.frames-only']
        if k=='SA':frames=frames[:-38000]
        if x=='tail':frames=frames[:-19000]
        a=values[stem+('allocator.reserved' if active else 'allocator.after')]
        s=None if k!='SI' or x=='tail' else values[stem+'status.after']
        d=values[stem+('reserve.descriptor' if active else terminal+'.descriptor')]
    else:
        base=Path('tests/fixtures/dev3-allocation-reservation' if k.startswith('E') else 'tests/fixtures/dev3-dirty-allocation')
        stem=f'p{p}-n{n}-';active=k.endswith('A');prior=[i for i in (1,8,63) if i<n];work=len(prior)+1
        frames=b''.join(prefix.read(root,base/(f'p{p}-n{i}-'+part+'.frame')) for i in prior for part in ('reserve','cancel'))
        frames+=prefix.read(root,base/(stem+'reserve.frame'))
        if not active:frames+=prefix.read(root,base/(stem+'cancel.frame'))
        part='reserve' if active else 'cancel'
        a=prefix.read(root,base/(stem+part+'.page'));d=prefix.read(root,base/(stem+part+'.descriptor'));s=None
    files[WAL]+=frames
    hold_count=n if not active else (int(c>0) if k.startswith('S') else max((i for i in (1,8,63) if i<n),default=0))
    hold=(1<<(hold_count+1))-2 if hold_count else 0
    return files,dict(allocator=a,status=s,descriptor=d,active=active,work=work,lsn=number(a,64),count=n,terminal_hold_mask=hold,complete_offset=25296 if x=='tail' else len(files[WAL]))


def put_home(files,image):
    offset=number(image,32)*PAGE;old=files[DATA]
    files[DATA]=old[:offset]+image+old[offset+PAGE:]


def selections(files,generation,batch,page=None,cut=9):
    logical=wire.control(generation,batch,page);bank=(generation-1)%2
    full=logical+bytes(PAGE-len(logical));selector=wire.selector(files['d3-profile'],generation,bank,logical)
    if cut>=1:files['d3-dwb-stage']=full
    if cut>=3:files[f'd3-dwb-{bank}']=full
    if cut>=5:files['d3-dwb-select-stage']=b''
    if cut>=6:files['d3-dwb-select-stage']=selector
    if cut>=8:files['DWB']=files.pop('d3-dwb-select-stage')


def flush(files,image,batch,cut=31,partial=None):
    selections(files,3*batch-1,batch,cut=min(cut,9))
    if cut>=10:
        raw=wire.slot(batch,image);files['d3-dwb-images']=raw+files['d3-dwb-images'][len(raw):]
    if cut>=12:selections(files,3*batch,batch,image,cut=min(cut-11,9))
    if cut>=21:
        if partial is None:put_home(files,image)
        else:
            offset=number(image,32)*PAGE;old=files[DATA];files[DATA]=old[:offset]+image[:partial]+old[offset+partial:]
    if cut>=23:selections(files,3*batch+1,batch,cut=min(cut-22,9))


def prepared(root,case,adversarial=True):
    row=ALL_CASES[case];x=row['context'];files,o=material(root,row)
    a,s=o['allocator'],o['status']
    if x in ('a_flushed','both_flushed','raw_corrupt') or x.startswith('boot'):
        flush(files,a,1)
    if x in ('s_flushed','both_flushed','raw_corrupt') or x.startswith('boot'):
        flush(files,s,2 if x!='s_flushed' else 1)
    if x in ('s_pending_torn','future_pending','repair_partial','slot_corrupt'):
        flush(files,s,1,19)
    if x=='a_pending_torn':flush(files,a,1,19)
    if x=='historical':
        base=Path('tests/fixtures/dev3-dirty-allocation/p1-n1-reserve.page')
        old=prefix.read(root,base);flush(files,old,1,19)
    if x=='retirement_unknown':
        flush(files,a,1);flush(files,s,2,19)
    if x in ('stage4096','stage0'):flush(files,s,1,18 if x=='stage4096' else 16)
    if not adversarial:return files,o
    if x in ('s_pending_torn','s_torn','a_pending_torn','a_torn'):
        raw=bytearray(files[DATA]);raw[(PAGE if x.startswith('s_') else 0)+8000]^=1;files[DATA]=bytes(raw)
    if x in ('raw_corrupt','slot_corrupt'):
        raw=bytearray(files['d3-dwb-images']);raw[1000]^=1;files['d3-dwb-images']=bytes(raw)
    if x=='future_pending':
        raw=bytearray(s);raw[64:72]=(3).to_bytes(8,'little');raw[-32:]=hashlib.sha256(raw[:-32]).digest();put_home(files,bytes(raw))
    if x=='checksum':
        raw=bytearray(files[WAL]);raw[-1]^=1;files[WAL]=bytes(raw)
    if x=='boot5':files.update({name:bytes(Q) for name in SLOTS})
    if x=='boot6':files.update({SLOTS[0]:bytes(17),SLOTS[1]:bytes(Q-1)+b'\x01'})
    return files,o


def recover_model(before,o,fault='none',reject=False):
    files=dict(before)
    if fault=='boot0_partial':files[SLOTS[0]]=bytes(32768);return files
    files[SLOTS[0]]=bytes(Q)
    if fault=='boot1_partial':files[SLOTS[1]]=bytes(163840);return files
    files[SLOTS[1]]=bytes(Q)
    if fault in ('boot_sync_after','boot_dir_after') or reject:return files
    selected=files['DWB'];generation=number(selected,32);bank=selected[81]
    control=files[f'd3-dwb-{bank}'];batch=number(control,40)
    if control[6]==1:
        image=files['d3-dwb-images'][128:128+PAGE]
        if fault=='repair_partial':
            offset=number(image,32)*PAGE;raw=files[DATA];files[DATA]=raw[:offset]+image[:8192]+raw[offset+8192:];return files
        put_home(files,image)
        selections(files,generation+1,batch,cut=8 if fault=='retirement_publish_after' else 9)
        if fault=='retirement_publish_after':return files
    for image in (o['allocator'],o['status']):
        if image is None:continue
        at=number(image,32)*PAGE
        if files[DATA][at:at+PAGE]==image:continue
        batch+=1
        if fault=='redo_second_partial' and at==PAGE:
            flush(files,image,batch,21,8192);return files
        flush(files,image,batch)
    return files


def histories(case):
    x=ALL_CASES[case]['context']
    if x in ('checksum','boot6'):return [('reject_acquire','none')]
    if x in ('a_torn','future_pending','slot_corrupt'):return [('reject_recover','none')]
    first={'repair_partial':'repair_partial','retirement_unknown':'retirement_publish_after','redo_second_partial':'redo_second_partial','boot2':'boot0_partial','boot3':'boot1_partial'}
    if x=='boot4':return [('fail','boot_sync_after'),('fail','boot_dir_after'),('recover','none'),('verify','none')]
    if x in first:return [('fail',first[x]),('recover','none'),('verify','none')]
    return [('recover','none'),('verify','none')]


def snapshots(root,case):
    files,o=prepared(root,case);result=[dict(files)];steps=histories(case)
    for mode,fault in steps:
        if mode=='reject_acquire':pass
        else:files=recover_model(files,o,fault,mode=='reject_recover')
        result.append(dict(files))
    return result,o


def facts(files,o):
    d=o['descriptor'];wal=files[WAL];sel=files['DWB'];bank=files[f'd3-dwb-{sel[81]}']
    r=len(wal)-4096;w=number(d,912);p=number(d,1008);inventory=number(d,552)
    credit=(p-w)//PAGE;topup=max(1+int(o['status'] is not None)+inventory-credit,0)*PAGE
    s=len(files.get('d3-dwb-select-stage',b''));z=sum(len(files.get(name,b'')) for name in SLOTS)
    return dict(complete_lsn=o['lsn'],complete_offset=o['complete_offset'],source_length=len(wal),highwater=o['work'],active_mask=(1<<(o['count']+1))-2 if o['active'] else 0,terminal_hold_mask=o['terminal_hold_mask'],inventory=inventory,inventory_digest=d[560:592].hex(),remaining_wal=w,remaining_pages=number(d,936),remaining_frames=number(d,960),remaining_memory=number(d,968),remaining_files=number(d,992),remaining_protected=p,replay_topup=topup,actual_owned=22069248+r+s+z,outstanding=2097152-z+4096-s+12288+w,owned_peak=24182784+r+w,protected=23134208+r+p+topup,epoch_credit=0,restored=1,generation=number(sel,32),batch=number(bank,40),bank=sel[81])


def failed_facts(before,o,fault,context):
    """F07 last acknowledged stamps/credits, never guessed physical completion."""
    result=facts(before,o)
    if fault=='boot0_partial':completed=0
    elif fault in ('boot1_partial','boot_sync_after'):completed=1
    else:completed=2
    growth=sum(Q-len(before.get(name,b'')) for name in SLOTS[:completed])
    result['actual_owned']+=growth;result['outstanding']-=growth
    if fault=='retirement_publish_after':
        # Exact staged metadata was acknowledged; publish outcome is uncertain.
        # Keep the selector credit charged even when rename actually consumed it.
        credit=4096-len(before.get('d3-dwb-select-stage',b''))
        result['actual_owned']+=credit;result['outstanding']-=credit
    result['restored']=int(fault=='redo_second_partial' or context=='a_torn')
    result['source_stabilized']=str(not fault.startswith('boot')).lower()
    keys=('actual_owned','outstanding','owned_peak','protected','replay_topup','remaining_wal','remaining_pages','remaining_frames','remaining_memory','remaining_files','remaining_protected','epoch_credit','restored','source_stabilized')
    return {k:result[k] for k in keys}
