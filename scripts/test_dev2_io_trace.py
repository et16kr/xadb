#!/usr/bin/env python3
"""Select new P3 analysis vectors or one explicit root-owner native case."""
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import dev2_io_analysis as analysis
import dev2_io_supervisor as supervisor
import dev2_io_trace as trace


def raw(rows):
    counts = Counter(analysis.KINDS.index(row.split()[0]) for row in rows)
    return '\n'.join(rows + ['DISABLED 1000'] + [f'@counts[{k}]: {v}' for k,v in sorted(counts.items())])+'\n'


BASE = ['FS_IN 110 7 99 42', 'FS_OUT 190 7 99 42 0', 'WAIT_IN 120 7 99 8', 'WAIT_OUT 180 7 99 8 0',
        'ISSUE 125 1234 2 0 18446744073709551615', 'COMPLETE 170 1234 0 0 0']


def analyze(rows, **kwargs):
    return analysis.analyze(raw(rows), **(dict(start=100,end=200,worker=7,device=99,inode=42,expected=1)|kwargs))


class AnalysisVectors(unittest.TestCase):
    def test_union_clip_and_shared_overlap(self):
        self.assertEqual(analysis.union([(3,6),(1,4),(6,9),(12,15)]),[(1,9),(12,15)])
        self.assertEqual(analysis.overlap([(0,10),(5,15)],[(3,7),(12,20)]),7)
        value=analyze(BASE)
        self.assertEqual((value['fsync_ns'],value['fsync_journal_overlap_ns'],value['request_overlap_ns']['flush']),(80,60,45))
        self.assertEqual(value['zero_byte_flush_count'],1)

    def test_cross_cpu_order_and_pointer_reuse(self):
        rows=BASE+['ISSUE 175 1234 131073 8 50','COMPLETE 185 1234 0 8 8']
        value=analyze(list(reversed(rows)))
        self.assertEqual(value['request_count'],2)
        self.assertEqual(value['request_overlap_ns']['fua'],10)
        self.assertEqual(value['request_overlap_ns']['all_requests'],55)

    def test_partial_completion_withholds_proportions(self):
        rows=BASE[:4]+['ISSUE 125 5 1 8 0','COMPLETE 140 5 0 3 8','COMPLETE 170 5 0 5 5']
        self.assertIsNone(analyze(rows)['request_overlap_ns'])

    def test_partial_before_window_taints_whole_generation(self):
        rows=BASE[:4]+['ISSUE 50 5 1 8 0','COMPLETE 80 5 0 3 8','COMPLETE 170 5 0 5 5']
        self.assertIsNone(analyze(rows)['request_overlap_ns'])

    def test_orphan_after_window_can_cover_measurement(self):
        self.assertIsNone(analyze(BASE+['COMPLETE 250 8 0 8 8'])['request_overlap_ns'])

    def test_launch_rejects_reparented_child_before_exec(self):
        with patch.object(supervisor.os,'geteuid',return_value=0),patch.object(supervisor.os,'getppid',return_value=1),patch.object(supervisor.os,'execve') as execute:
            with self.assertRaisesRegex(RuntimeError,'parent lost'): supervisor.collector_exec(9,Path('/dev/shm/xadb-io-test'))
            execute.assert_not_called()
        with patch.object(supervisor.os,'geteuid',return_value=0),patch.object(supervisor.os,'getppid',return_value=9),patch.object(supervisor.os,'execve') as execute:
            supervisor.collector_exec(9,Path('/dev/shm/xadb-io-test'))
            self.assertEqual(execute.call_args.args[0],'/usr/bin/bpftrace')

    def test_ambiguous_ties_and_duplicate_interval_rejected(self):
        for rows in (BASE+['ISSUE 125 1234 2 0 0'], BASE+['FS_IN 130 7 99 42'],BASE+['FS_OUT 190 7 99 42 0']):
            with self.subTest(rows=rows), self.assertRaises(ValueError): analyze(rows)

    def test_orphan_requeue_error_open_request_withhold_proportions(self):
        for extra in (['COMPLETE 150 8 0 0 0'],['ISSUE 130 8 1 8 0','REQUEUE 150 8'],
                      ['ISSUE 130 8 1 8 0','COMPLETE 150 8 1 8 8'],['ISSUE 130 8 1 8 0'],
                      ['ISSUE 130 8 1 8 0','COMPLETE 150 8 0 9 8'],
                      ['ISSUE 130 8 1 8 0','ISSUE 140 8 1 8 0','COMPLETE 170 8 0 8 8']):
            with self.subTest(extra=extra):
                value=analyze(BASE+extra)
                self.assertEqual(value['status'],'INCONCLUSIVE_REQUEST_COVERAGE')
                self.assertIsNone(value['request_overlap_ns'])

    def test_boundary_request_clipping_and_explicit_outside_unknown(self):
        value=analyze(BASE+['ISSUE 90 8 1 8 0','COMPLETE 210 8 0 8 8','COMPLETE 50 9 0 0 0'])
        self.assertEqual(value['request_overlap_ns']['all_requests'],80)
        self.assertEqual(value['boundary_unknown_count'],1)

    def test_measurement_inode_count_clock_and_return_fail_closed(self):
        for values in ({'expected':512},{'inode':43},{'device':100},{'worker':8},{'start':150},{'end':150},{'start':200}):
            with self.subTest(values=values),self.assertRaises(ValueError): analyze(BASE,**values)
        for rows in (BASE[1:], BASE[:-1]+['FS_IN 195 7 99 42'],[v.replace('42 0','42 -5') for v in BASE]):
            with self.subTest(rows=rows),self.assertRaises(ValueError): analyze(rows)

    def test_loss_count_overflow_unknown_record_rejected(self):
        text=raw(BASE)
        for mutated in (text.replace('@counts[5]: 1','@counts[5]: 2'),text+'@counts[5]: 1\n',
                        text+'Lost 1 events\n', text+'WARNING: map full\n',text.replace('DISABLED 1000',''),
                        text.replace('DISABLED 1000','DISABLED 150'),text.replace('ISSUE 125','ALIEN 125')):
            with self.subTest(mutated=mutated),self.assertRaises(ValueError): analysis.parse(mutated)
        # New bound parser is exercised without constructing a product workload.
        with self.assertRaisesRegex(ValueError,'event bound'):
            analysis.parse('\n'.join(['REQUEUE 1 8']*200001))

    def test_fixed_probe_parameters_and_clock(self):
        program=supervisor.render(7,99,259,6)
        self.assertNotIn('__WORKER__',program)
        self.assertIn('rawtracepoint:vmlinux:block_rq_complete',program)
        self.assertIn('nsecs(monotonic)',program)
        for value in ('7;system()',-1,2**32,True):
            with self.subTest(value=value),self.assertRaises(ValueError): supervisor.render(value,99,259,6)

    def test_close_failure_still_reaps_and_preserves(self):
        from unittest.mock import Mock
        with tempfile.TemporaryDirectory() as d:
            spool=Path(d)/'spool';spool.mkdir()
            (spool/'supervisor.json').write_text(json.dumps(dict(collector_gone=True,group_gone=True)))
            owner=trace.Trace.__new__(trace.Trace)
            owner.closed=False;owner.process=Mock();owner.err=Mock();owner.spool=spool
            owner.tools={};owner.command=[];owner.messages=[]
            owner.process.stdin.close.side_effect=BrokenPipeError('injected')
            with self.assertRaisesRegex(trace.window.DiagnosticError,'cleanup unconfirmed'): owner.finish(Path(d)/'saved')
            owner.process.wait.assert_called_once_with(timeout=8)
            self.assertTrue((Path(d)/'saved/supervisor.json').exists())
            self.assertIn('BrokenPipeError',(Path(d)/'saved/launcher.json').read_text())

    def test_constructor_setup_failure_closes_and_reaps(self):
        from unittest.mock import Mock
        with tempfile.TemporaryDirectory() as d:
            process=Mock();process.stdin.close.side_effect=BrokenPipeError('injected close')
            process.stdout.fileno.return_value=99
            with patch.object(trace.tempfile,'mkdtemp',return_value=d),patch.object(trace.window,'starttime',return_value=1),patch.object(trace.subprocess,'Popen',return_value=process),patch.object(trace.os,'set_blocking',side_effect=OSError('injected setup')):
                with self.assertRaisesRegex(trace.window.DiagnosticError,'construction failed'): trace.Trace(7,99,259,6)
            process.wait.assert_called_once_with(timeout=8)
            self.assertIn('injected setup',(Path(d)/'constructor-error.json').read_text())

    def test_blocked_payload_obeys_deadline_and_closes(self):
        from unittest.mock import Mock
        child=Mock();child.exited.return_value=False;child.process.stdin.fileno.return_value=99
        tick=Mock(side_effect=[None,trace.window.DiagnosticError('deadline exceeded')])
        with patch.object(trace.os,'set_blocking'),patch.object(trace.os,'write',side_effect=BlockingIOError):
            with self.assertRaisesRegex(trace.window.DiagnosticError,'deadline'): trace.send_payload(child,b'x'*65536,tick)
        child.process.stdin.close.assert_called_once()

    def test_payload_only_after_ready_and_done_before_trace_stop(self):
        calls=[]
        class FakeTrace:
            def __init__(self,*args): calls.append('attach')
            def tick(self): calls.append('ready'); return True
            def stop(self,tick): calls.append('stop')
        class Input:
            def write(self,data): calls.append(('payload',data))
            def close(self): calls.append('close')
        class Child:
            pid=7
            class process:
                stdin=Input()
        import io
        with tempfile.TemporaryDirectory() as d,patch.object(trace,'Trace',FakeTrace),patch.object(trace.window.Runner,'spawn',return_value=Child()),patch.object(trace.window.Runner,'send',side_effect=lambda *args:calls.append('DONE')),patch.object(trace,'send_payload',side_effect=lambda c,p,t:(c.process.stdin.write(p),c.process.stdin.close())):
            runner=trace.IoRunner(Path(d),True,99,259,6)
            runner.spawn([],Path(d),{},io.BytesIO(b'frame'),'worker',100)
            runner.send(None,'DONE token',0)
        self.assertEqual(calls,['attach','ready',('payload',b'frame'),'close','DONE','stop'])


class ProvenanceVectors(unittest.TestCase):
    BASE = BASE[:4] + ['ISSUE 125 1234 2 0 0 3 55', 'COMPLETE 170 1234 0 0 0 2 3 55']
    LOGICAL = ['ISSUE 130 9 131073 4096 0 3 66', 'COMPLETE 150 9 0 4096 4096 131073 3 66',
               'SEQ 151 9 5 2 0 3 131073 0 66 0', 'COMPLETE 152 9 0 0 0 131073 1 0']

    def check(self, rows):
        return analysis.analyze('SCHEMA 2\n'+raw(rows),start=100,end=200,worker=7,device=99,inode=42,expected=1)

    def test_v1_keeps_original_shape_and_verdict(self):
        value = analyze(BASE)
        self.assertNotIn('schema_version', value)
        self.assertEqual(value['request_overlap_ns']['all_requests'],45)
        self.assertIsNone(analyze(BASE+['COMPLETE 175 1234 0 0 0'])['request_overlap_ns'])

    def test_physical_logical_census_and_reuse(self):
        value = self.check(self.BASE+self.LOGICAL+[
            'ISSUE 175 9 1 8 0 1 0', 'COMPLETE 185 9 0 8 8 1 1 0'])
        self.assertEqual(value['completion_census'],dict(physical=3,logical=1,unknown=0,total=4))
        self.assertEqual(value['request_overlap_ns']['all_requests'],55)
        self.assertEqual(value['request_overlap_ns']['fua'],20)
        self.assertEqual(value['request_count'],3)

    def test_zero_and_multiple_action_masks_are_legal(self):
        for before, action in ((7,0),(0,7),(1,6),(3,4),(5,2),(6,1)):
            rows = self.BASE+[f'SEQ 175 9 {before} {action} 0 2 1 0 66 0','COMPLETE 176 9 0 0 0 1 0 0']
            self.assertEqual(self.check(rows)['completion_census']['logical'],1)
        value=self.check(self.BASE+['SEQ 175 9 0 0 0 2 1 4096 66 0'])
        self.assertEqual(value['status'],'CONNECTED_DIAGNOSTIC_ONLY')

    def test_repeated_marker_collision_taints_until_new_issue(self):
        rows = self.BASE+[
            'SEQ 80 9 5 2 0 3 131073 0 66 0',
            'SEQ 90 9 5 2 0 3 131073 0 66 0',
            'SEQ 151 9 5 2 0 3 131073 0 66 0',
            'COMPLETE 152 9 0 0 0 131073 1 0']
        value=self.check(rows)
        self.assertIsNone(value['request_overlap_ns'])
        self.assertEqual(value['completion_census'],dict(physical=1,logical=0,unknown=1,total=2))
        self.assertIsNone(self.check(rows[:-1])['request_overlap_ns'])
        extra=['SEQ 155 9 5 2 0 3 131073 0 66 0','COMPLETE 156 9 0 0 0 131073 1 0']
        self.assertEqual(self.check(rows+extra)['completion_census']['unknown'],2)
        value=self.check(rows[:8]+['ISSUE 153 9 1 8 0 1 0','COMPLETE 160 9 0 8 8 1 1 0'])
        self.assertEqual(value['completion_census']['physical'],2)

    def test_invalid_marker_fields_and_logical_mismatches_stay_unknown(self):
        marker=self.LOGICAL[2];complete=self.LOGICAL[3]
        bad_markers=[marker.replace('5 2 0','5 1 0'),marker.replace('5 2 0','8 2 0'),
                     marker.replace('5 2 0','5 2 1'),marker.replace('0 3 131073','0 1 131073')]
        bad_ends=[complete.replace('131073 1 0','131073 3 0'),complete.replace('131073 1 0','131073 0 0'),
                  complete.replace('131073 1 0','131073 1 9'),complete.replace('131073','1'),
                  complete.replace('9 0 0 0','9 1 0 0'),complete.replace('9 0 0 0','9 0 0 1')]
        cases=[self.LOGICAL[:2]+[m,complete] for m in bad_markers]+[self.LOGICAL[:3]+[c] for c in bad_ends]
        cases += [self.LOGICAL[:2]+[complete],self.LOGICAL[:3],
                  self.LOGICAL[:3]+[marker.replace('151','250')],
                  self.LOGICAL[:3]+[marker.replace('151','152'),complete.replace('152','153')],
                  self.LOGICAL[:3]+['ISSUE 152 9 1 0 0 1 0',complete.replace('152','153')]]
        for case in cases:
            with self.subTest(case=case):
                value=self.check(self.BASE+case)
                self.assertIsNone(value['request_overlap_ns'])
                self.assertEqual(value['completion_census']['logical'],0)

    def test_invalid_generation_cannot_recover_without_new_issue(self):
        endings=['SEQ 151 9 5 2 0 3 131073 0 66 0','COMPLETE 152 9 0 0 0 131073 1 0']
        bad_prefixes=[['SEQ 80 9 5 1 0 3 131073 0 66 0'],
                      ['SEQ 80 9 5 2 0 3 131073 0 66 0','COMPLETE 90 9 0 0 0 131073 3 0']]
        for prefix in bad_prefixes:
            value=self.check(self.BASE+prefix+endings)
            self.assertIsNone(value['request_overlap_ns'])
            self.assertEqual(value['completion_census']['logical'],0)

    def test_partial_boundary_ties_and_v2_shape(self):
        rows=self.BASE+['ISSUE 50 9 1 8 0 1 0','COMPLETE 80 9 0 3 8 1 1 0','COMPLETE 175 9 0 5 5 1 1 0']
        value=self.check(rows)
        self.assertIsNone(value['request_overlap_ns'])
        self.assertEqual(value['completion_census']['unknown'],2)
        for rows in (self.BASE+self.LOGICAL+[self.LOGICAL[2]],self.BASE+['ISSUE 125 1234 2 0 0 3 55']):
            with self.assertRaisesRegex(ValueError,'ambiguous'): self.check(rows)
        text='SCHEMA 2\n'+raw(self.BASE)
        self.assertEqual(analysis.parse(text),analysis.parse(raw(self.BASE)+'SCHEMA 2\n'))
        for value in (text.replace('SCHEMA 2','SCHEMA 3'),text+'SCHEMA 2\n',text.replace(' 2 3 55','')):
            with self.assertRaises(ValueError): analysis.parse(value)
        self.assertEqual(self.check(self.BASE+['COMPLETE 50 9 0 0 0 1 0 0'])['boundary_unknown_count'],1)

    def test_journal_partition_signed_return_and_incomplete(self):
        names=['start_commit','commit_locking','commit_flushing','commit_logging','end_commit']
        phases=[f'PHASE {t} 11 tracepoint:jbd2:jbd2_{name} 99 8' for t,name in zip([115,125,140,150,175],names)]
        value=self.check(self.BASE+phases)['journal_phases']
        self.assertEqual(value['overlap_ns'],dict(start_to_locking=5,locking_to_flushing=15,flushing_to_logging=10,logging_to_end=25,before_start=0,after_end=5))
        self.assertEqual(value['connected'][0]['return_minus_end_ns'],5)
        value=self.check([r.replace('WAIT_OUT 180','WAIT_OUT 170') for r in self.BASE]+phases)['journal_phases']
        self.assertEqual(value['connected'][0]['return_minus_end_ns'],-5)
        self.assertEqual(value['overlap_ns']['logging_to_end'],20)
        for bad in (phases[:-1],phases+[phases[0]],phases[:2]+[phases[2].replace('140','125')]+phases[3:],
                    [phases[0].replace('115','145')]+phases[1:]):
            value=self.check(self.BASE+bad)['journal_phases']
            self.assertIsNone(value['overlap_ns'])
            self.assertEqual(len(value['unknown']),1)
        outside=[f'PHASE {t} 11 tracepoint:jbd2:jbd2_{name} 99 8' for t,name in zip([90,95,145,195,210],names)]
        self.assertEqual(self.check(self.BASE+outside)['journal_phases']['overlap_ns']['flushing_to_logging'],35)

    def test_v2_probe_contract(self):
        program=supervisor.render(7,99,259,6)
        self.assertEqual(supervisor.PROBES,16)
        self.assertIn('SCHEMA 2',program)
        self.assertIn('fentry:vmlinux:blk_flush_complete_seq',program)
        self.assertNotIn('fexit:vmlinux:blk_flush_complete_seq',program)
        self.assertIn('$r->flush.saved_end_io',program)


class CompilerVectors(unittest.TestCase):
    def test_printf_payloads_use_invocation_stack(self):
        import re
        root=Path(os.environ['XADB_IO_TEST_ROOT'])/'stack-compile'
        root.mkdir()
        for name in ('sudo','timeout','bpftrace'):
            trace.window.trusted_tool('/usr/bin/'+name)
        program=root/'program.bt'
        program.write_text(supervisor.render(7,271581192,259,6))
        command=['/usr/bin/sudo','-n','--','/usr/bin/timeout','--kill-after=1s','20s',
                 '/usr/bin/bpftrace','--mode','codegen','--emit-llvm',str(root/'compiled'),str(program)]
        with (root/'compiler.log').open('x') as stream:
            result=subprocess.run(command,stdout=stream,stderr=subprocess.STDOUT,timeout=25)
        (root/'command.json').write_text(json.dumps(dict(command=command,returncode=result.returncode,
             source_sha256=trace.window.digest(program),scope='codegen only; no attach'),indent=2))
        self.assertEqual(result.returncode,0)
        ir=(root/'compiled.optimized.ll').read_text()
        self.assertNotIn('__bt__fmt_str_buf',ir)
        # BEGIN, disable, fsync2, wait2, journal5, block3, sequence1.
        allocations=re.findall(r'%printf_args = alloca %printf_t(?:\.\d+)?, align 8',ir)
        self.assertEqual(len(allocations),15)
        self.assertIn('on_stack_limit = 128',program.read_text())
        (root/'verdict.json').write_text(json.dumps(dict(status='PASS',printf_stack_allocations=len(allocations),
             shared_printf_buffer_references=0,ir_sha256=trace.window.digest(root/'compiled.optimized.ll'))))


class NativeOwner(unittest.TestCase):
    def run_case(self, case):
        root=Path(os.environ['XADB_IO_TEST_ROOT'])/case
        root.mkdir()
        worker=subprocess.Popen(['/usr/bin/python3','-c','import sys; sys.stdin.read()'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL)
        owner=None
        try:
            if case=='parent_death':
                code="""import os,sys,time,json
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from dev2_io_trace import Trace
owner=Trace(int(sys.argv[2]),271581192,259,6)
end=time.monotonic()+30
while not owner.tick():
 assert time.monotonic()<end
 time.sleep(.01)
Path(sys.argv[3]).write_text(str(owner.spool))
os._exit(0)
"""
                subprocess.run(['/usr/bin/python3','-c',code,str(Path(__file__).parent.resolve()),str(worker.pid),str(root/'spool-path')],check=True,timeout=35)
                spool=Path((root/'spool-path').read_text()); end=time.monotonic()+8
                while not (spool/'supervisor.json').exists():
                    self.assertLess(time.monotonic(),end);time.sleep(.02)
                import shutil
                shutil.copytree(spool,root/'trace');shutil.rmtree(spool)
            else:
                owner=trace.Trace(worker.pid,271581192,259,6)
                end=time.monotonic()+30
                while not owner.tick():
                    self.assertLess(time.monotonic(),end);time.sleep(.01)
                if case=='normal': owner.stop(lambda:None)
                elif case=='active_cancel':
                    owner.process.stdin.write(b'CANCEL\n');owner.process.stdin.flush()
                # prepare_failure deliberately closes after full attach, before payload.
                owner.finish(root/'trace');owner=None
            record=json.loads((root/'trace/supervisor.json').read_text())
            self.assertTrue(record['collector_gone'])
            self.assertEqual(record['status'],'STOPPED' if case=='normal' else 'FAILED')
            self.assertEqual(record['collector']['uid'],[0]*4)
            self.assertEqual(record['target']['uid'],[os.getuid()]*4)
            if case!='normal': self.assertIn('error',record)
            (root/'verdict.json').write_text(json.dumps({'status':'PASS','case':case,'receipt':record},indent=2))
        finally:
            if owner is not None: owner.finish(root/'failed-trace')
            worker.stdin.close();worker.wait(timeout=2)

    def test_watchdog(self):
        root=Path(os.environ['XADB_IO_TEST_ROOT'])/'watchdog';root.mkdir()
        worker=subprocess.Popen(['/usr/bin/python3','-c','import sys;sys.stdin.read()'],stdin=subprocess.PIPE)
        owner=None
        try:
            owner=trace.Trace(worker.pid,271581192,259,6)
            end=time.monotonic()+60
            while not owner.tick():
                self.assertLess(time.monotonic(),end);time.sleep(.01)
            while not any('done' in m for m in owner.messages):
                owner.tick();self.assertLess(time.monotonic(),end);time.sleep(.02)
            elapsed=time.monotonic()-owner.started
            owner.finish(root/'trace');owner=None
            record=json.loads((root/'trace/supervisor.json').read_text())
            self.assertIn('deadline',record['error']);self.assertTrue(record['collector_gone']);self.assertTrue(record['group_gone'])
            self.assertGreaterEqual(elapsed,55);self.assertLess(elapsed,60)
            (root/'verdict.json').write_text(json.dumps(dict(status='PASS',elapsed=elapsed)))
        finally:
            if owner: owner.finish(root/'failed-trace')
            worker.stdin.close();worker.wait(timeout=2)

    def test_supervisor_death(self):
        root=Path(os.environ['XADB_IO_TEST_ROOT'])/'supervisor_death';root.mkdir()
        worker=subprocess.Popen(['/usr/bin/python3','-c','import sys;sys.stdin.read()'],stdin=subprocess.PIPE)
        owner=None
        try:
            owner=trace.Trace(worker.pid,271581192,259,6)
            end=time.monotonic()+30
            while not owner.tick():
                self.assertLess(time.monotonic(),end);time.sleep(.01)
            ready=next(m['ready'] for m in owner.messages if 'ready' in m)
            # Pin both PIDs before identity verification and the intentional kill.
            code="""import os,sys,signal,select,json
from pathlib import Path
v=json.loads(sys.argv[1]);fds=[]
for key in ('supervisor','collector'):
 p=v[key];fd=os.pidfd_open(p['pid']);fds.append(fd)
 actual=int(Path('/proc',str(p['pid']),'stat').read_text().rsplit(')',1)[1].split()[19])
 assert actual==p['starttime']
signal.pidfd_send_signal(fds[0],signal.SIGKILL)
assert select.select([fds[1]],[],[],3)[0], 'collector survived supervisor'
print('COLLECTOR_EXITED_AFTER_SUPERVISOR_KILL')
"""
            result=subprocess.run(['/usr/bin/sudo','-n','--','/usr/bin/python3','-I','-c',code,json.dumps(ready)],capture_output=True,text=True,timeout=5)
            (root/'kill-result.json').write_text(json.dumps(dict(returncode=result.returncode,stdout=result.stdout,stderr=result.stderr,ready=ready),indent=2))
            self.assertEqual(result.returncode,0,result.stderr)
            # Supervisor was intentionally killed and cannot emit a receipt.
            with self.assertRaises(FileNotFoundError): owner.finish(root/'trace')
            import shutil
            shutil.rmtree(owner.spool);owner=None
            (root/'verdict.json').write_text(json.dumps(dict(status='PASS',scope='kernel-observed child exit; no fabricated supervisor receipt')))
        finally:
            if owner and not owner.closed: owner.finish(root/'failed-trace')
            worker.stdin.close();worker.wait(timeout=2)

    def test_normal(self): self.run_case('normal')
    def test_prepare_failure(self): self.run_case('prepare_failure')
    def test_active_cancel(self): self.run_case('active_cancel')
    def test_parent_death(self): self.run_case('parent_death')


if __name__=='__main__': unittest.main()
