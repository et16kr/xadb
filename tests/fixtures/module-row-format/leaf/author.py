#!/usr/bin/env python3
"""Independent local leaf+Undo+Status pairs; integers/SHA only.
Original Undo pairs supply their actual local roles. Other frame roles stay
opaque invalid sentinels. No product encoder/transition/producer is imported.
"""
from pathlib import Path
import json,runpy,struct,hashlib
ROOT=Path(__file__).resolve().parent
wire=runpy.run_path(str(ROOT.parent/'wire/author.py'))
put,sha,ref,actor=(wire[k] for k in ('put','sha','ref','actor'))
P=16384
ORIGINAL=(ROOT.parent/'undo/cases.bin').read_bytes()
assert sha(ORIGINAL).hex()=='0d6ab1e42741f1f120f10432a20ac9577cf2a3b21ea96a8f88d7f1e6760a7bb6'
INDEX={r[0]:r for r in [l.split('\t') for l in (ROOT.parent/'undo/index.tsv').read_text().splitlines()]}
def get(b,a,w=8):return int.from_bytes(b[a:a+w],'little')
def seal(b):b[-32:]=sha(b[:-32]);return b
def page(n,kind,owner,lsn,cells):
 b=bytearray(P);b[:6]=b'XDP1\x01\0';b[6]=kind;b[8:24]=wire['DB'];b[24:48]=ref(n)
 for a,v in [(48,owner),(56,1),(64,lsn)]:put(b,a,v)
 put(b,72,len(cells),2);put(b,74,128+4*len(cells),2);end=16352
 for i,c in enumerate(cells):
  end-=len(c);put(b,128+4*i,end,2);put(b,130+4*i,len(c),2);b[end:end+len(c)]=c
 put(b,76,end,2);return seal(b)
def schema(mode):
 columns={'int':[(2,0,b'i')],'null':[(2,1,b'i')],'max':[(4,0,b'b')],'mixed':[(1+i%4,int(i%5==0),('c'+str(i)).encode()) for i in range(32)]}[mode]
 return struct.pack('<HBB',1,len(columns),0)+b'T'+b''.join(struct.pack('<HBB',len(n),t,z)+n for t,z,n in columns)
def canonical(mode):
 if mode=='int':return struct.pack('<IBq',1,2,103)
 if mode=='null':return struct.pack('<IB',1,0)
 if mode=='max':return struct.pack('<IBI',1,4,2039)+bytes((i%251 for i in range(2039)))
 values=[]
 for i in range(32):
  t=1+i%4
  values.append(b'\0' if i%5==0 else bytes([1,i%2]) if t==1 else struct.pack('<Bq',2,-i) if t==2 else struct.pack('<BI',t,3)+('행'.encode() if t==3 else b'\x00\x80\xff'))
 return struct.pack('<I',32)+b''.join(values)
def record(n,c,committed=False,neighbor=False):
 b=bytearray(168+len(c));b[:6]=b'XRW1\x01\0';b[6]=0 if committed else 2;b[7]=1;put(b,8,n);b[16]=1 if committed else 2
 if committed:put(b,24,1 if neighbor else 9)
 else:b[24:72]=actor();b[120:160]=wire['undo'](14)
 b[72:112]=actor()[:40]
 if neighbor:put(b,104,9)
 put(b,112,n);put(b,160,len(c),4);b[168:]=c;return b
def entry(n,r):return struct.pack('<HH',8,len(r))+struct.pack('>Q',n)+r
def projection(value):
 b=bytearray(48);b[:8]=b'XRP1\x01\0\x01\0';b[8]=1
 for a,v in [(16,6),(24,1),(32,3)]:put(b,a,v)
 b[40]=int(value is not None);return sha(b+(value or b''))
def offsets(frame):
 ca=3488 if frame[7]==3 else 1888;na=ca+256;ra=len(frame)-32-get(frame,400 if frame[7]==3 else 312,4);out={}
 for i in range(get(frame,20,2)):
  out[frame[na]]=(na,ra);na+=get(frame,na+20,4);ra+=get(frame,ra+4,4)
 return ca,out
ROWS=[('forward-empty',2,'int',[]),('forward-first',2,'int',[5,7]),('forward-middle',2,'int',[1,5]),('forward-last',2,'int',[1,2]),('clr-middle',4,'int',[1,5]),('prune-middle',20,'int',[1,5])]+[(f'{label}-{mode}',op,mode,[]) for mode in ('null','max','mixed') for label,op in [('forward',2),('clr',4),('prune',20)]]
def make(row):
 name,op,mode,neighbors=row;has_h=bool(neighbors);old_name={2:'forward',4:'clr-full',20:'prune'}[op]+('-other-table' if has_h else '-none');idx=INDEX[old_name];at,fl=int(idx[1]),int(idx[2]);raw=ORIGINAL[at:at+fl+65536+5*P];old=raw[:fl];bank=bytearray(raw[fl:fl+65536]);proofs=raw[fl+65536:]
 c=canonical(mode);r=record(3,c);new=record(3,c,True) if op==20 else r
 meta=b'XBT1\x01\0'+bytes(26);before_rows={n:entry(n,record(n,c,True,True)) for n in neighbors};after_rows=dict(before_rows)
 if op!=2:before_rows[3]=entry(3,r)
 if op!=4:after_rows[3]=entry(3,new)
 before=page(20,1,6,90,[meta]+[before_rows[n] for n in sorted(before_rows)]);after=page(20,1,6,101,[meta]+[after_rows[n] for n in sorted(after_rows)])
 # Stable anchor10 points to current20; original Job hint11 is intentionally stale.
 anc=bytearray(48);anc[:7]=b'XTA1\x01\0\x01';anc[8:32]=ref(20);put(anc,32,2);anchor=page(10,11,6,80,[anc])
 s=schema(mode);cat=bytearray(232+len(s));cat[:8]=b'XCT1\x01\0\x01\0';cat[32]=1;put(cat,40,1)
 for a,v in [(8,6),(16,1),(24,1),(128,1)]:put(cat,a,v)
 cat[88:128]=actor()[:40];put(cat,120,9);cat[176:200]=ref(10);put(cat,224,len(s),4);cat[232:]=s
 cell_len=len(r)+12;j=memoryview(bank)[1024:2048];put(j,280,cell_len,4);j[288:320]=projection(r);cost=311608+2*cell_len+128*int(has_h);put(j,784,cost);put(j,880,cost)
 f=wire['frame'](op,cell_len,int(has_h));ca,parts=offsets(f);oldca,oldparts=offsets(old);f[ca:ca+256]=old[oldca:oldca+256]
 f[ca+176:ca+208]=projection(r if op!=2 else None);f[ca+208:ca+240]=projection(new if op!=4 else None)
 for kind,(na,ra) in parts.items():
  on,orr=oldparts[kind]
  if kind!=1:
   f[na:na+get(f,na+20,4)]=old[on:on+get(old,on+20,4)];f[ra:ra+get(f,ra+4,4)]=old[orr:orr+get(old,orr+4,4)]
   if kind==9:f[na+128:na+1152]=j;f[na+24:na+56]=sha(j)
   continue
  cursor=na+128
  for img,rows,sl,of,sz,dig in [(before,before_rows,4,8,12,24),(after,after_rows,6,10,16,56)]:
   if 3 not in rows:continue
   slot=sorted(rows).index(3)+1;cell=rows[3];put(f,na+sl,slot,2);put(f,na+of,get(img,128+4*slot,2),2);put(f,na+sz,len(cell),4);f[na+dig:na+dig+32]=sha(cell);f[cursor:cursor+len(cell)]=cell;cursor+=len(cell)
  put(f,ra+48,90);f[ra+64:ra+get(f,ra+4,4)]=after[72:16352]
 seal(f)
 return f+bank+proofs+anchor+before+after+cat+c,len(f),len(cat),len(c),has_h
if __name__=='__main__':
 assert not (ROOT/'cases.bin').exists(),'write-once'
 out=bytearray();index=[]
 for row in ROWS:
  raw,fl,cl,vl,h=make(row);index.append('\t'.join(map(str,(row[0],len(out),fl,cl,vl,row[1],int(h),sha(raw).hex()))));out+=raw
 (ROOT/'cases.bin').write_bytes(out);(ROOT/'index.tsv').write_text('\n'.join(index)+'\n')
 parents=['../wire/author.py','../undo/cases.bin','../undo/index.tsv']
 (ROOT/'input-plan.json').write_text(json.dumps({'scope':'LEAF_LOCAL_WITH_UNDO_STATUS; Root/afterJob/descriptors/auxiliary remain opaque invalid sentinels','literal_cases':len(ROWS),'oracle':'independent typed values/schema and explicit before/after pages; integers/SHA, no product encoder','recipes':{p:sha((ROOT/p).read_bytes()).hex() for p in parents}},indent=2)+'\n')
