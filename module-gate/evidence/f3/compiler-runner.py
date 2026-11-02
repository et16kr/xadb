import json,subprocess,shutil,tempfile,hashlib,os
from pathlib import Path
root=Path.cwd();out=root/'module-gate/evidence/f3/compiler';out.mkdir(parents=True,exist_ok=True)
source_path='crates/engine/src/storage/catalog/read.rs'
base=(root/source_path).read_text()
params="mut scope: CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>, private: &mut [u8;425_984], object: ObjectId, cursor: &mut Plan4ResidencyCursor"
start=f'\n#[allow(dead_code)]\nfn f3_borrow_probe<W: WriterNamespace>({params}) {{\n'
found='let step = lookup(scope.operation(private).unwrap(), object, cursor).unwrap();\nif let CatalogLookupStep::Found(record) = step { let copied = record;\n'
end='\n} else { scope.finish_checked().unwrap(); }\n}\n'
probes={
 'consume-before-close':(start+found+'std::hint::black_box(copied.header()); scope.finish_checked().unwrap();'+end,None),
 'close-before-copy-use':(start+found+'scope.finish_checked().unwrap(); std::hint::black_box(copied.header());'+end,'E0505'),
 'operation-reborrow-before-copy-use':(start+found+'let _again=scope.operation(private); std::hint::black_box(copied.header());'+end,'E0499'),
 'raw-candidate':(start+'let _ = CatalogCandidate { object, len: 0, header: None };\n}\n','E0451'),
 'raw-table-issuer':(start+'let _ = crate::storage::disk::CheckedTable::issue;\n}\n','E0624'),
 'production-legacy-unavailable':(start+'let _ = scope.legacy_operation(private);\n}\n','E0599'),
}
plan={'scope':'new checked operation loan and private authority only; compiler probes, no product test execution','base_read_sha256':hashlib.sha256(base.encode()).hexdigest(),'cases':{k:v[1] for k,v in probes.items()}}
(out/'plan.json').write_text(json.dumps(plan,indent=2)+'\n')
results=[]
with tempfile.TemporaryDirectory(prefix='xadb-f3-compiler-') as tmp:
 work=Path(tmp)/'source';work.mkdir()
 for name in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','crates','tests','scripts','.github']:
  src=root/name
  if src.is_dir():shutil.copytree(src,work/name,ignore=shutil.ignore_patterns('__pycache__'))
  elif src.exists():shutil.copy2(src,work/name)
 env={**os.environ,'CARGO_TARGET_DIR':str(Path(tmp)/'target'),'CARGO_INCREMENTAL':'0'}
 for name,(probe,expected) in probes.items():
  (work/source_path).write_text(base+probe);(out/(name+'.rs')).write_text(probe)
  command=['cargo','check','-p','xadb-engine','--lib','--locked','--message-format=json']
  p=subprocess.run(command,cwd=work,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=180)
  (out/(name+'.jsonl')).write_text(p.stdout);(out/(name+'.log')).write_text(p.stderr)
  diagnostics=[json.loads(l)['message'] for l in p.stdout.splitlines() if l.startswith('{') and json.loads(l).get('reason')=='compiler-message' and json.loads(l)['message'].get('level')=='error']
  codes=[d.get('code',{}).get('code') if d.get('code') else None for d in diagnostics]
  matched=expected in codes and any(expected==(d.get('code') or {}).get('code') and any(s['file_name']==source_path and s['line_start']>len(base.splitlines()) for s in d.get('spans',[])) for d in diagnostics)
  ok=(p.returncode==0 and not diagnostics) if expected is None else (p.returncode!=0 and matched)
  results.append({'case':name,'expected':expected,'exit':p.returncode,'codes':codes,'status':'PASS' if ok else 'FAIL'});(out/'result.json').write_text(json.dumps(results,indent=2)+'\n');print(name,results[-1],flush=True)
  if not ok:break
(out/'cleanup.json').write_text(json.dumps({'temporary_export_and_target':'removed'})+'\n')
