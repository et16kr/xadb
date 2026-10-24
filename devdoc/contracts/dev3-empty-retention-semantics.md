# DEV3 빈 거래의 정리 의무와 의미 그룹 표현

계약 ID: **D3-EMPTY-RETENTION-SEMANTICS-001**. plan4의 Root/Job, Root에 결합한 Aging descriptor, Undo가 없는 거래의 Begin·AbortEnd·Finish를 다루는 **순수 byte 검산·생성 계약**이다. 독립 D2에서 선택한 wire 입력 `0561d2c1fc40daeacf65d530b245932a8daf4b213e9476ca1df72a48023e3ae2` 중 이 표현 범위만 구현 대상으로 삼는다. 전체 G6 owner·REBASE·복구 설계를 이 문서의 구현 완료로 해석하지 않는다.

원 source에서 semantic137개와 ST01이 PASS했고, 도달성을 보정한 N07/N08 두 사례는 후속 source/ELF에서 PASS했다. 원 도달성 FAIL과 각 source의 결과를 보존한다. **최종 독립 D1을 이 순수 의미 처리 단위에서 제한 수용했다.** source·ELF·실행 범위별 결과와 후속 판정은 [선택 실행 증거](../evidence/dev3-empty-retention-semantics-20261024.md)에 수합한다.

[구현 계약](implementation-contracts.md), [plan4 설치](dev3-plan4-genesis.md), [원 공유 버퍼의 clean 소비](dev3-plan4-clean-owner.md), [obligation 표현](dev3-obligation-format.md), [WAL frame](dev3-wal-format.md), [거래 표현](dev3-transaction-format.md), [DEV3 단계별 시험](../plans/dev3-test-plan.md)을 함께 따른다. 기존 계약의 일반 의미 그룹·예약·복구 의무를 이 좁은 표현으로 대체하지 않는다.

## 1. 소유 범위와 호출자 의무

제품 범위는 `retention_page`, `retention_obligation`, `semantic_retention`과 이들을 연결하는 `storage::mod`, 공통 descriptor의 좁은 Aging 검산 경계, WAL tag3 envelope다. 새 view는 원 입력을 빌려 관측하며 pin, reservation, dirty/WAL requirement, stage, apply, publication 또는 재사용 권한을 반환하지 않는다. filesystem·WAL append·sync·epoch 발급 효과도 없다.

`retention_page`는 Root/Job의 canonical bytes와64-cell census를 검산한다. `retention_obligation`은 선택된 Profile 및 full Root page와 Aging descriptor를 결합한다. `semantic_retention`은 신뢰한 기대값과 before state에서 unpublished after pages를 계산하고, 이를 한 frame의 descriptor/native/redo와 대조한다.

실제 live/replay 호출자는 별도로 다음을 증명해야 한다.

- 원 owner와 같은 pool·namespace·full target identity의 membership 및 안정된 before state.
- 실제 ATT·Status와 전체 Job census의 양방향 일대일 대응, allocator·INIT provenance.
- 현재 연산과 이미 발급한 종료 의무를 감당하는 실제 admission·credit·counter 예약.
- 전체 그룹의 stage·durability·page publication, 중간 물리 상태의 REDO 및 오류 뒤 원 owner 보존.

local hash나 성공한 `RetentionObservation`을 이 증명의 대용으로 사용할 수 없다. 원 source WAL이 없는 base 검산, CURRENT/WRITER/epoch 선택, 같은 pool의 recovered live handoff, native 재시작은 후속 범위다.

## 2. 고정 geometry와 canonical Root/Job

namespace는 timeline1이며 첫 data file은 `0x0000000100000001`이다. plan4 Root는 physical page1, Jobs는2..6, Catalog는7/8이다. Root/Jobs는 기존 Object4·owner generation1·page incarnation1·level0 identity를 유지한다. Jobs page2..5는15cell씩, page6은4cell로 ordinal0..63을 구성한다. Status는 Object2·owner generation1, page9..63의 slot0 하나만 받는다. 실제 Status page incarnation은 호출자가 제공한 full identity와 결합한다.

JobSlotRef40은 PageRef24, slot:u16, reserved6, generation:u64다. Jobs의 exact page/slot geometry를 만족해야 하며 ordinal은 `(page_no−2)×15+slot`이다. 같은 physical 주소의 다른 incarnation을 같은 유효 참조로 취급하지 않는다.

모든 다중 byte 수치는 little endian이다. exact length·version·tag와 reserved0을 검사하며, decode한 spec을 canonical 재인코딩한 bytes와 대조한다. trailing bytes, 모르는 state, 비정규 padding을 수락하지 않는다. 기존 genesis validator는 여전히 Free/Unstarted만 받는다.

### Root512

`XQR1/version1`은 Aging ref, revision, cursor, apply LSN, issued/completed 누계, Free/Leased/Ready bitmask, ordered Job digest 및 cleanup budget digest를 결합한다.

- 세 mask는 서로 겹치지 않고 합집합이64bit 전부다. Leased는 최대32, non-Free는 최대55다.64개의 queue cell이 있다는 사실은64개의 정상 occupied Job이 가능하다는 뜻이 아니다.
- Aging absent는 revision1/cursor0/apply0/issued0/completed0/Free64와64개의 Free generation1 digest인 정확한 genesis다.
- Aging present는 같은 namespace의 `SystemWorkRef(Aging,id1,generation1)`이며 apply>0이다. `completed+non_free=issued`, `revision=2×issued−leased_count+completed+2`를 checked arithmetic으로 검산한다.
- cursor는0..63이며 Root page의 LSN은 Root.apply와 같다. full page는 exact identity/kind/level0/one-cell 구조를 만족한다.

### Job1024

`XQJ1/version1`의 닫힌 상태는 Free, Leased Terminal, Ready Terminal/Detach 세 가지다. Free는 magic/version/generation을 제외한 bytes가0이며 세대를 유지한다. non-Free는 source TxnRef와 StatusSlotRef를 필수로 가지며 두 generation이 같다. Job generation과 source generation은2 이상, lease LSN은 양수다.

Leased는 phase0/disposition0/handoff0/sender digest0이다. Ready는 phaseDetach1/dispositionAborted2이며 handoff LSN>lease LSN, sender-before digest를 보존한다. 둘 다 presence는 Status만 나타내는16이다. head/stop/cursor/saved-next/sequence/statement/remaining-root와 commit-q는0이다. Running·Suffix·Committed 또는 non-Free이면서 Status ref가 없는 상태는 이 entry에서 지원하지 않는다.

last_step은 Leased의 lease LSN, Ready의 handoff LSN, Free의0이다. Jobs page는 고정 cell 수와 모든 `last_step≤pageLSN`을 검사한다. pageLSN0은 Free generation1만 허용한다. Finish 뒤 Free generation을1로 되돌리거나 재사용 시 wrap하지 않는다.

### 전체64-cell census

`verify_census`는 caller가 빌린 `[[u8;1024];64]`를 ordinal 순서로 읽어 모든 cell의 정상성·세 mask·전체 bytes SHA·`last_step≤Root.apply`를 검사한다. 서로 다른 generation이라도 같은 TxnId가 두 non-Free Job에 있으면 거절한다. incarnation이 달라도 같은 physical Status file/page를 두 Job이 참조하면 거절한다.

이 검산은 실제 Status page나 ATT를 읽지 않는다. 이들에 대한 역방향 대응, selected revision 동안의 안정성 및 orphan 부재는 live/base 호출자의 별도 의무다.

## 3. Root에 결합한 Aging descriptor

wire 크기는 기존1536bytes다. 공통 `DescriptorSpec`·`SystemProgress`에 새 필드를 넣지 않고, 기존 work 구간384..480에 local kind7과 Root revision, full-page length16384, full Root page SHA, Root cell512 SHA를 기록한다. `AgingDescriptorView`는 원1536bytes를 빌리고 검산한 공통 descriptor 관측을 제공한다.

SystemProgress는 같은 Aging ref의 Running이며 sequence=Root.revision, cursor=Root.cursor, limit64다. descriptor apply=Root.apply, groups+1=Root.revision이다. inventory change LSN과 minimum WAL LSN은 birth를 유지하고 inventory items/growth/last mutation sequence는0, inventory digest는 SHA256(empty)다. 공통 owner chain·profile·reserved·budget summary 검산도 그대로 적용한다.

소비와 남은 의무는 다음 식을 만족해야 한다.

```text
consumed_wal = 19,672 + completed × 55,112
consumed_control = 0
remaining[3].steps = non_free_count
remaining[3].wal = remaining[3].protected = non_free_count × 55,112
```

다른 row와 additive 항목은0이다. non-Free가0이면 peak도0이며, 그 외에는 §6의 고정 peak 입력을 사용한다. Root의 budget digest는 같은5개 row를 나타낸다.

기존 generic `decode_descriptor`는 이 nonempty Aging work body를 계속 거절한다. generic ControlRef kind dispatch도 넓히지 않는다. 새 내부 policy 경계가 공통 descriptor 검산을 재사용한 뒤 exact work bytes를 검사하며, 입력 work bytes를0으로 바꿔 old decoder를 통과시키지 않는다.

## 4. 닫힌 의미 그룹과 tag 경계

새 explicit entry는 `produce_aging_start`, `produce_begin`, `produce_abort_end`, `produce_finish` 및 각각의 `observe_*`다. Commit, StatementFinish/CLR, Suffix, nonempty Undo와 일반 queue cleanup은 지원하지 않는다. 기존 RollbackStart는 닫힌 경로의 중간 입력이며 새 producer로 복제하지 않는다.

| 연산 | outer kind / payload | group·operation | exact 전체 frame bytes |
| --- | --- | --- | ---: |
| AgingStart | System3 / tag2 | XSG1 op16, XRO1 |19,672|
| BeginEmpty | Begin1 / tag3 | XHG1 op4, XHO1 |56,712|
| RollbackStart | Update2 / tag2 | 기존 XSG1 op3 |3,040|
| AbortEndEmpty | AbortEnd6 / tag3 | XHG1 op3, XHO1 |56,712|
| FinishEmptyTerminal | System3 / tag2 | XSG1 op17, XRO1 |55,112|

op16/op17은 outer TxnId와 previous transaction LSN이0이다. Begin은 sender-before가 없고 previous transaction LSN0, birth=apply=새 LSN/groups1이다. AbortEnd는 exact before sender와 owner chain을 요구한다. XHG1은 header256와 after descriptor2×1536, XSG1은 header192와 after descriptor1536을 쓴다. 새 XRO1/XHO1 operation body는256bytes다. 이 scope의 edge/count/inventory delta는 모두0이다.

tag3은 별도 `finish_retention_envelope`·`decode_retention_envelope`만 사용한다. old legacy entry는 tag2/tag3을 거절하고, tag2 entry는 tag3을, tag3 entry는 tag2를 거절한다. 기존 semantic opcode observer에16/17이나 새 두-owner 의미를 암묵적으로 허용하지 않는다. envelope는 기대 stream/epoch/predecessor, header/full-frame SHA, reserved, 정확한 길이를 검산하지만 의미 그룹의 before/after 권한을 발급하지 않는다.

### 상태 전이와 순서 검산

공통으로 Root/full64 census를 먼저 검산하고 Root·선택 Jobs·Status의 pageLSN이 global predecessor 이하인지 확인한다. 선택 Jobs page의 cell들은 같은 census 창과 byte 단위로 같아야 한다. Status는 정확한 one-cell home이며 incoming0을 요구한다. actor의 DB/timeline과 full Status identity가 기대값에 맞아야 한다.

1. **AgingStart:** exact Unstarted Root와64 Free generation1에서 첫 Aging를 만든다. actor/Status/Job/sender/이전 Aging 입력은 없다. queue가 비어도 이후 Aging는 Running으로 유지된다.
2. **BeginEmpty:** exact Free Status/Job의 checked generation successor가 actor/Job 기대 generation과 같아야 한다. actor epoch는 현재 stream epoch이며 local id는 이전 adopted high-water보다 크다. 기존 non-Free Job의 같은 TxnId/physical Status와 중복할 수 없다. Active(head=None), Leased Job, 양 after descriptor와 Root를 같은 그룹으로 계산한다.
3. **AbortEndEmpty:** 선택 Job은 exact source·Status·generation의 Leased이고, Status는 같은 actor의 AbortingFull(next=None), head=None이다. sender는 groups2, `previous=birth`, apply≤global predecessor이며 birth=Job lease다. inventory/minimum WAL은 birth, remaining은 terminal row 하나, consumed WAL은59,752여야 한다. Status pageLSN은 이 sender의 apply 이상이어야 한다. after는 Aborted/head=None, Tx remaining0/groups3, Ready Terminal/Detach, 전진한 Root/Aging다.
4. **FinishEmptyTerminal:** exact Ready Job과 같은 actor의 Aborted Status/head=None을 요구한다. Status pageLSN은 Ready.handoff LSN 이상이다. **StatusFree+JobFree+Root/Aging**을 한 그룹으로 계산하고 두 generation을 유지한다. completed를1 늘리고 cursor를 `(ordinal+1)%64`로 옮긴다. Status 없는 non-Free 중간 상태를 만들지 않는다.

LSN·revision·group·issued/completed·generation 증가는 checked다. producer가 현재 입력의 overflow를 거절한다는 사실과 live admission이 이미 발급한 모든 미래 종료 tick을 선예약했다는 사실은 구별한다. accepted-but-unacknowledged Begin 처리와 종료 예약은 실제 owner 구현의 후속 의무다.

### native와 redo의 같은 target 결합

canonical target 순서는 Root→선택 Jobs→Status이며 AgingStart는 Root만 갖는다. NativeEffect header128의 kind는 각각8/9/5, mode3, redo index는0/1/2다. body512/1024/224의 before+after를 포함하므로 native 길이는1,152/2,176/576bytes다. Root slot0 offset15,840, Job slot k offset`16,352−1,024×(k+1)`, Status slot0 offset16,128을 사용한다.

각 native는 exact identity/kind/level/slot/offset/body length와 full before/after digest를 결합한다. redo는 같은 target당 하나의 Patch `[72,16352)`, data16,280+header64=16,344bytes다. prior pageLSN은 실제 before에서 가져온다. observer는 신뢰한 before/context로 after를 계산하고 native·redo가 그 결과와 같은지 검사한다. 입력 frame의 after bytes끼리만 비교하여 통과시키지 않는다. 실제 pageLSN/trailer 갱신·혼합 home 복구는 이 순수 관측의 실행 효과가 아니다.

## 5. 종료 예산의 보존

Begin의 additive 예약은 다음과 같다. 표의 row는0부터 센다. wal과 protected는 같은 값이며 control/growth/pages/undo_slots/status_slots는0이다.

| 의무 소유 | row | steps | wal·protected | 소비 |
| --- | ---: | ---: | ---: | --- |
| Tx terminal |2|1|56,712|AbortEndEmpty|
| Tx rollback |3|1|3,040|RollbackStart|
| Aging에 포함된 Job cleanup |3|1|55,112|FinishEmptyTerminal|

Begin 현재56,712와 미래114,864를 합한 admission은171,576bytes다. 미래 Tx 몫은59,752, Aging 몫은55,112다. Job은 Aging debt의 내역이므로 global 합에 다시 더하지 않는다. AgingStart의19,672는 별도 현재 소비다.

AbortEnd의 transfer X는0이다. op3의 transfer digest는 tag1..5를 갖는 canonical128-byte row×5에서 consumable·peak가 전부0인640bytes의 SHA이며 zero32가 아니다. **op4만 zero32 sentinel**을 쓴다. AbortEnd는 Tx terminal56,712를 한 번 소비하고 Tx remaining을0으로 만든다. Aging chain은 전진하지만 같은 WAL 비용을 Aging consumed에 더하지 않고, 기존 Job cleanup debt를 재취득하거나 반환하지 않는다.

Finish는 Aging의55,112/1step을 한 번 소비하고 Job·Root aggregate에서 같은 debt를 없앤다. Status/Job generation은 다음 Begin에서만 증가한다. 이 닫힌 다섯 frame의 길이 합은191,248이며 별도 allocation reserve21,200+INIT38,000=59,200을 포함하지 않는다. 실제 WAL/protected charge 해제와 unlink/dirsync는 이 숫자 검산 밖의 owner 의무다.

## 6. 대여 수명과 source 상한

`RetentionExpected`는 선택된 Profile·FrameExpectation과 actor/Status/Job의 기대값이다. 후보 frame에서 expected를 복사해 자기 검산을 만들지 않는다. `RetentionBefore`는 세 before page, sender/Aging descriptor 및64-cell census를 읽기 전용으로 빌린다. `RetentionAfter`는 caller의 unpublished page scratch를 가변 대여한다. `RetentionObservation`은 producer의 output 또는 observer의 input frame과 그 안의 after descriptor slice를 빌린다. observation이 살아 있는 동안 같은 bytes를 가변 재대여하지 않는다.

Root/Job/Aging의 local encoder는 오류 때 output을 보존한다. semantic producer/observer의 after page scratch는 오류 때 일부 바뀔 수 있으므로 실패한 scratch를 설치하지 않는다. frame 길이 preflight와 local envelope의 output 보존 보장을 전체 semantic 호출의 모든 scratch 보존으로 확대하지 않는다. immutable before와 observer input은 바꾸지 않는다.

pure source 상한은 `semantic_retention::source_bytes()`에 포함된 실제 argument/local/Result/borrowed view·descriptor·hash 값과 그 내부 Aging source 값, 별도 Root/Job/page component를 합쳐 **65,536bytes 이하**로 검산한다. wide descriptor와 오류 `Result`도 포함하며 성공 값만 계상하지 않는다. ST01은 실제 수치를 cap assertion 전에 출력한다. 이 source-value 검산은 machine stack/RSS 측정이 아니다.

caller backing은 이 core 합과 분리한다.

| 대여 | 구성 | bytes |
| --- | --- | ---: |
| 읽기 전용 | before page3×16,384 + descriptor2×1,536 + Profile4,096 + census64×1,024 |121,856|
| 가변 | after page3×16,384 + frame65,536 |114,688|

normative pure wire 입력의 ResourcePeak6는 `[4,114688,0,4,5,1]`이다. 이는 frame/memory/lock/registry/file/DWB 항목의 표현 입력이며 **actual live caller F05 승인값이 아니다**. 원 owner·pool·Writer/File·guard·maintenance·WAL slot·cleanup의 동시 수명은 후속 live F05에서 더해 검산해야 한다. fixture 생성 output cap과 시험 host backing도 제품 source 상한에 섞지 않는다.

## 7. 독립 입력과 현재 검증 경계

[독립 literal generator](../../tests/fixtures/dev3-plan4-empty-retention/generator.py)는 명시 offset·little-endian 정수·SHA 식으로 기대 bytes를 만들며 제품 encoder/decoder/binary를 기대값 원천으로 쓰지 않는다. [provenance](../../tests/fixtures/dev3-plan4-empty-retention/provenance.md)의 원 plan4 raw/reference SHA와 [input-plan](../../tests/fixtures/dev3-plan4-empty-retention/input-plan.json)의 exact selector/ID/operation/expected를 결합한다. source-only 준비 문구와 실제 생성·실행 receipt는 각 시점의 기록으로 보존한다.

기본 profile1/ordinal0 vector는 새 raw24개398,608bytes와 immutable reference10개151,552bytes다. generation은 create-new와 승인 source/input hash를 사용하며 원 partial·실패·완료 raw를 덮지 않는다. 이 fixture의 Status page9/incarnation2/Free generation1은 의미 검산의 before context이며 실제 reserve/INIT provenance를 발급하지 않는다.

현재 필수는 semantic139+static1=140이다. 원 semantic137 PASS와 ST01 PASS를 원 source/ELF 범위로 유지한다. N07/N08의 첫 실패는 각각 generic Jobs pageLSN 및 Free Status incoming 제약 때문에 목표 semantic branch 전에 차단된 시험 입력 문제였다. 수정 source는 N07의 정상 Jobs setup과 N08의 occupied Aborting Status setup을 먼저 검산하며, 이 두 사례만 후속 실행해 통과했다. 실패를 PASS로 재분류하거나 나머지 완료138개를 반복하지 않는다.

S5 추가475는 profile별 producer/observer16, 다른 Job ordinal의 Begin/Finish126, generation 경계6, Root reserved127, Job reserved4, XHO reserved15, XRO reserved23, envelope prefix158로 별도 선택한다. 현재 S5 추가는 미실행이며 전체615를 완료했다고 쓰지 않는다. 기존 하위 wrapper·완료 inventory도 호출하지 않는다.

현재 증거는 pure codec·semantic·tag3·source census에 한정된다. 전체 G6의 live admission/Aging 실행/dirty flush/base/새 epoch/recovered same-pool/native3PID, 그 실제 F05, DEV3 최종 S5 및 첫 행 거래 인수는 여전히 미완료다.
