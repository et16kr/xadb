# DEV4 병합 전 리뷰와 선택 수정 검증

2026-10-26 · **변경 범위 독립 리뷰·수정 검증 완료 / 원 깊이 5 결과 보존 / FE14·영속 재개·DEV4/Q1 미완료**.

사용자의 Opus DEV4 브랜치 리뷰·문제 수정·main PR 병합 사전 준비 요청에 따른 기록이다.
대상은 `dev4-independent`의 `688b9af..7327e7a` 5개 commit, 59개 파일이다. 원 작업 트리
`/home/et16/work/xadb-dev4`와 main의 DEV3 미커밋 변경은 수정하지 않았다.
관리 worktree `/home/et16/.codex/worktrees/dev4-merge-review/xadb`에서 `codex/dev4-merge-review`를 만들고,
원격 main `9275154`를 `2877436`으로 합쳤다. 충돌은 `devdoc/README.md` 하나였으며 양쪽 기록을 보존했다.
로컬 main의 미push commit 2개와 미커밋 작업은 이 PR에 포함하지 않았다.

## 발견과 수정

| 심각도 | 문제·기존 반례 | 수정 |
| --- | --- | --- |
| P2 | descriptor·matching continuation의 T가 다른 actor/E/S여도 조립 성공 | 선택 T의 caller scope 검사 |
| P2 | public result의 slot.statement 역참조가 없어도 Fetch 조립 성공 | statement session·slot 역참조 검산; parked private result의 primitive·scope도 확인 |
| P2 | 외래 resource 또는 O identity를 원 holder/T와 연결해 출력 | 실제 선택한 live owner의 원 scope와 transferred O full identity, O/T 출생 session·scope 검산 |
| P1 | visited index를 할당·초기화한 뒤 메모리 한도 검사; 작은 budget에서도 정상 profile 16MiB·깊이 5 profile 512MiB를 먼저 할당 | checked 크기 계산·할당 전 budget 검사 |
| P2 | block header 한 칸만 차감한 뒤 Vec 자동 성장으로 상한 초과; 재할당 공존분이 peak 보고에서 누락 | exact header 예약·옛/새 배열 공존분 차감과 보수적 peak 기록 |
| P2 | state cap 0에서 initial을 게시하고 Drained 반환 | 할당·게시 전 StateCap 반환 |
| P2 | 매우 큰 workers의 allowance 곱셈 overflow panic | checked 산술·CapacityCap 반환 |

선택 참조 수정은 [조립기 계약](../contracts/dev4-lifecycle-assembler.md)의 S2/S3와 채택 readset-v2 §3/§5의
누락을 보완한다. State 전체 도달성 검증을 추가하지 않았고 다른 종류 continuation의 payload도 새로 읽지 않는다.
원 holder가 있는 선택 row에서만 scope를 비교하며 정상 transferred owner와 fence-retired private result를 유지한다.
탐색기 수정은 [자원 한도 계약](../contracts/dev4-lifecycle-explorer.md)을 유지한다. worker allowance는 여전히 추정이며,
`capacity_peak`는 재할당 공존분을 포함한 보수적 차감 peak이고 실측 메모리는 별도 `VmHWM`이다.

## 실행 선택과 결과

[기계 판독 기록](dev4-merge-review-20261026.json)에 선택 함수·case ID·source/ELF/log SHA와 실제 exit code를 보존했다.
원 로그는 이 worktree의 `artifacts/dev4-merge-review/`에 있다. 검증 전 마지막 source diff는 explorer 초기화/예약/peak와
assembler 선택 참조 검사 3개 구현 파일 및 2개 시험 파일이다. 아래 선택을 실행 전에 고정했다.

| 범위 | 선택 이유 | 실제 결과 |
| --- | --- | --- |
| explorer 신규 4개 RED | index budget·state cap0·worker overflow·header budget 원 결함 재현 | 모두 FAIL, 원 로그 보존 |
| assembler 신규 5개 RED | foreign T 2종·slot backlink·resource scope·O scope 원 결함 재현 | 모두 FAIL (`Ok` 대 `InvalidState`), 원 로그 보존 |
| explorer 신규 5함수 | 위 4개 교정과 metadata 재할당 peak 경계 | 모두 PASS |
| explorer 기존 depth-one 1함수 | 변경 bootstrap/reserve 경로의 정상 BFS 연결 | PASS, 25 states·1533 slots |
| assembler 신규 8함수 | 원5개 + private result scope·statement session·O birth session 경계 | 모두 PASS, 각각 256KiB worker의 새 입력1개 |
| assembler 기존 8개 exact case | 변경된 descriptor/continuation/owner/O 경로의 정상·terminal·transferred·retired·peer census 보존 | 모두 PASS, 그룹 전체 미실행 |

GREEN은 신규13함수와 변경 경로의 기존 정상9회, 총22회이며 같은 code에 대한 추가 반복은 없다.
compile은 `cargo test -p xadbtest -p xadbtest-reference --lib --offline --locked --no-run`, 실행은 보존 ELF에서
exact 함수와 `XADB_ASSEMBLY_CASE`를 사용했다. source는 실행 전후 동일했다. 이후 `Report.capacity_peak`의
Rustdoc만 명료화했으며 시험 당시 SHA와 최종 주석 SHA를 나누어 남겼다. 주석 변경 때문에 시험을 반복하지 않았다.
전체 workspace·하위 wrapper·조립기1994 전수·모델/checker 완료 suite·깊이5 BFS를 재실행하지 않았다.

원 [깊이 5 실행](dev4-lifecycle-explorer-20261025.md)은 원 source의 `DrainedUnarchived`이며 새 수정 source의
전체 공간 PASS로 승계하지 않는다. 원 FAIL·INCONCLUSIVE도 보존한다. checker 과거 증거의 최종 표는 assembler1994
재실행을 기록하지만 마지막 범위 문장에는 조립기 미실행이라고 되어 있다. 이 표현 불일치는 당시 표·원 로그의 범위로
해석하며, 이번에 그 전수를 다시 실행하거나 과거 실행을 새 PASS로 재분류하지 않았다.

## 독립 검토와 범위

통합·수정 담당은 `/root`다. AGENTS/DP01의 별도 검토 요구에 따라 구현자와 다른 AI 실행을 사용했다.

| 검토자 | 검토·교정 확인 | 판정 |
| --- | --- | --- |
| `/root/review_assembler` | assembler5개 Rust 파일·소유 계약/증거·fixture/provenance; root 수정3파일·RED5·GREEN 신규8/선택8과 source hash | 초기 P2 3종 지적, 수정 D1 ACCEPT |
| `/root/review_model` | 모델7개 Rust 파일·소유 계약/원문/증거; root explorer 수정2파일·RED4·GREEN6과 source hash | 모델 ACCEPT_LIMITED, explorer 수정 ACCEPT; peak 문구 요청 반영 |
| `/root/review_checker` | checker/차등15개 Rust 파일·소유 계약/증거; merge의 공유 문서6개를 양 parent와 비교, 최종 계약/구현/계획/증거/색인12개와 exact 로그 대조 | 확정 코드 결함 없음, 문서 D1 ACCEPT·상태 문구 보완 확인 |
| `/root` | explorer3개 Rust 파일·계약/증거·공유 문서·최종 diff와 선택 실행·수정 통합 | 위 수정을 반영 |

검토자는 기존 시험을 실행하지 않았다. 모델과 checker의 일반 손상 Env1 입력 전체 검증은 기존 계약 영역 밖이다.
fixture 검토는 변경 rich/supplement336개의 raw/expected header, owner 정렬·중복, obligation count/summary,
전체 1994 inventory/hash, 파생 상태의 선택 T/O/resource scope, provenance Python 전체와 원자료 byte 보존을
포함한다. rich304 모든 기대 필드를 정본에서 개별 수작업 재도출한 것은 아니다. 기존 RW Resume Eof+Incomplete
독립 witness 공백도 이 수정에서 완료로 바꾸지 않는다.

OCR preview의 total_files=59, reviewable_files=32, reviewed_files=32, skipped_files=0,
coverage_rate=100%다. 자동 제외27개도 아래 문서/fixture 정적 검토에 포함했다. 이 비율은 파일 범위 집계이며
모든 경로·모든 의미 조합 검증률이 아니다. 원 59개 파일의 `(path,status)`와 검토 경계는 다음과 같다.

| 원 변경 경로 | 상태 | 검토 경계 |
| --- | --- | --- |
| `crates/testing/controller/src/lib.rs` | modified | root / 전체 소스 |
| `crates/testing/controller/src/lifecycle.rs` | added | root / 전체 소스 |
| `crates/testing/controller/src/lifecycle/tests.rs` | added | root / 전체 소스 |
| `crates/testing/reference/src/lifecycle/assembler_literal.rs` | added | assembler / 전체 소스 |
| `crates/testing/reference/src/lifecycle/assembler_tests.rs` | added | assembler / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/delta.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/expect.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/fence.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/gates.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/ident.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/materialize.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/owners.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/result.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/session.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/shared.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/statement.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker/txn.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/checker_tests.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/differential_tests.rs` | added | checker / 전체 소스 |
| `crates/testing/reference/src/lifecycle/input_assembler.rs` | added | assembler / 전체 소스 |
| `crates/testing/reference/src/lifecycle/input_assembler/rows.rs` | added | assembler / 전체 소스 |
| `crates/testing/reference/src/lifecycle/mod.rs` | modified | assembler / 전체 소스 |
| `crates/testing/reference/src/lifecycle/model.rs` | added | model / 전체 소스 |
| `crates/testing/reference/src/lifecycle/model/common.rs` | added | model / 전체 소스 |
| `crates/testing/reference/src/lifecycle/model/edit.rs` | added | model / 전체 소스 |
| `crates/testing/reference/src/lifecycle/model/result.rs` | added | model / 전체 소스 |
| `crates/testing/reference/src/lifecycle/model/session.rs` | added | model / 전체 소스 |
| `crates/testing/reference/src/lifecycle/model/statement.rs` | added | model / 전체 소스 |
| `crates/testing/reference/src/lifecycle/model_tests.rs` | added | model / 전체 소스 |
| `devdoc/README.md` | modified | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/contracts/README.md` | modified | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/contracts/dev4-lifecycle-assembler.md` | added | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/contracts/dev4-lifecycle-checker.md` | added | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/contracts/dev4-lifecycle-explorer.md` | added | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/contracts/dev4-lifecycle-model.md` | added | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/documents.json` | modified | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/evidence/README.md` | modified | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/evidence/dev4-lifecycle-assembler-20261025.md` | added | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/evidence/dev4-lifecycle-checker-20261025.md` | added | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/evidence/dev4-lifecycle-explorer-20261025.md` | added | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/evidence/dev4-lifecycle-model-20261025.md` | added | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/implementation/dev4-implementation.md` | modified | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/plans/dev4-preparation-plan.md` | modified | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/policies/repository-build-plan.json` | modified | owner 검토자·root / 계약·증거·통합 정합 |
| `devdoc/policies/repository-build-plan.md` | modified | owner 검토자·root / 계약·증거·통합 정합 |
| `tests/fixtures/dev4-lifecycle-assembly/README.md` | modified | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/provenance/author-continuation.py` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/provenance/author-normal-rest.py` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/provenance/author-rich.py` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/provenance/rich-snapshot.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/rich/correction.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/rich/rich-continuation-3.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/rich/rich-normal-2.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/rich/rich-normal-rest-4.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/selection-inventory.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/supplement/cases-2.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/supplement/cases-3.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |
| `tests/fixtures/dev4-lifecycle-assembly/supplement/cases.json` | added | assembler / fixture 구조·hash·작성표 (위 한계 적용) |

## 병합 준비와 남은 의무

main 변경을 통합한 source 위에서 관련 선택 검증을 마쳤다. push 때는 저장소 pre-push hook이 전송 tip의 임시 export에서
fmt·workspace Clippy·문서 검사를 수행한다. 이 기록 작성 시점에는 push gate가 아직 실행되지 않았으며 결과는 실제
push 로그와 PR에 남긴다. 문서 편집 중 verifier는 실행하지 않았다. 자동 merge·main push는 이번 작업에 포함하지 않는다.

이 PR은 DEV4 P5 시험 도구를 병합할 준비이며 DEV4 전체 완료 판정이 아니다. archive·durable checkpoint/restart,
witness·FE14, 깊이6, 실제 SQL/engine 연결과 DEV3 공급 인수는 별도 미완료다. 원 깊이5 성공 관측을 그 범위로 확대하지 않는다.
