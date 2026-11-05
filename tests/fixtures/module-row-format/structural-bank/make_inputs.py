"""Independent XOD1 literals; no product executable or descriptor encoder.

Root/Child are Finished Structural owner70/epoch1 under the first pinned
Base4 profile. These bytes are observations, not selected-source authority.
"""
from hashlib import sha256
from pathlib import Path
from struct import pack_into, unpack_from

ROOT = Path(__file__).resolve().parent
PROFILE = (ROOT.parent / "envelopes/profiles.bin").read_bytes()[:4096]


def descriptor(wal_bytes):
    raw = bytearray(1536)
    raw[:4] = b"XOD1"
    pack_into("<H", raw, 4, 1)
    raw[6] = 6
    pack_into("<I", raw, 8, 1536)
    raw[16] = 2
    raw[24:40] = PROFILE[16:32]
    raw[40:48] = PROFILE[32:40]
    raw[48] = 2
    for at, value in ((56, 70), (64, 1), (72, 70), (88, 70),
                      (104, unpack_from("<Q", PROFILE, 40)[0]),
                      (360, 1), (368, 1), (376, 1), (544, 70),
                      (608, wal_bytes), (624, 1)):
        pack_into("<Q", raw, at, value)
    raw[96] = 1
    pack_into("<I", raw, 112, 4096)
    raw[128:160] = sha256(PROFILE).digest()
    raw[352] = 2
    raw[353] = 4
    raw[448:480] = sha256(b"").digest()
    raw[560:592] = sha256(b"").digest()
    for index in range(5):
        raw[640 + 128 * index] = index + 1
    raw[-32:] = sha256(raw[:-32]).digest()
    return raw


def base(parent, raw):
    b = bytearray((ROOT.parent / "base4-closure" / parent).read_bytes())
    for at, value in ((96, 70), (216, 70), (248, 70)):
        pack_into("<Q", b, at, value)
    b[4096 + 33 * 1536:4096 + 34 * 1536] = raw
    # XOI current cut changes, while Finished Structural owns no current item.
    inv = 98304
    length = unpack_from("<I", b, inv + 12)[0]
    pack_into("<Q", b, inv + 48, 70)
    b[inv + length - 32:inv + length] = sha256(b[inv:inv + length - 32]).digest()
    for index in range(5):
        at = 1280 + 64 * index
        offset, length = unpack_from("<II", b, at + 4)
        b[at + 32:at + 64] = sha256(b[offset:offset + length]).digest()
    b[-32:] = sha256(b[:-32]).digest()
    return b


if __name__ == "__main__":
    for name, length in (("root", 84744), ("child", 84640)):
        raw = descriptor(length)
        (ROOT / (name + ".xod")).write_bytes(raw)
        (ROOT / (name + ".xrb")).write_bytes(base("genesis.xrb", raw))
        (ROOT / ("live-" + name + ".xrb")).write_bytes(base("live.xrb", raw))
    files = sorted((*ROOT.glob("*.xod"), *ROOT.glob("*.xrb")))
    (ROOT / "inputs.sha256").write_text("".join(
        f"{sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in files))
