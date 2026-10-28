#!/usr/bin/env python3
"""Independent scope3 WIRE_ONLY literals; integers/SHA, no product imports.
Descriptor/native/auxiliary bodies are explicit sentinels, not semantic state.
Output is write-once. No accepted history, original source, grant or durability.
"""
from pathlib import Path
import hashlib
import json
import struct

ROOT = Path(__file__).resolve().parent
DB = bytes(range(1, 17))
def sha(b): return hashlib.sha256(b).digest()
def put(b, a, v, w=8): struct.pack_into({2: '<H', 4: '<I', 8: '<Q'}[w], b, a, v)
def ref(n): return struct.pack('<QQQ', 0x100000001, n, 2)
def undo(n): return ref(n) + struct.pack('<H6xQ', 0, 2)
def actor(): return DB + struct.pack('<QI4xQQ', 1, 1, 1, 2)
def owner(system=False):
    value = DB + struct.pack('<QB7xQQ', 1, 3, 1, 1) if system else actor()
    return bytes([2 if system else 1]) + bytes(7) + value

def core(op, h, mode):
    b = bytearray(256); put(b, 0, op, 2); put(b, 4, 256, 4)
    flags = 0
    if op in (2, 4, 11, 12): put(b, 8, 3); put(b, 16, 7)
    current = op in (2, 4, 11, 12, 20, 21, 22)
    next_ref = 13 if op in (2, 4, 20) and h else None
    target = None
    if op == 3:
        current = mode != 'empty'
        next_ref = 14 if current else None
        if mode.startswith('statement'): put(b, 8, 3)
        if mode == 'statement-mark': target = 12
        if mode == 'statement-equal': target = 14
    if op == 5:
        put(b, 8, 3); current = mode == 'mark'; next_ref = 14 if current else None; target = next_ref
    if op == 4 and mode == 'target': target = 12
    if op == 4 and mode == 'last': target = 13
    if current: flags |= 1; b[32:72] = undo(14); put(b, 24, 8)
    if next_ref: flags |= 2; b[72:112] = undo(next_ref)
    if target: flags |= 4; b[112:152] = undo(target)
    if op in (2, 4, 20): flags |= 24; b[176:208] = sha(b'WIRE_ONLY before'); b[208:240] = sha(b'WIRE_ONLY after')
    if op == 6: put(b, 152, 9)
    if op in (17, 20, 21, 22): put(b, 160, 62); put(b, 168, 63)
    put(b, 2, flags, 2)
    return b

# page ordinal, page kind, object, native kind, fixed length, slot
ROLES = {'root': (1, 12, 4, 8, 512, 0), 'job': (2, 13, 4, 9, 1024, 1),
         'status': (9, 9, 2, 5, 224, 0), 'undo': (14, 8, 3, 3, 432, 0),
         'row': (20, 1, 6, 1, 0, 1)}
NAMES = {1: ('root','job','status'), 2: tuple(ROLES), 3: ('status',),
         4: tuple(ROLES), 5: ('status',), 6: ('root','job','status'),
         7: ('root','job','status'), 11: ('root','job','undo'), 12: ('root','job'),
         16: ('root',), 17: ('root','job','status'), 20: tuple(ROLES),
         21: ('root','job','undo'), 22: ('root','job','undo')}

def bodies(op, c):
    natives, redos = [], []
    for i, name in enumerate(NAMES[op]):
        number, page_kind, obj, kind, size, slot = ROLES[name]
        size = c if name == 'row' else size
        before = bytes([64 + i]) * size
        after = bytes([96 + i]) * size
        if name == 'row' and op == 2: before = None
        if name == 'row' and op == 4: after = None
        b = bytearray(128); b[0] = kind; b[1] = int(before is not None) + 2 * int(after is not None); put(b, 2, i, 2)
        for value, s, o, length, digest in [(before,4,8,12,24),(after,6,10,16,56)]:
            put(b, s, slot if value is not None else 65535, 2)
            if value is not None:
                put(b, o, 16352-size*(slot+1), 2); put(b, length, size, 4); b[digest:digest+32] = sha(value)
        b += (before or b'') + (after or b''); put(b, 20, len(b), 4); natives.append(b)
        # The redo span is opaque here, deliberately not a semantic after page.
        length = 224 if op in (3, 5) else 16280
        r = bytearray(64); r[0] = 1; r[1] = page_kind; put(r, 4, 64+length, 4)
        r[8:32] = ref(number)
        if number < 9: put(r, 24, 1)
        put(r, 32, obj); put(r, 40, 1); put(r, 48, 100)
        put(r, 56, 16128 if op in (3, 5) else 72, 2); put(r, 58, length, 2)
        r += bytes([128+i])*length; redos.append(r)
    return b''.join(natives), b''.join(redos), len(natives)

def frame(op, c=0, h=0, mode='empty'):
    handoff = op in (1,2,4,6,7,11,12)
    g = bytearray(256 if handoff else 192); g[:4] = b'XHG1' if handoff else b'XSG1'
    for a,v in [(4,2),(6,op),(8,len(g)),(10,3)]: put(g,a,v,2)
    put(g,16,101)
    prior_tx = 0 if op == 1 else 100
    native, redo, count = bodies(op,c)
    e,k,i = (3+h,2,int(op==2)) if op in (2,4,20) else (0,0,int(op in (11,12)))
    descriptors = bytes([209])*1536
    if handoff:
        put(g,24,prior_tx); put(g,32,100); g[40:96] = owner(); g[96:152] = owner(True)
        if prior_tx: g[152:184] = sha(b'WIRE_ONLY prior Tx')
        g[184:216] = sha(b'WIRE_ONLY prior Aging')
        for a,v in [(216,1536),(220,1536),(224,256),(228,len(native)),(232,128*e+96*k+208*i),(240,len(redo))]: put(g,a,v,4)
        for a,v in [(236,count),(238,count),(244,e),(246,k),(248,i)]: put(g,a,v,2)
        descriptors += bytes([210])*1536
    else:
        prior = 0 if op == 16 else 100; put(g,24,prior); g[32:88] = owner(op not in (3,5))
        if prior: g[88:120] = sha(b'WIRE_ONLY prior owner')
        for a,v in [(120,1536),(136,len(native)),(140,128*e),(144,96*k),(148,208*i),(152,len(redo))]: put(g,a,v,4)
        for a,v in [(124,count),(126,e),(128,k),(130,i),(132,count)]: put(g,a,v,2)
    g += descriptors + core(op,h,mode) + native + bytes([225])*(128*e) + bytes([226])*(96*k) + bytes([227])*(208*i) + redo
    put(g,12,len(g),4)
    b=bytearray(160); b[:4]=b'XWF1';put(b,4,1,2)
    b[6]={1:1,2:2,3:2,4:4,5:2,6:5,7:6,11:2,12:2}.get(op,3);b[7]=3 if handoff else 2
    put(b,8,160,2);put(b,12,len(g)+192,4);put(b,16,len(g),4);put(b,20,count,2)
    b[24:40]=DB
    for a,v in [(40,1),(56,101),(64,100)]:put(b,a,v)
    put(b,48,1,4)
    if b[6]!=3:b[72:112]=actor()[:40];put(b,112,prior_tx)
    b[128:160]=sha(b[:128]); b+=g; b+=sha(b)
    return b

def main():
    assert not (ROOT/'frames.bin').exists(), 'write-once fixture'
    cases=[(f'op-{op}',op,185 if op in (2,4,20) else 0,0,'empty') for op in NAMES]
    cases += [(f'row-{op}-{c}-{h}',op,c,h,'empty') for op in (2,4,20) for c,h in [(185,1),(2228,0),(2228,1)]]
    cases += [(f'rollback-{m}',3,0,0,m) for m in ('full-head','statement-none','statement-mark','statement-equal')]
    cases += [('finish-mark',5,0,0,'mark'),('clr-target',4,185,1,'target'),('clr-last',4,185,1,'last')]
    out=bytearray(); index=[]
    for name,op,c,h,mode in cases:
        raw=frame(op,c,h,mode); index.append(f'{name}\t{len(out)}\t{len(raw)}\t{op}\t{c}\t{h}\t{sha(raw).hex()}');out+=raw
    assert len(out)<4*1024*1024
    (ROOT/'frames.bin').write_bytes(out);(ROOT/'index.tsv').write_text('\n'.join(index)+'\n')
    (ROOT/'input-plan.json').write_text(json.dumps({'scope':'WIRE_ONLY opaque body sentinels; not semantic history or current recovery', 'literal_cases':len(cases), 'raw_bytes':len(out), 'author':'independent integers and SHA; no product encoder or other author import'},indent=2)+'\n')
if __name__=='__main__':main()
