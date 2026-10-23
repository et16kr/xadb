# DEV3 진행 현황 후속 리뷰·상태 기록 보완

2026-10-23 · **정적 리뷰 완료 / 진행 기록 D0 보완 / 새 제품 결함 확정 없음 / 제품 시험 NOT_RUN / DEV3·S5 미완료**

[현재 구현](../implementation/dev3-implementation.md#dev3-work-breakdown) · [구현 순서](../plans/dev3-implementation-plan.md#sequence) · [시험 선정 기준](../plans/dev3-test-plan.md#selection) · [앞선 코어 리뷰와 snapshot 수정](dev3-code-review-20261023.md)

## 결론과 범위

사용자의 “Dev3의 지금까지 진행 내용을 리뷰하고 문제를 바로 수정” 요청으로 현재 공급·잔여 인수와 최신 구현을 대조했다. DEV3는 표현·모델과 실제 저장 기반의 제한 단위를 공급한 상태다. 내부 S2의 일반 buffer/tree/undo는 남아 있으며 제한된 3.12 복구 공급이 S3 거래·전체 복구 완료를 뜻하지 않는다. 제품 gate S1과 내부 최종 S5도 미완료다. 분류 수나 테스트 수로 완료율을 계산하지 않았다.

기존 코어51파일 리뷰의 결과와 D3-OCR-001 수정 전 FAIL·수정 후5건 PASS·독립 D1 기록을 수신했다. 그51파일 전체를 이번에 새로 전수 감사하거나5건을 재실행하지 않았다. 이번 직접 코드 검토는 allocator reload commit `b9d3641`의 OCR 대상19파일 및 후속 `3e0116e`의 snapshot 수정 Rust4파일 diff다. 이전 호출 경로·계약은 이 변경을 판단할 문맥으로 열람했다. 전체 DEV3 시험 도구·모델·제품 코드를 새로 전수 검토했다는 주장은 하지 않는다.

착수 때 기존 snapshot 수정은 미커밋 상태였고 검토 중 별도 작업에서 `3e0116e4c158f8f5767fb2961d52905729ac20eb`로 commit됐다. 이번 작업이 만든 commit이 아니다. 그 뒤 기준 HEAD와 검토 source를 고정했다. 기존 COMM·DEV2·AGENTS 작업은 유지했다. 공유 README/catalog에는 이번 항목만 추가했다.

## 발견 사항과 즉시 수정

| ID | 분류 | 문제·영향 | 처리 |
| --- | --- | --- | --- |
| D3-PROGRESS-001 | medium / documentation · D0 | 구현 문서 머리말은10월20일 표현·모델 요약에 머물고, 3.05 현재 표에는 등록 코어·동일 주소 reload가 빠져 있었다. 두 단계 JSON의 `leaf_progress.scope`도 초기 DWB 설치 시점 설명을 유지했고 후속 dirty/Status/effects/registration/reload 증거 연결이 빠져 있어 소비자가 실제 공급 범위를 잘못 읽을 수 있었다 | 현재 구현의 머리말·3.05 표, 구현/상위/evidence 색인, 두 단계 JSON의 제한 범위·소유 계약·증거 링크를 갱신했다. 기존 `evidence_ref`와 historical receipt는 보존했다 |

새 제품 결함은 위 직접 검토 범위에서 확정하지 못했다. D3-OCR-001은 이미 수정된 사항이며 이번 신규 발견·수정으로 합산하지 않는다. 최종 snapshot의 원 phase/cause 보존·외부 `CallerMustResolve`·공통 fence 및 installation/terminal 의무 유지, 정상 guard 반환 시 해제를 현재 diff와5개 결정적 시험의 단언에서 확인했다.

이번 수정은 현재 상태 설명과 참조의 동기화다. 계약·알고리즘·시험 기준·제품/시험 코드는 변경하지 않았다. 단계 전체 `execution_status=NOT_RUN`, 빈 최종 인수 `evidence_refs`, `run_entrypoint=null`과 공식 배포 미실행 상태는 유지했다. 기존 source별 PASS·FAIL·NOT_RUN을 갱신하거나 새 결과로 합치지 않았다. D0이며 별도 D1/D2 승격이나 독립 검토 완료를 새로 주장하지 않는다.

## 진행·남은 경계

| 영역 | 확인한 현재 공급 | 남은 필수 범위 |
| --- | --- | --- |
| 표현·모델 | page/key/reference·행/tree/WAL/undo/status/control 표현, M01–M06 유한 모델의 기존 증거 | 실제 거래·I/O 통합은 각 소비 단계에서 별도 인수 |
| 할당·WAL·flush | reserve/cancel, dirty stage, allocator 한 페이지 flush, 폐쇄 두-frame Status INIT/home flush | 일반 INIT·공간 확장/free/reuse, 일반 opcode/CLR/commit |
| buffer 수명 | 고정 등록·세대 pin, 동일 주소 clean allocator home 재읽기·세대 갱신 | Empty/load/eviction·다른 주소·N+1 pressure·반복 Status 변경·일반 pool |
| 복구 | 초기 SELECT·현행 prefix 분석·고정 bootstrap·historical Pending 수리·예약 인계·page별 REDO의 닫힌 owner | 일반 BOOT/F05·REBASE·새 epoch·undo/CLR·STABILIZE/READY·반복 장애 인수 |
| 거래·최종 인수 | 관련 표현·설계·부분 공급 | mutable tree/행/index/catalog, isolation·commit ACK, checkpoint/회수/backup, 자원 종합·PF09·최종 S5 전수 |

다음 기능은 기존 [병합 순서](../plans/dev3-implementation-plan.md#parallel-waves)에 따라 일반 page 수명·live undo/tree와 첫 행 거래의 공급·복구 장벽을 연결한다. 이 리뷰가 새 기능 개발이나 최종 S5 실행을 시작한 것은 아니다.

## 직접 정적 검토 내용

- 원 Bound·descriptor·active 예약·WAL/flush 소유와 Drop fence가 consuming adapter에서 보존되는지, live split borrow가 reload를 차단하는지 확인했다.
- reload 이전 dirty/pin/claim/DPT/requirement·terminal/installation·MAX 거절, 실제 selected read, full page equality, prefix/descriptor 재검산, 세대 설치 전 모든 실패 검사를 대조했다.
- 원인/효과/확실성 보존, 구조 불일치·poison·unwind·공통 fence, stale permit의 새 dirty 세대 오염 방지와 실패 후 원 등록 보존을 검토했다.
- native의 독립 literal·RAM/home/descriptor/WAL·정확 read trace·namespace mutation0·FD/시간 한도, compiler의 unmodified baseline·정확 E0499/primary span·artifact/source 결합, checker의 known-bad 단언을 읽었다. 실제 시험 실행으로 세지 않는다.
- 기존 residency 증거에 기록된 소유19개 source의 SHA-256과 현재 파일을 대조했다.18개가 일치하고 `dirty_allocation.rs`만 후속 snapshot 수정으로 다르다. 해당 차이는 `3e0116e` diff와 별도 증거에 연결하며 원 r9–r12 receipt의 범위를 늘리지 않는다.

## OCR 파일별 coverage

`open-code-review-delegate`의 `ocr delegate preview --format json --commit b9d3641` 및 `ocr delegate rule --format json`으로 파일/규칙을 고정했다. 수정 파일은 commit diff와 현재 문맥, 추가 파일은 동일 원문의 전체 내용을 읽었다. OCR는 선택·규칙 도구이고 실제 판단은 `/root`가 수행했다.

- `total_files=29`, `reviewable_files=19`, `reviewed_files=19`, `skipped_files=0`, `coverage_rate=100%`(19/19).
- OCR 제외10개: Markdown7개는 `unsupported_ext`, compiler fixture3개는 `default_path`. 해당10개도 계약·증거·fixture 문맥으로 읽었으나 위19개 분모에 합산하지 않는다. 이 비율은 DEV3 전체 coverage가 아니다.
- `3e0116e`의 Rust4파일 diff는 별도 보조 범위다. `dirty_allocation.rs`는 위19개와 중복되며 단순 합산하지 않는다.

| path | OCR status | 처리 |
| --- | --- | --- |
| `crates/engine/src/storage/dirty_flush.rs` | modified | reviewed |
| `crates/engine/src/storage/dirty_frame.rs` | modified | reviewed |
| `crates/engine/src/storage/dirty_frame/registration.rs` | modified | reviewed |
| `crates/engine/src/storage/dirty_frame/residency.rs` | added | reviewed |
| `crates/engine/src/storage/wal_io.rs` | modified | reviewed |
| `crates/engine/src/storage/wal_io/allocator_flush.rs` | modified | reviewed |
| `crates/engine/src/storage/wal_io/allocator_residency.rs` | added | reviewed |
| `crates/engine/src/storage/wal_io/allocator_residency_native_tests.rs` | added | reviewed |
| `crates/engine/src/storage/wal_io/allocator_residency_tests.rs` | added | reviewed |
| `crates/engine/src/storage/wal_io/bootstrap.rs` | modified | reviewed |
| `crates/engine/src/storage/wal_io/dirty_allocation.rs` | modified | reviewed |
| `crates/engine/src/storage/wal_io/doublewrite_install.rs` | modified | reviewed |
| `crates/engine/src/storage/wal_io/mutable_storage.rs` | modified | reviewed |
| `devdoc/documents.json` | modified | reviewed |
| `devdoc/evidence/dev3-allocator-residency-20261023.json` | added | reviewed |
| `devdoc/policies/repository-build-plan.json` | modified | reviewed |
| `scripts/check_dev3_allocator_residency_borrows.py` | added | reviewed |
| `scripts/dev3_allocator_residency_native.py` | added | reviewed |
| `scripts/test_dev3_allocator_residency_acceptance.py` | added | reviewed |

## 실행·보존 상태

검토 보조 자료는 로컬 `artifacts/dev3-progress-review-20261023-pqk8rwju/`에 보존했다: `baseline.json`, `preview.json`, `rules.json`, `source-manifest.json`, `review.json`. Git 배포 receipt나 제품 시험 결과가 아니다. 문서 수정 전19파일은 검토 snapshot에서 달라지지 않았음을 확인했다. 이후 문서/catalog의 이번 D0 diff는 별도로 읽기 검토했다.

제품·시험·harness 코드 변경이 없어 제품 시험·compiler probe·fmt/Clippy를 실행하지 않았다. 기존 완료 시험·하위 wrapper·COMM 인수·S5도 실행하지 않았다. 문서 자동 검사는 사용자 지시대로 push gate에 남겼다. 이번 commit/push는 없다.
