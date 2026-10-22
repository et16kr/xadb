#!/usr/bin/env python3
"""Author independent closed-effects snapshots/vectors; not validation or execution."""
import hashlib
import json
from pathlib import Path
import re
import sys
sys.dont_write_bytecode=True
import dev3_recovery_effects_native as native


def main():
    root=native.ROOT;o=native.oracle
    spec=dict(schema='xadb-closed-recovery-native-v1',unit=o.UNIT,status='INPUT_ONLY_NOT_RUN',generation_is_not_validation=True,selector=native.SELECTOR,cases=o.ALL_CASES,groups=dict(mandatory=list(o.CASES),s5=[c for c in o.S5_CASES if c not in o.ALIASES],s5_matrix_all=list(o.S5_CASES),aliases=o.ALIASES),budgets=dict(case_seconds=native.CASE_SECONDS,total_seconds=native.TOTAL_SECONDS,database_bytes=native.DB_LIMIT,retained_database_bytes=native.TOTAL_LIMIT,fd_limit=native.FD_LIMIT,failed_snapshots_bytes=native.SNAPSHOT_LIMIT,logs_bytes=native.LOG_LIMIT),s5_budgets=dict(case_seconds=30,total_seconds=1200,database_bytes=24<<20,retained_database_bytes=1536<<20),expected={},oracle_scope='Full independently expected actual producer WAL+fixed file bytes and all phase snapshots; canonical physical A/S/descriptor/ledger and per-call read/write hashes/order. Whole source unchanged. BOOT rewrites on every entry; only home/DWB write0 on idempotent reentry. Lower producer reuse is not a second installation PASS.')
    for case in o.ALL_CASES:
        snapshots,obs=o.snapshots(root,case);producer,_=o.prepared(root,case,False)
        steps=o.histories(case);vectors=[]
        for index,(mode,fault) in enumerate(steps):
            mutations=native.vectors.mutation_vector(snapshots[index],obs,mode,fault)
            vectors.append(dict(mode=mode,fault=fault,mutations=[{k:v for k,v in row.items() if k!='_payload'} for row in mutations],failed_accounting=o.failed_facts(snapshots[index],obs,fault,o.ALL_CASES[case]['context']) if mode in ('fail','reject_recover') else None,new_site_calls=native.vectors.effect_calls(snapshots[index],obs,mode,fault,o.ALL_CASES[case]['context']) if mode!='reject_acquire' else [],expected_error=native.expected_error(case,fault) if mode in ('fail','reject_acquire','reject_recover') else None))
        succeeds=any(mode in ('recover','verify') for mode,_ in steps)
        spec['expected'][case]=dict(producer=native.wire_identity(producer),snapshots=[native.wire_identity(f) for f in snapshots],inspection={key:None if obs[key] is None else dict(bytes=len(obs[key]),sha256=o.sha(obs[key])) for key in ('allocator','status','descriptor')},facts=o.facts(snapshots[-1],obs) if succeeds else None,process_steps=vectors)
    spec['provenance']=dict(allocation_count_history='n1 -> cancel -> n8 -> cancel -> n63 -> cancel/active; frozen n63 is work3/LSN5..6, not fresh work1',status='Published independent struct/hashlib Status table creates expected wire; no candidate Rust encoder called',historical_pending='Actual captured reserveLSN1 image, concurrent actual cancelLSN2, actual publish AfterError; never splice producer WAL',boot='Q1048576/64KiB chunks, expected zero SHA; st_blocks separately observed',effect='Every postlease error CallerMustResolve/MayHaveChanged; actual input immutability distinct from lease effect')
    spec['literal_calls']=dict(absent=native.vectors.boot_calls([None,None]),existing_full=native.vectors.boot_calls([1048576,1048576]),source=native.vectors.SOURCE_CALLS,boot_before_union=dict(absent_calls=49,existing_only_zero_validation_calls=36,total=85,existing_alias='all common provision/write/sync/reopen/snapshot calls alias absent branch; only existing zero-scan RO open/named/reads add sites',supersedes='Original proposal80 was NOT_RUN. F07 now requires create-close-RW opened/named first stamp, post-sync RO opened/named and BOOTdirsync final slot0/slot1/source snapshots. Absent49+existing-only36=85; source stabilization3 is separate.'))
    spec['literal_calls']['new_site_histories']={}
    for label,case in (('allocator_single_reserve','NR04'),('historical_pending_later_cancel','NR13'),('status_second_home','NR01')):
        files,obs=o.prepared(root,case)
        calls=native.vectors.effect_calls(files,obs,'recover','none',o.ALL_CASES[case]['context'])
        seen={};occurrences=[]
        for index,call in enumerate(calls,1):
            key=tuple(call[k] for k in ('phase','kind','name','offset','length'));prior=seen.get(key,[])
            ident=f'{label}:{index:03}'
            occurrences.append(dict(id=ident,call=call,tuple_occurrence=len(prior)+1,same_tuple_first=prior[0] if prior else None,fault_occurrence_is_distinct=True));seen.setdefault(key,[]).append(ident)
        spec['literal_calls']['new_site_histories'][label]=dict(case=case,before_calls=calls,occurrences=occurrences,authored_calls_sha256=o.sha(json.dumps(calls,sort_keys=True,separators=(',',':')).encode()),preinspect_scope='Old readonly Prefix read sites excluded; Inventory and every new call included; canonical re-scan physical reads retained',call_count=len(calls))
    spec['checker_budget']=dict(methods=30,aggregate_seconds=30,selectors=['test_dev3_recovery_effects','test_dev3_recovery_effects_native','test_dev3_recovery_effects_compiler'])
    paths={root/path for path in o.prefix.DEPENDENCIES}
    for directory in ('crates/engine/tests/fixtures/dev3-storage-session/literal','tests/fixtures/dev3-doublewrite-install/literal'):
        paths.update(p for p in (root/directory).rglob('*') if p.is_file())
    paths.update(root/path for path in ('scripts/dev3_recovery_effects_oracle.py','scripts/dev3_recovery_effects_vectors.py','scripts/dev3_status_home_flush_literals.py','devdoc/contracts/dev3-recovery-effects.md'))
    spec['dependencies']=[dict(path=str(p.relative_to(root)),bytes=p.stat().st_size,sha256=native.digest(p)) for p in sorted(paths)]
    spec['dependency_bytes']=sum(row['bytes'] for row in spec['dependencies'])
    spec['generator']=dict(path=str(Path(__file__).resolve().relative_to(root)),sha256=native.digest(Path(__file__)))
    raw=(json.dumps(spec,indent=2)+'\n').encode();(root/native.INPUT).write_bytes(raw)
    path=root/'scripts/dev3_recovery_effects_native.py';text=path.read_text();text=re.sub(r"INPUT_SHA256='[^']+'",f"INPUT_SHA256='{hashlib.sha256(raw).hexdigest()}'",text);path.write_text(text)
    print(json.dumps(dict(status='INPUT_ONLY_NOT_RUN',mandatory=len(o.CASES),s5_matrix=len(o.S5_CASES),s5_additional=len(o.S5_CASES)-len(o.ALIASES),dependencies=len(spec['dependencies']),dependency_bytes=spec['dependency_bytes'],manifest_sha256=hashlib.sha256(raw).hexdigest())))


if __name__=='__main__':main()
