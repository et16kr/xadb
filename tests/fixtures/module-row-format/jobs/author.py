#!/usr/bin/env python3
"""Independent JOB_LOCAL_ONLY explicit state pairs; integer/SHA raw recipes.
Other frame roles and sender descriptor semantics remain opaque sentinels.
"""
from pathlib import Path
import runpy,struct,json
ROOT=Path(__file__).resolve().parent
w=runpy.run_path(str(ROOT.parent/'wire/author.py'));l=runpy.run_path(str(ROOT.parent/'ledger/author.py'))
put,sha,ref,actor=(w[k] for k in ('put','sha','ref','actor'));P=16384;MAX=2**64-1
SENDER=bytes((i*17%251 for i in range(1536)))
def get(b,a,n=8):return int.from_bytes(b[a:a+n],'little')
def free(g=1):
 b=bytearray(1024);b[:6]=b'XQJ1\x03\0';put(b,8,g);return b
def terminal(state='leased'):
 b=l['cell']({'leased':4,'committed':5,'aborted':6}[state]);b[80:120]=w['undo'](9)
 if state=='committed':put(b,264,9)
 return b
def root(kind='reserved',seq=3,h=0,c=185,cursor=0):
 b=l['job'](l['root'](kind,seq,h=h,c=c,cursor=cursor))
 if kind=='committed':put(b,264,9)
 return b
def budget(b,kind,c,h,cursor):
 out=bytearray(640)
 for i in range(5):out[i*128]=i+1
 steps,wal=(1,55112) if kind=='terminal' else (5,311608+2*c+128*h) if kind in ('reserved','retained') else (4-cursor,(4-cursor)*55528)
 for at,v in [(8,steps),(16,wal),(64,9),(72,425984),(88,16),(96,5),(104,1),(112,wal)]:put(out,384+at,v)
 b[384:]=out
# name,op,ordinal,old/new kind,h,cursor,C,oldgen,terminal outcome
ROWS=[('begin-genesis',1,0,'free','terminal',0,0,0,1,'none'),('begin-slot14',1,14,'free','terminal',0,0,0,1,'none'),('begin-slot15',1,15,'free','terminal',0,0,0,1,'none'),('begin-slot63',1,63,'free','terminal',0,0,0,1,'none'),('begin-max',1,0,'free','terminal',0,0,0,MAX-1,'none'),('reserve',11,1,'free','reserved',0,0,185,1,'leased'),('reserve-max-slot63',11,63,'free','reserved',1,0,2228,MAX-1,'leased'),('forward',2,1,'reserved','retained',0,0,185,2,'leased'),('forward-max-slot15',2,15,'reserved','retained',1,0,2228,2,'leased'),('cancel',12,1,'reserved','free',0,0,185,2,'leased'),('commit-mixed',6,0,'terminal-leased','terminal-committed',0,0,0,2,'leased'),('abort-compensated',7,0,'terminal-leased','terminal-aborted',0,0,0,2,'leased'),('clr',4,1,'retained','compensated',0,0,185,2,'leased'),('clr-h',4,1,'retained','compensated',1,0,2228,2,'leased'),('prune',20,1,'retained','committed',0,0,185,2,'committed'),('prune-h',20,1,'retained','committed',1,0,2228,2,'committed')]+[(f'release-{kind}-{cursor}',21,1,kind,kind,0,cursor,0,2,'committed' if kind=='committed' or cursor==1 else 'aborted') for kind in ('committed','compensated') for cursor in range(3)]+[('rootfree-committed',22,1,'committed','free',0,3,0,2,'committed'),('rootfree-compensated',22,1,'compensated','free',0,3,0,2,'aborted'),('terminalfinish-committed',17,0,'terminal-committed','free',0,0,0,2,'committed'),('terminalfinish-aborted',17,0,'terminal-aborted','free',0,0,0,2,'aborted')]
def page(index,lsn,cells):
 b=bytearray(P);b[:6]=b'XDP1\x01\0';b[6]=13;b[8:24]=w['DB'];b[24:48]=ref(2+index);put(b,40,1)
 for a,v in [(48,4),(56,1),(64,lsn)]:put(b,a,v)
 for a,v in [(72,len(cells)),(74,128+4*len(cells)),(76,16352-1024*len(cells))]:put(b,a,v,2)
 for s,c in enumerate(cells):at=16352-1024*(s+1);put(b,128+4*s,at,2);put(b,130+4*s,1024,2);b[at:at+1024]=c
 b[-32:]=sha(b[:-32]);return b
def offsets(f):
 ca=3488 if f[7]==3 else 1888;na=ca+256;ra=len(f)-32-get(f,400 if f[7]==3 else 312,4)
 for i in range(get(f,20,2)):
  if f[na]==9:return ca,na,ra
  na+=get(f,na+20,4);ra+=get(f,ra+4,4)
 raise ValueError('missing job')
def make(spec):
 name,op,ordinal,old,new,h,cursor,c,gen,term=spec
 bank=[free() for _ in range(64)]
 if term!='none':bank[0 if ordinal!=0 else 1]=terminal(term)
 if old=='free':b=free(gen)
 elif old.startswith('terminal-'):b=terminal(old[9:]);bank[1]=free()
 else:b=root(old,3,h,c,cursor)
 bank[ordinal]=b
 if h:bank[2]=root('retained',2,0,185)
 if op==6:bank[1]=root('retained',3,0,185);bank[2]=root('compensated',1)
 if op==7:bank[1]=root('compensated',3);bank[2]=root('compensated',1)
 a=bytearray(b)
 if op in (1,11):
  a=terminal() if op==1 else root('reserved',3,h,c);put(a,8,gen+1);put(a,64,101)
 elif op==2:a[6]=2;put(a,72,101);a[328:360]=sha(SENDER)
 elif op in (6,7):a[6]=2;a[120]=1 if op==6 else 2;put(a,72,101);put(a,264,9 if op==6 else 0);a[328:360]=sha(SENDER)
 elif op in (4,20):a[6]=4 if op==4 else 3;a[200:248]=bytes(48);a[280:320]=bytes(40);put(a,264,9 if op==20 else 0);put(a,272,101);budget(a,new,c,h,0)
 elif op==21:a[121]=cursor+1;budget(a,new,c,h,cursor+1)
 if new=='free':a=free(gen)
 else:put(a,376,101)
 index,slot=divmod(ordinal,15);cells=bank[index*15:min(index*15+15,64)];before_lsn=0 if op==1 and gen==1 else 100
 before=page(index,before_lsn,cells);ac=list(cells);ac[slot]=a;after=page(index,101,ac)
 f=w['frame'](op,c if op in (2,4,20) else 0,h if op in (2,4,20) else 0);ca,na,ra=offsets(f)
 if op in (2,4,11,12,20,21,22):put(f,ca+24,3)
 if op in (2,4,11,12):put(f,ca+8,3);put(f,ca+16,3)
 if op in (17,20,21,22):put(f,ca+160,62);put(f,ca+168,(ordinal+1)%64)
 if op in (2,6,7):f[312:344]=sha(SENDER)
 put(f,na+4,slot,2);put(f,na+6,slot,2);put(f,na+8,16352-1024*(slot+1),2);put(f,na+10,16352-1024*(slot+1),2)
 f[na+128:na+1152]=b;f[na+1152:na+2176]=a;f[na+24:na+56]=sha(b);f[na+56:na+88]=sha(a)
 f[ra+8:ra+32]=ref(2+index);put(f,ra+24,1);put(f,ra+48,before_lsn);f[ra+64:ra+get(f,ra+4,4)]=after[72:16352]
 f[-32:]=sha(f[:-32]);return f+b''.join(bank)+before+after+SENDER,len(f)
if __name__=='__main__':
 assert not (ROOT/'cases.bin').exists(),'write-once'
 out=bytearray();index=[]
 for spec in ROWS:
  raw,fl=make(spec);index.append('\t'.join(map(str,(spec[0],len(out),fl,*spec[1:],sha(raw).hex()))));out+=raw
 (ROOT/'cases.bin').write_bytes(out);(ROOT/'index.tsv').write_text('\n'.join(index)+'\n')
 (ROOT/'input-plan.json').write_text(json.dumps({'scope':'JOB_LOCAL_ONLY; descriptor semantics/other frame roles opaque invalid sentinels','literal_cases':len(ROWS),'oracle':'explicit before/after state pairs, raw integer/SHA recipes, no product imports','recipes':{p:sha((ROOT/p).read_bytes()).hex() for p in ['../wire/author.py','../ledger/author.py','../job3/cells.bin']}},indent=2)+'\n')
