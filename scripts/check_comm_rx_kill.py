#!/usr/bin/env python3
"""Verify selected native RX-prefix/SIGKILL cells, never the full COMM gate."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import secrets
import selectors
import signal
import socket
import subprocess
import tempfile
import time

from check_comm import wire
from check_dev0 import check_report_paths, digest, source_fingerprint
from comm_campaign import capture_source, identity, write_json

ROOT = Path(__file__).resolve().parent.parent
PAYLOAD = bytes(range(64))
FRAME = wire(PAYLOAD)
PREFIXES = {'header_partial': 5, 'body_partial': 17,
            'digest_partial': len(FRAME) - 1, 'complete': len(FRAME)}
CELLS = tuple(f'{family}/{stage}/{repetition}'
              for family in ('tcp4', 'tcp6', 'unix')
              for stage in PREFIXES for repetition in range(3))
MAX_RECEIPT_BYTES = 8192


def read_receipt(process, raw_log, timeout):
    """Bound the protocol and retain raw bytes even on malformed/partial JSON."""
    if process.stdout is None:
        raise ValueError('worker stdout unavailable')
    value = getattr(process, '_comm_line_buffer', bytearray())
    deadline = time.monotonic() + timeout
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        try:
            while True:
                newline = value.find(b'\n')
                if newline >= 0:
                    line = bytes(value[:newline])
                    del value[:newline + 1]
                    return json.loads(line)
                remaining = deadline - time.monotonic()
                if remaining <= 0 or not selector.select(remaining):
                    raise ValueError('worker receipt deadline')
                chunk = os.read(process.stdout.fileno(), 4096)
                if chunk:
                    raw_log.write(chunk)
                    raw_log.flush()
                    if raw_log.tell() > MAX_RECEIPT_BYTES:
                        raise ValueError('worker receipt exceeds bounded protocol')
                    value.extend(chunk)
                else:
                    raise ValueError('worker exited before complete receipt')
        finally:
            process._comm_line_buffer = value


def check_identity(receipt, expected, event):
    if not isinstance(receipt, dict):
        raise ValueError(f'malformed {event} receipt')
    for key, value in expected.items():
        if key not in receipt or receipt[key] != value or type(receipt[key]) is not type(value):
            raise ValueError(f'wrong {key} in {event} receipt')
    if receipt.get('event') != event:
        raise ValueError(f'missing {event} receipt')


def check_prefix(receipt, expected, stage):
    check_identity(receipt, expected, 'rx_prefix')
    complete = stage == 'complete'
    wanted = {'phase': 'Open', 'read_bytes': PREFIXES[stage], 'written_bytes': 0,
              'received_frames': int(complete), 'overflowed': False,
              'payload': list(PAYLOAD) if complete else None}
    for key, value in wanted.items():
        if key not in receipt or receipt[key] != value or type(receipt[key]) is not type(value):
            raise ValueError(f'RX-prefix witness mismatch: {key}')
    if complete and any(type(byte) is not int for byte in receipt['payload']):
        raise ValueError('RX payload contains non-byte values')
    registration = receipt.get('registration', {})
    if (not isinstance(registration, dict)
            or type(registration.get('slot')) is not int or registration['slot'] < 0
            or type(registration.get('generation')) is not int or registration['generation'] <= 0):
        raise ValueError('missing stream incarnation')


def check_termination(receipt):
    if (not isinstance(receipt, dict)
            or type(receipt.get('exit_code')) is not int
            or receipt['exit_code'] != -signal.SIGKILL or receipt.get('reaped') is not True
            or receipt.get('closure') not in ('EOF', 'ConnectionReset')
            or receipt.get('unexpected_peer_bytes') != ''
            or type(receipt.get('prefix_observed_ns')) is not int
            or type(receipt.get('signal_sent_ns')) is not int
            or receipt['prefix_observed_ns'] <= 0
            or receipt['signal_sent_ns'] < receipt['prefix_observed_ns']):
        raise ValueError('missing ordered SIGKILL/reap/peer-close witness')


def build_worker(source, session):
    command = ['cargo', 'build', '-p', 'xadbtest-transport-worker', '--bin', 'comm_fault',
               '--locked', '--offline', '--message-format=json']
    environment = os.environ.copy()
    environment['CARGO_TARGET_DIR'] = str(session / 'target')
    result = subprocess.run(command, cwd=source, env=environment, capture_output=True, timeout=180)
    (session / 'build.jsonl').write_bytes(result.stdout)
    (session / 'build.stderr').write_bytes(result.stderr)
    if result.returncode:
        raise RuntimeError('RX-prefix fixture build failed')
    messages = [json.loads(line) for line in result.stdout.splitlines() if line.startswith(b'{')]
    executables = [m['executable'] for m in messages if m.get('reason') == 'compiler-artifact'
                   and m.get('target', {}).get('name') == 'comm_fault' and m.get('executable')]
    if (len(executables) != 1
            or not any(m.get('reason') == 'build-finished' and m.get('success') is True for m in messages)):
        raise ValueError('missing unique RX-prefix executable')
    binary = Path(executables[0])
    return binary, {'command': command, 'binary_sha256': digest(binary),
                    'stdout_sha256': digest(session / 'build.jsonl'),
                    'stderr_sha256': digest(session / 'build.stderr')}


def run_cell(binary, source_hash, binary_hash, cell, directory):
    directory.mkdir(parents=True)
    family, stage, repetition = cell.split('/')
    nonce = secrets.token_hex(16)
    command = [str(binary), family, str(PREFIXES[stage]), nonce]
    # Keep AF_UNIX's pathname below the OS limit even in a deep source export.
    with tempfile.TemporaryDirectory(prefix='xadb-comm-rx-') as scratch:
        unix_path = Path(scratch) / 'private' / 'socket'
        if family == 'unix':
            command.append(str(unix_path.parent))
        report = {'cell': cell, 'family': family, 'stage': stage, 'repetition': int(repetition),
                  'status': 'RUNNING', 'command': command, 'source_sha256': source_hash,
                  'binary_sha256': binary_hash, 'nonce': nonce,
                  'peer_prefix_hex': FRAME[:PREFIXES[stage]].hex()}
        process = None
        peer = None
        try:
            with ((directory / 'stderr.log').open('xb') as stderr,
                  (directory / 'stdout.jsonl').open('xb') as raw_log):
                process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=stderr)
                expected = {'fixture': 'comm-rx-prefix-v1', 'family': family,
                            'prefix_bytes': PREFIXES[stage], 'nonce': nonce,
                            'pid': process.pid, 'source_sha256': source_hash,
                            'profile': 'debug', 'target': 'x86_64-unknown-linux-gnu'}
                ready = read_receipt(process, raw_log, 5)
                report['ready'] = ready
                check_identity(ready, expected, 'ready')
                if digest(Path(f'/proc/{process.pid}/exe')) != binary_hash:
                    raise ValueError('running process is not the built fixture binary')
                if family == 'unix':
                    if ready.get('unix') != str(unix_path) or ready.get('tcp') is not None:
                        raise ValueError('wrong owned Unix endpoint')
                    peer = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    peer.settimeout(5)
                    peer.connect(str(unix_path))
                else:
                    address = ready.get('tcp', '')
                    if not isinstance(address, str) or ':' not in address:
                        raise ValueError('malformed TCP fixture endpoint')
                    host, port = address.rsplit(':', 1)
                    host = host.strip('[]')
                    if host != ('127.0.0.1' if family == 'tcp4' else '::1'):
                        raise ValueError('non-loopback fixture endpoint')
                    port = int(port)
                    if not 1 <= port <= 65535:
                        raise ValueError('TCP fixture port outside 1..65535')
                    peer = socket.socket(socket.AF_INET if family == 'tcp4' else socket.AF_INET6,
                                         socket.SOCK_STREAM)
                    peer.settimeout(5)
                    peer.connect((host, port))
                peer.sendall(FRAME[:PREFIXES[stage]])
                report['sent_prefix_bytes'] = PREFIXES[stage]
                prefix = read_receipt(process, raw_log, 5)
                report['prefix'] = prefix
                check_prefix(prefix, expected, stage)
                report['prefix_observed_ns'] = time.monotonic_ns()
                if process.poll() is not None:
                    raise ValueError('worker exited before controller SIGKILL')
                if digest(Path(f'/proc/{process.pid}/exe')) != binary_hash:
                    raise ValueError('fixture binary identity changed before SIGKILL')
                report['signal_sent_ns'] = time.monotonic_ns()
                process.send_signal(signal.SIGKILL)
                report['exit_code'] = process.wait(timeout=5)
                report['reaped'] = True
                # The reader may have buffered a duplicate receipt. Reject it as
                # well as any bytes remaining in the pipe after the reap.
                remainder = bytes(getattr(process, '_comm_line_buffer', b''))
                tail = process.stdout.read(MAX_RECEIPT_BYTES + 1)
                raw_log.write(tail)
                raw_log.flush()
                remainder += tail
                report['trailing_stdout_hex'] = remainder.hex()
                if remainder:
                    raise ValueError('unexpected duplicate or trailing worker receipt')
                try:
                    observed = peer.recv(1)
                    report['closure'] = 'EOF' if observed == b'' else 'UnexpectedBytes'
                    report['unexpected_peer_bytes'] = observed.hex()
                except ConnectionResetError:
                    report['closure'] = 'ConnectionReset'
                    report['unexpected_peer_bytes'] = ''
                check_termination(report)
                report['status'] = 'PASS'
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            report['status'] = 'FAIL'
            report['error'] = str(error)
        finally:
            if process is not None:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)
                    report['cleanup_kill'] = True
                if process.stdout is not None:
                    process.stdout.close()
            if peer is not None:
                peer.close()
            report['stderr_sha256'] = digest(directory / 'stderr.log')
            report['stdout_sha256'] = digest(directory / 'stdout.jsonl')
            write_json(directory / 'validation.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cell', action='append', choices=CELLS,
                        help='select one new cell; repeat for a changed or failed subset')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    check_report_paths([args.report])
    selected = list(CELLS) if args.cell is None else args.cell
    if len(set(selected)) != len(selected):
        parser.error('duplicate selected cell')
    parent = ROOT / 'artifacts/comm'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='rx-kill-', dir=parent))
    source = session / 'source'
    report = {'schema_version': 1, 'date': datetime.now(timezone.utc).isoformat(),
              'scope': 'C-V09 selected native RX-prefix/process-kill cells only',
              'status': 'RUNNING', 'c1_status': 'INCOMPLETE', 'origin': identity(ROOT),
              'execution_root': str(source), 'selected_cells': selected, 'cells': []}
    try:
        report['source_sha256'] = capture_source(ROOT, source)
        report['generator_sha256'] = {name: digest(source / name) for name in (
            'scripts/check_comm_rx_kill.py', 'scripts/check_comm.py',
            'scripts/comm_campaign.py',
            'crates/testing/transport-worker/src/bin/comm_fault.rs')}
        write_json(session / 'manifest.json', report)
        binary, report['build'] = build_worker(source, session)
        print(f'COMM RX-prefix source: {source}', flush=True)
        for cell in selected:
            result = run_cell(binary, report['source_sha256'], report['build']['binary_sha256'],
                              cell, session / 'cells' / cell)
            report['cells'].append(result)
            print(f'{cell}: {result["status"]}', flush=True)
            if result['status'] != 'PASS':
                raise ValueError(f'RX-prefix cell failed: {cell}')
        if (source_fingerprint(source) != report['source_sha256']
                or digest(binary) != report['build']['binary_sha256']):
            raise ValueError('retained source or binary changed during execution')
        report['status'] = 'PASS'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
    write_json(session / 'validation.json', report)
    if args.report:
        write_json(args.report, report)
    print(f'COMM RX-prefix {report["status"]}; C1 INCOMPLETE: {session / "validation.json"}', flush=True)
    return int(report['status'] != 'PASS')


if __name__ == '__main__':
    raise SystemExit(main())
