import os,subprocess,pathlib,json,hashlib,time,sys,resource
root=pathlib.Path('/workspace/xadb'); out=pathlib.Path('/workspace/scratch/f0-results')
elf=next(p for p in pathlib.Path('/workspace/scratch/f0-target/debug/deps').glob('engine-*') if p.is_file() and os.access(p,os.X_OK) and p.suffix=='')
evidence=json.loads((root/'devdoc/policies/module-gate.json').read_text())['evidence']
env=dict(os.environ,**evidence['env']); manifest={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','crates','tests','scripts','Cargo.toml','Cargo.lock','rust-toolchain.toml'],cwd=root,text=True).splitlines() if (root/p).is_file()}
if os.environ.get('F0_SOURCE_MANIFEST'):
 manifest=json.loads(pathlib.Path(os.environ['F0_SOURCE_MANIFEST']).read_text())
label=sys.argv[1]; selectors=sys.argv[2:]; source=hashlib.sha256(json.dumps(manifest,sort_keys=True).encode()).hexdigest(); binary=hashlib.sha256(elf.read_bytes()).hexdigest(); extra={k:v for k,v in env.items() if k.startswith('XADB_') and k not in evidence['env'] and not k.endswith('_SHA256')}; inp=hashlib.sha256(json.dumps(dict(selectors=selectors,extra_env=extra),sort_keys=True).encode()).hexdigest()
for key,val in [('source',source),('elf',binary),('input',inp)]:
 for name in evidence['identity_env'][key]: env[name]=val
(out/f'{label}-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
(out/f'{label}-input.json').write_text(json.dumps(dict(source=source,elf=binary,selectors=selectors,env={**{k:env[k] for k in evidence['env']},**extra},timeout=120),indent=2)+'\n')
resource.setrlimit(resource.RLIMIT_CORE,(0,0)); results=[]
for i,s in enumerate(selectors):
 log=out/f'{label}-{i:03d}.log'; started=time.monotonic()
 with log.open('wb') as f:
  try: rc=subprocess.run([str(elf),s,'--exact','--nocapture','--test-threads=1'],cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=120).returncode
  except subprocess.TimeoutExpired: rc='TIMEOUT'
 row=dict(selector=s,rc=rc,seconds=round(time.monotonic()-started,2),log=log.name,sha256=hashlib.sha256(log.read_bytes()).hexdigest());results.append(row)
 print(json.dumps(row),flush=True)
 (out/f'{label}-results.json').write_text(json.dumps(results,indent=2)+'\n')
