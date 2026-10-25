#!/usr/bin/env python3
"""PF04 CPU2/18 max-only control, owned by a bounded systemd service.

The privileged CLI accepts only apply/restore in a root-installed, hash-bound
copy. It does not accept sysfs paths or arbitrary commands. Firmware clock is
not measured here. A persistent lease excludes cooperating runs, not other
privileged sysfs writers. Conflicting state is quarantined without overwriting.
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import sys
import time

CPUS = (2, 18)
ORIGINAL_MAX = "5460527"
TARGET_MAX = "2500000"
FIELDS = ("related_cpus", "affected_cpus", "scaling_driver", "scaling_governor",
          "energy_performance_preference", "scaling_min_freq", "scaling_max_freq")
MODES = ("normal", "partial-pre-failure", "body-sigkill")
SUPERVISION = {"User": "1000", "Type": "exec", "Restart": "no", "RemainAfterExit": "no",
               "KillMode": "control-group", "RuntimeMaxUSec": "10s", "TimeoutStartUSec": "10s",
               "TimeoutStopUSec": "30s", "SendSIGKILL": "yes"}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def read_json(path):
    with path.open("rb") as stream:
        raw = stream.read(131073)
    require(len(raw) <= 131072, "state size limit")
    return json.loads(raw)


def save_json(path, value):
    """Replace a complete state record atomically; intent precedes every write."""
    temporary = path.with_suffix(".tmp")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "w") as stream:
        json.dump(value, stream, sort_keys=True, indent=2)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)
    fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


class Sysfs:
    """Only the two fixed maximum-frequency files are writable."""
    root = Path("/sys/devices/system/cpu")

    def snapshot(self):
        policies = {}
        for path in sorted((self.root / "cpufreq").glob("policy*")):
            require(path.is_dir() and not path.is_symlink(), "policy identity")
            policies[path.name] = {key: (path / key).read_text().strip() for key in FIELDS}
        identity = {}
        for cpu in CPUS:
            base = self.root / f"cpu{cpu}"
            require((base / "cpufreq").resolve() == self.root / "cpufreq" / f"policy{cpu}",
                    "CPU policy mapping")
            identity[str(cpu)] = {
                key: (base / "acpi_cppc" / key).read_text().strip()
                for key in ("nominal_freq", "nominal_perf")
            }
            identity[str(cpu)]["siblings"] = (base / "topology/thread_siblings_list").read_text().strip()
        return {
            "policies": policies, "identity": identity,
            "boost": (self.root / "cpufreq/boost").read_text().strip(),
            "driver_mode": (self.root / "amd_pstate/status").read_text().strip(),
            "online": (self.root / "online").read_text().strip(),
            "boot": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
        }

    def write_max(self, cpu, value):
        require(cpu in CPUS and value in (ORIGINAL_MAX, TARGET_MAX), "write outside contract")
        (self.root / "cpufreq" / f"policy{cpu}" / "scaling_max_freq").write_text(value + "\n")


def check_baseline(value):
    require(value["boost"] == "1" and value["driver_mode"] == "active", "global identity")
    for cpu in CPUS:
        require(value["identity"][str(cpu)] == {
            "nominal_freq": "2500", "nominal_perf": "76", "siblings": "2,18"
        }, "CPU identity")
        require(value["policies"][f"policy{cpu}"] == {
            "related_cpus": str(cpu), "affected_cpus": str(cpu),
            "scaling_driver": "amd-pstate-epp", "scaling_governor": "performance",
            "energy_performance_preference": "performance", "scaling_min_freq": "1480264",
            "scaling_max_freq": ORIGINAL_MAX,
        }, "policy baseline")


def check_known(current, baseline, intended):
    """Allow only our intent-covered max values; all other identity is exact."""
    normalized = json.loads(json.dumps(current))
    for cpu in intended:
        policy = normalized["policies"][f"policy{cpu}"]
        require(policy["scaling_max_freq"] in (ORIGINAL_MAX, TARGET_MAX), "max conflict")
        policy["scaling_max_freq"] = ORIGINAL_MAX
    require(normalized == baseline, "identity or unowned setting conflict")


class Control:
    """State machine; the CLI supplies the operation lock and trusted paths."""
    def __init__(self, directory, lease, sysfs, supervision=None):
        self.directory, self.lease, self.sysfs = directory, lease, sysfs
        self.config_path = directory / "config.json"
        self.path = directory / "state.json"
        self.config = read_json(self.config_path)
        self.supervision = supervision

    def owned(self):
        return self.lease.exists() and os.path.samefile(self.config_path, self.lease)

    def save(self, state):
        state["updated_monotonic_ns"] = time.monotonic_ns()
        save_json(self.path, state)

    def apply(self):
        require(not self.path.exists(), "apply is single use")
        state = {"config": self.config, "status": "PREPARING", "intents": [], "events": []}
        state["supervision"] = self.supervision
        self.save(state)
        try:
            require(not self.lease.exists(), "persistent lease occupied")
            state["baseline"] = self.sysfs.snapshot()
            check_baseline(state["baseline"])
            self.save(state)
            os.link(self.config_path, self.lease)
            state["status"] = "APPLYING"
            self.save(state)
            for cpu in CPUS:
                before = self.sysfs.snapshot()
                check_known(before, state["baseline"], state["intents"])
                require(before["policies"][f"policy{cpu}"]["scaling_max_freq"] == ORIGINAL_MAX,
                        "unexpected pre-write max")
                state["intents"].append(cpu)
                state["events"].append({"cpu": cpu, "operation": "apply-intent",
                                        "pre": ORIGINAL_MAX, "post": TARGET_MAX})
                self.save(state)
                self.sysfs.write_max(cpu, TARGET_MAX)
                after = self.sysfs.snapshot()
                check_known(after, state["baseline"], state["intents"])
                require(after["policies"][f"policy{cpu}"]["scaling_max_freq"] == TARGET_MAX,
                        "apply readback mismatch")
                state["events"].append({"cpu": cpu, "operation": "apply-readback", "snapshot": after})
                self.save(state)
                if self.config["mode"] == "partial-pre-failure":
                    raise RuntimeError("injected failure after first write")
            state["status"] = "APPLIED"
            self.save(state)
        except BaseException as error:
            state["status"] = "APPLY_FAILED"
            state["apply_error"] = repr(error)
            self.save(state)
            raise

    def restore(self):
        if not self.owned():
            if self.path.exists():
                state = read_json(self.path)
                require(state["config"] == self.config, "state binding")
                require(not state["intents"] or state["status"] == "RESTORED", "lost owned lease")
            return
        state = read_json(self.path)
        require(state["config"] == self.config, "state binding")
        state["manager_exit"] = {key: os.environ.get(key) for key in
                                 ("SERVICE_RESULT", "EXIT_CODE", "EXIT_STATUS", "INVOCATION_ID")}
        try:
            check_known(self.sysfs.snapshot(), state["baseline"], state["intents"])
            for cpu in reversed(state["intents"]):
                before = self.sysfs.snapshot()
                check_known(before, state["baseline"], state["intents"])
                current = before["policies"][f"policy{cpu}"]["scaling_max_freq"]
                state["events"].append({"cpu": cpu, "operation": "restore-intent",
                                        "pre": current, "post": ORIGINAL_MAX})
                self.save(state)
                if current != ORIGINAL_MAX:
                    self.sysfs.write_max(cpu, ORIGINAL_MAX)
                after = self.sysfs.snapshot()
                require(after["policies"][f"policy{cpu}"]["scaling_max_freq"] == ORIGINAL_MAX,
                        "restore readback mismatch")
                check_known(after, state["baseline"], state["intents"])
                state["events"].append({"cpu": cpu, "operation": "restore-readback", "snapshot": after})
                self.save(state)
            state["final"] = self.sysfs.snapshot()
            require(state["final"] == state["baseline"], "final restoration mismatch")
            state["status"] = "RESTORED"
            self.save(state)
            require(self.owned(), "lease replaced during restore")
            self.lease.unlink()
        except BaseException as error:
            state["status"] = "QUARANTINED"
            state["restore_error"] = repr(error)
            self.save(state)
            raise


def trusted(path, directory=False):
    info = path.lstat()
    require(info.st_uid == 0 and not info.st_mode & 0o022, "untrusted ownership/mode")
    require(stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode),
            "unexpected path type")


def installed_config():
    source = Path(__file__).absolute()
    directory = source.parent
    require(source.name == "control.py" and directory.parent == Path("/run"), "installation path")
    require(re.fullmatch(r"xadb-pf04-frequency-[a-f0-9]{16}", directory.name), "installation id")
    for path in (Path("/"), Path("/run"), directory):
        trusted(path, directory=True)
    trusted(source)
    trusted(directory / "config.json")
    config = read_json(directory / "config.json")
    require(set(config) == {"id", "uid", "mode", "sha256", "unit"}, "config fields")
    require(config["id"] == directory.name and config["unit"] == directory.name + ".service", "unit binding")
    require(type(config["uid"]) is int and config["uid"] == 1000, "caller UID")
    require(config["mode"] in MODES, "functional mode")
    require(hashlib.sha256(source.read_bytes()).hexdigest() == config["sha256"], "code hash binding")
    require(Path("/proc/self/cgroup").read_text().strip() == "0::/system.slice/" + config["unit"],
            "systemd cgroup binding")
    return directory, config


def deadline(_signum, _frame):
    raise TimeoutError("helper deadline")


def check_supervision(unit):
    result = subprocess.run(
        ["/usr/bin/systemctl", "show", unit, "--no-pager", "--property=" + ",".join(SUPERVISION)],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=2, check=True,
        env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"})
    rows = result.stdout.decode().splitlines()
    observed = dict(row.split("=", 1) for row in rows)
    require(len(rows) == len(observed) and observed == SUPERVISION, "systemd supervision mismatch")
    return observed


def main():
    require(len(sys.argv) == 2 and sys.argv[1] in ("apply", "restore", "body"), "fixed command")
    directory, config = installed_config()
    action = sys.argv[1]
    if action == "body":
        require(os.getuid() == config["uid"] and os.geteuid() == config["uid"], "body UID")
        status = dict(line.split(":", 1) for line in Path("/proc/self/status").read_text().splitlines())
        require(all(int(status[key].strip(), 16) == 0 for key in ("CapPrm", "CapEff", "CapAmb")),
                "body capabilities")
        observed = Sysfs().snapshot()
        for cpu in CPUS:
            require(observed["policies"][f"policy{cpu}"]["scaling_max_freq"] == TARGET_MAX,
                    "body target max")
        print(json.dumps({"body": config["mode"], "uid": os.getuid(), "cap_eff": status["CapEff"].strip(),
                          "snapshot": observed}), flush=True)
        if config["mode"] == "body-sigkill":
            os.kill(os.getpid(), signal.SIGKILL)
        return
    require(os.geteuid() == 0, "root helper required")
    signal.signal(signal.SIGALRM, deadline)
    signal.alarm(5 if action == "apply" else 25)
    lock_path = Path("/run/xadb-pf04-frequency.lock")
    fd = os.open(lock_path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        trusted(lock_path)
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        supervision = check_supervision(config["unit"]) if action == "apply" else None
        control = Control(directory, Path("/run/xadb-pf04-frequency.lease"), Sysfs(), supervision)
        getattr(control, action)()
    finally:
        os.close(fd)
        signal.alarm(0)


if __name__ == "__main__":
    main()
