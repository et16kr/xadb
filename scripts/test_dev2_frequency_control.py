#!/usr/bin/env python3
"""New PF04 control boundaries only; all sysfs writes use file fixtures."""
import json
import os
from pathlib import Path
import signal
import tempfile
import unittest
from unittest.mock import patch

import dev2_frequency_control as control


def baseline():
    value = {"boost": "1", "driver_mode": "active", "online": "0-31", "boot": "test-boot",
             "identity": {}, "policies": {}}
    for cpu in (2, 18, 4):
        value["policies"][f"policy{cpu}"] = {
            "related_cpus": str(cpu), "affected_cpus": str(cpu),
            "scaling_driver": "amd-pstate-epp", "scaling_governor": "performance",
            "energy_performance_preference": "performance", "scaling_min_freq": "1480264",
            "scaling_max_freq": "5460527"}
    for cpu in (2, 18):
        value["identity"][str(cpu)] = {"nominal_freq": "2500", "nominal_perf": "76", "siblings": "2,18"}
    return value


class FakeSysfs:
    def __init__(self, path, state):
        self.path, self.state = path, state
        self.path.write_text(json.dumps(baseline()))
        self.writes = []
        self.crash = False
        self.fail_restore = False

    def snapshot(self):
        return json.loads(self.path.read_text())

    def write_max(self, cpu, value):
        saved = control.read_json(self.state)
        event = saved["events"][-1]
        assert event["cpu"] == cpu and event["post"] == value
        assert event["operation"] in ("apply-intent", "restore-intent")
        self.writes.append((cpu, value))
        if self.fail_restore and value == "5460527":
            raise OSError("restore write refused")
        current = self.snapshot()
        current["policies"][f"policy{cpu}"]["scaling_max_freq"] = value
        self.path.write_text(json.dumps(current))
        if self.crash:
            os._exit(73)


class ControlCases(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.directory = self.root / "owned"
        self.directory.mkdir()
        self.config = {"mode": "normal", "id": "owned"}
        self.write_config()
        self.lease = self.root / "lease"
        self.fs = FakeSysfs(self.root / "sysfs.json", self.directory / "state.json")
        self.owner = control.Control(self.directory, self.lease, self.fs)

    def tearDown(self):
        self.temp.cleanup()

    def write_config(self):
        (self.directory / "config.json").write_text(json.dumps(self.config))

    def state(self):
        return control.read_json(self.directory / "state.json")

    def test_normal_max_only_restoration_and_repeat_restore(self):
        self.owner.apply()
        self.assertEqual(self.fs.writes, [(2, "2500000"), (18, "2500000")])
        self.assertTrue(self.owner.owned())
        self.owner.restore()
        self.assertEqual(self.fs.writes[-2:], [(18, "5460527"), (2, "5460527")])
        self.assertEqual(self.fs.snapshot(), baseline())
        self.assertFalse(self.lease.exists())
        self.owner.restore()
        self.assertEqual(len(self.fs.writes), 4)
        with self.assertRaisesRegex(RuntimeError, "single use"):
            self.owner.apply()

    def test_partial_pre_failure_preserves_error_and_restores_one(self):
        self.owner.config["mode"] = "partial-pre-failure"
        with self.assertRaisesRegex(RuntimeError, "injected"):
            self.owner.apply()
        self.assertEqual(self.state()["intents"], [2])
        self.owner.restore()
        self.assertEqual(self.fs.writes, [(2, "2500000"), (2, "5460527")])
        self.assertIn("injected", self.state()["apply_error"])
        self.assertEqual(self.fs.snapshot(), baseline())

    def test_process_death_after_write_recovers_persisted_intent(self):
        pid = os.fork()
        if pid == 0:
            self.fs.crash = True
            self.owner.apply()
            os._exit(1)
        _, status = os.waitpid(pid, 0)
        self.assertEqual(os.waitstatus_to_exitcode(status), 73)
        self.assertEqual(self.state()["events"][-1]["operation"], "apply-intent")
        self.assertTrue(self.owner.owned())
        self.owner.restore()
        self.assertEqual(self.fs.snapshot(), baseline())
        self.assertEqual(self.fs.writes, [(2, "5460527")])

    def test_invalid_identity_rejected_before_lease_and_write(self):
        current = baseline()
        current["identity"]["2"]["nominal_perf"] = "77"
        self.fs.path.write_text(json.dumps(current))
        with self.assertRaisesRegex(RuntimeError, "CPU identity"):
            self.owner.apply()
        self.owner.restore()
        self.assertFalse(self.lease.exists())
        self.assertEqual(self.fs.writes, [])

    def test_other_run_lease_cannot_be_removed_or_overwritten(self):
        other = self.root / "other.json"
        other.write_text('{"id":"other"}')
        os.link(other, self.lease)
        with self.assertRaisesRegex(RuntimeError, "occupied"):
            self.owner.apply()
        self.owner.restore()
        self.assertTrue(os.path.samefile(other, self.lease))
        self.assertEqual(self.fs.writes, [])

    def test_external_max_conflict_quarantines_without_writes(self):
        self.owner.apply()
        current = self.fs.snapshot()
        current["policies"]["policy2"]["scaling_max_freq"] = "3000000"
        self.fs.path.write_text(json.dumps(current))
        with self.assertRaisesRegex(RuntimeError, "max conflict"):
            self.owner.restore()
        self.assertEqual(self.state()["status"], "QUARANTINED")
        self.assertTrue(self.lease.exists())
        self.assertEqual(len(self.fs.writes), 2)
        self.assertEqual(self.fs.snapshot(), current)

    def test_unowned_policy_change_quarantines(self):
        self.owner.apply()
        current = self.fs.snapshot()
        current["policies"]["policy4"]["scaling_governor"] = "powersave"
        self.fs.path.write_text(json.dumps(current))
        with self.assertRaisesRegex(RuntimeError, "unowned"):
            self.owner.restore()
        self.assertTrue(self.owner.owned())
        self.assertEqual(len(self.fs.writes), 2)

    def test_restore_write_failure_keeps_lease_and_error(self):
        self.owner.apply()
        self.fs.fail_restore = True
        with self.assertRaisesRegex(OSError, "refused"):
            self.owner.restore()
        self.assertEqual(self.state()["status"], "QUARANTINED")
        self.assertTrue(self.owner.owned())

    def test_state_binding_and_missing_lease_fail_closed(self):
        self.owner.apply()
        state = self.state()
        state["config"]["id"] = "not-owner"
        control.save_json(self.owner.path, state)
        with self.assertRaisesRegex(RuntimeError, "state binding"):
            self.owner.restore()
        state["config"] = self.config
        control.save_json(self.owner.path, state)
        self.lease.unlink()
        with self.assertRaisesRegex(RuntimeError, "lost owned"):
            self.owner.restore()
        self.assertEqual(len(self.fs.writes), 2)

    def test_apply_deadline_after_intent_is_restorable(self):
        with patch.object(self.fs, "write_max", side_effect=lambda *_: control.deadline(signal.SIGALRM, None)):
            with self.assertRaises(TimeoutError):
                self.owner.apply()
        self.assertEqual(self.state()["intents"], [2])
        self.owner.restore()
        self.assertEqual(self.fs.writes, [])
        self.assertEqual(self.fs.snapshot(), baseline())

    def test_privileged_cli_rejects_uninstalled_source_and_untrusted_path(self):
        with self.assertRaisesRegex(RuntimeError, "installation path"):
            control.installed_config()
        with self.assertRaisesRegex(RuntimeError, "untrusted"):
            control.trusted(self.directory / "config.json")
        with patch.object(Path, "write_text") as write:
            for cpu, value in ((4, "2500000"), (2, "3000000")):
                with self.assertRaisesRegex(RuntimeError, "outside contract"):
                    control.Sysfs().write_max(cpu, value)
            write.assert_not_called()

    def test_manager_limits_and_exit_receipt(self):
        from subprocess import CompletedProcess
        rows = [f"{key}={value}" for key, value in control.SUPERVISION.items()]
        with patch.object(control.subprocess, "run") as run:
            run.return_value = CompletedProcess([], 0, ("\n".join(rows)).encode(), b"")
            self.assertEqual(control.check_supervision("owned.service"), control.SUPERVISION)
            self.assertEqual(run.call_args.kwargs["timeout"], 2)
            for wrong in (rows[:-1], rows + [rows[0]], [r.replace("RuntimeMaxUSec=10s", "RuntimeMaxUSec=infinity") for r in rows]):
                run.return_value = CompletedProcess([], 0, ("\n".join(wrong)).encode(), b"")
                with self.assertRaisesRegex(RuntimeError, "supervision"):
                    control.check_supervision("owned.service")
        self.owner.apply()
        with patch.dict(os.environ, {"SERVICE_RESULT": "signal", "EXIT_CODE": "killed", "EXIT_STATUS": "KILL"}):
            self.owner.restore()
        self.assertEqual(self.state()["manager_exit"]["SERVICE_RESULT"], "signal")


if __name__ == "__main__":
    unittest.main()
