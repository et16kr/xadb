# DEV4 — embedded SQL 구현·검증 계획

2026-10-19 · **DEV4-DESIGN-001·REVIEW-002–005 ACCEPTED / 제품·Q1 NOT_RUN**

[SQL 소유 계약](../contracts/dev4-sql-design.md) · [소스 조사](../evidence/dev4-source-research.md) · [단계 완료](stage-completion.json) · [저장 선행](dev3-implementation-plan.md) · [개발 운영](../policies/development-policy.md)

2026-10-22 [18개 세부 작업 분류](dev4-work-breakdown.md#catalogue)는 아래 D4-0–7을 공급·소비 단위로 나누고 선행·산출물·완료 조건·기존 인수를 연결한다. 후속 [분배 보완 002](dev4-work-breakdown.md#work-review-002)는 기존55개 ID 아래 큰 작업을 더 나누고 INSERT·commit/reopen·rollback을 먼저 연결하며, 상태 모델·R1–R4·PF10 개별 산출물을 배정한다. [공급 순서](dev4-work-breakdown.md#sequence)와 [시험 선택](dev4-work-breakdown.md#test-scope)을 함께 적용한다. 아래 과거 전체/하위 회귀·V2/V3·current-build closure 문구에도 [변경된 코드만 시험하는 사용자 지시](../../AGENTS.md)가 우선한다. DEV3 S5의 전수 예외를 DEV4에 확대하지 않는다. 제품·Q1 상태는 NOT_RUN을 유지한다.

[재점검 003](dev4-work-breakdown.md#work-review-003)은 공통 parser/INSERT와 SELECT clause 선행을 나누고, bound dependency 검사를 SELECT/DML 공통으로 공급하며, 오류 변환·독립 모델·typed 관측·외부 ACK의 담당을 구체화한다. [상대 분량·첫 종료점](dev4-work-breakdown.md#workload-review)은 구현/검증 준비 비용을 포함한 배정 기준이며 실측 일정이 아니다.

<a id="entry"></a>
## 1. 착수와 완료를 구분한다

DEV4의 기존 task는 XADB-011, gate는 Q1이며 실제 엔진 통합·인수는 DEV3에 의존한다. [D4-PREP-001](../contracts/dev4-independent-frontend.md#scope)의 독립 lexer/parser·값 처리와 검증 도구는 해당 leaf 입력 동결 뒤 먼저 개발할 수 있다. [병행 준비 계획](dev4-preparation-plan.md)은 허용 ID·공통 자원 부분·FE01–16과 DEV3 수신을 구분한다. DEV2 P0/K0 volatile 실행이나 이관 사실, DEV3 설계 검토를 실제 엔진 공급 증거로 사용하지 않는다. 현재 SQL crate·CLI·실행 harness는 없고 run entrypoint/evidence는 null/빈 목록을 유지한다. 이번 계획 확정만으로 개발·시험을 시작하지 않는다.

엔진 소비 구현의 선행 인수에는 DEV3-DESIGN-003의 실제 disk row/catalog/index·bounded scan/point/range, statement token·영속 undo mark와 read-your-writes, 부분 NULL·문장 최종 상태 unique·key lock/overflow, WAL-before-page·commit WAL sync·UNKNOWN/RecoveryRequired fencing·reopen, fuzzy checkpoint/일관 backup/restore·retention과 해당 최소 완료 증거가 필요하다. 새 DEV3 결정의 별도 독립 검토·채택은 확인했으며 실제 공급 증거를 확인한 뒤 해당 구현을 시작한다. 기존 stage DAG와 XADB-011의 통합 의존을 유지한다. COMM/PART/NG·고급 SQL을 추가 선행으로 만들지 않는다.

| 공급 경계 | SQL이 받을 계약·실제 인수 |
| --- | --- |
| catalog/DDL | 고정 snapshot lookup, 열 type/nullable/byte length, table/index identity, schema epoch/index 정의 대조. PK/UNIQUE 포함 create와 drop의 단일 원자 native DDL. SQL precommit 종료 뒤 새 catalog 검증 |
| row/index | typed scan/point/range+own writes/undo consistent read, cursor 수명/EOF/RowId 순서, owned row, 부분 NULL 및 최신 committed+own final delta에 대한 문장 최종 UNIQUE·key lock |
| transaction | begin_statement token을 대상 scan→단일 execute_statement→finish_statement까지 공유. 최초 q 선택/horizon 등록의 gate·published q 보존 기준·guard 인계 수신. 영속 undo mark·rollback 예약·논리 payload와 실제 WAL 예산 구분, ReadOnly/ReadWrite·table 단위 Serializable validation |
| durable/lifecycle | commit outcome/certainty/fencing, DDL prior/ddl 두 outcome과 NotCommittedRecoveryRequired 전달, 실제 undo 완료/실패·Drop 소유 인계·engine close/open, owner temp namespace·quota·cleanup 효과 port |
| verification | native literal row/history oracle·실제 ACK 수집·fault hooks·독립 lower regression. imported receipt를 현재 fingerprint에 대입하지 않음 |

공급 API가 설계와 다르면 해당 owner에게 필요한 최소 경계를 D2로 통합한 후 SQL에서 소비한다. private storage 접근, SQL에서 UNIQUE 재구현, 여러 commit으로 하나의 DDL/DML 자체를 쪼개는 우회는 허용하지 않는다. SQL DDL의 이전 거래 commit과 독립 DDL 거래는 명시한 서로 다른 두 거래이며 각각의 원자성과 outcome을 유지한다.

DEV3-REVIEW-004의 모델·실제 disk 인수를 구분해 수신한다. D3-M01–M06의 논리 상태 모델만으로 DWB·undo/rollback의 실제 파일 시험을 대체하지 않는다. D4-F02에는 statement/transaction token과 undo·lock·자원 예약의 종료/인계 책임을, D4-F05에는 실제 admission 한도·DWB batch/deadline·version-edit gate 계측을 기록한다. 기존 stage DAG를 바꾸거나 SQL 안에 저장 예약 manager를 추가하지 않는다.

<a id="sequence"></a>
## 2. 구현 순서와 소유

아래 D4-0–7은 XADB-011 내부 책임 묶음이며 새 제품 stage나 완료 gate가 아니다. [4.01–4.18 작업 분류](dev4-work-breakdown.md#catalogue)의 좁은 실행 단위가 실제 배정 기준이다. 특히 D4-5의 상태/commit 기반과 D4-6의 oracle·관측을 D4-3 이전/구현 동반으로 공급하고, D4-4 전체 완료를 DDL·prepared의 선행으로 두지 않는다. 전체 분류의 완료를 서로 선행으로 묶지 않으며 DEV3→DEV4 진입 조건은 유지한다. 실제 배정·병렬 실행은 아직 없다. 파일은 기존 계획 패키지 안에서 구현할 때만 추가하고 Cargo/지문/clean export를 함께 등록한다. 문법·의미 정의와 independent oracle의 생산 구현은 분리한다.

| 기존 ID | 책임·산출물 | 선행·완료 범위 |
| --- | --- | --- |
| D4-0 | 공급 경계 대조·F01–F05, 독립 literal/vector inventory, oracle 부정 사례 | 소비 leaf의 공통 API와 byte/오류 입력 동결 뒤 관련 구현 착수. D4-PREP-001 앞단은 독립 진행, engine 소비는 실제 DEV3 선행 수신 |
| D4-1 | lexer/parser와 bounded AST·span·CLI 문장 경계 | F01. 정상·경계·잘린 입력·unsupported·fuzz. parser 단독 PASS는 Q1 아님 |
| D4-2 | binder·catalog dependency·타입/NULL·metadata·DDL lowering | D4-1 및 실제 DEV3 catalog/DDL. 대문자 이름·SQL 빈 text·identity/schema race·PK 강화·원자 DDL |
| D4-3 | INSERT 입력·원자 statement, RowScan/filter/project/limit, UPDATE/DELETE 대상 확정 | 해당 D4-2·D4-5 기반과 실제 native statement. INSERT는 SELECT cursor에 의존하지 않음. 작은 commit/reopen·문장 실패를 D4-6 관측과 함께 먼저 인수 |
| D4-4 | index point/range 선택, external sort·bounded spill/cleanup | D4-3, 실제 DEV3 index와 F03. force-scan 및 literal 기대값 대조·정렬·큰 working set |
| D4-5 | session/prepared/bind/result 상태, transaction/autocommit·stale 오류·DDL precommit | F02·해당 D4-2/3/4 소비. 상태/commit/abort 기반을 먼저 공급하고 SELECT 종료·DDL·prepared를 각각 통합. direct/prepared 공통 표현은 초기부터 동결 |
| D4-6 | embedded CLI·XADBTest SQL adapter·native/SQL history 연결·PF10 | 해당 기능과 F04/05. 최소 adapter·fault/ACK 관측·계측은 첫 경로에 공급, R1–R4·restore·P10-A–E를 각각 배정. CLI 전체 완성을 기본 복구/비용 비교의 선행으로 두지 않음 |
| D4-7 | Q1 전체 inventory 수합·별도 검토·현재 build closure | 모든 필수 variant·SC02·KU13·PF10·영향 lower regression PASS. official distribution은 별도 |

독립 lexer/parser/값·검증 도구는 [병행 준비 계획](dev4-preparation-plan.md#sequence)의 좁은 범위에서 구현할 수 있다. 실제 engine 소비의 선행은 유지한다. D4-2와 D4-5의 공통 BoundStatement/handle/error 계약은 해당 통합 전에 고정하며 아직 사용하지 않는 전체 handle/session API를 lexer의 선행으로 만들지 않는다. D4-3/4의 executor 조립은 한 소유자가 통합한다. 별도 구현 agent를 실제 배정하려면 파일 소유·공유 변경·기준 revision을 그때 기록한다. 현재 병렬 성능 향상을 주장하지 않는다.

<a id="freeze"></a>
## 3. leaf 구현 전 남은 동결 항목

| ID | 이 설계가 정한 것 | 구현 전에 고정할 실제 입력 |
| --- | --- | --- |
| D4-F01 | 문법·대문자 이름·SQL 빈 text 정규화·부분 NULL/문장 최종 UNIQUE·span·수치 한도 | exact token/AST/Rust signature, 원래 text 타입 보존과 native canonical fixture, 모든 production error→SQL code/effect vector, malformed/unsupported token corpus와 hash |
| D4-F02 | `?`·prepare 추론·owned bind·metadata·상태표·stale reject·DDL 두 outcome | engine catalog dependency accessor, scan/mutation 공통 statement token·opaque row observation과 대기 없는 X-lock 검증, snapshot q/등록·aging의 동기화와 published q 보존·guard 인계 증거, session/result 수명, DDL validation/precommit 경계·두 receipt/오류/취소·NotCommittedRecoveryRequired 전이, ROLLBACK/close_session/Drop의 undo 소유 인계·fencing API·literal vector·실제 thread barrier |
| D4-F03 | bounded external sort와 SQL memory/disk/FD 예산 | 임시 file/record/manifest byte layout·독립 hex, owner cleanup port·disk reservation, merge/ENOSPC/short I/O fault cuts와 allocation inventory |
| D4-F04 | Q1 필수 사례와 native/literal independence·rollback/recrash | SQL worker adapter/protocol version·DDL 두 outcome/ACK·NotCommittedRecoveryRequired를 포함한 row/error/event schema, D4-V17–28·DEV3 WAL/undo/CLR·bootstrap/예약 복원과 fault 교차 inventory, mutation·negative verdict·seed/schedule·실행 명령 |
| D4-F05 | PF10 workload군·기본 V2/V3·계측 책임 | 실제 host/CPU/FS·fixture cardinality/distribution/hash·release binary/feature, warmup/sample·RSS/I/O·계측 비용·회귀 threshold·paired 순서와 재현 recipe |

담당은 해당 leaf 구현자와 SQL/engine 계약 담당, 통합 담당이며 실제 이름을 착수 기록에 배정한다. 빈 담당·null entrypoint를 동결 완료로 바꾸지 않는다. 각 동결은 DP01 별도 검토를 받고 구현 후보를 실행하기 전에 hash를 기록한다. 실패를 없애기 위해 사후 예산/variant를 줄이지 않는다. D4-PREP-001의 [확정/잔여 표](dev4-preparation-plan.md#freeze)는 F01/F04 앞단과 필요한 자원 부분을 분리하며 이 표 전체의 동결 완료를 뜻하지 않는다.

002의 배정에 따라 F02에는 [수명 통합 상태/action 표](dev4-work-breakdown.md#lifecycle-model)와 유한 모델 입력을, F03에는 [DEV3 sorter 재사용·분리 판단](dev4-work-breakdown.md#sort-reuse)을 포함한다. F04는 각 case의 주 담당 실행 ID·소비자·최소/확대 변형·관련 diff·원래 receipt를 연결하며 상태 탐색기/known-bad·R1–R4·restore를 각각 선택 가능하게 동결한다. F05는 P10-A–E별 fixture/수치 예산/명령을 동결한다. 준비 산출물과 동결 완료 후의 구현 소비를 구분하여 F03/F05 자신을 준비 작업의 완료 선행으로 두지 않는다.

003에서 F01/F02 오류 vector의 실제 변환 구현은4.13.1.2가 소유하고 각 기능이 phase·실제 cleanup/outcome을 공급한다. F04 literal/coverage는4.16.1.1/3.1, typed 관측은4.16.2.1, 외부 ACK/fault 전달은4.16.2.2에 배정한다. CLI·worker가 별도 오류 효과 판정을 만들거나 실제 engine 결과를 정답 계산에 공유하지 않는다.

<a id="acceptance"></a>
## 4. Q1 필수 인수 행렬

이 표는 기존 SC02·KU13·PF10의 세부 의무다. stage-completion.json의 기존 required_test_refs를 교체하거나 줄이지 않는다. 각 행의 필수 변형을 leaf에서 case ID로 펼치고 누락·미도달·timeout은 PASS가 아니다. 현재 전부 NOT_RUN이다.

| ID / 연결 | 필수 변형 | 독립 판정·필수 관측 |
| --- | --- | --- |
| D4-T01 / SC02 | 모든 문법 production·keyword·식별자 대문자/quote·PUBLIC prefix·주석·UTF-8·EOF/span, bytes/tokens/nesting 경계±1 | D4-V18·literal AST/metadata/오류 위치, multi-statement API 거절, batch 경계가 literal의 `;`를 나누지 않음 |
| D4-T02 / SC02 | Int64 min/max±1·Bool·SQL 빈 text literal/bind·native 빈 text·빈 Binary·공백/NUL·UTF-8 길이·NULL, 모든 비교×타입·T/F/U | D4-V19/20·독립 literal/단순 evaluator. 빈 text의 untyped NULL 오변환·native read 정규화·암묵 cast·NULL=TRUE mutant 거절 |
| D4-T03 / SC02,KU13 | 네 DDL·PK/UNIQUE/NOT NULL, composite 1/4/5·index 8/9·key1024/1025, 부분/전체 NULL·drop/recreate·PK index 보호·precommit | D4-V17/21/22. 각 DDL catalog+row/index는 old/new 중 하나. 이전 DML과 DDL의 Committed/Aborted/NotAttempted/NotCommittedRecoveryRequired/Unknown은 별도 판정. native DDL-in-txn 거절 유지 |
| D4-T04 / SC02,KU13 | INSERT multi-row·UPDATE/DELETE empty/all/some·동일 값 update·missing column·문장 최종 unique 및 중간 key/quota 오류 | D4-V05/06/17, affected count·이전 statement 유지·모든 index rollback·대상 한 번·256/257 경계. native key 교환 성공 하위 회귀, SQL 지원으로 오인하지 않음 |
| D4-T05 / SC02,KU13 | scan/point/range·복합 prefix·inclusive/exclusive·NULL bounds·AND residual·OR/NOT fallback·자기 insert/update/delete·undo consistent read | D4-V25의 all-null unique 다중 결과, literal rows + 독립 native history + force-scan의 삼중 대조. 단건 조기 종료·누락/중복/잘못된 NULL bound mutant 거절 |
| D4-T06 / SC02 | ORDER BY ASC/DESC×NULLS 기본/명시×복수 열·미선택 열·동률·LIMIT 0/1/Int64 max | 별도 sort 기대값. 무순서 결과는 bag multiplicity, tie는 허용 순서 집합, unique tie-breaker 사례는 exact order |
| D4-T07 / SC02 | 모든 지원 DML/SELECT direct/prepared, marker 위치·hint/추론·count/type/NULL/bytes·caller buffer 해제 | immutable metadata·typed 값 독립성, bind 실패 원자 보존, 값의 SQL injection 재해석 없음 |
| D4-T08 / SC02 | 모든 handle 상태 전이·EOF/조기 close·double close·다른 session/engine·slot reuse·stale plan, 재bind/재execute·prepare/DDL race·D4-V15/16·close_session/Drop undo 실패 | 첫 효과 전 거절, old result/token 재사용 없음, terminal descriptor도 bounded·fetch 실패 후 거래/statement 상태·정상 cleanup 뒤 memory/pin/FD/quota 회복. 미완료 undo/lock 소유 유실과 정상 rollback 오응답 거절 |
| D4-T09 / SC02,KU13 | ON/OFF×BEGIN/COMMIT/ROLLBACK/DDL×성공/오류, read-your-writes·두 table mixed transaction·같은 table writer 및 무변경 ReadWrite conflict·독립 table writer 동시 성공 | 실제 2-thread schedule·external history·일관된 snapshot. 같은 table conflict 누락과 무관한 table까지 전역 q로 거절하는 mutant 모두 거절. 대기 유무와 무관한 잠금 후 논리 버전 변경은 execute 중 해당 행의 첫 effect 전에 전체 거래 abort, SQL 숨은 재실행 금지. Oracle READ COMMITTED 결과로 혼용하지 않음 |
| D4-T10 / SC02,KU13 | update WAL/undo/CLR·data/undo flush·commit record append/sync/publication/reply cut, short write·sync error·kill·reopen·반복 recovery·checkpoint/backup/restore·DDL 두 거래 사이 cut | D4-V23/24 및 아래 R1–R4. 실제 ACK·ABORTED·NotCommittedRecoveryRequired·UNKNOWN history와 native oracle. 첫 update WAL과 첫 commit record 시도 구분, current-build ACK 보존·partial batch 금지·durable 이후 취소 오판 금지 |
| D4-T11 / SC02 | parse/bind/execute/fetch/sort·commit 전후 cancel/timeout, statement/transaction rollback I/O 실패·도중 crash·복구 재장애, allocation/quota·spill ENOSPC·truncated file·cleanup 실패 | D4-V22/23·R1/R2, undo 완료 전 정상 ABORTED 응답 금지·확정 NotCommitted와 RecoveryRequired/Fenced 보존·복구 뒤 미commit 거래 전체 소멸, incomplete prefix, orphan quota 유지·소유 namespace만 회수 |
| D4-T12 / SC02 | interactive/batch CLI·stdin/file·Unicode/NULL/binary 출력·metadata/count/error·중간 실패·EOF·DDL 두 outcome | typed embedded 결과와 독립 출력 fixture, exit status·stdout/stderr·종료 시 숨은 commit 없음. DDL 실패 뒤 이미 commit된 이전 DML을 명시 |
| D4-T13 / KU13 | literal SQL/native 동등 workload·row/index/catalog recovery·알려진 잘못된 결과/ACK/order/error·잘못된 binary/recipe | 같은 production 경로 둘의 일치만으로 PASS 금지. oracle/runner는 부정 사례를 거절해야 함 |
| D4-T14 / PF10 | 아래 workload/계측 모드·큰 working set·sort spill·native 대비 SQL 비용 | plan→실제 node→storage 경로·동등 결과·자원 상한·비교 유효성·bundle 재현 |

[DEV4-REVIEW-005](../contracts/dev4-sql-design.md#mathematical-review)의 D4-V27은 T04/T10/T11에, D4-V28은 T08/T11에 포함한다. 반복 statement 실패 뒤 S1 논리 값과 실제 WAL/CLR·undo/page·transaction lock 회계를 독립 대조하며 quota counter의 과거값 복원 mutant를 거절한다. 전체 abort 예약은 보존한다. cursor snapshot은 유지하면서 execute/fetch/close마다 operation context를 새로 연결하고, 이전 호출 deadline 경과·늦은 cancel·현재 호출 취소·parked 재개의 원래 deadline을 검사한다. F02에 API·소유/해제 시점을, F04/F05에 fixed schedule·잔여 charge/예약 및 peak를 동결한다. 두 vector의 제품 실행은 NOT_RUN이다.

각 지원 statement×타입에는 정상/NULL(허용/거절)/빈 값/경계/오류가 필요하다. direct와 prepared는 지원 교집합 전부를 비교하고 DDL/transaction control은 prepare에서 명시 Unsupported를 확인한다. 각 index kind(PK/unique/nonunique)×단일/복합 key의 적용 가능한 point/range/DML·crash 변형을 포함한다. 전체 문법·타입 항목이 어느 test ID에 연결됐는지 coverage inventory를 만든다. 기능이 없는 조합은 근거 있는 N/A이며 후속 필수 기능을 삭제하지 않는다.

T04/09/11/14는 [초기 처리 범위](../contracts/dev4-sql-design.md#capacity-boundary)를 함께 인수한다. 256/257개 대상의 단일 DML, 여러 성공 statement가 거래 한도에 도달한 뒤 실패, 같은 table의 다른 행/독립 table writer를 구별한다. count 성공을 부분 처리와 혼동하지 않고 실패한 문장 전체 복원·앞선 성공 변경 유지·명시 COMMIT/ROLLBACK 결과를 확인한다. 낮은 WAL/frame/FD 여유에서 cancel·close_session·Drop 인계와 rollback을 수행하여 예약 소유 유실·거짓 ABORT를 거절한다. 한도가 큰 workload를 거절했다면 처리량 표본에서 조용히 제외하지 않고 실패 원인/비율로 보고한다.

T09/D4-V26은 `T1 대상 materialization → T2 commit/unlock → T1 즉시 X lock` barrier를 추가한다. T1 execute의 conflict와 전체 undo, T2 ACK 보존을 commit 호출 전에 검사한다. 기다린 lock·삭제·같은 값의 새 버전도 충돌해야 하며, 앞선 own-write 관측·논리 변경 없는 split은 행 버전 충돌을 만들지 않는다. native ReadCommitted 반례는 DS06의 선행 증거로 받고 SQL은 현재 Serializable로 실행한다. SQL이 observation을 누락/대체하거나 commit validation으로 execute 검사를 대신하는 mutant를 거절한다.

T09는 [DEV3 snapshot 획득](../contracts/disk-storage-design.md#snapshot-acquisition)의 실제 DS04 선행 증거를 수신하고, idle prepare와 execute 최초 획득을 writer/aging과 교차한다. q10 선택/등록 사이 및 status11 설치/frontier10의 경계에서 catalog/row 결과가 q10과 일치해야 한다. SQL이 engine guard 대신 raw q를 넘기거나 result 인계 중 마지막 horizon 소유를 놓는 mutant를 거절한다. 이 시험은 SQL ReadCommitted 노출을 추가하지 않으며 현재 Serializable·implicit ReadOnly 경로를 검증한다.

<a id="aggregate-pressure"></a>
T07/T08/T11/T14는 [DEV4-REVIEW-006](../contracts/dev4-sql-design.md#extensibility-review)의 합산 자원 반례를 포함한다. F02/F03/F05에서 2개 이상 session의 prepare/bind 교체와 sort/merge/fetch를 겹쳐 SQL 전체 memory64 MiB·temporary disk1 GiB·FD64 각각의 잔여량−1/동일/+1 경계를 고정한다. 개별 query 한도 이내여도 DB 총량 초과는 거절하고, old+new binding 공존 예약 실패는 이전 binding/metadata로 재실행 가능해야 한다. concurrent merge의 입력+출력 peak와 삭제 실패 bytes는 새 query admission에도 남는다. 한 query의 ENOSPC/cancel/close/crash cleanup이 다른 query의 파일·pin·quota를 해제하지 않아야 하며, 소유 cleanup은 새 foreground 예약 없이 진행한다. 오염/절단 spill 거절·실패 unlink 재시도·복구 뒤 소유 namespace cleanup도 실제 I/O로 확인한다. query별 검사만 하는 mutant, binding 교체 전 이전 charge 해제, 삭제 실패를 반환량으로 계산하는 mutant를 거절한다. 현재 **NOT_RUN**이며 SQL aggregate quota가 이번에 처음 도입된 것은 아니다.

<a id="verification"></a>
## 5. 독립성·fault·검증 강도

SQL oracle는 작은 typed row bag·3값 evaluator·명시 순서/허용 tie 집합·serial history로 기대값을 만든다. production parser/binder/optimizer/codec/row replay를 기대값 계산에 재사용하지 않는다. native/SQL 비교는 adapter 차이 검출을 보완하며 단독 oracle가 아니다. 외부 DBMS는 optional differential 연구 대상이고 XADB 의미의 정답은 아니다.

SC02의 실행/오류 행렬에 [설계 vector](../contracts/dev4-sql-design.md#vectors)를 연결한다. core 경계의 malformed 입력과 독립 byte vector, stale generation/parameter lifetime·누락 cleanup 반례를 먼저 만든다. shared protocol은 입력/관측 타입만 공유하고 semantic oracle는 testing/reference 책임에서 독립 구현한다. SQL adapter의 실제 package/dependency는 구현 때 저장소 정책과 함께 D2로 등록하며 빈 worker를 만들지 않는다.

V1은 신규 vector/원래 반례와 인접 경계, V2는 관련 전체 property suite의 고정 64 seed×최대128 operation·보존 fuzz corpus·실제 disk 통합·하위 회귀를 요구한다. V3는 256 seed×최대1024 operation(앞선 seed 포함), 관련 parser/codec target당 최소30분 fuzz, 관련 실제 동시성 suite 최소30분, 비결정적 필수 fault 셀 3회 fresh fixture 반복을 요구한다. DP03보다 약화하지 않는다. 입력 경계는 seed가 우연히 생성하기를 기다리지 않고 고정 사례로 둔다.

작은 handle/session 상태 모델은 2 session, 각 prepared 2개, 활성 result 1개/session, operation 길이 6 이내의 유한 공간을 F02의 명시 action/parameter 동치류로 전수 탐색한다. 탐색 상태/전이 최대 수·완주 receipt는 구현 전에 계산·고정하고 상한 도달은 INCONCLUSIVE다. 이 모델과 실제 thread·I/O 검증은 서로 대체하지 않는다.

상태 계약은 4.12.1·4.13.1·4.14.2가 공급하고 4.13 수명 통합 담당이 수합한다. 독립 모델 입력/탐색기는 4.16.1.2, 완주·known-bad는 4.16.3.2, 실제 교차 수명은 4.12.3·4.13.3.2·4.14.4에 배정한다. 모델의 정상/오류 전이를 production 구현으로 정답 계산하지 않는다. 구체 산출물과 실제 배정 기록은 [상태 모델 표](dev4-work-breakdown.md#lifecycle-model)를 따른다.

fault 목록은 DEV2/DEV3 필수 cuts를 그대로 유지하고 SQL 경계의 prepare/bind·대상 materialization·engine statement 호출·sort run write/merge·fetch prefix·응답 관측 cuts를 추가한다. crash 대상은 소유 worker만이며 외부 controller의 ACK 이력을 보존한다. kill을 power-loss 증거로 확대하지 않는다. 새 fixture와 현재 build의 reopen/restore가 필수이며 사내 이전 버전 호환은 요구하지 않는다.

<a id="rollback-recovery"></a>
아래는 D4-T10/T11의 필수 교차 이력이며 모두 NOT_RUN이다. 실패한 statement와 미commit transaction을 구분한다. 모든 cut는 실제 경로 도달 witness를 요구하고 행·모든 index·catalog 및 native 독립 이력을 대조한다.

| 이력 | 장애 cut와 독립 기대값 |
| --- | --- |
| R1: S1 성공→S2 중간 key/quota 또는 최종 UNIQUE 실패→statement rollback 완료→COMMIT ACK | ACK 뒤 kill/reopen 및 recovery 도중 반복 장애: S1만 durable, S2 row/index는 전부 없음. 이미 돌려놓은 S2가 replay로 부활하는 mutant 거절 |
| R2: S1 성공→S2 실패→statement rollback 중 kill, COMMIT 미시도 | S1도 포함한 해당 거래 전체 없음. 다른 거래의 ACK는 유지. rollback 시작/중간/완료 직전과 recovery 재장애를 포함하고 S1을 durable로 오판하지 않음 |
| R3: DML COMMIT durable→응답 전달 전 kill | 외부 응답 UNKNOWN 허용, 거래 전체는 복구 후 보존. 재시작을 반복해도 중복 적용/누락 없음; SQL 자동 재실행 금지 |
| R4: DDL precommit→DDL 검증/build→DDL commit | 이전 commit의 시도/sync/응답, 그 뒤 DDL 호출 전, DDL 준비·commit·응답 cut 각각 실행. 두 transaction identity/outcome을 분리하며 durable 이전 DML은 DDL 실패/취소/UNKNOWN에도 보존 |

실행 담당 단위는 R1=4.17.2.1, R2=4.17.2.2, R3=4.17.3.1, R4=4.17.3.2이며 checkpoint/restore 교차는 4.17.3.3이다. R3의 좁은 DML 사례는 DDL/전체 result 완료를 기다리지 않는다. 초기 사례의 결과는 원래 source/범위로 보존하고 남은 문장/index/fault 변형을 별도 인수한다. 이 분리는 기존 T10/T11 필수 cuts를 삭제하거나 재시험 예외를 만드는 것이 아니다.

공급 대상 DEV3-DESIGN-003의 undo 적용 전후·redo-only CLR mini-transaction·다음 UndoRef/progress의 원자 기록·WAL sync·page flush·recovery 재장애 cuts를 하위 owner와 동결한다. R2와 T11의 낮은 자원 변형은 BEGIN WAL 삭제·부분 CLR 이후 재시작에서 engine의 BOOT_RESERVE/RESERVE_RESTORE·epoch 설치·잔여 undo 진행 증거를 받는다. 정상 admitted 이미지의 quota 경계 복구는 성공해야 하며 descriptor 손상/IO 실패의 Fenced 거절과 별도 판정한다. SQL은 CLR이나 별도 복구 ledger를 생성하지 않는다. 첫 update WAL 기록을 CommitAttempted로 오판하지 않으며 첫 commit record byte 시도를 기준으로 삼는다. rollback I/O 실패는 확정 NotCommitted·RecoveryRequired/Fenced로 관측하고, 복구 완료 뒤 R1–R4의 행/index 보장을 검사한다. DEV2 private/COW 회귀를 이 disk 경로의 실행 증거로 대체하지 않는다.

실행 진입점은 실제 harness 구현 때 정한다. 현재 `check_dev2_reference.py --with-substrate`는 DEV2 일부 경로이며 Q1 명령으로 표기하지 않는다. 실행 보고서는 source+dirty/binary/contract/oracle/fixture hash, command·OS/FS/toolchain, schedule/fault 도달, expected/actual·cleanup, 미해결 의무·별도 검토를 포함한다. Q1 완료까지 영향받는 실제 DEV2/DEV3 회귀를 유지한다.

<a id="cli"></a>
## 6. embedded CLI 인수 경계

계획 `xadb-cli`는 명시 DB 경로의 create/open을 구분하고 알려지지 않은 기존 파일을 지우지 않는다. 새 DB 생성은 별도 CLI 옵션이며 SQL의 CREATE TABLE과 다르다. interactive prompt는 autocommit·active transaction·incomplete result를 표시한다. batch는 파일/stdin을 streaming으로 읽고 첫 오류에 멈추는 fail-fast가 기본이다. 종료/EOF는 active 거래의 engine rollback을 요청하며 자동 COMMIT이 아니다. undo 실패는 RecoveryRequired/Fenced와 비정상 종료로 보고하고 정상 rollback으로 표시하지 않는다. 이미 성공한 autocommit 문장을 뒤의 오류 때문에 취소했다고 표시하지 않는다.

stdout은 결과·metadata·count, stderr는 위치·code/effect/transaction state를 포함한 진단이다. SQL 빈 text 입력은 NULL로 출력하고, native가 직접 저장한 non-NULL 빈 text 및 빈 Binary는 NULL과 구별한다. binary는 hex, text는 escaping 가능한 표시를 쓴다. 기계 출력은 typed 값과 ordinal을 가진 versioned JSON records로 제공하며 row prefix와 terminal completion/error를 분리한다. DDL은 오류에도 prior_transaction_outcome과 ddl_outcome을 출력하여 앞선 DML의 commit을 보존한다. 모든 문장이 정상 완료하면 exit 0, SQL/실행 실패면 1, CLI 인자/입력 열기 실패면 2다. SIGINT는 cancel 요청이며 이미 시도한 commit을 자동 재전송하지 않는다. 정확한 option/record schema·출력 vector는 F04에 동결한다.

기본 prepared 검증은 embedded typed API/XADBTest에서 수행한다. bind 값을 전달하려고 CLI에서 SQL 문자열 치환 기능을 만들지 않는다. network·JDBC/ODBC가 없어도 Q1을 검증할 수 있어야 한다.

<a id="performance"></a>
## 7. PF10: 비용을 설명할 실행 profile

| workload군 | 동일 fixture/결과에서 비교할 경로 | 관측 |
| --- | --- | --- |
| P10-A | point/range: forced scan ↔ 자동 index, native ↔ direct ↔ 반복 prepared | parse/bind/plan·node loops/rows·visited row/page·latency |
| P10-B | predicate selectivity 0/단일/부분/전체·NULL/중복/편향 | residual rows·불필요 방문·같은 결과/오류·table 단위 false conflict와 독립 table 처리량 |
| P10-C | ORDER BY 단일/복합·작은/큰 LIMIT, 정렬 입력이 sort memory의 최소4배 | run/merge pass·spill bytes·disk peak·time-to-first-row·전체 fetch 시간 |
| P10-D | multi-row DML·제약 실패·read-your-writes·commit·reopen | SQL overhead와 engine version-edit gate/row-key lock·DWB/commit sync·공개 비용 분리·ACK/rollback 동등 |
| P10-E | 계측 off/상시/상세, 결과 fetch chunk·조기 close·quota pressure | 정확성·유실 공개·계측 비용·RSS/FD/pin·취소 회수 |

각 군은 [4.18.2.1–5](dev4-work-breakdown.md#task-18)에 각각 배정한다. 4.18.1의 관측과 해당 F05를 기능 구현에 붙이고 비교 가능한 해당 경로에서 선택 실행한다. 4.18.3은 이 결과와 미완료 의무를 수합하며 새로운 전체 실행 명령이 아니다.

적어도 한 SQL scan/index workload는 설정한 전체 page buffer의 4배 이상 working set을 사용하고 실제 file I/O witness를 남긴다. 작은 sort budget만 넘긴 것으로 DISK working set 인수를 대신하지 않는다. DEV3 RC-T14의 row/index/locator/inventory 각각에 대한 인수는 하위 회귀로 유지한다. OS cache cold/warm은 구별하고 공유 host cache를 전역으로 지우지 않는다.

F05에서 cardinality·bytes·분포·seed·warmup/sample/반복·장치 동시 부하·memory/disk ceilings·주 지표·p99/쓰기량/복구/계측 비용의 수치 회귀 예산을 실행 전에 고정한다. 변경의 성능 이득을 주장할 때 DP07의 같은 의미 baseline/candidate 최소10 paired run과 불확실성 보고를 사용한다. 처음 구현한 parser에 존재하지 않는 이전 성능을 만들지 않으며 native와의 비교는 SQL 추가 비용이지 동일 API의 속도 개선 주장이 아니다. 잘못된 결과·sync 생략·요청 누락의 빠른 run은 INVALID다.

시도/성공 TPS, SerializationConflict·Limit·BufferBusy·Fenced 비율을 분리한다. 동일한 SQL·결과·격리·durability에서 batch1/8/64의 공급 profile을 식별하고 PF09의 flush 비용과 연결한다. 초기 규칙 기반 plan의 저선택도/편향 비용과 작은 LIMIT의 전체 sort 비용을 기록하며, 성능표를 위해 자동 commit 분할·격리 완화·sync 생략을 허용하지 않는다.

<a id="work-record"></a>
## 8. 설계 작업·독립 검토 기록

| 항목 | 기록 |
| --- | --- |
| 분류·담당 | DEV4-DESIGN-001 D2, 작업·계약·통합 `/root` |
| 착수 identity | HEAD `4a923214dd91c5566779652e59e13883eb940672`, dirty path/hash manifest SHA-256 `d1bcd74d2fe51bac51c3e32f2e228877d1a368686f6fb4ed83c42210de931403`, 108개 변경/삭제/미추적 파일. 로컬 비교 사본 `/tmp/xadb-dev4-design-base` |
| 독점 수정 | 새 SQL 설계·이 계획·DEV4 소스 조사. 제품 코드·Cargo·checker·기존 실행 evidence는 수정하지 않음 |
| 공유 수정 | devdoc README·contracts/plans/evidence README·documents.json, development-stages Markdown/JSON·stage-completion DEV4/SC02, SQL 초안 Markdown/JSON의 정본 안내·SQL 작성 목록 참조 |
| 중복 확인 | 기존 DEV2 이관·시험 폴더 재구성·DEV3 병렬 계획·push 정책 변경이 미커밋으로 존재. 최신 파일의 DEV4 절/참조만 순차 편집하고 기존 변경 보존 |
| 작업 중 기준 변경 | 외부 작업이 기존 변경을 `b4d1d8cdd86bfa7d502948708445878c712c5a79`로 commit한 것을 관측. 이 작업은 commit하지 않았고 현재 해당 HEAD 대비 DEV4 문서 15개만 변경됨. 이 commit의 제품 검증을 본 설계에서 새로 주장하지 않음 |
| 순서·생성물 | 소유 계약/소스 조사→구현/인수 계획→기존 JSON/색인 연결→독립 검토/수정. 문서 검사 생성물은 편집 중 갱신하지 않음 |
| 독립 검토 | `/root/dev4_design_review`: 새 문서 3개·기존 문서/JSON/색인 12개와 기존 DEV2/DEV3 소유 계약 대조. P2 2건 수정 후 최종 설계 검토 PASS·통합 가능, 필수 수정 지적 없음. 4 checkout revision/origin/tracked 상태·7개 source hash/열람 범위도 독립 확인. 계약/통합 담당 `/root` 반영 확인 |
| 검증 상태 | source revision·선택한 코드 범위 실제 열람. 문서 자동 검사는 DOC-002에 따라 push 때 수행; 현재 제품 시험·Q1·PF10·공식 배포 NOT_RUN |

설계의 독립 검토가 완료돼도 D4-F01–F05의 실제 입력과 DEV3 선행 증거는 남는다. 단계 상태·기존 실행 receipt를 PASS로 갱신하지 않는다.

1차 독립 검토자 `/root/dev4_design_review`가 두 P2 경계를 지적했다. 완료 result의 EOF/close 유효성을 무제한 보존할 수 있던 경계는 statement별 terminal descriptor 1개·고정 slot/generation·전체 quota로 제한했다. fetch 실패/close와 execute 초기 오류의 후속 상태는 ReadyBound/Closed/InvalidHandle·ON implicit 종료·OFF Active 유지·engine fencing 우선으로 명시했다. D4-V15/16과 D4-T08에 반복 수명·실패 후 거래 vector를 연결했다. 수정 후 최종 재검토에서 두 지적 해소를 확인했다. 추가 문구 권고에 따라 ON implicit 거래의 종료는 terminal 오류로 한정하고 API 진입 거절·RowBufferTooSmall의 상태 보존을 명시했다.

이 검토는 설계 일관성·조사 identity의 범위다. 검토자는 파일 수정·문서 자동 검사·제품 시험을 수행하지 않았으며 기존 외부 commit의 제품 변경을 승인하지 않았다. parser 재귀 전체 한도와 bounded spill record/merge 예약도 검토했지만 실제 자원 사용·복구·성능 PASS는 실행 전까지 주장하지 않는다.

<a id="oracle-followup"></a>
## 9. Oracle 관련 채택 범위와 후속 의무

[DEV4-REVIEW-002](../contracts/dev4-sql-design.md#oracle-review)는 이름·SQL 빈 text·UNIQUE·DDL 조립을 구체화한다. Oracle 19c에서 확인한 일부 의미를 XADB의 선택으로 고정한 것이며 Oracle 실행 비교나 전체 호환 인증은 아니다. 기존 SQL 초안의 소문자·빈 text 보존·DDL 거절 예제는 역사적 제안으로 보존하고 현행 Q1 기대값으로 사용하지 않는다.

| 경계 | 현재 DEV4 인수 | 후속 의무·착수 조건 |
| --- | --- | --- |
| 격리 | 거래 snapshot+table read/write-set 검증, OFF/BEGIN의 무변경 ReadWrite도 validation. 같은 table false conflict와 독립 table 동시 성공을 측정 | DEV3-DESIGN-003의 native ReadCommitted·statement token·행/키 잠금 공급을 실제 검증한 뒤 SQL isolation 문법과 default를 결정. 관측 버전 변경 시 전체 abort하는 보수적 의미와 Oracle의 차이를 유지 |
| 저장·복구 | 채택된 DEV3-DESIGN-003의 mutable page·WAL·영속 undo·CLR과 statement rollback·ACK·UNKNOWN/RecoveryRequired, R1–R4 | WAL-before-page·pageLSN/redo·undo/CLR·doublewrite·checkpoint·retention·반복 장애의 실제 선행 검증 후 D4 공급 경계 동결 |
| 값·문법 | SQL empty Utf8 입력만 typed NULL; native 빈 값 보존. 현재 VARCHAR/BIGINT/VARBINARY·LIMIT·`?`와 고정 PUBLIC namespace | native 모든 입력의 Oracle 값 의미 강제, VARCHAR2/NUMBER/DATE·이름 bind·사용자 schema·확장 SQL은 해당 기능 설계에서 별도 구체화. NUMBER를 Int64 alias로 선언하지 않음 |

SQL READ COMMITTED 노출과 DEV3 저장 구현은 이 보완의 제품 구현이나 Q1 완료 주장이 아니다. 담당은 해당 SQL/transaction/storage 계약 담당이며 실제 착수 때 DP01 기록과 독립 oracle·동시성/fault profile을 동결한다. DEV3 공급 안을 소비하면서 하위 계약을 과거 COW·전역 q에 맞춰 되돌리지 않는다.

<a id="oracle-work-record"></a>
## 10. Oracle·ARIES 리뷰 보완 작업 기록

| 항목 | 기록 |
| --- | --- |
| 분류·담당 | DEV4-REVIEW-002 D2, 작업·SQL/커널 경계 계약·통합 `/root` |
| 권한 | 사용자의 Oracle·ARIES 기준 DEV4 리뷰 요청과 후속 “수정해 주세요”. 기존 D2 위임 안의 설계 구체화 |
| 착수 identity | HEAD `2a0c3ec3aee1bda39af4d394eba2bb3ae3c70df1`, 기존 dirty 31개. 로컬 원문 사본 `/tmp/xadb-dev4-oracle-base-rz3589u2`, identity.json SHA-256 `c75d7e9c9e6d5abbca51342601156317e8ef03690e3848701b0a950e84d7d94f` |
| 독점 수정 | DEV4 설계·이 계획·DEV4 조사 기록. 원래 조사/독립 검토는 당시 범위로 남기고 새 결정 연결 |
| 공유 수정 | implementation-contracts의 native/SQL DDL 경계, development-stages Markdown/JSON·stage-completion DEV4/SC02, SQL 초안 Markdown/JSON의 후속 처분, devdoc/contracts/plans/evidence README·documents.json |
| 중복·순서 | 기존 DEV2 성능 구현·DEV3 보완·저장소 배치의 미commit 변경을 확인. 해당 제품/DEV3 파일은 쓰지 않고 최신 정본을 읽어 소비. 계약→plan/evidence→색인/JSON→독립 검토 순차 반영 |
| 공급 계약 변경 관측 | 작업 중 DEV3-DESIGN-003 READY_FOR_REVIEW와 implementation-contracts의 DEV3 profile 추가를 관측. 최종 검토 시 별도 DEV3 검토 뒤 ACCEPTED 전환 확인. DEV4의 과거 COW/전역 q/한 거래 frame 전제를 statement token·table validation·WAL/영속 undo·RecoveryRequired로 정합화. DEV3 원문과 공유 파일의 다른 작업 변경은 보존 |
| 검증 | 의미·반례·현재 소유 계약과 diff의 읽기 검토. DOC-002에 따라 문서 자동 검사는 push 전용으로 미실행. 제품 코드 변경·Oracle 실행 비교·Q1·성능/복구 시험 NOT_RUN |
| 독립 검토 | `/root/dev4_oracle_review`: 기준 사본 대비 14개 문서/JSON의 DEV4 변경과 최신 DEV3-DESIGN-003·커널 경계 대조. 최종 D2 채택 차단 P1/P2 없음. Oracle 공식 문서·ARIES 원문 일부와 CUBRID revision/hash/보상 로그도 독립 확인. 계약/통합 담당 `/root` 반영 확인, DEV4-REVIEW-002 ACCEPTED |

별도 검토의 DDL 진입 거절 시 Active 유지, D4-V18의 빈 catalog fixture, DDL type_length 변환·범위 검사 시점을 반영했다. 새 DEV3 경계에서는 두 DDL outcome의 NotCommittedRecoveryRequired, ROLLBACK/close_session/Drop의 undo·lock 소유, scan/mutation의 공통 statement token, 논리 payload와 WAL 예산 구분, 최신 committed+own final delta UNIQUE를 보완했다. 미commit rollback 도중 crash의 기대값은 S1도 포함한 거래 전체 소멸이며, rollback 완료 후 COMMIT ACK를 받은 R1과 구분한다.

최종 별도 검토는 이 DEV4 설계 변경의 일관성·조사 identity에 한정한다. 검토자는 파일 수정·문서 자동 검사·제품 시험을 하지 않았고, 동시 DEV3 작업 자체나 제품 실행을 이 검토로 승인하지 않았다. 설계 채택을 Q1·성능·복구 PASS로 승격하지 않는다.

<a id="architecture-review"></a>
## 11. REVIEW-003 보완

[이번 비교·보완 기록](../evidence/dev3-dev4-architecture-review.md)은 DEV3의 상태/예약/배치 공급과 DEV4의 초기 처리 한계·all-null lookup을 연결한 D2 작업이다. 기존 REVIEW-002의 검토 identity와 실행 상태는 유지한다. 새 D4-V25·교차 인수도 NOT_RUN이며, 문서 보완과 독립 검토를 Q1 실행 증거로 사용하지 않는다.

<a id="conflict-reserve-review"></a>
## 12. REVIEW-004 보완

D4-V26/T09는 대상 관측 뒤 타 거래가 잠금을 이미 해제한 경우도 execute 단계에서 검사한다. T10/T11·R2는 DEV3의 영속 예약 인계와 부분 CLR 후 복구 증거를 수신한다. 담당·기준 사본·공유 수정·독립 검토는 [공통 작업 기록](dev3-implementation-plan.md#conflict-reserve-review)에 둔다. 이 설계 보완으로 SQL ReadCommitted를 노출하거나 실제 DEV3/Q1 실행 상태를 승격하지 않는다.
