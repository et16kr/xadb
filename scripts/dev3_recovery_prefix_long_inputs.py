#!/usr/bin/env python3
"""Generate S5 final-state inputs only; neither execute nor validate the product."""
import hashlib
import json
from pathlib import Path
import re
import sys
sys.dont_write_bytecode=True
import dev3_recovery_prefix_long_native as long


def main():
    root=long.ROOT
    files,allocator,descriptor,facts=long.oracle(root)
    spec=dict(schema='xadb-recovery-prefix-native-long-v1',unit='D3-RECOVERY-PREFIX-001',group='s5_long',status='INPUT_ONLY_NOT_RUN',generation_is_not_validation=True,selector=long.SELECTOR,cases=long.CASES,case_seconds=long.CASE_SECONDS,total_seconds=long.TOTAL_SECONDS,database_bytes=long.DB_LIMIT,retained_database_bytes=long.TOTAL_LIMIT,fd_limit=5,oracle_scope='Actual exclusive reserve63/cancel1000 producer followed by separate-PID RAM prefix analysis. Final allocator/descriptor/full ledger are independently expected. WAL length/header and all file bytes/inodes/hashes remain unchanged during analysis; whole source WAL bytes are not independently predicted. Home/slot/stage reads, mutation, stabilization, BOOT_RESERVE, repair and READY are outside this proof.',expected=dict(fixed_files={name:dict(bytes=len(raw),sha256=long.native.sha(raw)) for name,raw in files.items()},allocator_sha256=long.native.sha(allocator),descriptor_sha256=long.native.sha(descriptor),facts=facts))
    spec['patches']=dict(integer_encoding='u64 little endian; published page/XAL1/XOD1 offsets',allocator=dict(template='tests/fixtures/dev3-allocation-reservation/p1-n63-cancel.page',fields=[dict(offset=64,value=2000,meaning='pageLSN')]+[dict(offset=10144+64+96*i+8,value=1001,meaning=f'Free entry{i} incarnation') for i in range(1,64)],checksum=dict(offset=16352,algorithm='SHA256 bytes[0:16352]')),descriptor=dict(template='tests/fixtures/dev3-allocation-reservation/p1-n63-cancel.descriptor',fields=[dict(offset=offset,value=value,meaning=meaning) for offset,value,meaning in ((56,1000,'SystemWorkRef work id'),(72,2000,'apply LSN'),(80,1999,'previous owner LSN'),(88,1999,'birth LSN'),(544,2000,'inventory change LSN'))],checksum=dict(offset=1504,algorithm='SHA256 bytes[0:1504]')),unchanged_descriptor_facts=dict(state='SystemFinished',phase='Finished',sequence=1,cursor=63,limit=63,groups=2,consumed_wal=68192,remaining_all=0,inventory_items=0),wal=dict(frame_bytes=34096,frames=2000,header_bytes=4096,source_bytes=68196096,expected_full_wal_hash=None))
    paths={root/path for path in long.native.DEPENDENCIES}
    for base in ('crates/engine/tests/fixtures/dev3-storage-session/literal','tests/fixtures/dev3-doublewrite-install/literal'):
        paths.update(path for path in (root/base).rglob('*') if path.is_file())
    spec['dependencies']=[dict(path=str(path.relative_to(root)),bytes=path.stat().st_size,sha256=long.digest(path)) for path in sorted(paths)]
    spec['generator']=dict(path=str(Path(__file__).resolve().relative_to(root)),sha256=long.digest(Path(__file__)))
    raw=(json.dumps(spec,indent=2)+'\n').encode();(root/long.INPUT).write_bytes(raw)
    path=root/'scripts/dev3_recovery_prefix_long_native.py';source=path.read_text();source=re.sub(r"INPUT_SHA256='[^']+'",f"INPUT_SHA256='{hashlib.sha256(raw).hexdigest()}'",source);path.write_text(source)
    print(json.dumps(dict(status='INPUT_ONLY_NOT_RUN',cases=1,manifest_sha256=hashlib.sha256(raw).hexdigest())))


if __name__=='__main__':main()
