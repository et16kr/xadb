"""Bounded independent one-slot ownership model, independent of product implementation."""
from dataclasses import dataclass,replace,asdict
from collections import deque,Counter
from pathlib import Path
import json,hashlib
@dataclass(frozen=True)
class State:
 mode:str='idle'
 complete:int=0
 synced:int=0
 durable:int=0
 owner:int=-1
 phase:int=0
 gate:bool=False

def step(s,actor,action,mutant=None):
 if action=='observe':return s
 if s.mode=='fenced':
  if mutant=='clear_fence' and action=='stage':return replace(s,mode='complete',complete=s.synced+1,owner=actor,phase=0)
  return s
 if action=='stage':
  if s.mode!='idle' and not(mutant=='reuse_io_slot' and s.mode=='io'):return s
  return replace(s,mode='complete',complete=s.complete+1,owner=actor,phase=0,synced=s.complete+1 if mutant=='publish_before_sync' else s.synced)
 if action=='take_io' and s.mode=='complete':return replace(s,mode='io',owner=actor,phase=0,gate=mutant=='io_under_gate')
 if action=='write' and s.mode=='io' and actor==s.owner and s.phase==0:return replace(s,phase=1)
 if action=='sync' and s.mode=='io' and actor==s.owner and s.phase==1:return replace(s,phase=2,durable=s.complete)
 if action=='publish' and s.mode=='io' and actor==s.owner and s.phase==2:return replace(s,mode='idle',synced=s.complete,owner=-1,phase=0)
 if action=='fault' and s.mode in ('complete','io'):return replace(s,mode='fenced',gate=False)
 return s

def invariant(before,s,action):
 if before.mode=='fenced' and s.mode!='fenced':return 'fence must not clear'
 if s.gate and s.mode=='io':return 'no file IO under append gate'
 if not(s.synced<=s.durable<=s.complete<=s.synced+1):return 'confirmed <= durable <= complete <= confirmed+one slot'
 if s.mode=='idle' and not(s.complete==s.synced==s.durable and s.owner==-1 and s.phase==0):return 'idle returns one drained slot'
 if s.mode in ('complete','io') and s.complete!=s.synced+1:return 'pending slot protects exactly one complete frame'
 if s.mode=='complete' and (s.phase!=0 or s.durable!=s.synced):return 'complete has no IO or durability yet'
 if s.mode=='io' and s.phase<2 and s.durable!=s.synced:return 'durability before sync'
 return None
ACTIONS=('stage','take_io','write','sync','publish','fault','observe')
def explore(mutant=None):
 initial=State();q=deque([(initial,0,[])]);seen={(initial,0)};trans=0;counts=Counter();modes=Counter()
 while q:
  s,depth,history=q.popleft();modes[s.mode]+=1
  if depth==8:continue
  for actor in (0,1):
   for action in ACTIONS:
    n=step(s,actor,action,mutant);trans+=1;counts[action]+=1
    why=invariant(s,n,action)
    if why:return {'status':'REJECTED_MUTANT','mutant':mutant,'reason':why,'witness':history+[(actor,action)],'before':asdict(s),'after':asdict(n)}
    item=(n,depth+1)
    if item not in seen:seen.add(item);q.append((n,depth+1,history+[(actor,action)]))
 return {'status':'PASS_BOUNDED_SLOT_MODEL','actors':2,'max_depth':8,'states_with_depth':len(seen),'transitions':trans,'operation_counts':dict(counts),'state_mode_counts':dict(modes)}

def evaluate():
    normal = explore()
    assert normal == {'status': 'PASS_BOUNDED_SLOT_MODEL', 'actors': 2, 'max_depth': 8,
                      'states_with_depth': 113, 'transitions': 1246,
                      'operation_counts': {action: 178 for action in ACTIONS},
                      'state_mode_counts': {'idle': 13, 'complete': 22, 'io': 42, 'fenced': 36}}
    mutants = [explore(m) for m in ('publish_before_sync', 'reuse_io_slot', 'clear_fence', 'io_under_gate')]
    assert all(m['status'] == 'REJECTED_MUTANT' and m['witness'] for m in mutants)
    return {'normal': normal, 'mutants': mutants,
            'scope': 'bounded abstract ownership only; actual runtime schedules and bytes are separate'}
