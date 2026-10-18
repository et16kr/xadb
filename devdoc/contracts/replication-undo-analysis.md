# XADB — recovery undo의 로그 보존과 RP 제약 검토

2026-10-18 범위 갱신: undo-store 저장 결정은 [DEV3-DESIGN-003](disk-storage-design.md#undo)이 채택한다. 아래의 “현재 private/COW 유지·미래 undo-store”는 DEV2 기준선/이전 제안의 범위다. DEV3는 영속 undo와 독립 redo 보존을 구현해야 하며 실행은 NOT_RUN이다. RP capture/spool·archive·PITR는 계속 후속 범위이며 DEV3 초기 선행에 넣지 않는다.




**사용자가 원하는 방향은 가능하다.** undo 정보를 redo에 남기되, checkpoint 이후 rollback/recovery undo가 과거 redo를 역탐색하지 않고 **복구 가능한 undo tablespace의 거래별 chain**을 사용하도록 하면, 장기 미완료 거래 때문에 과거 로그 전체를 유지하는 제약을 제거할 수 있다. 목적은 로그에서 before value를 없애는 것이 아니라 **undo를 수행하기 위해 원본 로그 파일을 붙잡는 의존성의 제거**다.


Oracle은 이 구분의 좋은 비교 대상이다. rollback은 undo segment와 transaction table을 사용하고 online redo는 checkpoint·archive 조건 후 재사용한다. 반면 GoldenGate는 자체 재시작에 필요한 archived redo를 별도로 보존하며, Bounded Recovery로 장기 거래의 capture 상태를 복원한다. **undo 공간, online redo 재사용, archived redo 삭제는 서로 다른 수명이다.**

<a id="section-1"></a>

## 1. 보존 owner 설계 제안

아래는 디스크 엔진의 미래 replication·undo/redo 보존을 위한 일반 설계 제안이다. 현재 private/COW 커널과 개발 순서를 변경하지 않으며 실제 복제·undo-store 엔진은 없다. 구현과 비교 조사는 [공개 참고 자료](../reference/opensource-references.md)를 출발점으로 별도 기록한다.

<a id="section-2"></a>

## 2. Undo와 redo 보존

원본 로그를 회수하려면 rollback·redo·allocator·transaction 상태를 복구할 전체 의존성이 다른 durable 표현으로 옮겨져야 한다. undo page flush 하나만으로 모든 소비자의 로그 의존성이 해소된다고 가정하지 않는다.

<a id="section-3"></a>

## 3. 복제 소비자의 보존

Read-ahead나 전송 위치는 durable restart 위치가 아니다. 열린 transaction payload·decoder·savepoint·target 재시도 상태가 살아 있어야 원본 로그 pin을 해제할 수 있다. backlog 예산 고갈은 backpressure·명시 오류·선언한 reseed 정책으로 처리한다.

<a id="section-4"></a>

## 4. 검토할 비교 질문

각 공개 DBMS가 local redo, rollback, replication restart, archive/backup의 보존 owner를 어떻게 나누는지 조사한다. 자료별 구현 차이를 XADB의 채택 사실로 혼동하지 않는다.

<a id="section-5"></a>

## 5. XADB에서 분리할 세 가지 복구 기준

아래 이름은 제안하는 설계 개념이며 구현된 API·wire format이 아니다.

| 저장소 | 영속화할 내용 | 기존 로그를 놓을 수 있는 근거 |
|---|---|---|
| Undo store | 거래별 undo chain, TxnId/generation, head, 상태, rollback 진행점 | transaction undo가 삭제 대상 로그를 참조하지 않고 여기서 계속됨 |
| Recovery checkpoint | data·undo·거래 상태·allocator/catalog의 일관된 복구 기준과 redo 재개 위치 | 삭제 범위의 변경을 재구성하는 데 원본 로그가 불필요 |
| Capture checkpoint/spool | 열린 거래의 변경 조각·부분 rollback·schema·decoder 상태와 이후 읽을 위치 | RP 장애 후 거래 시작 로그를 다시 읽지 않고 재개 가능 |

### 5.1 Undo store만 먼저 바꿀 때

거래의 `undo_head`를 물리 로그 LSN에서 안정적인 `UndoRef`로 바꾸고, record의 이전 연결·소유 거래·재사용 generation을 보존한다. DISK·index·catalog·allocator·NTA 등 **취소 가능한 모든 효과**가 이 chain 또는 동등한 durable 상태로 복구되어야 한다. row undo만 옮기고 metadata undo가 과거 로그를 요구하면 pin은 남는다.

undo page·transaction head/state·allocation metadata의 쓰기는 redo로 보호한다. checkpoint를 durable하게 선택한 뒤에만 과거 로그의 undo pin을 해제한다. rollback 중 crash 후에도 이미 되돌린 효과를 중복 적용하거나 건너뛰지 않도록 보상 기록/진행점의 재복구 계약이 필요하다. page의 write 호출 성공만으로 durable checkpoint를 선언하지 않는다.

이렇게 하면 **RP가 없을 때 장기 거래의 시작 로그를 회수하고도 abort/restart undo할 수 있어야 한다.** 이것이 첫 번째 독립 인수 기준이다. RP를 켰을 때의 회수까지 이 단계 성공으로 주장하지 않는다.

### 5.2 RP도 과거 원본 로그를 놓게 할 때

기존 로그를 읽는 capture 경로를 유지할 수 있다. 반드시 모든 DML에 새 종류의 logical WAL을 즉시 도입할 필요는 없다. 우선 **이미 읽은 로그의 열린 거래 상태를 durable spool/checkpoint로 보존**하여 원본 로그의 의무를 인계하는 안이 사용자 목적에 직접 대응한다.

```text
원본 redo/undo-redo ── capture ── durable capture checkpoint + spool
       │                                  │
       │ DB checkpoint도 통과             ├─ 열린 거래: COMMIT/ABORT/부분 rollback 대기
       │ 모든 다른 pin도 해제             └─ commit된 거래: 전송·target apply
       └──────────── 원본 로그 회수 가능
```

안전한 순서는 ① capture payload와 미완료 거래/decoder 상태 작성 ② 파일·namespace 영속화 ③ 그 자료와 재개 위치를 가리키는 manifest/checkpoint 영속 선택 ④ 원본 로그 pin 전진이다. **최신 read LSN만 기록하고 buffer를 잃는 방식은 불충분하다.** checkpoint 직전 상태가 불완전하면 이전 checkpoint와 필요한 로그를 사용한다. 이미 회수한 로그를 필요로 하는 fallback을 성공 경로로 남겨두면 안 된다.

열린 거래가 나중에 commit하면 spool의 전체 살아남은 연산을 내보낸다. abort면 폐기하고, savepoint/statement rollback은 해당 부분만 제거/무효화한다. LOB/row-piece가 여러 로그에 걸치면 미완성 조립 상태도 복구해야 한다. 이미 target에 전달한 미commit 작업이 있다면 target-local rollback과 restart 계약도 함께 유지한다.

spool이 source commit의 유일한 원본이 아니라도, **원본 로그를 회수한 뒤에는 spool이 복제 복구의 필수 자료**가 된다. source/target 장애 범위별 내구성과 backup·손상 감지·재동기화 정책을 명시한다. 느린 target이 source WAL을 잡는 대신 spool을 키울 수 있으므로 공간 의무가 사라지는 것은 아니다. 다만 오래된 거래 하나 때문에 무관한 거래의 전체 WAL suffix를 보존하는 결합은 줄일 수 있다.

이를 위해 spool도 거래별 extent/segment 또는 안전한 compaction으로 회수할 수 있어야 한다. 가장 오래된 열린 거래 때문에 spool의 모든 후속 파일을 붙잡는 구조라면 보존 문제가 다른 디렉터리로 이동할 뿐이다. 완료된 다른 거래의 공간을 회수하면서 열린 거래의 필수 자료만 남기는 경우를 RU-11/14에서 검증한다.

### 5.3 회수 판단은 owner별로 계산한다

```text
삭제 가능(segment)
  = local redo recovery에 불필요
  ∧ transaction undo가 이 segment 없이 가능
  ∧ RP capture/restart가 이 segment 없이 가능
  ∧ backup/PITR/physical standby/기타 owner의 보존 의무 충족
```

단일 로그 stream의 연속 경계로 단순화할 수 있을 때만 `min(local_redo, undo_dependency, capture_restart, archive/backup/standby)` 같은 low watermark를 사용한다. undo checkpoint가 완전하면 오래된 transaction begin LSN은 undo_dependency에 남지 않는다. multi-stream XADB에서는 기존 계약대로 참조 집합/stream별 위치를 사용하며 lane LSN을 숫자 하나로 비교하지 않는다.

DB undo pin 해제와 RP pin 해제는 별도로 관측해야 한다. 필요한 지표는 `oldest_active_txn`, `oldest_undo_log_dependency`, `local_recovery_start`, `capture_read_position`, `capture_durable_restart`, `target_durable_progress`, owner별 보존 byte, undo/spool byte다. 단순한 “RP lag 0”으로 로그 삭제 가능을 판단하지 않는다.

<a id="section-6"></a>

## 6. 현재 private/COW 설계와의 관계

현재 XADB는 commit 없는 private 변경을 공유 checkpoint에 포함하지 않으므로 crash loser의 shared-page undo를 위해 오래된 WAL을 읽는 전제 자체를 피한다. 다만 runtime statement rollback·live snapshot·private allocation 보존은 남고, L2의 오래 준비한 payload는 commit 전까지 필요한 reservation을 유지한다. 따라서 현재 설계도 “active 거래가 전혀 공간을 붙잡지 않는다”고 표현하면 안 된다.

사용자가 원하는 undo tablespace를 채택한다면 저장 방식 ADR에서 private/COW와의 역할을 확정해야 한다. **RP 측에서는 이를 금지할 본질적 제약이 없다.** RP가 바라볼 계약은 undo page의 위치가 아니라 변경 정보의 완전성, transaction 경계, durable capture restart와 보존 인계다.

새 logical ChangeBatch를 commit 경로에서 직접 만드는 대안도 가능하지만 이번 결론의 필수 조건은 아니다. 그것만으로 기존 로그 기반 장기 거래의 restart pin이 자동 해결되는 것도 아니다. source restart·capture 재구성과 GC까지 함께 검증해야 한다. root 참조만 있는 DISK journal을 logical replication 자료로 오인하지 않는 조건도 유지한다. WAL-first의 완전한 redo를 사용하는 경우에도 그 자료의 logical capture 완전성과 재시작 보존 계약은 따로 확인한다.

<a id="section-7"></a>

## 7. 기존 복제 설계의 세분화

모든 작업은 **미구현 제안**이다. RP-U ID는 기존 XADB task 번호와 완료 gate를 대체하지 않는다. 정확한 dependency와 R11 매핑은 JSON에 있다.

| ID | 작업 | 완료 판단 |
|---|---|---|
| RP-U01 | 기존 undo·checkpoint·RP 보존 owner 분해 | 매체·거래·옵션별 원본 로그 필요 사유가 구분됨 |
| RP-U02 | undo tablespace chain·거래 상태·모든 undo 효과 설계 | 과거 로그 없이 rollback할 수 있는 durable closure 명세 |
| RP-U03 | undo/data checkpoint·반복 recovery·로그 회수 | RP off 상태에서 열린 거래의 시작 로그 삭제 후 정상 undo |
| RP-U04 | capture의 열린 거래·decoder·savepoint 상태 inventory | 재시작에 필요한 전체 상태와 원본 자료 목록 확정 |
| RP-U05 | durable capture checkpoint/spool·원본 로그 인계 | commit 전에도 보존 인계 가능, crash 후 원본 시작점 불필요 |
| RP-U06 | target progress·dedup·미commit apply/rollback | 재전송·target crash에도 부분/중복 확정 없음 |
| RP-U07 | 공통 retention·공간 제한·archive·reseed | owner별 해제, 공간 부족 때 조용한 누락 없음 |
| RP-U08 | bootstrap·DDL/LOB·충돌·부분 구독 통합 | 기존 R11 업무 의미를 유지하며 재시작 가능 |
| RP-U09 | 동기 모드·failover·관측·장기 거래 qualification | 선언한 RPO와 실제 owner별 보존량 검증 |

RP-U02/03은 undo tablespace 저장 대안을 채택할 때 구현한다. 기존 COW 기준선의 필수 선행 작업으로 강제하지 않는다. RP-U04/05는 기존 RP의 parser를 유지하는 안과 typed logical capture 안 모두에 적용되는 재시작 의무다.

<a id="section-8"></a>

## 8. 인수 계획과 이번에 수행한 검증

아래 14개는 모두 **NOT_RUN**이다. 독립 controller의 거래 이력과 실제 source/target 재시작 결과로 검사해야 한다. source 조사나 DEV0/DEV1 harness 결과는 실행 증거를 대신하지 않는다.

| ID | 시나리오 | 필요한 결과 |
|---|---|---|
| RU-01 | RP off, 장기 갱신 T + 많은 다른 commit, checkpoint 후 T 시작 로그 삭제, crash | undo store에서 T 전체 rollback, ACK 거래 보존 |
| RU-02 | 같은 조건에서 정상 ROLLBACK·savepoint rollback | 삭제한 로그를 읽지 않고 정확한 상태 복원 |
| RU-03 | undo page/head/allocator/data/checkpoint 각 영속화 경계의 crash | 불완전 checkpoint로 로그 회수하지 않음 |
| RU-04 | recovery undo 중 반복 crash | 중복 undo·누락 없이 종료, rollback 진행점 일치 |
| RU-05 | RP on, 읽기만 완료·spool 미영속 상태에서 GC 시도 | 원본 로그 삭제 거절 |
| RU-06 | 열린 거래 spool checkpoint 후 시작 로그 삭제, capture crash, 이후 COMMIT | 시작 로그 재독 없이 정확한 전체 거래 전송 |
| RU-07 | RU-06 뒤 ABORT·부분 rollback·statement 실패 | 취소 범위가 target에 확정되지 않음 |
| RU-08 | row-piece/LOB 조립 중 checkpoint와 crash | 미완성 상태 재개, 해제된 포인터 없이 정확한 값 |
| RU-09 | spool payload/manifest/pin 이동 각 경계 crash 및 손상 | 유효 old/new 기준 선택 또는 명시 오류, 누락 건너뛰기 금지 |
| RU-10 | target send/receive/spool/apply/progress 경계 crash | 계약한 내구성만 ACK, 재전송의 중복 효과 없음 |
| RU-11 | 장기 RP 중단·spool full·archive/backup/standby pin | owner별 보존, 감속/거절/명시적 reseed |
| RU-12 | bootstrap·DDL/drop-recreate·PK 변경·다중 writer before 충돌 | 기존 R11 의미와 schema/identity 유지 |
| RU-13 | mixed DISK·index/catalog/NTA undo, 선택 시 XA prepared | 남아 있는 log-based undo 의존 탐지; 미지원 XA는 명시 거절 |
| RU-14 | 동일 내구성에서 기존 pin·undo store·capture checkpoint 비교 | WAL/undo/spool 총 byte·checkpoint I/O·p99·rollback/recovery 시간 분리 측정 |

특히 RU-01/06은 **실제로 과거 로그가 없는 소유 시험 환경**에서 수행해야 한다. 로그가 우연히 남아 있는 테스트는 독립성을 입증하지 못한다. 반대로 RU-05/09는 조기 삭제/진척만 기록하는 mutant를 거절해야 한다. 로그 보존 감소를 총 저장 공간 감소나 성능 개선과 동일시하지 않는다.
