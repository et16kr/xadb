# DEV3 구현과 잔여 설계의 Oracle Altibase PostgreSQL 비교 리뷰

2026-11-04 · **재검토 반영 / 정적 리뷰와 개선 제안 / 계약·일정 변경 없음 / 제품 시험 새 실행 없음**

[저장 설계 정본](../contracts/disk-storage-design.md) · [16개 작업 명세](../contracts/dev3-task-designs.md) · [구현 계획](../plans/dev3-implementation-plan.md) · [일정 기준선](../plans/dev3-schedule-baseline.md) · [PR #48의 방향 리뷰](dev3-oracle-direction-review-20261103.md)

DEV3의 저장·복구 뼈대는 타당하다. mutable page, 단일 WAL, 영속 Undo, steal/no-force, 논리 rollback과 redo-only CLR을 유지할 이유가 충분하다. 다만 **현재 첫 Row profile의 비용과 거래 제약을 그대로 일반화하면, 기능 확대보다 먼저 WAL 재검증·로그량·그룹별 영속 대기·쓰기 직렬화·공간 효율이 한계가 될 가능성이 높다.** 일반 행 변경·인덱스·동시성 구현 전에 이 경계를 설계 작업으로 분리하는 편이 좋다. 재검토에서도 기본 방향은 유지하되, Root page 분산이나 로그 크기 축소만으로 일반 DBMS의 동시성·처리량을 얻을 수 없다는 점을 더 분명히 했다.

비교 기준은 Oracle 19c, Altibase 7.1의 디스크 저장 구조, PostgreSQL 18이다. 제품별 구현을 복제하는 것이 아니라 트랜잭션 의미, 복구 보장, 정상 실행 비용과 확장성을 대조한다. 이 리뷰의 제안을 채택해 형식이나 의미를 바꾸는 작업은 해당 소유 계약에서 별도로 구체화한다.

<a id="scope"></a>
## 1. 기준과 확인 범위

| 항목 | 기준 |
| --- | --- |
| 검토 source | main `2397633015e7410bbc591c81095ba49f621073ca`, PR #49 병합 뒤 |
| 재검토 대조본 | PR #50 최초 문서 commit `22160497d3747a2d58d4af937ce11a9b53cf1b75`. 재검토 때도 main은 위 source로 같음. [대조 결과](#second-pass) |
| 이전 리뷰 | PR #48, merge `a45e37c66647c72cb60a8c31dde4b9863b1dab62`. UPDATE 이전의 DEV3 방향 리뷰이며 PR #47의 DELETE 검토와 다름 |
| 범위 | DEV3 저장 설계·16개 작업 명세·일정 01–19, 구현된 첫 Row profile와 주요 WAL·전이·읽기·복구 경로 |
| 방법 | 계약과 관련 코드의 정적 대조, 기존 실행 증거 확인, 공개 DBMS 공식 문서 비교 |
| 한계 | 전체 소스의 모든 경로를 감사한 결과나 신규 성능·복구 시험 결과가 아님. 기존 테스트 결과는 원 source와 범위의 증거로 유지 |
| 변경 분류 | D0 기록 추가. 현재 계약·제품 의미·완료 상태·일정 가중치를 바꾸지 않음 |

현재 환경에서 PostgreSQL·Altibase의 외부 로컬 source checkout을 새로 조사하지 않았다. 저장소에 보존된 과거 비교 기록과 이번에 확인한 공식 문서를 구분한다. Altibase 비교는 디스크 테이블과 Undo tablespace에 한정한다.

재검토는 같은 작성자가 계약·호출 경로·공식 자료를 다시 대조한 것이다. 별도 검토자의 독립 D1/D2 승인으로 기록하지 않는다. 현재 비용은 코드와 기존 계수로, 미구현 알고리즘의 비용은 설계 조건부 분석으로 표시한다. 비교 제품과 같은 동작을 갖는 것이 곧 제품 목표는 아니며, [제품 방향](../plans/project-direction.md)은 특정 dialect 전체 호환을 아직 채택하지 않았다.

<a id="implemented"></a>
## 2. 구현된 범위와 유지할 설계

[첫 Row profile](../contracts/dev3-module-consumers.md#profile)은 첫 extent 64 page, 1 MiB, Root page 1, Job page 2–6, Job cell 64개, table당 level 0 leaf 하나, inline 값 최대 2,048 B의 제한된 경로다. INSERT·DELETE·UPDATE, 자동커밋·명시 거래, statement/full rollback, 읽기, READY 복구와 live 정리를 연결했다. 이는 일반적인 SQL DML·트리·동시 거래 기능의 완료와 구별해야 한다.

특히 [PR #49의 UPDATE](../contracts/dev3-row-update.md#scope)는 **committed이고 Prune된 행 하나를 RowId로 골라 전체 값을 교체하는 기능**이다. 같은 거래가 넣거나 바꾼 행은 `Unsupported`, 자기 DELETE 행은 0행, Prune 전·Root Job 보유·다른 writer 행은 `Busy`다. 크기 변화는 같은 leaf 안에서만 허용하고, overflow·인덱스·split·일반 row lock·여러 행 또는 술어 UPDATE는 남아 있다. own/repeated write는 이전 버전 chain과 pin 수명까지 넓혀야 하므로 일정 08-4에 남긴 구분이 합리적이다.

| 유지할 선택 | 타당한 이유와 비교 시 주의점 |
| --- | --- |
| mutable page와 영속 Undo | Oracle 및 Altibase 디스크 저장과 같은 계열이다. PostgreSQL의 heap 다중 버전 방식과 다르지만, 그 차이가 결함은 아니다 |
| 단일 WAL, WAL-before-data, steal/no-force | 데이터 page의 flush 시점과 commit을 분리하면서 crash recovery를 구성하는 적절한 기반이다 |
| stable RowId, 논리 Undo, redo-only CLR | split 뒤 물리 위치가 바뀌어도 논리 변경을 되돌릴 수 있다. 전체 옛 page를 복원해 다른 거래의 변경을 지우는 위험을 피한다 |
| 구조 변경을 별도 redo-only 작업으로 처리 | 사용자 거래 rollback이 완료된 split 같은 구조 변경을 되돌리지 않도록 하는 방향이 타당하다 |
| snapshot·Undo·reader pin의 소유권 | 읽기 일관성과 재사용을 함께 증명할 수 있다. 단순 commit 순서만 보고 회수하지 않는 이유가 있다 |
| durable ACK와 UNKNOWN 구분 | 디스크 결정과 호출자에게 보이는 결과를 분리하는 필요한 계약이다 |
| rollback·flush·정리에 필요한 자원 선예약 | 자원 부족을 처리하는 경로가 다시 같은 자원을 기다리며 멈추지 않게 한다 |

Oracle의 SCN·transaction table과 XADB의 commit 번호·Status, Altibase의 TSS·Undo segment는 비교 가능한 역할이다. 그렇다고 필드 배치나 동시성 비용이 같다는 뜻은 아니다. PostgreSQL의 HOT도 indexed column 변경 여부와 같은 heap page의 공간 같은 조건에 의존한다. XADB의 stable RowId와 versioned index 설계에 그대로 옮길 수는 없다([공식 자료](#references)).

<a id="priorities"></a>
## 3. 일반화 전에 확인할 비용과 설계 경계

<a id="live-replay"></a>
### 3.1 정상 실행에서 WAL 전체를 반복 재생하는 비용

[M0 계수 기록](f6-wal-revalidation-survey-20261102.md#f6a-m0)은 Int64 한 열 행의 INSERT 거래와 정리에 전체 replay가 10회 발생한다고 보고한다. 거래가 4회, 정리가 6회이며 시작 WAL 길이를 E₀라 하면 다음과 같다.

| 항목 | 원 기록의 값 |
| --- | --- |
| 거래와 정리의 읽기 | `30 × E₀ + 8,691,603 B` |
| hash 입력 | `20 × E₀ + 5,794,402 B` |
| 거래와 정리의 WAL 증가 | `629,363 B` |
| E₀ = 16,911,284 B에서 읽기 | 약 516 MB |

이는 API 수준의 읽기 바이트 계수이며 실제 장치 읽기량이나 처리시간 측정값이 아니다. 같은 epoch에서 WAL이 거래마다 늘고 같은 경로를 반복하면 누적 재생량은 거래 수의 제곱에 비례한다. 실제 TPS 저하율을 이 식만으로 계산할 수는 없지만, DB 작업량과 무관한 과거 전체 검산이 정상 경로에 있는 구조는 확인된다.

관련 구현은 [history.rs의 `replay_durable`](../../crates/engine/src/storage/wal_io/plan4_retention/history.rs)다. 파일 접근, prefix hash, 재구성, 재검산은 일관된 source를 확인하는 역할도 한다. 따라서 검사를 지우는 수정은 적절하지 않다. **권고는 live 상태를 변경 시점에 증분 유지하고, 정상 연산은 해당 page·ledger의 전이를 검증하며, 전체 replay는 복구·독립 audit의 책임으로 분리하는 것**이다. 새 opcode·회수·crash 뒤 재구성까지 같은 상태를 도출해야 한다.

F6-b를 최종 성능 단계의 선택적 개선으로만 두기보다, 일반 경로의 상태 소유와 결합해 먼저 설계할 것을 권고한다. 변경 구현에 맞춘 작은 계수 비교부터 하고, 시간 측정과 대규모 시험은 기존 시험 정책에 따라 별도로 선택한다.

<a id="redo"></a>
### 3.2 작은 변경도 넓은 page patch가 되는 형식

[Row operation shape](../../crates/engine/src/storage/semantic_row/operation.rs)의 Forward redo는 page patch 다섯 개만으로 81,720 B다. 각 patch의 16,344 B는 record header 64 B와 page 본문 16,280 B로 구성된다. frame의 다른 정보까지 포함하면 더 커진다. M0에서 정리 WAL 367,106 B가 거래 자체 262,257 B보다 컸다는 점도 중요하다. 사용자 값 크기만 줄여서는 해결되지 않는다.

여기에는 생산자뿐 아니라 [WAL 형식](../contracts/dev3-wal-format.md)의 제약도 있다. **한 frame에서 같은 물리 page는 subrecord 하나만 허용하며, 떨어진 여러 변경은 그 사이의 바뀌지 않은 bytes까지 포함한 연속 구간으로 합친다.** page 앞의 slot metadata와 뒤의 row bytes가 함께 바뀌면 작은 Patch codec을 사용해도 거의 전체 본문을 기록할 수 있다([생산 경로](../../crates/engine/src/storage/semantic_row/transition/frame.rs)).

권고는 page별 여러 변경 구간 또는 연산별 redo를 비교하는 것이다. page당 적용 순서·중복 판정·pageLSN·부분 적용·scratch 상한·오류 검출을 함께 설계해야 한다. 형식의 단일 구간 제약을 둔 이유가 bounded 검산이므로, 로그량만 줄이고 이 성질을 잃어서는 안 된다.

native before/after 정보도 지금은 단순한 시험용 덤프가 아니다. [Row replay](../../crates/engine/src/storage/wal_io/plan4_retention/history/row.rs)는 `native_job`, `native_before` 등을 읽고 RowReserve의 효과·RowId·C′·projection을 해석한다. 이를 운영 WAL에서 제거하려면 동일 의미를 복구할 입력과 검증을 먼저 공급해야 한다. PostgreSQL의 checkpoint 뒤 첫 변경 full-page image도 torn page 복구 목적이 있으므로, full-page 정보 자체를 불필요하다고 일반화하지 않는다.

<a id="shared-metadata"></a>
### 3.3 공유 Root와 Commit 설치 순서

[native page mask](../../crates/engine/src/storage/semantic_row/operation.rs)는 Begin·Commit·AbortEnd·TerminalFinish, Reserve·Forward·CLR·Prune 및 여러 정리 연산에 공통 Root/Job을 포함한다. 일반 행 변경과 다중 writer로 넓힐 때 논리적으로 다른 행도 같은 메타데이터를 갱신하는 구조다. 모든 연산이 그런 것은 아니다. RollbackStart·StatementFinish는 Status만, AgingStart는 Root만 사용하는 예외가 있다.

여기에 [Commit의 의미 설치 장벽](../contracts/dev3-task-11-commit.md#installation-barrier)이 더해진다. Commit LSN c가 durable이지만 Status 설치 전일 때 다른 거래가 같은 Status page의 다른 slot을 c+1로 바꾸고 flush하면, 복구가 높은 pageLSN을 보고 앞선 Commit 효과를 건너뛸 수 있다. 현재 전역 장벽은 이 순서 문제를 막기 위한 안전장치다.

**권고는 공유 정리 정보를 거래·segment 등으로 분산하는 방안과, page별 log/apply 순서를 보장하는 방안을 함께 검토하는 것**이다. WAL 순서 부여가 필요하다는 사실과 모든 writer의 전체 설치를 직렬화해야 한다는 주장은 구분한다. 장벽 제거는 publish·checkpoint·복구까지 포함한 대체 증명 뒤에만 가능하다. group commit 역시 검토할 수 있지만, 이를 추가하는 것만으로 Root 경쟁이나 page 적용 순서가 해결되지는 않는다.

재검토에서 확인한 현재 제약은 공통 page보다 더 앞에 있다. [실제 `MutationPort`](../../crates/engine/src/storage/wal_io/plan4_apply.rs)는 `RetentionLedger`와 operation workspace를 독점 가변 대여한다. [거래 계약](../contracts/dev3-task-10-concurrency.md#module-row-isolation)도 validation부터 durable·설치·Q 공개까지 이 유일 writer가 직렬화한다고 명시한다. 여러 transaction capsule을 유지하고 순서대로 처리할 수 있다는 것과 여러 writer의 준비·실행이 병렬이라는 것은 다르다. **Root를 나눠도 이 소유 경계를 그대로 두면 쓰기 작업은 계속 직렬이다.**

[Row preflight와 lease](../../crates/engine/src/storage/wal_io/plan4_apply/row/write.rs)는 `close_reads_for_terminal`로 읽기 gate를 닫고, preflight·Reserve·Forward를 거치는 동안 그 소유를 유지한다. [read admission](../../crates/engine/src/transaction/concurrency/read.rs)은 같은 core의 모든 기존 read operation이 끝나기를 기다리고 새 read operation은 `InstallationPending`으로 거절한다. [preflight 본문](../../crates/engine/src/storage/wal_io/plan4_retention/row_preflight.rs)의 LOAD·전체 replay도 이 범위에 있다. 이는 page latch의 짧은 배타 구간과 다르며, 관련 없는 행의 새 읽기도 영향을 받는다. 단, [Commit](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_commit.rs)은 Pending 대기 전에 gate를 다시 열므로 모든 Commit sync 동안 읽기가 막힌다고 일반화하지 않는다.

첫 profile에서 이 배타성은 source loan·자원 합산·설치 정합성을 단순화하는 유효한 선택이다. 일반화 시에는 writer 소유, reader admission, 공유 Root/Status, WAL slot, 설치 장벽을 각각 나누어 비용을 관측해야 한다. gate를 좁히려면 현재 writer와 reader가 배타라는 전제로 계산한 동시 메모리 peak, pin 수명, 같은 page 적용 순서를 다시 증명해야 한다.

<a id="transaction-semantics"></a>
### 3.4 거래 의미를 일반 DML 전에 확정할 필요

[동시성 계약](../contracts/dev3-task-10-concurrency.md#isolation)은 이미 구체적인 의미를 택했다. 다음 차이는 버그로 단정할 사안은 아니지만, Oracle·Altibase·PostgreSQL을 기준으로 제품을 키우려면 호출자에게 보이는 동작으로 먼저 정해야 한다.

| 항목 | XADB의 현재 계약 또는 구현 | 비교와 판단 |
| --- | --- | --- |
| 기본 격리 | Serializable | Oracle·PostgreSQL의 기본 Read Committed와 다르다. 기본값과 제공 보장을 분리해 결정할 필요가 있다 |
| RC 쓰기 충돌 | row X-lock 뒤 관측 버전이 다르면 전체 거래 `SerializationConflict`. 즉시 획득·이미 보유한 lock도 같은 규칙 | PostgreSQL RC는 충돌 writer를 기다린 뒤 새 row에 WHERE 조건을 재평가한다. Oracle RC의 statement 일관성과도 구별된다 |
| Serializable 충돌 범위 | table·catalog 변경 번호 검증. 같은 table의 다른 행에서도 보수적 충돌 가능 | 올바른 보수적 방식일 수 있지만 경합 비용이 크다. PostgreSQL의 SSI와 같은 동작은 아니다 |
| deadlock 피해자 | cycle의 가장 큰 TxnId를 골라 전체 rollback | Oracle은 deadlock을 해소하기 위해 statement 하나를 rollback하며 앞선 statement 변경은 남길 수 있다 |
| 자기 변경 뒤 cursor | private revision이 달라지면 기존 cursor를 무효화 | 열린 cursor의 가시성·재시작·오류를 사용자 API 계약으로 설명해야 한다 |

기본 격리를 RC로 바꿀지, 충돌 시 대기·statement 재시작 또는 전체 abort를 택할지, cursor와 deadlock의 범위를 어떻게 할지 일반 DML 전에 동작표로 재확인할 것을 권고한다. 현재 Serializable 기본값과 전체 abort는 이미 채택된 계약이며 미결정 결함이 아니다. 그대로 유지할 수도 있지만 호환성·충돌률을 제품 동작으로 설명해야 한다. statement 재시작은 snapshot만 바꾸는 기능이 아니다. 이미 전달한 결과, 잠금, 부분 변경, rollback 자원과 재실행 가능한 효과까지 정의해야 한다.

Serializable을 단순 snapshot isolation으로 바꿔 충돌을 줄이는 것도 같은 보장을 유지하는 최적화가 아니다. write skew 등 허용하는 이상 현상이 달라진다. Oracle의 이름과 동작을 따를지, PostgreSQL처럼 직렬 실행에 상응하는 보장을 목표로 할지 명시해야 한다. 이 선택을 현재 첫 UPDATE의 제한을 푸는 구현과 섞지 않는 편이 좋다.

UNIQUE 비교도 검사 시점과 NULL 규칙을 분리해야 한다. XADB의 statement-final 검사는 key 교환을 지원하는 의도된 선택이다. PostgreSQL의 기본 non-deferrable UNIQUE는 row 변경 시 즉시 검사하며, deferrable immediate는 statement 끝에 검사한다. 또한 PostgreSQL 기본 NULL 구분과 `NULLS NOT DISTINCT` 어느 쪽도 XADB의 “전체 NULL만 예외, 부분 NULL은 충돌” 규칙과 동일하지 않다. 비교 시험은 단순히 양쪽에 `UNIQUE`를 선언했다고 같은 의미로 취급하면 안 된다([검사 시점](https://www.postgresql.org/docs/18/sql-set-constraints.html), [NULL 규칙](https://www.postgresql.org/docs/18/indexes-unique.html)).

<a id="representation"></a>
### 3.5 행 표현과 Undo 배치의 공간 비용

[표현·자원 계약](../contracts/dev3-resource-bounds.md)의 행 header 168 B와 index value 160 B는 작은 값에서 큰 비용이다. 현재 첫 profile의 행별 Undo page와 공유 Root/Job 한도는 슬롯 수만 늘려 해결하기보다 page 안에 여러 Undo record를 배치하는 일반 구조와 함께 검토할 필요가 있다.

PR #48의 Int64 계산은 정정이 필요하다. [Row 형식](../contracts/dev3-row-format.md)에서 Int64 한 열의 canonical 값은 `4 + 1 + 8 = 13 B`다. cell은 `180 + 13 = 193 B`, slot을 더한 entry는 197 B이므로 leaf 예산 16,188 B에는 82행이 들어간다. 16 KiB page 대비 canonical 값 비율은 약 6.51%, 실제 8 B 정수만 세면 약 4.00%다. 이전 리뷰의 cell 185 B·85행·2.59%는 최소 5 B 값에 해당하며 Int64 계산이 아니다. **수치는 바로잡아야 하지만 작은 행의 공간 효율이 낮다는 결론은 유지된다.**

짧은 참조, 중복 namespace·identity 표현의 축소, Undo record packing, page 단위 거래 참조를 대안으로 비교할 것을 권고한다. Oracle의 ITL이나 Altibase의 TSS 구조는 참고점이다. ITL 도입에는 slot 고갈·재사용·가시성 복원 문제가 따라오므로 단순히 header를 줄이는 변경으로 취급하지 않는다.

Altibase 7.1의 Undo tablespace는 TSS와 Undo segment를 짝지은 transaction segment를 설명하며, 하나의 transaction segment에서 한 update transaction이 실행된다. 이처럼 거래 상태와 Undo 배치를 함께 보아야 한다. XADB의 영속 edge/count를 줄이는 경우에도 snapshot·rollback·reader pin·index·overflow·복구가 모두 요구하지 않는다는 회수 증명이 선행해야 한다. commit 순서만으로 즉시 회수하는 대안은 충분하지 않다.

상한을 큰 기능의 지원 보장으로 해석하는 것도 피해야 한다. [자원 벡터 계약](../contracts/dev3-resource-bounds.md#resource-vector-proof)에서 private 합계 64 MiB에 거래당 8 MiB를 실제로 예약하면 최대 크기 거래의 동시 admission은 8개 이하이고, transaction 32개 한도는 32개 최대 거래의 동시 성공 약속이 아니다. 일반 최대 128 KiB 행을 바꿀 때 old/new overflow와 before Undo가 각각 9 page이면 일시 공존만 27 page, 432 KiB다. index·split·WAL은 별도다. 새 opcode의 성공 경로뿐 아니라 rollback·cleanup까지 이 벡터가 들어가는지 확인해야 한다.

소스별 byte census는 이런 증명에 유용하지만 모든 작은 타입 변화가 여러 소비자의 재계약으로 번지면 유지 비용이 커진다. 실제 peak를 증명한 여유 있는 envelope와 phase별 수명 합성으로 경계를 안정화하는 대안도 검토할 수 있다. RSS 측정만으로 source 회계를 대체하거나 기존 자원 한도를 자동 확대하는 제안은 아니다.

<a id="checkpoint"></a>
### 3.6 checkpoint의 bounded RAM과 반복 I/O

[checkpoint 불변 입력 설계](../contracts/dev3-task-13-checkpoint.md#immutable-capture-source)는 pin한 base와 WAL prefix를 같은 cut까지 재생하고, 같은 cut의 bounded ATT/DPT·volatile 소유 snapshot을 결합한다. 변경 중인 home page를 정답으로 삼지 않는 점과 flush 완료를 WAL만으로 재구성하지 않는 점은 타당하다.

문제는 전체 identity를 메모리에 두지 않기 위해 B개씩 선택하며 base와 WAL을 반복 순회한다는 것이다. identity가 T개, base가 I bytes, prefix가 W bytes라면 입력 순회량은 적어도 `ceil(T/B) × (I + W)`에 비례한다. B가 고정되고 I가 T에 비례하면 DB 크기에 대한 제곱 비용이 생길 수 있다. 계약도 bounded RAM이 선형 I/O를 뜻하지 않는다고 명시한다.

**권고는 증분 inventory, 디스크에 정렬·색인된 ownership metadata, 변경 집합을 이용하는 대안을 비교하고 scan 횟수·읽기 바이트·WAL pin 기간의 상한을 함께 정하는 것**이다. checkpoint가 보존한 prefix 때문에 새 WAL 공간이 부족해지는 경우에도 기존 rollback·flush·checkpoint가 끝날 자원을 남겨야 한다. 이 문제는 아직 남은 설계에 속하므로 실제 checkpoint 성능 결함이 측정됐다고 표현하지 않는다.

<a id="durability-boundaries"></a>
### 3.7 각 변경 그룹의 영속 완료 대기

WAL bytes와 sync 경계는 다른 비용이다. [Forward](../../crates/engine/src/storage/wal_io/plan4_retention/row_forward.rs)는 공통 `apply_existing`을 호출하고, [그 적용 경로](../../crates/engine/src/storage/wal_io/plan4_retention/catalog_lifecycle.rs)는 page latch를 놓은 뒤에도 `wait_durable`이 성공해야 반환한다. Begin·Reserve·CLR·정리도 해당 그룹의 영속 완료 경계를 가진다. [물리 WAL](../../crates/engine/src/storage/wal_io/physical.rs)은 한 slot의 `Idle → Complete → IoOwned → Idle` 경로에서 해당 frame을 쓰고 sync한 뒤 slot을 돌려준다. 따라서 현재는 Commit끼리의 group commit뿐 아니라 **한 거래 안의 여러 변경을 WAL buffer에 누적하는 경로도 제한된다.**

M0의 한 행 거래는 Begin·Reserve·Forward·Commit 네 그룹과 정리 여섯 그룹으로 구성된다. 이 그룹별 대기 구조는 코드에서 확인했으며 실제 sync 호출 수·시간을 이번에 새로 측정한 것은 아니다. 앞의 no-force 평가는 data page를 Commit마다 강제 flush하지 않는다는 뜻이지, WAL을 Commit에서만 sync한다는 뜻은 아니다.

[PostgreSQL WAL 설명](https://www.postgresql.org/docs/18/wal-intro.html)과 [Oracle COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html)은 durable Commit과 로그 flush 결합·batch의 구분을 보여 준다. XADB도 정상 변경의 stage·메모리 적용, WAL write·sync, dirty page flush 허가, Commit ACK를 별도 경계로 평가할 필요가 있다. **권고는 ACK 내구성을 유지하면서 어떤 중간 그룹을 연속 stage할 수 있는지 검토하는 것**이다. 미동기 효과를 포함한 live 검증, queued frame 소유·예약, partial write, ordered apply, flusher의 durable-prefix 확인까지 바뀌므로 `wait_durable` 호출 삭제만으로 처리할 수 없다.

<a id="multi-row-bridge"></a>
### 3.8 다중 행 DML과 현재 cursor 규칙의 연결

PR #50 최초판은 own mutation 뒤 cursor 무효화를 주로 사용성 차이로 다뤘다. 재검토 결과 [DEV4 DML 설계](../contracts/dev4-sql-design.md)는 이미 같은 statement token에서 RowUID·원래 값·opaque observation을 bounded 목록으로 먼저 모으고, cursor를 닫은 뒤 RowId 순서로 각 행을 한 번만 변경하도록 정했다. 따라서 현재 cursor 규칙 때문에 다중 행 DML 설계가 전혀 없다는 지적은 성립하지 않는다.

이 대상 고정은 index key를 바꾼 행을 scan이 다시 만나 중복 변경하는 Halloween 문제를 막는 합리적인 첫 구현이다. 필요한 후속 작업은 이 계약을 일반 DEV3 observation·own/repeated write·index delta·statement rollback에 실제로 연결하는 것이다. cursor를 닫아도 token과 observation은 유효해야 하며, 첫 행을 바꾼 뒤 다른 행의 원 observation을 어떤 조건으로 소비하는지 고정해야 한다. old observation을 새 snapshot으로 위조하거나 cursor revision 검사를 통째로 없애서는 안 된다.

남은 mutation 256개·논리 payload 1 MiB·private 8 MiB 등의 한도를 넘으면 명시 실패한다는 설계도 보존한다. 이를 내부 여러 Commit으로 나누면 원자성이 달라진다. 이 제한은 첫 SQL 공급에는 설명 가능한 선택이지만 대형 일괄 DML 지원과는 구별해야 한다. 일반화 인수에는 key 이동, 같은 행의 반복 쓰기, 마지막 행의 UNIQUE 실패 뒤 전체 statement 복원과 이전 성공 statement 보존을 포함할 것을 권고한다.

<a id="audit-cost"></a>
### 3.9 full audit와 restore의 반복 순회 비용

[full audit](../contracts/dev3-task-15-backup-integrity.md)는 고정 B개 target마다 가능한 영속 source field 전체를 다시 읽어 incoming edge multiset을 계산한다. target T개와 edge E개라면 이 부분의 방문량은 `E × ceil(T/B)`에 비례하며, E가 T와 함께 커지고 B가 고정이면 제곱 증가가 가능하다. [PF09 명세](../contracts/dev3-task-16-qualification.md#comparison-measurement)도 이미 이 비용을 식별했다. checkpoint와 비슷한 비용 형태지만 별도의 알고리즘·실행 경로다.

정상 open의 VERIFY_RECOVERY는 복구 대상과 관련 상태만 검사하고 변경 없는 cold table을 매번 전수 검사하지 않는다. 따라서 이 지적을 모든 startup이 full audit를 한다는 주장으로 바꾸면 안 된다. 반면 restore는 source backup의 full integrity를 요구하므로 audit 비용이 restore 완료시간에 직접 영향을 준다.

정확한 비교 대상은 audit·restore의 coverage, 반복 방문, 읽기 bytes, 중단 시간이다. 현재 spill 없이 bounded 재순회를 택한 설계는 RAM 안전성의 출발점으로 유효하다. 대형 DB에서 운영 가능한 시간인지 따로 평가하고, 필요하면 quota를 정한 외부 정렬·분할 집계 같은 대안을 owner 계약에서 검토해야 한다. 현재의 “추가 disk spill 없음” 계약을 문서 보강만으로 변경하지 않는다.

<a id="remaining"></a>
## 4. 남은 기능별 설계 판단

| 영역과 소유 문서 | 현재 방향에 대한 판단 | 구현 또는 일반화 때 닫을 조건 |
| --- | --- | --- |
| [3.01 형식·정체성](../contracts/dev3-task-01-formats.md) | 신뢰한 expected identity와 payload를 분리하고 checksum·generation·소유를 검산하는 방향은 유지 | codec 존재와 실제 효과·복구 공급을 구별. 새 opcode마다 native/redo/edge/obligation의 결합과 잘린·변조 입력을 검증. 검증을 줄이는 최적화는 제거한 검사가 어디서 보장되는지 명시 |
| [3.02 파일·읽기](../contracts/dev3-task-02-files-reads.md)와 [3.03 할당](../contracts/dev3-task-03-allocation.md) | 첫 extent·선택 읽기·제한 INIT/예약 공급을 일반 growth/reuse와 구별한 방향은 유지 | FILEMAP의 영속 선택 전 새 주소 참조 금지, old/new closure 공존, 물리 bytes와 page token의 중복 차감 방지. generation 재사용과 copy 전 flush claim의 경합 검증 |
| [3.04 WAL](../contracts/dev3-task-04-wal.md)와 [3.11 Commit](../contracts/dev3-task-11-commit.md) | 실제 stage/apply·durable·공개와 UNKNOWN 경계는 유지 | [3.2절](#redo)·[3.3절](#shared-metadata)·[3.7절](#durability-boundaries)의 형식·writer 소유·대기 비용을 별도 평가. 여러 page 중 일부만 home에 반영된 상태와 durable 뒤 ACK 전 장애 검증 |
| [트리·일반 행 변경](../contracts/dev3-task-06-tree-rows.md) | stable anchor와 logical Undo, redo-only 구조 변경은 유지 | own/repeated write·occupied-before·다단 Undo, split 뒤 rollback, 큰 행 복원과 recovery를 함께 검증. 순차 RowId에서 균등 split은 왼쪽 page를 반쯤 비운 채 남길 수 있으므로 rightmost split 정책 비교 |
| [cursor와 overflow](../contracts/dev3-task-06-tree-rows.md) | last key로 재탐색하는 방식은 구조 변경에 강한 단순 출발점 | 매 행 root 재탐색 비용과 batch/hint 대안 비교. 검증된 hint 수명·자기 변경 무효화·overflow pin/회수 명시. persistent sibling link를 즉시 필수로 만들 필요는 없음 |
| [인덱스·PK/UNIQUE](../contracts/dev3-task-07-indexes.md) | versioned presence는 XADB의 stable RowId와 일관된 가능한 선택 | row와 모든 index의 원자적 Undo/CLR, 거래 전체 own 최종 상태, statement-final UNIQUE, all-NULL 예외와 부분 NULL 충돌, snapshot의 옛 key 탐색을 함께 검증 |
| [Catalog·DDL](../contracts/dev3-task-08-catalog-ddl.md) | generation과 stale handle, 버전 있는 Catalog는 유지 | 실제 CREATE의 제한을 넘어 DROP·자식 index 의존·이름 재사용·실패 복구 닫기. 전체 DB close/drain을 쓰는 첫 offline build는 안전한 출발점이나 table 범위로 좁힐 가능성 검토 |
| [3.09 Undo·rollback](../contracts/dev3-task-09-undo-rollback.md) | transaction chain과 version chain, statement mark와 full abort를 분리한 방향은 유지 | 일반 UPDATE의 Transaction before와 chain/pin, index·Catalog 대상 CLR, rollback 재삽입 split, BEGIN WAL 없이 Active loser 복구. 논리 rollback 뒤 WAL·lock·retained Undo를 실제 반환 전 감액하지 않음 |
| [동시성](../contracts/dev3-task-10-concurrency.md) | page latch와 거래 lock 대기·rollback 책임을 나눈 방향은 타당 | 일반 observation·대기·deadlock·취소와 여러 Status slot, 같은 table의 다른 행 writer를 통합. 거래 의미는 [3.4절](#transaction-semantics)에서 먼저 결정 |
| [buffer/flush](../contracts/dev3-task-05-buffer-flush.md)와 [recovery](../contracts/dev3-task-12-recovery.md) | WAL-before-data·DWB·generation·ACK 보존은 유지 | 첫 extent와 resident profile을 넘어 dirty flush→evict→reload, 여러 extent, buffer보다 큰 working set, 낮은 자원에서 rollback 진행성 검증. process cut을 torn write·전원 장애·복구 중 crash의 전체 증거로 확대하지 않음 |
| [checkpoint](../contracts/dev3-task-13-checkpoint.md)와 [회수](../contracts/dev3-task-14-reclamation.md) | 같은 cut의 소유·재사용 금지와 정리 진행성은 유지 | 반복 scan 비용, current/previous/new output 공존, 장기 reader의 Undo 보존과 writer admission 정책, cleanup 자체의 WAL 예약 닫기 |
| [backup·restore](../contracts/dev3-task-15-backup-integrity.md) | offline exact-cut C는 첫 제품 범위로 적절 | 전체 namespace 재작성 비용과 stale handle 방지의 관계 검토. restore 뒤 새 ACK와 두 번째 복구까지 검증. online backup/PITR는 DEV8 범위와 구별 |
| [최종 인수](../contracts/dev3-task-16-qualification.md)와 [테스트 계획](../plans/dev3-test-plan.md) | 최종 S5의 정확성·복구·대형 profile을 나눈 방향은 타당 | 기능 구현 시 작은 계수 관측을 붙이고, 최종에는 1/2/4 writer·같은 table 다른 행·장기 reader·buffer 초과 데이터를 검증. API 읽기량, 실제 장치 I/O, WAL량, 처리시간을 별도 지표로 보고 |

인덱스에서 PostgreSQL이 heap의 tuple version과 HOT에 맡긴 역할을 XADB의 index value에서 단순 삭제하면 옛 snapshot의 key 탐색을 잃을 수 있다. 반대로 현재 큰 index value를 반드시 유지해야 한다는 뜻도 아니다. 필요한 과거 의미를 어느 구조가 소유할지 먼저 결정해야 한다.

DWB도 성능 개선 대상이 될 수 있지만, PostgreSQL의 full-page image와 교환 가능한 비용 항목 하나로 취급하지 않는다. page 손상 복구의 정답 입력과 보존 기간을 먼저 유지하고 batch·sync 비용을 비교해야 한다. 장기 reader 때문에 Undo를 보존하는 정책 역시 writer 처리량과 읽기 성공 보장의 선택이다. Oracle의 보존 정책과 다르다는 이유만으로 오류라고 판정하지 않는다.

<a id="comparison-conditions"></a>
### 4.1 세 DBMS와 비교할 동작과 조건

| 축 | 비교 기준과 XADB에 대한 판단 |
| --- | --- |
| 오래된 버전 보존 | Oracle의 retention guarantee는 기본 비활성이며, 활성화하면 unexpired Undo를 지키기 위해 DML이 실패할 수 있다. PostgreSQL도 다른 거래가 볼 수 있는 row version을 VACUUM으로 제거하지 않는다. Altibase 디스크 역시 장기 거래가 Undo·index key 회수를 지연시킨다. XADB의 horizon 보호는 합리적이며, admission 거절·보존량·최장 snapshot·회수 처리량을 관측할 필요가 있다 |
| Active 거래와 WAL 회수 | Altibase 7.1 디스크 checkpoint 설명은 Active Begin LSN과 dirty LSN의 보존을 요구한다. XADB는 persistent Undo와 checkpoint head/progress가 충분하면 Begin WAL을 자동 floor에서 제외하려 한다. 이것은 다른 설계 선택으로 유지할 수 있으며, 실제 Begin WAL 삭제 후 rollback·재장애 인수가 조건이다 |
| 영속성 설정 | Oracle의 Commit WAIT/NOWAIT·IMMEDIATE/BATCH, PostgreSQL의 fsync·synchronous_commit·full_page_writes·wal_level, Altibase의 COMMIT_WRITE_WAIT_MODE·LOG_BUFFER_TYPE을 기록한다. Altibase 7.1 문서의 기본 0/0은 Commit이 로그의 디스크 기록을 기다리지 않는 설정이므로 durable ACK의 XADB와 기본값 TPS를 바로 비교하면 안 된다 |
| 의미와 workload | RC/Serializable 보장, UNIQUE 검사 시점·NULL, row 폭·index 수·key 변경 유무, autocommit/명시 거래, 장기 reader, warm/cold·buffer·장치 조건을 맞춘다. 단순 성공 TPS뿐 아니라 원래 요청 기준 retry·거절·UNKNOWN·p99를 같이 보고한다 |

근거는 [Oracle Undo 관리](https://docs.oracle.com/en/database/oracle/oracle-database/19/admin/managing-undo.html), [PostgreSQL VACUUM](https://www.postgresql.org/docs/18/routine-vacuuming.html), [Altibase 7.1 거래·디스크 checkpoint·영속성](https://manual.altibase.com/7.1/en/admin/administrator/8.-Managing-Transactions/), [Oracle COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html), [PostgreSQL WAL 설정](https://www.postgresql.org/docs/18/runtime-config-wal.html)이다. 설정 이름이 비슷하다는 이유로 동일 장애 보장을 추정하지 않고, 비교 대상의 ACK·장애 후 허용 결과를 먼저 고정해야 한다. Altibase의 메모리 DB ping-pong checkpoint 설명을 디스크 엔진 비교에 가져오지 않는다.

<a id="previous-review"></a>
## 5. PR 48 리뷰에서 유지하거나 보정할 주장

| 기존 주장 | 이번 판단 |
| --- | --- |
| 저장·MVCC·복구 뼈대는 맞지만 첫 profile을 그대로 키우기 어렵다 | 유지. 현재 구현과 남은 설계를 관통하는 핵심 판단이다 |
| 한 행 거래와 정리의 WAL 629,363 B, live replay의 누적 제곱 비용 | M0의 해당 INSERT profile과 epoch 조건에서 유지. UPDATE 전체나 다른 DBMS 대비 실측 속도로 확대하지 않음 |
| Oracle 한 행 변경은 수백 byte 수준 | 동일 schema·인덱스·Undo·정리 범위를 맞춘 측정이 없어 정량 성능 비교 근거로 사용하지 않음 |
| 작은 Patch를 쓰면 WAL량을 줄일 수 있다 | 방향은 맞지만 page당 단일 연속 구간 제약도 바꿀지 검토해야 함 |
| native before/after를 운영 WAL 밖으로 뺄 수 있다 | 현재 replay의 의미 입력이므로 대체 복구 입력을 먼저 설계해야 함 |
| 모든 Row 연산이 Root/Job을 바꾼다 | 예외를 보정. 주요 변경·정리 경로의 공통 page 병목 가능성은 유지 |
| Int64 cell 185 B, 85행, payload 2.59% | 193 B cell, 82행, canonical payload 약 6.51%로 정정. 실제 정수 값은 약 4.00% |
| UPDATE 전에 계약·형식을 다시 정해야 한다 | PR #49에서 첫 UPDATE는 이미 병합됨. 다음 판단 시점은 일반 행 변경·인덱스·동시성으로 넓히기 전 |
| checkpoint의 bounded 설계 | 안전성과 별개로 반복 I/O 비용을 추가 검토할 필요가 있음 |

이전 문서는 당시 source와 판단의 기록으로 보존한다. 이번 문서는 후속 source의 범위와 추가 근거를 제공하며, 이전 증거의 실행 결과나 승인 상태를 다시 쓰지 않는다.

<a id="next-steps"></a>
## 6. 권고하는 다음 작업 순서

1. **거래 의미를 먼저 정리한다.** 기본 격리, RC 충돌과 statement 재시작, Serializable의 보장, cursor, deadlock을 사용자 관점의 예제로 고정한다.
2. **live replay·redo 형식·WAL 영속 대기·writer 소유를 각각 작은 설계 단위로 분리한다.** 기존 복구·적용 순서·자원 증명을 유지하는 대안을 비교하고 채택된 것부터 owner 계약에 반영한다. 공통 page 분산, Undo packing과 짧은 참조의 형식 영향도 함께 확인한다. 어떤 후보든 bytes·sync·gate 대기 가운데 실제로 바꾼 항목을 구분한다.
3. **일반 행 변경과 트리를 닫는다.** own/repeated write, Undo chain, overflow, split 후 rollback·복구까지 완성하고 이후 index가 소비할 안정된 의미를 만든다. DEV4의 대상 고정·같은 statement token 계약에 맞게 observation을 연결한다.
4. **인덱스와 동시 거래를 통합하면서 비용을 관측한다.** 같은 table의 다른 행 writer, 작은 행과 큰 행, 장기 reader를 구분하고 checkpoint·full audit·restore의 scan 및 WAL retention 비용을 일반 규모로 넓히기 전에 평가한다.

적용 시점은 구분한다. 일반 Row·index 형식을 동결할 때는 redo·Undo 표현과 최악 예약을, 실제 병렬 writer를 공급할 때는 MutationPort·read gate·WAL slot·설치 순서를, checkpoint/audit/restore를 구현할 때는 반복 I/O와 보존 공간을 함께 닫는다. ITL·group commit·외부 정렬 같은 대안은 이 의무를 만족시키는 후보이며 모두를 먼저 구현해야 한다는 뜻은 아니다. 선택은 기존 보장, 실제 비용, 검증과 유지 부담을 함께 보고 한다.

현재 [일정 기준선](../plans/dev3-schedule-baseline.md)은 PR #49까지 33/86일, 38.37% 적립과 53일 미적립 가중치를 기록한다. 이는 일정 가중치의 비율이며 제품 완성도나 시험 coverage 비율이 아니다. 기존 56일 잔여 예측은 UPDATE 적립 전 값이고, 07 적립 뒤 새 날짜 예측은 아직 없다. 위 개선을 채택해 작업량이 늘면 현행 기준선의 재견적 절차에 따라 반영해야 하며, 이 리뷰만으로 분모나 완료 항목을 바꾸지 않는다.

<a id="acceptance-examples"></a>
### 6.1 다음 구현에서 확인할 대표 조건

아래는 owner 계약의 기존 의무를 구현 작업에 연결할 검토 기준이다. 이번 문서 변경에서 시험을 실행하거나 기존 inventory를 대체하지 않는다.

| 구현 경계 | 확인할 사례와 판정 |
| --- | --- |
| 일반 행·다중 행 DML | own INSERT→UPDATE→UPDATE→DELETE와 statement rollback·full abort의 옛 값 복원. 대상 목록을 만든 뒤 key를 옮겨도 각 RowId를 한 번만 처리. 마지막 행 오류 뒤 이전 성공 statement 보존 |
| row·index 원자성 | old/new key와 row 사이 각 그룹 cut에서 snapshot별 row↔index 일치. UNIQUE 실패·rollback·재시작에 대해 이미 있는 독립 typed 기대값 확장 |
| 병렬화 | 다른 table, 같은 table 다른 행, 같은 행·key를 구분. writer의 준비/replay/sync/install 구간에 reader를 진입시켜 값 일관성과 admission·대기 결과 확인. 단일 MutationPort를 mutex로 감싼 결과를 병렬 writer 공급으로 보고하지 않음 |
| WAL 대기 변경 | 여러 staged frame, partial write/sync, flusher가 먼저 durable을 확보하는 경우, durable 뒤 ACK 전 장애. stage·sync·page flush·ACK의 순서와 UNKNOWN 보존 |
| 재사용과 회수 | 옛 snapshot·Undo pin·copy 전 flush claim·DWB Pending 각각이 남은 동안 재사용 금지. 모두 종료한 뒤 새 incarnation INIT과 stale 참조 거절 |
| 일반 복구 | 같은 그룹의 일부 page만 더 높은 LSN인 mixed home, BEGIN WAL 제거 뒤 Active/부분 CLR, REBASE·새 epoch 선택 직후 재장애, READY 뒤 새 ACK의 두 번째 reopen |
| 규모와 자원 | 실제로 양립 가능한 row/index/높이 입력에서 forward·CLR·정리의 동시 peak와 허용 admission 수. checkpoint/audit의 T·B·E·읽기량을 함께 보고 정상 open의 cold page 비용과 분리 |

특히 allocator inventory가 buffer의 4배라는 S5 요구는 단순 row 데이터 4배와 다르다. [대형 profile 계산](../contracts/dev3-task-16-qualification.md)은 buffer 64 MiB에서 최소 43,241 extents, 약 45.34 GB의 extent bytes를 요구한다. 기본 owned 8 GiB로는 불가능하며 64 GiB 인수 profile은 준비 방향만 채택되어 아직 지원 tuple·전체 회계·실제 fixture가 남아 있다. sparse apparent size나 같은 metadata의 반복 읽기로 이 의무를 충족했다고 할 수 없다. 이 준비를 S5 실행 당일의 환경 문제로 남기지 않을 것을 권고한다.

<a id="evidence"></a>
## 7. 실행 증거의 해석

[UPDATE 검토 기록](../../module-gate/evidence/row-update/review.md)은 원 source의 선택 197개 사례와 native 18개 process를 보고한다. 이번 검토에서 제품 시험을 다시 실행하지 않았다. 기존 native 결과의 inventory·receipt·종료 결과를 확인한 것은 그 기록의 범위를 확인한 것이며, 일반 UPDATE·일반 crash 내구성·전체 DEV3의 신규 PASS가 아니다.

기존 검토는 크기가 줄어든 committed UPDATE의 재시작 사례 보강을 기록하며, 커진 committed UPDATE의 재시작은 여전히 미검증이라고 명시한다. 이 한계를 보존한다. 문서 추가 PR의 포맷·정적·문서 검사는 해당 PR에서 별도로 보고하며 제품 실행 증거와 구분한다.

<a id="second-pass"></a>
### 7.1 PR 50 최초 문서와 재검토 결과의 대조

| 최초 문서의 내용 또는 공백 | 재검토 결과 | 보강 위치 |
| --- | --- | --- |
| 기본 저장·Undo·복구 방향은 타당 | 유지. identity·참조 수명·ACK/UNKNOWN·자원 보존은 비용 개선 때도 유지할 조건 | 2절·4절 |
| 공유 Root/Commit 장벽이 동시성 위험 | 강화. 실제로 유일 MutationPort가 쓰기를 직렬화하고 RowWrite의 read gate가 새 읽기도 제한함 | [3.3절](#shared-metadata) |
| 큰 WAL와 group commit 검토 | 보완. 개별 Reserve·Forward까지 durable 완료를 요구하는 현재 slot/반환 경계를 별도 비용으로 식별 | [3.7절](#durability-boundaries) |
| cursor 무효화는 동작 차이 | 범위 보정. DEV4에 이미 bounded 대상 고정 설계가 있음. 미설계라고 판정하지 않고 DEV3 observation·일반 DML 연결 의무로 명시 | [3.8절](#multi-row-bridge) |
| checkpoint의 반복 I/O | 유지하고 확대. full audit의 edge 재순회와 restore 비용은 별도 경로이며 정상 open 전수검사로 오해하지 않게 구분 | [3.6절](#checkpoint)·[3.9절](#audit-cost) |
| 거래 의미를 동결할 필요 | 현재 선택은 이미 채택된 계약임을 명시. 기본 RC·재시작·격리 변경은 호환성 목표에 따른 재검토 제안 | [3.4절](#transaction-semantics) |
| 자원 상한·최종 큰 fixture | 최대 거래 수와 실제 admission을 구분하고, 64 GiB qualification profile의 미공급을 구체화 | [3.5절](#representation)·[6.1절](#acceptance-examples) |
| 비교 DBMS와 같은 조건으로 측정 | Altibase의 기본 영속성 설정, 세 제품의 장기 버전 보존, UNIQUE 의미를 구체적인 비교 조건으로 추가 | [4.1절](#comparison-conditions) |

새로 확인한 코드 제약과 기존 계약의 미구현 의무를 구분한 결과다. main의 제품 코드가 바뀌어 생긴 회귀로 분류하지 않는다. “설계 뼈대 유지, 일반화 전에 정상 경로 비용과 의미 경계를 정리”라는 결론은 같으며, 이를 전면 재작성이나 현재 제한 경로의 정합성 실패 판정으로 확대하지 않는다.

<a id="references"></a>
## 8. 공개 DBMS 비교 자료

아래 자료는 역할·동작 비교의 근거다. 같은 workload의 성능 측정이나 동일 구현이라는 주장의 근거로 사용하지 않는다.

- [Oracle 19c Data Concurrency and Consistency](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/data-concurrency-and-consistency.html): 기본 Read Committed, statement/transaction 일관성, Serializable과 deadlock의 statement rollback.
- [Oracle 19c Physical Storage Structures](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/physical-storage-structures.html): redo change vector와 data·Undo·transaction table 변경의 관계.
- [Altibase MVCC 구조](https://docs.altibase.com/pages/viewpage.action?pageId=14647512): 디스크 테이블의 in-place 변경과 Undo를 이용한 버전 유지.
- [Altibase 7.1 The Undo Tablespace](https://manual.altibase.com/7.1/en/admin/administrator/6.-Managing-Tablespaces/The-Undo-Tablespace/): TSS와 Undo segment, transaction segment의 거래 실행·만료·재사용.
- [PostgreSQL 18 Transaction Isolation](https://www.postgresql.org/docs/18/transaction-iso.html): RC의 충돌 row 대기·조건 재평가, Repeatable Read와 Serializable의 보장 차이.
- [PostgreSQL 18 Heap Only Tuples](https://www.postgresql.org/docs/18/storage-hot.html): heap 다중 버전과 HOT 적용 조건.
- [PostgreSQL 18 WAL Configuration](https://www.postgresql.org/docs/18/wal-configuration.html): checkpoint·full-page image·WAL flush와 group commit 비용.
