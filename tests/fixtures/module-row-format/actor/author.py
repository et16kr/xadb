#!/usr/bin/env python3
"""Independent sparse Base4 actor graphs; integers/SHA only, no product import.
Opaque history/descriptors/inventory remain deliberately outside this input.
"""
from pathlib import Path
import hashlib, json, struct
ROOT=Path(__file__).resolve().parent
P=16384; DB=bytes(range(1,17)); SECTIONS=[(4096,55296),(65536,6144),(73728,22528),(98304,P),(262144,64*P)]
def sha(b):return hashlib.sha256(b).digest()
def put(b,a,v,w=8):struct.pack_into({2:'<H',4:'<I',8:'<Q'}[w],b,a,v)
def get(b,a,w=8):return int.from_bytes(b[a:a+w],'little')
def seal(b):b[-32:]=sha(b[:-32]);return b
def parent(name,h):
 b=(ROOT/name).read_bytes();assert sha(b).hex()==h;return b
INITIAL=parent('../base4/initial.xrb','9473833616a56fbf74fcfc51d14c5eeb2ff96a4ce7cc3889d3f3eb3e809f4df0')
JOBS=parent('../ledger/jobs.bin','52c188341453ec54263b70d1abbffe7335c91abff336e4045eb04ac27b72f750')
HOMES=parent('../ledger/homes.bin','ae2fab7f4cb47ecc73941c0917fa218a9bf31b92128619337e981b5bc60ae297')
def ref(n,inc=2):return struct.pack('<QQQ',0x100000001,n,inc)
def slot(n):return ref(n)+bytes(8)+struct.pack('<Q',2)
def page(n,kind,owner,lsn,cells,inc=2):
 b=bytearray(P);b[:6]=b'XDP1\x01\0';b[6]=kind;b[8:24]=DB;b[24:48]=ref(n,inc)
 for a,v in [(48,owner),(56,1),(64,lsn)]:put(b,a,v)
 put(b,72,len(cells),2);put(b,74,128+4*len(cells),2);end=P-32
 for i,c in enumerate(cells):
  start=end-len(c);put(b,128+4*i,start,2);put(b,130+4*i,len(c),2);b[start:end]=c;end=start
 put(b,76,end,2);return seal(b)
def row(n,actor,committed=False):
 b=bytearray(181);b[:6]=b'XRW1\x01\0';b[6]=0 if committed else 2;b[7]=1;put(b,8,n)
 b[16]=1 if committed else 2
 if committed:put(b,24,7)
 else:b[24:72]=actor;b[120:160]=slot(11+n)
 b[72:112]=actor[:40];put(b,112,n);put(b,160,13,4);b[168:]=struct.pack('<IBq',1,2,100+n);return b
def entry(n,value):return struct.pack('<HH',8,len(value))+struct.pack('>Q',n)+value
def projection(n,value):
 b=bytearray(48);b[:8]=b'XRP1\x01\0\x01\0';b[8]=1
 for a,v in [(16,6),(24,1),(32,n)]:put(b,a,v)
 b[40]=1;return sha(b+value)
def make(case,parent_index):
 bank=[bytearray(JOBS[parent_index*65536+i*1024:parent_index*65536+(i+1)*1024]) for i in range(64)]
 home=bytearray(HOMES[parent_index*224:(parent_index+1)*224]);actor=bank[0][16:64]
 if case in (1,2,9):
  bank[3]=bytearray(bank[2]);bank[3][128:168]=slot(14)
  for a,v in [(64,30),(72,31),(168,3),(248,3),(256,3),(320,3),(376,31)]:put(bank[3],a,v)
  put(home,8,3);home[104:144]=slot(14)
  if case==2:home[184:224]=slot(14)
 orphan=case==9
 if orphan:
  bank[3]=bytearray(bank[10]);put(home,8,2);home[104:144]=slot(13)
 rootfree=case in (6,10)
 if rootfree:bank[1]=bytearray(bank[10])
 images={i:bytearray(INITIAL[262144+i*P:262144+(i+1)*P]) for i in range(9)}
 # A committed table created by a distinct actor. Actual row writer local1.
 schema=struct.pack('<HBB',1,1,0)+b'T'+struct.pack('<HBB',1,2,0)+b'i'
 cat=bytearray(232+len(schema));cat[:8]=b'XCT1\x01\0\x01\0';cat[32]=1;put(cat,40,1)
 for a,v in [(8,6),(16,1),(24,1),(128,1)]:put(cat,a,v)
 cat[88:128]=actor[:40];put(cat,120,9);cat[176:200]=ref(10);put(cat,224,len(schema),4);cat[232:]=schema
 meta=bytearray(images[8][get(images[8],128,2):get(images[8],128,2)+32])
 images[8]=page(8,5,5,90,[meta,entry(6,cat)],1)
 anc=bytearray(48);anc[:7]=b'XTA1\x01\0\x01';anc[8:32]=ref(11);put(anc,32,1)
 images[10]=page(10,11,6,6,[anc])
 rows=[]
 for j in bank:
  if j[7]!=3:continue
  n=get(j,320);state=j[6];cursor=j[121]
  if state<3:
   oldc=get(j,280,4);h=(get(j,784)-311608-2*oldc)//128;put(j,280,193,4)
   wal=311608+386+128*h;put(j,784,wal);put(j,880,wal);j[288:320]=projection(n,row(n,actor))
  u=bytearray(432);u[:6]=b'XUR1\x01\0';put(u,8,2)
  if state!=1:
   u[6]=1 if state==2 else 2;u[24:72]=actor
   for a,v in [(16,n),(72,n),(80,2 if state==2 else 0),(96,6),(104,1),(112,n)]:put(u,a,v)
   u[88]=1
   previous=max([get(k,168) for k in bank if k[7]==3 and k[6]==2 and get(k,168)<n],default=0)
   if state==2 and previous:u[326]=1;u[120:160]=slot(11+previous);put(u,160,previous)
   if state>=3:u[326]=3 if cursor>=2 else (2 if n>1 else 0);u[327]=3 if cursor>=1 else 0;u[328]=cursor
  images[11+n]=page(11+n,8,3,get(j,376),[u])
  if state in (2,3):rows.append(entry(n,row(n,actor,state==3)))
 if rootfree:rows.insert(0,entry(1,row(1,actor,True)))
 if orphan:rows.append(entry(3,row(3,actor)))
 images[11]=page(11,1,6,90,[meta]+rows)
 if case==10:
  second=bytearray(cat);put(second,8,7);put(second,120,10);put(second,40,2);second[176:200]=ref(16);second[236]=ord('U')
  images[8]=page(8,5,5,90,[meta,entry(6,cat),entry(7,second)],1)
  other_anchor=bytearray(anc);other_anchor[8:32]=ref(17)
  images[16]=page(16,11,7,6,[other_anchor]);images[17]=page(17,1,7,90,[meta,entry(19,row(19,actor,True))])
 images[9]=page(9,9,2,90,[home])
 # Root commitment over the independently stated actor Job bank.
 root=bytearray(INITIAL[262144+P+get(INITIAL,262144+P+128,2):262144+P+get(INITIAL,262144+P+128,2)+512])
 root[6]=1;root[16:32]=DB;put(root,32,1);root[40]=3;put(root,48,1);put(root,56,1)
 masks=[0,0,0];budget=bytearray(640)
 for r in range(5):budget[128*r]=r+1
 for i,j in enumerate(bank):
  masks[0 if j[6]==0 else 1 if j[6]==1 else 2]|=1<<i
  for r in range(5):
   for a in range(8,120,8):
    at=128*r+a;v=get(j,384+at);old=get(budget,at);put(budget,at,max(old,v) if 64<=a<112 else old+v)
 occupied=64-masks[0].bit_count()
 for a,v in [(64,2),(80,99),(88,occupied+int(rootfree)),(96,masks[0]),(104,masks[1]),(112,masks[2]),(376,int(rootfree)),(392,20),(400,7 if case==10 else 6),(408,7),(432,20)]:put(root,a,v)
 root[128:160]=sha(b''.join(bank));root[160:192]=sha(budget)
 images[1]=page(1,12,4,99,[root],1)
 for i in range(5):images[2+i]=page(2+i,13,4,99,bank[15*i:min(15*i+15,64)],1)
 allocator=images[0];at=get(allocator,128,2)+64
 for n,img in images.items():
  if n<9:continue
  a=at+n*96;allocator[a:a+96]=bytes(96);allocator[a]=2;allocator[a+1]=img[6];put(allocator,a+8,2);put(allocator,a+16,get(img,48));put(allocator,a+24,1)
 put(allocator,64,90);seal(allocator)
 b=bytearray(INITIAL)
 for a,v in [(96,99),(192,10),(208,10),(224,1),(248,99),(264,512),(272,masks[1]),(280,masks[2]),(288,2),(296,1)]:put(b,a,v)
 put(b,256,int(bank[0][6]==1),4)
 for n,img in images.items():
  b[262144+n*P:262144+(n+1)*P]=img;a=65536+n*96;b[a:a+96]=bytes(96);b[a:a+3]=bytes([1,img[6],0]);b[a+8:a+56]=img[24:72];b[a+56:a+88]=sha(img)
  if n<9:continue
  p=bytearray(352);role=1 if n==9 else 3 if n in (10,16) else 4 if n in (11,17) else 2;p[:5]=bytes([1,role,img[6],0,2 if n in (10,11,16,17) else 1]);p[8:48]=img[24:64]
  lsn=2 if n==9 else 6 if n in (10,11,16,17) else 4
  put(p,48,lsn);put(p,56,1,4);p[64:96]=sha(b'opaque original INIT');p[96:128]=sha(b'opaque initial page')
  p[128:168]=actor[:40];put(p,160,10 if n>=16 else 9 if n in (10,11) else n);put(p,168,2 if n in (10,11,16,17) else 1);put(p,176,lsn-1)
  if n in (10,11,16,17):
   for a,v in [(184,7 if n>=16 else 6),(192,1),(200,1)]:put(p,a,v)
   p[208:232]=ref(16 if n>=16 else 10);p[232:256]=ref(17 if n>=16 else 11)
  b[73728+n*352:73728+(n+1)*352]=p
 for i,(a,size) in enumerate(SECTIONS):b[1280+i*64+32:1280+i*64+64]=sha(b[a:a+size])
 seal(b)
 return b[:4096]+b[65536:98304]+b[262144:262144+18*P]
if __name__=='__main__':
 selected=[0,4,7,8,11,12,13,15,5,4,13]
 (ROOT/'cases.bin').write_bytes(b''.join(make(i,n) for i,n in enumerate(selected)))
 (ROOT/'index.json').write_text(json.dumps(dict(cases=['empty','three-retained','statement-target-one','compensated-active','compensated-committed','head-pruned','root-free-row-survives','abort-release','reserved-after-retained','orphan-row-known-bad','two-table-root-free'],record_bytes=331776,parent_ledger_cases=selected),indent=2)+'\n')
