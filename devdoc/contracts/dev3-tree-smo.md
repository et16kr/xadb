# DEV3 tree SMO — 완결 structural group 연결 설계

2026-11-04 · **D3-TREE-SMO-001 / scope4 wire·owner·provenance 설계 D2 ACCEPT / 구현·F02·F05·fault 인수 미완료**.

사용자의 일정08 전체 구현 요청과 기존 D2 위임에 따른 내부 설계다. 계약·구현 담당은
`/root`, 독립 검토자는 `/root/dev3_08_review`다. 비교 대안과 권한 경로의 정적 사전 검토는
받았고 구체 wire·owner·provenance 범위는 필수 지적 해소 뒤 D2 수용됐다. live admission은 아직
수용되지 않았다. 효과 구현은
해당 F01/F02/F05와 독립 검토를 닫은 뒤 연결한다. [작업 정본](dev3-task-06-tree-rows.md)의
요구를 대체하지 않으며 [일정08](../plans/dev3-schedule-baseline.md#items)을 완료로 적립하지 않는다.

## 결정과 범위

root split과 parent에 공간이 있는 child split을 각각 하나의 redo-only system 그룹으로
완결한다. top-down 준비가 내려가기 전에 parent의 separator 공간을 확보하고, 완료 그룹마다
현재 anchor에서 다시 찾는다. pending 사용자 변경을 포함하지 않고 기존 행 bytes/version을 그대로 재배치한다.
사용자 변경이 나중에 거절되거나 abort되어도 완료 SMO와 사용한 WAL은 유지한다.

기존 원 writer·SharedPool·canonical replay·ledger를 사용한다. 별도 tree 저장소나 cache,
무검증 INIT token, candidate header가 스스로 정한 identity는 허용하지 않는다. 최초 live
소비자는 Row tree다. 공통 후보는 Row/Index/Catalog family를 지원하지만 index/catalog의
실제 객체 의미·행 원자성은 일정09/10에서 연결한다.

reserve→INIT 두 그룹은 현재 count-one allocation owner를 일부 재사용할 수 있으나,
root의 두 신규 page와 nonempty INIT provenance는 여전히 확장해야 한다. crash 후 미연결
page의 inventory·취소·미래 budget까지 생긴다. 한 완결 그룹은 그 영속 중간 상태를 없애는
대신 원 pool에 신규1~2개와 기존 변경 page를 함께 검산하는 sealed claim이 필요하다.
이번 선택은 후자의 권한 경로를 구현·검증한다는 조건부다.

빈 leaf unlink·non-root unary 재구성·retirement는 별도 후속 opcode다. 아래 Finished/잔여0
규칙을 그 opcode에 자동 적용하지 않는다. root 축소 이외의 unary를 level이 다른 grandparent에
직접 연결하지 않는다. 이 후속 범위까지 닫기 전 08-1은 미완료다.

## 구조 전이

RootSplit은 allocator·stable anchor·기존 root(left)·신규 right·신규 root의5 page를 변경한다.
ChildSplit은 allocator·stable anchor·left·신규 right·기존 parent의5 page다. 다섯 identity는
서로 물리 주소가 다르고, 신규 page는 canonical allocator의 선택 가능한 Free 중 낮은 ordinal부터
선택하여 incarnation을1 증가시킨다. 현재 첫 extent의 한계는 유지하며 부족하면 effect 전 Capacity다.

모든 선택은 accepted canonical before에서 검산한다. Catalog의 exact object/generation/schema,
anchor identity·revision·root, source의 family/level/arity·전체 tree 도달성, child의 실제 parent와
separator 위치를 확인한다. RootSplit은 source가 현재 root이며 level<15여야 한다.
ChildSplit은 parent가 source보다 정확히1 level 높고 separator 삽입 후에도 한 page에 들어가야 한다.
source·new right·parent/new root의 모든 row/version 또는 child를 정확히 보존한다.

leaf는 [예약 가중 분할](dev3-tree-format.md#weighted-split-candidate)을 사용한다. 현재 cell과
실제 도달 가능한 rollback before 상태에서 계산한 최대 비용을 RowId별로 분배한다. Job 숫자만을
임의 weight로 신뢰하지 않는다. 처음에는 pending 없이 구조만 분할하고 다시 admission한다.
한 split만으로 pending 행 공간을 확보한다는 보장은 없다. internal은 기존 byte-balanced pivot을
승격한다. parent 공간 부족이면 parent부터 분할한 후 재탐색하며 잘린 cascade를 공개하지 않는다.

anchor identity는 그대로이고 revision은 정확히1 증가한다. ChildSplit은 root ref/level을 유지하며
RootSplit은 신규 root와 level+1을 기록한다. 다섯 after(allocator 포함)의 pageLSN은 complete group LSN이다.
logical row identity, stamp, UndoRef 및 row/Undo/overflow 참조 multiset은 구조 이동으로 바꾸지 않는다.

scope4는 allocator Reserved 또는 어느 scope의 RootReserved Job도 있는 동안 거절한다.
RowReserve 전에 SMO를 완료하며 기존 Retained roots는 허용하되 그 rollback 공간을 보존한다.
revision/LSN/incarnation 산술 한계와 모든 quota는 첫 effect 전에 검산한다.

## 제안 F01: scope4 frame

기존 physical semantic tag2, outer System, XSG1 version2·scope4를 사용한다.
opcode1=RootSplit,2=ChildSplit이다. outer160·group192·descriptor1536·core256·native640·redo·trailer32다.
group header의 기존 scope1/2 section layout을 재사용하되 native5·redo5, edge/count/item은0이다.
기존 scope의 decoder를 느슨하게 만들지 않고 scope4를 정확히 dispatch한다.

outer는 `txn=None,previous_txn_lsn=0`이다. group[16..24]는 complete LSN,
group[24..32]의 owner-chain previous는0,group[32..88]은 **새 Structural owner**,
group[88..120]의 prior owner descriptor digest는zero다. 매 그룹이 새 owner의 첫 그룹이므로
이전 bank descriptor를 기존 serializer의 `before`로 넘기지 않는다. 교체할 bank slot의 이전
high-water/digest는 아래 core176..184/224..256에서만 검산한다.

native는 물리 target 순서의128B 레코드5개다. 기존 page는 before full digest/LSN과 after full digest,
신규 page는 before 없음·INIT flag를 기록한다. before/after page를 native에 중복 저장하지 않는다.

| native byte | 의미 |
| --- | --- |
| 0 | role:0 allocator,1 anchor,2 left,3 right,4 parent/newroot |
| 1 | INIT flag:0/1 |
| 2..4 | redo ordinal u16 |
| 4..6 | PageKind u8,level u8 |
| 6..8 | zero |
| 8..32 | PageRef |
| 32..48 | owner ObjectId·generation 각u64 |
| 48..56 | before LSN, INIT이면0 |
| 56..88 | before full digest, INIT이면zero |
| 88..120 | after full digest |
| 120..128 | zero |

core256은 다음과 같이 제안한다. 모든 숫자는 기존 형식의 little-endian이다.

| core byte | 의미 |
| --- | --- |
| 0..8 | XSM1, version1 u16, opcode u16 |
| 8..32 | ObjectHandle의 object·generation·schema epoch |
| 32..56 | stable anchor |
| 56..80 | source/left |
| 80..104 | 신규 right |
| 104..128 | parent 또는 신규 root |
| 128..152 | before root ref |
| 152..160 | before anchor revision |
| 160..164 | source kind·source level·arity·before root level 각u8 |
| 164..168 | zero |
| 168..176 | 재계산한 boundary/pivot u64 |
| 176..184 | 이전 structural high-water |
| 184..200 | left/right 예약 포함 비용 각u64; internal은 실제 비용 |
| 200..216 | left/right 실제 entry 비용 각u64 |
| 216..224 | zero |
| 224..256 | 교체 전 structural descriptor full digest; 최초이면zero |

기존 page의 redo는 기존 `Patch(offset=72,length=16280)`로 header의 불변 identity를 유지하고
공통 redo 적용기가 pageLSN/checksum을 재생성한다. 신규 page는 full INIT16,384B다.
redo header64B를 포함하면 RootSplit은3×16,344+2×16,448=81,928B,
ChildSplit은4×16,344+16,448=81,824B다. 전체는 각각84,744B·84,640B로 기존98,304B frame에 든다.
이 산술은 native/group serializer와 literal에서 다시 검증하며 그 자체가 live F05가 아니다.

## System owner·회계

owner는 StructuralChange, id=complete group LSN, generation=원 writer epoch다.
새 id의 첫이자 마지막 그룹으로 chain birth=apply=id,previous=0,groups=1이다.
progress Finished·sequence1·cursor=limit=1, remaining5행 모두0, inventory items/growth0와
empty digest, inventory.change_lsn=apply, consumed_wal=실제 frame bytes, consumed_control=0,
minimum_wal_lsn=0,last_mutation_sequence=0을 정확히 검산한다. Finished만 보고 잔여0이라 추정하지 않는다.

live bank의 Structural slot은34로 제안한다(Aging32,Allocation33 유지). base wire는
Allocation32,Structural33,Aging34를 유지한다. seed/pack 양방향 변환과 header structural
high-water를 명시적으로 연결한다. 이전 structural descriptor는 완료·잔여0인 때만 교체하고,
core의 이전 digest/high-water를 canonical bank와 대조한다. 이미 사용한 WAL은 실제 prefix 길이로
계속 차지하며 새 owner id를 만드는 것으로 물리 WAL charge를 되감지 않는다.

새 page의 현재 owner는 tree object다. Structural owner는 INIT의 역사적 출처이며 완료 후 page의
회수 권한이 아니다. retention/dirty owner의 WAL 보호와 selected base 인계가 끝나기 전 physical
INIT 기록을 잃지 않는다. unlink에서 나오는 retired page는 별도 잔여 owner가 필요하다.

## nonempty INIT와 base의 역사 증명

기존 empty System 및 CREATE INIT role1..4의 recipe 재구성은 유지한다. 새 role5=split right,
role6=new root를 제안한다.352B claim의 공통 page/owner/init LSN/epoch/initial digest/frame digest는
유지하고, owner class는 System, source는 위 StructuralChange다. reserve LSN 칸은 atomic birth라
init LSN과 같다. role1..4의 reserve<init 규칙을 넓히지 않는다.

새 role는 raw[3]에 실제 level을 넣고 object handle184..208, anchor208..232,
source left232..256, 같은 그룹 peer ref256..280(ChildSplit은zero), opcode280..288,
before/after anchor revision288..304, core digest304..336,zero336..352를 사용한다.
peer/source는 역사 정보다. 적법한 퇴역 후 해당 PageRef가 계속 현재 할당되어 있다고 요구하지 않는다.
살아 있는 같은 그룹 claim들이 있으면 frame/epoch/object/anchor/birth 정보를 서로 검산한다.

새 claim은 **accepted complete scope4를 canonical before에서 재계산하고 exact INIT redo를 비교한
뒤에만** 만든다. candidate가 제시한 임의 initial/frame digest를 복사하지 않는다. 기존 claim은 그
incarnation의 적법한 퇴역까지 byte-for-byte 보존하며 CREATE origin을 Structural origin으로 바꾸지 않는다.

현재 base4의 역사 보증은 다음 귀납 경로다.

1. `plan4_recovery/analysis.rs::preinspect`가 selected WAL의 complete frame만 검산하고,
   소비한 prefix hash를 동일 handle 재읽기와 대조한다.
2. `history/catalog.rs::state_digest`는 provenance 전체를 포함한다. `pack_capture`가 이를
   AnalysisSeal과 대조하고 `capture_base`가 canonical claims를 그대로 빌린다.
3. epoch base write/readback 및 CURRENT의 full digest 공개가 끝난 뒤 old WAL을 퇴역한다.
4. `select.rs::try_closure`가 CURRENT의 full digest/generation/LSN을 검산한 selected bytes만
   VerifiedPlan4Base로 인계한다. BaseOnly와 다음 seed는 그 인증된 predecessor claims를 승계한다.

legacy `check_init_provenance`는 현재 base4 production 경로가 아니다. `prove_capture`는 flush할
현재 page를 canonical replay와 맞추는 경로이며 새로운 base 역사 증명의 이름으로 쓰지 않는다.

raw base decode는 새 role의 initial_digest만 바꾸고 내부 checksum을 다시 만든 입력을 형식상
수락할 수 있다. raw 관측은 historical INIT가 실제 발생했음을 증명하지 않는다. 실제 runtime은
기존 CURRENT digest와 달라진 base를 거절해야 한다. CURRENT까지 새 digest로 꾸민 독립 파일셋의
원 INIT를 다시 증명한다는 보장은 기존 selected-predecessor 신뢰 모델에도 없다.

## 원 pool과 전체 tree 검산의 연결 의무

원 SharedPool 안에 기존 변경 target과 신규1~2개를 함께 검산하는 sealed claim을 추가한다.
관측된 Free entry/TargetRequest만으로 INIT 권한을 만들지 않는다. 전체 target·quota·DPT·WAL 준비,
version/tree gate와 정렬된 latch 재검산, complete stage, requirement 보존, page/control/history/DPT
설치 순서를 유지한다. 첫 효과 뒤 부분 설치나 불명 I/O는 기존 fatal fence로 처리한다.

base4는 현재 anchor에서 전체 internal/leaf의 exact identity·level·범위·한 부모·allocator 소유를
검산하고 미도달 allocated tree page를 거절해야 한다. selected-path helper만으로 이를 대신하지 않는다.
CREATE pair[1]은 생성 이력이며 현재 root/leaf가 아니다. certificate의 level과 tree kind,
System claim의 kind별 high-water·동일 그룹 두 INIT, incarnation 상한 산술도 함께 확장한다.

최소 변경 지점은 control/seed/pack mapping, `history/catalog::system_initial_claims`,
base `image_records`·`catalog/graph/ledger`·`catalog/graph/relations`·`row/graph`,
`recovery::apply_frame` scope dispatch, 원 dirty INIT claim/install, Row forward/CLR/Prune의
현재 leaf 선택이다. 세부 source-value 계산은 실제 type/call lifetime을 기준으로 추가한다.

## F05와 검증 동결 잔여

frame의 산술만으로 admission하지 않는다.425,984B loan 안에서 before/after·native/descriptor·
reference scratch·weights·parent cell·live snapshot과 canonical replay loan의 동시 수명을 분리해
계산한다. 다섯 after81,920B, 네 before65,536B, 최대 frame84,744B, 후보refs16,208B,
weights8,088B,metadata32B,최대 root cell1,060B만 합해도257,588B다. 실제 primitive의
local/return carriers·nested decoder·typed read·sealed INIT와 source proof는 이 소계 밖이며
정확 census와 admission 경계 시험을 닫기 전 효과를 허용하지 않는다.

필수 새 사례에는 root/child split 및 internal propagation, stale revision/identity,
before-해시/가중치/INIT/redo 변조, Reserved 배제, capacity 직전 거절·완료 SMO 보존,
사용자 abort 뒤 구조 유지, CLR의 현재 leaf 재탐색, full-tree 고아/다중부모/범위 손상,
base claim 내부 checksum 재계산 뒤 selected-digest 거절, BaseOnly와 다음 rebase의 claim 보존을 둔다.
실제 WAL stage·sync·page install·DWB/home·CURRENT publication의 fault cut과 reopen은 live 연결 시
입력·기대값·한도를 먼저 동결한다. native 실행 전에는 완료라고 표시하지 않는다.

재검토 조건은 frame/target/loan 한도 변경, 다중 extent, 새 family의 live 활성화,
반복 Undo/overflow로 weight 증명 확장, unlink/retirement debt 추가다. 이 문서는 위 잔여를 숨긴
효과 구현 허가가 아니며 해당 단위의 독립 검토와 실제 검증을 계속 요구한다.
