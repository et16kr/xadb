"""Exercise the pre-push gate against disposable repositories and a local remote."""
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parent
FORMATTED_RUST = 'pub fn answer() -> u32 {\n    42\n}\n'
UNFORMATTED_RUST = 'pub fn answer()->u32{42}\n'


class PushGateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='xadb-push-test-')
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.repo, self.remote = root / 'repo', root / 'remote.git'
        self.run_command(['git', 'init', '--bare', str(self.remote)], root)
        self.run_command(['git', 'init', '-b', 'main', str(self.repo)], root)
        self.git('config', 'user.name', 'Push gate fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'commit.gpgsign', 'false')
        (self.repo / 'scripts/git-hooks').mkdir(parents=True)
        shutil.copy2(SCRIPTS / 'check_push.py', self.repo / 'scripts/check_push.py')
        hook = self.repo / 'scripts/git-hooks/pre-push'
        shutil.copy2(SCRIPTS / 'git-hooks/pre-push', hook)
        hook.chmod(0o755)
        (self.repo / 'devdoc').mkdir()
        # This fixture tests transport gating, not the document checker's semantics.
        self.verifier = self.repo / 'devdoc/verify_documents.py'
        self.verifier.write_text(
            "from pathlib import Path\n"
            "Path('devdoc/generated.json').write_text('temporary')\n"
            "raise SystemExit(int(Path('verdict').read_text()))\n")
        self.verdict = self.repo / 'verdict'
        self.verdict.write_text('0')
        # Use the real pinned formatter and Clippy on a dependency-free fixture.
        shutil.copy2(SCRIPTS.parent / 'rust-toolchain.toml', self.repo / 'rust-toolchain.toml')
        (self.repo / 'Cargo.toml').write_text(
            '[package]\nname = "xadb-push-fixture"\nversion = "0.1.0"\nedition = "2024"\n'
            '[features]\nlint-fixture = []\n')
        (self.repo / 'src').mkdir()
        self.rust_source = self.repo / 'src/lib.rs'
        self.rust_source.write_text(FORMATTED_RUST)
        self.run_command(['cargo', 'generate-lockfile', '--offline'], self.repo)
        self.commit('valid baseline')
        self.git('config', 'core.hooksPath', 'scripts/git-hooks')
        self.git('remote', 'add', 'origin', str(self.remote))

    def run_command(self, args, cwd, expected=0, **kwargs):
        result = subprocess.run(args, cwd=cwd, text=True, capture_output=True, **kwargs)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return result

    def git(self, *args, expected=0):
        return self.run_command(['git', *args], self.repo, expected)

    def commit(self, message):
        self.git('add', '.')
        self.git('commit', '-m', message)
        return self.git('rev-parse', 'HEAD').stdout.strip()

    def remote_tip(self, ref='refs/heads/main'):
        return self.run_command(['git', '--git-dir', str(self.remote),
                                 'rev-parse', '--verify', ref], self.repo).stdout.strip()

    def test_committed_tip_controls_push_and_checkout_is_unchanged(self):
        self.git('push', 'origin', 'main')
        good = self.remote_tip()
        self.verdict.write_text('1')
        bad = self.commit('rejected snapshot')
        self.verdict.write_text('0')  # An uncommitted fix must not authorize bad.
        self.git('push', 'origin', 'main', expected=1)
        self.assertEqual(self.remote_tip(), good)
        self.assertNotEqual(bad, good)
        fixed = self.commit('fixed snapshot')
        self.verdict.write_text('1')  # Uncommitted work must not reject fixed.
        self.git('push', 'origin', 'main')
        self.assertEqual(self.remote_tip(), fixed)
        self.assertEqual(self.verdict.read_text(), '1')
        self.assertFalse((self.repo / 'devdoc/generated.json').exists())

    def test_annotated_tag_deduplication_and_deletion(self):
        self.git('tag', '-a', 'good', '-m', 'fixture tag')
        result = self.git('push', 'origin', 'main', 'refs/tags/good')
        self.assertEqual((result.stdout + result.stderr).count('checking '), 1)
        self.verdict.write_text('1')
        self.commit('bad local checkout')
        result = self.git('push', 'origin', ':refs/tags/good')
        self.assertNotIn('checking ', result.stdout + result.stderr)

    def test_one_bad_tip_blocks_all_ref_updates(self):
        self.git('push', 'origin', 'main')
        old = self.remote_tip()
        self.verdict.write_text('1')
        self.commit('bad topic')
        self.git('branch', 'bad')
        self.verdict.write_text('0')
        self.commit('good main')
        self.git('push', 'origin', 'main', 'bad', expected=1)
        self.assertEqual(self.remote_tip(), old)
        refs = self.run_command(['git', '--git-dir', str(self.remote),
                                 'for-each-ref', '--format=%(refname)'], self.repo)
        self.assertNotIn('refs/heads/bad', refs.stdout)

    def test_missing_verifier_and_malformed_input_block(self):
        self.verifier.unlink()
        self.commit('missing verifier')
        self.git('push', 'origin', 'main', expected=1)
        self.run_command(['python3', 'scripts/check_push.py'], self.repo, 1,
                         input='not a valid update\n')

    def test_rust_format_checks_committed_tip_without_editing_checkout(self):
        self.git('push', 'origin', 'main')
        good = self.remote_tip()
        self.rust_source.write_text(UNFORMATTED_RUST)
        self.commit('unformatted Rust')
        self.rust_source.write_text(FORMATTED_RUST)
        result = self.git('push', 'origin', 'main', expected=1)
        self.assertIn('Rust format verification failed; push blocked', result.stderr)
        self.assertEqual(self.remote_tip(), good)
        self.assertEqual(self.rust_source.read_text(), FORMATTED_RUST)

        fixed = self.commit('formatted Rust')
        self.rust_source.write_text(UNFORMATTED_RUST)
        self.git('add', 'src/lib.rs')
        (self.repo / 'untracked.txt').write_text('in-progress work')
        status = self.git('status', '--porcelain').stdout
        staged = self.git('diff', '--cached').stdout
        self.git('push', 'origin', 'main')
        self.assertEqual(self.remote_tip(), fixed)
        self.assertEqual(self.rust_source.read_text(), UNFORMATTED_RUST)
        self.assertEqual(self.git('status', '--porcelain').stdout, status)
        self.assertEqual(self.git('diff', '--cached').stdout, staged)
        self.assertFalse((self.repo / 'devdoc/generated.json').exists())

    def test_unformatted_tag_blocks_all_ref_updates(self):
        self.git('push', 'origin', 'main')
        old = self.remote_tip()
        self.rust_source.write_text(UNFORMATTED_RUST)
        self.commit('unformatted tagged snapshot')
        self.git('tag', '-a', 'bad-format', '-m', 'unformatted fixture')
        self.rust_source.write_text(FORMATTED_RUST)
        self.commit('formatted main')
        self.git('push', 'origin', 'main', 'refs/tags/bad-format', expected=1)
        self.assertEqual(self.remote_tip(), old)
        refs = self.run_command(['git', '--git-dir', str(self.remote),
                                 'for-each-ref', '--format=%(refname)'], self.repo)
        self.assertNotIn('refs/tags/bad-format', refs.stdout)

    def test_missing_cargo_manifest_blocks_push(self):
        self.git('push', 'origin', 'main')
        old = self.remote_tip()
        (self.repo / 'Cargo.toml').unlink()
        self.commit('missing Cargo manifest')
        result = self.git('push', 'origin', 'main', expected=1)
        self.assertIn('Rust format verification failed; push blocked', result.stderr)
        self.assertEqual(self.remote_tip(), old)

    def test_clippy_checks_test_targets_and_committed_tip(self):
        self.git('push', 'origin', 'main')
        old = self.remote_tip()
        bad_source = '''#[cfg(test)]
mod tests {
    enum Point { AfterSnapshot, AfterStage, AfterApply, AfterSync }
    #[test]
    fn fixture() {
        let _ = [Point::AfterSnapshot, Point::AfterStage,
                 Point::AfterApply, Point::AfterSync];
    }
}
'''
        self.rust_source.write_text(bad_source)
        self.run_command(['cargo', 'fmt', '--all'], self.repo)
        self.run_command(['cargo', 'fmt', '--all', '--check'], self.repo)
        self.commit('formatted test-only lint error')
        self.rust_source.write_text(FORMATTED_RUST)
        result = self.git('push', 'origin', 'main', expected=1)
        self.assertIn('Clippy verification failed; push blocked', result.stderr)
        self.assertIn('enum_variant_names', result.stdout + result.stderr)
        self.assertEqual(self.remote_tip(), old)

        fixed = self.commit('fixed lint')
        self.rust_source.write_text(bad_source)
        self.git('add', 'src/lib.rs')
        status = self.git('status', '--porcelain').stdout
        staged = self.git('diff', '--cached').stdout
        external_target = self.repo / 'unexpected-build-output'
        environment = dict(os.environ, CARGO_TARGET_DIR=str(external_target))
        self.run_command(['git', 'push', 'origin', 'main'], self.repo, env=environment)
        self.assertEqual(self.remote_tip(), fixed)
        self.assertEqual(self.git('status', '--porcelain').stdout, status)
        self.assertEqual(self.git('diff', '--cached').stdout, staged)
        self.assertFalse(external_target.exists())
        self.assertFalse((self.repo / 'devdoc/generated.json').exists())

    def test_clippy_checks_nondefault_feature_type_complexity(self):
        self.git('push', 'origin', 'main')
        old = self.remote_tip()
        self.rust_source.write_text('''#[cfg(all(test, feature = "lint-fixture"))]
mod tests {
    use std::sync::{Arc, Mutex};
    fn capture(_: Arc<Mutex<Option<(u64, u64, u64, u64)>>>) {}
    #[test]
    fn fixture() { capture(Arc::new(Mutex::new(None))); }
}
''')
        self.run_command(['cargo', 'fmt', '--all'], self.repo)
        self.run_command(['cargo', 'fmt', '--all', '--check'], self.repo)
        self.commit('nondefault feature lint error')
        result = self.git('push', 'origin', 'main', expected=1)
        self.assertIn('Clippy verification failed; push blocked', result.stderr)
        self.assertIn('type_complexity', result.stdout + result.stderr)
        self.assertEqual(self.remote_tip(), old)

    def test_clippy_requires_committed_lockfile(self):
        self.git('push', 'origin', 'main')
        old = self.remote_tip()
        (self.repo / 'Cargo.lock').unlink()
        self.commit('missing lockfile')
        result = self.git('push', 'origin', 'main', expected=1)
        self.assertIn('Clippy verification failed; push blocked', result.stderr)
        self.assertIn('--locked', result.stdout + result.stderr)
        self.assertEqual(self.remote_tip(), old)


if __name__ == '__main__':
    unittest.main()
