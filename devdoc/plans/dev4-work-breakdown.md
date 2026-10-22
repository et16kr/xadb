# DEV4 — 세부 작업 분류와 공급·인수 계획

2026-10-22 · **18개 분류·55개 하위 작업 / D4-WORK-BREAKDOWN-001 ACCEPTED / 제품·Q1 NOT_RUN**

[SQL 의미 정본](../contracts/dev4-sql-design.md) · [기존 구현·검증 계획](dev4-implementation-plan.md) · [이번 ARIES·Oracle·로컬 소스 조사](../evidence/dev4-source-research.md#work-breakdown-20261022) · [단계 완료 정본](stage-completion.json) · [DEV3 분해 방식](dev3-implementation-plan.md#remaining-work)

이 문서는 기존 DEV4의 D4-0–7을 실제 배정 가능한 하위 작업으로 나눈다. 모든 작업은 기존 XADB-011 내부이며 DEV3 → DEV4, Q1, SC02·KU13·PF10과 채택된 SQL 의미를 유지한다. 번호는 실행 순서·진행률·새 gate가 아니다. **현재 4.01–4.18의 제품 구현·실행은 모두 NOT_RUN이며, 분류 완료는 leaf 입력 동결이나 DEV4 착수·완료를 뜻하지 않는다.**

DEV3처럼 큰 책임을 분류하고 각 책임을 좁은 공급 단위와 소비 경로의 통합 인수로 나눈다. 의미·API의 소유자는 기존 SQL/engine 계약이고, 이 문서는 작업 범위·직접 선행·산출물·인수 연결을 소유한다. 실제 Rust signature·spill bytes·runner·수치 profile은 D4-F01–F05에서 아직 동결해야 한다. 이번 요청으로 제품 구현·시험 실행을 재개하지 않는다.

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
| 입력 동결 | 관련 F 항목, 타입/오류/소유·byte vector, 독립 기대값, 자원 계산, 실제 case ID·fault cut·thread barrier·명령·fixture hash |
| 구현 동반 | production 변경과 그 변경에 해당하는 테스트·fixture·oracle·harness. 독립 oracle에 production 의미 계산을 복사하지 않음 |
| 제한 공급 | 실제 구현한 경로와 선택 시험 결과, source/binary/fixture identity, 미연결 소비자·미실행 변형. parser PASS를 SQL 실행 PASS로 표시하지 않음 |
| 통합 완료 | 새로 연결·수정한 경로의 원자성·오류·수명·자원 인수와 별도 검토, 남은 의무·실패 원본. 다른 분류 전체 완료를 암묵적으로 승격하지 않음 |

설계·literal 기대값 준비는 DEV3와 독립적으로 할 수 있다. production 착수는 [기존 DEV4 진입 조건](dev4-implementation-plan.md#entry)의 실제 DEV3 최소 공급 증거와 해당 leaf 동결 뒤다. 공급 API가 부족하면 engine owner에게 최소 계약 보완을 요청하고 SQL의 private page/WAL 접근·UNIQUE 재구현·분할 commit으로 우회하지 않는다. 분류마다 새 crate를 만들지 않으며 실제 파일·빌드 등록은 구현과 함께 한다.

<a id="task-01"></a>
## 3. 4.01 — 저장 공급 수신·입력 동결

정본: [진입 조건](dev4-implementation-plan.md#entry), [F01–F05](dev4-implementation-plan.md#freeze).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.01.1 공급·차이 목록 | 현행 SQL/DEV3 owner 계약·실제 구현 기록 | catalog/DDL, scan/point/range·owned row, token/observation·undo, commit/fencing, temp 효과 port 각각의 API·source·실행 범위와 미충족을 구분. 표현·유한 모델을 실제 disk 증거로 오인하지 않음 |
| 4.01.2 공통 경계 동결 | 4.01.1, 해당 실제 DEV3 공급 | F01/F02의 AST/bound/metadata·error/outcome·session/result/token 소유 경계를 소비 leaf별로 동결·독립 검토. 존재하지 않는 API 이름을 callable로 등록하지 않음 |
| 4.01.3 실행 입력 수합 | 해당 4.08/4.16/4.18의 준비 산출물 | F03 spill bytes·F04 관측/fixture·F05 자원/성능 profile의 담당·완료/잔여를 관리. 아직 쓰지 않는 모든 미래 입력을 lexer의 선행으로 만들지 않음 |

<a id="task-02"></a>
## 4. 4.02 — 어휘·이름·문장 경계

정본: [유한 문법과 이름](../contracts/dev4-sql-design.md#grammar).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.02.1 token·span | 해당 F01, 4.14.1의 입력/할당 예산 | UTF-8·keyword·정수/text/binary·quote·주석, 원문 byte span/EOF와 잘린 입력의 literal vector. SQL 입력1 MiB·token65,536 경계 |
| 4.02.2 identifier 정규화 | 4.02.1 | ASCII 대문자·quoted bytes·PUBLIC prefix, 길이/control 거절과 V18. native catalog bytes의 전역 정규화는 하지 않음 |
| 4.02.3 단일 API·batch 경계 | 4.02.1 | API 다중 문장 거절, CLI streaming 분리 입력 공급. literal/comment의 `;`·`?`를 구분하고 batch의 빈 구간·미종료 입력 위치 보존 |

<a id="task-03"></a>
## 5. 4.03 — bounded parser·AST·오류 위치

정본: [문법](../contracts/dev4-sql-design.md#grammar), [자원](../contracts/dev4-sql-design.md#resources).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.03.1 SELECT·predicate AST | 4.02.1/2, F01 | NOT/AND/OR 우선순위·괄호·비교·IS NULL·projection/ORDER BY/LIMIT. AST8,192·모든 재귀 nesting64의 경계와 단락 이전 전체 구문 검사 |
| 4.03.2 DML·DDL·거래 AST | 4.03.1의 공통 parser, 4.02.3 | 네 DDL·세 DML·거래 제어의 지원 production, marker 허용 위치. DDL type_length는 구문과 precommit 이후 값 검사를 분리 |
| 4.03.3 진단·malformed 입력 | 4.03.1/2 | Syntax/Unsupported·원문 span/EOF, 잘린 token·알려진 후속 구문·한도±1 corpus. parser fuzz 입력/기대 거절은 구현과 함께 작성하며 Q1 완료로 승격하지 않음 |

<a id="task-04"></a>
## 6. 4.04 — SQL 값·NULL·predicate

정본: [값·타입](../contracts/dev4-sql-design.md#values), [SQL/native 빈 text 경계](../contracts/dev4-sql-design.md#text-boundary).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.04.1 typed scalar 입력 | 4.02.1, F01 | Int64 min/max±1, Bool/Utf8/Binary·typed/untyped NULL 구분, checked 길이·대입 한도. 암묵 cast·절단 없이 오류 |
| 4.04.2 text 정규화·대입 검사 | 4.04.1 | 원래 타입 확인 뒤 SQL literal/bind의 빈 Utf8만 typed NULL. native 빈 Utf8·빈 Binary·공백/NUL 보존, NOT NULL·목적 열 길이와 V19/20 |
| 4.04.3 3값 predicate·비교 | 4.03.1, 4.04.1/2 | 독립 T/F/U 표와 모든 지원 타입 비교·IS NULL. WHERE는 TRUE만 통과하며 key total order·sort NULL 위치와 SQL 일반 비교를 구분 |

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
| 4.06.3 실행 view에서 유효성 검사 | 4.06.1/2의 해당 plan, 4.13.1 | 동일 engine guard에서 DB/timeline/open·table/index identity/정의 대조 후 첫 row 접근 전 StalePlan. data commit만으로 stale 금지, 자동 reprepare·이름 재검색 금지 |

index path의 실제 결과는 4.07.3에서 literal·native·force-scan과 대조한다. planner 단독의 plan 모양 검사는 그 결과 동등성을 대신하지 않는다.

<a id="task-07"></a>
## 9. 4.07 — SELECT pull 실행

정본: [SELECT 실행](../contracts/dev4-sql-design.md#execution).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.07.1 scan/filter·owned row 공급 | 4.06.1/3, 4.04.3, 4.13.1, 4.14.1 | statement token·snapshot+own writes 고정, native cursor open/next/close·EOF, 외부에 page guard를 빌리지 않음 |
| 4.07.2 projection/LIMIT | 4.07.1, 4.05.2 | 미선택 ORDER BY 열을 sort 입력까지 유지, LIMIT0/1/최대·중복 projection·bag multiplicity. sort 없는 결과를 제한된 owned batch 소비자에게 공급 |
| 4.07.3 index·sort·result 연결 | 4.07.2, 해당 4.06.2/4.08.2/4.12.2 | point/range의 missing/duplicate·own insert/update/delete·undo read 및 NULL 다중 결과. ORDER BY는 sort 완료 뒤 결과, fetch/조기 close의 snapshot 수명까지 인수 |

4.07.1/2의 기준 cursor를 4.09가 먼저 소비할 수 있다. 4.07.3 전체 완료를 4.08·4.12 기반 구현의 선행으로 삼지 않는다.

<a id="task-08"></a>
## 10. 4.08 — ORDER BY·외부 정렬·임시 파일

정본: [정렬·자원](../contracts/dev4-sql-design.md#resources), [합산 예약](../contracts/dev4-sql-design.md#aggregate-pressure).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.08.1 comparator·run 표현 | 4.04·4.06.1의 해당 표현, 4.14.1, F03 | ASC/DESC·기본/명시 NULLS·복수 열·내부 RowId tie-breaker. key+RowId+projected 값의 독립 spill bytes·header/version/length/checksum/count 동결 |
| 4.08.2 bounded run/merge | 4.08.1, 실제 temp 효과 port·F03 | 4 MiB sort·256 KiB record·fan-in8·FD/디스크 예산 안의 run/다중 merge·disk manifest 순회. 입력+출력 공존 peak·short I/O/ENOSPC/손상 거절 |
| 4.08.3 소유 cleanup·SQL 연결 | 4.08.2, 4.12.2, 4.14.2/3 | fetch 완료/취소/close·unlink 실패 charge, lease 이후 소유 namespace만 crash cleanup. 다른 query의 파일 보존, 새 foreground 예약 없는 정리와 실제 ORDER BY 오류 결과 |

작은 LIMIT의 전체 sort 비용, 사용자에게 보장하지 않는 tie 순서, query256 MiB/DB1 GiB·FD10/64 등은 기존 정본을 유지한다. PostgreSQL/SQLite sorter 비교는 분해 근거이며 해당 temp format·threading·top-N을 도입한 결정이 아니다.

<a id="task-09"></a>
## 11. 4.09 — DML 대상 확정·단일 statement

정본: [DML·문장 최종 UNIQUE·observation](../contracts/dev4-sql-design.md#execution).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.09.1 대상·값 선확정 | 4.05, 4.07.1/2, 4.13.1·4.14.1의 해당 공급 | INSERT 열/tuple·omitted NULL 검사, UPDATE/DELETE의 RowUID·원래 값·opaque observation 목록. cursor는 mutation 전에 닫고 같은 statement token 유지. 남은 거래 한도 사전 검사 |
| 4.09.2 native mutation·finish 연결 | 4.09.1, 실제 DEV3 mutation/UNIQUE/undo 공급 | UPDATE/DELETE 대상 RowId 순서·한 번씩, 한 execute_statement→finish_statement. INSERT VALUES 순서, 동일 값 UPDATE도 affected count. 최종 UNIQUE·모든 index 원자성은 engine 결과로 판정 |
| 4.09.3 실패·충돌·잔류 회계 | 4.09.2, 4.14.2 | 중간 key/quota·최종 UNIQUE 실패에서 앞선 statement 보존, partial count 성공 금지. 즉시 X-lock에서도 observation 검증→전체 abort 구분. V26/27·256/257·반복 실패 후 잔류 charge와 전체 abort 예약 확인 |

SQL의 자체 역 DML·hidden retry·여러 commit 분할은 금지한다. runtime rollback 의미는 여기서 인수하고 crash 중 rollback·CLR 재실행은 4.17에서 별도로 연결한다.

<a id="task-10"></a>
## 12. 4.10 — DDL 정의·precommit·두 결과

정본: [DDL 경계](../contracts/dev4-sql-design.md#ddl-boundary), [타입·제약 정의](../contracts/dev4-sql-design.md#values).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.10.1 precommit 이전 진입 판정 | 4.03.2/3, 4.13.1, 해당 F02 | 지원 문법·namespace·session/handle·ResultBusy·사전 취소만으로 진입 판단. 진입 거절은 기존 상태 유지: Active이면 prior/ddl 모두 NotAttempted, Idle이면 prior=NotPresent·ddl=NotAttempted |
| 4.10.2 새 catalog의 DDL lowering | 4.05.1/2의 공통 binding, 실제 DEV3 원자 DDL 공급 | precommit 뒤 호출하는 semantic 검증. CREATE/DROP TABLE/INDEX·PK nullable 강화·소유 제약 index 보호·부분 NULL·index/key 한도. PK/UNIQUE 포함 create는 하나의 native DDL |
| 4.10.3 이전 거래 종료→독립 DDL | 4.10.1/2, 4.13.2, 4.14.2 | Active이면 prior=Committed 뒤 새 catalog 검증/DDL, Idle이면 prior=NotPresent로 독립 DDL. 이름·길이 오류도 이미 commit된 DML 유지. prior의 NotPresent와 두 거래의 NotAttempted/Committed/Aborted/NotCommittedRecoveryRequired/Unknown·receipt를 정본의 적용 범위대로 반환; V21/22 |

DDL semantic validation을 일반 DML binding 순서에 끼워 precommit보다 앞당기지 않는다. DDL 두 거래 사이·각 commit 응답 경계의 kill/reopen은 4.17.3의 R4에 배정한다.

<a id="task-11"></a>
## 13. 4.11 — prepared·parameter·binding

정본: [typed prepared](../contracts/dev4-sql-design.md#prepared), [교체 peak](../contracts/dev4-sql-design.md#aggregate-pressure).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.11.1 parameter 추론·hint | 4.03/4.04·4.05.2의 해당 공급 | lexical `?` ordinal, 목적/비교/LIMIT 문맥 타입·NULL/bytes, 불명 타입과 충돌 hint 거절. metadata 불변·DDL/거래 제어 prepare Unsupported |
| 4.11.2 owned bind_all | 4.11.1, 4.12.1, 4.14.1 | count/type/UTF-8→빈 text→NULL/길이→old+new 예약→원자 교체. 실패 시 이전 binding 전체 보존, caller buffer 해제·injection 문자를 SQL로 재해석하지 않음 |
| 4.11.3 반복 실행·direct 공통화 | 4.11.2, 해당 4.06.3/4.07/4.09·4.12.2 | 지원 SELECT/DML의 direct/prepared 결과·오류 동등, 매 execute dependency 검사, 실행 사이 binding 유지. DDL/거래 제어는 direct dispatch에 유지 |

<a id="task-12"></a>
## 14. 4.12 — result·handle·fetch

정본: [상태·수명](../contracts/dev4-sql-design.md#prepared).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.12.1 slot/generation 기반 | F02, 4.14.1 | session incarnation·slot generation·closed tombstone의 bounded 표현, 교차 session/engine·slot 재사용 거절. statement당 terminal descriptor1개와 direct용1개 |
| 4.12.2 result·batch 상태 | 4.12.1, 4.07.1/2, 4.13.1, 4.14.2 | session당 활성 result1개, owned fetch 1–128행/256 KiB. RowBufferTooSmall은 위치 보존, EOF/terminal 오류/조기 close 구분·complete 정확성 |
| 4.12.3 종료·반복 수명 통합 | 4.12.2, 4.13.3, 4.14.3 | ROLLBACK/close_session 무효화, current generation close 멱등·과거 handle 거절. V15 반복·V16 거래 유지·V28 호출 context와 pin/FD/메모리 회수 |

4.12.1은 독립 상태 기반을 먼저 공급하며 SELECT·prepared 전체 완성을 기다리지 않는다. terminal 오류에 row·spill·snapshot을 붙잡아 두지 않는다.

<a id="task-13"></a>
## 15. 4.13 — session·statement scope·거래 종료

정본: [session·오류 효과](../contracts/dev4-sql-design.md#transactions), [복구 경계](../contracts/dev4-sql-design.md#recovery-boundary).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.13.1 상태·statement scope 공급 | F02, 실제 DEV3 transaction/snapshot/undo 공급, 4.14.1 | Idle/Active/Fenced·ON/OFF·BEGIN과 API 진입 오류, begin/finish/취소의 token 소유. SELECT/DML이 소비할 최소 경계를 먼저 제공; guard 대신 raw q 재구성 금지 |
| 4.13.2 COMMIT·autocommit 통합 | 4.13.1, 해당 4.09.2/3·4.12.2 | ON DML은 COMMITTED 뒤 count, ON SELECT는 ReadOnly 해제·durable receipt 없음. OFF/BEGIN은 ReadWrite validation·서로 독립 table 성공. commit attempt/sync/공개/응답과 certainty 분리 |
| 4.13.3 ROLLBACK·오류·종료 통합 | 4.13.1/2, 4.14.2/3, 4.12.2 | 복구 가능한 statement 실패의 Active 유지와 전체 abort의 Idle·undo 미완료 Fenced 구분. result 취소 후 undo 확인·소유 인계. UNKNOWN 자동 retry/rollback 금지 |

4.13.1은 DML 전체에 의존하지 않는다. 4.13.2/3에서 그 소비 결과를 인수하므로 session과 executor의 서로 다른 전체 완료를 선행으로 요구하는 순환을 만들지 않는다. 현재 SQL Serializable만 소비하고 Oracle READ COMMITTED를 노출하지 않는다.

<a id="task-14"></a>
## 16. 4.14 — 취소·합산 자원·cleanup

정본: [호출별 context](../contracts/dev4-sql-design.md#operation-context), [자원](../contracts/dev4-sql-design.md#resources), [합산 peak](../contracts/dev4-sql-design.md#aggregate-pressure).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.14.1 admission·회계 기반 | F01/F02·해당 F03/F05, 기존 foundation/effect 계약 | 입력/AST·prepared/binding·workspace·sort·FD/disk의 checked 예약과 owner 수명. DB memory64 MiB·temp1 GiB·FD64는 전체 합산, cleanup 작업 공간도 admission에 포함 |
| 4.14.2 호출 context·취소 경계 | 4.14.1, F02 | execute/fetch/close별 generation/cancel/deadline, 같은 호출 재개는 원래 deadline 유지. parse/row/sort/I/O checkpoint, engine rollback/commit 결과 우선; 완료 호출의 취소가 다음 fetch에 전파되지 않음 |
| 4.14.3 정리·인계 port | 4.14.1/2, 실제 engine abort/fence·temp cleanup 공급 | SQL 자원은 소유분만 반환, engine undo/lock/잔여 예약 소유 인계 또는 fence. Drop hidden commit·새 allocation·panic 금지. 삭제 실패 bytes charge·낮은 자원 cleanup |
| 4.14.4 다중 session 소비 인수 | 해당 4.08/4.11/4.12/4.13의 연결 | 각 query 한도 이내이나 합산 초과, old+new binding·merge 입력+출력 peak, 취소/close/crash가 다른 owner 자원을 회수하지 않음. T07/08/11/14의 기존 합산 반례 |

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
| 4.16.1 독립 literal·coverage 준비 | 채택된 문법/값/오류·T/V 정본 | row bag·3값·허용 tie 집합·명시 순서·serial history, V01–28과 grammar/type/statement 교차 목록. 설계 준비는 제품 선행과 독립; 실제 oracle 코드는 해당 동결 뒤 작성 |
| 4.16.2 worker·관측 adapter | 해당 F04, 공급된 SQL/engine 경로 | SQL/native 입력·row/error/ACK·DDL 두 outcome·RecoveryRequired 관측. 외부 controller에 실제 ACK 보존, protocol/version·명령·source/fixture/seed/schedule identity |
| 4.16.3 부정 판정·coverage 수합 | 4.16.1/2와 해당 기능 | 잘못된 값/NULL/order/ACK·누락 cleanup·잘못된 binary/recipe를 거절. native/SQL 두 production 경로 일치만으로 PASS 금지, bag 중복·incomplete prefix·N/A 근거 보존 |

4.16은 기능 구현과 함께 테스트를 추가하는 공통 책임이다. 모든 제품 기능이 끝난 뒤 oracle를 처음 만들지 않는다. 긴 fuzz/부하/조합의 실행 입력과 검증 강도는 원래 계획을 유지하되 실제 실행 선택은 아래 사용자 지시를 따른다.

<a id="task-17"></a>
## 19. 4.17 — rollback·ACK·disk 복구 통합

정본: [R1–R4](dev4-implementation-plan.md#rollback-recovery), [DEV3 복구 수신](../contracts/dev4-sql-design.md#recovery-boundary).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.17.1 fault·ACK 관측 연결 | 4.16.2, 해당 DEV3 WAL/undo/CLR·복구 실제 증거 | update WAL과 첫 commit record 시도 구별, append/sync/공개/응답·undo/CLR·flush 도달 witness. 소유 worker의 kill·short write/sync 오류·fresh fixture 식별 |
| 4.17.2 R1/R2·rollback 재장애 | 4.17.1, 4.09.3·4.13.3 | S2 rollback 완료→S1 commit ACK면 S1만 보존. rollback 중 commit 미시도 crash면 S1도 포함한 거래 전체 소멸. 낮은 자원·부분 CLR·예약 복원과 V27 잔류 비용, 다른 거래 ACK 보존 |
| 4.17.3 R3/R4·checkpoint/restore | 4.17.1, 4.10.3·4.13.2, 해당 DEV3 checkpoint/backup/restore 공급 | durable 뒤 응답 유실·DDL 두 거래 cut의 별도 identity, 반복 reopen·복구 재장애. row/모든 index/catalog와 current-build 복구 결과·새 ACK 보존을 독립 이력으로 판정 |

ARIES는 partial/total rollback·WAL·redo-only CLR·반복 장애를 구분할 연구 입력이다. SQL이 CLR/undo chain·page 복구·별도 reserve ledger를 구현하는 작업은 추가하지 않는다. DEV3 선행 결과는 원래 source·범위의 증거로 수신하고, SQL 통합에서 바뀐 경로의 검증과 구분한다. kill을 power loss 검증으로 확대하지 않는다.

<a id="task-18"></a>
## 20. 4.18 — 계측·PF10·Q1 수합

정본: [PF10](dev4-implementation-plan.md#performance), [Q1](stage-completion.json).

| 하위 ID·산출물 | 직접 선행 | 완료 조건 |
| --- | --- | --- |
| 4.18.1 계측 입력·profile 준비 | F05·각 연산자/효과 경계 | parse/bind/plan/node ID·rows/loops·storage/lock/gate/flush·sort/peak/commit outcome을 연결할 계획. fixture·분포·host/FS·release·비용/회귀 예산을 실행 전에 동결 |
| 4.18.2 실제 관측·성능 | 해당 4.07–4.15 경로·4.16 판정 | P10-A–E의 native/direct/prepared·scan/index·spill·계측 off/상시/상세. page buffer4배 working set과 실제 I/O, sort memory4배 입력. 성공/시도TPS·conflict/Limit/Fenced·자원·cleanup 분리 |
| 4.18.3 Q1 증거 closure | 관련 4.01–4.17 산출물, 4.18.2 | SC02·KU13·PF10, T01–14/V01–28/R1–4·합산 자원 인수의 원래 source/범위와 잔여 의무 수합·별도 검토. NOT_RUN·미도달·실패가 남으면 Q1 미완료; official distribution은 별도 |

성능 비교는 동일 의미·정답·durability가 전제다. 존재하지 않는 이전 SQL 성능을 기준선으로 만들거나 실패 workload를 표본에서 숨기지 않는다. 분류 수·파일 수·계획 채택을 기능 완료율로 환산하지 않는다.

<a id="sequence"></a>
## 21. 공급 순서와 작업 크기

아래 묶음은 기존 D4-0–7 안의 준비·공급·통합 순서다. 문서/독립 기대값 준비는 앞당길 수 있지만 production 선행 gate는 유지한다. **하위 ID의 좁은 공급**을 소비하며 대분류 전체를 선행으로 읽지 않는다.

| 묶음 | 먼저 준비·공급할 단위 | 뒤에서 닫을 소비·인수 |
| --- | --- | --- |
| A: 공통 입력 | 4.01.1/2, 4.16.1, 4.18.1; 4.14.1–3·4.12.1·4.13.1의 해당 계약/기반 | F01/F02 및 실제 DEV3 공급 확인. F03–05는 해당 소비 전에 동결하며 전부를 lexer의 선행으로 묶지 않음 |
| B: front end | 4.02 → 4.03, 4.04 → 4.05 → 4.06.1/3; 4.11.1/2 | D4-1/2의 문법·값·metadata·오류. DDL 구문/정의 준비와 실제 precommit 조립은 구분 |
| C: 기본 실행 | 4.07.1/2 → 4.09.1–3, 4.12.2 → 4.13.2/3 | D4-3의 read-your-writes·대상 선확정·문장/거래 오류. 기준 cursor로 각 소비자 검증 |
| D: 검색·정렬·사용 수명 | 4.06.2, 4.08.1/2 → 4.07.3·4.08.3; 4.10.3·4.11.3·4.12.3·4.14.4 | D4-4/5의 전체 지원 경로. 4.10.1/2는 먼저 준비하고 DDL precommit은 4.13.2 공급 뒤 통합 |
| E: 외부 관측·인수 | 4.15, 기능별 4.16.2/3·4.17·4.18.2 → 4.18.3 | D4-6/7의 CLI·native/SQL·복구·PF10·Q1 증거. adapter/관측은 첫 해당 경로부터 붙이고 최종까지 미루지 않음 |

4.08·4.09·4.10·4.12–4.14·4.17·4.18은 큰 분류이므로 한 번의 구현 작업으로 배정하지 않는다. 예를 들어 4.08.2의 run 생성과 각 merge/fault, 4.17.2의 R1과 R2는 각각 검증 가능한 추가 단위로 나눌 수 있다. 4.02–4.06도 모든 문법/타입을 한꺼번에 구현해야만 공급하는 방식보다 고정 의미별 unit를 먼저 닫되, 제한 공급을 전체 SQL 지원으로 표시하지 않는다.

이번 작업에서 구현 agent·파일 소유를 배정하지 않는다. 향후 서로 겹치지 않는 lexer/vector 준비·sorter·CLI 표시를 분리할 수 있어도 공통 BoundStatement/오류·session/result·문서 JSON의 변경은 통합 담당이 순차 반영한다. 실제 배정 시 [파일 소유 정책](../policies/parallel-development-policy.md)에 따라 기준 revision·독점/공유 경로·수신 산출물·검토자를 기록한다.

<a id="test-scope"></a>
## 22. 인수 연결과 시험 선택

이 문서는 시험 실행을 요청하거나 새 전수 예외를 만들지 않는다. [AGENTS.md의 사용자 지시](../../AGENTS.md)가 기존 V2/V3·하위 회귀·최종 closure 문구보다 우선한다. DEV3 최종 S5의 전수 예외는 **DEV3에만** 적용되며 DEV4/Q1·DEV0/1/2·전체 workspace로 확대하지 않는다.

- 기능 구현과 함께 해당 테스트·fixture·독립 oracle·fault harness를 추가한다. 실행 전 마지막 검증 이후 실제 코드 diff와 선택 case의 관계를 기록한다.
- 변경된 코드의 필요한 사례와 새 통합 경로를 검증한다. 상위 기능 추가·commit 변경·전체 fingerprint 변경·최종 인수 수합만으로 완료된 무변경 코드를 재시험하지 않는다.
- T01–14의 필수 inventory는 coverage 의무로 유지한다. 4.18.3은 기존 결과의 원래 source/범위와 미실행·실패·변경 영향을 대조하여 증거를 수합하며 전체 wrapper 재실행을 뜻하지 않는다. 필요한 신규/미완료 검증은 관련 기능의 diff·잔여 의무를 명시하여 선택한다.
- 기존 FAIL·중단·NOT_RUN은 보존한다. 원래 결과를 현재 실행 PASS로 덮어쓰거나 변경된 경로를 옛 PASS로 충족시키지 않는다. 미충족 인수가 있으면 Q1 미완료로 남긴다.

4.16.1/3에서 T별 담당을 펼칠 때 다음 교차 항목을 빠뜨리지 않는다: T03↔4.10 DDL 두 결과, T04/09↔4.09 즉시 잠금 observation, T07/08/11/14↔4.14 합산 pressure, T10/11↔4.17 R1–R4, T12↔4.15 typed 출력, T13↔4.16 known-bad, T14↔4.18 실제 disk 비용. Oracle differential은 선택 연구이며 XADB literal/history oracle를 대체하지 않는다.

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
