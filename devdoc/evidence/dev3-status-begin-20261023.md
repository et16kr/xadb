# DEV3 최초 빈 거래 Begin 준비

2026-10-23 · **순수 준비 구현·선택 검증 완료, 제한 단위 최종 독립 D1 수용**.
[소유 계약](../contracts/dev3-obligation-format.md#begin-preparation)과
[source·receipt 색인](dev3-status-begin-20261023.json)을 따른다. DEV3 기준 commit은 `742aaea`,
실행 HEAD는 별도 COMM 문서 commit `b118af2`다. 실제 거래 발급·Begin stage/apply·일반 pool·
rollback/terminal·현재-build 복구와 전체 DEV3/S5 완료가 아니다.

## 실제 공급

private `semantic_group::begin`의 observer/materializer/producer는 caller가 준 현재 epoch와
issued local+1 후보, 기존 Free(g) 슬롯의 g+1, PageRef/owner/generation을 검산한다.
page prior LSN은 nonzero이면서 global predecessor 이하이며 equality를 강제하지 않는다.
선택 home만 빈 Active로 전이하고3040-byte Begin frame·1536-byte 최초 descriptor·16KiB
페이지를 생산한다. 단일 native/Patch, 변화 없는 나머지 cell, 정확 초기 예약량을 full-state로
검산한 immutable observation이며 실제 ID·pin·claim·reservation·apply permit를 발급하지 않는다.

occupied Status home 집합을 directory 정본으로 삼는 후속 owner, RAM issued high-water와
adopted high-water 분리, 최초 Free1→Active2, 영속 StatusSlot consume와 구별되는 ephemeral
claim, 빈 rollback/terminal의 비용 경계는 D2 계약에 고정했다. 전역 census와 이 예약의 실제
충분성은 후속 live owner·terminal·F05 구현/검증 없이는 수용하지 않는다.

## source별 실행과 실패 보존

원본은 `artifacts/dev3/status-begin-20261023`에 보존한다. 독립 raw offset/struct/SHA로 작성한
literal4의 총37,344 bytes를 제품 encoder와 독립적으로 준비했다. 최초 draft의 magic/redo
header 정정은 제품 실행 전이며 채택 `draft-vectors-r1`과 원 초안을 모두 남긴다.

- r0 source `1e7bca7c…`: Rust2024 예약어 `gen`으로 fmt parse FAIL. source·전체 진단을 보존했다.
- r1 source `b7d4700c…`, ELF `a041a2d8…`: 수정 후 compile PASS. static layout1과 fmt·engine
  all-target/all-feature Clippy PASS. 독립 실행 gate 수용 뒤 신규 필수12 exact selector를
  각각 실행해12 PASS다. 원 source/ELF/명령/결과/hash를 각각 보존했다.

새12개는 raw literal의 observer/producer/materializer 전체 일치, slot0/70와 최대 세대,
epoch/local/owner binding, before page precondition, 각 wire section 손상, 정확 budget,
native/redo/after page 결합, frame의 모든 proper prefix/trailing과 입력 불변을 포함한다.
기존 RollbackStart 및 하위 wrapper·무변경 시험은 반복하지 않았다. native I/O와 새 borrow
compiler probe는 순수 준비에 해당하지 않는다. 미실행을 PASS로 세지 않는다.

## 자원과 독립 검토

source-value component는 nested expected/observation/descriptor/home/view/header/result와
encoder/hash scratch를 포함해31,040≤65,536 bytes다. caller의 before/after page·frame loans는
35,808 bytes로 별도이며 합66,848을 component64KiB 안이라고 주장하지 않는다. 전체 live owner,
OS stack/RSS/OOM 또는 미래 operation114,688의 실제 충분성을 증명하는 값이 아니다.

보존8MiB의 원 문구에는 도구 산출물 구분이 빠져 있었다. 독립 검토와 함께 시험 vector·선택
로그/receipt·review 기록의 한도로 범위를 명시했다. source export·ELF·compiler/cache는 별도
도구 보존물이며 [색인](dev3-status-begin-20261023.json)에 실제 bytes를 기록한다. 전체 artifact
root가8MiB 이하였다고 주장하지 않는다. 원 D2와639조합의 순차 생성·재사용 의무는 유지한다.

`/root/pool_design_review`가 `open-code-review-delegate`의 파일 선정·규칙과 독립 의미 검토를
사용했다. frozen D2는 순수 준비 범위로 수용됐고 r1 실행 gate는 OCR13항목 중 reviewable3개,
제외된 관련6개를 검토하고 별도 DEV2 4개를 제외했다. 최종 D1은 staged21개(제품/fixture8·문서13)를 이 순수 준비 단위로 수용했다. OCR22항목 중
reviewable8개 전부·제외된 관련13개를 수동 검토했고, 별도 DEV2 파일1개와 공유3개의 unstaged
hunk를 제외했다. 제품/fixture8개는 r1과 byte 일치하며 원33개 receipt의 hash/bytes를 대조했다.

[정확 inventory](../../crates/engine/tests/fixtures/dev3-status-begin/manifest.json)의 S5 확대는
profile3×slot71×generation3=639조합으로 작성·NOT_RUN이다. 순차 vector 생성·재사용을 적용하며
중간12개도 최종 S5에 포함된다. 문서 자동 검사는 push 전용이므로 실행하지 않았고 push하지 않는다.
별도 DEV2 변경은 이 단위의 commit에서 제외한다.
