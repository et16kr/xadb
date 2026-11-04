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
