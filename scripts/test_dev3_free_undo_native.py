#!/usr/bin/env python3
"""Failure-oriented checks for the new native crash admission/audit boundaries."""
import unittest
import tempfile
from pathlib import Path
from unittest.mock import patch
import dev3_free_undo_native as native
from dev3_free_undo_native import CUTS, HOME, audit_cut


def rows(*events):
    return [[str(i), *event] for i, event in enumerate(events)]


class CrashAudit(unittest.TestCase):
    def test_home_cut_requires_latest_publication_sync_and_no_home_success(self):
        arm = ('arm', CUTS[0], '180224', '16384', 'ARMED:1')
        prefix = [arm, ('publish', 'd3-dwb-select-stage', '0', '0', 'DWB:Ok(())'),
                  ('dirsync', 'namespace', '0', '0', 'Ok(())')]
        end = [('cut', CUTS[0], '180224', '16384', 'REACHED:1'),
               ('barrier', CUTS[0], '0', '1', 'REACHED')]
        audit_cut(rows(*prefix, *end), CUTS[0])
        zero_fill = ('write', HOME, '180224', '16384', 'OK:16384:zero-fill')
        audit_cut(rows(zero_fill, *prefix, *end), CUTS[0])
        for wrong in [
            [*prefix[:2], *end],
            [*prefix, prefix[1], *end],
            [*prefix[1:], *end],
            [arm, *prefix, *end],
            [*prefix[1:], *end[:1], arm, end[1]],
            [*prefix, zero_fill, *end],
            [*prefix, ('write', HOME, '180224', '16384', 'OK:16384:hash'), *end],
            [*prefix, *end, ('dirsync', 'namespace', '0', '0', 'Ok(())')],
        ]:
            with self.assertRaises(ValueError):
                audit_cut(rows(*wrong), CUTS[0])

    def test_durable_cut_requires_actual_unlink_then_sync_and_unforced_page(self):
        prefix = [('remove', 'd3-wal-00000001', '0', '0', 'Ok(())'),
                  ('dirsync', 'namespace', '0', '0', 'Ok(())')]
        end = ('barrier', CUTS[1], '0', '1', 'REACHED')
        audit_cut(rows(*prefix, end), CUTS[1])
        for wrong in [
            [prefix[0], end],
            [prefix[1], prefix[0], end],
            [*prefix, ('write', HOME, str(13 * 16384), '16384', 'ATTEMPT'), end],
            [*prefix, ('write', HOME, str(12 * 16384), '16384', 'ATTEMPT'), end],
            [*prefix, prefix[0], prefix[1], end],
        ]:
            with self.assertRaises(ValueError):
                audit_cut(rows(*wrong), CUTS[1])

    def test_cumulative_and_preallocation_caps_apply_after_fast_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / 'run'
            (output / 'database').mkdir(parents=True)
            for phase in ('produce', 'recover'):
                (output / phase).mkdir()
                (output / phase / 'stdout.log').write_bytes(b'123456')
            limits = dict(native.BUDGET, output_bytes=8, artifact_bytes=100)
            with patch.dict(native.BUDGET, limits, clear=True):
                with self.assertRaises(ValueError):
                    native.enforce_bounds(output, root)
                (output / 'recover' / 'stdout.log').write_bytes(b'1')
                native.enforce_bounds(output, root, 90)
                with self.assertRaises(ValueError):
                    native.enforce_bounds(output, root, 94)

    def test_wrong_or_duplicate_reached_barrier_never_qualifies(self):
        for events in [
            [('barrier', CUTS[0], '0', '1', 'REACHED')],
            [('barrier', CUTS[1], '0', '1', 'REACHED')],
            [('barrier', CUTS[1], '0', '1', 'REACHED')] * 2,
        ]:
            with self.assertRaises(ValueError):
                audit_cut(rows(*events), CUTS[1])


if __name__ == '__main__':
    unittest.main()
