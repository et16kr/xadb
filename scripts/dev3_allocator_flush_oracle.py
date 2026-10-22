#!/usr/bin/env python3
"""Independent batch-one flush literals and declarative trace witnesses.

Only struct/hashlib and other Python standard-library modules are used. Product
encoders, product dependencies and flush_io source are never loaded. The frozen
dirty page literals are INPUTS, not new proof of the old dirty producer. --write
creates a new directory and reports GENERATED_INPUTS_NOT_EXECUTION, never PASS.
"""
import argparse
import copy
import csv
import hashlib
import io
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = Path('tests/fixtures/dev3-allocator-flush-one')
DIRTY = Path('tests/fixtures/dev3-dirty-allocation')
INSTALL = Path('tests/fixtures/dev3-doublewrite-install')
GENESIS = Path('crates/engine/tests/fixtures/dev3-storage-session/literal')
DIRTY_SHA = '17a3b3565f93809795b36eec3eb438442d215709f49bdd16ee4209d4e769db12'
INSTALL_SHA = 'b794e062669d4e7442d1398a484dadfaafd193ab5ff9de70788a6a8177139811'
PAGE, SLOT, BANK, SELECTOR = 16384, 16544, 16384, 4096
DATABASE = bytes(range(1, 17))
COUNTS = {'FN': 6, 'FA': 13, 'FB': 31, 'FP': 11, 'FU': 6, 'FC': 6, 'FR': 3}
CONTRACTS = ('devdoc/contracts/dev3-doublewrite-format.md',
             'devdoc/contracts/dev3-control-format.md',
             'devdoc/contracts/dev3-page-format.md')
DRAFT = 'artifacts/dev3/resume-20261022/allocator-flush-one-mandatory-draft-r1.tsv'
DRAFT_SHA = '69a3e3f01ebdc89fee6cef6f59c821ff333edd005774a01203be4522febc5e34'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def seal(data):
    return bytes(data) + hashlib.sha256(data).digest()


def uint(data, offset):
    return struct.unpack_from('<Q', data, offset)[0]


def control(generation, batch, page=None):
    """XDW1 table: header128, optional one entry88, trailer32."""
    require(0 < generation < 2**64 and 0 <= batch < 2**64, 'control counters')
    require(page is None or batch > 0, 'pending batch')
    out = bytearray(128 + (88 if page is not None else 0))
    struct.pack_into('<4sHBB16sQQQH', out, 0, b'XDW1', 1,
                     int(page is not None), 0, DATABASE, 1, generation, batch,
                     int(page is not None))
    if page is not None:
        inspect_page(page)
        struct.pack_into('<Q', out, 56, uint(page, 64))
        struct.pack_into('<HBB', out, 128, 0, page[6], page[7])
        out[136:160] = page[24:48]
        out[160:176] = page[48:64]
        out[176:184] = page[64:72]
        out[184:216] = hashlib.sha256(page).digest()
    return seal(out)


def slot(batch, page):
    """XDS1 slot0, sequence=batch, full image checksum plus envelope checksum."""
    inspect_page(page)
    require(0 < batch < 2**64, 'slot sequence')
    out = bytearray(128)
    struct.pack_into('<4sHH16sQQ', out, 0, b'XDS1', 1, 0, DATABASE, 1, batch)
    out[40:64], out[64:80], out[80:88] = page[24:48], page[48:64], page[64:72]
    out[88:90] = page[6:8]
    out[96:128] = hashlib.sha256(page).digest()
    return seal(out + page)


def selector(profile, generation, bank, selected):
    """XCS1 role2, ControlRef kind4; digest covers exact logical XDW1."""
    require(len(profile) == SELECTOR and bank in (0, 1), 'selector input')
    require(profile[16:32] == DATABASE and uint(profile, 32) == 1, 'profile namespace')
    out = bytearray(SELECTOR - 32)
    struct.pack_into('<4sHBB16sQQQ', out, 0, b'XCS1', 1, 2, 0,
                     DATABASE, 1, generation, uint(profile, 40))
    out[48:80] = hashlib.sha256(profile).digest()
    struct.pack_into('<BBHIQI', out, 80, 4, bank, 0, 0, uint(selected, 32), len(selected))
    out[112:144] = hashlib.sha256(selected).digest()
    return seal(out)


def inspect_page(page):
    require(len(page) == PAGE, 'page length')
    require(page[8:24] == DATABASE and page[6:8] == b'\x0a\0', 'allocator identity')
    require(struct.unpack_from('<5Q', page, 24) == (0x100000001, 0, 1, 1, 1), 'target identity')
    require(uint(page, 64) > 0, 'logged image')
    require(page[-32:] == hashlib.sha256(page[:-32]).digest(), 'input page checksum')


def effects():
    result = []
    steps = ('stage_write', 'stage_sync', 'bank_write', 'bank_sync', 'selector_create',
             'selector_write', 'selector_sync', 'selector_publish', 'directory_sync')
    for phase in ('issuance', 'pending', 'retirement'):
        if phase == 'pending':
            result.extend([('images.slot_write', 'write', SLOT), ('images.file_sync', 'sync', 0)])
        if phase == 'retirement':
            result.extend([('home.page_write', 'write', PAGE), ('home.file_sync', 'sync', 0)])
        for step in steps:
            kind = ('write' if step.endswith('_write') else 'create' if step.endswith('_create')
                    else 'publish' if step.endswith('_publish') else 'directory_sync'
                    if step == 'directory_sync' else 'sync')
            length = (SELECTOR if step == 'selector_write' else BANK) if kind == 'write' else 0
            result.append((phase + '.' + step, kind, length))
    return [{'ordinal': i, 'name': name, 'kind': kind, 'bytes': length}
            for i, (name, kind, length) in enumerate(result, 1)]


EFFECTS = effects()
WRITES = tuple(row['ordinal'] for row in EFFECTS if row['kind'] == 'write')
UNKNOWNS = (8, 9, 19, 20, 30, 31)


def phase_at(cut):
    return 0 if cut <= 9 else 1 if cut <= 20 else 2


def material_state(initial, images, phase_files, cut=32, fault='none'):
    """Declarative writes/publish-prefix model from the 31-effect contract.

    The returned files are affected regions only: image slot0 and home page0;
    all remaining file bytes are independently fixed as unchanged. A failed
    lower call is attempted but not counted completed even after mutation.
    """
    state = dict(initial)
    for row in EFFECTS:
        n, kind = row['ordinal'], row['kind']
        if (n > cut and fault not in ('none', 'short', 'interrupted')) or (n == cut and fault == 'before'):
            break
        phase = phase_at(n)
        group = phase_files[phase]
        target = payload = None
        if kind == 'write':
            if n == 10:
                target, payload = 'd3-dwb-images:slot0', images['slot']
            elif n == 21:
                target, payload = 'home:page0', images['home']
            elif row['name'].endswith('selector_write'):
                target, payload = 'd3-dwb-select-stage', group['selector']
            elif row['name'].endswith('stage_write'):
                target, payload = 'd3-dwb-stage', group['bank']
            else:
                target, payload = 'd3-dwb-' + str(group['bank_index']), group['bank']
            if n == cut and fault == 'half':
                prefix = len(payload) // 2
                old = state[target]
                state[target] = payload[:prefix] + old[prefix:]
            else:
                state[target] = payload
        elif kind == 'create':
            require('d3-dwb-select-stage' not in state, 'create-new stage exists')
            state['d3-dwb-select-stage'] = b''
        elif kind == 'publish':
            state['DWB'] = state.pop('d3-dwb-select-stage')
        if n == cut and fault in ('before', 'half', 'after'):
            break
    completed = 31 if fault in ('none', 'short', 'interrupted') else cut - 1
    known_phase = sum(completed >= n for n in (9, 20, 31)) - 1
    possible = [known_phase]
    if fault in ('before', 'after') and cut in UNKNOWNS:
        # Protection union begins before publish is attempted, even if actual
        # BeforeError leaves old namespace bytes. It is not a claim of mutation.
        possible.append(phase_at(cut))
    return state, {'attempted': 31 if fault in ('none', 'short', 'interrupted') else cut,
                   'completed': completed, 'known_selection_index': known_phase,
                   'possible_selection_indices': possible,
                   'home_sync_confirmed': completed >= 22,
                   'physical_effect_cut': min(cut, 31),
                   'physical_write_subcalls_at_cut': 2 if fault in ('half', 'short', 'interrupted') else 1,
                   'original_lower_error_required': fault in ('before', 'half', 'after'),
                   'top_effect': 'CallerMustResolve' if fault in ('before', 'half', 'after') else None,
                   'fault_call_mutated': fault in ('half', 'after')}


def valid_witness():
    return {'events': ['claim', 'capture', 'wal_durable', 'issuance_durable', 'image_write',
                       'image_sync', 'pending_durable', 'home_write', 'home_sync',
                       'retirement_durable', 'finalize', 'claim_return', 'slot_reuse'],
            'requirement_lsn': 1, 'captured_lsn': 1, 'same_origin': True,
            'pending_count': 1, 'retired_count': 1, 'captured_counter': 1,
            'live_counter': 1, 'final_counter': 1, 'redirty': False,
            'final_dpt_present': False, 'completion_count': 1, 'selected_authority': True}


def inspect_witness(w):
    """Small independent order/ownership checker; no product state machine."""
    events = w['events']
    def precedes(a, b, why):
        require(a in events and b in events and events.index(a) < events.index(b), why)
    precedes('claim', 'capture', 'N01')
    require(w['requirement_lsn'] == w['captured_lsn'], 'N02')
    require(w['same_origin'], 'N03')
    precedes('wal_durable', 'image_write', 'N04')
    precedes('image_sync', 'pending_durable', 'N05')
    precedes('issuance_durable', 'image_write', 'N06')
    precedes('pending_durable', 'home_write', 'N07')
    precedes('home_sync', 'retirement_durable', 'N08')
    require(w['retired_count'] == w['pending_count'], 'N09')
    precedes('retirement_durable', 'slot_reuse', 'N10')
    precedes('finalize', 'claim_return', 'N11')
    require(not w['redirty'] or w['final_dpt_present'], 'N12')
    require(w['final_counter'] == w['live_counter'] >= w['captured_counter'], 'N13')
    if 'finalize_busy' in events:
        cut = events.index('finalize_busy')
        require(not any(e in events[cut + 1:] for e in
                        ('wal_io', 'image_write', 'image_sync', 'home_write', 'home_sync',
                         'namespace_read', 'file_read', 'selector_publish')), 'N14')
    require(w['completion_count'] == 1, 'N15')
    require(w['selected_authority'], 'N16')
    precedes('retirement_durable', 'finalize', 'N17')
    precedes('image_write', 'image_sync', 'N18')
    precedes('capture', 'image_write', 'N19')
    return True


def negative_witnesses():
    rows = []
    for i in range(1, 20):
        w = copy.deepcopy(valid_witness())
        events = w['events']
        pairs = {1: ('claim', 'capture'), 4: ('wal_durable', 'image_write'),
                 6: ('issuance_durable', 'image_write'), 7: ('pending_durable', 'home_write'),
                 8: ('home_sync', 'retirement_durable'), 10: ('retirement_durable', 'slot_reuse'),
                 11: ('finalize', 'claim_return'),
                 17: ('retirement_durable', 'finalize'),
                 18: ('image_write', 'image_sync'), 19: ('capture', 'image_write')}
        if i in pairs:
            a, b = pairs[i]
            events.remove(a)
            events.insert(events.index(b) + 1, a)
        elif i == 2: w['requirement_lsn'] = 2
        elif i == 3: w['same_origin'] = False
        elif i == 5: events.remove('image_sync')
        elif i == 9: w['pending_count'] = 2
        elif i == 12: w.update(redirty=True, live_counter=2, final_counter=2)
        elif i == 13: w['final_counter'] = 0
        elif i == 14: events.extend(['finalize_busy', 'namespace_read'])
        elif i == 15: w['completion_count'] = 2
        elif i == 16: w['selected_authority'] = False
        rows.append({'id': f'N{i:02}', 'expected_rejection': f'N{i:02}', 'witness': w})
    return rows


def json_bytes(value):
    return (json.dumps(value, indent=2) + '\n').encode()


def tsv_bytes(fields, rows):
    out = io.StringIO(newline='')
    writer = csv.DictWriter(out, fields, delimiter='\t', lineterminator='\n')
    writer.writeheader()
    writer.writerows(rows)
    return out.getvalue().encode()


def build(root):
    outputs, inputs, batches, fault_rows, expanded = {}, [], [], [], []
    dirty_raw = (root / DIRTY / 'manifest.json').read_bytes()
    install_raw = (root / INSTALL / 'manifest.json').read_bytes()
    require(digest(dirty_raw) == DIRTY_SHA and digest(install_raw) == INSTALL_SHA, 'frozen provenance manifests')
    dirty = json.loads(dirty_raw)
    dirty_rows = {row['path']: row for row in dirty['literals']}
    def read_input(path, expected):
        data = (root / path).read_bytes()
        require(digest(data) == expected, f'input hash {path}')
        inputs.append({'path': str(path), 'bytes': len(data), 'sha256': expected})
        return data
    def put(path, data):
        require(path not in outputs, 'duplicate output')
        outputs[path] = data
        return path
    # Original fresh literals are inputs only. Recompute every new operational control.
    installed = json.loads(install_raw)
    for profile_id in (1, 2, 3):
        installed_files = installed['installed_files'][str(profile_id)]
        profile = read_input(GENESIS / f'profile{profile_id}-d3-profile.bin', installed_files['d3-profile']['sha256'])
        initial_page = read_input(GENESIS / f'profile{profile_id}-allocation.bin',
                                  '1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7')
        initial_empty = control(1, 0)
        initial_selector = selector(profile, 1, 0, initial_empty)
        require(digest(initial_selector) == installed_files['DWB']['sha256'], 'initial selector provenance')
        stage = read_input(INSTALL / 'literal' / f'profile{profile_id}-d3-dwb-stage.bin',
                           installed_files['d3-dwb-stage']['sha256'])
        state = {'d3-dwb-stage': stage, 'd3-dwb-0': initial_empty + bytes(BANK-len(initial_empty)),
                 'd3-dwb-1': bytes(BANK), 'DWB': initial_selector,
                 'd3-dwb-images:slot0': bytes(SLOT), 'home:page0': initial_page}
        for file, data in state.items():
            put(f'p{profile_id}-initial-{file.replace(":", "-")}.bin', data)
        for index, (count, operation) in enumerate(((1,'reserve'),(1,'cancel'),(8,'reserve'),(8,'cancel'),(63,'reserve'),(63,'cancel')), 1):
            stem = f'p{profile_id}-n{count}-{operation}'
            page_name = stem + '.page'
            page = read_input(DIRTY / page_name, dirty_rows[page_name]['sha256'])
            require(uint(page,64) == index, 'history LSN')
            image = slot(index, page)
            put(stem + '-slot.bin', image)
            put(stem + '-home.before', state['home:page0'])
            put(stem + '-home.after', page)
            phases = []
            phase_meta = []
            for phase_index, phase in enumerate(('issuance','pending','retirement')):
                generation = 3 * (index-1) + 2 + phase_index
                selected = control(generation, index, page if phase == 'pending' else None)
                bank_index = (generation - 1) % 2
                bank = selected + bytes(BANK-len(selected))
                select = selector(profile,generation,bank_index,selected)
                phases.append({'bank':bank,'selector':select,'bank_index':bank_index})
                phase_meta.append({'phase':phase,'generation':generation,'batch':index,'bank_index':bank_index,
                                   'logical':put(stem+'-'+phase+'.control',selected),
                                   'bank':put(stem+'-'+phase+'.bank',bank),
                                   'selector':put(stem+'-'+phase+'.selector',select)})
            before = {k: digest(v) for k,v in state.items()}
            final, facts = material_state(state, {'slot':image,'home':page},phases)
            batches.append({'id':stem,'profile':profile_id,'count':count,'operation':operation,
                            'lsn':index,'batch':index,'before':before,'phases':phase_meta,
                            'slot':stem+'-slot.bin','home_before':stem+'-home.before','home_after':stem+'-home.after',
                            'after':{k:digest(v) for k,v in final.items()},'facts':facts})
            modes = [('normal',32,'none')]+[('after',n,'after') for n in range(1,32)]
            modes += [('short',n,'short') for n in WRITES]+[('interrupted',n,'interrupted') for n in WRITES]
            for label,cut,mode in modes:
                selected_state, diag = material_state(state,{'slot':image,'home':page},phases,cut,mode)
                eid=f'S5-P{profile_id}-N{count}-{operation.upper()}-{label.upper()}-{cut:02}'
                expanded.append({'id':eid,'batch_input':stem,'mode':label,'cut':cut,
                                 'status':'S5_INPUT_NOT_RUN','physical_sha256':{k:digest(v) for k,v in selected_state.items()},'facts':diag})
            if profile_id == 1 and index == 1:
                for group, cuts, mode in [('FB',range(1,32),'before'),('FP',WRITES,'half'),('FU',UNKNOWNS,'after')]:
                    for ordinal,cut in enumerate(cuts,1):
                        selected_state,diag=material_state(state,{'slot':image,'home':page},phases,cut,mode)
                        # Compact immutable full bytes for every changed region at mandatory fault cuts.
                        files={}
                        for file,data in selected_state.items():
                            name=f'fault-{group}{ordinal:02}-{file.replace(":","-")}.bin'
                            files[file]={'literal':put(name,data),'bytes':len(data),'sha256':digest(data)}
                        fault_rows.append({'id':f'{group}{ordinal:02}','cut':cut,'mode':mode,'files':files,'facts':diag})
            state = final
    original=(root / DRAFT).read_bytes()
    require(digest(original)==DRAFT_SHA,'original 75 inventory identity')
    rows=list(csv.DictReader(io.StringIO(original.decode()),delimiter='\t'))
    addition={'case_id':'FA13','group':'admission','scenario':'TEST_ONLY valid capture then corrupt claim: V01 claim.attempt=0; V02 attempt!=last_attempt; V03 claim.index=64; V04 claim.registration.generation=2; V05 captured.stamp.identity.objectId mismatch; V06 captured.stamp.recLSN=0',
              'expected':'Invariant/CallerMustResolve; preserve original corrupt claim/capture; common fence; no DWB/home I/O; variants exactly V01..V06', 'status':'INPUT_NOT_RUN'}
    rows.insert(next(i for i,r in enumerate(rows) if r['case_id']=='FB01'),addition)
    for row in rows:
        row['status']='INPUT_NOT_RUN'
        if row['case_id']=='FR03':
            row['scenario']='independent negative trace/ownership witnesses N01..N19; original16 plus retirement-before-finalize/image-write-before-sync/capture-before-image witnesses'
            row['expected']='reject each of19 named mutants once for its exact reason; actual valid witness accepted; no product mutant execution; original16 retained in generation artifacts'
    require(len(rows)==76,'mandatory census')
    put('mandatory-cases.tsv',tsv_bytes(['case_id','group','scenario','expected','status'],rows))
    put('effects.json',json_bytes(EFFECTS))
    put('fault-expectations.json',json_bytes(fault_rows))
    put('expanded-cases.json',json_bytes(expanded))
    put('expanded-cases.tsv',tsv_bytes(['id','batch_input','mode','cut','status'],[{k:r[k] for k in ('id','batch_input','mode','cut','status')} for r in expanded]))
    put('known-bad.json',json_bytes({'valid':valid_witness(),'negative':negative_witnesses()}))
    witness_rows=[]
    for row in [{'id':'VALID','expected_rejection':'ACCEPT','witness':valid_witness()}]+negative_witnesses():
        witness_rows.append({'id':row['id'],'expected':row['expected_rejection'],**{k:','.join(v) if isinstance(v,list) else int(v) if isinstance(v,bool) else v for k,v in row['witness'].items()}})
    put('known-bad.tsv',tsv_bytes(list(witness_rows[0]),witness_rows))
    fault_files=[{'id':r['id'],'file':name,**meta} for r in fault_rows for name,meta in r['files'].items()]
    put('fault-files.tsv',tsv_bytes(['id','file','literal','bytes','sha256'],fault_files))
    expanded_files=[{'id':r['id'],'file':name,'sha256':sha} for r in expanded for name,sha in r['physical_sha256'].items()]
    put('expanded-files.tsv',tsv_bytes(['id','file','sha256'],expanded_files))
    batch_rows=[{'id':b['id'],'profile':b['profile'],'count':b['count'],'operation':b['operation'],'lsn':b['lsn'],'batch':b['batch'],'slot':b['slot'],'home_before':b['home_before'],'home_after':b['home_after']} for b in batches]
    put('batches.tsv',tsv_bytes(list(batch_rows[0]),batch_rows))
    require(len(expanded)==972 and len(fault_rows)==48,'expanded/fault census')
    return outputs, {'schema':'xadb-dev3-allocator-flush-oracle-v1','status':'GENERATED_INPUTS_NOT_EXECUTION',
        'product_tests':'NOT_RUN','oracle_selftests':'NOT_RUN','native_tests':'NOT_RUN','wal_tests':'SEPARATE_DENOMINATOR',
        'generator':{'path':'scripts/dev3_allocator_flush_oracle.py','sha256':digest(Path(__file__).read_bytes())},
        'contracts_read':{path:digest((root/path).read_bytes()) for path in CONTRACTS},
        'provenance_manifests':{str(DIRTY/'manifest.json'):DIRTY_SHA,str(INSTALL/'manifest.json'):INSTALL_SHA,DRAFT:DRAFT_SHA},
        'inputs':inputs,'mandatory_cases':76,'group_counts':COUNTS,'logical_effects':31,'write_effects':list(WRITES),
        'fault_cases':48,'known_bad':19,'s5_cases':972,'s5_census':{'normal':18,'after':558,'short':198,'interrupted':198},
        'history_per_profile':['reserve1','cancel1','reserve8','cancel8','reserve63','cancel63'],
        'batches':batches,'affected_region_contract':{'d3-dwb-images:slot0':{'offset':0,'bytes':SLOT,'remaining_bytes_zero':16711680-SLOT},'home:page0':{'offset':0,'bytes':PAGE,'remaining_bytes_unchanged':1048576-PAGE},'d3-dwb-reserved':'unchanged8192zero'},
        'literals':[{'path':name,'bytes':len(data),'sha256':digest(data)} for name,data in sorted(outputs.items())]}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=ROOT)
    parser.add_argument('--output',type=Path,default=OUTPUT)
    parser.add_argument('--write',action='store_true',required=True)
    args=parser.parse_args(); root=args.root.resolve()
    output=args.output if args.output.is_absolute() else root/args.output
    require(not output.exists(),'output exists: preserve original')
    files,manifest=build(root)
    output.mkdir(parents=True,exist_ok=False)
    for name,data in files.items():
        with (output/name).open('xb') as f:f.write(data)
    with (output/'manifest.json').open('xb') as f:f.write(json_bytes(manifest))
    print(json.dumps({'status':manifest['status'],'literal_count':len(files),'mandatory':76,'s5':972,'output':str(output),'manifest_sha256':digest((output/'manifest.json').read_bytes())}))


if __name__=='__main__': main()
