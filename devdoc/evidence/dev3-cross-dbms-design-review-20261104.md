# DEV3 구현과 잔여 설계의 Oracle Altibase PostgreSQL 비교 리뷰

2026-11-04 · **정적 리뷰와 개선 제안 / 계약·일정 변경 없음 / 제품 시험 새 실행 없음**

[저장 설계 정본](../contracts/disk-storage-design.md) · [16개 작업 명세](../contracts/dev3-task-designs.md) · [구현 계획](../plans/dev3-implementation-plan.md) · [일정 기준선](../plans/dev3-schedule-baseline.md) · [PR #48의 방향 리뷰](dev3-oracle-direction-review-20261103.md)

DEV3의 저장·복구 뼈대는 타당하다. mutable page, 단일 WAL, 영속 Undo, steal/no-force, 논리 rollback과 redo-only CLR을 유지할 이유가 충분하다. 다만 **현재 첫 Row profile의 비용과 거래 제약을 그대로 일반화하면, 기능 확대보다 먼저 WAL 재검증·로그량·공유 메타데이터·공간 효율이 한계가 될 가능성이 높다.** 일반 행 변경·인덱스·동시성 구현 전에 이 경계를 설계 작업으로 분리하는 편이 좋다.

비교 기준은 Oracle 19c, Altibase 7.1의 디스크 저장 구조, PostgreSQL 18이다. 제품별 구현을 복제하는 것이 아니라 트랜잭션 의미, 복구 보장, 정상 실행 비용과 확장성을 대조한다. 이 리뷰의 제안을 채택해 형식이나 의미를 바꾸는 작업은 해당 소유 계약에서 별도로 구체화한다.

<a id="scope"></a>
## 1. 기준과 확인 범위

| 항목 | 기준 |
| --- | --- |
| 검토 source | main `2397633015e7410bbc591c81095ba49f621073ca`, PR #49 병합 뒤 |
| 이전 리뷰 | PR #48, merge `a45e37c66647c72cb60a8c31dde4b9863b1dab62`. UPDATE 이전의 DEV3 방향 리뷰이며 PR #47의 DELETE 검토와 다름 |
| 범위 | DEV3 저장 설계·16개 작업 명세·일정 01–19, 구현된 첫 Row profile와 주요 WAL·전이·읽기·복구 경로 |
| 방법 | 계약과 관련 코드의 정적 대조, 기존 실행 증거 확인, 공개 DBMS 공식 문서 비교 |
| 한계 | 전체 소스의 모든 경로를 감사한 결과나 신규 성능·복구 시험 결과가 아님. 기존 테스트 결과는 원 source와 범위의 증거로 유지 |
| 변경 분류 | D0 기록 추가. 현재 계약·제품 의미·완료 상태·일정 가중치를 바꾸지 않음 |

현재 환경에서 PostgreSQL·Altibase의 외부 로컬 source checkout을 새로 조사하지 않았다. 저장소에 보존된 과거 비교 기록과 이번에 확인한 공식 문서를 구분한다. Altibase 비교는 디스크 테이블과 Undo tablespace에 한정한다.

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
## 3. 일반화 전에 다룰 여섯 가지

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

기본 RC, 충돌 시 대기·statement 재시작 또는 전체 abort, cursor 수명, deadlock rollback 범위를 일반 DML 전에 하나의 동작표로 동결할 것을 권고한다. statement 재시작은 snapshot만 바꾸는 기능이 아니다. 이미 전달한 결과, 잠금, 부분 변경, rollback 자원과 재실행 가능한 효과까지 정의해야 한다.

Serializable을 단순 snapshot isolation으로 바꿔 충돌을 줄이는 것도 같은 보장을 유지하는 최적화가 아니다. write skew 등 허용하는 이상 현상이 달라진다. Oracle의 이름과 동작을 따를지, PostgreSQL처럼 직렬 실행에 상응하는 보장을 목표로 할지 명시해야 한다. 이 선택을 현재 첫 UPDATE의 제한을 푸는 구현과 섞지 않는 편이 좋다.

<a id="representation"></a>
### 3.5 행 표현과 Undo 배치의 공간 비용

[표현·자원 계약](../contracts/dev3-resource-bounds.md)의 행 header 168 B와 index value 160 B는 작은 값에서 큰 비용이다. 현재 첫 profile의 행별 Undo page와 공유 Root/Job 한도는 슬롯 수만 늘려 해결하기보다 page 안에 여러 Undo record를 배치하는 일반 구조와 함께 검토할 필요가 있다.

PR #48의 Int64 계산은 정정이 필요하다. [Row 형식](../contracts/dev3-row-format.md)에서 Int64 한 열의 canonical 값은 `4 + 1 + 8 = 13 B`다. cell은 `180 + 13 = 193 B`, slot을 더한 entry는 197 B이므로 leaf 예산 16,188 B에는 82행이 들어간다. 16 KiB page 대비 canonical 값 비율은 약 6.51%, 실제 8 B 정수만 세면 약 4.00%다. 이전 리뷰의 cell 185 B·85행·2.59%는 최소 5 B 값에 해당하며 Int64 계산이 아니다. **수치는 바로잡아야 하지만 작은 행의 공간 효율이 낮다는 결론은 유지된다.**

짧은 참조, 중복 namespace·identity 표현의 축소, Undo record packing, page 단위 거래 참조를 대안으로 비교할 것을 권고한다. Oracle의 ITL이나 Altibase의 TSS 구조는 참고점이다. ITL 도입에는 slot 고갈·재사용·가시성 복원 문제가 따라오므로 단순히 header를 줄이는 변경으로 취급하지 않는다.

Altibase 7.1의 Undo tablespace는 TSS와 Undo segment를 짝지은 transaction segment를 설명하며, 하나의 transaction segment에서 한 update transaction이 실행된다. 이처럼 거래 상태와 Undo 배치를 함께 보아야 한다. XADB의 영속 edge/count를 줄이는 경우에도 snapshot·rollback·reader pin·index·overflow·복구가 모두 요구하지 않는다는 회수 증명이 선행해야 한다. commit 순서만으로 즉시 회수하는 대안은 충분하지 않다.

<a id="checkpoint"></a>
### 3.6 checkpoint의 bounded RAM과 반복 I/O

[checkpoint 불변 입력 설계](../contracts/dev3-task-13-checkpoint.md#immutable-capture-source)는 pin한 base와 WAL prefix를 같은 cut까지 재생하고, 같은 cut의 bounded ATT/DPT·volatile 소유 snapshot을 결합한다. 변경 중인 home page를 정답으로 삼지 않는 점과 flush 완료를 WAL만으로 재구성하지 않는 점은 타당하다.

문제는 전체 identity를 메모리에 두지 않기 위해 B개씩 선택하며 base와 WAL을 반복 순회한다는 것이다. identity가 T개, base가 I bytes, prefix가 W bytes라면 입력 순회량은 적어도 `ceil(T/B) × (I + W)`에 비례한다. B가 고정되고 I가 T에 비례하면 DB 크기에 대한 제곱 비용이 생길 수 있다. 계약도 bounded RAM이 선형 I/O를 뜻하지 않는다고 명시한다.

**권고는 증분 inventory, 디스크에 정렬·색인된 ownership metadata, 변경 집합을 이용하는 대안을 비교하고 scan 횟수·읽기 바이트·WAL pin 기간의 상한을 함께 정하는 것**이다. checkpoint가 보존한 prefix 때문에 새 WAL 공간이 부족해지는 경우에도 기존 rollback·flush·checkpoint가 끝날 자원을 남겨야 한다. 이 문제는 아직 남은 설계에 속하므로 실제 checkpoint 성능 결함이 측정됐다고 표현하지 않는다.

<a id="remaining"></a>
## 4. 남은 기능별 설계 판단

| 영역과 소유 문서 | 현재 방향에 대한 판단 | 구현 또는 일반화 때 닫을 조건 |
| --- | --- | --- |
| [트리·일반 행 변경](../contracts/dev3-task-06-tree-rows.md) | stable anchor와 logical Undo, redo-only 구조 변경은 유지 | own/repeated write·occupied-before·다단 Undo, split 뒤 rollback, 큰 행 복원과 recovery를 함께 검증. 순차 RowId에서 균등 split은 왼쪽 page를 반쯤 비운 채 남길 수 있으므로 rightmost split 정책 비교 |
| [cursor와 overflow](../contracts/dev3-task-06-tree-rows.md) | last key로 재탐색하는 방식은 구조 변경에 강한 단순 출발점 | 매 행 root 재탐색 비용과 batch/hint 대안 비교. 검증된 hint 수명·자기 변경 무효화·overflow pin/회수 명시. persistent sibling link를 즉시 필수로 만들 필요는 없음 |
| [인덱스·PK/UNIQUE](../contracts/dev3-task-07-indexes.md) | versioned presence는 XADB의 stable RowId와 일관된 가능한 선택 | row와 모든 index의 원자적 Undo/CLR, 거래 전체 own 최종 상태, statement-final UNIQUE, all-NULL 예외와 부분 NULL 충돌, snapshot의 옛 key 탐색을 함께 검증 |
| [Catalog·DDL](../contracts/dev3-task-08-catalog-ddl.md) | generation과 stale handle, 버전 있는 Catalog는 유지 | 실제 CREATE의 제한을 넘어 DROP·자식 index 의존·이름 재사용·실패 복구 닫기. 전체 DB close/drain을 쓰는 첫 offline build는 안전한 출발점이나 table 범위로 좁힐 가능성 검토 |
| [동시성](../contracts/dev3-task-10-concurrency.md) | page latch와 거래 lock 대기·rollback 책임을 나눈 방향은 타당 | 일반 observation·대기·deadlock·취소와 여러 Status slot, 같은 table의 다른 행 writer를 통합. 거래 의미는 [3.4절](#transaction-semantics)에서 먼저 결정 |
| [buffer/flush](../contracts/dev3-task-05-buffer-flush.md)와 [recovery](../contracts/dev3-task-12-recovery.md) | WAL-before-data·DWB·generation·ACK 보존은 유지 | 첫 extent와 resident profile을 넘어 dirty flush→evict→reload, 여러 extent, buffer보다 큰 working set, 낮은 자원에서 rollback 진행성 검증. process cut을 torn write·전원 장애·복구 중 crash의 전체 증거로 확대하지 않음 |
| [checkpoint](../contracts/dev3-task-13-checkpoint.md)와 [회수](../contracts/dev3-task-14-reclamation.md) | 같은 cut의 소유·재사용 금지와 정리 진행성은 유지 | 반복 scan 비용, current/previous/new output 공존, 장기 reader의 Undo 보존과 writer admission 정책, cleanup 자체의 WAL 예약 닫기 |
| [backup·restore](../contracts/dev3-task-15-backup-integrity.md) | offline exact-cut C는 첫 제품 범위로 적절 | 전체 namespace 재작성 비용과 stale handle 방지의 관계 검토. restore 뒤 새 ACK와 두 번째 복구까지 검증. online backup/PITR는 DEV8 범위와 구별 |
| [최종 인수](../contracts/dev3-task-16-qualification.md)와 [테스트 계획](../plans/dev3-test-plan.md) | 최종 S5의 정확성·복구·대형 profile을 나눈 방향은 타당 | 기능 구현 시 작은 계수 관측을 붙이고, 최종에는 1/2/4 writer·같은 table 다른 행·장기 reader·buffer 초과 데이터를 검증. API 읽기량, 실제 장치 I/O, WAL량, 처리시간을 별도 지표로 보고 |

인덱스에서 PostgreSQL이 heap의 tuple version과 HOT에 맡긴 역할을 XADB의 index value에서 단순 삭제하면 옛 snapshot의 key 탐색을 잃을 수 있다. 반대로 현재 큰 index value를 반드시 유지해야 한다는 뜻도 아니다. 필요한 과거 의미를 어느 구조가 소유할지 먼저 결정해야 한다.

DWB도 성능 개선 대상이 될 수 있지만, PostgreSQL의 full-page image와 교환 가능한 비용 항목 하나로 취급하지 않는다. page 손상 복구의 정답 입력과 보존 기간을 먼저 유지하고 batch·sync 비용을 비교해야 한다. 장기 reader 때문에 Undo를 보존하는 정책 역시 writer 처리량과 읽기 성공 보장의 선택이다. Oracle의 보존 정책과 다르다는 이유만으로 오류라고 판정하지 않는다.

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
2. **live replay·redo 형식·공유 메타데이터를 각각 작은 설계 단위로 분리한다.** 기존 복구·적용 순서·자원 증명을 유지하는 대안을 비교하고 채택된 것부터 owner 계약에 반영한다. Undo packing과 짧은 참조의 형식 영향도 함께 확인한다.
3. **일반 행 변경과 트리를 닫는다.** own/repeated write, Undo chain, overflow, split 후 rollback·복구까지 완성하고 이후 index가 소비할 안정된 의미를 만든다.
4. **인덱스와 동시 거래를 통합하면서 비용을 관측한다.** 같은 table의 다른 행 writer, 작은 행과 큰 행, 장기 reader를 구분하고 checkpoint의 scan/WAL retention 비용을 일반 규모로 넓히기 전에 평가한다.

현재 [일정 기준선](../plans/dev3-schedule-baseline.md)은 PR #49까지 33/86일, 38.37% 적립과 53일 미적립 가중치를 기록한다. 이는 일정 가중치의 비율이며 제품 완성도나 시험 coverage 비율이 아니다. 기존 56일 잔여 예측은 UPDATE 적립 전 값이고, 07 적립 뒤 새 날짜 예측은 아직 없다. 위 개선을 채택해 작업량이 늘면 현행 기준선의 재견적 절차에 따라 반영해야 하며, 이 리뷰만으로 분모나 완료 항목을 바꾸지 않는다.

<a id="evidence"></a>
## 7. 실행 증거의 해석

[UPDATE 검토 기록](../../module-gate/evidence/row-update/review.md)은 원 source의 선택 197개 사례와 native 18개 process를 보고한다. 이번 검토에서 제품 시험을 다시 실행하지 않았다. 기존 native 결과의 inventory·receipt·종료 결과를 확인한 것은 그 기록의 범위를 확인한 것이며, 일반 UPDATE·일반 crash 내구성·전체 DEV3의 신규 PASS가 아니다.

기존 검토는 크기가 줄어든 committed UPDATE의 재시작 사례 보강을 기록하며, 커진 committed UPDATE의 재시작은 여전히 미검증이라고 명시한다. 이 한계를 보존한다. 문서 추가 PR의 포맷·정적·문서 검사는 해당 PR에서 별도로 보고하며 제품 실행 증거와 구분한다.

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
