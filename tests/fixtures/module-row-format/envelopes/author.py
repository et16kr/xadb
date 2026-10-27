#!/usr/bin/env python3
"""Independent fixed control/page envelopes over previously independent cells.
Only struct and hashlib; no product codec, fixture author or test helper import.
"""
from pathlib import Path
import hashlib
import struct

ROOT = Path(__file__).resolve().parent
DB = bytes(range(1, 17))

def put(out, at, value, width=8):
    struct.pack_into({2:'<H',4:'<I',8:'<Q'}[width], out, at, value)

def seal(out):
    out[-32:] = hashlib.sha256(out[:-32]).digest()
    return out

def control(kind, length, version):
    out = bytearray(length)
    out[:4] = b'XDC1'
    put(out,4,version,2)
    out[6] = kind
    put(out,8,128,2)
    put(out,12,length,4)
    out[16:32] = DB
    put(out,32,1)
    put(out,40,1)
    put(out,56,length-160,4)
    return out

def profile(identifier):
    out = control(1,4096,3)
    values64 = {8:8<<30,16:1<<30,24:256<<20,32:16<<20,
                40:[4,16,64][identifier-1]<<20,48:8<<20,56:1<<20,
                64:8<<20,72:64<<20,80:16<<20,88:4<<20,96:12<<20,
                104:4<<20,160:16384*129,168:2<<20}
    values32 = {0:identifier,112:32,116:256,120:1<<20,124:32,128:128,
                136:16384,148:8192,156:16384,176:2,180:4096,184:1<<20}
    values16 = {132:16,134:8,140:64,142:64,144:1010,152:64}
    for width, values in [(8,values64),(4,values32),(2,values16)]:
        for at,value in values.items():
            put(out,128+at,value,width)
    return seal(out)

def page(number, kind, lsn, cells):
    out = bytearray(16384)
    out[:4] = b'XDP1'
    put(out,4,1,2)
    out[6] = kind
    out[8:24] = DB
    for at,value in [(24,0x100000001),(32,number),(40,1),(48,4),(56,1),(64,lsn)]:
        put(out,at,value)
    put(out,72,len(cells),2)
    put(out,74,128+4*len(cells),2)
    end = 16352
    for i,cell in enumerate(cells):
        start = end-len(cell)
        put(out,128+4*i,start,2)
        put(out,130+4*i,len(cell),2)
        out[start:end] = cell
        end = start
    put(out,76,end,2)
    return seal(out)

def parent(path, expected):
    data = (ROOT.parent/path).read_bytes()
    assert hashlib.sha256(data).hexdigest() == expected
    return data

def write():
    initial = parent('root3/initial-root.bin','d6079c1ea15ea2668be3150c6128d9be4a72e28855336ea7c1d971f73d0b3fc4')
    free = parent('root3/free-job.bin','aee93979e2d5bf1901a0daf894a8f8c705fcb3335581f215f405b6326a8dbe69')
    # Job3 bank0 and matching Root3 are complete independently authored cells.
    active = parent('job3/roots.bin','f9415abed73f9957e2ce28cd2cf2b24345857f94cc8529e272a193605072367e')
    jobs = parent('job3/jobs.bin','7942f26f444bf6ce5888da2055fd91d43a8f183932a6a6da9357fb5399d8d5ce')
    output = bytearray()
    for root, bank, lsn in [(initial,free*64,0),(active[:512],jobs[:65536],99)]:
        output += page(1,12,lsn,[root])
        for index in range(5):
            cells = [bank[i*1024:(i+1)*1024] for i in range(15*index,min(15*index+15,64))]
            output += page(2+index,13,lsn,cells)
    (ROOT/'pages.bin').write_bytes(output)
    (ROOT/'profiles.bin').write_bytes(b''.join(profile(i) for i in [1,2,3]))
    # Envelope only: not a complete validated FileMapRoot semantic input.
    (ROOT/'filemap-envelope.bin').write_bytes(seal(control(2,224,1)))

if __name__ == '__main__':
    write()
