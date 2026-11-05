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
대상 table의 RootReserved Row root와 non-Free Catalog Job만 거절한다.)
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
| B point/cursor (`plan4_apply/row/read.rs`) | anchor LOAD 뒤 resident page만 따라 route하고, 비거주 RowInternal을 만나면 그 page를 LOAD한 뒤 처음부터 다시 걷는 restartable walk(LOAD 최대32단계). leaf 발급 시 anchor revision과 leaf의 bounds를 다시 확인. cursor는 leaf에 다음 key가 없으면 그 leaf의 upper bound로 다음 leaf를 route |
| lookup LOAD membership | RowInternal은 level1..15, 그 밖의 target은 level0. Index/Catalog internal은 일정09/10 |

B 읽기 operation의 `close_reads_for_retirement` gate가 split과 한 operation을 배타로 두므로,
operation 안의 여러 route는 같은 tree를 본다. 한 cursor의 step 사이에 split이 끼면 다음 step이
key로 다시 route한다. 이것은 SMO를 건너는 cursor/locator 인수(일정08-2)를 대신하지 않는다.

**admission.** 원 `split_row`는 대상 table의 RootReserved Row root와 non-Free Catalog Job만
무효과 Unsupported로 거절한다. Retained·Releasing·terminal Row 부채는 허용한다. 선택 가중치는
기존대로 각 Retained root의 restore bytes를 그 행의 쪽에 둔다.

**source census.** 원 charge 44,304B는 바꾸지 않았다. B cursor.row_load는 34,573/34,586B(여유13B),
point.row_load 34,381B다. B route 단계(row_route)는 `route_row_step`이 품는 tree codec까지 세어
cursor 7,737B, point 7,545B다. C preflight는 여유184B, reserve는 여유499B 안이다. 예약 D는
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
| SPL-B01 | B driver의 literal 두 leaf(10,20 / 30,40, separator 30). cursor가 20 뒤 왼쪽 leaf의 upper bound로 오른쪽 leaf를 route해 30·40을 돌려주고 끝남. 범위가 separator에서 끝나면 오른쪽 leaf를 route하지 않고 끝남 |
| SMO-E01 | 기존 Active·Commit 후 거절 단언을 위 admission에 맞춰 제거. 깨끗한 leaf의 Root→Child 효과 검산은 그대로 |

이 절은08-1을 완료로 적립하지 않는다. 빈 leaf unlink·root shrink·retirement와 Capacity 시 A의
자동 split 호출자가 남아 있다.
