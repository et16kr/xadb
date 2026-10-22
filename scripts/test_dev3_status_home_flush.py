"""New Status flush runner tests; synthetic logs are not product evidence."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import dev3_status_home_flush as check


def transcript(ids, spec, *, resource=None):
    selector = spec['cases'][ids[0]]['selector']
    lines = [f'test {selector} ...']
    for key in ids:
        lines.append(check.BEGIN + key)
        for variant in spec['cases'][key]['variants']:
            lines.append(check.VARIANT + key + '\t' + variant)
        if key == 'RF01':
            for name, cap in spec['resource_caps'].items():
                lines.append(f'RESOURCE_STATUS_FLUSH {name} {1 if resource is None else resource} {cap}')
        lines.append(check.CASE + key)
    lines.append(check.COMPLETE + spec['cases'][ids[0]]['group'] + ' ' + str(len(ids)))
    lines.append('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 999 filtered out')
    return '\n'.join(lines) + '\n'


class StatusHomeFlushChecks(unittest.TestCase):
    def test_selection_aliases_source_build_binding_and_budgets(self):
        spec = check.runtime_input()
        self.assertEqual(len(spec['groups']['MANDATORY']), 43)
        self.assertEqual(len(spec['groups']['S5_FAULTS']), 564)
        self.assertEqual(len(spec['groups']['S5_HISTORIES']), 24)
        self.assertEqual(len(spec['cases']), 631)
        self.assertEqual(check.selections(['PF08', 'PF01'], spec), ['PF01', 'PF08'])
        for bad in ([], ['BA03', 'BA03'], ['MANDATORY', 'BN01'], ['SF-P1-SA-PREFIX-10'], ['NATIVE']):
            with self.assertRaises(ValueError):
                check.selections(bad, spec)
        self.assertEqual(check.jobs(['RF01', 'RF02', 'RF03'], spec), [['RF01', 'RF02'], ['RF03']])
        self.assertEqual(check.remaining_seconds(['BN01'], {'mandatory':119.5}, spec), ('mandatory', .5))
        with self.assertRaisesRegex(ValueError, 'AGGREGATE_BUDGET'):
            check.remaining_seconds(['BN01'], {'mandatory':120}, spec)
        receipt = dict(status='PASS', source_root=str(check.ROOT), source_sha256='a'*64,
                       binary_sha256='b'*64, command=check.BUILD_COMMAND, exit_code=0, errors=0)
        check.check_build_receipt(receipt, check.ROOT, 'a'*64, 'b'*64)
        for key, value in (('status','FAIL'),('source_root','/foreign'),('source_sha256','c'*64),
                           ('binary_sha256','c'*64),('command',['cargo','test']),('exit_code',1),('errors',1)):
            with self.assertRaisesRegex(ValueError, 'BUILD_BINDING'):
                check.check_build_receipt(dict(receipt, **{key:value}), check.ROOT, 'a'*64, 'b'*64)

    def test_strict_case_variant_and_completion_protocol(self):
        spec = check.runtime_input()
        for chosen in (['BN01'], ['BA03', 'BA05'], ['RF02'], ['PF01', 'PF08'], [spec['groups']['S5_FAULTS'][0]]):
            good = transcript(chosen, spec)
            check.check_runtime(good, chosen, spec)
            check.check_runtime(good.replace(' ...\n', ' ... ', 1), chosen, spec)
            for bad in (good.replace('1 passed','0 passed'), good.replace(' ...','-foreign ...',1),
                        good.replace(check.BEGIN+chosen[0]+'\n','',1),
                        good.replace(check.CASE+chosen[0],check.CASE+'foreign',1),
                        good+check.CASE+chosen[0]+'\n',good+'STATUS_FLUSH_UNKNOWN 1\n',
                        good+'RESOURCE_STATUS_FLUSH_BAD x 1 1\n',
                        good+'RESOURCE_STATUS_FLUSH\n',
                        good.replace(check.COMPLETE,'STATUS_FLUSH_COMPLETE_EXTRA ',1)):
                with self.assertRaises(ValueError):
                    check.check_runtime(bad, chosen, spec)
        good = transcript(['RF02'], spec)
        marker = check.VARIANT+'RF02\tfile-aggregate\n'
        self.assertIn(marker, good)
        for bad in (good.replace(marker,''),good.replace(marker,marker+marker)):
            with self.assertRaisesRegex(ValueError,'VARIANT'):
                check.check_runtime(bad,['RF02'],spec)
        progress = check.raw_progress('test selected ... ' + check.BEGIN + 'BN01\n' +
                                      check.CASE + 'BN01\n' + check.BEGIN + 'BN02\n', 'selected')
        self.assertEqual(progress['raw_case_begin'], ['BN01', 'BN02'])
        self.assertEqual(progress['raw_case_completed'], ['BN01'])
        good = transcript(['BA03'], spec)
        marker = check.VARIANT+'BA03\tactual-C5-unpublished\n'
        self.assertIn(marker, good)
        for bad in (good.replace(marker,''),good.replace(marker,marker+marker),
                    good.replace(marker,check.VARIANT+'BA05\tactual-C5-unpublished\n')):
            with self.assertRaises(ValueError):
                check.check_runtime(bad, ['BA03'], spec)

    def test_frozen_resource_cap_and_known_bad_witness_census(self):
        spec = copy.deepcopy(check.runtime_input())
        # Synthetic schema values exercise the predicate; actual caps are frozen
        # separately in the adopted resource manifest before execution.
        spec['resource_caps'] = {'constructor_phase':65536,'status_codec':3584}
        good = transcript(['RF01'], spec)
        check.check_runtime(good, ['RF01'], spec)
        for bad in (good.replace('constructor_phase 1 65536','constructor_phase 1 65537'),
                    good.replace('status_codec 1 3584','status_codec 3585 3584'),
                    good.replace('RESOURCE_STATUS_FLUSH status_codec 1 3584\n',''),
                    good.replace(check.CASE+'RF01','RESOURCE_STATUS_FLUSH status_codec 1 3584\n'+check.CASE+'RF01')):
            with self.assertRaisesRegex(ValueError, 'RESOURCE'):
                check.check_runtime(bad, ['RF01'], spec)
        output, rejected = check.run_witness()
        self.assertEqual(rejected,[f'N{i:02}' for i in range(1,13)])
        check.check_runtime(output,['RF03'],spec,python=True)
        for bad in (output.replace('NEGATIVES 12','NEGATIVES 11'), output+check.NEGATIVES+'12\n'):
            with self.assertRaises(ValueError):
                check.check_runtime(bad,['RF03'],spec,python=True)

    def test_literal_full_census_hash_and_provenance_rejection(self):
        check.check_inputs()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root/check.FIXTURE_REL
            shutil.copytree(check.ROOT/check.FIXTURE_REL,target)
            manifest=json.loads((target/'manifest.json').read_text())
            dependencies=[manifest['generator']['path']]+[r['path'] for r in manifest['provenance_inputs'] if not r['path'].startswith('artifacts/')]
            for relative in dependencies:
                path=root/relative;path.parent.mkdir(parents=True,exist_ok=True)
                shutil.copyfile(check.ROOT/relative,path)
            check.check_inputs(root)
            literal=next(target.glob('blobs/*.bin')); original=literal.read_bytes()
            literal.write_bytes(original+b'X')
            with self.assertRaisesRegex(ValueError,'LITERAL_IDENTITY'):
                check.check_inputs(root)
            literal.write_bytes(original)
            extra=target/'unregistered';extra.write_bytes(b'')
            with self.assertRaisesRegex(ValueError,'FIXTURE_CENSUS'):
                check.check_inputs(root)
            extra.unlink()
            generator=root/manifest['generator']['path'];generator.write_text(generator.read_text()+'\n')
            with self.assertRaisesRegex(ValueError,'GENERATOR_IDENTITY'):
                check.check_inputs(root)


if __name__ == '__main__':
    unittest.main()
