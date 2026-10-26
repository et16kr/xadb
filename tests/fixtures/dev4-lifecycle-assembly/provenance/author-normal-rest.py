#!/usr/bin/env python3
"""Explicit remaining normal matrices; no production/state-transition oracle.

Case IDs select the fixed hand-authored symbols and rows below. The script cannot
assemble an arbitrary State. Pre-state variations are separately declared.
"""
import copy,importlib.util,json
from pathlib import Path
H=Path(__file__).resolve().parent
sp=importlib.util.spec_from_file_location('initial_tables',H/'author-normal.py');a=importlib.util.module_from_spec(sp);sp.loader.exec_module(a)
cp=copy.deepcopy
T=[6,0,255,1,1,1,0];O=[10,0,255,1,1,1,0];P=[3,0,0,1,1,1,1];P2=[3,0,1,1,1,2,1];R=[4,0,0,1,1,1,1];RS=[4,0,1,1,1,1,1]
Q=[8,0,255,1,1,1,0];AN=[12,255,255,1,0,1,0]
def owner(n,k,d,b,af,o=None):return {'resource':[9,0,255,1,1,n,0],'resource_kind':k,'after_disposition':d,'obligation':o,'before_holder':b,'after_holder':af}
def ob(dec,s,n):return {'obligation':cp(O),'known_decision':dec,'after_ownership_summary':s,'associated_owner_count':n}
RWREL=[owner(1,'Undo','Released',T,None),owner(2,'Locks','Released',T,None),owner(3,'AbortReserve','Released',T,None)]
CWREL=[owner(3,'Snapshot','Released',R,None),owner(4,'Scratch','Released',R,None),owner(5,'Temp','Released',R,None)]
CSREL=[owner(8,'Snapshot','Released',RS,None),owner(9,'Scratch','Released',RS,None),owner(10,'Temp','Released',RS,None)]
PLANREL=[owner(1,'Plan','Released',P,None),owner(2,'Binding','Released',P,None)]
FENCE=[owner(1,'Undo','Held',T,T,O),owner(2,'Locks','Held',T,T,O),owner(3,'AbortReserve','Held',T,T,O),owner(6,'Recovery','Held',None,T,O)]
FENCE11=[owner(1,'Undo','Held',T,T,O),owner(2,'Locks','Held',T,T,O),owner(3,'AbortReserve','Held',T,T,O),owner(11,'Recovery','Held',None,T,O)]
OFFREL=[owner(3,'Undo','Released',T,None),owner(4,'Locks','Released',T,None),owner(5,'AbortReserve','Released',T,None)]
OFFFENCE=[owner(3,'Undo','Held',T,T,O),owner(4,'Locks','Held',T,T,O),owner(5,'AbortReserve','Held',T,T,O),owner(6,'Recovery','Held',None,T,O)]
pre=[];cases=[]
existing={x['id'] for f in ['initial.json','original-links.json','rich.json','rich-normal-2.json','rich-continuation-3.json'] for x in json.loads((H/f).read_text())['cases']}
inventory=json.loads((H.parent/'assembler-inventory-proposal-v2.json').read_text())
rootgroups={'new_error_integration','local_relations','prospective_boundaries','readset_controls'}
missing=[x for x in inventory['cases'] if x['group'] not in rootgroups and x['id'] not in existing]
# Only explicit fields are used for each fixed pre-state source.
CTXFACT={'original:sel.open':(3,2),'original:sel.eof':(4,3),'original:sel.closed':(4,3),'original:sel.reexecuted':(5,4),'original:rw.select_open':(6,3),'original:rw.begun':(2,1),'original:rw.written':(4,2),'original:rw.recovery_pending':(5,3),'original:sel.prepared':(2,1),'original:sel.parked':(4,2),
'rich:existing-implicit-rw-fenced':(5,3),'rich:existing-off-origin-caller-on':(4,2),'rich:current-invalid-statement':(2,1),'rich:current-closed-statement':(4,3),'rich:parked-fetch-fenced-retired':(4,2),'rich:recovery-transferred':(5,3),'rich:eleven-owner-fenced-before-close':(6,3)}
MAP={'plan:existing-implicit-rw-fenced':'rich:existing-implicit-rw-fenced','plan:existing-off-origin-caller-on':'rich:existing-off-origin-caller-on','plan:current-invalid-statement':'rich:current-invalid-statement','plan:current-closed-statement':'rich:current-closed-statement','plan:parked-fetch-fenced-retired':'rich:parked-fetch-fenced-retired','plan:recovery-transferred':'rich:recovery-transferred','plan:peer-existing-obligation':'original:rw.recovery_pending','plan:eleven-owner-fenced-before-close':'rich:eleven-owner-fenced-before-close'}
for phase in ['BeforeAttempt','Attempted','DurableConfirmed','Published','NoWriteValidated']:MAP['plan:peer-'+phase]='rich:peer-'+phase;CTXFACT['rich:peer-'+phase]=(2,1)
for item in missing:
    ident=item['id']; idx=item['raw_index'];ps=MAP.get(item['pre_state'],item['pre_state']);group=ident.split('/')[1]
    action=a.DESCRIPTORS[idx]['expected_action'];x=a.initial_literal(action);ctx,fact=CTXFACT[ps]
    actor=action['actor'];outcome=action['outcome'];e=x['environment']
    if x['context'] is not None:x['context']=[5,actor,255,1,actor+1,ctx,0]
    if e is not None:e['fact']=[11,255,255,1,0,fact,0]
    if group.startswith('fetch-'):
      x['target']=cp(RS if group=='fetch-explicit' else R)
      if e is not None:
        e['transaction']=None if group in ['fetch-retired','fetch-foreign'] else cp(T)
        if group=='fetch-open' and outcome in [1,5,6,7,8]:e['owners']=cp(CWREL)
        if group=='fetch-explicit':
          if outcome in [1,5,6,7]:e['owners']=cp(CSREL)
          if outcome==8:e.update(commit_class=0,owners=cp(RWREL+CSREL))
          if outcome==9:e.update(commit_class=1,owners=cp(FENCE11),obligations=[ob('NotCommittedRecoveryRequired','AllHeld',4)])
    elif group=='close-open':
      x['target']=cp(R if action['api']=='CloseResult' else P)
      if e is not None:e.update(transaction=cp(T),owners=cp(CWREL if action['api']=='CloseResult' else PLANREL+CWREL))
    elif group=='close-stmt-closed':x['target']=cp(P);e['transaction']=cp(T)
    elif group=='invalid-stmt':
      x['target']=cp(P)
      if e is not None:
        if idx==400:e.update(transaction=cp(T),owners=[owner(3,'Snapshot','Held',None,R),owner(4,'Scratch','Held',None,R),owner(5,'Temp','Held',None,R)])
        if idx==408:e['transaction']=cp(T)
        if idx==1168:e['owners']=cp(PLANREL)
    elif group=='inspect':
      if action['api']=='InspectStatement':x['target']=cp(P if idx==1322 else P2)
    elif group.startswith('commit-'):
      if e is not None:
        e['transaction']=cp(T)
        if group!='commit-ro':
          if outcome in [0,1,2,5,6]:e['owners']=cp(RWREL)
          if outcome in [3,4,7]:e.update(owners=cp(FENCE if group=='commit-changed' else [owner(1,'Undo','Held',T,T,O),owner(2,'Locks','Held',T,T,O),owner(3,'AbortReserve','Held',T,T,O),owner(4,'Recovery','Held',None,T,O)]),obligations=[ob({3:'NotCommittedRecoveryRequired',4:'Unknown',7:'Committed'}[outcome],'AllHeld',4)])
          if group=='commit-changed' and outcome==0:e.update(commit_class=4,receipt=cp(Q),anchor=cp(AN))
    elif group=='rollback-ro':
      if e is not None:e['transaction']=cp(T)
      if outcome==0:e['owners']=cp(CWREL)
    elif group=='rollback-explicit':pass # only entry classes; no Env.
    elif group=='close-session-open':
      if e is not None:e.update(transaction=cp(T),owners=cp(FENCE11),obligations=[ob('NotCommittedRecoveryRequired','AllHeld',4)])
    elif group=='close-session-fenced':
      if e is not None:e.update(transaction=cp(T),owners=[],obligations=[ob('NotCommittedRecoveryRequired','None',0)])
    elif group=='execute-existing-off-mode-on':
      x['target']=cp(P)
      if e is not None:
        e['transaction']=cp(T)
        if outcome==7:e.update(commit_class=0,owners=cp(OFFREL))
        if outcome in [8,9,10]:e.update(owners=cp(OFFFENCE),obligations=[ob({8:'NotCommittedRecoveryRequired',9:'Unknown',10:'Committed'}[outcome],'AllHeld',4)])
    elif group=='execute-existing-implicit':
      x['target']=cp(P);e['transaction']=cp(T)
      if outcome==7:
        r=cp(RWREL)
        for row in r:row['obligation']=cp(O)
        e.update(commit_class=0,owners=r,obligations=[ob('NotCommittedRecoveryRequired','AllReleased',3)])
      else:e['obligations']=[ob({8:'NotCommittedRecoveryRequired',9:'Unknown',10:'Committed'}[outcome],'None',0)]
    elif group=='resume-private-retired':
      x.update(context=[5,0,255,1,1,3,0],original_deadline='FutureAtEntry');e['transaction']=cp(T)
      if idx==1490:e['owners']=cp(CWREL)
    elif group=='signal-completed':x['target']=[5,0,255,1,1,2,0]
    elif group=='signal-parked':x['target']=[5,0,255,1,1,3,0]
    elif group=='begin-commit':pass # no txn/environment; context exact already.
    elif group.startswith('peer-') and group!='peer-existing-source':
      phase=group[len('peer-'):]
      if phase not in ['Published','NoWriteValidated']:
        pt=[6,1,255,1,2,1,0];po=[10,1,255,1,2,1,0]
        e['owners']=[{'resource':[9,1,255,1,2,n,0],'resource_kind':kind,'after_disposition':'Held','obligation':cp(po),'before_holder':cp(pt) if n<4 else None,'after_holder':cp(pt)} for n,kind in [(1,'Undo'),(2,'Locks'),(3,'AbortReserve'),(4,'Recovery')]]
        e['obligations']=[{'obligation':po,'known_decision':{'BeforeAttempt':'NotAttempted','Attempted':'Unknown','DurableConfirmed':'Committed'}[phase],'after_ownership_summary':'AllHeld','associated_owner_count':4}]
    elif group=='peer-existing-source':pass # owner closure: unchanged peer O is omitted.
    elif group=='handoff-idempotence':
      released=action['api']=='RecoveryConfirmedFreshEngine';r=[owner(1,'Undo','Released' if released else 'Transferred',O,None if released else O,O),owner(2,'Locks','Released' if released else 'Transferred',O,None if released else O,O),owner(3,'AbortReserve','Released' if released else 'Transferred',O,None if released else O,O),owner(6,'Recovery','Released' if released else 'Transferred',O,None if released else O,O)]
      if not released:r=r[:3]+[owner(4,'Plan','Released',P,None),owner(5,'Binding','Released',P,None)]+r[3:];e.update(transaction=cp(T),cleanup=3)
      e['owners']=r;v=ob('NotCommittedRecoveryRequired','AllReleased' if released else 'AllTransferred',4)
      if not released or outcome==0:e['obligations']=[v]
      elif outcome==1:e['obligations']=[]
      elif outcome==2:w=cp(v);w['obligation'][5]=255;e['obligations']=[v,w]
      elif outcome==3:v['obligation'][3]=255;e['obligations']=[v]
      else:e['obligations']=[v,cp(v)]
    elif group=='owner':
      e.update(transaction=cp(T),cleanup=3,commit_class=255,owners=[owner(1,'Undo','Transferred',T,O,O),owner(2,'Locks','Transferred',T,O,O),owner(3,'AbortReserve','Transferred',T,O,O),owner(4,'Plan','Released',P,None),owner(5,'Binding','Released',P,None),owner(6,'Plan','Released',P2,None),owner(7,'Binding','Released',P2,None),owner(8,'Snapshot','Released',R,None),owner(9,'Scratch','Released',R,None),owner(10,'Temp','Released',R,None),owner(11,'Recovery','Transferred',T,O,O)],obligations=[ob('NotCommittedRecoveryRequired','AllTransferred',4)])
    else:raise AssertionError(ident)
    cases.append({'id':ident,'pre_state':ps,'raw_index':idx,'raw_action':action,'expected_input':x})
# Complete explicit delta recipes for new selected readsets.
def prestate(id,base,put,classification='assembler selected read-set variation; not complete model-history/reachability evidence'):
 pre.append({'id':id,'base':base,'put':put,'classification':classification})
prestate('rich:existing-off-origin-caller-on','original:off.written',{'/actors/0/autocommit':'On'})
prestate('rich:current-invalid-statement','original:sel.prepared',{'/prepared_slots/0:0/state':'Invalid'})
prestate('rich:current-closed-statement','original:sel.closed',{'/prepared_slots/0:0/state':'Closed','/issued_statements/1/life':'CurrentClosed','/owners/1/disposition':'Released','/owners/1/current_holder':None,'/owners/1/previous_holder':P,'/owners/2/disposition':'Released','/owners/2/current_holder':None,'/owners/2/previous_holder':P})
prestate('rich:parked-fetch-fenced-retired','original:sel.parked',{'/engines/1/admission':'Fenced','/issued_results/1/public_generation_live':False,'/issued_results/1/retirement':'EngineFence','/issued_results/1/retirement_context':[5,0,255,1,1,3,0],'/private_descriptors/1/public_generation_live':False,'/private_descriptors/1/blocked_by_fence':True,'/prepared_slots/0:0/next_result_generation':3,'/actors/0/active_public_result':None})
p={}
for n in [1,2,3,6]:p.update({f'/owners/{n}/disposition':'Transferred',f'/owners/{n}/current_holder':O,f'/owners/{n}/previous_holder':T,f'/owners/{n}/transfer_count':1})
p['/obligations/1/disposition']='AllTransferred';prestate('rich:recovery-transferred','original:rw.recovery_pending',p)
for phase,decision in [('BeforeAttempt','NotAttempted'),('Attempted','Unknown'),('DurableConfirmed','Committed'),('Published','Committed'),('NoWriteValidated','Committed')]:
 p={'/transactions/1/progress':phase,'/transactions/1/decision':decision}
 if phase in ['Published','NoWriteValidated']:
  p.update({'/transactions/1/life':'Ended','/actors/1/transaction':None})
  for n in [1,2,3]:p[f'/owners/{n}/disposition']='Released';p[f'/owners/{n}/current_holder']=None;p[f'/owners/{n}/previous_holder']=[6,1,255,1,2,1,0]
 prestate('rich:peer-'+phase,'rich:peer-rw-unattempted',p)
# Hand-authored five-action prefix: Begin, Prepare Select slot0, Prepare Dml
# slot1, Execute Select slot0, Rollback Incomplete. Source record edits are
# complete logical history values; no expected successor is generated by model.
C3=[5,0,255,1,1,3,0];C4=[5,0,255,1,1,4,0];C5=[5,0,255,1,1,5,0];F1=[11,255,255,1,0,1,0];F2=[11,255,255,1,0,2,0]
p={'/next_nonce/Resource':12,'/next_nonce/Obligation':2,'/actors/0/active_public_result':None,'/prepared_slots/0:0/state':'ResultOpen','/prepared_slots/0:0/descriptor_result':R,'/prepared_slots/0:0/next_result_generation':3,'/prepared_slots/0:1/state':'ReadyBound','/prepared_slots/0:1/descriptor_result':None,'/prepared_slots/0:1/next_result_generation':1,'/issued_statements/1/shape':'Select','/issued_statements/1/metadata':'SelectN0','/issued_statements/2/shape':'Dml','/issued_statements/2/metadata':'DmlN0','/issued_statements/2/creation_context':C3,'/bindings/2/creation_context':C3,'/issued_results/1/id':R,'/issued_results/1/statement':P,'/issued_results/1/creation_context':C4,'/issued_results/1/retirement':'EngineFence','/issued_results/1/public_generation_live':False,'/issued_results/1/retirement_context':C5,'/private_descriptors/1/result':R,'/private_descriptors/1/statement':P,'/private_descriptors/1/public_generation_live':False,'/private_descriptors/1/blocked_by_fence':True,'/transactions/1/prior_successful_write':False,'/transactions/1/life':'RecoveryPending','/transactions/1/decision':'NotCommittedRecoveryRequired','/transactions/1/client_knowledge':'KnownNotCommittedRecoveryRequired','/transactions/1/obligation':O,'/transactions/1/completion_fact':F2,'/engines/1/admission':'Fenced','/engines/1/first_cause':'UndoIncomplete','/engines/1/known_decision':'NotCommittedRecoveryRequired','/engines/1/cause_transaction':T,'/engines/1/first_fence_context':C5}
for n in [6,7]:p[f'/owners/{n}/birth_context']=C3
for n in [8,9,10]:p[f'/owners/{n}/original_holder']=R;p[f'/owners/{n}/current_holder']=R;p[f'/owners/{n}/birth_context']=C4;p[f'/owners/{n}/last_fact']=F1
p['/owners/11']={'id':[9,0,255,1,1,11,0],'resource_kind':'Recovery','disposition':'Held','transfer_count':0,'original_holder':T,'current_holder':T,'previous_holder':None,'birth_context':C5,'last_fact':F2}
p['/obligations/1']={'id':O,'engine':[1,255,255,1,0,1,0],'transaction':T,'origin_session':[2,0,255,1,1,1,0],'creation_reason':'KnownNotCommittedUndoPending','latest_reason':'KnownNotCommittedUndoPending','reason_mask':1,'disposition':'AllHeld','known_decision':'NotCommittedRecoveryRequired','settlement':'Outstanding','first_fact':F2,'latest_fact':F2,'recovery_fact':None}
p['/facts/1']={'id':F1,'kind':'Execution','api':'Execute','outcome_class':0,'cleanup':'NotConsumed','decision':'NotAttempted','delivery':'NotApplicable','next_work':'NotConsumed','consumed_parts':1,'engine':[1,255,255,1,0,1,0],'obligation_count':0,'owner_count':3,'context':C4,'transaction':T,'receipt':None,'fresh_engine':None,'anchor':None}
p['/facts/2']={'id':F2,'kind':'RollbackCleanup','api':'Rollback','outcome_class':1,'cleanup':'Incomplete','decision':'NotCommittedRecoveryRequired','delivery':'NotApplicable','next_work':'NotConsumed','consumed_parts':2,'engine':[1,255,255,1,0,1,0],'obligation_count':1,'owner_count':4,'context':C5,'transaction':T,'receipt':None,'fresh_engine':None,'anchor':None}
p['/fact_owners/1']=[{'resource':[9,0,255,1,1,n,0],'disposition':'Held','obligation':None,'before_holder':None,'after_holder':R} for n in [8,9,10]]
p['/fact_owners/2']=[{'resource':[9,0,255,1,1,n,0],'disposition':'Held','obligation':O,'before_holder':T if n!=11 else None,'after_holder':T} for n in [1,2,3,11]]
p['/fact_obligations/2']=[{'obligation':O,'known_decision':'NotCommittedRecoveryRequired','disposition':'AllHeld','owner_count':4}]
prestate('rich:eleven-owner-fenced-before-close','original:rw.select_open',p,'hand-authored candidate five-action prefix, still requires independent full-field/history review; no model/runtime/global-maximum claim')
assert len(cases)==130 and len({x['id'] for x in cases})==130
for c in cases:
 e=c['expected_input']['environment']
 if e is not None:assert len(e['owners'])==len({tuple(x['resource']) for x in e['owners']})
f=a.write_new('rich-normal-rest-4.json',{'status':'INDEPENDENT_NORMAL_LITERAL_CANDIDATE_NOT_D2_FROZEN_NOT_RUN','cases':cases,'prestates':pre,'owner_closure':'unchanged existing peer O omitted; cause update retains count0 O; read-set v3 CommitFact correction applies in prior part3','execution':'static authoring only'})
m={'status':'CANDIDATE_NOT_RUN','new_cases':130,'cumulative_unique_cases':1886,'files':[f,{'path':'author-normal-rest.py','sha256':a.sha(Path(__file__)),'bytes':Path(__file__).stat().st_size}],'preserved_manifests':[{'path':n,'sha256':a.sha(H/n)} for n in ['manifest.json','manifest-part2.json','manifest-part3.json']],'execution':'no product/model/codec/test calls'}
print(json.dumps({'data':f,'manifest':a.write_new('manifest-part4.json',m)},indent=2))
