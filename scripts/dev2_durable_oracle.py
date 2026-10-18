#!/usr/bin/env python3
"""Independent DEV2 disk image and external ACK oracle (Python standard library).

No production codec, recovery implementation, or worker-reported verdict is used.
The wire grammar is the frozen AXF1/XDK1 format in devdoc/implementation/dev2-
implementation.md. This program is read-only: inspect a retained, quiescent crash
image, never a directory concurrently used by an engine. An ACCEPTED result only
qualifies this bounded case; process kill is not a power-loss qualification.

CLI: --directory IMAGE --history history.bin [--history ...] --expect case.json
     --expect-sha256 DIGEST [--output result.json]

The expectation digest must be retained by the controller BEFORE the campaign.
Expectations have version=1 and expected_outcome=RECOVER_OK, REJECT_CORRUPTION,
or REJECT_INCOMPLETE_CREATE (absent CURRENT or WRITER is not classified as damage),
fault_model=process_kill|deterministic_io|qualified_power_loss,
allow_incomplete_tail=bool, database=[16 bytes], timeline=positive integer,
history_complete=true. The separate recovered file contains
{status:READY,q:uint,state:{tables:[],rows:[]}} OR
{status:REJECTED,normal_queries:0,new_acks:0}. Recovered observations
must be externally captured; they are not a worker's recovery-verdict assertion.
The --recovered digest is recorded independently of the frozen expectation.

State uses handle=[object,generation,schema_epoch]; tables are {handle,name,
columns:[{name,data_type,nullable}]}; rows are {table:handle,row,values:[native
Cell objects]}. Optional checkpoint_evidence entries bind generation,digest,cut,
state and prefix=[{transaction:{database,timeline,writer_epoch,local},q,witness}].
Each witness is an exact distinct retained row in that checkpoint, independently
recorded by the controller. Missing pruned-prefix membership proof is INCOMPLETE,
never an ACK-preservation PASS inferred from final-state equality.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import struct
import sys
from dataclasses import dataclass

sys.dont_write_bytecode = True
MAX_PAYLOAD = 1 << 20
MAX_SEGMENT = 16 << 20
MAX_WAL = 256 << 20
MAX_FILES = 4096
MAX_HISTORY = 64 << 20
MAX_PROOF = 16 << 20
MAX_TRANSACTIONS = 131072
MAX_STATE = 128 << 20  # Conservative independent logical budget, not Rust RSS.
ZERO = bytes(32)
FILE_PATTERN = re.compile(r'(?:LOCK|CURRENT|WRITER|wal-([0-9a-f]{8})-([0-9a-f]{16})\.axf|resume-([0-9a-f]{8})\.axf|manifest-([0-9a-f]{16})\.axf|chunk-([0-9a-f]{16})-([0-9a-f]{8})\.axf|select-([0-9a-f]{8})-([0-9a-f]{16})\.tmp)\Z')


class OracleError(Exception):
    def __init__(self, verdict, code, detail):
        super().__init__(detail)
        self.verdict, self.code = verdict, code


class Corruption(OracleError):
    def __init__(self, detail):
        super().__init__('REJECTED', 'CORRUPTION', detail)


def invalid(detail):
    raise OracleError('INVALID', 'EVIDENCE', detail)


def incomplete(detail):
    raise OracleError('INCOMPLETE', 'LIMIT_OR_MISSING_EVIDENCE', detail)


def reject(detail):
    raise OracleError('REJECTED', 'HISTORY_MISMATCH', detail)


def require(condition, detail):
    if not condition:
        raise Corruption(detail)


def integer(value, low=0, high=(1 << 64) - 1):
    if type(value) is not int or not low <= value <= high:
        invalid('integer outside evidence bounds')
    return value


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            invalid('duplicate JSON field: ' + key)
        result[key] = value
    return result


def json_bytes(data):
    try:
        return json.loads(data, object_pairs_hook=unique_object,
                          parse_constant=lambda _: invalid('nonfinite JSON number'))
    except (ValueError, UnicodeError, RecursionError) as error:
        invalid('invalid JSON: ' + str(error))


def sha(data):
    return hashlib.sha256(data).hexdigest()


def reserved(number):
    require(number >> 32 != 0 and number & 0xffffffff != 0, 'unreserved logical identity')
    return number


class Reader:
    """Independent cursor grammar; counts are checked before constructing arrays."""
    def __init__(self, data):
        self.data, self.at, self.cost = data, 0, 0

    def take(self, size):
        end = self.at + size
        require(end <= len(self.data), 'truncated complete record')
        value, self.at = self.data[self.at:end], end
        return value

    def number(self, fmt):
        return struct.unpack('<' + fmt, self.take(struct.calcsize('<' + fmt)))[0]

    def u8(self): return self.number('B')
    def u16(self): return self.number('H')
    def u32(self): return self.number('I')
    def u64(self): return self.number('Q')

    def boolean(self):
        value = self.u8()
        require(value in (0, 1), 'noncanonical boolean')
        return bool(value)

    def charge(self, amount):
        self.cost += amount
        if self.cost > 8 * MAX_PAYLOAD:
            incomplete('independent decoded-record budget exceeded')

    def count(self, value, maximum, minimum_wire, element_cost):
        require(value <= maximum and value * minimum_wire <= len(self.data) - self.at,
                'record count exceeds wire bounds')
        self.charge(value * element_cost)
        return value

    def name(self):
        size = self.u16()
        require(1 <= size <= 128, 'schema name length')
        self.charge(size * 4 + 64)
        try:
            value = self.take(size).decode('utf-8')
        except UnicodeError:
            raise Corruption('schema name UTF-8')
        require('\0' not in value, 'schema name NUL')
        return value

    def handle(self):
        result = (reserved(self.u64()), self.u64(), self.u64())
        require(result[1:] == (1, 1), 'object generation/schema epoch')
        return result

    def table(self):
        handle, name = self.handle(), self.name()
        count = self.count(self.u16(), 32, 5, 256)
        require(count > 0, 'empty table columns')
        columns = []
        for _ in range(count):
            column, kind, nullable = self.name(), self.u8(), self.boolean()
            require(kind in (1, 2, 3, 4) and column not in [c['name'] for c in columns], 'schema column/type')
            columns.append({'name': column, 'data_type': ('bool', 'int64', 'utf8', 'binary')[kind - 1], 'nullable': nullable})
        return {'handle': handle, 'name': name, 'columns': columns}

    def row(self):
        table, row = self.handle(), reserved(self.u64())
        start = self.at
        count = self.count(self.u32(), 32, 1, 192)
        require(count > 0, 'empty row')
        values = []
        for _ in range(count):
            tag = self.u8()
            require(tag <= 4, 'scalar tag')
            if tag == 0:
                cell = {'type': 'null'}
            elif tag == 1:
                cell = {'type': 'bool', 'value': self.boolean()}
            elif tag == 2:
                cell = {'type': 'int64', 'value': self.number('q')}
            else:
                size = self.count(self.u32(), 65536, 1, 4)
                raw = self.take(size)
                if tag == 3:
                    try:
                        value = raw.decode('utf-8')
                    except UnicodeError:
                        raise Corruption('scalar UTF-8')
                else:
                    value = raw.hex()
                cell = {'type': 'utf8' if tag == 3 else 'binary', 'value': value}
            values.append(cell)
            require(self.at - start <= 128 * 1024, 'canonical row exceeds 128 KiB')
        return {'table': table, 'row': row, 'values': values}

    def link(self):
        item = {'epoch': self.u32(), 'segment': self.u64(), 'end': self.u64(),
                'size': self.u64(), 'q': self.u64(), 'digest': self.take(32).hex()}
        require(item['epoch'] > 0 and item['segment'] > 0 and 85 <= item['end'] <= item['size'] <= MAX_SEGMENT,
                'predecessor boundary')
        return item

    def anchor(self):
        item = {'epoch': self.u32(), 'segment': self.u64(), 'header': self.take(32).hex(),
                'offset': self.u64(), 'digest': self.take(32).hex(), 'q': self.u64()}
        require(item['epoch'] > 0 and item['segment'] > 0 and 85 <= item['offset'] <= MAX_SEGMENT, 'anchor boundary')
        return item

    def reference(self):
        generation = self.u64()
        require(generation > 0, 'zero manifest generation')
        return {'generation': generation, 'digest': self.take(32).hex()}


def decode_payload(payload):
    reader = Reader(payload)
    require(reader.take(4) == b'XDK1', 'engine record magic')
    if reader.u16() != 1 or reader.u64() != 1:
        raise OracleError('REJECTED', 'UNSUPPORTED_FORMAT', 'unsupported engine format/features')
    kind, database, timeline, epoch = reader.u8(), reader.take(16).hex(), reader.u64(), reader.u32()
    require(int(database, 16) != 0 and timeline > 0 and epoch > 0, 'zero namespace')
    record = {'kind': kind, 'identity': (database, timeline, epoch)}
    if kind == 1:
        record.update(segment=reader.u64(), start=reader.u64(), origin=reader.u8())
        record['previous'] = reader.link() if reader.boolean() else None
        record['resume'] = reader.take(32).hex()
        previous, origin = record['previous'], record['origin']
        require(record['segment'] > 0, 'zero segment')
        good = origin == 0 and previous is None and record['start'] == 0 and record['resume'] == ZERO.hex()
        good |= origin == 3 and previous is None and record['resume'] == ZERO.hex()
        if previous:
            good |= origin == 1 and previous['epoch'] == epoch and previous['segment'] < record['segment'] and previous['q'] == record['start'] and previous['end'] == previous['size'] and record['resume'] == ZERO.hex()
            good |= origin == 2 and previous['epoch'] < epoch and previous['q'] == record['start'] and record['resume'] != ZERO.hex()
        require(good, 'segment origin/predecessor')
    elif kind == 2:
        record.update(local=reader.u64(), previous_q=reader.u64(), q=reader.u64())
        count = reader.count(reader.u16(), 256, 25, 256)
        require(record['local'] > 0 and record['q'] == record['previous_q'] + 1 and count > 0, 'batch sequence/count')
        changes, row_ids = [], set()
        for _ in range(count):
            tag = reader.u8()
            require(tag in (1, 2, 3, 4), 'change tag')
            if tag == 1:
                change = {'change': 'create', 'table': reader.table()}
            elif tag == 2:
                change = {'change': 'drop', 'table': reader.handle()}
            elif tag == 3:
                change = {'change': 'put', 'row': reader.row()}
            else:
                change = {'change': 'delete', 'table': reader.handle(), 'row': reserved(reader.u64())}
            if tag <= 2:
                require(count == 1, 'mixed DDL/DML batch')
            else:
                number = change['row']['row'] if tag == 3 else change['row']
                require(number not in row_ids, 'duplicate global row after-image')
                row_ids.add(number)
            changes.append(change)
        body = reader.at
        require(reader.take(4) == b'XDCM' and reader.u64() == record['q'] and reader.u16() == count and reader.u32() == body,
                'commit trailer mismatch')
        record['changes'] = changes
    elif kind == 3:
        record.update(end=reader.u64(), q=reader.u64(), digest=reader.take(32).hex())
        require(85 <= record['end'] <= MAX_SEGMENT - 133, 'seal boundary')
    elif kind == 4:
        record.update(segment=reader.u64(), previous=reader.link())
        require(record['segment'] > 0 and record['previous']['epoch'] < epoch, 'resume boundary')
    elif kind == 5:
        record.update(segment=reader.u64(), header=reader.take(32).hex())
        require(record['segment'] > 0, 'zero writer segment')
    elif kind == 6:
        record['current'] = reader.reference()
        record['previous'] = reader.reference() if reader.boolean() else None
        require(record['previous'] is None or record['previous']['generation'] < record['current']['generation'], 'CURRENT generation order')
    elif kind == 7:
        record.update(generation=reader.u64(), cut=reader.u64(), high_water=reader.u32(), anchor=reader.anchor())
        count = reader.count(reader.u16(), 64, 33, 256)
        record['tables'] = [reader.table() for _ in range(count)]
        count = reader.count(reader.u32(), MAX_FILES - 1, 48, 256)
        record['chunks'] = [{'index': reader.u32(), 'bytes': reader.u64(), 'digest': reader.take(32).hex(), 'rows': reader.u32()} for _ in range(count)]
        record['provenance'] = None
        if reader.boolean():
            record['provenance'] = {'database': reader.take(16).hex(), 'timeline': reader.u64(), 'generation': reader.u64(), 'digest': reader.take(32).hex()}
        require(record['generation'] > 0 and epoch == record['anchor']['epoch'] <= record['high_water'] and record['anchor']['q'] == record['cut'], 'manifest identity/cut')
        require(len({t['handle'][0] for t in record['tables']}) == len(record['tables']) and len({t['name'] for t in record['tables']}) == len(record['tables']), 'duplicate manifest table')
        for index, chunk in enumerate(record['chunks']):
            require(chunk['index'] == index and chunk['rows'] > 0 and 138 <= chunk['bytes'] <= MAX_PAYLOAD + 42, 'chunk inventory')
        source = record['provenance']
        require(source is None or (source['database'] == database and source['timeline'] > 0 and source['timeline'] != timeline and source['generation'] > 0), 'restore provenance')
    elif kind == 8:
        record.update(generation=reader.u64(), index=reader.u32())
        count = reader.count(reader.u32(), MAX_PAYLOAD // 37, 37, 256)
        require(record['generation'] > 0 and record['index'] < MAX_FILES and count > 0, 'chunk bounds')
        record['rows'] = [reader.row() for _ in range(count)]
        keys = [(r['table'][0], r['row']) for r in record['rows']]
        require(all(a < b for a, b in zip(keys, keys[1:])), 'chunk row order')
    else:
        raise Corruption('unknown engine record kind')
    require(reader.at == len(payload), 'record trailing bytes')
    return record


def decode_envelope(raw):
    require(len(raw) >= 42 and raw[:4] == b'AXF1', 'AXF1 magic/truncation')
    version, length = struct.unpack_from('<HI', raw, 4)
    if version != 1:
        raise OracleError('REJECTED', 'UNSUPPORTED_FORMAT', 'unsupported AXF1 version')
    if length > MAX_PAYLOAD:
        incomplete('AXF1 declared payload exceeds 1 MiB')
    require(len(raw) == length + 42, 'AXF1 length')
    require(hashlib.sha256(raw[:-32]).digest() == raw[-32:], 'AXF1 checksum')
    return raw[10:-32]


def decode_frame(raw):
    return decode_payload(decode_envelope(raw))


@dataclass
class Frame:
    record: dict
    raw: bytes
    end: int


class Image:
    def __init__(self, directory):
        self.directory, self.inventory, self.used = Path(directory), {}, {}
        wal_bytes = 0
        try:
            with os.scandir(self.directory) as entries:
                for entry in entries:
                    if len(self.inventory) == MAX_FILES:
                        incomplete('namespace exceeds 4096 files')
                    parsed_name = FILE_PATTERN.fullmatch(entry.name)
                    if not parsed_name:
                        raise Corruption('unrecognized database filename')
                    groups = parsed_name.groups()
                    require(all(value is None or int(value, 16) > 0 for index, value in enumerate(groups) if index != 5), 'zero generated filename identity')
                    metadata = entry.stat(follow_symlinks=False)
                    require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1, 'database file type/link count')
                    limit = MAX_SEGMENT if entry.name.startswith('wal-') else MAX_PAYLOAD + 42
                    if metadata.st_size > limit:
                        incomplete('file exceeds independent bounded profile: ' + entry.name)
                    self.inventory[entry.name] = self.stamp(metadata)
                    if entry.name.startswith('wal-'):
                        wal_bytes += metadata.st_size
            if wal_bytes > MAX_WAL:
                incomplete('WAL exceeds 256 MiB')
        except OSError as error:
            incomplete('namespace I/O: ' + str(error))

    @staticmethod
    def stamp(metadata):
        return metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns

    def open(self, name):
        require(name in self.inventory, 'missing selected file: ' + name)
        try:
            descriptor = os.open(self.directory / name, os.O_RDONLY | os.O_NOFOLLOW)
            stream = os.fdopen(descriptor, 'rb')
            if self.stamp(os.fstat(stream.fileno())) != self.inventory[name]:
                stream.close()
                invalid('image changed before read')
            return stream
        except OSError as error:
            incomplete('required-file I/O: ' + str(error))

    def metadata(self, name):
        with self.open(name) as stream:
            raw = stream.read(MAX_PAYLOAD + 43)
        require(len(raw) <= MAX_PAYLOAD + 42, 'metadata exceeds one frame')
        self.used[name] = sha(raw)
        return decode_frame(raw), raw

    def frame(self, stream, size):
        start = stream.tell()
        if start == size:
            return None
        header = stream.read(min(10, size - start))
        require(header[:min(4, len(header))] == b'AXF1'[:min(4, len(header))], 'partial AXF1 magic')
        if (len(header) == 5 and header[4] != 1) or (len(header) >= 6 and struct.unpack_from('<H', header, 4)[0] != 1):
            raise OracleError('REJECTED', 'UNSUPPORTED_FORMAT', 'partial AXF1 version')
        if len(header) < 10:
            return 'incomplete'
        length = struct.unpack_from('<I', header, 6)[0]
        if length > MAX_PAYLOAD:
            incomplete('declared WAL payload exceeds 1 MiB')
        require(length >= 43, 'engine AXF1 payload too short')
        if length + 42 > size - start:
            return 'incomplete'
        raw = header + stream.read(length + 32)
        require(len(raw) == length + 42, 'image shortened while reading')
        return Frame(decode_frame(raw), raw, start + len(raw))

    def confirm(self):
        try:
            names = {entry.name for entry in os.scandir(self.directory)}
            if names != set(self.inventory):
                invalid('image namespace changed during inspection')
            for name, stamp in self.inventory.items():
                if self.stamp(os.stat(self.directory / name, follow_symlinks=False)) != stamp:
                    invalid('image changed during inspection')
        except OSError as error:
            incomplete('image verification I/O: ' + str(error))


def wal_name(epoch, segment): return f'wal-{epoch:08x}-{segment:016x}.axf'
def manifest_name(generation): return f'manifest-{generation:016x}.axf'
def chunk_name(generation, index): return f'chunk-{generation:016x}-{index:08x}.axf'


class State:
    def __init__(self):
        self.tables, self.rows, self.cost = {}, {}, 0

    @staticmethod
    def row_cost(row):
        return 256 + sum(192 + (4 * len(c['value']) if c['type'] in ('utf8', 'binary') else 0) for c in row['values'])

    def budget(self):
        if self.cost > MAX_STATE or len(self.rows) > MAX_TRANSACTIONS:
            incomplete('independent logical-state budget exceeded')

    def put(self, row, epoch, checkpoint=False):
        handle = row['table']
        require(handle in self.tables and handle[0] >> 32 <= epoch and row['row'] >> 32 <= epoch, 'row table/future identity')
        old = self.rows.get(row['row'])
        require(old is None or (not checkpoint and old['table'] == handle), 'duplicate global live RowId')
        require(checkpoint or old is not None or row['row'] >> 32 == epoch, 'new row issued outside batch epoch')
        columns = self.tables[handle]['columns']
        require(len(columns) == len(row['values']), 'row/schema arity')
        for column, value in zip(columns, row['values']):
            require((value['type'] == 'null' and column['nullable']) or value['type'] == column['data_type'], 'row/schema type/nullability')
        self.cost += self.row_cost(row) - (self.row_cost(old) if old else 0)
        self.rows[row['row']] = row
        self.budget()

    def create(self, table, epoch):
        handle = table['handle']
        require(handle[0] >> 32 <= epoch and handle not in self.tables and all(t['name'] != table['name'] for t in self.tables.values()) and len(self.tables) < 64, 'create identity/name/catalog bounds')
        self.tables[handle] = table
        self.cost += 1024 + len(table['columns']) * 1024
        self.budget()

    def apply(self, batch):
        epoch = batch['identity'][2]
        for change in batch['changes']:
            kind = change['change']
            if kind == 'create':
                require(change['table']['handle'][0] >> 32 == epoch, 'new table issued outside batch epoch')
                self.create(change['table'], epoch)
            elif kind == 'drop':
                handle = change['table']
                require(handle in self.tables and handle[0] >> 32 <= epoch, 'drop missing/future table')
                self.cost -= 1024 + len(self.tables[handle]['columns']) * 1024
                del self.tables[handle]
                for row_id in [key for key, row in self.rows.items() if row['table'] == handle]:
                    self.cost -= self.row_cost(self.rows.pop(row_id))
            elif kind == 'put':
                self.put(change['row'], epoch)
            else:
                row_id, handle = change['row'], change['table']
                require(handle in self.tables and handle[0] >> 32 <= epoch and row_id >> 32 <= epoch and row_id in self.rows and self.rows[row_id]['table'] == handle, 'delete absent/future row')
                self.cost -= self.row_cost(self.rows.pop(row_id))

    def snapshot(self):
        return {'tables': sorted(self.tables.values(), key=lambda t: t['handle']),
                'rows': sorted(self.rows.values(), key=lambda r: (r['table'], r['row']))}


def checkpoint(image, reference, identity):
    manifest, raw = image.metadata(manifest_name(reference['generation']))
    require(manifest['kind'] == 7 and sha(raw) == reference['digest'] and manifest['generation'] == reference['generation'] and manifest['identity'][:2] == identity[:2] and manifest['identity'][2] <= identity[2], 'selected manifest binding')
    state, previous = State(), None
    epoch = min(manifest['high_water'], manifest['anchor']['epoch'])
    for table in manifest['tables']:
        state.create(table, epoch)
    for item in manifest['chunks']:
        chunk, raw = image.metadata(chunk_name(manifest['generation'], item['index']))
        require(sha(raw) == item['digest'] and len(raw) == item['bytes'] and chunk['kind'] == 8 and chunk['identity'] == manifest['identity'] and chunk['generation'] == manifest['generation'] and chunk['index'] == item['index'] and len(chunk['rows']) == item['rows'], 'checkpoint chunk binding')
        for row in chunk['rows']:
            key = row['table'][0], row['row']
            require(previous is None or previous < key, 'global checkpoint row order')
            state.put(row, epoch, checkpoint=True)
            previous = key
    return manifest, state


def inspect_candidate(image, reference, writer, allow_tail):
    identity = writer['identity']
    manifest, state = checkpoint(image, reference, identity)
    checkpoint_state = state.snapshot()
    anchor = manifest['anchor']
    chain, seen = [], set()
    epoch, segment = identity[2], writer['segment']
    while True:
        require((epoch, segment) not in seen, 'predecessor cycle')
        seen.add((epoch, segment))
        if len(seen) > MAX_FILES:
            incomplete('predecessor plan budget')
        name = wal_name(epoch, segment)
        with image.open(name) as stream:
            size = image.inventory[name][2]
            frame = image.frame(stream, size)
        require(isinstance(frame, Frame) and frame.record['kind'] == 1, 'required complete WAL header')
        header = frame.record
        require(header['identity'] == (identity[0], identity[1], epoch) and header['segment'] == segment, 'WAL header namespace')
        digest = sha(frame.raw)
        require(chain or digest == writer['header'], 'WRITER header digest')
        if header['origin'] == 2:
            resume, raw = image.metadata(f'resume-{epoch:08x}.axf')
            require(resume['kind'] == 4 and resume['identity'] == header['identity'] and resume['segment'] == segment and resume['previous'] == header['previous'] and sha(raw) == header['resume'], 'resume metadata authentication')
        chain.append((name, header, frame.end, digest))
        if (epoch, segment) == (anchor['epoch'], anchor['segment']):
            break
        require(header['previous'] is not None, 'selected chain never reaches anchor')
        epoch, segment = header['previous']['epoch'], header['previous']['segment']
    chain.reverse()
    batches, prior, tail_records = {}, None, []
    retained_cost = state.cost
    tip_q = manifest['cut']
    for index, (name, header, header_end, header_hash) in enumerate(chain):
        size = image.inventory[name][2]
        if prior:
            require(header['previous'] == prior['link'] and header['start'] == prior['link']['q'], 'predecessor prefix/q binding')
            require(header['origin'] != 1 or prior['sealed'], 'rotation requires sealed predecessor')
        else:
            require(header_hash == anchor['header'], 'anchor header hash')
        offset, q, sealed, anchor_seen = header_end, header['start'], False, index != 0
        digest = hashlib.sha256()
        with image.open(name) as stream:
            digest.update(stream.read(header_end))
            while True:
                if index == 0 and offset == anchor['offset']:
                    require(q == manifest['cut'] and digest.hexdigest() == anchor['digest'], 'anchor prefix digest/q')
                    anchor_seen = True
                require(not sealed or offset == size, 'bytes after seal')
                frame = image.frame(stream, size)
                if frame is None:
                    break
                if frame == 'incomplete':
                    successor = chain[index + 1][1] if index + 1 < len(chain) else None
                    authenticated = successor is not None and successor['origin'] == 2 and successor['previous']['end'] == offset and successor['previous']['size'] == size
                    require(not sealed and (successor is None or authenticated), 'incomplete interior/sealed frame')
                    if not allow_tail:
                        incomplete('case does not authorize incomplete-tail fault profile')
                    tail_records.append({'file': name, 'offset': offset, 'size': size, 'authorized_resume': authenticated})
                    break
                record = frame.record
                require(record['identity'] == header['identity'], 'WAL frame namespace')
                if record['kind'] == 2:
                    require(record['previous_q'] == q and record['q'] == q + 1, 'noncontiguous WAL q')
                    q = record['q']
                    transaction = (*record['identity'], record['local'])
                    require(transaction not in batches, 'duplicate transaction identity')
                    if len(batches) == MAX_TRANSACTIONS:
                        incomplete('independent transaction-set budget exceeded')
                    decoded_cost = 2048
                    for change in record['changes']:
                        if change['change'] == 'put':
                            decoded_cost += State.row_cost(change['row'])
                        elif change['change'] == 'create':
                            decoded_cost += 1024 + 1024 * len(change['table']['columns'])
                        else:
                            decoded_cost += 256
                    retained_cost += max(len(frame.raw) * 8, decoded_cost)
                    if retained_cost + state.cost > MAX_STATE:
                        incomplete('retained checkpoint/batch/state budget exceeded')
                    batches[transaction] = record
                    if index != 0 or offset >= anchor['offset']:
                        require(anchor_seen and q == tip_q + 1, 'replay q discontinuity')
                        state.apply(record)
                        tip_q = q
                elif record['kind'] == 3:
                    require(record['end'] == offset and record['q'] == q and record['digest'] == digest.hexdigest(), 'seal prefix/q')
                    sealed = True
                else:
                    raise Corruption('non-batch/non-seal inside WAL')
                digest.update(frame.raw)
                offset = frame.end
            require(anchor_seen, 'anchor offset is not complete frame boundary')
            stream.seek(0)
            image.used[name] = hashlib.file_digest(stream, 'sha256').hexdigest()
        prior = {'link': {'epoch': header['identity'][2], 'segment': header['segment'], 'end': offset, 'size': size, 'q': q, 'digest': digest.hexdigest()}, 'sealed': sealed}
    require(prior is not None and tip_q == prior['link']['q'], 'replay final cut')
    return {'identity': identity, 'reference': reference, 'manifest': manifest,
            'checkpoint_state': checkpoint_state, 'state': state.snapshot(), 'q': tip_q,
            'batches': batches, 'tails': tail_records, 'closure': dict(image.used)}


def inspect_image(directory, allow_incomplete_tail=False):
    image = Image(directory)
    if 'CURRENT' not in image.inventory or 'WRITER' not in image.inventory:
        raise OracleError('REJECTED', 'INCOMPLETE_CREATE', 'CURRENT/WRITER absent; creation/deletion cause is not inferable')
    current, _ = image.metadata('CURRENT')
    writer, _ = image.metadata('WRITER')
    require(current['kind'] == 6 and writer['kind'] == 5 and current['identity'][:2] == writer['identity'][:2], 'selector namespace/kind')
    try:
        result = inspect_candidate(image, current['current'], writer, allow_incomplete_tail)
        result['fallback'] = False
    except Corruption as original:
        if current['previous'] is None:
            raise
        image.used = {name: digest for name, digest in image.used.items() if name in ('CURRENT', 'WRITER')}
        result = inspect_candidate(image, current['previous'], writer, allow_incomplete_tail)
        result['fallback'], result['current_corruption'] = True, str(original)
    image.confirm()
    return result


def database_hex(value):
    if not isinstance(value, list) or len(value) != 16:
        invalid('database must be a 16-byte JSON array')
    result = bytes(integer(n, 0, 255) for n in value).hex()
    if int(result, 16) == 0:
        invalid('zero evidence database')
    return result


def transaction_key(value):
    if not isinstance(value, dict) or set(value) != {'database', 'timeline', 'writer_epoch', 'local'}:
        invalid('transaction identity shape')
    return (database_hex(value['database']), integer(value['timeline'], 1),
            integer(value['writer_epoch'], 1, 0xffffffff), integer(value['local'], 1))


def transaction_json(key):
    return {'database': list(bytes.fromhex(key[0])), 'timeline': key[1], 'writer_epoch': key[2], 'local': key[3]}


def evidence_handle(value):
    if not isinstance(value, (list, tuple)) or len(value) != 3:
        invalid('handle must have three integers')
    handle = tuple(integer(n, 1) for n in value)
    if handle[1:] != (1, 1) or not (handle[0] >> 32 and handle[0] & 0xffffffff):
        invalid('invalid evidence handle')
    return handle


def table_identity(value, namespace):
    if not isinstance(value, dict) or set(value) != {'database', 'timeline', 'object', 'generation', 'schema_epoch'}:
        invalid('table identity shape')
    if (database_hex(value['database']), integer(value['timeline'], 1)) != namespace:
        invalid('table evidence namespace')
    return evidence_handle([value['object'], value['generation'], value['schema_epoch']])


def normalize_cell(cell):
    if not isinstance(cell, dict) or cell.get('type') not in ('null', 'bool', 'int64', 'utf8', 'binary'):
        invalid('cell shape/tag')
    kind = cell['type']
    if set(cell) != ({'type'} if kind == 'null' else {'type', 'value'}):
        invalid('cell fields')
    if kind == 'null':
        return {'type': kind}
    value = cell['value']
    if kind == 'bool' and type(value) is not bool:
        invalid('boolean cell')
    if kind == 'int64':
        integer(value, -(1 << 63), (1 << 63) - 1)
    if kind == 'utf8' and (not isinstance(value, str) or len(value.encode('utf-8')) > 65536):
        invalid('UTF-8 cell bound')
    if kind == 'binary':
        if isinstance(value, list) and len(value) <= 65536:
            value = bytes(integer(byte, 0, 255) for byte in value).hex()
        elif isinstance(value, str) and re.fullmatch(r'(?:[0-9a-f]{2}){0,65536}', value):
            pass
        else:
            invalid('binary cell bound/encoding')
    return {'type': kind, 'value': value}


def normalize_spec(spec, handle):
    if not isinstance(spec, dict) or set(spec) != {'name', 'columns'}:
        invalid('table spec fields')
    if not isinstance(spec['name'], str) or not 1 <= len(spec['name'].encode('utf-8')) <= 128 or '\0' in spec['name']:
        invalid('table name')
    columns = spec['columns']
    if not isinstance(columns, list) or not 1 <= len(columns) <= 32:
        invalid('columns bound')
    seen = set()
    for column in columns:
        if not isinstance(column, dict) or set(column) != {'name', 'data_type', 'nullable'}:
            invalid('column fields')
        name = column['name']
        if not isinstance(name, str) or not 1 <= len(name.encode('utf-8')) <= 128 or '\0' in name or name in seen:
            invalid('column name')
        if column['data_type'] not in ('bool', 'int64', 'utf8', 'binary') or type(column['nullable']) is not bool:
            invalid('column type/nullability')
        seen.add(name)
    return {'handle': handle, 'name': spec['name'], 'columns': columns}


def normalize_row(row):
    if not isinstance(row, dict) or set(row) != {'table', 'row', 'values'} or not isinstance(row['values'], list) or not 1 <= len(row['values']) <= 32:
        invalid('row evidence shape')
    number = integer(row['row'], 1)
    if not (number >> 32 and number & 0xffffffff):
        invalid('row evidence issuer')
    return {'table': evidence_handle(row['table']), 'row': number,
            'values': [normalize_cell(cell) for cell in row['values']]}


def normalize_state(value):
    if not isinstance(value, dict) or set(value) != {'tables', 'rows'} or not isinstance(value['tables'], list) or not isinstance(value['rows'], list):
        invalid('state evidence shape')
    if len(value['tables']) > 64 or len(value['rows']) > MAX_TRANSACTIONS:
        incomplete('state evidence bound')
    state = State()
    try:
        for table in value['tables']:
            if not isinstance(table, dict) or set(table) != {'handle', 'name', 'columns'}:
                invalid('table state fields')
            state.create(normalize_spec({'name': table['name'], 'columns': table['columns']}, evidence_handle(table['handle'])), 0xffffffff)
        for row in value['rows']:
            state.put(normalize_row(row), 0xffffffff, checkpoint=True)
    except Corruption as error:
        invalid('inconsistent external state: ' + str(error))
    return state.snapshot()


def canonical_changes(changes):
    # Statement ordering is checked by the separate history oracle. These are
    # already final after-images; their row ordering carries no commit meaning.
    return sorted((json.dumps(change, sort_keys=True, separators=(',', ':')) for change in changes))


@dataclass
class Attempt:
    transaction: tuple
    outcome: str
    q: object
    changes: list
    invoked: int
    returned: object
    source: str
    must_exist: bool = False
    issued_rows: tuple = ()


class History:
    def __init__(self, namespace):
        self.namespace, self.attempts, self.files = namespace, {}, []
        self.events, self.bytes, self.assertions, self.logical_cost = 0, 0, 0, 0

    def add(self, attempt):
        if attempt.transaction[:2] != self.namespace:
            invalid('transaction outside case namespace')
        old = self.attempts.get(attempt.transaction)
        if old is not None:
            if old.outcome == attempt.outcome == 'ABORT':
                return
            invalid('transaction reused by two external terminal/commit attempts')
        if len(self.attempts) >= 4096:
            incomplete('controller transaction evidence bound')
        self.attempts[attempt.transaction] = attempt
        self.assertions += 1

    def journal(self, path):
        try:
            with Path(path).open('rb') as stream:
                data = stream.read(MAX_HISTORY - self.bytes + 1)
        except OSError as error:
            incomplete('controller journal unavailable: ' + str(error))
        self.bytes += len(data)
        if self.bytes > MAX_HISTORY:
            incomplete('aggregate journal byte budget')
        self.files.append({'path': str(path), 'sha256': sha(data), 'bytes': len(data)})
        offset, sequence, last_time, run, terminal, kill_intent = 0, 0, 0, None, False, False
        pending, sent, actors, tables, rows, transactions = {}, set(), set(), {}, {}, {}
        used_requests = set()
        while offset < len(data):
            if terminal or len(data) - offset < 29:
                invalid('journal bytes after terminal or partial header')
            kind, seq, nanos, request, length = struct.unpack_from('>BQQQI', data, offset)
            offset += 29
            if seq != sequence or nanos < last_time or length > 4 * MAX_PAYLOAD + 4 or offset + length > len(data):
                invalid('journal sequence/time/payload boundary')
            sequence, last_time = sequence + 1, nanos
            self.events += 1
            if sequence > 4096 or self.events > 16384:
                incomplete('journal event budget')
            payload = data[offset:offset + length]
            offset += length
            event = self.events
            if kind in (1, 3):
                self.logical_cost += length * 16 + 2048
                if self.logical_cost > MAX_STATE:
                    incomplete('decoded journal logical budget')
                if len(payload) < 4 or struct.unpack_from('>I', payload)[0] != len(payload) - 4:
                    invalid('native envelope length')
                message = json_bytes(payload[4:])
                required = {'version', 'run', 'request', 'actor', 'command' if kind == 1 else 'observation'}
                if kind == 3:
                    required.add('passed_gate')
                if not isinstance(message, dict) or set(message) != required or type(message['version']) is not int or message['version'] != 1:
                    invalid('native envelope fields/version')
                run_id = database_hex(message['run'])
                if run is None:
                    run = run_id
                if run != run_id or integer(message['request'], 1, 1024) != request:
                    invalid('native run/request binding')
                integer(request, 1, 1024)
                actor = integer(message['actor'], 0, 31)
            elif payload:
                invalid('non-data event has payload')
            if kind == 1:
                if kill_intent or request in used_requests or actor in actors or len(pending) >= 32:
                    invalid('duplicate/inflight/after-kill invocation')
                command = message['command']
                if not isinstance(command, dict) or set(command) != {'gate', 'action'} or not isinstance(command['action'], dict):
                    invalid('command fields')
                action = command['action']
                operation = action.get('operation')
                if operation not in ('configure_engine', 'configure_fault', 'create_database', 'open_database', 'close_database', 'restore_database', 'configure_gate', 'create_table', 'drop_table', 'begin', 'lookup', 'bind_row', 'statement', 'read', 'open_scan', 'next_page', 'close_scan', 'commit', 'rollback', 'cancel', 'checkpoint', 'collect_garbage', 'backup', 'metrics', 'durability_metrics'):
                    invalid('unknown durable operation')
                pending[request] = (actor, action, event, command['gate'])
                actors.add(actor)
                used_requests.add(request)
            elif kind == 2:
                if request not in pending or request in sent or kill_intent:
                    invalid('Sent without unique live Invoke')
                sent.add(request)
            elif kind == 3:
                if request not in pending or request not in sent or kill_intent:
                    invalid('Observe without live Sent or after kill')
                expected_actor, action, invoked, gate = pending.pop(request)
                if actor != expected_actor or message['passed_gate'] != gate:
                    invalid('response actor/gate mismatch')
                actors.remove(actor)
                observation = message['observation']
                if not isinstance(observation, dict):
                    invalid('observation object')
                self.observe(action, observation, actor, invoked, event, tables, rows, transactions, f'{path}:{seq}')
            elif kind == 4:
                if request != 0 or kill_intent:
                    invalid('kill-intent identity/duplicate')
                kill_intent = True
            elif kind in (5, 6):
                if request != 0 or (kind == 5 and (kill_intent or pending)) or (kind == 6 and not kill_intent):
                    invalid('invalid process termination boundary')
                if kind == 6:
                    for actor, action, invoked, _ in pending.values():
                        operation = action['operation']
                        if operation == 'commit':
                            transaction = transactions.get((actor, action.get('transaction')))
                            if transaction is None:
                                incomplete('pending commit lacks observed Begin identity')
                            self.add(Attempt(transaction['id'], 'PENDING', None, list(transaction['changes'].values()), invoked, None, f'{path}:kill', issued_rows=tuple(transaction['inserted'])))
                        elif operation in ('create_table', 'drop_table'):
                            incomplete('pending DDL lacks an externally issued transaction identity')
                terminal = True
            else:
                invalid('unknown journal event kind')
        if not terminal:
            incomplete('journal has no observed normal-finish or owned kill completion')

    def observe(self, action, observation, actor, invoked, returned, tables, rows, transactions, source):
        operation, result = action['operation'], observation.get('result')
        alias = (actor, action.get('transaction'))
        current = transactions.get(alias)
        if result == 'error':
            failure = observation.get('failure')
            if not isinstance(failure, dict) or failure.get('domain') not in ('transaction', 'storage'):
                invalid('typed failure shape')
            returned_id = failure.get('transaction')
            identity = transaction_key(returned_id) if returned_id is not None else (current['id'] if current else None)
            if current and identity != current['id']:
                invalid('failure transaction differs from observed Begin')
            outcome = failure.get('outcome') if failure['domain'] == 'storage' else failure.get('effect')
            if outcome not in ('active', 'not_attempted', 'aborted', 'unknown', 'committed'):
                invalid('failure outcome')
            if outcome == 'active':
                return  # Failed statement contributes no after-image.
            if identity is None:
                if operation in ('commit', 'create_table', 'drop_table') and outcome in ('unknown', 'committed'):
                    incomplete('uncertain attempt lacks explicit transaction identity')
                return
            changes = list(current['changes'].values()) if current else ([] if outcome in ('not_attempted', 'aborted') else self.ddl_changes(action, tables, None))
            self.add(Attempt(identity, 'ABORT' if outcome in ('not_attempted', 'aborted') else 'UNKNOWN', None,
                             changes, invoked, returned, source, must_exist=outcome == 'committed',
                             issued_rows=tuple(current['inserted']) if current else ()))
            transactions.pop(alias, None)
            return
        if operation == 'begin' and result == 'begun':
            if alias in transactions:
                invalid('duplicate active transaction alias')
            transactions[alias] = {'id': transaction_key(observation['identity']), 'changes': {}, 'inserted': set(), 'mode': action.get('mode')}
        elif operation in ('create_table', 'lookup') and result == 'table':
            handle = table_identity(observation['identity'], self.namespace)
            table = normalize_spec(observation['spec'], handle)
            tables[action['table']] = handle
            if operation == 'create_table':
                requested = normalize_spec(action['spec'], handle)
                if table != requested:
                    reject('CreateTable response schema differs from request')
                receipt = observation.get('commit')
                if receipt is None:
                    incomplete('DDL success predates actual transaction/q receipts')
                self.commit(receipt, [{'change': 'create', 'table': table}], invoked, returned, source)
        elif operation == 'statement' and result == 'statement':
            if current is None:
                invalid('statement without observed Begin')
            if current['mode'] != 'read_write':
                reject('read-only transaction accepted a statement')
            mutations, issued = action.get('mutations'), observation.get('inserted')
            if not isinstance(mutations, list) or len(mutations) > 256 or not isinstance(issued, list):
                invalid('statement mutation/issued-row bounds')
            issued_map = {}
            for value in issued:
                if value['alias'] in issued_map:
                    invalid('duplicate issued row alias')
                item = value['identity']
                issued_map[value['alias']] = (table_identity(item['table'], self.namespace), integer(item['row'], 1))
            expected_issued = []
            for mutation in mutations:
                table = tables.get(mutation.get('table'))
                if table is None:
                    invalid('successful statement has unbound table alias')
                row_alias, kind = mutation.get('row'), mutation.get('mutation')
                if kind == 'insert':
                    expected_issued.append(row_alias)
                    if row_alias not in issued_map or issued_map[row_alias][0] != table or row_alias in rows:
                        invalid('insert result binding')
                    rows[row_alias] = issued_map[row_alias]
                    current['inserted'].add(issued_map[row_alias][1])
                if row_alias not in rows or rows[row_alias][0] != table:
                    invalid('mutation actual row identity is unavailable')
                row_id = rows[row_alias][1]
                if kind in ('insert', 'replace'):
                    values = mutation.get('values')
                    if not isinstance(values, list) or not 1 <= len(values) <= 32:
                        invalid('statement cell bound')
                    current['changes'][row_id] = {'change': 'put', 'row': {'table': table, 'row': row_id, 'values': [normalize_cell(c) for c in values]}}
                elif kind == 'delete':
                    if row_id in current['inserted']:
                        current['changes'].pop(row_id, None)
                    else:
                        current['changes'][row_id] = {'change': 'delete', 'table': table, 'row': row_id}
                else:
                    invalid('statement mutation kind')
            if set(expected_issued) != set(issued_map) or len(expected_issued) != len(issued_map):
                invalid('issued rows do not match inserts')
        elif operation in ('commit', 'drop_table') and result == 'committed':
            if operation == 'commit':
                if current is None or transaction_key(observation['transaction']) != current['id']:
                    invalid('commit receipt lacks matching Begin')
                changes = list(current['changes'].values())
                transactions.pop(alias)
            else:
                changes = self.ddl_changes(action, tables, None)
            self.commit(observation, changes, invoked, returned, source,
                        tuple(current['inserted']) if current else ())
        elif operation == 'rollback' and result == 'done':
            if current is None:
                invalid('rollback without observed Begin')
            self.add(Attempt(current['id'], 'ABORT', None, [], invoked, returned, source))
            transactions.pop(alias)
        elif operation == 'bind_row' and result == 'done':
            identity = action['identity']
            if action['row'] in rows:
                invalid('duplicate bound row alias')
            rows[action['row']] = (table_identity(identity['table'], self.namespace), integer(identity['row'], 1))
        elif operation in ('create_database', 'open_database', 'restore_database') and result == 'ready':
            if (database_hex(observation['database']), integer(observation['timeline'], 1)) != self.namespace:
                invalid('Ready namespace differs from case')
        elif operation in ('configure_fault', 'configure_engine', 'configure_gate'):
            if observation != {'result': 'done'}:
                invalid('control operation must return Done')
        elif operation in ('begin', 'statement', 'commit', 'rollback', 'create_table', 'drop_table'):
            invalid('unexpected successful transaction/DDL observation')

    def ddl_changes(self, action, tables, returned_table):
        if action['operation'] == 'drop_table':
            handle = tables.get(action.get('table'))
            if handle is None:
                incomplete('DropTable lacks prior actual TableIdentity binding')
            return [{'change': 'drop', 'table': handle}]
        if action['operation'] == 'create_table':
            incomplete('failed/pending CreateTable lacks returned table identity for payload proof')
        return []

    def commit(self, receipt, changes, invoked, returned, source, issued_rows=()):
        if not isinstance(receipt, dict) or not {'transaction', 'position', 'wrote'} <= set(receipt) or type(receipt['wrote']) is not bool:
            invalid('commit receipt fields')
        identity = transaction_key(receipt['transaction'])
        q = integer(receipt['position'])
        if not receipt['wrote']:
            if changes:
                incomplete('net-zero write requires an independent Begin-snapshot diff')
            # A no-op/read-only success is not a durable write ACK.
            return
        if not changes or q == 0:
            reject('write ACK has no final changes or positive q')
        self.add(Attempt(identity, 'ACK', q, changes, invoked, returned, source, issued_rows=issued_rows))


def verify_history(plan, history, expectation):
    if history.assertions == 0:
        invalid('empty durable assertion history')
    batches = plan['batches']
    committed = {key: batch['q'] for key, batch in batches.items()}
    cut = plan['manifest']['cut']
    if cut > MAX_TRANSACTIONS:
        incomplete('checkpoint prefix proof exceeds independent transaction bound')
    evidence = expectation.get('checkpoint_evidence', [])
    if not isinstance(evidence, list) or len(evidence) > MAX_FILES:
        invalid('checkpoint evidence list')
    matching = [item for item in evidence if isinstance(item, dict) and item.get('generation') == plan['reference']['generation'] and item.get('digest') == plan['reference']['digest']]
    if len(matching) > 1:
        invalid('duplicate selected checkpoint evidence')
    selected = matching[0] if matching else None
    if selected is not None:
        if set(selected) != {'generation', 'digest', 'cut', 'state', 'prefix'} or selected['cut'] != cut:
            invalid('checkpoint evidence binding')
        if normalize_state(selected['state']) != plan['checkpoint_state']:
            reject('selected checkpoint differs from independently retained expected state')
        prefix = selected['prefix']
        if not isinstance(prefix, list) or len(prefix) > MAX_TRANSACTIONS:
            incomplete('checkpoint membership proof bound')
        witness_rows, prefix_ids = set(), set()
        for item in prefix:
            if not isinstance(item, dict) or set(item) != {'transaction', 'q', 'witness'}:
                invalid('checkpoint membership proof fields')
            key, q = transaction_key(item['transaction']), integer(item['q'], 1, cut)
            if key in prefix_ids:
                invalid('duplicate prefix transaction proof')
            prefix_ids.add(key)
            attempt = history.attempts.get(key)
            if attempt is None:
                incomplete('pruned transaction lacks controller history')
            if key in committed:
                if committed[key] != q:
                    reject('checkpoint proof contradicts retained WAL q')
                continue
            row = normalize_row(item['witness'])
            witness_key = row['table'], row['row']
            if witness_key in witness_rows:
                invalid('one row reused as two transaction membership witnesses')
            witness_rows.add(witness_key)
            if {'change': 'put', 'row': row} not in attempt.changes:
                invalid('prefix witness was not written by the external transaction')
            if row['row'] not in attempt.issued_rows:
                incomplete('prefix witness is not a uniquely issued Insert of its transaction')
            for other_key, other in history.attempts.items():
                if other_key == key:
                    continue
                for change in other.changes:
                    number = change['row']['row'] if change['change'] == 'put' else (change['row'] if change['change'] == 'delete' else None)
                    if number == row['row']:
                        incomplete('another transaction modified the claimed distinct witness')
            if row not in plan['checkpoint_state']['rows']:
                reject('pruned transaction witness missing from checkpoint')
            committed[key] = q
    by_q = {}
    for key, q in committed.items():
        if q in by_q and by_q[q] != key:
            reject('two transactions claim the same committed q')
        by_q[q] = key
    if any(q not in by_q for q in range(1, cut + 1)):
        incomplete('pruned checkpoint prefix lacks independent transaction membership proof')
    if cut == 0:
        if plan['checkpoint_state'] != {'tables': [], 'rows': []}:
            reject('genesis checkpoint contains uncommitted state')
    elif selected is None:
        # Every prefix transaction still exists in authenticated WAL. Rebuild its
        # expected checkpoint from external command after-images in decoded q
        # order, rather than trusting the checkpoint's final rows alone.
        expected = State()
        for q in range(1, cut + 1):
            key = by_q[q]
            attempt = history.attempts.get(key)
            if attempt is None:
                incomplete('checkpoint transaction lacks original command history')
            expected.apply({'identity': key[:3], 'changes': attempt.changes})
        if expected.snapshot() != plan['checkpoint_state']:
            reject('checkpoint differs from independent controller-command replay')
    for key, attempt in history.attempts.items():
        present = key in committed
        if attempt.outcome == 'ACK' and not present:
            reject('acknowledged transaction is absent from selected commit set')
        if attempt.outcome == 'ABORT' and present:
            reject('explicitly aborted transaction appears in selected commit set')
        if attempt.must_exist and not present:
            reject('known committed error receipt lost its durable decision')
        if present:
            if attempt.q is not None and committed[key] != attempt.q:
                reject('external q witness differs from independent decoded q')
            if key in batches and canonical_changes(attempt.changes) != canonical_changes(batches[key]['changes']):
                reject('whole commit after-images differ from external successful statements')
    unexplained = set(committed) - set(history.attempts)
    if unexplained:
        if expectation.get('history_complete') is True:
            reject('selected transaction has no ACK, UNKNOWN or pending attempt')
        incomplete('journal does not cover selected transaction set')
    if expectation.get('history_complete') is not True:
        incomplete('case does not assert complete external write-attempt coverage')
    writes = [attempt for key, attempt in history.attempts.items() if key in committed]
    for earlier in writes:
        if earlier.returned is None:
            continue
        for later in writes:
            if earlier.returned < later.invoked and committed[earlier.transaction] >= committed[later.transaction]:
                reject('commit q order violates external response-before-invocation order')
    # Prefix proof plus every later decoded batch must explain precisely one
    # continuous sequence. Numeric TxnId.local need NOT follow commit ordering.
    if len(by_q) != plan['q'] or any(q not in by_q for q in range(1, plan['q'] + 1)):
        incomplete('complete q-prefix membership evidence unavailable')
    return [{'transaction': transaction_json(key), 'q': q,
             'external_outcome': history.attempts[key].outcome,
             'membership': 'retained_wal' if key in batches else 'checkpoint_witness'}
            for q, key in sorted(by_q.items())]


def check(directory, histories, expectation, recovered):
    """Return a structured bounded-case result. Never run or modify the engine."""
    result = {'schema': 1, 'verdict': 'INCOMPLETE', 'oracle': 'independent-python-dev2-v1',
              'coverage': ['selected_file_bytes', 'checkpoint_and_replay_state', 'whole_transaction_membership', 'external_q_witnesses', 'response_before_invocation_order'],
              'not_qualified': ['power_loss_hardware', 'full_serializable_read_history', 'performance']}
    history = None
    try:
        if not isinstance(expectation, dict) or expectation.get('version') != 1 or expectation.get('expected_outcome') not in ('RECOVER_OK', 'REJECT_CORRUPTION', 'REJECT_INCOMPLETE_CREATE'):
            invalid('expectation version/outcome')
        if expectation.get('fault_model') not in ('process_kill', 'deterministic_io', 'qualified_power_loss') or type(expectation.get('allow_incomplete_tail')) is not bool:
            invalid('frozen fault model/tail policy')
        namespace = database_hex(expectation.get('database')), integer(expectation.get('timeline'), 1)
        result.update(fault_model=expectation['fault_model'], expected_outcome=expectation['expected_outcome'])
        history = History(namespace)
        for path in histories:
            history.journal(path)
        if not isinstance(recovered, dict):
            incomplete('actual externally observed reopen result missing')
        try:
            plan = inspect_image(directory, expectation['allow_incomplete_tail'])
        except OracleError as error:
            expected_error = {'CORRUPTION': 'REJECT_CORRUPTION', 'INCOMPLETE_CREATE': 'REJECT_INCOMPLETE_CREATE'}.get(error.code)
            if expected_error is None or expectation['expected_outcome'] != expected_error:
                raise
            if recovered != {'status': 'REJECTED', 'normal_queries': 0, 'new_acks': 0}:
                reject('corrupt image was not rejected before normal queries/new ACKs')
            result.update(verdict='ACCEPTED', independent_outcome=expected_error, rejection=str(error))
            result['coverage'] = ['selected_file_rejection', 'external_reopen_rejected_before_queries_or_new_acks']
            return result
        if expectation['expected_outcome'] != 'RECOVER_OK':
            reject('predeclared corruption vector was independently recoverable')
        if plan['identity'][:2] != namespace:
            reject('image namespace differs from external case')
        if recovered.get('status') != 'READY':
            reject('valid selected image did not reopen successfully')
        if set(recovered) != {'status', 'q', 'state'} or integer(recovered['q']) != plan['q']:
            reject('actual reopened q differs from independent replay')
        actual = normalize_state(recovered['state'])
        if actual != plan['state']:
            reject('actual reopened state differs from independent replay')
        if 'expected_state' in expectation and normalize_state(expectation['expected_state']) != plan['state']:
            reject('literal expected state differs from independent replay')
        membership = verify_history(plan, history, expectation)
        result.update(verdict='ACCEPTED', independent_outcome='RECOVER_OK', fallback=plan['fallback'],
                      checkpoint_generation=plan['reference']['generation'], checkpoint_cut=plan['manifest']['cut'],
                      replay_q=plan['q'], commits=membership, tails=plan['tails'], selected_files=plan['closure'])
    except OracleError as error:
        result.update(verdict=error.verdict, code=error.code, detail=str(error))
    except (OSError, MemoryError) as error:
        result.update(verdict='INCOMPLETE', code='RESOURCE_OR_IO', detail=str(error))
    except (KeyError, TypeError, ValueError, UnicodeError, OverflowError, RecursionError) as error:
        result.update(verdict='INVALID', code='EVIDENCE_SHAPE', detail=str(error))
    finally:
        if history is not None:
            result['journals'] = history.files
            result['journal_events'] = history.events
    return result


def bounded_json(path):
    with Path(path).open('rb') as stream:
        data = stream.read(MAX_PROOF + 1)
    if len(data) > MAX_PROOF:
        incomplete('JSON evidence exceeds 16 MiB')
    return json_bytes(data), sha(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--directory', required=True)
    parser.add_argument('--history', action='append', default=[], required=True)
    parser.add_argument('--expect', required=True)
    parser.add_argument('--expect-sha256', required=True, help='controller-retained pre-campaign expectation digest')
    parser.add_argument('--recovered', required=True, help='separate externally captured actual reopen/scan observation')
    parser.add_argument('--output')
    arguments = parser.parse_args()
    try:
        expectation, expected_digest = bounded_json(arguments.expect)
        if not re.fullmatch('[0-9a-f]{64}', arguments.expect_sha256) or expected_digest != arguments.expect_sha256:
            invalid('expectation digest differs from predeclared controller receipt')
        recovered, recovered_digest = bounded_json(arguments.recovered)
        result = check(arguments.directory, arguments.history, expectation, recovered)
        result.update(expectation_sha256=expected_digest, recovered_observation_sha256=recovered_digest,
                      oracle_source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    except OracleError as error:
        result = {'schema': 1, 'verdict': error.verdict, 'code': error.code, 'detail': str(error)}
    except OSError as error:
        result = {'schema': 1, 'verdict': 'INCOMPLETE', 'code': 'EVIDENCE_IO', 'detail': str(error)}
    rendered = json.dumps(result, sort_keys=True, indent=2) + '\n'
    if arguments.output:
        # Exclusive output creation preserves earlier failure receipts.
        with Path(arguments.output).open('x', encoding='utf-8') as stream:
            stream.write(rendered)
    print(rendered, end='')
    return {'ACCEPTED': 0, 'REJECTED': 1, 'INVALID': 2, 'INCOMPLETE': 3}[result['verdict']]


if __name__ == '__main__':
    sys.exit(main())
