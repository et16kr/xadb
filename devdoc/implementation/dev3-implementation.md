# DEV3 구현·검증 현황

2026-10-20 · **page/key/reference 표현·M01–M06 유한 모델 로컬 검증 완료 / DEV3·S1 인수 미완료**

[설계](../contracts/disk-storage-design.md) · [구현 계획](../plans/dev3-implementation-plan.md) · [단계 완료 정본](../plans/stage-completion.json)

<a id="wave-1"></a>
## 첫 작업의 범위와 소유

사용자의 DEV3 진행·가능한 병렬 작업 요청으로 시작한다. DEV2 전체 인수의 PF04 실패는 유지한다. 이번 범위는 XADB-028의 공통 page 표현 leaf와 S0의 D3-M02 모델이다. 실제 page 파일·buffer·WAL/undo/CLR·tree·commit 경로는 아직 연결하지 않는다.

| F07 항목 | 기록 |
| --- | --- |
| 분류·작업/계약/통합 | D2 · `/root` |
| 기준 commit | `641f28ed666c73f1988bb8102b22c11d6023f3e1` |
| 착수 dirty diff | `git diff --binary` SHA-256 `957b7438010c5149e8b1fbea7fa8669f25cad1117856d91825732fabfcec0a7b` |
| 기존 동시 변경 | DEV6 source research·feature-design-inventory Markdown/JSON·user-manual-draft의 4개 변경 보존. 이 작업의 수정 범위에서 제외 |
| root 소유 | `crates/engine/src/storage/page.rs`, `page_tests.rs`, 이 구현 기록과 page byte 계약, DEV3 실행 조립 스크립트 |
| 모델 담당 | `/root/dev3_model`: `crates/testing/reference/tests/dev3_snapshot_model.rs`, `devdoc/contracts/dev3-snapshot-model.md`만 독점 작성 |
| 공급 조사·독립 vector | `/root/dev3_supply`: F06 실제 symbol/test·선행 읽기 전용 조사 뒤 `scripts/dev3_page_vectors.py`와 `crates/engine/tests/fixtures/dev3-page-v1-{empty,cells}.bin` 독점 작성. production codec 미열람 |
| 독립 검토 | `/root/dev3_review`: 원문·diff·반례·실행 결과 검토, 구현자와 별도 실행 |
| 공유 파일 | engine lib/storage 모듈 연결, README·documents.json·repository-build-plan Markdown/JSON·단계의 실제 상태 연결은 root가 최신 원문에 순차 반영 |
| 의미 중복 | page codec은 bytes/identity/bounds만 처리. 모델은 별도 추상 history와 edge를 사용하며 production codec·가시성 알고리즘을 import하지 않음 |
| 병합 순서 | byte/model 계약 동결 → 독립 vector/model와 page codec → 독립 검토·수정 → 동일 소스 회귀·증거 → 상태/색인 갱신 |
| 실행 입력 | 기존 `crates/tests/scripts` fingerprint·clean export roots 안에 배치. 신규 dependency·source root 없음 |
| 검증 | 페이지 경계/잘림/손상·독립 vector, M02-P01 고정 부분 모델 전수와 mutants, DEV2 P0/K0 및 DEV0/1 회귀. M02 전체는 미완료. 문서 자동 검사는 push gate에서만 수행 |

F01은 이번 [page envelope](../contracts/dev3-page-format.md)만 구체화하며 cell 의미·WAL·undo·DWB 등의 동결은 남는다. F02/F03은 [M02-P01 부분 모델](../contracts/dev3-snapshot-model.md)의 명시된 범위만 대상으로 한다. M02 전체 모델·DS04 실제 thread·파일 내구성은 미완료다. F04/F05는 이번 무효과 표현 leaf에는 적용되지 않으며 실제 I/O/예약 leaf 전에 반드시 완료한다.

<a id="wave-2"></a>
## 전체 DEV3 목표의 두 번째 병렬 작업

사용자가 DEV3 완료와 가능한 병렬 개발을 요청했다. 첫 leaf를 전체 완료로 바꾸지 않으며 S0–S5와 기존 인수 조건을 유지한다. 아래는 착수 소유 기록이며 실행 증거가 아니다.

| F07 항목 | 기록 |
| --- | --- |
| 작업·계약·통합 담당 | D2 · `/root` |
| 기준 commit | `641f28ed666c73f1988bb8102b22c11d6023f3e1` |
| 착수 tracked dirty diff | `git diff --binary` SHA-256 `6118b85df0360633527c573837e7c95c6e09a6a16ff8f306dc5b36eb416a39cb` |
| 수신 실행 소스 | 첫 leaf 및 DEV2 복구를 검증한 지문 `3bc3dcd16fa443555f6cc084be22870e8c10674a8006c9da827b1a9615094ad1` |
| 기존 변경 | 첫 작업의 모든 미commit 파일과 기존 DEV6 관련 4개 변경을 보존한다. 첫 작업의 원본 receipt는 수정하지 않는다 |
| `/root/dev3_model` 소유 | M01/M02 모델·계약: `crates/testing/reference/tests/dev3_undo_model.rs`, 기존 `dev3_snapshot_model.rs`, `devdoc/contracts/dev3-undo-model.md`, 기존 `dev3-snapshot-model.md` |
| `/root/dev3_review` 소유 | M03/M06 모델·계약: `crates/testing/reference/tests/dev3_progress_model.rs`, `dev3_reservation_model.rs`, `devdoc/contracts/dev3-progress-model.md`, `dev3-reservation-model.md` |
| `/root/dev3_supply` 소유 | M04/M05 모델·계약: `crates/testing/reference/tests/dev3_recovery_model.rs`, `dev3_doublewrite_model.rs`, `devdoc/contracts/dev3-recovery-model.md`, `dev3-doublewrite-model.md` |
| `/root` 소유 | 무효과 복합 key 표현: `crates/engine/src/storage/key.rs`, `key_tests.rs`, `devdoc/contracts/dev3-key-format.md`; foundation `src/codec.rs`, `tests/key_buffer.rs`도 root 단일 작성. key 검증 뒤 root가 `storage/page.rs`의 PageRef wire helper, `storage/anchor.rs`, `anchor_tests.rs`, `storage/undo_ref.rs`, `undo_ref_tests.rs`, `devdoc/contracts/dev3-reference-format.md`의 무효과 표현을 순차 작성한다. 이 기록·공유 모듈 연결·README·문서/빌드/단계 JSON·runner 조립은 root가 순차 반영 |
| 계약 선행 | 각 모델은 F02/F03 action·fixture·fairness·oracle·mutant·시간 예산을 실행 전 고정하고 소유 계약과 대조한다. 모델 graph 상한 도달은 INCONCLUSIVE이며 실행 후 범위 축소로 PASS를 만들지 않는다 |
| 의미 중복 | M01/M02 edge·pin, M03/M06 진행·예약, M04/M05 WAL·DWB 경계를 각 계약에 명시한다. 공통 production 알고리즘을 독립 oracle에 복사하지 않는다. 실제 production 링크/회수·I/O 효과는 관련 모델·F05 이후에만 연결한다 |
| 독립 검토 | 각 모델 구현자는 다른 모델 묶음을 교차 검토한다. root 구현은 별도 agent가 검토한다. 작성자가 자신의 결과를 독립 검토했다고 세지 않는다 |
| 병합 순서 | 상세 계약 동결 → 모델/표현 구현·국소 실행 → 교차 검토·필수 수정 → 합친 소스에서 실행/회귀 → 새 receipt·상태/색인 |
| 생성물·검증 | 모델 receipt는 새 경로로 보존하며 첫 작업의 결과를 덮어쓰지 않는다. source roots/dependency 추가 없음. 문서 검사는 push 직전만 수행한다 |

M04/M05 담당 구현이 동결된 뒤 `/root/dev3_supply`는 `crates/engine/src/storage/tree_page.rs`, `tree_page_tests.rs`, `devdoc/contracts/dev3-tree-format.md`의 다음 무효과 표현 leaf를 독점 작성한다. root가 상세 byte/정렬/분할 계약을 먼저 검토하고 공유 module·색인·runner는 계속 root가 연결한다. 이 단계는 기존 page/key/reference codec을 소비하는 공통 tree 표현이며 file/buffer/WAL/transaction 효과를 추가하지 않는다. root는 별도로 동결 M04를 검토·실행하고, `/root/dev3_model`은 작성자가 다른 M06의 독립 검토를 수행한다.

M06 독립 검토 뒤 `/root/dev3_model`은 `scripts/dev3_progress_receipts.py`, `scripts/test_dev3_progress_receipts.py`만 독점 작성하여 M03/M06의 suite별 고정 분모·receipt 검사 helper를 공급한다. entry runner `scripts/check_dev3_models.py`와 모델 source/contract는 이 작업의 수정 범위가 아니다. root가 helper를 별도 검토하고 entry runner에 순차 연결한다.

M03/M06 source 동결 뒤 `/root/dev3_review`는 `devdoc/contracts/dev3-transaction-format.md`, `crates/engine/src/storage/transaction_record.rs`, `transaction_record_tests.rs`를 독점 담당한다. 먼저 TxnId·TxnRef·VersionStamp·논리 행 버전과 status cell의 상세 byte/상태 계약을 작성하고 root의 독립 검토를 받는다. 무효과 codec 범위이며 실제 slot 할당·visibility·commit·rollback·참조 계수 변경과 recovery obligation 집행은 포함하지 않는다. 표현만으로 status 회수·복구가 완성되었다고 하지 않는다. 공유 module·색인·runner 변경은 root가 순차 반영한다.

root는 `devdoc/contracts/dev3-overflow-format.md`, `crates/engine/src/storage/overflow.rs`, `overflow_tests.rs`의 row overflow 표현을 담당한다. 기존 page/ref를 소비하는 immutable chunk byte·고정 chain 위치/길이/digest 검산만 정의하며, 실제 page 할당·chain I/O·row 공개·horizon 회수는 포함하지 않는다. 상세 계약과 구현은 다른 담당의 독립 검토 뒤 통합한다.

첫 구현 commit `d58c26b` 이후 `/root/dev3_model`은 `devdoc/contracts/dev3-wal-format.md`, `crates/engine/src/storage/wal_frame.rs`, `wal_frame_tests.rs`를, `/root/dev3_supply`는 `devdoc/contracts/dev3-allocation-format.md`, `crates/engine/src/storage/allocation_page.rs`, `allocation_page_tests.rs`를 독점 담당한다. 각각 byte/ID/한도·독립 vector·상태 판정을 먼저 제안하고 root의 독립 계약 검토 후 표현 구현을 시작한다. 실제 append/sync·redo·extent 확장·예약 token 발급/회수는 F04/F05와 해당 모델 이후 별도 통합한다. 공유 API/형식 변경이 필요하면 먼저 조정하며 root가 공유 module과 색인을 수정한다.

transaction 표현·leaf checker 검토가 끝난 뒤 `/root/dev3_review`는 `devdoc/contracts/dev3-row-format.md`, `crates/engine/src/storage/row_record.rs`, `row_record_tests.rs`의 무효과 row/index version cell과 canonical row bytes를 담당한다. 기존 tree의 value 영역·transaction identity·overflow 표현을 소비하며 schema·visibility·실제 row 변경을 완성한 것으로 처리하지 않는다. 상세 계약을 root가 먼저 검토하고 구현을 진행한다.

root는 `devdoc/contracts/dev3-doublewrite-format.md`, `crates/engine/src/storage/doublewrite_control.rs`, `doublewrite_control_tests.rs`의 selected doublewrite control 표현을 담당한다. control/selector/header까지 포함한 고정1010-slot/16 MiB pool의 최대64-page pending batch·identity·digest·sequence를 구체화하며 실제 selector·slot write/sync·home repair/retirement는 포함하지 않는다. 상세 계약과 독립 byte vector를 먼저 작성하여 다른 담당의 검토 후 구현한다. 계약 검토·allocation 독립 검토를 마친 `/root/dev3_model`에게 이 세 파일의 후속 구현·시험 독점 쓰기를 넘기며, root가 완성 소스와 결과를 독립 검토한다. 공유 module·색인은 root 소유를 유지한다.

WAL/row 독립 검토를 마친 뒤 `/root/dev3_review`는 `devdoc/contracts/dev3-undo-format.md`의 undo record·chunk 상세 F01 제안만, `/root/dev3_supply`는 `devdoc/contracts/dev3-control-format.md`의 선택 control/FILEMAP/profile 상세 F01 제안만 독점 작성한다. 아직 제품 파일은 만들지 않는다. root의 계약 검토 뒤 실제 구현 소유·시험을 확정한다. undo의 참조 계수/edge·예약 descriptor와 control의 영속 selector 효과는 표현과 별도이며 근거 없이 완료로 처리하지 않는다. root는 `devdoc/contracts/dev3-resource-bounds.md`의 실제 codec 크기 기반 자원 계산·미완료 항목을 담당하며 공유 runner/색인 단일 쓰기를 유지한다.

DWB 표현을 동결한 뒤 `/root/dev3_model`은 `devdoc/contracts/dev3-obligation-format.md`의 영속 recovery obligation descriptor·원자 WAL group의 F01/F02 제안만 독점 작성한다. 제품 소스·실행은 root 검토 전 진행하지 않는다. root의 자원 계산과 기존 allocator owner·TxnRef/status·WAL frame을 소비하며, 미구현 undo/control과의 연결을 명시한다. source 지문 `e419f2d74f954c3926f70cf7ac39626e8171b09fc50bb0925c707b2c9fedb0f7`의 새 통합 회귀 동안 실행 checker는 동결한다.

root는 `devdoc/contracts/dev3-catalog-format.md`의 versioned table/index catalog cell·typed definition F01 제안을 담당한다. 현재 K0 TableSpec 의미와 DEV3 index 규칙을 재사용하며 이름 조회·DDL/visibility·실제 catalog undo는 별도다. 독립 계약 검토 전 제품 파일을 만들지 않는다.

2026-10-21 undo 상세 계약·20개 literal/2개 page를 root가 수신 검토한 뒤 `/root/dev3_review`에게 `crates/engine/src/storage/undo_record.rs`, `undo_record_tests.rs`와 해당 계약의 독점 구현·시험을 배정한다. root catalog 계약·6개 literal의 독립 검토 수용 뒤 root에게 `crates/engine/src/storage/catalog_record.rs`, `catalog_record_tests.rs`와 해당 계약 구현·시험을 배정한다. 두 leaf는 caller-buffer 무효과 표현이며 공유 module·runner·색인은 root가 순차 연결한다. 세 번째 커밋의 검증 소스와 분리하여 다음 묶음으로 기록한다.

같은 날 control 상세 계약과11개 control·2개 reference·1개 entry literal을 root가 독립 대조하여 수용했다. `/root/dev3_supply`에게 `crates/engine/src/storage/control.rs`, `control_tests.rs`와 `devdoc/contracts/dev3-control-format.md`의 독점 구현·시험을 배정한다. typed selector 확장안 A를 선택하되 이 leaf는 byte 표현만 구현하며 실제 foundation/platform/worker port 확장·I/O fault는 후속 별도 소유로 남긴다. bootstrap1 MiB와 알려진 고정 공간 소계는 전체 F05 인수 근거가 아니다.

root는 `devdoc/contracts/dev3-selector-port.md`에서 FILEMAP/DWB typed selector의 port·fault 경계와 고정 시험을 먼저 정의한다. `/root/dev3_model`의 독립 계약 검토 수용 뒤 `/root/dev3_review`가 `crates/foundation/src/namespace.rs`, `crates/platform/src/namespace.rs`, `crates/platform/tests/contracts.rs`, `crates/testing/protocol/src/fault.rs`, `crates/testing/engine-worker/src/fault.rs`, 새 `fault_tests.rs`, `crates/testing/engine-worker/tests/native_fault.rs`의 exhaustive match 수신을 독점 구현한다. 기존 namespace 효과의 이름을 확장하는 기반 작업이며 DEV3 DB 생성·파일 확장·복구 알고리즘을 구현한 것으로 표시하지 않는다.

<a id="key-leaf"></a>
### 복합 key의 제한된 검증

`storage::key`에 1–4열·escaped 1,024-byte composite, 별도 8-byte RowId suffix, canonical prefix 검증·NULL 메타데이터·typed tuple 순서 비교를 구현했다. foundation scalar bytes는 유지하며 기존 `encode_key`와 새 caller-buffer encoder가 같은 구현을 쓴다. 출력 크기와 모든 입력은 쓰기 전에 검사한다. 실제 index tree·schema/PK·unique lock·statement-final 검사는 이 leaf의 기능이 아니다.

`/root/dev3_model`이 key 계약과 foundation/engine 구현·시험을 별도 정적 검토하여 필수 수정 사항 없음으로 보고했고 root가 확인했다. UTF-8의 escaped NUL 구간 검증, canonical bytes와 Eq/Ord 일관성, output 보존과 RowId 경계를 검토했다. 이 검토자는 별도 제품 시험을 실행하지 않았으며 root 실행을 자신의 새 증거로 세지 않는다.

같은 reviewer가 key/scalar를 추가한 runner의 고정 test-name·분모·중복·summary·exit code 검사와 자기검사를 별도 정적 검토했다. 이 검토도 추가 제품 실행으로 세지 않는다.

고정 입력의 새 시험은 foundation3·key5 PASS이며 400개의 두-column tuple 모든 pair에 RowId 세 값을 교차 검산한다. root의 `cargo test -p xadb-foundation -p xadb-engine --all-features --locked --offline`은 실행된 모든 시험을 통과했다(DEV2 복구15 포함). 명시 V3 장시간 시험3개는 ignored로 남는다. 이는 작업 트리 국소 검사이며 그 자체를 동결 소스의 하위 전체 회귀로 세지 않는다. 해당 두 crate의 all-targets/all-features Clippy도 PASS다.

`python3 -B scripts/check_dev3_leaf.py`의 새 보존 소스는 `artifacts/dev3/leaf-ca08c7jy/source`, 실행 지문은 `0c91f2d3d4db2e6cf0c870df90275eecf5f66c2d6b799403b77f0c78a15adcc3`다. page7·key5·scalar3·P01 정상4/결함8·budget 검사·runner 자기검사7을 통과했다. source/contract/binary 전후 hash를 기록한 [원본 receipt](../evidence/dev3-key-leaf-xadb-20261020-r1.json)의 SHA-256은 `36fc152cd6ca324739ba7f826ec90d1539f1021a2c464e828090bdcd9b75cf23`다. 이 실행에는 `--with-regressions`를 사용하지 않았다. 동시에 작성 중인 M01–M06 새 파일은 이 receipt의 실행 분모에 없으며 전체 모델·현재 작업 트리·실제 disk 인수로 확대하지 않는다.

<a id="reference-leaf"></a>
### PageRef·UndoRef·anchor 표현

PageRef24-byte·UndoRef40-byte와 안정 anchor48-byte cell을 구현했다. root page를 읽을 때 DB/owner/generation·PageRef·kind/level을 대조한다. 같은 file/page의 다른 incarnation도 anchor의 자기 주소로 거절한다. 이 codec은 실제 reference pin·undo slot 수명·SMO·파일 I/O를 수행하지 않는다.

anchor4·reference2 국소 시험과 foundation/engine Clippy를 통과했고, 마지막 자기 주소 강화 뒤 anchor4를 다시 실행했다. `/root/dev3_model`의 독립 정적 검토에서 필수 수정 사항이 없었으며 reviewer의 별도 실행으로 세지 않는다. 보존 source 실행은 아래에 새 receipt로 연결한다. leaf runner의 고정 목록에 이6개를 추가했으며 기존 P01과 page/key/scalar 분모는 유지한다.

`python3 -B scripts/check_dev3_leaf.py`가 `artifacts/dev3/leaf-7ikonvz2`의 보존 소스에서13개 check를 통과했다. page7·key5·scalar3·anchor4·reference2·P01 정상4/결함8·budget·runner 자기검사7을 포함한다. source SHA-256은 `5b548dbafa6acdf712bcb91386a5a62caaa0a4a79bd053e24b665b6367a61e18`이고 [원본 receipt](../evidence/dev3-reference-leaf-xadb-20261020-r1.json)는 `f38fb269a96376cc9430ce3eccfa92aaa11618da95d99bc0a74dd62d78752971`다. 이 실행의 하위 전체 회귀는 NOT_RUN이다.

<a id="models-125"></a>
### M01·M02·M05의 독립 모델 실행

root는 별도 `scripts/check_dev3_models.py`와 자기검사6개를 구현했다. 현재 등록된 M01/M02/M05를 명시적으로 선택하여 보존 source·release binary·contract·log identity와 case/witness/반례 분모를 검사한다. `/root/dev3_model`은 이 runner와 reference leaf 등록을 독립 정적 검토했다. 실제 action보다 넓었던 cut 목록을 바로잡고 존재하지 않는 cut의 거절 자기검사를 추가했다. M02 prepared prelude17/17/18 actions는 고정된 원문과 정확히 비교하며 이후 depth16 trace와 구분한다.

root가 각 모델의 실제 source·oracle·transition·witness/반례 재생을 독립 검토했다. M05의 초기 ACK oracle 복사와 complete-home workspace 누수는 각각 고정 실패/이전 실행을 보존하고 수정했다. M02는 chain6의 state cap 실패를 `/tmp/xadb-m01-m02-r3`에 보존했다. D2 검토 뒤 미래 안전성·guard에 영향을 주지 않는 `observed_q` 기록만 visited-key에서 제거했다. 실제 node/trace는 유지하고 관측 successor에서 witness를 채집·재생한다. 기존 fixture·action·depth·cap을 줄이지 않았다.

`python3 -B scripts/check_dev3_models.py --model M01 --model M02 --model M05`의 source는 `890e59f6ec0f25aaf1a042bd7db15e90fc7e1bd7e207a9237323656fc147a536`이며 `artifacts/dev3/models-2cs421cq`에 보존했다. [원본 receipt](../evidence/dev3-m01-m02-m05-xadb-20261020-r1.json)의 SHA-256은 `fb7985cb9931b4381c862064c23c32b486e2c4b894122c0b08d6bb929159640f`다.11개 check는 build2·모델3·예산3·mixed-home 회귀1·toolchain·runner 자기검사로 구성된다.

| 모델 | 정상 / 결함 | 정상 상태 / 전이 | 별도 판정 |
| --- | --- | --- | --- |
| M01 | 16 / 12 | 384,967 / 1,352,795 | 전체4 undo/status slot 재사용, statement/full rollback·row/index 이력, 모든 witness/반례 재생 |
| M02 | 13 / 23 | 4,679,488 / 22,660,918 | acquisition4·chain6·handoff3, 각 case≤1,000,000 states·8,000,000 transitions·60초, 미탐색0 |
| M05 | 3 / 9 | 946 / 16,732 | 두-file pending 전체 retirement·WAL redo·frame 해제, mixed-home repair 완료 |

이 보존 source에 포함된 M03/M04/M06 작성 중 파일은 이 receipt의 실행 분모가 아니다. M01/M02 작성자의 release/debug 별도4시험과 receipt 일치도 확인했으나 이 root receipt는 release 실행에만 귀속한다. 현재 작업 트리 하위 전체 회귀·실제 thread·파일 장애·DEV3 전체 인수는 후속 의무다.

<a id="model-4"></a>
### M04의 독립 모델 실행

M04는 정상4개가 state cap을 만난 r3/r4/r5와 source/contract/log를 `artifacts/dev3-wave2-supply-review-r3` 등 각 경로에 보존했다. oracle 전용 capture-time home LSN과 사건 witness 누적값을 전이 검사/원본 trace로 옮기고, 미래 동작에 사용되지 않는 분석 결과·과거 prefix·peak 기록·선택 완료 capture의 중복 상태를 D2 검토했다. 마지막에는 Capture에서 독립 검산한 DPT bit를 **visited-key에서만** 정규화했다. 실제 checkpoint/DPT·8개 home/live page·node/trace는 유지한다. DPT만 다른 Select는 이 모델의 미래 동작과 안전성에 영향을 주지 않는 stutter이며, 모든 successor 검사를 중복 병합 전에 수행한다.

이 모델의 redo는8개 page 모두를 처리하므로 DPT 기반 작업 선택·bounded I/O를 검증하지 않는다. group의3개 workspace token도 실제 page별 frame pin mapping을 나타내지 않는다. multi-page pin은 M03/M06·제품 실행, Rebase의 실제 물리 closure는 M06 및 실제 복구의 별도 의무다. 이러한 제한은 전체 DEV3 의무를 삭제하지 않는다.

root가 동결 source·독립 oracle·quotient·모든 witness/반례 재생을 검토했고 모델 작성자 `/root/dev3_supply`는 root runner의 M04 고정 분모와 오류 판정을 독립 검토했다. `python3 -B scripts/check_dev3_models.py --model M04`는 `artifacts/dev3/models-7ck61v71`에서 정상5·결함10·예산 검사를 통과했다. 정상 총1,855,194 states·5,110,100 transitions, 각 depth16·미탐색0이며 case별 기존 cap을 유지했다.

source SHA-256은 `67bbccfdfa571dc268ea7152ee444680fd7777833dc615ce76bb0c2b4f664070`, [원본 receipt](../evidence/dev3-m04-xadb-20261020-r1.json)는 `38f5b93d0ce9cded8ac1c4bc589b5f7e2506152a4a7e3c05d43531db93fd495b`다. 이 receipt는 앞의 M01/M02/M05 실행과 다른 source export에 귀속하며 실제 disk 복구나 전체 통합 회귀로 합쳐 주장하지 않는다.

<a id="models-all"></a>
### M01–M06의 동일 소스 실행

M03의 실제 frame 계수는 owner별 pin 합계가 아니라 공유 page 주소의 합집합으로 수정했다. 공유 reader contention의 cleanup fixture와 page 준비 순서, queued turn의 snapshot/content admission·reader drain·flush 구간을 각각 구분했다. closed progress는 depth16 안전성 탐색으로 종료를 주장하지 않고 전체 도달 graph의 weak-fair SCC와 실제 prefix/cycle 재생을 검사한다. M06은 같은3-frame/2-FD 의미에서 forward와 진행용 소유를 나누고, 반복 statement 실패의 retained bytes·32개 허용 거래와 별개인 모델의 작은 상태 공간·고정 두 bootstrap 슬롯·비영 growth의 FILEMAP 장벽을 검사한다.

root는 M03 source·반례·진행 graph를 독립 검토했다. M06 작성자와 다른 `/root/dev3_model`은 최종 source를 별도 컴파일하여7개 시험·240개 receipt를 독립 실행했고 작성자 결과와 elapsed를 제외한 값이 같음을 확인했다. 과거 M03 first/third와 M06 second 실패는 정확 source 사본을 찾지 못해 log/hash-only 한계를 `artifacts/dev3/m03-m06-20261020/source-retention.json`에 남겼다. 찾은 과거 사본과 최종4개 source/contract는 별도로 보존했다. 이 제한을 현재 source의 실패 복원 증거로 과장하지 않는다.

`/root/dev3_model`이 M03/M06의 고정 inventory 검사 helper와24개 변조 자기검사를 작성했고 root가 source·case/action/marker·schema·반례·budget/NA 경계를 독립 검토했다. 기존 runner6 검사와 함께 통과했다. `/root/dev3_review`가 main 통합의 checker source 불일치 가능성을 지적하여, root는 이미 읽은 checker4개의 hash와 export 사본을 대조하고 progress helper를 보존 사본에서 직접 load하도록 보완했다. `/root/dev3_model`이 마지막 main 변경을 별도 정적 검토하고6+24 자기검사를 실행하여 PASS를 확인했다. 이 지문은 실행 증거의 연결이며 OS 실행 격리를 의미하지 않는다.

`python3 -B scripts/check_dev3_models.py --model M01 --model M02 --model M03 --model M04 --model M05 --model M06`는 보존 source `artifacts/dev3/models-c17b_4tr/source`에서29개 check를 통과했다. source 지문은 `bfc583887bddf1f95f2b50a64ae222f105d28daa78f5ee4631cba08967499298`다. [원본 receipt](../evidence/dev3-models-xadb-20261020-r1.json)는 변경 없이 복사했으며 SHA-256은 `9ddea8bbda946fe3cfae25161c62492becb3c84645ed941d88d412899400025f`다.

| 모델 | 정상 / 지정 반례 | 정상 상태 / 전이 |
| --- | --- | --- |
| M01 | 16 / 12 | 384,967 / 1,352,795 |
| M02 | 13 / 23 | 4,679,488 / 22,660,918 |
| M03 | 232 / 21 | 30,478 / 64,333 |
| M04 | 5 / 10 | 1,855,194 / 5,110,100 |
| M05 | 3 / 9 | 946 / 16,732 |
| M06 | 188 / 52 | 125,887 / 205,179 |

정상457·지정 반례127은 각 suite가 선언한 bounded/closed graph 범위에 귀속한다. M06의 zero-demand보다1 작은20개 입력은 비적용 선언이며 실행 PASS가 아니다. 예산 강제 소진6개와 M05 mixed-home workspace 회귀도 같은 binary에서 통과했다. 작성 중 tree/overflow/transaction 표현이 export에 포함되어도 이 모델 실행 분모나 실제 디스크 효과로 세지 않는다.

이 실행은6개 모델의 고정 범위 검증이다. 실제 opcode별 F05 수치·WAL/CLR/allocator bytes·thread/I/O·DS01–06·PF09·DEV3 전체 인수는 미완료다. M04의 보수적 전체8page redo와 실제 frame mapping 부재 등 앞서 기록한 모델 제한을 유지한다. 기존 DEV2 PF04 실패를 승격하지 않으며 하위 전체 회귀와 current-build recovery는 후속 실제 통합에서 새로 연결한다.

사용자의 단계별 commit 지시에 따라, 검증을 마친 page/key/reference·유한 모델·runner/원본 receipt를 첫 구현 commit으로 묶는다. tree/overflow/transaction 표현은 독립 검토·실행 뒤 후속 commit으로 분리하며 기존4개 동시 변경은 제외한다. 문서 자동 검사는 기존 push gate에서만 수행한다.

<a id="supply"></a>
## F06 공급 확인

수신 revision은 위 기준 HEAD다. `/root/dev3_supply`가 실제 symbols·시험·보존 source와 receipt를 읽었다. 공급의 `engine/foundation/platform/testing/reference` 파일과 DEV2 kernel/reference checker는 보존 e3 source와 byte 차이가 없었다. 당시 실행 소스 지문은 `e3bccf1a865b75deafa9eeac551f17591df406da1c41ccbdf9ec909c9c8e0f1a`이며 보존 사본은 `/home/et16/xp2-r2/final/source`다. 실제 ext4 보고서 `/home/et16/xk-mqsnomp2/artifacts/d2k-hmv8bn4m/report.json`의 hash `a0eb86b8def9621b5f5cbb3aba540764ed8531ecbc29233d2357fa981cca1803`와18개 PASS 그룹을 확인했다. 이는 [DEV2 재실행 기록](dev2-implementation.md#dev2-resumed-tests-20261019)의 과거 증거이며 이번 소스의 새 실행으로 세지 않는다.

| 분류 | 실제 공급·시험 | DEV3 경계 |
| --- | --- | --- |
| reuse unchanged | foundation `identity::{ObjectId,RowId,Generation,SchemaEpoch,CommitSeq}`·`value::Value`·`error::{Effect,Certainty,RetryScope}`·`codec::{ContentHasher,content_digest,encode_key}` | page에서 DatabaseId/ObjectId/Generation과 digest 사용. scalar key는 향후 composite 표현의 입력, unique 의미는 별도 |
| reuse unchanged, 소비 전 재확인 | foundation `FileIo`·`DirectoryPort`·`WriterNamespace`, platform Linux adapter | 이번 leaf는 I/O port를 소비하지 않음. DWB/namespace의 실제 적합성은 S2에서 검증 |
| adapt | engine `api::{DatabaseId,TableHandle,RowUid,TableSpec}`·`identity::Issuer`; reference `history::check`·`model::Model` | 논리 ID/독립 기대값 유지, 새 page·table validation·undo 관측은 별도 |
| XADB-027 최소 공급 | `substrate` CRUD·stale namespace·drop/recreate·quota 시험 | 실제 K0 최소 기반 존재. COW storage는 DEV2-only |
| XADB-010 최소 공급 | `transaction::execute_statement/commit_with`; `failed_statement_preserves_previous_rows_and_exact_private_leases`, `competing_real_writers_publish_one_root_and_old_reader_keeps_its_value` | statement/원자 공개 의미는 공급, global-q·private rollback 구현은 DEV2-only |
| XADB-009 최소 공급 | `database/maintenance::{checkpoint,backup,restore_new,collect_garbage}`; `checkpoint_every_mutating_boundary_preserves_the_selected_ack_closure`, backup/restore/GC boundary 시험 | exact C·오류 의미는 공급, immutable closure는 DEV2-only |
| DEV2-only | durable `codec::{Batch,Record}`·AXF1 WAL·COW replay/retention 알고리즘 | interleaved WAL/CLR·analysis/redo/undo·ATT/DPT로 재사용했다고 하지 않음 |
| new DEV3 / not supplied | page encoder/decoder는 이번 신규 leaf. allocator·buffer·tree·UndoRef/status·CLR·DWB·디스크 거래/복구는 미공급 | 표현 시험을 해당 실구현·내구성 증거로 확대하지 않음 |

XADB-028의 XADB-009/010/027 최소 공급을 확인하여 제한된 표현 leaf에 착수한다. DEV2 전체 인수·DEV3 영속 gate를 충족했다는 뜻은 아니다. 실제 I/O 경로를 소비하는 다음 통합에서는 current-build kernel recovery를 새로 실행해야 한다.

<a id="decision"></a>
## D3-LEAF-001 결정

| 항목 | 내용 |
| --- | --- |
| 상태·일자 | ACCEPTED · 2026-10-20 |
| 근거·소유 | 사용자 진행 요청·DP01 위임. 작업/계약/통합 `/root`, 독립 검토 `/root/dev3_review` |
| 선택 | 기존 16 KiB·128-byte header·SHA-256·slot directory 계약의 공통 표현을 구현. snapshot 모델은 reference 시험에 독립 배치 |
| 대안·이유 | 전체 WAL/undo 형식을 한 번에 고정하면 미검증 소비 관계가 커진다. 먼저 무효과 page envelope를 검증하고 cell 의미/내구성은 소유 leaf에서 확정 |
| 대체 범위 | 앞선 page 크기/identity/검증 의무는 유지. F01의 page envelope 부분만 구체화; DEV2 형식·기존 fixture·단계 gate는 유지 |
| 재검토 | 실제 tree/undo/status cell 요구와 충돌, bytes의 모호한 해석, 모델 반례·미완주, page codec의 검증 범위 확대 |
| 검토·실행 | `/root/dev3_review`의 page/model 독립 검토와 `/root/dev3_supply`의 runner 독립 검토에서 필수 지적 해소. 계약/통합 담당 `/root` 반영 확인. 최종 동일 소스 실행은 아래 별도 기록 |

<a id="review"></a>
## 독립 검토와 반영

page/model 검토자는 원문·실제 코드·독립 fixture·부정 사례를 읽고 Rust page7·Python vectors·model2를 별도로 실행했다. page의 기대값은 codec을 읽지 않은 다른 담당이 작성했다. page 표현은 private storage에 유지하며 S2 소비 연결 전까지의 좁은 dead-code 허용·제거 조건을 남겼다.

부분 모델은 단일 reader의 제어 순서로 제외한 source-latch/guard 인계 overlap 네 전제를 계약 표에 명시했다. 이 보완은 실제 actor 전체의 동등성 증명이 아니며 M02 전체 미완료 상태를 유지한다. 첫 통합의 Clippy `map_entry` 실패는 보존했고 `Entry::Vacant`로 수정한 뒤 동일한 탐색 수를 재확인했다.

runner 검토에서는 0개 시험·누락 witness·잘못된 반례/JSON 타입·누락 vector 분모·실행 중 identity 변경·timeout/부모 종료 뒤 남은 자손의 거짓 PASS 경계를 수정했다. 고정 이름/분모·trace/violation·source/contract/binary hash·실제 toolchain을 검사한다. 정상 종료 후에도 자손을 강제 정리해야 하면 실패이며, timeout·실패 로그와 결과는 보존한다. 다른 실행의 PASS를 실패한 source의 결과로 소급하지 않는다.

최종 runner 검토는 `check_dev3_leaf.py` SHA-256 `df2cb7cf89efa77c8fc1f207254edf0bf9b6aa7d8ea2dcae78f6252b331e5f30` 및6개 부정 시험에 귀속한다. 모델의 Clippy 수정3줄도 별도 검토자가 보존 소스와 대조하여 최종 hash `a21730d78355accb23aeb410a0cc6da2552dd6e96926a3310821c2326e8f24f2`에 PASS를 연결했다. 전체 source identity는 실행 receipt에 기록한다.

<a id="validation"></a>
## 2026-10-20 실행 결과

최종 source SHA-256은 `3bc3dcd16fa443555f6cc084be22870e8c10674a8006c9da827b1a9615094ad1`이다. 실행 전후 보존 소스와 완료 시 작업 트리의 실행 입력 지문이 같다. [최종 leaf receipt](../evidence/dev3-leaf-xadb-20261020-r1.json), [하위 회귀](../evidence/dev3-dev2-regression-xadb-20261020-r1.json), [DEV2 복구 회귀](../evidence/dev3-dev2-kernel-xadb-20261020-r1.json)는 실행기가 생성한 bytes 그대로 보존했다.

| 실행 | 실제 결과·한계 |
| --- | --- |
| `python3 -B scripts/check_dev3_leaf.py --with-regressions` | 새 source-only export에서9개 조립 check PASS. 포맷·workspace Clippy/default build는 하위 DEV0에 포함 |
| page Rust 시험 | 7개 PASS. 독립 empty/cells fixture와 전체 bytes 일치, 모든 잘림/단일 byte 손상, 재계산 checksum의 구조 손상, identity·kind·level·정확 공간 경계·오류 시 output 보존·64×128 profile |
| 독립 Python page oracle | 잘림16,384·단일 byte 손상16,384·초과 길이1·checksummed 의미 손상27·identity/kind 불일치7 거절. 정상 경계17·생성 오류2 확인 |
| D3-M02-P01 | 정상4개, 총2,346 states·6,414 transitions, 미탐색0. 지정한 오류의 mutant8개 검출·최단 반례 재생. state/transition/time 예산 강제 소진 self-test PASS. 전체 M02의 완료가 아님 |
| checker 자체 | 고정 receipt 분모·wrong/missing output·JSON type·반례·자손/timeout에 대한6개 시험 PASS |
| 하위 회귀 | DEV0 8·DEV1 8·DEV2 P0/K0 28개 check PASS. native controller256×1,024 요청 및 catalog 확장 profile 포함. PF04·전체 DEV2 승격은 제외 |
| 현재-build DEV2 kernel | 같은 source의 짧은 별도 export에서18개 check PASS. 실제 cache-loss·동시성 및 native33셀×3회=99셀·독립 oracle 포함. 기존 DEV2 공급 경계의 회귀이며 DEV3 page 복구 증거가 아님 |

첫 `artifacts/dev3/leaf-wotbjvtm` 실행은 page/model에 통과했지만 하위 Clippy의 `map_entry`에서 실패했다. [실패 receipt](../evidence/dev3-leaf-xadb-20261020-r0.json)를 보존한다. 수정 후 새 `artifacts/dev3/leaf-q_h1yrhv`에서 최종 검사를 처음부터 실행했다. source export·retained binary·stdout·각 log hash·model witness/counterexample은 이 bundle에 남아 있다. DEV2 kernel의 짧은 export는 `/home/et16/x3k-iag4ivl3`이며 handoff와 명령 로그도 같은 leaf bundle에 보존했다.

실행 증거는 Linux 로컬 개발 환경에 귀속한다. 새로운 계측 성능 비교·30분 DEV3 fuzz/thread·공식 배포·보호 CI는 실행하지 않았다. 계약의 byte/pin 논리 검증과 실제 디스크 엔진 인수를 구별한다.

<a id="tree-overflow-transaction"></a>
## tree·overflow·거래 표현 통합

첫 commit `d58c26b` 다음 묶음은 [tree](../contracts/dev3-tree-format.md), [overflow](../contracts/dev3-overflow-format.md), [거래 표현](../contracts/dev3-transaction-format.md)의 무효과 codec이다. tree는 정렬·child identity·routing·byte 기준 split 위치를, overflow는 고정 chunk·완결 순서·주소 중복·전체 digest를 검산한다. 거래 표현은 TxnId/TxnRef·물리 StatusSlotRef·논리 행 버전·VersionStamp와192-byte status cell을 구분한다. 실제 tree 변경·파일 I/O·할당·상태 전이·참조 pin·회수·복구 obligation은 아직 연결하지 않았다.

| 독립 검토 | 확인한 내용·실행 범위 |
| --- | --- |
| tree: `/root/dev3_model` | 전체 계약·source/tests와6개 독립 golden, 정렬/child alias·분할 보존 검토. 별도 보존 사본에서9개 시험 PASS, `/tmp/xadb-tree-independent-review-0lkwgrdk/validation.json`. root가 DB literal의 `0x11` 표기와 oversize 오류 설명을 정정했으며 source/tests는 불변 |
| overflow: `/root/dev3_supply` | 순서·동일 주소의 incarnation alias·실패 후 verifier 상태 보존·작은 고정 메모리 검토. 독립 Python으로 literal/profile16개 SHA 검산. root가 visited-only cycle·실패한 finish 뒤 계속 처리·empty/2-cell 반례를 보강하고8개 시험 PASS. 2-cell 입력이 정상 chunk를 포함하도록 마지막 fixture를 정정하여 해당 guard를 직접 검증 |
| transaction: `/root` | 전체 byte/state/namespace/lookup·encoder 무효과 검토. Python struct로12개 literal을 production 없이 재구성하여 일치 확인.14개 허용 상태·226개 거절 상태, historical epoch와 물리/논리 generation 구별 확인. 별도 기록 `artifacts/dev3/transaction-independent-root/review.json` |
| leaf runner: `/root/dev3_review` | 실제 Rust tree9/overflow8/transaction8 목록과 runner 대조. 별도7개 자기검사와 malformed 결과59개 모두 거절. `artifacts/dev3/leaf-review-1nwdxoa1/review.json`. hash 검사는 F07 파일 동결·전후 디스크 변경 검출이며 bytecode 인증·compile/read 경쟁 방어·보호 CI가 아님 |

transaction 첫 국소 실행은 V07/V11 hex 전사의 여분 zero로 fixture parser에서 실패했다. `artifacts/dev3/transaction-leaf-33tydcg7`의 source·계약·로그를 보존했다. offset·크기·상태·시험 분모를 바꾸지 않고 전사만 고친 새 실행 `artifacts/dev3/transaction-leaf-6oqixkko`에서8개 시험·engine Clippy·소유 파일 fmt가 통과했다. 실행 당시 계약 hash와 이후 D0 상태 기록을 구별하며 이전 receipt를 덮어쓰지 않았다.

통합 leaf의 보존 source는 `artifacts/dev3/leaf-2lgnrs9h/source`, 지문은 `97171da71471a479063a6d5217d794aebfc7655c7035eadbf4ee178835f547e9`다. 같은 지문의 별도 짧은 export `/home/et16/x3k-ombuok44/source`에서 현재-build DEV2 kernel18개 check, native33셀×3회=99셀·독립 oracle·cache-loss/thread 검사를 통과했다. [원본 복구 receipt](../evidence/dev3-expressions-dev2-kernel-xadb-20261020-r1.json)의 SHA-256은 `a10d020d5cc767fde8379ca8bdca3e9b7b477a07922ad0d3e48612536b233668`이다. DEV2의 기존 capability 회귀이며 DEV3 disk 복구 인수로 확대하지 않는다. 같은 source의 [leaf 통합 원본](../evidence/dev3-expressions-leaf-xadb-20261020-r1.json)과 [하위 회귀 원본](../evidence/dev3-expressions-dev2-regression-xadb-20261020-r1.json)도 PASS다. engine 표현43개(page7/key5/anchor4/UndoRef2/tree9/overflow8/transaction8), foundation scalar3개, 부분 모델 정상4/결함8을 확인했다. DEV0/1과 DEV2 P0/K0 회귀는 해당 runner의 고정 범위이며 성능·30분 stress·DEV3 실제 disk 인수는 포함하지 않는다.

<a id="wal-allocation-row-doublewrite"></a>
## WAL·할당·행·doublewrite 표현의 독립 검토

두 번째 단계 commit `519553b` 뒤 네 표현을 함께 연결한다. 공통 page의 kind/level 검사 구현은 바꾸지 않고 crate 내부에서 소비할 수 있도록 visibility만 넓혔다. 실제 파일 I/O·selector·allocator·visibility·WAL append/recovery는 아직 구현하지 않았다.

| 표현 | 작성 / 독립 검토 | 국소 검증·보존 |
| --- | --- | --- |
| WAL frame·page redo | `/root/dev3_model` / `/root/dev3_review` | `artifacts/dev3-wal-model-r1/validation.json`의11시험·Clippy PASS. 별도 `artifacts/dev3/wal-review-qyev9udk/review.json`에서11시험과 추가2검사 PASS: 정상 prefix1,365·잘못된 prefix250·재checksum 손상14 |
| extent allocator | `/root/dev3_supply` / `/root/dev3_model` | `artifacts/dev3-allocation-supply-r1/validation.json`의8시험·Clippy PASS. 별도 `artifacts/dev3-allocation-independent-r1/validation.json`에서도8시험 PASS |
| canonical row·row/index version | `/root/dev3_review` / `/root/dev3_supply` | `artifacts/dev3/row-leaf-47e3v90a/validation.json`의10시험·Clippy PASS. 별도 `artifacts/dev3-row-independent-r1/validation.json`에서10시험과 독립 literal16개·overflow head 재구성 확인 |
| selected DWB control·slot | `/root/dev3_model` / `/root` | 계약·source·10시험의 독립 정적 검토는 `artifacts/dev3/doublewrite-independent-root/review.json`. 작성자 최종 `artifacts/dev3-doublewrite-model-r2/validation.json`에서10시험·Clippy·소유 fmt PASS. root가 최종2줄 시험 diff와 상태 기록을 추가 검토한 `artifacts/dev3/doublewrite-independent-root-r2/review.json`도 PASS. 통합 실행은 별도 |

WAL은 partial header의 embedded TxnId DB/timeline/reserved를 도착한 byte부터 대조하고 epoch 필드가 완성되면0을 거절한다. tail 예외는 caller가 증명한 active/unrequired EOF suffix에만 제한한다. byte frame이 semantic group·obligation·실제 recovery를 인증하는 것은 아니다. allocation은64개 항목 전체를 검증하며 namespace/claimant/owner/generation을 대조하지만 실제 token 발급은 수행하지 않는다. row/index의 논리 버전은 각각 해당 entry의 마지막 변경을 뜻하며 unrelated row update 뒤 항상 같다고 가정하지 않는다.

row 첫 Clippy `collapsible_if` 실패는 `artifacts/dev3/row-leaf-reo9zm7v/`에 보존했다. allocation의 첫 Clippy도 같은 병렬 row 실패의 영향을 기록했으며 수정 후 새 로그를 남겼다. allocation/row production 의미나 고정 corpus는 축소하지 않았다. DWB 첫 formatter의 예약어 `gen` 오류, 실수로 live cwd에서 실행한10시험의 NOT_QUALIFIED 기록, 첫 보존 실행의 Clippy `needless_range_loop` 오류는 각각 원본을 보존한다. 마지막 수정은 같은 전체 entry를 enumerate하는 시험 코드이며 production bytes/판정은 바뀌지 않는다.

DWB는 최초 초안의 image-only1,024개 계산을 독립 검토에서 수정했다. 전체16 MiB 안에 control/selector/staging64 KiB와 header·image·checksum을 포함한16,544-byte slot1,010개가 들어간다. 선택 Pending control의 batch id와 slot sequence를 결합하여 target/image가 같아도 이전 batch slot을 거절한다. caller buffer·작은 고정 scratch만 사용하며 정적 호출경로 확인을 runtime allocation 계측으로 주장하지 않는다.

leaf runner는 네 모듈의11/8/10/10개 이름을 고정하여 같은 보존 engine binary에서 실행하고, 각각 정확한 이름·분모·summary와 네 계약 hash를 확인한다. `/root/dev3_review`가 등록 diff·원문과7개 자기검사를 독립 검토하여 PASS했다. `artifacts/dev3/leaf-inventory-review-lc4qe5u9/`에 checker 사본·로그를 보존했다. 합친 source 지문은 `e419f2d74f954c3926f70cf7ac39626e8171b09fc50bb0925c707b2c9fedb0f7`, 보존 경로는 `artifacts/dev3/leaf-iupungbw/source`다. 동일 지문의 짧은 export `/home/et16/x3k-9m6a7i9c/source`에서 현재-build kernel18 check·native99셀·cache-loss/thread·독립 oracle PASS를 확인했다. [원본 복구 receipt](../evidence/dev3-storage-formats-dev2-kernel-xadb-20261021-r1.json)의 SHA-256은 `d3d65e03ab68c6f5f0598a4daadedfc0575358a89963d83be000577badb7cf1b`다. 같은 source의 [leaf 통합 원본](../evidence/dev3-storage-formats-leaf-xadb-20261021-r1.json)과 [하위 회귀 원본](../evidence/dev3-storage-formats-dev2-regression-xadb-20261021-r1.json)도 PASS다. engine 표현82개·foundation scalar3개·부분 모델 정상4/결함8·runner 자기검사7개와 DEV0/1·DEV2 P0/K0 고정 회귀를 확인했다. 새 catalog/undo/control 병렬 구현은 이 지문과 다음 커밋으로 분리하며 이 실행 결과를 적용하지 않는다. 성능·30분 stress·실제 DEV3 disk 인수는 포함하지 않는다.

[실제 표현 크기와 자원 계산](../contracts/dev3-resource-bounds.md)은 위 codec 길이를 F05 입력으로 연결한다. 기본8 GiB로 allocator inventory의64 MiB buffer×4 인수를 만족할 수 없음을 독립 산술 검토했다. 제품 기본을 유지한64 GiB 인수 준비 방향과 실제 지원/실행은 구별한다. opcode별 예약과 undo/control/descriptor·실제 I/O 검증은 남는다.

<a id="catalog-undo-control"></a>
## Catalog·undo·control 표현의 독립 검토

세 번째 단계 commit `c800e81` 뒤 catalog6·undo12·control10개 시험을 가진 표현을 연결했다. root가 catalog를, `/root/dev3_review`가 undo를, `/root/dev3_supply`가 control을 작성했다. 공유 module·runner·색인·통합은 root가 소유한다. 선택 파일의 실제 I/O 확장은 다음 묶음이며 이 표현의 검증 지문에 포함하지 않는다.

| 표현 | 독립 검토·별도 실행 | 결과와 한계 |
| --- | --- | --- |
| catalog | `/root/dev3_model`, `artifacts/dev3/catalog-independent-model-r1/review.json` |6시험·독립 literal reader6개 PASS. 다른 DB/timeline의 index/table view 결합을 거절. 생성8,192개는 table 홀수 열 수·index arity2/4만 가지며 전체 열32와arity1..4는 별도 경계/행렬이 검사 |
| undo | `/root`, `artifacts/dev3/undo-independent-root-idhtzexi/review.json` |12시험 PASS. 독립 검토에서 Free/Releasing 상태·진행점 accessor 누락을 보완. Releasing before-image 노출은 거절하며 실제 undo 적용·회수는 미구현 |
| control | `/root`, `artifacts/dev3/control-independent-root-_g_0xw1y/review.json` |10시험 PASS. 선택 reference·profile exact tuple·FILEMAP range/누적 count·실패 cursor 보존·canonical data 이름을 검토. 실제 DB open/BOOT_RESERVE·선택 파일 설치는 미구현 |
| runner 등록 | `/root/dev3_supply`, `artifacts/dev3/leaf-registration-independent-supply-r1/review.json` |실제 컴파일된 신규28시험 이름·세 계약·receipt 필드 대조, 자기시험7개 PASS, 잘못된 합성 결과30종 거절. 전체 제품28시험 실행과 구별 |

catalog 첫 보존 실행 `artifacts/dev3/catalog-leaf-ci_khgn4/validation.json`은6시험·Clippy·fmt 모두 PASS했다. undo의 `undo-leaf-r1`은12시험 PASS 뒤 Clippy `needless_range_loop`로 FAIL했으며, r2 수정과 r3 accessor 보완의 별도 source/로그를 남겼다. control의 `control-supply-r1`은10시험 PASS 뒤 Clippy `nonminimal_bool`로 FAIL했고 같은 의미의 검사식으로 고친 r2가 PASS했다. 고정 분모를 줄이지 않았으며 이전 실패와 원본 receipt를 보존한다.

이 leaf는 caller-buffer 표현이다. UndoRoot의 previous 두 edge만 저장된 sequence를 가지며 Row/Index/Catalog/TxnHead의 UndoRef40에 없는 sequence를 읽었다고 주장하지 않는다. catalog occupied before body는 undo에 아직 연결하지 않는다. FILEMAP의 최대8,192-entry wire 경계와 generated536,448 entries는 기본8GiB admission·실제 inventory 접근량의 증거가 아니다. control 고정 소계23,130,112 bytes도 전체 F05를 대신하지 않는다. `/root/dev3_supply`가 [자원 계산](../contracts/dev3-resource-bounds.md)의 신규6행을 source·계약과 별도 대조하여 산술과 범위가 일치함을 확인했다(문서 SHA-256 `244c4a0355ad4e8d5c3f94789eb640f344342f7c4fb4481ef26fa3c832a63644`).

통합 source는 `/home/et16/x3q-47fzzb7b/source`에 보존했으며 지문은 `f54bf8744a9b3c77caa7d4787e53ac3567bb03ca387369b1cfb6649c45ea2fc9`다. 같은 지문에서 [leaf 원본](../evidence/dev3-metadata-formats-leaf-xadb-20261021-r1.json)의24개 check와 engine 표현110개·foundation scalar3개·부분 모델 정상4/결함8·runner 자기시험7개가 PASS했다. [하위 회귀 원본](../evidence/dev3-metadata-formats-dev2-regression-xadb-20261021-r1.json)의28개 check에서 DEV0/1과 DEV2 P0/K0의 고정 범위·확장 worker 이력을 통과했다. [같은 소스의 복구 원본](../evidence/dev3-metadata-formats-dev2-kernel-xadb-20261021-r1.json)의18개 check와 native99셀·cache-loss/thread·독립 oracle도 PASS다. 복구 receipt SHA-256은 `a9ece36a8c2eacade0a17b545419de341d15adc08d4f525a57615c1d1cc3f79d`다. 이 결과는 실제 DEV3 disk 효과나 전체 인수가 아니다.

<a id="remaining"></a>
## 다음 구현의 경계

1. 남은 recovery obligation·의미 WAL group·catalog undo 연결을 구체화한다. 검증한 page/key/tree/reference/status/row/WAL/allocator/DWB 표현은 실제 target 존재·선택·효과를 보장하지 않는다.
2. M01–M06의 고정 유한 범위 검증을 실제 구현의 참조·진행·예약 프로토콜로 연결하고 opcode별 산식을 완료한다. 모델의 제한을 production 병렬 실행에 그대로 가정하지 않는다.
3. F04/F05의 실제 I/O fault·opcode 자원표·rollback/flush 진행을 동결한 뒤 allocator·buffer·WAL/DWB를 통합한다. DEV2와 새 DEV3의 current-build recovery·실제 thread·DS/PF 시험을 별도로 실행한다.

DEV2 PF04 실패, DEV3 DS01–06·PF09·S1 인수는 남아 있다. 사용자의 후속 지시로 단계별 commit을 수행하며 push·전체 문서 검증은 아직 실행하지 않았다.

<a id="paused-20261021"></a>
## 2026-10-21 사용자 요청에 따른 중지 경계

사용자가 단계 commit 뒤 중지를 요청하여 catalog·undo·control 표현의 통합 검증을 마지막 실행으로 삼는다. 병렬 작업도 중단했으며 다음 작업은 자동으로 진행하지 않는다.

- FILEMAP/DWB selector port는 독립 계약 검토 뒤7개 source/test 경로에 초안을 작성했지만 compile·시험·Clippy·fmt·독립 소스 검토는 모두 NOT_RUN이다. 이 초안은 표현 검증 commit에서 제외하여 working tree에 보존했다. `artifacts/dev3/selector-port-paused-r1/status.json`, `source/`, `contract.md`, `tracked-draft.patch`에 재개용 원본과 hash가 있다. 별도 실행 세션은 없다.
- recovery obligation 초안의 최종 hash는 `afe4fc433ab62fa0c58aa6be3ed5172540710c576554442ac0f5f3e679af2a04`다. status incoming count의224-byte home·queue/cursor·최소 opcode projection·terminal 작업 소유 인계는 아직 채택/동결 전이다. r0 vector는 원래 초안 구조 검토 범위이며 새 의미의 제품 검증이 아니다.
- 실제 page read의 최소 I/O 단위는 조사만 했고 새 계약·제품 코드를 작성하지 않았다. 재개 시 selector source의 첫 검증/독립 검토와 이 단위의 F04/F05 경계를 먼저 확정한다. page read에는 전체 FILEMAP 검증·정확한 파일 길이·열린 handle과 metadata의 결합·generation 고정이 필요하며, root16KiB와 chunk/page 재사용16KiB의 scratch 제안은 아직 미채택이다.

전체 DEV3는 미완료이며 DEV2 PF04 실패도 그대로다. 현재 단계의 source 지문·raw receipt와 미검증 초안을 구분하여 재개한다.
