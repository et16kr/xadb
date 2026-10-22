#!/usr/bin/env python3
"""New flush harness selftests; no product invocation or build."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
import check_dev3_allocator_flush_runtime as c

ROOT=Path(__file__).resolve().parent.parent


def transcript(group,ids):
    lines=['running 1 test',f'test {c.PREFIX+c.SELECTORS[group]} ... ']
    for identifier in ids:
        lines.extend(['FLUSH_CASE_BEGIN\t'+identifier])
        if identifier=='FA13':lines.extend('FLUSH_VARIANT\tFA13\tV'+str(i).zfill(2) for i in range(1,7))
        if identifier=='FR03':lines.append('FLUSH_ORACLE_NEGATIVES 19')
        lines.append('FLUSH_CASE\t'+identifier)
    lines.extend(['FLUSH_COMPLETE '+group+' '+str(len(ids)),'ok','',
                  'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 999 filtered out; finished in 0.01s'])
    # Real libtest prefixes only the first emitted marker on its selector line.
    return '\n'.join(lines).replace(' ... \n',' ... ')


class FlushHarnessTests(unittest.TestCase):
    def test_mandatory_and_s5_census_are_exact_and_separate(self):
        groups=c.mandatory_cases((ROOT/c.FIXTURE).read_text())
        expanded=c.expanded_cases((ROOT/c.EXPANDED).read_text())
        self.assertEqual(sum(map(len,groups.values())),76)
        self.assertEqual(len(expanded),972)
        self.assertEqual(c.select(groups,expanded,['FB31','FN02']),{'FN':['FN02'],'FB':['FB31']})
        self.assertEqual(c.select(groups,expanded,expanded_requested=[expanded[-1]]),{'S5':[expanded[-1]]})
        for bad in ([],['FN01','FN01'],['FA14'],['S5-P1-N1-RESERVE-NORMAL-32']):
            with self.assertRaises(ValueError):c.select(groups,expanded,bad)
        with self.assertRaises(ValueError):c.select(groups,expanded,['FN01'],s5=True)
        with self.assertRaises(ValueError):c.select(groups,expanded,s5=True,expanded_requested=[expanded[0]])

    def test_exact_markers_and_single_case_retry(self):
        for group,ids in [('FN',['FN02']),('FA',['FA13']),('FR',['FR03']),('S5',['S5-P1-N1-RESERVE-NORMAL-32'])]:
            with self.subTest(group):
                actual=c.evaluate(transcript(group,ids),{'exit_code':0,'timed_out':False},group,ids)
                self.assertEqual(actual['cases'],ids)
                self.assertEqual(actual['case_count'],1)

    def test_missing_duplicate_foreign_begin_and_completion_rejected(self):
        text=transcript('FN',['FN01','FN02'])
        bad=[text.replace('FLUSH_CASE\tFN01\n',''),
             text.replace('FLUSH_CASE\tFN01\n','FLUSH_CASE\tFN01\nFLUSH_CASE\tFN01\n'),
             text.replace('FLUSH_CASE_BEGIN\tFN01','FLUSH_CASE_BEGIN\tFN99'),
             text.replace('FLUSH_COMPLETE FN 2','FLUSH_COMPLETE FN 3'),
             text.replace('1 passed; 0 failed','2 passed; 0 failed'),
             text.replace('FLUSH_CASE_BEGIN\tFN01','FLUSH_CASE_BAD\tFN01'),
             text.replace('FLUSH_CASE_BEGIN\tFN01','FLUSH_CASE\tFN01').replace('FLUSH_CASE\tFN02','FLUSH_CASE_BEGIN\tFN02')]
        for damaged in bad:
            with self.assertRaises(ValueError):c.evaluate(damaged,{'exit_code':0},'FN',['FN01','FN02'])
        for process in ({'exit_code':1},{'exit_code':0,'timed_out':True}):
            with self.assertRaises(ValueError):c.evaluate(text,process,'FN',['FN01','FN02'])

    def test_variants_and_negative_witnesses_are_not_optional(self):
        for group,identifier,marker in [('FA','FA13','FLUSH_VARIANT\tFA13\tV06'),('FR','FR03','FLUSH_ORACLE_NEGATIVES 19')]:
            text=transcript(group,[identifier])
            for damaged in (text.replace(marker,''),text+'\n'+marker,text.replace(marker,marker+' BAD')):
                with self.assertRaises(ValueError):c.evaluate(damaged,{'exit_code':0},group,[identifier])
        with self.assertRaises(ValueError):c.evaluate(transcript('FN',['FN01'])+'\nFLUSH_ORACLE_NEGATIVES 19',{'exit_code':0},'FN',['FN01'])

    def test_inventory_tamper_and_build_identity_rejected(self):
        text=(ROOT/c.FIXTURE).read_text()
        for damaged in (text.replace('FA13','FA14'),text.replace('INPUT_NOT_RUN','PASS',1),text+text.splitlines()[1]+'\n'):
            with self.assertRaises(ValueError):c.mandatory_cases(damaged)
        expanded=(ROOT/c.EXPANDED).read_text()
        with self.assertRaises(ValueError):c.expanded_cases(expanded.replace('AFTER-31','AFTER-32',1))
        root=Path('/frozen');binary=Path('/frozen/build/engine')
        receipt={'status':'PASS','source_root':str(root),'binary':str(binary),'source_sha256':'s','binary_sha256':'b','command':c.BUILD_COMMAND,'exit_code':0}
        c.verify_build(receipt,root,binary,'s','b')
        for key in receipt:
            changed=copy.deepcopy(receipt);changed[key]='wrong'
            with self.assertRaises(ValueError):c.verify_build(changed,root,binary,'s','b')

    def test_manifest_and_fixture_identities(self):
        manifest=json.loads((ROOT/c.MANIFEST).read_text())
        normal=c.digest(ROOT/c.FIXTURE);expanded=c.digest(ROOT/c.EXPANDED)
        identities=c.verify_inputs(ROOT,manifest,normal,expanded)
        self.assertIn(c.ORACLE,identities);self.assertIn(c.FIXTURE,identities)
        for key,value in [('mandatory_cases',75),('s5_cases',971),('known_bad',15),('oracle_manifest_sha256','0'*64),('fa13_variants',[])]:
            changed=copy.deepcopy(manifest);changed[key]=value
            with self.assertRaises(ValueError):c.verify_inputs(ROOT,changed,normal,expanded)
        with self.assertRaises(ValueError):c.safe_file(ROOT,'../escape')
        with self.assertRaises(ValueError):c.safe_file(ROOT,'/absolute')


if __name__=='__main__':unittest.main()
