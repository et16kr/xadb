"""New Status flush native checker tests; fabricated evidence is not a product run."""
import copy
from pathlib import Path
import tempfile
import unittest
import dev3_status_home_flush_native as n


def stdout(profile, size):
    c, batches = int(profile == 3), 3 if profile == 3 else 2
    return (f'running 1 test\ntest {n.SELECTOR} ... STATUS_FLUSH_NATIVE_COMPLETE profile={profile} '
            f'cancellations={c} batches={batches} effects={31*batches} bytes={size}\nok\n'
            'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 99 filtered out; finished in 0.01s\n')


def event_text(rows):
    return ''.join('\t'.join(map(str,r))+'\n' for r in rows)


def inventory_text(rows):
    return ''.join(name+'\t'+'\t'.join(map(str,rows[name]))+'\n' for name in sorted(rows))


def fd_text(profile):
    return event_text(n.expected_fd_events(profile))


def resources():
    return 'wrapped_peak\t2\nwrapped_final\t0\nobserved_fd_peak\t4\nobserved_fd_final\t0\n'


class StatusFlushNativeChecks(unittest.TestCase):
    def test_selection_marker_and_frozen_input_identity(self):
        spec=n.native_input()
        self.assertEqual(spec['profiles'],[1,2,3])
        self.assertEqual(n.selected([3,1]),[1,3])
        for bad in ([],[1,1],[0],[4]):
            with self.assertRaises(ValueError):n.selected(bad)
        text=stdout(3,22200000);n.check_stdout(text,3,22200000)
        for old,new in [('profile=3','profile=2'),('cancellations=1','cancellations=0'),
                        ('batches=3','batches=2'),('effects=93','effects=62'),
                        ('bytes=22200000','bytes=22200001'),('1 passed','0 passed'),(n.SELECTOR,'wrong')]:
            with self.assertRaises(ValueError):n.check_stdout(text.replace(old,new),3,22200000)
        for bad in (text+text,text+'STATUS_FLUSH_NATIVE_BAD\n'):
            with self.assertRaises(ValueError):n.check_stdout(bad,3,22200000)

    def test_campaign_effects_and_exact_fd_boundary_sequence(self):
        for profile in (1,2,3):
            effects=n.all_events(profile)
            self.assertEqual(len(effects),101 if profile==3 else 66)
            self.assertEqual(sum(r[0]=='write' and r[1]==n.WAL for r in effects),4 if profile==3 else 2)
            for batch,target in (value for kind,value in n.campaign(profile) if kind=='flush'):
                rows=n.flush_events(batch,target)
                self.assertEqual(len(rows),31)
                self.assertEqual(rows[9],('write','d3-dwb-images',0,16544,16544))
                self.assertEqual(rows[20],('write',n.HOME,16384 if target=='S' else 0,16384,16384))
                self.assertEqual([i+1 for i,r in enumerate(rows) if r[0]=='publish'],[8,19,30])
                self.assertEqual(rows[2][1],f'd3-dwb-{batch%2}')
            trace=fd_text(profile)
            n.check_fd('3\t4\t0\n',trace,resources(),profile)
            rows=trace.splitlines(keepends=True)
            for bad in (''.join(rows[1:]),''.join(rows[:-2]),trace+rows[-1],
                        trace.replace('read\t4\n','',1),trace.replace('close\t0','close\t3'),
                        trace.replace('open\t4','open\t5',1),trace.replace('baseline\t3','baseline\t4')):
                with self.assertRaises(ValueError):n.check_fd('3\t4\t0\n',bad,resources(),profile)
            with self.assertRaises(ValueError):n.check_fd('3\t4\t0\n',trace,resources().replace('wrapped_peak\t2','wrapped_peak\t3'),profile)
            self.assertEqual(n.parse_events(event_text(effects)),effects)
            with self.assertRaises(ValueError):n.parse_events('write\tname\t0\t1\tbad\n')
        self.assertEqual(n.campaign(3)[3],('flush',(1,'A')))
        self.assertEqual(n.campaign(3)[4],('wal',38000))

    def test_full_literal_native_layout_and_fixed_wal_growth(self):
        for profile in (1,2,3):
            initial=n.initial_files(n.ROOT,profile)
            self.assertEqual(sum(map(len,initial.values())),22069248)
            self.assertEqual(int.from_bytes(initial['d3-bootstrap'][120:128],'little'),2)
            self.assertEqual(int.from_bytes(initial['d3-bootstrap'][280:288],'little'),2)
            keys=n.batch_keys(profile)
            final=n.expected_files(n.ROOT,profile,keys[-1])
            growth=101600 if profile==3 else 59200
            self.assertEqual(len(final[n.WAL]),4096+growth)
            self.assertEqual(set(initial),set(final))
            self.assertEqual(len(final[n.HOME]),1048576)
            self.assertEqual(len(final['d3-dwb-images']),16711680)
            self.assertEqual(final[n.HOME][32768:],bytes(1048576-32768))
            self.assertEqual(final['d3-dwb-images'][16544:],bytes(16711680-16544))
            self.assertEqual(final['DWB'],n.literal(n.ROOT,keys[-1]+'/retirement.selector'))
            self.assertEqual(final['d3-profile'],initial['d3-profile'])
            size=sum(map(len,final.values()))
            resources=n.check_resources(size,len(final[n.WAL]))
            self.assertEqual(resources['fixed_owned_charge_bytes'],22073344)
            with self.assertRaisesRegex(ValueError,'FIXED_OWNED'):n.check_resources(size+1,len(final[n.WAL]))
        pre=n.expected_files(n.ROOT,3,'native-P3-b1')
        self.assertEqual(len(pre[n.WAL]),4096+63600)
        self.assertEqual(pre[n.HOME][16384:32768],bytes(16384))
        final=n.expected_files(n.ROOT,3,'native-P3-b3')
        self.assertNotEqual(pre[n.HOME][:16384],final[n.HOME][:16384])

    def test_complete_artifact_and_independent_corruption_rejection(self):
        profile=3
        with tempfile.TemporaryDirectory() as temporary:
            case=Path(temporary);db=case/'database';db.mkdir()
            initial=n.initial_files(n.ROOT,profile)
            final=n.expected_files(n.ROOT,profile,n.batch_keys(profile)[-1])
            for name,value in final.items():(db/name).write_bytes(value)
            current=n.inventory(db)
            before={name:[len(value),*current[name][1:4],n.sha(value)] for name,value in initial.items()}
            (case/'before.tsv').write_text(inventory_text(before))
            (case/'after.tsv').write_text(inventory_text(current))
            for batch,key in enumerate(n.batch_keys(profile),1):
                expected=n.expected_files(n.ROOT,profile,key)
                rows={name:[len(value),*current[name][1:4],n.sha(value)] for name,value in expected.items()}
                (case/f'batch{batch}.after.tsv').write_text(inventory_text(rows))
                target=('A','S','A')[batch-1];lsn=3 if batch==1 else 4;counter=1 if target=='S' else lsn
                (case/f'batch{batch}.events.tsv').write_text(event_text(n.flush_events(batch,target)))
                (case/f'batch{batch}.clean.tsv').write_text(f'{target}\t{lsn}\t{counter}\ttrue\ttrue\ttrue\t{batch}\t{3*batch+1}\t0\n')
            dumps={'wal-header.bin':initial[n.WAL],'wal.complete':final[n.WAL],
                   'allocator.after':n.literal(n.ROOT,'p3-c1/work2-allocator.after'),
                   'status.after':n.literal(n.ROOT,'p3-c1/work2-status.after'),
                   'owner.descriptor':n.literal(n.ROOT,'p3-c1/work2-init.descriptor')}
            for name,value in dumps.items():(case/name).write_bytes(value)
            owner_names = ('status_installation_none','base_installation_none','base_batch_none',
                           'terminal_pending_false','flush_capture_none','flush_retired_none',
                           'flush_authority_none','flush_phase_ready','healthy')
            owner_text=''.join(name+'\ttrue\n' for name in owner_names)
            (case/'owner-state.tsv').write_text(owner_text)
            (case/'ram.tsv').write_text('4\t4\t1\tfalse\tfalse\ttrue\ttrue\t0\ttrue\n')
            (case/'events.tsv').write_text(event_text(n.all_events(profile)))
            (case/'fd.tsv').write_text('3\t4\t0\n');(case/'fd-events.tsv').write_text(fd_text(profile))
            (case/'resources.tsv').write_text(resources())
            (case/'stdout.log').write_text(stdout(profile,sum(map(len,final.values()))))
            (case/'stderr.log').write_text('');(case/'process.json').write_text('{}\n')
            self.assertEqual(n.check_case(case,n.ROOT,profile)['batches'],3)
            for name in owner_names:
                (case/'owner-state.tsv').write_text(owner_text.replace(name+'\ttrue',name+'\tfalse'))
                with self.assertRaisesRegex(ValueError,'FINAL_OWNER_STATE'):n.check_case(case,n.ROOT,profile)
            for bad in (owner_text.split('\n',1)[1],owner_text+owner_text.splitlines()[0]+'\n',
                        owner_text.replace('flush_phase_ready','foreign_phase')):
                (case/'owner-state.tsv').write_text(bad)
                with self.assertRaisesRegex(ValueError,'FINAL_OWNER_STATE'):n.check_case(case,n.ROOT,profile)
            (case/'owner-state.tsv').write_text(owner_text)
            for name in ('owner.descriptor','status.after','ram.tsv','events.tsv','batch1.clean.tsv','batch2.after.tsv','fd-events.tsv'):
                path=case/name;original=path.read_bytes();path.write_bytes(original+b'X')
                with self.assertRaises((ValueError,KeyError)):n.check_case(case,n.ROOT,profile)
                path.write_bytes(original)
            extra=db/'unexpected';extra.write_bytes(b'')
            with self.assertRaises(ValueError):n.check_case(case,n.ROOT,profile)
            extra.unlink()
            path=db/n.HOME;value=path.read_bytes();path.write_bytes(value[:-1]+b'X')
            with self.assertRaises(ValueError):n.check_case(case,n.ROOT,profile)


if __name__=='__main__':unittest.main()
