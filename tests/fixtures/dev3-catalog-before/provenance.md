# Catalog before independent raw provenance

Input preparation only; product/build/tests were not executed.

The first generator invocation failed before any output write: `IndexError: list index out of range` in eager metadata-default evaluation. Its exit status was 1; no script duration receipt existed. The corrected invocation below generated the first raw/manifest files. This failure is retained in manifest preparation_history and is not classified as a product failure or PASS.

The generator uses only Python struct/hashlib and frozen offset/definition rules. No product parser, encoder, or output is imported. All files use create_new; existing files are never overwritten. The exact generation output and elapsed time are in manifest.json.

N05/N06 deliberately have P03 bytes and different caller expectations. N38 changes only payload, leaving both digest fields unchanged; N40/N50 deliberately recompute the digest. Referenced U04/U08 remain their original source inputs, not newly executed lower tests.

Generator SHA256: `d5faff5ba650901709b83363d012a78c99e9fd71c1590ca7776af6c994e7387b`.

Spec SHA256: `58d0b314df8477a924587e7f0ec4e62a28be24aeee6fe8dc5a4fbcf22dde6444`.

Generation: `{"elapsed_seconds": 0.006903973000589758, "kind": "independent_fixture_generation_only", "mandatory": 123, "raw_bytes": 55227, "raw_files": 68, "s5": 9534, "started_utc": "2026-10-24T08:29:20.269306+00:00"}`

Metadata revision 2 copies U04/U08 with their original SHA into tracked references/ (two referenced historical raw files, separate from new raw68). Runtime reads only tracked inputs. The original generator manifest, provenance, and source remain byte-exact in artifacts/dev3/first-row-transaction-20261024/input-preparation/catalog-before-generation-r1-*. Regeneration requires the frozen input/spec documents and the original two raw sources, and refuses to overwrite existing raw/manifest/reference files. The generation receipt is original; metadata revision is not a second fixture-generation PASS.

Current generator SHA256 (tracked-reference support, not reexecuted): `04618822fa028af4a0ecd9ec895c958d4ad74e57eab08fdd7670f3b634049708`. Original executed generator SHA remains generation.generator_sha256 in the manifest.
