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

먼저 `history::row::table_source`의 leaf 선택을 canonical Catalog·현재 anchor에서
Job RowId로 찾도록 연결했다. pair[1]은 현재 leaf의 근거로 읽지 않는다. downstream
Row transition의 level0 제한은 유지하므로 multi-level 행 변경·복구 완료는 아니다.
[선택 로그·hash·검토](../../module-gate/evidence/dev3-08-row-selector/)를 보존한다.

| 실행 | 결과와 귀속 |
| --- | --- |
| row-selector-r0 | 시험 submodule이 scope3 macro보다 먼저 선언되어 compile FAIL, 실행0 |
| row-selector-r1 | 신규 자원1 PASS, 기능2 FAIL: h-501 Job의 실제 RowId7을2로 가정 |
| row-selector-r2 | 추가 caller census 수정 뒤 자원1 PASS, 기능2 FAIL: 실제4행을3행으로 가정 |
| row-selector-r3 | 수정 기능2와 변경 selector를 부르는 기존 replay5:7 PASS. 완료된 무변경 자원 시험은 반복하지 않음 |

새 source 계산은 semantic24,323B, 원 owner peak64,016B·metadata187,024B·whole252,560B로
기존 한도 안이다. reviewer가 찾은 Prune의 typed read와 탐색 중첩은 `row_group_terms`에
명시했고 실제 CheckedKey 크기를 썼다. 별도 D2/D1 ACCEPT·필수0, engine Clippy PASS다.
전체 wrapper·무변경 하위 codec 시험과 native I/O 시험은 실행하지 않았다.

후속 [SMO 제안](../contracts/dev3-tree-smo.md)은 nonempty INIT의 원 WAL→state seal→
selected base 증거 승계를 정리한다. scope4의 실제 효과·원 pool 설치·F05·fault inventory는 아직 미완료이며
이 문서나 위 selector 시험을 그 실행 증거로 세지 않는다.

<a id="wire-observer"></a>
### 순수 scope4 형식 관찰자

`978b7901` 이후 `semantic_row::structure`에 Row-tree RootSplit/ChildSplit의 불변 Core/Layout
관측을 구현했다. 기존 scope3 decoder는 그대로다. 정확84,744/84,640B와 native/redo 각5개,
새 Structural owner의 Finished·잔여0 descriptor, group LSN/epoch/profile, core의 root/parent
관계·물리 alias·scalar 비용, native/redo 일대일 대응을 검산한다.

[원 로그·source/ELF hash](../../module-gate/evidence/dev3-08-wire/)의 r0는 신규6개만 실행했다.
build1분55초·실행0.05초,6 PASS·기존2,208개 filtered다. source별 원 실행이며 무변경 시험은
반복하지 않았다. 독립 reviewer `/root/dev3_08_review`는 사전 D2와 구현·원 r0 결과 D1을
각각 ACCEPT·필수0으로 판정했다. raw XSG/XSM/XOD/native를 제품 관찰자와 별도로 조립했고,
일반 descriptor decoder가 수락하는 Finished+nonzero debt를5개 remaining행 각각에서 거절했다.

engine all-targets/all-features Clippy-r0는 `collapsible_match`와 `needless_range_loop`를
지적했다. 동일 조건의 match guard와 고정5개 offset의 enumerate로 고친 뒤 r1은1분33초에
PASS다. reviewer가 두 표현의 동등성을 별도로 ACCEPT했고 runtime6개는 반복하지 않았다.
원 r0 source/ELF와 최종 lint-only source hash를 구분한다. 문서 자동 검사는 push 전용이므로
이 편집·commit 단계에서는 실행하지 않았다.

fixture는 page1 allocator·opaque tree cells를 포함하는 WIRE_ONLY다. 실제 profile의 allocator
주소/세대·extent 소유, XBT1/Row grammar, canonical before와 두 PATCH digest, 실제 split
재계산·INIT 권한·live/replay adoption·F05는 검증하지 않는다. INIT만 page envelope와 실제
full image digest를 비교한다. PATCH commitment를 바꿔도 형식 관측이 수락하는 사례를 두어
후속 canonical 검증 의무를 숨기지 않았다. 전체08과 08-1 적립은 여전히0이다.

<a id="tree-audit"></a>
### 전체 root-reachable tree 관찰자

`ddfc7682` 이후 `tree_page::audit`에 caller workspace를 사용하는16-frame DFS를 추가했다.
각 page를 exact identity/family/level/arity와 상속 범위로 검산하고, 전체 visited의 물리 주소
중복(새 incarnation 포함)을 callback 전에 거절한다. 성공은 pre-order expectation 목록이며
오류 때 부분 workspace는 accepted tree가 아니다. 기존 selected-path 탐색·그 소비자의
source census는 변경하지 않았다.

[원 로그·hash](../../module-gate/evidence/dev3-08-audit/)의 r0 신규6개는6 PASS,
build1분40초·runtime0.02초·기존2,214개 filtered다. Row/Catalog/Index 전체10page,
Index2열 composite+RowId, off-path ancestor 범위/identity, 두 부모의 공유 child·ancestor
incarnation alias, capacity0..10·callback 오류·garbage tail, 다른file의 같은page_no,
unreachable page 한계를 확인했다. 별도 reviewer `/root/dev3_08_review`의 사전 D2와
구현·원 실행 D1은 ACCEPT·필수0이다.

engine all-targets/all-features Clippy-r0도1분20초에 PASS했다. 새 제품/시험 source는 첫
runtime 이후 바뀌지 않았으며 기존 검색·범용 codec·scope4 시험은 반복하지 않았다.

사전 검토에서 높이16의 전체 정상 tree는 최소65,535page라는 지적을 반영했다. 그 사례는
16단계 leftmost path 뒤 다음 sibling을 읽기 전 NoSpace라는 경계 시험이며 최대높이 전체
정상 인수가 아니다. metadata-only unary internal도 기존 codec대로 거절한다. allocator-owned
집합과 반환 목록을 실제 같은 cut에서 비교하는 orphan/소유 검증, Row/Undo 의미, 실제 SMO·
base adoption·F05는 아직 연결하지 않았다. 08 적립은0으로 유지한다.

[원 pool 수명 설계](../contracts/dev3-tree-smo.md#pool-lifetime)도 별도 D2 ACCEPT·필수0으로
구체화했다. 신규1/2개·전체target5개 claim, selected-DWB 증거, generation tombstone,
stage 전 취소 경계와 guard0 durable 대기, installed frontier와 후속 provenance replay를
구분한다. 실제 타입 크기·F05·fault inventory·효과 구현은 아직 미완료다. 이 문서 변경으로
제품 시험을 실행하지 않았으며 앞선 PASS를 새 source의 실행으로 바꾸지 않았다.

<a id="pool-claim-metadata"></a>
### 원 pool Structural claim metadata

`2719e791` 뒤 [metadata 계약](../contracts/dev3-tree-smo.md#pool-claim-metadata)에 따라
원 claim enum·검산·ordinary gate와 dynamic RowInternal 등록, 등록 보존 single finish를
추가했다. 독립 검토자의 사전 반례를 입력으로 사용했다. 실제 authority factory·claim 설치·
WAL/image/control/DPT 효과·durable 완료·복구는 아직 연결하지 않았다.

[원 로그·source/ELF hash](../../module-gate/evidence/dev3-08-claim/)를 보존한다.
변경 전 신규 진단은 실제 원 타입 크기를 측정했고, r0는 신규 시험의 OwnerRole import와
존재하지 않는 claim_load 호출 때문에 compile FAIL·실행0이었다. 실제 원 경로인
claim_retention_load로 고치고 독립 검토에서 발견한 rank의 중첩 carrier 계산 누락을
반영했다. r1은 신규8개만 실행해8 PASS(build1분32초·runtime0.01초·기존2,220개 filtered)다.
완료한 무변경 시험이나 전체 wrapper는 실행하지 않았다.

System/TxPair/Structural record는 각216B, identity224B, slot/Option232B,
registry13,616B로 기존 pool 크기를 유지했다. 신규 branch2,083B는 변경 없는 pair branch
2,211B보다128B 작다. 현재 source 식의 이 지배 관계로 common entry phases
`(80,3307,76)`이 그대로임을 확인하며, 과거 로그의 숫자 일치만을 근거로 삼지 않는다.
전체 SMO F05와 효과 인수는 여전히 미완료다.

engine all-targets/all-features Clippy-r0도1분22초에 PASS했다. r1 이후 제품·시험 source는
변경하지 않았다. 독립 reviewer가 실제 source와 원 로그·hash 및 계약 경계를 확인해
D2/D1 ACCEPT·필수0으로 판정했다([검토 기록](../../module-gate/evidence/dev3-08-claim/review.md)).

<a id="actual-undo-weights"></a>
### 현재 profile의 실제 Undo 가중치

`b29505e6` 뒤 [가중치 계약](../contracts/dev3-tree-smo.md#actual-undo-weights)에 따라
선택 leaf의 Row/Job과 실제 allocator-owned U를 결합하는 순수 관찰자를 추가했다.
기존 cells helper8개의 body는 그대로 두고 같은 semantic_row 안에서만 재사용한다.
current projection·nearest predecessor·typed before를 검산하고 현재 비용과 실제 복원
비용의 최대를 계산한다. canonical source 안정성·전체 census·설치 권한은 제공하지 않는다.

[원 로그·source/ELF hash](../../module-gate/evidence/dev3-08-weights/)를 보존한다.
기존 독립 D/U literal의 native Row/Job/U after만 입력으로 추출하고, 기대 비용은 literal
before/after Row cell 길이에서 별도로 계산했다. 기존 transition 시험은 실행하지 않았다.
r0 신규6개 중5 PASS·1 FAIL(build1분41초·runtime0.01초·기존2,228개 filtered)이다.
실패는 용량 경계 fixture의 canonical 값 overhead를1B 크게 계산해 제품 함수 호출 전에
발생했다. 이를 고치고 typed Undo 전용 반례·새 missing-H/LSN 사례를 보강한 r1은 변경3개만
실행해3 PASS(build35.83초·runtime0.01초·2,232개 filtered)다.

독립 검토에서 missing-H 반례가 앞선 unmatched Transaction 행에서 먼저 거절되는 점과
wrong-H로 적은 mutation이 실제 previous-version edge를 바꾼 점을 발견했다. missing-H는
앞선 row9를 제거하고 callback `[11]`을 확인하도록 고쳤으며 실제 H 주소 offset128의
mutant를 추가했다. r2는 이 변경2개만 실행해2 PASS(build25.30초·runtime0.01초·2,233개
filtered)다. 고유 시험7개이며 원 source별 FAIL/PASS를 최종 source의 전수 실행으로 바꾸지 않는다.

Clippy-r0의 nonminimal_bool 지적은 `!is_some_and(a == b)`를 `is_none_or(a != b)`로
바꿨다. reviewer가 동등성을 확인했고 무변경 완료 사례를 반복하지 않았다. 원 r0/r1/r2
source와 ELF hash를 구분한다. engine all-targets/all-features Clippy-r1은1분18초에 PASS다. 독립 reviewer `/root/dev3_08_review`는 실제 source·계약·
원 로그 및 보강 반례를 확인해 한정 D2/D1 ACCEPT·필수0으로 판정했다.

실제 current-profile 최대 행 비용은2,232B이고 leaf weighted 경계16,188/16,189를 시험했다.
이는 generic8,094B 행·overflow·own/repeated 변경의 인수가 아니다. schema의 Catalog 귀속,
전체 Root/terminal/ATT·Status·incoming census, off-leaf retained 행, allocator-owned tree
집합 비교, live/replay 및 실제 source census/F05는 후속 canonical owner에 남는다.

<a id="five-image-candidate"></a>
### 현재 tree에서 다섯 이미지 재계산

`6cc2004f` 뒤 [후보 계약](../contracts/dev3-tree-smo.md#five-image-candidate)에 따라
RootSplit/ChildSplit의 before 선택·실제 전체 tree/객체 소유 집합·Undo 가중치와 역방향
Retained Row 대응을 검산하고 다섯 after·core를 재계산한다. 원 page callback과 명시적
Structural bank 입력을 빌리며 C arena 배치를 복제하지 않는다. 전후 전체 tree와 ordered
Row bytes digest를 비교한다. 아직 WAL encoder/observer나 설치 권한은 아니다.

[원 로그·hash](../../module-gate/evidence/dev3-08-candidate/)의 r0는 compile FAIL13·runtime0이다.
시험 import, private child 호출, enum 이름과 descriptor 인자 borrow를 고쳤다. 실제 공개
child_expectation API를 사용했고 side encoder의 workspace 수명을 나누었다. r1은 신규9개만
실행해9 PASS(build1분48초·runtime0.04초·기존2,235개 filtered)다.

독립 검토에서 제안한 revision 최댓값의 Exhausted 분류와 previous Structural의 미래 epoch·
미래 cut·Finished 잔여 debt, 부정 WAL 비용·누락 descriptor를 반영했다. r2는 변경1·신규1만
실행해2 PASS(build36.68초·runtime0.02초·2,243개 filtered)다. 새 internal API의 leaf 입력과
refs 부족/정확 경계를 보강한 r3는 변경1개만 실행해1 PASS(build28.80초·runtime0.01초·
2,244개 filtered)다. 고유10개이며 기존 후보·wire·weights 시험은 반복하지 않았다.

Clippy-r0는 nonminimal_bool 한 건을 지적했다. `!is_some_and(eq)`를 `is_none_or(ne)`로
바꿨고 독립 reviewer가 동등성을 확인했다. 이전 실행을 최종 source의 전수 실행으로
바꾸지 않으며 r0 실패와 r1/r2/r3 source/ELF hash를 보존한다. Clippy-r1은1분19초에 PASS다.
독립 D1/D2는 한정 ACCEPT·필수0이다([검토 기록](../../module-gate/evidence/dev3-08-candidate/review.md)).

parent 공간 부족 시험은 unique child405개의 locally-valid page를 사용한다. 이는64-page
extent에 들어가는 정상 전체 tree가 아니며 그 정상 분기 도달을 주장하지 않는다. 현재
MAX_PAGE_SLOTS3,244를 사용한 실제 refs/weights 크기를 계약에 기록했다. 제안 F05 소계를
실제 계산으로 재사용하지 않으며 after의 불변 수명 결합·실제 source census·global closure의
일반화·live/replay/fault 인수는 후속 의무다. 일정08 적립은0이다.

<a id="scope4-producer"></a>
### scope4 생산과 정본 before 비교

`a5570a38` 뒤 [생산·비교 계약](../contracts/dev3-tree-smo.md#scope4-producer)을 구현했다.
각 공개 호출이 원 before에서 새 후보를 만든 뒤 private emitter가 그 after를 immutable하게
빌린다. group·descriptor·core·native와 canonical decoded redo 전체를 비교하고 outer·trailer는
scope4 decoder로 검산한다. 이전 Candidate를 받아 caller가 바꾼 after를 쓰는 API는 없다.

[원 로그·hash](../../module-gate/evidence/dev3-08-frame/)의 r0는 신규6개만 실행해6 PASS다.
build1분39초·runtime0.09초·기존2,245개 filtered다. 별도 raw section writer는 이미 검증한
candidate after/core를 입력으로 쓰며 제품 emitter·descriptor/redo encoder는 호출하지 않는다.
Root/Child leaf/internal 네 형식의 exact bytes와 after를 대조했다. Root/Child 각8종 변조는
checksum을 보정해 wire decode의 수락과 canonical observe의 거절을 모두 확인했다.

자체 점검에서 이전 epoch 사례가 old descriptor와 old before를 조합한 local 형식 증거에
머무는 점을 보강했다. 첫 Internal RootSplit의 실제 after5를 fixture에 반영하고 그 successor의
leaf13을 다음 epoch에서 ChildSplit한다. root와 parent가 다른 deeper child도 함께 덮는다.
r1은 이 변경1개만 실행해1 PASS(build32.77초·runtime0.03초·2,250개 filtered)다. 고유6개이며
무변경 완료 사례와 이전 suite는 반복하지 않았다. engine all-targets/all-features Clippy-r0는
1분18초에 PASS다. 독립 D1/D2는 한정 ACCEPT·필수0이다
([검토 기록](../../module-gate/evidence/dev3-08-frame/review.md)).

현재 fixture는 table/tree와 Job/U의 local 관계이며 전체 Root/Status/ATT와 선택된 역사까지
증명하는 입력은 아니다. pure observation 반환 뒤의 출력 변경은 후속 설치 경로가 별도로
결합해야 한다. stage/install·복구 수락·global closure·실제 source census/F05·native fault는
미완료이며 일정08 적립은0을 유지한다.

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
