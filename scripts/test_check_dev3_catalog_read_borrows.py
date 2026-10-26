"""Only the new probe generation; shared compiler/parser tests are not repeated."""
import unittest

from check_dev3_catalog_read_borrows import KINDS, LIB, MODULE, PROBE, probe_source


class CatalogReadProbeTests(unittest.TestCase):
    def test_record_lifetime_release_and_drop_mark_exact_consumption(self):
        for kind, mutation in (
                ('release', 'guard.release_checked().unwrap();'), ('drop', 'drop(guard);')):
            negative, line = probe_source(kind, False)
            self.assertEqual(KINDS[kind], 'E0505')
            self.assertEqual(negative.splitlines()[line - 1].strip(),
                             mutation + ' // expected diagnostic')
            self.assertLess(negative.index(mutation), negative.index('black_box(record'))
            positive, marker = probe_source(kind, True)
            self.assertIsNone(marker)
            self.assertLess(positive.index('black_box(record'), positive.index(mutation))

    def test_owner_mutation_is_bounded_by_guard_use(self):
        negative, line = probe_source('owner', False)
        self.assertEqual(KINDS['owner'], 'E0499')
        self.assertEqual(negative.splitlines()[line - 1].strip(),
                         'let alias = &mut *owner; // expected diagnostic')
        self.assertLess(negative.index('let alias'), negative.index('guard.lookup'))
        positive, marker = probe_source('owner', True)
        self.assertIsNone(marker)
        self.assertLess(positive.index('release_checked'), positive.index('let alias'))

    def test_private_probe_module_is_exact_and_unknown_kind_rejected(self):
        self.assertEqual(PROBE, LIB.removesuffix('.rs') + '/catalog_borrow_probe.rs')
        self.assertIn('#[path = "read/catalog_borrow_probe.rs"]', MODULE)
        for kind in KINDS:
            for positive in (False, True):
                text, marker = probe_source(kind, positive)
                self.assertEqual(text.count('// expected diagnostic'), int(not positive))
                self.assertEqual(marker is None, positive)
        with self.assertRaises(ValueError):
            probe_source('foreign', False)


if __name__ == '__main__':
    unittest.main()
