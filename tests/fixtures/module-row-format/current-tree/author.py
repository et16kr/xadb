"""Independent post-split/unlink Base4 observations; no product encoder or authority.

The pinned live parent contains rows1/2/3, of which2/3 are Retained INSERTs.
RootSplit70 moves row3 to16 and installs internal root17. ChildSplit80 then
moves row2 to18. Original Job leaf hints and CREATE claims10/11 stay unchanged.
These raw snapshots prove current-state checks, not selected WAL history.
"""
from pathlib import Path
from hashlib import sha256
from struct import pack_into, unpack_from

ROOT = Path(__file__).resolve().parent
P = 16384
ARENA, CERTS, CLAIMS, BANK, INVENTORY = 262144, 65536, 73728, 4096, 98304
SECTIONS = ((BANK, 36 * 1536), (CERTS, 64 * 96), (CLAIMS, 64 * 352),
            (INVENTORY, P), (ARENA, 64 * P))

def digest(b): return sha256(b).digest()
def get(b, at, width=8): return int.from_bytes(b[at:at + width], 'little')
def put(b, at, value, width=8): pack_into({2:'<H', 4:'<I', 8:'<Q'}[width], b, at, value)
def seal(b): b[-32:] = digest(b[:-32]); return b
def pref(n): return (0x100000001).to_bytes(8, 'little') + n.to_bytes(8, 'little') + (2).to_bytes(8, 'little')
def image(b, n): return bytearray(b[ARENA+n*P:ARENA+(n+1)*P])
def cells(p): return [bytes(p[get(p,128+4*i,2):get(p,128+4*i,2)+get(p,130+4*i,2)]) for i in range(get(p,72,2))]

def page(template, n, kind, level, lsn, values):
    p=bytearray(P); p[:72]=template[:72]; p[6]=kind; p[7]=level; p[24:48]=pref(n)
    put(p,64,lsn); put(p,72,len(values),2); put(p,74,128+4*len(values),2)
    end=P-32
    for i,v in enumerate(values):
        end-=len(v); p[end:end+len(v)]=v; put(p,128+4*i,end,2); put(p,130+4*i,len(v),2)
    put(p,76,end,2)
    return seal(p)

def install(b,n,p):
    b[ARENA+n*P:ARENA+(n+1)*P]=p
    c=bytearray(96); c[0]=1; c[1]=p[6]; c[2]=p[7]; c[8:56]=p[24:72]; c[56:88]=digest(p)
    b[CERTS+n*96:CERTS+(n+1)*96]=c

def claim(initial,role,lsn,operation,peer,before_revision):
    c=bytearray(352); c[:5]=bytes((1,role,initial[6],initial[7],1))
    c[8:48]=initial[24:64]; put(c,48,lsn); put(c,56,1,4)
    c[96:128]=digest(initial); c[64:96]=digest(b'frame'+lsn.to_bytes(8,'little'))
    c[128:144]=initial[8:24]; put(c,144,1); c[152]=2; put(c,160,lsn); put(c,168,1)
    for at,v in ((176,lsn),(184,6),(192,1),(200,1),(280,operation),(288,before_revision),(296,before_revision+1)):
        put(c,at,v)
    c[208:232]=pref(10); c[232:256]=pref(11)
    if peer is not None: c[256:280]=pref(peer)
    c[304:336]=digest(b'core'+lsn.to_bytes(8,'little'))
    return c

def finish(b,lsn,operation):
    for at in (96,216,248): put(b,at,lsn)
    d=bytearray((ROOT.parent/'structural-bank'/('root.xod' if operation==1 else 'child.xod')).read_bytes())
    for at in (56,72,88,544): put(d,at,lsn)
    b[BANK+33*1536:BANK+34*1536]=seal(d)
    length=get(b,INVENTORY+12,4); put(b,INVENTORY+48,lsn)
    if get(b,INVENTORY+64):
        at=INVENTORY+2176; size=get(b,at+12,4); put(b,at+48,lsn)
        b[at+size-32:at+size]=digest(b[at:at+size-32])
        b[INVENTORY+88:INVENTORY+120]=digest(b[at:at+size])
    b[INVENTORY+length-32:INVENTORY+length]=digest(b[INVENTORY:INVENTORY+length-32])
    for i,(at,size) in enumerate(SECTIONS): b[1280+i*64+32:1280+(i+1)*64]=digest(b[at:at+size])
    return seal(b)

def allocator(b,allocated,lsn):
    a=image(b,0); at=get(a,128,2)+64
    for n,kind in allocated:
        entry=bytearray(96); entry[0]=2; entry[1]=kind
        for off,v in ((8,2),(16,6),(24,1)): put(entry,off,v)
        a[at+n*96:at+(n+1)*96]=entry
    put(a,64,lsn); install(b,0,seal(a))

def anchor(b,revision,lsn,root=17):
    old=image(b,10); c=bytearray(cells(old)[0]); c[7]=1; c[8:32]=pref(root); put(c,32,revision)
    install(b,10,page(old,10,11,0,lsn,[c]))

def internal(template,keys,children,lsn,root=17):
    meta=bytearray(32); meta[:6]=b'XBT1\x01\x00'; meta[8:32]=pref(children[0])
    values=[meta]+[b'\x08\x00\x00\x00'+k.to_bytes(8,'big')+pref(n) for k,n in zip(keys,children[1:])]
    return page(template,root,2,1,lsn,values)

def rebuild(b,n,values):
    old=image(b,n); p=page(old,n,old[6],old[7],get(old,64),values)
    p[24:48]=old[24:48]; install(b,n,seal(p))

def root_split(parent):
    b=bytearray(parent); original=image(b,11); rows=cells(original)
    free=[n for n in range(9,64) if b[CERTS+n*96]==0]
    right_no,root_no=free[:2]
    left=page(original,11,1,0,70,rows[:3]); right=page(original,right_no,1,0,70,[rows[0]]+rows[3:])
    key=int.from_bytes(rows[3][4:12],'big')
    root=internal(original,[key],[11,right_no],70,root_no)
    for n,p in ((11,left),(right_no,right),(root_no,root)): install(b,n,p)
    for n,p,role,peer in ((right_no,right,5,root_no),(root_no,root,6,right_no)):
        b[CLAIMS+n*352:CLAIMS+(n+1)*352]=claim(p,role,70,1,peer,1)
    allocator(b,[(right_no,1),(root_no,2)],70); anchor(b,2,70,root_no)
    return finish(b,70,1)

def variant(name):
    data=(ROOT.parent/'base4-closure/variants.bin').read_bytes(); at=0
    while at<len(data):
        base,size=data[at:at+2]; label=data[at+2:at+2+size].decode(); at+=2+size
        count=get(data,at,4); at+=4
        patches=[]
        for _ in range(count):
            off,size=get(data,at,4),get(data,at+4,4); at+=8
            patches.append((off,data[at:at+size])); at+=size
        if label==name:
            assert base==2
            b=bytearray((ROOT.parent/'base4-closure/live.xrb').read_bytes())
            for off,patch in patches: b[off:off+len(patch)]=patch
            return b
    raise ValueError(name)

def effect_state(parent):
    """Two real Retained RowValue U records: DELETE row20 (D1009) on
    left11; shrinking UPDATE row40 (D1300) on right16. Catalog column Binary4.
    Raw observations deliberately do not claim original selected WAL history.
    """
    b=bytearray(parent)
    catalog=[bytearray(c) for c in cells(image(b,8))]; catalog[1][-3]=4
    rebuild(b,8,catalog)
    leaf=[bytearray(c) for c in cells(image(b,11))]
    jobs=[bytearray(c) for n in range(2,7) for c in cells(image(b,n))]
    def canon(n): return (1).to_bytes(4,'little')+bytes([4])+n.to_bytes(4,'little')+b'x'*n
    def entry(row,record): return (8).to_bytes(2,'little')+len(record).to_bytes(2,'little')+row.to_bytes(8,'big')+record
    def record(old,value):
        r=bytearray(old[:168])+value; put(r,160,len(value),4); return r
    # The pre-existing committed row is also checked with the new same-cut schema.
    leaf[1]=entry(1,record(leaf[1][12:],canon(1)))
    for ordinal,(effect,n_old,n_new) in enumerate([(1,1000,0),(2,1500,200)],1):
        row=ordinal*20; old=canon(n_old); current=record(leaf[ordinal+1][12:],canon(n_new)); put(current,8,row)
        if effect==1:
            current=current[:168]; current[6]=3; current[7]=0; put(current,160,0,4)
        j=jobs[ordinal]; put(j,168,78+ordinal); put(j,320,row); c=180+len(old); new_c=12+len(current)
        put(j,280,c,4); j[284]=effect
        if effect==2: put(j,286,new_c,2)
        projection=bytearray(48); projection[:9]=b'XRP1\x01\x00\x01\x00\x01'
        for at,v in ((16,6),(24,1),(32,row)): put(projection,at,v)
        projection[40]=1; j[288:320]=digest(projection+current)
        wal=(311608+c if effect==1 else 311428+c+2*new_c)+128*(ordinal-1)
        put(j,784,wal); put(j,880,wal)
        u=bytearray(cells(image(b,11+ordinal))[0]); put(u,16,78+ordinal); put(u,112,row);
        if ordinal==2: put(u,160,79)
        u[216]=1; put(u,224,2)
        u[272:312]=leaf[1][12+72:12+112]; put(u,312,row)
        put(u,320,len(old),4); u[324]=2; u[325]=1
        rebuild(b,11+ordinal,[u+old]); leaf[ordinal+1]=entry(row,current)
    rebuild(b,11,leaf)
    # Sum the unchanged and new cleanup rows using the contract's sum/max rules.
    def add(rows):
        out=bytearray(640)
        for i in range(5):
            out[i*128]=i+1
            for at in range(8,120,8):
                values=[get(r,i*128+at) for r in rows]
                put(out,i*128+at,max(values) if 64<=at<112 else sum(values))
        return out
    aggregate=add([j[384:] for j in jobs])
    offset=0
    for n in range(2,7):
        count=len(cells(image(b,n))); rebuild(b,n,jobs[offset:offset+count]); offset+=count
    root=bytearray(cells(image(b,1))[0])
    # Complete current issuer observation: 80 Undo reservations, 3 Begins,
    # 50 canceled and 30 completed Jobs plus 3 current Jobs. 63 RowId attempts
    # and one CREATE fit the Undo bound; current U79/U80 follow the old rows.
    # Historical revisions/WAL are deliberately not reconstructed by this fixture.
    for at,value in ((88,83),(376,30),(384,50),(392,80),(432,63)): put(root,at,value)
    root[128:160]=digest(b''.join(jobs)); root[160:192]=digest(aggregate)
    rebuild(b,1,[root])
    tx=bytearray(b[BANK:BANK+1536]); rows=bytearray(tx[640:1280])
    clr=91096+2*(180+1009)+90916+2*(180+1509)+(180+209)+128
    put(rows,16,clr); put(rows,112,clr); tx[640:1280]=rows
    put(tx,1328,sum(get(rows,i*128+112) for i in range(5)))
    b[BANK:BANK+1536]=seal(tx)
    aging=bytearray(b[BANK+34*1536:BANK+35*1536]); aging[640:1280]=aggregate
    for k in range(6): put(aging,1280+k*8,max(get(aggregate,i*128+64+k*8) for i in range(5)))
    put(aging,1328,sum(get(aggregate,i*128+112) for i in range(5)))
    aging[416:448]=digest(image(b,1)); aging[448:480]=digest(root)
    b[BANK+34*1536:BANK+35*1536]=seal(aging)
    b[512:1152]=add([rows,aggregate])
    return b

def fill_leaf(parent,ordinal,reservation,extra=0):
    b=bytearray(parent); rows=cells(image(b,ordinal)); used=sum(len(c)+4 for c in rows[1:])
    # TREE_ENTRY_CAPACITY = 16384 - header128 - checksum32 - metadata32 - slot4.
    remaining=16188-reservation-used+extra; key=2 if ordinal==11 else 41
    template=cells(image(b,11))[1][12:]
    while remaining:
        cost=min(2232,remaining); assert cost>=193
        n=cost-193; canon=(1).to_bytes(4,'little')+bytes([4])+n.to_bytes(4,'little')+b'z'*n
        r=bytearray(template[:168])+canon; put(r,8,key); put(r,112,key); put(r,160,len(canon),4)
        rows.append((8).to_bytes(2,'little')+len(r).to_bytes(2,'little')+key.to_bytes(8,'big')+r)
        remaining-=cost; key+=1
    rows=[rows[0]]+sorted(rows[1:],key=lambda c:c[4:12]); rebuild(b,ordinal,rows)
    return finish(b,70,1)

def actor_state(parent,effect,state):
    """Layout/actor-only vector. ATT/inventory are intentionally not repaired;
    no full Base acceptance is claimed. Target row40 is on current leaf16.
    """
    b=bytearray(parent); jobs=[bytearray(c) for n in range(2,7) for c in cells(image(b,n))]
    j=jobs[2]; j[284]=effect
    if effect==1:
        j[286:288]=bytes(2); wal=311608+get(j,280,4)+128
        put(j,784,wal); put(j,880,wal)
    u=bytearray(cells(image(b,13))[0]); leaf=cells(image(b,16)); current=bytearray(leaf[1][12:])
    old=bytearray(current[:168])+u[432:]; old[6]=0; old[16]=1; old[24:72]=bytes(48); put(old,24,2)
    old[72:120]=u[272:320]; old[120:160]=bytes(40); put(old,160,len(u)-432,4)
    if state=='reserved':
        j[6]=1; put(j,64,71); put(j,72,0); put(j,376,71); j[328:360]=bytes(32)
        u=bytearray(432); u[:6]=b'XUR1\x01\x00'; put(u,8,2)
        value=old
    else:
        j[6]=3 if state=='committed' else 4; j[121]=1
        j[200:248]=bytes(48); j[280:284]=bytes(4); j[285:320]=bytes(35)
        put(j,264,3 if state=='committed' else 0); put(j,272,59 if state=='compensated' else 69); put(j,376,71)
        # Cursor1 has released the absent previous-version edge. Its three
        # remaining release groups retain the same Row execution peak.
        put(j,776,3); put(j,784,3*55528); put(j,880,3*55528)
        u=u[:432]; u[6]=2; put(u,80,0); u[120:168]=bytes(48)
        u[320:324]=bytes(4); u[325]=0; u[326]=2; u[327]=3; u[328]=1
        terminal=jobs[0]; terminal[6]=2; terminal[120]=1
        put(terminal,72,60); put(terminal,264,3); put(terminal,376,60)
        if state=='compensated': value=old
        elif effect==1: value=None
        else:
            value=current; value[6]=0; value[16]=1; value[24:72]=bytes(48); put(value,24,3); value[120:160]=bytes(40)
    def entry(value): return (8).to_bytes(2,'little')+len(value).to_bytes(2,'little')+(40).to_bytes(8,'big')+value
    rebuild(b,16,[leaf[0]]+([] if value is None else [entry(value)]))
    rebuild(b,13,[u]); p=image(b,13); put(p,64,71); install(b,13,seal(p))
    status=bytearray(cells(image(b,9))[0]); put(status,8,1); status[104:144]=jobs[1][128:168]
    if state!='reserved': status[38]=3; put(status,88,3)
    rebuild(b,9,[status]); p=image(b,9); put(p,64,71); install(b,9,seal(p))
    aggregate=bytearray(640)
    for i in range(5):
        aggregate[i*128]=i+1
        for at in range(8,120,8):
            values=[get(job,384+i*128+at) for job in jobs]
            put(aggregate,i*128+at,max(values) if 64<=at<112 else sum(values))
    offset=0
    for n in range(2,7):
        count=len(cells(image(b,n))); rebuild(b,n,jobs[offset:offset+count]); offset+=count
    p=image(b,2); put(p,64,71); install(b,2,seal(p))
    root=bytearray(cells(image(b,1))[0]); put(root,80,71)
    leased,ready=(5,2) if state=='reserved' else (0,7)
    put(root,104,leased); put(root,112,ready); put(b,272,leased); put(b,280,ready)
    if state!='reserved':
        put(root,408,3); put(b,256,0,4)
    root[128:160]=digest(b''.join(jobs)); root[160:192]=digest(aggregate)
    rebuild(b,1,[root]); p=image(b,1); put(p,64,71); install(b,1,seal(p))
    return finish(b,90,1)

def internal_root_split(parent):
    b=bytearray(parent); template=image(b,17); original=image(b,11); values=cells(original)
    leaf18=page(original,18,1,0,75,[values[0],values[2]])
    leaf20=page(original,20,1,0,76,[values[0]])
    leaf11=page(original,11,1,0,75,values[:2])
    # A valid internal page has at least two children. The empty fourth leaf
    # is a current-state observation, not a claimed split output or history.
    left=internal(template,[2],[11,18],80,17); right=internal(template,[4],[16,20],80,19)
    root=internal(template,[3],[17,19],80,21); root[7]=2; seal(root)
    for n,p in ((11,leaf11),(18,leaf18),(20,leaf20),(17,left),(19,right),(21,root)): install(b,n,p)
    for n,p,lsn,before,left_no in ((18,leaf18,75,2,11),(20,leaf20,76,3,16)):
        c=claim(p,5,lsn,2,None,before); c[232:256]=pref(left_no)
        b[CLAIMS+n*352:CLAIMS+(n+1)*352]=c
    for n,p,role,peer in ((19,right,5,21),(21,root,6,19)):
        c=claim(p,role,80,1,peer,4); c[232:256]=pref(17)
        b[CLAIMS+n*352:CLAIMS+(n+1)*352]=c
    allocator(b,[(18,1),(20,1),(19,2),(21,2)],80); anchor(b,5,80,21)
    a=image(b,10); at=get(a,128,2); a[at+7]=2; install(b,10,seal(a))
    return finish(b,80,1)

def finish_unlink(b,lsn,wal):
    """Bank33 = a Finished unlink owner: the child-split descriptor with its
    lsn fields and consumed WAL (offset608) replaced by the exact frame bytes."""
    b=finish(b,lsn,2); d=bytearray(b[BANK+33*1536:BANK+34*1536]); put(d,608,wal)
    b[BANK+33*1536:BANK+34*1536]=seal(d)
    for i,(at,size) in enumerate(SECTIONS): b[1280+i*64+32:1280+(i+1)*64]=digest(b[at:at+size])
    return seal(b)

def xtr1(entries):
    """(page, incarnation, kind, level, retire LSN) in physical order."""
    c=bytearray(16+40*len(entries)); c[:6]=b'XTR1\x01\x00'; put(c,6,len(entries),2)
    for i,(n,inc,kind,level,lsn) in enumerate(entries):
        e=16+40*i; c[e:e+16]=(0x100000001).to_bytes(8,'little')+n.to_bytes(8,'little')
        put(c,e+16,inc); c[e+24]=kind; c[e+25]=level; put(c,e+32,lsn)
    return bytes(c)

def listed_anchor(b,revision,lsn,root,level,entries):
    old=image(b,10); c=bytearray(cells(old)[0]); c[7]=level; c[8:32]=pref(root); put(c,32,revision)
    install(b,10,page(old,10,11,0,lsn,[c,xtr1(entries)]))

def leaf_unlink(child):
    """From child-split80 (root17 [2,3] -> 11,18,16): ChildSplit85 had added
    empty leaf19 right of16 (revision4); LeafUnlink90 removed it again. Root17
    is rewritten at90, 19 stays allocated and listed (revision5)."""
    b=bytearray(child); original=image(b,11); meta=cells(original)[0]
    leaf=page(original,19,1,0,85,[meta]); install(b,19,leaf)
    c=claim(leaf,5,85,2,None,3); c[232:256]=pref(16); b[CLAIMS+19*352:CLAIMS+20*352]=c
    install(b,17,internal(original,[2,3],[11,18,16],90))
    allocator(b,[(19,1)],85); listed_anchor(b,5,90,17,1,[(19,2,1,0,90)])
    return finish_unlink(b,90,35248)

def root_collapse(parent):
    """From the live leaf11 (rows1/2/3): RootSplit70 at separator1 left the
    CREATE leaf11 empty (right16 holds every row, root17 [1]); RootCollapse90
    made16 the root leaf and listed11 and17 (revision3)."""
    b=bytearray(parent); original=image(b,11); rows=cells(original)
    left=page(original,11,1,0,70,rows[:1]); right=page(original,16,1,0,70,rows)
    root=internal(original,[1],[11,16],70)
    for n,p in ((11,left),(16,right),(17,root)): install(b,n,p)
    for n,p,role,peer in ((16,right,5,17),(17,root,6,16)):
        b[CLAIMS+n*352:CLAIMS+(n+1)*352]=claim(p,role,70,1,peer,1)
    allocator(b,[(16,1),(17,2)],70)
    listed_anchor(b,3,90,16,0,[(11,2,1,0,90),(17,2,2,1,90)])
    return finish_unlink(b,90,19032)

PARENTS = {'base4-closure/variants.bin': '4cb06bcb8548f45075be8bde033eff3eca9e9b58e3beb1e124b49399b939fd88', 'base4-closure/live.xrb': '5a446fbf0fab629b7bfc10b5040e0a54b77ea6e2a993ca58db70482f3f5c94d9', 'structural-bank/root.xod': '55a03eeda6771aae4e226dc3615955f71a1509d0b595fa016754bacfe19cfda3', 'structural-bank/child.xod': 'b1de53f096a5c2be5bb891c04fded4da72966873000dc39a13edaf04e2880b73'}

def main():
    for name,sha in PARENTS.items():
        assert digest((ROOT.parent/name).read_bytes()).hex()==sha, name
    parent=(ROOT.parent/'base4-closure/live.xrb').read_bytes()
    b=bytearray(parent); original=image(b,11); rows=cells(original)
    left=page(original,11,1,0,70,rows[:3]); right=page(original,16,1,0,70,[rows[0],rows[3]])
    root=internal(original,[3],[11,16],70)
    for n,p in ((11,left),(16,right),(17,root)): install(b,n,p)
    for n,p,role,peer in ((16,right,5,17),(17,root,6,16)):
        b[CLAIMS+n*352:CLAIMS+(n+1)*352]=claim(p,role,70,1,peer,1)
    allocator(b,[(16,1),(17,2)],70); anchor(b,2,70); finish(b,70,1)
    (ROOT/'root-split.xrb').write_bytes(b)
    (ROOT/'internal-root-split.xrb').write_bytes(internal_root_split(b))
    left=page(original,11,1,0,80,rows[:2]); right=page(original,18,1,0,80,[rows[0],rows[2]])
    for n,p in ((11,left),(18,right),(17,internal(original,[2,3],[11,18,16],80))): install(b,n,p)
    b[CLAIMS+18*352:CLAIMS+19*352]=claim(right,5,80,2,None,2)
    allocator(b,[(18,1)],80); anchor(b,3,80); finish(b,80,2)
    (ROOT/'child-split.xrb').write_bytes(b)
    (ROOT/'unlink-leaf.xrb').write_bytes(leaf_unlink(b))
    (ROOT/'unlink-collapse.xrb').write_bytes(root_collapse(parent))
    for name in ['state-row-reserved','state-ready-writer','state-compensated-writer']:
        (ROOT/(name+'.xrb')).write_bytes(root_split(variant(name)))
    mixed=root_split(effect_state(parent)); (ROOT/'delete-update.xrb').write_bytes(mixed)
    for ordinal,reservation in [(11,1009),(16,1300)]:
        for extra in (0,1):
            (ROOT/f'delete-update-{ordinal}-{extra}.xrb').write_bytes(fill_leaf(mixed,ordinal,reservation,extra))
    for effect in (1,2):
        for state in ('reserved','committed','compensated'):
            (ROOT/f'actor-{effect}-{state}.xrb').write_bytes(actor_state(mixed,effect,state))
    inputs=[ROOT.parent/'base4-closure/variants.bin', ROOT.parent/'base4-closure/live.xrb', ROOT.parent/'structural-bank/root.xod', ROOT.parent/'structural-bank/child.xod']
    (ROOT/'parents.sha256').write_text(''.join(digest(p.read_bytes()).hex()+'  '+str(p.relative_to(ROOT.parent))+'\n' for p in inputs))
    (ROOT/'inputs.sha256').write_text(''.join(digest(p.read_bytes()).hex()+'  '+p.name+'\n' for p in sorted(ROOT.glob('*.xrb'))))

if __name__=='__main__': main()
