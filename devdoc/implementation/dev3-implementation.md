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
