# DEV3 일정08 첫 트리 구조 후보 구현·선택 검증

2026-11-04 · **순수 후보 공급 LOCAL_VERIFIED / 독립 D2/D1 ACCEPT / 실제 SMO·08 미완료**

사용자는 일정08 전체를 끝까지 진행하도록 요청했다. 기준은 UPDATE PR #49 병합
`23976330`, 작업 branch는 `codex/dev3-08-tree-generalization`이다. 작업·계약·통합은
`/root`, 독립 검토는 `/root/dev3_08_review`다. 이 기록은 첫 부분 공급의 결과이며
[일정](../plans/dev3-schedule-baseline.md#items)의 08-1 적립 조건은 아직 충족하지 않았다.

## 구현과 경계

[소유 계약](../contracts/dev3-tree-format.md#structural-candidates)에 따라 기존 XBT1과
byte-balanced splitter를 재사용했다. internal separator 삽입·child 제거, root/unary 축소의
후속 작업을 나타내는 Collapse, leaf/internal 양쪽 이미지와 새 root 표현을 제공한다.
기존 leaf 후보 편집에서도 동일한 split 이미지 구현을 부른다. heap·별도 pool·writer·새
wire 형식은 추가하지 않았다. 모든 입력은 untrusted 후보이며 WAL/install 권한이 아니다.

순수 후보만으로 실제 tree split/행 변경/rollback/복구를 완료로 표시하지 않는다.
특히 user edit가 포함된 leaf 후보를 redo-only system 그룹으로 설치해서는 안 된다.

## 선택 실행

로그는 [원 증거 디렉터리](../../module-gate/evidence/dev3-08-structure/)에 보존한다.
Rust 1.95.0·고정 Cargo.lock·offline dependency·4 build jobs를 사용했다. 대상은
`cargo test -p xadb-engine --lib --locked --offline`의 새 structural cases뿐이다.
각 선택 실행의 build 포함 시간 한도는180초다. 기존 전체 wrapper·무변경 하위 시험은 실행하지 않았다.

| 실행 | 범위 | 결과 |
| --- | --- | --- |
| r0 | 새7개 compile | E0502: 오류 보존 시험에서 scratch가 metadata를 대여한 상태로 먼저 metadata를 읽음. 실행0 |
| r1 | 최초 새7개 | 7 PASS. assertion 순서를 scratch→metadata로 고침. source·ELF hash 보존 |
| r2 | 보강한 overflowing_parent 사례1 + 검토 추가2 | 3 PASS. 무변경 기존6개는 미실행. source·ELF hash 보존 |

고유9개 완료다. 기대 page는 독립 XDP1/XBT1 literal 조립이며 source나 기대값 생성에
product tree/page encoder를 사용하지 않는다. 실제 후보의 양쪽 이미지·pivot·child 순서,
level15 경계, exact child/incarnation·alias, 손상·잘린 cell, scratch/output 실패 보존을 확인했다.
각 원 결과는 해당 source의 증거이며 후기 source 전체9개 재실행으로 바꾸지 않는다.

Clippy는 engine의 all-targets/all-features를 정적으로 검사했다. 최초 `manual_div_ceil`
지적은 시험의 `(count+1)/2`를 동등한 `count.div_ceil(2)`로 바꿔 해소했고 재검사는 PASS다.
이 lint-only 변경으로 완료 runtime 시험을 반복하지 않았으며 원 r1/r2 source hash를 유지한다.

## 검토

[독립 검토 기록](../../module-gate/evidence/dev3-08-structure/review.md)을 따른다.
첫 검토의 non-root Collapse 지적1건과 시험 권고4건을 반영했다. 재검토 ACCEPT·필수0이다.
root의 높이 축소와 달리 non-root는 parent→child level−1을 지키는 재구성이 필요하다.
문서 자동 검사는 push 전용이며 편집 단계에서는 실행하지 않았다.

## 이어지는 실제 연결

### 예약 가중치와 immutable 탐색의 후속 공급

첫 checkpoint `b89a7084` 이후 [가중 분할](../contracts/dev3-tree-format.md#weighted-split-candidate)과
[선택 경로 탐색](../contracts/dev3-tree-format.md#immutable-tree-search)을 추가했다.
실행 로그·source/ELF hash와 별도 검토는
[후속 증거](../../module-gate/evidence/dev3-08-search/)에 보존한다.

| 실행 | 변경 범위·결과 |
| --- | --- |
| weighted-r0 | 신규3개:2 PASS, exhaustive oracle1 FAIL. 유효 입력에 항상 단일 boundary가 있다는 oracle 가정이 틀렸고 제품은 NotSplittable을 안전하게 반환 |
| weighted-r1 | 수정 oracle와 검토 반례 신규1개:2 PASS. 원 성공2개는 반복하지 않음. 고유4개 완료 |
| search-r0 | 새 bounded path 탐색4개:4 PASS |
| search-r1 | rightmost 조상 상한과 callback 오류 전파를 보강한2개:2 PASS. 무변경2개는 반복하지 않음. 고유4개 완료 |

가중치는 수치 입력일 뿐이며 아직 Undo/owner 그래프에서 유도하지 않는다. pending 행의
bytes·version을 구조-only 후보에 설치하지 않는다. 단일 split으로 수용하지 못하면 먼저
구조-only split 후 재탐색해야 한다는 반례를 고정했다. 탐색은 3 family·최대16 read의
immutable 경로만 검산하며, live latch/pin이나 off-path audit를 제공하지 않는다.

독립 reviewer `/root/dev3_08_review`가 계약·source와 위 원 로그를 직접 읽고 두 공급 모두
D2/D1 ACCEPT·필수0으로 판정했다. weighted 최종 assertion의 상수8094를 동일한 request
field에서 읽도록 바꾼 것은 lint-only이며 동등성 재검토를 받았다. 원 runtime 증거를
후기 source의 재실행으로 바꾸지 않았다. 각 실행 한도180초·build4 jobs였으며 전체 wrapper와
무변경 기존 시험은 실행하지 않았다.

후속 engine all-targets/all-features Clippy는 최초 literal `Some`의 `unwrap()` lint1건을
지적했다. request를 plain struct로 두고 동일한 두 호출에서 `Some(request)`로 감싸도록
고쳤으며 재검사는 PASS다. 이 lint-only 수정으로 runtime 시험을 반복하지 않았다.

### 다음 제품 연결

08 전체의 승인 범위는 유지한다. 다음 작업은 08-1의 실제 system owner·WAL·복구다.
독립 사전 검토에서 새 사용자 판단이 필요한 제품 방향 충돌은 발견하지 않았다.

1. 원 writer/pool에서 structural group·새 page 예약/INIT·anchor revision·retirement를 연결한다.
   기존 단일 frame 한도 안에 완결 구조를 만드는 top-down 사전 split을 우선 검토한다.
   큰 cascade를 한 frame에 묶는 대안은 실제 target·scratch·WAL 상한을 먼저 확인해야 한다.
2. `semantic_row/leaf.rs`, `row_preflight.rs`, `row_rollback_select.rs`의 level0·고정 leaf를
   현재 anchor→RowId 탐색으로 바꾼다. Job pair[1]은 이미 이동한 행의 정본 위치가 될 수 없다.
3. `plan4_recovery_base/row/graph.rs`와 replay의 CREATE 최초 provenance와 현재 tree를
   구분한다. 도달성·소유·고아 page·rollback 공간 예약을 검산하고 실제 fault/reopen을 수행한다.
4. 08-2는 SMO 전후 cursor/locator, 08-3은 old/new row overflow와 별도 Undo chunks 소유,
   08-4는 RowId당 여러 Undo root·own/repeated write·참조 multiset·CLR chain이다.

새 opcode·F05·fault inventory는 효과 구현 전에 동결한다. 일반 extent 성장은 일정12와
구분하되 현재 extent의 Capacity 거절을 정확히 유지한다. 이번 순수 후보 공급에는
적립하지 않아 현재 누적38.37%와 적립33/86을 유지한다. 이후 각 일정 항목의
완료·검토·병합 조건을 채울 때 해당 배정분을 적립한다. main 병합은 AGENTS의 기존 사용자 지시에 따라
사람이 수행한다.
