#!/usr/bin/env python3
"""Contract-authored operation vectors; never learned from a successful trace."""
import dev3_recovery_effects_oracle as model


def row(phase,kind,name,offset=0,length=0):return dict(phase=phase,kind=kind,name=name,offset=offset,length=length)


def boot_calls(existing):
    """F07 exact logical port calls; close has no fallible port call."""
    calls=[row('Inventory','Inventory','.',0,18)]
    for i,length in enumerate(existing):
        if length is None:continue
        name=model.SLOTS[i];calls.extend((row('Inventory','OpenRead',name),row('Inventory','Metadata',name)))
        for offset in range(0,length,65536):calls.append(row('Inventory','Read',name,offset,min(65536,length-offset)))
    for i,length in enumerate(existing):
        name=model.SLOTS[i];phase='BootstrapProvision'
        if length is None:calls.append(row(phase,'Create',name))
        calls.extend((row(phase,'OpenWrite',name),row(phase,'Metadata',name)))
        calls.extend(row(phase,'Write',name,j*65536,65536) for j in range(16))
        calls.extend((row(phase,'FileSync',name),row(phase,'OpenRead',name),row(phase,'Metadata',name)))
    calls.append(row('BootstrapProvision','DirectorySync','.'))
    calls.extend(row('BootstrapProvision','Metadata',name) for name in (*model.SLOTS,model.WAL))
    return calls


SOURCE_CALLS=[row('SourceStabilize','FileSync',model.WAL),row('SourceStabilize','DirectorySync','.'),row('SourceStabilize','Metadata',model.WAL)]


def mutation_vector(before,oracle,mode,fault):
    """Expected raw mutating API calls, including payload SHA, from wire tables."""
    files=dict(before);result=[]
    def event(op,name,offset=0,length=0,raw=None):
        value=dict(op=op,name=name,offset=offset,length=length,sha256=None if raw is None else model.sha(raw))
        if raw is not None:value['_payload']=raw
        result.append(value)
    def write(name,raw,offset=0):event('write',name,offset,len(raw),raw)
    def select(g,b,image=None,phase='selection'):
        logical=model.wire.control(g,b,image);bank=(g-1)%2;full=logical+bytes(16384-len(logical));selector=model.wire.selector(files['d3-profile'],g,bank,logical)
        write('d3-dwb-stage',full);event('sync','d3-dwb-stage');write(f'd3-dwb-{bank}',full);event('sync',f'd3-dwb-{bank}')
        if 'd3-dwb-select-stage' not in files:event('create','d3-dwb-select-stage')
        write('d3-dwb-select-stage',selector);event('sync','d3-dwb-select-stage');event('publish','d3-dwb-select-stage')
        model.selections(files,g,b,image,8)
        if fault=='retirement_publish_after' and phase=='repair_retirement':return False
        event('dirsync','namespace');return True
    if mode=='reject_acquire':return result
    for i,name in enumerate(model.SLOTS):
        if name not in files:event('create',name)
        for j in range(16):
            write(name,bytes(65536),j*65536)
            if fault=='boot0_partial' and i==0 and j==0:return result
            if fault=='boot1_partial' and i==1 and j==2:return result
        files[name]=bytes(model.Q);event('sync',name)
        if fault=='boot_sync_after' and i==1:return result
    event('dirsync','namespace')
    if fault=='boot_dir_after':return result
    event('sync',model.WAL);event('dirsync','namespace')
    if mode=='reject_recover':return result
    selected=files['DWB'];bank=files[f'd3-dwb-{selected[81]}'];b=model.number(bank,40)
    if bank[6]==1:
        image=files['d3-dwb-images'][128:128+16384];at=model.number(image,32)*16384
        if files[model.DATA][at:at+16384]!=image:
            write(model.DATA,image,at)
            if fault=='repair_partial':return result
        event('sync',model.DATA);model.put_home(files,image)
        if not select(model.number(selected,32)+1,b,phase='repair_retirement'):return result
    for image in (oracle['allocator'],oracle['status']):
        if image is None:continue
        at=model.number(image,32)*16384
        if files[model.DATA][at:at+16384]==image:continue
        b+=1;select(3*b-1,b)
        write('d3-dwb-images',model.wire.slot(b,image));event('sync','d3-dwb-images');select(3*b,b,image)
        write(model.DATA,image,at)
        if fault=='redo_second_partial' and at==16384:return result
        event('sync',model.DATA);model.put_home(files,image);select(3*b+1,b)
    return result


def check_raw_mutations(rows,expected):
    actual=[]
    for op,name,offset,length,value in rows:
        if op not in ('write','sync','dirsync','create','publish'):continue
        actual.append(dict(op=op,name=name,offset=int(offset),length=int(length),sha256=value.split(':')[-1] if op=='write' else None))
    model.require(actual==[{k:v for k,v in e.items() if k!='_payload'} for e in expected],'FAIL_LITERAL_DURABILITY_ORDER_OR_PAYLOAD')
    return len(actual)


def check_read_bytes(rows,before,expected):
    """Check actual read hashes against the independently modeled state at that call."""
    files=dict(before);cursor=iter(expected);reads=0
    for op,name,offset,length,value in rows:
        offset,length=int(offset),int(length)
        if op=='read':
            count,sha=value.split(':');count=int(count)
            model.require(name in files and 0<=count<=length and offset+count<=len(files[name]),'FAIL_READ_RANGE')
            model.require(sha==model.sha(files[name][offset:offset+count]),'FAIL_READ_CONTENT_IDENTITY');reads+=1
        elif op in ('write','sync','dirsync','create','publish'):
            item=next(cursor)
            if op=='create':files[name]=b''
            elif op=='write':
                parts=value.split(':');count=int(parts[1] if parts[0]=='ERR_PARTIAL' else parts[0]);raw=item['_payload'][:count]
                old=files[name];files[name]=old[:offset]+bytes(max(0,offset-len(old)))+raw+old[offset+count:]
            elif op=='publish':files['DWB']=files.pop(name)
    return reads


def effect_calls(before, oracle, mode, fault, context="plain"):
    """F05--F09 independently authored complete new-site call order.

    PREINSPECT's previously verified calls precede Inventory and are excluded.
    Canonical source reads are included even though they do not use RecoveryCall.
    """
    calls=[];files=dict(before)
    def add(p,k,n,at=0,size=0):calls.append(row(p,k,n,at,size))
    def opened(p,n,write=False):
        add(p,'OpenWrite' if write else 'OpenRead',n);add(p,'Metadata',n)
    def read(p,n,at,size):add(p,'Read',n,at,size)
    def home(p,index,write=None):
        opened(p,model.DATA,write is not None)
        if write is None:read(p,model.DATA,index*16384,16384)
        else:
            if write:add(p,'Write',model.DATA,index*16384,16384)
            add(p,'FileSync',model.DATA)
    def selected(p):
        opened(p,'DWB');read(p,'DWB',0,4096)
        s=files['DWB'];name=f'd3-dwb-{s[81]}';opened(p,name)
        read(p,name,0,248 if files[name][6]==1 else 160)
    def select(p,g,b,image=None):
        for name in ('d3-dwb-stage',f'd3-dwb-{(g-1)%2}'):
            opened(p,name,True);add(p,'Write',name,0,16384);add(p,'FileSync',name)
        name='d3-dwb-select-stage';add(p,'Metadata',name)
        if name not in files:add(p,'Create',name)
        opened(p,name,True);add(p,'Write',name,0,4096);add(p,'FileSync',name);add(p,'Metadata',name)
        add(p,'Publish','DWB',0,4096);add(p,'DirectorySync','.')
        model.selections(files,g,b,image,8)
    existing=[len(files[n]) if n in files else None for n in model.SLOTS]
    calls.extend(boot_calls(existing))
    calls.extend(SOURCE_CALLS)
    s=files['DWB'];b=model.number(files[f'd3-dwb-{s[81]}'],40)
    if files[f'd3-dwb-{s[81]}'][6]==1:
        p='PendingValidate';selected(p);opened(p,'d3-dwb-images');read(p,'d3-dwb-images',0,16544)
        image=files['d3-dwb-images'][128:128+16384];index=model.number(image,32)
        # Invalid selected slot rejects before opening the home.
        if context=='slot_corrupt':return calls
        home(p,index)
        if context=='future_pending':return calls
        home('PendingHome',index,files[model.DATA][index*16384:(index+1)*16384]!=image)
        model.put_home(files,image);select('Retirement',model.number(s,32)+1,b)
    home('Redo',0)
    if context=='a_torn':return calls
    if oracle['status'] is not None:home('Redo',1)
    # Header + bounded frame body, separately for every physical source frame.
    wal=files[model.WAL];at=4096
    while at<len(wal):
        available=len(wal)-at
        if available<160:
            read('Redo',model.WAL,at,available);break
        read('Redo',model.WAL,at,160);size=int.from_bytes(wal[at+12:at+16],'little')
        if size>available:break
        read('Redo',model.WAL,at+160,size-160);at+=size
    add('Flush','Metadata',model.WAL)
    for index,image in enumerate((oracle['allocator'],oracle['status'])):
        if image is None or files[model.DATA][index*16384:(index+1)*16384]==image:continue
        b+=1;select('Flush',3*b-1,b)
        opened('Flush','d3-dwb-images',True);add('Flush','Write','d3-dwb-images',0,16544);add('Flush','FileSync','d3-dwb-images')
        select('Flush',3*b,b,image);home('Flush',index,True);model.put_home(files,image);select('Flush',3*b+1,b)
    home('Verify',0)
    if oracle['status'] is not None:home('Verify',1)
    selected('Verify');add('Snapshot','Metadata',model.WAL)
    # Prefix truncation is authored by the failing syscall, never a success log.
    targets={
        'boot0_partial':('BootstrapProvision','Write',model.SLOTS[0],0),
        'boot1_partial':('BootstrapProvision','Write',model.SLOTS[1],131072),
        'boot_sync_after':('BootstrapProvision','FileSync',model.SLOTS[1],0),
        'boot_dir_after':('BootstrapProvision','DirectorySync','.',0),
        'repair_partial':('PendingHome','Write',model.DATA,16384),
        'retirement_publish_after':('Retirement','Publish','DWB',0),
        'redo_second_partial':('Flush','Write',model.DATA,16384),
    }
    if fault in targets:
        target=targets[fault]
        end=next(i for i,c in enumerate(calls) if (c['phase'],c['kind'],c['name'],c['offset'])==target)
        return calls[:end+1]
    return calls


def raw_call(call):
    kind=call['kind'];name=call['name'];length=call['length']
    op={'OpenRead':'open_meta','OpenWrite':'open_meta','Metadata':'metadata','Read':'read','Write':'write','Create':'create','FileSync':'sync','DirectorySync':'dirsync','Publish':'publish','Inventory':'list'}[kind]
    if kind in ('DirectorySync','Inventory'):name='namespace'
    if kind=='Publish':name='d3-dwb-select-stage';length=0
    return (op,name,call['offset'],length)


def check_call_order(rows, expected):
    actual=[(op,name,int(offset),int(length)) for op,name,offset,length,_ in rows]
    start=next((i for i,r in enumerate(actual) if r[0]=='list'),None)
    model.require(start is not None,'FAIL_INVENTORY_EVENT_MISSING')
    model.require(actual[start:]==[raw_call(c) for c in expected],'FAIL_LITERAL_FULL_PORT_CALL_ORDER')
    return len(expected)
