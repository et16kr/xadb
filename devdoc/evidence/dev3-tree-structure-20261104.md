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

<a id="base-structural-claims"></a>
### Base4 Structural birth 관측

`1a4caa6d` 뒤 [Base4 birth 계약](../contracts/dev3-tree-smo.md#base-structural-claims)에 따라
certificate의 RowInternal level과 Structural INIT role5/6를 Profile3 한정으로 확장했다.
System owner의 atomic birth·generation/epoch·주소·revision·kind/level을 검산하며 같은
init LSN의 claims 전체를 대조한다. 현재 peer incarnation이 살아 있으면 대응 birth를
요구하고 퇴역·다른 incarnation 재사용이면 옛 peer의 현재 존재를 강제하지 않는다.
CREATE pair accessor와 기존 role1..4 규칙은 유지한다. role6는 생성 이력이며 현재 root
또는 현재 revision의 단언이 아니다. shared decoder와 group loop의 source bound도 추가한다.

[원 로그·hash](../../module-gate/evidence/dev3-08-base-claims/)의 r0는 신규8개를 실행해
4 PASS/4 FAIL(build1분55초·runtime0.01초·기존2,251개 filtered)이다. 네 실패는 internal
fixture에 metadata만 있고 separator가 없는 입력 오류였으며 독립 검토도 이를 지적했다.
독립 literal separator를 추가한 r1은 영향4개만 실행해3 PASS/1 FAIL(build26.29초·
runtime0.09초·2,255개 filtered)이다. 남은 실패는 기존 ACTIVE가 raw-layout literal이므로
그 Anchor의 semantic decode까지 유효하다고 가정한 시험 입력 오류다. 해당 시험에 실제
canonical anchor를 넣고 같은 birth의 다른 System kind 충돌 사례를 추가했다. r2는 변경1·
신규1을 실행해2 PASS(build28.83초·runtime0.04초·2,258개 filtered)다.

raw initial digest를 바꾸고 내부 checksum을 다시 만든 base는 layout이 수락한다. 원 fixture의
full closure 정상 baseline 없이 그 후 full decode 거절을 Structural의 거절 증거로 쓰지 않는다.
독립 검토 권고대로 그 부수 assertion을 제거하고 관측 한계만 검산하도록 범위를 좁혔다.
r3는 변경1개만 실행해1 PASS(build27.55초·runtime0.01초·2,259개 filtered)다. selected
CURRENT와 원 WAL의 역사 보증은 이 시험의 공급 범위가 아니다.

source의 별도 검토는 outer Result만으로 nested identity decoder의 array·constructor·
iterator·두 group loop/비교 carrier가 독립 검산되지 않는 점을 지적했다. 실제 overflow를
발견한 것은 아니다. 기존 Base4 source 시험의 독립 inventory에 해당 체인을 추가하고,
layout 및 certificate 상계와 개별 grammar/group 상계를 함께 비교한다.

r4는 수정된 기존 source 시험1개만 실행해1 PASS(build27.34초·runtime0.00초·2,259개
filtered)다. grammar1,934≤2,252, group1,315≤2,668, certificate1,515≤1,542B이며 전체
phase가 상계 이내다. Row peak는 inventory51,867+outer7,808=59,675B로 유지된다.
Row layout20,232B·Catalog layout19,592B의 증가는 해당 caller peak를 바꾸지 않는다.

Clippy-r0는 제품에서 쓰지 않는 StructuralOperation reexport를 지적했다. 이를 제거하고
두 시험은 같은 enum을 원 경로에서 import하도록 고쳤다. reviewer는 이 세 import만
역치환하면 r4 source SHA와 정확히 같음을 확인했다. runtime 본문은 같아 재실행하지 않고
final-source hash를 별도로 남겼다. Clippy-r1은 engine all-targets/all-features에서41.10초에
PASS다. 독립 D1/D2는 한정 ACCEPT·필수0이며 [검토 기록](../../module-gate/evidence/dev3-08-base-claims/review.md)에
신규 고유9개·수정된 기존 source1개의 실제 source별 결과를 구분했다.

현재 mint·System ledger/bank·전체 current tree closure·scope4 replay 수락·설치/F05는
연결하지 않았으며 일정08 적립은0이다. 원 실패와 source/ELF hash는 보존하고 성공한
무변경 시험을 반복하지 않는다.

<a id="structural-bank"></a>
### Finished split bank와 복구 인계

`ba0b9710` 뒤 [bank 계약](../contracts/dev3-tree-smo.md#structural-bank)을 연결했다.
공통 순수 validator는 Profile3의 Finished Structural descriptor 전체 필드·high-water·실제
cut/epoch와 Root/Child WAL 비용을 검산한다. 최신 생존 birth는 같은 owner 전체와 operation을
요구하고 이전 birth는 같은 epoch를 허용하되 최신 id/epoch를 넘지 못한다. Allocation INIT
선택에서 Structural을 분리한다. 원 RAM34 metadata도 admission-only·복구 remaining·pack에서
같은 observer로 검사하며, wire33→RAM34 seed와 반대 pack은 high-water를 그대로 보존한다.
seed의 byte 변환만 별도 helper로 분리했고 테스트용 VerifiedBase 생성자는 추가하지 않았다.

[원 로그·source/ELF hash](../../module-gate/evidence/dev3-08-bank/)의 r0는 새 테스트의
private SelectedControl import 때문에 compile FAIL(사례 미실행)이다. 원 타입의 공개 경로로
수정한 r1은 신규15개와 영향받은 기존 source/caller4개, 총19개를 선택했다. build1분41초·
runtime0.05초,15 PASS/4 FAIL·기존2,256개 filtered다. graph2개는 기존 CREATE leaf의 birth를
덮어써 원 pair 검증에서 먼저 거절된 시험 입력 오류다. 독립 검토도 같은 오류를 발견했다.
원 CREATE10/11을 보존하고 별도63의 leaf·allocator·certificate·claim을 함께 추가하도록 고쳤다.
같은 id에 서로 다른 epoch/generation인 owner의 거절도 해당 사례에 보강했다.

나머지2개는 recovery source 시험의 증거 환경값 누락으로 census 본문에 들어가지 못했다.
r2 build31.14초 뒤 실제 source/ELF/input SHA 환경에서 이4개만 실행해4 PASS(runtime0.05초,
기존2,271개 filtered)다. 앞선 성공15개는 반복하지 않았다. 실제 full-closure 정상 baseline과
raw layout·ledger 성공을 먼저 확인한 뒤, 아직 연결하지 않은 Structural tree의 명시적 거절을
검증했다. 독립 XOD와 XRB fixture는 제품 encoder를 쓰지 않으며 기존 정상 genesis/live에
bank·capture/XOI cut·checksum만 더했다. raw 관측은 원 split의 역사 증명이 아니다.

source 검산은 validation2,856≤3,376, raw10,239≤12,207, seed helper16,799+outer480≤25,439B다.
Base4 ledger Structural21,490≤34,565, 전체 peak59,675B다. actual memory/native recovery의
peak65,392/65,400, metadata104,152/104,160, whole169,688/169,696B이며, retained live의
peak64,016/64,024, metadata187,024/187,032, whole252,560/252,568B다. 원65,536/196,608/
262,144B 한도를 늘리지 않았다. 이는 해당 관측·인계의 source 상한이며 전체 live SMO F05는 아니다.

독립 D1/D2는 source-r2·원 로그·literal SHA를 대조해 한정 ACCEPT·필수0이다.
Clippy-r0의 `8 * size_of::<u64>()` manual_bits 지적은 바이트 배열 크기인
`size_of::<[u64;8]>()`로 동등하게 바꾼다. runtime body와 계산값은 같아 완료 시험을 반복하지 않는다.
Clippy 최종 결과와 독립 동등성 확인은 같은 [검토 기록](../../module-gate/evidence/dev3-08-bank/review.md)에 남긴다.

사용자의 현재 완료 요청은 **08-1 전체**다. 다음 작업은 전체 current tree closure와 실제
system owner·WAL·복구, 원 pool 설치·retirement·fault/F05다. 08-2–08-4는 별도 후속이다.
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


<a id="current-tree-closure"></a>
### 같은 Base4의 현재 Row tree 전체 검산

`1fa86020` 뒤 [현재 tree 계약](../contracts/dev3-tree-smo.md#current-tree-closure)을 연결했다.
Catalog ordinal만 받는 borrowed 관찰자가 같은 LayoutView의 schema·anchor·root를 선택한다.
전체 tree의 range/identity/incarnation/alias와 allocator 역소유를 검산하며 Structural birth의
object·anchor·revision·LSN도 결합한다. actor와 전체 graph는 현재 RowId 경로를 찾고 모든
leaf를 역검사한다. CREATE leaf hint는 생성 이력으로 보존한다. Retained에만 현재 leaf
LSN≥Job.last_step을 요구하고, DELETE/UPDATE D는 실제 typed Undo 증명 뒤 해당 leaf에 적립한다.

[원 source/ELF hash와 로그](../../module-gate/evidence/dev3-08-current-tree/)는 단위별 원 결과다.
새10개, 변경 기존 source census/bank 기대2개와 실제 memory/native caller2개가 각각의 source에서
PASS했다. 동일 코드의 완료 시험을 전체 suite로 반복하지 않았다. r0 source/bank PASS 후 제품
수정은 미사용 import3개 제거뿐이고 독립 역치환 SHA 대조로 동등성을 확인했다.

| 실행 | 실제 범위와 결과 | 실패 원인/다음 수정 |
| --- | --- | --- |
| r0 | build117초;5개 중2 PASS/3 FAIL | 독립 fixture의 frame/initial digest 위치가 반대여서 raw birth group에서 거절됨 |
| r1 | build40.15초;8개 중5 PASS/3 FAIL | 추가 MIXED fixture의 RowId 상한만 높이고 issuer 산술을 누락함 |
| r2 | 준비 실패;동일3개 FAIL | author IndentationError 뒤 shell이 잘못 계속 실행함. 새 입력 검증 증거가 아닌 무효 준비 시도이며 이후 set -e로 차단 |
| r3 | build22.35초;실패한 MIXED3개 PASS | issuer/완료/취소/Undo sequence를 함께 정합화;runtime0.12초 |
| r4 | build42.72초;새 상태 행렬1개 FAIL | Ready terminal의 Header active count를 누락함 |
| r5 | build36.40초;실패 행렬+새 level2 두 개 FAIL | compensated release_start가 Commit보다 늦음;internal fixture가 최소2child 조건을 위반함 |
| r6 | build20.00초;위2개 PASS | CLR59<Commit60<Release71, leaf70;internal당2child·이전 leaf birth75/76을 관측;runtime0.14초 |
| callers | 실제 ELF/source/input 결속2개 PASS | 변경된 중첩 source 상한을 실제 memory/native 타입까지 연결;I/O 수행 시험은 아님 |

정상 full closure는 leaf RootSplit/ChildSplit·높이2 internal RootSplit·이동 Retained INSERT와
DELETE/UPDATE를 포함한다. 같은 table의 서로 다른 leaf에 D1009/D1300을 배분하고 여유 공간이
정확히 D인 두 입력은 수락, D보다1B 적은 입력은 거절했다. schema/실제 Undo body 불일치,
잘못된 child/range/incarnation·orphan·전역 logical 중복·Retained stale LSN·anchor 역사 관계와
HW=u64::MAX에서 revision 비교를 확인했다. 정상 Reserved/Ready/compensated INSERT 부모도
분할 뒤 full closure의 census를 유지한다.

DELETE/UPDATE의 Reserved/ReleasingCommitted/ReleasingCompensated는 **actor-only** 원바이트
관측6개와 각각의 반대 존재/부재6개로 검증한다. Root census·Status·Undo·current tree를 실제
actor::observe로 검산하며 ATT/inventory 전체 closure 수락으로 확대하지 않는다. 원 hint11의
행40은 현재 leaf16에 있고, 각 정상 입력의 leaf LSN70은 Job.last_step71보다 낮다.
DELETE committed는 부재, UPDATE committed는 원 logical·Committed(q)·undoNone, compensated는
U before logical/stamp의 복원 행이다. raw split/current bytes는 selected WAL 역사나 권한이 아니다.

source 독립 inventory는 current tree23,817≤28,696B, actor41,748≤42,352, table 최대35,345≤45,152B다.
DFS16/visited64와 point search의 private carrier는 별도 정적 검토했고, caller census에서는 그
production helper를 호출한다(그 하위까지 독립 runtime 측정했다고 주장하지 않는다).
기존 inventory51,867B가 지배하여 Base4 peak59,675B는 유지한다. 실제 recovery memory/native
peak65,392/65,400, metadata104,152/104,160, whole169,688/169,696B와 retained live
peak64,016/64,024, metadata187,024/187,032, whole252,560/252,568B도 원 한도 안이다.
이는 전체 SMO F05 완료가 아니다. Clippy/최종 독립 인수는 같은 디렉터리의 review 기록을 따른다.

사용자의 완료 목표는 여전히08-1 전체다. scope4 canonical replay와 실제 INIT mint, 원 pool
claim/stage/install/durability 수명, empty-leaf unlink·unary repair·root shrink·retirement와
fault/F05는 후속 필수다. 08-2 cursor·08-3 overflow·08-4 반복/own write는 별도 일정이며,
이번 관측 단위에는 일정 적립을 하지 않는다.

<a id="split-source-bound"></a>
### 분할 후보와 frame의 임시 공간

`4d96672f` 뒤 [분할 source 계약](../contracts/dev3-tree-smo.md#split-source-bound)을 구현했다.
범용 MAX_PAGE_SLOTS 대신 현재 Row 최소 record168B+tree prefix/key12B+slot4B에서
leaf weight87을 계산하고, internal separator40B의404개에 metadata/삽입 여유를 더한
reference406을 사용한다. 전역 page 형식·지원 payload·원 recovery/F05 cap은 바꾸지 않았다.

독립 입력의 nullable85행은 분할 전후 bytes가 모두 같고, tombstone87개는 page에 들어가며
88개는 넘친다. generic tree에 들어가는 untyped88cell은 후보에서 거절되고 output은 보존된다.
부모403→404separator 삽입은 성공하고 기존404→405 거절도 유지된다. 이 밀도 입력은 local
candidate/형식 검증이며 정상 전체 actor history의 수락으로 확대하지 않는다.

원 [source·입력·ELF hash와 로그](../../module-gate/evidence/dev3-08-split-source/)를 보존한다.
r0는 fresh build117초 뒤 신규 density4/source1과 직접 영향받는 기존 후보/frame5, 총10개
PASS(0.11초)다. 첫 독립 검토에서 source 시험의 Undo wrapper/SHA와 parser/emitter 계산이
부족함을 지적했다. canonical432·material/outline wrapper, codec과 동시 생존하는 SHA1536을
명시한 뒤 r1은 변경/new source시험2개만 PASS(build31.47초)했다. 완료된 기능9개나
wire/weights/bank/Base4 suite는 반복하지 않았다. Clippy r0의 동치 const 표현과 test-only
import 지적2개도 원 FAIL을 보존했다. 최종 lint·독립 검토는 같은 디렉터리 review에 결속한다.

| source 단계 | 독립 inventory / 제품 상한(B) |
| --- | --- |
| candidate prepare retained values | 26,312 / 31,776 |
| after-audit와 nested traversal/weights | 15,749 / 19,436 |
| typed weights와 Undo wrappers | 13,629 / 15,820 |
| parent 편집 | 10,877 / 16,793 |
| Catalog/anchor 읽기 | 6,604 / 9,148 |
| frame parser | 11,999 / 17,495 |

candidate peak는51,212B, frame은 retained LayoutView 등을 포함해57,420B다. codec helper의
검증된 typed bound와 private traversal carrier는 재사용하며, 하위 코드 전체를 이번에
독립 측정했다고 주장하지 않는다. 이 수치는 source value inventory로 compiler stack/RSS가
아니며 before/after·WAL loan 및 실제 caller capture는 다음 canonical/live 연결에서 더한다.
scope4 replay·INIT mint·원 pool SMO와 retirement/fault/F05는 계속08-1의 필수 잔여다.

<a id="scope4-replay"></a>
### 원 arena의 canonical scope4 replay와 INIT 기록 발급

`ea8b87b6` 뒤 [scope4 replay 계약](../contracts/dev3-tree-smo.md#scope4-replay)을 구현했다.
원 RAM34 전체를 먼저 관측하고 같은 arena·Job cells에서 다섯 after를 재계산한다. canonical
frame 비교 뒤 실제 frame/최초 image/core digest와 owner·LSN/epoch·역사 관계를 담은352B INIT를
만든다. RootSplit의 두 기록·다섯 target을 모두 검산한 뒤 arena·claims·Finished bank를 갱신한다.
실패 시 원 상태와 cursor는 유지하며 after는 사용 불가 workspace다. analysis의72B target은
kind/level을 함께 보존하고 같은 incarnation의 home/Pending과 비교한다. 최종 flush도 그 level을 쓴다.

원 [source·ELF·입력 hash와 실행 로그](../../module-gate/evidence/dev3-08-scope4-replay/)를
보존한다. 관련 고유30개(신규12, 직접 영향 기존18)와 실제 memory/native caller2개가 각
수정 source에서 통과했다. r1에서 완료된 무변경 tombstone 상한 시험1개를 잘못 재실행했다.
그 함수는 바뀐 split/allocator를 호출하지 않으므로 필요 검증30개에 넣지 않으며, 이 실행은
AGENTS의 무변경 재실행 제한을 지키지 못한 사실로 남긴다. 이후 선택에서 제외했다.

| 실행 | 범위·결과 | 실패와 후속 |
| --- | --- | --- |
| r0 | compile FAIL, 실행0 | 새 source inventory의 PageRef import 누락 |
| r1 | 14 PASS | 신규 replay/level6, 영향 기존7, 위 무변경1. refs88·allocator helper 적용 |
| callers-r1 | 실제 타입2 FAIL | recovery peak84,109/84,117, retained metadata205,741/205,749·whole271,277/271,285로 원 cap 초과 |
| r2 | 19 PASS | compact audit2·공통 walk6, phase 분리와 sparse allocator에 직접 영향받는 후보/replay/source11 |
| callers-r2 | 실제 타입2 PASS | 아래 원 cap3종을 유지; I/O 수행 시험은 아님 |
| r3 | 7개 중5 PASS/2 FAIL | 새 reached 조회가 기존 ‘parent 다섯 번째 read’ 오류 cut을 앞당김; raw Base가 역사 digest 변조를 거절한다는 시험 가정 오류 |
| r4 | 3개 중2 PASS/1 FAIL | Catalog8 두 번째 조회로 build 뒤 검산 실패 확인, 실제 scope1 Reserve guard PASS; zero digest도 raw 관측에서는 허용됨 |
| r5 | 실패한 full Base1 PASS | 실제 문법인 System source tag1을2로 바꾸는 반례로 정정; 제품 의미는 바꾸지 않음 |
| r6 | 일반 parent 경계2 PASS | const-generic emitter로 옮긴 뒤 미실행이던406 refs의403→404 삽입/404→405 거절; r5 source·ELF 그대로 사용 |

분할 전 상태는 독립 byte author의 full Base4 관측이다. 새 internal Root/Child before도
제품 encoder를 쓰지 않았고 raw 부모의 SHA를 보존했다. 정상 Root/Child leaf/internal에서
행 bytes·Job cells·기존 claims를 보존하며 새 INIT352B 전부를 독립 구성값과 비교한다.
실제 replay 출력으로 만든 raw after Base4도 네 경우 모두 전체 decode를 통과하고, 새 claim의
source tag·미래 LSN·revision·level 변조는 재봉인 뒤 거절한다. raw initial digest는 zero도
포함한 역사 관측 값이며 진위는 selected-source seal이 소유한다. r3/r4의 잘못된 거절 기대를
원 실패와 함께 보존하고 검증 범위를 확대하지 않는다.
독립 검토자도 처음 zero digest를 문법 반례로 잘못 수용했으나, 실제 codec에 nonzero 조건이
없음을 재확인하고 그 판단을 정정했다. 담당과 검토자의 오류 모두 제품 규칙 완화로 처리하지 않았다.

boundary·weight·before digest 변조는 wire decode 수락을 먼저 확인한 뒤 fresh canonical 비교의
거절을 검사한다. 두 번째 INIT image/claim 충돌, 빈 bank garbage·잘못된 HW·non-STABLE,
scope/cut/epoch·RootReserved도 원 상태 보존으로 확인했다. Allocation Reserved는 high-water0의
genesis에서 실제 scope1 Reserve를 관측해 만들었다. 다른 cut의 유효 split을 semantic 비교 전에
정확한 Invariant로 막는 guard 시험이며 Reserved 해소·재시도나 selected history 증거는 아니다.
그 외 raw before 시험의 allocation cursor는 non-Reserved stub이다(원 before의 high-water5와
다름). scope4가 그 cursor의 Reserved 여부만 읽는 범위이며 실제 Base seed/후속 scope1 인계는
효과 인수에서 별도로 확인한다. VerifiedBase나 dirty INIT 권한 factory를 추가하지 않았다.

원 cap을 지키기 위해 성공한 first-extent audit의64B preorder를 사용한다. 일반 audit와 같은
walk의 range/identity/level 검산을 공유하고 page0/63 수락,64·다른 file·incarnation alias·범위
거절과 callback 경계를 검증했다. ordinal의 full identity는 동일 allocator와 원 image에서
재관측한다. generic parent406은 유지하고 검증된 tree의 split/parent는88 refs를 쓴다.
private build→verify_after→public Candidate 순서를 지키며 allocator는 cell6208 복사와 선택
entry1/2개 변경으로 전체 entries 배열을 없앴다. 서로 끝난 phase의 값을 중첩해 세지 않는다.

| source 단계 | 독립 inventory / 제품 상한(B) |
| --- | --- |
| private build 잔존 값 | 4,912 / 4,968 |
| before prefix·public wrapper·after prefix | 1,152/1,216 · 962/964 · 856/936 |
| tree audit·typed weights | 15,749/18,892 · 13,629/15,820 |
| bounded parent·allocator emission | 5,789/11,705 · 10,538/12,426 |
| frame outer·candidate peak·frame peak | 2,112/2,176 · 20,940 · 23,116 |
| replay compare·claim finishing | 25,228/25,924 · 6,442/10,170 |

compact wrapper의 추가 borrow를120B로 명시한 r3에서도 지배 branch와 위 candidate/frame
값은 같다. actual caller 원 실행은 r2에 귀속하며 후기 source 전체 재실행으로 바꾸지 않는다.
recovery memory/native peak65,392/65,400, metadata104,152/104,160, whole169,688/169,696B와
retained live64,016/64,024·187,024/187,032·252,560/252,568B는 원65,536/196,608/262,144B
한도를 만족한다. 이는 source-value 검산이며 compiler stack/RSS나 실제 SMO F05 인수가 아니다.
Clippy와 최종 독립 D1/D2는 같은 증거 디렉터리의 review 기록에 결속한다.

08-1 전체 완료 목표는 계속된다. 원 writer/pool의 claim·stage·install·durability, 선택된 WAL의
실제 복구·CURRENT publication·native fault/reopen, empty-leaf unlink·unary repair·root shrink·
retirement와 전체 F05는 필수 잔여다. 일정08 적립0·누적38.37%는 유지한다.
