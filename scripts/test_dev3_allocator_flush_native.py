#!/usr/bin/env python3
"""New native flush evidence checks; no native/product execution."""
from copy import deepcopy
import errno
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock,patch
import dev3_allocator_flush_native as n

ROOT=Path(__file__).resolve().parent.parent


class FlushNativeChecks(unittest.TestCase):
    def test_case_selection_and_exact_worker_marker(self):
        self.assertEqual(n.select(),[1,2,3]);self.assertEqual(n.select([3,1]),[1,3])
        for bad in ([],[1,1],[0],[4]):
            with self.assertRaisesRegex(ValueError,'FAIL_NATIVE_CENSUS'):n.select(bad)
        text=('running 1 test\ntest '+n.SELECTOR+' ... FLUSH_NATIVE_COMPLETE profile=2 batches=2 effects=62 bytes=12345 peak_files=2\nok\n'
              'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.01s\n')
        actual=n.check_stdout(text,2,12345);self.assertEqual(actual['tracked_wrapper_and_namespace_bound'],4)
        for old,new in [('profile=2','profile=1'),('batches=2','batches=1'),('effects=62','effects=31'),
                        ('bytes=12345','bytes=12346'),('peak_files=2','peak_files=3'),
                        ('1 passed;','2 passed;'),(n.SELECTOR,'wrong::selector')]:
            with self.assertRaises(ValueError):n.check_stdout(text.replace(old,new),2,12345)
        with self.assertRaises(ValueError):n.check_stdout(text+text,2,12345)
        with self.assertRaises(ValueError):n.check_stdout(text+'FLUSH_NATIVE_COMPLETE malformed\n',2,12345)

    def test_exact_two_batch_mutation_sequence(self):
        for batch in (1,2):
            rows=n.expected_events(batch);self.assertEqual(len(rows),31)
            self.assertEqual(rows[9],['write','d3-dwb-images','0','16544','16544'])
            self.assertEqual(rows[20],['write',n.HOME,'0','16384','16384'])
            self.assertEqual([i+1 for i,r in enumerate(rows) if r[0]=='publish'],[8,19,30])
            self.assertEqual([i+1 for i,r in enumerate(rows) if r[0]=='dirsync'],[9,20,31])
            self.assertEqual(rows[2][1],'d3-dwb-'+str(batch%2))
            n.check_events(rows,batch)
            for index in range(31):
                for column in range(5):
                    damaged=deepcopy(rows);damaged[index][column]='wrong'
                    with self.assertRaisesRegex(ValueError,'FAIL_FLUSH_EFFECT_SEQUENCE'):n.check_events(damaged,batch)
            for damaged in (rows[:-1],rows+[rows[-1]],rows[::-1]):
                with self.assertRaises(ValueError):n.check_events(damaged,batch)

    def test_clean_keeps_lsn_counter_and_removes_claim(self):
        for batch in (1,2):
            values=[str(batch),str(batch),'true','true','true',str(batch),str(3*batch+1)]
            n.check_clean(values,batch)
            for index in range(7):
                bad=values.copy();bad[index]='0'
                with self.assertRaisesRegex(ValueError,'FAIL_CLEAN_FINALIZE'):n.check_clean(bad,batch)

    def test_file_bytes_identity_alias_and_replacement(self):
        initial={'LOCK':(0,n.sha(b'')),'DWB':(4096,'a'*64),'home':(10,'b'*64)}
        before={name:[length,7,index,1,checksum] for index,(name,(length,checksum)) in enumerate(initial.items(),10)}
        after=deepcopy(before);after['DWB'][2]=99
        n.check_inventory(before,after,initial,initial)
        # Recycled DWB inode is legal; all other existing objects must be stable.
        n.check_inventory(before,before,initial,initial)
        changes=[('home',2,55),('home',3,2),('home',0,11),('home',4,'c'*64),('DWB',1,8)]
        for name,index,value in changes:
            bad=deepcopy(after);bad[name][index]=value
            with self.assertRaises(ValueError):n.check_inventory(before,bad,initial,initial)
        alias=deepcopy(after);alias['home'][1:3]=alias['DWB'][1:3]
        with self.assertRaisesRegex(ValueError,'FAIL_FILE_ALIAS'):n.check_inventory(before,alias,initial,initial)
        extra=deepcopy(after);extra['stage']=after['home']
        with self.assertRaisesRegex(ValueError,'FAIL_FILE_CENSUS'):n.check_inventory(before,extra,initial,initial)

    def test_literal_initial_and_two_batch_final_expectations(self):
        installed=json.loads((ROOT/n.INSTALL/'manifest.json').read_text())
        for profile in (1,2,3):
            initial=n.initial_expected(ROOT,profile)
            self.assertEqual(initial['DWB'][1],installed['installed_files'][str(profile)]['DWB']['sha256'])
            # This helper only needs the header content; manifest validation is
            # separate in evaluate_case. The exact concatenation length is fixed.
            header=bytes(4096)
            for batch,part in ((1,'reserve'),(2,'cancel')):
                expected=n.expected_after(ROOT,profile,batch,header)
                self.assertEqual(expected[n.WAL][0],4096+batch*21200)
                self.assertEqual(expected[n.HOME][0],1048576)
                self.assertEqual(expected['d3-dwb-images'][0],16711680)
                self.assertEqual(expected['d3-profile'],initial['d3-profile'])
                selector=(ROOT/n.FIXTURES/f'p{profile}-n1-{part}-retirement.selector').read_bytes()
                self.assertEqual(expected['DWB'],(4096,n.sha(selector)))
                self.assertNotIn('d3-dwb-select-stage',expected)
        self.assertEqual(n.zero_tail_hash(b'abc',7),hashlib.sha256(b'abc'+bytes(7)).hexdigest())

    def test_manifest_types_and_fixed_plan(self):
        plan=json.loads((ROOT/n.FIXTURES/'native-manifest.json').read_text())
        self.assertEqual(plan,n.native_plan())
        self.assertEqual((plan['database_limit_bytes'],plan['total_limit_bytes']),(24<<20,72<<20))
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'before.tsv'
            row='LOCK\t0\t7\t8\t1\t'+n.sha(b'')+'\n';path.write_text(row)
            self.assertEqual(n.parse_manifest(path),{'LOCK':[0,7,8,1,n.sha(b'')]})
            for data in (row+row,row.replace('LOCK','../escape'),row.replace('\t1\t','\t2\t'),row.replace(n.sha(b''),'bad')):
                path.write_text(data)
                with self.assertRaises(ValueError):n.parse_manifest(path)

    def test_fixed_footprint_separates_actual_wal_append_growth(self):
        for profile in (1,2,3):
            initial=n.initial_expected(ROOT,profile)
            initial_bytes=sum(row[0] for row in initial.values())
            growth=sum((ROOT/n.DIRTY/f'p{profile}-n1-{part}.frame').stat().st_size for part in ('reserve','cancel'))
            self.assertEqual((initial_bytes,initial[n.WAL][0],growth),(22069248,4096,42400))
            actual=n.check_resources(initial_bytes+growth,initial[n.WAL][0]+growth)
            self.assertEqual(actual['fixed_owned_charge_bytes'],22073344)
            self.assertEqual(actual['wal_bytes'],46496)
            self.assertEqual(actual['wal_append_bytes'],42400)
            self.assertEqual(actual['replacement_peak_bound_bytes'],22115744)
            # A fixed object growing one byte must not hide in the WAL exemption.
            with self.assertRaisesRegex(ValueError,'FAIL_FIXED_OWNED_BOUND'):
                n.check_resources(initial_bytes+growth+1,initial[n.WAL][0]+growth)
            # The append exemption cannot bypass the actual DB/replacement limit.
            extra=n.DB_LIMIT-(initial_bytes+growth)-n.SELECTOR_STAGE+1
            with self.assertRaisesRegex(ValueError,'FAIL_DATABASE_BUDGET'):
                n.check_resources(initial_bytes+growth+extra,initial[n.WAL][0]+growth+extra)
        for database,wal in ((10,4096),(4096,4095),(4096,-1)):
            with self.assertRaisesRegex(ValueError,'FAIL_WAL_RESOURCE_SHAPE'):n.check_resources(database,wal)

    def test_actual_fd_boundary_census_and_limits(self):
        rows=[['baseline','3']]
        for batch in (1,2):
            rows.extend([['open','4'],['read','4']])
            rows.extend([[row[0],'4'] for row in n.expected_events(batch)])
            rows.append(['close','3'])
        rows.append(['final','0'])
        actual=n.check_fd('3\t4\t0\n',rows)
        self.assertEqual((actual['baseline'],actual['observed_boundary_peak'],actual['final']),(3,4,0))
        for summary in ('4\t4\t0\n','3\t6\t0\n','3\t4\t1\n','3\t5\t0\n','3\t4\t0','3\t4\t0\nextra'):
            with self.assertRaises(ValueError):n.check_fd(summary,rows)
        variants=[rows[1:],rows[:-1],rows+[rows[-1]],rows[::-1]]
        for tag in ('open','read','close'):
            variants.append([row for row in rows if row[0]!=tag])
        for index,value in ((1,['open','6']),(2,['read','bad']),(3,['unknown','4']),(3,['sync','4'])):
            damaged=deepcopy(rows);damaged[index]=value;variants.append(damaged)
        for damaged in variants:
            with self.assertRaises(ValueError):n.check_fd('3\t4\t0\n',damaged)

    def test_proc_unavailable_requires_same_child_terminal_observation(self):
        denied=PermissionError(errno.EACCES,'denied','/proc/77/fd')
        for returns,stopped in (([0],False),([None,0],False),([-9],True)):
            child=SimpleNamespace(pid=77,poll=Mock(side_effect=returns))
            stop=Mock();stop.is_set.return_value=stopped
            observed={'proc_terminal_unavailable':[]}
            self.assertTrue(n.terminal_proc_unavailable(denied,child,stop,observed))
            row=observed['proc_terminal_unavailable'][0]
            self.assertEqual((row['errno'],row['path'],row['poll_before'],row['poll_after']),
                             (errno.EACCES,'/proc/77/fd',returns[0],returns[-1]))
            self.assertEqual(row['classification'],'PROC_OBSERVATION_UNAVAILABLE_WITH_TERMINAL_CHILD')
            self.assertEqual(row['stop_before'],stopped)
            self.assertEqual(stop.wait.call_count,1 if returns[0] is None else 0)
        for stopped in (False,True):
            child=SimpleNamespace(pid=77,poll=Mock(return_value=None));stop=Mock();stop.is_set.return_value=stopped
            observed={'proc_terminal_unavailable':[]}
            self.assertFalse(n.terminal_proc_unavailable(denied,child,stop,observed))
            self.assertEqual(observed['proc_terminal_unavailable'],[])
        child=SimpleNamespace(pid=77,poll=Mock(return_value=0));stop=Mock();stop.is_set.return_value=False
        self.assertTrue(n.terminal_proc_unavailable(PermissionError(errno.EACCES,'denied','/proc/77/fd/4'),
                                                  child,stop,{'proc_terminal_unavailable':[]}))
        for error in (PermissionError(errno.EACCES,'denied','/proc/78/fd'),
                      PermissionError(errno.EACCES,'denied','/proc/77/fd-other'),
                      PermissionError(errno.EACCES,'denied','/proc/77/fd/../status'),
                      PermissionError(errno.EACCES,'denied','/tmp/database/home'),
                      OSError(errno.EIO,'I/O failure','/proc/77/fd')):
            child=SimpleNamespace(pid=77,poll=Mock(return_value=0));stop=Mock()
            self.assertFalse(n.terminal_proc_unavailable(error,child,stop,{'proc_terminal_unavailable':[]}))
            child.poll.assert_not_called()
        # Exercise the sampler's actual exception dispatch without a child process
        # or /proc access. Persistent live denial remains an ordinary failure.
        for returns,expected_errors,expected_unavailable in (([None,None,0],0,1),([None,None,None],1,0)):
            child=SimpleNamespace(pid=77,poll=Mock(side_effect=returns));stop=Mock();stop.is_set.return_value=False
            observed={'sample_errors':[],'proc_terminal_unavailable':[]}
            with patch.object(n.Path,'iterdir',side_effect=denied):
                n.sample_process(child,Path('/tmp/database'),stop,observed)
            self.assertEqual(len(observed['sample_errors']),expected_errors)
            self.assertEqual(len(observed['proc_terminal_unavailable']),expected_unavailable)


if __name__=='__main__':unittest.main()
