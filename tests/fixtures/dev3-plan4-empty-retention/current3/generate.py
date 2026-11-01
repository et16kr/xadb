#!/usr/bin/env python3
"""F0 current Profile3 empty lifecycle literals; independent explicit fields.

Imports only hash-pinned Python fixture serializers, never product code or
observed product output. Historical p1-o00 remains byte-for-byte unchanged.
"""
import hashlib
import importlib.util
from pathlib import Path
import struct
import sys
sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
SOURCES = {
    'tests/fixtures/module-row-format/catalog3/serializer3.py': '0b23d146ef7fa5a1ba712baee37c60c882011f36832ef11b5f1d2b49dea70de1',
    'tests/fixtures/dev3-plan4-free-undo/generate_literals.py': 'a9a646f5fd047b1fbd73d743d99d56cde9a8217bf65587d08e9dd1f892e6bb42',
}
def load(path, expected):
    source = REPO / path
    assert hashlib.sha256(source.read_bytes()).hexdigest() == expected
    spec = importlib.util.spec_from_file_location('f0_' + source.parent.name, source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def generate():
    t = load(next(iter(SOURCES)), SOURCES[next(iter(SOURCES))]).load()
    system_path = 'tests/fixtures/dev3-plan4-free-undo/generate_literals.py'
    a = load(system_path, SOURCES[system_path])
    profile = t.profile(); initial = t.fresh(profile); files = {}
    def emit(name, raw):
        assert name not in files
        files[name] = bytes(raw)
    emit('references/REF-P1-d3-profile.bin', profile)
    for n in range(9):
        if n == 0: raw = initial['images'][0]
        elif n == 1: raw = initial['images'][1]
        elif 2 <= n <= 6: raw = t.page(n,13,4,0,initial['jobs'][(n-2)*15:(n-1)*15])
        elif n == 7: raw = initial['anchor']
        else: raw = initial['images'][3]
        emit(f'references/P{n:02}.bin', raw)
    reserve, allocator, desc, _ = a.operation(profile,initial['images'][0],9,2,1,1,'reserve',1)
    emit('raw/reserve.wal',reserve);emit('raw/reserve.page',allocator);emit('raw/reserve.descriptor',desc)
    for terminal in ['cancel','status']:
        wire,page,descriptor,payload=a.operation(profile,allocator,9,2,1,1,terminal,1,old=desc)
        emit(f'raw/{terminal}.wal',wire);emit(f'raw/{terminal}.page',page);emit(f'raw/{terminal}.descriptor',descriptor)
        if payload: emit('raw/status-free.page',payload)
        if terminal=='status':
            initial['images'][0]=page;initial['images'][4]=payload
    initial['entries'][9]=(2,2,9,2,b'');initial['last_lsn']=2
    p=t.plan(1,canceled=True); state=initial
    phases=[(16,'aging','aging-start'),(1,'begin','begin'),(3,'emptyrollback','rollback'),(7,'abort','abort'),(17,'finish','finish')]
    states={}
    for lsn,(op,phase,name) in enumerate(phases,3):
        plan={} if op==16 else p
        after,ordinal=t.successor(state,plan,op,phase,lsn)
        wire,_=t.frame(state,after,plan,op,ordinal)
        emit(f'raw/{name}.wal',wire)
        for role,label in [(1,'root'),(2,'jobs'),(4,'status')]:
            label_phase='start' if op==16 else name
            if op==16 and role!=1:continue
            if op==3 and role in (1,2):continue
            emit(f'raw/{label}-{label_phase}.page',after['images'][role])
        if op!=3:emit(f'raw/aging-{("start" if op==16 else name)}.descriptor',after['aging'])
        if op in (1,3,7):emit(f'raw/tx-{name}.descriptor',after['tx'])
        state=after;states[lsn]=state
    # Epoch2's new local1 reuses Status generation3; canonical Job selection
    # advances from cursor1 to Job ordinal1 (its first use, generation2).
    state=__import__('copy').deepcopy(state);state['tx']=None
    t.actor=lambda local=1,generation=2:t.DB+struct.pack('<QI4xQQ',1,2,local,generation)
    p=t.plan(1,canceled=True);p.update(generation=3,terminal=1,terminal_gen=2,root=2)
    for lsn,(op,phase,name) in enumerate(phases[1:],8):
        after,ordinal=t.successor(state,p,op,phase,lsn)
        wire,_=t.frame(state,after,p,op,ordinal)
        wire=bytearray(wire);struct.pack_into('<I',wire,48,2);wire[128:160]=t.sha(wire[:128]);wire=t.seal(wire)
        emit(f'raw/second-{name}.wal',wire)
        for role,label in [(1,'root'),(2,'jobs'),(4,'status')]:
            if op==3 and role in (1,2):continue
            emit(f'raw/second-{label}-{name}.page',after['images'][role])
        if op!=3:emit(f'raw/second-aging-{name}.descriptor',after['aging'])
        if op in (1,3,7):emit(f'raw/second-tx-{name}.descriptor',after['tx'])
        state=after
    return files

def free_undo(lifecycle):
    path = 'tests/fixtures/dev3-plan4-free-undo/generate_literals.py'
    a = load(path, SOURCES[path]); files = {}
    profile = lifecycle['references/REF-P1-d3-profile.bin']
    initial = lifecycle['references/P00.bin']
    files['reference/profile.bin'] = profile
    for n in range(9): files[f'reference/P{n:02}.bin'] = lifecycle[f'references/P{n:02}.bin']
    for lsn, name in enumerate(['aging-start','begin','rollback','abort','finish'],3):
        files[f'reference/memory-{lsn:02}.wal'] = lifecycle[f'raw/{name}.wal']
    for role in ['root','jobs','status']:
        files[f'reference/memory-{role}-finish.page'] = lifecycle[f'raw/{role}-finish.page']
    files['reference/memory-aging-finish.descriptor'] = lifecycle['raw/aging-finish.descriptor']
    frames = {}; before = initial
    for work, kind in enumerate(['status','undo','undo','status','undo','status','undo'],1):
        number, birth = 8+work,2*work-1
        epoch = 1 if work<=3 else 2 if work<=5 else 3
        first,reserved,desc,_=a.operation(profile,before,number,2,work,birth,'reserve',epoch)
        second,before,_,payload=a.operation(profile,reserved,number,2,work,birth,kind,epoch,desc)
        frames[birth],frames[birth+1]=first,second
        files[f'native-page{number}-init.page']=payload
        files[f'native-allocator-l{birth+1}.page']=before
    for epoch,lsns in [(1,range(1,7)),(2,range(7,11)),(3,range(11,15))]:
        files[f'native-wal-e{epoch}-suffix.bin']=b''.join(frames[n] for n in lsns)
    _,reserved,desc,_=a.operation(profile,initial,9,2,1,1,'reserve',1)
    for terminal in ['undo','cancel']:
        files[f'pure/{terminal}-02.wal']=a.operation(profile,reserved,9,2,1,1,terminal,1,desc)[0]
    before=lifecycle['raw/status.page']
    for work,number,incarnation,birth,terminal in [(2,10,2,8,'undo'),(3,11,2,10,'cancel'),(4,11,3,12,'undo')]:
        first,reserved,desc,_=a.operation(profile,before,number,incarnation,work,birth,'reserve',1)
        second,before,_,_=a.operation(profile,reserved,number,incarnation,work,birth,terminal,1,desc)
        files[f'memory/{birth:02}.wal']=first;files[f'memory/{birth+1:02}.wal']=second
    cancel,page=a.reserved_recovery(profile,initial)
    files['reserved-cancel-l6.wal']=cancel;files['reserved-cancel-l6.page']=page
    return files

def write_files(root, files):
    for name,raw in sorted(files.items()):
        path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
        if path.exists():assert path.read_bytes()==raw, f'non-reproducible {name}'
        else:path.write_bytes(raw)
    index='path\tbytes\tsha256\n'+''.join(f'{name}\t{len(raw)}\t{hashlib.sha256(raw).hexdigest()}\n' for name,raw in sorted(files.items()))
    (root/'raw-index.tsv').write_text(index)
    print(root.name,len(files),sum(map(len,files.values())),hashlib.sha256(index.encode()).hexdigest())

def main():
    files=generate()
    write_files(HERE,files)
    write_files(HERE/'free-undo',free_undo(files))
if __name__=='__main__':main()
