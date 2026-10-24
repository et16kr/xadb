#!/usr/bin/env python3
"""Independent literal-table expansion, not a product/model/codec execution.

This script reads the frozen raw descriptors and already-authored selector literals,
then expands the explicit initial-state prefix/environment tables below. No state
transition, general owner selection, resolver, assembler, codec or Rust helper is
imported. Existing original inputs are copied byte-for-byte in their logical values.
Run only as static fixture preparation. Never call its output a runtime PASS.
"""
import copy
import hashlib
import json
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
ART = HERE.parent
RAW = ROOT / 'tests/fixtures/dev4-lifecycle-raw-selection'
ORIGINAL = json.loads((ART / 'assembler-oracle-proposal-v1.json').read_text())
DESCRIPTORS = json.loads((RAW / 'raw-descriptors.json').read_text())['cases']
SELECTORS = json.loads((RAW / 'selectors.json').read_text())['cases']
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
canon = lambda x: json.dumps(x,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()
# Hand-authored actor-scoped prospective symbols for this ONE fixed initial state.
C = [[5,0,255,1,1,1,0],[5,1,255,1,2,1,0]]
RECEIPT = [[8,0,255,1,1,1,0],[8,1,255,1,2,1,0]]
FACT = [11,255,255,1,0,1,0]
ANCHOR = [12,255,255,1,0,1,0]
FRESH_ENGINE = [1,255,255,2,0,2,0]
APIS = ['Prepare','BindAll','Execute','Fetch','CloseResult','CloseStatement','Begin','SetAutocommit','CommitAtomic','Rollback','CloseSession','ReopenSession','InspectStatement','InspectSession','SignalContext','ParkFetch','ResumeFetch','BeginCommit','CommitFact','FinishCommit','RecoveryConfirmedFreshEngine']
# No initial Issued token authenticates. Frozen sentinels are copied from the
# earlier INDEPENDENT literal baseline, never resolved through product code.
TARGET_KIND = {'BindAll':'Statement','Execute':'Statement','CloseStatement':'Statement','InspectStatement':'Statement','Fetch':'Result','CloseResult':'Result','ParkFetch':'Result','SignalContext':'Context'}
TARGETS = {(x['input']['kind'],x['input']['actor'],next(iter(x['input']['selector'])),next(iter(x['input']['selector'].values()))):x['expected_raw_tuple'] for x in SELECTORS if x['group']=='selector_baseline'}
assert len(TARGETS)==96
# Explicit context/control partition. Continuations are unmatched in initial.
FRESH = {'Prepare','BindAll','Execute','Fetch','CloseResult','CloseStatement','Begin','SetAutocommit','CommitAtomic','Rollback','CloseSession','ParkFetch','BeginCommit'}
ENTRY = {'Prepare':(1,2),'BindAll':(8,9),'Execute':(1,2),'Fetch':(3,4),'CloseResult':(1,2),'CloseStatement':(1,2),'Begin':(1,2),'SetAutocommit':(1,2),'CommitAtomic':(8,9),'Rollback':(2,3),'CloseSession':(2,3)}
# Bind A1 exact literal vectors. N0 and unauthenticated success/entry are empty.
VALUE_A={'tag':0,'byte_length':1,'bytes':[97]}
VALUE_B={'tag':0,'byte_length':1,'bytes':[98]}
BIND_N0 = [[],[],[],[{'tag':1,'byte_length':8,'bytes':[1,0,0,0,0,0,0,0]}],[{'tag':2,'byte_length':0,'bytes':[]}],[{'tag':0,'byte_length':2,'bytes':[97,97]}],[{'tag':0,'byte_length':1,'bytes':[255]}],[VALUE_A],[],[],[VALUE_A,VALUE_B]]
BIND_N1 = [[VALUE_A],[VALUE_B],[],BIND_N0[3],BIND_N0[4],BIND_N0[5],BIND_N0[6],[VALUE_A],[VALUE_A],[VALUE_A],[VALUE_A,VALUE_B]]
# Each row = kind, work_kind, work_class, cleanup, commit, receipt policy,
# anchor policy, fresh-engine policy. This table is ONLY the empty initial state:
# target/continuation txn None, no owners/O, no peer census, caller txn None.
# Policies 'new' and 'none' select the fixed literal symbols above, not history.
ENV = {
 'Execute': [
  ('Execution','Execute',0,255,255,'none',False,False),None,None,
  ('Execution','Execute',3,1,255,'none',False,False),
  ('Execution','Execute',4,1,255,'none',False,False),
  ('Execution','Execute',5,1,255,'none',False,False),
  ('Execution','Execute',6,1,255,'none',False,False),
  ('Execution','Execute',7,1,255,'none',False,False),
  ('Execution','Execute',8,2,1,'none',False,False),
  ('Execution','Execute',9,255,7,'none',False,False),
  ('Execution','Execute',10,255,8,'new',True,False)],
 'Fetch': [
  ('FetchWorkOrCleanup','Fetch',0,255,255,'none',False,False),
  ('FetchWorkOrCleanup','Fetch',1,1,255,'none',False,False),
  ('FetchWorkOrCleanup','Fetch',2,255,255,'none',False,False),None,None,
  ('FetchWorkOrCleanup','Fetch',5,1,255,'none',False,False),
  ('FetchWorkOrCleanup','Fetch',6,1,255,'none',False,False),
  ('FetchWorkOrCleanup','Fetch',7,1,255,'none',False,False),
  ('FetchWorkOrCleanup','Fetch',8,1,255,'none',False,False),
  ('FetchWorkOrCleanup','Fetch',9,2,255,'none',False,False)],
 'CloseResult':[('LocalCleanup','None',255,1,255,'none',False,False),None,None],
 'CloseStatement':[('LocalCleanup','None',255,1,255,'none',False,False),None,None],
 'CommitAtomic':[
  ('CommitAtomic','None',255,255,5,'new',False,False),
  ('CommitAtomic','None',255,255,6,'none',False,False),
  ('CommitAtomic','None',255,1,0,'none',False,False),
  ('CommitAtomic','None',255,2,1,'none',False,False),
  ('CommitAtomic','None',255,255,7,'none',False,False),
  ('CommitAtomic','None',255,1,0,'none',False,False),
  ('CommitAtomic','None',255,1,0,'none',False,False),
  ('CommitAtomic','None',255,255,8,'new',True,False),None,None],
 'Rollback':[('RollbackCleanup','None',255,1,0,'none',False,False),('RollbackCleanup','None',255,2,1,'none',False,False),None,None],
 'CloseSession':[('SessionCleanup','None',255,1,255,'none',False,False),('SessionCleanup','None',255,2,1,'none',False,False),None,None],
 'ResumeFetch':[
  ('FetchWorkOrCleanup','Resume',0,1,255,'none',False,False),('FetchWorkOrCleanup','Resume',0,2,255,'none',False,False),
  ('FetchWorkOrCleanup','Resume',1,1,255,'none',False,False),('FetchWorkOrCleanup','Resume',1,2,255,'none',False,False),
  ('FetchWorkOrCleanup','Resume',2,1,255,'none',False,False),('FetchWorkOrCleanup','Resume',2,2,255,'none',False,False),
  ('FetchWorkOrCleanup','Resume',3,1,255,'none',False,False),('FetchWorkOrCleanup','Resume',3,2,255,'none',False,False),
  ('FetchWorkOrCleanup','Resume',4,1,255,'none',False,False),('FetchWorkOrCleanup','Resume',4,2,255,'none',False,False)],
 'CommitFact':[
  ('CommitProgress','None',255,255,2,'none',False,False),('CommitProgress','None',255,255,3,'new',True,False),
  ('CommitProgress','None',255,255,4,'none',False,False),('CommitProgress','None',255,255,5,'new',False,False),
  ('CommitProgress','None',255,255,6,'none',False,False)],
 'FinishCommit':[
  ('CommitFinish','None',255,255,4,'none',False,False),('CommitFinish','None',255,1,0,'none',False,False),
  ('CommitFinish','None',255,2,1,'none',False,False),('CommitFinish','None',255,255,7,'none',False,False),
  ('CommitFinish','None',255,255,8,'none',False,False)],
 'RecoveryConfirmedFreshEngine':[
  ('RecoveryConfirmed','None',255,4,255,'none',False,True),('RecoveryConfirmed','None',255,4,255,'none',False,True),
  ('RecoveryConfirmed','None',255,4,255,'none',False,True),('RecoveryConfirmed','None',255,4,255,'none',False,True),
  ('RecoveryConfirmed','None',255,4,255,'none',False,True)]}

def initial_literal(action):
    # Finite table expansion with no pre-state argument: not an assembler oracle
    # algorithm capable of computing arbitrary state-dependent expectations.
    api,actor,outcome=action['api'],action['actor'],action['outcome']
    result={'schema_version':1,'api':api,'api_tag':APIS.index(api),'actor':actor,
      'outcome_tag':255 if outcome is None else outcome,'prepare_slot':255,'prepare_shape':255,'parameter_count':255,'set_autocommit':255,
      'target':None,'context':copy.deepcopy(C[actor]) if api in FRESH else None,
      'original_deadline':'Absent','supplied_cancel':False,'deadline_reached':False,'bind_values':[],'environment':None}
    if api=='Prepare':
      result.update(prepare_slot=action['prepare']['slot'],prepare_shape={'Select':0,'Dml':1}[action['prepare']['shape']],parameter_count=action['prepare']['parameter_count'])
    if api=='SetAutocommit':result['set_autocommit']={'On':0,'Off':1}[action['autocommit']]
    if api in TARGET_KIND:
      s=action['selector'];result['target']=copy.deepcopy(TARGETS[(TARGET_KIND[api],actor,s['kind'],s['index'])])
    if api in ENTRY:
      cancelled,timedout=ENTRY[api]
      if outcome==cancelled:result['supplied_cancel']=True
      if outcome==timedout:result.update(original_deadline='ReachedAtEntry',deadline_reached=True)
    if api in ['ParkFetch','BeginCommit'] and outcome==1:result['original_deadline']='FutureAtEntry'
    if api=='BindAll':result['bind_values']=copy.deepcopy(BIND_N0[outcome])
    row=ENV[api][outcome] if api in ENV else None
    if row is not None:
      kind,work,workclass,cleanup,commit,receipt,anchor,fresh=row
      result['environment']={'envelope_schema':1,'fact':copy.deepcopy(FACT),'kind':kind,'work_kind':work,'work_class':workclass,'cleanup':cleanup,'commit_class':commit,'reserved0':0,'transaction':None,'receipt':copy.deepcopy(RECEIPT[actor]) if receipt=='new' else None,'anchor':copy.deepcopy(ANCHOR) if anchor else None,'fresh_engine':copy.deepcopy(FRESH_ENGINE) if fresh else None,'obligations':[],'owners':[]}
    return result

def original_literal(pair):
    x=copy.deepcopy(pair['original_prefix_source_view'])
    x['environment']=copy.deepcopy(ORIGINAL['original_envelopes_source_view'][pair['environment_ref'].split('/',1)[-1]]) if pair['environment_ref'] else None
    return x

def write_new(name,obj):
    path=HERE/name
    if path.exists():raise RuntimeError('preserve existing candidate before correction: '+str(path))
    path.write_text(json.dumps(obj,ensure_ascii=False,indent=2)+'\n')
    return {'path':name,'sha256':sha(path),'bytes':path.stat().st_size}

if __name__=='__main__':
    initial=[]
    for row in DESCRIPTORS:
      i=row['index'];initial.append({'id':f'asm/initial/{i:04}','pre_state':'original:initial','raw_index':i,'raw_action':row['expected_action'],'expected_input':initial_literal(row['expected_action'])})
    original=[];links=[]
    for p in ORIGINAL['unique_original_pairs']:
      idx=p['raw_index_hand_selected'];expected=original_literal(p)
      if p['pre_state_ref']=='states/initial':
        assert expected==initial[idx]['expected_input'],p['id']
        links.append({'original_pair':p['id'],'case':initial[idx]['id'],'pair_sha256':p['pre_input_pair_sha256']});continue
      original.append({'id':'asm/original/'+p['id'],'pre_state':'original:'+p['pre_state_ref'].split('/',1)[1],'raw_index':idx,'raw_action':DESCRIPTORS[idx]['expected_action'],'expected_input':expected,'original_pair_sha256':p['pre_input_pair_sha256']})
    extra=ORIGINAL['seven_original_byte_vector_links'][-1]
    pair=next(p for p in ORIGINAL['unique_original_pairs'] if p['id']=='sel.stale')
    expected=original_literal(pair);expected['target']=copy.deepcopy(extra['literal_put']['/target'])
    original.append({'id':'asm/original/input-malformed-child-u9','pre_state':'original:sel.reexecuted','raw_index':902,'raw_action':DESCRIPTORS[902]['expected_action'],'expected_input':expected,'original_pair_sha256':extra['pre_input_pair_sha256']})
    assert len(initial)==1533 and len(original)==27 and len(links)==3
    # New A1 rich examples: one existing N0 state and one explicit N1 read-set.
    rich=[]
    for n,pre in [(0,'original:sel.prepared'),(1,'rich:n1-bound')]:
      for outcome in range(11):
        idx=48+outcome;x=initial_literal(DESCRIPTORS[idx]['expected_action'])
        x['target']=[3,0,0,1,1,1,1];x['context']=[5,0,255,1,1,2,0]
        x['bind_values']=copy.deepcopy((BIND_N0,BIND_N1)[n][outcome])
        rich.append({'id':f'asm/bind-n{n}/{idx:04}','pre_state':pre,'raw_index':idx,'raw_action':DESCRIPTORS[idx]['expected_action'],'expected_input':x})
    prestate={'id':'rich:n1-bound','base':'original:sel.prepared','put':{'/issued_statements/1/parameter_count':1,'/issued_statements/1/metadata':'SelectN1Utf8NotNullMax1','/bindings/1/value':'A61'},'classification':'exact assembler read-set variation; not a full model reachability claim'}
    common={'status':'INDEPENDENT_LITERAL_CANDIDATE_NOT_D2_FROZEN_NOT_RUN','authorship':'hand-authored fixed-initial tables and copied original logical literals; no product/model/codec output','logical_input_schema':'same input logical fields as original vectors; nonempty bind value {tag:u8,byte_length:u16,bytes:[u8]} with exact payload length'}
    files=[]
    files.append(write_new('initial.json',dict(common,pre_state='original:initial',cases=initial)))
    files.append(write_new('original-links.json',dict(common,cases=original,overlap_links=links,all_51_occurrences=ORIGINAL['all_51_occurrences'],original_vectors=ORIGINAL['seven_original_byte_vector_links'])))
    files.append(write_new('rich.json',dict(common,prestates=[prestate],cases=rich)))
    source_pins=[{'path':str(p.relative_to(ROOT)),'sha256':sha(p)} for p in [RAW/'raw-descriptors.json',RAW/'selectors.json',ART/'assembler-oracle-proposal-v1.json',ART/'assembler-owner-scope-v2.md',ART/'review-assembler-design-v1.md',ROOT/'tests/fixtures/dev4-lifecycle-model/literals.json']]
    manifest=dict(common,id='D4-LM-ASSEMBLER-NORMAL-CANDIDATE-001',baseline_revision='1dd7bed1fc03bead2158fba439a06673dff29158',counts={'initial':1533,'original_nonoverlap':27,'original_overlap_links':3,'rich_bind':22,'unique_proposed_calls_materialized':1582,'product_calls':0,'new_error_literals':0},files=files+[{'path':'author-normal.py','sha256':sha(Path(__file__)),'bytes':Path(__file__).stat().st_size}],source_pins=source_pins,pending=['Further rich-state A2-A5 normal inputs from inventory; use a new version/append-only file with correction history.','Conditional read-set and compound-error table; error/cap/poison literals remain unwritten.','Independent review of all new literal tables and actual input schema before final D2.','Future observation mutation inventory and exact field-comparison harness; no automatic product mutant loop.'],execution='Static authoring only; no product or old test calls; no full model replay or BFS claim.')
    receipt=write_new('manifest.json',manifest)
    print(json.dumps({'manifest':receipt,'counts':manifest['counts'],'files':files},ensure_ascii=False,indent=2))
