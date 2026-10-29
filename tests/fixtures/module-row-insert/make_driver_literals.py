"""Independent MCA00-D literals: canonical rows (dev3-row-format: u32 LE column
count, then tag 0 null / 2 int64 LE / 3 utf8 u32-LE length + bytes), their
SHA-256, and scope3 frame lengths from dev3-module-consumers 6.6
(Forward 91304+C+128h, CLR 91096+C+128h, C = 180 + L). stdlib only."""
import hashlib
import struct


def int64(v):
    return b"\x02" + struct.pack("<q", v)


def utf8(s):
    b = s.encode("utf-8")
    return b"\x03" + struct.pack("<I", len(b)) + b


rows = {
    "R1": [int64(42), utf8("xadb")],
    "R2": [int64(-1), utf8("")],
    "R3": [int64(7), utf8("한")],
}
for name, values in rows.items():
    body = struct.pack("<I", len(values)) + b"".join(values)
    c = 180 + len(body)
    print(name, "L=%d" % len(body), "C=%d" % c)
    print("  bytes", ", ".join("0x%02x" % x for x in body))
    print("  sha256", ", ".join("0x%02x" % x for x in hashlib.sha256(body).digest()))
    for h in (0, 1):
        print("  forward h=%d" % h, 91304 + c + 128 * h, " clr h=%d" % h, 91096 + c + 128 * h)
