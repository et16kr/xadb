#!/usr/bin/env python3
"""Freeze independent existing literal inputs for native prefix inspection; never run product code."""
import hashlib
import json
from pathlib import Path
import re
import sys
sys.dont_write_bytecode=True
import dev3_recovery_prefix_native as native


def main():
    root=Path(__file__).resolve().parent.parent
    spec=dict(schema='xadb-recovery-prefix-native-v1',unit='D3-RECOVERY-PREFIX-001',status='INPUT_ONLY_NOT_RUN',generation_is_not_validation=True,selector=native.SELECTOR,cases=native.CASES,case_seconds=native.CASE_SECONDS,total_seconds=native.TOTAL_SECONDS,database_bytes=native.DB_LIMIT,total_bytes=native.TOTAL_LIMIT,fd_limit=native.FD_LIMIT,scope='RAM prefix only; no home/slot/stage reads, mutation, stabilization, BOOT_RESERVE or repair',expected={})
    for case in native.CASES:
        files,oracle=native.expected_files(root,case)
        spec['expected'][case]=dict(files={name:dict(bytes=len(raw),sha256=native.sha(raw)) for name,raw in files.items()},inspection={key:None if oracle[key] is None else dict(bytes=len(oracle[key]),sha256=native.sha(oracle[key])) for key in ('allocator','status','descriptor')},facts=native.expected_observation(files,oracle,case))
    paths={root/path for path in native.DEPENDENCIES}
    for base in ('crates/engine/tests/fixtures/dev3-storage-session/literal','tests/fixtures/dev3-doublewrite-install/literal'):
        paths.update(path for path in (root/base).rglob('*') if path.is_file())
    spec['dependencies']=[dict(path=str(path.relative_to(root)),bytes=path.stat().st_size,sha256=native.digest(path)) for path in sorted(paths)]
    spec['generator']=dict(path=str(Path(__file__).resolve().relative_to(root)),sha256=native.digest(Path(__file__)))
    raw=(json.dumps(spec,indent=2)+'\n').encode();(root/native.INPUT).write_bytes(raw)
    path=root/'scripts/dev3_recovery_prefix_native.py';source=path.read_text();source=re.sub(r"INPUT_SHA256 = '[^']+'",f"INPUT_SHA256 = '{hashlib.sha256(raw).hexdigest()}'",source);path.write_text(source)
    print(json.dumps(dict(status='INPUT_ONLY_NOT_RUN',cases=len(native.CASES),manifest_sha256=hashlib.sha256(raw).hexdigest())))


if __name__=='__main__':main()
