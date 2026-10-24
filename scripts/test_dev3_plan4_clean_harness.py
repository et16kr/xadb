"""Changed G5 harness boundaries only; all product invocation is mocked."""
from contextlib import ExitStack
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import dev3_plan4_clean_cases as cases
import dev3_plan4_clean_native as native

IDENTITY = ['a' * 64, 'b' * 64, 'c' * 64]


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value))


def census_fixture(root, eligible=1024):
    run = root / 'static'
    run.mkdir()
    selector = 'g5_source_memory'
    case = 'P4-G5-ST01'
    lines = [f'P4_CONTEXT\t{case}\t' + '\t'.join(IDENTITY), f'P4_BEGIN\t{case}']
    for n in cases.PADDING:
        lines.append(f'P4_CENSUS\tB08-candidate\tpadding={n}\tfile={n + 8}\tfresh_fits=true'
                     f'\tdoublewrite_owner=100\tdoublewrite_plan=100\tsetup_fits=true'
                     f'\tg5_fits={str(n < eligible).lower()}\tg5_peak={65000 + n}')
    lines.append(f'P4_PASS\t{case}')
    text = '\n'.join(lines) + '\n'
    stdout = run / '01.stdout'
    stdout.write_text(text)
    job = {'selector': selector, 'selected_ids': [case], 'stdout': stdout.name,
           'stdout_sha256': cases.sha(stdout), 'truncated': False,
           'parsed': cases.common.parse_markers(text, selector, [case], IDENTITY)}
    receipt = run / 'result.json'
    write_json(receipt, {'identity': IDENTITY, 'finalized': True,
                         'cases': {case: 'PASS'}, 'jobs': [job]})
    gate = {'schema': 'XADB_PLAN4_G5_GATE_INPUT_V1', 'identity': IDENTITY,
            'static_receipt_path': str(receipt), 'static_receipt_sha256': cases.sha(receipt),
            'capacity_padding': eligible}
    write_json(root / 'gate-execution-input.json', gate)
    return gate, receipt, stdout


def worker_fixture(root):
    root.mkdir()
    case, pid = 'P4-G5-NATIVE-P1', 41
    execution = {'id': case, 'pid': str(pid), 'profile': '1',
                 **dict(zip(('source_sha256', 'elf_sha256', 'input_sha256'), IDENTITY))}
    (root / 'execution.tsv').write_text(''.join(f'{k}\t{v}\n' for k, v in execution.items()))
    text = f'running 1 test\nP4_CONTEXT\t{case}\t' + '\t'.join(IDENTITY) + f'\nP4_BEGIN\t{case}\nP4_NATIVE_COMPLETE\t{case}\t41\nP4_PASS\t{case}\n1 passed; 0 failed;\n'
    (root / 'stdout.log').write_text(text)
    (root / 'stderr.log').write_text('')
    (root / 'entropy.tsv').write_text('1\t41\t0102030405060708090a0b0c0d0e0f10\n')
    effects = ['acquire\tLOCK\t0\t0\tCREATED', 'entropy\tnamespace\t0\t16\tOK',
               'write\tdata\t0\t1\tOK', 'g5_begin\tnamespace\t0\t0\tOK',
               'read\tdata\t0\t1\tOK', 'drop_lease\tLOCK\t0\t0\tOK']
    (root / 'effects.tsv').write_text(''.join(f'{i}\t{row}\n' for i, row in enumerate(effects)))
    resources = {'g5_event_begin': 4, 'g5_mutations': 0, 'g5_entropy': 0, 'handles_final': 0,
                 'fd_final': 0, 'setup_entropy': 1, 'setup_mutations': 1, 'handles_peak': 3,
                 'fd_peak': 5, 'events': 6}
    (root / 'resources.tsv').write_text(''.join(f'{k}\t{v}\n' for k, v in resources.items()))
    rows = 'name\tbytes\tdevice\tinode\tsha256\n' + ''.join(f'f{i}\t1\t2\t{i+1}\t{"d" * 64}\n' for i in range(15))
    (root / 'before.tsv').write_text(rows)
    (root / 'after.tsv').write_text(rows)
    process = {'pid': pid, 'exact': True, 'exit_code': 0, 'truncated': False, 'elapsed_seconds':0.01,
               'parsed': cases.common.parse_markers(text, native.SELECTOR, [case], IDENTITY)}
    return process, {'files': native.index_tsv(root / 'before.tsv')}


class G5Harness(unittest.TestCase):
    def test_h01_actual_static_capacity_gate(self):
        """G5-H01: exact first eligible type, original actual census, no product call."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            gate, _, _ = census_fixture(root)
            self.assertEqual(cases.capacity_gate(root, IDENTITY)[0], 1024)
            gate['capacity_padding'] = 2048
            write_json(root / 'gate-execution-input.json', gate)
            with self.assertRaisesRegex(ValueError, 'first eligible'):
                cases.capacity_gate(root, IDENTITY)

    def test_h02_gate_rejects_changed_evidence_or_no_legal_type(self):
        """G5-H02: bind source/ELF/receipt/old DWB admission before B08 effects."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            gate, receipt, stdout = census_fixture(root)
            with self.assertRaisesRegex(ValueError, 'identity'):
                cases.capacity_gate(root, ['x' * 64, *IDENTITY[1:]])
            stdout.write_text(stdout.read_text().replace('doublewrite_owner=100', 'doublewrite_owner=20000'))
            with self.assertRaisesRegex(ValueError, 'stdout drift'):
                cases.capacity_gate(root, IDENTITY)
            record = json.loads(receipt.read_text())
            record['jobs'][0]['stdout_sha256'] = cases.sha(stdout)
            write_json(receipt, record)
            gate['static_receipt_sha256'] = cases.sha(receipt)
            write_json(root / 'gate-execution-input.json', gate)
            with self.assertRaisesRegex(ValueError, 'no legitimate'):
                cases.capacity_gate(root, IDENTITY)

    def test_h03_g5_source_manifest_and_raw_binding(self):
        """G5-H03: new manifest/recipe and immutable old raw are distinct bindings."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); source = root / 'source'; source.mkdir()
            data = {'schema': 'XADB_PLAN4_G5_INPUT_V1', 'counts': {'mandatory':78,'mandatory_memory':75,
                'mandatory_static':2,'mandatory_native':1,'s5_additional':128,'s5_memory':126,
                's5_native':2,'total':206,'compiler':10}, 'cases':{f'P4-G5-X{i}':{} for i in range(206)},
                'raw_manifest_sha256':'raw-m', 'raw_index_sha256':'raw-i'}
            write_json(source / cases.INPUT / 'manifest.json', data)
            for path in (source/'rust-toolchain.toml',source/'Cargo.lock',root/'build.log',
                         source/cases.RAW/'manifest.json', source/cases.INPUT/'execution-recipes.json'):
                path.parent.mkdir(parents=True,exist_ok=True); path.write_text('fixed')
            (root/'xadb-engine-tests').write_bytes(b'\x7fELFfixed')
            fields = [('toolchain_file_sha256',source/'rust-toolchain.toml'),('cargo_lock_sha256',source/'Cargo.lock'),
                ('build_log_sha256',root/'build.log'),('binary_sha256',root/'xadb-engine-tests'),
                ('manifest_sha256',source/cases.INPUT/'manifest.json'),('raw_manifest_sha256',source/cases.RAW/'manifest.json'),
                ('recipes_sha256',source/cases.INPUT/'execution-recipes.json')]
            build = {'exit_code':0,'command':['cargo','test','-p','xadb-engine','--lib','--all-features','--locked','--no-run','--message-format=json'],
                'source_sha256':'fixed-source','owned_files':{'Cargo.lock':cases.sha(source/'Cargo.lock')},
                **{key:cases.sha(path) for key,path in fields}}
            write_json(root/'build-receipt.json',build)
            with patch.object(cases.common,'source_fingerprint',return_value='fixed-source'), patch.object(cases.raw_oracle,'fixture',return_value=({}, {'manifest.json':'raw-m','raw-index.tsv':'raw-i'})):
                self.assertEqual(cases.validate_build(root)[0],source)
                (source/cases.INPUT/'execution-recipes.json').write_text('drift')
                with self.assertRaisesRegex(ValueError,'recipes_sha256'):
                    cases.validate_build(root)

    def test_h04_new_memory_runner_process_mock_and_budget(self):
        """G5-H04: one exact new group, total/process ledgers and no inherited B08 env."""
        self.assertEqual(cases.require_audit(0,0,31,1,30),30)
        for wall,process,limit in [(41,1,30),(7,1,5)]:
            with self.assertRaisesRegex(ValueError,'audit-only'):
                cases.require_audit(0,0,wall,process,limit)
        with self.assertRaisesRegex(ValueError,'audit-only'):
            cases.require_audit(31,1,2,1,30)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp); revision=root/'revision'; revision.mkdir(); source=revision/'source'; source.mkdir()
            manifest=source/'manifest.json'; case='P4-G5-C01'; selector='g5_clean_lifecycle'
            write_json(manifest,{'cases':{case:{'phase':'mandatory','category':'memory','selector':selector}}})
            (revision/'build-receipt.json').write_text('{}')
            calls=[]
            def invoke(command,cwd,env,stdout,stderr,actual_selector,selected,identity,*budgets):
                calls.append((command,env,budgets))
                text=f'P4_CONTEXT\t{case}\t'+'\t'.join(identity)+f'\nP4_BEGIN\t{case}\nP4_PASS\t{case}\n'
                stdout.write_text(text); stderr.write_text('')
                return {'parsed':cases.common.parse_markers(text,actual_selector,selected,identity),'elapsed_seconds':0.01,'exact':True,'truncated':False}
            with ExitStack() as stack:
                stack.enter_context(patch.object(cases,'validate_build',return_value=(source,revision/'ELF',manifest,{},IDENTITY)))
                stack.enter_context(patch.object(cases,'runner_bindings',return_value={}))
                stack.enter_context(patch.object(cases.common,'load_prior',return_value=(set(),[],0.0,0)))
                stack.enter_context(patch.object(cases.common,'invoke',side_effect=invoke))
                stack.enter_context(patch('sys.argv',['g5',str(revision),'memory','--output',str(revision/'run')]))
                cases.main()
            self.assertEqual(len(calls),1)
            self.assertEqual(calls[0][0][1:], [selector,'--exact','--nocapture','--test-threads=1'])
            self.assertNotIn('XADB_PLAN4_G5_CAPACITY_PADDING',calls[0][1])
            report=json.loads((revision/'run/result.json').read_text())
            self.assertEqual(report['process_wall_seconds'],0.01)
            self.assertGreaterEqual(report['execution_wall_seconds'],report['measured_before_receipt_seconds'])
            self.assertTrue(report['finalized'])

    def test_h05_native_new_clean_boundary_and_whole_identity(self):
        """G5-H05: actual setup trace is allowed; effects after new bind boundary fail."""
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'worker'; process,files=worker_fixture(root)
            native.validate_worker(root,1,IDENTITY,process,files)
            trace=(root/'effects.tsv').read_text()
            (root/'effects.tsv').write_text(trace.replace('4\tread','4\twrite'))
            with self.assertRaisesRegex(ValueError,'persistent mutation'):
                native.validate_worker(root,1,IDENTITY,process,files)
            (root/'effects.tsv').write_text(trace)
            changed=copy.deepcopy(files); changed['files']['f0']['inode']+=1
            with self.assertRaisesRegex(ValueError,'identity changed'):
                native.validate_worker(root,1,IDENTITY,process,changed)

    def test_h06_native_completed_or_uncertain_attempt_cannot_repeat(self):
        """G5-H06: new output directory does not bypass actual worker completion."""
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp); attempt=root/'attempt'; attempt.mkdir()
            process,_=worker_fixture(attempt/'worker')
            report={'schema':native.SCHEMA,'case':'P4-G5-NATIVE-P1','identity':IDENTITY,
                'finalized':True,'attempt_state':'FINISHED','process':process,'elapsed_seconds':1,
                'retained_index':native.retained_index(attempt)}
            write_json(attempt/'result.json',report)
            with self.assertRaisesRegex(ValueError,'cannot be replayed'):
                native.history(root,'P4-G5-NATIVE-P1')
            report['finalized']=False; report['attempt_state']='INVOKING'
            write_json(attempt/'result.json',report)
            with self.assertRaisesRegex(ValueError,'unfinished'):
                native.history(root,'P4-G5-NATIVE-P1')

    def test_h07_native_prior_failed_artifacts_immutable(self):
        """G5-H07: cross-profile aggregate carries failure bytes/time and hashes."""
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp); attempt=root/'failed'; attempt.mkdir()
            (attempt/'error.log').write_text('original failure')
            report={'schema':native.SCHEMA,'case':'P4-G5-NATIVE-P2','finalized':True,
                'elapsed_seconds':2.5,'retained_index':native.retained_index(attempt)}
            write_json(attempt/'result.json',report)
            _,elapsed,retained,logs=native.history(root,'P4-G5-NATIVE-P1')
            self.assertEqual(elapsed,2.5); self.assertGreater(retained,16); self.assertEqual(logs,retained)
            (attempt/'error.log').write_text('changed failure')
            with self.assertRaisesRegex(ValueError,'changed'):
                native.history(root,'P4-G5-NATIVE-P1')

    def test_h08_native_single_product_call_oracle_failure_holds(self):
        """G5-H08: completed worker plus oracle failure is saved and never rerun."""
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp); revision=root/'revision'; revision.mkdir(); source=revision/'source'; source.mkdir()
            manifest=source/'manifest.json'; case='P4-G5-NATIVE-P1'
            write_json(manifest,{'cases':{case:{'selector':native.SELECTOR}}})
            (revision/'build-receipt.json').write_text('{}')
            def invoke(command,cwd,env,stdout,stderr,*rest):
                # The runner has already created worker; fixture uses a temporary child.
                generated=stdout.parent/'generated'; process,_=worker_fixture(generated)
                for path in list(generated.iterdir()): path.rename(stdout.parent/path.name)
                generated.rmdir()
                return process
            with ExitStack() as stack:
                stack.enter_context(patch.object(native.clean,'validate_build',return_value=(source,revision/'ELF',manifest,{},IDENTITY)))
                stack.enter_context(patch.object(native.clean,'runner_bindings',return_value={}))
                stack.enter_context(patch.object(native.oracle,'fixture',return_value=({},{})))
                stack.enter_context(patch.object(native.oracle,'expected_files',return_value={}))
                stack.enter_context(patch.object(native.oracle,'compare_files',side_effect=ValueError('oracle mismatch')))
                invocation=stack.enter_context(patch.object(native.common,'invoke',side_effect=invoke))
                stack.enter_context(patch('sys.argv',['g5',str(revision),'--output',str(revision/'run')]))
                with self.assertRaisesRegex(ValueError,'oracle mismatch'): native.main()
                self.assertEqual(invocation.call_count,1)
            report=json.loads((revision/'run/result.json').read_text())
            self.assertEqual(report['status'],'PARTIAL_OR_FAIL')
            self.assertEqual(report['attempt_state'],'FINISHED')
            with self.assertRaisesRegex(ValueError,'cannot be replayed'): native.history(root,case)

    def test_h13_loaded_helper_must_match_frozen_source(self):
        """G5-H13: actual loaded cases/native/common/raw paths bind before effects and after audit."""
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp); (root/'scripts').mkdir()
            actual=[Path(cases.__file__),Path(cases.common.__file__),Path(cases.raw_oracle.__file__),Path(native.__file__)]
            for path in actual: (root/'scripts'/path.name).write_bytes(path.read_bytes())
            bindings=cases.runner_bindings(root,[native.__file__])
            self.assertEqual(len(bindings),4)
            cases.check_bindings(root,bindings)
            (root/'scripts'/Path(cases.common.__file__).name).write_text('changed frozen helper')
            with self.assertRaisesRegex(ValueError,'changed during'):
                cases.check_bindings(root,bindings)
            with self.assertRaisesRegex(ValueError,'differs from frozen'):
                cases.runner_bindings(root,[native.__file__])


if __name__ == '__main__':
    unittest.main()
