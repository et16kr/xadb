#!/usr/bin/env python3
"""Independent current Row transaction observations, stdlib only.
No product/other-author import or actual WAL/physical ownership claim.
"""
from pathlib import Path
import hashlib
import json
import struct
ROOT = Path(__file__).resolve().parent
PARENTS = {
 '../job3/cells.bin': 'd2ea7dae34cd200c8cf6d75500e669f8ddb54ddbb32f7ab26575648342de8dc7',
 '../envelopes/profiles.bin': 'e8f5b232f8583c50d697db8ba29d920c10e5cc3c81a939f2cd4289edc0d5767a',
}
def sha(b): return hashlib.sha256(b).digest()
def put(b, at, v, w=8): struct.pack_into({2:'<H',4:'<I',8:'<Q'}[w],b,at,v)
def parent(name):
 b=(ROOT/name).read_bytes(); assert sha(b).hex()==PARENTS[name]; return b
CELLS = parent('../job3/cells.bin')
def cell(n): return bytearray(CELLS[n*1024:(n+1)*1024])
def ref(page):
 b=bytearray(40)
 for at,v in [(0,0x100000001),(8,page),(16,2),(32,2)]: put(b,at,v)
 return b
# kind: reserved, retained, compensated, committed-cleanup. C/h are meaningful
# only while reserved/retained; releases retain sequence/statement/mutation.
def root(kind, seq, *, h=0, c=185, cursor=0, statement=None):
 return dict(kind=kind,seq=seq,h=h,c=c,cursor=cursor,statement=seq if statement is None else statement)
def job(spec):
 k=spec['kind']; q=spec['seq']; cursor=spec['cursor']
 index={'reserved':19,'retained':21,'committed':23+cursor,'compensated':27+cursor}[k]
 b=cell(index); b[80:120]=ref(9); b[128:168]=ref(11+q)
 for at,v in [(168,q),(248,spec['statement']),(256,q),(320,q),(64,10*q)]:put(b,at,v)
 if k in ('reserved','retained'):
  b[200:224]=ref(10)[:24]; b[224:248]=ref(11)[:24]; put(b,280,spec['c'],4)
  wal=311608+2*spec['c']+128*spec['h']; put(b,784,wal); put(b,880,wal)
  put(b,376,10*q if k=='reserved' else 10*q+1)
  if k=='retained':put(b,72,10*q+1)
 else:
  put(b,72,10*q+1); put(b,272,60 if k=='committed' else 40)
  put(b,376,(60 if k=='committed' else 40) if cursor==0 else 60+cursor)
  if k=='committed':put(b,264,7)
 return b

def case(name, state, roots=(), *, credit=False, target=None, apply=45, descriptor=True, mutation=None):
 return dict(name=name,state=state,roots=list(roots),credit=credit,target=target,apply=apply,descriptor=descriptor,mutation=mutation)
CASES=[
 case('begin-empty','active',apply=1),
 case('active-empty-credit','active',credit=True,apply=2),
 case('reserved-h0','active',[root('reserved',1)],credit=True,apply=10),
 case('retained-h0','active',[root('retained',1)],credit=True,apply=20),
 case('retained-h0-h1','active',[root('retained',1),root('retained',2,h=1,c=2228)],credit=True,apply=30),
 case('reserved-h1','active',[root('retained',1),root('reserved',2,h=1,c=2228)],credit=True,apply=20),
 case('statement-finished','active',[root('retained',1),root('compensated',2)]),
 case('statement-rollback','statement',[root('retained',1),root('retained',2,h=1,c=2228)],target=1,apply=35),
 case('statement-mark-reached','statement',[root('retained',1),root('compensated',2)],target=1),
 case('full-abort-progress','abort',[root('retained',1),root('retained',2,h=1,c=2228)],apply=35),
 case('full-abort-complete','abort',[root('compensated',1),root('compensated',2)]),
 case('committed-mixed','committed',[root('retained',1),root('retained',2,h=1,c=2228),root('compensated',3)],apply=50),
 case('post-prune','committed',[root('retained',1),root('committed',2,cursor=1),root('compensated',3,cursor=2)],apply=63,descriptor=False),
 case('post-prune-empty-head','committed',[root('committed',1,cursor=3),root('compensated',2,cursor=1)],apply=63,descriptor=False),
 case('aborted','aborted',[root('compensated',1),root('compensated',2)],apply=50),
 case('post-abort-release','aborted',[root('compensated',1,cursor=3),root('compensated',2,cursor=2)],apply=63,descriptor=False),
 case('canceled-high-water','active',mutation=9),
 case('statement-none-target','statement',[root('retained',1),root('retained',2,h=1,c=2228)],apply=35),
 case('statement-none-reached','statement',[root('compensated',1),root('compensated',2)]),
]
def rows(case):
 b=bytearray(640)
 for i in range(5): b[i*128]=i+1
 def debt(i, steps, wal, undo=0):
  if not steps:return
  for at,v in [(8,steps),(16,wal),(48,undo),(64,9),(72,425984),(88,16),(96,5),(104,1),(112,wal)]:put(b,i*128+at,v)
 terminal=case['state'] in ('committed','aborted')
 roots=case['roots']; live=[r for r in roots if r['kind'] in ('reserved','retained')]
 if not terminal:debt(0,len(live),sum(91096+r['c']+128*r['h'] for r in live))
 reserved=[r for r in roots if r['kind']=='reserved']
 if reserved:
  r,=reserved;debt(1,1,91304+r['c']+128*r['h'],1)
 if not terminal:debt(2,1,56712)
 n=(3 if case['credit'] else 1) if case['state']=='active' else 2 if case['state']=='statement' else 0
 debt(3,n,n*3040)
 return b

def write():
 banks=[];homes=[];descriptors=[];budgets=[];index=[]
 for case in CASES:
  state=case['state']; roots=case['roots']; retained=[r for r in roots if r['kind']=='retained']
  terminal=cell(5 if state=='committed' else 6 if state=='aborted' else 4)
  terminal[80:120]=ref(9)
  if state=='committed':put(terminal,264,7)
  bank=terminal+b''.join(job(r) for r in roots)+cell(0)*(63-len(roots)); banks.append(bank)
  core=bytearray(192);core[:6]=b'XTS1\x01\x00';core[6]={'active':1,'statement':2,'abort':2,'committed':3,'aborted':4}[state]
  core[7]=1 if state=='statement' else 2 if state=='abort' else 0;core[8:56]=terminal[16:64]
  if state=='committed':put(core,56,7)
  head=max((r['seq'] for r in retained),default=0)
  for bit,at,q in [(1,72,head),(2,112,case['target'] or 0),(4,152,head if state in ('statement','abort') else 0)]:
   if q:core[64]|=bit;core[at:at+40]=ref(11+q)
  home=bytearray(224);home[:6]=b'XSH1\x01\x00';put(home,6,32,2);put(home,8,len(retained));home[32:]=core;homes.append(home)
  budget=rows(case);budgets.append(budget)
  b=bytearray(1536)
  if case['descriptor']:
   b[:6]=b'XOD1\x01\x00';b[6]=core[6];put(b,8,1536,4);b[16]=1;b[24:72]=terminal[16:64];b[160:352]=core
   reserved=any(r['kind']=='reserved' for r in roots);lsn=case['apply'];mutation=case['mutation'] if case['mutation'] is not None else max((r['seq'] for r in roots),default=0)
   for at,v in [(72,lsn),(80,0 if lsn==1 else 1),(88,1),(104,1),(544,lsn if reserved else 1),(552,int(reserved)),(624,1 if lsn==1 else 2),(1336,1),(1344,mutation)]:put(b,at,v)
   b[96]=1;put(b,112,4096,4);b[128:160]=sha(parent('../envelopes/profiles.bin')[:4096])
   b[560:592]=sha(b'opaque-current-UndoSlot-claim' if reserved else b'');b[640:1280]=budget
   for at in range(64,112,8):put(b,1280+at-64,max(int.from_bytes(budget[i*128+at:i*128+at+8],'little') for i in range(5)))
   put(b,1328,sum(int.from_bytes(budget[i*128+112:i*128+120],'little') for i in range(5)))
   b[-32:]=sha(b[:-32])
  descriptors.append(b);index.append(case)
 for name,values in [('jobs.bin',banks),('homes.bin',homes),('descriptors.bin',descriptors),('budgets.bin',budgets)]: (ROOT/name).write_bytes(b''.join(values))
 (ROOT/'index.json').write_text(json.dumps(index,indent=2)+'\n')
if __name__=='__main__':write()
