#!/usr/bin/env python3
"""Project a frozen Profile3 CREATE manifest's checkpoints into checkpoints.tsv.

A pure projection of manifest.json; it regenerates nothing and imports no
generator, serializer, or product code (stdlib only). Before writing, the same
projection must reproduce all three frozen version2 checkpoints.tsv files
byte-for-byte from their pinned manifests, so the column grammar is the v2
grammar. Output files are create-new.
"""
from pathlib import Path
import argparse
import hashlib
import json

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
# (v2 directory, manifest sha256, checkpoints.tsv sha256)
V2 = (
    ('native', '87aa46dd81711cc5080097c674e6fb419e371680ea3e19faa5bafbe3e4847b23',
     '0d1abf8d70b01cbaeea218f3cdcb49194e1e17f0b65f362f7f83877b8d98551e'),
    ('memory-gap', 'edfe57082b56540ea5e71f0d5109ca98dc131369d1906388f5dd171f69d3a92b',
     'e0cd3791ddb9a7064387a68809cb3cbe8b26e266e2c824271677c2b16c168268'),
    ('memory-retire', '049f4416ceaab3ccb18581f49808aa0d5d526664ad5ef90ca386a62ec8e0199c',
     '44abad550f706b5c2fffa39cd0ddc7f6d5c352f67ee18e253d7fd31876324164'),
)
TARGETS = ('native3', 'memory-gap3', 'memory-retire3')
HEADER = ('scenario', 'lsn', 'q', 'pages', 'jobs', 'aging', 'system', 'active_transaction', 'masks', 'tables')


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def need(ok, what):
    if not ok:
        raise ValueError(what)


def project(manifest):
    out = '\t'.join(HEADER) + '\n'
    for c in manifest['checkpoints']:
        pages = ','.join(f'{n}:{h}' for n, h in sorted(c['pages'].items(), key=lambda item: int(item[0])))
        need(len(c['pages']) == 64, ('64 logical pages', c['scenario'], c['lsn']))
        row = (c['scenario'], c['lsn'], c['scalar']['Q'], pages, c['jobs'], c['aging'],
               c['system'] or '-', c['active_transaction'] or '-',
               ','.join(map(str, c['root_masks'])), ','.join(map(str, c['live_tables'])) or '-')
        out += '\t'.join(map(str, row)) + '\n'
    return out.encode()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--target', required=True, choices=TARGETS)
    parser.add_argument('--manifest-sha256', required=True)
    args = parser.parse_args()
    for name, manifest_sha, tsv_sha in V2:
        raw = (ROOT / name / 'manifest.json').read_bytes()
        need(sha(raw) == manifest_sha, ('v2 manifest pin', name))
        need(sha(project(json.loads(raw))) == tsv_sha == sha((ROOT / name / 'checkpoints.tsv').read_bytes()),
             ('v2 projection reproduced', name))
    target = ROOT / args.target
    raw = (target / 'manifest.json').read_bytes()
    need(sha(raw) == args.manifest_sha256, 'explicit target manifest')
    manifest = json.loads(raw)
    need(manifest['authority'] == 'NONE', 'manifest authority')
    tsv = project(manifest)
    receipt = dict(status='FROZEN_MANIFEST_PROJECTION_NO_REGENERATION', source_manifest_sha256=args.manifest_sha256,
                   projector_sha256=sha(Path(__file__).read_bytes()),
                   v2_reproduced={name: tsv_sha for name, _, tsv_sha in V2},
                   file='checkpoints.tsv', sha256=sha(tsv), bytes=len(tsv), rows=len(manifest['checkpoints']),
                   product_executions=0, generator_additional_runs=0, auditor_additional_runs=0)
    with (target / 'checkpoints.tsv').open('xb') as f:
        f.write(tsv)
    with (target / 'checkpoint-projection-receipt.json').open('x') as f:
        json.dump(receipt, f, indent=2)
        f.write('\n')
    print(json.dumps(receipt, sort_keys=True))


if __name__ == '__main__':
    main()
