# DEV4 — embedded SQL 구현·검증 계획

2026-10-18 · **DEV4-DESIGN-001 D2 기본 설계 ACCEPTED·독립 검토 완료 / 구현 착수 전 입력 일부 미동결 / 제품 실행 NOT_RUN**

[SQL 소유 계약](../contracts/dev4-sql-design.md) · [소스 조사](../evidence/dev4-source-research.md) · [단계 완료](stage-completion.json) · [저장 선행](dev3-implementation-plan.md) · [개발 운영](../policies/development-policy.md)

<a id="entry"></a>
## 1. 착수와 완료를 구분한다

DEV4의 기존 task는 XADB-011, gate는 Q1이며 DEV3에 의존한다. DEV2 P0/K0 volatile 실행이나 이관 사실, DEV3 설계 검토를 DEV4 production 착수 증거로 사용하지 않는다. 설계·독립 기대값 준비는 먼저 가능하다. 현재 SQL crate·CLI·실행 harness는 없고 run entrypoint/evidence는 null/빈 목록을 유지한다.

선행 인수에는 DEV3의 실제 disk row/catalog/index·bounded scan/point/range, statement mark와 read-your-writes, 즉시 unique·key overflow, 단일 commit의 data/WAL sync·UNKNOWN fencing·reopen, checkpoint/restore·retention과 해당 최소 완료 증거가 필요하다. 기존 stage DAG와 XADB-011의 task 의존을 변경하지 않는다. COMM/PART/NG·고급 SQL을 추가 선행으로 만들지 않는다.

| 공급 경계 | SQL이 받을 계약·실제 인수 |
| --- | --- |
| catalog/DDL | 고정 snapshot lookup, 열 type/nullable/byte length, table/index identity, schema epoch/index 정의 대조. PK/UNIQUE 포함 create와 drop의 단일 원자 DDL |
| row/index | typed scan/point/range+private delta, cursor 수명/EOF/RowId 순서, owned row, 즉시 constraint 오류 |
| transaction | 대상 확정 뒤 단일 execute_statement, mark rollback·남은 mutation/byte 예산, ReadOnly/ReadWrite·coarse validation |
| durable/lifecycle | commit outcome/certainty/fencing, 취소 경계, engine close/open, owner temp namespace·quota·cleanup 효과 port |
| verification | native literal row/history oracle·실제 ACK 수집·fault hooks·독립 lower regression. imported receipt를 현재 fingerprint에 대입하지 않음 |

공급 API가 설계와 다르면 해당 owner에게 필요한 최소 경계를 D2로 통합한 후 SQL에서 소비한다. private storage 접근, SQL에서 UNIQUE 재구현, 여러 commit으로 DDL/문장을 쪼개는 우회는 허용하지 않는다.

<a id="sequence"></a>
## 2. 구현 순서와 소유

아래 D4 단계는 XADB-011 내부 leaf이며 새 제품 stage나 완료 gate가 아니다. 실제 배정·병렬 실행은 아직 없다. 파일은 기존 계획 패키지 안에서 구현할 때만 추가하고 Cargo/지문/clean export를 함께 등록한다. 문법·의미 정의와 independent oracle의 생산 구현은 분리한다.

| 순서 | 책임·산출물 | 선행·완료 범위 |
| --- | --- | --- |
| D4-0 | 공급 경계 대조·F01–F05, 독립 literal/vector inventory, oracle 부정 사례 | 설계 준비 가능. 모든 공통 API와 byte/오류 입력 동결 뒤 관련 production 착수 |
| D4-1 | lexer/parser와 bounded AST·span·CLI 문장 경계 | F01. 정상·경계·잘린 입력·unsupported·fuzz. parser 단독 PASS는 Q1 아님 |
| D4-2 | binder·catalog dependency·타입/NULL·metadata·DDL lowering | D4-1 및 실제 DEV3 catalog/DDL. identity/schema race·PK 강화·원자 DDL |
| D4-3 | RowScan/filter/project/limit + DML 대상 확정·원자 statement | D4-2, 실제 native statement. read-your-writes·순서·실패·cancel·serializable |
| D4-4 | index point/range 선택, external sort·bounded spill/cleanup | D4-3, 실제 DEV3 index와 F03. force-scan 및 literal 기대값 대조·정렬·큰 working set |
| D4-5 | session/prepared/bind/result 상태, transaction/autocommit·stale 오류 | D4-2–4, F02. execute_direct와 typed prepared를 같은 실행 경로로 연결 |
| D4-6 | embedded CLI·XADBTest SQL adapter·native/SQL history 연결·PF10 | D4-5, F04/05. 실제 파일·thread·kill/reopen, 독립 결과·자원·하위 회귀 |
| D4-7 | Q1 전체 inventory 수합·별도 검토·현재 build closure | 모든 필수 variant·SC02·KU13·PF10·영향 lower regression PASS. official distribution은 별도 |

독립 lexer/vector 준비는 저장 구현과 겹칠 수 있으나 production 선행 gate를 생략하지 않는다. D4-2와 D4-5의 공통 BoundStatement/handle/error 계약을 먼저 고정하고, D4-3/4의 executor 조립은 한 소유자가 통합한다. 별도 구현 agent를 실제 배정하려면 파일 소유·공유 변경·기준 revision을 그때 기록한다. 현재 병렬 성능 향상을 주장하지 않는다.

<a id="freeze"></a>
## 3. leaf 구현 전 남은 동결 항목

| ID | 이 설계가 정한 것 | 구현 전에 고정할 실제 입력 |
| --- | --- | --- |
| D4-F01 | 문법·이름·타입·NULL·제약·span·수치 한도 | exact token/AST/Rust signature, 모든 production error→SQL code/effect vector, malformed/unsupported token corpus와 hash |
| D4-F02 | `?`·prepare 추론·owned bind·metadata·상태표·stale reject | engine catalog dependency accessor, session/statement/result token 표현, 모든 전이×오류×취소의 literal vector·실제 thread barrier |
| D4-F03 | bounded external sort와 SQL memory/disk/FD 예산 | 임시 file/record/manifest byte layout·독립 hex, owner cleanup port·disk reservation, merge/ENOSPC/short I/O fault cuts와 allocation inventory |
| D4-F04 | Q1 필수 사례와 native/literal independence | SQL worker adapter/protocol version·expected row/error/event schema, mutation·negative verdict inventory, seed/schedule 목록·실행 명령 |
| D4-F05 | PF10 workload군·기본 V2/V3·계측 책임 | 실제 host/CPU/FS·fixture cardinality/distribution/hash·release binary/feature, warmup/sample·RSS/I/O·계측 비용·회귀 threshold·paired 순서와 재현 recipe |

담당은 해당 leaf 구현자와 SQL/engine 계약 담당, 통합 담당이며 실제 이름을 착수 기록에 배정한다. 빈 담당·null entrypoint를 동결 완료로 바꾸지 않는다. 각 동결은 DP01 별도 검토를 받고 구현 후보를 실행하기 전에 hash를 기록한다. 실패를 없애기 위해 사후 예산/variant를 줄이지 않는다.

<a id="acceptance"></a>
## 4. Q1 필수 인수 행렬

이 표는 기존 SC02·KU13·PF10의 세부 의무다. stage-completion.json의 기존 required_test_refs를 교체하거나 줄이지 않는다. 각 행의 필수 변형을 leaf에서 case ID로 펼치고 누락·미도달·timeout은 PASS가 아니다. 현재 전부 NOT_RUN이다.

| ID / 연결 | 필수 변형 | 독립 판정·필수 관측 |
| --- | --- | --- |
| D4-T01 / SC02 | 모든 문법 production·keyword·식별자 quote/fold·주석·UTF-8·EOF/span, bytes/tokens/nesting 경계±1 | literal AST/오류 위치, multi-statement API 거절, batch 경계가 literal의 `;`를 나누지 않음 |
| D4-T02 / SC02 | Int64 min/max±1·Bool·빈 text/binary·NUL·UTF-8 길이·NULL, 모든 비교×타입, T/F/U truth table | production 비교/codec을 쓰지 않는 literal/단순 evaluator. 암묵 cast·NULL=TRUE mutant 거절 |
| D4-T03 / SC02,KU13 | 네 DDL·PK/UNIQUE/NOT NULL, composite 1/4/5·index 8/9·key1024/1025, duplicate·drop/recreate·PK backing index 보호 | catalog+row/index 원자 상태, 준비 실패·drain timeout·복구에서 old/new 중 하나. DDL-in-txn이 기존 DML을 commit하지 않음 |
| D4-T04 / SC02,KU13 | INSERT multi-row·UPDATE/DELETE empty/all/some·동일 값 update·missing column·statement 중간 unique/key/quota 오류 | D4-V05/06, affected count·이전 statement 유지·모든 index rollback·대상 한 번만 변경·256/257 mutation 경계 |
| D4-T05 / SC02,KU13 | scan/point/range·복합 prefix·inclusive/exclusive·NULL bounds·AND residual·OR/NOT fallback·private insert/update/delete | literal rows + 독립 native history + force-scan의 삼중 대조. 누락/중복/잘못된 NULL bound mutant 거절 |
| D4-T06 / SC02 | ORDER BY ASC/DESC×NULLS 기본/명시×복수 열·미선택 열·동률·LIMIT 0/1/Int64 max | 별도 sort 기대값. 무순서 결과는 bag multiplicity, tie는 허용 순서 집합, unique tie-breaker 사례는 exact order |
| D4-T07 / SC02 | 모든 지원 DML/SELECT direct/prepared, marker 위치·hint/추론·count/type/NULL/bytes·caller buffer 해제 | immutable metadata·typed 값 독립성, bind 실패 원자 보존, 값의 SQL injection 재해석 없음 |
| D4-T08 / SC02 | 모든 handle 상태 전이·EOF/조기 close·double close·다른 session/engine·slot reuse·stale plan, 재bind/재execute·prepare/DDL race·D4-V15/16 | 첫 효과 전 거절, old result/token 재사용 없음, terminal descriptor도 bounded·fetch 실패 후 거래/statement 상태·memory/pin/FD/quota 회복 |
| D4-T09 / SC02,KU13 | ON/OFF×BEGIN/COMMIT/ROLLBACK/DDL×성공/오류, read-your-writes·두 table mixed transaction·coarse writer conflict·read-only | 실제 2-thread schedule·external history·일관된 snapshot·두 writer 성공 mutant 거절 |
| D4-T10 / SC02,KU13 | SQL commit의 data/WAL append/sync/publication/reply cut, short write·sync error·kill·reopen·반복 recovery·checkpoint/backup/restore | 실제 ACK·ABORTED·UNKNOWN history와 native oracle. current-build ACK 보존·partial batch 금지·durable 이후 취소 오판 금지 |
| D4-T11 / SC02 | parse/bind/execute/fetch/sort·commit 전후 cancel/timeout, allocation/quota·spill ENOSPC·truncated file·cleanup 실패·crash | statement/txn 효과, incomplete prefix, engine fencing, orphan quota 유지·소유 namespace만 회수 |
| D4-T12 / SC02 | interactive/batch CLI·stdin/file·Unicode/NULL/binary 출력·metadata/count/error·중간 실패·EOF | typed embedded 결과와 독립 출력 fixture, exit status·stdout/stderr·implicit commit 없음 |
| D4-T13 / KU13 | literal SQL/native 동등 workload·row/index/catalog recovery·알려진 잘못된 결과/ACK/order/error·잘못된 binary/recipe | 같은 production 경로 둘의 일치만으로 PASS 금지. oracle/runner는 부정 사례를 거절해야 함 |
| D4-T14 / PF10 | 아래 workload/계측 모드·큰 working set·sort spill·native 대비 SQL 비용 | plan→실제 node→storage 경로·동등 결과·자원 상한·비교 유효성·bundle 재현 |

각 지원 statement×타입에는 정상/NULL(허용/거절)/빈 값/경계/오류가 필요하다. direct와 prepared는 지원 교집합 전부를 비교하고 DDL/transaction control은 prepare에서 명시 Unsupported를 확인한다. 각 index kind(PK/unique/nonunique)×단일/복합 key의 적용 가능한 point/range/DML·crash 변형을 포함한다. 전체 문법·타입 항목이 어느 test ID에 연결됐는지 coverage inventory를 만든다. 기능이 없는 조합은 근거 있는 N/A이며 후속 필수 기능을 삭제하지 않는다.

<a id="verification"></a>
## 5. 독립성·fault·검증 강도

SQL oracle는 작은 typed row bag·3값 evaluator·명시 순서/허용 tie 집합·serial history로 기대값을 만든다. production parser/binder/optimizer/codec/row replay를 기대값 계산에 재사용하지 않는다. native/SQL 비교는 adapter 차이 검출을 보완하며 단독 oracle가 아니다. 외부 DBMS는 optional differential 연구 대상이고 XADB 의미의 정답은 아니다.

SC02의 실행/오류 행렬에 [설계 vector](../contracts/dev4-sql-design.md#vectors)를 연결한다. core 경계의 malformed 입력과 독립 byte vector, stale generation/parameter lifetime·누락 cleanup 반례를 먼저 만든다. shared protocol은 입력/관측 타입만 공유하고 semantic oracle는 testing/reference 책임에서 독립 구현한다. SQL adapter의 실제 package/dependency는 구현 때 저장소 정책과 함께 D2로 등록하며 빈 worker를 만들지 않는다.

V1은 신규 vector/원래 반례와 인접 경계, V2는 관련 전체 property suite의 고정 64 seed×최대128 operation·보존 fuzz corpus·실제 disk 통합·하위 회귀를 요구한다. V3는 256 seed×최대1024 operation(앞선 seed 포함), 관련 parser/codec target당 최소30분 fuzz, 관련 실제 동시성 suite 최소30분, 비결정적 필수 fault 셀 3회 fresh fixture 반복을 요구한다. DP03보다 약화하지 않는다. 입력 경계는 seed가 우연히 생성하기를 기다리지 않고 고정 사례로 둔다.

작은 handle/session 상태 모델은 2 session, 각 prepared 2개, 활성 result 1개/session, operation 길이 6 이내의 유한 공간을 F02의 명시 action/parameter 동치류로 전수 탐색한다. 탐색 상태/전이 최대 수·완주 receipt는 구현 전에 계산·고정하고 상한 도달은 INCONCLUSIVE다. 이 모델과 실제 thread·I/O 검증은 서로 대체하지 않는다.

fault 목록은 DEV2/DEV3 필수 cuts를 그대로 유지하고 SQL 경계의 prepare/bind·대상 materialization·engine statement 호출·sort run write/merge·fetch prefix·응답 관측 cuts를 추가한다. crash 대상은 소유 worker만이며 외부 controller의 ACK 이력을 보존한다. kill을 power-loss 증거로 확대하지 않는다. 새 fixture와 현재 build의 reopen/restore가 필수이며 사내 이전 버전 호환은 요구하지 않는다.

실행 진입점은 실제 harness 구현 때 정한다. 현재 `check_dev2_reference.py --with-substrate`는 DEV2 일부 경로이며 Q1 명령으로 표기하지 않는다. 실행 보고서는 source+dirty/binary/contract/oracle/fixture hash, command·OS/FS/toolchain, schedule/fault 도달, expected/actual·cleanup, 미해결 의무·별도 검토를 포함한다. Q1 완료까지 영향받는 실제 DEV2/DEV3 회귀를 유지한다.

<a id="cli"></a>
## 6. embedded CLI 인수 경계

계획 `xadb-cli`는 명시 DB 경로의 create/open을 구분하고 알려지지 않은 기존 파일을 지우지 않는다. 새 DB 생성은 별도 CLI 옵션이며 SQL의 CREATE TABLE과 다르다. interactive prompt는 autocommit·active transaction·incomplete result를 표시한다. batch는 파일/stdin을 streaming으로 읽고 첫 오류에 멈추는 fail-fast가 기본이다. 종료/EOF는 active 거래 rollback이며 자동 COMMIT이 아니다. 이미 성공한 autocommit 문장을 뒤의 오류 때문에 취소했다고 표시하지 않는다.

stdout은 결과·metadata·count, stderr는 위치·code/effect/transaction state를 포함한 진단이다. NULL·빈 문자열·빈 binary를 구별하고 binary는 hex, text는 escaping 가능한 표시를 쓴다. 기계 출력은 typed 값과 ordinal을 가진 versioned JSON records로 제공하며 row prefix와 terminal completion/error를 분리한다. 모든 문장이 정상 완료하면 exit 0, SQL/실행 실패면 1, CLI 인자/입력 열기 실패면 2다. SIGINT는 cancel 요청이며 이미 시도한 commit을 자동 재전송하지 않는다. 정확한 option/record schema·출력 vector는 F04에 동결한다.

기본 prepared 검증은 embedded typed API/XADBTest에서 수행한다. bind 값을 전달하려고 CLI에서 SQL 문자열 치환 기능을 만들지 않는다. network·JDBC/ODBC가 없어도 Q1을 검증할 수 있어야 한다.

<a id="performance"></a>
## 7. PF10: 비용을 설명할 실행 profile

| workload군 | 동일 fixture/결과에서 비교할 경로 | 관측 |
| --- | --- | --- |
| P10-A | point/range: forced scan ↔ 자동 index, native ↔ direct ↔ 반복 prepared | parse/bind/plan·node loops/rows·visited row/page·latency |
| P10-B | predicate selectivity 0/단일/부분/전체·NULL/중복/편향 | residual rows·불필요 방문·같은 결과/오류·coarse conflict |
| P10-C | ORDER BY 단일/복합·작은/큰 LIMIT, 정렬 입력이 sort memory의 최소4배 | run/merge pass·spill bytes·disk peak·time-to-first-row·전체 fetch 시간 |
| P10-D | multi-row DML·제약 실패·read-your-writes·commit·reopen | SQL overhead와 engine sync/공개 비용 분리·ACK/rollback 동등 |
| P10-E | 계측 off/상시/상세, 결과 fetch chunk·조기 close·quota pressure | 정확성·유실 공개·계측 비용·RSS/FD/pin·취소 회수 |

적어도 한 SQL scan/index workload는 설정한 전체 page buffer의 4배 이상 working set을 사용하고 실제 file I/O witness를 남긴다. 작은 sort budget만 넘긴 것으로 DISK working set 인수를 대신하지 않는다. DEV3 RC-T14의 row/index/locator/inventory 각각에 대한 인수는 하위 회귀로 유지한다. OS cache cold/warm은 구별하고 공유 host cache를 전역으로 지우지 않는다.

F05에서 cardinality·bytes·분포·seed·warmup/sample/반복·장치 동시 부하·memory/disk ceilings·주 지표·p99/쓰기량/복구/계측 비용의 수치 회귀 예산을 실행 전에 고정한다. 변경의 성능 이득을 주장할 때 DP07의 같은 의미 baseline/candidate 최소10 paired run과 불확실성 보고를 사용한다. 처음 구현한 parser에 존재하지 않는 이전 성능을 만들지 않으며 native와의 비교는 SQL 추가 비용이지 동일 API의 속도 개선 주장이 아니다. 잘못된 결과·sync 생략·요청 누락의 빠른 run은 INVALID다.

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
