"""Known-bad tests for new closed-effects acceptance code; product execution0."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock,patch
import dev3_recovery_effects_native as r


class ClosedRecoveryNativeCheckerTests(unittest.TestCase):
    def test_explicit_selection_duplicates_alias_and_budget_classes(self):
        self.assertEqual(len(r.selected(None)),31)
        for names in ([],['typo'],['NR01','NR01'],['NR01','NS5001']):
            with self.subTest(names=names),self.assertRaises(ValueError):r.selected(names)
        alias,target=next(iter(r.oracle.ALIASES.items()))
        with self.assertRaises(ValueError):r.selected([alias,target])

    def test_exact_pid_mode_zero_runner_duplicate_protocol(self):
        text=f'test {r.SELECTOR} ... RECOVERY_EFFECTS_NATIVE_BEGIN mode=recover pid=12\nRECOVERY_EFFECTS_NATIVE_CASE mode=recover pid=12 outcome=recovered PASS\nRECOVERY_EFFECTS_NATIVE_COMPLETE count=1\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n'
        r.check_stdout(text,'recover',12,'recovered')
        for bad in ('',text+text,text.replace(r.SELECTOR,'wrong'),text.replace('pid=12','pid=13'),text.replace('1 passed','0 passed'),text.replace('count=1','count=0')):
            with self.subTest(bad=bad),self.assertRaises(ValueError):r.check_stdout(bad,'recover',12,'recovered')

    def test_worker_timeout_exit_and_forged_receipt(self):
        good=dict(timed_out=False,exit_code=0,elapsed_seconds=1);r.check_process(good)
        for change in (dict(timed_out=True),dict(exit_code=1),dict(elapsed_seconds=31),dict(elapsed_seconds=-1)):
            with self.subTest(change=change),self.assertRaises(ValueError):r.check_process(dict(good,**change))
        root=Path('/frozen');receipt=dict(status='PASS',source_sha256='s',binary_sha256='b',source_root=str(root),command=r.BUILD_COMMAND,exit_code=0,errors=0)
        r.check_build_receipt(receipt,root,'s','b')
        for change in (dict(source_sha256='x'),dict(binary_sha256='x'),dict(command=['wrong']),dict(source_root='/other'),dict(status='FAIL')):
            with self.subTest(change=change),self.assertRaises(ValueError):r.check_build_receipt(dict(receipt,**change),root,'s','b')
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'bad';path.write_bytes(b'not ELF')
            with self.assertRaises(ValueError):r.check_binary_format(path)

    def test_source_census_is_required_and_peak_cannot_be_forged(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp);file=path/'source-census.tsv'
            with self.assertRaises(ValueError):r.check_census(path)
            good='outer\t100\nprefix_nested\t200\ninventory\t300\nwork_nested\t400\nentry_phase\t450\nreturn_phase\t480\npeak\t500\nwriter\t32\nfile\t32\nmaintenance\t12582912\nflush\t4194304\n'
            file.write_text(good);r.check_census(path)
            for bad in (good.replace('inventory\t300\n',''),good.replace('peak\t500','peak\t499'),good.replace('outer\t100','outer\t65536').replace('peak\t500','peak\t65936'),good.replace('maintenance\t12582912','maintenance\t262144')):
                file.write_text(bad)
                with self.assertRaises(ValueError):r.check_census(path)

    def test_raw_durable_order_missing_sync_and_payload_mutants(self):
        before,o=r.oracle.prepared(r.ROOT,'NR01')
        expected=r.vectors.mutation_vector(before,o,'recover','none')
        rows=[[e['op'],e['name'],str(e['offset']),str(e['length']),('0:'+e['sha256']) if e['op']=='write' else 'OK'] for e in expected]
        r.vectors.check_raw_mutations(rows,expected)
        syncs=[i for i,e in enumerate(expected) if e['op'] in ('sync','dirsync')]
        for index in syncs:
            bad=rows[:index]+rows[index+1:]
            with self.subTest(missing=index),self.assertRaises(ValueError):r.vectors.check_raw_mutations(bad,expected)
        source=next(i for i,e in enumerate(expected) if e['op']=='sync' and e['name']==r.oracle.WAL)
        pending=next(i for i,e in enumerate(expected) if e['op']=='write' and e['name']==r.oracle.DATA)
        bad=[list(row) for row in rows];bad[source],bad[pending]=bad[pending],bad[source]
        with self.assertRaises(ValueError):r.vectors.check_raw_mutations(bad,expected)
        bad=[list(row) for row in rows];bad[1][-1]='65536:wrong'
        with self.assertRaises(ValueError):r.vectors.check_raw_mutations(bad,expected)

    def test_error_phase_cause_and_fault_call_are_exact(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp);expected=r.expected_error('NB02','boot0_partial')
            def save(row):(path/'error.tsv').write_text(''.join(f'{k}\t{v}\n' for k,v in row.items()))
            save(expected);r.check_error(path,'NB02','boot0_partial')
            for key,value in (('phase','Redo'),('cause','Invariant'),('effect','Unchanged'),('last_call','None'),('lease_effect','NotAcquired')):
                save(dict(expected,**{key:value}))
                with self.subTest(key=key),self.assertRaises(ValueError):r.check_error(path,'NB02','boot0_partial')
            before,o=r.oracle.prepared(r.ROOT,'NB02');ledger=r.oracle.failed_facts(before,o,'boot0_partial','boot2')
            (path/'observation.tsv').write_text(''.join(f'{k}\t{v}\n' for k,v in dict(ledger,actual_owned=ledger['actual_owned']+32768).items()))
            with self.assertRaises(ValueError):r.check_failed_accounting(path,before,o,'boot0_partial','boot2')

    def test_full_bytes_and_original_inode_cannot_be_replaced(self):
        with tempfile.TemporaryDirectory() as temp:
            db=Path(temp);(db/'LOCK').write_bytes(b'');(db/'file').write_bytes(b'abc');before=r.compare_files(db,{'LOCK':b'','file':b'abc'})
            (db/'file').write_bytes(b'abd')
            with self.assertRaises(ValueError):r.compare_files(db,{'LOCK':b'','file':b'abc'})
            fake={name:dict(row) for name,row in before.items()};fake['file']['inode']+=1
            with self.assertRaises(ValueError):r.preserved_inodes(before,fake)


    def test_wait_value_error_kills_and_reaps(self):
        child=Mock()
        with patch.object(r,'wait_child',side_effect=ValueError('expired')),patch.object(r,'kill_and_reap') as reap:
            with self.assertRaises(ValueError):r.finish_child(child,lambda:0,{})
            reap.assert_called_once_with(child)

    def test_wait_keyboard_interrupt_kills_and_reaps(self):
        child=Mock()
        with patch.object(r,'wait_child',side_effect=KeyboardInterrupt),patch.object(r,'kill_and_reap') as reap:
            with self.assertRaises(KeyboardInterrupt):r.finish_child(child,lambda:1,{})
            reap.assert_called_once_with(child)

    def test_post_identity_elapsed_time_is_in_budget(self):
        with patch.object(r.time,'monotonic',return_value=301):
            with self.assertRaises(ValueError):r.check_final_budget(0,300,0)

    def test_log_space_is_bounded_at_final_pass(self):
        with patch.object(r.time,'monotonic',return_value=1):
            with self.assertRaises(ValueError):r.check_final_budget(0,300,r.LOG_LIMIT+1)

    def test_full_call_order_rejects_missing_metadata_and_readback(self):
        before,o=r.oracle.prepared(r.ROOT,'NR04');expected=r.vectors.effect_calls(before,o,'recover','none')
        rows=[[*map(str,r.vectors.raw_call(c)),'OK'] for c in expected]
        r.vectors.check_call_order(rows,expected)
        for i,c in enumerate(expected):
            if c['kind'] not in ('Metadata','Read'):continue
            with self.subTest(i=i),self.assertRaises(ValueError):r.vectors.check_call_order(rows[:i]+rows[i+1:],expected)

    def test_full_call_order_rejects_wrong_chunk_offset(self):
        expected=r.vectors.boot_calls([1048576,1048576]);rows=[[*map(str,r.vectors.raw_call(c)),'OK'] for c in expected]
        i=next(i for i,c in enumerate(expected) if c['kind']=='Read' and c['offset']==65536)
        rows[i][2]='0'
        with self.assertRaises(ValueError):r.vectors.check_call_order(rows,expected)

    def test_full_call_order_requires_inventory(self):
        with self.assertRaises(ValueError):r.vectors.check_call_order([],r.vectors.boot_calls([None,None]))

    def test_read_hash_binds_actual_range_not_only_file_hash(self):
        with self.assertRaises(ValueError):r.vectors.check_read_bytes([['read','file','1','2','2:'+r.oracle.sha(b'ab')]],{'file':b'abc'},[])

    def test_database_limit_includes_reserved_files(self):
        with tempfile.TemporaryDirectory() as temp:
            db=Path(temp)
            with (db/'d3-rebase-0').open('wb') as stream:stream.truncate(r.DB_LIMIT+1)
            with self.assertRaises(ValueError):r.inventory(db)

    def test_producer_cut_cannot_accept_wrong_ordinal_or_missed_fault(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp);(path/'producer-error.txt').write_text('CallerMustResolve closed recovery native after-effect unknown')
            for resources in (dict(cut_hits=0,cut_ordinal=19),dict(cut_hits=1,cut_ordinal=18)):
                with self.subTest(resources=resources),self.assertRaises(ValueError):r.check_producer_cut(path,dict(context='historical'),resources)
            good='phase: Selection { index: 1, step: SelectorSync }, cause: Io(Error { code: IoWriteUnknown, context: "sync outcome unknown", io_kind: Some(Other) }), effect: CallerMustResolve'
            (path/'producer-error.txt').write_text(good)
            (path/'events.tsv').write_text('sync\td3-dwb-select-stage\t0\t0\tOK\n')
            r.check_producer_cut(path,dict(context='stage4096'),dict(cut_hits=1,cut_ordinal=18))
            for bad in (good.replace('sync outcome unknown','write outcome requires caller resolution'),good.replace('index: 1','index: 0'),'CallerMustResolve closed recovery native after-effect unknown'):
                (path/'producer-error.txt').write_text(bad)
                with self.subTest(error=bad),self.assertRaises(ValueError):r.check_producer_cut(path,dict(context='stage4096'),dict(cut_hits=1,cut_ordinal=18))
            (path/'producer-error.txt').write_text(good)
            (path/'events.tsv').write_text('sync\td3-dwb-stage\t0\t0\tOK\n')
            with self.assertRaises(ValueError):r.check_producer_cut(path,dict(context='stage4096'),dict(cut_hits=1,cut_ordinal=18))


if __name__=='__main__':unittest.main()
