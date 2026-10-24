# Independent Free undo/page literals

Prepared by `/root/dev3_independent_review` at base `cd8b8762398ae6489a639348d14594fa35c20629` for D3-UNDO-FREE-PAGE-001. These are inputs, not product execution results. No Rust/product encoder, product output, repository generator or verifier was executed. The one-time construction used Python standard-library byte arrays, `struct.pack_into` and `hashlib.sha256`, directly from the published offset tables in `dev3-undo-format.md` §3 and `dev3-page-format.md` F01. The manifest records the exact base-contract provenance and raw hashes.

The four newly constructed literals are `free-g1.bin`, `free-g6.bin`, `free-gmax.bin` and `initial-page.bin`. Each Free record begins as432 zero bytes: magic0..4=`XUR1`, version u16 at4=1, generation u64 at8=1/6/MAX. State6 and reserved7 stay zero; every byte16..432 remains zero. There is no target, creator, sequence or cell checksum. Generation MAX is a valid stored Free representation, not permission to reuse it.

The page begins as16,384 zero bytes: `XDP1` at0, format u16 at4=1, kind6=8, level7=0, DB8..24=`31` repeated16 times. Its six u64 values at24..72 are FileId0x100000001, page number2, incarnation1, owner3, owner generation1 and LSN1234. Three u16 values at72/74/76 are count1/lower132/upper15920. Slot0's directory at128/130 is offset15920/length432. The generation1 Free bytes occupy15920..16352. The final32 bytes are SHA-256 of bytes0..16352. All header reserve bytes and unused page space remain zero. Incarnation1 and LSN1234 here are representation inputs, not allocator-issued or durable claims.

Equivalent construction recipe, shown for reproducibility rather than as an executed product program:

```python
free = bytearray(432)
free[:4] = b"XUR1"
struct.pack_into("<H", free, 4, 1)
struct.pack_into("<Q", free, 8, generation)

page = bytearray(16384)
page[:4] = b"XDP1"
struct.pack_into("<HBB", page, 4, 1, 8, 0)
page[8:24] = bytes([0x31]) * 16
struct.pack_into("<QQQQQQ", page, 24, 0x100000001, 2, 1, 3, 1, 1234)
struct.pack_into("<HHH", page, 72, 1, 132, 15920)
struct.pack_into("<HH", page, 128, 15920, 432)
page[15920:16352] = free_generation_1
page[16352:] = hashlib.sha256(page[:16352]).digest()
```

Two portable compatibility copies retain original preimplementation identity:

| Local file | Original independent input | SHA-256 |
| --- | --- | --- |
| existing-U02-live.bin | artifacts/dev3/undo-format-preimplementation/U02.bin | a093939a406f521e17f54d4f54e9cbee936c85a4dc849d0955df1511cfb38c5a |
| existing-U13-releasing.bin | artifacts/dev3/undo-format-preimplementation/U13.bin | 04628be9babed93325eed27f0f77f0e183d60679de90e6813f2f26423365f6da |

Both copies are432 bytes and were checked against the published original hashes before copying. U02 is Live Row Missing/incoming1/two Never edges. U13 is Releasing RowValue/Chunks16,061/cursor3/released0/next40. Their original context remains DB bytes1..16/timeline17, root reference file1:2/page30/incarnation4/slot0/generation6, target Row owner5/generation6/entity258, creator epoch18/local19/status-generation20, sequence100 and mutation21. These copies support only focused compatibility of newly shared header rules. They do not authorize rerunning the old full undo suites or claim a new Live/rollback result. The independently constructed Free generation6 hash also matches the original U01 identity.

No fixture supplies membership, a pin, an immutable role, reservation authority, a WAL requirement, an installed page or recovery permission. The new caller must provide those independently when the mutable mixed consumer is implemented.
