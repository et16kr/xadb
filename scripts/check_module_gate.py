#!/usr/bin/env python3
"""Module boundary and evidence gate (XADB-MODULE-GATE-003).

The checker reads Git objects only. It never builds, imports or executes the
analysed tree, so a trusted revision of this file and of the manifest can
judge a pull request head as data (`repository-build-plan.md#module-gate`).

Subcommands:
  check  base/head judgement: registration, dependency, export, review and
         evidence rules (`--mode push` leaves review/evidence as PENDING)
  plan   the CI selection (head selectors and export sides) for a head
  files  the path -> blob map that a review record lists for base..head
  final  CI control only: `check` plus acceptance of the unprivileged build
         run of the same head (`module-build.yml`)
"""
import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath

MANIFEST = 'devdoc/policies/module-gate.json'
MANIFEST_SCHEMA = 'XADB_MODULE_GATE_MANIFEST_V1'
RECORD_SCHEMA = 'XADB_MODULE_GATE_RECORD_V1'
RECEIPT_SCHEMA = 'XADB_MODULE_BUILD_RECEIPT_V1'
REPORT_SCHEMA = 'XADB_MODULE_GATE_REPORT_V1'
CLASSES = {'D1': 1, 'D2': 2}
# Anything a build or a test can read: every file under crates/ and tests/
# (fixtures included), the workspace manifests, the toolchain and Cargo config.
RUST_INPUTS = re.compile(r'(?:.*\.rs|(?:.*/)?Cargo\.toml|Cargo\.lock|rust-toolchain(?:\.toml)?'
                         r'|\.cargo/.*|crates/.*|tests/.*)\Z')
# A libtest filter the runner passes after `--`: a module path, never a flag.
SELECTOR = re.compile(r'[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z0-9_]+)*(?:::)?\Z')


class GateError(Exception):
    """The checker could not judge. Always blocks; never a PASS or N/A."""


# --------------------------------------------------------------------------
# Git objects (data only)
# --------------------------------------------------------------------------

def git(repo, *args, data=None):
    result = subprocess.run(['git', *args], cwd=repo, input=data, capture_output=True)
    if result.returncode:
        raise GateError(f'git {" ".join(args)}: '
                        f'{result.stderr.decode(errors="replace").strip()}')
    return result.stdout


def resolve(repo, rev):
    return git(repo, 'rev-parse', '--verify', '--end-of-options',
               rev + '^{commit}').decode().strip()


class Tree:
    """Read-only view of one commit's tree."""

    def __init__(self, repo, commit):
        self.repo, self.commit = repo, resolve(repo, commit)
        self.blobs, self.cache, self.links = {}, {}, {}
        for entry in git(repo, 'ls-tree', '-r', '-z', '--full-tree', self.commit).split(b'\0'):
            if entry:
                meta, path = entry.split(b'\t', 1)
                mode, kind, oid = meta.decode().split()
                if kind == 'blob' and mode != '120000':
                    self.blobs[path.decode()] = oid
                else:
                    # A symlink or a submodule is never source: rustc would
                    # follow it, the checker would parse the link text.
                    self.links[path.decode()] = mode

    def files(self):
        return self.blobs.keys()

    def exists(self, path):
        return path in self.blobs

    def blob(self, path):
        return self.blobs.get(path)

    def read(self, path):
        if path not in self.cache:
            if path not in self.blobs:
                raise GateError(f'{self.commit}: missing {path}')
            self.preload([path])
        return self.cache[path]

    def text(self, path):
        try:
            return self.read(path).decode()
        except UnicodeDecodeError as error:
            raise GateError(f'{self.commit}:{path}: not UTF-8') from error

    def preload(self, paths):
        """Batch-read blobs through one `git cat-file --batch`."""
        wanted = sorted({p for p in paths if p in self.blobs and p not in self.cache})
        if not wanted:
            return
        data = git(self.repo, 'cat-file', '--batch',
                   data=''.join(self.blobs[p] + '\n' for p in wanted).encode())
        offset = 0
        for path in wanted:
            end = data.index(b'\n', offset)
            _oid, kind, size = data[offset:end].decode().split()
            if kind != 'blob':
                raise GateError(f'{path}: {kind}')
            size = int(size)
            self.cache[path] = data[end + 1:end + 1 + size]
            offset = end + 1 + size + 1


def changes(repo, old, new):
    """Name-status of old..new; a rename reports both paths."""
    raw = git(repo, 'diff', '--name-status', '-z', '-M', '--no-ext-diff', '--no-textconv',
              old, new)
    fields = [f.decode() for f in raw.split(b'\0') if f]
    out, index = [], 0
    while index < len(fields):
        status = fields[index]
        if status[0] in 'RC':
            out.append((status[0], fields[index + 1], fields[index + 2]))
            index += 3
        else:
            path = fields[index + 1]
            out.append((status[0], None if status[0] == 'A' else path,
                        None if status[0] == 'D' else path))
            index += 2
    return out


# --------------------------------------------------------------------------
# Rust source: comments, literals and the logical module tree
# --------------------------------------------------------------------------

RAW_STRING = re.compile(r'b?r(#*)"')
CHAR = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]{1,6}\}|.)|[^\\'\n])'")


def strip_rust(text):
    """(comments removed, comments and literal contents removed).

    Offsets and newlines are preserved so lines and positions agree.
    """
    keep, code = list(text), list(text)
    i, n = 0, len(text)

    def blank(start, end, both):
        for k in range(start, min(end, n)):
            if text[k] != '\n':
                code[k] = ' '
                if both:
                    keep[k] = ' '

    while i < n:
        c = text[i]
        word = i > 0 and (text[i - 1].isalnum() or text[i - 1] == '_')
        raw = None if word else RAW_STRING.match(text, i)
        if text.startswith('//', i):
            end = text.find('\n', i)
            end = n if end < 0 else end
            blank(i, end, True)
            i = end
        elif text.startswith('/*', i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith('/*', j):
                    depth, j = depth + 1, j + 2
                elif text.startswith('*/', j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            blank(i, j, True)
            i = j
        elif raw:
            close = '"' + raw.group(1)
            end = text.find(close, raw.end())
            end = n if end < 0 else end
            blank(raw.end(), end, False)
            i = end + len(close)
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == '\\' else 1
            blank(i + 1, j, False)
            i = j + 1
        elif c == "'":
            m = CHAR.match(text, i)
            if m:
                blank(i + 1, m.end() - 1, False)
                i = m.end()
            else:
                i += 1  # a lifetime or a loop label
        else:
            i += 1
    return ''.join(keep), ''.join(code)


def match_brace(code, open_at):
    depth = 0
    for k in range(open_at, len(code)):
        if code[k] == '{':
            depth += 1
        elif code[k] == '}':
            depth -= 1
            if depth == 0:
                return k
    raise GateError('unbalanced braces')


MOD = re.compile(r'((?:#\s*\[[^\]]*\]\s*)*)(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+(?:r#)?(\w+)\s*([;{])')
PATH_ATTR = re.compile(r'#\s*\[\s*path\s*=\s*"([^"]*)"\s*\]')
# `#[cfg(...)]` predicates are evaluated three-valued (True, False, None =
# not known) against two fact sets. PRODUCTION_CFG knows only `test` (the
# product build may enable features or target other platforms), so an item
# is test-only when its predicate is False there. TEST_CFG is the CI build
# runner of `evidence.test_command` (ubuntu-24.04, x86_64). Cargo features
# and profile settings are left unknown: a change can turn on default
# features or debug assertions. A `#[test]` is planned exactly when its
# predicates are True there, is absent when one is False, and runs through
# its module selector when one is not known.
CFG_ATTR = re.compile(r'#\s*\[\s*cfg\s*\(([^\]]*)\)\s*\]')
CFG_TOKEN = re.compile(r'\s*(?:([A-Za-z_]\w*)|"([^"\\]*)"|([(),=]))')
PRODUCTION_CFG = {'test': False}
TEST_CFG = {'test': True, 'unix': True, 'windows': False, 'target_os': 'linux',
            'target_family': 'unix', 'target_arch': 'x86_64', 'target_pointer_width': '64',
            'target_endian': 'little'}
TEST_MARK = re.compile(r'#\s*\[\s*test\s*\]')
# What may follow `#[cfg(test)]` for it to remove an item: further attributes,
# a visibility and qualifiers, then an item keyword. On a field, a match arm,
# a statement or an expression the attribute removes only that part, and the
# code after it stays production.
TEST_ITEM = re.compile(r'(?:\s*#\s*\[[^\]]*\])*\s*(?:pub\s*(?:\([^)]*\))?\s*)?'
                       r'(?:(?:unsafe|async|default|extern\s*(?:"[^"]*")?)\s+)*'
                       r'(?:fn|mod|struct|enum|union|impl|trait|type|const|static|use|'
                       r'macro_rules\s*!|extern\s+crate)\b')


def normalize(path):
    parts = []
    for part in PurePosixPath(path).parts:
        if part == '..':
            if not parts:
                raise GateError(f'path escapes the tree: {path}')
            parts.pop()
        elif part != '.':
            parts.append(part)
    return '/'.join(parts)


def cfg_value(text, facts):
    """A `cfg(...)` predicate's value under `facts`: True, False or None."""
    pos = 0

    def token():
        nonlocal pos
        m = CFG_TOKEN.match(text, pos)
        if not m:
            raise ValueError(text)
        pos = m.end()
        return m

    def peek(symbol):
        m = CFG_TOKEN.match(text, pos)
        return bool(m) and m.group(3) == symbol

    def predicate():
        nonlocal pos
        name = token().group(1)
        if name is None:
            raise ValueError(text)
        if peek('('):
            token()
            values = []
            while not peek(')'):
                values.append(predicate())
                if not peek(')') and token().group(3) != ',':
                    raise ValueError(text)
            token()
            if name == 'all':
                return False if False in values else (True if None not in values else None)
            if name == 'any':
                return True if True in values else (False if None not in values else None)
            if name == 'not' and len(values) == 1:
                return None if values[0] is None else not values[0]
            return None
        if peek('='):
            token()
            value = token().group(2)
            if value is None:
                raise ValueError(text)
            known = facts.get(name)
            if isinstance(known, str):
                return known == value
            return None
        known = facts.get(name)
        return known if isinstance(known, bool) else None

    try:
        result = predicate()
        return result if text[pos:].strip() == '' else None
    except ValueError:
        return None


def cfg_attrs(attrs, facts):
    """The combined value of the `#[cfg]` attributes in `attrs` (all must hold)."""
    values = [cfg_value(m.group(1), facts) for m in CFG_ATTR.finditer(attrs)]
    return False if False in values else (True if None not in values else None)


TEST_FN = re.compile(r'((?:#\s*\[[^\]]*\]\s*)*#\s*\[\s*test\s*\]\s*(?:#\s*\[[^\]]*\]\s*)*)'
                     r'(?:pub\s*(?:\([^)]*\))?\s*)?(?:async\s+)?fn\s+(?:r#)?(\w+)')


class RustFile:
    """One source file: declarations, inline modules and cfg(test) ranges."""

    def __init__(self, path, text):
        self.path = path
        self.keep, self.code = strip_rust(text)
        self.starts = [0] + [k + 1 for k, ch in enumerate(text) if ch == '\n']
        self.inline = []     # (open, close, name, test)
        self.declared = []   # (offset, inline chain, name, #[path], cfg(test), inline?)
        self.not_test = set()  # (inline chain, name) of a mod absent under TEST_CFG
        self.maybe_test = set()  # (inline chain, name) of a mod whose TEST_CFG is unknown
        found = []
        for m in MOD.finditer(self.code):
            attrs = self.keep[m.start(1):m.end(1)]
            path_attr = PATH_ATTR.search(attrs)
            test = cfg_attrs(attrs, PRODUCTION_CFG) is False
            found.append((m.start(), m.group(2), path_attr.group(1) if path_attr else None,
                          test, m.group(3) == '{', m.end() - 1, cfg_attrs(attrs, TEST_CFG)))
        for _offset, name, _attr, test, inline, brace, _present in found:
            if inline:
                self.inline.append((brace, match_brace(self.code, brace), name, test))
        for offset, name, attr, test, inline, _brace, present in found:
            self.declared.append((offset, self.enclosing(offset), name, attr, test, inline))
            if present is False:
                self.not_test.add((self.enclosing(offset), name))
            elif present is None:
                self.maybe_test.add((self.enclosing(offset), name))
        self.test_ranges = [(s, e) for s, e, _name, test in self.inline if test]
        for m in CFG_ATTR.finditer(self.code):
            if cfg_value(self.keep[m.start(1):m.end(1)], PRODUCTION_CFG) is not False:
                continue
            if not TEST_ITEM.match(self.code, m.end()):
                continue
            k = m.end()
            while k < len(self.code) and self.code[k] not in '{;':
                k += 1
            if k < len(self.code):
                end = match_brace(self.code, k) if self.code[k] == '{' else k
                self.test_ranges.append((m.end(), end))
        # `#[test]` functions by inline module: the selection runs the cases
        # a changed file declares (attributes are read from the kept text).
        # `known` is False when the case's own cfg or an enclosing inline
        # module's cfg is not known on the runner: it may be absent there.
        self.tests = []      # (offset, inline chain, fn name, known)
        literal = 0
        absent = {c + (n,) for c, n in self.not_test}
        maybe = {c + (n,) for c, n in self.maybe_test}
        for m in TEST_FN.finditer(self.code):
            literal += 1
            attrs = self.keep[m.start(1):m.end(1)]
            chain = self.enclosing(m.start())
            if any(chain[:k] in absent for k in range(1, len(chain) + 1)):
                continue
            present = cfg_attrs(attrs, TEST_CFG)
            if re.search(r'#\s*\[\s*ignore\b', attrs) or present is False:
                continue
            known = present is True and not any(chain[:k] in maybe
                                                for k in range(1, len(chain) + 1))
            self.tests.append((m.start(), chain, m.group(2), known))
        # `#[test]` the literal scan cannot name (inside `macro_rules!`
        # bodies): the planner runs the whole module instead.
        self.unnamed_tests = len(TEST_MARK.findall(self.code)) > literal

    def enclosing(self, offset):
        """Inline module names enclosing an offset, outermost first."""
        return tuple(name for start, end, name, _t in sorted(self.inline)
                     if start < offset <= end)

    def inline_test(self, offset):
        return any(s < offset <= e for s, e in self.test_ranges)

    def line(self, offset):
        lo, hi = 0, len(self.starts)
        while lo + 1 < hi:
            mid = (lo + hi) // 2
            if self.starts[mid] <= offset:
                lo = mid
            else:
                hi = mid
        return lo + 1


class Crate:
    """Logical module tree of one crate from its root file, through #[path]."""

    def __init__(self, tree, root):
        self.tree, self.root = tree, root
        self.source_dir = str(PurePosixPath(root).parent)
        tree.preload([p for p in tree.files() if p.endswith('.rs')])
        self.logical = {}      # file -> {logical tuple: test_only}
        self.children = {}     # logical tuple -> {child names}
        self.declared_by = {}  # file -> {declaring files}
        self.files, self.missing = {}, []
        self.no_test = set()   # logical modules absent from the test binary
        self.maybe_test = set()  # logical modules that may be absent from it (cfg unknown)
        self.visit(root, ('crate',), False, True)
        self.modules = set(self.children) | {
            logical + chain for path, logicals in self.logical.items() for logical in logicals
            for chain in [()] + [c + (n,) for _o, c, n, _a, _t, i in self.rust(path).declared
                                 if i]}
        for parent, names in self.children.items():
            self.modules.update(parent + (n,) for n in names)

    def rust(self, path):
        if path not in self.files:
            self.files[path] = RustFile(path, self.tree.text(path))
        return self.files[path]

    def visit(self, path, logical, test, owns_dir, no_test=False, maybe=False):
        # rustc: the root, a mod.rs and a #[path]-loaded file own their
        # directory; an ordinary `name.rs` resolves children under `name/`.
        seen = self.logical.setdefault(path, {})
        if logical in seen and (not seen[logical] or test):
            return
        seen[logical] = test if logical not in seen else (seen[logical] and test)
        if no_test:
            self.no_test.add(logical)
        elif maybe:
            self.maybe_test.add(logical)
        source = self.rust(path)
        directory = str(PurePosixPath(path).parent)
        if owns_dir or PurePosixPath(path).name in ('mod.rs', 'lib.rs', 'main.rs'):
            stem = directory
        else:
            stem = f'{directory}/{PurePosixPath(path).stem}'
        for _offset, chain, name, attr, decl_test, inline in source.declared:
            parent = logical + chain
            self.children.setdefault(parent, set()).add(name)
            if inline:
                continue
            chain_test = any(t for _s, _e, n, t in source.inline if n in chain)
            base = '/'.join([stem, *chain]) if chain else (directory if attr else stem)
            if attr is not None:
                candidates = [normalize(f'{base}/{attr}')]
            else:
                candidates = [f'{base}/{name}.rs', f'{base}/{name}/mod.rs']
            found = [c for c in candidates if self.tree.exists(c)]
            if not found:
                self.missing.append(f'{path}: mod {name} -> {candidates[0]}')
                continue
            self.declared_by.setdefault(found[0], set()).add(path)
            absent = no_test or (chain, name) in source.not_test or any(
                (chain[:k], chain[k]) in source.not_test for k in range(len(chain)))
            unknown = maybe or (chain, name) in source.maybe_test or any(
                (chain[:k], chain[k]) in source.maybe_test for k in range(len(chain)))
            self.visit(found[0], parent + (name,), test or decl_test or chain_test,
                       attr is not None, absent, unknown)

    def test_only(self, path):
        return all(self.logical.get(path, {(): True}).values())

    def unreached(self):
        prefix = self.source_dir + '/'
        return sorted(p for p in self.tree.files()
                      if p.startswith(prefix) and p.endswith('.rs') and p not in self.logical)


USE = re.compile(r'(?<![\w$])use\s+')
PUB_USE = re.compile(r'(?<![\w$])pub\s*(?:\([^)]*\))?\s*use\s+')
INLINE = re.compile(r'(?<![\w$])(\$?crate|super|self)((?:\s*::\s*\w+)+)')
IDENT_PATH = re.compile(r'(?<![\w:$.])([a-z_][a-z0-9_]*)((?:\s*::\s*\w+)+)')
VISIBILITY = re.compile(r'pub\s*\(\s*(?:in\s+[^)]*|crate|super|self)\s*\)')


def use_paths(code, start, aliases=None):
    """Expand the `use` tree that starts at `start`; return ([segments], end).

    `aliases` (a list) receives (segments, local name) for every `as` rename.
    A glob leaf keeps its trailing `*` segment.
    """
    end = code.index(';', start)
    text = re.sub(r'\s+as\s+(\w+|_)', lambda m: '@' + m.group(1), code[start:end])
    text = re.sub(r'\s+', '', text)
    out = []

    def expand(prefix, body):
        depth, part, parts = 0, '', []
        for ch in body:
            depth += (ch == '{') - (ch == '}')
            if ch == ',' and depth == 0:
                parts.append(part)
                part = ''
            else:
                part += ch
        parts.append(part)
        for item in filter(None, parts):
            brace = item.find('{')
            if brace >= 0:
                head = [s for s in item[:brace].split('::') if s]
                expand(prefix + head, item[brace + 1:item.rindex('}')])
            else:
                item, _at, alias = item.partition('@')
                segs = [s for s in item.split('::') if s]
                full = prefix if segs == ['self'] else prefix + segs
                out.append(full)
                if alias and alias != '_' and aliases is not None:
                    aliases.append((full, alias))

    expand([], text)
    return out, end


def resolve_path(crate, here, segments, names=None):
    """Logical path of `segments` written in module `here`.

    `names` maps local names that a `use` brought into `here` (renames,
    module imports and glob-imported modules) to their logical paths, so a
    path through an imported ancestor is resolved like the full path.
    """
    segments = [s.lstrip('$') for s in segments if s and s != '*']
    if not segments:
        return None
    if segments[0] == 'crate':
        return ('crate',) + tuple(segments[1:])
    if segments[0] in ('super', 'self'):
        current, k = list(here), 0
        while k < len(segments) and segments[k] in ('super', 'self'):
            if segments[k] == 'super':
                if len(current) <= 1:
                    return None
                current.pop()
            k += 1
        return tuple(current) + tuple(segments[k:])
    # A name-led path is local only when it names a child module of the
    # current module; otherwise it is an extern crate, the prelude or an
    # alias whose own `use` declaration is checked.
    if segments[0] in crate.children.get(tuple(here), ()):
        return tuple(here) + tuple(segments)
    if names and segments[0] in names:
        return names[segments[0]] + tuple(segments[1:])
    return None


def imported_names(crate):
    """Per logical module: local name -> logical path of what `use` imports.

    Renames (`use crate::storage as st`), imports of a module, and glob
    imports of a module (its child modules and the names it imported) are
    followed; a path that starts with such a name resolves to the imported
    module. Without this an alias or glob of an ancestor would hide an edge.
    """
    direct, globs = {}, {}
    for path, logicals in crate.logical.items():
        source = crate.rust(path)
        code = VISIBILITY.sub(lambda m: ' ' * len(m.group(0)), source.code)
        for m in USE.finditer(code):
            aliases = []
            try:
                paths, _end = use_paths(code, m.end(), aliases)
            except ValueError:
                continue
            renamed = {tuple(full) for full, _a in aliases}
            for logical in logicals:
                here = logical + source.enclosing(m.start())
                table = direct.setdefault(here, {})
                for full, alias in aliases:
                    target = resolve_path(crate, here, full)
                    if target:
                        table[alias] = target
                for segments in paths:
                    if segments and segments[-1] == '*':
                        target = resolve_path(crate, here, segments[:-1])
                        if target:
                            globs.setdefault(here, []).append(target)
                    elif tuple(segments) not in renamed:
                        target = resolve_path(crate, here, segments)
                        if target and target in crate.modules:
                            table.setdefault(target[-1], target)
    names = {}
    for here in set(direct) | set(globs):
        table = {}
        for target in globs.get(here, []):
            for child in crate.children.get(target, ()):
                table[child] = target + (child,)
            table.update(direct.get(target, {}))
        table.update(direct.get(here, {}))
        names[here] = table
    return names


def references(crate, path, names=None):
    """[(line, logical target, test scope, raw)] for one file."""
    source = crate.rust(path)
    code = VISIBILITY.sub(lambda m: ' ' * len(m.group(0)), source.code)
    file_test = crate.test_only(path)
    names = names or {}
    found = set()

    def add(offset, segments):
        test = file_test or source.inline_test(offset)
        for logical in crate.logical.get(path, {}):
            here = logical + source.enclosing(offset)
            target = resolve_path(crate, here, segments, names.get(here))
            if target:
                found.add((source.line(offset), target, test, '::'.join(segments)))

    blanked = list(code)
    for m in USE.finditer(code):
        try:
            paths, end = use_paths(code, m.end())
        except ValueError:
            continue
        for segments in paths:
            add(m.start(), segments)
        for k in range(m.start(), end + 1):
            if blanked[k] != '\n':
                blanked[k] = ' '
    code = ''.join(blanked)
    for m in INLINE.finditer(code):
        add(m.start(), [m.group(1)] + [s.strip() for s in m.group(2).split('::') if s.strip()])
    for m in IDENT_PATH.finditer(code):
        if m.group(1) not in ('crate', 'super', 'self', 'std', 'core', 'alloc'):
            add(m.start(), [m.group(1)] + [s.strip() for s in m.group(2).split('::')
                                           if s.strip()])
    return sorted(found)


class Analysis:
    """References of every file of each crate at one head, independent of manifests."""

    def __init__(self, tree, roots):
        self.tree, self.crates, self.refs = tree, {}, {}
        for root in roots:
            if not tree.exists(root):
                continue
            crate = Crate(tree, root)
            self.crates[root] = crate
            names = imported_names(crate)
            for path in crate.logical:
                self.refs[(root, path)] = references(crate, path, names)


# --------------------------------------------------------------------------
# Manifest
# --------------------------------------------------------------------------

def glob_regex(pattern):
    out, k = '', 0
    while k < len(pattern):
        if pattern.startswith('**/', k):
            out, k = out + '(?:.*/)?', k + 3
        elif pattern.startswith('**', k):
            out, k = out + '.*', k + 2
        elif pattern[k] == '*':
            out, k = out + '[^/]*', k + 1
        elif pattern[k] == '?':
            out, k = out + '[^/]', k + 1
        else:
            out, k = out + re.escape(pattern[k]), k + 1
    return re.compile(out + r'\Z')


class Manifest:
    KINDS = ('feature', 'join', 'registration', 'common-api', 'common')

    def __init__(self, data, origin):
        try:
            self.parse(data, origin)
        except (KeyError, TypeError, AttributeError) as error:
            raise GateError(f'{origin}: malformed manifest ({error!r})') from error

    def parse(self, data, origin):
        self.data, self.origin = data, origin
        if data.get('schema') != MANIFEST_SCHEMA:
            raise GateError(f'{origin}: manifest schema')
        self.modules, self.order = {}, []
        for module in data['modules']:
            mid = module['id']
            if mid in self.modules or mid in ('*', '-') or module['kind'] not in self.KINDS:
                raise GateError(f'{origin}: module {mid} duplicate, reserved or bad kind')
            if ('allow' in module) == ('deny' in module):
                raise GateError(f'{origin}: {mid} needs exactly one of allow/deny')
            for selector in module.get('ci_selectors', []):
                if not SELECTOR.match(selector):
                    raise GateError(f'{origin}: {mid}.ci_selectors: {selector!r}')
            if not isinstance(module.get('deny_items', []), list):
                raise GateError(f'{origin}: {mid}.deny_items')
            if not all(isinstance(x, str) and x.startswith('crate::')
                       for x in module.get('deny_paths', [])):
                raise GateError(f'{origin}: {mid}.deny_paths must be logical paths')
            module = dict(module, _files=[glob_regex(p) for p in module['files']],
                          _logical=[tuple(p.split('::')) for p in module.get('logical', [])])
            self.modules[mid] = module
            self.order.append(mid)
        names = set(self.modules) | {'*'}
        for mid, module in self.modules.items():
            for key in ('allow', 'allow_test', 'deny', 'ci_also'):
                unknown = set(module.get(key, [])) - names
                if unknown:
                    raise GateError(f'{origin}: {mid}.{key} names unknown {sorted(unknown)}')
        self.unscoped = [glob_regex(p) for entry in data['unscoped'] for p in entry['files']]
        self.policy = [glob_regex(p) for p in data['policy_files']]
        self.review = {cls: [glob_regex(p) for p in pats]
                       for cls, pats in data['review_required'].items() if cls in CLASSES}
        self.records = glob_regex(data['records'])
        self.exceptions = data['exceptions']
        keys = set()
        for ex in self.exceptions:
            key = (ex['source'], ex['target'], ex['scope'])
            if (key in keys or ex['target'] not in self.modules
                    or ex['scope'] not in ('test', 'production')):
                raise GateError(f'{origin}: exception {ex.get("id")} invalid or duplicate')
            for field in ('id', 'contract', 'reason', 'owner', 'review_when', 'paths',
                          'baseline'):
                if not ex.get(field):
                    raise GateError(f'{origin}: exception {ex.get("id")} lacks {field}')
            if not isinstance(ex['paths'], list) or not all(
                    isinstance(p, str) and (p.startswith('crate::') or p.startswith('fixture::'))
                    for p in ex['paths']):
                raise GateError(f'{origin}: exception {ex["id"]} paths must be logical paths')
            keys.add(key)
        for export in data['exports']:
            if export['keeps'] not in self.modules or export['removes'] not in self.modules:
                raise GateError(f'{origin}: export {export.get("side")} names unknown modules')
            if not export['command'] or export['command'][0] != 'cargo':
                raise GateError(f'{origin}: export {export.get("side")} command must be cargo')
        for key in ('allowed_selectors', 'input_roots'):
            if not isinstance(data['attested'][key], list):
                raise GateError(f'{origin}: attested.{key}')
        for mid, patterns in data['attested'].get('required_for', {}).items():
            if mid not in self.modules or not isinstance(patterns, list):
                raise GateError(f'{origin}: attested.required_for.{mid}')
        for key in ('build_workflow', 'artifact', 'head_command', 'test_command', 'skip'):
            if key not in data['evidence']:
                raise GateError(f'{origin}: evidence.{key}')
        for item in data['evidence'].get('executor_only', []):
            if not (isinstance(item, dict) and isinstance(item.get('selector'), str)
                    and SELECTOR.match(item['selector']) and item.get('reason')):
                raise GateError(f'{origin}: evidence.executor_only {item!r}')

    @classmethod
    def at(cls, tree):
        """The manifest at a commit, or None when the gate is not adopted there."""
        if not tree.exists(MANIFEST):
            return None
        try:
            data = json.loads(tree.text(MANIFEST))
        except ValueError as error:
            raise GateError(f'{tree.commit}:{MANIFEST}: {error}') from error
        return cls(data, f'{tree.commit[:12]}:{MANIFEST}')

    def owner(self, path):
        for mid in self.order:
            if any(r.match(path) for r in self.modules[mid]['_files']):
                return mid
        if any(r.match(path) for r in self.unscoped):
            return '-'
        return None

    def target(self, logical):
        best, length = None, -1
        for mid in self.order:
            for prefix in self.modules[mid]['_logical']:
                if logical[:len(prefix)] == prefix and len(prefix) > length:
                    best, length = mid, len(prefix)
        return best

    def is_policy(self, path):
        return any(r.match(path) for r in self.policy)

    def review_class(self, path):
        """D2 for policy files, D1 for reviewed inputs, None otherwise."""
        if self.records.match(path):
            return None
        if self.is_policy(path):
            return 'D2'
        for cls in sorted(self.review, key=CLASSES.get, reverse=True):
            if any(r.match(path) for r in self.review[cls]):
                return cls
        return None

    def kind(self, mid):
        return self.modules[mid]['kind']

    def files_of(self, tree, mid):
        return {p for p in tree.files() if self.owner(p) == mid}


def allowed(manifest, source, target, test):
    if target == source:
        return True
    src = manifest.modules[source]
    if manifest.kind(target) in ('feature', 'join') and src['kind'] != 'join':
        return False  # no product feature may reach another, in any scope
    if 'allow' in src:
        names = set(src['allow']) | (set(src.get('allow_test', [])) if test else set())
        return '*' in names or target in names
    return target not in set(src['deny'])


# --------------------------------------------------------------------------
# Rules
# --------------------------------------------------------------------------

def exception_paths(ex):
    return [tuple(p.split('::')) for p in ex.get('paths', [])]


def dependency_findings(manifest, analysis):
    """Forbidden edges at a head under one manifest; exceptions are shrink-only."""
    errors, used, used_paths = [], set(), set()
    for entry in manifest.data['crates']:
        crate = analysis.crates.get(entry['root'])
        if crate is None:
            errors.append(f'crate root {entry["root"]} missing')
            continue
        errors += [f'module tree: {m}' for m in crate.missing]
        errors += [f'crate file outside the module tree: {p}' for p in crate.unreached()]
        prefix = crate.source_dir + '/'
        errors += [f'symlink or submodule (mode {mode}) under a crate source root: {p}'
                   for p, mode in sorted(analysis.tree.links.items()) if p.startswith(prefix)]
        tokens = [(mid, word) for mid, m in manifest.modules.items()
                  for word in m.get('fixture_tokens', [])]
        for path in sorted(crate.logical):
            source = manifest.owner(path)
            if source in (None, '-'):
                errors.append(f'crate file without a module: {path}')
                continue
            # The file's owner (by path) and the module its logical path
            # belongs to (by prefix) must agree, or a file could be declared
            # into another module's namespace with another module's rules.
            for logical in sorted(crate.logical[path]):
                if manifest.target(logical) != source:
                    errors.append(f'module mismatch: {path} is {source}\'s but is declared as '
                                  f'{"::".join(logical)} ({manifest.target(logical)})')
            refs = [(line, manifest.target(logical), test, raw, logical)
                    for line, logical, test, raw in analysis.refs[(entry['root'], path)]]
            rust = crate.rust(path)
            for mid, word in tokens:
                for m in re.finditer(re.escape(word), rust.keep):
                    refs.append((rust.line(m.start()), mid,
                                 crate.test_only(path) or rust.inline_test(m.start()), word,
                                 ('fixture', word)))
            for word in manifest.modules[source].get('deny_items', []):
                for m in re.finditer(r'(?<![\w$])' + re.escape(word) + r'(?!\w)', rust.code):
                    test = crate.test_only(path) or rust.inline_test(m.start())
                    if not test:
                        errors.append(f'forbidden item {word} in {source} (production) '
                                      f'{path}:{rust.line(m.start())}')
            denied = [tuple(x.split('::')) for x in manifest.modules[source].get('deny_paths', [])]
            for line, target, test, raw, logical in sorted(set(refs),
                                                           key=lambda r: (r[0], r[3])):
                for prefix in denied:
                    if not test and logical[:len(prefix)] == prefix:
                        errors.append(f'forbidden path {"::".join(prefix)} in {source} '
                                      f'(production) {path}:{line}: {raw}')
                if target is None or allowed(manifest, source, target, test):
                    continue
                scope = 'test' if test else 'production'
                hits = [ex for ex in manifest.exceptions if ex['source'] == path
                        and ex['target'] == target and ex['scope'] in (scope, 'production')
                        and any(logical[:len(p)] == p for p in exception_paths(ex))]
                if hits:
                    used.add(hits[0]['id'])
                    used_paths.update((hits[0]['id'], p) for p in exception_paths(hits[0])
                                      if logical[:len(p)] == p)
                    continue
                errors.append(f'forbidden dependency {source}->{target} ({scope}) '
                              f'{path}:{line}: {raw} -> {"::".join(logical)}')
    for ex in manifest.exceptions:
        if ex['id'] not in used:
            errors.append(f'stale exception {ex["id"]} {ex["source"]}->{ex["target"]}: '
                          'the edge no longer occurs; remove it (exceptions only shrink)')
            continue
        for path in exception_paths(ex):
            if (ex['id'], path) not in used_paths:
                errors.append(f'stale exception path {ex["id"]} {"::".join(path)}: no '
                              'reference uses it any more; remove it (exceptions only shrink)')
    return errors


def _narrowing(key, old, new):
    """Whether a change of a top-level manifest key only tightens the gate."""
    old, new = old or {}, new or {}
    if key == 'policy_files':
        return set(new) >= set(old)
    if key == 'review_required':
        return all(set(new.get(c, [])) >= set(p) for c, p in old.items())
    if key == 'unscoped':
        return ({p for e in new for p in e['files']} <= {p for e in old for p in e['files']})
    if key == 'attested':
        rest = {k for k in set(old) | set(new)
                if k not in ('reason', 'allowed_selectors', 'input_roots', 'required_for')}
        return (set(new.get('allowed_selectors', [])) <= set(old.get('allowed_selectors', []))
                and set(new.get('input_roots', [])) >= set(old.get('input_roots', []))
                and all(set(new.get('required_for', {}).get(m, [])) >= set(p)
                        for m, p in old.get('required_for', {}).items())
                and all(old.get(k) == new.get(k) for k in rest))
    if key == 'evidence':
        rest = (set(old) | set(new)) - {'skip'}
        return (set(new.get('skip', [])) <= set(old.get('skip', []))
                and all(old.get(k) == new.get(k) for k in rest))
    return False


def manifest_changes(trusted, head):
    """What the head manifest changes; every item needs the D2 policy review.
    Anything that can let more through is marked "(relaxation)": compared by
    the effective allow relation of every module pair and scope, not by the
    spelling of allow/deny."""
    if trusted.data == head.data:
        return []
    found = []
    for mid, module in head.modules.items():
        old = trusted.modules.get(mid)
        if old is None:
            found.append(f'module {mid} added (relaxation)')
            continue
        for key in ('kind', 'files', 'logical'):
            if module.get(key) != old.get(key):
                found.append(f'{mid}.{key} changed (relaxation)')
        for key in ('ci_selectors', 'ci_also', 'deny_items', 'deny_paths', 'fixture_tokens'):
            if set(old.get(key, [])) - set(module.get(key, [])):
                found.append(f'{mid}.{key} narrowed (relaxation)')
            elif module.get(key) != old.get(key):
                found.append(f'{mid}.{key} changed')
    found += [f'module {mid} removed (relaxation)' for mid in trusted.modules
              if mid not in head.modules]
    common = sorted(set(trusted.modules) & set(head.modules))
    for src in common:
        for tgt in common:
            for test in (False, True):
                if allowed(head, src, tgt, test) and not allowed(trusted, src, tgt, test):
                    found.append(f'{src}->{tgt} ({"test" if test else "production"}) newly '
                                 'allowed (relaxation)')
    old_ex = {e['id']: e for e in trusted.exceptions}
    new_ex = {e['id']: e for e in head.exceptions}
    found += [f'exception {i} added (relaxation)' for i in new_ex if i not in old_ex]
    for i in sorted(set(new_ex) & set(old_ex)):
        old, new = old_ex[i], new_ex[i]
        if old == new:
            continue
        wider = (old['source'] != new['source'] or old['target'] != new['target']
                 or (old['scope'], new['scope']) == ('test', 'production')
                 or set(new['paths']) - set(old['paths']))
        found.append(f'exception {i} changed' + (' (relaxation)' if wider else ''))
    found += [f'exception {i} removed' for i in old_ex if i not in new_ex]
    for key in ('crates', 'unscoped', 'policy_files', 'review_required', 'records',
                'exports', 'attested', 'evidence'):
        if trusted.data.get(key) != head.data.get(key):
            tight = _narrowing(key, trusted.data.get(key), head.data.get(key))
            found.append(f'{key} changed' + ('' if tight else ' (relaxation)'))
    return found or ['metadata changed']


def export_findings(manifest, tree, crate, repo, scratch):
    """Run the trusted export tool on the head and check what it removed."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    try:
        import module_single_source
    finally:
        sys.path.pop(0)
    errors, inventories = [], {}
    joins = {p for m in manifest.modules if manifest.kind(m) == 'join'
             for p in manifest.files_of(tree, m)}
    registrations = {p for m in manifest.modules if manifest.kind(m) == 'registration'
                     for p in manifest.files_of(tree, m)}
    for export in manifest.data['exports']:
        side, keeps, removes = export['side'], export['keeps'], export['removes']
        out = Path(scratch) / f'export-{side}'
        try:
            inventory, _files = module_single_source.export(tree.commit, out, side, repo=repo)
        except SystemExit as error:
            errors.append(f'export {side}: {error}')
            continue
        finally:
            shutil.rmtree(out, ignore_errors=True)
        inventories[side] = {k: inventory[k] for k in
                             ('removed', 'files', 'source_manifest_sha256')}
        kept = manifest.files_of(tree, keeps)
        owned = manifest.files_of(tree, removes)
        removable = owned | joins | {e['source'] for e in manifest.exceptions
                                     if e['target'] == removes}
        registering = registrations | {d for p in removable
                                       for d in crate.declared_by.get(p, ())}
        removed = set(inventory['removed'])
        for path in sorted(removed):
            if path in kept:
                errors.append(f'export {side} removed {path} of {keeps}, which it must keep')
            elif path not in removable:
                errors.append(f'export {side} removed {path}, which is not {removes}\'s')
        for patch in inventory['patches']:
            if patch['file'] not in registering:
                errors.append(f'export {side} patched {patch["file"]}, not a registration '
                              f'of {removes}')
        for line in inventory['remaining_references']:
            errors.append(f'export {side} still references {removes}: {line}')
        for path in sorted(owned - removed):
            errors.append(f'export {side} left {removes} file {path} in the export')
    return errors, inventories


def load_records(tree, paths):
    records, errors = [], []
    for path in paths:
        try:
            data = json.loads(tree.text(path))
        except (GateError, ValueError) as error:
            errors.append(f'record {path}: unreadable ({error})')
            continue
        if not isinstance(data, dict) or data.get('schema') != RECORD_SCHEMA:
            errors.append(f'record {path}: schema')
            continue
        for review in data.get('reviews', []):
            problems = []
            if review.get('class') not in CLASSES:
                problems.append('class')
            if review.get('verdict') != 'ACCEPT' or review.get('required_open') != 0:
                problems.append('verdict is not ACCEPT with required_open 0')
            if not review.get('reviewer') or review.get('reviewer') == review.get('author'):
                problems.append('reviewer missing or the author')
            if not isinstance(review.get('files'), dict) or not review['files']:
                problems.append('files')
            if not review.get('log'):
                problems.append('log')
            if problems:
                errors.append(f'record {path}: review {review.get("id")}: {", ".join(problems)}')
        records.append((path, data))
    return records, errors


def valid_reviews(records):
    return [r for _p, d in records for r in d.get('reviews', [])
            if r.get('class') in CLASSES and r.get('verdict') == 'ACCEPT'
            and r.get('required_open') == 0 and r.get('reviewer')
            and r.get('reviewer') != r.get('author') and isinstance(r.get('files'), dict)
            and r.get('log')]


def review_coverage(tree, records, need):
    """Every path in `need` (path -> (class, scopes)) at its head blob."""
    errors, reviews = [], valid_reviews(records)
    for path, (cls, scopes) in sorted(need.items()):
        blob = tree.blob(path) or 'deleted'
        if not any(CLASSES[r['class']] >= CLASSES[cls] and r['files'].get(path) == blob
                   and set(scopes) <= set(r.get('scopes', [])) for r in reviews):
            errors.append(f'review missing: {path} needs {cls}'
                          + (f' {"+".join(sorted(scopes))}' if scopes else '')
                          + f' at blob {blob[:12]}')
    return errors


def input_identity(tree, roots):
    """sha256 over (path, blob) of every file under the attested input roots."""
    value = hashlib.sha256()
    for path in sorted(tree.files()):
        if any(path == r or path.startswith(r.rstrip('/') + '/') for r in roots):
            value.update(f'{path}\0{tree.blob(path)}\n'.encode())
    return value.hexdigest()


def attested_findings(manifest, tree, records):
    errors, accepted = [], []
    config = manifest.data['attested']
    patterns = [glob_regex(p) for p in config['allowed_selectors']]
    identity = input_identity(tree, config['input_roots'])
    for path, data in records:
        for item in data.get('attested', []):
            selector = item.get('selector', '')
            if not any(r.match(selector) for r in patterns):
                errors.append(f'record {path}: {selector} may not be attested (CI only)')
            elif item.get('input_identity') != identity:
                errors.append(f'record {path}: attested {selector} ran on inputs '
                              f'{str(item.get("input_identity"))[:12]}, head is {identity[:12]} '
                              '(stale)')
            elif (item.get('result') != 'PASS' or not item.get('log')
                  or not re.fullmatch(r'[0-9a-f]{64}', str(item.get('log_sha256')))
                  or not item.get('reviewed_by')):
                errors.append(f'record {path}: attested {selector} lacks PASS, log, '
                              'log_sha256 or reviewed_by')
            else:
                accepted.append(selector)
    return errors, accepted, identity


def changed_cases(manifest, analysis, paths):
    """What each changed crate file's tests need, by owning module:
    ({module: exact libtest names}, {module: module-path selectors},
    {module: names and selectors that may be absent on the runner},
    {file: names under `evidence.executor_only`}).

    Literal `#[test] fn` cases are named exactly (`--skip` words excluded).
    A file whose `#[test]` the literal scan cannot name (generated by
    `macro_rules!`) gets the selector of its module instead. Tests and
    modules whose cfg is False on the runner (`#[cfg(not(test))]`) plan
    nothing. A case whose own cfg or module cfg is not known there (a cargo
    feature) is planned as optional: run when listed, never required to be.
    Cases under `evidence.executor_only` need inputs only their executor
    supplies; CI does not run them and the file needs a reviewed N/A.
    """
    skip = manifest.data['evidence']['skip']
    executor_only = [e['selector'] for e in manifest.data['evidence'].get('executor_only', [])]
    cases, selectors, optional, executor = {}, {}, {}, {}
    for entry in manifest.data['crates']:
        crate = analysis.crates.get(entry['root'])
        if crate is None:
            continue
        for path in sorted(paths):
            if path not in crate.logical:
                continue
            owner = manifest.owner(path)
            rust = crate.rust(path)
            for logical in crate.logical[path]:
                if logical in crate.no_test:
                    continue
                maybe = logical in crate.maybe_test
                if rust.unnamed_tests:
                    selector = '::'.join(logical[1:]) + '::'
                    if any(selector.startswith(e) for e in executor_only):
                        executor.setdefault(path, set()).add(selector)
                    else:
                        (optional if maybe else selectors).setdefault(owner, set()).add(
                            selector)
                for _offset, chain, name, known in rust.tests:
                    full = '::'.join((logical + chain)[1:] + (name,))
                    if any(word in full for word in skip):
                        continue
                    if any(full == e or full.startswith(e) and e.endswith('::')
                           for e in executor_only):
                        executor.setdefault(path, set()).add(full)
                    else:
                        (cases if known and not maybe else optional).setdefault(
                            owner, set()).add(full)
    return cases, selectors, optional, executor


def record_selectors(records):
    errors, selectors = [], set()
    for path, data in records:
        for selector in data.get('ci_selectors', []):
            if isinstance(selector, str) and SELECTOR.match(selector):
                selectors.add(selector)
            else:
                errors.append(f'record {path}: ci_selector {selector!r} is not a module path')
    return errors, selectors


def selection(manifest, touched, records, rust, cases=None, generated=None, optional=None):
    """CI work for a head: filters, exact changed cases, optional cases and
    selectors (cfg not known on the runner), and export sides. Every export is
    built (compile independence); it runs the kept feature's own selectors only
    when that feature changed (AGENTS.md: no tests for unchanged code)."""
    if not rust:
        return {'rust': False, 'selectors': [], 'cases': [], 'exports': []}
    cases, generated, optional = cases or {}, generated or {}, optional or {}

    def split(names):
        return {'optional_cases': sorted(n for n in names if not n.endswith('::')),
                'optional_selectors': sorted(n for n in names if n.endswith('::'))}

    selectors = {s for names in generated.values() for s in names}
    for mid in touched:
        if mid not in manifest.modules:
            continue
        selectors.update(manifest.modules[mid].get('ci_selectors', []))
        for also in manifest.modules[mid].get('ci_also', []):
            selectors.update(manifest.modules[also].get('ci_selectors', []))
    selectors |= record_selectors(records)[1]
    evidence = manifest.data['evidence']
    return {'rust': True, 'head_command': evidence['head_command'],
            'test_command': evidence['test_command'], 'skip': evidence['skip'],
            'test_args': evidence.get('test_args', []), 'env': evidence.get('env', {}),
            'identity_env': evidence.get('identity_env', {}),
            'selectors': sorted(selectors),
            'cases': sorted({c for names in cases.values() for c in names}),
            **split({c for names in optional.values() for c in names}),
            'exports': [{'side': e['side'], 'command': e['command'],
                         'selectors': sorted(set(manifest.modules[e['keeps']].get(
                             'ci_selectors', []) if e['keeps'] in touched else ())
                             | generated.get(e['keeps'], set())),
                         'cases': sorted(cases.get(e['keeps'], ())),
                         **split(optional.get(e['keeps'], set()))}
                        for e in manifest.data['exports']]}


def coverage_findings(manifest, touched, records, plan, cases, generated=None,
                      unscoped=(), executor=None):
    """Each touched module, and each changed Rust/test input that no module
    owns (fixtures under tests/, other crates), needs selected CI evidence
    or a reviewed N/A."""
    if not plan['rust']:
        return []
    generated = generated or {}
    reviewed = {path for path, data in records
                if valid_reviews([(path, data)])}
    na_items = [item for path, data in records if path in reviewed
                for item in data.get('na', []) if isinstance(item, dict) and item.get('reason')]
    na = {item.get('module') for item in na_items}
    na_paths = [glob_regex(g) for item in na_items for g in item.get('paths', [])
                if isinstance(g, str)]
    record_sel = record_selectors(records)[1]
    by_selector = {manifest.target(('crate',) + tuple(x for x in sel.split('::') if x))
                   for sel in record_sel}
    errors = []
    for mid in sorted(m for m in touched if m in manifest.modules):
        module = manifest.modules[mid]
        if (module.get('ci_selectors') or cases.get(mid) or generated.get(mid)
                or mid in by_selector or mid in na):
            continue
        errors.append(f'no CI evidence selected for {mid}: no module selector, no changed '
                      'case, no record ci_selector in it and no reviewed N/A')
    for path in sorted(unscoped):
        if record_sel or any(r.match(path) for r in na_paths):
            continue
        errors.append(f'no CI evidence selected for unscoped input {path}: no record '
                      'ci_selector and no reviewed N/A naming it')
    for path, names in sorted((executor or {}).items()):
        if any(r.match(path) for r in na_paths):
            continue
        errors.append(f'{path} changes {len(names)} executor-only case(s) that CI does not run '
                      f'(evidence.executor_only, e.g. {sorted(names)[0]}): a reviewed N/A '
                      'naming the path with the executor result is required')
    return errors


DEFINITION = re.compile(r'(?<![\w$])(?:struct|enum|union|trait|type|fn|const|static|mod)\s+'
                        r'(?:r#)?(\w+)')


def module_index(crate):
    """Logical module -> [(file, inline chain)] that spell it."""
    index = {}
    for path, logicals in crate.logical.items():
        rust = crate.rust(path)
        chains = [()] + [c + (n,) for _o, c, n, _a, _t, i in rust.declared if i]
        for logical in logicals:
            for chain in chains:
                index.setdefault(logical + chain, []).append((path, chain))
    return index


PUB_BEFORE = re.compile(r'pub\s*(?:\([^)]*\))?\s*$')


def file_items(crate, path):
    """(definitions {(chain, name)}, uses [(chain, segments, local name, pub?)])."""
    cache = crate.__dict__.setdefault('_items', {})
    if path not in cache:
        rust = crate.rust(path)
        defs = {(rust.enclosing(m.start()), m.group(1)) for m in DEFINITION.finditer(rust.code)}
        uses = []
        code = VISIBILITY.sub(lambda m: ' ' * len(m.group(0)), rust.code)
        for m in USE.finditer(code):
            aliases = []
            try:
                paths, _end = use_paths(code, m.end(), aliases)
            except ValueError:
                continue
            renamed = {tuple(full): alias for full, alias in aliases}
            chain = rust.enclosing(m.start())
            public = bool(PUB_BEFORE.search(rust.code[max(0, m.start() - 64):m.start()]))
            uses += [(chain, s, renamed.get(tuple(s), s[-1]), public) for s in paths if s]
        cache[path] = (defs, uses)
    return cache[path]


def definition_files(crate, index, names, module, name, stack=frozenset()):
    """Files that define `module::name` or re-export it on the way: the
    item's own definition, found by following `use`/`pub use` (renames and
    globs included) through the modules that import it. A name that is a
    module (declared, renamed or re-exported) brings its whole subtree
    (`module_files`). Empty when the definition is not found (a macro, an
    extern crate). `stack` breaks cycles."""
    key = ('item', module, name)
    if key in stack or len(stack) > 32:
        return set()
    stack = stack | {key}
    found = {path for path, chain in index.get(module, [])
             if (chain, name) in file_items(crate, path)[0]}
    if found:
        if module + (name,) in index:
            found |= module_files(crate, index, names, module + (name,), stack)
        return found
    for path, chain in index.get(module, []):
        for use_chain, segments, local, _public in file_items(crate, path)[1]:
            if use_chain != chain:
                continue
            if segments[-1] == '*':
                target = resolve_path(crate, module, segments[:-1], names.get(module))
                hits = definition_files(crate, index, names, target, name,
                                        stack) if target else set()
            elif local == name:
                target = resolve_path(crate, module, segments, names.get(module))
                hits = set()
                if target and len(target) > 1:
                    hits = definition_files(crate, index, names, target[:-1], target[-1],
                                            stack)
                if target and target in index:
                    hits |= module_files(crate, index, names, target, stack)
            else:
                continue
            if hits:
                found |= hits | {path}
    return found


def module_files(crate, index, names, target, stack=frozenset()):
    """Files behind a module the common API re-exports whole (`pub use m;`,
    `pub use m as n;`, `pub use m::*;`, also through other re-exports):
    every file of its subtree, and what those files re-export (`pub use`),
    followed to the definitions."""
    key = ('module', target)
    if key in stack or len(stack) > 32:
        return set()
    stack = stack | {key}
    found = set()
    for logical, places in index.items():
        if logical[:len(target)] != target:
            continue
        for path, chain in places:
            found.add(path)
            for use_chain, segments, _local, public in file_items(crate, path)[1]:
                if use_chain != chain or not public:
                    continue
                dest = resolve_path(crate, logical, segments, names.get(logical))
                if not dest:
                    continue
                if segments[-1] == '*' or dest in index:
                    found |= module_files(crate, index, names, dest, stack)
                if segments[-1] != '*' and len(dest) > 1:
                    found |= definition_files(crate, index, names, dest[:-1], dest[-1], stack)
    return found


def common_sources(manifest, crate):
    """Files that define what the common API modules re-export (`pub use`),
    or re-export it on the way: a change there changes the common contract
    and needs D2 common. When a definition cannot be found the nearest
    logical module's file stands in for it."""
    index = module_index(crate)
    names = imported_names(crate)
    out = {}
    for path, logicals in crate.logical.items():
        mid = manifest.owner(path)
        if mid not in manifest.modules or manifest.kind(mid) != 'common-api':
            continue
        rust = crate.rust(path)
        for m in PUB_USE.finditer(rust.code):
            try:
                paths, _end = use_paths(rust.code, m.end())
            except ValueError:
                continue
            for logical in logicals:
                here = logical + rust.enclosing(m.start())
                for segments in paths:
                    target = resolve_path(crate, here, segments, names.get(here))
                    if not target:
                        continue
                    if segments[-1] == '*':
                        files = module_files(crate, index, names, target)
                    else:
                        files = definition_files(crate, index, names, target[:-1], target[-1])
                        if target in index:
                            files |= module_files(crate, index, names, target)
                    if not files:
                        probe = target
                        while probe and probe not in index:
                            probe = probe[:-1]
                        files = {p for p, _c in index.get(probe, [])}
                    for found in files - {path}:
                        out.setdefault(found, set()).add(mid)
    return out


def link_findings(manifest, tree):
    """The manifest's contract anchors and exception sources exist at the head."""
    errors = []
    for ex in manifest.exceptions:
        doc, _hash, anchor = ex['contract'].partition('#')
        doc_path = f'devdoc/{doc}'
        if not tree.exists(ex['source']):
            errors.append(f'exception {ex["id"]}: source {ex["source"]} does not exist')
        if not tree.exists(doc_path):
            errors.append(f'exception {ex["id"]}: contract {doc_path} does not exist')
        elif anchor and f'id="{anchor}"' not in tree.text(doc_path):
            errors.append(f'exception {ex["id"]}: contract anchor {ex["contract"]} missing')
    return errors


def attribute_findings(tree):
    """`git archive` honours export-ignore/export-subst; the export and build
    runners read the tree through it, so a change may not use them."""
    return [f'{p}: export-ignore/export-subst would change what CI builds'
            for p in sorted(tree.files()) if PurePosixPath(p).name == '.gitattributes'
            and re.search(rb'export-(?:ignore|subst)', tree.read(p))]


def judge(repo, base, head, policy=None, mode='merge', exports=True):
    """Judge merge-base(base, head)..head under the trusted manifest."""
    base_c, head_c = resolve(repo, base), resolve(repo, head)
    merge_base = git(repo, 'merge-base', base_c, head_c).decode().strip()
    policy_c = resolve(repo, policy) if policy else merge_base
    policy_tree = Tree(repo, policy_c)
    report = {'schema': REPORT_SCHEMA, 'mode': mode, 'base': base_c, 'head': head_c,
              'merge_base': merge_base, 'policy': policy_c,
              'manifest_blob': policy_tree.blob(MANIFEST),
              'checker_blob': policy_tree.blob('scripts/check_module_gate.py'),
              'errors': [], 'pending': [], 'notes': []}
    trusted = Manifest.at(policy_tree)
    if trusted is None:
        report.update(status='NOT_ADOPTED', notes=[f'no {MANIFEST} at policy {policy_c}'],
                      selection={'rust': False, 'selectors': [], 'cases': [], 'exports': []},
                      summary=f'not adopted at {policy_c[:12]}')
        return report
    head_tree = Tree(repo, head_c)
    errors, pending, notes = report['errors'], report['pending'], report['notes']
    try:
        head_manifest = Manifest.at(head_tree)
        if head_manifest is None:
            errors.append(f'head deletes {MANIFEST}')
    except GateError as error:
        head_manifest = None
        errors.append(f'head manifest invalid: {error}')
    diff = changes(repo, merge_base, head_c)
    touched, need = {}, {}
    for status, old, new in diff:
        for path in filter(None, (old, new)):
            owner = trusted.owner(path)
            if owner is None and head_manifest is not None:
                owner = head_manifest.owner(path)  # registered by the (reviewed) head manifest
            if owner is None:
                errors.append(f'unregistered path {path} ({status})')
                continue
            if owner != '-' and owner in trusted.modules:
                touched.setdefault(owner, set()).add(path)
            cls = trusted.review_class(path)
            if cls:
                need[path] = (cls, {'policy'} if trusted.is_policy(path) else set())
    judging = head_manifest or trusted
    roots = sorted({e['root'] for m in (trusted, head_manifest) if m for e in m.data['crates']})
    analysis = Analysis(head_tree, roots)
    base_tree = Tree(repo, merge_base)
    base_analysis = Analysis(base_tree, roots)
    sources = {}
    for found in (analysis, base_analysis):
        for crate in found.crates.values():
            for path, mids in common_sources(trusted, crate).items():
                sources.setdefault(path, set()).update(mids)
    defining = sorted({p for _s, o, n in diff for p in (o, n) if p} & set(sources))
    features = sorted(m for m in touched if trusted.kind(m) == 'feature')
    if len(features) > 1:
        notes.append(f'joint change across {features}: their files need a D2 joint review')
        for mid in features:
            for path in touched[mid]:
                need[path] = ('D2', need.get(path, ('D1', set()))[1] | {'joint'})
    for mid in sorted(m for m in touched if trusted.kind(m) == 'common-api'):
        notes.append(f'common API {mid} changed: its files need a D2 common review')
        for path in touched[mid]:
            need[path] = ('D2', need.get(path, ('D1', set()))[1] | {'common'})
    for path in defining:
        notes.append(f'{path} defines what {"/".join(sorted(sources[path]))} re-exports: '
                     'D2 common review')
        need[path] = ('D2', need.get(path, ('D1', set()))[1] | {'common'})
    relax = manifest_changes(trusted, head_manifest) if head_manifest else []
    if relax:
        notes.append('manifest changes (D2 policy review): ' + '; '.join(relax))
    errors += dependency_findings(judging, analysis)
    errors += link_findings(judging, head_tree)
    errors += attribute_findings(head_tree)
    if judging is not trusted:
        only = [f for f in dependency_findings(trusted, analysis)
                if f not in errors and not f.startswith('stale exception')]
        if only:
            notes.append(f'{len(only)} finding(s) only under the trusted manifest, accepted '
                         'only through the D2 policy review: ' + '; '.join(only[:5]))
    rust = any(RUST_INPUTS.match(p) for _s, o, n in diff for p in (o, n) if p)
    record_paths = sorted(n for _s, _o, n in diff if n and trusted.records.match(n))
    records, record_errors = load_records(head_tree, record_paths)
    errors += record_errors
    if exports and rust and trusted.data['crates'][0]['root'] in analysis.crates:
        crate = analysis.crates[trusted.data['crates'][0]['root']]
        with tempfile.TemporaryDirectory(prefix='xadb-module-gate-') as scratch:
            export_errors, report['exports'] = export_findings(trusted, head_tree, crate,
                                                               repo, scratch)
        errors += export_errors
    attested_errors, report['attested'], report['input_identity'] = attested_findings(
        trusted, head_tree, records)
    errors += attested_errors
    errors += record_selectors(records)[0]
    obligations = review_coverage(head_tree, records, need)
    cases, generated, optional, executor = changed_cases(trusted, analysis,
                                                         {n for _s, _o, n in diff if n})
    plan = selection(trusted, touched, records, rust, cases, generated, optional)
    unscoped = {p for _s, o, n in diff for p in (o, n)
                if p and RUST_INPUTS.match(p) and trusted.owner(p) == '-'}
    obligations += coverage_findings(trusted, touched, records, plan, cases, generated,
                                     unscoped, executor)
    if rust:
        for mid, patterns in sorted(trusted.data['attested'].get('required_for', {}).items()):
            for pattern in patterns if mid in touched else []:
                regex = glob_regex(pattern)
                if not any(regex.match(sel) for sel in report['attested']):
                    obligations.append(f'{mid} changed: attested {pattern} at this head\'s '
                                       f'inputs {report["input_identity"][:12]} is missing')
    (errors if mode == 'merge' else pending).extend(obligations)
    status = 'FAIL' if errors else 'PASS'
    summary = f'{status}: {len(errors)} error(s), {len(pending)} pending'
    if any('relaxation' in r for r in relax):
        summary += '; POLICY RELAXATION (D2 policy record)'
    report.update(
        touched={m: sorted(p) for m, p in sorted(touched.items())},
        review_needed={p: {'class': c, 'scopes': sorted(s)} for p, (c, s) in sorted(need.items())},
        records=record_paths, selection=plan, status=status, summary=summary)
    return report


# --------------------------------------------------------------------------
# CI final aggregator: accept the build run of the same head as data
# --------------------------------------------------------------------------

def verify_receipt(report, receipt, run, repository, checker_sha256):
    errors = []
    if not isinstance(receipt, dict) or receipt.get('schema') != RECEIPT_SCHEMA:
        return ['build receipt schema']
    if run.get('head_sha') != report['head'] or receipt.get('head') != report['head']:
        errors.append(f'build run/receipt are for {run.get("head_sha")}/{receipt.get("head")}, '
                      f'the head is {report["head"]} (stale result)')
    if run.get('event') != 'pull_request' or (
            run.get('path', '').split('@')[0] != receipt.get('workflow')):
        errors.append(f'build run event/path {run.get("event")}/{run.get("path")} unexpected')
    if (run.get('head_repository') or {}).get('full_name') != repository:
        errors.append('build run is from another repository')
    if receipt.get('checker_sha256') != checker_sha256:
        errors.append('build receipt planned with another checker revision')
    # The default branch may move between the two workflows; the plan is the
    # same when the manifest and checker blobs are.
    for key in ('manifest_blob', 'checker_blob'):
        if receipt.get(key) != report.get(key):
            errors.append(f'build receipt {key} {receipt.get(key)} != {report.get(key)}')
    want = report['selection']
    if receipt.get('selection') != want:
        errors.append('build receipt selection differs from the trusted plan')
    results = {r.get('name'): r for r in receipt.get('results', []) if isinstance(r, dict)}
    # (name, kind, planned): kind 'build'; 'selector' (at least one listed,
    # every listed passes or is ignored); 'cases' (every planned name listed
    # once and passing); 'optional-selector' / 'optional-cases' (the cfg is
    # not known on the runner: what is listed passes, none may be listed).
    expected = []
    for prefix, part in ([('head', want)] + [(f'export-{e["side"]}', e)
                                             for e in want['exports']] if want['rust'] else []):
        expected.append((f'{prefix}-build', 'build', None))
        expected += [(f'{prefix}:{s}', 'selector', None) for s in part['selectors']]
        if part.get('cases'):
            expected.append((f'{prefix}-cases', 'cases', len(part['cases'])))
        expected += [(f'{prefix}?:{s}', 'optional-selector', None)
                     for s in part.get('optional_selectors', [])]
        if part.get('optional_cases'):
            expected.append((f'{prefix}-optional-cases', 'optional-cases',
                             len(part['optional_cases'])))
    for name, kind, planned in expected:
        result = results.get(name)
        if result is None:
            errors.append(f'build receipt lacks {name} (missing, cancelled or skipped)')
            continue
        if result.get('status') != 'PASS':
            errors.append(f'{name}: {result.get("status")}')
            continue
        if kind == 'build':
            continue
        listed, passed = result.get('listed'), result.get('passed')
        ignored = result.get('ignored', 0)
        ok = (isinstance(listed, int) and isinstance(passed, int) and isinstance(ignored, int)
              and result.get('failed') == 0)
        if ok and kind == 'selector':
            ok = passed >= 1 and passed + ignored == listed
        elif ok and kind == 'cases':
            ok = listed == planned and passed == planned
        elif ok and kind == 'optional-selector':
            ok = passed + ignored == listed
        elif ok:
            ok = listed <= planned and passed == listed
        if not ok:
            errors.append(f'{name}: listed {listed} passed {passed} failed '
                          f'{result.get("failed")} (every planned case must be listed and pass)')
    extra = sorted(set(results) - {n for n, _k, _p in expected})
    if extra:
        errors.append(f'build receipt has undeclared results {extra}')
    return errors


class SameHostRedirect(urllib.request.HTTPRedirectHandler):
    """Follow a redirect, but never carry the token to another host (the
    artifact download redirects to a signed storage URL)."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        new = super().redirect_request(req, fp, code, msg, headers, newurl)
        if new is not None and (urllib.parse.urlsplit(newurl).netloc
                                != urllib.parse.urlsplit(req.full_url).netloc):
            for key in [k for k in new.headers if k.lower() == 'authorization']:
                del new.headers[key]
            new.unredirected_hdrs.pop('Authorization', None)
        return new


def api(url, token, raw=False, opener=None):
    request = urllib.request.Request(url)
    # An unredirected header is sent to `url` only, never to a redirect target.
    request.add_unredirected_header('Authorization', f'Bearer {token}')
    request.add_header('Accept', 'application/vnd.github+json')
    request.add_header('X-GitHub-Api-Version', '2022-11-28')
    opener = opener or urllib.request.build_opener(SameHostRedirect)
    with opener.open(request, timeout=60) as response:
        data = response.read()
    return data if raw else json.loads(data)


def final(args, report, fetch=api, sleep=time.sleep):
    """Wait for the build run of the head, then accept it only as data."""
    if not report['selection']['rust']:
        report['notes'].append('no Rust/test input changed: the build run is N/A')
        return []
    token = os.environ.get('GITHUB_TOKEN')
    if not token:
        return ['GITHUB_TOKEN missing']
    root = f'https://api.github.com/repos/{args.repository}'
    workflow = PurePosixPath(args.build_workflow).name
    deadline = time.time() + args.wait
    while True:
        runs = fetch(f'{root}/actions/workflows/{workflow}/runs?head_sha={report["head"]}'
                   '&event=pull_request&per_page=30', token).get('workflow_runs', [])
        runs = [r for r in runs if r.get('head_sha') == report['head']]
        runs.sort(key=lambda r: (r['run_number'], r.get('run_attempt', 1)), reverse=True)
        run = runs[0] if runs else None
        if run and run.get('status') == 'completed':
            break
        if time.time() > deadline:
            return [f'build run for {report["head"]} not completed within {args.wait}s '
                    f'({run.get("status") if run else "missing"})']
        sleep(30)
    report['build_run'] = {'id': run['id'], 'attempt': run.get('run_attempt'),
                           'url': run.get('html_url'), 'conclusion': run.get('conclusion')}
    errors = []
    if run.get('conclusion') != 'success':
        errors.append(f'build run {run["id"]} concluded {run.get("conclusion")}')
    artifacts = fetch(f'{root}/actions/runs/{run["id"]}/artifacts', token).get('artifacts', [])
    named = [a for a in artifacts if a['name'] == args.artifact and not a.get('expired')]
    if len(named) != 1:
        return errors + [f'build run {run["id"]}: {len(named)} {args.artifact} artifacts']
    with zipfile.ZipFile(io.BytesIO(fetch(named[0]['archive_download_url'], token,
                                        raw=True))) as archive:
        receipt = json.loads(archive.read('receipt.json'))
    checker = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    return errors + verify_receipt(report, receipt, run, args.repository, checker)


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------

def review_files(repo, base, head):
    base_c, head_c = resolve(repo, base), resolve(repo, head)
    merge_base = git(repo, 'merge-base', base_c, head_c).decode().strip()
    tree, trusted = Tree(repo, head_c), Manifest.at(Tree(repo, merge_base))
    if trusted is None:
        raise GateError(f'no {MANIFEST} at {merge_base}')
    files = {}
    for _status, old, new in changes(repo, merge_base, head_c):
        for path in filter(None, (old, new)):
            if trusted.review_class(path):
                files[path] = tree.blob(path) or 'deleted'
    return dict(sorted(files.items()))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    sub = parser.add_subparsers(dest='command', required=True)
    for name in ('check', 'plan', 'files', 'final'):
        p = sub.add_parser(name)
        p.add_argument('--repo', default='.')
        p.add_argument('--base', required=True)
        p.add_argument('--head', required=True)
        p.add_argument('--policy', help='trusted manifest revision (default: merge-base)')
        p.add_argument('--mode', choices=('push', 'merge'), default='merge')
        p.add_argument('--out', help='write the JSON report here')
        if name == 'final':
            p.add_argument('--repository', required=True)
            p.add_argument('--build-workflow', required=True)
            p.add_argument('--artifact', required=True)
            p.add_argument('--wait', type=int, default=7500)
    args = parser.parse_args(argv)
    try:
        if args.command == 'files':
            print(json.dumps(review_files(args.repo, args.base, args.head), indent=1))
            return 0
        report = judge(args.repo, args.base, args.head, args.policy, args.mode,
                       exports=args.command != 'plan')
        if args.command == 'plan':
            print(json.dumps(report['selection'], indent=1, sort_keys=True))
            return 0
        if args.command == 'final':
            if report['status'] == 'NOT_ADOPTED':
                # The control workflow exists only where the gate is adopted;
                # losing the manifest there must not turn every PR green.
                report['errors'].append(f'no {MANIFEST} at the trusted revision')
            elif report['errors']:
                report['notes'].append('the judgement already failed: the build run is not '
                                       'awaited')
            else:
                report['errors'] += final(args, report)
            report['status'] = 'FAIL' if report['errors'] else 'PASS'
            report['summary'] = (f'{report["status"]}: {len(report["errors"])} error(s)'
                                 + ('; POLICY RELAXATION (D2 policy record)'
                                    if 'RELAXATION' in report.get('summary', '') else ''))
    except (GateError, OSError, KeyError, ValueError, TypeError, urllib.error.URLError) as error:
        report = {'schema': REPORT_SCHEMA, 'status': 'ERROR',
                  'errors': [f'checker error: {error!r}'], 'pending': [], 'notes': [],
                  'summary': 'ERROR: the checker could not judge'}
    if args.out:
        Path(args.out).write_text(json.dumps(report, indent=1) + '\n')
    for key, label in (('errors', 'FAIL'), ('pending', 'PENDING'), ('notes', 'NOTE')):
        for line in report.get(key, []):
            print(f'XADB module gate: {label} {line}', file=sys.stderr)
    print(f'XADB module gate: {report["status"]} ({len(report.get("errors", []))} error(s), '
          f'{len(report.get("pending", []))} pending)', file=sys.stderr)
    return 0 if report['status'] in ('PASS', 'NOT_ADOPTED') else 1


if __name__ == '__main__':
    raise SystemExit(main())
