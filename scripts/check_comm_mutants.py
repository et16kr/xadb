#!/usr/bin/env python3
"""Kill the frozen C-V16 production mutants using independent original tests."""
import argparse
from datetime import datetime, timezone
import difflib
import json
from pathlib import Path
import re
import subprocess
import tempfile

from check_dev0 import check_report_paths, digest, source_fingerprint
from comm_campaign import build_test, capture_source, identity, write_json

ROOT = Path(__file__).resolve().parent.parent
CONNECTION = 'crates/transport/src/connection.rs'
CODEC = 'crates/foundation/src/codec.rs'
RESOURCE = 'crates/transport/src/resources.rs'

# Exact production replacements and killing witnesses are fixed before execution.
# Edits are applied only to the retained export and always restored afterwards.
MUTANTS = [
    {'id': 'checksum', 'suite': 'framing', 'test': 'every_small_bit_corruption_is_terminal_without_resynchronization',
     'edits': [(CODEC, 'if Sha256::digest(&frame[..end])[..] != frame[end..] {',
                'if false && Sha256::digest(&frame[..end])[..] != frame[end..] {')]},
    {'id': 'endian', 'suite': 'framing', 'test': 'independent_vectors_boundaries_and_header_rejection_before_allocation',
     'edits': [(CODEC, 'u32::from_le_bytes([header[6], header[7], header[8], header[9]])',
                'u32::from_be_bytes([header[6], header[7], header[8], header[9]])')]},
    {'id': 'quota-plus-one', 'suite': 'acceptance_boundaries', 'test': 'global_payload_exact_limit_and_one_byte_over_are_distinguished',
     'edits': [(RESOURCE, 'payload: Quota::new(PROCESS_PAYLOAD_BYTES),', 'payload: Quota::new(PROCESS_PAYLOAD_BYTES + 1),')]},
    {'id': 'offset-reset', 'suite': 'lifecycle', 'test': 'short_io_interrupt_block_and_zero_keep_offsets_and_exact_failure_bytes',
     'edits': [(CONNECTION, 'tx.offset += n;', 'tx.offset = n;')]},
    {'id': 'sliding-deadline', 'suite': 'acceptance_boundaries', 'test': 'trickle_progress_never_extends_the_first_byte_deadline',
     'edits': [(CONNECTION, 'if n > 0 && self.rx.filled == 0 {', 'if n > 0 {')]},
    {'id': 'stale-event', 'suite': 'lifecycle', 'test': 'drop_partial_tx_cancel_after_last_write_and_stale_event_are_terminal',
     'edits': [(CONNECTION, 'if registration != self.registration || now < self.retry_at {', 'if now < self.retry_at {')]},
    {'id': 'cancel-next-frame', 'suite': 'acceptance_boundaries', 'test': 'cancellation_after_partial_tx_never_sends_the_next_accepted_frame',
     'edits': [(CONNECTION, '        if self.shared.cancellation.check().is_err() {\n            return Err(ErrorKind::Cancelled.into());\n        }\n', ''),
               (CONNECTION, '        self.shared.check_cancellation(&a)?;\n', '')]},
    {'id': 'connect-interest', 'suite': 'lifecycle', 'test': 'connecting_has_write_interest_without_tx_and_deadlines_are_absolute',
     'edits': [(CONNECTION, 'let writable = self.phase == ConnectionPhase::Connecting || self.has_tx();', 'let writable = self.has_tx();')]},
    {'id': 'hup-is-eof', 'suite': 'acceptance_boundaries', 'test': 'paused_receive_retains_hangup_until_leases_return_and_buffered_frame_is_delivered',
     'edits': [(CONNECTION, '        self.hangup_hint |= ready.hangup;\n',
                '        self.hangup_hint |= ready.hangup;\n        if ready.hangup { self.peer_write_closed = true; self.begin_drain(); }\n')]},
    {'id': 'stale-clock', 'suite': 'lifecycle', 'test': 'last_io_crossing_deadline_never_reports_success_but_keeps_full_progress',
     'edits': [(CONNECTION, '            self.check()?;\n            let tx = self.tx.as_mut().ok_or(ErrorKind::AdapterViolation)?;',
                '            let write_started = self.check()?;\n            let tx = self.tx.as_mut().ok_or(ErrorKind::AdapterViolation)?;'),
               (CONNECTION, '            let now = self.check()?;\n            match result {\n                Ok(0) => return Err(ErrorKind::WriteZero.into()),',
                '            let now = write_started;\n            match result {\n                Ok(0) => return Err(ErrorKind::WriteZero.into()),')]},
    {'id': 'drop-pending', 'suite': 'lifecycle', 'test': 'drop_partial_tx_cancel_after_last_write_and_stale_event_are_terminal',
     'edits': [(CONNECTION, 'impl Drop for Connection {\n    fn drop(&mut self) {\n        self.close(ErrorKind::Closed.into());\n    }\n}',
                'impl Drop for Connection {\n    fn drop(&mut self) {}\n}')]},
]
for _mutant in MUTANTS:
    _mutant['expected_failure_marker'] = f'COMM_MUTANT {_mutant["id"]}:'


def apply_edits(source, mutant):
    originals = {}
    try:
        for name, old, new in mutant['edits']:
            path = source / name
            original = path.read_text()
            originals.setdefault(path, original)
            if original.count(old) != 1:
                raise ValueError(f'{mutant["id"]}: mutation site absent or ambiguous: {name}')
            path.write_text(original.replace(old, new, 1))
    except BaseException:
        for path, original in originals.items():
            path.write_text(original)
        raise
    return originals


def check_execution(output, code, name, mutant=False, marker=None):
    """A named semantic test failure counts; crash/timeout/build failure do not."""
    state = 'FAILED' if mutant else 'ok'
    if not re.search(r'^running 1 test$', output, re.M) or not re.search(rf'^test {re.escape(name)} \.\.\. {state}$', output, re.M):
        raise ValueError('missing exact executed test or wrong terminal state')
    if mutant:
        if code != 101 or 'test result: FAILED. 0 passed; 1 failed; 0 ignored;' not in output or 'panicked at ' not in output or not marker or marker not in output:
            raise ValueError('mutant failure lacks semantic assertion witness')
    elif code != 0 or 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in output:
        raise ValueError('baseline test did not pass')


def execute(source, item, directory, mutant=False):
    directory.mkdir()
    binary, build = build_test(source, item['suite'], directory)
    command = [str(binary), '--exact', item['test'], '--nocapture']
    result = subprocess.run(command, cwd=source, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
    (directory / 'execution.log').write_bytes(result.stdout)
    check_execution(result.stdout.decode(), result.returncode, item['test'], mutant,
                    marker=item['expected_failure_marker'])
    return build | {'command': command, 'exit_code': result.returncode,
                    'log_sha256': digest(directory / 'execution.log')}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    check_report_paths([args.report])
    parent = ROOT / 'artifacts/comm'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='mutants-', dir=parent))
    source = session / 'source'
    report = {'schema_version': 1, 'scope': 'C-V16 eleven production mutants; local only',
              'status': 'RUNNING', 'date': datetime.now(timezone.utc).isoformat(),
              'execution_root': str(source), 'origin': identity(ROOT), 'mutants': []}
    exit_code = 0
    try:
        report['source_sha256'] = capture_source(ROOT, source)
        report['generator_sha256'] = digest(Path(__file__))
        report['inventory'] = MUTANTS
        write_json(session / 'manifest.json', report)
        for item in MUTANTS:
            print(f'C-V16 {item["id"]}: baseline', flush=True)
            directory = session / item['id']
            directory.mkdir()
            entry = {'id': item['id'], 'status': 'RUNNING'}
            report['mutants'].append(entry)
            entry['baseline'] = execute(source, item, directory / 'baseline')
            originals = apply_edits(source, item)
            try:
                patches = []
                for path, original in originals.items():
                    name = str(path.relative_to(source))
                    patches.extend(difflib.unified_diff(original.splitlines(True), path.read_text().splitlines(True), fromfile='a/' + name, tofile='b/' + name))
                (directory / 'production.patch').write_text(''.join(patches))
                entry['patch_sha256'] = digest(directory / 'production.patch')
                entry['mutant_source_sha256'] = source_fingerprint(source)
                print(f'C-V16 {item["id"]}: mutant', flush=True)
                entry['mutant'] = execute(source, item, directory / 'mutant', mutant=True)
                entry['status'] = 'KILLED'
            finally:
                for path, original in originals.items():
                    path.write_text(original)
            if source_fingerprint(source) != report['source_sha256']:
                raise ValueError('baseline source not restored')
            write_json(session / 'progress.json', report, replace=True)
        if len(report['mutants']) != 11 or any(m['status'] != 'KILLED' for m in report['mutants']):
            raise ValueError('required mutant inventory incomplete')
        report['status'] = 'PASS'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        exit_code = 1
    write_json(session / 'validation.json', report)
    if args.report:
        write_json(args.report, report)
    print(f'C-V16 {report["status"]}: {session / "validation.json"}', flush=True)
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
