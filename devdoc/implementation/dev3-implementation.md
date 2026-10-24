# DEV3 구현·검증 현황

2026-10-23 · **표현·유한 모델, 제한 allocator/Status 쓰기·flush·닫힌 복구, 등록 코어·동일 주소 reload의 선택 검증 / 일반 저장 경로·DEV3·제품 S1·최종 S5 미완료**

[설계](../contracts/disk-storage-design.md) · [구현 계획](../plans/dev3-implementation-plan.md) · [단계 완료 정본](../plans/stage-completion.json)

2026-10-23 [진행 현황 후속 리뷰](../evidence/dev3-progress-review-20261023.md): 최신 allocator reload 구현·시험 도구와 기존 snapshot 수정, 단계별 공급·잔여 인수를 대조했다. 요약 표·단계 JSON·색인의 오래된 진행 설명을 수정했다. 새 제품 결함은 검토 범위에서 확정하지 않았으며 제품 코드 변경·시험 재실행은 없다.

2026-10-23 [D3-OCR-001 후속 수정](../evidence/dev3-code-review-20261023.md#fix-followup): reserve/cancel의 WAL 영속화 후 최종 snapshot 실패를 원인 보존·`CallerMustResolve`·공통 fence로 처리하고 설치 의무를 유지한다. 신규 실제 경합/정상/Status wrapper5건 PASS, 변경 파일 fmt·관련 engine Clippy PASS, 별도 D1 검토 수용. 최초 반례 FAIL과 기존 결과는 보존하며 [추가5 inventory](../plans/dev3-test-plan.md#post-durable-snapshot-inventory)를 최종 S5에 연결했다. 전체 DEV3/S5는 미완료다.

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

<a id="resumed-20261021"></a>
## 2026-10-21 DEV3 완료 목표 재개

사용자가 중단 지점부터 DEV3 전체 완료를 요청하여 위 중지를 해제했다. 작업·계약·통합 담당은 `/root`, selector 독립 검토는 `/root/dev3_selector_review`다. 기준 commit은 `d96bd1a9475f3a77fe156230fad86179c69d5df3`, 착수 tracked diff SHA-256은 `3c153087da5f2d5ab33c8346ce218e6c08f059dab2698a8ddb1b9b5ec2c62f22`다. 새 `fault_tests.rs`를 포함한 실행 입력 지문은 `e8d134580b646dcee5302559c780248025a940d81c37175a24ca0e8e2e5d41cf`다. 기존 DEV6 관련4개 문서 변경은 보존하며 이 작업의 소유 범위에서 제외한다.

root가 중단된 selector7개 source/test 경로의 수정·검증 소유를 이어받는다. 독립 검토자는 source를 수정하지 않고 계약·diff·고정 반례·실제 결과를 별도 확인한다. 공유 구현 기록·selector 계약·등록/runner는 root가 순차 반영한다. 기존 상세 계약 → 초안 compile·고정6시험 → 독립 검토·필요 수정 → 같은 보존 source의 하위 회귀·현재-build 복구 → 새 증거·상태 갱신 순서다. 원본 중지 사본과 실행 실패는 덮어쓰지 않는다.

`artifacts/dev3/selector-resume-r0/source`에서 첫 compile·고정6시험·관련4개 package Clippy를 통과했다. 포맷 검사만 FAIL이며 원본 로그와 receipt를 같은 bundle에 보존했다. 이 결과는 selector 기반 leaf에 한정하고 실제 DEV3 복구·단계 인수로 승격하지 않는다. 후속 실제 page read는 FILEMAP 전체 검증·열린 handle의 길이/identity·고정 generation·bounded scratch의 F02/F04/F05를 먼저 검토한다. 새 source path는 구현 때 등록하고 문서 자동 검사는 push gate에 유지한다.

### 열린 handle metadata 공급

[page 읽기 제안의 §2](../contracts/dev3-page-io.md)는 `/root/dev3_selector_review`의 별도 검토를 수용한 D2 port다. `/root`가 foundation/platform `namespace.rs`, engine-worker `fault.rs`·`fault_tests.rs`·`durable_performance/ports.rs`, engine `tests/support/durable_disk.rs`, 새 platform `tests/namespace_metadata.rs`를 단일 작성한다. 기준은 selector r2의 보존 사본 `artifacts/dev3/leaf-m2r6ek3c/source`이며 source/test 새5시험과 exact6/2/10/8 및 계수2호출을 고정한다. 기존 `open_existing`의 호출·fault 의미를 보존하고 새 method에서 handle metadata를 반환한다. 공유 runner·배치/문서 색인은 root가 검토 뒤 순차 연결한다.

선행 selector 통합 사본은 수정하지 않는다. metadata port는 별도 `artifacts/dev3/metadata-port-r0/source`에서 compile·5시험·Clippy·fmt를 실행하며 이전 source의 PASS를 이 변경으로 확대하지 않는다. 실제 reader는 아직 구현하지 않았으며 profile DB 결합·정확한 logical read 길이와 남은 API/fault/시험 조립 의무를 별도 계약에 기록했다.

<a id="selector-resume-results"></a>
### Selector 재개 검증 결과

중단 초안의 원래6시험은 독립 검토에서 통과했다. 첫 통합 r1은 serial `--nocapture`가 시험의 witness 출력을 결과 줄에 끼워 넣어 runner가 정상 실행을 누락으로 판단했다. 출력 순서에 맞는 `--show-output`으로 수정하고 자기시험에 원래 반례를 추가했다. 독립 검토자는 실제 platform2·protocol1·worker3시험과 runner7시험을 별도로 실행하여 수정 결과를 확인했다. 첫 fmt 실패와 runner r1 실패는 `artifacts/dev3/selector-resume-r0/`와 `selector-resume-leaf-r1.json`에 보존했다.

최종 selector source 지문은 `a270ffcfedf4c3696ba131fa0842366625c47ccc410f59afb734afe63777dded`, 보존 경로는 `artifacts/dev3/leaf-m2r6ek3c/source`다. [leaf 원본30check](../evidence/dev3-selector-leaf-xadb-20261021-r2.json), [하위 회귀 원본28check](../evidence/dev3-selector-dev2-regression-xadb-20261021-r2.json), [현재-build 복구 원본18check](../evidence/dev3-selector-dev2-kernel-xadb-20261021-r2.json)가 같은 지문에서 PASS했다. 복구는 native99셀·cache-loss·thread·독립 oracle 범위이며 DEV2 PF04와 DEV3 전체 인수를 완료한 것으로 해석하지 않는다.

metadata port는 [원본 receipt](../evidence/dev3-metadata-port-xadb-20261021-r0.json)의 source `311b9964dd0b64431f8014c8c4829f2beb28415e4adc0760a0665a5699642eee`에서5시험·Clippy·fmt PASS와 별도 독립 검토를 마쳤다. 보존 소스는 `artifacts/dev3/metadata-port-r0/source`, 검토는 `artifacts/dev3/page-io-contract-review-20261021/metadata-port-review.md`다. 이후 page reader를 합친 소스의 하위 회귀는 별도로 실행한다.

<a id="page-reader-results"></a>
### 선택 page reader 구현과 검증

`/root`가 `storage/page_io.rs`·`page_io_tests.rs`, test-support feature의 `testing.rs`, worker `tests/page_io.rs`, 독립 `scripts/dev3_disk_vectors.py`와5 literal, Cargo·module·runner 등록을 작성했다. `/root/dev3_selector_review`가 별도 D2 계약·구현·runner 검토를 맡는다. 검토한 원문은 `artifacts/dev3/page-io-contract-review-20261021/r3-reader-proposal.md`이며 root32KiB workspace와 최대2 file handles, profile 결합과 모든 selected chunk 검증 후 요청 page를 읽는 경로다. 정상 engine API에 READY를 추가하지 않는다.

첫 컴파일은 Namespace private import와 façade의 Result/Option 혼동을 발견하여 수정했다. 원본 `artifacts/dev3/page-reader-r0/tests.log`·`tests-r1.log`를 보존한다. 내부5시험·read fault56셀과 native 시험은 통과했으나 독립 검토에서 비선택 bank1 fixture의 header/reference가0인 결함을 발견했다. slot1 header와 전이 hash를 바로잡고 bank1 정상 선택을 내부·native에 추가했다. 기존8거절·56fault를 유지하고 native5셀에 정상1셀을 추가했으며, DB mismatch의 data read0도 명시 검사한다. 최종 자체 실행은 `tests-r4.log`·`native-r1.log`다.

통합 r1은 추가 assertion 반영을 위해 하위 회귀 전에 중단했으며 `page-reader-leaf-r1.json`에 FAIL과 종료한 child를 보존한다. 최종 source `8e86232efcf44310a22b4607c3ff0e70348f1258e05ce8223af1062462735329`를 `artifacts/dev3/leaf-_zivd6il/source`에 보존했다. [r2 leaf40check](../evidence/dev3-page-reader-leaf-xadb-20261021-r2.json)·[하위 회귀28check](../evidence/dev3-page-reader-dev2-regression-xadb-20261021-r2.json)와 같은 지문의 짧은 export `/home/et16/x3r-r0z3aarf/source`에서 [현재-build DEV2 복구18check](../evidence/dev3-page-reader-dev2-kernel-xadb-20261021-r2.json)가 PASS했다. engine 표현·reader115시험, selector6·metadata5·native reader1시험(6셀), 기존 scalar3·부분 모델 정상4/결함8과 DEV0/1·DEV2 P0/K0의 고정 회귀를 확인했다. DEV2 복구는 native99셀·cache-loss/thread·독립 oracle 범위다. 첫 kernel handoff r2는 임시 Python bytecode가 포함된 다른 지문이어서 실행하지 않았고 r3 handoff에서 leaf와 exact 지문을 확인했다. [품질·feature 기록](../evidence/dev3-page-reader-quality-xadb-20261021-r2.json)은 Clippy·fmt PASS와 engine 기본 features[] 및 native 조립의 test-support 활성화를 구별한다. 이 기록의 “full regression pending”은 품질 기록 생성 당시 상태이며 완료된 회귀는 위 별도 원본이 증명한다. 전체 DEV3 disk 복구·S1 인수는 미완료다.

사용자의 디스크 정리 요청에 따라 완료된 native worker 실행 파일은 manifest SHA-256·전체 bytes·mode/device가 같은 사본만 hardlink로 통합했다. 데이터 파일·보존 source·로그·receipt 경로와 내용은 유지하며 이후 그 실행 파일을 수정하지 않는다. `artifacts/dev3/completed-worker-cleanup-20261021/summary.json`과 journal에4,814개 통합·약104.29GiB 회수, `incremental-cache-cleanup.json`에 완료된 빌드 캐시 약8.29GiB 제거를 기록했다. 실행 중인 source와 target은 제외했다.

최종 독립 증거 검토 `artifacts/dev3/page-reader-independent-review-20261021/r2-final-evidence-review.json`은 retained source 두 사본·계약16개·checker3개·로그104개·asset58개를 대조하고 위 좁은 통합 범위의 PASS를 확인했다. 기본 feature와 시험 feature, page-reader 지문과 후속 status-home 지문을 구별한다.

<a id="status-home-results"></a>
### 상태 core와 영속 incoming count의 결합

selector·선택 page reader 단위를 `1811579`로 commit한 뒤, `/root`가 [224-byte 상태 home](../contracts/dev3-status-home.md)의 private codec·page loader와7시험·3개 독립 literal을 연결했다. 논리 core192·기존82-slot 표현은 보존하고 새 home loader는71-slot 한도·모든 cell·DB/timeline·동일 TxnId 중복·참조 generation을 확인한다. 순수 incoming 계산은 제거를 먼저 검사하므로 net0이어도 기존 count보다 많은 edge를 제거할 수 없다. 실제 count 변경·queue·terminal·WAL 적용은 미구현이다.

국소 source `ea3ba171acbcb22147fbb06f60738215d67bf74694e3b41d3c0ac9764131d885`의 [원본 실행](../evidence/dev3-status-home-local-xadb-20261021-r0.json)은7시험·Clippy·fmt PASS다. count64셀(정상29·거절35), 독립 입력식8,192개, embedded core 부정23개를 검증했다. `/root/dev3_selector_review`의 `artifacts/dev3/status-home-independent-review-20261021/r1-evidence-review.json`은 실제 binary를 별도 재실행하고3 literal을 독립 재구성했다. 이 국소 source에는 하위 회귀를 실행하지 않았다.

runner 등록 뒤 source `dd7163fbe37164c2c1a6ae75304ad416e9a1407583ae4e9391191c2bca72e790`를 `artifacts/dev3/leaf-g7shzdyd/source`에 보존했다. [같은 source의 현재-build DEV2 복구](../evidence/dev3-status-home-dev2-kernel-xadb-20261021-r1.json)18check·native99셀과 [Clippy·fmt](../evidence/dev3-status-home-quality-xadb-20261021-r1.json)는 PASS다. [통합 leaf41check](../evidence/dev3-status-home-leaf-xadb-20261021-r1.json)·[하위 회귀28check](../evidence/dev3-status-home-dev2-regression-xadb-20261021-r1.json)도 PASS했다. engine 표현·reader122시험과 기존 selector·metadata·native reader·부분 모델 및 DEV0/1·DEV2 P0/K0의 고정 범위다. 실제 DEV3 복구·S1 인수는 미완료다.

완료된 page reader 실행의 worker890개를 같은 manifest SHA·bytes의3개 binary로 hardlink 통합하여 약19.49GiB, 완료된 disposable Cargo target을 제거하여 약3.90GiB를 추가 회수했다. 검증 binary·source·로그와 data는 보존했다. 원본 journal과 요약은 `artifacts/dev3/page-reader-worker-cleanup-20261021/`에 있으며 상태 home의 실행 중 target은 제외했다.

최종 독립 검토 `artifacts/dev3/status-home-independent-review-20261021/r3-final-evidence-review.json`은 같은 지문의 source 두 사본·승인 제품 입력7개·로그105개·고유 asset 경로1,392개와 계약17개·loaded checker3개를 대조했다. 현재-build kernel의3×33개 결과·consistency·controller negative inventory도 확인하여 이 좁은 상태 home 단위의 commit 차단 사항이 없음을 기록했다.

<a id="obligation-descriptor-results"></a>
### 복구 obligation descriptor 표현

상태 home 단위를 `63c0ebd`로 commit한 뒤 `/root`가 [obligation §12](../contracts/dev3-obligation-format.md#12-독립-descriptor-표현-leaf-제안)의 private `obligation.rs`·`obligation_tests.rs`,3개1536-byte literal과 고정8시험을 구현했다. 5개 budget의 checked 합/peak, growth bytes와 item cardinality, full owner/profile·chain·SystemProgress를 검증한다. 실제 inventory·이전 descriptor나 profile 파일을 읽지는 않으며 admission·WAL 의미 적용·terminal 인계는 미구현이다.

독립 검토의 초기 지적에 따라 u128 cardinality 상·하한과 conditional reserved corpus를 보완했다. 국소 source `33aea0a6aa08aa609efa2bb34406a09ebf676e1d1733e7098720c66d0b294154`의 [원본 실행](../evidence/dev3-obligation-local-xadb-20261021-r0.json)은8시험·Clippy·fmt PASS다. 기존 r0 literal3개의 의미와 hash는 보존했고 4,608개 bit0 손상·잘림·재checksum 논리 오류·8,192개 독립 입력식을 검증했다. `artifacts/dev3/obligation-independent-review-20261021/r1-retained-evidence-review.json`은 실제 보존 binary의8시험을 별도로 재실행했다. 이 국소 source에는 하위 회귀를 실행하지 않았다.

runner의8개 exact 시험 등록은 `artifacts/dev3/obligation-independent-review-20261021/r2-runner-review.json`으로 별도 검토했다. 통합 source `7dabd13a01a71460f68fa3644a94803943e11501fd6c8ff92127d0b686a8827e`를 `artifacts/dev3/leaf-_4j_9p1u/source`와 `/home/et16/x3o-q75pibfb/source`에 보존했다. clean export에서 실행한 최초 leaf r1은 `.git` identity가 없어 제품 실행 전에 실패했고 원본 log와 launch-failure receipt를 보존했다. repository에서 같은 source의 r2를 실행했다. [현재-build DEV2 kernel18check·native99셀](../evidence/dev3-obligation-dev2-kernel-xadb-20261021-r1.json)과 [Clippy·fmt](../evidence/dev3-obligation-quality-xadb-20261021-r1.json)는 PASS다. [통합 leaf42check](../evidence/dev3-obligation-leaf-xadb-20261021-r2.json)·[하위 회귀28check](../evidence/dev3-obligation-dev2-regression-xadb-20261021-r2.json)도 PASS했다. engine 표현·reader130시험 및 기존 selector·metadata·native reader·부분 모델과 DEV0/1·DEV2 P0/K0의 고정 회귀다. 실제 DEV3 복구·S1 인수는 미완료다.

완료된 상태 home worker890개를3개 동일 binary로 통합하여 약19.49GiB를 추가 회수했다. `artifacts/dev3/status-home-worker-cleanup-20261021/summary.json`과 journal은 manifest SHA·bytes 대조 및 원본 data/source/log 보존을 기록한다. 실행 중 descriptor bundle은 제외했다. 별도 `build-cache-cleanup.json`에 완료된 disposable Cargo target 약3.58GiB 제거와 보존 binary 확인을 기록했다.

완료된 descriptor 통합 실행도 `artifacts/dev3/obligation-worker-cleanup-20261021/summary.json`과 journal에서 worker890개·3개 동일 binary를 대조하여 약19.49GiB를 추가 회수했다. SHA 불일치로 보존해야 할 사본은0개였으며 검증 경로·bytes와 data는 유지했다.

최종 독립 검토 `artifacts/dev3/obligation-independent-review-20261021/r3-final-evidence-review.json`은 retained source 두 사본·승인 제품 입력10개와 runner2개·계약18개·checker3개·로그106개·고유 hash 경로1,394개를 대조했다. native99의 result/consistency·완전한 inventory와 binary hash를 확인했고, kernel engine의159 PASS와 기존30분 fuzz1 ignored를 구분했다. 이 descriptor 표현 단위의 commit 차단 사항은 없으며 전체 DEV3 인수로 승격하지 않는다.

<a id="ownership-observation-results"></a>
### Ownership bytes·합계·물리 충돌의 bounded 관측

`/root`가 [ownership 검산 계약](../contracts/dev3-ownership-format.md)의 제품 구현·통합을, `/root/dev3_selector_review`가 독립 검토를 맡는다. 선행 통합 source는 `7dabd13a01a71460f68fa3644a94803943e11501fd6c8ff92127d0b686a8827e`이며 descriptor 단위 commit은 `d94bb32`다. 독립 D2 수용 뒤 private `ownership.rs`와 시험·9 literal, ControlKind5/6과 validate-only descriptor helper를 단일 작성한다. 공유 module·runner·색인·배치 등록은 root가 순차 반영한다.4×16KiB scratch와 고정 owner 통계·4-way merge로 관측 bytes/합계/물리 충돌을 확인하며 실제 intent 유일성·slot 상태·선택 source 고정·RESERVE_RESTORE 성공은 제공하지 않는다. 제품 compile과 초기 고정12시험·관련 control10/descriptor8·Clippy/fmt를 통과했다. 독립 검토에서 category 검산이 count 오류에 가려지는 시험 공백을 찾아 총합을 보존하는30개 반례를 추가했다. chunk의 선언 count/길이 모순이 identity 오류로 분류되던 구현도 Corrupt로 수정했으며 수정 후12시험이 PASS했다. 최종 보존 source의 독립 재현·하위 회귀 결과는 아래와 같다.

완료된 descriptor leaf의 disposable Cargo target도 `artifacts/dev3/obligation-worker-cleanup-20261021/build-cache-cleanup.json`에서 정리했다. 별도 보존10개 binary의 SHA를 전후 확인했으며 약3.38GiB를 회수했다. 현재 ownership 검증에 쓰는 target은 유지한다.

최종 국소 source `31ab0f0140d244ef21826292cb986790cffac2a8e71f5ed5c8d10650e89e42bc`는 `artifacts/dev3/ownership-retained-r2/source`에 보존했다. [국소 원본](../evidence/dev3-ownership-local-xadb-20261021-r2.json)의 ownership12·control10·descriptor8·Clippy/fmt는 PASS다. 독립 검토 `artifacts/dev3/ownership-independent-review-20261021/r2-retained-evidence-review.json`은 실제 보존 binary의30시험과 source/binary/hash를 재현했다. 초기 r0/r1은 count/길이 오류 분류 수정 전 사본으로 원래 결과를 보존하며 최종 소스로 대체 해석하지 않는다. 같은 source의 [DEV2 kernel18check·native99셀](../evidence/dev3-ownership-dev2-kernel-xadb-20261021-r1.json), [통합 leaf43check](../evidence/dev3-ownership-leaf-xadb-20261021-r1.json), [하위 회귀28check](../evidence/dev3-ownership-dev2-regression-xadb-20261021-r1.json)도 PASS했다. 통합 source는 `artifacts/dev3/leaf-8v68xoiu/source`와 `/home/et16/x3i-egjna3v3/source`에 보존했다. engine 표현·reader142시험 및 기존 selector·metadata·native reader·부분 모델과 DEV0/1·DEV2 P0/K0의 고정 회귀다. 실제 inventory slot 상태·현재 intent 유일성·복구 예약 복원과 전체 DEV3 인수는 미완료다.

완료된 초기 ownership r0/r1 target에서 약0.43GiB, kernel target에서 약0.49GiB를 제거하고 보존 binary hash를 확인했다. kernel worker206개를1개 동일 binary로 hardlink 통합하여 약5.03GiB를 추가 회수했다. `artifacts/dev3/ownership-cache-cleanup-20261021/`과 `ownership-kernel-worker-cleanup-20261021/`에 경로·전후 hash·journal이 있다. 현재 실행 중인 leaf는 제외했다.

최종 독립 검토 `artifacts/dev3/ownership-independent-review-20261021/r4-final-evidence-review.json`은 같은 source의 leaf43·하위 회귀28·DEV1/DEV0 각8·kernel18check 결과와 계약19개·checker3개·로그111개·asset hash 참조474개를 대조했다. kernel crash 이미지의 상세 해시는 선행 `r3-kernel-partial-review.json`의3,550개 참조 대조와 연결했다. 추가 제품 재실행이나 전체 DEV3 인수로 표시하지 않으며, 이 ownership 관측 단위의 commit 차단 사항은 없다.

완료된 ownership leaf의 worker684개를 동일 binary2개로 통합해 약14.46GiB를 회수했다. `artifacts/dev3/ownership-leaf-worker-cleanup-20261021/`에 전체 journal이 있다. leaf와 국소 r2의 disposable target도 별도 보존11개 binary SHA를 전후 검산하고 약3.56GiB 제거했으며 `artifacts/dev3/ownership-cache-cleanup-20261021/final-targets.json`에 기록했다.

<a id="ownership-intent-results"></a>
### 실제 profile과 현재 growth intent 검산

관측 단위를 `93d4a37`로 commit한 뒤 [ownership §7](../contracts/dev3-ownership-format.md#7-실제-profile과-현재-inventory-intent의-결합)의 D2를 독립 수용했다. root는 기존 ownership 두 source/test 파일에서 실제 profile 결합·8GiB growth 상한·128KiB 고정 intent key 검산을 구현한다. 독립 검토자는 기존 `/root/dev3_selector_review`다. 기존12시험·9 literal과 약한 관측 API는 보존하며 별도 고정8시험·실제 profile3개를 추가한다. 국소·kernel 실행과 독립 재현에 이어 같은 source의 하위 통합 회귀도 완료했다.

source `e83efc08cfdd1c0233343dc936b3f682361f9385750cbca155fdb1e66f64836f`를 `artifacts/dev3/ownership-intent-retained-r0/source`에 보존했다. [국소 원본](../evidence/dev3-ownership-intent-local-xadb-20261021-r0.json)의 ownership20·control10·descriptor8과 Clippy/fmt가 PASS다. 현재 intent 생성8,192개는 정상3,157·중복거절5,035이며, 최대8,192개 growth·8193번째 key guard·36-owner quota·u64 byte합 초과·반복source변경을 검증했다. 기존12시험과9 literal은 보존했다. `artifacts/dev3/ownership-intent-independent-review-20261021/r0-retained-evidence-review.json`은 별도 binary38시험 및 source/binary hash를 직접 재현했고, runner 자기시험7개도 확인했다. 같은 source의 [DEV2 kernel18check·native99셀](../evidence/dev3-ownership-intent-dev2-kernel-xadb-20261021-r0.json)은 `/home/et16/x3t-fgbp64iv/source`의 짧은 export에서 PASS했다. [통합 leaf43check](../evidence/dev3-ownership-intent-leaf-xadb-20261021-r0.json)와 [하위 회귀28check](../evidence/dev3-ownership-intent-dev2-regression-xadb-20261021-r0.json)도 PASS했다. 통합 source는 `artifacts/dev3/leaf-zzw0bux9/source`에 보존했으며 engine 표현·reader150시험과 기존 selector·metadata·native reader·부분 모델 및 DEV0/1·DEV2의 고정 회귀를 포함한다. 현재 intent 유일성 외의 과거 재사용 방지·실제 slot 대조·예약 복원·DEV3 전체 인수는 미완료다.

완료된 같은 source의 kernel target 약0.49GiB와 byte-identical worker206개의 중복 약5.03GiB를 정리했다. `artifacts/dev3/ownership-intent-cache-cleanup-20261021/kernel-target.json`과 `ownership-intent-kernel-worker-cleanup-20261021/summary.json`에 별도 보존 binary SHA·원본 경로·전체 journal을 남겼다. 이전에 완료된 leaf3개의 disposable target도 보존 binary8개의 SHA를 전후 대조하고 약5.55GiB 제거했으며 `ownership-intent-cache-cleanup-20261021/older-leaf-targets.json`에 기록했다. receipt가 target 내부 binary를 직접 참조하는 초기 selector 사본과 실행 중인 leaf는 보존했다.

최종 독립 감사 `artifacts/dev3/ownership-intent-independent-review-20261021/r2-final-evidence-review.json`은 같은 source의 leaf43·하위 회귀28·DEV1/DEV0 각8·kernel18/native99, 로그111개·hash 참조474개/183경로·고정26 suite(engine150)·핵심 계약19개·checker3개와 등록 원본4개의 byte 일치를 확인했다. 선행 kernel 상세 감사의 receipt도 같으며 이 제한된 구현 단위의 commit 차단 사항은 없다. 실제 예약 복원·전체 DEV3 완료 증거로 승격하지 않는다.

`ff6617c` commit 뒤 완료된 intent leaf의 worker684개를 동일 binary2개로 통합하여 약14.46GiB, leaf·국소 보존 사본의 disposable target에서 약3.92GiB를 추가 회수했다. `artifacts/dev3/ownership-intent-leaf-worker-cleanup-20261021/summary.json`과 `ownership-intent-cache-cleanup-20261021/final-targets.json`에 전체 경로·hash·journal과 별도 보존11개 binary 검산을 남겼다. source·receipt·로그·crash data는 보존했다.

<a id="rollback-start-results"></a>
### RollbackStart의 descriptor·상태 page·WAL 결합

선행 통합은 `ff6617c`와 source `e83efc08cfdd1c0233343dc936b3f682361f9385750cbca155fdb1e66f64836f`다. 전체 거래의 Active→Aborting(Transaction) 전이를 다음 단위로 준비한다. `/root`가 `storage/semantic_group.rs`·시험과 실제 사용에 필요한 page/status-home/WAL 하위 helper, module·runner·fixture·배치·소유 계약을 순차 작성한다. `/root/dev3_selector_review`는 API·독립 raw 입력·고정 부정 corpus를 검토한다. 별도 source root·dependency·feature나 실제 append/write API는 추가하지 않는다.

`artifacts/dev3/semantic-followup-review-20261021/`의 r0/r1 검토 뒤 r2 API/corpus를 제안했다. `rollback-start-vectors-r1/`의 raw bytes는 거래 head 없음/있음 각각 3040-byte frame·before/after descriptor·full page를 포함한다. 독립 raw 감사는 이들 바이트의 결합을 확인했으며 제품 실행 결과는 아니다. r0 입력에서 reserved PageToken과 실제 상태 page가 겹쳤던 제안은 r1에서 서로 다른 page로 수정했고, r0를 채택 fixture로 재표기하지 않는다. 최종 오류·비용 matrix를 포함한 [obligation §13](../contracts/dev3-obligation-format.md#13-rollbackstart의-full-state-결합-검산)을 D2 수용했다. 채택 입력 SHA는 `7c8ad96387ea38b89c860e7f0b17c8b3bc184dbf87f0ca5d1dac4c27659121c7`이며 제품 코드 작성 전에 동결했다. 고정12시험, raw 입력의 reserved 반례2786개, typed-valid 비용148개와 생성8192개를 구현·검증한다.

최종 국소 source `99bec497054e4095c4c5cbedb7e30ae89f9cc4cbc9910670f04deafabb5b472a`는 `artifacts/dev3/rollback-start-retained-r1/source`에 보존했다. [국소 원본](../evidence/dev3-rollback-start-local-xadb-20261021-r1.json)은 semantic12·WAL11·page7·status home7·descriptor8·control10·transaction8의63시험과 Clippy/fmt PASS다. 독립 검토에서 outer kind를 operation dispatch보다 먼저 검사하도록 수정했고, native checksum에 가려지던 page/body 불일치 반례와 실제 UndoRef 전체 필드·trusted generation 비교를 보완했다. 초기 시험의 잘못된 profile offset·Free 상태 반례와 수정 전 실행 로그는 원래 결과대로 남겼다. `artifacts/dev3/rollback-start-independent-review-20261021/r2-static-review.json`과 `r3-retained-evidence-review.json`은 변경 검토와 보존 binary63시험의 독립 재현·전후 hash를 기록한다.

같은 source의 [DEV2 kernel18check·native99셀](../evidence/dev3-rollback-start-dev2-kernel-xadb-20261021-r0.json)은 `/home/et16/x3r-5atyac6t/source`의 짧은 export에서 PASS했다. 독립 `r4-kernel-partial-review.json`은 source5사본·계약19·checker3·로그28·hash참조3524개/2892경로, engine191 PASS와 기존fuzz1 ignored, 고정33cut×3의 result/consistency99개를 대조했다. 이는 현재 build의 기존 DEV2 복구 회귀이며 실제 DEV3 append/적용/복구 결과가 아니다. [통합 leaf44check](../evidence/dev3-rollback-start-leaf-xadb-20261021-r0.json)와 [하위 회귀28check](../evidence/dev3-rollback-start-dev2-regression-xadb-20261021-r0.json)도 PASS했다. 통합 source는 `artifacts/dev3/leaf-w0p1unmm/source`에 보존했으며 engine 표현·reader162시험, 기존 selector·metadata·native reader·부분 모델과 DEV0/1·DEV2의 고정 회귀를 포함한다.

완료된 초기 국소 target 약0.23GiB와 kernel target 약0.62GiB를 제거하고 별도 보존 binary hash를 확인했다. kernel worker206개는 동일 binary1개로 통합하여 약5.03GiB를 추가 회수했다. `artifacts/dev3/rollback-start-cache-cleanup-20261021/` 및 `rollback-start-kernel-worker-cleanup-20261021/summary.json`에 원본 경로·hash·journal을 남겼고 source·로그·장애 data·receipt는 보존했다.

최종 독립 감사 `artifacts/dev3/rollback-start-independent-review-20261021/r5-final-evidence-review.json`은 같은 source의 전체 결과, 로그116개·asset hash 참조10101개/8442경로·계약19개·checker3개·승인 제품 입력6개·고정27 suite와 등록 원본4개의 byte 일치를 확인했다. 보존 소스의 당시 계약 hash와 이후 상태 문구 정정을 구분했으며, 이 RollbackStart 관측 단위의 commit 차단 사항은 없다. append·page 적용·예약 복원·DEV3 전체 완료 증거로 승격하지 않는다.


`1f85de8` commit 뒤 완료된 RollbackStart leaf의 worker684개를 동일 binary2개로 통합하여 약14.46GiB, leaf·국소 사본의 disposable target에서 약3.94GiB를 추가 회수했다. `artifacts/dev3/rollback-start-leaf-worker-cleanup-20261021/summary.json`과 `rollback-start-cache-cleanup-20261021/final-targets.json`에 전체 journal과 별도 보존11개 binary 검산을 남겼다. source·receipt·로그·crash data는 보존했다.

<a id="read-buffer-results"></a>
### 선택된 clean page의 bounded buffer

선행 commit `1f85de8`과 source `99bec497054e4095c4c5cbedb7e30ae89f9cc4cbc9910670f04deafabb5b472a` 뒤 [읽기 버퍼 계약](../contracts/dev3-read-buffer.md)을 통합한다. D2 r4 채택 입력 결합 SHA는 `3f4fb31c76815daf587d01af63acf1f4678f78acf1ced3a4f70574c215daf554`다. `/root`가 storage/read_buffer·고정10시험·cfg(test) 공통 reader port·page_io의 순수 preflight·module·native test façade와2시험·borrow compiler runner·소유 계약·배치·색인을 순차 작성한다. 기존 `/root/dev3_selector_review`는 독립 D1 검토를 맡는다. 별도 crate·dependency·제품 엔진은 추가하지 않는다. actual selected reader를 사용하지만 dirty page·write/flush·거래 예약·전체 S2/DEV3 완료는 제공하지 않는다. 구현 초안의 compile 및 국소 정적 검토는 전체 시험 성공을 대신하지 않는다.


최종 source `061c0d9ee7ace9d623370513471d3778d41956cd4c6e4b7a141d1e4176ab703c`를 `artifacts/dev3/read-buffer-retained-r2/source`에 보존했다. [국소 원본](../evidence/dev3-read-buffer-local-xadb-20261021-r2.json)은 buffer10·기존 reader5·page7·control10과 native buffer2·기존 native reader1의35시험 및 Clippy/fmt PASS다. 독립 clock model의8,192단계와 실제 두 thread의8,192 ABA cycle, typed-valid alias18·preflight6·read fault16(정상8/오류8)·손상6종을 확인했다. 기존 reader의5시험·56 fault cell과 native6 fixture는 보존했다. [compiler 원본](../evidence/dev3-read-buffer-borrows-xadb-20261021-r1.json)은 unmodified baseline1·positive3 compile과 negative3의 exact E0505/E0499·primary probe span을 확인했고, checker 자기시험6개가 잘못된 진단과 timeout/잔존 프로세스의 거절을 검증했다. 이 결과는 runtime allocation/RSS나 전체 F05를 인증하지 않는다.

독립 D1에서 다른 FileId의 같은 page 번호를 동시에 읽는 반례와 실제 Loading 중 unwind 직후 fence 관측을 보완했다. source `ea367cc3…`의 r1은35시험·compiler7·현재-build kernel18/native99 PASS이나 최종 runner 소스와 구별한다. 초기 leaf r0는 의도적 panic 출력이 `--nocapture` 결과 행을 나누는 문제를 단일 시험으로 재현한 뒤 명시 중단했으며, partial FAIL receipt와 interruption log를 보존했다. buffer suite만 `--show-output`으로 바꿨다. 제품 코드/시험은 바뀌지 않았으며 `artifacts/dev3/read-buffer-independent-review-20261021/r3-static-corpus-review.json`과 `r4-runner-mode-review.json`에 독립 검토를 남겼다. 최초 compile의 closure/타입·Rustdoc 오류와 수정 전 로그도 원래 결과로 보존한다.

같은 최종 source의 [DEV2 kernel18check/native99셀](../evidence/dev3-read-buffer-dev2-kernel-xadb-20261021-r1.json)은 `/home/et16/x3b-f3kamred/source`의 짧은 export에서 PASS했다. [통합 leaf49check](../evidence/dev3-read-buffer-leaf-xadb-20261021-r1.json)와 [하위 회귀28check](../evidence/dev3-read-buffer-dev2-regression-xadb-20261021-r1.json)도 PASS했다. 통합 source는 `artifacts/dev3/leaf-rjjwn_eo/source`에 보존했고 engine 표현·reader·buffer172시험, native buffer·기존 selector/metadata/reader·부분 모델과 DEV0/1·DEV2 회귀를 포함한다. 실제 dirty page·WAL append/sync·write/flush·거래 예약 복원·DEV3 복구와 전체 인수는 남아 있다.

완료된 이전 kernel r0의 worker206개를 동일 binary1개로 통합해 약5.03GiB를 회수했다. 완료/명시 중단된 국소·compiler·leaf·kernel의 disposable target6개에서도 보존 binary hash를 전후 검산하고 약3.34GiB를 제거했다. `artifacts/dev3/read-buffer-old-kernel-worker-cleanup-20261021/summary.json`과 `read-buffer-cache-cleanup-20261021/superseded-targets.json`에 전체 journal과 원래 identity를 보존했으며 final 실행 source·로그·장애 data는 유지했다.


최종 source의 국소35시험·compiler7개 결과는 독립 검토자가 보존 binary를 직접 재실행하고 실제 compiler 진단/primary span을 대조했다(`artifacts/dev3/read-buffer-independent-review-20261021/r5-retained-borrow-review.json`). 최종 DEV2 kernel의18check·native33종×3회와 binary/build·장애 image hash도 별도로 검산했다(`r6-kernel-partial-review.json`). 이 검토 뒤 final kernel의 동일 worker206개를1개로 통합해 약5.03GiB, disposable target에서 약0.63GiB를 회수했다. `artifacts/dev3/read-buffer-final-kernel-worker-cleanup-20261021/summary.json`과 `read-buffer-cache-cleanup-20261021/final-kernel-target.json`에 hash·원래 identity·회수량을 남겼고 보존 실행 binary6개·source·로그·장애 data는 유지했다.


최종 독립 감사 `artifacts/dev3/read-buffer-independent-review-20261021/r7-final-evidence-review.json`은 leaf49·하위28·DEV0/1각8check,29개 고정 named suite의189시험(engine20 suite172개 포함), 원본5개의 byte 일치와 로그123개·asset hash 참조10,173개/8,500고유경로를 확인했다. 소스/시험의 차단 사항은 없었다. 문서 수동 대조에서 발견한 development-stages.json의 이전 leaf_progress를 stage-completion.json의 같은 제한 범위와 맞췄다. 문서 verifier는 편집/commit 중 실행하지 않으며, DEV3 전체 execution_status는 NOT_RUN으로 유지한다.


`d6d62d4` commit의 clean export가 최종 검증 source `061c0d9e…`와 같고 worktree가 깨끗함을 `artifacts/dev3/read-buffer-commit-verification-20261021.json`으로 확인했다. 최종 독립 결정 `read-buffer-independent-review-20261021/r7-final-decision.json`은 이 제한 clean buffer 단위의 commit 차단 사항이 없음을 기록한다. 완료된 leaf의 worker684개는 SHA·전체 bytes·mode·device를 확인한 뒤 동일 binary2개로 통합해 약14.47GiB를 회수했고, 국소/통합/compiler의 disposable target3개에서도 보존 binary14개의 hash를 확인하며 약4.56GiB를 제거했다. `artifacts/dev3/read-buffer-final-leaf-worker-cleanup-20261021/summary.json`과 `read-buffer-cache-cleanup-20261021/final-targets.json`에 경로·hash·journal을 남겼다. source export·raw receipt·로그·compiler probe·장애 data는 유지한다.


<a id="genesis-wal-results"></a>
### 최초 epoch의 physical WAL 설치·append/sync

선행 `74f5d37`와 source `061c0d9ee7ace9d623370513471d3778d41956cd4c6e4b7a141d1e4176ab703c` 뒤 [genesis WAL 소유 계약](../contracts/dev3-wal-io.md)을 채택했다. D2 입력 결합 `47c1bcab04b203135e1b59b25a5e06190dddf364c191f24a555c67409e0972ae`와 독립 결정은 `artifacts/dev3/wal-append-design-review-20261021/r7-independent-d2-decision.json`에 보존한다. root가 실제 fresh namespace producer·한 slot·gate 밖 I/O·시험/native/compiler/runner를 순차 구현하고 기존 독립 reviewer가 D1을 맡는다. 채택 당시의 독립 계산은 literal3 전체 bytes·생성8192·유한 slot113/1246·durable28cut/82image에 한정되었으며 제품 실행은 아래 후속 기록과 구분한다. tag2 SemanticGroup·일반 recovery·DB READY·commit ACK·global F05는 미완료다.


최종 source `3162fc43600dba0ed74d7b0fbe10b732c71e50fb24002df6153934982c0a03d8`를 `artifacts/dev3/genesis-wal-quality-r6/source`에 보존했다. [국소 원본](../evidence/dev3-genesis-wal-local-xadb-20261021-r6.json)의 고정 runtime10·native3, Clippy/fmt와 checker 자기시험은 PASS다. 설치 profile1/2/3의 실제 entropy→profile/epoch/WRITER binding, 12개 설치 effect의 before/after 오류24·unwind12·readback 오류9·handle mismatch4, 단일 interval의 경계/오류/fence·2개 stale snapshot schedule을 검산했다. 독립 byte 생성64×128의8192ops와 tagged PageRedo의 stage/sync·malformed 재해시 거절도 확인했다. native는 실제 Linux 설치 오류24·append 오류3과 after hook8종×3의 SIGKILL/raw image·새 genesis 거절을 보존한다. kill은 power-loss가 아니며 commit ACK·page flush 권한은 제공하지 않는다.

[compiler 원본](../evidence/dev3-genesis-wal-borrows-xadb-20261021-r1.json)은 unmodified baseline1·prepared input 해제 positive1과 slot 재borrow/두 session의 negative2를 실제 E0499·primary span으로 검증했다. [모델 원본](../evidence/dev3-genesis-wal-models-xadb-20261021-r1.json)은 실제 producer binary/source/log에 결합한14effect/raw bytes와 고정 durable28cut/82image를 대조하며, 별도 slot 유한 모델113states/1246transitions·mutant4를 포함한다. 둘을 실제 thread/runtime·일반 복구와 혼동하지 않는다.

같은 source의 [통합 leaf54check](../evidence/dev3-genesis-wal-leaf-xadb-20261021-r1.json)와 [DEV0/1/2 하위 회귀28check](../evidence/dev3-genesis-wal-dev2-regression-xadb-20261021-r1.json)는 `artifacts/dev3/leaf-2l_rfcl3/source`에 보존했다. [DEV2 current-build kernel18check·native99셀](../evidence/dev3-genesis-wal-dev2-kernel-xadb-20261021-r1.json)은 `/home/et16/x3w-31owcqzs/source`의 짧은 export에서 PASS했다. 고정 named suite31개의202시험(engine21 suite182개 포함)과 kernel engine211 PASS·기존 장시간 fuzz1 ignored를 구분한다. DEV2 PF04 실패와 DEV3 전체 execution_status NOT_RUN은 유지한다.

초기 r0/r1 시험 compile 오류와 r4 Clippy의 시험 helper2개 nested-if 실패는 원래 source/log에 보존했다. 독립 D1의 sync-unwind 반례는 각 fault의 실제 hit·event 순서·원래 lower cause 검산으로 닫았고, tag1 지원의 실제 session 연결과 native entropy 반환값 관측도 보강했다. 첫 통합 r0(source9229d00e…)의 DEV0 workspace 실행은 native 시험의 retry 단계에서 Busy로 실패했다. `genesis-wal-parallel-native-repro-r0/result.json`의 변경 전 실행 파일에서도 같은 오류를 재현했다. 다른 시험의 child 생성이 writer FD 수명과 겹칠 수 있어 시험 프로세스 내부 mutex로 세 fixture의 수명을 격리했으며 제품 Busy 처리와 실제 WAL thread schedule은 유지했다. failing syscall 구간의 직접 증거는 없어 FD 상속의 세부 인과는 미확정으로 남긴다. 원래 FAIL·source·로그·파일은 보존했다. 최종 소스의 [native 반복 원본](../evidence/dev3-genesis-wal-parallel-native-xadb-20261021-r1.json)은 `--test-threads=3`의 고정 시험3개를 32회 모두 통과했고, 독립 reviewer도 보존 binary를 실행하여 실제 파일 bytes를 확인했다.

root와 독립 reviewer의 역할·확정 범위는 `artifacts/dev3/genesis-wal-independent-review-20261021/`의 기록을 따른다. 완료된 이전 target 정리는 `genesis-wal-cache-cleanup-20261021-r0.json`(4개·약360MB logical), r1(3개·1.73GiB allocated), r2(6개·4.84GiB allocated)에 기록했다. superseded kernel worker206개는 동일 bytes를 확인해 hardlink로 바꾸어 약5.05GiB를 회수했다(`genesis-wal-superseded-kernel-worker-cleanup-20261021/summary.json`). source·로그·실행 binary·원본 FAIL 파일은 보존했다. 최종 kernel 독립 r7 감사 뒤 동일 worker206개에서 약5.05GiB와 disposable target에서 약0.50GiB를 추가 회수했다(`genesis-wal-final-kernel-worker-cleanup-20261021/summary.json`, `genesis-wal-final-kernel-target-cleanup-20261021.json`).

최종 독립 감사 `artifacts/dev3/genesis-wal-independent-review-20261021/r8-final-decision.json`은 같은 source의 leaf54·하위28·DEV0/1각8·kernel18/native99·국소11, 고정31suite202개와 원본7개의 byte 일치를 확인했다. 로그127개·hash 참조10,175개/8,495경로, native323/catalog325 세션과 실제 compiler/model 연결도 검산했다. staged source가 실행본과 같고 이 제한 WAL 단위의 commit 차단 사항은 없다. 문서 verifier는 실행하지 않았으며 전체 DEV3 NOT_RUN은 유지한다.

`8d7f547` commit의 clean export가 최종 검증 source3162fc43…와 같고 작업 트리가 깨끗함을 `artifacts/dev3/genesis-wal-commit-verification-20261021.json`으로 확인했다. 독립 감사 뒤 완료 leaf의 worker684개를 같은 bytes의 binary2개로 통합해 약14.52GiB를 회수했고, 국소/통합/compiler target5개에서 보존 binary14개의 hash를 전후 확인하며 약4.69GiB를 정리했다. `genesis-wal-final-leaf-worker-cleanup-20261021/summary.json`과 `genesis-wal-final-target-cleanup-20261021.json`에 경로·hash·원래 identity·회수량을 남겼으며 source export·raw receipt·로그·compiler probe·장애 data는 유지한다.


<a id="rollback-materialization-results"></a>
### RollbackStart status page 구성

[소유 계약 §14](../contracts/dev3-obligation-format.md#rollback-materialization)의 D2 입력 결합 `9c85d61c1272524ca946be02674476b9e103b948f25ff37fdd9214524beaba4a`를 독립 수용했다. root가 기존 semantic observer에 caller scratch 구성 함수를 연결하고 기존12+새5시험을 구현했다. 독립 reviewer는 D1을 담당한다. 준비 단계의 raw literal2 계산과 실제 제품 실행은 구별하며 후속 결과는 아래에 기록한다. live dirty frame·tag2 append·예약 복원·DEV3 전체 인수는 제공하지 않는다.


source `3493fa223b10e6393ae019e131eccaa6274f39e77a73b6ec0aa38975378f87f2`를 `artifacts/dev3/rollback-materialization-quality-r1/source`에 보존했다. [국소 원본](../evidence/dev3-rollback-materialization-local-xadb-20261021-r1.json)은 semantic17(기존12+추가5), checker 자기시험7, all-targets/all-features Clippy와 fmt를 통과했다. 구성 결과는 기존 literal2의 전체16KiB와 같고, 고정64×128의8192개 독립 raw after page와 비교했다. expected/frame/redo/before의28개 반례와 typed-valid 비용148+native12개를 exact 오류로 거절하며 immutable 입력 보존을 확인했다. 동일 scratch의 성공→late 실패→성공, pageLSN/prior0·frameLSN MAX와 strict 이전상태 검산, valid Init 거절도 포함한다. 실패한 output은 폐기하며 native I/O·thread 효과나 live 적용 권한을 새로 제공하지 않는다.

같은 source의 [통합 leaf54check](../evidence/dev3-rollback-materialization-leaf-xadb-20261021-r0.json)와 [하위 회귀28check](../evidence/dev3-rollback-materialization-dev2-regression-xadb-20261021-r0.json)는 `artifacts/dev3/leaf-zahj6ekd/source`에서 PASS했다. [DEV2 current-build kernel18check](../evidence/dev3-rollback-materialization-dev2-kernel-xadb-20261021-r0.json)은 `/home/et16/x3m-uas5dv0l/source`의 짧은 export에 보존한다. 실제 native99셀과 DEV2 복구 결과는 기존 현재-build 회귀이며 DEV3 recovery 인수가 아니다. DEV2 PF04 실패·전체 DEV3 execution_status NOT_RUN은 유지한다. 편집/commit 중 문서 verifier는 실행하지 않는다.

독립 D1 `artifacts/dev3/rollback-materialize-independent-review-20261021/r2-d1-decision.json`은 원래 observer 오류 순서·12시험과 비용 경계가 보존되었고 추가5시험이 채택 계약에 맞음을 확인했다. 보존 engine binary로 semantic17·checker7을 독립 재실행해 PASS했다. wrapper와 중첩 observer의 고정 source/type 회계는10,592bytes로 검토했으며 caller16KiB output을 별도로 구분했다. 이는 정확한 compiler stack/RSS 측정이나 runtime admission·전체 F05의 증명이 아니다. 추가 page-sized local·heap·I/O·lock·unsafe·의존성은 없다.

독립 kernel 감사 `rollback-materialize-independent-review-20261021/r3-kernel-decision.json`은18check·native33×3=99, image102디렉터리/696파일, pre-open447/post633hash, run manifest201, Cargo와 보존6binary·features[]를 확인했다. hash 참조3,529개/2,896고유경로와 원래 driver/log가 일치했다. kernel engine216 PASS·기존 장시간 fuzz1 ignored를 구분한다. 제한 단위의 검토 PASS를 전체 DEV3나 새 recovery 결과로 확대하지 않는다.

완료 kernel 감사 뒤 동일 worker206개를 SHA·전체 bytes·mode·device 확인 후 hardlink로 통합해5,417,885,696bytes(약5.05GiB)를 회수했다. disposable target의537,837,568bytes(약0.50GiB)도 보존6binary의 hash를 전후 확인한 뒤 정리했다. `artifacts/dev3/rollback-materialization-kernel-worker-cleanup-20261021/summary.json`과 `rollback-materialization-kernel-target-cleanup-20261021.json`에 원래 경로·hash·회수량을 남겼으며 source·raw receipt·로그·native image는 보존한다.

최종 독립 감사 `artifacts/dev3/rollback-materialize-independent-review-20261021/r4-final-decision.json`은 leaf54·하위28·DEV0/1각8·kernel18/native99·국소6과 고정31suite207시험(engine21suite187개)을 확인했다. 등록 원본4개가 byte 단위로 같고 로그122개·hash 참조10,156개/8,489고유경로, native323/catalog325세션·필수 lower85시험의 증거가 일치했다. staged314 source의 지문도 실행 source3493fa22…와 같으며 이 제한 단위의 commit 차단 사항은 없다. 이후 D0 정리·감사 기록 외 제품 source를 바꾸지 않았다.

최종 독립 감사 뒤 완료 leaf worker684개를 동일 bytes의 binary2개로 hardlink 통합해15,589,957,632bytes(약14.52GiB)를 회수했다. 국소/통합/compiler target5개도 보존 binary14개의 hash를 전후 확인한 뒤4,524,736,512bytes(약4.21GiB)를 정리했다. `artifacts/dev3/rollback-materialization-leaf-worker-cleanup-20261021/summary.json`과 `rollback-materialization-final-target-cleanup-20261021.json`에 경로·hash·회수량을 보존한다. 이번 단위의 kernel 정리를 포함한 회수량은 약24.28GiB이며 source·raw receipt·로그·compiler probe·native data는 유지한다. 정리 후 파일시스템의 여유 공간은131GiB였다.


<a id="rollback-producer-results"></a>
### RollbackStart WAL·상태 page 생산

[소유 계약 §15](../contracts/dev3-obligation-format.md#rollback-producer)의 독립 D2 입력 결합 `0d56ec96adab9ec85953df20d29653cbb0f6c3023241045054e35bbe1cbe95f1`을 확인했다. root가 typed before 상태에서 caller frame/page를 생산하는 함수를 구현했으며 독립 reviewer가 D1을 맡는다. 이전 단위 commit2473a8f의 clean export가 검증 source3493fa22…와 일치함을 `artifacts/dev3/rollback-materialization-commit-verification-20261021.json`으로 확인했다. raw 예산11입력의 준비 산술과 후속 제품 실행은 구별하며 실제 결과는 아래에 기록한다.


source `f08f01d72f06a9578a129927a7a17dfe2450734e8fbd86dcfeb8c11e4da4d515`를 `artifacts/dev3/rollback-producer-quality-r0/source`에 보존했다. [국소 원본](../evidence/dev3-rollback-producer-local-xadb-20261021-r0.json)은 semantic22(기존17+생산5)·WAL13(기존11+finalizer2)·checker 자기시험7·all-targets/all-features Clippy·fmt를 통과했다. literal2/dense1과 raw seed64×128=8192개의 전체3040-byte frame·16KiB page가 독립 정답과 같았다. source/context29·길이35844·preflight복합2·typed-valid 예산11개의 정확한 오류, 정상 경계10개와 같은 output의 성공→실패→성공을 확인했다. raw 예산11개는 사전 고정 SHA와 같고 기존 전체 before decoder를 통과한 뒤 BudgetMismatch를 반환했다. lower finalizer는 정상6·오류15개에서 payload 보존·오류 무효과·legacy exact/두 tail의 tag2 거절을 확인했다. 두 compound는 preflight가 source보다 먼저임을 확인하며 expected→LSN의 상대 순서는 정적 검토로 구분한다.

같은 source의 [통합 leaf54check](../evidence/dev3-rollback-producer-leaf-xadb-20261021-r0.json)와 [하위 회귀28check](../evidence/dev3-rollback-producer-dev2-regression-xadb-20261021-r0.json)는 `/home/et16/work/xadb/artifacts/dev3/leaf-d_x7eels/source`에 보존한다. [현재-build DEV2 kernel18check](../evidence/dev3-rollback-producer-dev2-kernel-xadb-20261021-r0.json)은 `/home/et16/x3p-gte9qar6/source`의 별도 짧은 export에서 PASS했다. 실제 native99셀은 현재-build DEV2 회귀이며 새로운 DEV3 recovery 결과가 아니다. DEV2 PF04 실패·전체 DEV3 execution_status NOT_RUN은 유지한다. 문서 verifier는 편집/commit 중 실행하지 않는다.

독립 D1 `artifacts/dev3/rollback-producer-independent-review-20261021/r3-d1-decision.json`은 제품 로직·고정 corpus에 차단 사항이 없음을 확인했다. 보존 engine binary로 새 semantic5+전체 WAL13의18개 시험과 checker7을 별도 실행해 PASS했고, 원본 local semantic22/WAL13·품질7check의 로그·build·driver·source hash도 검산했다. DWARF 타입과 소스의 보수적 회계에서 구성 분기는12,392bytes, 중첩 materializer 분기는12,024bytes로 caller 출력19,424bytes를 제외한16KiB 한도 이내였다. 이는 compiler stack/RSS 측정이나 runtime admission·전체 F05 증명이 아니다.

독립 kernel 감사 `rollback-producer-independent-review-20261021/r4-kernel-decision.json`은18check·native33×3=99, image102디렉터리/696파일, pre-open447/post633hash, manifest201과 보존6binary를 확인했다. hash 참조3,582개/2,951경로와 Cargo features[]가 일치했다. engine223 PASS·기존 장시간 fuzz1 ignored, transaction7/cache15/native harness4×3과 thread64의 실행 분모를 구별한다. 이 결과는 DEV2 현재-build 회귀이며 새 DEV3 recovery 인수가 아니다.

완료 kernel 감사 뒤 동일 worker206개를 manifest/SHA/전체bytes/mode/device 확인 후 hardlink로 통합해5,417,885,696bytes를 회수했다. disposable target은 보존6binary의 hash를 전후 확인한 뒤538,329,088bytes를 정리했다. `artifacts/dev3/rollback-producer-kernel-worker-cleanup-20261021/summary.json`과 `rollback-producer-kernel-target-cleanup-20261021.json`에 경로·hash·회수량을 남겼다. source·raw receipt·로그·native image는 보존한다.

최종 독립 감사 `artifacts/dev3/rollback-producer-independent-review-20261021/r6-final-decision.json`은 국소7·leaf54·하위28·DEV0/1각8·kernel18/native99와 고정31suite214시험(engine21suite194개)을 확인했다. 등록 원본4개가 byte 단위로 같고 로그123개·hash 참조10,212개/8,543고유경로, native323/catalog325세션·필수 lower85시험의 증거가 일치했다. staged21개 변경과 제품314개 파일의 지문도 실행 sourcef08f01d7…와 같으며 이 제한 단위의 commit 차단 사항은 없다. 후속 변경은 실제 감사·정리 결과의 D0 기록으로 제한하며 제품 source를 바꾸지 않는다. 정리 검토에서는 보존 실행 파일19개와 정확한 대상4개를 확인했다. 접근 가능한 `/proc` 관측의 한계를 남겼고 완료 receipt·root의 실행 종료와 정리 중 새 실행 금지를 함께 전제로 삼았다.

최종 감사 뒤 leaf worker684개를 동일 bytes의 binary2개로 hardlink 통합해15,589,957,632bytes(약14.52GiB)를 회수했다. 국소/통합/compiler target4개도 보존 binary13개의 hash를 전후 확인한 뒤4,253,200,384bytes(약3.96GiB)를 정리했다. `artifacts/dev3/rollback-producer-leaf-worker-cleanup-20261021/summary.json`과 `rollback-producer-final-target-cleanup-20261021.json`에 경로·hash·회수량을 보존한다. kernel 정리를 포함한 이번 단위의 총 회수량은 약24.03GiB이며 source·raw receipt·로그·compiler probe·native data는 유지한다. `rollback-producer-cleanup-summary-20261021.json`의 정리 직후 파일시스템 여유 공간은 약128.75GiB다.

<a id="storage-session-design"></a>
## 최초 저장 세션의 구현 전 계약 수신

2026-10-21 [최초 저장 세션 계약](../contracts/dev3-storage-session.md)을 독립 D2 검토 후 채택했다. 최초 extent·FILEMAP 두 bank·영속 staging descriptor, WAL/reader의 단일 namespace owner, 한 reader lease와 공통 fence, 실패 시 partial artifact/interval 보존을 구체화했다. 동결한 기대 bytes/효과/유한 장애 모델·compiler 입력은 준비 자료이며 제품 실행은 NOT_RUN이다.

이 등록의 제품 source는 직전 RollbackStart producer `f08f01d72f06a9578a129927a7a17dfe2450734e8fbd86dcfeb8c11e4da4d515` 그대로다. 그 검증 결과를 새 저장 세션의 실행 증거로 표시하지 않는다. 구현·runtime12/native3/compiler8·동일 source 하위 회귀·현재-build 복구·독립 D1을 수행한 뒤 별도 결과를 기록한다. 전체 DEV3 및 DEV2 PF04 상태는 변경하지 않는다.


<a id="storage-session-results"></a>
## 최초 저장 세션의 실제 설치·공통 fence

계약 commit `0e72e3c` 뒤 `/root`가 [최초 저장 세션 계약](../contracts/dev3-storage-session.md)의 D1 구현·통합을 맡고 `/root/dev3_selector_review`가 독립 검토를 맡는다. 기존 WAL owner 안에서 첫1MiB extent, FILEMAP 두 bank,4096-byte staging descriptor와 selector를 설치한다. 한 borrowed reader와 WAL의 공통 fence를 연결하고 reader의 namespace MutexGuard는 개별 open 동안만 보유한다. Frame/guard의 local poison은 공통 fence보다 먼저 판정하며 실패·unwind 뒤 실제 interval과 partial artifact를 보존한다. 고정 초기 mapping 밖의 typed PageRedo는 append 전에 거절한다. 후속 dirty frame·home write·tag2 append·reopen/recovery·S1 인수는 남아 있다.

구현 중 독립 검토에서 constructor의 poison/common/capacity 우선순위, guard.view의 마지막 local 검사와 reader container Drop보다 앞선 lease 해제 문제를 수정했다. 마지막 문제는 container Drop을 실제 thread에서 멈춘 동안 두 번째 reader가 Busy인 반례로 닫았다. r0 runtime12와 보강한 r1 runtime12·native3·compiler8은 개발 사본의 결과로 보존한다. r1 source는 `bab17bc8f786bdac04316e1c0713bb042fb5b51f4d88914284d1f34bcbf84fb7`, 원본 위치는 `artifacts/dev3/storage-session-draft-r1/`다. 이 사본의 native3은 정상3profile·오류676·SIGKILL39건을 실행했지만 후속 inode/compile-time source identity·외부 checker 보강 이전이다. 최종 소스 검증과 구별한다.

독립 source/type audit `artifacts/dev3/storage-session-independent-review-20261021/r6-resource-ledger.md`는 prepare41992·stream50184·readback65536의 보수적 상한에 실제 중첩 codec/SHA와 readback의 추가 File2개를 포함한다. caller scratch49152를 별도로 더한 peak는114688이다. 실제 타입은 Fresh504·Genesis328·File32·plan168·names645·entries4096·reader384·cursor224였으며 scalar 분할11528은16384 reserve 안에 든다. 이 회계는 소스에서 소유한 값의 상한이며 compiler stack/RSS나 전체 F05 인증이 아니다. oversized owner/File의 무효과 거절과3개 profile에서 reader128회 수명·handle peak3도 고정 시험에 포함한다.

첫 통합 후보 source `c1fddc280d79d5c8a0d62d07474b25addba5704b963cfcb5d7a558b67a81a174`를 `artifacts/dev3/leaf-xl_o1n30/source`에 보존했다. engine/engine-worker all-targets/all-features Clippy와 fmt는 PASS다. 이 후보의 개별 검증과 마지막 하위 회귀 실패는 아래 후속 기록과 구분한다. 문서 verifier는 편집·commit 중 실행하지 않는다.

완료된 개발 사본의 disposable target만 정리했다. r0의221634377bytes와 r1 runtime/native·compiler의1099680180bytes를 제거했고, 각 `cleanup.json`에 경로와 보존 hash를 남겼다. 원본 소스·로그·실행 binary·compiler probe·native DB는 보존했다.


같은 c1fddc28… 후보의 현재-build DEV2 kernel18check/native99셀은 PASS다. 원본 `artifacts/dev3/storage-session-kernel-r0.json`과 짧은 export `/home/et16/x3s-r46qtfly/source/artifacts/d2k-c7_ibny2`를 보존한다. 독립 r8 감사는 source4개·계약22개·loaded checker7개·실행 파일6개·source-bound manifest201개와 native image102디렉터리/696파일을 대조했다. engine235 PASS·기존 장시간 fuzz1 ignored, transaction7·cache-loss15·native33×3의 범위를 구별하며 새로운 DEV3 reopen 결과로 표시하지 않는다.

완료 kernel 감사 뒤 동일 worker206개를 hash/전체 bytes/mode/device 확인 후 hardlink 통합하여5496356864bytes를 회수했다. disposable target에서도 실행 파일6개의 hash를 전후 확인한 뒤545452032bytes를 정리했다. `artifacts/dev3/storage-session-kernel-worker-cleanup-20261021/summary.json`과 `storage-session-kernel-target-cleanup-20261021.json`에 원래 identity와 정리 기록을 남겼다. 별도의 완료 Clippy target336781897bytes도 제거했으며 진행 중 leaf·하위 회귀는 정리하지 않았다.


c1fddc28… 통합 r0는 마지막 하위 회귀에서 FAIL했다. DEV2→DEV1의600초 전체 제한이 먼저 끝났고 DEV3 실행기가 남은 process group을 SIGKILL했다. 원본 [실패 receipt](../evidence/dev3-storage-session-leaf-xadb-20261021-r0.json)의58개 선행 check 성공을 전체 통합 성공으로 표시하지 않는다. 미완성 DEV0/1 receipt와 소실된 nested PIPE 출력은 복원한 실행 증거로 만들지 않는다. 이후 [전체 시간 예산 D2](../contracts/dev3-storage-session.md#aggregate-regression-budget)와 부분 로그 보존을 채택했으며 새 source로 전 분모를 검증한다.

r0 실행 종료를 확인한 뒤 compiler target4개에서 ELF 실행 파일140경로/110고유파일을 별도 보존하고 disposable object/cache의2379948032bytes(약2.22GiB)를 정리했다. `artifacts/dev3/storage-session-failed-target-cleanup-20261021.json`에 원래 실행 파일 경로/hash·로그 hash·FAIL receipt를 연결했다. source·native DB·compiler probe·개별 PASS/최종 FAIL을 보존했다.


하위 회귀 wrapper 보완은 commit `ed1fc29`에 분리했다. 새 후보 source `edec600699a4b157c17636819923bb34fc0c30d5e1bd532ce6a1cc780b84910f`의 변경은 c1fddc28… 대비 DEV0/1/2 runner와 evidence 자기시험4개 파일뿐이며 저장 세션 제품·고정 입력은 같다. `artifacts/dev3/leaf-8tl81at8/source`에서 새 runtime12/native3/compiler8·model을 실행했다. r13 독립 감사는 실제 trace1050·durable676cut/6804image와 native718DB/5172files·715child의 source/binary/실제 inode를 확인했다. r12는 같은 source의 kernel18/native99·quality3과 source4개·계약22개·checker7개·binary6개·manifest201개를 확인했다. 최종 [leaf59](../evidence/dev3-storage-session-leaf-xadb-20261021-r1.json)와 [하위 회귀28](../evidence/dev3-storage-session-dev2-regression-xadb-20261021-r1.json)이 PASS다. DEV0 workspace self-tests는594.62초, DEV1의 DEV0 회귀 전체는609.92초였으며 새 사전 고정 예산 안에서 완료했다. DEV2의 controller/catalog256seed 확장 이력은 각각311.80/315.97초에 PASS였다. 이 시간은 성능 인수 증거가 아니다. quality·compiler·model·native·leaf·하위 회귀·kernel의 원본7개 receipt를 byte 단위로 동일하게 등록했다. stage의 execution_status=NOT_RUN과 DEV2 PF04 실패, dirty/home/recovery 및 전체 DEV3 인수 미완료는 유지한다.

새 kernel 감사 뒤 worker206개를 동일한 실행 파일로 hardlink 통합하여5496356864bytes를 회수했다. kernel target의545439744bytes와 완료 Clippy/주 checkout의 미사용 incremental cache1285644288bytes도 정리했다. 기록은 `storage-session-kernel-worker-cleanup-r1-20261021/summary.json`, `storage-session-kernel-target-cleanup-r1-20261021.json`, `storage-session-idle-cache-cleanup-20261021.json`이다. 진행 중 leaf/하위 회귀 target·DB는 제외했으며 완료 kernel의6개 실행 파일과 원본 로그·source·native image를 보존했다.


최종 [r14 독립 감사](../evidence/dev3-storage-session-review-xadb-20261021-r1.json)는 staged81경로와 source368파일의 지문 일치, leaf59·하위28·DEV0/1각8, named33suite/229시험, 원본7개 등록을 확인했다. lower native323/catalog325세션과124로그·17471hash 참조를 검산하고 차단 지적 없이 제한된 단위의 통합을 수용했다. r12/r13의 kernel·새 기능 감사는 재사용했으며 검토자가 제품 시험을 재실행한 것은 아니다. 최초600초 timeout FAIL 원본은 그대로 보존한다.

최종 감사 뒤 완료 leaf의 worker684개를2개 동일 binary group으로 hardlink 통합하여15731716096bytes를 회수했다. target4개에서는 ELF156경로/122고유 실행 파일을 별도 보존한 뒤2585403392bytes를 정리했다. 두 정리의 추가 회수량은 약17.06GiB이며 원래13개 실행 파일·소스·probe·로그·원본 receipt·native DB를 유지했다. 기록은 `artifacts/dev3/storage-session-leaf-worker-cleanup-r1-20261021/summary.json`과 `storage-session-leaf-target-cleanup-r1-20261021.json`이다. 정리 전후 source/실행 파일/log/receipt hash가 같음을 확인했으며 공유 hardlink 실행 파일은 이후 in-place rebuild 대상으로 사용하지 않는다.

<a id="doublewrite-install-contract"></a>
## 최초 DWB 설치 계약 채택

2026-10-21 [D3-DWB-INSTALL-001](../contracts/dev3-doublewrite-install.md)을 r9 독립 D2 검토 뒤 채택했다. literal12·effect25·observation32·model50cut/308image/max32 및 semantic47의 정확 오류를 제품과 독립적으로 계산했다. 채택 당시 제품 source·runtime10/native3/compiler9·하위 회귀는 NOT_RUN이었으며 이 정적 검토를 실행 PASS로 세지 않는다. 이후 실행은 아래 실제 경로 기록으로 구분한다. `/root`가 구현·통합, `/root/dev3_selector_review`가 별도 검토를 맡는다. dirty/home/일반 복구·전체 DEV3와 DEV2 PF04 해결은 남아 있다.

<a id="doublewrite-install-results"></a>
## 최초 DWB 설치의 실제 경로

계약 commit `df01ec4` 뒤 `/root`가 D3-DWB-INSTALL-001의 구현·통합, `/root/dev3_selector_review`가 독립 D1을 맡는다. pristine Fresh를 소비하여 XDI1 staging descriptor, 고정 images 공간, Empty control이 있는 bank0·zero-filled bank1과 DWB selector를 설치하고 전체 bytes와 namespace identity를 readback한다. 반환 owner는 caller의 4MiB workspace를 계속 빌리고 clean reader와 공통 fence만 제공한다. dirty frame·Pending batch·home write·retirement·일반 reopen/recovery는 후속 의무다.

독립 초기 검토의 short I/O 전체 trace, configured trigger와 read continuation 구분, 원래 Namespace kind/io_kind 검산을 반영했다. 후속 검토에서는 실제 실행 결과를 이용한 checker31 반례와 native 오류의 원래 code/kind/effect/io_kind 비교를 보강했다. 제품의 의미·동결 입력·시험 분모·예산은 바꾸지 않았다. `artifacts/dev3/doublewrite-install-draft-r2/`의 source `4e594d598c104d2ebddc34ef9e749bc432d66b0173e90b0c75605355609cfbf0`에서 runtime10·native3·compiler9·checker 자기시험4·진단 parser7·Clippy/fmt가 PASS다. 실제 trace75·observation96, 모델50cut/308image/max32, native92DB(정상3/오류50/SIGKILL39), 실제 관측 결과를 변형한 checker31도 대조했다. 개발 검증을 최종 V2나 전체 DEV3 완료로 확대하지 않는다.

첫 통합 후보 `b09c98a5…`는 checker 보완으로 대체되어 실행 중 SIGINT로 종료했다. 원래 FAIL과 source/log를 `artifacts/dev3/doublewrite-install-v2-r0.json` 및 interruption 기록에 남겼다. 후속 r1 driver는 Git metadata가 없는 export에서 leaf origin 조회가 실패하여 제품 시험을 시작하지 못했으며 원래 로그·FAIL을 보존했다. 같은 source의 r2 driver는 leaf의 자체 snapshot과 짧은 경로의 kernel/quality export를 사용했다. [통합 원본](../evidence/dev3-doublewrite-install-leaf-xadb-20261021-r2.json)의 선행63check는 완료됐으나 마지막 하위 회귀를 당시 사용자 지시로 SIGINT 중단하여 최종 상태는 FAIL이다. [중단 원본](../evidence/dev3-doublewrite-install-user-stop-xadb-20261021-r2.json)과 [종료 관측](../evidence/dev3-doublewrite-install-stop-terminal-xadb-20261021-r2.json)을 보존한다. queued DEV2 kernel은 NOT_RUN이며 r2 driver의 후속 quality도 실행하지 않았다. 이미 같은 source에서 완료한 국소 Clippy/fmt 결과와 구별한다. DEV2 PF04 실패·전체 DEV3 미완료는 유지한다.

완료 개발 r0/r1/r2와 중단된 통합 r0의 target만 정리하여 합계 약2.64GiB를 회수했다. 각 `doublewrite-install-*-target-cleanup-r0/r1/r2.json`에 ELF 실행 파일96경로(각 정리의 고유파일 수 합계46)의 보존 경로/hash, source·로그·원래 receipt 지문을 남겼다. source·compiler probe·native DB·실행 binary는 보존했다.


사용자는 이후 테스트 방침을 **단계에 관계없이 수정한 코드만 시험하고, 수정 없는 코드는 재시험하지 않는다**로 정정했다. [AGENTS.md](../../AGENTS.md)의 commit `4273343`과 [소유 계약의 적용 범위](../contracts/dev3-doublewrite-install.md#changed-code-test-scope)를 따른다. 현재 source fingerprint가 기존 4e594d59…와 같아 제품·하위 회귀를 추가 실행하지 않았다. [변경 범위 검증 정리](../evidence/dev3-doublewrite-install-scoped-xadb-20261021-r2.json)는 기존 실행의 파생 기록이다. [국소 실행](../evidence/dev3-doublewrite-install-local-xadb-20261021-r2.json)·[compiler](../evidence/dev3-doublewrite-install-borrows-xadb-20261021-r2.json)·[모델](../evidence/dev3-doublewrite-install-model-xadb-20261021-r2.json)·[native](../evidence/dev3-doublewrite-install-native-xadb-20261021-r2.json), [r12 D1](../evidence/dev3-doublewrite-install-d1-local-xadb-20261021-r2.json)·[r13 개별 완료 감사](../evidence/dev3-doublewrite-install-d1-partial-xadb-20261021-r2.json)는 원본 byte 그대로 등록했다. 최초 Empty DWB 설치의 제한 검증만 주장하며 원본 통합 FAIL을 전체 PASS로 승격하지 않는다.

종료된 통합 leaf의 disposable target5개에서도 ELF144경로/112고유파일과 canonical 실행 파일14개·source·로그·원본 receipt를 보존한 뒤 2589478912bytes(약2.41GiB)를 추가 회수했다. 정리 전후 hash·원래 경로는 `artifacts/dev3/doublewrite-install-leaf-target-cleanup-r2.json`에 남겼다. native DB와 compiler probe 입력은 보존하며 제품 시험은 재실행하지 않았다.

[r14 최종 독립 감사](../evidence/dev3-doublewrite-install-review-xadb-20261021-r2.json)는 staged68경로·설정 source407파일의 동일성, 원본9개 등록과 파생 기록1개, 종료된 선행63/전체64check의 범위를 확인하고 최초 설치 단위의 commit을 수용했다. r12/r13을 재사용했으며 제품·checker·컴파일·문서 verifier·테스트 실행과 native DB 재해시는 하지 않았다. 원본 통합 FAIL·하위 회귀 중단·kernel NOT_RUN 및 전체 DEV3 미완료를 유지한다.


<a id="allocation-reservation-partial"></a>
## 2026-10-21 페이지 예약·취소 부분 구현과 사용자 요청 중단

[D3-ALLOC-RESERVE-001](../contracts/dev3-allocation-reservation.md)을 실제 owner에 연결했다. 최초 DWB 설치 owner를 소비하여 선택된 allocator page0을 읽고, Free→Reserved와 Reserved→Free를 semantic WAL·authoritative frame·descriptor·dirty recLSN에 함께 적용한다. 취소는 소비한 generation을 되돌리지 않으며 첫 recLSN·WAL 보존량·work high-water를 유지한다. stage 직후 unwind에도 예약 자원을 소유하고 sync 및 최종 검산 성공 뒤에만 terminal page token을 반환한다.

[부분 실행 원본](../evidence/dev3-allocation-reservation-partial-xadb-20261021.json)에 source 사본·ELF·로그 SHA와 시도별 결과를 보존했다. r2의 새 runtime8은 첫 실행 PASS다. profile3개×1/8/63 예약·취소의 독립 frame/page/descriptor/item bytes, 고정 의미 반례541개와 D1 추가4개를 대조했다. 검토 후 바뀐 tag 판정·오류 원인·자원 소유 검산만 r3/r4에서 선택했다. r3 AR05는 새 assertion이 기존 WAL의 poison→Fenced 분류를 Poisoned로 잘못 기대하여 FAIL했고 원본을 보존했다. 기대값 정정 후 AR05만 r4에서 PASS했다. 다른 완료된 시험을 반복하지 않았고 DEV0/1/2·전체 leaf/workspace·문서 검사는 실행하지 않았다.

**이 결과는 메모리 포트를 통한 실제 owner 호출의 제한 검증이다.** native2(13DB), compiler baseline+6개 probe, 실제 수정한 WAL stage helper의 관련4시험, Clippy와 전체 allocation 단위 인수는 미완료다. 현재 dirty page는 RAM에 남으며 payload INIT·home/DWB 운영·일반 recovery·READY·transaction ACK를 제공하지 않는다. 사용자의 “적당한 단계에서 중단” 요청에 따라 이 부분 구현을 commit하고 추가 개발·검증을 중단한다. 전체 DEV3 목표와 기존 인수 의무는 유지한다.

재개할 때는 이 원본 source delta와 완료한 사례를 기준으로 미실행 native/compiler/stage helper 검증을 이어간다. 동일 코드의 완료된 시험을 처음부터 다시 돌리지 않는다. 실행 전 원본 source·binary·fixture와 변경 경로의 관계를 확인한다.

<a id="allocation-lint-fix"></a>
## 2026-10-22 예약·취소 테스트 hook의 Clippy 수정

Actions의 DEV1→DEV0 정적 검사에서 `allocation_reservation_tests.rs`의 `enum_variant_names`와 `type_complexity`를 거절했다. 당시 push gate는 fmt·문서 검사만 실행했으므로 두 Rust lint를 검출할 수 없었다. 위 부분 구현의 Clippy 미실행 기록은 당시 결과로 보존한다.

`Point`의 공통 `After` 접두사를 제거하고 네 값의 의미를 “해당 단계 직후”로 명시했다. 부모의 `cfg(test)` 참조와 시험 분기만 함께 바꾸었으며 hook 위치·panic 진단·생산 경로는 유지했다. terminal 관측 tuple은 이름 있는 `TerminalCapture`와 `CaptureSlot`으로 바꾸고 `Copy` 값으로 읽어 동기화 장벽에서 mutex guard를 보유하지 않는다.

이번 코드 diff는 fault hook 이름·분기와 terminal 관측 표현에 한정한다. 다음 두 시험을 `cargo test -p xadb-engine --lib <전체 시험명> --locked -- --exact`로 각각 실행하여 각1개 PASS, 각254개 제외를 확인했다.

- `storage::wal_io::bootstrap::doublewrite_install::allocation_reservation::tests::allocation_ar05_stage_cuts_and_fencing`
- `storage::wal_io::bootstrap::doublewrite_install::allocation_reservation::tests::allocation_ar07_terminal_barrier_and_retained_protection`

`cargo clippy -p xadb-engine --all-targets --all-features --locked -- -D warnings`도 PASS했다. 이는 현재 checkout의 해당 package 정적 검사이며 과거 NOT_RUN을 소급 PASS로 바꾸거나 allocation 전체 인수를 완료한 기록이 아니다. native2/13DB·compiler baseline+6 probes·원래 WAL stage 관련4시험·component64KiB 증명과 전체 DEV3 인수는 남아 있다. 같은 코드의 다른 완료된 runtime 사례나 DEV0/1/2·전체 workspace 기능 시험은 실행하지 않았다.

재발 방지는 [DOC-004](../policies/documentation-policy.md#doc-004)의 commit export Clippy gate로 구현했다. 새 gate 회귀3개만 선택하여 PASS했고 기존7개는 반복하지 않았다. 실제 push의 workspace Clippy·fmt·문서 검증은 별도 gate이며 기능 시험을 실행하지 않는다.

<a id="allocation-reservation-accepted"></a>
## 2026-10-22 예약·취소 3.03.1 잔여 인수

[단위 인수](../evidence/dev3-allocation-reservation-acceptance-20261022.md)는 원래 부분 runtime 결과를 보존하고 새 native2그룹/13 Linux DB, compiler baseline+6 probe, 수정 WAL stage의 관련4시험을 연결한다. [중첩 자원 검토](../evidence/dev3-allocation-resource-review-20261022.md)의 타입 합 admission과 실제 pthread64KiB에서 constructor·reserve63·cancel63·observe/drop을 검증했다. stack 실패 원본과 수정된 경로의 AR02/AR04·AR03 선택 검증, 관련 fmt/Clippy를 각각 source별로 남겼다. 무변경 하위 시험·전체 wrapper·문서 verifier는 실행하지 않았다.

독립 검토가 D1/D2를 수용하여 제한된 예약·취소 3.03.1의 잔여 인수를 완료했다. source별 범위와 판정은 위 단위 기록을 따른다. INIT·운영 dirty buffer·home/DWB flush·일반 recovery·READY·거래 ACK와 전체 DEV3/S5는 여전히 미완료다. 다음 공급은 3.10.1.a의 apply/content/version guard이며 이후 semantic stage/apply와 dirty 기반을 연결한다.

<a id="concurrency-admission"></a>
## 2026-10-22 적용 admission·version gate 3.10.1.a.1

[소유 계약](../contracts/dev3-concurrency-admission.md) 아래34-owner 역할 분할·apply/content admission·실제 exclusive version mutex를 구현했다. 기존 SessionFence를 빌리며 별도 fence를 복제하지 않는다. non-Clone/non-Send·독점 borrow, checked generation/retirement, version 진입 전후 검사, unwind fence-before-release와 blocking Drop 회계를 검증했다. expected 거절의 정리 중 처음 관측한 poison/손상/fence도 CallerMustResolve로 반환한다. Commit stage/pending/install은 cfg(test) 모의 명세만 제공한다.

[인수 기록](../evidence/dev3-concurrency-admission-20261022.md)과 [실행 색인](../evidence/dev3-concurrency-admission-20261022.json)은 원래 source의 compiler12, 새 checker4+4, runtime7그룹/45사례(독립 부정20·layout27 포함)를 보존한다. runtime은5.205초였고 최종 fmt/engine Clippy도 PASS했다. 최초 test 임시수명 빌드FAIL과 test-only ClippyFAIL을 보존했으며 의미가 같은 let-chain 정리 뒤 runtime은 반복하지 않았다. 확대512개 스케줄은 준비했지만 NOT_RUN이다. source 논리 control7248·nested temporary896·constructor7832bytes와 실제 target layout을 구분하며 전체64KiB ingress/물리 stack을 인증하지 않는다.

독립 D1과 F05 target-layout 검토가 제한 .a.1 단위 인수를 수용했다. 3.10.1.a의 나머지 page/tree rank·거래 잠금, snapshot·transient pin·FIFO, 실제 dirty/WAL/Commit 소비와 DEV3 S5는 미완료다. 다음 실제 공급은 같은 저장 owner의 authoritative dirty frame·content latch와 typed stage/apply 결합이다.

<a id="dirty-stage"></a>
## 첫 dirty frame·stage 소비자

[채택 계약](../contracts/dev3-dirty-stage.md)의3.05.1-A/B +3.04.1.a는 실제 allocator1에 caller borrowed payload·전체 pin set·ordered content·version/append/DPT 순서를 연결한다. immutable after-image를 소유한 private permit으로 reserve/cancel을 설치하고, guard/pin 반환 뒤 sync하며 실패한 staged obligation과 첫 recLSN을 보존한다.

[인수 기록](../evidence/dev3-dirty-stage-20261022.md)과 [실행 색인](../evidence/dev3-dirty-stage-20261022.json)은 mandatory53를 원래 r1 완료 prefix6, r2 repair2, 정규6그룹45로 구별한다. 독립 부정55·compiler18·native3·새 checker6+6+4·oracle4 및 resource-context Rust1/10사례를 검증했다. 원 profile 집계 오류와 runtime r0/r1 FAIL은 보존하며 통과한 무변경 코드를 반복하지 않았다. 최종 fmt/engine Clippy는 PASS다.

실제 source-value 상한과 layout을 검산했으며 physical stack/RSS를 새로 측정하지 않았다. S5 normal18operation 확대는 작성/NOT_RUN이다. INIT·동적 target 등록·flush·retirement·FIFO·일반 recovery/READY와 전체 DEV3는 남아 있다.

<a id="allocator-flush"></a>
## allocator 한 페이지 flush·whole retirement

[소유 계약](../contracts/dev3-allocator-flush.md)의 제한3.05.2/3.05.3은 actual allocator1의 coherent capture·authentic WAL force·DWB issuance/image/Pending·home sync·whole retirement·조건부 RAM finalize를 연결한다. Ready/WaitingWal/RetiredPendingFinalize와 exact claim을 유지하며 view Drop 뒤 continuation, 동시 redirty, fatal/unwind 보존을 제공한다.

[선택 인수](../evidence/dev3-allocator-flush-20261022.md)·[source별 색인](../evidence/dev3-allocator-flush-20261022.json)은 필수76를 자원2·완결5그룹67·독립 감사 완료 prefix3·수정/미실행4로 구분한다. WAL30·새 compiler12·변경 permit1과 baseline·native3의6batch/186효과·새 checker25를 검증했다. native P1의 원 driver는 보조 proc 관측 오류로 FAIL이며 완료된 bytes/events/FD는 독립 재검산으로 수용했다. P1을 재실행하지 않았고 sampler 불완전을 보존했다. 최종 fmt/engine Clippy PASS, 제한 단위의 독립 종합 검토를 수용했다.

31개 실제 layout과 source 수명 식, File/aggregate effect 전 거절을 확인했다. hot63960/cold48576≤새65536, concrete deep24624, 기존 dirty와 합계262144다. physical stack/RSS/latency는 측정하지 않았다. S5 batch1 확대972는 작성/NOT_RUN이며 batch8/64·다중 file·INIT·new owner·free/reuse·eviction/pressure·일반 recovery/READY와 전체 DEV3는 미완료다.

<a id="status-bootstrap"></a>
## 불변 Status bootstrap 역할

[소유 계약](../contracts/dev3-status-bootstrap.md)은 실제 `d3-bootstrap` plan2의 immutable ObjectId2/gen1/kind9/level0 역할을 공급한다. `create_status`의 전체 설치/readback 뒤 `StatusRole`을 대여하며 poison/fence 우선순위와 owner 수명을 보존한다. 기존 allocator adapter는 plan2를 변경 효과 전에 거절한다. 소비형 adapter의 기존 owner 소비/fence 책임은 유지한다.

[선택 인수](../evidence/dev3-status-bootstrap-20261022.md)·[source별 실행 색인](../evidence/dev3-status-bootstrap-20261022.json)은 새5leaf/32case, 변경 decoder의 기존 corpus1, 실제 native profile1 DB1/350효과/5,296,128bytes, compiler semantic3와 별도 infrastructure baseline1의 PASS를 보존한다. 새 checker7과 이후 수정한1method만의 추가 검증을 구분한다. 보존 source `cf219808…`의 fmt/engine Clippy PASS이며 제한 단위의 최종 독립 D1을 수용했다.

prepare12208/readback14788·Fresh role424/Mutable role792 bytes의 실제 typed census와 generic reader padding 초과의 effect 전 거절을 확인했다. component65536·caller scratch49152를 증액하지 않았으며 stack/RSS 측정은 아니다. 새 S5 추가65는 작성/NOT_RUN이다. 실제 INIT·membership·거래 directory·일반 recovery/READY·전체 DEV3 인수는 미완료다.

<a id="status-init"></a>
## 폐쇄 두-frame Status INIT

[소유 계약](../contracts/dev3-status-init.md)의3.03.2.a는 실제 reserve의 exact 권한에서 allocator/Status 두 image를 같은 stage로 설치하고, 마지막 publication과 동일 requirement의 durable 확인 뒤 claim을 반환한다. whole pin/precheck·unwind fence·retained obligation을 유지하며 immutable 두-pin reader가 현재 membership과 Free cell을 검증한다.

[선택 인수](../evidence/dev3-status-init-20261022.md)·[source별 색인](../evidence/dev3-status-init-20261022.json)은 필수93을 성공 그룹83·CF12 파생 감사2·CF06 완료 prefix7·수정08 exact1로 구분한다. shared 자원 selector1, compiler semantic13과 별도 baseline, 실제 native3/16효과/66,427,744bytes, checker unique8 및 변경 method4만의 후속 실행을 보존한다. 원 compile/runtime/compiler/checker FAIL은 유지하며 최종 fmt/engine Clippy PASS다. 제한 단위의 최종 독립 D1을 수용했다.

실제 dirty hot191424/cold157408≤196608, reader3480≤3584, exclusive source join≤8192와 padded generic readback 거절을 확인했다. 원 예약을 증액하지 않았으며 stack/RSS 측정이 아니다. S5 추가116은 작성/NOT_RUN, 새 runtime union209다. Status home flush·실제 clean-before-INIT·writer/flusher 경쟁·일반 recovery/거래 directory/READY 및 전체 DEV3는 미완료다.

<a id="status-home-flush"></a>
## Status home flush의 선택 인수

[소유 계약](../contracts/dev3-status-home-flush.md)의3.03.2.b는 기존 bound를 소비하는 별도 owner와 sealed Status authority를 작성했다. 실제 publication의 allocator/Status를 각각 whole batch1로 capture→WAL force→DWB→home→retirement→조건부 finalize하며, Ready/WaitingWal/RetiredPendingFinalize에서 exact target을 보존한다. Status home은 zero 또는 정확한 현재 image만 허용하고 기존 allocator predicate는 유지한다.

[source별 선택 증거](../evidence/dev3-status-home-flush-20261022.md)와 [실행 색인](../evidence/dev3-status-home-flush-20261022.json)에 필수43=Rust42+RF03 witness1, compiler semantic6, native3, checker unique16을 연결했다. strict 성공42행과 원 실패 그룹의 완료 BA01 prefix1을 구분하며 원 FAIL을 보존한다. native는 실제7batch/flush217효과와 전체 파일·WAL·최종 owner 상태를 검산했다. S5 추가588=physical570의 exact mandatory alias6을 제외한564+history24는 작성/NOT_RUN이며 unique runtime/witness631이다. 제한 단위의 최종 독립 D1을 수용했다.

원 dirty196608/flush65536·I/O8192·component65536·4MiB workspace 예약을 증액하지 않는다. 실제77 layout와 source/return-carrier guard, Status codec/FILEMAP 수명 분리를 검산했으며 stack/RSS 측정이 아니다. .a와 allocator의 원 결과를 보존하고 변경 없는 하위 시험을 반복하지 않는다. 일반 pool/batch8/64/multifile·free/reuse·recovery/READY·전체 DEV3는 미완료다.

<a id="paused-after-status-home-flush"></a>
## 2026-10-22 작은 단위 종료·인계

사용자의 “하던 것만 작은 단위로 완료해서 commit하고 새 작업 단위를 시작하지 않는다”는 요청에 따라, 마지막 제품 완료 경계를 [Status home flush 3.03.2.b](#status-home-flush)로 고정하고 작업을 정리한다. allocator flush `0968838`, Status bootstrap `3813235`, Status INIT `d5c2628`, Status home flush `771713a`는 각각 commit을 마쳤다. 전체 DEV3·최종 S5 인수는 미완료다.

마지막 단위의 필수43·compiler semantic6·native3·checker unique16과 독립 D1 결과는 [기존 선택 증거](../evidence/dev3-status-home-flush-20261022.md)를 그대로 보존한다. source별 성공과 원 FAIL을 구별하며 S5 추가588은 작성·NOT_RUN이다. 이번 종료 정리는 문서만 변경하므로 제품 시험·컴파일·정적 검사와 push 전용 문서 검사를 새로 실행하지 않는다.

후속 후보 3.12.1.a의 LEASE/PREFLIGHT/SELECT는 총괄과 `/root/allocation_compiler`, `/root/allocation_native`가 기존 계약·소스·시험 입력을 읽은 상태에서 멈췄다. 두 담당의 proposal 파일은 미작성이고 제품·시험 코드 변경도 없다. API·오류·자원 상한·지원 범위·시험 inventory는 미확정이며 D2 채택이나 구현 완료로 계산하지 않는다. BOOT_RESERVE·repair·일반 recovery/READY는 남은 작업으로 유지한다.

현재 진행하던 제품 코드의 미커밋 변경은 없다. 재개 시에는 [잔여 작업 순서](../plans/dev3-implementation-plan.md#work-organization)와 위 완료 경계를 확인해 후속 후보를 결정한다. 이 인계 기록 자체는 후속 구현·시험을 시작하는 지시가 아니다.

<a id="recovery-select"></a>
## 2026-10-22 DEV3 재개 — 초기 복구 입력 선택

사용자의 DEV3 개발 시작 요청에 따라 [기존 종료·인계](#paused-after-status-home-flush) 이후 작업을 재개했다. 첫 단위3.12.1.a의 [소유 계약](../contracts/dev3-recovery-select.md)에 따라 초기 FILEMAP·Empty DWB·genesis header-only WAL의 닫힌 선택을 구현하고 제한 인수를 마쳤다. 초기 입력 선택과 reserve/cancel·Status INIT/flush의 실제 복원·검산 목표3.12.1–4를 구별한다.

기준 source는 `fdd7ae61ca6b03417a4d40482d4c32d971a3361d`이며 시작 제품 dirty diff는 없다. 계약/통합 `/root`, 제품·unit `/root/recovery_implementation`, native/compiler `/root/recovery_acceptance`, 독립 검토 `/root/recovery_design_review`로 배정했다. 모델 요청 설정과 전용/공유 경로·병합 순서는 소유 계약 F07에 기록한다. 기존 시험 결과는 원래 범위로 보존하고 무변경 하위 wrapper를 실행하지 않는다.

새 SELECT owner가 lease를 보유하고 borrowed observation만 반환한다. XBS1/XDI1은 기존 생성의 pure preparation을 공유하며 fixed geometry·선택 control·opened/named identity를 검산한다. scratch는 선택 호출 중에만 빌리고, 초기 home/비선택 bytes는 읽지 않는다. 진행 selector·Pending·actual nonempty WAL은 명시 거절하며 reader/writer·예약·repair·READY 권한을 만들지 않는다.

[실제 인수](../evidence/dev3-recovery-select-20261022.md)·[source별 색인](../evidence/dev3-recovery-select-20261022.json): 원 source 필수170·compiler semantic5+control1·checker15 PASS, native scaffold 수명 수정과 test helper 조건식 변경 뒤 관련4만 재검증·native8 PASS. 최종 fmt와 engine Clippy PASS, 별도 최종 D1 ACCEPTED다. 모든170을 마지막 binary에서 재실행한 결과가 아니며 원 compile/native/static FAIL을 보존했다. 무변경 하위 wrapper·완료 시험을 반복하지 않았다.

S5 matrix1176·mandatory alias48·추가1128은 작성·NOT_RUN이다. BOOT_RESERVE·repair·일반 recovery/READY·전체 DEV3·최종 S5는 미완료다. 다음은 선택 owner의 실제 profile 예약과 producer prefix의 지원 의미를 확정해 후속 장벽에 연결하는 단위다. 문서 자동 검사는 push 전용으로 유지한다.

<a id="recovery-prefix"></a>
## 현행 WAL prefix의 닫힌 분석 — 선택 검증

기준 HEAD `412bd7b8477cde8abee86de4c5d18611f9d66385` 뒤 [소유 계약](../contracts/dev3-recovery-prefix.md)에 따라 실제 reserve/cancel·Status INIT의 tag2 WAL을 canonical RAM 상태로 분석하는 후속 공급을 구현했다. pure expected context·immutable bootstrap을 공유하며 live role/fence/권한과 기존 초기 SELECT 제한을 유지한다. lease·source WAL File·caller256KiB borrow를 보유한 owner는 allocator/선택적 Status/descriptor·active owner와 남은 의무만 관측한다.

[실제 검증](../evidence/dev3-recovery-prefix-20261022.md)·[source별 색인](../evidence/dev3-recovery-prefix-20261022.json): 새 scanner42·core189의 고유231 완료 단언을 확인했고 완결 job223과 원 실패 job 내 완료 단언8을 구별한다. r3의 관련 RS6·native31/64worker·compiler8+control1, r1 checker15, 최종 r5 fmt/engine Clippy를 보존한다. 전체231을 마지막 binary에서 재실행한 결과가 아니며 원 compile/static/runtime FAIL을 유지한다. fixture 수정 뒤 해당 두 실패만 재검증했다. 별도 최종 독립 D1을 수용했으며 남은 필수 수정은 없다.

source census는 mock33336/native33408≤64KiB, caller256KiB 별도다. native 폐쇄 분석 Filepeak2·mutation0, producer 포함 FD5·모두 final0을 확인했다. selected Pending metadata와 그 historical LSN의 canonical digest를 검산하며 raw image/home 수리를 인증하지 않는다. active reserve를 버리거나 terminal로 바꾸지 않고 actual owned/selector credit/remaining page credit/topup과 terminal 주소 union을 구별한다.

S5 matrix304·alias31·추가273과 별도 native PL01은 작성·NOT_RUN이다. 무변경 하위 wrapper·완료 시험·SELECT 나머지164를 반복하지 않았다. 다음은 실제 BOOT 자원·예약 인계·Pending repair·page별 REDO·repair 재장애다. 일반 recovery/READY·전체 DEV3/S5 인수와 push 전용 문서 검사는 미완료다.

<a id="recovery-effects"></a>
## 현재 genesis의 실제 닫힌 복구 — 선택 검증·최종 독립 D1 수용

2026-10-23 [소유 계약](../contracts/dev3-recovery-effects.md)의 제한 구현설계 D2와 첫 실행 gate를 별도 검토한 뒤 수용했다. 제품 기준 `e6153995bdc6a708346db4039efaf963d5c7b618`, 착수 HEAD `e9cc2737a392b2a1ce726346bb36298c62ca438c`의 기존 CI 변경을 구별한다. 고정 두 슬롯 실제 provisioning·source 안정화·historical Pending 수리·same-prefix 예약 한 번 인계·page별 REDO와 별도 recovery DWB flush를 구현했다. consuming owner는 lease/source File·caller maintenance12MiB/flush4MiB 전체 대여를 유지하며 성공/실패 모두 닫힌 관측만 제공한다.

[실제 검증](../evidence/dev3-recovery-effects-20261023.md)·[source별 색인](../evidence/dev3-recovery-effects-20261023.json): runtime 고유393 완료 단언은 r3 완결 job381·원 실패 job의 EN001–007 완료 단언7·r4 완결 job5로 나뉜다. native는 r3 NR01–14와 r4 실패/미도달17이력의 고유31/95worker이며 원 실패 producer까지 전체96worker를 보존한다. r3 compiler9+control1·r1 checker30과 r4 변경 method1·최종 r4 fmt/engine Clippy를 확인했다. 완료 무변경 사례와 하위 wrapper를 반복하지 않았다. 전체393/31을 마지막 binary에서 재실행한 결과가 아니며 원 FAIL·loop 뒤 gate 미도달과 후속 읽기 감사 시점도 구별한다. 별도 최종 독립 D1을 수용했으며 남은 필수 수정은 없다.

native typed source census는 outer9008+max(prefix33408, inventory3320, work31160)=42416≤64KiB, W16/File48이고 caller16MiB 대여는 별도다.62개 post-lease 관측과 FD peak5/final0·closed File peak2를 확인했다. 고유31 DB 최종525file·747824793bytes, 원 실패까지 전체541file·769957337bytes≤768MiB, 최대24322304bytes≤24MiB와 failed snapshot/기타 증거 한도를 확인했다. failed ledger의 마지막 확인 charge와 실제 raw 물리 길이를 구별한다. 일반 OOM/OS stack/RSS나 전역 disk 선확보 증거는 아니다.

최종 source `b02136125e51902c166d453835466c0150fbaeabb18e255ecbf0b4e9053d216a`, ELF `d6107928b47368e8c1d929d8cebdd8e05825afbc7aba7f1c6b940e8d82f357c5`다. r0 fixture 사전 검토 미채택, r1 compile·r2 lint·r3 두 harness 실패와 최소 수정은 원본으로 보존했다. 제품 로직은 r2 뒤 불변이며 r4는 EN008 fault 해제와 native cut18의 정확 typed/raw 검증만 바꿨다. S5 runtime matrix3504·alias36·추가3468과 native matrix63·alias7·추가56은 작성·NOT_RUN이다.

사용자 요청에 따라 이 단위의 최종 독립 검토·local commit 뒤 멈추며 다음 leaf는 시작하지 않는다. 기존 readonly Prefix 공개 제한과 원 증거를 보존한다. 일반 fixed BOOT/F05·REBASE body/continuation/reclaim·새 epoch/CLR/undo/READY·전체 task12/DEV3/S5는 미완료다. 문서 verifier는 push 전용으로 이번 편집·commit에서는 실행하지 않는다.

<a id="registration-core"></a>
## 고정 등록 코어·세대 pin — 일반 pool의 선행 공급

2026-10-23 기준 `4c1fa5b`에서 [D3-REGISTRATION-001](../contracts/dev3-task-05-buffer-flush.md#registration-core)을 구현했다. 기존 SharedPool가 등록 코어를 바로 소비하며 bootstrap provenance·Status 일회 공개의 고정 제한을 유지한다. pin은 slot/generation을 보유하고 read/write·관측·flush finalize·반납에서 검산한다. 전체 token이 유효하기 전에는 count를 하나도 감소하지 않는다. target 요청은 Copy이며 독립 immutable reader와 병행하는 새 권한을 만들지 않는다.

[실행 증거](../evidence/dev3-registration-core-20261023.md)·[색인](../evidence/dev3-registration-core-20261023.json): r6 source 신규 runtime11·관련 소비10, Linux DB1/두batch·compiler semantic2+control1 PASS. r5 dirty static과 r6 Status static의 원 source를 구별하고 r3 source 미보존 layout 관측·원 compile FAIL도 남겼다. generic3072·Status reader3608/예약3648·dirty193600과 flush 합계259136≤262144를 검토했다. 최종 engine Clippy/fmt와 독립 제품 D1을 수용했다. 문서 verifier는 push 전용이며 이번에는 실행하지 않았다.

이 공급은 일반 directory 전환의 선행 refactor다. 동적 load/eviction/reload·실제 frame 수를 넘는 집합·반복 Status 변경은 다음 구현이며 3.05/DEV3/S5 완료를 주장하지 않는다. source 무변경 시험과 하위 wrapper는 반복하지 않았다.

<a id="allocator-residency"></a>
## allocator 동일 주소의 resident 세대 reload — 제한 선택 검증

2026-10-23 기준 `d31cfb9`에서 [D3-ALLOCATOR-RESIDENCY-001](../contracts/dev3-allocator-residency.md)을 구현했다. consuming wrapper가 원 Bound·descriptor·active 예약·WAL/flush와 Drop을 유지하고 exclusive reload가 실제 selected home을 검산해 generation을 갱신한다. 같은 주소 clean image만 허용하며 old permit·poison·불명 lower 오류·source 불일치는 fence한다. 원 frame/registration/descriptor 보존과 scratch의 부분 변경을 구별한다.

[실행 증거](../evidence/dev3-allocator-residency-20261023.md)·[source별 색인](../evidence/dev3-allocator-residency-20261023.json): 새 runtime19·Fixed 직접2·static4, native1/compiler3/checker10의 고유 선택을 검증했다. r9/r10 같은 source의 다른 ELF와 r11/r12 테스트만 수정한 source를 구별하고 원 FAIL/NOT_RUN을 보존했다. unrelated concurrent 작업은 보존 export 밖의 별도 변경으로 남겼다. conversion61952≤65536, outer2400≤4096, Status+flush259144≤262144와 native File3/FD5/final0을 확인했다. 최종 독립 D1을 수용했다.

32round/64reload S5 확대는 작성·NOT_RUN이다. Empty/다른 주소 교체·N+1 pressure·Status 반복 변경·일반 pool/거래/recovery/READY·전체 DEV3/S5는 미완료다. 이 단위에서 문서 verifier·push를 실행하지 않았다.

<a id="allocator-eviction"></a>
## allocator 실제 비상주 구간·다른 frame 재적재

2026-10-23 [D3-ALLOCATOR-EVICTION-001](../contracts/dev3-allocator-residency.md#eviction-lifecycle)은
`evict_allocator`→실제 MissingTarget→`load_allocator(slot)`를 연결한다. 독점 guard가 원
Bound·예약·descriptor/WAL/flush를 보존하고, 실패한 읽기는 불변 prior와 세대 tombstone을
남겨 명시 재시도한다. 재적재 후 기존 writer/flush와 reload가 현재 슬롯을 따라간다.
[선택 증거](../evidence/dev3-allocator-eviction-20261023.md)는 신규 runtime18·관련7·static5,
Linux 한 이력/두 load와 compiler3+control1을 source별로 구분한다. 원 compile/static FAIL과
독립 리뷰 EVICT-001/002의 수정을 보존한다. OCR delegate의 reviewable11개와 별도 문서·
fixture10개를 검토한 최종 독립 D1을 수용했으며 남은 차단 지적은 없다.

S5 확대32round는 작성·NOT_RUN이다. 실제 다른 주소·N+1 집합·Status 반복 변경·일반
pool/pressure·recovery/READY·전체 DEV3/S5는 미완료이며 문서 검사는 push 전용이다.

<a id="operational-registration"></a>
## 일반 page 등록 코어 P1 — 슬롯별 publication

2026-10-23 [D3-OPERATIONAL-PAGES-001 P1](../contracts/dev3-task-05-buffer-flush.md#operational-registration)은
기존 SharedPool directory의 각 슬롯에 prepared generation과 OnceLock request를 둔다.
generation·교체는 독점 단계로 제한하고 shared INIT publication은 세대를 바꾸지 못한다.
Fixed provenance·allocator tombstone·pin token 의미를 보존하며 별도 전역 publication을 제거했다.
[선택 증거](../evidence/dev3-operational-registration-20261023.md)는 static5·runtime20·Linux2와
원 r0 compile FAIL을 구분한다. D2·실행 전 F05와 OCR delegate 기반 최종 D1은 P1 범위로 독립 수용됐다.
P1 인수 당시 P2/P3는 미공급이었다. 후속 [P2 live owner](#operational-status)와 [반복 복구](#operational-recovery)를 구별하며, 후속 [P3 현재 membership load](#status-load)의 실제3주소/2frame 교체와 검증 상태를 구별한다.
일반 pool·전체 DEV3/S5를 완료로 승격하지 않는다.

<a id="dev3-work-breakdown"></a>
## DEV3 하부 작업 분류와 현재 상태

2026-10-22 후속 [방향 리뷰 반영](../plans/dev3-implementation-plan.md#direction-review-adoption)은 제한 쓰기의 복구→일반 page 수명/live undo→첫 행 거래 연결 순서를 정리한다. 위 종료·인계는 당시 기록으로 보존하며, 후속 [초기 입력 선택](#recovery-select)·[현행 prefix 분석](#recovery-prefix)·[현재 genesis의 실제 닫힌 복구](#recovery-effects)의 실제 API·source/type 회계·선택 인수를 구별한다. 제한된 BOOT·nonempty 복구와 일반 BOOT/F05·64GiB 지원·새 운영 계측·READY·거래 ACK·DEV3/S5의 미완료를 구별한다.

각 작업의 개발 상세 명세와 처리·상태 설계도는 [16개 작업별 문서](../contracts/dev3-task-designs.md)에 있다. 이 설계 작성은 아래 실제 구현·검증 상태를 변경하지 않는다.

2026-10-23 갱신: 기존 16개 분류의 ID를 유지하며 현재 구현 범위를 아래에 정리한다. 실행 순서·하위 작업의 선행/산출물/완료 조건은 [잔여 작업 계획](../plans/dev3-implementation-plan.md#remaining-work)이 소유한다. [작업 크기·우선순위·병렬 운영](../plans/dev3-implementation-plan.md#work-organization)에 따라 실제 배정은 검증 가능한 하위 작업으로 하고 분류 수를 진행률로 환산하지 않는다. 번호는 실행 순서가 아니며 기존 DEV3 기능 범위·S1 인수 조건·다른 단계의 선행을 변경하지 않는다. 표현/모델 검증을 실제 제품 기능 완료로 계산하지 않는다.

| 분류 | 하부 작업 | 현재 상태와 남은 경계 |
| --- | --- | --- |
| 3.01 형식·정체성 | page/참조/key/행/tree/WAL/undo/status/control codec | 여러 표현·독립 byte·유한 모델 검증 완료. 실제 효과와 구분 |
| 3.02 파일·읽기 기반 | namespace·FILEMAP·extent 최초 설치·selector·selected reader | 최초 설치·읽기와 [불변 Status bootstrap 역할](#status-bootstrap)의 제한 구현 검증. 일반 reopen은 미완료 |
| 3.03 공간 할당 | 예약·취소 잔여 인수, INIT 소비, extent 확장, eligible free/reuse | [예약·취소](#allocation-reservation-accepted)와 [한 Status INIT](#status-init)의 제한 연결. [Status home flush](#status-home-flush)의 제한 선택 인수. 일반 INIT·확장·free/reuse 미완료 |
| 3.04 WAL 변경 경로 | semantic producer·stage/apply·append/sync·owner chain | [dirty stage](#dirty-stage)·[authentic WAL requirement/flush force](#allocator-flush)·[Status INIT 두 image](#status-init), genesis·RollbackStart·예약/취소 연결. 일반 opcode/CLR/commit 연결 미완료 |
| 3.05 dirty buffer·flush | dirty/pin/recLSN 등록, WAL→DWB→home flush, retirement/eviction·압력하 진행 | [allocator 한 페이지 flush](#allocator-flush)·[폐쇄 두-frame INIT/reader](#status-init)·[Status home flush](#status-home-flush)의 제한 선택 인수. [등록 코어·세대 pin](#registration-core)과 [같은 allocator 주소의 clean reload](#allocator-residency)와 [실제 퇴출·다른 frame 재적재](#allocator-eviction)까지 공급. [P1 슬롯 등록](#operational-registration)·[P2 반복 immutable Status INIT](#operational-status)·[현재-build 복구](#operational-recovery)까지 제한 공급. [P3 현재 membership Status load](#status-load)의 구현·선택 검증·제한 단위 최종 독립 D1 수용. [정확1..64 전체 대상 pin](#bounded-target-set)과 [여러 Status 상주 수명](#resident-status-pool)의3frame/4주소 이력·현재-build 복구를 추가했다. 일반 page load/eviction·N+1 pressure·live Status 변경·batch8/64·다중 file 미완료 |
| 3.06 트리·행 | mutable tree CRUD·split·root 변경·overflow·stable cursor·locator | 표현·부분 모델 존재. 실제 변경 경로 미완료 |
| 3.07 인덱스 | point/range scan·presence·PK/UNIQUE·build/취소 | 표현·의미 계약 존재. 실제 인덱스와 statement-final unique 미완료 |
| 3.08 카탈로그·DDL | table/index 정의·이름·객체 수명·create/drop | 표현 존재. native 원자 DDL과 catalog 변경 미완료 |
| 3.09 undo·rollback | persistent undo/status·statement rollback·전체 rollback·CLR | 표현·RollbackStart 생산과 [최초 Begin 순수 준비](#begin-preparation) 공급. 실제 발급·live Begin/undo/CLR 진행 미완료 |
| 3.10 동시성·가시성 | snapshot 등록·잠금·관측 행 검증·두 격리 수준 | [3.10.1.a.1 실제 admission/version gate](#concurrency-admission) 제한 검증. page/tree rank·거래 잠금·snapshot/pin/FIFO와 reader/writer/ager 실제 통합 미완료 |
| 3.11 commit·공개 | 여러 객체 원자 commit·durable ACK/UNKNOWN·frontier | 실제 disk 거래 인수 미완료 |
| 3.12 재시작 복구 | preflight/bootstrap·DWB repair·analysis/예약 복원·redo·REBASE·epoch/WRITER·undo/verify·STABILIZE/READY | [초기 입력 SELECT](#recovery-select)·[prefix 분석](#recovery-prefix)의 제한 인수와 [현재 genesis 실제 닫힌 복구](#recovery-effects)와 [반복 Status 복구](#operational-recovery)의 선택 검증. [장벽별 8개 작업](../plans/dev3-implementation-plan.md#recovery-remaining)의 일반 BOOT/F05·REBASE/새 epoch·undo/READY·전체 반복 장애 인수 미완료 |
| 3.13 checkpoint | fuzzy ATT/DPT capture·WAL floor·fallback | 표현/계약 존재. 실제 checkpoint 인계 미완료 |
| 3.14 공간 회수 | undo/status/delete aging·horizon·generation 재사용 | 유한 모델 존재. 실제 회수·압력하 진행 미완료 |
| 3.15 백업·무결성 | S3/fault 인수용 full audit·독립 이력, 이후 offline exact-cut backup/restore | 실제 DEV3 경로 미완료. audit 공급은 backup 완료를 기다리지 않음 |
| 3.16 자원 종합·성능·최종 인수 | 항목별 자원 회계 종합·PF09·DS01–06·KU09/10·RC-T12–14 | opcode별 F05는 각 effect 전 선행. 이 항목은 증거 종합과 남은 인수이며 전체 DEV3 미완료 |

기존 재개 우선 단위 [3.03.1 예약·취소](../plans/dev3-implementation-plan.md#allocation-remaining)는 위 인수 기록을 따른다. 3.10.1.a.1의 공통 admission/version gate는 위 기록을 따르며 dirty frame·typed stage/apply 첫 소비자는 위 기록을 따른다. allocator1 한 페이지 flush와 Status owner의 영속 binding·예약 payload INIT에 이어 [Status home flush와 두 target의 순차 flush](#status-home-flush)까지 제한 인수를 마쳤다. 후속 복구 진입의 [초기 SELECT](#recovery-select)와 [prefix 분석](#recovery-prefix)에 이어 [현재 genesis 실제 닫힌 복구](#recovery-effects)를 선택 검증했다. 일반 BOOT/F05·REBASE/새 epoch·undo/READY는 남아 있다. [원본 부분 검토](../evidence/dev3-allocation-reservation-partial-review-xadb-20261021.json)의 remaining은 역사적 상태로 보존하고 이후 단위별 인수 기록을 함께 읽으며, 완료된 무변경 runtime 사례는 반복하지 않는다. 각 새 쓰기 경로의 자원 계산·복구 의미·fault 입력은 그 경로 전에 정한다. free/reuse는 회수 가능성 판정을, 실제 행 변경은 undo/status 기반을 먼저 수신하되 서로 전체 분류 완료를 요구하지 않는다. 복구 전용 REBASE는 정상 fuzzy checkpoint와 별도이며 선행으로 공급한다. 계획 작성 당시의 미실행 상태와 이후 위 단위별 구현·실행 기록을 구분한다.

예약·취소의 [native와 compiler 병렬 작업](../plans/dev3-implementation-plan.md#parallel-execution-plan)은 해당 인수 기록에 실제 담당·source·결과를 연결했다. 64KiB 분석·공통 수정·stage 검증은 총괄이 조정하고 독립 검토자를 별도로 둔다. 실제 담당/경로·기준 source·자원 profile을 고정하기 전에는 작업 배정이나 동시 실행 완료로 기록하지 않는다.

<a id="operational-status"></a>
## 반복 Status INIT의 consuming owner

2026-10-23 [P2 소유 계약](../contracts/dev3-task-05-buffer-flush.md#operational-status-p2)에 따라
원 flush bound를 소비하는 operational owner와 clean Status 퇴출·세대 준비를 공급했다.
A INIT→flush→퇴출→B cancel→재예약 INIT의 실제 owner 및 Linux home/WAL 비교를
[선택 증거](../evidence/dev3-operational-status-20261023.md)에 보존한다. static6·신규 runtime14,
관련 closed selector1·native1을 source별로 구별하며 compiler semantic5+control1·checker4도 PASS이며 OCR delegate 독립 D1을 live owner 한정 수용했다.
원 compile 실패를 보존하고 완료 시험은 반복하지 않는다. 이 live owner 인수 당시 반복 sequence
복구는 미구현이었다. 후속 [현재-build 반복 복구](#operational-recovery)에서 별도 새 증거를
공급한다. 인수 당시 남았던 P3 membership load·3주소/2frame은 [별도 후속](#status-load)으로 연결한다. 반복 live Status 변경·batch8/64·pressure·DEV3/S5는 남아 있다.

<a id="operational-recovery"></a>
## 반복 immutable Status 이력의 현재-build 복구

2026-10-23 [F09](../contracts/dev3-recovery-effects.md#operational-status-recovery)는 기존 effect owner에
첫 extent Status1..63의 canonical census·주소별 Pending/REDO·예약 topup을 연결한다.
readonly Prefix의 첫 INIT 제한은 유지한다. 두 pass는 같은 trusted terminal/population
validator를 사용하고 각 Status의 고유 INIT LSN·identity·full image digest를 검산한다.
기존 maintenance12MiB+flush4MiB loan 안에서64page와 고정 census를 사용한다.

[선택 증거](../evidence/dev3-operational-recovery-20261023.md)의 신규65(원 r1의57+새 r5의8),
관련3·fresh Linux4이력/14worker·compiler3+control1 PASS는 각각 보존 source의 결과다.
원 compile/checker/Clippy FAIL을 보존하고 OCR delegate 최종 독립 D1을 이 제한 복구 단위로 수용했다. native는 정상·부분
INIT WAL·B home 부분 쓰기·retirement 장애 뒤 새 process 복구와 재진입의 전체 bytes를
독립 literal로 대조했다. 정적 source/type 회계는 memory51304/native51512≤65536이며
entry FD5/final0이다. OS stack/RSS나 전체 opcode F05 인증으로 일반화하지 않는다.

P2 반복 immutable Status의 현재-build 복구 통합을 공급했으며 새 S5 확대2379개는 작성·
NOT_RUN이다. P3 membership 기반 A reload·3주소/2frame은 [후속](#status-load)으로 구별한다. live Status 변경, 일반 pool/
pressure·REBASE/READY·전체 DEV3/S5는 미완료다. 문서 검사는 push 전용으로 실행하지 않는다.

<a id="status-load"></a>
## P3 — 현재 membership Status home load

[P3 계약](../contracts/dev3-task-05-buffer-flush.md#operational-status-p3)에 따라 기존 두-frame owner에
`load_status(PageRef)`를 연결했다. dirty allocator의 현재 membership과 descriptor/WAL prefix를
검산하고 selected home을 기존 scratch에 읽은 뒤 clean payload의 generation/publication을
교체한다. 실제3주소/2frame A/B 읽기와 후속 prepare/INIT가 대상이다.
[선택 증거](../evidence/dev3-status-load-20261023.md)는 source별 실행/원 FAIL/미실행과
OCR delegate 독립 리뷰를 구별한다. 최종 독립 D1은 이번 제한 load 단위로 수용했으며 일반 pool·pressure·live Status 변경·
전체 DEV3/S5 완료가 아니다. load는 파일을 쓰지 않으므로 앞선 반복 이력 복구의 원 결과를
보존하고 변경 없는 복구 시험은 반복하지 않았다.


<a id="begin-preparation"></a>
### 최초 빈 거래 Begin의 순수 준비

[obligation §16](../contracts/dev3-obligation-format.md#begin-preparation)의
`semantic_group::begin`은 현재 epoch/local+1, 실제 Free(g)→Active(g+1), home/page identity와
page prior≤global predecessor를 검산한다. caller의3040-byte frame·16KiB after page에 단일
home native/Patch와 최초 descriptor를 생성하며 immutable observation을 반환한다.
이 결과는 실제 발급·slot claim·reservation·stage/apply 권한이 아니다.

[선택 증거](../evidence/dev3-status-begin-20261023.md)는 r0 reserved keyword fmt FAIL,
r1 compile/runtime12/static1/fmt/Clippy PASS, frozen D2와 독립 runtime gate를 보존한다.
component31,040과 caller loans35,808은 별도이며 실제 live owner/RSS/F05 완료가 아니다.
최종 독립 D1을 순수 준비 단위로 수용했다. S5 profile3×slot71×generation3의639조합은 작성·NOT_RUN이다.
일반 authoritative pool, directory census/ID 발급, 실제 Begin→rollback/terminal와 복구,
tree/행 거래 및 전체 DEV3/S5는 남아 있다.

<a id="bounded-target-set"></a>
## 단일 SharedPool의 정확 전체 대상 pin 집합

[D3-TARGET-SET-001](../contracts/dev3-task-05-buffer-flush.md#bounded-target-set)은 기존
`reserve_targets`와 같은 directory·pins·DPT에서 exact const N1..64를 처리한다. 모든 대상의
중복/lookup/순위·quota를 검산한 뒤 count를 함께 늘리며, read/write와 release도 전체 집합을
검산·정리한다. 기본2 carrier와 per-frame34/전체68/owner34 한도는 유지한다.

[선택 증거](../evidence/dev3-target-set-20261023.md)는 source별 runtime19·static2·checker3,
compiler7과 원 compile FAIL을 보존하며 최종 독립 D1을 제한 단위로 수용했다. 일반 mutable registration/
load·실제 N+1 dirty/flush/evict/reload·256/1024/4096 backing·live Begin/undo/terminal·복구는
후속이다. bootstrap 시험 페이지는 실제 일반 I/O/load 인수가 아니다. S5 확대576은 작성·NOT_RUN이며
전체 DEV3/S5를 완료로 승격하지 않는다.


<a id="resident-status-pool"></a>
## 여러 immutable Status의 실제 상주 수명

[D3-RESIDENT-STATUS-001](../contracts/dev3-task-05-buffer-flush.md#resident-status-pool)은 원
SharedPool/권한에서 caller2..64 fresh frame을 받아 slot별 INIT·주소별 exact read/flush·clean 퇴출과
지정 슬롯 home load를 연결한다. allocator0은 고정이며 선택 슬롯은 INIT 목적지다. 필수3frame/4주소의
실제 A/B/C 이력 뒤 별도 process recovery/reentry가 같은 build에서 통과했다.

[선택 증거](../evidence/dev3-resident-status-20261023.md)는 신규31·직접4·static2·compiler9·checker8·
native1의 원 source별 결과·compile/runtime/Clippy FAIL과 수정 해당 사례를 구별한다. 최종 독립 D1을
제한 단위로 수용했다. F05 whole261736≤262144(여유408)이며 원 범위를 넓히는 RSS 주장은 없다.
S5 profile2/3×N4/8/64,146victim/8200late 조합은 작성·NOT_RUN이다. loaded live Begin/undo·자동 victim/
pressure·다음 extent·큰 backing·일반 READY·tree/행 거래·전체 DEV3/S5는 미완료다.


<a id="empty-transaction"></a>
## 빈 거래의 실제 owner 연결 작업

[작업·선택 증거](../evidence/dev3-empty-transaction-20261023.md)의 목표는 기존 resident Status에서
실제 Begin→RollbackStart→빈 AbortEnd 및 새 process의 닫힌 복구다. `semantic_group::empty_abort`
순수 준비와 공통 native/page binding을 작성했으며 [§17](../contracts/dev3-obligation-format.md#empty-abort-preparation)의
D2와 component41640바이트의 실행 gate를 수용하고 신규13·직접 관련2·static1 및 fmt/Clippy를 첫 실행에서 통과했다. 최종 독립 D1은 순수 준비 범위로 수용했다. 당시 실제 TxnId 발급·예약/설치 owner·live Status·통합 F05·해당 복구는 미공급이었다.
소비자 미도달 상태로 두고 순수 helper의 선택 결과를 실제 거래 완료로 세지 않는다.

동일 통합의 [소유권 인계](../contracts/dev3-obligation-format.md#empty-transaction-handoff)를 추가했다.
원 ResidentStatus를 소비해 system branch를 감추고,32slot용 caller bank53,248바이트를 보존한다.
실제 allocator pin/descriptor 검산과 forgotten claim/cleanup 장벽을 구현했다. 전환만의 자원식과 source별 선택 결과는
[같은 증거](../evidence/dev3-empty-transaction-20261023.md#owner-handoff)에 보존한다.

2026-10-24 [§19의 실제 live 설치](../contracts/dev3-obligation-format.md#empty-transaction-live)를
연결했다. 원 pool/WAL에서 Begin·RollbackStart·빈 AbortEnd를 실행하고32slot 원장·directory
census·사전 예약·pending 실패 fence·durable terminal 뒤 반환을 검산한다. 신규22·직접 관련2,
변경된 자원식과 compiler2·checker1 및 fmt/Clippy의 source별 결과와 원 FAIL은
[증거](../evidence/dev3-empty-transaction-20261023.md#live-installation)에 보존한다.
최종 독립 D1은 WAL/RAM 설치 단위로 수용했으며 selected resident만 지원한다. 이 시점에는 live Status flush/load, 새 process의
현재-build 복구·재진입이 미공급이었으며 같은 통합의 소비자 도달0/3·DEV3/S5 미완료를 유지한다.

2026-10-24 [§20 이력 기반 live flush](../contracts/dev3-obligation-format.md#empty-transaction-flush)를
연결했다. 원 WAL의 전체 provisioning과 선택 거래 전이로 만든 exact historical image만 기존
DWB/home 경로가 허용한다. complete prefix EOF·semantic 불일치는 fence하고 알려진 일시적
읽기 실패는 재시도 가능하다. 신규 runtime11·native1, 변경 layout static3·compiler2·checker1의
source별 결과와 원 실패는 [같은 증거](../evidence/dev3-empty-transaction-20261023.md#live-history-flush)에
보존한다. native3상태 page와 WAL 전체 bytes를 독립 literal과 대조했다. 최종 독립 D1은 이력/flush/continuation 한정 수용했으며
live load·새 process 복구·소비자 도달·DEV3/S5는 여전히 미완료다.
