# XRB1 독립 입력 출처

이하 `SOURCE_ONLY`·`NOT_GENERATED`·`NOT_RUN`은 최초 입력 작성 시점의 기록으로 보존한다. 후속 생성·source별 필수 검증과 현재 상태는 [복구 codec 실행 증거](../../../devdoc/evidence/dev3-recovery-control-base-20261024.md)를 따른다. 원 입력의 실행 전제와 당시 미공급 설명을 후속 PASS로 소급 변경하지 않는다.

2026-10-24 · **SOURCE_ONLY / NOT_GENERATED / NOT_COMPILED / NOT_RUN**.

이 디렉터리는 [wire 결정 §5](../../../artifacts/dev3/plan4-g6-empty-retention-wire-decision-20261024.md)의 XRB1 scope1 전용이다. 제품 `plan4_recovery_base.rs`의 최초 source SHA는 `978e3ce9a304fb7d9e9126d1c8320910f2897fba110f13c3ddd96ec61211c975`다. 현재 pure140의 source freeze 및 최대48경로 inventory와 분리한다. 이 최초 source 당시에는 module 연결과 ownership borrowed verifier·Catalog genesis·allocation terminal의 세 bridge가 미공급이었다. 후속 codec-checkpoint-r1에는 세 bridge와 module 연결을 포함하며 독립 source/input D2가 이를 확인했다. 원 최초 source의 상태와 현재 연결 상태를 구분한다. source 작성 수용은 생성·실행 또는 F05 승인과 다르다.

`generator.py`는 stdlib `struct`/`hashlib`만으로 fixed offset을 쓴다. 제품 encoder/decoder/binary와 이전 fixture generator를 import·실행하지 않는다. 원 plan4 `manifest.json` SHA `82880ea05480b9779bb2d5764c88f6423d2c74da1d4e68e8bf2bb0092e8eba22`, `raw-index.tsv` SHA `0c057920532b389a678c4fc1ea74e82d86a4d457dbd90b8d88ff16cfedd09e13`에서 P00..P08, profile1·2, FILEMAP1, XBS1, WAL header1의14개 original을 읽는다. 각 원 byte/length/SHA를 생성 전후 검산하고 새 output에 복사하며 원본은 쓰지 않는다. 선택 reference는167,936bytes다.

새 raw는15×1,048,576=15,728,640bytes다. raw15+reference14=29files/15,896,576bytes에 새 index/manifest를 더하고16MiB 이내여야 한다. raw names는 `input-plan.json`에 고정한다. 변형마다1MiB 파일을 저장하지 않고 Rust 시험이 원 raw를 복사한 뒤 명시 offset과 독립 SHA 식으로 변형한다. generator 실행 전 approval은 wire/generator/input-plan SHA, exact15names/bytes, ResourcePeak6 및 `APPROVED_SEMANTIC_INPUT` 또는 `F05_ACCEPTED`의 근거 SHA를 결합해야 한다. 제품 `EMPTY_PEAK`를 읽어 값을 자동 채택하지 않는다.

정상 snapshot의 생성 순서는 각 Status에 reserve/INIT를 먼저1/2,3/4,…로 배치한 후 AgingStart, 각 Begin, Ready 대상의 RollbackStart/AbortEnd다. 끝난 경우에만 Finish를 추가한다. 이는 가능한 문법적 이력의 **독립 값 계산**이며 그 WAL frame 또는 실제 producer를 생성·실행한 증거가 아니다. 모든 Status는9..63/object2/gen1/slot0/incarnation2다. Tx local1..N, generation2, 처음 Free Status generation1→Occupied2→Free2이며 Job도 동일하게2를 유지한다. 32 Active+23 Ready의 경우 ATT32와 전체 non-Free55를 정확 채운다. ordinal63은 한 Job 위치의 정상값이며64non-Free 상태가 아니다.

AllocationMaintenance의 count1 Prepared는 Terminal remaining의 step1/WAL38000/pages1/protected70768, peak(frames2,memory114688,locks0,registry0,files1,doublewrite0)다. Finished는 remaining0·consumed59200(Initialized) 또는42400(Cancelled), inventory_change=terminal apply/minimum0를 쓴다. 이는 기존 count1 allocation 계약에 있는 숫자를 독립 식으로 옮긴 것이며 새 bridge가 실제 같은 정본을 검산해야 한다. reserve의 Item96은 PageToken1/category3/units1/changeLSN=reserve이며 descriptor digest는 owner ordinal을0으로 정규화한 item SHA다.

`reserved32`는32 Status INIT와 AgingStart/32 Begin 뒤 L98에 Page41을 reserve한다. ATT32+Prepared AllocationMaintenance+Aging의34 active owners, Item96의 실제 owner ordinal32와 normalized ordinal0 SHA를 구별한다. allocation apply98/root apply97이며 Reserved 동안 새 group은 추가하지 않는다.

XOI1 embedded bank는 active ATT 뒤 Prepared AllocationMaintenance, Running Aging 순서다. Finished descriptor는 XRB1 bank에 남지만 XOI enumeration에서 빠진다. root bank2/index0/generation=base generation/captureL이며 count1일 때 chunk0을+2176에 둔다. chunk의 owner-table SHA, root의 full chunk SHA, descriptor의 normalized item SHA는 서로 다른 값을 독립 계산한다. 모든 unused region은0이다.

base header의 `source_prefix_SHA`는 설명된 snapshot 이름을 hash한 **명시 pure claim**이다. `epoch2`의 source-header SHA도 symbolic claim이며 CURRENT/XWE2/WRITER를 공급하지 않는다. prefix hash의 실제 WAL provenance, immutable selected closure, actual charged bytes·DWB retirement floor와 actual unique INIT는 이 codec fixture가 증명하지 않는다. INIT callback 시험은 별도 원 recipe에 대한 claim 대조일 뿐 `VerifiedBase`를 발급하지 않는다. 새 epoch raw는 원 TxnRef epoch1을 유지하고 header source epoch2/local0, predecessor full SHA를 인계해 raw decode 경계만 검산한다.

full-file SHA에는 XRB1 trailer도 포함한다. internal trailer SHA는 앞1,048,544bytes만 덮으므로 둘을 혼동하지 않는다. A/B copy 시험은 같은 bytes가 같은 full SHA라는 codec 조건만 확인한다. CURRENT fallback의 ACK 손실·동시 선택·실제 파일 publication·old WAL 회수는 다른 owner/native 단위의 의무다.

미래 생성은 외부 timeout30초/메모리64MiB/원 stdout·stderr·exit·elapsed receipt 아래 한 번만 한다. output directory 및 모든 파일은 create-new다. 실패 partial은 삭제·덮어쓰기 없이 보존하며 source/input 수정 후에는 새 revision output으로 준비한다. 생성기 import/실행·Python checker·Rust build/runtime·fixture 생성은 아직0이다.

Rust의 `XB_*` marker와 exact include/exclude/completed 정책은 [시험 입력](../../../artifacts/dev3/plan4-g6-empty-retention-20261024/base-test-input-source-r0.md)에 있다. 현재 static 측정에는 공급된 세 common bridge가 포함된다. 실행 순서는 새 compile → 새 static 측정 → pure 호출의 자원 판정 → 해당 semantic 사례다. parent caller와 live owner의 동시 생존 자원은 이 pure 측정에서 제외하며, 실제 live F05의 수용은 별도 미완료다.
