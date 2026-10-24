# G6 N=0 독립 literal 소스

2026-10-24 · **SOURCE_ONLY / NOT_GENERATED / NOT_RUN**.

`generator.py`는 stdlib `struct`/`hashlib`와 명시 offset으로 새 XQR1/XQJ1/XOD1/XSG1/XHG1/XRO1/XHO1/XWF1을 구성한다. 제품 encoder/decoder/binary와 이전 fixture generator를 import하거나 실행하지 않는다. 제품 출력은 기대 bytes의 입력이 아니다. 원 wire 후보 SHA `0561d2c1fc40daeacf65d530b245932a8daf4b213e9476ca1df72a48023e3ae2`의 source 작성 수용과 실제 fixture 생성 승인은 구별한다.

원 `../dev3-plan4-genesis/manifest.json` SHA `82880ea05480b9779bb2d5764c88f6423d2c74da1d4e68e8bf2bb0092e8eba22`, `raw-index.tsv` SHA `0c057920532b389a678c4fc1ea74e82d86a4d457dbd90b8d88ff16cfedd09e13`를 먼저 검산한다. 선택한 P00..P08와 해당 profile 하나의 exact bytes/SHA만 참조한다. 독립 구성한 genesis Root/Job page가 원 raw와 같은지도 생성 시 검산한다. 기존42raw, manifest, index는 변경하지 않는다. 새 output에는 선택 reference10개를 보존해 이후 제품 시험이 ignored artifact나 생성기에 의존하지 않게 한다.

단일 vector의 새 raw는24개/398,608bytes, reference는10개/151,552bytes다. 새 raw는12개의 full page,7개의1536-byte descriptor,5개의 WAL frame이다. `raw-index.tsv`는 이34개 파일의 exact length/SHA를 기록하고 manifest는 index의 SHA를 보존한다. Rust runtime은 `vectors/p{profile}-o{ordinal:02}/`의 index와 executor가 주는 `XADB_G6_VECTOR_INDEXES=p1-o00=<SHA>,...`를 결합한다. 이 추가 index는 metadata이며 raw24/ref10 분모를 바꾸지 않는다. WAL 길이는19,672/56,712/3,040/56,712/55,112이며 합191,248이다. reserve21,200+INIT38,000은 별도59,200bytes이며 이 생성기는 그 두 frame이나 실제 recovery database를 만들지 않는다. Status page9/incarnation2/slot0/Free generation1은 actual reserve/INIT가 공급해야 하는 semantic context다. 이 raw가 설치·allocation provenance를 대신하지 않는다.

기본 입력은 profile1/Job ordinal0다. ordinal은0..63의 local semantic 위치를 나타내며 실제 first-fit allocator가 임의 ordinal을 선택했다는 주장이 아니다. 최대55non-Free,ATT32 규칙은 별도 global census 입력으로 검산한다.64non-Free 정상 입력은 생성하지 않는다.

ResourcePeak6는 제품의 provisional `EMPTY_PEAK`를 읽지 않는다. generation gate JSON이 exact6정수와 `APPROVED_SEMANTIC_INPUT` 또는 `F05_ACCEPTED`, 그 근거 SHA를 명시해야 한다. 전자는 독립 codec 입력에 쓸 숫자라는 뜻이며 actual live caller F05 통과가 아니다. 승인된 numeric input과 제품 recipe가 다르면 첫 효과 전에 HOLD한다.

외부10초 timeout과 원 stdout/stderr/exit/elapsed receipt 하에서 한 번의 `python3 -B ... --repo <frozen-source> --output <new-directory> --approval <gate> --approval-sha256 <SHA>`만 실행한다. gate는 generator SHA/wire SHA/profile/ordinal/raw count/bytes/peak를 모두 결합한다. output directory와 모든 file은 create-new이며 실패 partial은 보존한다. output cap2MiB, 기존 original source/input의 마지막 SHA 재검산을 포함한다. 후속 수정은 원 output을 덮지 않고 새 revision/lineage로 남긴다.

아직 generator 실행, raw SHA 산출, 제품 compile/runtime/native/checker는 수행하지 않았다. 실제 source gate와 독립 recipe 검토 뒤 root가 최초 생성 명령과 receipt를 소유한다.
