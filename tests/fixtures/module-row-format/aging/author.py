#!/usr/bin/env python3
"""Root3-bound Aging descriptors/candidate observations, stdlib only.
No product or other author import. These inputs do not claim actual WAL history.
"""
from pathlib import Path
import hashlib
import struct
ROOT=Path(__file__).resolve().parent
PARENTS={
 '../root3/initial-root.bin':'d6079c1ea15ea2668be3150c6128d9be4a72e28855336ea7c1d971f73d0b3fc4',
 '../root3/free-job.bin':'aee93979e2d5bf1901a0daf894a8f8c705fcb3335581f215f405b6326a8dbe69',
 '../job3/jobs.bin':'7942f26f444bf6ce5888da2055fd91d43a8f183932a6a6da9357fb5399d8d5ce',
 '../job3/cells.bin':'d2ea7dae34cd200c8cf6d75500e669f8ddb54ddbb32f7ab26575648342de8dc7',
 '../envelopes/profiles.bin':'e8f5b232f8583c50d697db8ba29d920c10e5cc3c81a939f2cd4289edc0d5767a',
}
def sha(b):return hashlib.sha256(b).digest()
def put(b,at,v,w=8):struct.pack_into({2:'<H',4:'<I',8:'<Q'}[w],b,at,v)
def get(b,at):return int.from_bytes(b[at:at+8],'little')
def seal(b):b[-32:]=sha(b[:-32]);return b
def parent(n):
 b=(ROOT/n).read_bytes();assert sha(b).hex()==PARENTS[n];return b

def root_page(root):
 b=bytearray(16384);b[:6]=b'XDP1\x01\x00';b[6]=12;b[8:24]=bytes(range(1,17))
 for at,v in [(24,0x100000001),(32,1),(40,1),(48,4),(56,1),(64,get(root,80))]:put(b,at,v)
 for at,v in [(72,1),(74,132),(76,15840),(128,15840),(130,512)]:put(b,at,v,2)
 b[15840:16352]=root;return seal(b)
def summarize(bank):
 budget=bytearray(640)
 for i in range(5):budget[i*128]=i+1
 masks=[0,0,0]
 for i in range(64):
  job=bank[i*1024:(i+1)*1024];masks[0 if job[6]==0 else 1 if job[6]==1 else 2]|=1<<i
  for r in range(5):
   for at in range(8,120,8):
    off=r*128+at;prev=get(budget,off);v=get(job,384+off)
    put(budget,off,max(prev,v) if 64<=at<112 else prev+v)
 return masks,budget

def descriptor(root,page,budget,previous):
 b=bytearray(1536);b[:7]=b'XOD1\x01\x00\x05';put(b,8,1536,4);b[16]=2;b[24:72]=root[16:64]
 for at,v in [(72,get(root,80)),(80,previous),(88,1),(104,1),(360,get(root,64)),(368,0),(376,64),
              (392,get(root,64)),(544,1),(608,get(root,416)),(624,get(root,64)-1),(1336,1)]:put(b,at,v)
 b[96]=1;put(b,112,4096,4);b[128:160]=sha(parent('../envelopes/profiles.bin')[:4096])
 b[352:354]=bytes([3,2]);b[384]=7;put(b,400,16384,4);b[416:448]=sha(page);b[448:480]=sha(root)
 b[560:592]=sha(b'');b[640:1280]=budget;b[1280:1328]=budget[384+64:384+112];put(b,1328,get(budget,384+112))
 return seal(b)

def main():
 initial=parent('../root3/initial-root.bin');free=parent('../root3/free-job.bin');jobs=parent('../job3/jobs.bin');cells=parent('../job3/cells.bin')
 # Empty, Catalog-only, Row-only and mixed banks exercise both peak recipes.
 catalog=cells[1024:2048]+cells[7*1024:8*1024]+free*62
 banks=[free*64,catalog,jobs[:65536],jobs[3*65536:4*65536],free*64]
 pages=[root_page(initial)];descriptors=[];aggregates=[];previous=0
 for i,bank in enumerate(banks):
  root=bytearray(initial);root[6]=1;root[16:32]=bytes(range(1,17));put(root,32,1);root[40]=3;put(root,48,1);put(root,56,1)
  lsn=[1,98,99,100,101][i]
  for at,v in [(64,i+2),(80,lsn),(392,20 if i else 0),(400,6 if i else 5),(408,2 if i else 0),(416,19672+i*1000),(432,20 if i else 0)]:put(root,at,v)
  masks,budget=summarize(bank)
  for at,v in zip([96,104,112],masks):put(root,at,v)
  # Independent current observations: each transition's opcode/WAL delta is
  # intentionally outside the descriptor producer's contract.
  put(root,88,64-masks[0].bit_count());root[128:160]=sha(bank);root[160:192]=sha(budget)
  page=root_page(root);pages.append(page);descriptors.append(descriptor(root,page,budget,previous));aggregates.append(budget);previous=lsn
 (ROOT/'pages.bin').write_bytes(b''.join(pages));(ROOT/'jobs.bin').write_bytes(b''.join(banks));(ROOT/'descriptors.bin').write_bytes(b''.join(descriptors));(ROOT/'aggregates.bin').write_bytes(b''.join(aggregates))
if __name__=='__main__':main()
