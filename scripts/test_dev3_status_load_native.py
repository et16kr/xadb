"""Known-bad checker boundaries; no product or native database execution."""
import tempfile
import unittest
from pathlib import Path
import dev3_status_load_native as check

class LoadEvidence(unittest.TestCase):
    def test_missing_or_extra_test_cannot_pass(self):
        for text in ('', 'test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured;'):
            with self.assertRaises(ValueError):check.check_stdout(text,0)
        good=f'test {check.SELECTOR} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;'
        check.check_stdout(good,0)
        with self.assertRaises(ValueError):check.check_stdout(good+'\n'+good,0)
        with self.assertRaises(ValueError):check.check_stdout(good,1)

    def test_wrong_read_or_effect_cannot_pass(self):
        rows=[]
        for offset in (16384,32768,16384):
            for name,at,count in [('d3-profile',0,4096),('FILEMAP',0,4096),('d3-filemap-0',0,320),('d3-filemap-0',16384,256),('d3-filemap-0',16384,256),(check.history.model.DATA,offset,16384)]:
                rows.append(f'read\t{name}\t{at}\t{count}\t{count}')
        good='\n'.join(rows);check.check_reads(good)
        for bad in ('',good.replace('32768','16384'),good+'\nwrite\tx\t0\t1\t1','\n'.join(rows[:-1])):
            with self.assertRaises(ValueError):check.check_reads(bad)

    def test_full_file_oracle_rejects_changed_home(self):
        root=Path(__file__).resolve().parent.parent
        files=check.expected_files(root)
        with tempfile.TemporaryDirectory() as directory:
            db=Path(directory)
            for name,raw in files.items():(db/name).write_bytes(raw)
            check.evidence.compare_files(db,files)
            path=db/check.history.model.DATA
            raw=bytearray(path.read_bytes());raw[16384+128]^=1;path.write_bytes(raw)
            with self.assertRaises(ValueError):check.evidence.compare_files(db,files)

    def test_build_receipt_cannot_hide_error_or_missing_completion(self):
        good=[dict(reason='build-finished',success=True)]
        check.check_build_rows(good)
        for bad in ([],good+good,[dict(reason='build-finished',success=False)],good+[dict(reason='compiler-message',message=dict(level='error'))]):
            with self.assertRaises(ValueError):check.check_build_rows(bad)

    def test_source_binary_checker_drift_is_failure(self):
        original=dict(source='s',binary='b',receipt='r',build='c',checkers={'runner':'a','oracle':'o'})
        check.check_identity(original,dict(original))
        for key in original:
            changed=dict(original);changed[key]='different'
            with self.assertRaises(ValueError):check.check_identity(original,changed)

if __name__=='__main__':unittest.main()
