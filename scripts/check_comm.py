#!/usr/bin/env python3
"""Verify implemented COMM leaves with retained logs; never grant full C1 closure."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import selectors
import shutil
import socket
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from check_dev0 import SOURCE_PATHS, check_report_paths, digest, source_fingerprint, verify_source_identity, write_report

ROOT = Path(__file__).resolve().parent.parent
# Frozen leaf inventory. Full C-V01--18/V3 closure remains a separate obligation.
REQUIRED_TESTS = {
    'connection::tests::drain_timestamp_is_sampled_under_the_admission_gate',
    'resources::tests::connection_control_layout_fits_fixed_budget',
    'independent_vectors_boundaries_and_header_rejection_before_allocation',
    'every_small_cut_coalescing_byte_input_and_partial_eof_preserve_delivery',
    'every_small_bit_corruption_is_terminal_without_resynchronization',
    'maximum_frame_boundaries_and_large_corruption_keep_quotas',
    'v2_64_seeds_128_frames_chunked_against_independent_bytes',
    'short_io_interrupt_block_and_zero_keep_offsets_and_exact_failure_bytes',
    'quotas_hold_external_rx_and_terminal_tickets_across_owner_drop',
    'connection_limit_and_global_payload_competition_release_without_leaks',
    'last_io_crossing_deadline_never_reports_success_but_keeps_full_progress',
    'drain_discards_partial_rx_timer_but_preserves_verified_frames_and_tx',
    'drop_partial_tx_cancel_after_last_write_and_stale_event_are_terminal',
    'connecting_has_write_interest_without_tx_and_deadlines_are_absolute',
    'clock_regression_closes_gate_permanently_and_zero_progress_is_bounded',
    'concurrent_producers_and_cancellation_return_every_ticket_and_charge',
    'interest_clock_regression_is_sticky_even_after_time_recovers',
    'native_ipv4_ipv6_and_owned_unix_independent_peer_vectors',
    'native_pending_connect_without_tx_and_refusal',
    'owned_unix_preserves_files_symlinks_and_replaced_socket_paths',
    'native_buffered_frame_then_fin_is_delivered_before_eof',
    'empty_native_wait_preserves_submillisecond_timeout',
    'listener_fairness_eight_attempt_budget_full_pause_and_generation_rejection',
    'listener_wouldblock_backoff_empty_wait_and_invalid_registration_close',
    'wait_error_keeps_original_cause_and_clock_domains_cannot_mix',
    'permanently_ready_connections_each_receive_bounded_turn',
    'retained_hup_never_doubles_per_step_read_budget',
    'error_only_hint_without_socket_error_is_backed_off',
}


def wire(payload):
    """Independent AXF1 encoder: no production codec/state transition imports."""
    if len(payload) > 1048576:
        raise ValueError('payload limit')
    body = b'AXF1\x01\x00' + len(payload).to_bytes(4, 'little') + payload
    return body + hashlib.sha256(body).digest()


def inspect_wire(data):
    if len(data) < 42 or data[:6] != b'AXF1\x01\x00':
        raise ValueError('invalid frame header')
    size = int.from_bytes(data[6:10], 'little')
    if size > 1048576 or len(data) != size + 42 or hashlib.sha256(data[:-32]).digest() != data[-32:]:
        raise ValueError('invalid frame size/digest')
    return data[10:-32]


def check_inventory(output):
    names = set(re.findall(r'^test (\S+) \.\.\. ok$', output, re.M))
    missing = REQUIRED_TESTS - names
    if missing:
        raise ValueError(f'required tests missing: {sorted(missing)}')
    if re.search(r'test result: FAILED', output):
        raise ValueError('failed suite')


def check_ready(receipt, expected_source, pid):
    if receipt.get('event') != 'ready' or receipt.get('source_sha256') != expected_source or receipt.get('pid') != pid or receipt.get('profile') != 'debug':
        raise ValueError('wrong worker identity or missing ready receipt')


def check_closed(receipt, mode, frames):
    expected = {'echo': 'Closed', 'corrupt': 'InvalidFrame', 'truncated': 'TruncatedFrame'}[mode]
    if receipt.get('event') != 'closed' or receipt.get('reason') != expected:
        raise ValueError('missing terminal or wrong fault outcome')
    if receipt.get('payload_bytes') != 0 or receipt.get('connections') != 0:
        raise ValueError('resource ledger not returned to baseline')
    if receipt.get('frames') != frames or receipt.get('flushed') != frames or receipt.get('failed') != 0:
        raise ValueError('missing peer/flush history')


def read_ready(process):
    # One bounded line. No unbounded stdout or blocking readline from a bad binary.
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        data = bytearray()
        end = time.monotonic() + 5
        while b'\n' not in data and len(data) < 4096:
            remaining = end - time.monotonic()
            if remaining <= 0 or not selector.select(remaining):
                raise ValueError('worker ready deadline')
            chunk = os.read(process.stdout.fileno(), 1)
            if not chunk:
                raise ValueError('worker exited before ready')
            data += chunk
    if not data.endswith(b'\n'):
        raise ValueError('oversize worker receipt')
    return json.loads(data)


def native_case(binary, family, mode, session, expected_source):
    directory = session / f'{family}-{mode}'
    directory.mkdir()
    argv = [str(binary), family]
    unix_root = tempfile.TemporaryDirectory(prefix='xadb-comm-unix-') if family == 'unix' else None
    if unix_root:
        argv.append(str(Path(unix_root.name) / 'private'))
    peer = None
    process = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    receipt = {'command': argv, 'pid': process.pid, 'family': family, 'mode': mode, 'peer_started': False, 'observations': []}
    try:
        ready = read_ready(process)
        receipt['ready'] = ready
        check_ready(ready, expected_source, process.pid)
        if family == 'unix':
            peer = socket.socket(socket.AF_UNIX)
            address = ready['unix']
        else:
            peer = socket.socket(socket.AF_INET if family == 'tcp4' else socket.AF_INET6)
            host, port = ready['tcp'].rsplit(':', 1)
            address = (host.strip('[]'), int(port))
        peer.settimeout(5)
        peer.connect(address)
        receipt['peer_started'] = True
        receipt['socket_buffers'] = {'send': peer.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF), 'receive': peer.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF)}
        if mode == 'echo':
            for size in (0, 1, 2, 64, 4096, 65536, 1048576):
                payload = bytes(i % 251 for i in range(size))
                expected = wire(payload)
                start = time.monotonic_ns()
                for offset in range(0, len(expected), 997):
                    peer.sendall(expected[offset:offset + 997])
                observed = bytearray()
                while len(observed) < len(expected):
                    chunk = peer.recv(min(65536, len(expected) - len(observed)))
                    if not chunk:
                        raise ValueError('peer EOF before complete echo')
                    observed += chunk
                if bytes(observed) != expected or inspect_wire(observed) != payload:
                    raise ValueError('independent peer mismatch')
                receipt['observations'].append({'payload_bytes': size, 'wire_sha256': hashlib.sha256(observed).hexdigest(), 'round_trip_ns': time.monotonic_ns() - start})
            peer.shutdown(socket.SHUT_WR)
        elif mode == 'corrupt':
            bad = bytearray(wire(b'fault'))
            bad[10] ^= 1
            peer.sendall(bad)
            receipt['fault'] = {'kind': 'checksum_bit_flip', 'byte': 10, 'sent_bytes': len(bad)}
        elif mode == 'truncated':
            peer.sendall(wire(b'fault')[:11])
            peer.shutdown(socket.SHUT_WR)
            receipt['fault'] = {'kind': 'body_eof', 'sent_bytes': 11}
        elif mode == 'kill':
            peer.sendall(wire(bytes(1048576))[:512])
            process.kill()
            process.wait(timeout=5)
            if process.returncode != -9:
                raise ValueError('process kill not witnessed')
            receipt['fault'] = {'kind': 'SIGKILL', 'sent_bytes': 512, 'returncode': process.returncode}
            receipt['cleanup'] = 'PROCESS_REAPED_NO_IN_PROCESS_QUOTA_CLAIM'
            return receipt
        if peer.recv(1) != b'':
            raise ValueError('unexpected trailing peer bytes')
        output, errors = process.communicate(timeout=5)
        receipt['stderr'] = errors.decode(errors='replace')
        receipt['returncode'] = process.returncode
        if process.returncode != 0:
            raise ValueError('worker failure')
        closed = json.loads(output)
        check_closed(closed, mode, 7 if mode == 'echo' else 0)
        receipt['closed'] = closed
        if family == 'unix' and Path(ready['unix']).exists():
            raise ValueError('owned socket not cleaned')
        return receipt
    finally:
        if peer:
            peer.close()
        if process.poll() is None:
            process.kill()
        process.communicate(timeout=5)
        write_report(directory / 'receipt.json', receipt)
        if unix_root:
            unix_root.cleanup()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--with-regressions', action='store_true', help='run current DEV2 P0/K0 + DEV1/DEV0 regression gate')
    parser.add_argument('--snapshot', action='store_true', help='verify a retained source-only export while other workspace tasks continue')
    args = parser.parse_args()
    check_report_paths([args.report])
    parent = ROOT / 'artifacts/comm'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='check-', dir=parent))
    if args.snapshot:
        export = session / 'source'
        captured = False
        for _ in range(3):
            before = source_fingerprint()
            if export.exists():
                shutil.rmtree(export)
            export.mkdir()
            for name in (*SOURCE_PATHS, 'devdoc'):
                source = ROOT / name
                if source.is_dir():
                    shutil.copytree(source, export / name)
                else:
                    shutil.copy2(source, export / name)
            if source_fingerprint(export) == before == source_fingerprint():
                captured = True
                break
        if not captured:
            raise RuntimeError('workspace kept changing while capturing source; no coherent export')
        command = [sys.executable, '-B', 'scripts/check_comm.py', '--report', str(session / 'export-validation.json')]
        if args.with_regressions:
            command.append('--with-regressions')
        result = subprocess.run(command, cwd=export, timeout=3600)
        report = json.loads((session / 'export-validation.json').read_text())
        report['retained_source_export'] = str(export.relative_to(ROOT))
        report['execution_root'] = str(export)
        report['live_source_sha256_after'] = source_fingerprint()
        report['evidence_scope'] = 'The exact retained source export only; later concurrent workspace edits are excluded.'
        write_report(session / 'validation.json', report)
        if args.report:
            write_report(args.report, report)
        print(f'COMM exported leaf {report["status"]}: {session / "validation.json"}', flush=True)
        return result.returncode
    checks = []
    report = {'schema_version': 1, 'date': datetime.now(timezone.utc).isoformat(), 'scope': 'COMM implemented C0-C3 leaves and focused C4 native/checker integration; not full C1',
              'status': 'RUNNING', 'c1_status': 'INCOMPLETE', 'official_distribution': False, 'artifact_directory': str(session.relative_to(ROOT)), 'checks': checks,
              'unverified': ['full depth-8 two-connection model', 'full native fault matrix x3', '30-minute codec/event fuzz and thread stress', 'C-V16 complete mutant inventory', 'C-V18 workload/Always-Detailed overhead acceptance', 'complete diagnostic ring/latency profile'],
              'environment': {'uname': list(platform.uname()), 'cpu_count': os.cpu_count()}, 'property_profile': {'seeds': list(range(64)), 'operations_per_seed': 128, 'generator': 'tests/framing.rs LCG 6364136223846793005*x+1'}}
    env = os.environ.copy()
    env['CARGO_TARGET_DIR'] = str(ROOT / 'target')

    def command(name, argv, inventory=False):
        print(f'{name}: {" ".join(argv)}', flush=True)
        result = subprocess.run(argv, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=900)
        log = session / f'{name}.log'
        log.write_bytes(result.stdout)
        checks.append({'id': name, 'command': argv, 'exit_code': result.returncode, 'status': 'PASS' if result.returncode == 0 else 'FAIL', 'log': str(log.relative_to(ROOT)), 'log_sha256': digest(log)})
        if result.returncode != 0:
            print(result.stdout.decode(errors='replace'), flush=True)
            raise RuntimeError(f'{name} failed')
        if inventory:
            check_inventory(result.stdout.decode())
        return result.stdout

    exit_code = 0
    try:
        before = source_fingerprint()
        report['source_sha256_before'] = before
        report['lockfile_sha256'] = digest(ROOT / 'Cargo.lock')
        report['contract_sha256'] = {name: digest(ROOT / name) for name in ('devdoc/contracts/comm-design.md', 'devdoc/plans/comm-implementation-plan.md')}
        command('checker-negative-tests', ['python3', '-B', '-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'test_check_comm.py'])
        command('transport-default', ['cargo', 'test', '-p', 'xadb-transport', '--no-default-features', '--locked', '--offline', '--', '--test-threads=1'], inventory=True)
        command('transport-instrumented', ['cargo', 'test', '-p', 'xadb-transport', '--all-features', '--locked', '--offline', '--', '--test-threads=1'], inventory=True)
        command('comm-lint', ['cargo', 'clippy', '-p', 'xadb-transport', '-p', 'xadbtest-transport-worker', '--all-features', '--all-targets', '--locked', '--offline', '--', '-D', 'warnings'])
        metadata = json.loads(command('dependency-boundary', ['cargo', 'metadata', '--format-version', '1', '--locked', '--offline', '--filter-platform', 'x86_64-unknown-linux-gnu']))
        packages = {p['id']: p for p in metadata['packages']}
        nodes = {n['id']: n for n in metadata['resolve']['nodes']}
        report['normal_dependency_closures'] = {}
        for name, allowed in [('xadb-transport', {'xadb-transport', 'xadb-foundation'}), ('xadbtest-transport-worker', {'xadbtest-transport-worker', 'xadb-transport', 'xadb-platform', 'xadb-foundation'})]:
            pending = [p['id'] for p in packages.values() if p['name'] == name]
            seen = set()
            while pending:
                current = pending.pop()
                if current in seen:
                    continue
                seen.add(current)
                pending += [d['pkg'] for d in nodes[current]['deps'] if any(k['kind'] is None for k in d['dep_kinds'])]
            if any(packages[p]['source'] is None and packages[p]['name'] not in allowed for p in seen):
                raise ValueError('forbidden normal workspace dependency')
            report['normal_dependency_closures'][name] = sorted(packages[p]['name'] for p in seen)
        with tempfile.TemporaryDirectory(prefix='xadb-comm-independent-') as directory:
            command('independent-build', ['cargo', 'build', '-p', 'xadb-transport', '--no-default-features', '--locked', '--offline', '--target-dir', directory])
        command('worker-default-build', ['cargo', 'build', '-p', 'xadbtest-transport-worker', '--locked', '--offline'])
        binary = session / 'xadbtest-transport-worker'
        shutil.copy2(ROOT / 'target/debug/xadbtest-transport-worker', binary)
        report['binary_sha256'] = digest(binary)
        report['native'] = []
        for family in ('tcp4', 'tcp6', 'unix'):
            for mode in ('echo', 'corrupt', 'truncated', 'kill'):
                print(f'native: {family} {mode}', flush=True)
                report['native'].append(native_case(binary, family, mode, session, before))
        if args.with_regressions:
            command('lower-regressions', ['python3', 'scripts/check_dev2_reference.py', '--with-substrate', '--report', str(session / 'dev2-regression.json')])
        else:
            report['lower_regressions'] = 'NOT_RUN_IN_THIS_INVOCATION'
        report['source_sha256_after'] = verify_source_identity(before, before)
        report['source_sha256'] = before
        report['status'] = 'PASS'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        exit_code = 1
    write_report(session / 'validation.json', report)
    if args.report:
        write_report(args.report, report)
    print(f'COMM leaf {report["status"]}; C1 INCOMPLETE: {session / "validation.json"}', flush=True)
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
