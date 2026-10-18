"""Markdown navigation, document-ID resolution and frozen evidence checks."""
from collections import Counter
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit

from markdown_it import MarkdownIt


class DocumentIndex:
    """Resolve registered document IDs independently of their physical directory."""

    def __init__(self, root):
        self.root = Path(root)
        self.entries = json.loads((self.root / 'documents.json').read_text())['documents']

    def __truediv__(self, reference):
        entry = self.entries.get(str(reference))
        if entry is not None:
            return self.root / entry['path']
        # An unregistered ID resolves to a missing file, never a guessed basename.
        return self.root / str(reference)


class InlineReferences(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.ids, self.links = [], []
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if 'id' in attrs:
            self.ids.append(attrs['id'])
        if tag == 'a' and 'href' in attrs:
            self.links.append(attrs['href'])
        if tag == 'img' and 'src' in attrs:
            self.links.append(attrs['src'])


def markdown_references(source):
    """Parse Markdown, ignoring code examples; include explicit and GFM heading IDs."""
    tokens = MarkdownIt('commonmark', {'html': True}).enable('table').parse(source)
    links, explicit_ids, heading_ids = [], [], []
    slug_counts = Counter()
    for i, token in enumerate(tokens):
        if token.type == 'heading_open':
            title = tokens[i + 1].content
            slug = re.sub(r'[^\w\- ]', '', title.lower()).replace(' ', '-')
            number = slug_counts[slug]
            heading_ids.append(slug + (f'-{number}' if number else ''))
            slug_counts[slug] += 1
        for child in [token, *(token.children or [])]:
            if child.type == 'link_open':
                links.append(child.attrGet('href'))
            elif child.type == 'image':
                links.append(child.attrGet('src'))
            elif child.type in {'html_block', 'html_inline'}:
                parsed = InlineReferences(child.content)
                explicit_ids.extend(parsed.ids)
                links.extend(parsed.links)
    return links, explicit_ids, set(explicit_ids + heading_ids)


def normalize_contract_links(source):
    """Contract table comparisons use stable file IDs, not their relative routes."""
    def normalize(match):
        ref = match.group(1)
        url = urlsplit(ref)
        if url.scheme or url.netloc or not url.path:
            return match.group(0)
        return '](' + Path(url.path).name + ('#' + url.fragment if url.fragment else '') + ')'
    return re.sub(r'\]\(([^)]+)\)', normalize, source)


def validate_markdown(root):
    root = Path(root)
    errors, documents, cache = [], [], {}
    paths = sorted(root.rglob('*.md'))
    paths += [p for p in (root.parent / 'README.md', root.parent / 'AGENTS.md') if p.is_file()]
    for path in paths:
        cache[path.resolve()] = markdown_references(path.read_text())
    for path in paths:
        links, explicit_ids, _ = cache[path.resolve()]
        duplicates = [key for key, count in Counter(explicit_ids).items() if count > 1]
        if duplicates:
            errors.append(f'{path}: duplicate explicit anchors {duplicates}')
        for link in links:
            url = urlsplit(link)
            if url.scheme or url.netloc:
                continue
            target = (path.parent / unquote(url.path)).resolve() if url.path else path.resolve()
            if target.suffix == '.html':
                errors.append(f'{path}: local HTML link is retired: {link}')
            if not target.exists():
                errors.append(f'{path}: missing link {link}')
            elif url.fragment and target.suffix == '.md':
                if target not in cache:
                    cache[target] = markdown_references(target.read_text())
                if unquote(url.fragment) not in cache[target][2]:
                    errors.append(f'{path}: missing Markdown anchor {link}')
        documents.append({'file': str(path.relative_to(root.parent)), 'links': len(links),
                          'duplicate_ids': len(duplicates),
                          'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    return errors, documents


def validate_document_layout(root):
    root = Path(root)
    errors = []
    entries = DocumentIndex(root).entries
    paths = [entry['path'] for entry in entries.values()]
    duplicates = [p for p, count in Counter(paths).items() if count > 1]
    if duplicates:
        errors.append('catalog: duplicate paths ' + str(duplicates))
    roles = {'policies', 'plans', 'contracts', 'implementation', 'reference', 'examples',
             'evidence', 'history', 'entrypoint', 'tooling'}
    for doc_id, entry in entries.items():
        target = root / entry['path']
        if not target.resolve().is_relative_to(root.resolve()) or not target.is_file():
            errors.append('catalog: missing or outside document ' + doc_id)
        if entry['role'] not in roles:
            errors.append('catalog: invalid role ' + doc_id)
        expected_role = entry['path'].split('/')[0] if '/' in entry['path'] else 'entrypoint'
        if entry['role'] != expected_role:
            errors.append('catalog: role/path mismatch ' + doc_id)
    infrastructure = {'documents.json', 'document-migration.json', 'document-validation.json',
                      'verify_documents.py', 'document_tools.py', 'test_document_tools.py', 'requirements.txt'}
    for path in root.rglob('*'):
        if not path.is_file() or '__pycache__' in path.parts:
            continue
        relative = path.relative_to(root).as_posix()
        if path.suffix == '.html':
            errors.append('format: HTML document/template is forbidden: ' + relative)
        elif path.suffix in {'.md', '.json', '.py', '.txt'} and relative not in infrastructure and relative not in paths:
            errors.append('catalog: unregistered document ' + relative)
    for group in roles - {'entrypoint', 'tooling'}:
        index = root / group / 'README.md'
        if not index.is_file():
            errors.append('catalog: missing role index ' + group)
    # Historical receipts may name retired commands; active recipes must not invoke them.
    active_groups = ('policies', 'plans', 'contracts', 'implementation', 'reference', 'examples')
    active_markdown = [root / 'README.md', root.parent / 'README.md', root.parent / 'AGENTS.md']
    active_markdown += [p for group in active_groups for p in (root / group).glob('*.md')]
    for path in active_markdown:
        if path.is_file() and re.search(r'python3\s+\S*build_(?:report|review_docs|structure_report)\.py', path.read_text()):
            errors.append('format: retired HTML command in ' + str(path))
    # JSON contract links are document IDs; artifact filenames in examples are not.
    def check_refs(value, key, owner):
        if isinstance(value, dict):
            for field, child in value.items():
                check_refs(child, field, owner)
        elif isinstance(value, list):
            for child in value:
                check_refs(child, key, owner)
        elif isinstance(value, str) and (key.endswith(('_ref', '_refs', '_reference'))
                                         or key in {'document', 'references', 'file', 'evidence', 'specification'}):
            if re.fullmatch(r'[\w-]+\.(?:md|json|py)(?:#[\w-]+)?', value):
                name = value.split('#')[0]
                if name not in entries:
                    errors.append(f'catalog: unresolved JSON document ID {owner}:{key}={value}')
    for group in active_groups:
        for path in (root / group).glob('*.json'):
            check_refs(json.loads(path.read_text()), '', path.relative_to(root))
    migration = json.loads((root / 'document-migration.json').read_text())
    for saved in migration['preserved_evidence']:
        path = root / saved['path']
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != saved['sha256']:
            errors.append('evidence: migrated original changed: ' + saved['path'])
    for converted in migration.get('converted_reports', []):
        path = root / converted['path']
        if not path.is_file():
            errors.append('evidence: missing converted report: ' + converted['path'])
            continue
        blocks = [hashlib.sha256(t.content.rstrip('\n').encode()).hexdigest()
                  for t in MarkdownIt().parse(path.read_text()) if t.type == 'fence']
        if blocks != converted['preserved_pre_sha256']:
            errors.append('evidence: converted source excerpt changed: ' + converted['path'])
    return errors
