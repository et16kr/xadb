#!/usr/bin/env python3
"""Independent Base4 raw-layout inputs. stdlib only, no product/generator import.
Opaque descriptors/inventory and digest claims deliberately do NOT close a graph,
original INIT witness, durable history, or actual Profile3 database selection.
"""
from pathlib import Path
import hashlib
import struct

ROOT = Path(__file__).resolve().parent
DB = bytes(range(1, 17))
BASE = 2097152
PAGE = 16384
SECTIONS = [(4096,55296,36,1536),(65536,6144,64,96),
            (73728,22528,64,352),(98304,PAGE,1,PAGE),(262144,64*PAGE,64,PAGE)]
PARENTS = {
 '../envelopes/pages.bin':'a6877a6d30c795f532803ce348a56962735652d80d1d06c28239d873f9fac6d7',
 '../envelopes/profiles.bin':'e8f5b232f8583c50d697db8ba29d920c10e5cc3c81a939f2cd4289edc0d5767a',
 '../../dev3-plan4-recovery-base/vectors/p1/references/P00.bin':'077d06af8b37bec32a9472ed498d3fa884fbe5e8fd84c457f6ebfa96e9946cfd',
 '../../dev3-plan4-recovery-base/vectors/p1/references/P07.bin':'c666b6d1954d2b19faa83d5bdb381f4edd337af835a47713e31d1efa52bcee90',
 '../../dev3-plan4-recovery-base/vectors/p1/references/P08.bin':'646d2c91744594c80afc26e4919dffece69dd996e436a1390191a164c5a68567',
}
def sha(b): return hashlib.sha256(b).digest()
def put(b,at,v,width=8): struct.pack_into({2:'<H',4:'<I',8:'<Q'}[width],b,at,v)
def get(b,at,width=8): return int.from_bytes(b[at:at+width],'little')
def seal(b):
    b[-32:]=sha(b[:-32])
    return b
def ref(n,inc=2): return struct.pack('<QQQ',0x100000001,n,inc)
def source(local,txn=False):
    return DB+struct.pack('<QIIQQ',1,1,0,local,2 if txn else 1)
def page(n,kind,owner,lsn):
    b=bytearray(PAGE);b[:6]=b'XDP1\x01\x00';b[6]=kind;b[8:24]=DB;b[24:48]=ref(n)
    for at,v in [(48,owner),(56,1),(64,lsn)]:put(b,at,v)
    put(b,74,128,2);put(b,76,PAGE-32,2)
    return seal(b)
def load(path):
    b=(ROOT/path).read_bytes();assert sha(b).hex()==PARENTS[path];return b

def make(active):
    queue=load('../envelopes/pages.bin')[(6*PAGE if active else 0):((12 if active else 6)*PAGE)]
    profiles=load('../envelopes/profiles.bin');profile=profiles[:4096]
    images={0:bytearray(load('../../dev3-plan4-recovery-base/vectors/p1/references/P00.bin')),
            7:load('../../dev3-plan4-recovery-base/vectors/p1/references/P07.bin'),
            8:load('../../dev3-plan4-recovery-base/vectors/p1/references/P08.bin')}
    images.update({i+1:queue[i*PAGE:(i+1)*PAGE] for i in range(6)})
    provenance={}
    if active:
        for n,kind,owner,lsn in [(9,9,2,2),(10,8,3,4),(11,11,6,6),(12,1,6,6)]:
            images[n]=page(n,kind,owner,lsn)
            p=bytearray(352);p[:5]=bytes([1,n-8,kind,0,1 if n<11 else 2]);p[8:48]=images[n][24:64]
            put(p,48,lsn);put(p,56,1,4);p[64:96]=sha(b'opaque INIT frame claim'+bytes([lsn]));p[96:128]=sha(images[n])
            p[128:176]=source(n-8 if n<11 else 1,n>=11);put(p,176,lsn-1)
            if n>=11:
                for at,v in [(184,6),(192,1),(200,1)]:put(p,at,v)
                p[208:232]=ref(11);p[232:256]=ref(12)
            provenance[n]=p
        allocator=images[0];start=get(allocator,128,2)+64
        for n in range(9,14):
            at=start+n*96;allocator[at:at+96]=bytes(96);allocator[at]=2 if n<13 else 1;put(allocator,at+8,2)
            if n<13:
                allocator[at+1]=images[n][6];put(allocator,at+16,get(images[n],48));put(allocator,at+24,1)
            else:
                allocator[at+2]=1;allocator[at+32:at+80]=source(3)
        put(allocator,64,7);seal(allocator)
    b=bytearray(BASE);b[:8]=b'XRB1\x04\x00\x01\x00';put(b,8,BASE,4);put(b,12,4096,4);b[16:32]=DB
    root=queue[get(queue,128,2):get(queue,128,2)+512]
    capture=99 if active else 1
    for at,v in [(32,1),(40,1),(80,4),(88,1),(96,capture),(152,4096),(192,3 if active else 0),
                 (208,3 if active else 0),(224,1 if active else 0),(240,5),(248,capture),
                 (264,512 if active else 0),(272,get(root,104)),(280,get(root,112)),
                 (288,get(root,64)),(296,get(root,64)-1),(448,1),(504,1),(1152,32<<20)]:put(b,at,v)
    b[48:80]=sha(profile);put(b,144,1,4);put(b,148,1,4);put(b,1184,1,4)
    b[160:192]=sha(b'opaque prefix claim');b[1192:1224]=sha(b'opaque WAL header claim')
    terminals=0
    for index in range(1,6):
        pg=queue[index*PAGE:(index+1)*PAGE]
        for slot in range(get(pg,72,2)):
            start=get(pg,128+4*slot,2)
            terminals+=pg[start+6]==1 and pg[start+7]==1
    put(b,256,terminals,4);put(b,260,2 if active else 0,4);b[304]=3 if active else 0
    for at,kind,length,digest in [(320,1,4096,sha(profile)),(384,2,224,sha(b'opaque FileMap claim'))]:
        b[at]=kind;put(b,at+8,1);put(b,at+16,length,4);b[at+32:at+64]=digest
    b[464:496]=sha(b'opaque bootstrap claim')
    for row in range(5):b[512+128*row]=row+1
    for n,img in images.items():
        at=65536+96*n;b[at:at+3]=bytes([1,img[6],0]);b[at+8:at+56]=img[24:72];b[at+56:at+88]=sha(img)
        b[262144+n*PAGE:262144+(n+1)*PAGE]=img
    for n,p in provenance.items():b[73728+n*352:73728+(n+1)*352]=p
    for i,(at,size,count,width) in enumerate(SECTIONS):
        head=1280+64*i;put(b,head,i+1,2)
        for off,v in [(4,at),(8,size),(12,count),(16,width)]:put(b,head+off,v,4)
        b[head+32:head+64]=sha(b[at:at+size])
    return seal(b)

if __name__=='__main__':
    for name,active in [('initial',False),('active',True)]:
        (ROOT/(name+'.xrb')).write_bytes(make(active))
