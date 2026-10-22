#!/usr/bin/env python3
"""Only new independent flush-oracle/checker selftests; never product execution."""
import copy
import json
from pathlib import Path
import struct
import unittest
import dev3_allocator_flush_oracle as o


class FlushOracleTests(unittest.TestCase):
    def test_control_slot_selector_fields_and_hash_domains(self):
        root=Path(__file__).resolve().parent.parent
        page=(root/o.DIRTY/'p1-n1-reserve.page').read_bytes()
        profile=(root/o.GENESIS/'profile1-d3-profile.bin').read_bytes()
        pending=o.control(3,1,page)
        self.assertEqual(len(pending),248)
        self.assertEqual(struct.unpack_from('<QQH',pending,32),(3,1,1))
        self.assertEqual(pending[136:184],page[24:72])
        self.assertEqual(pending[184:216].hex(),o.digest(page))
        self.assertEqual(pending[-32:].hex(),o.digest(pending[:-32]))
        slot=o.slot(1,page)
        self.assertEqual(len(slot),16544)
        self.assertEqual(slot[128:16512],page)
        self.assertEqual(slot[-32:].hex(),o.digest(slot[:-32]))
        selected=o.selector(profile,3,0,pending)
        self.assertEqual(selected[80:82],b'\4\0')
        self.assertEqual(selected[112:144].hex(),o.digest(pending))
        self.assertNotEqual(selected[112:144],pending[-32:])
        self.assertEqual(selected[-32:].hex(),o.digest(selected[:-32]))

    def test_all_nineteen_bad_witnesses_have_their_own_reason(self):
        self.assertTrue(o.inspect_witness(o.valid_witness()))
        rows=o.negative_witnesses()
        self.assertEqual([r['id'] for r in rows],[f'N{i:02}' for i in range(1,20)])
        for row in rows:
            with self.subTest(row['id']),self.assertRaisesRegex(ValueError,'^'+row['expected_rejection']+'$'):
                o.inspect_witness(row['witness'])

    def test_valid_redirty_and_busy_ram_continuation(self):
        witness=o.valid_witness()
        witness.update(redirty=True,live_counter=2,final_counter=2,final_dpt_present=True)
        witness['events'].insert(witness['events'].index('finalize'),'finalize_busy')
        self.assertTrue(o.inspect_witness(witness))
        already_durable=copy.deepcopy(witness)
        already_durable['events'].remove('wal_durable')
        already_durable['events'].insert(0,'wal_durable')
        self.assertTrue(o.inspect_witness(already_durable))
        for event in ('wal_io','namespace_read','file_read','home_write'):
            damaged=copy.deepcopy(witness);damaged['events'].append(event)
            with self.assertRaisesRegex(ValueError,'N14'):o.inspect_witness(damaged)

    def test_effect_inventory_and_physical_prefix_contract(self):
        self.assertEqual(len(o.EFFECTS),31)
        self.assertEqual(o.WRITES,(1,3,6,10,12,14,17,21,23,25,28))
        self.assertEqual([r['ordinal'] for r in o.EFFECTS if r['kind']=='publish'],[8,19,30])
        self.assertEqual([r['ordinal'] for r in o.EFFECTS if r['kind']=='directory_sync'],[9,20,31])
        initial={'d3-dwb-stage':b'aaaa','d3-dwb-0':b'bbbb','d3-dwb-1':b'cccc','DWB':b'old',
                 'd3-dwb-images:slot0':b'img0','home:page0':b'home0'}
        phases=[{'bank':bytes([65+i])*4,'selector':bytes([88+i])*4,'bank_index':(i+1)%2} for i in range(3)]
        images={'slot':b'img1','home':b'home1'}
        before,facts=o.material_state(initial,images,phases,1,'before')
        self.assertEqual(before,initial);self.assertEqual((facts['attempted'],facts['completed']),(1,0))
        partial,_=o.material_state(initial,images,phases,1,'half')
        self.assertEqual(partial['d3-dwb-stage'],b'AAaa')
        create,_=o.material_state(initial,images,phases,5,'after')
        self.assertEqual(create['d3-dwb-select-stage'],b'')
        publish,facts=o.material_state(initial,images,phases,8,'after')
        self.assertEqual(publish['DWB'],b'XXXX');self.assertNotIn('d3-dwb-select-stage',publish)
        self.assertEqual(facts['possible_selection_indices'],[-1,0])
        before_publish,before_fact=o.material_state(initial,images,phases,8,'before')
        self.assertEqual(before_publish['DWB'],b'old')
        self.assertEqual(before_fact['possible_selection_indices'],[-1,0])
        _,before_dirsync=o.material_state(initial,images,phases,9,'before')
        self.assertEqual(before_dirsync['possible_selection_indices'],[-1,0])
        full,_=o.material_state(initial,images,phases)
        for mode in ('short','interrupted'):
            progressed,facts=o.material_state(initial,images,phases,1,mode)
            self.assertEqual(progressed,full);self.assertEqual(facts['completed'],31)

    def test_checked_inputs_reject_invalid_identity_and_counters(self):
        root=Path(__file__).resolve().parent.parent
        page=bytearray((root/o.DIRTY/'p1-n1-reserve.page').read_bytes())
        page[24]=2
        with self.assertRaisesRegex(ValueError,'target identity'):o.slot(1,bytes(page))
        for generation,batch in ((0,1),(2**64,1),(1,-1),(1,2**64)):
            with self.assertRaises(ValueError):o.control(generation,batch)

    def test_frozen_literals_and_census(self):
        root=Path(__file__).resolve().parent.parent
        manifest=json.loads((root/o.OUTPUT/'manifest.json').read_text())
        self.assertEqual(manifest['mandatory_cases'],76)
        self.assertEqual(manifest['s5_census'],{'normal':18,'after':558,'short':198,'interrupted':198})
        self.assertEqual(len(manifest['batches']),18)
        for row in manifest['literals']:
            data=(root/o.OUTPUT/row['path']).read_bytes()
            self.assertEqual((len(data),o.digest(data)),(row['bytes'],row['sha256']))
        self.assertEqual([b['batch'] for b in manifest['batches'][:6]],list(range(1,7)))
        self.assertEqual([p['generation'] for p in manifest['batches'][1]['phases']],[5,6,7])
        for b in manifest['batches']:
            self.assertEqual((root/o.OUTPUT/b['home_after']).read_bytes(),
                             (root/o.DIRTY/(b['id']+'.page')).read_bytes())


if __name__=='__main__':unittest.main()
