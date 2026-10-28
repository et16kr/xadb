#!/usr/bin/env python3
"""UNDO_LOCAL_ONLY raw expected pairs. No product imports/encoders.
Reuses preserved integer framing/Job/Status recipes; explicit state table below
is independent from the Rust transition. Other roles remain opaque sentinels.
"""
from pathlib import Path
import json,runpy,hashlib,struct
ROOT=Path(__file__).resolve().parent
wire=runpy.run_path(str(ROOT.parent/'wire/author.py'))
ledger=runpy.run_path(str(ROOT.parent/'ledger/author.py'))
status=runpy.run_path(str(ROOT.parent/'status/author.py'))
put,sha,ref,undo,actor=(wire[k] for k in ('put','sha','ref','undo','actor'))
P=16384

def free(g):
 b=bytearray(432);b[:6]=b'XUR1\x01\0';put(b,8,g);return b

def material(state,seq=3,*,table=6,previous=0,cursor=0,cut=False):
 b=free(2);b[6]=state;put(b,16,seq);b[24:72]=actor()
 for a,v in ((72,seq),(80,2 if state==1 else 0),(96,table),(104,1),(112,seq)):put(b,a,v)
 b[88]=1
 if state==1 and previous:b[326]=1;b[120:160]=undo(11+previous);put(b,160,previous)
 if state==2:b[326]=3 if cursor>=2 else 2 if cut else 0;b[327]=3 if cursor>=1 else 0;b[328]=cursor
 return b

def page(n,cell,lsn):
 b=bytearray(P);b[:6]=b'XDP1\x01\0';b[6]=8;b[8:24]=wire['DB'];b[24:48]=ref(n)
 for a,v in ((48,3),(56,1),(64,lsn)):put(b,a,v)
 for a,v in ((72,1),(74,132),(76,15920),(128,15920),(130,432)):put(b,a,v,2)
 b[15920:16352]=cell;b[-32:]=sha(b[:-32]);return b

def job(kind,seq,*,h=False,cursor=0,table=6):
 j=ledger['job'](ledger['root'](kind,seq,h=int(h),cursor=cursor))
 put(j,176,table)
 if table!=6:j[200:224]=ref(16 if table==7 else 18);j[224:248]=ref(17 if table==7 else 19)
 return j

# name, op, h levels, old/new cell recipe, original selected Job kind/cursor,
# statement rollback flag, planned generation. No computed expected transition.
ROWS=[
 ('reserve',11,0,'free1','free2','free',0,False,2),
 ('reserve-max',11,0,'free-max-1','free-max','free',0,False,2**64-1),
 ('forward-none',2,0,'free2','live','reserved',0,False,2),
 ('forward-other-table',2,1,'free2','live','reserved',0,False,2),
 ('forward-two-predecessors',2,2,'free2','live','reserved',0,False,2),
 ('clr-full-none',4,0,'live','releasing','retained',0,False,2),
 ('clr-full-other-table',4,1,'live','releasing','retained',0,False,2),
 ('clr-statement-last-mark',4,1,'live','releasing','retained',0,True,2),
 ('prune-none',20,0,'live','releasing','retained',0,False,2),
 ('prune-other-table',20,1,'live','releasing','retained',0,False,2),
 ('release0-never',21,0,'release0','release1','compensated',0,False,2),
 ('release0-cut',21,1,'release0','release1','committed',0,False,2),
 ('release1-never',21,0,'release1','release2','compensated',1,False,2),
 ('release1-cut',21,1,'release1','release2','committed',1,False,2),
 ('release2',21,0,'release2','release3','compensated',2,False,2),
 ('rootfree-compensated',22,0,'release3','free2','compensated',3,False,2),
 ('rootfree-committed',22,0,'release3','free2','committed',3,False,2),
]
def cell(recipe,h):
 if recipe=='free1':return free(1)
 if recipe=='free2':return free(2)
 if recipe=='free-max-1':return free(2**64-2)
 if recipe=='free-max':return free(2**64-1)
 if recipe=='live':return material(1,previous=2 if h else 0)
 return material(2,cursor=int(recipe[-1]) if recipe.startswith('release') and recipe[-1].isdigit() else 0,cut=bool(h))

def make(row):
 name,op,h,old,new,kind,cursor,statement,generation=row
 bank=[ledger['cell'](0) for _ in range(64)];bank[0]=ledger['cell'](4);bank[0][80:120]=undo(9)
 bank[1]=ledger['cell'](0) if kind=='free' else job(kind,3,h=h>0,cursor=cursor)
 active=op in (2,4,20)
 if active and h:bank[2]=job('retained',2,h=h==2,table=7)
 if active and h==2:bank[3]=job('retained',1,table=8)
 before_lsn=5 if kind=='free' else int.from_bytes(bank[1][376:384],'little')
 before=page(14,cell(old,h),before_lsn);after=page(14,cell(new,h),101)
 hp=page(13,material(1,seq=2,table=7,previous=1 if h==2 else 0),21) if active and h else bytes(P)
 next_ref=13 if active and h else None
 target=13 if statement else None
 core=status['core'](op,14,next_ref,target,3 if op in (2,4,11) else 0,3 if op in (2,4,11) else 0);put(core,24,3);put(core,64,generation)
 if op in (21,22):put(core,160,62);put(core,168,63)
 f=wire['frame'](op,185 if active else 0,int(active and h>0));ca=3488 if f[7]==3 else 1888;f[ca:ca+256]=core
 sb=sa=bytes(P)
 if active:
  hcount=h
  if op==2:bc=status['home']('active',next_ref,hcount);ac=status['home']('active',14,hcount+1)
  elif op==4:
   bc=status['home']('statement' if statement else 'full',14,hcount+1,target,14)
   ac=status['home']('statement' if statement else 'full',next_ref,hcount,target,next_ref)
  else:bc=status['home']('commit',14,hcount+1);ac=status['home']('commit',next_ref,hcount)
  sb=status['page'](bc,100);sa=status['page'](ac,101)
 na=ca+256;ra=len(f)-32-int.from_bytes(f[400:404] if f[7]==3 else f[312:316],'little')
 for _ in range(int.from_bytes(f[20:22],'little')):
  length=int.from_bytes(f[na+20:na+24],'little');rlen=int.from_bytes(f[ra+4:ra+8],'little');k=f[na]
  b=a=None
  if k==9:b=bank[1]
  if k==3:b=before[15920:16352];a=after[15920:16352];put(f,ra+48,before_lsn);f[ra+64:ra+rlen]=after[72:16352]
  if k==5:b=sb[16128:16352];a=sa[16128:16352];f[ra+64:ra+rlen]=sa[72:16352]
  if b is not None:f[na+128:na+128+len(b)]=b;f[na+24:na+56]=sha(b)
  if a is not None:f[na+128+len(b):na+128+len(b)+len(a)]=a;f[na+56:na+88]=sha(a)
  na+=length;ra+=rlen
 f[-32:]=sha(f[:-32])
 return f+b''.join(bank)+before+after+hp+sb+sa

def main():
 assert not (ROOT/'cases.bin').exists(),'write-once'
 out=bytearray();index=[]
 for row in ROWS:
  raw=make(row);flen=len(raw)-65536-5*P
  index.append('\t'.join(map(str,(row[0],len(out),flen,row[1],row[2],int(row[7]),row[8],sha(raw).hex()))));out+=raw
 (ROOT/'cases.bin').write_bytes(out);(ROOT/'index.tsv').write_text('\n'.join(index)+'\n')
 parents=['../wire/author.py','../ledger/author.py','../status/author.py','../job3/cells.bin']
 (ROOT/'input-plan.json').write_text(json.dumps({'scope':'UNDO_LOCAL_ONLY; Status same-frame integration for Forward/CLR/Prune; other roles opaque invalid sentinels','literal_cases':len(ROWS),'bytes':len(out),'oracle':'explicit state pairs; integer/SHA, no product calls','recipes':{p:sha((ROOT/p).read_bytes()).hex() for p in parents}},indent=2)+'\n')
if __name__=='__main__':main()
