#!/usr/bin/env python3
"""Independent literal DEV3 read fixtures; no production codec or DB creation."""
import argparse
import hashlib
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parent.parent
DIRECTORY = ROOT / 'crates/engine/tests/fixtures'
FROZEN = {
    'profile': (4096, '985651bdbfde061063f76265665c2aff331c9b01486e0be4de60223d6ec327d8'),
    'selector': (4096, '4b383d1f2ad756630421555efcdef18ea40bba314b07a42333099ef75bf7fbae'),
    'root': (416, '13e967b9db3b3e29d0bcf2bc6698de6f06b63e9a9b02e91a71bc5a624ba2de69'),
    'chunk0': (6304, 'e2a132d9659f20a874339bf29328a9b529af734bfbdd72cae510601e744c81ed'),
    'chunk1': (256, '0f186336d4102cbcc31474f3e3188f55cc3f4ae18c985aa0486e9909baa7e7d9'),
}


def put(buffer, offset, width, value):
    struct.pack_into({2: '<H', 4: '<I', 8: '<Q'}[width], buffer, offset, value)


def seal(buffer):
    buffer[-32:] = hashlib.sha256(buffer[:-32]).digest()
    return bytes(buffer)


def control(kind, index, count, payload, generation=23):
    buffer = bytearray(160 + len(payload))
    buffer[:4] = b'XDC1'
    buffer[6] = kind
    buffer[16:32] = b'\x11' * 16
    for offset, width, value in [(4, 2, 1), (8, 2, 128), (12, 4, len(buffer)),
                                  (32, 8, 17), (40, 8, generation), (48, 4, index),
                                  (52, 4, count), (56, 4, len(payload))]:
        put(buffer, offset, width, value)
    buffer[128:-32] = payload
    return seal(buffer)


def profile():
    payload = bytearray(3936)
    for offset, value in [(8, 8 << 30), (16, 1 << 30), (24, 256 << 20),
                          (32, 16 << 20), (40, 4 << 20), (48, 8 << 20),
                          (56, 1 << 20), (64, 8 << 20), (72, 64 << 20),
                          (80, 16 << 20), (88, 4 << 20), (96, 12 << 20),
                          (104, 4 << 20), (160, 16384 * 129), (168, 1 << 20)]:
        put(payload, offset, 8, value)
    for offset, value in [(0, 1), (112, 32), (116, 256), (120, 1 << 20),
                          (124, 32), (128, 128), (136, 16384), (148, 8192),
                          (156, 16384), (176, 2), (180, 4096), (184, 1 << 20)]:
        put(payload, offset, 4, value)
    for offset, value in [(132, 16), (134, 8), (140, 64), (142, 64),
                          (144, 1010), (152, 64)]:
        put(payload, offset, 2, value)
    return control(1, 0, 0, payload, generation=19)


def reference(kind, index, content):
    buffer = bytearray(64)
    buffer[0] = kind
    put(buffer, 4, 4, index)
    put(buffer, 8, 8, 23)
    put(buffer, 16, 4, len(content))
    buffer[32:] = hashlib.sha256(content).digest()
    return bytes(buffer)


def entry(number):
    file = (1 << 32) + 2 + number
    buffer = bytearray(96)
    for offset, value in [(0, file), (8, 1 << 20), (16, 5), (24, 6),
                          (32, file), (40, 0), (48, 1)]:
        put(buffer, offset, 8, value)
    buffer[56:80] = f'd3-data-{file:016x}'.encode('ascii')
    buffer[88] = 1
    return bytes(buffer)


def vectors():
    profile_bytes = profile()
    chunks = [control(3, 0, 64, b''.join(entry(n) for n in range(64))),
              control(3, 1, 1, entry(64))]
    payload = bytearray(64 + 96 * 2)
    for offset, width, value in [(0, 4, 65), (4, 4, 2), (8, 8, 65 << 20),
                                  (16, 8, (1 << 32) + 2), (24, 8, (1 << 32) + 66),
                                  (32, 2, 64), (34, 2, 96)]:
        put(payload, offset, width, value)
    for index, (first, count) in enumerate([(2, 64), (66, 1)]):
        offset = 64 + index * 96
        payload[offset:offset + 64] = reference(3, index, chunks[index])
        put(payload, offset + 64, 2, count)
        put(payload, offset + 72, 8, (1 << 32) + first)
        put(payload, offset + 80, 8, (1 << 32) + first + count - 1)
    root = control(2, 0, 2, payload)
    selector = bytearray(4096)
    selector[:4] = b'XCS1'
    selector[6] = 1
    selector[8:24] = b'\x11' * 16
    for offset, width, value in [(4, 2, 1), (24, 8, 17), (32, 8, 29), (40, 8, 19)]:
        put(selector, offset, width, value)
    selector[48:80] = hashlib.sha256(profile_bytes).digest()
    selector[80:144] = reference(2, 0, root)
    return {'profile': profile_bytes, 'selector': seal(selector), 'root': root,
            'chunk0': chunks[0], 'chunk1': chunks[1]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='write exactly the five literal fixtures')
    args = parser.parse_args()
    generated = vectors()
    if args.write and any((DIRECTORY / f'dev3-io-{name}.bin').exists() for name in generated):
        raise SystemExit('refusing to overwrite existing literal fixtures')
    for name, content in generated.items():
        actual = (len(content), hashlib.sha256(content).hexdigest())
        if actual != FROZEN[name]:
            raise SystemExit(f'generator differs from frozen contract literal: {name}')
        path = DIRECTORY / f'dev3-io-{name}.bin'
        if args.write:
            path.write_bytes(content)
        if path.read_bytes() != content:
            raise SystemExit(f'literal fixture mismatch: {path}')
        print(f'{path.name} {actual[0]} {actual[1]}')


if __name__ == '__main__':
    main()
