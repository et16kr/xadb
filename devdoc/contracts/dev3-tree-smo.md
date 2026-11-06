# DEV3 tree SMO — 완결 structural group 연결 설계

2026-11-04 · **D3-TREE-SMO-001 / scope4 wire·owner·provenance 및 원 pool 수명 설계 D2 ACCEPT / 실제 타입·F05·효과·fault 인수 미완료**.

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

<a id="wire-observer"></a>
## 순수 scope4 관찰자 공급과 사전 시험 범위

`semantic_row::structure`는 첫 Row 소비자의 scope4 형식만 검산한다. 기존 scope3
관찰자를 넓히지 않고 RowLeaf/RowInternal 이외 family는 Unsupported다. group/core,
완료·잔여0 descriptor, native5개와 redo5개의 identity·LSN·INIT 대응을 확인한다.
성공은 불변 bytes 관측이며 canonical before·분할 재계산·allocation·install·replay adoption
증거가 아니다. before/after digest는 canonical page와 후속 대조해야 하며, INIT의 after
digest만 현재 완전한 image와 즉시 대조할 수 있다. INIT도 page envelope·full digest까지만
검산하며 XBT1 cell·typed Row grammar는 후속 canonical 관찰자의 의무다. allocator role0은
kind/level·native/redo 동일성만 확인하며 실제 allocator 주소·object/generation·현재 extent의
membership은 원 owner가 결합해야 한다. scalar 비용은 실제 source/Undo와
대조하기 전 untrusted다. 신규 encoder·원 pool 효과는 이 순수 단위 밖이다.

사전 고정 시험은 새6개로 나눈다: (1) root/child × leaf/internal의 독립 raw 형식4개와
접근자·정확 frame 길이, (2) core의 scalar·alias·root 관계 경계, (3) group/owner 및
Finished이지만 잔여가 있는 descriptor, (4) native/redo의 순서·역할·identity·INIT digest
변조, (5) 잘린 입력·section 길이·unsupported scope/tag, (6) core 높이·비용 경계와
첫/후속 structural high-water. XSG/XSM/XOD/native는 시험에서 raw로 조립하고 제품의
scope4 parser를 기대값 생성에 쓰지 않는다. profile은 기존 독립 literal, 하위 page/redo
encoder는 입력 준비에만 쓴다. 실제 split 정답 시험으로 확대하지 않는다. 새 시험만
선택하며 실행별 build 포함180초·4 jobs, source/ELF hash와 최초 실패를 보존한다.
실제 source별 실행·독립 D1 결과는 [08 증거](../evidence/dev3-tree-structure-20261104.md#wire-observer)에 기록한다.

<a id="pool-lifetime"></a>
## F02 구체안 — 원 pool의 Structural claim 수명

상태는 **D2 ACCEPT / 실제 타입 F05·effect/fault 인수 미완료**다. 앞의 scope4 bytes와
별도로 원 `dirty_init::InitClaimIdentity`, `RegistrationDirectory`, plan4 writer/flush/control
owner를 연결하는 수명을 정한다. `TxPair`를 Structural로 가장하거나 두 번째 pool을 만들지 않는다.

### claim과 등록

- `InitClaimIdentity::Structural`을 추가한다. record는 최대2 신규 TargetRequest,
  Structural SystemWorkRef, birth LSN, pool generations/indices 각2개, init_count1|2,
  정확5 target mask를 가진다. RootSplit은 신규2개, ChildSplit은1개다. 비활성 두 번째
  원소는 첫 원소의 동일 값과 generation/index0으로 정규화하며 모든 주소/pin 검사는
  `[..init_count]`만 사용한다. 비활성 칸이 별도 frame/target 권한을 갖지 않는다.
- 필드 산술은 기존 최대 record 안에 들 것으로 예상하지만 증거가 아니다. 새 record뿐 아니라
  `InitClaimIdentity`, `Option<InitClaimSlot>`, `PoolRegistry`의 전후 실제 크기와 그 소비
  census를 비교해야 한다. 기존256B fits 상한만 통과해 registry 증액0으로 표시하지 않는다.
- RootSplit은 기존 `closed_pair`, ChildSplit은 `closed_index`를 사용한다. single finish는
  등록을 보존하고 closed 표지만 해제하는 전용 경로다. 등록까지 지우는 `retire_retention`을
  완료 경로로 쓰지 않는다. System INIT의 published closed_index와 미완료 unpublished
  claim을 구분하고 다른 claim과의 동시 소유를 거절한다.
- `accepts_retention_payload`는 dynamic RowInternal의 level1..15를 좁게 추가한다. 기존
  고정 page/Status/Undo/TreeAnchor/RowLeaf의 level0 규칙, System INIT의 Status/Undo 한정,
  TxPair의 TreeAnchor+RowLeaf shape는 유지한다. 원 loader/base certificate도 해당 새
  RowInternal identity를 검산할 수 있어야 하며 한 곳의 허용만으로 live 공급을 끝내지 않는다.

### 첫 효과 전 준비

원 C의 private factory만 authority를 만든다. 같은 canonical cut의 Catalog handle/schema,
anchor revision/root, allocator, 전체 tree 도달성/소유/고아, 실제 Undo rollback weights,
Reserved 배제와 capacity를 확인한다. decoded frame/Free entry/TargetRequest 자체가 권한이 아니다.
준비 중 canonical before에서 전체 split을 재계산하고 exact INIT image를 비교한다.
5개 target·3또는4개의 latched before stamp·신규1또는2개를 정확히 결합하고, mask의5bit만으로
target 권한을 추정하지 않는다. allocator의 lowest Free9..63과 incarnation+1은 원 공급대로다.

원 pool의 clean victim 선택은 기존 대상5개와 겹치지 않아야 한다. 선택 당시 frame stamp와
pool generation을 claim 설치 시 재검산한다. claim/closed 표지와 소비한 pool generation은
동일 registry guard 아래 설치한다. 원 version/tree exclusion과 physical target 순서의
content latch, content→registry 순서를 유지한다. 실패 반환은 latch 역순→version→pin이다.

RAM의 pin/claim0만으로 I/O quiescence를 증명하지 않는다. 원 selected-DWB/flush owner의
borrowed 증거로 신규 물리 주소의 과거 incarnation에 남은 selected/진행 중 I/O를 해결했음을
확인한다. 원 mutable bound와 Structural ordinary gate로 claim 설치부터 반환까지 새 capture를
배제한다. 취소 permit도 이 원 I/O 경로와 allocator before stamp를 다시 결합한다.

### stage·설치·취소·완료

1. authority와 전체 quota·WAL capacity/prefix/frame을 준비하고 RAM claim을 얻는다. authentic
   WalRequirement는 준비 값이 아니라 실제 complete stage만 발급한다. Structural은
   claim 중 ordinary pin/load/capture를 TxPair처럼 차단한다. 아직 persistent Reserved나
   allocator incarnation 변경은 없다. prepare/claim 실패는 정의한 무효과 오류로 반환한다.
2. 전체5 target을 순서대로 latch하고 before stamp·실제 image를 재검산한다. `enter_stage`가
   첫 WAL 호출 **직전** 일반 취소를 폐쇄한다. 성공 반환을 기다렸다가 폐쇄하지 않는다.
3. 원 stage의 authentic `Effect::Unchanged` rejection만 sealed proof로 prestage 상태를
   복원할 수 있다. stage 진입 뒤 불명 오류·panic·미해결 Drop은 fence하며 성공처럼 취소하지 않는다.
4. complete stage의 원 receipt와 sealed install permit으로5 after/control/DPT를 설치한다.
   `birth == Structural owner.id == complete group LSN`이며 TxPair의 `lsn>reserve` 규칙을
   복사하지 않는다. 부분 설치·control/history/provenance 불일치·오류는 기존 fatal fence다.
5. 원 `RuntimeHistory::installed`는 accepted requirement의 **installed frontier**를 전진시킨다.
   현재 함수는 canonical arena/provenance를 갱신하지 않는다. canonical 재계산·INIT 비교는
   준비 단계의 의무이고, INIT provenance claim 생성은 durability 확인 뒤 guard0에서 수행하는
   후속 canonical replay의 의무다. live 경로에 별도 in-memory 채택 owner를 만들지 않는다.
   설치 완료 token은 durable 완료 token이 아니다. single publish가 즉시 resolve 가능해도
   ordinary gate는 installed frontier와 아래 durable 처리가 끝날 때까지 유지한다.
6. 성공 경로에서도 `release_checked`로 latch 역순→version→pin을 반환하여 guard0을 확인한다.
   RAM claim·ordinary gate·pending만 유지한 채 **원 `wait_durable`을 기다린 뒤 finish한다.**
   authentic WalRequirement와 원 pending
   소유권을 유지하고,5개 DPT는 해당 durability 확인 전 DWB/home으로 진행하지 않는다.
   durable 성공 뒤 완료 token·등록·DPT receipt를 확인해 등록을 보존한 채 claim/closed를
   해제하고 마지막에 pending을 비운다. 실패는 fence, 완료 SMO는 사용자 abort로 취소하지 않는다.

prestage 취소는 unpublished 등록과 claim을 함께 반환하되 이미 소비한 **pool-slot generation은
tombstone으로 남긴다**. clean victim의 옛 등록을 복원할 의무는 없다. allocator의 영속
incarnation은 아직 바뀌지 않았으므로 되감는 쓰기도 없다. 취소한 key는 재사용할 수 없다.

대안은 install 뒤 ordinary gate를 먼저 열고 pending/WAL 보호를 비동기로 유지하는 방식이다.
첫 구현은 기존 CREATE의 원 durable 대기를 재사용하여 완료 수명·오류 경계를 줄인다.
비용은 구조 그룹당 sync1회이며 성능 개선으로 주장하지 않는다. 비동기 완료는 실제 병목
근거와 pending 교체/Drop/DWB 경쟁 증거를 갖춘 별도 D2에서 검토한다.

후속 효과 시험의 필수 반례는 enter_stage 뒤 불명 실패, single publication 뒤 history 실패,
과거 selected DWB 잔존, victim 재선택으로 stamp/generation 변경, ChildSplit 비활성 칸의
alias 오용, 취소 후 stale key, Root/Child 부분 설치, wait_durable 오류와 finish 이전 Drop이다.
정확 fault point 목록·독립 fixture·source-value census는 effect 활성화 전에 추가로 동결한다.

2026-11-04 `/root/dev3_08_review`가 새절을 읽기 전용으로 검토했다. guard0 대기,
authentic requirement 발급 시점, installed frontier와 provenance의 구분을 수정한 뒤
문서 범위 D2 ACCEPT·필수0이다. [검토 기록](../../module-gate/evidence/dev3-08-smo-lifetime/review.md)을
따르며 실제 타입·F05·fault/effect 실행을 승인 또는 PASS로 표시하지 않는다.

<a id="pool-claim-metadata"></a>
### 원 pool claim metadata 공급

`dirty_init::StructuralClaimRecord`와 원 `InitClaimIdentity::Structural`을 추가했다.
1|2 count를 slice 접근 전에 검산하고, Child의 비활성 칸을 정규화한다. birth/work ID,
Row owner·level·주소와 exact5 mask, active INIT의 closed single/pair·pool generation,
기존 allocator/anchor/source/(Child parent) 역할과 실제5 pin을 확인한다. 신규 right는
level0..14이고 Root의 새 root만15까지 가능하다. 물리 주소 alias는 incarnation이 달라도
거절한다. 실제 canonical target5개와 before stamp의 sealed 권한은 후속 factory 의무다.

원 directory는 dynamic RowInternal level1..15만 넓혔고, `finish_retention_slot`은
published exact registration/index를 확인한 뒤 등록과 generation을 보존하고 closed 표지만
지운다. Structural claim은 기존 ordinary pin/load/capture gate에 들어간다. System/TxPair의
분기 정책은 그대로이며 신규 dispatch 시험에서 함께 대조했다. 실제 claim factory,
victim 선택/claim 설치·stage·image/control/DPT 설치·durable finish는 아직 제공하지 않는다.

Rust1.95.0·64bit의 변경 전 실제 타입을 신규 진단으로 측정했다. System/TxPair record216B,
identity224B, slot/Option232B, registry13,616B다. 새 Structural record도216B이고 나머지
실제 타입 크기는 전후 동일하다. 새 검산 branch는2,083B이며, 변경 없는 기존 pair branch의
2,211B 안에 들어 공통 entry phases `(80,3307,76)`을 유지한다. source 식은 rank의
PageRef 인코딩 임시값과 실제 중첩 carrier를 포함한다. 이는 전체 SMO F05 완료가 아니다.

신규8개 선택 시험과 보존한 변경 전 측정·compile 실패·source/ELF hash는
[공급 증거](../evidence/dev3-tree-structure-20261104.md#pool-claim-metadata)에 귀속한다.

<a id="actual-undo-weights"></a>
## 현재 Row profile의 실제 Undo 가중치

분할 입력의 숫자 가중치를 원 cut의 Row/Job/Undo로 유도한다. 순수 local observer
`semantic_row::structure::weights::derive`의 입력은 frame cut, table handle/anchor/schema,
선택 leaf expectation/image, 원64 Job cells, decoded allocator와 immutable Undo image
callback이다. schema의 실제 Catalog 귀속·원 source 안정성·전체 Status/ATT/commit frontier는
상위 canonical owner 의무다. 이 결과 자체로 설치 권한을 만들지 않는다.

현재 profile은 inline≤2048, RowId당 root1개와 Missing 또는 committed RowValue before다.
Committed 행은 Inline·undo=None, Transaction 행은 정확한 Retained Row Job과
actor/logical mutation/UndoRef/현재 XRW1 projection을 결합한다. INSERT·UPDATE는 Inline,
DELETE는 Deleted이며 실제 C/C'와 Job 수치를 비교한다. pair[0]은 anchor와 같아야 하고
pair[1]은 이동 전 hint일 수 있어 현재 leaf와 같을 필요가 없다.

전역 RootReserved를 거절한다. predecessor 선택에 필요한 root의 Undo sequence·물리 U
주소(incarnation이 달라도 같은 주소)·RowId 중복과 같은 actor의 scope/mutation 충돌을
먼저 거절한다. 기존 nearest predecessor와 h bit, 실제 transaction edge를 대조한다.
이 제한 검사는 전체 Root3/terminal/ATT census를 대체하지 않는다. 기존 전체 Job census의
same-object pair 전체 동일성은 이동한 hint를 허용하도록 실제 SMO 연결 때 따로 바꿔야 한다.

선택 U는 allocator의 owner3/generation1/Undo·정확 incarnation에 속해야 한다. leaf와
allocator·U의 pageLSN은 cut 이하, leaf·U는 해당 Job.last_step 이상이다. 기존
`cells::check_live`는 DELETE/UPDATE의 outline까지만 검산하므로 반드시 같은 schema의
typed Undo decode와 live_before도 수행한다. 실제 Live·RowValue·Committed stamp·old
logical·Inline payload와 previous-version Never를 확인한다. own/repeated/version-chain과
overflow의 일반화는08-3/08-4에서 연결한다.

행별 weight는 다음 실제 비용의 최대다.

- 현재 행: XRW1 bytes 길이+16(tree prefix/key12와 slot4).
- Missing 복원:0.
- Inline RowValue 복원: 실제 Undo payload 길이+184. payload 길이+180을 Job C와 대조한다.

각 weight≤8094, 합≤16188을 검산하고 caller output의 성공 prefix만 반환한다. 출력 workspace
capacity 부족은 callback 전에 거절한다. 다른 오류는 부분 prefix를 남길 수 있으며 성공 결과 없이
소비하면 안 된다. 선택 leaf 밖 Retained 행의 존재, 전체 tree/allocator-owned 차이와
전체 incoming/소유 관계는 후속 canonical observer가 검산한다. 현재 입력 범위의 최대
inline 행 비용은2232이므로 이 observer 시험만으로 개별8094 최대 행을 인수하지 않는다.

독립 reviewer의 사전 D2에서 typed Undo와 current projection, predecessor의 유일성,
same-cut LSN/allocator, historical hint, 오류 출력과 권한 범위를 지적받아 반영했다.
새 fixture는 기존 독립 D/U literal의 native after Row/Job/U만 입력으로 추출한다.
기대 비용은 literal before/after Row cell 길이에서 별도로 계산하며 Job C나 제품 U
decoder로 만들지 않는다. 기존 transition suite는 실행하지 않는다. 실행·D1 결과와 전체
SMO의 실제 source census/F05는 각각 원 증거와 후속 효과 인수에 귀속한다.

<a id="five-image-candidate"></a>
## 같은 cut의 다섯 이미지 후보

`semantic_row::structure::candidate`는 원 owner가 안정화한 profile/frame cut·table handle·
source PageRef·JobCells·이전 Structural high-water/descriptor와 immutable page callback을
받는다. C의 replay arena나 bank 배치를 복제하지 않는다. complete LSN은 원 cut+1, 새 epoch는
원 frame 입력이다. 이전 Structural owner는 같은 namespace의 완료·잔여0여야 하며 이전 epoch도
허용한다. 이전 bank와 callback이 실제 정본이라는 source 증명은 원 owner에 남는다.

현재 fixed CatalogLeaf에서 exact table/schema/anchor를 얻고, 그 anchor의 전체 tree와
allocator의 객체 소유 집합을 양방향 비교한다. object로 먼저 분류한 뒤 generation·kind를
검산하므로 잘못된 generation이나 kind의 orphan도 누락하지 않는다. allocator Reserved와
RootReserved를 배제하고 재사용 가능한 최저 Free부터 선택하며 incarnation 최댓값은 건너뛴다.
leaf마다 실제 Undo 가중치를 검산하고 해당 객체의 Retained Row Job마다 현재 RowId가 있는지도
역방향으로 확인한다. 이전 leaf hint나 CREATE pair를 현재 위치의 근거로 사용하지 않는다.

RootSplit/ChildSplit은 현재 source의 도달성과 실제 parent를 검산한 뒤 다섯 after와 core를
만든다. unchanged internal은 `prepare_internal_split`이 기존 pivot 선택을 재사용하며 pending
separator를 가짜로 삽입하지 않는다. internal pivot은 승격되므로 양쪽 실제 비용 합은 원 비용보다
pivot 비용만큼 작다. leaf 후보에는 pending 변경을 넣지 않는다.

다섯 after를 원 before 위에 겹쳐 본 전체 tree와 allocator 소유·실제 가중치를 다시 검산한다.
전후 ordered RowId·길이·XRW1 bytes digest도 비교해 행 bytes/version 보존을 확인한다.
output은 caller workspace이며 오류 때 부분 변경될 수 있다. 성공 Candidate 없이 어떤 after도
소비하면 안 된다. 원 before는 불변이며 이 API는 heap·I/O·pin·WAL·설치 효과나 권한이 없다.

전체 Root/terminal/Status/ATT·commit frontier·incoming·역사 증명은 기존 canonical owner의
의무다. 기존 전체 closure의 level0·pair 동일성 가정을 실제로 확장하기 전에는 이 결과를
scope4 replay acceptance나 sealed INIT 권한으로 승격하지 않는다. Catalog가 committed-but-
unpruned이면 기존 Status/terminal 증명으로 처리하며 stamp를 무조건 Committed로 제한하지 않는다.
이 순수 후보 공급과 후속 source census/F05·native 효과 인수는 구분한다.

사전 D2는 위 입력·scope·출력 수명 및 wrong-generation orphan·이전 epoch 반례를 요구했다.
새 시험은 literal Row/U의 leaf Root/Child 분할, internal Root/Child pivot·child 보존, separator
1/2개·child incarnation alias 거절, Reserved·Free capacity·generation 소진, stale handle·orphan,
후반 callback 오류의 부분 output을 다룬다. 원64-page extent에서 유일 child를 요구하는 Row
internal은40B entry이므로 정상 parent가16,188B를 채울 수 없다. parent 용량 경계는 분리된
순수 편집 함수에서 검산하고 이 profile의 정상 whole-tree 분기 도달을 주장하지 않는다.

최초 후보 공급은 page::MAX_PAGE_SLOTS3,244를 사용해 refs/side_refs와 parent refs가
각각51,904B, weights/audit scratch가 각각25,952B였다. 앞의 제안 소계16,208B/8,088B는
당시 실제 구현의 측정·F05가 아니었다. 후속 [분할 source 검산](#split-source-bound)은
현재 Row 형식에 맞춰 reference406·weight87로 한정하고 실제 중첩 수명을 합산했다.
Candidate 자체는 after를 borrow하지 않지만 [scope4 생산·정본 비교](#scope4-producer)가
같은 호출의 재계산과 immutable output borrow로 emitter 수명을 닫는다. 성공 후 caller가
바꾼 after에 이전 성공 결과를 재사용하는 API는 공급하지 않는다.

<a id="scope4-producer"></a>
## scope4 생산·정본 비교

`structure::frame::produce/observe`는 같은 호출 안에서 `candidate::prepare`를 먼저 수행한 뒤
그 결과와 다섯 after의 immutable borrow를 private Write/Compare emitter에 넘긴다. 외부
Candidate나 이전 성공 결과를 받아 frame을 만드는 우회 API는 없다. page callback·Job/bank의
원 cut 안정성과 전체 actor/history closure는 앞 절과 같이 원 owner 의무다.

새 descriptor는 원 complete LSN·epoch로 만든 Structural owner의 Finished·잔여0이다.
group의 previous/digest는0이고 이전 bank의 high-water/digest는 core에만 기록한다.
native5/redo5는 실제 물리 순서이며 기존 page의 priorLSN·before/after full digest와 신규
INIT full image를 결합한다. 하나의 emitter가 group·descriptor·core·native를 쓰거나 exact
비교하고 redo는 canonical decoded 모든 필드/data를 비교한다. outer header·exact length·
trailer는 scope4 decoder와 재계산 header 비교로 함께 검산한다. 두 번째 full frame은 만들지 않는다.

frame buffer 부족은 다섯 after 준비 뒤 확인하므로 after는 이미 바뀔 수 있지만 frame은
보존된다. 성공은 정확 used prefix의 길이를 반환하며 그 밖 tail을 보존한다. 그 외 오류에서는
두 buffer 모두 workspace이고 성공 없이는 소비하지 않는다. 호출 중 after를 바꿀 외부 실행
간격은 없다. 반환 후 caller가 바꾼 after에 대한 설치 권한은 별도 문제이며, live 설치 경로가
immutable borrow 또는 재검산으로 결합해야 한다. 이 API는 stage·설치·복구 수락 권한이 아니다.

사전 D2는 이 API 경계와 단일 emitter를 한정 수용했다. 새 시험의 독립 section serializer는
이미 별도로 시험한 후보의 이미지/core를 입력으로 받되 제품 frame emitter·descriptor/redo
encoder를 호출하지 않는다. 이는 frame 형식 oracle이며 전체 SMO 의미의 별도 재구현은 아니다.
Root/Child leaf/internal의 bytes, wire decode는 수락하는 checksum 보정 변조의 canonical
거절, 실제 before 결합, output 재계산, buffer/tail·이전 epoch owner·callback 오류를 검산한다.

<a id="base-structural-claims"></a>
## Base4 certificate와 Structural birth 관측

shared `image_records`의 RowInternal certificate는 Profile3에서만 level1..15를 허용하고,
기존 kind는 level0을 유지한다. certificate raw[2]·현재 page header·provenance raw[3]은
같아야 한다. Base3/Profile2 외곽 결합과 기존 INIT role1..4의 level0·reserve<init·zero tail은
바뀌지 않는다. 이 확장은 raw layout 관측이며 full graph나 selected-source 권한은 아니다.

role5/6의 source는 namespace가 같은 StructuralChange이고 id=reserve=init LSN,
generation=writer epoch다. 현재 제한 profile의 object generation/schema epoch는1이다.
right는 RowLeaf0 또는 RowInternal1..14, new root는 RowInternal1..15다. ChildSplit은
role5와 zero peer만 허용한다. before revision은 양수이며 checked increment한 값이 after다.
anchor·left·self·RootSplit peer는 첫 extent의 dynamic reference이고 물리 주소가 서로 달라야
한다. incarnation만 바꾼 alias도 거절한다. `object()`는 Structural에도 값을 주지만 `pair()`는
CREATE 출처만 반환한다. 별도 borrowed `StructuralInit`이 역사 필드를 노출한다.

Base4 layout은 새 role의 미래 writer epoch를 거절하고 **같은 init LSN 전체**를 비교한다.
owner 전체·epoch·object·anchor·left·frame/core digest·operation·birth revisions가 같아야 한다.
RootSplit의 살아 있는 claims는 최대2개, 서로 role5/6·상호 peer·level 차이1이어야 하며,
ChildSplit은 role5 한 개다. 한 claim의 owner/epoch를 바꿔 비교 그룹을 피할 수 없다.

role6는 생성 당시의 역할이므로 현재 anchor root나 현재 revision과 같다고 요구하지 않는다.
historical source/peer의 현재 할당도 요구하지 않는다. 정확한 peer incarnation이 현재 살아
있으면 같은 birth의 대응 claim이어야 한다. 퇴역했거나 같은 주소가 다른 incarnation으로
재사용됐으면 그 옛 peer의 존재를 강제하지 않는다. CREATE pair의 현재 상호 존재 규칙은 이
순수 공급에서 아직 바꾸지 않으며 실제 unlink/retirement 연결 때 따로 일반화한다.

raw layout은 임의 initial digest의 역사적 진실성을 보증하지 않는다. mint는 앞 절의 accepted
canonical frame·exact INIT 비교 뒤 원 history owner만 수행한다. 이 birth 관측 공급 당시에는
System ledger·bank·현재 전체 tree closure·replay dispatch를 확장하지 않았으므로 raw 수락을 복구 수락으로
승격하지 않는다. 기존 Allocation System 소비자에 새 claims를 그대로 넘기는 우회도 금지한다.

source bound는 shared provenance grammar·borrowed accessor의 실제 typed carriers와 결과를
명시해 Base3/Base4 layout에 더하고, Base4 group loop의 views·ranges 및 certificate의 새
scalar branch를 별도로 센다. caller의2MiB/page loan은 기존 소유자가 계산하며 이 관측층
계산만으로 SMO F05나 live admission이 충족됐다고 판정하지 않는다.

<a id="structural-bank"></a>
## Finished split bank와 복구 인계

`structure::bank`는 앞의 RootSplit/ChildSplit Finished descriptor 전 필드를 관측하는 순수
validator다. Profile3, 같은 namespace의 Structural owner, id=high-water≤실제 cut,
generation≤실제 writer epoch를 요구한다. phase=Finished, sequence/cursor/limit=1,
chain apply=birth=id·previous0·groups1, empty inventory의 change=id, WAL은 Root84,744 또는
Child84,640B, control·remaining·minimum WAL·last mutation은0이다. typed validator는
decoded DescriptorSpec를 빌리고 raw observer는 원 descriptor decoder로 reserved bytes까지
검산한다. high-water0은 descriptor 없음과 동치다. 관측 결과는 원 split 발생이나 권한의
증명이 아니다. candidate의 previous는 기존 owner 선행검사·generic decode와 오류 경계를 유지한다.

Base4/Profile3만 header의 Structural high-water를 허용하고 bank wire slot33에 그 마지막
descriptor를 보존한다. Finished를 active owner·remaining·XOI에서 빼기 **전에** 위 조건을
검사한다. Allocation의 wire32·Aging의 wire34와 그 기존 의미는 유지한다. 살아 있는 최신
Structural claim은 owner 전체와 operation–WAL 비용까지 마지막 descriptor와 같아야 한다.
이전 claim은 id<latest.id, epoch≤latest.epoch다. 같은 epoch의 여러 split은 허용하며,
역사 claims가 모두 퇴역했어도 마지막 Finished descriptor는 유지할 수 있다. System INIT
관측은 kind별로 분리하고 Allocation 한 개 INIT 선택에 Structural claims를 섞지 않는다.

원 RAM bank에서는 Structural slot34, Allocation33, Aging32다. Structural row1664B는
descriptor1536B, high-water1536..1544, STABLE byte1624이며 그 외 metadata는0이다.
empty row는 전부0이다. admission-only `control/catalog::check`도 같은 원 row observer를
호출하여 high-water·완료 상태·부채0·prefix cut/epoch와 metadata를 검사해야 한다.
`history::validate_live`를 먼저 호출하는 다른 입구만을 근거로 이 검사를 생략하지 않는다.

verified base seed는 wire33을 RAM34로, capture pack은 RAM34를 wire33으로 정확하게
인계하며 high-water도 원 값 그대로 복사한다. immutable bank의 remaining/pack 순회 앞에서
같은 RAM observer를 실제 selected-source cut/epoch로 호출한다. 그 관측이 반환된 뒤 같은
immutable bytes를 순회하므로 두 owned descriptor bank를 만들지 않는다. observer가 끝나기
전후의 실제 typed carrier·source bound를 별도 검산하고 기존 F05 한도를 늘리지 않는다.

이 bank 공급에서도 scope4 replay dispatch와 INIT mint는 아직 열지 않는다. 현재 tree의 ownership·
전체 도달성·Row/Undo 검산이 완성될 때까지 Structural tree가 포함된 full closure를 실제로
거절하는 경계(RowLeaf의 CREATE pair 검사·RowInternal Unsupported 또는 동등한 명시 gate)를
유지한다. nonempty INIT의 raw provenance 관측만으로 selected predecessor의 역사 신뢰를
새로 획득했다고 표현하지 않는다.

<a id="current-tree-closure"></a>
## Base4의 현재 Row tree 전체 검산

이 단위는 current tree의 관측과 full Base4 closure를 연결한다. 구현·선택 실행은
[원 증거](../evidence/dev3-tree-structure-20261104.md#current-tree-closure)를 따른다.
Profile3/Base4에 한정하고 Base3의 단일 CREATE leaf 규칙은 유지한다.

현재 table의 기준은 Catalog record가 지목하는 안정 anchor와 그 anchor의 현재 root다.
CREATE pair[1]은 생성 이력이며 현재 root 또는 현재 행 위치가 아니다. CREATE의 기존 두
claim 결합은 이 split 연결 단위에서 유지한다. 최초 leaf 자체의 retirement와 그때의 역사
pair 승계는 뒤의 retirement 단위에서 별도로 검증한다.

공통 borrowed tree 관찰자는 LayoutView와 Catalog ordinal을 받아 그 view의 Catalog bytes에서
record·schema를 직접 선택한다. 다른 cut의 CatalogRecord나 schema를 인자로 받지 않는다.
같은 immutable LayoutView에서 anchor·root를 읽고, 현재 tree의
모든 page를 bounded audit로 방문한다. 첫 file의 동적 ordinal9..63, 실제 certificate의
전체 identity/kind/level, owner/generation, 엄격한 key 범위와 전역 physical alias 부재를
검산한다. 도달한 집합은 allocator가 해당 object에 할당한 RowLeaf/RowInternal 집합과 정확히
같아야 한다. 다른 object의 child, 고아 leaf/internal, 잘못된 incarnation, 중복 child는
거절한다. anchor와 같은 object의 다른 종류 page 또는 추가 anchor도 허용하지 않는다.
최대64개의 기대값과 깊이16의 불변 audit scratch는 호출 안에서 끝나고, 반환에는 borrowed
schema·anchor/root와 검산한 leaf mask만 남긴다. 이는 live 권한이나 선택 이력의 증명이 아니다.

Structural role5/6 page의 object와 역사 anchor는 현재 Catalog identity/anchor와 같아야 한다.
birth의 after revision은 현재 anchor revision 이하이고 init LSN은 anchor의 현재 page LSN
이하다. anchor의 revision1 초과는 Structural high-water가 있는 경우에만 허용하며 revision은
1+그 high-water 이내다(u128 비교로 HW=u64::MAX도 overflow 없이 처리; LSN을 이용한 보수적
상한이며 실제 split 횟수 증명이 아님). 원 CREATE
anchor/leaf의 provenance bytes는 바꾸지 않는다. allocation incarnation의 보수적 역사 상한은
기존 Allocation/CREATE 항에 Structural high-water를 u128로 더해 overflow 없이 계산한다.

Row actor 관찰자와 전체 graph는 같은 현재 tree를 쓴다. 각 Job은 안정 anchor만 결합하고
RowId를 현재 root에서 찾아 존재/부재·상태·projection·Undo·Status를 검사한다.
Retained 행의 현재 leaf LSN≥Job.last_step도 확인한다. Reserved/Releasing에는 Undo release만
진행된 정상 상태가 있으므로 이 조건을 일괄 적용하지 않는다. 역방향 검사는
검산한 모든 leaf의 실제 행을 훑어 누락 Job과 전역 RowId/logical identity 중복을 거절한다.
leaf 열거의 physical ordinal과 cell 위치는 이 immutable 호출 안에서만 식별자로 쓰며 cursor
계약이 아니다. 두 번의 열거 사이에 페이지가 바뀌는 live 입력은 허용하지 않는다.

복원 공간은 각 leaf에 실제 존재하는 Retained RowId의 DELETE/UPDATE 분량만 적립한다.
그 전에 모든 Row actor의 실제 typed Undo before와 Job cell 길이·현재 행·projection 결합이
완료돼야 한다. 따라서 같은 table의 다른 leaf 분량을 중복 청구하지 않으며, Job의 길이만
믿고 typed Undo 검산을 생략하지 않는다. Reserved·Releasing·terminal·전역 incoming/Undo
검산은 기존 그대로 유지한다. 구조 변경 직전의 Reserved 금지는 별도 SMO 후보의 전제다.

full closure 수락 시험은 독립적으로 쓴 split 이후의 leaf/internal/root·allocator·anchor·
birth·bank를 정상 이력 fixture와 결합한다. 실제 이동한 Retained INSERT/DELETE/UPDATE와
이전 leaf hint, 다른 leaf의 복원 공간, orphan/alias/wrong range/owner/incarnation 및
Root/Status/Undo 변조를 구별한다. 다중 leaf에서도 Reserved/Releasing INSERT/DELETE/UPDATE의
기존 present/absent 규칙(DELETE/UPDATE 상태 행렬은 실제 actor 관찰자 한정), 이동 leaf의 stale LSN, 다른 schema 혼입과 두 leaf 각각의 D 경계를
검증한다. 중첩 each_row/unique_row에서 살아 있는 table/leaf 반환값도 source census에 포함한다. 앞의 leaf별 D는
[DELETE](dev3-row-delete.md#leaf-reservation)·[UPDATE](dev3-row-update.md#leaf-reservation)의
단일-leaf 합산식을 일반화한 같은 불변식이다. raw digest를 재계산한 것만으로 selected CURRENT/WAL
역사 검증을 통과했다고 주장하지 않는다. 모든 새 nested audit/lookup와 실제 caller census를
독립 계산하고 원 cap을 유지한다. scope4 replay·INIT mint·원 pool 설치는 그다음 연결이다.

<a id="split-source-bound"></a>
## Canonical split 호출의 임시 공간

scope4 replay 연결 전에 후보·frame 재계산의 source 상한을 검산한다. 현재 범용
MAX_PAGE_SLOTS는 임의 page cell의 상한이어서 Row SMO의 reference/weight 배열에 그대로
쓰면 원 recovery 호출 한도를 넘는다. 전역 page 형식이나 cap은 바꾸지 않는다.

현재 Row payload는 최소168B header이고 tree cell의 prefix/key12B와 slot4B를 더하면
최소184B다(Deleted도 포함). 따라서 leaf당 최대 floor(16188/184)=87행이다.
Internal cell은 prefix4B+RowId8B+PageRef24B+slot4B=40B로 최대404개 separator다.
일반 parent 편집의 reference scratch는404에 metadata와 parent 삽입1개를 더한406개,
leaf weight scratch는87개로 한정한다. 실제 분할의 refs/side refs는88개다. 이는 동일 원본에
대한 전체 before audit가 성공하고 source가 그 visited 집합에서 선택된 뒤에만 적용한다.
첫 extent의 서로 다른 최대64page를 전부 방문하는 audit는 internal의 모든 child와 alias를
검사하므로 해당 internal의 metadata+separator 수(=child 수)는64 이하다. leaf87+metadata1이
이를 지배한다. audit 없이 쓰는 일반 parent 편집의406개는 줄이지 않는다.
이 상한은 실제 decode 후 허용되는 현재 Row 형식의
모든 입력을 포함한다. 지원하지 않는 payload나 과다한 raw cell 입력은 후보를 얻지 못하며
scratch 부족을 수락으로 바꾸지 않는다. 부모가 가득 찬 정상 입력의 Capacity 거절과 출력
보존도 그대로 검증한다. Row overflow/일반 schema 지원 범위를 넓히는 계약이 아니다.

같은 호출에서 살아 있는 원 refs·side refs/parent refs, weights, allocation cell,
preorder64·DFS16, 전후 table/view와 typed Undo·Row·Catalog codec 결과 및 frame 비교의 descriptor,
core/native/redo 결과를 source census에 합산한다. 완료한 단계의 max와 중첩 caller의 sum을
구분하고 callback capture도 호출자가 별도 가산한다. 반환 candidate와 after5는 권한이 아니며,
after5는 기존 caller loan으로 세고 새 배열을 숨겨 할당하지 않는다. 이 계산은 실제 compiler
stack/RSS 측정이 아니다. 원 recovery/F05 한도는 유지하며 실제 caller 연결은 다음 단계에서
그 callable owner의 census로 다시 검산한다.

scope4 실제 호출자 연결의 D2 보완은 별도 allocator64 배열을 없앤다. 검증된 원 cell6208을
private `encode_allocator`의 배열로 복사하고 선택한1/2개의96B entry만 기존 encoder로 바꾼다.
동일 immutable allocator에서 lowest eligible Free를 선택하며 각 entry의 다음 generation을
다시 확인한다. 나머지 entry bytes는 보존하고 `encode_allocation_page`로 전체 inventory를
검산한 뒤 after0를 쓴다. caller 잔존 값에는 helper의 배열·codec 최대 중첩을 더한다.

first-extent audit는 기존 iterative walk를 공유한다. root와 같은 file·page_no<64를 bit 연산
전에 검사하고 u64 seen으로 incarnation이 다른 physical alias도 callback 전에 거절한다.
root도 seen에 넣으며64B ordinal 목록에 preorder를 보존한다. 반환 count만 유효 prefix이고
나머지는0이다. 성공한 동일 immutable cut의 allocator·원 page에서 incarnation·owner/gen/kind·
level을 다시 얻으며 ordinal 자체를 정체성으로 쓰지 않는다. 범용 audit의 cross-file 허용·
capacity와 tail None·depth16/range 검산은 유지한다. 공통 source에는 public wrapper와 visitor의
root·loan/counter·네 borrow capture120B를 추가한다.

private build가 반환한 뒤 verify_after를 시작하고 public prepare는 전후 ordered row hash까지
같을 때만 Candidate를 반환한다. 생성 전의 Candidate/Result를 모든 중첩 단계에 중복 가산하지
않으며 실제 반환 단계와 public wrapper가 쥔 draft는 각각 센다. `inline(never)`는 함수 경계를
유지하는 장치이며 공간 검증을 대신하지 않는다. 기존 source/F05 cap과 wire bytes는 유지한다.
담당 `/root`, 독립 `/root/dev3_08_review` D2 ACCEPT·필수0이며 실제 선택 결과는
[scope4 기록](../evidence/dev3-tree-structure-20261104.md#scope4-replay)에 둔다.

선택 시험은 새 최대 밀도 leaf·parent404/삽입405 거절·source inventory와 이 배열 크기에
직접 영향받는 기존 Root/Child leaf/internal 후보 및 canonical frame 비교로 한정한다.
완료된 무변경 wire/weights/bank/Base4 suite는 반복하지 않는다.

typed Undo의 INSERT 검산은 canonical432를 둔 check_material→encode_material과 실제
encode/decode를 합한다. DELETE/UPDATE는 material·outline·metadata 반환을 유지하는
check_outline chain을 별도로 센다. SHA1536은 해시를 호출하는 codec과 같은 branch에 더하며,
완료된 별도 digest 단계와의 max만으로 중첩 공간을 대체하지 않는다. frame의 parser와
emitter도 descriptor/redo/order/header 및 반환 carrier를 독립 inventory와 대조한다.

<a id="scope4-replay"></a>
## Canonical scope4 replay와 최초 기록 발급

다음 연결은 원 recovery/history arena의 scope4 분기다. arbitrary raw frame은 table/source를
선택할 입력일 뿐이며, `frame::observe`가 같은 accepted before arena·Job cells·기존 Structural
bank에서 다섯 after를 새로 계산하고 frame 전체를 비교해야 다음 단계로 간다. 원
ReplayAllocation의 Reserved 상태와 모든 RootReserved는 split 앞에서 거절한다. cut·epoch·
namespace와 이전 high-water/descriptor digest도 기존 cursor와 원 bank에서 검산한다.
이전 RAM34 전체1664B를 `control::observe_structural_bank_row(profile, 실제cut, epoch, row)`로
먼저 관측한다. candidate의 descriptor/HW 검사만으로 PHASE=STABLE·unused metadata0·
HW0⇔빈 row 검산을 대신하지 않는다. zero-HW garbage와 non-STABLE row를 덮어 정상화하지 않는다.

새 history 함수는 원 arena/cells/bank/after8/claims64를 분리해 빌린다. after의 앞5page만
사용하고 별도 replay buffer나 pool을 만들지 않는다. RootSplit의 right/new-root 두 INIT,
ChildSplit의 right 한 INIT는 allocator가 고른 Free page이며 기존 canonical arena image와
claim이 비어 있어야 한다. 삭제된 다른 incarnation의 physical home은 여기의 before가 아니다.
기존 allocator generation 증분·고아/중복/범위·전후 Row bytes·실제 Undo 가중치는 candidate가
검산한다. 이 원자 system group은 Job·Status·ATT·사용자 Undo를 변경하지 않는다.

새 role5/6의352B 기록은 fresh comparison 성공 뒤 private 함수에서만 작성한다. 실제 frame
digest, 정확한 최초 after digest, 비교된 core digest, 원 header의LSN/epoch와 완료 SystemWorkRef,
object/anchor/left/peer·전후 revision을 기록한다. Root 두 기록과 다섯 target의 정체성·범위·
기존 claim 충돌을 모두 확인한 뒤에만 arena·claims·Structural RAM34 row를 함께 갱신한다.
실패한 compare/claim 준비는 canonical arena/cells/bank/claims/cursor를 바꾸지 않으며 after는
실패 workspace다. 이 기록을 원 dirty INIT 권한으로 직접 전환하지 않는다.
Root 두 번째 destination image/claim만 충돌하는 반례도 첫 INIT를 publish하지 않는지 확인한다.

구조 완료 descriptor는 같은 frame의 비교된 bytes이며 high-water=LSN, STABLE, 나머지 metadata0
형식으로 RAM34에 둔다. 기존 state digest는 claims까지 포함한다. selected WAL prefix→
analysis seal→base pack/readback→CURRENT full digest→old-WAL retirement의 기존 신뢰 사슬을
우회하지 않는다. raw Base4 관측 fixture만으로 selected history를 증명했다고 하지 않는다.

analysis의 transient target table은 기존72B/target을 유지한다. kind를 담던 u64의 하위8bit에
kind, 다음8bit에 level을 넣고 나머지는0으로 검사한다. 기존 level0 bytes는 같다. Profile3
동적 object의 RowInternal은 level1..15만 허용하고, 나머지 page kind는 level0을 유지한다.
canonical image·physical home의 level과 decoded target을 비교하며 다른 level은 거절한다.
이는 사내 transient scratch의 표현이며 Base/WAL의 wire layout 변경은 아니다.
`note_target`도 image의 decoded level과 request.level을 쓰기 전에 대조한다. DWB의
`observe_pending`은 같은 incarnation에서 pending.level==request.level을 요구하고,
`flush_final`은 request.level을 PendingImage에 보존한다. 미래/과거 incarnation의 기존
‘나중 exact accepted INIT가 증명’ 경로에서는 다른 incarnation의 level을 현재 target과
섞어 비교하지 않는다. RowInternal은 Profile3·ordinal9..63·owner≥6·owner generation1·
level1..15의 결합으로만 허용한다. kind-word 상위48bit는 거절하고 legacy level0 bytes를
보존한다. target encode/decode·home·selected Pending·최종 flush가 같은 level 규칙을 쓴다.

선택 검증은 원 full-state before에서 Root/Child leaf/internal replay, 실제 INIT digest/role·
bank/high-water 인계, 유효 checksum을 가진 canonical 변조와 occupied destination/claim의
무효과 거절, 잘못된 scope/cut/epoch/Reserved, level을 보존하는 target/physical 관측을 포함한다.
전체 source census에는 줄인 frame23,116B 위의 dispatch·selection·mint/return·callback 수명을 더해
기존 recovery/metadata/live cap으로 검산한다. 실제 owner의 선택된 WAL과 CURRENT publication,
원 pool 설치·native fault/재진입 및 retirement는 이후 효과 인수로 연결하며 이 순수 replay
단위만으로08-1 완료를 선언하지 않는다.

<a id="original-split-prepare"></a>
### 원 이력과 pool의 split 비교 준비

F02의 첫 연결은 원 `RetentionWriter::prepare_row_split`과 private
`PreparedStructural`이다. `/root/dev3_08_review`가 준비 순서와 아래 한정을
D2로 수용했으며 구현·선택 실행의 결과는 [08 증거](../evidence/dev3-tree-structure-20261104.md)에
별도로 기록한다. 이 결과는 claim·WAL stage·install 권한이 아니다.

- 원 `existing_entry`의 health, pending/population Reserved 배제, visibility/core,
  idle owner/reader, owner identity와 ledger 검산을 재사용한다. `ControlCut`의
  bank·Jobs·XOI digest와 issued/adopted를 생산 **전** 포착한다.
- 원 `RuntimeHistory`의 checkout에서 `validate_live`로 실제 durable WAL을 재생한다.
  파일 읽기가 끝난 뒤 같은 arena와 원 Jobs/Structural bank로 split을 새로 계산한다.
  Candidate의 전체 트리·allocator·Undo weight·전역 RootReserved 검산을 유지한다.
- 새1/2개 목적지의 canonical image와352B claim이 비어 있어야 한다. Root의 둘째
  INIT도 검사한다. Child의 role4는 기존 parent이므로 이 vacancy 검사의 대상이 아니다.
- 기존3/4개 target만 물리 순서로 원 pool에서 읽는다. before bytes가 canonical
  arena와 같고 identity/kind/level 및 latched allocator의 Allocated entry와 일치해야
  한다. 실제 FrameStamp와 role 대응을 보존하며 INIT에 옛 stamp를 만들지 않는다.
- 성공·실패 모두 read의 checked 반환, replay loan의 checked 반환, audit 종료 순서를
  따른다. prefix 전체와 최초 ControlCut을 종료 때 다시 대조한다. WAL/control/history의
  installed/durable/pending/issued 상태를 전진시키지 않는다.
- 원425984B `RowLoanLayout`의 FRAME·BEFORE·AFTER를 빌린다. frame과 before/after의
  불변 borrow 및 정렬한 request/role/stamp를 private non-Clone 결과에 함께 보관한다.

원 writer에서 실제 claim을 시작하기 전에는 owner/core/binding/fence, prefix/control cut,
각 before stamp를 다시 확인해야 한다. 남은 전체 F02 준비에는 기존 outstanding WAL/steps를
보존한 이번 그룹의 용량·counter 여유, XOI successor, victim 세대와 selected-DWB/flush의
quiescence가 포함된다. 이 비교 준비 단위는 그 의무를 대신하지 않으며 실제 effect를 열지 않는다.

<a id="structural-admission"></a>
### 분할의 기존 부채 보존과 clean victim 선택

후속 D2 구체안은 같은 원 replay loan에서 split 후 allocator와 변경하지 않은 Root3·
ATT/Allocation/Aging/Job을 `ProjectedInventory`로 읽어 XOI successor를 만드는 것이다.
기존 `encode_row_current`를 재사용하며 Finished Structural은 잔여 부채0이므로 XOI에
새 가짜 owner를 넣지 않는다. 결과는 원 BEFORE의 여유 role5에 두어 caller memory를
늘리지 않고 Prepared의 immutable borrow로 보존한다. 그 공간은 before target이 아니다.

첫 claim 전 `check_progress_room`으로 이번 frame과 기존 future WAL/steps 모두를 합산한다.
WAL end·LSN, 기존3/4개 target의 modification counter+1+future steps, 변경하지 않은
RetentionRoot의 revision+future steps를 검사한다. 신규 INIT counter는1이고,
양수 group LSN+future steps 검사가 그 counter의 남은 여유도 보수적으로 보장한다.
검사 실패는 원 writer/pool/WAL/control을 바꾸지 않는 오류다. stage 직전의 같은 cut·
identity·stamp 재검산은 별도 의무이며 이 사전 계산으로 생략할 수 없다.

원 pool의 `StructuralCandidates`는 신규1/2개만 선택하고 exact5 target 전부를 victim에서
제외한다. 기존 bounded `retention_candidate_at`을 재사용하되 TxPair로 가장하지 않는다.
선택은 복사 관측이며 pin·claim·generation을 소비하지 않는다. active slot의 content stamp와
next generation을 한 최종 registry guard 아래 다시 대조한다. Child의 비활성 둘째 slot은
첫째와 같은 값·next generation0으로 정규화하며 별도 latch·주소·quota 권한이 없다.
실제 claim은 원 선택 provenance와 모든 active stamp/generation을 다시 확인해야 한다.

필수 선택 시험은 active Row 부채가 있는 원 prepare의 exact WAL fit/1byte 부족과 XOI
보존, LSN/counter/revision의 독립 경계, Root2/Child1 clean slot, 둘째 victim 부족,
exact5 제외·이미 등록된 신규 주소·latch busy·generation 소진·선택 중 stamp 변화다.
고유 시험과 변경된 source census만 실행하고 완료한 무변경 사례를 반복하지 않는다.
이 단계도 selected-DWB/flush lease·실제 claim·stage/install/finish 및 fault/reopen을
완료한 것으로 표시하지 않는다. 독립 검토와 실제 실행은 별도 증거에 남긴다.

2026-11-04 `/root/dev3_08_review`가 방향을 D2 ACCEPT했다. `InventoryRows`의 세 None은
기존 원 행 보존이며, 원 base generation·새 group LSN과 같은 canonical Root3를 쓴다.
후보의 dirty→Flush 관측은 실제 clean claim 권한이 아니다. 원 flush 뒤 다시 선택하고
claim 시 재검산한다. published System closed_index 자체는 새 일괄 거절 조건이 아니며
미완료 claim·pin·load·flush의 기존 독점 규칙을 유지한다. 선택의 content guard는 개별
선언으로 registry→둘째→첫째 반환 순서를 보장한다. 실제 공급 결과는
[08 증거](../evidence/dev3-tree-structure-20261104.md#structural-admission)에 기록한다.

<a id="structural-effect"></a>
### 실제 RootSplit/ChildSplit 효과와 fault inventory

상태는 **2026-11-04 독립 D2 ACCEPT / clean Row leaf의 실제 effect 구현**이다.
선택 실행·실패·최종 검토는 [원 증거](../evidence/dev3-tree-structure-20261104.md#structural-effect)에 결속한다. 원 MutationPort의 private split 진입점은
이미 검증한 prepare와 StructuralCandidates를 소비한다. 원 MaintenancePort의 storage/pool
origin을 대조하고, Ready·잔존 capture/authority/durable/retired 없음·selected DWB의 idle
preflight를 확인한 mutable flush lease를 claim 전부터 finish/cancel까지 연속 보유한다.
이 검사는 신규1/2 physical request 및 selected generation에 결속한다. dirty 후보는 무효과
Busy로 반환하며 원 호출자가 flush 후 다시 준비한다. 새 별도 I/O owner나 fake TxPair는 없다.

진입의 guardless health·pool/storage/flusher origin 확인 뒤 원 owner의
`close_reads_for_retirement`와 원 isolation의 `OperationSourceAdmission::catalog_progress`를
얻어 deep prepare부터 전체 effect 반환까지 보유한다. source admission 실패도 gate를 checked
reopen한다. 실제 새 effect phase의 최대가 기존 `CatalogProgressCharge` 안인지 계산하고
검사하며, 필요하면 원 charge 계산의 별도 branch로 넣어 실제 caller 전체 cap을 다시 확인한다.
종료는 성공/거절/fatal 모두 source의 `finish_checked`와 gate의 `reopen_checked`를 각각 시도해
오류를 합친다. source 반환 오류를 이유로 gate 반환을 건너뛰지 않는다. 별도 source owner나
standalone 수치만으로 concurrent ReadPort의 예산을 예약한 것으로 표시하지 않는다.

첫 effect의 private admission은 대상 table의 object/generation과 같은 **모든 non-Free Job**을
거절한다. Row의 Active/RootRetained뿐 아니라 Commit 뒤 Prune·release가 남은 상태와 관련
Catalog Job도 포함한다. same-cut canonical 검산과 Job 부재에서 대상 행이 Committed·Undo 없음임을
확인한다. 현재 `semantic_row/leaf`·`transition/prepare`·`row_preflight`의 level0 제한 때문에
준비의 active debt 허용을 실제 effect 허용으로 그대로 확대하지 않는다. 다른 table의 원 부채는
위 credit 계산대로 보존한다. 이 한정은 첫 연결의 임시 경계이며08-1 완료 조건을 줄이지 않는다.
(2026-11-05: 이 한정은 [current-leaf 소비자 일반화](#current-leaf-consumers)로 대체됐다. 이제
allocator Reserved·모든 scope/table의 RootReserved 배제는 유지하며, 그 뒤 대상 table의
non-Free Catalog Job을 추가로 거절한다. Row의 Forward 이후 부채는 허용한다.)
최종08-1에서는 current-leaf 소비자를 일반화하고 이 한정을 제거한 뒤 Active INSERT/DELETE/UPDATE의
split→Commit/Abort/CLR/Prune와 후속 다단계 행 변경을 검증한다. SMO-P03의 무효과 active 준비는 유지한다.

private StructuralAuthority가 같은 owner/core/binding/fence·prefix/control·frame digest와
exact5 request/기존3또는4stamp/신규1또는2를 묶는다. pool은 선택의 active stamp·next generation을
content→registry 순서에서 다시 검사한 뒤 closed slot/pair와 Structural claim을 한 번에 둔다.
claim key는 exact5 request를 별도로 결속하며 mask만으로 다른 target을 승인하지 않는다.
선택과 claim 사이 변경은 무효과 Stale/Busy이고, 세대를 소비한 prestage cancel은 세대
tombstone을 보존한다. key의 미해결 Drop은 fence한다.

whole5 전용 pin/write continuation은 sorted target content를 잠그고 before stamp와 immutable
before bytes를 다시 대조한다. `enter_stage`를 첫 WAL 호출 직전에 실행하고 원 stage의 authentic
Unchanged만 취소를 다시 연다. complete requirement를 원 pending에 둔 뒤 private permit으로
5 after·Structural bank34(완료 descriptor/high-water/STABLE)·XOI·DPT와 신규 등록을 설치한다.
`birth == work.id == requirement.lsn`이며 모든 새 page의 counter는1이다. pair 또는 single 등록
공개 뒤에도 Structural ordinary gate는 유지한다. `RuntimeHistory::installed` 후 모든 guard를
반환하고 원 `wait_durable`을 guard0에서 기다린다.

`wait_durable`의 authentic 결과가 원 requirement를 confirms한 뒤 `control.durable`에 반영하고,
새 complete prefix의 LSN/end가 원 requirement와 같은지 확인한다. 그 뒤에도 claim/pending과
flush lease를 유지한 채 원 `validate_live`를 다시 실행한다.
원 replay가 scope4를 재계산하고 INIT provenance를 만들며 bank/Jobs/XOI를 대조한다. replay loan의
checked return 후 authentic DurableWal·완료 token·등록/DPT를 함께 확인하여 claim/closed만
해제한다. 등록과 소비한 generation은 보존하고 pending은 마지막에 비운다. 이미 완료된 SMO는
사용자 rollback 대상이 아니다. complete stage 뒤 checkout Busy 등 본래 Unchanged인 오류도
일반 취소로 되돌리지 않고 fence/CallerMustResolve로 승격한다. 단계 중 불명 결과·부분 설치·
panic·실패한 반환은 fence한다.

첫 effect 실행 전에 실제 타입의 source census를 준비한다. 별도 선택 상한12,464B와 prepare
상한33,372B를 단순 max만 하여 전체 F05라고 쓰지 않는다. 준비·선택/claim·whole write/stage/install·
guard0 durable/replay·finish/cancel 각 phase에서 Prepared, lease, key, 배열, Result와 깊은
decoder/registry/대기 호출이 함께 살아 있는 값을 계산하고 원 memory/native caller cap을 유지한다.

| ID | 고정 경계와 판정 |
| --- | --- |
| SMO-E01 | 실제 원 writer에서 committed/pruned4행의 RootSplit, 이어2행 leaf의 ChildSplit. 각5page와1/2 INIT·bank/XOI·DPT·등록·두 leaf 행 분포를 검산하고 후속 canonical replay 수락. (대상 table의 Active·Commit 후 미정리 Job 거절은 [current-leaf 소비자](#current-leaf-consumers)의 admission으로 대체됐다) |
| SMO-E02 | 선택 victim stamp/generation 변경은 claim 전 무효과 거절. 기존 대상 before stamp/bytes는 whole5 close 뒤 재검산하며 그때 처음 발견한 차이는 checked release→prestage cancel로 반환. 실제 cancel은 generation tombstone을 보존하고 stale key 재사용을 거절 |
| SMO-E03 | 원 flush의 진행 중 capture/selected DWB 또는 다른 binding·generation: claim 전 거절. RAM pin0만으로 lease를 발급하지 않음 |
| SMO-E04 | enter_stage 직후 원 WAL Unchanged Capacity: stage rejection proof·checked release·cancel, WAL/control 불변 및 후속 재진입 성공 |
| SMO-E05 | enter_stage 뒤 불명 WAL 오류·panic/Drop: fence, 일반 cancel 또는 성공 없음 |
| SMO-E06 | Root/Child 각각 첫 image, 전체 image, control, DPT 설치 뒤 fault: fence 및 역순 guard 반환. 부분 결과를 완료 token으로 쓰지 않음 |
| SMO-E07 | Root 첫 등록/두 등록 전후, Child single 공개 직후 fault 및 history installed 실패: ordinary gate 유지·fence, 신규 등록을 일반 읽기에 조기 공개하지 않음 |
| SMO-E08 | installed 후 guard0 durable 대기 중 일반 pin/load/capture 거절, 원 wait_durable 실패: fence. 성공 token만으로 finish 불가 |
| SMO-E09 | durable 후 canonical replay/INIT 대조·checked return 실패, finish 전 Drop: fence, pending을 먼저 지우지 않음 |
| SMO-E10 | 실제 WAL의 Root/Child complete stage 전후·설치 전후·durable/finish 경계에서 재시작. 독립 before/after 행 집합·트리/allocator/INIT·bank 검산, 새 동작 후 두 번째 재진입 |

이 ID는 실행 분모를 사후 줄이는 목록이 아니다. deterministic memory fault를 먼저 구현하고,
SMO-E10의 native cut/프로세스 입력과 독립 기대값은 native 실행 전에 별도 고정한다. 아직
없는 fault seam 또는 미도달한 cut은 NOT_RUN으로 남긴다. 실제 type census·효과 fixture를
검토하기 전에는 이 설계 채택만으로 F05나08-1을 완료로 표시하지 않는다.


실제 native 검증에서 복구 후 epoch 활성화가 `root.level == 0`을 요구하여 완료된
RootSplit의 READY 진입을 막는 것을 확인했다. 원 `history::validate_live`가 같은 replay의
전체 tree closure를 검산한 뒤, epoch seed는 Row tree·allocator의 owner/generation/kind와
실제 root level에 따른 RowLeaf/RowInternal decoder expectation을 확인한다. 새 traversal이나
별도 authority는 추가하지 않는다. 이 수정은 복구된 split의 READY 인계이며 다단계 Row CRUD
공급은 아니다. 기존 scalar packet 안에서 root kind를 보관하므로 source cap은 유지한다.


원 live flush도 새 RowInternal을 소비한다. RetentionFlusher의 kind 선택, 원 FlushIo의
capture/mapping 검산과 retirement 인계에서 Profile3·RowInternal level1..15를 허용한다.
원 canonical Retention authority·capture·kind/level/stamp/attempt 결속과 실제 DWB/홈 write는
유지한다. Allocator/Status 정책의 level0 조건은 바뀌지 않는다. 새 root의 dirty→Complete→clean
관측은 Child의 clean-victim 시험에 포함하며 다단계 Row 변경 지원으로 확대하지 않는다.


<a id="current-leaf-consumers"></a>
## current-leaf 소비자 일반화

상태는 **2026-11-05 구현·memory 시험 PASS / 독립 D2 ACCEPT(2회차)**이다. 사용자가 Codex 브랜치를
받아 08-1을 완료하라고 요청했고, 이 절은 그중 [효과 계약](#structural-effect)이 최종08-1 조건으로
남긴 current-leaf 소비자 일반화와 admission 한정 제거를 다룬다. 빈 leaf unlink·root shrink·
retirement opcode와 A의 자동 split 호출자는 이 절의 범위가 아니며08-1 잔여로 남는다.

**현재 leaf의 정의.** 한 Row의 현재 RowLeaf는 같은 cut에서 anchor의 현재 tree를 그 RowId의
big-endian key로 route하여 도달하는 leaf다. root level0이면 root 자체다. Job의 pair[1]은 Reserve가
그때 현재였던 leaf를 적은 hint일 뿐이다. Forward만 pair[1]이 여전히 현재 leaf임을 요구한다.
RootReserved root가 있는 동안 split을 거절하므로 Reserve와 Forward 사이에 구조 그룹이 끼지 않는다.
CLR·Prune·recovery는 pair[1]을 믿지 않고 route한 leaf에서 행을 찾는다.

**route 한 단계.** `tree_page::search::route_row_step`이 한 page를 기대 kind/level로 decode하고,
key가 부모가 준 bounds 안인지 확인한 뒤 RowInternal이면 child와 좁힌 bounds를 돌려준다.
`find_leaf_in_images`의 loop 본문과 같은 판정이며, 호출자는 각 page를 자기 짧은 read 아래에서만
읽는다. depth는 root level로 묶이고, 호출자가 anchor revision을 단계 사이에 고정한다.

| 소비자 | 일반화 |
| --- | --- |
| transition `table_leaf`/`leaf_source` | level0이면 anchor root, split tree이면 호출자가 route해 공급한 leaf image(anchor·root와 다른 table-owned dynamic page). Forward만 pair[1]과 같아야 함 |
| 예약 D (`leaf_reservation`) | 그 leaf에 RowId가 있는 table의 Retained root restore bytes 합. restore bytes가 0인 root(INSERT, 다른 scope의 root)는 조회하지 않음. 단일 leaf tree에서는 기존 table 합과 같음. 각 leaf 단계가 `typed_leaf` 뒤 자기 view decode 옆에서 계산하고(`leaf_reserved!`), DELETE·UPDATE의 `prepare_value`는 `value_source`가 돌아온 뒤 자기 never-inline frame(`value_reserved`)에서 계산 |
| A 그룹 관측 (`row/insert/group.rs`) | level0이면 root가 그 leaf, split tree이면 C가 같은 cut에서 route해 공급한 leaf(root·anchor와 다름). Forward만 pair[1] 일치 |
| C preflight (`row_preflight`) | `RowRouteCursor`로 anchor→RowInternal→leaf를 같은 residency 집합에 올리고 leaf를 source로 고정. 새 RowId는 가장 오른쪽 leaf로 route |
| C rollback 선택·live cleanup (`row_rollback_select`, `row_start/retire`) | CLR/Prune 대상 행의 leaf를 route. Prune의 route callee는 LOAD 단계와 max |
| recovery (`recovery_leaf`, `history/row::current_leaf`) | canonical arena에서 같은 route. `validate_live`는 pool route와 arena leaf를 대조 |
| B point/cursor (`plan4_apply/row/read.rs`) | anchor LOAD 뒤 page·level·revision·복사 bounds를 유지하며 각 RowInternal→leaf를 한 번씩 LOAD한다. level은 매 단계 정확히1 감소하므로 tree LOAD는 최대16회이고 allocator+anchor+교체 가능한 tree frame1개로 진행한다. LOAD와 pin 사이 다른 reader가 page를 퇴거시키면 무효과 Busy로 재시도한다. leaf 발급 시 anchor revision과 leaf의 bounds를 다시 확인. cursor는 leaf에 다음 key가 없으면 그 leaf의 upper bound로 다음 leaf를 route |
| lookup LOAD membership | RowInternal은 level1..15, 그 밖의 target은 level0. Index/Catalog internal은 일정09/10 |

B 읽기 operation의 `close_reads_for_retirement` gate가 split과 한 operation을 배타로 두므로,
operation 안의 여러 route는 같은 tree를 본다. 한 cursor의 step 사이에 split이 끼면 다음 step이
key로 다시 route한다. 이것은 SMO를 건너는 cursor/locator 인수(일정08-2)를 대신하지 않는다.

**admission.** 원 `split_row`의 canonical 준비는 기존대로 allocator Reserved와
**모든 scope/table의 RootReserved**를 거절한다. `weights::derive`와 candidate의 역방향 검산이
RootReserved를 `OrderMismatch`로 거절하므로, 뒤의 table별 검사까지 도달하지 않는다.
준비가 성공한 뒤 대상 table의 non-Free Catalog Job은 무효과 Unsupported로 추가 거절한다.
Retained·Releasing·terminal Row 부채는 허용한다. 선택 가중치는 기존대로 각 Retained root의
restore bytes를 그 행의 쪽에 둔다. 이전 문구의 “대상 table만”은 이 전역 준비 조건을
누락한 설명 오류이며, 이번 정정은 admission 동작이나 기존 Reserved 배제 계약을 바꾸지 않는다.

**source census.** tree codec 산정은 `tree_page::codec_source_bytes(page_codec)`가 소유하며 Row/Catalog는 page envelope 비용만 전달한다. immutable/live route는 typed 비교·상속 bounds·child 선택의 순수 helper를 공유한다. search frame은 search 모듈이 소유하고 history는 anchor/관측 frame만 더한다. 기존 보수적 search 항목은 유지하고 새 helper·복사 bounds 비용을 추가했다.

원 charge 44,304B는 바꾸지 않았다. B cursor.row_load는 34,572/34,586B(여유14B),
point.row_load 34,380B다. B route 단계(row_route)는 `route_row_step`이 품는 tree codec까지 세어
cursor 8,307B, point 8,115B다. C preflight는 여유184B, reserve는 여유499B 안이다. 예약 D는
`typed_leaf` 아래에서 돌지 않으므로 transition의 leaf·delete·reserve 단계는 D를 단계 frame의
callee로 센다. D와 그 Corrupt 판정은 이제 각 단계의 view decode 뒤(`prepare_leaf`에서는 lsn·pair 판정 뒤,
`prepare_value`에서는 `value_source` 뒤)에 오지만 출력 전이다. 두 결함이 겹칠 때 보고하는 오류 종류만
달라지고, 수락·거절되는 입력은 같다. S1B Row arm frame은 scope4 structural arm 26,420B가 기존 phase 최대보다 커서 그
값으로 고정했다. `check`는 structural bank 관측과 ATT/Job scan을 각자 never-inline frame으로
나눴고, 두 frame 모두 receiver·Profile·prefix 인자를 센다. 모두 source-value 회계이며 compiler
stack/RSS 측정이 아니다.

| ID | 고정 경계와 판정 |
| --- | --- |
| SPL-J01 | 한 거래가7..10을 INSERT(Retained4)한 상태에서 RootSplit 허용, 두 leaf 2/2. 같은 거래의 다섯째 INSERT는 오른쪽 leaf. Commit q2, drain 26 groups, Row Job0, point/cursor와 재시작(q2, CLR0)이 다섯 행을 읽음 |
| SPL-J02 | 7,8 commit 뒤 9..12 INSERT 상태에서 RootSplit, rollback의 InsertClr4가 route한 leaf에서 행 제거, drain17. 다음 INSERT(q3)는 오른쪽 leaf. 재시작이 7,8,13을 읽음 |
| SPL-J03 | 7..10 commit·RootSplit 뒤 DELETE 7·UPDATE 10→100 상태에서 오른쪽 leaf ChildSplit(1/1), rollback이 둘을 복원(drain9). 같은 변경의 Commit q3, DeletePrune·UpdatePrune(drain11), INSERT q4는 가장 오른쪽 leaf. 재시작 q4가8,9,100,11을 읽음 |
| SPL-J04 | split tree에서 승자 DELETE 8 Commit(정리 전)과 다른 Status의 패자 DELETE 7·UPDATE 9·INSERT 11, 그 옆 ChildSplit 뒤 파일 채취. 재시작이 승자 Prune과 패자 CLR3을 route한 leaf에서 실행, READY q3·Row Job0, cursor가7,9,10을 읽음 |
| SPL-J05 | table의 CREATE Catalog Job이 남아 있으면 split은 무효과 Unsupported. WAL·bank·Jobs 불변, fence 없음, B가 네 행을 읽음 |
| SPL-J07 | 실제 RootSplit→두 ChildSplit→internal RootSplit으로 level2·singleton leaf4를 만든다. 12-frame pool에서 별도 owner가 unrelated page9개를 pin하고 tree를 cold로 만든 뒤 point4개와 cursor의 End까지 검증 |
| SPL-J06 | Commit 이후 RootSplit으로 오른쪽 leaf로 이동한 INSERT, DELETE·UPDATE의 정리 전 파일로 재시작. Prune 세 종류가 현재 leaf를 찾아 READY q2/q3·CLR0·Row Job0, 같은 파일의 두 번째 재진입도 같은 행을 읽음 |
| SPL-B02 | 잘못된 leaf upper bound가 route key보다 작거나 같으면 InvalidProgress. page/operation 해제, output·cursor progress 불변 |
| SPL-B01 | B driver의 literal 두 leaf(10,20 / 30,40, separator 30). cursor가 20 뒤 왼쪽 leaf의 upper bound로 오른쪽 leaf를 route해 30·40을 돌려주고 끝남. 범위가 separator에서 끝나면 오른쪽 leaf를 route하지 않고 끝남 |
| SMO-E01 | 기존 Active·Commit 후 거절 단언을 위 admission에 맞춰 제거. 깨끗한 leaf의 Root→Child 효과 검산은 그대로 |

이 절은 08-1을 완료로 적립하지 않는다. 빈 leaf unlink·root shrink·retirement와 Capacity 시 A의
[자동 split 호출자](#automatic-split)가 남아 있다.

<a id="automatic-split"></a>
## A 자동 split 호출자

상태는 **2026-11-05 구현 / 독립 D2 3회차 ACCEPT, 필수 0**이다. 1차 D2(계약 초안)는 REJECT였고
필수 2건(F4 예산, A literal 시험)을 반영했다. 2회차는 그 둘의 해소와 설계 변경 둘(값 검사 발동, 필요 검산 뒤
부채 검사)을 받아들이고, 예약 D>0의 Reserve Capacity 경로에 실제 C 증거가 없다는 필수 1을 냈다. SPL-A10과
서술 정정으로 반영했고, [3회차 검토](../../module-gate/evidence/dev3-08-1-auto-split/review.md)에서 ACCEPT를 받았다. 사용자의 08-1 완료
요청에서 [current-leaf 소비자](#current-leaf-consumers) 다음으로 남은 제품 호출자를 다룬다. 이 절은 leaf가
찬 INSERT·UPDATE를 A가 split으로 이어 가는 경로만 정한다. 빈 leaf unlink·root shrink·retirement opcode는
이 절의 범위가 아니며 08-1 잔여로 남는다.

**발동.** A의 INSERT·UPDATE 행 하나의 시도가 다음 둘 중 하나로 끝나고, 그 preflight finish가 cleanup 오류
없이 끝났으며 `must_resolve`가 아닐 때만이다.

1. A의 값 검사(C validation lane 위의 `validate_insert`)가 `InsertError::Capacity`를 돌려줌. route한 leaf의
   여유(`leaf_free_bytes`, D 없음)가 `L+ROW_HEADER_BYTES+16`보다 작다는 뜻이다. 실측으로 꽉 찬 leaf는
   Reserve 전에 여기서 먼저 걸린다. L은 값의 canonical 길이로, schema가 이미 받아들인 값이므로 A가
   `canonical_values_len`으로 센다.
2. C의 Reserve가 `Before`로 정확히 `Unchanged(Prepare, Capacity)`를 돌려줌(`RetentionError::row_capacity`;
   `_`·Frame·다른 phase·effect는 포함하지 않는다). L은 `RowInputPlan::encoded_len`이다. 이 Capacity는 leaf
   공간(`check_candidate`, D 포함)과 WAL 상한(`check_progress_room`) 두 곳에서 나오므로, C가 아래 필요
   검산으로 둘을 가른다. Retained INSERT root는 D에 아무것도 더하지 않으므로, leaf 쪽 Reserve Capacity는 같은
   leaf에 Retained DELETE·줄어든 UPDATE root가 있어 D>0일 때만 난다(SPL-A10). D=0이면 찬 leaf는 1이 먼저 잡는다.

Preflight의 Capacity(Free Undo 없음, `row_high` overflow)와 그 밖의 거절·Fatal, Forward 이후 단계는
발동하지 않는다. DELETE는 공간을 쓰지 않으므로 해당하지 않는다. 발동하면 A는 L을 `full`에 남기고, 그
시도의 `Failure`는 그대로 쥔다.

**C 진입 `split_row_live`.** `MutationPort::split_row_live(owner, table: &CheckedTable, row: Option<RowId>, length, private, flusher) -> Result<RowSplitStep>`.
`row`는 UPDATE의 대상 RowId이고 INSERT는 None이다(같은 cut의 Root `row_high+1`). 다른 session·pool의
table은 Unchanged BindingMismatch다. 답은 셋이고, `Split`은 split 관측을 싣지 않는다(A는 같은 행을 다시
시도할 뿐이고, 결과 slot을 작게 두어 F4 예산을 지킨다).

| 답 | 뜻 |
| --- | --- |
| `Split` | RootSplit 또는 ChildSplit 한 그룹이 설치되고 frame이 durable. A는 같은 행을 preflight부터 다시 시도 |
| `FlushRequired(target)` | 거주 단계가 이 dirty victim의 flush를 먼저 요구. 준비·stage·pin 없음. A가 flush한 뒤 같은 split을 다시 부름 |
| `NotNeeded` | route한 leaf가 그 행과 예약 D를 이미 담음(C의 규칙). 아무 효과 없음. A는 그 시도의 실패로 행을 끝냄 |

shell은 `split_row`와 같다. health·origin·pool/flusher binding, `CheckedTable`의 core·binding·fence,
`close_reads_for_retirement`(owner Idle, read operation 없음)와 `catalog_progress` admission을 잡고, 모든
출구에서 checked 반환한다. statement view는 받지 않는다. split은 사용자 거래의 일부가 아니다. 비용으로,
매 split은 그 owner의 reads를 닫고 split frame 하나의 durable wait(fsync)를 한다. `NotNeeded`와 부채
Unsupported 답도 shell(모든 owner의 admitted read 대기)과 `catalog_progress`, route LOAD와 그 FlushRequired
flush를 치른다. SQL 계층은 `Stage::Split` Unsupported를 "split이 필요하지만 이 table에서는 아직 할 수 없음"으로
옮긴다.

본문(`split_live_admitted`)은 길이 검사와 두 never-inline 단계다.

0. **길이.** L이 `MIN_CANONICAL..=INLINE_ROW_BYTES`(5..=2048, RowReserve producer의 범위) 밖이면 무효과 Unchanged
   `RowSemantic(InvalidInput)`이다. 아무것도 읽지 않는다.
1. **거주와 선택(`split_resident`).** retire의 Prune route처럼 anchor를 LOAD하고(INSERT면 Root도 LOAD해
   `row_high+1`을 key로 읽는다), anchor의 현재 root부터 key를 route하며 각 RowInternal을 LOAD한다. route의
   LOAD는 anchor와 그 page만 보호한다. 경로의 page마다 "Row separator 하나를 더 받을 공간"
   (`search::separator_room`: `(len+1)·40 ≤ TREE_ENTRY_CAPACITY` 16,188. len은 `TreePageView::len`, metadata
   entry를 세지 않는 entry 수이고, 40은 Row internal cell 36B(4+8+24)와 slot 4B다. 경계는 len 404)을 적는다.
   깊이는 root level로 묶인다(≤15).
   - **필요 검산(`leaf_needs_split`).** route한 leaf의 typed read와 control Jobs로 `check_candidate`와 같은
     함수 `leaf_short`를 본다. INSERT는 `L+ROW_HEADER_BYTES+16`, UPDATE는
     `max(0, L+ROW_HEADER_BYTES+12−C)`(C는 그 leaf의 대상 cell). D는 그 leaf의 `leaf_reservation`, 여유는
     `leaf_free_bytes`다. `필요+D ≤ 여유`이면 `NotNeeded`. UPDATE 대상이 route한 leaf에 없어도 `NotNeeded`다.
     그래서 WAL 상한의 Reserve Capacity는 `NotNeeded`가 되어 원 실패가 그대로 남는다.
   - **split 부채(`check_split_debt`).** 필요가 확인된 뒤, split 집합을 읽기 전에 table의 RootReserved Row
     root나 non-Free Catalog Job을 보고 무효과 Unchanged Unsupported로 거절한다. 필요 검산보다 뒤에 두므로
     필요 없는 행은 부채가 있어도 `NotNeeded`이고, 필요한 행은 준비 없이 싸게 거절된다. `split_row_admitted`가
     준비한 cut에서 같은 검사를 다시 한다.
   - **대상(`search::split_target`).** leaf에서 위로, 부모가 separator를 받을 수 있는 가장 낮은 page를
     고른다. 그런 부모가 없으면 root다. root면 RootSplit, 아니면 그 부모와의 ChildSplit이다. 부모가 찬 경우
     위쪽을 먼저 나누고, A의 다음 시도가 다시 아래로 내려온다(잘린 cascade를 공개하지 않는다는 위 결정과
     같다). 첫 extent(64 page)에서는 부모의 child가 64 이하라 "부모가 참" 분기는 실제로 닿지 않고 SPL-A07의
     순수 시험으로만 덮인다.
   - **split 집합 거주.** anchor·대상·부모(RootSplit은 anchor·root)를 서로 보호하며 LOAD한다. allocator는
     frame 0이다. 그다음 INIT victim을 producer와 같은 함수(`structural_flush_victim`, 곧 `structural_candidates`의
     `retention_candidate_at`, clean 우선)로 미리 본다. 보호 집합은 producer의 targets에서 INIT를 뺀 것과
     정확히 같다(RootSplit {allocator, anchor, root}, ChildSplit {allocator, anchor, 대상, 부모}). 필요한
     수(Child 1, Root 2) 안에 dirty 후보가 있으면 그 frame의 target으로 `FlushRequired`다. 이 검사는 읽기만
     한다. 최소 pool은 이 집합과 INIT 수를 더한 frame이고, 그보다 작으면 LOAD나 victim 선택의 원 무효과
     거절이 난다.
2. **효과.** `split_row_admitted(table.handle, 대상)`를 그대로 부른다. admission(allocator Reserved·모든
   RootReserved 거절, 대상 table의 non-Free Catalog Job Unsupported, Retained·Releasing·terminal 허용),
   canonical 준비, victim 선택, 설치, durable wait와 그 거절은 [효과 계약](#structural-effect)과 위 절
   그대로다. 단계 1과 2는 같은 port 호출 안이므로 그 사이에 다른 owner가 residency를 바꾸지 못한다.
   `FlushRequired` 왕복 사이의 변경은 다음 호출의 단계 1이 처음부터 다시 보며, 그래도 놓친 dirty victim은
   2가 무효과 Busy로 거절하고 A가 행을 끝낸다.

첫 extent의 Free page가 모자라거나 root level 15에서 RootSplit하려 하면 producer의 Exhausted이고 C에서
`RowSemantic(Exhausted)`가 된다. leaf가 entry 1개뿐이면 그 전에 필요 검산이 `NotNeeded`를 낸다(가장 큰 행
2,232B와 그 D를 더해도 16,188 안이다). 모두 무효과다.

**A의 행 loop.** INSERT·UPDATE statement의 행 하나는 바깥 loop에서 돈다. 시도(`insert_row`·`update_row`,
preflight·flush·validate·Reserve…)가 실패하면 never-inline 단계 `split_leaf`가 `full`을 꺼낸다.

- `full`이 비었으면 split하지 않고 그 시도의 실패로 행을 끝낸다.
- `Split`이면 행의 split 횟수를 하나 늘리고 같은 행을 preflight부터 다시 시도한다. 한 행의 split은
  `MAX_ROW_SPLITS` = 16회다. 넘으면 마지막 시도의 실패로 행을 끝낸다. 진행성: split마다 key 쪽 leaf의
  entry와 D가 줄고, entry 1개 leaf에서는 언제나 `NotNeeded`이며, 부모 cascade는 깊이 15로 묶인다. 첫
  extent에서 깊이는 2 이하라 16회로 충분하다.
- `FlushRequired`는 preflight의 flush와 같은 `flush`(`try_flush`·`continue_flush`)로 처리하고 같은 split을
  다시 부른다. 횟수는 늘리지 않는다(flush마다 dirty page 하나가 clean이 되므로 끝난다). flush 실패는
  `Stage::Flush` 또는 `Stage::ContinueFlush`로 행을 끝낸다.
- `NotNeeded`는 그 시도의 실패(`Stage::Validate` Capacity 또는 `Stage::Reserve` Capacity)를 그대로 돌려준다.
- split 거절은 새 `Stage::Split`과 그 오류로 행을 끝낸다. C가 Fatal·CallerMustResolve면 `must_resolve`다.
- 행이 끝난 뒤의 statement 처리(`end_statement`의 Returned·RollbackRequired·MustResolve)는 지금과 같다.
  split은 행을 적용하지 않으므로 applied 수를 바꾸지 않는다. split 뒤 재시도가 다른 이유(예: preflight
  Busy)로 실패하면 그 오류로 행이 끝나고 split은 durable로 남는다.

**사용자에게 보이는 변화.** CREATE Catalog Job이 남은 table의 찬 leaf INSERT는 지금까지 `Stage::Validate`
Capacity였고, 이제 `Stage::Split` Unsupported다(둘 다 무효과). split이 필요하지만 그 table에서는 아직 할 수
없다는 실제 원인을 보여 주려는 선택이다. 소비자 inventory MCA05-V30("다음 split 거절")은 이 거절로
기대를 바꾼다. table의 CREATE Catalog Job을 live로 정리하는 제품 호출자는 아직 없으므로, 일정 10-1(table
일반 create·drain) 전에는 제품 table에서 자동 split이 늘 이 거절로 끝난다. 시험은 `retire_create!`로 그
부채를 먼저 정리한다.

**발견한 기존 편차(이 절 밖).** UPDATE의 값 검사는 새 cell 전체를 leaf 여유와 비교한다(`encode_input`,
`L'+184 > 여유`). 그래서 여유가 새 cell 전체보다 작은 leaf에서는 같은 크기·줄어드는 UPDATE와, 증가분+D는
들어가는 커지는 UPDATE도 `Stage::Validate` Capacity로 거절된다. [UPDATE 계약](dev3-row-update.md#known-deviation-validate-capacity)은
`entries+max(0, C'−C)+D ≤ 16188`을 허용하므로 이는 07의 편차다. C의 필요 검산은 계약대로 증가분 규칙을 쓰므로
이 경우 `NotNeeded`가 되고 원 실패가 남는다(거절 전에 위 shell·LOAD 비용을 치른다). 증가분이 안 들어가는
UPDATE는 split 뒤 성공한다(SPL-A04). 편차는 UPDATE 계약에 기록하고 수정은 별도 작업으로 다룬다.

**거래·rollback·복구.** split은 redo-only system 그룹이다. 같은 행이 나중에 거절되거나 statement·거래가
rollback·abort되어도 split은 남고, 그 행과 이후 행·CLR·Prune은 current-leaf 규칙대로 현재 leaf를 쓴다.
split은 statement·거래 중간(이미 Retained root가 있는 상태)에서도 같은 admission으로 허용된다. split 뒤·행의
Reserve 전에 crash하면 재시작은 완결 split만 REDO하고 그 행은 효과가 없다. 새 WAL 형식·recovery 단계는 없다.
D>0 때문에 생긴 split은 cleanup으로 D가 사라지면 불필요했던 것이 되지만 unlink가 없어 되돌리지 않는다.
첫 extent의 가용 page가 약 55개이므로 열린 거래가 많은 부하에서는 extent를 먼저 쓸 수 있다(08-1 잔여의
unlink·shrink에서 다룬다).

**F4 상한.** C가 `ROW_SPLIT_CALLER_BYTES = 6144`를 정의하고 `storage::disk`로 재노출한다(값은 preflight의
6,144와 같고, 들어가는 가장 큰 값이 아니다).

- C: 두 writer에서 `split_row_live`의 단계별 census(`structural_live::source_bytes`, SPL-A08)를 낸다.
  carrier(table source, tree·need·결과 slot, 참조) 1,352B, 거주 단계 36,900B(shell 진입 + carrier +
  `split_resident` frame + 가장 큰 callee: LOAD 본문, route, typed read, 부채의 Job decode, victim
  look-ahead), 효과 단계 37,438B(원 split 본문의 peak에서 `split_row`의 진입 slot을 live 진입 slot으로 바꾼 값).
  peak 37,438 + 6,144 = 43,582 ≤ 44,304(여유 722B)다. charge와 다른 F4 상수는 바꾸지 않는다.
- A: 호출 지점 `insert.split`·`update.split`은 `Bound::Split`(6,144), split 안의 flush
  `*.split_try_flush`·`*.split_continue_flush`와 값 검사 거절의 finish `*.validate_finish`는
  `ROW_WRITE_CALLER_BYTES`(11,520)다. `budget_tests`의 match에 split arm을 명시하고, `CALL_SITES`는 62다.
  바깥 행 frame은 `full`(Option<usize>)과 횟수(usize), 그 시도의 결과와 `split_leaf`의 결과 slot
  (`Result<bool, Failure>`)만 쥐고, split 호출과 그 flush는 `split_leaf` 단계에만 있다. 실측(autocommit/explicit): insert.preflight 5,716/5,483(여유 428/661),
  update.preflight 5,665/5,432(479/712), insert.split 5,388/5,155(756/989), update.split 5,161/4,928
  (983/1,216). MCA00-D12 live_insert 11,644 ≤ 12,288(여유 644). 상수를 바꾸면 두 쪽 census를 다시 통과한다.

| ID | 고정 경계와 판정 |
| --- | --- |
| SPL-A01 | `fill!`(autocommit INSERT 한 행씩 commit·drain)로 큰 행 7개가 한 leaf를 채움(q 2..8). 다음 INSERT의 값 검사가 Capacity, RootSplit 한 번 뒤 q 9 Committed. 두 leaf의 key 집합, 새 statement read, 재시작 read |
| SPL-A02 | 한 autocommit statement의 큰 행 15개가 중간에 RootSplit과 ChildSplit을 거쳐 모두 적용·q 2 Commit. leaf 3개 이상, key 순서, drain, read, 재시작 |
| SPL-A03 | 명시 거래 statement의 큰 행 8개가 8번째 행의 값 검사 Capacity에서 split(D를 만들지 않는 거래의 Retained INSERT root 7개 옆), Done 8. rollback 뒤 행은 없고 split(두 leaf, 빈)은 남음. 다음 INSERT q 2 Commit, read, 재시작 |
| SPL-A04 | 100B 행 + 큰 행 7개(q 2..9, 여유 271B). 100B 행을 큰 값으로 바꾸는 UPDATE가 값 검사 Capacity, split 뒤 q 10 Commit. 새 값 read와 재시작 |
| SPL-A05 | 18-frame pool(`fill!`이 통과하는 가장 작은 pool)에서 C가 직접 `FlushRequired`(WAL·Jobs·leaf 불변)를 답함. 같은 상태의 A INSERT가 flush 뒤 같은 split을 다시 불러 RootSplit·q 9 Commit, 재시작 read. 이 pool에서 split 뒤 새 statement의 table issue는 C의 table issue가 MissingTarget으로 거절하므로(split 전 17-frame에서도 같다) read는 재시작 뒤에만 본다 |
| SPL-A06 | 거절: CREATE Catalog Job이 남은 T의 찬 leaf INSERT는 `Stage::Split` Unsupported로 abort, leaf 불변. 같은 T에 C를 직접 부르면 들어가는 행은 `NotNeeded`, 찬 행은 Unsupported, WAL·Jobs·leaf 불변. 정리 뒤 들어가는 INSERT와 같은 크기 UPDATE는 `NotNeeded`, 길이 0·4·2049(`MIN_CANONICAL` 5와 2048 바깥)는 `RowSemantic`, 찬 행은 Split |
| SPL-A07 | 순수 함수: `separator_room(n)` ⇔ n+1 entry 인코딩 성공(경계 404), `split_target`이 부모 여유가 있는 가장 낮은 page, 모두 차면 root |
| SPL-A08 | C census: 위 단계별 값, 6,144 상수, peak + 6,144 ≤ 44,304 = charge (memory·native W). `spl_row_capacity_is_exactly_the_unchanged_prepare_capacity`, `spl_leaf_short_is_exact_at_the_boundary`(INSERT·UPDATE의 `필요+D = 여유`와 1B 초과) |
| SPL-A10 | 예약 D>0의 Reserve Capacity: 큰 행 7개(q 2..8) 뒤 명시 거래가 첫 행을 DELETE(tombstone 뒤 여유 2,612B, D = 2,228−180 = 2,048). C 직접 호출은 경계 380B 행(380+184+D = 2,612)에 `NotNeeded`, WAL·Jobs·leaf 불변. 같은 거래의 381B 행 INSERT는 값 검사(565 ≤ 2,612)를 지나 Reserve Capacity, RootSplit 뒤 Done 1, Commit q 9, 두 leaf, read, 재시작. C가 D를 0으로 읽으면 381B 행이 `NotNeeded`로 실패하므로 D 배선도 고정된다 |
| SPL-A09 | A literal: 값 검사 Capacity와 Reserve Capacity가 깨끗한 finish 뒤 split을 L과 함께 부름, Split 뒤 같은 행 재시도, FlushRequired는 flush 뒤 같은 split(미계수), NotNeeded는 원 실패, split 거절은 `Stage::Split`, Fatal은 `must_resolve`, 16회 상한, split flush 거절은 Flush stage, 뒤 행의 split, 재시도의 preflight Busy. Preflight Capacity, Reserve Busy·Unsupported, 거절된 finish, Fatal finish는 split하지 않음. MCA00-D04·D05의 호출 기록은 Capacity 뒤 `Split`(NotNeeded)을 더함 |

PR #54 후속 리뷰는 아래 신규 사례를 추가했다. 모두 실제 C/A/B 경로와 메모리 I/O double의 파일에서
새 recovery를 실행한다. native process crash·실제 fsync 검증을 추가한 것으로 해석하지 않는다.

| ID | 새 검증 경계 |
| --- | --- |
| SPL-REVIEW-01 | committed 큰 행 7개 위에서 live split만 완료하고 행을 재시도하지 않은 파일을 재시작. Q=8, CLR=0, 기존 값 7개·leaf 2개 유지, 8번째 행 없음 |
| SPL-REVIEW-02 | Active 거래의 큰 행 7개 위에서 live split만 완료한 파일을 재시작. Q=1, loser CLR=7, Row Job=0, 행은 없고 빈 leaf 2개 유지 |
| SPL-REVIEW-03 | 명시 거래의 앞 statement 7행 뒤, 다음 statement가 split·1행 INSERT 후 NULL 거절. 그 statement만 rollback, 앞 7행 유지, 후속 INSERT·Commit 성공(Q=2, 8행), 재시작 값·leaf 2개 검산 |

실행 identity와 독립 검토는 [후속 리뷰 증거](../../module-gate/evidence/dev3-08-1-auto-split/review-followup.md)를 따른다.

남은 D2 선택(시험 미작성): SPL-A05가 A의 split flush 호출을 직접 단언하는 관측, 최소 pool 경계 시험,
FlushRequired 왕복 사이 경합(`Stage::Split` Busy), SPL-A01..A04의
split 종류 직접 관측. 18-frame pool에서 split 뒤 table issue의 MissingTarget(SPL-A05)은 C table issue의 작은 pool
한계로 남긴다.

이 절로 08-1을 완료로 적립하지 않는다. 빈 leaf unlink·root shrink·retirement가 남는다.

<a id="empty-leaf-unlink"></a>
## 빈 leaf unlink·root 축소·retirement 등록

상태는 **2026-11-05 설계 / 독립 D2 3회차 ACCEPT, 필수 0 / U1·U2 구현·memory 시험 PASS([U1 결과](#unlink-u1),
[U2 결과](#unlink-u2)), U3..U4 구현 없음**이다. 설계는 1회차 CONDITIONAL(필수 3)과
2회차 CONDITIONAL(필수 1)을 반영했다([검토 기록](../../module-gate/evidence/dev3-08-1-unlink-design/review.md)). 08-1의 마지막
범위(3.06.1.c, T06-06)를 닫는 설계다. 08-1은 아래 U4까지 끝나야 완료로 적립한다.

2026-11-06 PR #55 후속 검토는 retire LSN과 snapshot q의 구분 및 U1–U3의 단계별 census 의무를 보강했다.
기존 구조 primitive를 대상으로 새로 작성한 두 시험의 결과와 미구현 경계는
[후속 검토 기록](../../module-gate/evidence/dev3-08-1-unlink-design/review.md#pr55-followup)에 둔다.

이 절이 대체하거나 확장하는 앞 서술은 다음과 같다.

- [결정과 범위](#결정과-범위)의 "Finished/잔여0 규칙을 그 opcode에 자동 적용하지 않는다": unlink 그룹의 owner도
  Finished·잔여0이다. 잔여(퇴역 page)는 아래 anchor 목록이 tree object의 것으로 기록한다.
- [System owner·회계](#system-owner회계)의 "unlink에서 나오는 retired page는 별도 잔여 owner가 필요하다": 그
  잔여 owner는 tree object이고 기록은 anchor 목록이다.
- [제안 F01](#제안-f01-scope4-frame)의 "native5·redo5"와 [bank](#structural-bank)의 "WAL은 Root84,744 또는
  Child84,640B": opcode 3·4의 수와 길이를 더한다. split의 값은 그대로다.
- [Base4 birth 관측](#base-structural-claims)의 "CREATE pair의 현재 상호 존재 규칙은 … 실제 unlink/retirement
  연결 때 따로 일반화한다"와 [현재 tree 전체 검산](#current-tree-closure)의 "최초 leaf 자체의 retirement와 그때의
  역사 pair 승계": 퇴역 page가 Allocated로 남으므로 기존 pair 규칙(`validate_pairs`)이 그대로 성립한다. 일반화는
  page를 실제로 Free로 만드는 3.03.4로 미룬다.
- split 공급도 두 곳이 바뀐다: candidate의 anchor encode가 목록을 보존하고, candidate의 소유 검산이 목록을
  받는다(아래 U1). [F05 절](#f05와-검증-동결-잔여)의 재검토 조건("unlink/retirement debt 추가")에 해당한다.
- [current-leaf 소비자](#current-leaf-consumers)의 "S1B Row arm frame은 scope4 structural arm 26,420B": U1에서
  split `audit`가 `RetiredList`와 항목 검산 frame을 세므로 그 arm이 26,452B(+32B)가 되었고 S1B 고정값도 그 값이다.
  live split peak(37,438B)과 복구 peak은 바뀌지 않았다([U1 결과](#unlink-u1)).

### 범위와 전이

실제로 비어 있는(cell 0개) non-root RowLeaf L을 tree에서 떼어 낸다. underfull merge·재분배는 하지 않는다.
L의 parent P에 따라 둘 중 하나다.

| 전이 | 조건 | 바뀌는 page | 퇴역 page |
| --- | --- | --- | --- |
| LeafUnlink(opcode 3) | P의 separator가 2개 이상 | anchor(revision+1, 목록 추가), P(L의 edge와 separator 하나 제거) | L |
| RootCollapse(opcode 4) | P가 root(level 1)이고 separator가 1개 | anchor(root := 남는 child S, level 0, revision+1, 목록 추가) | L, P |

- LeafUnlink는 기존 `prepare_internal_edit`의 RemoveChild를 쓴다. L이 child i>0이면 separator i−1을 지우고
  L의 범위는 왼쪽 child가 이어받는다. i=0이면 child 1이 leftmost가 되고 separator 0을 지운다. L이 비어
  있으므로 어떤 행도 범위를 옮기지 않는다. ancestor separator는 바뀌지 않는다.
- RootCollapse의 S는 L의 형제이므로 언제나 level 0 leaf다. S의 bytes는 바뀌지 않는다(page에 parent·sibling
  link가 없다). S도 비어 있으면 결과는 빈 root leaf, 곧 빈 table의 정상 표현이다.
- 하지 않는 경우(`NotNeeded`): L이 root, L에 cell이 있음, P가 non-root이고 separator 1개(남는 child를
  level이 다른 grandparent에 잇지 않는다. 이 L은 빈 leaf로 남으며 정상 상태다). 그래서 root level이 2 이상인
  tree는 낮아지지 않는다(RootCollapse는 root level 1→0만 한다). 그 축소와 non-root unary 재구성은 이 절 밖이다.
- 빈 leaf는 이미 정상 상태다(SPL-A03, SPL-REVIEW-02). cell 0개는 snapshot이 필요한 tombstone이 모두
  DeletePrune(오래된 snapshot 옆에서는 Busy로 보류)이나 INSERT CLR로 지워졌다는 뜻이므로 L에 대한
  Retained root·leaf 예약 D는 없다. 그래도 아래 준비는 이를 다시 검산한다.

### retired 목록: anchor의 XTR1 cell

퇴역 page는 allocator에서 `Allocated(g, table, kind)` 그대로 남는다(unlink는 Free가 아니다). 그 page가
고아 손상이 아니라 적법한 퇴역임을 anchor의 두 번째 cell이 기록한다. 기존 XTA1 cell 0은 바뀌지 않는다.

| XTR1 byte | 의미 |
| --- | --- |
| 0..4 | `XTR1` |
| 4..6 | version 1 |
| 6..8 | 항목 수 n, 1..=55(첫 extent dynamic ordinal 9..63) |
| 8..16 | zero |
| 16+40i.. | 항목 i: PageRef 24B, PageKind u8, level u8, zero 6B, retire LSN u64 |

- 항목은 (file, page_no) 엄격 증가 순서다. 목록이 비면 cell 1이 없다(n=0을 encode하지 않는다). 최대 cell은
  16+40×55 = 2,216B로 anchor page의 cell 여유 안에 든다.
- 항목 검산: file은 첫 file, page_no 9..63, anchor·현재 root·다른 항목과 물리 주소가 다름(incarnation만 다른
  alias도 거절), kind/level은 RowLeaf 0 또는 RowInternal 1..15, retire LSN은 0보다 크고 anchor pageLSN 이하.
  retire LSN은 퇴역시킨 그룹의 complete LSN(그 Structural owner의 id)이므로 Structural high-water 이하이고,
  퇴역 page는 그 뒤 쓰이지 않으므로 그 page의 pageLSN보다 크다. 이 LSN은 구조 그룹의 순서와 퇴역 이력을
  나타내며 snapshot의 commit sequence q와 비교하지 않는다. 후속 3.14.1 eligibility는
  [회수 계약 §5.1](dev3-task-14-reclamation.md#algorithms)의 published Q·등록 q로 구한 horizon과
  rollback/object/backup 소유, 마지막 영속 edge·transient pin·flush claim·DWB 조건을 실제 반환 시점에 다시
  검산한다. XTR1의 LSN이나 read gate 통과만으로 Free/reuse 권한을 만들지 않는다.
- 운반: `TreeAnchor`(lifetime 없는 Copy 값, 여러 census가 `size_of`로 센다)는 바꾸지 않는다. `decode_anchor`는
  cell 1개 또는 2개를 받아 cell 1의 문법만 검산하고, 목록은 별도 borrowed 함수
  `anchor::retired(input, expected) -> RetiredList<'_>`로 Base4 closure와 candidate만 읽는다. 따라서 root만 쓰는
  나머지 호출자의 반환 타입은 유지한다. 다만 XTR1 decoder의 borrowed view·항목·Result·반복 지역값은 새 source
  비용이다. U1에서 anchor codec과 split candidate·Base4 및 decoder를 호출하는 Row preflight·visibility 등
  영향받는 census의 중첩 frame 비용과 상한을 다시 검산한다. 타입 크기 불변을 census 불변의 근거로 삼지 않는다.
  cell 1은 Row tree anchor에서만 받는다(Catalog·Index anchor는 Corrupt). Profile2/Base3 경로는 목록이 있는
  anchor를 거절한다.
- writer: 지금 Row anchor를 다시 쓰는 곳은 split candidate 하나다. 그 anchor after(revision+1, RootSplit의 root
  교체)는 목록을 bytes 그대로 보존한다. CREATE와 Base INIT 재구성의 anchor는 초기 image라 목록이 없다.
- 3.03.4가 page를 Free로 만들 때 같은 그룹에서 항목을 지운다(이 절 밖). 그 Free 그룹의 WAL credit은 3.03.4
  admission의 몫이며 이 절의 owner는 잔여 0이라 credit을 예약하지 않는다. DROP TABLE은 목록의 page까지
  object의 모든 page를 회수한다(일정 10).

대안과 판단:

| 대안 | 판단 |
| --- | --- |
| Structural owner의 inventory에 잔여 page를 둔다 | system owner는 kind마다 하나다(같은 kind 둘은 inventory 검산이 거절). bank의 Structural 칸은 마지막 descriptor 하나이고 Finished·잔여0일 때만 교체되므로, 잔여가 있으면 이후 모든 split·unlink가 막힌다. 현재 plan4 bank의 Structural 행도 빈 inventory만 검산한다 |
| allocator에 Retired 상태를 더한다 | allocation 형식·모든 decoder·3.03.4 계약을 바꾼다 |
| 퇴역 page 자체에 표지를 쓴다 | 세 family의 page 형식을 바꾸고 퇴역 page마다 redo가 필요하다 |
| 바로 Free | 금지(3.14.1→3.03.4→3.14.2) |
| **anchor 목록(선택)** | unlink가 어차피 anchor를 바꾸므로 target이 늘지 않고, 퇴역 page의 소유가 그 tree object에 정확히 남는다. 그룹의 Structural owner는 split과 같이 Finished·잔여0이다 |

### scope4 frame (opcode 3·4)

XSG1 version 2·scope 4·outer `txn=None`·group header의 기존 칸을 쓴다. 새 Structural owner, descriptor
Finished·잔여0, consumed WAL = frame bytes는 split과 같다. 수는 opcode마다 고정이다.

| opcode | envelope record_count | native 수(group 124)·bytes(136) | redo 수(132)·bytes(152) | frame bytes |
| --- | --- | --- | --- | --- |
| 3 LeafUnlink | 2 | 3 · 384 | 2 · 32,688 | 160+192+1536+256+384+32,688+32 = 35,248 |
| 4 RootCollapse | 1 | 4 · 512 | 1 · 16,344 | 160+192+1536+256+512+16,344+32 = 19,032 |

native 128B는 split과 같은 배치이고 물리 target 순서다. 아래 "native role"은 native byte 0의 값이며, 같은 문서의
provenance claim role5/6(split right·new root)과 다른 이름공간이다. opcode마다 role 집합이 정확히 고정되고
role마다 kind·level·page를 core 칸에서 고른다(role bitmask는 u16으로 넓힌다).

| opcode | native role → core 칸, kind/level | 바뀜 |
| --- | --- | --- |
| 3 | 1 anchor → 32, TreeAnchor 0 | 예 |
| 3 | 4 P → 104, RowInternal 1 | 예 |
| 3 | 6 퇴역 leaf L → 56, RowLeaf 0 | 아니요 |
| 4 | 1 anchor → 32, TreeAnchor 0 | 예 |
| 4 | 8 퇴역 internal P → 104, RowInternal 1 | 아니요 |
| 4 | 6 퇴역 leaf L → 56, RowLeaf 0 | 아니요 |
| 4 | 7 survivor S → 80, RowLeaf 0 | 아니요 |

- INIT은 모두 0이다. 바뀌는 page는 before LSN·digest와 after digest를 갖고, redo ordinal은 redo record의 0부터
  dense index이며 redo record는 바뀌는 native의 물리 순서다. redo는 기존 `Patch(offset=72,length=16280)`다.
- 바뀌지 않는 page는 before LSN·digest가 canonical before와 같고 after digest = before digest, redo ordinal =
  0xFFFF이며 redo record가 없다.

| core byte | 의미 |
| --- | --- |
| 0..8 | XSM1, version 1, opcode 3 또는 4 |
| 8..32 | ObjectHandle |
| 32..56 | stable anchor |
| 56..80 | L |
| 80..104 | opcode 4의 S, opcode 3은 zero |
| 104..128 | P |
| 128..152 | before root ref |
| 152..160 | before anchor revision |
| 160..164 | L kind(RowLeaf)·L level 0·zero·before root level |
| 164..166 | P의 before separator 수 u16, 166..168 zero |
| 168..176 | 지운 child index |
| 176..184 | 이전 structural high-water |
| 184..192 | 지운 separator의 RowId |
| 192..200 | after 목록 길이 |
| 200..224 | zero |
| 224..256 | 교체 전 structural descriptor full digest, 최초이면 zero |

`bank::validate`는 opcode별 WAL(35,248·19,032)을 더 받는다. 두 값은 split의 84,744·84,640과 겹치지 않는다.
`decode_layout`는 opcode의 길이·record_count·native·redo 수와 bytes를 위 표대로 정확히 대조한다.

### 준비·효과·복구

**준비(같은 cut의 canonical before).** split candidate와 같은 입력(Catalog handle/schema, anchor와 목록, 전체
tree와 allocator 소유 집합, Jobs)에서 다음을 검산한 뒤에만 after와 frame을 만든다. candidate의 소유 검산
(`audit`)은 split과 공유한다.

1. allocator Reserved·어느 scope의 RootReserved Job도 없음(scope4 규칙). table의 non-Free Catalog Job도
   없음(`check_split_debt`와 같은 부채 규칙, 일정 10-1 전 제품 table은 Unsupported).
2. L은 현재 tree에서 도달하는 non-root RowLeaf이고 cell이 0개이며, L로 route되는 Retained Row Job이 없다.
3. P는 L의 실제 parent이고 위 전이 표의 조건을 만족한다. RootCollapse의 S는 P의 다른 child다.
4. revision+1, 목록 길이+1(또는 +2)·주소 alias·retire LSN 산술, 그리고 split admission과 같은
   `check_progress_room`(WAL·수정 counter·RetentionRoot revision 여유)과 XOI successor(`inventory_after`)를 첫
   효과 전에 검산한다. 바뀌는 page마다 counter+1이다.
5. after를 before 위에 겹친 전체 tree가 도달 집합 ⊎ 목록 = 소유 집합을 만족하는지 다시 본다.

**live 효과.** C 입구 `unlink_leaf_live(owner, table: ObjectHandle, key: RowId, private, flusher) ->
RowUnlinkStep {Unlinked(opcode), FlushRequired(target), NotNeeded}`. drain에는 statement view가 없어 `CheckedTable`을
발급할 수 없으므로 note의 ObjectHandle을 받아 같은 cut의 Catalog로 검산한다. key는 L로 route되는 RowId다(아래
note). split과 같은 `structural_shell`(read gate 닫음·CatalogProgress admission) 안에서 anchor LOAD, route
LOAD(anchor와 현재 page만 보호), L·P·S LOAD, 부채 검사를 하고 위 준비를 한다.

설치는 INIT이 없으므로 원 pool의 Structural INIT claim(`StructuralClaimKey`는 INIT 1|2·target 5개를 요구한다)을
쓰지 않는다. 기존 page만 바꾸는 기존 설치 본문 `apply_existing_n::<3|4>`에 새 `ExistingGroup::Structural` 변종을
더한다. after를 갖는 target은 opcode 3의 anchor·P, opcode 4의 anchor뿐이고, L(opcode 4의 P·S도)은 stamp만
대조한다. control part는 split의 `StructuralControlCommit`과 같은 규칙으로 bank 34의 descriptor·high-water·
STABLE과 XOI successor를 쓰고, mark/check staged 규칙도 같다. 이 본문이 stage·page/metadata/DPT 설치·
`history.installed`·`wait_durable`을 한다. durable 뒤 split이 하는 canonical replay(`validate_live`)는 INIT
provenance를 발급할 일이 없으므로 하지 않는다. 같은 cut의 canonical 준비와 frame 비교가 증명이다.

- 거절: stage 전 거절은 Unchanged, stage 뒤 실패는 fatal fence다. 퇴역 page는 shell의 read gate(
  `close_reads_for_retirement`) 때문에 다른 읽기 연산이 쥘 수 없다. shell은 다른 Owner의 열린 읽기 연산이 끝나기를
  기다린 뒤 gate를 닫는다(`wait_admitted_reads`). gate가 `Concurrency(Busy)` Unchanged로 거절하는 것은 이 Owner
  자신의 읽기가 열려 있거나 다른 gate가 이미 닫혀 있을 때뿐이다. 준비와 설치 사이에 target stamp가 바뀌면 설치
  본문의 `matches_stamp`가 `Frame(Stale)` Unchanged로 거절한다(split의 `RetentionCause::Busy`와 다른 경로).
- 퇴역 page는 Free가 되지 않으므로 dirty·flush claim·DWB 상태는 평소처럼 진행된다.

**복구.** canonical scope4 replay(`history/structure.rs`)가 opcode 3·4를 dispatch하여 같은 준비를 canonical
before에서 다시 하고, native before digest·redo·after 목록을 정확히 비교한 뒤 anchor·P after를 arena에
반영하고 Structural bank 칸을 새 descriptor로 바꾼다. INIT provenance claim은 만들지 않는다. 복구 관측
(`RetentionFrameObservation`)의 `changed`에는 바뀐 page만 넣는다. 퇴역 page를 `retired`에 넣지 않는다. 그 칸은
Free를 뜻해 arena와 claim을 지운다
(`plan4_recovery/analysis.rs`의 관측, `history/catalog.rs`의 arena·claim 정리). 그래서 퇴역 page의 기존 claim(CREATE pair 또는
role5/6)은 그대로 남는다. 복구는 새 unlink를 시작하지 않는다(빈 leaf는 정상 상태다).

**Base4 closure.** [현재 tree 전체 검산](#current-tree-closure)의 `reached == owned`를 `reached ⊎ retired ==
owned`로 바꾼다. 목록의 각 항목은 같은 object·incarnation·kind·level로 할당되어 있어야 하고(certificate
대조), RowLeaf 항목은 cell 0개, RowInternal 항목은 유효한 internal로 decode되어야 한다(그 child는 따라가지
않는다). 목록에 없는 고아, 도달하면서 목록에도 있는 page, 할당되지 않은 항목, 비지 않은 퇴역 leaf는
거절한다. revision ≤ 1+Structural high-water 규칙은 그대로다. Row actor·graph·D 적립은 도달한 leaf만 쓴다.
split candidate의 `audit`(그 `build`와 `verify_after`)도 같은 규칙과 항목 검산으로 바꾼다. split은 목록을
바꾸지 않으므로 `verify_after`는 after anchor의 목록이 before와 bytes로 같은지도 본다. 이를 빼면 첫 unlink 뒤
그 table의 모든 split이 고아 판정으로 거절된다.

**소비자.** B의 각 읽기 연산과 C의 current-leaf 소비자(CLR·Prune·Release)는 anchor에서 route하므로 퇴역
page에 닿지 않는다. 읽기 연산 도중에는 read gate 때문에 unlink가 끼어들지 않는다. Forward는 Reserve 때의
pair[1] leaf에 쓰지만, RootReserved가 하나라도 있으면 unlink가 거절되므로 그 leaf가 그 사이에 퇴역하지 않는다.
Job의 pair[1]은 그 뒤 퇴역 leaf를 가리킬 수 있으며 이미 이동 전 hint로 허용된다. 복구 정리도 pair[1] 대신
route한 leaf를 쓴다. 퇴역 page는 Allocated로 남으므로 split의 Free 선택에 들지 않는다.

### 호출자

C는 live DeletePrune 또는 INSERT CLR이 non-root leaf의 마지막 cell을 지울 때 (table, 지운 행의 RowId)를
RAM note로 남긴다(writer당 최대 8개, 가득 차면 새 note를 버린다. 재시작하면 사라진다). note는 권한이 아니라
hint이며 `unlink_leaf_live`가 모든 조건을 다시 검산한다. 부채가 있는 table의 note는 `unlink_next`가 조용히
버린다.

A의 정리 drain(`retire_finished`)은 정리 단계를 마친 뒤(`complete`) `unlink_next()`가 None일 때까지, 최대
`CLEANUP_UNLINKS` = 8회 `unlink_leaf_live`를 부른다.

- `FlushRequired`는 split과 같이 flush(`try_flush`·`continue_flush`) 뒤 다시 부르고 횟수에 넣지 않는다.
- `Unlinked`·`NotNeeded`는 note를 지운다.
- Busy 계열(Unchanged이고 cause가 `Concurrency(Busy)`, `Frame`의 Stale·Busy, `Visibility(Busy)`)은 note를 남기고
  이번 drain의 unlink를 멈춘다. `Retired`의 정리 결과는 그대로이고 `unlink_held`가 1이 된다.
- 그 밖의 Unchanged 거절은 note를 지우고 `RetireStop`(`Stage::Unlink`)으로 한 번 알린다(남기면 원인이 사라질
  때까지 매 drain이 같은 거절로 끝난다). Fatal은 같은 `RetireStop`이고 `must_resolve`다.
- `Retired`에 `unlinked`·`unlink_held` 수를 더한다.

F4: drain의 새 지점은 `cleanup.unlink_next`·`cleanup.unlink`·`cleanup.unlink_try_flush`·
`cleanup.unlink_continue_flush`이고 기존 caller 상한 `LIVE_ROW_CLEANUP`(4,096B)에 묶는다. 따라서 C의 unlink 본문
peak은 44,304 − 4,096 = 40,208B 이하여야 한다(split 효과 37,438B, live 정리 본문 39,564B). A 쪽도 빡빡하다:
autocommit 실행기 caller term을 넣은 drain census가 이미 3,979/4,096B(여유 117B)이므로 `Retired`의 새 필드와
unlink 단계는 inline되지 않는 별도 frame에 둔다. U3에서 split과 같은 단계별 census로 실측하고, 맞지 않으면
상한을 올리지 않고 이 계약을 D2로 다시 본다.

비용: unlink마다 준비의 canonical replay 1회와 fsync 1회다. drain당 최대 8회가 붙는다. 그룹 수에 대한 이차
비용은 [F6-a](../evidence/f6-wal-revalidation-survey-20261102.md)의 live 재검증 비용과 같은 성격이다.

**사용자에게 보이는 변화.** SQL 결과·오류는 같다. 다만 공간 사용이 달라진다. 퇴역 page는 3.03.4 전까지 첫
extent를 계속 차지하므로 unlink는 tree를 줄이고 route 비용을 줄이지만 page를 돌려주지 않는다. RootCollapse 뒤
다시 자라면 RootSplit이 새 page 2개를 쓰므로(예: 큰 거래의 split → rollback → collapse → 다시 삽입), 그런
부하에서는 unlink가 없을 때보다 첫 extent를 먼저 소진할 수 있다. 소진되면 [자동 split](#automatic-split)이
Capacity로 거절한다. 제품 table은 CREATE Catalog Job 때문에 일정 10-1 전에는 unlink도 하지 않는다.

### 시험 계획 (U1·U2 행의 결과는 [U1 결과](#unlink-u1)·[U2 결과](#unlink-u2), 나머지 NOT_RUN)

| ID | 경계와 독립 판정 |
| --- | --- |
| UNL-P01..P04 | XTR1 codec: 독립 literal(n=1, 2, 55), n=0·56 거절, 순서·alias·file/page_no·kind/level·LSN 거절, Row가 아닌 anchor의 cell 1 거절, cell 1개 anchor의 기존 bytes 불변 |
| UNL-P05..P08 | 순수 candidate: child index 0·중간·끝의 P after bytes를 독립 XBT1 조립과 비교, RootCollapse의 anchor after, 빈 S, NotNeeded 세 경우, 비지 않은 L·stale revision·alias 거절 |
| UNL-P09..P10 | split과의 결합: 목록이 있는 tree에서 RootSplit·ChildSplit 준비 수락과 anchor after의 목록 보존(bytes), 목록 밖 고아·목록을 바꾼 split after 거절 |
| UNL-F01..F04 | opcode 3·4 frame의 독립 section serializer 대조(제품 emitter 미사용), record_count·native/redo 수·role 집합·redo ordinal·퇴역 after≠before·퇴역 redo·길이 변조의 canonical 거절, bank validate의 opcode별 WAL |
| UNL-B01..B06 | Base4: 목록의 leaf/internal 수락, 목록 밖 고아, 도달+목록, 미할당 항목, 비지 않은 퇴역 leaf, wrong incarnation/kind 거절 |
| UNL-B07..B09 | CREATE leaf(pair[1])를 퇴역시킨 Base4의 `validate_pairs`와 재시작, unlink descriptor가 bank에 있고 그보다 오래된 role5/6 claim이 있는 Base4, unlink 뒤 base 재작성(BaseOnly·다음 rebase)의 목록·claim 보존 |
| UNL-R01..R03 | canonical replay의 opcode 3·4 수락과 재시작 read·leaf 수·목록, frame 변조 거절, 연속 split→unlink→split의 bank high-water |
| UNL-R04 | (U2에서 더함) unlink replay arm·candidate·frame의 source 회계와 그 독립 목록, scope4 회계 = split·unlink 중 큰 쪽 |
| UNL-L01..L08 | live: 중간 leaf의 행을 모두 DELETE·commit·drain → LeafUnlink, leftmost leaf, RootCollapse, 두 child가 빈 root → 빈 root leaf, non-root unary는 NotNeeded로 남음, rollback한 INSERT의 CLR note → unlink(SPL-A03 상태), CREATE Catalog Job table은 note 버림, unlink 뒤 split이 퇴역 page를 쓰지 않음. 각각 재시작 read. 재시작 read는 WAL의 unlink frame을 지나는 제품 rebase(`capture_base`)의 목록·claim 보존(U2에서 넘어온 B09 부분)도 본다 |
| UNL-L09..L15 | 작은 pool의 FlushRequired 왕복, 다른 Owner의 열린 읽기를 shell이 기다린 뒤(`ReadGateWaiting` test point) 읽기가 끝나면 unlink, 준비와 설치 사이 target stamp를 바꾸는 seam의 `Frame(Stale)` Unchanged가 note를 남김(Busy 계열), root level 2 tree에서 non-root P(separator ≥2) 아래 LeafUnlink와 지운 separator·이웃 경계 key의 point/cursor 읽기, 가장 오른쪽 leaf unlink 뒤 새 INSERT의 route, 오래된 snapshot이 DeletePrune을 Busy로 잡는 동안 unlink 없음·끝난 뒤 unlink, C 직접 호출의 RootReserved·allocator Reserved 거절 |
| UNL-A01..A03 | A literal(drain 순서·상한 8·FlushRequired 미계수·Busy 계열·그 밖의 Unchanged·Fatal·`Stage::Unlink`), 9번째 note 버림, census(C peak + 4,096 ≤ 44,304, A 새 지점) |
| UNL-C01..C03 | U1: XTR1 decoder·split candidate·Base4와 영향받는 호출자의 source census, U2: opcode dispatch·canonical replay·base capture census, U3: writer의 8개 RAM note를 shared owner/port charge에, drain 결과 필드를 caller frame에 포함한 산정. fresh/recovered writer 양쪽에서 각 단계의 실제 타입·중첩 호출이 기존 charge 안에 드는지 검산하며 상한 증가는 별도 D2 |
| UNL-N01..N06 | native fault cut: stage 전후, sync, install, DWB/home, CURRENT 공개와 reopen. 각 cut 뒤 tree·목록·allocator 일치 |

기대값은 typed 정렬 목록·독립 bytes로 만들고 제품 encoder·탐색 결과로 만들지 않는다. AGENTS.md에 따라 새
시험과 함께, 단계마다 바뀌는 코드에 닿는 기존 시험(U1: split candidate·canonical 비교, anchor codec, Base4 closure,
`decode_layout`, bank, U2: scope4 replay, U3: drain·census)만 선택해 돌리고 전체 suite는 돌리지 않는다.

### 구현 단계

| 단계 | 내용 | 시험 |
| --- | --- | --- |
| U1 | XTR1 codec과 `anchor::retired`, split candidate의 anchor 보존과 `audit` 변경, 순수 unlink candidate, opcode 3·4 producer/observer, bank·`decode_layout`, Base4 closure·영향받는 census | UNL-P·F·B·C01 |
| U2 | canonical replay dispatch, 복구 관측의 changed/retired 구분, base capture의 목록·census | UNL-R·C02 |
| U3 | `ExistingGroup::Structural`·live 효과·RAM note·A drain·census | UNL-L·A·C03 |
| U4 | native fault cut | UNL-N |

각 단계는 독립 D2와 PR 하나다. U4가 끝나면 08-1(2단위)을 완료로 적립한다. 이 절의 설계 부분은 어느 단계의
PASS도 주장하지 않는다. 단계의 결과는 아래 각 단계 절에만 적는다.

<a id="unlink-u1"></a>
### U1 결과

상태는 **2026-11-05 구현·memory 시험 PASS, 2026-11-06 [UNL-C01 census](#unlink-u1-census) 보강, 독립 D2 ACCEPT·필수 0**이다.
D2는 구현을 1회차 CONDITIONAL(필수 1) 뒤 2회차 ACCEPT, census 보강을 3회차 CONDITIONAL(필수 1) 뒤 4회차 ACCEPT로
받았다([최종 검토 기록](../../module-gate/evidence/dev3-08-1-unlink-u1/review.md), 앞 회차는 같은 폴더의
`review-r1.md`..`review-r3.md`). 제품 경로는 아직 이 코드를 부르지 않는다
(replay dispatch는 U2, live 효과·호출자는 U3). 그래서 사용자에게 보이는 동작은 바뀌지 않는다.

구현:

- `anchor.rs`: XTR1 `RetiredPage`·`RetiredList`(`find`, 물리 순서 병합 `extend`), `retired(input, expected)`,
  `encode_listed_anchor`(빈 목록이면 `encode_anchor`를 부른다). `encode_anchor`는 main의 함수 그대로다(본문 같음,
  doc만 추가). `decode_anchor`는 cell 1·2개를 받고 cell 1을 위 항목 규칙으로 검산한다. 검산은 `decode_page`가
  돌아온 뒤 부르는 별도 함수 `check_list`가 하므로 root만 읽는 decoder frame에 늘어나는 지역값은
  그 Result 하나다. `retired`는 anchor 전체를 decode한 뒤 같은 입력의 envelope를 다시 검증해 cell 1을 빌린다.
  목록이 있으면 anchor revision이 2 이상이어야 한다(목록은 unlink 그룹이 revision+1과 함께만 쓴다). 목록이 나쁜
  encode 입력은 InvalidInput이다.
- `semantic_row/structure.rs`·`bank.rs`: `Operation::{LeafUnlink=3, RootCollapse=4}`, opcode별 frame bytes,
  bank의 consumed WAL 35,248·19,032. bank 관측 이름 `FinishedSplit`을 `FinishedStructural`로 바꿨다(동작 같음).
  split의 `decode_core`·`init`은 opcode 1·2만 받고 3·4는 Unsupported다. split의 `decode_layout`은 split 길이
  (84,744·84,640)만 받으며(다른 길이는 Corrupt) 그 길이의 opcode 3·4는 Unsupported다. opcode 3·4의
  정확한 대조는 새 `structure::unlink::decode_layout`(길이·record_count·native/redo 수·role 집합·role별 core 칸과
  kind/level·redo ordinal dense·바뀌지 않는 native의 0xFFFF와 after=before digest·redo target)이 한다. U2의
  replay dispatch는 frame 길이(두 문법의 길이는 겹치지 않는다)로 둘 중 하나를 고르고, 고른 decoder가 opcode를 다시
  대조한다. finished descriptor 검산은 두 쪽이 같은
  `validate_finished_owner`를 쓴다.
- split candidate: anchor after가 `encode_listed_anchor`로 목록 bytes를 보존하고, `audit`이 `reached ⊎ retired ==
  owned`와 항목 검산(never-inline `retired_pages`: 같은 object·incarnation·generation·kind로 Allocated, 도달하지
  않음, 그 kind/level로 decode, pageLSN 0 아님·retire LSN 미만, leaf는 cell 0개)을 한다. `verify_after`는 after
  목록이 before와 bytes로 같은지 본다.
- `structure::unlink::candidate::prepare`: 위 [준비](#준비효과복구) 1..3과 5, 4의 revision·목록 길이·alias·
  retire LSN 산술을 한다(`check_progress_room`·`inventory_after`는 C의 live 준비로 U3). 결과는
  `Prepared::{Unlink, NotNeeded}`이고 after는 anchor·P(opcode 3) 또는 anchor(opcode 4)다. 목록 길이+퇴역 수 > 55는
  Exhausted로 거절하지만, 첫 extent의 dynamic page가 55개이고 anchor·root가 그중 둘을 쓰므로 실제 tree에서는 닿지
  않는다(codec의 n=56 거절은 UNL-P02).
- `structure::unlink::frame::{produce, observe}`: split의 finished descriptor·native 배치·redo cursor를 공유하는
  opcode 3·4 producer/observer. observer는 같은 cut에서 준비를 다시 하고 bytes로 비교하며, `NotNeeded`인 입력의
  frame은 IdentityMismatch다.
- Base4 `current_tree::observe`: [Base4 closure](#준비효과복구) 규칙. source 회계에 `RetiredList`·`RetiredPage`
  각 2개를 더했다.
- fixture: `tests/fixtures/module-row-format/current-tree/author.py`가 제품 코드 없이 `unlink-leaf.xrb`(ChildSplit
  85로 생긴 빈 leaf 19를 90에 LeafUnlink, 목록 [19], revision 5)와 `unlink-collapse.xrb`(RootSplit 70이 CREATE
  leaf 11을 비우고 90에 RootCollapse, root 16 level 0, revision 3, 목록 [11 leaf, 17 internal])를 쓴다. 기존
  fixture bytes는 같다.

<a id="unlink-u1-census"></a>
UNL-C01 source census(2026-11-06, PR #55 후속 보강 반영). 타입 크기가 같다는 것을 근거로 쓰지 않고, XTR1이 더한
지역값·중첩 호출을 decoder 경로마다 세어 기존 census의 page 항 자리에 넣었다. `anchor.rs`의 산정 함수:

| 함수 | 쓰는 곳 | 값(memory·native 같음) |
| --- | --- | --- |
| `decode_source_bytes(page)` | root만 읽는 기존 호출자. 자기 anchor 값은 그대로 세고 page 항을 이것으로 바꾼다: `check_list` Result 2개 + max(page, `check_list` 586B) | page 938B(page codec)·1,869B(tree codec)·786B(Undo decode 항)에서 page + 2B |
| `decode_increment_bytes(page)` | 중첩 decode를 허용 bytes 안에 이미 든 평평한 합(Catalog read guard, plan4 Catalog lookup codec, genesis 둘, Base Catalog graph) | 2B |
| `retired_source_bytes(page)` | 목록을 읽는 U1 코드(Base4, split candidate의 `audit`·`build`): `retired` 전체(frame + decoder 전체 frame 703B + 위 callee, 또는 두 번째 page decode) | Base4(page 2,240B) 3,609B, split `audit`·`build`(page codec + SHA scratch 2,474B) 3,843B |
| `listed_encode_source_bytes(page)` | split candidate `build`의 anchor after | page codec + SHA scratch 2,474B에서 2,998B |
| `extend_source_bytes()` | U1의 unlink candidate(목록 병합). 그 candidate·frame은 시험만 부르고 제품 호출자가 없어 이 단계에는 census가 없다. U2 replay·U3 live census가 이 항을 쓴다 | 409B |

`check_list`(586B)는 그 walk(408B)를 포함해도 기존 호출자가 넘기는 page 항(938·1,869·786B)보다 작아서, root만
읽는 경로의 증분은 Result 하나다(`size_of` 1B를 이 census들의 관례대로 두 번 세어 2B). UNL-C01 시험 둘이 세 page
항 모두에서 이를 확인한다. `check_list`는 `#[inline(never)]`라 machine frame에서도 decoder와 따로다. 기존 census에 넣은 곳: C
`row_preflight`(anchor_decode, `table_root`), history `current_leaf`·`table_source_bytes`, visibility epochs,
Catalog read guard, B 읽기(route read helper, schema 항), plan4 Catalog lookup codec, genesis 둘, semantic_row
transition `anchor()`(`table_leaf`), semantic_catalog `catalog_image`, Base Catalog graph relations, Base4
`current_tree`, split candidate(`table`·`retired_chain`·`build`). Catalog 쪽 decode도 `check_list`를 지나므로(목록이
있으면 Corrupt) 같이 셌다. `encode_anchor`는 바뀌지 않았으므로 그 호출자(CREATE pair `initialize_pair`, genesis,
Base INIT 재구성)에는 항을 넣지 않았다.

바뀐 값(작성자 checkout, U1 최종 code, 위 env 자리표시): 기존 상한은 하나도 바꾸지 않았고 모두 안에 든다.

| 항목 | 전 | 후 |
| --- | --- | --- |
| C Row lifecycle 본문(S2W-C03 고정값, memory·native) | 40,881 | 40,883(BEGIN 2,816과 합쳐 charge 44,304에 여유 607 → 605B) |
| preflight 본문 / 합계(charge 44,304) | 37,384 / 44,120 | 37,386 / 44,122 |
| producer_lane·validation_lane 본문 | 23,505·14,656 | 23,507·14,658 |
| live Row 정리 본문(LIVE_ROW_CLEANUP 4,096과 합쳐) | 39,564(여유 644) | 39,566(여유 642) |
| 표 발급 CatalogFull charge / Pending 옆 여유(memory·native) | 34,586 / 52·44 | 34,588 / 50·42 |
| 복구 cleanup·control·select·clr_lane 본문 | 39,424·37,262·37,532·27,985 | 각 +2 |

S2W-C03은 "Row lifecycle이 바뀌지 않음"을 정확한 값으로 고정하는 시험이라 기대값을 40,883B로 고쳤다(늘어난 2B는
위 Result). [Begin 상한 계약](dev3-module-boundaries.md#begin-statement-ceilings)의 여유 607B와 그 앞 기록들의
40,881B는 당시 값이다. history `table_source_bytes`는 수용된 식(search frame 옆에 tree 또는 Catalog codec)을
그대로 두고 anchor decode를 그 옆의 다른 갈래로 더했다(8,748B 그대로).

미결(U3에서 다시 본다): A 생산자의 group observer(`row/insert/group.rs`)가 부르는 `decode_anchor`의 중첩 비용은
이 단위 전부터 어느 census에도 이름으로 없다. A의 생산자 inventory(`insert.rs` `local_source_carrier_bytes`,
CLR_PRODUCER 19,456B에 19,436B, 여유 20B)는 문서대로 callee를 세지 않는 carrier 목록이고("shared codec locals …
still require the root's source census"), 그 root census가 이 decode를 항으로 갖지 않는다. XTR1이 그 callee에 더한
것은 위 Result와 page 항 이하의 `check_list`다. 이 공백은 XTR1이 만든 것이 아니지만 UNL-C01의 "영향받는 호출자"이므로,
A의 census를 다루는 U3(UNL-A03·C03)에서 decoder 전체 frame과 page codec을 A 쪽 항으로 세어 본다. 기존 상한 안에
들지 않으면 상한을 임의로 올리지 않고, 올리려면 별도 D2·사용자 결정을 거친다.

시험(모두 PASS, memory). 경로는 `cargo test -p xadb-engine --lib -- --list`의 libtest 경로다.

| ID | libtest 경로 |
| --- | --- |
| UNL-P01..P04 | `storage::anchor::tests::unl_p01_*`..`unl_p04_*` |
| UNL-P05..P10 | `storage::semantic_row::structure::weights::tests::candidate::unlink::unl_p05_*`..`unl_p10_*` |
| UNL-F01..F03 | `storage::semantic_row::structure::weights::tests::candidate::unlink::frame::unl_f01_*`..`unl_f03_*`(제품 emitter 없이 쓴 section serializer, F02 변조 14개) |
| UNL-F04 | `storage::semantic_row::structure::bank::tests::unl_f04_*` |
| UNL-B01, B07..B09 | `storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_b01_*` |
| UNL-B02..B06 | `storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_b02_b06_*`(변조 6개, 모두 IdentityMismatch) |
| UNL-C01 | `storage::anchor::tests::unl_c01_xtr1_codec_cost_per_decoder_path`(page codec·tree codec 항), `…retention::read::catalog::tests::unl_c01_undo_decode_term_covers_the_list_check`(Undo decode 항), 그리고 위 census 표의 값을 찍고 상한과 대조하는 기존 시험(S2W-C01..C06, S3-C01, S4-C01, F4 C commit·rollback·begin, MBC40-V09, f4_row_operation, live_row_retirement, repro_full_read_beside_pending, writer gate floors, recovery_source_·retention_source_·runtime_core_source_·g5_source_·plan4_source_census, row_read_census) |

계획과 다르게 남긴 것:

- UNL-P08의 Catalog 부채(non-Free Catalog Job → Unsupported)는 `check_debt`로 구현했으나 이 단계의 독립 시험이
  없다. U2에서 non-Free Catalog Job 하나를 넣은 단위 시험(NotNeeded보다 먼저 거절)을 더하고, U3의 UNL-L07이 CREATE
  Catalog Job table로 다시 본다. P08은 RootReserved Job·allocator Reserved(OrderMismatch), stale·alias·non-leaf
  source, revision 소진(Exhausted), high-water 불일치를 본다.
- UNL-B07의 재시작과 B09의 BaseOnly·다음 rebase capture는 복구 경로가 필요해 U2(UNL-R)로 미룬다. U1의 B07은
  퇴역한 CREATE leaf를 가리키는 Job pair[1] hint가 그대로 수락되는 것을, B09는 Base4 decode→encode가 목록·퇴역
  image·claim을 bytes 그대로 남기는 것을 본다. B08은 bank 33의 unlink descriptor(owner 90) 옆에 그보다 오래된
  split claim(70, 80, 85)이 있는 Base4의 수락이다.
- S1B Row arm frame 고정값은 26,420B에서 26,452B가 되었다(위 대체 목록). 그 밖에 바뀐 회계 고정값은 없다.

실행한 명령(작성자 checkout, U1 최종 code. 아래 `<hex>`는 64자 자리표시 hex이고, 세 XADB_CATALOG_*_SHA256에
준다. AGENTS.md에 따라 바뀐 코드에 닿는 시험만 골랐고 전체 suite는 돌리지 않았다):

| 명령(`cargo test -p xadb-engine --lib -- …`) | 결과 |
| --- | --- |
| `storage::anchor::tests:: storage::semantic_row::structure:: storage::plan4_recovery_base::row` | 113 passed, 0 ignored(UNL 16개 모두 여기 있다) |
| `retention::history::structure:: retention::recovery::structural_ catalog_live::structural:: plan4_recovery::analysis::structural_bank_tests plan4_recovery::tests::row_gate:: retire::driver::structural_ split_rows::` | 59 passed, 1 ignored(`structural_effect_native_worker`: native controller가 부르는 worker). 바뀐 `structural_prepare.rs`·`history/structure.rs`·`structural_effect.rs`의 시험, S1B census, SPL-A08, live split 호출자 SPL-A01..A06·SPL-J01..J07을 포함한다 |
| XADB_PLAN4_RECOVERY_*_SHA256에도 `<hex>`를 주고 `plan4_recovery::tests::recovery_source_` | 2 passed(바뀐 Base4 `current_tree::source_bytes`가 들어가는 recovery 자원 회계) |

`storage::plan4_recovery_base::tests::`의 XRB 시험 13개는 [module-gate 정책](../policies/module-gate.json)의
`evidence.executor_only` case다(XADB_XRB_INCLUDE와 frozen fixture 디렉터리·index hash를 executor만 준다). CI는
이를 돌리지 않는다. 바뀐 Base4 decode를 지나므로 작성자가 executor 역할로 직접 돌렸다: fixture
`tests/fixtures/dev3-plan4-recovery-base/vectors/p1`, `XADB_XRB_INDEX_SHA256` = 그 `raw-index.tsv`의 sha256,
source·ELF·input hash는 자리표시, 시험마다 자기 ID 전부를 `XADB_XRB_INCLUDE`로 주고 `--exact`로 하나씩 실행했다.
결과:

| 시험 | 결과 |
| --- | --- |
| `xrb_canonical_snapshots` C001..C015, `xrb_section_rejections` S001..S016, `xrb_global_census_rejections` G001..G024, `xrb_provenance_and_bindings` I001..I008, `xrb_encoder_literals` E001..E008, `xrb_partial_source_layout` ST001 | 모두 XB_PASS |
| `bridges::xrb_catalog_bridge_literals` BC001..BC006, `xrb_allocation_bridge_states` BA001..BA024, `xrb_reserved_terminal_adjacency` BB001, `xrb_borrowed_inventory_bridge` BI001..BI008, `xrb_bridge_source_layout` BST001 | 모두 XB_PASS |
| `xrb_header_rejections` H002..H016 | XB_PASS(`cases()`는 첫 실패에서 멈추므로 H001 뒤의 15개는 `XADB_XRB_INCLUDE=XB-H002,…,XB-H016`로 따로 실행했다) |
| `xrb_header_rejections` H001(header version 2를 Unsupported로 기대) | **XB_FAIL**("invalid base accepted"). main `1c9486a`의 같은 명령에서도 같은 위치(`plan4_recovery_base_tests.rs:295`)로 실패하므로 U1 전부터 있던 실패다. 원인은 분석하지 않았다(`read_header_format`은 Row가 아닌 mode에서 version 2를 V2System으로 받는다). 이 단계에서 고치지 않고 별도 작업으로 넘겼다(재현: 위 fixture·env로 `XADB_XRB_INCLUDE=XB-H001 cargo test -p xadb-engine --lib -- --exact storage::plan4_recovery_base::tests::xrb_header_rejections`) |
| `xrb_s5_reserved_header_bytes` | `#[ignore]` 시험이라 실행되지 않음 |

UNL-C01 보강과 그 D2 수정 뒤(`4ef9577`의 code) 다시 실행한 것: 위 env에 더해 PLAN4_GENESIS·PLAN4_LIVE·PLAN4_RECOVERY·
G6·G6RC·CATALOG의 source·ELF·input hash와 ALLOCATION·DIRTY·MIXED_OWNER source hash에 자리표시 hex를,
`RUST_MIN_STACK`에 33554432를 주었다. 위 세 명령의 시험, 바뀐 census 파일의 시험, census 표의 기존 시험과 UNL-C01
둘(합 578개)을 `--exact`로 실행했다: **570 passed, 1 failed, 7 ignored**. 실패 하나는 executor 전용
`plan4_pages::tests::g5_bind_boundaries`다(B08이 `XADB_PLAN4_G5_CAPACITY_PADDING`을 요구한다). 그 값을 처음
수락되는 2048로 주고 따로 돌리면 16/16 PASS다. ignored 7개는 `#[ignore]` 시험(g5 S5 넷, genesis S5 둘)과 native
worker다. XRB 13개도 이 code에서 다시 돌렸고 결과는 위 표와 같다(H001만 XB_FAIL, H002..H016은 따로 실행해 15/15).

<a id="unlink-u2"></a>
### U2 결과

상태는 **2026-11-05 구현·memory 시험 PASS / 독립 D2 결과는 [U2 검토
기록](../../module-gate/evidence/dev3-08-1-unlink-u2/review.md)**이다. 복구 replay가 이제 opcode 3·4 frame을
받는다. 하지만 그 frame을 쓰는 제품 경로는 아직 없다(live 효과·호출자는 U3). 그래서 사용자에게 보이는 동작은
바뀌지 않는다.

구현:

- `plan4_retention/recovery.rs`의 scope4 dispatch: `history::structure::is_unlink`(frame 길이 35,248·19,032)이면
  `apply_unlink`, 아니면 split의 `apply`를 부른다. 두 함수는 따로 부르는 frame이라 겹치지 않는다. unlink의 after
  2장은 기존 after 8장 대여의 앞부분이다.
- `history/structure.rs`의 `apply_unlink`([복구](#준비효과복구)):
  1. `unlink::decode_layout`.
  2. Structural bank 칸의 이전 owner(`observe_structural_bank_row`).
  3. candidate 입력(table = core의 object, source = core의 leaf).
  4. `unlink::frame::observe`: 같은 cut에서 준비를 다시 하고 frame bytes를 비교한다.
  5. 바뀐 target(anchor, LeafUnlink면 P)마다 page_no < 64, page LSN = frame LSN, after image의 identity·kind·level이
     native target과 같은지 본다.
  6. 모두 통과한 뒤에만 arena에 after를 쓰고 Structural bank 칸을 frame descriptor·LSN·STABLE로 바꾼다.
  7. 관측의 `initialized`·`retired`는 비운다. 퇴역 page는 claim과 함께 Allocated로 남고, `retired` 칸은 Free를
     뜻하기 때문이다.
- source 회계:
  - `semantic_row/structure/unlink/source.rs`: unlink candidate·layout·frame 회계. split 회계와 같은 단계 규칙을
    쓰고, split의 table·audit·previous·parent edit chain을 그대로 쓴다. split frame 회계의 descriptor·redo 항목은
    함수로 뽑았고 값은 같다.
  - `history::structure::unlink_source_bytes`: `apply`와 같은 규칙으로 compare 단계(관측이 돌아오기 전)와 finishing
    단계(`result`·target 검산·공개)를 나눈다.
  - scope4 회계는 단계마다 split과 unlink 중 큰 쪽이다. unlink compare는 24,876B로 split의 25,036B보다 작고,
    finishing은 6,026B로 split의 10,170B보다 작다. 그래서 다음 값이 모두 그대로다: history frame 26,452B, recovered
    peak 65,392/65,400B(memory/native), live Row lifecycle 40,883B(U1 UNL-C01 뒤 값), preflight C 여유 184B.
  - 처음 계산에서는 compare·finishing을 한 단계로 합쳐 unlink arm이 25,972B였고, history frame이 27,388B가
    되었다. live Row lifecycle 회계가 이 frame을 포함하므로 preflight의 C 여유가 −744B가 되었다(S2W-C01·C02·C03·C06과
    S1B 회계 C01·C02 실패). 그래서 위처럼 `apply`의 단계 규칙에 맞췄다. 코드도 그 순서다: `result`는 관측이 돌아온 뒤 선언된다.
    unlink compare의 split 아래 여유는 160B다.
- fixture: `current-tree/author.py`에 `recapture`(capture LSN 96/248과 inventory LSN만 옮기고 다시 봉인)와 아래 두
  함수를 더했다. 두 fixture는 capture 89이므로 frame LSN은 90이다. 기존 fixture bytes는 같고 `inputs.sha256`을
  갱신했다.
  - `leaf_unlink_before`: ChildSplit 85가 빈 leaf 19를 남긴 상태(root 17 [2,3,4] → 11,18,16,19, revision 4).
    `unlink-leaf-before.xrb`.
  - `root_collapse_before`: RootSplit 70이 CREATE leaf 11을 비운 상태(root 17 [1], revision 2).
    `unlink-collapse-before.xrb`.

<a id="unlink-u2-census"></a>
UNL-C01·C02 census(2026-11-06, U1 UNL-C01 뒤 보강). 값은 memory·native가 같고, 아래 시험이 찍는다.

- UNL-C01(U2 code의 목록 호출): unlink candidate의 목록 호출을 call 전체로 센다.
  - `build`의 목록 읽기: `retired_source_bytes`(page codec + SHA scratch 2,474B)로 3,843B.
  - 병합과 anchor encode: `extend`(409B) 또는 `listed_encode_source_bytes`. page 항은 이 chain의 기존 항(tree
    codec·digest)과 split candidate의 page codec + SHA scratch 중 큰 쪽이다. anchor chain은 2,661B에서 3,790B가 된다.
  - `verify_after`의 anchor decode: `whole_decode_source_bytes`(decoder 전체 frame + callee)로 2,698B에서 3,403B가 된다.
  - 셋 다 기존 최대보다 작다. build 쪽 최대는 parent edit 11,705B이고 verify 쪽 최대는 audit 18,908B다. 그래서
    candidate peak 21,096B와 frame peak 23,084B는 그대로다. UNL-R04 source 목록에도 같은 호출을 더했다.
- UNL-C02 dispatch: `recovery::apply_frame`의 scope4 dispatch 값(`is_unlink`의 길이 검사, after 2장 대여와 그
  `try_into` Result)을 `unlink_dispatch_bytes` 48B로 센다. 이 값은 unlink 쪽에만 있으므로 unlink arm의 두 단계에
  더했다. 더한 뒤 compare는 24,876 + 48 = 24,924B로 split 25,036B보다 작고, finishing은 6,026 + 48 = 6,074B로 split
  10,170B보다 작다. scope4 최대는 split arm 그대로이고 history frame도 26,452B 그대로다.
- UNL-C02 canonical replay: `apply_unlink`의 회계(`unlink_source_bytes`)가 observer(`unlink::source_bytes`, 위
  candidate 포함)를 callee로 갖는다. 그래서 위 UNL-C01 항이 replay에도 그대로 들어간다.
- UNL-C02 base capture: replay한 unlink 상태의 `capture_base`는 Base4 encoder를 부른다. encoder의 검증은
  current-tree closure에서 anchor 목록을 읽는다. 이 경로의 값은 다음과 같다.
  - closure 28,928B 안에 목록 읽기 3,609B가 있다(U1 UNL-C01).
  - Base4 row codec 59,675B가 이 closure를 포함한다.
  - capture encode 단계는 owner + codec(64,203/64,211B, memory/native)이고, recovered peak 65,392/65,400B 안에 든다.
  - U2는 capture code를 바꾸지 않았다.
- fresh·recovered writer:
  - fresh writer의 live Row lifecycle 회계(S2W-C01..C06)는 history frame을 포함한다. 그 frame이 그대로이므로
    40,883B도 그대로다.
  - recovered writer의 회계(`recovery_source_` memory·native, owner 회계)는 peak 65,392/65,400B 그대로다.
  - 상한은 바꾸지 않았다.

| ID | libtest 경로 |
| --- | --- |
| UNL-C02 dispatch | `…retention::history::structure::tests::unl_c02_scope4_dispatch_beside_the_unlink_arm`(독립 dispatch 목록, unlink arm + dispatch < split arm) |
| UNL-C02 capture | `storage::plan4_recovery_base::row::graph::tests::current_tree_tests::unl_c02_current_tree_closure_counts_the_retired_list`, `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_recovery::tests::unl_c02_base_capture_holds_the_row_codec`(memory·native) |
| UNL-C01(U2) | 위 UNL-R04 source 시험(목록 호출을 더한 독립 목록) |

시험(모두 PASS, memory). 경로는 `cargo test -p xadb-engine --lib -- --list`의 libtest 경로다(`…retention`은
`storage::wal_io::bootstrap::doublewrite_install::mutable_storage::plan4_pages::retention`).

| ID | libtest 경로와 판정 |
| --- | --- |
| UNL-R01 | `…retention::recovery::structural_replay_tests::unlink_replay::unl_r01_*`. 두 before fixture에서 제품 producer로 frame을 만들어 replay한다. 판정: 바뀐 page가 [10] 또는 [10, 17], 행·Job·claim 불변, 64 page 전부가 독립 after fixture와 bytes로 같음, bank 34 = frame descriptor·LSN 90·STABLE. raw capture가 after fixture 전체와 bytes로 같고 Base4로 decode된다(B09). 그 capture에서 다시 시작하면(B07) 행·목록이 같고, 퇴역하지 않은 leaf마다 unlink가 NotNeeded다 |
| UNL-R02 | `…unlink_replay::unl_r02_*`. 두 opcode 각각 다음을 본다. core revision·separator·retired_after, anchor native의 before digest, anchor redo의 byte 하나: 다시 hash한 뒤에도 `decode_layout`을 통과하지만 replay는 Corrupt로 거절한다. 빈 leaf에 범위 밖 행이 든 cut과 P page 손상은 Tree 오류, 이전 bank 칸 손상은 Invariant다. LeafUnlink frame을 RootCollapse cut에 주면 IdentityMismatch다. dispatch는 길이만 보므로, unlink 길이에 split opcode(1)를 넣은 frame과 split 길이에 unlink opcode(3)를 넣은 frame(둘 다 다시 hash)을 각각 고른 decoder가 Unsupported로 거절하는 것도 본다. 모든 거절에서 replay digest·cursor가 그대로다 |
| UNL-R03 | `…unlink_replay::unl_r03_*`. 첫 split(RootSplit 70)은 before fixture의 상태이고, replay하는 것은 RootCollapse 90과 그 뒤 root leaf 16의 RootSplit 91이다. 판정: 새 page 2개가 11·16·17이 아님, decode한 목록 항목 [11, 17] 유지, bank 칸이 91, 연쇄 상태의 capture가 Base4로 decode되고 재시작해도 행·목록이 같다 |
| UNL-R04 | `storage::semantic_row::structure::unlink::source_tests::unl_r04_*`(parser·emitter·candidate 단계와 frame outer의 독립 목록), `…retention::history::structure::tests::unl_r04_*`(replay arm의 compare·finishing 독립 목록과 scope4 최대) |
| UNL-P08(Catalog 부채) | `storage::semantic_row::structure::weights::tests::candidate::unlink::unl_p08_catalog_job_of_the_table_refuses_before_not_needed`. 독립 job3 literal(RootRetained, Catalog, table 6 generation 1. 원본 TxnRef의 database만 이 profile 것으로 바꿨다)을 넣으면 빈 leaf(unlink 대상)와 행이 있는 leaf(원래 NotNeeded) 모두 Unsupported다. 넣지 않으면 각각 unlink·NotNeeded다 |

U1에서 미룬 것의 처리:

- B07 재시작·B09 다음 capture: R01이 replay 상태의 raw capture를 독립 after fixture와 bytes로 대조하고, 그
  capture에서 다시 시작한다. 제품 rebase(`RecoveryState::capture_base`, BaseOnly)가 unlink frame을 지나는 경로는
  WAL에 실제로 쓰인 unlink frame이 있어야 하므로 U3 live 시험(UNL-L의 재시작 read)이 본다.
- P08 Catalog 부채: 위 단위 시험을 더했다. U3의 UNL-L07이 CREATE Catalog Job table로 다시 본다.

계획과 다르게 남긴 것:

- R02의 "비지 않은 leaf" 변조는 범위 밖 행이 든 잘못된 tree(Tree 오류)다. 범위 안 행이 든 유효한 tree를 만들려면
  행 record를 정확히 다시 encode해야 한다. 그래서 유효한 NotNeeded는 R01 재시작의 NotNeeded와 U1 P07이 본다.
- 계획의 R01 "leaf 수"는 page 64개 전체의 bytes 대조로 대신했다.

실행한 명령(작성자 checkout, U2 최종 code. `<hex>`는 64자 자리표시 hex이고, 세 XADB_CATALOG_*_SHA256과 두 번째·세
번째 명령의 XADB_PLAN4_RECOVERY_*_SHA256에 준다. AGENTS.md에 따라 바뀐 코드에 닿는 시험만 골랐고 전체 suite는
돌리지 않았다):

| 명령(`cargo test -p xadb-engine --lib -- …`) | 결과 |
| --- | --- |
| `storage::anchor::tests:: storage::semantic_row::structure:: storage::plan4_recovery_base::row` | 115 passed, 0 ignored(U1의 113개 + UNL-R04 source·UNL-P08 Catalog 부채) |
| `retention::history::structure:: retention::recovery::structural_ catalog_live::structural:: plan4_recovery::analysis::structural_bank_tests plan4_recovery::tests::row_gate:: retire::driver::structural_ split_rows:: row::census_tests:: retention::owner_tests::` | 79 passed, 1 ignored(`structural_effect_native_worker`). UNL-R01..R03과 history 쪽 UNL-R04(source 쪽 R04는 첫 명령), 바뀐 dispatch를 지나는 split replay·live split·재시작 시험(SPL-A01..A06·A08·A10·SPL-J01..J07·structural effect. SPL-A07·A09는 U2 code에 닿지 않아 고르지 않았다), S1B 회계(26,452B), S2W-C01..C06(live Row 회계·C 여유), owner 회계(recovered peak ≤ 65,536)를 포함한다 |
| `plan4_recovery::tests::recovery_source_` | 2 passed(바뀐 history 회계가 들어가는 recovery 자원 회계) |

U2는 Base4 decode(`plan4_recovery_base`)를 바꾸지 않으므로 executor 전용 XRB 시험은 다시 돌리지 않았다.

U1 위로 다시 올리고 UNL-C01·C02를 더한 뒤 다시 실행한 것:
- env: U1의 재실행과 같다. source·ELF·input hash에는 자리표시 hex를 모두 주었고 `RUST_MIN_STACK=33554432`다.
- 범위: 위 세 명령의 시험, UNL-C02 셋, U1이 재실행한 시험 전부(UNL-C01 census 시험 포함)다. 겹친 것을 뺀 592개를
  `--exact`로 실행했다.
- 결과: **584 passed, 1 failed, 7 ignored**. 실패 하나는 executor 전용 `g5_bind_boundaries`다. padding 2048로 따로
  돌리면 16/16 PASS다.

scope4 dispatch를 지나는 두 시험은 main `1725df0`에서도 같은 값으로 실패한다. 두 시험은
`…retention::recovery::row_tests::s1b_scope_routes_only_to_its_own_arm`과 format 시험
`mbc40_v01_profile2_binding_rejected_at_owner_grammar_sites`다. 두 시험 모두 frame을 scope 4로 바꾸면 Unsupported라고
기대한다. 실제로는 structural decoder의 길이 검사에서 Corrupt로 거절된다.

원인은 시험 기대값이다. split의 scope4 arm(`37a0465`)이 들어올 때 이 기대를 고치지 않았다. 별도 작업("Fix scope 4
frame routing test")이 시험 두 파일만 고치고, 그 commit을 이 브랜치에 merge한다. 이 단계에서는 고치지 않았다.
