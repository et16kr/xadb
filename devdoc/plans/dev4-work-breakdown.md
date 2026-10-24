# DEV4 — 세부 작업 분류와 공급·인수 계획

2026-10-22 · **18개 분류·기존 55개 작업 ID 유지 / D4-WORK-BREAKDOWN-001–003 ACCEPTED / 독립 앞단 제한 구현·Q1 미완료**

[SQL 의미 정본](../contracts/dev4-sql-design.md) · [기존 구현·검증 계획](dev4-implementation-plan.md) · [이번 ARIES·Oracle·로컬 소스 조사](../evidence/dev4-source-research.md#work-breakdown-20261022) · [단계 완료 정본](stage-completion.json) · [DEV3 분해 방식](dev3-implementation-plan.md#remaining-work)

이 문서는 기존 DEV4의 D4-0–7을 실제 배정 가능한 하위 작업으로 나눈다. 모든 작업은 기존 XADB-011 내부이며 DEV3 → DEV4, Q1, SC02·KU13·PF10과 채택된 SQL 의미를 유지한다. 번호는 실행 순서·진행률·새 gate가 아니다. **분류 채택 당시 4.01–4.18의 제품 구현·실행은 모두 NOT_RUN이었다.** 이후 [DEV3 병행 준비 계획](dev4-preparation-plan.md)에 따라 독립 lexer·이름·splitter·P2 INSERT/거래 parser·P3 scalar 입력을 제한 구현했다. 실제 범위와 원 source별 증거는 [현재 구현](../implementation/dev4-implementation.md)을 따른다. 분류 완료나 앞단 일부의 성공은 전체 DEV4·Q1 완료를 뜻하지 않는다.

DEV3처럼 큰 책임을 분류하고 각 책임을 좁은 공급 단위와 소비 경로의 통합 인수로 나눈다. 의미·API의 소유자는 기존 SQL/engine 계약이고, 이 문서는 작업 범위·직접 선행·산출물·인수 연결을 소유한다. 실제 Rust signature·spill bytes·runner·수치 profile은 D4-F01–F05에서 아직 동결해야 한다. 이번 요청으로 제품 구현·시험 실행을 재개하지 않는다.

후속 [분배 검토 002](#work-review-002)는 기존 55개 ID를 보존하면서 큰 작업 아래에 `4.xx.y.z` 실행 단위를 추가한다. 하위 단위를 펼친 부모 ID는 범위·인수 수합용이며 부모와 자식을 별도 완료량으로 중복 계산하지 않는다. 55는 공수·동시 배정 수가 아니다. [초기 영속 경로](#sequence), [상태 통합·독립 모델](#lifecycle-model), [정렬 재사용 판단](#sort-reuse)을 실제 배정에 적용한다.

[재점검 003](#work-review-003)은 같은 18개 책임을 유지하고, 앞단의 실제 선행·공통 오류 변환·시험 도구 공급을 더 좁힌다. [분량 비교](#workload-review)는 구현·검증 준비의 상대적 부담과 미확정 입력을 구분하며 실측 일정으로 사용하지 않는다.

<a id="catalogue"></a>
## 1. 전체 18개 분류

아래 T/V/F는 각각 기존 계획의 D4-T01–T14, SQL 계약의 D4-V01–V28, D4-F01–F05다. 인수 연결은 담당 범위이며 축약 목록만 통과해 완료할 수 없다. 시험의 필수 변형은 [원래 Q1 행렬](dev4-implementation-plan.md#acceptance)이 우선한다.

| ID | 분류·소유 역할 | 주요 산출물 | 기존 순서·인수 |
| --- | --- | --- | --- |
| [4.01](#task-01) | DEV3 공급 수신·공통 입력 동결 / SQL·engine 계약 | 공급 증거표, F01–F05의 leaf별 동결 목록 | D4-0·전 단계 / F01–05, T13 |
| [4.02](#task-02) | 어휘·이름·문장 경계 / front end | token/span, 인용·주석·batch 분리 | D4-1 / T01, V18 |
| [4.03](#task-03) | 문법·AST·진단 / front end | bounded AST, 문장 종류 dispatch, Syntax/Unsupported | D4-1 / T01, V13 |
| [4.04](#task-04) | 값·타입·NULL·predicate / SQL 의미 | typed literal, 입력 정규화, 3값 평가 | D4-2 / T02, V01–04/19/20 |
| [4.05](#task-05) | catalog binding·metadata / binder | 객체/열 정체성, BoundStatement, 불변 metadata | D4-2 / T02/03/07/08, V07/08/10/18 |
| [4.06](#task-06) | 계획·의존성·access path / planner | RowScan 기준선, point/range 선택, stale 검사 | D4-2/4 / T05/08, V10/25 |
| [4.07](#task-07) | SELECT pull 실행 / executor | scan/filter/project/limit, snapshot·own-write 연결 | D4-3/4 / T05/06/08, V01–03/13/14/20/25 |
| [4.08](#task-08) | ORDER BY·외부 정렬·spill / sorter | run/merge, 임시 codec·manifest·cleanup | D4-4 / F03, T06/11/14, V03/14 |
| [4.09](#task-09) | INSERT/UPDATE/DELETE·문장 원자성 / DML | 대상 선확정, observation 전달, native 단일 statement | D4-3 / T04/09/11, V05/06/17/26/27 |
| [4.10](#task-10) | DDL lowering·precommit / DDL 조립 | 네 DDL, 제약 정의, prior/ddl 두 outcome | D4-2/5 / T03/09/10/11, V17/21/22/24 |
| [4.11](#task-11) | prepared·parameter·owned bind / prepared | 타입 추론/hint, bind_all 원자 교체, direct 공통 경로 | D4-2/5 / T07/08, V07–09/12/19 |
| [4.12](#task-12) | result·handle·fetch 수명 / result | generation, bounded batch, EOF/error/close 상태 | D4-5 / T08/11, V09/14–16/28 |
| [4.13](#task-13) | session·transaction·commit 오류 / session | autocommit/BEGIN, statement scope, outcome·fencing | D4-3/5 / T09/10/11, V11/16/21–24/26/27 |
| [4.14](#task-14) | 취소·deadline·합산 자원·종료 / SQL 자원 | 호출별 context, memory/disk/FD 회계, undo 소유 인계 | D4-0–6 공통 / T07/08/11/14, V14–16/22/27/28 |
| [4.15](#task-15) | embedded facade·CLI / CLI 조립 | create/open, interactive/batch, typed 출력·exit | D4-6 / T12, V18–24 |
| [4.16](#task-16) | 독립 oracle·SQL adapter·coverage / XADBTest | literal 기대값, native/SQL 이력, known-bad 판정 | D4-0/6 / F04, T01–13, SC02·KU13 |
| [4.17](#task-17) | disk 복구·rollback 교차 인수 / SQL·engine 통합 | R1–R4, ACK/reopen/recrash, checkpoint·restore 연결 | D4-6 / T10/11/13, V23/24/27, KU13 |
| [4.18](#task-18) | 관측·PF10·Q1 증거 수합 / 통합·성능 | node/storage 계측, P10-A–E, 누락/실패 closure | D4-0/6/7 / F05, T14, SC02·KU13·PF10 |

4.01·4.14·4.16·4.18은 마지막에 붙이는 부가 작업이 아니다. 소비 leaf 전에 필요한 경계·예약·기대값·관측을 공급하고, 전체 기능을 연결한 인수는 뒤에서 수합한다. 기존 [DEV3 작업](dev3-implementation-plan.md#remaining-work)의 완료를 이 표에서 대신 선언하지 않는다.

<a id="entry-exit"></a>
## 2. 공통 착수·완료 기록

| 시점 | 남길 내용 |
| --- | --- |
| 착수 | 하위 ID, 실제 담당·별도 검토자·통합 담당, 기준 revision/dirty 범위, 독점/공유 경로, 소비하는 DEV3 API·증거와 미충족 공급 |
| 분량·배정 | 필요한 API·미확정 결정, production/fixture/oracle/harness 각각의 변경 범위, 독립 종료 가능한 인수, 난도·불확실성·예상 공수 범위와 근거. 측정 전 기간은 추정으로 표시하고 부모/자식·공유 harness 비용은 중복 합산하지 않음 |
| 입력 동결 | 관련 F 항목, 타입/오류/소유·byte vector, 독립 기대값, 자원 계산, 실제 case ID·fault cut·thread barrier·명령·fixture hash |
| 구현 동반 | production 변경과 그 변경에 해당하는 테스트·fixture·oracle·harness. 독립 oracle에 production 의미 계산을 복사하지 않음 |
| 제한 공급 | 실제 구현한 경로와 선택 시험 결과, source/binary/fixture identity, 미연결 소비자·미실행 변형. parser PASS를 SQL 실행 PASS로 표시하지 않음 |
| 통합 완료 | 새로 연결·수정한 경로의 원자성·오류·수명·자원 인수와 별도 검토, 남은 의무·실패 원본. 다른 분류 전체 완료를 암묵적으로 승격하지 않음 |

설계·literal 기대값 준비는 DEV3와 독립적으로 할 수 있다. [D4-PREP-001](../contracts/dev4-independent-frontend.md#scope)의 4.02/4.03/4.04와 필요한 공통 앞단 부분·독립 검증 도구는 해당 leaf 동결 뒤 DEV3와 병행 구현할 수 있다. [준비 계획](dev4-preparation-plan.md)의 허용 범위 밖 engine 소비 구현은 [DEV4 진입 조건](dev4-implementation-plan.md#entry)의 실제 DEV3 최소 공급 증거를 요구한다. 공급 API가 부족하면 engine owner에게 최소 계약 보완을 요청하고 SQL의 private page/WAL 접근·UNIQUE 재구현·분할 commit으로 우회하지 않는다. 분류마다 새 crate를 만들지 않으며 실제 파일·빌드 등록은 구현과 함께 한다.

<a id="task-01"></a>
## 3. 4.01 — 저장 공급 수신·입력 동결

정본: [진입 조건](dev4-implementation-plan.md#entry), [F01–F05](dev4-implementation-plan.md#freeze).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.01.1 공급·차이 목록 | 현행 SQL/DEV3 owner 계약·실제 구현 기록 | catalog/DDL, scan/point/range·owned row, token/observation·undo, commit/fencing, temp 효과 port 각각의 API·source·실행 범위와 미충족을 구분. 표현·유한 모델을 실제 disk 증거로 오인하지 않음 |
| 4.01.2 공통 경계 동결 | 독립 앞단은 D4-PREP-001과 실제 foundation 공급; engine 소비는4.01.1·해당 실제 DEV3 공급 | F01/F02의 AST/bound/metadata·error/outcome·session/result/token 소유 경계를 소비 leaf별로 동결·독립 검토. 전체 session/engine 경계를 lexer의 선행으로 두지 않고 존재하지 않는 API 이름을 callable로 등록하지 않음 |
| 4.01.3 실행 입력 수합 | 해당 4.08/4.16/4.18의 준비 산출물 | F03 spill bytes·F04 관측/fixture·F05 자원/성능 profile의 담당·완료/잔여를 관리. 아직 쓰지 않는 모든 미래 입력을 lexer의 선행으로 만들지 않음 |

<a id="dev5-supply"></a>
### DEV5 소비를 위한 SQL 공급 배정

[DEV5 D5-S03–S05](dev5-work-breakdown.md#supply)의 공급을 아래 기존 작업에 배정한다. 추가 SQL 기능이나 server 의존성을 도입하지 않으며 기존 분류 단위의 의미·Q1 인수는 유지한다. 현재는 OPEN_SUPPLY이고 실제 구현자·검토자·signature/receipt는4.01.1/2와5.01.1에서 소비 전에 확정한다.

| 공급 | 기존 작업·소유 | 공급 완료 조건 |
| --- | --- | --- |
| D5-S03 continuation | 4.13.1 statement/session 소유, 4.14.2 operation context, 4.14.3 cleanup; D5-S02 engine 대기 수신 | SELECT/DML/fetch/DDL의 실제 대기 지점에서 token·변경·예약·cursor 위치를 보존해 실행 소유 반환/재개. 같은 호출 deadline 유지, 새 fetch context 교체, stack borrow·부분 pin 잔류 금지, thread 이동 가능성·중복 wake/취소/close 검토. 호출 context만 존재하는 상태를 공급 완료로 세지 않음 |
| D5-S04 분류·권한 callback | 4.03 parser class, 4.10 DDL 조립, 4.11 prepared class, 4.13 거래 제어 | expected_result 첫 효과 전 검사와 자기 ROLLBACK의 신뢰 가능한 분류. DDL precommit 전 COMMIT 권한·후 semantic ddl 권한 및 두 outcome 보존. F01/F02·D5-F01/F02 공동 동결 |
| D5-S05 outcome·cleanup | 4.12.3 result 종료, 4.13.2/3 commit/rollback, 4.14.3 인계 | engine receipt/certainty·fence 보존, result/statement/txn/continuation 단일 cleanup owner. 실패 undo를 ABORTED로 만들지 않음. 의미 표를 먼저 공급하고 실제 SQL/engine 연결과 실패 사례를 별도 인수 |

동기 embedded facade는 동일 계약을 구동하고 server 타입을 import하지 않는다. 정확한 Rust continuation/Send·effect port 선택은 공급 구현 때 D2 검토한다. API 검증에 실제 server 전체를 선행시키지 않고 좁은 SQL/engine 시험을 공급하며, 실제 B3 연결은 DEV5가 담당한다. 계획 수정만으로 제품 구현·테스트를 시작하지 않는다.

<a id="task-02"></a>
## 4. 4.02 — 어휘·이름·문장 경계

정본: [유한 문법과 이름](../contracts/dev4-sql-design.md#grammar).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.02.1 token·span | 해당 F01, 4.14.1.1의 입력/할당 예산 | UTF-8·keyword·정수/text/binary·quote·주석, 원문 byte span/EOF와 잘린 입력의 literal vector. SQL 입력1 MiB·token65,536 경계 |
| 4.02.2 identifier 정규화 | 4.02.1 | ASCII 대문자·quoted bytes·PUBLIC prefix, 길이/control 거절과 V18. native catalog bytes의 전역 정규화는 하지 않음 |
| 4.02.3 단일 API·batch 경계 | 4.02.1 | API 다중 문장 거절, CLI streaming 분리 입력 공급. literal/comment의 `;`·`?`를 구분하고 batch의 빈 구간·미종료 입력 위치 보존 |

<a id="task-03"></a>
## 5. 4.03 — bounded parser·AST·오류 위치

정본: [문법](../contracts/dev4-sql-design.md#grammar), [자원](../contracts/dev4-sql-design.md#resources).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.03.1 공통 parser·SELECT·predicate AST | 아래 4.03.1.1–3 | 공통 입력과 SELECT/predicate 산출물을 수합. AST8,192·모든 재귀 nesting64와 단락 이전 전체 구문 검사 |
| 4.03.2 DML·DDL·거래 AST | 아래 4.03.2.1–3 | 네 DDL·세 DML·거래 제어의 지원 production, marker 허용 위치. DDL type_length는 구문과 precommit 이후 값 검사를 분리 |
| 4.03.3 진단·malformed 입력 | 해당 4.03.1/2의 구현 경로 | Syntax/Unsupported·원문 span/EOF, 잘린 token·알려진 후속 구문·한도±1 corpus. 각 문장 구현과 동시에 공급하고 전체 parser 완료를 기다리지 않음 |

| 실행 단위 | 직접 선행 | 공급·종료 조건 |
| --- | --- | --- |
| 4.03.1.1 공통 parser 기반 | 4.02.1/2/3, F01 | token cursor·이름/scalar/marker 표현·dispatch·종료 검사·node/depth 예산. SELECT clause 구현 없이 공급 가능 |
| 4.03.1.2 predicate | 4.03.1.1 | NOT/AND/OR·괄호·비교·IS NULL의 bounded 표현. UPDATE/DELETE와 SELECT가 같은 표현을 소비 |
| 4.03.1.3 SELECT clause | 4.03.1.1/2 | projection/FROM/ORDER BY/LIMIT·전체 문장 진단. sorter나 실행기의 완성이 선행은 아님 |
| 4.03.2.1 INSERT·거래 제어 | 4.03.1.1 | VALUES·열/tuple과 BEGIN/COMMIT/ROLLBACK/SET AUTOCOMMIT. 초기 INSERT/commit 경로가 SELECT parser를 기다리지 않음 |
| 4.03.2.2 UPDATE/DELETE | 4.03.1.1/2 | SET·선택 WHERE·문장 종류별 marker/진단. SELECT projection/order/limit은 비필수 |
| 4.03.2.3 DDL | 4.03.1.1 | 네 DDL·제약 정의·type_length 원문 보존. semantic validation은 4.10에 전달하며 parser에서 catalog를 조회하지 않음 |

4.03.3의 corpus·진단과 T01 인수는 위 각 공급에 붙인다. 자식 일부의 성공을 모든 문법 지원으로 표시하지 않으며 parser fuzz 의무는 [시험 선택](#test-scope)에 따라 관련 변경에 연결한다.

<a id="task-04"></a>
## 6. 4.04 — SQL 값·NULL·predicate

정본: [값·타입](../contracts/dev4-sql-design.md#values), [SQL/native 빈 text 경계](../contracts/dev4-sql-design.md#text-boundary).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.04.1 typed scalar 입력 | 4.02.1, F01 | Int64 min/max±1, Bool/Utf8/Binary·typed/untyped NULL 구분, checked 길이·대입 한도. 암묵 cast·절단 없이 오류 |
| 4.04.2 text 정규화·대입 검사 | 4.04.1 | 원래 타입 확인 뒤 SQL literal/bind의 빈 Utf8만 typed NULL. native 빈 Utf8·빈 Binary·공백/NUL 보존, NOT NULL·목적 열 길이와 V19/20 |
| 4.04.3 3값 predicate·비교 | 4.03.1.2, 4.04.1/2 | 독립 T/F/U 표와 모든 지원 타입 비교·IS NULL. WHERE는 TRUE만 통과하며 key total order·sort NULL 위치와 SQL 일반 비교를 구분 |

<a id="task-05"></a>
## 7. 4.05 — catalog binding·metadata

정본: [binder 경계](../contracts/dev4-sql-design.md#execution), [prepared metadata](../contracts/dev4-sql-design.md#prepared).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.05.1 이름·열·정체성 binding | 4.01.2의 catalog/guard 공급, 4.03/4.04의 해당 표현 | 하나의 catalog view에서 ObjectId/Generation/SchemaEpoch·열 ordinal·타입 결정. LIMIT0/거짓 조건도 없는 열·타입 오류를 숨기지 않음 |
| 4.05.2 BoundStatement·metadata | 4.05.1 | `*`의 catalog 순서, alias·중복 출력 이름의 ordinal, nullable/bytes·원본 identity. parameter 문맥 요구를 4.11.1에 전달하며 metadata를 execute마다 조용히 바꾸지 않음 |
| 4.05.3 catalog 수명 연결 | 4.05.1/2, 4.13.1 | idle prepare의 짧은 view와 active 거래 view 분리, engine guard의 연속 소유. DDL의 객체 존재·길이·제약 검사는 4.10의 precommit 이후 경계에서 호출 |

<a id="task-06"></a>
## 8. 4.06 — typed plan·index 선택·dependency

정본: [실행·access path](../contracts/dev4-sql-design.md#execution), [실행 시 stale 검사](../contracts/dev4-sql-design.md#prepared).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.06.1 RowScan 기준 plan | 4.05.2, F02 | node ID·typed operand/parameter slot·dependency 목록. private page/root 주소 없이 scan→filter→sort 필요 표시→project→limit 표현 |
| 4.06.2 point/range 선택 | 4.06.1, 실제 DEV3 index/key 공급 | leading equality prefix+다음 range·잔여 predicate·고정 후보 순위. OR/NOT fallback, 일반 `= NULL`과 IS NULL 구별, all-null unique의 복수 행 보존 |
| 4.06.3 실행 view에서 유효성 검사 | 4.05.1/2의 해당 bound dependency·F02, 4.13.1; SELECT access path 연결은 해당 4.06.1/2 | 동일 engine guard에서 DB/timeline/open·table/index identity/정의 대조 후 첫 row 접근/효과 전 StalePlan. SELECT/DML이 같은 검사를 소비하며 INSERT에 RowScan plan을 요구하지 않음. data commit만으로 stale 금지, 자동 reprepare·이름 재검색 금지 |

index path의 실제 결과는 4.07.3에서 literal·native·force-scan과 대조한다. planner 단독의 plan 모양 검사는 그 결과 동등성을 대신하지 않는다.

<a id="task-07"></a>
## 9. 4.07 — SELECT pull 실행

정본: [SELECT 실행](../contracts/dev4-sql-design.md#execution).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.07.1 scan/filter·owned row 공급 | 4.06.1/3, 4.04.3, 4.13.1, 4.14.1.1 | statement token·snapshot+own writes 고정, native cursor open/next/close·EOF, 외부에 page guard를 빌리지 않음 |
| 4.07.2 projection/LIMIT | 4.07.1, 4.05.2 | 미선택 ORDER BY 열을 sort 입력까지 유지, LIMIT0/1/최대·중복 projection·bag multiplicity. sort 없는 결과를 제한된 owned batch 소비자에게 공급 |
| 4.07.3 index·sort·result 연결 | 4.07.2, 해당 4.06.2/4.08.3.2/4.12.2 | point/range의 missing/duplicate·own insert/update/delete·undo read 및 NULL 다중 결과. ORDER BY는 sort 완료 뒤 결과, fetch/조기 close의 snapshot 수명까지 인수 |

4.07.1/2의 기준 cursor를 4.09가 먼저 소비할 수 있다. 4.07.3 전체 완료를 4.08·4.12 기반 구현의 선행으로 삼지 않는다.

<a id="task-08"></a>
## 10. 4.08 — ORDER BY·외부 정렬·임시 파일

정본: [정렬·자원](../contracts/dev4-sql-design.md#resources), [합산 예약](../contracts/dev4-sql-design.md#aggregate-pressure).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.08.1 comparator·run 표현 | 4.04·4.06.1의 해당 표현, 4.14.1의 해당 예약 계약 | 아래 재사용 판단과 F03 준비를 먼저 작성·검토하고, 동결 뒤 comparator/codec 구현. ASC/DESC·NULLS·복수 열·RowId tie-breaker와 key+RowId+projected 값의 독립 bytes·header/version/length/checksum/count |
| 4.08.2 bounded run/merge | 아래 4.08.2.1–3 | run 생성·spill·다중 merge의 별도 산출물을 수합. 입력+출력 peak·short I/O/ENOSPC/손상 인수 유지 |
| 4.08.3 소유 cleanup·SQL 연결 | 아래 4.08.3.1/2 | cleanup 기반을 SQL result보다 먼저 공급하고 실제 ORDER BY와 연결한 인수 수합 |

| 실행 단위 | 직접 선행 | 공급·종료 조건 |
| --- | --- | --- |
| 4.08.2.1 메모리 run | 4.08.1, 4.14.1.1 | bounded 입력·정렬·반복 읽기·동률·빈 run, 4 MiB와 최대 record 회계. SELECT 대신 고정 typed 입력으로 공급 가능 |
| 4.08.2.2 spill·manifest 생성 | 4.08.2.1, F03, 실제 temp port, 4.14.1.2·4.14.3.2 | 소유 등록·배타 생성·bounded manifest·run write/읽기, short I/O·ENOSPC·절단·손상과 실패 charge. full result 수명에 의존하지 않음 |
| 4.08.2.3 다중 merge | 4.08.2.2 | fan-in8·FD10·최대256 KiB record에서 buffer와 입력+출력 disk peak, run 수가 fan-in을 넘는 여러 pass·잔여 run·merge fault. 모든 run 이름을 RAM에 모으지 않음 |
| 4.08.3.1 sorter cleanup | 4.08.2.2의 소유 경로, 4.14.2·4.14.3.2; merge 변형은 4.08.2.3 | 실패 unlink charge·재시도, 취소·오류·lease 뒤 crash cleanup, 타 query 파일 보존. 새 foreground 예약 없이 단독 sorter 자원 반환 |
| 4.08.3.2 ORDER BY/result 통합 | 4.08.2.3·4.08.3.1, 4.07.1/2·4.12.2 | 미선택 정렬 열·LIMIT·fetch/조기 close·terminal error와 snapshot/자원 인수. 4.07.3의 정렬 부분과 같은 통합 증거를 참조 |

<a id="sort-reuse"></a>
F03 준비에서 SQL/engine 계약 담당은 [DEV3 offline build](../contracts/dev3-task-07-indexes.md)의 실제 정렬 공급과 다음 경계를 대조한다. 공통화 여부·실제 API/revision·재사용 또는 분리 이유를 4.08.1의 산출물로 기록하고 별도 D2 검토한다. 이번 계획이 공통 sorter 구현이나 새 crate를 승인한 것은 아니다.

| 비교할 부분 | 판단 기준 |
| --- | --- |
| run 입출력·merge·bounded buffer·임시 자원 port | 실제 소비자 둘의 의미·수명·오류·quota 소유가 일치하면 기존 하위 경계 재사용을 우선 비교. SQL을 engine의 의존성으로 만들거나 private API 접근을 허용하지 않음 |
| comparator·record 표현 | SQL ASC/DESC/NULLS·projected 값과 native composite key·unique/all-null 의미는 각 owner 유지. byte 형식 공유도 동일 계약일 때만 선택 |
| staging·cleanup | durable index build와 비영속 query spill의 공개·복구·예약 수명을 구분. SQL spill을 WAL/commit dependency에 편입하지 않음 |

일치하는 공급이 없으면 이유·중복 범위·재검토 조건을 남겨 별도 SQL 구현을 선택할 수 있다. 미래 범용화를 위해 DEV3 gate·Q1을 확장하지 않는다.

작은 LIMIT의 전체 sort 비용, 사용자에게 보장하지 않는 tie 순서, query256 MiB/DB1 GiB·FD10/64 등은 기존 정본을 유지한다. PostgreSQL/SQLite sorter 비교는 분해 근거이며 해당 temp format·threading·top-N을 도입한 결정이 아니다.

<a id="task-09"></a>
## 11. 4.09 — DML 대상 확정·단일 statement

정본: [DML·문장 최종 UNIQUE·observation](../contracts/dev4-sql-design.md#execution).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.09.1 대상·값 선확정 | 아래 4.09.1.1/2 | INSERT 값 준비와 UPDATE/DELETE 관측 목록을 별도 공급. 남은 거래 한도 사전 검사 |
| 4.09.2 native mutation·finish 연결 | 아래 4.09.2.1/2, 실제 DEV3 mutation/UNIQUE/undo 공급 | 한 execute_statement→finish_statement·statement-final UNIQUE 유지. 문장별 공급을 전체 DML 완료로 승격하지 않음 |
| 4.09.3 실패·충돌·잔류 회계 | 아래 4.09.3.1–3 | statement 복원·전체 conflict abort·잔류 물리 회계를 별도 인수하고 수합 |

| 실행 단위 | 직접 선행 | 공급·종료 조건 |
| --- | --- | --- |
| 4.09.1.1 INSERT 입력 | 4.03.2.1·4.05의 INSERT binding, 4.04.1/2, 4.06.3·4.13.1·4.14.1.1 | 열/tuple·omitted NULL·타입/bytes·남은 mutation/payload 한도. RowScan/projection/result 완료를 선행으로 두지 않음 |
| 4.09.1.2 UPDATE/DELETE 대상 | 4.03.2.2·해당 4.05, 4.06.3·4.07.1의 filter cursor, 4.13.1·4.14.1.1 | RowUID·원래 값·opaque observation을 제한 목록에 수집, mutation 전에 cursor close·같은 statement token 유지. SELECT projection/LIMIT은 비필수 |
| 4.09.2.1 INSERT statement | 4.09.1.1, native mutation/finish | VALUES 순서·count·모든 index 원자성·최종 UNIQUE. 명시 거래 안의 statement 성공이며 autocommit은 4.13.2.1에서 결합 |
| 4.09.2.2 UPDATE/DELETE statement | 4.09.1.2, native mutation/finish | RowId 순서·한 번씩·동일 값 UPDATE count·Halloween 방지. 두 문장 종류를 각각 선택 가능한 사례로 공급 |
| 4.09.3.1 statement 실패·용량 | 해당 4.09.2.1 또는 .2, 4.14.2·4.14.3.1 | 중간 key/quota·최종 UNIQUE·취소·256/257에서 S2만 복원하고 S1 보존·partial count 금지. INSERT 사례를 먼저 공급하고 UPDATE/DELETE는 해당 구현에 붙임 |
| 4.09.3.2 관측 충돌 | 4.09.2.2, 실제 native observation/전체 abort | V26의 즉시/대기 X-lock·삭제·같은 값 새 버전·own write·split을 구분, commit 전 첫 effect 이전 전체 conflict 결과 |
| 4.09.3.3 반복 실패 잔류 회계 | 해당 4.09.3.1, 4.13.3.1, 실제 engine 잔류 charge/예약 | V27의 WAL/CLR·undo/page·잠금 charge와 전체 abort 예약. runtime 사례와 4.17의 crash 사례를 구분 |

SQL의 자체 역 DML·hidden retry·여러 commit 분할은 금지한다. runtime rollback 의미는 여기서 인수하고 crash 중 rollback·CLR 재실행은 4.17에서 별도로 연결한다.

<a id="task-10"></a>
## 12. 4.10 — DDL 정의·precommit·두 결과

정본: [DDL 경계](../contracts/dev4-sql-design.md#ddl-boundary), [타입·제약 정의](../contracts/dev4-sql-design.md#values).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.10.1 precommit 이전 진입 판정 | 4.03.2.3·해당 4.03.3, 4.13.1, 해당 F02 | 지원 문법·namespace·session/handle·ResultBusy·사전 취소만으로 진입 판단. 진입 거절은 기존 상태 유지: Active이면 prior/ddl 모두 NotAttempted, Idle이면 prior=NotPresent·ddl=NotAttempted |
| 4.10.2 새 catalog의 DDL lowering | 4.05.1/2의 공통 binding, 실제 DEV3 원자 DDL 공급 | precommit 뒤 호출하는 semantic 검증. CREATE/DROP TABLE/INDEX·PK nullable 강화·소유 제약 index 보호·부분 NULL·index/key 한도. PK/UNIQUE 포함 create는 하나의 native DDL |
| 4.10.3 이전 거래 종료→독립 DDL | 4.10.1/2, 4.13.2.1의 commit adapter, 4.14.2 | Active이면 prior=Committed 뒤 새 catalog 검증/DDL, Idle이면 prior=NotPresent로 독립 DDL. 이름·길이 오류도 이미 commit된 DML 유지. prior의 NotPresent와 두 거래의 NotAttempted/Committed/Aborted/NotCommittedRecoveryRequired/Unknown·receipt를 정본의 적용 범위대로 반환; V21/22 |

DDL semantic validation을 일반 DML binding 순서에 끼워 precommit보다 앞당기지 않는다. DDL 두 거래 사이·각 commit 응답 경계의 kill/reopen은 4.17.3의 R4에 배정한다.

<a id="task-11"></a>
## 13. 4.11 — prepared·parameter·binding

정본: [typed prepared](../contracts/dev4-sql-design.md#prepared), [교체 peak](../contracts/dev4-sql-design.md#aggregate-pressure).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.11.1 parameter 추론·hint | 4.03/4.04·4.05.2의 해당 공급 | lexical `?` ordinal, 목적/비교/LIMIT 문맥 타입·NULL/bytes, 불명 타입과 충돌 hint 거절. metadata 불변·DDL/거래 제어 prepare Unsupported |
| 4.11.2 owned bind_all | 4.11.1, 4.12.1, 4.14.1.1 | count/type/UTF-8→빈 text→NULL/길이→old+new 예약→원자 교체. 실패 시 이전 binding 전체 보존, caller buffer 해제·injection 문자를 SQL로 재해석하지 않음 |
| 4.11.3 반복 실행·direct 공통화 | 4.11.2, 해당 4.06.3/4.07/4.09·4.12.2 | 지원 SELECT/DML의 direct/prepared 결과·오류 동등, 매 execute dependency 검사, 실행 사이 binding 유지. DDL/거래 제어는 direct dispatch에 유지 |

<a id="task-12"></a>
## 14. 4.12 — result·handle·fetch

정본: [상태·수명](../contracts/dev4-sql-design.md#prepared).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.12.1 slot/generation 기반 | F02, 4.14.1.1 | session incarnation·slot generation·closed tombstone의 bounded 표현, 교차 session/engine·slot 재사용 거절. statement당 terminal descriptor1개와 direct용1개 |
| 4.12.2 result·batch 상태 | 4.12.1, 4.07.1/2, 4.13.1, 4.14.2 | session당 활성 result1개, owned fetch 1–128행/256 KiB. RowBufferTooSmall은 위치 보존, EOF/terminal 오류/조기 close 구분·complete 정확성 |
| 4.12.3 종료·반복 수명 통합 | 4.12.2, 4.13.3, 4.14.3 | ROLLBACK/close_session 무효화, current generation close 멱등·과거 handle 거절. V15 반복·V16 거래 유지·V28 호출 context와 pin/FD/메모리 회수 |

4.12.1은 독립 상태 기반을 먼저 공급하며 SELECT·prepared 전체 완성을 기다리지 않는다. terminal 오류에 row·spill·snapshot을 붙잡아 두지 않는다. 상태 계약·유한 모델의 실제 담당과 산출물은 [공통 수명 통합](#lifecycle-model)을 따른다.

<a id="task-13"></a>
## 15. 4.13 — session·statement scope·거래 종료

정본: [session·오류 효과](../contracts/dev4-sql-design.md#transactions), [복구 경계](../contracts/dev4-sql-design.md#recovery-boundary).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.13.1 상태·statement scope·오류 공급 | 아래 4.13.1.1/2 | SELECT/DML이 소비할 상태와 공통 오류 변환을 별도 공급하고 수합. guard 대신 raw q 재구성 금지 |
| 4.13.2 COMMIT·autocommit 통합 | 아래 4.13.2.1–3 | DML commit·SELECT ReadOnly 해제·명시 ReadWrite validation을 구분하여 수합 |
| 4.13.3 ROLLBACK·오류·종료 통합 | 아래 4.13.3.1/2 | result 없는 거래 종료를 먼저 공급하고 result 취소·무효화와 통합. UNKNOWN 자동 retry/rollback 금지 |

| 실행 단위 | 직접 선행 | 공급·종료 조건 |
| --- | --- | --- |
| 4.13.1.1 session·statement 상태 | F02, 실제 DEV3 transaction/snapshot/undo 공급, 4.14.1.1 | Idle/Active/Fenced·ON/OFF·BEGIN·begin/finish/취소 token 소유와 진입 전/후 상태. 오류 표현은 4.13.1.2와 결합하고 다른 문장 전체를 기다리지 않음 |
| 4.13.1.2 공통 SQL 오류 변환 | 해당 F01/F02의 오류 vector·foundation/engine 오류 계약 | code/phase/span·effect/state/certainty/retry_scope·원인 engine code, DDL 두 outcome의 공통 표현/변환. 입력 진단과 cleanup 결과를 합칠 때 RecoveryRequired/Fenced·확정 commit을 보존. 독립 literal 반례로 검산하고 첫 SQL 소비에 공급 |
| 4.13.2.1 commit adapter·작은 DML | 4.13.1, 4.14.2·4.14.3.1, 실제 engine commit; SQL 결합은 4.09.2.1·4.09.3.1의 INSERT 부분 | commit attempt/sync/공개/응답·certainty·fencing 전달. Idle ON INSERT는 COMMITTED 뒤 count; 명시/OFF INSERT는 statement 성공 count와 Active를 유지하고 별도 COMMIT outcome을 인수. SELECT/result·다른 DML·DDL을 기다리지 않고 native fixture의 commit 경계도 소비 |
| 4.13.2.2 ON SELECT 종료 | 4.13.1·4.12.2, 4.14.3.1 | EOF/close/terminal error의 ReadOnly 해제·durable receipt 없음, API 진입 오류/RowBufferTooSmall 상태 보존 |
| 4.13.2.3 명시 ReadWrite validation | 4.13.2.1, 해당 4.07/4.09·4.12 | OFF/BEGIN의 무변경 validation·같은 table 충돌·독립 table 성공. 해당 SQL 경로별로 T09 인수 |
| 4.13.3.1 result 없는 abort/close | 4.13.1, 4.14.2·4.14.3.1, 실제 engine abort/fence; DML 인수는 해당 4.09 | rollback·close_session·Drop 인계에서 undo 완료와 Idle/Fenced 구별, 미완료 소유 유지. commit adapter 전체나 SELECT 수명 완성을 기다리지 않음 |
| 4.13.3.2 result를 포함한 종료 | 4.13.3.1·4.12.2, 해당 4.13.2·4.14.3 | result 취소→undo 확인·generation 무효화, 앞선 DML·알려진 commit 결과 유지. 4.12.3과 같은 통합 증거 연결 |

4.13.1은 DML 전체에 의존하지 않는다. 4.13.2/3에서 그 소비 결과를 인수하므로 session과 executor의 서로 다른 전체 완료를 선행으로 요구하는 순환을 만들지 않는다. 현재 SQL Serializable만 소비하고 Oracle READ COMMITTED를 노출하지 않는다.

4.13.1.2의 구현 담당은 SQL 공통 오류 경계를 소유하고, 각 기능은 자기 phase·실제 cleanup/outcome 입력을 공급한다. 담당 분류와 코드 의존은 구분한다. 공통 오류 표현을 사용하려고 lexer/parser가 session runtime을 호출하거나 의존하게 만들지 않으며 실제 배치는 F01/F02에서 정한다. CLI·worker는 이 결과를 표시/기록하며 오류 효과를 다시 판정하지 않는다. 이 단위는 engine rollback·commit을 실행하거나 SQLSTATE를 정하는 두 번째 manager가 아니다. T01/03/07–11의 관련 오류와 4.16.3.1의 잘못된 효과·certainty 반례를 연결한다.

<a id="lifecycle-model"></a>
### 상태 통합 책임과 유한 모델 배정

실제 착수 때 4.13의 **수명 통합 담당 1명**을 지정한다. 4.12는 result/handle, 4.13은 transaction/statement, 4.14는 operation context·SQL 자원/engine 인계의 계약 담당을 유지한다. 통합 담당은 이들의 변경 순서와 아래 상태표를 조정하며 독립 oracle를 대신 작성·승인하는 역할은 아니다.

공통 산출물은 `호출/사전 상태 → 반환 code/effect/certainty → result 상태 → statement 상태 → transaction 상태 → 남은 snapshot/undo/lock/temp/예약 소유자` 표다. 정상 EOF·조기 close·terminal fetch 오류·RowBufferTooSmall·ROLLBACK·close_session·undo 실패/Fenced·commit 도중 취소·늦은 이전 호출 취소·slot 재사용을 포함한다. 의미는 SQL 정본을 연결하고 새 규칙을 복제하지 않는다.

| 산출물·담당 작업 | 선행·동결 | 완료 조건 |
| --- | --- | --- |
| 상태/action 계약: 4.12.1·4.13.1·4.14.2, 4.13 통합 담당 수합 | F02 준비; 설계는 production 선행과 독립 | 위 상태표와 generation·parameter 동치류·불가능 전이·예외를 별도 검토. 구현은 실제 공급·해당 동결 뒤 |
| 독립 모델 입력·탐색기: 4.16.1.2 | 위 F02 상태/action 동결, F04 model recipe | 기존 의무인 2 session·각 prepared2개·활성 result1개/session·operation 길이6 이내의 유한 공간. 상태/전이 최대 수와 종료 상한을 실행 전에 계산·고정; production 전이 함수를 정답으로 재사용하지 않음 |
| 모델 완주·known-bad: 4.16.3.2 | 4.16.1.2의 새 모델 코드·입력 | stale generation 수락·늦은 cancel 전파·result 해제와 거래 commit 혼동·undo 미완료를 Idle로 낮추는 mutant 거절. 탐색 완주 receipt, 미도달·상한 도달은 INCONCLUSIVE. 실제 I/O/thread 인수를 대체하지 않음 |
| 실제 교차 수명: 4.12.3·4.13.3.2·4.14.4 | 해당 소비 경로·4.16.2 관측 | 같은 상태/action ID로 T08/09/11·V15/16/28의 실제 결과와 소유 회수/인계를 연결. 모델 PASS를 제품 PASS로 승격하지 않음 |

<a id="task-14"></a>
## 16. 4.14 — 취소·합산 자원·cleanup

정본: [호출별 context](../contracts/dev4-sql-design.md#operation-context), [자원](../contracts/dev4-sql-design.md#resources), [합산 peak](../contracts/dev4-sql-design.md#aggregate-pressure).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.14.1 admission·회계 기반 | 아래 4.14.1.1/2 | memory와 temp disk/FD를 각각 공급하며 DB 총량·cleanup 사전 예약의 공통 계약 수합 |
| 4.14.2 호출 context·취소 경계 | 4.14.1.1, F02; temp 소비 연결에는 4.14.1.2 | execute/fetch/close별 generation/cancel/deadline, 같은 호출 재개는 원래 deadline 유지. parse/row/sort/I/O checkpoint, engine rollback/commit 결과 우선; 완료 호출의 취소가 다음 fetch에 전파되지 않음 |
| 4.14.3 정리·인계 port | 아래 4.14.3.1/2 | engine abort/예약 인계와 query temp cleanup의 서로 다른 소유·실패 경계 수합 |
| 4.14.4 다중 session 소비 인수 | 해당 4.08/4.11/4.12/4.13의 연결 | 각 query 한도 이내이나 합산 초과, old+new binding·merge 입력+출력 peak, 취소/close/crash가 다른 owner 자원을 회수하지 않음. T07/08/11/14의 기존 합산 반례 |

| 실행 단위 | 직접 선행 | 공급·종료 조건 |
| --- | --- | --- |
| 4.14.1.1 memory 예약 | F01/F02·해당 F05의 memory 부분, foundation 계약 | 입력/AST·prepared/binding·workspace·batch·sort의 checked 예약·owner 반환, DB64 MiB·old+new peak와 cleanup 공간. lexer는 입력/할당 부분만 소비 |
| 4.14.1.2 temp disk/FD 예약 | 4.14.1.1의 owner 계약, F03·해당 F05, 실제 temp 효과 port | query/DB disk·FD 합산과 merge 공존 peak·실패 unlink charge, allocator/engine budget과 이중 배정 금지 |
| 4.14.3.1 engine abort/수명 인계 | 4.14.1.1·4.14.2, 실제 engine abort/fence | snapshot·undo/lock·남은 예약의 종료/인계와 실패 Fenced. Drop hidden commit·새 allocation·panic 금지. query spill이 없어도 공급 가능 |
| 4.14.3.2 temp cleanup port | 4.14.1.2·4.14.2, 실제 owner temp port·F03 | 소유 등록/인계·삭제 실패 회계·lease 뒤 cleanup, 타 owner 보존. sorter 전용 codec/manifest 소비는 4.08에서 검증하며 port가 sorter 완성을 기다리지 않음 |

4.14.2는 memory만 쓰는 호출에는 4.14.1.1을, temp 소비 호출에는 4.14.1.2를 받는다. 4.14.4는 memory/binding, disk/merge, FD, engine 인계 교차 사례를 각각 선택 가능하게 배정하고 첫 해당 소비부터 붙인다. 모든 자원 종류의 전체 완료를 새 lexer/commit의 선행으로 묶지 않는다.

4.14.1–3의 공통 공급을 먼저 만든 뒤 각 소비 경로에서 통합한다. 전체 압력 인수 4.14.4를 parser·sorter·session 기반의 선행으로 되돌리지 않는다.

<a id="task-15"></a>
## 17. 4.15 — embedded facade·CLI

정본: [CLI 인수](dev4-implementation-plan.md#cli), [패키지 경계](../contracts/dev4-sql-design.md#execution).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.15.1 open/create 조립 | 실제 engine 공급, 4.13·4.14의 해당 경계 | 명시 DB 경로·create/open 구분, platform 효과 주입·SQL facade. 모르는 기존 파일 삭제 금지; 실행 가능한 패키지를 구현 때 등록 |
| 4.15.2 interactive/batch·출력 | 4.02.3, 지원 direct 실행·4.12, F04 | streaming stdin/file·기본 fail-fast, typed metadata/row/count·versioned JSON의 prefix/terminal, byte span·stderr·exit0/1/2. NULL/native 빈 text/Binary 구분 |
| 4.15.3 오류·EOF·SIGINT | 4.15.1/2, 4.10.3·4.13.3·4.14.2/3 | EOF는 미commit 거래 rollback, 실패는 비정상 종료·Fenced. 이미 commit된 DDL prior 결과·autocommit 성공 보존. bind 문자열 치환 기능 없이 typed API로 prepared 인수 |

<a id="task-16"></a>
## 18. 4.16 — 독립 oracle·SQL adapter·coverage

정본: [검증 독립성](dev4-implementation-plan.md#verification), [의미 vector](../contracts/dev4-sql-design.md#vectors).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.16.1 독립 literal·coverage·모델 | 채택된 문법/값/오류·T/V 정본; 모델 구현은 F02/F04 | row bag·3값·허용 tie 집합·명시 순서·serial history, V01–28과 grammar/type/statement 교차 목록. 별도 산출물로 [상태 모델 입력·탐색기](#lifecycle-model)를 배정. 설계 준비는 제품 선행과 독립; 실제 oracle 코드는 해당 동결 뒤 작성 |
| 4.16.2 worker·관측 adapter | 해당 F04, 공급된 SQL/engine 경로 | SQL/native 입력·row/error/ACK·DDL 두 outcome·RecoveryRequired 관측. 외부 controller에 실제 ACK 보존, protocol/version·명령·source/fixture/seed/schedule identity |
| 4.16.3 부정 판정·coverage 수합 | 해당 4.16.1; 실제 관측 판정은 4.16.2와 해당 기능 | 잘못된 값/NULL/order/ACK·누락 cleanup·잘못된 binary/recipe를 거절. 모델 완주/known-bad는 실제 SQL worker 완성을 기다리지 않음. native/SQL 두 production 경로 일치만으로 PASS 금지, bag 중복·incomplete prefix·N/A 근거 보존 |

4.16은 기능 구현과 함께 테스트를 추가하는 공통 책임이다. 모든 제품 기능이 끝난 뒤 oracle를 처음 만들지 않는다. 긴 fuzz/부하/조합의 실행 입력과 검증 강도는 원래 계획을 유지하되 실제 실행 선택은 아래 사용자 지시를 따른다.

| 실행 단위 | 직접 선행 | 공급·종료 조건 |
| --- | --- | --- |
| 4.16.1.1 literal·case inventory | T/V 의미 정본으로 준비; 실행 oracle 구현은 해당 F04 동결 뒤 | 입력·독립 기대값·N/A 이유·주 담당 ID·최소/확대 변형을 F04에 공급. 첫 INSERT/오류 사례부터 준비하며 미래 전체 조합을 초기 경로의 선행으로 두지 않음 |
| 4.16.1.2 상태 모델 입력·탐색기 | F02 상태/action으로 recipe·상한 준비; 탐색기 구현은 해당 F04 동결 뒤 | 기존 유한 공간·상한을 F04에 공급하고 동결 뒤 독립 전이 계산 구현. SQL worker 없이 공급 가능 |
| 4.16.2.1 typed SQL/native 관측 adapter | 해당 F04·4.16.1.1, 실제 해당 SQL/engine API | row/count/error·metadata·prefix/terminal·outcome 관측. CLI 출력에 의존하지 않고 최초 경로부터 공급 |
| 4.16.2.2 외부 ACK·fault 연결 | 4.16.2.1의 해당 관측, 기존 controller/engine fault port·F04 | worker 밖 ACK·transaction identity와 실제 cut witness·fresh fixture를 연결. 4.17.1의 SQL/engine cut 대조에 공급하며 R1–R4 전체 실행 완료는 선행이 아님 |
| 4.16.3.1 판정 부정 사례·coverage | 해당 4.16.1.1·4.16.2.1; crash 판정은 4.16.2.2 | wrong row/bag/order/error/ACK·누락/미도달·잘못된 binary/recipe 거절. 실행한 경로와 남은 inventory 구분, production 오류 변환을 기대값 계산에 사용하지 않음 |
| 4.16.3.2 모델 완주·known-bad | 4.16.1.2 | [수명 모델](#lifecycle-model)의 고정 공간 완주·지정 mutant 거절, 상한 도달은 INCONCLUSIVE. 실제 SQL adapter 완성을 기다리지 않음 |

4.16.1/2/3은 이 자식의 범위 수합이며 별도 구현 묶음으로 중복 배정하지 않는다. F04 입력의 준비와 동결 후 oracle/adapter 구현을 구분하여 inventory 자신의 준비에 F04 완료를 요구하지 않는다. 복구 fault 정의/판정의 owner는 4.17, 관측 전달의 owner는 4.16이다. 같은 ACK 수집기나 verdict를 두 번 만들지 않는다.

<a id="task-17"></a>
## 19. 4.17 — rollback·ACK·disk 복구 통합

정본: [R1–R4](dev4-implementation-plan.md#rollback-recovery), [DEV3 복구 수신](../contracts/dev4-sql-design.md#recovery-boundary).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.17.1 fault·ACK 관측 연결 | 4.16.2.2의 해당 관측, 해당 DEV3 WAL/undo/CLR·복구 실제 증거 | update WAL과 첫 commit record 시도 구별, append/sync/공개/응답·undo/CLR·flush 도달 witness. 소유 worker의 kill·short write/sync 오류·fresh fixture 식별 |
| 4.17.2 R1/R2·rollback 재장애 | 아래 4.17.2.1/2 | statement rollback 완료 뒤 commit과 rollback 중 미commit crash를 별도 인수 |
| 4.17.3 R3/R4·checkpoint/restore | 아래 4.17.3.1–3 | DML 응답 유실·DDL 두 거래·checkpoint/restore 교차의 별도 identity/결과 수합 |

| 실행 단위 | 직접 선행 | 독립 기대값·종료 조건 |
| --- | --- | --- |
| 4.17.2.1 R1 | 4.17.1, 해당 4.09.3.1·4.13.2.1 | S1 성공→S2 실패→statement rollback 완료→COMMIT ACK 뒤 kill/reopen·복구 재장애에서 S1만 보존. 작은 INSERT 사례부터 시작 |
| 4.17.2.2 R2 | 4.17.1, 해당 4.09.3.1·4.13.3.1, native rollback/recovery | rollback 시작/중간/완료 직전 crash에서 S1도 포함한 미commit 거래 전체 소멸·다른 거래 ACK 보존. 낮은 자원·부분 CLR·예약 복원·반복 장애와 V27 교차는 해당 공급에 붙임 |
| 4.17.3.1 R3 | 4.17.1·4.13.2.1, 해당 4.09.2.1 | DML durable 뒤 응답 유실·UNKNOWN 관측과 거래 전체 보존·반복 reopen의 중복/누락 거절. DDL·ORDER BY·SELECT result는 선행이 아님 |
| 4.17.3.2 R4 | 4.17.1·4.10.3·4.13.2.1 | prior/DDL 두 transaction의 시도/sync/응답 및 사이 cut를 각각 판정. DDL 실패/취소/UNKNOWN에도 durable prior 보존 |
| 4.17.3.3 checkpoint·restore 교차 | 4.17.1, 해당 SQL 변경 경로, 실제 DEV3 checkpoint/backup/restore 공급 | row/모든 index/catalog·새 timeline/open identity·restore 뒤 새 ACK와 재시작을 독립 판정. R1–R4 필요한 변형별 입력·도달 witness를 연결하되 무변경 하위 시험은 재실행하지 않음 |

4.17.1의 관측 adapter는 첫 commit/rollback SQL 연결과 함께 공급한다. 각 실행 단위는 최소 사례와 남은 문장/index/fault 변형을 구분하여 기록한다. 초기 R1/R2/R3 성공만으로 T10/T11 또는 해당 부모 작업 전체를 완료하지 않는다.

ARIES는 partial/total rollback·WAL·redo-only CLR·반복 장애를 구분할 연구 입력이다. SQL이 CLR/undo chain·page 복구·별도 reserve ledger를 구현하는 작업은 추가하지 않는다. DEV3 선행 결과는 원래 source·범위의 증거로 수신하고, SQL 통합에서 바뀐 경로의 검증과 구분한다. kill을 power loss 검증으로 확대하지 않는다.

<a id="task-18"></a>
## 20. 4.18 — 계측·PF10·Q1 수합

정본: [PF10](dev4-implementation-plan.md#performance), [Q1](stage-completion.json).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.18.1 계측 입력·profile 준비 | 각 연산자/효과 계약·기존 PF10 | F05를 준비·검토·동결하고 해당 leaf의 계측을 함께 구현. parse/bind/plan/node ID·rows/loops·storage/lock/gate/flush·sort/peak/commit outcome 연결. 미래 모든 workload의 입력을 초기 node 계측의 선행으로 두지 않음 |
| 4.18.2 실제 관측·성능 | 아래 4.18.2.1–5 | P10-A–E의 독립 산출물 수합. 성공/시도TPS·conflict/Limit/Fenced·자원·cleanup 분리 |
| 4.18.3 Q1 증거 closure | 관련 4.01–4.17 산출물, 4.18.2 | SC02·KU13·PF10, T01–14/V01–28/R1–4·합산 자원 인수의 원래 source/범위와 잔여 의무 수합·별도 검토. NOT_RUN·미도달·실패가 남으면 Q1 미완료; official distribution은 별도 |

| 실행 단위 | 직접 선행 | profile별 인수 |
| --- | --- | --- |
| 4.18.2.1 P10-A | 해당 4.06/4.07·4.11, 4.16 판정·4.18.1 | scan/index·native/direct/prepared의 동등 point/range 결과와 parse/bind/plan·방문 row/page·시간. scan/index workload 중 적어도 하나에 전체 page buffer4배 working set·실제 file I/O witness |
| 4.18.2.2 P10-B | 해당 4.04/4.07/4.09·4.13, 4.16 판정·4.18.1 | 선택도·NULL/중복/편향, residual·방문량·table 충돌·독립 table 처리량 |
| 4.18.2.3 P10-C | 4.08·해당 4.07/4.12, 4.16 판정·4.18.1 | sort memory4배 입력·복합 정렬·작은/큰 LIMIT, run/merge pass·disk peak·첫 row/전체 fetch 시간 |
| 4.18.2.4 P10-D | 해당 4.09·4.13·4.17, 4.16 판정·4.18.1 | DML·제약 실패·commit/reopen의 동등 ACK/rollback·SQL 비용과 engine lock/gate/DWB/sync 비용 분리 |
| 4.18.2.5 P10-E | 해당 4.11–4.14, 4.16 판정·4.18.1 | off/상시/상세 계측의 비용·유실·정확성, fetch chunk·조기 close·합산 pressure·RSS/FD/pin·취소 회수 |

각 군의 실제 fixture·수치 예산·명령은 F05 동결 뒤 실행한다. 새 경로의 기능 인수와 관측 준비를 함께 배정하고, 관련 성능 실행은 해당 경로가 비교 가능해졌을 때 선택한다. 모든 CLI 표시 기능을 native/direct 비용 측정의 선행으로 두지 않는다. 미완료 조합과 긴 실행은 해당 군의 잔여 의무로 보존하며 Q1 수합을 이유로 완료된 무변경 사례를 다시 실행하지 않는다.

성능 비교는 동일 의미·정답·durability가 전제다. 존재하지 않는 이전 SQL 성능을 기준선으로 만들거나 실패 workload를 표본에서 숨기지 않는다. 분류 수·파일 수·계획 채택을 기능 완료율로 환산하지 않는다.

<a id="sequence"></a>
## 21. 공급 순서와 작업 크기

아래 묶음은 기존 D4-0–7의 책임 안에서 좁은 공급과 인수를 배정하는 우선순위다. D4-0–7을 전부 완료한 뒤 다음 번호를 시작하는 장벽으로 해석하지 않는다. [D4-PREP-001의 독립 모듈](dev4-preparation-plan.md#sequence)은 DEV3와 병행 구현할 수 있다. engine 소비의 DEV3 진입 조건·Q1 전체 범위는 유지하며 실제 공급 미완료를 이 순서표로 면제하지 않는다.

| 묶음 | 먼저 준비·공급할 단위 | 뒤에서 닫을 소비·인수 |
| --- | --- | --- |
| A: 공통 입력·관측 | 4.01.1/2, 4.16.1 상태 모델/literal, 4.18.1; 4.14.1.1·4.14.2·4.14.3.1·4.12.1·4.13.1의 해당 기반 | F01/F02·실제 DEV3 공급. 4.16.2의 최소 row/error/ACK 관측과 4.17.1 fault adapter를 첫 소비 경로 전에 연결 |
| B: 작은 영속 SQL 경로 | 4.02→4.03.1.1/2.1·해당 4.03.3→4.04.1/2·4.05의 INSERT binding→4.06.3→4.09.1.1/2.1/3.1→4.13.2.1·4.13.3.1; 별도로 SELECT 표현→4.06.1/3→4.07.1/2→4.12.2·4.13.2.2 | native로 준비한 정본 fixture에서 직접 INSERT·COMMIT·reopen, scan/fetch/close. SQL DDL·index 선택·sort 전체를 기다리지 않음. native fixture를 SQL CREATE 성공 증거로 계산하지 않음 |
| C: 실패·내구성 경계 | 4.17.2.1/2·4.17.3.1, 해당 4.09.3.3 | 초기 INSERT의 S2 실패/S1 보존, 전체 abort·R1/R2/R3·RecoveryRequired. 실제 rollback 중 crash·반복 recovery·ACK 독립 판정 |
| D: 문장·상태 확장 | 4.11.1/2/3, 4.09.1.2/2.2/3.2, 4.13.2.3/3.2·4.12.3, 4.10.1/2/3→4.17.3.2 | prepared/UPDATE/DELETE·관측 충돌·DDL 두 outcome과 R4. parameter slot·타입 문맥·metadata는 B의 표현 준비 때부터 4.05/4.11이 함께 동결 |
| E: index·sort·합산 소비 | 4.06.2, 4.08.1→4.08.2.1–3→4.08.3.1/2, 해당 4.14.1.2/3.2·4.14.4, 4.07.3 | index와 sorter는 선행 충족 시 별도 진행. memory/binding pressure는 D부터, disk/FD pressure는 해당 sorter 소비부터 인수 |
| F: CLI·잔여 변형·수합 | 4.15, 4.17.3.3·R1–R4의 남은 변형, 4.18.2.1–5→4.18.3 | Q1 필수 inventory closure. 4.16.2/3·계측·각 PF10 군은 첫 관련 기능부터 붙이며 F에서 처음 구현하지 않음 |

4.08·4.09·4.13/14·4.17·4.18은 위 명시한 자식 단위로 배정한다. 4.02–4.06·4.10/12도 실제 API/파일·반례가 확정되면 의미별로 좁혀 배정하며 모든 문법/타입 전체를 초기 경로의 선행으로 두지 않는다. 작은 공급의 완료와 부모의 전체 지원·인수 완료를 구분한다. A–F는 별도 제품 gate나 배타적인 기간이 아니며, 실제 선행이 충족된 단위는 다른 묶음 전체를 기다리지 않는다.

분량은 변경할 상태·I/O/fault 종류·소유 인계·독립 oracle/harness 개발량으로 추정한다. 4.02.2의 이름 정규화와 4.08.2.3의 merge, 4.17의 crash 이력을 동등한 1건으로 환산하지 않는다. 초기 배정 후 실제 구현·검토·선택 시험·재작업 시간을 별도 기록하여 다음 단위의 추정에 반영한다. 기능 수·문서 분량이나 아직 측정하지 않은 속도로 기간·효율을 확정하지 않는다.

이번 작업에서 구현 agent·파일 소유를 배정하지 않는다. 향후 서로 겹치지 않는 lexer/vector 준비·sorter·CLI 표시를 분리할 수 있어도 공통 BoundStatement/오류·session/result·문서 JSON의 변경은 통합 담당이 순차 반영한다. 실제 배정 시 [파일 소유 정책](../policies/parallel-development-policy.md)에 따라 기준 revision·독점/공유 경로·수신 산출물·검토자를 기록한다.

<a id="workload-review"></a>
### 18개 책임의 분량·불확실성 재점검

아래는 현재 계약에서 예상되는 **상대적 준비/구현/검증 부담**이며 기간·인원·완료율 추정이 아니다. `큼`은 별도 I/O·소유 인계·독립 harness가 여러 개 있다는 뜻이다. 실제 API와 최소 사례를 확인한 뒤 [착수 기록](#entry-exit)에 공수 범위를 적는다. 기존 18개를 균등 분배하거나 모두 독립 개발자에게 하나씩 배정하는 근거로 사용하지 않는다.

| 분류 | 상대 부담·주 불확실성 | 배정 판단·첫 종료점 |
| --- | --- | --- |
| 4.01 공급/동결 | 중간, DEV3 실제 공급에 따라 변동 큼 | 공통 문서 전부의 완료 대신 소비 leaf별 API·receipt 차이 목록. 외부 대기는 구현 공수와 별도 기록 |
| 4.02 lexer | 중간, bytes/span·경계 corpus | 세 하위 단위 유지. token/span과 단일 문장 경계를 B에 먼저 공급 |
| 4.03 parser | 중간, 공통 재귀 예산·문장별 오류 | 위 여섯 자식과 구현 동반 진단. INSERT/거래 제어에 SELECT clause를 선행시키지 않음 |
| 4.04 값/타입 | 중간, SQL/native 빈 text·3값 교차 | scalar/입력 정규화 먼저, predicate 뒤. 이미 존재하는 foundation 의미를 중복 구현하지 않음 |
| 4.05 binder | 중간~큼, catalog guard·parameter/metadata | 공통 identity 공급과 문장별 binding 인수 구분. INSERT의 column/type dependency를 먼저 공급 |
| 4.06 planner | 중간, composite bound·stale race | 공통 dependency 검사를 SELECT plan에서 분리 공급; index 선택/동등성은 별도 종료 |
| 4.07 SELECT | 중간~큼, snapshot·pull/EOF | scan/filter 공급 후 projection/result 통합. UPDATE/DELETE에 projection 전체를 요구하지 않음 |
| 4.08 sorter | 큼, 실제 temp port·merge peak·I/O 실패 | 기존 run/spill/merge/cleanup 단위를 유지. F03 재사용 판단·최대 record 예약 검토를 조기 준비 |
| 4.09 DML | 큼, 단일 statement·observation·undo | INSERT 먼저, 대상 조회/변경·충돌·잔류 회계 각각 인수. 행 조회와 mutation 사이 token 보존 |
| 4.10 DDL | 중간~큼, precommit 뒤 오류·두 outcome | 세 기존 단위 유지. 작은 CREATE TABLE 조립부터 공급하되 index build/drain의 native 구현은 DEV3 책임 |
| 4.11 prepared | 중간~큼, 추론·불변 metadata·교체 peak | B의 표현 준비 때 marker/dependency 계약, 반복 실행은 해당 SELECT/DML과 연결 |
| 4.12 result | 큼, generation·오류/EOF·거래 수명 | slot 기반 먼저, bounded fetch 뒤, 종료 교차 인수. 4.13 수명 통합 담당과 같은 상태표 사용 |
| 4.13 session | 큼, commit/abort·fence·오류 우선순위 | 상태/공통 오류와 commit/abort adapter를 A/B에 공급. SELECT 종료·명시 validation·DDL 두 결과는 해당 소비와 연결 |
| 4.14 자원/취소 | 큼, 합산 peak·실패 cleanup | memory/context/engine 인계 먼저; disk/FD/temp는 sorter 전 공급. cleanup 예약을 마지막 통합으로 미루지 않음 |
| 4.15 CLI | 중간, streaming·출력·EOF/SIGINT | embedded facade는 B의 해당 공급부터, 표시/전체 batch 인수는 뒤. CLI 전체가 worker의 선행이 아님 |
| 4.16 시험 도구 | 큼, 독립 oracle·상태 탐색·관측 | literal/inventory·모델·typed adapter·외부 ACK를 각각 공급. 모델 완주와 runtime verdict를 별도 완료 |
| 4.17 복구 인수 | 큼, 실제 cut 도달·재장애·fixture | R1/R2/R3부터 각 이력 인수. R4/restore와 남은 문장·index 변형은 실제 소비 가능 시 추가 |
| 4.18 계측/수합 | 중간~큼, 환경/fixture·비교 비용 | 계측은 첫 기능, PF10은 각 군의 동등 결과 확보 뒤. closure 문서와 실측 실행 시간을 분리 |

즉시 줄여야 할 위험은 **상태/오류·native 공급·외부 ACK 연결**, 조기 설계 확인이 필요한 큰 작업은 **sorter의 최대 record/merge 예약과 독립 상태 공간**이다. 후자의 설계 준비를 A부터 하되 구현 전체·모델 완주를 첫 INSERT의 추가 장벽으로 만들지 않는다. 최적화·shared cache·새 범용 실행 framework는 현재 범위에 추가하지 않는다. 실측 전에는 이 배치가 유일한 최선이라고 단정하지 않으며, 첫 공급의 API 부족·검토 재작업·harness 준비 비용을 기록해 다음 배정을 조정한다.

<a id="test-scope"></a>
## 22. 인수 연결과 시험 선택

이 문서는 시험 실행을 요청하거나 새 전수 예외를 만들지 않는다. [AGENTS.md의 사용자 지시](../../AGENTS.md)가 기존 V2/V3·하위 회귀·최종 closure 문구보다 우선한다. DEV3 최종 S5의 전수 예외는 **DEV3에만** 적용되며 DEV4/Q1·DEV0/1/2·전체 workspace로 확대하지 않는다.

- 기능 구현과 함께 해당 테스트·fixture·독립 oracle·fault harness를 추가한다. 실행 전 마지막 검증 이후 실제 코드 diff와 선택 case의 관계를 기록한다.
- 변경된 코드의 필요한 사례와 새 통합 경로를 검증한다. 상위 기능 추가·commit 변경·전체 fingerprint 변경·최종 인수 수합만으로 완료된 무변경 코드를 재시험하지 않는다.
- T01–14의 필수 inventory는 coverage 의무로 유지한다. 4.18.3은 기존 결과의 원래 source/범위와 미실행·실패·변경 영향을 대조하여 증거를 수합하며 전체 wrapper 재실행을 뜻하지 않는다. 필요한 신규/미완료 검증은 관련 기능의 diff·잔여 의무를 명시하여 선택한다.
- 기존 FAIL·중단·NOT_RUN은 보존한다. 원래 결과를 현재 실행 PASS로 덮어쓰거나 변경된 경로를 옛 PASS로 충족시키지 않는다. 미충족 인수가 있으면 Q1 미완료로 남긴다.

4.16.1/3에서 T별 담당을 펼칠 때 다음 교차 항목을 빠뜨리지 않는다: T03↔4.10 DDL 두 결과, T04/09↔4.09 즉시 잠금 observation, T07/08/11/14↔4.14 합산 pressure, T10/11↔4.17 R1–R4, T12↔4.15 typed 출력, T13↔4.16 known-bad, T14↔4.18 실제 disk 비용. Oracle differential은 선택 연구이며 XADB literal/history oracle를 대체하지 않는다.

F04의 case inventory는 각 case의 주 담당 실행 ID 하나, 소비 작업, 최소/확대 변형, 관련 코드 diff, 명령·fixture·oracle·원래 receipt를 연결한다. 같은 통합 결과를 소비자들이 참조할 수 있지만 실행 건수·공수는 중복 집계하지 않는다. 부모 T/V 요구를 자식으로 펼칠 때 미배정/NOT_RUN 변형도 남긴다. 유한 상태 모델은 4.16.1/3, R1/R2/R3/R4는 각각 4.17.2.1/.2/3.1/3.2, PF10-A–E는 4.18.2.1–5에 연결한다.

003의 좁은 배정에서 parser T01은 4.03의 각 자식+4.03.3, 공통 오류 vector는 4.13.1.2, literal/coverage는 4.16.1.1/3.1, 유한 모델은 4.16.1.2/3.2, ACK 관측은 4.16.2.2→4.17.1로 연결한다. 기존 부모 ID 참조도 유효하며 이 세분화로 T/V/F·R1–R4/PF10의 필수 변형을 삭제하지 않는다.

<a id="decision"></a>
## 23. D4-WORK-BREAKDOWN-001 기록

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22. 기존 owner 계약의 공급·인수 배정 구체화; 새 SQL 의미·API 형식·수치 결정 없음 |
| 권한·담당 | 사용자의 “DEV3처럼 DEV4도 세부 항목 분류, ARIES·Oracle·로컬 DBMS 참고, 완료 뒤 commit” 요청. 작업/계약/통합 `/root` |
| 기준·기존 변경 | 조사 초기에 있던 DEV3 변경은 별도 commit `def35fb1e274040eb55d78fa68948ba1610166c1`에 반영된 상태를 확인한 뒤 편집 시작. 기준 worktree clean; 사본 `/tmp/xadb-dev4-breakdown-1hhjny89`. 이후 동시 DEV3/DEV5 변경을 관측했으며 공유 파일에서는 이번 DEV4 추가분만 commit 대상으로 분리. 기존 내용·과거 조사 identity 보존 |
| 선택·대안 | 기존 8단계만으로는 prepared/DDL/rollback/cleanup의 공급·실패 경계가 한 작업에 섞여 18개 책임·하위 단위로 연결. 18개를 번호순 완료하거나 DEV3의16개 개수에 맞추는 방식은 채택하지 않음 |
| 조사 | [2026-10-22 기록](../evidence/dev4-source-research.md#work-breakdown-20261022): Oracle19c·ARIES 원문 일부, PostgreSQL/MariaDB/CUBRID/SQLite 실제 revision·파일 hash·열람 범위. 외부 코드 도입·DBMS 실행 없음 |
| 범위·관계 | 새 작업 계획·기존 DEV4 계획/조사·진입/폴더 색인·documents.json·DEV4 단계/완료의 design_refs만 연결. XADB-011/D4-0–7·Q1·정본 SQL 계약·run_entrypoint/evidence/execution status 유지 |
| 검토·검증 | `/root/dev4_breakdown_review`가 새 계획·관련 DEV4 9개 문서 변경과 정본을 읽기 전용 D2 검토: PASS, 필수 수정 잔여 없음. 4.10.1/3의 Idle prior=NotPresent와 Active precommit 조건을 구분한 수정 재확인, 공급 DAG·T/V/F 연결·수명·시험 선택·NOT_RUN 보존 확인. 네 checkout identity와 11개 파일 hash·선택 소스 경계도 직접 대조; Oracle/ARIES는 조사자의 열람 기록 검토 범위. `/root` 반영 확인. 제품 코드 변경·제품 시험·compiler/Clippy·문서 자동 검사·push 미실행; 문서 검사는 DOC-002의 push gate에서 수행 |
| 재검토 | 공급 순환·미배정 인수·실제 API와의 불일치, SQL/DEV3 owner 변경, 자원 회계·DDL 순서·rollback 의미 혼동. 의미를 바꿀 필요가 있으면 해당 owner의 별도 D2로 처리 |

001 당시 분류와 검토 identity는 유지한다. 후속 실행 단위·분배/우선순위의 구체화는 [002 기록](#work-review-002)을 따른다.

<a id="work-review-002"></a>
## 24. D4-WORK-BREAKDOWN-002 — 분량·초기 통합·인수 배정 보완

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22. 작업 배정·공급 우선순위 구체화; SQL 의미·API 형식·수치 한도 변경 없음 |
| 권한·담당 | 사용자의 ARIES·Oracle·로컬 DBMS 기준 분배 검토와 후속 “수정해 주세요, 작업이 완료되면 commit”. 작업·계약·통합 `/root`; 별도 검토는 아래 실제 결과로 기록 |
| 기준·기존 변경 | HEAD `9f79b31b99f558f4e9a98b8bd752220b851e7bbb`. 기존 devdoc/README.md·documents.json의 DEV6/7 순서 변경은 보존·이번 commit 제외. 기준 사본 `/tmp/xadb-dev4-work-review-base-6ist8e81`, 최초 identity.json SHA-256 `38ab0af8fd801dbb90a79539414cb64004edd208d4a1d77612a4533696863b58`; stage-completion.json 사본은 후속 추가. 작업 중 DEV3/5/6/7 변경과 DEV3 commit `13968dd`를 관측했으며 공유 파일의 DEV4 변경만 분리하여 반영 |
| 선택·대안 | 18개 책임·55개 ID 유지, 큰 부모 아래 실행 단위 추가. INSERT와 대상 조회, DML commit과 SELECT 종료, R1–R4·restore·PF10 각 군을 분리. 전체 SQL 완성 뒤 복구/계측을 붙이거나 동일 개수로 공수를 나누는 안을 채택하지 않음 |
| 상태·정렬 | 4.13 수명 통합 담당과 4.16 독립 상태 모델/known-bad 배정. F03에서 DEV3 sorter 재사용·분리의 실제 경계를 비교·별도 검토; 공통 구현 채택은 미완료 |
| 근거 | [후속 실제 조사](../evidence/dev4-source-research.md#work-review-20261022): ARIES 원문 일부·Oracle19c·로컬5개 DBMS. 최적 기간/작업 수의 실측 증명은 아님 |
| 범위·순서 | 작업 계획→기존 구현 계획·stage-completion의 동결/배정 연결→조사/색인. 기존 문서 ID·경로·title 유지하여 documents.json mapping 변경 불필요. 제품·DEV3 owner·기존 실행 evidence·run_entrypoint/status는 변경하지 않음 |
| 검토·검증 | `/root/dev4_work_review`가 DEV4 7개 문서 변경과 SQL·DEV3 owner 정본을 읽기 전용 D2 검토. 4.13.2.1의 Idle ON count와 명시/OFF statement 성공·Active 유지 조건을 구분한 P2 수정 재확인 후 채택 가능, P1/P2 잔여 없음. 공급 DAG·인수 배정·상태/자원 소유·시험 범위·NOT_RUN 보존 확인, 로컬5개 checkout identity·10개 파일 SHA-256·명시 열람 범위를 직접 대조. Oracle/ARIES는 조사자의 열람 기록 검토 범위. 동시 DEV3/5/6/7 변경 및 DEV5 공급 절은 제외하며 `/root`가 반영 확인. 제품 코드 변경·제품 시험·compiler/Clippy·문서 자동 검사·push 미실행. 사용자가 후속 push 요청을 취소하고 DEV4만 완료·commit하도록 범위를 확정했으므로 전체 문서 자동 gate는 실행하지 않음. 문서 검사는 DOC-002의 향후 push gate에서 수행 |
| 대체·재검토 | 001의 대분류·ID·범위와 당시 검토 기록 보존; 현행 실행 배정/우선순위는 002로 구체화. 실제 API/소유·원자성 불일치, 배정 누락·선행 순환, sorter 재사용의 의미 차이, 초기 공수 추정과 실제 기록 차이에서 재검토 |

002의 당시 검토·identity는 보존하고 후속 선행·분량·공통 책임 보완은 아래 003에 기록한다.

<a id="work-review-003"></a>
## 25. D4-WORK-BREAKDOWN-003 — 앞단 선행·오류 소유·검증 준비량 재점검

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22. 작업 공급·분량 판단 구체화, 기존 SQL 의미·API 형식·수치·DEV3 진입 조건 유지 |
| 권한·담당 | 사용자의 DEV4 분배·분량·우선순위·내용·분류 검증, ARIES·Oracle·로컬 DBMS 대조, 개선 즉시 수정과 완료 뒤 한 번 commit 요청. 작업/계약/통합 `/root` |
| 기준·기존 변경 | 시작 HEAD `f6c965b9b133ffc083532215f98905f89e00c26f`. 사본 `/tmp/xadb-dev4-review3-base-h2e4vrcn`, identity SHA-256 `d71902f56e2392a51e64060ec2718a1fae3e482d25badfdee90734b9ab0f653b`. 기존 DEV3/5·공유 색인 변경은 보존. 작업 중 DEV5 commit `49bb781`과 후속 DEV3/6/7 변경을 관측했으며 그 변경·DEV5 공급 절을 이번 검토·commit으로 재승인하지 않음 |
| 선택·대안 | 18개 책임 유지. 공통 parser→INSERT/거래와 predicate→SELECT/UPDATE/DELETE를 분리, bound dependency 검사에서 RowScan 전체 선행을 제거. 4.13에 공통 SQL 오류 변환을 배정하고 4.16의 모델/관측/ACK/판정 준비를 별도 공급. 18개 균등 공수 배정·작업 수 맞추기·전체 front end/worker 완료 후 작은 영속 경로 착수는 채택하지 않음 |
| 근거·한계 | [003 실제 조사](../evidence/dev4-source-research.md#work-review-003-research): ARIES의 partial/total/restart undo·commit WAL, Oracle19c의 처리 단계·문장 원자성, 로컬5개 DBMS의 실제 revision·6개 파일 선택 열람. 분류 최적성·기간·병렬 속도 향상의 실측 증명은 없음 |
| 소유·분량 | 공통 오류 owner는 4.13, phase/cleanup 공급은 각 기능, 관측 전달은 4.16, 복구 cut/판정은 4.17. [18개 상대 부담](#workload-review)과 첫 종료점을 명시. 실제 담당/파일·공수 범위는 구현 착수 때 기록 |
| 범위·검증 | 작업 계획·기존 구현 계획·기존 조사·세 README의 DEV4 안내. 기존 ID/title/path 유지로 documents.json 신규 등록 없음. 제품 코드·시험·compiler/Clippy·문서 자동 검사·push 미실행; Q1 NOT_RUN·DEV3 미완료 보존. 자동 문서 검사는 DOC-002 push gate에서 수행 |
| 독립 검토 | `/root/dev4_review3`가 DEV4 계획2개·조사1개·README3개의 해당 변경과 SQL/정책 정본을 읽기 전용 검토: 채택 가능, 잔여 P1/P2 없음. 동일 guard·오류/cleanup·DDL 두 outcome·인수/NOT_RUN 보존 확인. F04 준비→동결→구현 구분과 각 기능의 오류 공급·lexer/parser의 session runtime 역의존 금지 보완을 재확인. 로컬5개 HEAD/origin/tracked 상태·6파일 hash/명시 범위, ARIES PDF hash·인쇄97–100쪽·Oracle 공식 두 문서를 직접 대조. 동시 DEV3/5/6/7 변경·DEV5 공급 절 제외, 제품 시험·문서 자동 검사·compiler/Clippy 미실행. `/root`가 반영 확인 |
| 대체·재검토 | 001/002의 기존 의미·당시 검토는 유지하며 현행 좁은 공급·책임은003으로 구체화. 실제 선행 순환/오류 분기 누락·DEV3 API 변화·추정과 실제 비용 차이 때 해당 작업 배정을 재검토 |
