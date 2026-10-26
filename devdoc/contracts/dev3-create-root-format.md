# DEV3 실제 CREATE의 Root512 v2

2026-10-25 · `D3-CATALOG-CREATE-001` 결합 D2 채택·구현 착수 수용 · [실제 소비 계약](dev3-catalog-create.md). 제품 효과·실행 인수 미완료.

2026-10-25 · /root · 결합 D2 채택 정본. 실제 source F05·입력 동결 전 제품 효과·시험 미입장. B1 commit35c879e 뒤 실제 Catalog CREATE/Commit/복구 소비자의 일부로만 구현하며 codec 단독 완료 단위를 만들지 않는다.

## Root512 wire

모든 정수는 기존 little-endian. magic XQR1, version2. 현재 page envelope/queue object4 generation1, first file/page1과 Jobs pages2..6은 유지한다.

| bytes | 의미 |
| --- | --- |
|0..4|XQR1|
|4..6|u16 version2|
|6|0 Unstarted / 1 Started|
|7|zero|
|8..12,12..16|u32 cell512, slots64|
|16..64|Started의 exact SystemWorkRef(Aging,id1,generation1,같은namespace); Unstarted zero|
|64,72,80|revision, queue cursor0..63, 마지막 이 Root를 변경한 group LSN apply|
|88,96,104,112|total Job leases issued, free/leased/ready bitmaps|
|120..128|zero|
|128..160|ordinal0..63 전체 Job1024의 content digest|
|160..192|nonfree Jobs의 budget rows를 성분별 합친 canonical640B digest|
|192..208,208,216,224|database16, timeline1, queue object4, generation1|
|232..256|원 Root PageRef|
|256..376|원 Jobs5 PageRef|
|376,384|완료 Job수, Forward 전 취소 Job수|
|392,400,408|namespace UndoSequence high, dynamic ObjectId high, adopted Commit C|
|416,424|Aging descriptor actual consumed_wal/consumed_control|
|432..512|전부zero|

kind/state에 따른 의미 외 reserved byte와 padding은 모두0. UnsupportedFormat은 version 불일치, UnsupportedState는 state 불일치, 다른 noncanonical bytes는 Corrupt로 구별한다. codec view는 observation이며 mutation/pin/credit authority가 아니다. output encode 오류는 전체 출력을 보존한다.

## local 및 전체 census 검산

비트맵은 서로배타·합집합u64MAX. nonfree≤55, leased에 별도32 제한은 두지 않는다. `issued=completed+canceled+popcount(nonfree)`는 checked 연산으로 확인한다. revision>0, cursor<64. Started에는 apply>0이며 원 Aging descriptor의 chain.groups+1=revision을 외부 whole census가 확인한다. old N=0의 issued/leased/completed로 revision을 유도하는 식은 v2에서 사용하지 않는다.

Unstarted는 revision1/cursor0/apply0/issued0/completed0/canceled0, all Free generation1 v2 Jobs, UndoHigh0/ObjectHigh5/C0/consumed0, 정규 empty budget digest로 정확히 한 상태다. Started는 ObjectHigh≥5, 나머지 high는 단조 증가하며 codec단독은 predecessor보다 증가한 사실을 인증하지 않는다. semantic observer가 각 opcode의 exact before/after를 대조한다. Undo/object 발급은 WAL LSN/slot/Tx local과 독립이며 Reserve가 checked successor를 실제 채택하고 Cancel도 되감지 않는다. C는 Commit에서만 coordinator의 checked C+1로 바뀌고, publication Q는C이하이다.

whole64 Job census는 원 stable revision/version gate에서 stream한다. root digest, masks, 각last_step≤apply와 exact aggregate budget digest를 동시에 대조한다. TerminalLeased+RootReserved는leased, TerminalReady+RootRetained+RootReleasing은ready. root issued는 모든 Job lease 누계이고 cancel은 실제 RootReserved 취소, completed는 RootFree/TerminalFinish에서만 증가한다. Root/Job generations MAX·counter MAX는 효과 전에 거절한다.

nonfree≤55의 근거는 Terminal마다 exact one-slot Status page 하나, RootJob마다 exact one-slot Undo page 하나가 pages9..63의 서로 다른 allocated system container라는 사실이다. Terminal간 source.id/Statuspage 중복 금지, RootJob간 Undo page/sequence 중복 금지, Status·Undo physical page 집합 겹침 금지다. RootJob은 exact source+Status와 일치하는 Terminal 하나를 가져야 하며 Terminal 하나를 여러 RootJob이 참조할 수는 있다. same source 다른 generation·같은Status 다른source는 거절한다. 첫 consumer에는 CREATE root 한 개만 허용해도 이 전체 census를 생략하지 않는다. 실제 actor ATT 최대32는 별도원한도다. Free pages/CREATE pair와 queue reserved pages를 개수근거에 섞지 않는다.

Job의 U reference는 cleanup ownership이고 native incoming edge가 아니다. RootJob이 남아있는 actor의 TerminalFinish를 막는다. native edges는 catalog stamp→Status, catalog.undo→U,status.head→U 및 존재하는 previous roles만 원multiset으로 센다. U creator identity를 영구숨은status edge로 추가하지 않는다.

Aging descriptor remaining=sum(all nonfree Job budget detail), 원 ATTsum+Aging를 전역ledger에서한번만계산한다. 동일 row의 steps/wal/control/growth/pages/undo_slots/status_slots/protected는checked합, ResourcePeak(frames/memory/locks/registry/files/doublewrite)는component별max를사용한다. 이는 단일 original Aging owner가 한cleanupgroup씩공유전용scratch로실행하고64Jobmetadata를별도고정회계하는조건이다. 서로동시에진행할수있는Tx owner간 peak는전역ledger의각owner예약으로따로합산한다. 기존obligation.rs::validate_descriptor의row별peakmax와같으며Job count만큼sharedscratch를중복예약하거나동시Tx들을max로합치지않는다. Tx+Aging frame의 실제wal전체는Tx만consume, Aging-only는Aging만consume. Root416/424는actual Aging 누계를보존하며19672+completed*55112 old식은폐기한다. descriptor progress revision/apply/digests와Root를양방향검산한다.

## 실제 fresh 진입과 호환 범위

새 namespace의 원 FreshStorageSession/BootstrapPlanKind::RetentionCatalogBootstrap(plan4), objecthigh5와 allocator9 allocated는유지한다. plan4 canonical_payload 및 inspection이 쓰는 current Root/Job genesis를명시version2로바꾼다. 새namespace/pool/WAL/plan5를추가하지않는다. 기존 v1 pure encode/decode 및그원fixture/증거는보존하되current runtime에서v1을암묵승격하지않는다. fresh 경로와base/replay경로가각자current버전을명시적으로검사한다.

scope2 XSG operation16 AgingStart는actualv2RootUnstarted+전체v2FreeJobs+원Aging없음을검산한다. 원Aging(id1,g1)을최초발급해chain.groups1, Rootrevision2/apply=currentLSN,consumed_wal19672,zerojobs/zeroremaining로설치한다. Root native1152+single Root redo16344, XSG headers/descriptors/core/trailer2176를합친19672B를한번청구한다. scope2 parser만이v2Root/Job의후속Begin을받고legacy scope가v2를우회수용하지않는다. System Status/FreeUndo INIT v2 flags1는별도기존scope를그대로유지한다.

이후 원 Begin56712는Terminal lease+Status generation발급+Root update를같은그룹으로설치한다. nativeCREATE Reserve는plannedU+pair+RootJob을실제inventory로확보하고Forward/Commit으로간다. actual Semantic terminalfactory를모든live/recovery/credit검산없이열지않는다.

current recovery는후속exact-image base v3/currentgenesis2만실행권한으로선택하도록이결합D2에서고정한다. old v1/v2 base의순수decode와기존실행원본은보존하고새runtime복구PASS로재분류하지않는다. S5에서형식영향을받는기존runtime입력은현재format으로적응하되pureolddecoder인수는보존한다.

## 구현·실행 입장 의무

Jobexactwire, 실제q/UndoRef pin의원자retirementgrant, caller operation/replay/dataarena와sourcevalueF05, Reserved inventory를포함한base/recoverygrammar를같이닫아야한다. 결합설계는채택되었지만offset산술만으로actualeffect입장또는실행PASS가되지는않는다.

## source-value 구현 경계

v1 RootSpec/JobSpec를전역으로확대하지않는다. v2 RootView는고정512B에대한borrowedreference와검산된namespace만보유하고필드는필요한시점에읽는다. v2 JobView도1024B와typedstate만보유하며budget640를ownedJobSpec에넣지않는다. 전체Job budget은한개의caller-local640Baccumulator로stream하고다음Job을decode할때이전wideResult를살려두지않는다. prepare의puresemantic호출이끝나고작은verifiedplan/digest를반환한뒤physicaltarget/Commitcarrier를생성하는phase를명시한다. 진단용원SourceCensus에서이상계가실제caller/Result/return중복을포함하는지새코드로확인한다.

B1의native recovery retained_live23072+flush42392=65464/65536(여유72)는변경전증거다. 같은ownedRootSpec에6×u64를추가하는식이면nested10복사만+480B가되어cap초과가능하므로그대로재사용하지않는다. exactsource정적검산없이어떤wire코드도effectpath로승격하지않는다.
