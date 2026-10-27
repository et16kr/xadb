#!/usr/bin/env python3
"""Independent Root3 cell literals: integer offsets and Python SHA-256 only.
No XADB encoder/decoder, no page wrapper, no actual storage authority.
"""
from pathlib import Path
import hashlib,struct
ROOT=Path(__file__).resolve().parent

def u64(out,at,value): struct.pack_into('<Q',out,at,value)

def root():
    out=bytearray(512)
    out[:4]=b'XQR1'
    struct.pack_into('<H',out,4,3)
    struct.pack_into('<II',out,8,512,64)
    u64(out,64,1);u64(out,96,2**64-1);u64(out,400,5)
    job=bytearray(1024);job[:4]=b'XQJ1';struct.pack_into('<H',job,4,3);u64(job,8,1)
    out[128:160]=hashlib.sha256(job*64).digest()
    rows=bytearray(640)
    for i in range(5):rows[i*128]=i+1
    out[160:192]=hashlib.sha256(rows).digest()
    out[192:208]=bytes(range(1,17))
    u64(out,208,1);u64(out,216,4);u64(out,224,1)
    for i in range(6):struct.pack_into('<QQQ',out,232+i*24,0x100000001,i+1,1)
    return out,job

if __name__=='__main__':
    initial,job=root()
    (ROOT/'initial-root.bin').write_bytes(initial)
    (ROOT/'free-job.bin').write_bytes(job)
    active=bytearray(initial);active[6]=1
    active[16:32]=bytes(range(1,17));u64(active,32,1);active[40]=3
    u64(active,48,1);u64(active,56,1)
    for at,value in [(64,42),(72,17),(80,99),(88,8),(96,2**64-2),(104,1),
                     (376,3),(384,4),(392,7),(400,6),(408,2),(416,123456),(424,16384),(432,7)]:u64(active,at,value)
    # Active commitments are intentionally opaque: only the later Job3 census
    # can relate them to 64 occupied/free Job cells and the retained debt.
    active[128:160]=bytes([0x33])*32;active[160:192]=bytes([0x44])*32
    (ROOT/'active-root.bin').write_bytes(active)
