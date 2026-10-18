"""Negative controls for the Markdown-only documentation integrity boundary."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from document_tools import DocumentIndex, markdown_references, validate_document_layout, validate_markdown


class DocumentationChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'devdoc'
        self.root.mkdir()
        self.entries = {}
        for role in ('policies', 'plans', 'contracts', 'implementation', 'reference', 'examples', 'evidence', 'history'):
            self.add(role + '/README.md', '# Index\n', role)
        self.save_catalog()
        (self.root / 'document-migration.json').write_text(json.dumps({'preserved_evidence': []}))

    def add(self, path, text, role='reference', doc_id=None):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
        self.entries[doc_id or path] = {'path': path, 'role': role}
        return target

    def save_catalog(self):
        (self.root / 'documents.json').write_text(json.dumps({'documents': self.entries}))

    def test_relocated_document_id_and_markdown_route(self):
        target = self.add('contracts/kernel.md', '# Kernel\n\n<a id="commit"></a>\n', 'contracts', 'kernel.md')
        self.add('reference/guide.md', '[contract](../contracts/kernel.md#commit)\n')
        self.save_catalog()
        self.assertEqual(DocumentIndex(self.root) / 'kernel.md', target)
        self.assertEqual(validate_markdown(self.root)[0], [])
        self.assertEqual(validate_document_layout(self.root), [])

    def test_code_examples_are_not_navigation(self):
        links, ids, _ = markdown_references('```md\n[missing](absent.md)\n<a id="x"></a>\n```\n`[other](none.md)`\n')
        self.assertEqual((links, ids), ([], []))

    def test_missing_target_and_anchor_are_rejected(self):
        self.add('reference/guide.md', '[absent](missing.md)\n[anchor](README.md#missing)\n')
        errors, _ = validate_markdown(self.root)
        self.assertTrue(any('missing link' in e for e in errors))
        self.assertTrue(any('missing Markdown anchor' in e for e in errors))

    def test_table_link_and_explicit_duplicate_are_checked(self):
        self.add('reference/guide.md', '| link |\n| --- |\n| [bad](absent.md) |\n\n<a id="x"></a>\n<a id="x"></a>\n')
        errors, _ = validate_markdown(self.root)
        self.assertTrue(any('missing link' in e for e in errors))
        self.assertTrue(any('duplicate explicit anchors' in e for e in errors))

    def test_html_and_retired_recipe_cannot_return(self):
        self.add('reference/old.html', '<p>old</p>')
        self.add('policies/bad.md', '```bash\npython3 devdoc/build_report.py\n```\n', 'policies')
        self.save_catalog()
        errors = validate_document_layout(self.root)
        self.assertTrue(any('HTML document/template is forbidden' in e for e in errors))
        self.assertTrue(any('retired HTML command' in e for e in errors))

    def test_catalog_missing_duplicate_and_outside_paths(self):
        self.entries.update({'absent': {'path': 'missing.md', 'role': 'reference'},
                             'alias': self.entries['reference/README.md'],
                             'outside': {'path': '../README.md', 'role': 'reference'}})
        self.save_catalog()
        errors = validate_document_layout(self.root)
        self.assertTrue(any('duplicate paths' in e for e in errors))
        self.assertEqual(sum('missing or outside document' in e for e in errors), 2)

    def test_unregistered_document_is_rejected(self):
        self.add('reference/new.md', '# New\n')
        self.assertTrue(any('unregistered' in e for e in validate_document_layout(self.root)))

    def test_frozen_evidence_changes_are_rejected(self):
        p = self.add('evidence/original.json', '{"status":"NOT_RUN"}\n', 'evidence')
        self.save_catalog()
        saved = {'path': 'evidence/original.json', 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
        (self.root / 'document-migration.json').write_text(json.dumps({'preserved_evidence': [saved]}))
        self.assertEqual(validate_document_layout(self.root), [])
        p.write_text('{"status":"PASS"}\n')
        self.assertTrue(any('migrated original changed' in e for e in validate_document_layout(self.root)))

    def test_json_contract_id_must_resolve(self):
        self.add('contracts/plan.json', '{"contract_ref":"missing.md"}\n', 'contracts')
        self.save_catalog()
        self.assertTrue(any('unresolved JSON document ID' in e for e in validate_document_layout(self.root)))

    def test_current_evidence_can_advance_without_rewriting_history(self):
        old = self.add('evidence/old.json', '{"status":"PASS"}\n', 'evidence', 'current.json')
        saved = {'path': 'evidence/old.json', 'sha256': hashlib.sha256(old.read_bytes()).hexdigest()}
        (self.root / 'document-migration.json').write_text(json.dumps({'preserved_evidence': [saved]}))
        self.entries['evidence/old.json'] = self.entries.pop('current.json')
        new = self.add('evidence/new.json', '{"status":"PASS","run":2}\n', 'evidence', 'current.json')
        self.save_catalog()
        self.assertEqual(validate_document_layout(self.root), [])
        self.assertEqual(DocumentIndex(self.root) / 'current.json', new)

    def test_converted_source_excerpt_cannot_be_silently_rewritten(self):
        p = self.add('history/report.md', '# Report\n\n```text\n[original](old.md)\n```\n', 'history')
        self.save_catalog()
        converted = {'path': 'history/report.md', 'preserved_pre_sha256': [
            hashlib.sha256(b'[original](old.md)').hexdigest()]}
        (self.root / 'document-migration.json').write_text(json.dumps({
            'preserved_evidence': [], 'converted_reports': [converted]}))
        self.assertEqual(validate_document_layout(self.root), [])
        p.write_text(p.read_text().replace('old.md', 'new.md'))
        self.assertTrue(any('converted source excerpt changed' in e for e in validate_document_layout(self.root)))


if __name__ == '__main__':
    unittest.main()
