# Pure CREATE transition harness handoff

2026-10-25 · independent input author `/root/dev3_review` · compiler/product/native execution0. Parent owns module declaration and coherent export/build/execution.

- Scope:72 exact functions `storage::semantic_catalog::transition_tests::{g01,c01..c20,r01..r11,k01..k12,p01..p02,n01..n26}`.
- Positive46 each compares actual produce(original before, Spec) against whole independent after pages and full golden frame, then observe(original before, independent golden) against the same whole after. Forward observation uses golden `decode_layout().forward_table()` as `CatalogTableInput::View`, not produced bytes.
- Before/context negatives21 require both produce and observe rejection. Frame negatives5(N16–19/N24) require observe rejection only. No checksum-only fallback, fake grants, ignored placeholders or derived expected outputs.
- Markers: `CREATE_TRANSITION BEGIN ID` / `CREATE_TRANSITION PASS ID scope=pure-transition authority=NONE`; require exact libtest result too. Expected total API calls139 if every selected input completes. Existing108 leaf must not be rerun.
- Fixtures:324 blobs /8,857,480B, largest158,440B; content addressed, write-once generation1. Manifest SHA `6da4346527580f3988deb480546fd6e4ee6aa57201a4bedbb7dec60c4f1509eb`.
- Generator SHA `3716f9d63f099051363bebeeaf0ebe4586ccd0984aa91735a7de82eba7d23b7e`; plan SHA `7730d42bae88b0ce3ae5fcea96162ae5d8d309050da6820ea09d5bc07572035e`.
- Harness SHA `2d36dd076e60932e2b9220fb85393ab722273136fe391fc0af4007bdac800ff7`. The harness pins all7 metadata files, streams all index members once, verifies directory inventory, then retains only each selected case's raw. Largest unique case raw472,808B. Original fixture+metadata+8-page output+163840-frame+32768 allowance is checked below4MiB; numerical product source F05 is separate.
- Bounds: one worker, case10s, selected aggregate120s, output8MiB, retained log/receipt64MiB, corpus20MiB. Harness asserts elapsed case time; parent controller must provide hard process/aggregate caps.
- Own single-file rustfmt completed; no compiler or test runner executed by this author.

Read-only independent input auditor `work/check_catalog_transition_literals.py` SHA `31fb0712fbae7dc37ae3d8eb8fe4292cb83a983297ae6af23d59473c51784178` ran once. Result `work/catalog-transition-input-audit-r0.json` SHA `08caa46907ce747b7226adcef978505196d35043cbcabb291e239ed3f9076e19` is PASS_INPUT_BYTES_ONLY: all324 input lengths/hashes/page envelopes,46 golden frame native/redo links, exact per-owner charge/progress, before/after Root+64Job census, state page links and required negative witnesses. This is not product semantic acceptance. Generator and auditor had no failure/retry; raw was not regenerated.
