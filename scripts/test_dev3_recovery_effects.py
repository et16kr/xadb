"""Known-bad runtime selector/protocol checks for the new recovery leaf only."""
import unittest
import dev3_recovery_effects as runner


class ClosedRecoveryEffectsRuntimeHarnessTests(unittest.TestCase):
    def spec(self):
        return dict(cases={'RP01':dict(selector='selector',budget=30,budget_group='core_mandatory'),'RP02':dict(selector='selector',budget=30,budget_group='core_mandatory')},groups={'mandatory':['RP01','RP02'],'s5':[]},budget_seconds={'core_mandatory':30})

    def output(self):
        return 'test selector ... RECOVERY_EFFECTS_CASE_BEGIN\tRP01\nRECOVERY_EFFECTS_CASE\tRP01\tPASS\nRECOVERY_EFFECTS_CASE_BEGIN\tRP02\nRECOVERY_EFFECTS_CASE\tRP02\tPASS\nRECOVERY_EFFECTS_COMPLETE count=2\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n'

    def test_empty_typo_duplicate_and_group_overlap(self):
        for selection in ([],['typo'],['RP01','RP01'],['mandatory','RP01']):
            with self.subTest(selection=selection), self.assertRaises(ValueError): runner.selections(selection,self.spec())
        self.assertEqual(runner.selections(['RP02','RP01'],self.spec()),['RP01','RP02'])

    def test_zero_wrong_test_duplicate_missing_case_and_count(self):
        output=self.output(); runner.protocol(output,['RP01','RP02'],'selector')
        for bad in ('',output.replace('test selector','test wrong'),output+output,output.replace('RECOVERY_EFFECTS_CASE\tRP02\tPASS\n',''),output.replace('count=2','count=1'),output.replace('1 passed','0 passed'),output.replace('RP01\tPASS','RP02\tPASS')):
            with self.subTest(output=bad), self.assertRaises(ValueError): runner.protocol(bad,['RP01','RP02'],'selector')

    def test_completed_prefix_preserved_as_assertions_after_failure(self):
        output=self.output().split('RECOVERY_EFFECTS_CASE\tRP02')[0]+'thread panicked\n'
        report=runner.protocol(output,['RP01','RP02'],'selector',False)
        self.assertEqual(report['completed_case_assertions'],['RP01'])
        self.assertEqual(report['active_case'],'RP02')
        with self.assertRaises(ValueError): runner.protocol(output,['RP01','RP02'],'selector')

    def test_timeout_budget_and_mixed_groups(self):
        with self.assertRaises(ValueError): runner.remaining(['RP01'],{'core_mandatory':30},self.spec())
        spec=self.spec();spec['cases']['RP02']['ignored']=True;spec['cases']['RP02']['budget_group']='core_s5'
        with self.assertRaises(ValueError): runner.remaining(['RP01','RP02'],{},spec)
        spec=self.spec(); spec['cases']['RX0001']=dict(selector='s5_selector',budget=30,ignored=True,budget_group='core_s5')
        spec['groups']['s5_matrix_all']=['RX0001']; spec['budget_seconds']['core_s5']=1200
        spec['aliases']={'RX0001':'RP01'}
        self.assertEqual(runner.remaining(['RX0001'],{},spec),('core_s5',30))
        self.assertEqual(runner.selections(['s5_matrix_all'],spec),['RX0001'])
        with self.assertRaises(ValueError): runner.selections(['mandatory','s5_matrix_all'],spec)
        def manifest(ids):
            return dict(case_env='XADB_RECOVERY_EFFECTS_CASES',cases=[dict(id=name,selector=name,group='mandatory') for name in ids],groups=dict(mandatory=ids,s5=[],s5_matrix_all=[],aliases={}),budget_seconds=dict(mandatory=dict(per_job=30,aggregate=120),s5=dict(per_job=1200,aggregate=1200)))
        normalized=runner.normalize({'semantic':manifest(['WP002','WP001']),'core':manifest(['RP001'])})
        self.assertEqual(list(normalized['cases']),['WP002','WP001','RP001'])
        self.assertEqual(runner.remaining(['RP001'],{'semantic_mandatory':120},normalized),('core_mandatory',30))
        with self.assertRaises(ValueError):runner.remaining(['WP001'],{'semantic_mandatory':120},normalized)


    def test_pass_without_matching_begin_is_rejected(self):
        bad=self.output().replace('RECOVERY_EFFECTS_CASE_BEGIN\tRP01\n','')
        with self.assertRaises(ValueError):runner.protocol(bad,['RP01','RP02'],'selector')

    def test_completion_before_last_pass_is_rejected(self):
        bad=self.output().replace('RECOVERY_EFFECTS_COMPLETE count=2\n','').replace('RECOVERY_EFFECTS_CASE_BEGIN\tRP02','RECOVERY_EFFECTS_COMPLETE count=2\nRECOVERY_EFFECTS_CASE_BEGIN\tRP02')
        with self.assertRaises(ValueError):runner.protocol(bad,['RP01','RP02'],'selector')

    def test_manifest_cannot_fall_back_from_wrong_env_or_budget(self):
        spec=dict(case_env='wrong',cases=[],groups=dict(mandatory=[],s5=[],s5_matrix_all=[],aliases={}),budget_seconds=dict(mandatory=dict(per_job=30,aggregate=180),s5=dict(per_job=1200,aggregate=1200)))
        with self.assertRaises(ValueError):runner.normalize({'core':spec})
        spec['case_env']='XADB_RECOVERY_EFFECTS_CASES';spec['budget_seconds']['mandatory'].pop('per_job')
        with self.assertRaises(ValueError):runner.normalize({'core':spec})


if __name__=='__main__':unittest.main()
