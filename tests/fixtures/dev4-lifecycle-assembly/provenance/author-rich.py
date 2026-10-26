#!/usr/bin/env python3
"""Second independent literal batch. Fixed owner lists, no state algorithm.

Original immutable literal inputs supply prefix shapes only. Each new family
crossing below names exact input members by hand; no model/post-state/product
resolver/codec is called. Existing candidate files are never overwritten.
"""
import copy, importlib.util, json, hashlib
from pathlib import Path
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('independent_initial_tables',HERE/'author-normal.py')
a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
cp=copy.deepcopy
P1=[3,0,0,1,1,1,1];P2=[3,0,1,1,1,2,1];R1=[4,0,0,1,1,1,1];R1S1=[4,0,1,1,1,1,1]
T1=[6,0,255,1,1,1,0];O1=[10,0,255,1,1,1,0]
PEERT1=[6,1,255,1,2,1,0];PEERO1=[10,1,255,1,2,1,0];PEERC1=[5,1,255,1,2,1,0]

def owner(identity,kind,disposition,before,after,obligation=None):
    return {'resource':identity,'resource_kind':kind,'after_disposition':disposition,'obligation':obligation,'before_holder':before,'after_holder':after}
def o0(n,kind,disposition,before,after,obligation=None):return owner([9,0,255,1,1,n,0],kind,disposition,before,after,obligation)
def ob(identity,decision,summary,count):return {'obligation':identity,'known_decision':decision,'after_ownership_summary':summary,'associated_owner_count':count}
# Every set below is a fixed hand-written list: no walk of pre-state owner tables.
NEW_RW_HELD=[o0(3,'Undo','Held',None,T1),o0(4,'Locks','Held',None,T1),o0(5,'AbortReserve','Held',None,T1)]
NEW_RW_RELEASED=[o0(3,'Undo','Released',None,None),o0(4,'Locks','Released',None,None),o0(5,'AbortReserve','Released',None,None)]
EXPLICIT_RW_RELEASED=[o0(1,'Undo','Released',T1,None),o0(2,'Locks','Released',T1,None),o0(3,'AbortReserve','Released',T1,None)]
NEW_FENCE=[o0(3,'Undo','Held',None,T1,O1),o0(4,'Locks','Held',None,T1,O1),o0(5,'AbortReserve','Held',None,T1,O1),o0(6,'Recovery','Held',None,T1,O1)]
EXPLICIT_FENCE=[o0(1,'Undo','Held',T1,T1,O1),o0(2,'Locks','Held',T1,T1,O1),o0(3,'AbortReserve','Held',T1,T1,O1),o0(6,'Recovery','Held',None,T1,O1)]
CURSOR_RELEASE=[o0(3,'Snapshot','Released',R1,None),o0(4,'Scratch','Released',R1,None),o0(5,'Temp','Released',R1,None)]
CLOSE_SESSION_TEN=[o0(1,'Undo','Released',T1,None),o0(2,'Locks','Released',T1,None),o0(3,'AbortReserve','Released',T1,None),o0(4,'Plan','Released',P1,None),o0(5,'Binding','Released',P1,None),o0(6,'Plan','Released',P2,None),o0(7,'Binding','Released',P2,None),o0(8,'Snapshot','Released',R1S1,None),o0(9,'Scratch','Released',R1S1,None),o0(10,'Temp','Released',R1S1,None)]
CLOSE_FENCED_SIX=[o0(1,'Undo','Transferred',T1,O1,O1),o0(2,'Locks','Transferred',T1,O1,O1),o0(3,'AbortReserve','Transferred',T1,O1,O1),o0(4,'Plan','Released',P1,None),o0(5,'Binding','Released',P1,None),o0(6,'Recovery','Transferred',T1,O1,O1)]
RECOVERY_FOUR=[o0(1,'Undo','Released',T1,None,O1),o0(2,'Locks','Released',T1,None,O1),o0(3,'AbortReserve','Released',T1,None,O1),o0(6,'Recovery','Released',T1,None,O1)]
PEER_FENCE=[owner([9,1,255,1,2,1,0],'Undo','Held',PEERT1,PEERT1,PEERO1),owner([9,1,255,1,2,2,0],'Locks','Held',PEERT1,PEERT1,PEERO1),owner([9,1,255,1,2,3,0],'AbortReserve','Held',PEERT1,PEERT1,PEERO1),owner([9,1,255,1,2,4,0],'Recovery','Held',None,PEERT1,PEERO1)]
CASES=[]
# Source original IDs already selected in the first batch must not be repeated.
EXISTING={(x['pre_state'],x['raw_index']) for filename in ['initial.json','original-links.json','rich.json'] for x in json.loads((HERE/filename).read_text())['cases']}
def put(name,pre,idx,ctxnonce,factnonce,target,txn,owners=None,obligations=None,changes=None):
    if (pre,idx) in EXISTING:return
    x=a.initial_literal(a.DESCRIPTORS[idx]['expected_action'])
    if x['context'] is not None:x['context']=[5,0,255,1,1,ctxnonce,0]
    x['target']=cp(target)
    if x['environment'] is not None:
        e=x['environment'];e['fact']=[11,255,255,1,0,factnonce,0];e['transaction']=cp(txn)
        if owners is not None:e['owners']=cp(owners)
        if obligations is not None:e['obligations']=cp(obligations)
        if changes:e.update(cp(changes))
    CASES.append({'id':name,'pre_state':pre,'raw_index':idx,'raw_action':a.DESCRIPTORS[idx]['expected_action'],'expected_input':x})
    EXISTING.add((pre,idx))
# Four exact existing/prospective origin partitions. Entry has no env. None of
# these pre-states has a peer or existing O, so the fixed fence lists are complete.
for group,pre,ctx,existing_rw in [('execute-select','original:sel.prepared',2,False),('execute-new-implicit','rich:dml-on-prospective',2,False),('execute-explicit','original:rw.prepared',3,True),('execute-off','original:off.prepared',3,False)]:
    for outcome in range(11):
        owners=[];obs=[];changes={}
        if group=='execute-new-implicit':
            if outcome in [0,3,4,5,6,7]:owners=NEW_RW_RELEASED;changes={'commit_class':4 if outcome==0 else 0}
            if outcome==0:changes.update(receipt=[8,0,255,1,1,1,0],anchor=[12,255,255,1,0,1,0])
            if outcome in [8,9,10]:owners=NEW_FENCE;obs=[ob(O1,{8:'NotCommittedRecoveryRequired',9:'Unknown',10:'Committed'}[outcome],'AllHeld',4)]
        elif group=='execute-explicit':
            if outcome==7:owners=EXPLICIT_RW_RELEASED;changes={'commit_class':0}
            if outcome in [8,9,10]:owners=EXPLICIT_FENCE;obs=[ob(O1,{8:'NotCommittedRecoveryRequired',9:'Unknown',10:'Committed'}[outcome],'AllHeld',4)]
        elif group=='execute-off':
            if outcome in [0,3,4,5,6]:owners=NEW_RW_HELD
            if outcome==7:owners=NEW_RW_RELEASED;changes={'commit_class':0}
            if outcome in [8,9,10]:owners=NEW_FENCE;obs=[ob(O1,{8:'NotCommittedRecoveryRequired',9:'Unknown',10:'Committed'}[outcome],'AllHeld',4)]
        # Select success is copied in original batch; failure RO has no RW set.
        put(f'asm/{group}/{400+outcome:04}',pre,400+outcome,ctx,1,P1,T1,owners,obs,changes)
# Crossed existing original full states: exact descriptor txn survives cached paths.
put('asm/close-closed/1072','original:sel.closed',1072,4,3,R1,T1,[])
put('asm/close-closed/1168','original:sel.closed',1168,4,3,P1,T1,[o0(1,'Plan','Released',P1,None),o0(2,'Binding','Released',P1,None)])
put('asm/close-unrelated/1168','original:rw.select_open',1168,6,3,P1,None,[o0(4,'Plan','Released',P1,None),o0(5,'Binding','Released',P1,None)])
put('asm/close-unrelated/1171','original:rw.select_open',1171,6,3,P2,T1,[o0(6,'Plan','Released',P2,None),o0(7,'Binding','Released',P2,None),o0(8,'Snapshot','Released',R1S1,None),o0(9,'Scratch','Released',R1S1,None),o0(10,'Temp','Released',R1S1,None)])
put('asm/close-session-open/1310','original:rw.select_open',1310,6,3,None,T1,CLOSE_SESSION_TEN,[],{'commit_class':0})
put('asm/close-session-fenced/1310','original:rw.recovery_pending',1310,5,3,None,T1,CLOSE_FENCED_SIX,[ob(O1,'NotCommittedRecoveryRequired','AllTransferred',4)],{'cleanup':3,'commit_class':255})
# Recovery mutations start from one fixed obligation. They do not alter owner rows.
for outcome in range(5):
    exact=ob(O1,'NotCommittedRecoveryRequired','AllReleased',4)
    if outcome==0:obs=[exact]
    elif outcome==1:obs=[]
    elif outcome==2:obs=[exact,ob([10,0,255,1,1,255,0],'NotCommittedRecoveryRequired','AllReleased',4)]
    elif outcome==3:obs=[ob([10,0,255,255,1,1,0],'NotCommittedRecoveryRequired','AllReleased',4)]
    else:obs=[exact,cp(exact)]
    put(f'asm/recovery-one/{1528+outcome:04}','original:rw.recovery_pending',1528+outcome,0,3,None,None,RECOVERY_FOUR,obs)
# A5: Idle A0 + real Begin A1 read-set; T1's NotAttempted survives raw NCR cause.
put('asm/idle-rollback-peer/1303','rich:peer-rw-unattempted',1303,2,1,None,None,PEER_FENCE,[ob(PEERO1,'NotAttempted','AllHeld',4)])
# A2 invalid child / unmatched continuation must not copy that peer census.
for idx in [573,574,575]:
    target=[254,0,0,1,1,255,1]
    put(f'asm/invalid-child-peer/{idx:04}','rich:peer-rw-unattempted',idx,2,1,target,None,[],[])
for idx in [1485,1520]:put(f'asm/unmatched-peer/{idx:04}','rich:peer-rw-unattempted',idx,0,1,None,None,[],[])
# Normal matched PF: exact original context bits and original txn, fresh context0.
# In these RO states Incomplete has no live RW census; provided cleanup2 remains.
for name,pre,deadline,cancel,reached in [('resume-unsignalled','original:sel.parked','FutureAtEntry',False,False),('resume-deadline','original:sel.signalled','FutureAtEntry',False,True),('resume-cancel','rich:parked-fetch-cancel','FutureAtEntry',True,False)]:
    for outcome in range(10):
        idx=1484+outcome
        if (pre,idx) in EXISTING:continue
        # fixed hand table for RO Complete cleanup: Rows/Buffer none; Eof,
        # ReadError,Conflict cursor; pending overrides every work at Complete.
        release_outcomes=[0,2,4,6,8] if cancel or reached else [2,6,8]
        owners=CURSOR_RELEASE if outcome in release_outcomes else []
        put(f'asm/{name}/{idx:04}',pre,idx,0,2,None,T1,owners,[])
        CASES[-1]['expected_input'].update(context=[5,0,255,1,1,3,0],original_deadline=deadline,supplied_cancel=cancel,deadline_reached=reached)
# Wrong continuation kind uses no PF context, even when pending controls exist.
for idx in list(range(1508,1513))+list(range(1518,1523)):
    put(f'asm/wrong-kind-fetch/{idx:04}','original:sel.parked',idx,0,2,None,None,[],[])
# Exact pre-state deltas, separate from expected inputs and never produced by model.
peer_put={'/actors/0/transaction':None,'/actors/1/transaction':PEERT1,'/contexts/1/id':PEERC1,'/transactions/1/id':PEERT1,'/transactions/1/birth_session':[2,1,255,1,2,2,0],
 '/owners/1/id':[9,1,255,1,2,1,0],'/owners/1/original_holder':PEERT1,'/owners/1/current_holder':PEERT1,'/owners/1/birth_context':PEERC1,
 '/owners/2/id':[9,1,255,1,2,2,0],'/owners/2/original_holder':PEERT1,'/owners/2/current_holder':PEERT1,'/owners/2/birth_context':PEERC1,
 '/owners/3/id':[9,1,255,1,2,3,0],'/owners/3/original_holder':PEERT1,'/owners/3/current_holder':PEERT1,'/owners/3/birth_context':PEERC1}
PRESTATES=[{'id':'rich:dml-on-prospective','base':'original:sel.prepared','put':{'/issued_statements/1/shape':'Dml','/issued_statements/1/metadata':'DmlN0'},'classification':'direct source variation; same N0 binding/provenance; no runtime reachability claim'},
 {'id':'rich:peer-rw-unattempted','base':'original:rw.begun','put':peer_put,'classification':'manually specified Begin-A1 counterpart of original Begin-A0; every changed identity/ref listed; no model execution'},
 {'id':'rich:parked-fetch-cancel','base':'original:sel.parked','put':{'/contexts/3/cancel_supplied_or_signalled':True},'classification':'original PF with explicit pending bit variation; no transition computed'}]
assert len({x['id'] for x in CASES})==len(CASES)
# Validate only static literal bookkeeping, not product behavior.
for x in CASES:
    e=x['expected_input']['environment']
    if e is not None:
        assert len(e['owners'])==len({tuple(o['resource']) for o in e['owners']})
        assert e['owners']==sorted(e['owners'],key=lambda o:tuple(o['resource']))
result={'status':'INDEPENDENT_NORMAL_LITERAL_CANDIDATE_NOT_D2_FROZEN_NOT_RUN','schema':'same as initial/original-links/rich.json','prestates':PRESTATES,'cases':CASES,'pending':['Existing implicit RW Fenced Success requires exact O/Recovery release selection confirmation; not authored here.','Read-set/compound-error literals remain deferred.']}
receipt=a.write_new('rich-normal-2.json',result)
manifest={'status':result['status'],'new_cases':len(CASES),'cumulative_unique_cases':1582+len(CASES),'new_error_literals':0,'new_prestates':len(PRESTATES),'files':[receipt,{'path':'author-rich.py','sha256':a.sha(Path(__file__)),'bytes':Path(__file__).stat().st_size}],'preserved_manifest':{'path':'manifest.json','sha256':a.sha(HERE/'manifest.json')},'execution':'static table expansion only; no product/model/codec/test calls'}
print(json.dumps({'manifest':a.write_new('manifest-part2.json',manifest),'data':receipt,'counts':{'new':len(CASES),'cumulative':1582+len(CASES)}},indent=2))
