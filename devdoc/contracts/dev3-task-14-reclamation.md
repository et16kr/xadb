# DEV3 3.14 — 공간 회수 개발 상세 명세와 설계도

2026-10-22 · **작업 상세 설계 / D2 상세화 수용 / 표현·모델 증거 보존 / 실제 aging·공간 반환·재사용 통합 미완료**

[공통 규칙](dev3-task-designs.md) · [회수 정본](disk-storage-design.md#reclaim) · [참조 수명](disk-storage-design.md#undo-lifecycle) · [snapshot/horizon](disk-storage-design.md#snapshot-acquisition) · [보존 압력](disk-storage-design.md#retention-pressure) · [undo 표현](dev3-undo-format.md) · [상태 home](dev3-status-home.md)

<a id="scope"></a>
## 1. 목적·공급 경계·현재 상태

snapshot과 rollback이 요구하는 값을 보존하면서 불필요해진 undo·status·삭제 row/index·overflow·retired object를 bounded owner queue로 정리하고 내부 공간을 재사용한다. whole-pack GC·전체 root tracing·전체 DB rewrite·필수 online shrink를 도입하지 않는다. page 내부 cell 정리는 logged compaction이며 undo의 안정 slot index를 이동시키지 않는다.

3.14.1은 3.05.3·3.09.1·3.10.1·3.11.1의 해당 공급 뒤 eligibility/horizon·마지막 pin/영속 edge/DWB 조건을 판정한다. allocator의 실제 free/reuse는 그 판정을 소비하는 3.03.4이며, 3.14.2는 3.03.4와 end-to-end 반환·재사용·압력 해소를 인수한다. **3.14.1 → 3.03.4 → 3.14.2**를 유지하며 allocator가 전체 회수 완료를 기다리는 순환을 만들지 않는다.

현재 undo root의 Free/Live/Releasing·cursor와 status home incoming은 표현/검산 공급이다. 실제 count 증감·terminal queue/credit·stamp/cut/release·slot 재사용 capability는 아니다. 특히 [terminal 작업의 경계](dev3-status-home.md)는 durable queue owner/credit을 같은 WAL 그룹으로 인계하는 계약이 동결되기 전 실제 terminal opcode를 Unsupported로 둔다. 이 문서가 그 미완료 제안을 채택하거나 실행 승인하지 않는다.

<a id="requirements"></a>
## 2. 요구 ID와 보존 소유

| 요구 ID | 명세 | 인수·소비 |
| --- | --- | --- |
| 3.14-R1 | gate 안 published Q와 registry로 H를 구하고 실제 cut/reuse마다 다시 검사. status11/frontier10·빈 registry에서도 q10 복원 보존 | DS04·3.10 |
| 3.14-R2 | incoming은 실제 영속 edge multiset의 수. snapshot/rollback/queue/transient pin·creator identity를 count로 합치지 않음 | DS04·M01/M02 |
| 3.14-R3 | terminal 상태·head와 잔여 cleanup owner/credit을 원자 인계. ATT 제거·abort 예약 반환이 회수 작업 소유를 없애지 않음 | 3.09/11·DS05 |
| 3.14-R4 | stamp·cut·edge/count·queue/진행점·obligation을 해당 완결 WAL 그룹으로 연결. rollback이 cut 전 edge를 되살리지 않음 | DS04·RC-T13 |
| 3.14-R5 | zero incoming과 horizon/rollback/transient 조건을 모두 만족할 때만 Releasing. 새 edge가 Releasing을 target으로 삼지 않음 | DS03/04 |
| 3.14-R6 | outgoing role·chunk를 durable cursor로 한 번씩 반환. 이미 반환한 chunk를 다시 읽어 다음 위치를 찾지 않음 | DS03/05·RC-T14 |
| 3.14-R7 | index key 정리→row/overflow 참조 제거→slot/extent 반환 순서를 유지. old row overflow는 별도 logged retirement owner가 보존 | DS03/04·3.06/07/08 |
| 3.14-R8 | 마지막 영속 edge·transient pin·prepared/inflight flush claim·DWB retirement와 정확 generation을 적용 시 검산하고 3.03.4에 반환. stale/ABA·wrap 거절 | DS03/04/05 |
| 3.14-R9 | 낮은 공간에서도 기존 예약으로 bounded aging/rollback/flush 진행. 정상 거래 종료나 먼저 회수될 공간에 자기 완료를 의존하지 않음 | DS03·M06 |
| 3.14-R10 | 압력 때 새 effect 전 거절과 bounded 원인 진단. 실제 returned bytes/page token/재사용/FS 반환을 구분하고 long snapshot을 강제 만료하지 않음 | DS03/04·PF09 |

| 소유 종류 | 보존 조건·회수와의 관계 |
| --- | --- |
| snapshot horizon | H 및 더 오래된 등록 소유가 필요로 하는 모든 버전 재구성. undo record q 하나를 비교하는 규칙으로 축소 금지 |
| Active rollback | reader가 없어도 transaction head/previous-transaction과 before 값 유지. RC statement 사이 snapshot 해제는 거래 rollback 소유 종료가 아님 |
| 영속 incoming edge | row/index header·transaction head·두 previous role 등 실제 field마다 중복도 포함. 동일 target에 두 field면2 |
| status edge | row/index/catalog Transaction stamp·undo before Transaction stamp. undo creator의 TxnId/original status generation은 생성 identity이며 status lookup edge 아님 |
| transient reference pin | source latch 해제 전 등록한 exact 대상의 generation 수명. edge cut 후에도 마지막 pin 전 reuse 금지 |
| queue/retirement | cleanup 진행점·old overflow·retired object·orphan build의 영속 owner와 credit. membership만으로 값 필요성/Free를 추정하지 않음 |
| DWB/control/recovery | image copy 전부터 pending retirement의 영속 완료와 RAM finalize까지 유지하는 physical-address flush claim, pending DWB 주소, current/previous checkpoint·FILEMAP/WAL closure가 필요한 mapping/bytes. reader pin이 없거나 control pending이 비어 있어도 flush claim이 남으면 free/reuse 금지. page free와 control/WAL 삭제 조건을 합치지 않음 |

<a id="interfaces"></a>
## 3. 입력·출력·소유 인계 설계

| 논리 연산 | 입력 | 반환·인계 |
| --- | --- | --- |
| 후보 순회 | 영속 owner queue·segment cursor와 예약 workspace | bounded 후보·정확 identity·보존 이유. 전체 page/root 목록을 RAM에 올리지 않음 |
| eligibility 평가 | 후보, published Q·등록 snapshot, rollback/object/backup·edge/pin/DWB 관측 | 현재 적용 구간의 판정 또는 보류 이유. 오래 보관해 free 권한으로 재사용할 token 아님 |
| terminal 인계 | terminal status/head, 남은 undo/retirement 소유와 credits | durable queue owner·진행 책임. 실제 bytes와 원자 그룹은 미동결 |
| stamp/cut | expected live source·대상 generation·원자 removed/added edge 집합 | 새 stamp/link·count·queue/progress·WAL 인계. old creator identity는 유지 |
| release batch | Releasing root와 durable cursor/released/next, 대상 allocator 소유 | 다음 durable cursor와 실제 반환량. 한 batch 실패를 완료 전체로 집계하지 않음 |
| allocator 반환 | 최신 eligibility와 exact generation·flush claim/DWB/pin 부재 | 3.03.4의 Free/새 reservation/INIT 전이. 판정만으로 실제 bytes 차감하지 않음 |
| pressure 진단 | 제한된 소유 스냅샷·ledger 값 | 자원/owner 종류·q 또는 LSN·age·잘림 표시·실제 회수량. 진단은 회수 권한 아님 |

읽은 후보가 stale이면 최신 상태에서 다시 판정한다. 손상·count 불일치·유효해야 할 edge 대상 소실을 정상 보류/NotFound로 숨기지 않는다. queue/record view는 caller bytes를 빌리는 표현이며 pin·lock·write 권한을 대신하지 않는다.

<a id="state"></a>
## 4. 회수 상태와 원자 처리

```mermaid
stateDiagram-v2
  [*] --> Retained: terminal 또는 retirement owner 인계
  Retained --> Retained: horizon rollback edge pin DWB 보호
  Retained --> Cut: 최신 gate 판정과 logged edge 절단
  Cut --> Retained: 아직 다른 참조 또는 소유 존재
  Retained --> Releasing: incoming0과 별도 조건 충족
  Cut --> Releasing: incoming0과 별도 조건 충족
  Releasing --> Releasing: outgoing role 또는 chunk와 cursor 원자 진행
  Releasing --> EligibleFree: 모든 outgoing 처리와 마지막 소유 확인
  EligibleFree --> Free: 3.03.4 allocator 반환
  Free --> Reused: 새 generation reservation과 완전 INIT
  Releasing --> Fenced: 손상 불명 효과 또는 부분 적용
```

Retained/Cut/EligibleFree는 논리 설계 상태이며 새 disk state tag가 아니다. 기존 undo의 실제 state는 Free/Live/Releasing을 따른다. 최초 incoming0의 Live record도 별도 소유가 있으면 Retained다. generation을 증가시키는 정확한 시점은 allocator·undo slot 정본을 따르며 cancel/free 때 과거 generation으로 되감지 않는다.

```mermaid
sequenceDiagram
  participant Q as Queue owner
  participant G as Version gate
  participant H as Publication and registry
  participant S as Source and target pages
  participant W as WAL owner
  participant A as Allocator
  Q->>Q: 후보와 전체 자원 준비
  Q->>G: progress turn과 gate 획득
  Q->>H: published Q 읽고 publication 해제
  Q->>H: snapshot과 rollback과 pin 소유 확인
  Q->>S: 정렬된 latch와 identity 재검사
  Q->>W: edge와 count와 cursor의 완결 그룹 stage
  W->>S: 모든 page와 pageLSN 적용
  W-->>Q: DPT와 owner 진행 등록
  Q-->>G: latch와 gate 해제
  Q->>A: 최신 free 조건을 소비하는 후속 반환
  Note over Q,A: 독립 판정과 실제 free는 동일한 오래된 관측값으로 분리 실행하지 않음
```

마지막 allocator 호출은 별도 후속 그룹이면 그 적용 gate에서 조건을 다시 검사한다. 필요하면 source edge 제거와 반환을 같은 그룹으로 묶는 opcode를 동결한다. 그림으로 무계약 원자성을 가정하지 않는다.

<a id="algorithms"></a>
## 5. 단계별 알고리즘

### 5.1 후보·horizon·최신 eligibility

1. terminal/retired owner의 영속 queue나 segment cursor에서 bounded 후보를 읽는다. owner generation·진행 LSN·정확 record/page identity를 검산한다. 후보 발견과 actual reclamation을 구분한다.
2. 필요한 forward/release WAL·전체 pin·ref registry·workspace를 예약하고 latch 밖에서 page를 준비한다. 공유 maintenance12MiB는 3.10의 선택된 turn owner만 얻으며 대기 시 반환한다.
3. version-edit gate 아래 tree/page/append/registry 전에 publication에서 Q를 읽고 해제한다. registry에서 H=min(Q, 등록 q들)와 rollback/object/backup 소유를 구한다. 빈 registry를 무한대 horizon으로 해석하지 않는다.
4. 실제 source/target의 generation·현재 link·incoming multiset·pin·flush claim·DWB/retirement 조건을 재검사한다. flush claim은 page incarnation과 별도로 물리 주소의 재사용을 차단하며, claim 획득과 allocator free/reuse 판정을 같은 소유권 경계에서 serialize한다. image가 private workspace에 복사되어 page pin이 해제된 뒤에도 claim을 검사한다. 오래된 before-image에 남은 옛 link가 현재도 유효하다고 추정하지 않는다. 준비가 달라졌으면 모든 적용 latch/gate를 놓고 재탐색한다.
5. 필요한 모든 snapshot 값과 Active rollback을 재구성할 수 있을 때만 해당 stamp/cut/release를 허용한다. 미충족은 보류 이유와 진행 cursor의 계약에 따라 다음 bounded 작업으로 넘긴다. active 거래 종료를 기다리며 workspace를 붙잡지 않는다.

### 5.2 terminal status·commit-q stamp

1. Commit/AbortEnd가 terminal이어도 head/incoming과 cleanup 작업은 남을 수 있다. terminal owner는 queue record·잔여 credit·진행 소유의 원자 인계가 확정되기 전 ATT descriptor/예약을 모두 반환하지 않는다. 이 실제 opcode는 해당 계약 동결 전 Unsupported다.
2. horizon을 통과한 `Committed(q)` 거래의 현재 header에 자기 TxnRef가 남으면 logged commit-q stamp로 바꾼다. 다른 거래가 이미 대체했다면 그 before-image의 resolved stamp와 현재 소유를 확인한다. `Aborted`에는 commit q를 만들지 않으며, 남은 undo before stamp의 참조는 release와 참조 정리 경로에서 제거한다. logical row identity는 변경하지 않는다.
3. stamp의 TxnRef 제거와 status home224의 incoming 감소를 같은 그룹으로 처리한다. count-only 대상 home의 core를 actor의 descriptor core로 잘못 복사하지 않는다. status generation·exact target과 원래 core를 검산한다.
4. undo creator identity는 이력 식별용으로 남긴다. creator를 edge로 세거나 이미 해제한 status를 creator로 재조회하지 않는다. 실제 before Transaction stamp만 status 수명 edge다.
5. 모든 실제 TxnRef edge·head/queue 소유·pin 등이 끝난 뒤에만 status slot 반환을 검토한다. terminal decode·count0만으로 Free를 만드는 shortcut을 허용하지 않는다.

### 5.3 logged cut와 rollback의 공존

1. version chain에서 오래된 suffix 후보를 정하되 H의 모든 snapshot과 Active rollback이 요구하는 경로를 독립적으로 확인한다. previous-version과 previous-transaction은 별도 role이며 같은 대상이면 중복도2를 유지한다.
2. source link를 Cut으로 만들고 target incoming을 감소시키며 queue/진행점·obligation을 같은 완결 그룹에 담는다. removed edge가 실제 old count에 포함되었는지 검사하고 checked 산술을 적용한다.
3. rollback은 현재 previous-version 상태를 소비한다. cut 전 저장된 참조를 무조건 부활시키지 않는다. CLR의 row/index 효과와 head/edge/count/UndoNextRef가 현재 보존 소유에 맞는지 3.09와 함께 확인한다.
4. 필요한 target이 이미 Releasing/Free이거나 유효 참조를 찾을 수 없으면 invariant failure/Fenced다. count 수치가 맞는다는 이유만으로 필요한 과거 값 손실을 허용하지 않는다.

### 5.4 outgoing release·chunk 반환

1. incoming0과 horizon/rollback/transient pin 조건을 만족하고 queue가 작업을 소유하면 Releasing으로 전환한다. 새 incoming edge를 연결하는 경로는 이 상태를 거절한다.
2. 기존 undo 형식의 고정 cursor를 따른다. 0→1은 previous-version, 1→2는 previous-transaction, 2→3은 before status 참조를 처리한다. 없는 edge도 처리 완료로 진행하며 각 role의 제거/count 변화와 cursor 갱신을 같은 WAL 그룹으로 묶는다.
3. cursor3에서 before stamp region은 zero다. chunk가 있으면 같은 그룹으로 release-next=head.root를 설치한다. Releasing의 raw before metadata를 reader/rollback 값으로 노출하지 않는다.
4. 현재 release-next의 chunk identity/index/header를 검산하고 next를 확보한다. allocator 반환·next 인계·released+1을 같은 그룹으로 처리한다. 이미 반환한 앞 chunk를 읽어 진행을 재구성하지 않는다. 마지막은 released=count와 next zero를 함께 남긴다.
5. cursor3/count완료 뒤 queue 소유·마지막 pin·해당 page 반환에 필요한 flush claim 해제·DWB retirement와 관련 참조 제거를 다시 확인하고 root slot을 반환한다. cell offset compaction은 가능해도 안정 directory slot index는 유지한다. 재점유는 checked generation 증가이며 MAX는 재사용하지 않는다.

### 5.5 index·row·overflow·object의 반환 순서

1. snapshot에 필요한 versioned index key/presence를 먼저 보존/정리한다. 논리 삭제라는 이유로 tree entry를 즉시 제거하지 않는다. 그 뒤 row/tombstone과 overflow 참조를 제거한다.
2. 교체된 old row overflow는 undo root에 저장되지 않는다. canonical before의 별도 undo chunks가 있다는 사실만으로 old chain을 즉시 반환하지 않는다. 별도 logged retirement owner가 head·allocator inventory·snapshot/rollback 필요성을 인계받아야 한다.
3. retired table/index/anchor와 orphan build는 catalog generation·allocation ownership·transaction status를 검산하고 필요한 old snapshot/object/backup 소유가 끝난 뒤 bounded batch로 정리한다. 같은 이름의 재생성은 다른 generation이다.
4. 구조 unlink는 retirement 등록까지 공급한다. eligible page/slot/extent는 3.03.4가 최신 조건과 해당 물리 page의 flush claim 해제·DWB whole-batch retirement를 확인하여 Free로 만들고 새 allocation은 새 incarnation INIT으로 보호한다. pending 선택 전 준비된 old-generation image와 retirement 선택 뒤 RAM finalize 이전의 claim도 반환을 막는다. mapping/control/WAL 보존은 해당 owner가 끝내기 전 제거하지 않는다.
5. 내부 Free page 증가, 실제 새 allocation에 재사용된 page, owned file bytes 감소, filesystem 반환량을 별도로 계측한다. extent 안 page가 Free여도 이미 실현된 file bytes는 계속 charged일 수 있다. S1은 online shrink/OS 반환을 필수로 요구하지 않는다.

### 5.6 WAL 보존과 회수 압력

1. WAL floor와 undo horizon을 구분한다. dirty undo/status/allocator의 recLSN과 선택 checkpoint closure가 남아 있으면 그 redo WAL을 보존한다. 오래된 Active 거래의 시작 WAL 자체를 영구 undo 입력으로 요구하지 않는다.
2. checkpoint+이후 WAL에서 같은 prefix의 head/progress·obligation·allocator 소유를 복원할 수 있을 때 begin WAL을 놓는다. 그 뒤 runtime abort와 crash/restart loser undo가 durable UndoRef chain으로 가능해야 한다. 새 foreground 한도를 낮췄다고 기존 소유를 줄이지 않는다.
3. 긴 reader와 churn으로 부족하면 다음 statement 첫 effect 전에 해당 자원을 거절한다. 이미 시작한 작업은 기존 오류/rollback 규약을 따른다. 내부 무한 재시도·snapshot 시간 만료·undo 덮어쓰기는 금지한다.
4. timeout/cancel은 해당 소유자의 실제 종료 경로가 pin/undo를 해제한 뒤만 eligibility를 바꾼다. owner 해제 후 실제 반환이 충분하면 admission을 재개하고, 여전히 부족하면 계속 거절한다.
5. 제한된 진단에 capacity/charged/reserved/requested, 보존 이유/owner generation, snapshot q 또는 WAL LSN, age·eligible/reclaimed/reused 수량을 넣는다. 겹친 보존 범위를 중복 합산하지 않고 잘림을 표시한다. 행/SQL/bind 원문을 넣지 않으며 진단 실패가 기존 오류 certainty나 rollback 진행을 바꾸지 않는다.

<a id="resources"></a>
## 6. 자원·실패·재시작

| 자원·실패 경계 | 계산·처리 의무 |
| --- | --- |
| queue/credit | terminal·retirement record와 bounded batch/cursor bytes, 다음 stamp/cut/release·WAL·allocator 작업의 잔여 비용. record format과 owner별 최대 queue 규모는 F01/F05 동결 전 미정 |
| 적용 집합 | source/target/head/status/count/allocator/queue page를 실제 identity로 합산. same target 두 edge는 count2지만 동일 page pin은 실제 identity가 같을 때만 공유 |
| 작업 메모리 | 단일 maintenance12MiB, 전용 flush4MiB, bounded registry/diagnostics·전체 page pin·nested codec scratch. 전체 undo/page 목록·root mark scratch 없음 |
| disk/WAL | owned8GiB 안 protected1GiB, retained WAL+예약≤256MiB. 이미 쓴 CLR/aging WAL은 실제 회수 전 charged; extent bytes와 내부 token 중복 charge 금지 |
| 준비 부족·stale 후보 | 첫 effect 전 보류/거절·재탐색. 공간이 생길 것이라는 가정으로 예약을 생략하거나 자기 rollback 완료를 aging 선행에 의존시키지 않음 |
| count/identity/cursor 손상 | underflow/overflow·순서 불일치·필요 target 소실을 보류/정상 부재로 낮추지 않음. 원인 보존·Fenced/RecoveryRequired |
| stage 뒤 partial apply·불명 I/O | 공통 fence·uncertain charge/소유 보존. Drop은 숨은 free/sync/완료 인계를 하지 않음 |
| 재시작 | logged cut/count/cursor/allocator 전이를 REDO하고 같은 role/chunk를 두 번 감소·반환하지 않음. crash 뒤 volatile pin 소멸과 durable rollback/queue 소유 복원을 구분 |

회수자는 FIFO turn으로 안정된 준비와 bounded 적용을 얻으며 준비 단계의 flusher를 막지 않는다. 한 batch가 끝나면 ticket/workspace를 반환한다. 실제 I/O가 종료하고 actor가 공정하게 실행되는 무장애 구간의 진행을 판정하며 영구 장치 실패나 계속되는 crash의 완료를 약속하지 않는다.

<a id="work"></a>
## 7. 하위 작업과 완료 조건

| 기존 ID·세부 단위 | 직접 공급·산출물 | 완료 경계 |
| --- | --- | --- |
| 3.14.1.a eligibility/horizon | 3.05.3·3.09.1·3.10.1·3.11.1 해당 공급 → owner별 보존 판정·최신 gate 재검사 | status/frontier gap·incoming/별도 소유·DWB 조건 검산. 실제 allocator 반환 완료를 선행으로 요구하지 않음 |
| 3.14.1.b owner queue·stamp/cut 준비 | .1.a와 terminal/queue F01/F02/F05 → 영속 소유/credit 인계, count·진행 그룹 | 미동결 terminal 실행 금지 유지. 논리 eligibility와 실제 공간 반환을 구분한 3.03.4 소비 계약 공급 |
| 3.03.4 외부 소비 | 3.14.1 + allocator/INIT/WAL/flush 공급 | 최신 eligibility의 실제 free·새 generation/INIT. 이 문서가 allocator 구현을 복제하지 않음 |
| 3.14.2.a release·retirement 통합 | 3.03.4 + .1 → outgoing cursor/chunks·index→row/overflow→slot/extent의 반환 | 실제 allocation state·참조 multiset·stale reference 거절·재사용량 확인 |
| 3.14.2.b 압력·장기 보존·복구 | .2.a와 관련 3.12/3.13 공급 → owner 해제/재개·begin-WAL 제거·재장애 | 긴 reader/Active rollback 보존, 실제 returned/reused·fixed peak·현재-build reopen. 전체 DEV3 인수와 구분 |

<a id="acceptance"></a>
## 8. 독립 인수 사례

모든 case는 앞으로의 실행 명세이며 이번 문서 작성으로 NOT_RUN이다. 기대 graph는 field별 edge multiset, 기대 snapshot/rollback 값은 별도 이력, bytes/token ledger는 actual file inventory에서 독립 계산한다. production count/queue 출력을 정답으로 삼지 않는다.

| Case | 고정 입력·독립 판정 | 요구·인수 |
| --- | --- | --- |
| R14-01 | q10 선택/등록 사이 ager와 status11/frontier10·빈 registry. gate 연속 보유·H10·A@10 재구성 | R1, DS04 |
| R14-02 | 같은 UndoRef를 두 previous role이 참조, status before edge와 creator만 있는 record를 혼합. incoming multiset·creator 비참조 일치 | R2, DS04 |
| R14-03 | terminal count0/head존재·count>0/head없음·queue credit 미인계. Free 금지·terminal 미동결 경로 Unsupported | R3, DS05 |
| R14-04 | A commit→B update→A aging/cut→B statement/full rollback→slot reuse. 필요한 값과 현재 link 보존·cut edge 부활 없음 | R4, DS04·RC-T13 |
| R14-05 | incoming0이지만 snapshot/Active rollback/pin 등 외부 보존 소유 존재 시 release 보류. 이 조건이 해소되고 release queue가 작업을 소유하면 Releasing 진행; queue 작업 소유와 마지막 참조가 끝나기 전 root slot의 final Free 금지 | R5, DS03/04 |
| R14-06 | Releasing cursor0/1/2/3 각각 crash·redo, 동일 target 두 role. count 중복 감소 없음·새 edge target 거절 | R5/6, RC-T14 |
| R14-07 | chunk 첫/중간/마지막 반환 전후 crash와 반환 page 재사용. next/released 원자 인계, 반환된 page 재읽기 없음 | R6, DS03/05 |
| R14-08 | old row overflow 교체·DELETE·snapshot·rollback, 별도 undo chunks. retirement inventory 누락/즉시 반환 mutant 거절 | R7, DS03/04 |
| R14-09 | index presence aging→row tombstone→overflow→slot/extent, drop/recreate·orphan build. owner generation과 반환 순서 보존 | R7/8, DS03/04 |
| R14-10 | source ref 복사→pin→unlatch→cut→마지막 pin 해제, DWB pending/retirement 동시 경합. free 적용 시 최신 조건 검사 | R8, DS04/05 |
| R14-11 | undo/status/page generation MAX·stale locator/UndoRef/TxnRef·slot compaction. wrap/ABA 거절·안정 slot index 유지 | R8, DS03/04 |
| R14-12 | 긴 reader+churn으로 undo/owned/WAL 각 경계 도달→새 effect 거절→기존 값 보존→owner 해제→실제 회수/재사용→admission | R9/10, DS03/04 |
| R14-13 | Active 거래의 begin WAL을 실제 삭제한 runtime abort 및 crash/restart. head/obligation/allocator closure로 완전 undo | R4/9, RC-T13/14 |
| R14-14 |4MiB buffer·낮은 quota·여러 유지보수와 flusher 필요 상황. partial pin/workspace 대기 없음, rollback이 선행 aging 없이 완료 | R9, DS03·M06 |
| R14-15 | release/CLR/terminal queue·current/previous closure 인계 cut와 반복 crash. 같은 page/token 중복 반환·잔여 credit 소실 없음 | R3/6/9, RC-T14 |
| R14-16 | 진단 최대 항목·잘림·겹친 소유·q/LSN·실제 file bytes/Free pages/reused/FS 반환. bounded 생성·오류 certainty 보존 | R10, DS03·PF09 |
| R14-17 | old generation image copy 뒤 page pin 해제/pending 선택 전, pending retirement 영속화 뒤 RAM finalize 전을 각각 barrier로 멈추고 free/reuse 요청. 두 구간 모두 physical-address flush claim이 반환을 막으며, claim 종료 뒤만 새 generation INIT 가능. pending/reader pin만 확인하는 mutant와 old image가 새 generation home을 덮는 결과를 거절 | R8, DS03/05 |

BEGIN WAL 회수와 actual free가 없는 표현 시험은 R14-13/실제 반환 PASS가 아니다. 변경한 production 경로와 남은 통합 의무만 선택하며 동일한 무변경 표현/모델·workspace/leaf/하위 wrapper는 반복하지 않는다. 기존 실패·중단·미실행 기록을 새 성공으로 대체하지 않는다.

<a id="sources"></a>
## 9. 실제 소스 진입점

| 파일·symbol | 현재 공급·남은 연결 |
| --- | --- |
| [undo_record.rs](../../crates/engine/src/storage/undo_record.rs): `UndoCell`, `linked_edges`, `before_status_ref`, `validate_release_chunk` | Free/Live/Releasing·edge·release cursor의 무효과 검산. live queue·WAL release·수명 집행은 미공급 |
| [status_home.rs](../../crates/engine/src/storage/status_home.rs): `StatusHome`, `adjust_incoming`, `decode_status_home_page` | core+incoming224-byte 표현·순수 원자 delta 계산. terminal owner/credit 및 slot 반환 capability 아님 |
| [allocation_page.rs](../../crates/engine/src/storage/allocation_page.rs), [allocation_reservation.rs](../../crates/engine/src/storage/wal_io/allocation_reservation.rs) | allocator 표현·제한 reserve/cancel 공급. 실제 eligible free/reuse는 3.03.4 |
| [overflow.rs](../../crates/engine/src/storage/overflow.rs), [ownership.rs](../../crates/engine/src/storage/ownership.rs), [obligation.rs](../../crates/engine/src/storage/obligation.rs) | chain·inventory·descriptor 표현과 검산. old overflow retirement/terminal queue의 새 영속 owner가 이미 있다고 가정하지 않음 |
| [doublewrite_control.rs](../../crates/engine/src/storage/doublewrite_control.rs) | DWB control 표현. 실제 whole-batch retirement는 3.05.3 소비 |
| [dev3_undo_model.rs](../../crates/testing/reference/tests/dev3_undo_model.rs), [dev3_snapshot_model.rs](../../crates/testing/reference/tests/dev3_snapshot_model.rs), [dev3_recovery_model.rs](../../crates/testing/reference/tests/dev3_recovery_model.rs) | 기존 독립 유한 상태 검산. 실제 aging/free/thread/reopen 인수로 확대하지 않음 |

<a id="freeze"></a>
## 10. 착수 전 추가 동결

| 항목 | 동결 의무 |
| --- | --- |
| F01 queue/retirement | terminal owner·credits·status/head 인계, old row overflow retirement record·allocator inventory, catalog/retired object/orphan 진행 record. bytes/IDs/checksum·원자 효과 미동결 |
| F01/F02 semantic 그룹 | stamp·cut·outgoing role release·chunk 반환·slot Free·queue enqueue/dequeue와 progress/obligation의 exact before/after. count와 durable cursor가 분리되는 중간 상태 금지 |
| F02 eligibility API | 최신 gate에서 사용할 소유/guard, source pin·physical-address flush claim·DWB retirement·queue claim 경합, claim 획득/해제와 allocator free/reuse의 원자 조정, stale 후보·Releasing target·MAX generation 처리·오류 효과 |
| F03/F04 | independent graph/snapshot/rollback/byte ledger·negative mutants, 실제 thread barrier와 stage/DWB/home/queue/allocator/restart cut, begin WAL 제거 witness |
| F05 | queue 크기·batch당 page/edge/chunk 수·WAL/credit·메모리·frame/FD·retirement worst-case, maintenance/progress 예약·반복 restart 고정 peak·bounded 진단 항목수 |
| F06/F07 | 실제 3.05.3/3.09.1/3.10.1/3.11.1 수신과3.03.4 소비 계약, source/case identity·독립 검토·단일 owner와 병합 순서 |

미동결 queue/credit/retirement를 표현 tag의 존재로 자동 채택하지 않는다. 회수 가능성 판정과 실제 반환·재사용·압력 해소를 각각 증거로 남기고, 필요한 소유가 남아 있는 동안 안전한 보류를 제품의 무조건 쓰기 가용성으로 과장하지 않는다.

<a id="retention-pressure-model"></a>
## 비교 분석 후속 — 장기 reader의 보존 압력과 진행

보존 Undo bytes U는 `U(t)=U(0)+∫(생성률−확인된 반환률)dt`다. reader/horizon 때문에 반환이0이고 초당 λ회 변경·회당 실제 retained u bytes이면 남은 예산 M의 소진 시간은 `M/(λu)`다. M=512MiB, λ=1000/s에서 u=1KiB/16KiB이면524.288초/32.768초다. 고정 비율의 예시이며 실제 workload 예측이 아니다.

실행 전 owner별 horizon/pin/queue/claim 지연과 charged/reserved 잔량을 기록한다. pressure에서는 새 foreground admission을 제한하면서 이미 admission한 rollback·flush·release가 사용할 예약을 보존한다. 긴 reader를 임의로 무효화하거나 아직 eligible하지 않은 Undo를 지우지 않는다. reader 종료 후 eligibility→edge cut→retirement→확인된 반환→새 generation 재사용을 각각 관측하고, 불명 삭제는 charge 반환으로 세지 않는다. 새 기능의 작은 pressure/재사용 case와 S5의 장기 reader·부하 조합을 분리한다.
