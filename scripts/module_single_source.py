#!/usr/bin/env python3
"""Build one single-source export for the module-independence MI03 check.

`--side a` is C+A: B's product files, fixtures and registration are removed
with every test module that links B. `--side b` is C+B: the same for A.
Each removal is a physical delete of a declared path or a declared one-line
or one-block registration patch (contract `#row-single-source-s8`); nothing
else in the committed tree changes. The export's Rust sources under crates/
are then checked for any remaining path to the removed module or its
fixtures (a hit fails); other text files that still name it (never compiled
into the crates: compile-fail fixtures, fixture authors' pins, documents) are
listed in the inventory. The inventory is then written.

Only builds the export directory and its inventory. Compiling and running
selectors are separate steps over the export.
"""
import argparse
import hashlib
import json
import re
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path, PurePosixPath

ENGINE = 'crates/engine/src/storage'
MEMORY = f'{ENGINE}/wal_io/plan4_recovery/tests/catalog_memory'
REGISTRY = f'{ENGINE}/wal_io/plan4_recovery/tests/catalog_memory.rs'

MANIFEST = 'devdoc/policies/module-gate.json'


def source_spec(data, side):
    """Derive physical removals from one reviewed ownership manifest.

    catalog_memory is the existing bounded registration group. Product roots
    come from the removed owner's files; JOIN units are removed from both
    exports. C-only exceptions carry their exact registration beside the edge.
    No candidate manifest is loaded implicitly by the trusted gate/runner.
    """
    exports = [e for e in data['exports'] if e['side'] == side]
    if len(exports) != 1:
        raise ValueError(f'export {side}: missing or duplicate side')
    export = exports[0]
    modules = {m['id']: m for m in data['modules']}
    removed = modules[export['removes']]
    module = removed['logical'][0].split('::')[-1]
    if not re.fullmatch(r'[a-z_][a-z0-9_]*', module):
        raise ValueError('invalid product registration')
    roots, optional_roots, tests, tests_c = [], set(), set(), []
    for owner in modules.values():
        if owner['id'] != removed['id'] and owner['kind'] != 'join':
            continue
        for pattern in owner['files']:
            if pattern.startswith(MEMORY + '/'):
                relative = pattern[len(MEMORY) + 1:]
                if relative.endswith('.rs') and '/' not in relative:
                    tests.add(relative[:-3])
                elif (relative.endswith('/**') and '/' not in relative[:-3]
                      and f'{MEMORY}/{relative[:-3]}.rs' in owner['files']):
                    pass  # The registered module's optional child directory.
                else:
                    raise ValueError(f'unsupported test ownership pattern: {pattern}')
            elif owner['id'] == removed['id']:
                roots.append(pattern.removesuffix('/**'))
                if pattern.endswith('/**'):
                    optional_roots.add(pattern[:-3])
            else:
                raise ValueError(f'unsupported JOIN ownership pattern: {pattern}')
    for ex in data['exceptions']:
        if ex['target'] != removed['id']:
            continue
        entry = ex.get('export_registration')
        if ex['scope'] != 'test' or not entry:
            raise ValueError(f'{ex["id"]}: missing test export registration')
        tests_c.append((entry['file'], entry['path'], entry['module'], ex['source']))
    if not roots:
        raise ValueError('missing product roots')
    if any(not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', name) for name in tests):
        raise ValueError('invalid test registration name')
    registrations = set()
    for file, attr, name, source in tests_c:
        if (not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', name)
                or str(PurePosixPath(file).parent / attr) != source
                or (file, name) in registrations):
            raise ValueError('invalid or duplicate exception registration')
        registrations.add((file, name))
    paths = roots + [x for entry in tests_c for x in (entry[0], entry[1], entry[3])]
    if (len(roots) != len(set(roots)) or any(
            not p or p.startswith('/') or '..' in p.split('/')
            or any(c in p for c in '*?\\') for p in paths)):
        raise ValueError('invalid export removal/registration path')
    tokens = [rf'row::{re.escape(module)}\b'] + [re.escape(t) for t in removed.get('fixture_tokens', [])]
    return dict(module=module, removed=roots, optional_roots=optional_roots,
                tests=sorted(tests), tests_c=tests_c,
                forbidden=re.compile('|'.join(tokens)))


def policy_data(repo, revision):
    """Read the explicitly selected policy as Git data, including for the CLI."""
    return json.loads(subprocess.check_output(
        ['git', 'show', f'{revision}:{MANIFEST}'], cwd=repo, text=True))


def sha(data):
    return hashlib.sha256(data).hexdigest()


def remove_registration(text, name):
    """Delete `#[path = "catalog_memory/<name>.rs"]` (+ attributes) `mod <name>;`."""
    pattern = re.compile(
        r'#\[path = "catalog_memory/' + re.escape(name) + r'\.rs"\]\n'
        r'(?:#\[[^\]]*\]\n)*mod ' + re.escape(name) + r';\n')
    found = pattern.findall(text)
    if len(found) != 1:
        raise SystemExit(f'registration of {name}: {len(found)} matches')
    return pattern.sub('', text), found[0]


def export(commit, out, side, repo=None, *, policy):
    spec = source_spec(policy, side)
    # `repo` lets the module gate export a fetched pull request head from a
    # trusted checkout; the default (the current checkout) is unchanged.
    repo = Path(repo) if repo else Path(subprocess.check_output(
        ['git', 'rev-parse', '--show-toplevel'], text=True).strip())
    full = subprocess.check_output(['git', 'rev-parse', '--verify', commit + '^{commit}'],
                                   cwd=repo, text=True).strip()
    if out.exists():
        raise SystemExit(f'{out} exists')
    out.mkdir(parents=True)
    with tempfile.TemporaryDirectory() as directory:
        archive = Path(directory) / 'tree.tar'
        subprocess.run(['git', 'archive', '--format=tar', '-o', str(archive), full],
                       cwd=repo, check=True)
        with tarfile.open(archive) as tar:
            tar.extractall(out, filter='data')
    patches, removed = [], []
    for rel in spec['removed']:
        path = out / rel
        if path.is_dir():
            removed += sorted(str(p.relative_to(out)) for p in path.rglob('*') if p.is_file())
            shutil.rmtree(path)
        elif path.is_file():
            removed.append(rel)
            path.unlink()
        elif rel not in spec['optional_roots']:
            raise SystemExit(f'declared path missing: {rel}')
    row = out / f'{ENGINE}/row.rs'
    text = row.read_text()
    line = f'pub(crate) mod {spec["module"]};\n'
    if text.count(line) != 1:
        raise SystemExit(f'row.rs registration of {spec["module"]}')
    row.write_text(text.replace(line, ''))
    patches.append(dict(file=f'{ENGINE}/row.rs', removed=line))
    registry = out / REGISTRY
    text = registry.read_text()
    for name in spec['tests']:
        text, block = remove_registration(text, name)
        patches.append(dict(file=REGISTRY, removed=block))
        for rel in (f'{MEMORY}/{name}.rs', f'{MEMORY}/{name}'):
            path = out / rel
            if path.is_dir():
                removed += sorted(str(p.relative_to(out)) for p in path.rglob('*')
                                  if p.is_file())
                shutil.rmtree(path)
            elif path.is_file():
                removed.append(rel)
                path.unlink()
    registry.write_text(text)
    for rel, attr, name, census in spec.get('tests_c', []):
        path = out / rel
        text = path.read_text()
        pattern = re.compile(r'(?:///[^\n]*\n)*#\[cfg\(test\)\]\n#\[path = "'
                             + re.escape(attr) + r'"\]\nmod ' + re.escape(name) + r';\n')
        found = pattern.findall(text)
        if len(found) != 1:
            raise SystemExit(f'cfg(test) registration in {rel}: {len(found)} matches')
        path.write_text(pattern.sub('', text))
        patches.append(dict(file=rel, removed=found[0]))
        (out / census).unlink()
        removed.append(census)
    # No remaining Rust path to the removed module or its fixtures (fails).
    left = []
    for path in sorted((out / 'crates').rglob('*.rs')):
        for number, content in enumerate(path.read_text().splitlines(), 1):
            if spec['forbidden'].search(content) and not content.lstrip().startswith('//'):
                left.append(f'{path.relative_to(out)}:{number}: {content.strip()}')
    # Every other text file that still names it: never compiled into the
    # export's crates (compile-fail fixtures, fixture authors' pins, docs).
    # Reported, not failed.
    other = []
    for path in sorted(p for p in out.rglob('*') if p.is_file()):
        rel = path.relative_to(out)
        if rel.parts[0] == 'crates' and path.suffix == '.rs':
            continue
        try:
            text = path.read_text()
        except (UnicodeDecodeError, ValueError):
            continue
        hits = sum(1 for line in text.splitlines() if spec['forbidden'].search(line))
        if hits:
            other.append(f'{rel}: {hits}')
    files = {str(p.relative_to(out)): sha(p.read_bytes())
             for p in sorted(out.rglob('*')) if p.is_file()}
    manifest = ''.join(f'{digest}  {name}\n' for name, digest in files.items())
    inventory = dict(schema='XADB_S8_SINGLE_SOURCE_V1', side=side, commit=full,
                     removed=removed, patches=patches, remaining_references=left,
                     other_text_references=other,
                     files=len(files), source_manifest_sha256=sha(manifest.encode()))
    return inventory, manifest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--side', choices=('a', 'b'), required=True)
    parser.add_argument('--policy', required=True, help='trusted policy commit/ref')
    parser.add_argument('--commit', default='HEAD')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--record', type=Path, required=True,
                        help='directory for inventory.json and source-files.sha256')
    args = parser.parse_args()
    inventory, manifest = export(args.commit, args.out, args.side,
                                 policy=policy_data(Path.cwd(), args.policy))
    args.record.mkdir(parents=True, exist_ok=True)
    (args.record / 'source-files.sha256').write_text(manifest)
    (args.record / 'inventory.json').write_text(json.dumps(inventory, indent=1) + '\n')
    print(json.dumps({k: (v if k != 'other_text_references' else len(v))
                      for k, v in inventory.items() if k != 'removed'}, indent=1))
    if inventory['remaining_references']:
        raise SystemExit('remaining references to the removed module')


if __name__ == '__main__':
    main()
