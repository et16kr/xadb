from pathlib import Path
import subprocess,json,hashlib,os
out=Path('module-gate/evidence/f3/r5');binary=Path('/tmp/xadb-f3-binary').read_text().strip();avail=subprocess.check_output([binary,'--list'],text=True)
cases=json.loads((out/'plan.json').read_text())['cases'];assert all(c+': test' in avail for c in cases)
identity={'XADB_CATALOG_SOURCE_SHA256':hashlib.sha256((out/'source-sha256.json').read_bytes()).hexdigest(),'XADB_CATALOG_ELF_SHA256':hashlib.sha256(Path(binary).read_bytes()).hexdigest(),'XADB_CATALOG_INPUT_SHA256':hashlib.sha256(Path('tests/fixtures/dev3-catalog-create/native3/input-plan.json').read_bytes()).hexdigest()};(out/'identity.json').write_text(json.dumps(identity,indent=2)+'\n');(out/'build.log').write_bytes(Path('/tmp/xadb-f3-build5.log').read_bytes())
results=[]
for i,c in enumerate(cases):
 try:
  p=subprocess.run([binary,'--exact',c,'--nocapture','--test-threads=1'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env={**os.environ,**identity,'RUST_MIN_STACK':'33554432'},timeout=60);log=p.stdout;code=p.returncode
 except subprocess.TimeoutExpired as e: log=e.stdout or b'';code='TIMEOUT'
 (out/f'case-{i:02}.log').write_bytes(log);results.append({'case':c,'exit':code});(out/'result.json').write_text(json.dumps(results,indent=2)+'\n');print(c.split('::')[-1],code,flush=True)
 if code!=0: break
