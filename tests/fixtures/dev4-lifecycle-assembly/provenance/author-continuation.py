#!/usr/bin/env python3
"""Hand-authored finite commit phase/receipt matrix; no arbitrary-state oracle.

Pre-state variations are explicit selected read-set fixtures. They are not model
replay outputs and do not claim complete history or normal BFS reachability.
"""
import copy,importlib.util,json
from pathlib import Path
H=Path(__file__).resolve().parent
sp=importlib.util.spec_from_file_location('initial_tables',H/'author-normal.py');a=importlib.util.module_from_spec(sp);sp.loader.exec_module(a)
cp=copy.deepcopy
T=[6,0,255,1,1,1,0];E=[1,255,255,1,0,1,0];C4=[5,0,255,1,1,4,0];C5=[5,0,255,1,1,5,0];O=[10,0,255,1,1,1,0];Q1=[8,0,255,1,1,1,0];A1=[12,255,255,1,0,1,0]
def row(n,k,d,b,af,o=None):return {'resource':[9,0,255,1,1,n,0],'resource_kind':k,'after_disposition':d,'obligation':o,'before_holder':b,'after_holder':af}
def obl(decision,summary,count):return {'obligation':cp(O),'known_decision':decision,'after_ownership_summary':summary,'associated_owner_count':count}
RELEASE=[row(1,'Undo','Released',T,None),row(2,'Locks','Released',T,None),row(3,'AbortReserve','Released',T,None)]
FENCE=[row(1,'Undo','Held',T,T,O),row(2,'Locks','Held',T,T,O),row(3,'AbortReserve','Held',T,T,O),row(6,'Recovery','Held',None,T,O)]
DRAIN=[row(1,'Undo','Released',T,None,O),row(2,'Locks','Released',T,None,O),row(3,'AbortReserve','Released',T,None,O),row(6,'Recovery','Released',T,None,O)]
HANDOFF=[row(1,'Undo','Transferred',T,O,O),row(2,'Locks','Transferred',T,O,O),row(3,'AbortReserve','Transferred',T,O,O),row(6,'Recovery','Transferred',T,O,O)]
def parked(c):return {'id':cp(c),'issue_ordinal':c[5],'api':'BeginCommit','status':'ParkedCommit','original_deadline':'FutureAtEntry','cancel_supplied_or_signalled':False,'deadline_reached':False,'cancel_observed_at_completion':False,'deadline_observed_at_completion':False,'completion_phase':'NotCompleted','statement':None,'result':None,'transaction':cp(T)}
def receipt(kind,anchor):return {'id':cp(Q1),'transaction':cp(T),'engine':cp(E),'kind':kind,'delivery':'NotDelivered','anchor':cp(anchor),'source_fact':None,'delivery_context':None}
# Name, source progress, decision, existing receipt kind, existing anchor, liveRW,
# next Receipt/Anchor. Values are an explicit finite inventory, not read from State.
PHASES=[
 ('BeforeAttempt','BeforeAttempt','NotAttempted',None,None,True,1,1),
 ('Attempted','Attempted','Unknown',None,None,True,1,1),
 ('DurableConfirmed','DurableConfirmed','Committed','DurableWrite',A1,True,2,2),
 ('Published','Published','Committed','DurableWrite',A1,False,2,2),
 ('NoWriteValidated','NoWriteValidated','Committed','NoWriteCompletion',None,False,2,1),
 ('NoWriteWithoutIdentity','NoWriteValidated','Committed',None,None,False,1,1)]
PRE=[];CASES=[]
for name,progress,decision,qkind,anchor,live,nq,na in PHASES:
    pre='rich:pc-'+name
    p={'/actors/0/parked_context':C4,'/actors/0/transaction':T,'/contexts/4':parked(C4),'/next_nonce/Context':5,'/transactions/1/parked_context':C4,'/transactions/1/progress':progress,'/transactions/1/decision':decision,'/transactions/1/life':'Active' if live else 'Ended','/transactions/1/receipt':Q1 if qkind else None,'/transactions/1/delivery':'NotDelivered' if qkind else 'NotApplicable','/next_nonce/Receipt':nq,'/next_nonce/Anchor':na}
    if progress=='NoWriteValidated':p['/transactions/1/prior_successful_write']=False
    if qkind:p['/receipts/1']=receipt(qkind,anchor)
    if not live:
      for n in [1,2,3]:p[f'/owners/{n}/disposition']='Released';p[f'/owners/{n}/current_holder']=None;p[f'/owners/{n}/previous_holder']=T
    PRE.append({'id':pre,'base':'original:rw.written','put':p,'classification':'fully specified assembler selected-readset variation, not a full-history invariant or model-reachability claim; actor.transaction is retained until Finish per actual wire-v2/identity-transition'})
    for idx in list(range(1508,1513))+list(range(1518,1523)):
      x=a.initial_literal(a.DESCRIPTORS[idx]['expected_action']);x.update(context=cp(C4),original_deadline='FutureAtEntry');e=x['environment'];e['fact']=[11,255,255,1,0,2,0];e['transaction']=cp(T)
      if idx==1509:e.update(receipt=[8,0,255,1,1,nq,0],anchor=[12,255,255,1,0,na,0])
      if idx==1510:e.update(receipt=cp(Q1) if qkind else None,anchor=cp(anchor),owners=cp(RELEASE) if live else [])
      if idx==1511:e.update(receipt=[8,0,255,1,1,nq,0],anchor=None,owners=cp(RELEASE) if live else [])
      if idx==1512:e.update(receipt=None,anchor=None,owners=cp(RELEASE) if live else [])
      if idx==1518:
        e.update(receipt=cp(Q1) if qkind else None,anchor=cp(anchor),commit_class=(5 if qkind else 6) if progress=='NoWriteValidated' else 4)
      if idx==1519:e['owners']=cp(RELEASE) if live else []
      if idx in [1520,1521,1522] and live:
        e.update(owners=cp(FENCE),obligations=[obl({1520:'NotCommittedRecoveryRequired',1521:'Unknown',1522:'Committed'}[idx],'AllHeld',4)])
      if idx==1522:e.update(receipt=cp(Q1) if qkind else None,anchor=cp(anchor))
      CASES.append({'id':f'asm/pc-{name}/{idx:04}','pre_state':pre,'raw_index':idx,'raw_action':a.DESCRIPTORS[idx]['expected_action'],'expected_input':x})
# Pending preattempt: supplied bits remain present; assembler does not perform gate.
PRE.append({'id':'rich:pc-BeforeAttempt-pending','base':'rich:pc-BeforeAttempt','put':{'/contexts/4/cancel_supplied_or_signalled':True,'/contexts/4/deadline_reached':True},'classification':'selected read-set variation, no model transition'})
for idx in [1508,1511,1519,1520]:
    src=next(x for x in CASES if x['id']==f'asm/pc-BeforeAttempt/{idx:04}');x=cp(src);x.update(id=f'asm/pc-pre-pending/{idx:04}',pre_state='rich:pc-BeforeAttempt-pending');x['expected_input'].update(supplied_cancel=True,deadline_reached=True);CASES.append(x)
# Wrong-kind PF cannot borrow the PC context or its transaction.
for idx in [1484,1485,1490,1491]:
    x=a.initial_literal(a.DESCRIPTORS[idx]['expected_action']);x['environment']['fact']=[11,255,255,1,0,2,0]
    CASES.append({'id':f'asm/wrong-kind-commit/{idx:04}','pre_state':'rich:pc-BeforeAttempt','raw_index':idx,'raw_action':a.DESCRIPTORS[idx]['expected_action'],'expected_input':x})
# A5 confirms a new Published fact can drain existing recovery owners and update
# current O knowledge while original facts/client knowledge remain unchanged.
p={'/actors/0/parked_context':C5,'/contexts/5':parked(C5),'/next_nonce/Context':6,'/transactions/1/parked_context':C5,'/transactions/1/progress':'DurableConfirmed','/transactions/1/decision':'Committed','/transactions/1/receipt':Q1,'/transactions/1/delivery':'NotDelivered','/receipts/1':receipt('DurableWrite',A1),'/next_nonce/Receipt':2,'/next_nonce/Anchor':2}
PRE.append({'id':'rich:pc-fenced-durable','base':'original:rw.recovery_pending','put':p,'classification':'selected read-set variation with existing O1 NCR, Recovery U6 and original client knowledge unchanged; no fabricated model result'})
for idx in [1510,1520]:
    x=a.initial_literal(a.DESCRIPTORS[idx]['expected_action']);x.update(context=cp(C5),original_deadline='FutureAtEntry');e=x['environment'];e.update(fact=[11,255,255,1,0,3,0],transaction=cp(T))
    if idx==1510:e.update(receipt=cp(Q1),anchor=cp(A1),owners=cp(DRAIN),obligations=[obl('Committed','AllReleased',4)])
    else:e.update(cleanup=3,owners=cp(HANDOFF),obligations=[obl('NotCommittedRecoveryRequired','AllTransferred',4)])
    CASES.append({'id':f'asm/pc-fenced-durable/{idx:04}','pre_state':'rich:pc-fenced-durable','raw_index':idx,'raw_action':a.DESCRIPTORS[idx]['expected_action'],'expected_input':x})
# A3 minimal closure: same pre-state except implicit origin. Published Execute
# releases RW3 only; Recovery U6 is intentionally absent from this input.
PRE.append({'id':'rich:existing-implicit-rw-fenced','base':'original:rw.recovery_pending','put':{'/transactions/1/origin':'ImplicitReadWrite'},'classification':'exact origin read-set variation; not full model reachability'})
for idx in [400,401,402,403,404,405,406]:
    x=a.initial_literal(a.DESCRIPTORS[idx]['expected_action']);x.update(context=[5,0,255,1,1,5,0],target=[3,0,0,1,1,1,1]);e=x['environment']
    if e is not None:e.update(fact=[11,255,255,1,0,3,0],transaction=cp(T))
    if idx==400:e.update(commit_class=4,receipt=cp(Q1),anchor=cp(A1),owners=cp(DRAIN[:3]),obligations=[obl('Committed','AllReleased',3)])
    CASES.append({'id':f'asm/execute-existing-implicit/{idx:04}','pre_state':'rich:existing-implicit-rw-fenced','raw_index':idx,'raw_action':a.DESCRIPTORS[idx]['expected_action'],'expected_input':x})
assert len(CASES)==77
assert len({x['id'] for x in CASES})==77
result={'status':'INDEPENDENT_NORMAL_LITERAL_CANDIDATE_NOT_D2_FROZEN_NOT_RUN','source_decisions':['assembler-readset-v3.md (CommitFact3/4 both RW release;4 no receipt/anchor)','actual wire-v2/identity-transition retained actor.transaction until Finish','owner A3 minimal closure RW3 only versus CommitFact Published drain RW+Recovery'],'prestates':PRE,'cases':CASES,'execution':'static hand-table expansion; no product/model/codec/test calls'}
f=a.write_new('rich-continuation-3.json',result)
man={'status':result['status'],'new_cases':77,'cumulative_unique_cases':1756,'new_error_literals':0,'files':[f,{'path':'author-continuation.py','sha256':a.sha(Path(__file__)),'bytes':Path(__file__).stat().st_size}],'preserved_manifests':[{'path':name,'sha256':a.sha(H/name)} for name in ['manifest.json','manifest-part2.json']],'execution':result['execution']}
print(json.dumps({'manifest':a.write_new('manifest-part3.json',man),'data':f},indent=2))
