#!/usr/bin/env python3
"""Build one single-source export for the module-independence MI03 check.

`--side a` is C+A: B's product files, fixtures and registration are removed
with every test module that links B. `--side b` is C+B: the same for A.
Each removal is a physical delete of a declared path or a declared one-line
or one-block registration patch (contract `#row-single-source-s8`); nothing
else in the committed tree changes. The export is then checked for any
remaining path to the removed module, and its inventory is written.

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
from pathlib import Path

ENGINE = 'crates/engine/src/storage'
MEMORY = f'{ENGINE}/wal_io/plan4_recovery/tests/catalog_memory'
REGISTRY = f'{ENGINE}/wal_io/plan4_recovery/tests/catalog_memory.rs'

# Test modules of catalog_memory that link A, B or both (registration name).
LINKS_A = ['row_write', 'row_v09', 'row_forward', 'row_cancel', 'row_live', 'row_commit',
           'row_rollback', 'row_recovery', 'row_read_live', 'row_join']
LINKS_B = ['row_read_live', 'row_recovery', 'row_join']

SIDES = {
    # C+A: B removed.
    'a': dict(module='read', removed=[f'{ENGINE}/row/read.rs', f'{ENGINE}/row/read',
                                      'tests/fixtures/module-row-read'],
              tests=LINKS_B, forbidden=re.compile(r'row::read\b|row::read::')),
    # C+B: A removed.
    'b': dict(module='insert', removed=[f'{ENGINE}/row/insert.rs', f'{ENGINE}/row/insert',
                                       'tests/fixtures/module-row-insert'],
              tests=LINKS_A, forbidden=re.compile(r'row::insert\b|row::insert::')),
}



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


def export(commit, out, side):
    spec = SIDES[side]
    repo = Path(subprocess.check_output(['git', 'rev-parse', '--show-toplevel'],
                                        text=True).strip())
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
        else:
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
    # No remaining Rust path to the removed module.
    left = []
    for path in sorted((out / 'crates').rglob('*.rs')):
        for number, content in enumerate(path.read_text().splitlines(), 1):
            if spec['forbidden'].search(content) and not content.lstrip().startswith('//'):
                left.append(f'{path.relative_to(out)}:{number}: {content.strip()}')
    files = {str(p.relative_to(out)): sha(p.read_bytes())
             for p in sorted(out.rglob('*')) if p.is_file()}
    manifest = ''.join(f'{digest}  {name}\n' for name, digest in files.items())
    inventory = dict(schema='XADB_S8_SINGLE_SOURCE_V1', side=side, commit=full,
                     removed=removed, patches=patches, remaining_references=left,
                     files=len(files), source_manifest_sha256=sha(manifest.encode()))
    return inventory, manifest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--side', choices=sorted(SIDES), required=True)
    parser.add_argument('--commit', default='HEAD')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--record', type=Path, required=True,
                        help='directory for inventory.json and source-files.sha256')
    args = parser.parse_args()
    inventory, manifest = export(args.commit, args.out, args.side)
    args.record.mkdir(parents=True, exist_ok=True)
    (args.record / 'source-files.sha256').write_text(manifest)
    (args.record / 'inventory.json').write_text(json.dumps(inventory, indent=1) + '\n')
    print(json.dumps({k: v for k, v in inventory.items() if k != 'removed'}, indent=1))
    if inventory['remaining_references']:
        raise SystemExit('remaining references to the removed module')


if __name__ == '__main__':
    main()
