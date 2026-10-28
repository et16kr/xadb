#!/usr/bin/env python3
"""STATUS_LOCAL_ONLY literal states/pages. Product-free integer/SHA author.
Uses the preserved wire-only author's envelope recipe, never its state oracle.
All non-Status roles remain opaque invalid sentinels. Write-once artifacts.
"""
from pathlib import Path
import hashlib
import json
import runpy
import struct
ROOT = Path(__file__).resolve().parent
wire = runpy.run_path(str(ROOT.parent/'wire/author.py'))
put, sha, actor, undo, ref = (wire[k] for k in ('put','sha','actor','undo','ref'))

def home(state, head=None, count=0, target=None, next_ref=None, generation=2):
    b=bytearray(224);b[:4]=b'XSH1';put(b,4,1,2);put(b,6,32,2);put(b,8,count)
    c=bytearray(192);c[:4]=b'XTS1';put(c,4,1,2)
    if state=='free':put(c,48,generation)
    else:
        c[8:56]=actor();put(c,48,generation)
        c[6]={'active':1,'statement':2,'full':2,'commit':3,'abort':4}[state]
        if state in ('statement','full'): c[7]=1 if state=='statement' else 2
        if state=='commit':put(c,56,9)
        for bit,n in enumerate((head,target,next_ref)):
            if n is not None:c[64]|=1<<bit;c[72+40*bit:112+40*bit]=undo(n)
    b[32:]=c
    return b

def page(cell, lsn):
    b=bytearray(16384);b[:4]=b'XDP1';put(b,4,1,2);b[6]=9;b[8:24]=wire['DB'];b[24:48]=ref(9)
    put(b,48,2);put(b,56,1);put(b,64,lsn)
    for a,v in ((72,1),(74,132),(76,16128),(128,16128),(130,224)):put(b,a,v,2)
    b[16128:16352]=cell;b[16352:]=sha(b[:16352]);return b

def core(op, current=None, next_ref=None, target=None, statement=0, mutation=0):
    b=bytearray(256);put(b,0,op,2);put(b,4,256,4);put(b,8,statement);put(b,16,mutation)
    if current is not None:put(b,24,8)
    for bit,n in enumerate((current,next_ref,target)):
        if n is not None:b[2]|=1<<bit;b[32+40*bit:72+40*bit]=undo(n)
    if op in (2,4,20):b[2]|=24;b[176:208]=sha(b'STATUS_LOCAL_ONLY before');b[208:240]=sha(b'STATUS_LOCAL_ONLY after')
    if op==6:put(b,152,9)
    return b

def frame(op, before, after, c):
    f=wire['frame'](op,185 if op in (2,4,20) else 0,int(c[2]&2!=0) if op in (2,4,20) else 0)
    ca=3488 if f[7]==3 else 1888;f[ca:ca+256]=c
    na=ca+256;count=int.from_bytes(f[20:22],'little');ra=len(f)-32-int.from_bytes(f[400:404] if f[7]==3 else f[312:316],'little')
    for _ in range(count):
        length=int.from_bytes(f[na+20:na+24],'little');rlen=int.from_bytes(f[ra+4:ra+8],'little')
        if f[na]==5:
            f[na+24:na+56]=sha(before);f[na+56:na+88]=sha(after);f[na+128:na+352]=before;f[na+352:na+576]=after
            off=16128 if op in (3,5) else 72;f[ra+64:ra+rlen]=page(after,101)[off:16352]
        na+=length;ra+=rlen
    f[-32:]=sha(f[:-32]);return f

# Independently specified source/destination states, not computed by a transition function.
A=lambda h=None,n=0:home('active',h,n)
S=lambda h,n,m:home('statement',h,n,m,h)
F=lambda h=None,n=0:home('full',h,n,next_ref=h)
Q=lambda h=None,n=0:home('commit',h,n)
rows=[]
def case(name,op,b,a,current=None,next_ref=None,target=None,stmt=0,mutation=0,external_stmt=0,mark=None,q=0):
    rows.append((name,op,b,a,core(op,current,next_ref,target,stmt,mutation),external_stmt,mark,q))
case('begin',1,home('free',generation=1),A())
case('forward-empty',2,A(),A(14,1),14,stmt=3,mutation=7,external_stmt=3)
case('forward-retained',2,A(13,2),A(14,3),14,13,stmt=3,mutation=7,external_stmt=3,mark=13)
case('rollback-full-empty',3,A(),F())
case('rollback-full-head',3,A(14,3),F(14,3),14,14)
case('rollback-statement-none',3,A(14,2),S(14,2,None),14,14,stmt=3,external_stmt=3)
case('rollback-statement-mark',3,A(14,3),S(14,3,12),14,14,12,stmt=3,external_stmt=3,mark=12)
case('rollback-statement-equal',3,A(14,3),S(14,3,14),14,14,14,stmt=3,external_stmt=3,mark=14)
case('rollback-statement-empty',3,A(),S(None,0,None),stmt=3,external_stmt=3)
case('escalate-statement-full',3,S(14,3,12),F(14,3),14,14)
case('clr-full-retained',4,F(14,3),F(13,2),14,13,stmt=2,mutation=5)
case('clr-full-last',4,F(14,1),F(),14,stmt=1,mutation=1)
case('clr-statement-retained',4,S(14,3,12),S(13,2,12),14,13,12,stmt=3,mutation=7,external_stmt=3,mark=12)
case('clr-statement-last-mark',4,S(14,3,13),S(13,2,13),14,13,13,stmt=3,mutation=7,external_stmt=3,mark=13)
case('clr-statement-last-empty',4,S(14,1,None),S(None,0,None),14,stmt=3,mutation=7,external_stmt=3)
case('finish-empty',5,S(None,0,None),A(),stmt=3,external_stmt=3)
case('finish-retained',5,S(14,3,14),A(14,3),14,14,14,stmt=3,external_stmt=3,mark=14)
case('commit-empty',6,A(),Q(),q=9)
case('commit-retained',6,A(14,3),Q(14,3),q=9)
case('abort-end',7,F(),home('abort'))
case('prune-retained',20,Q(14,3),Q(13,2),14,13,q=9)
case('prune-last',20,Q(14,1),Q(),14,q=9)
case('terminal-commit',17,Q(),home('free'))
case('terminal-abort',17,home('abort'),home('free'))

def main():
    assert not (ROOT/'cases.bin').exists(),'write-once'
    out=bytearray();index=[]
    for name,op,b,a,c,stmt,mark,q in rows:
        f=frame(op,b,a,c);record=f+page(b,100)+page(a,101)
        index.append('\t'.join(map(str,(name,len(out),len(f),stmt,mark or 0,q,sha(record).hex()))));out+=record
    (ROOT/'cases.bin').write_bytes(out);(ROOT/'index.tsv').write_text('\n'.join(index)+'\n')
    (ROOT/'input-plan.json').write_text(json.dumps({'scope':'STATUS_LOCAL_ONLY; other frame roles are semantic-invalid sentinels', 'literal_cases':len(rows),'bytes':len(out),'state_oracle':'explicit independent before/after pairs; no product encoder or transition call','framing_recipe':'../wire/author.py (preserved source; no execution of its main)','framing_recipe_sha256':sha((ROOT.parent/'wire/author.py').read_bytes()).hex()},indent=2)+'\n')
if __name__=='__main__':main()
