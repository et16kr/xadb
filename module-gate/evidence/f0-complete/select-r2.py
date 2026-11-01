import json,re,pathlib,subprocess
root=pathlib.Path('/workspace/xadb');out=pathlib.Path('/workspace/scratch/f0-results');m='storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
rows=json.loads((out/'baseline-results.json').read_text()); selectors=[r['selector'] for i,r in enumerate(rows) if i in [0,1,2,4,5,6,7,8,9,16,17,18]]
selectors.append(m+'plan4_pages::tests::g5_target_identity_boundaries')
text=(root/'crates/engine/src/storage/wal_io/mixed_system_owner/tests.rs').read_text()
# Changed snapshot helper is used by these scenarios; only paths with fatal
# observation or changed installer-cut observation are selected in this round.
for name,id,call in re.findall(r'scenario!\(\s*(\w+),\s*"([^"]+)",\s*(.*?)\n\);',text,re.S):
 if any(x in call for x in ['init_partial(', 'init_wal_failure(', 'bad_home(', 'final_home_drift(']) or (any(x in call for x in ['flush(', 'load_read_fault(', 'foreign_owner(', 'retired_continuation(']) and call.endswith('true)')):
  selectors.append(m+'status_init::status_flush::mixed::tests::'+name)
for name in re.findall(r'cut!\((\w+),', (root/'crates/engine/src/storage/wal_io/resident_status/tests.rs').read_text()):
 selectors.append(m+'status_init::status_flush::resident::tests::'+name)
assert len(selectors)==len(set(selectors))
subprocess.run(['python3',str(out/'run.py'),'r2',*selectors],check=True)
