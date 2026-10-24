# DEV4 — DEV3와 독립적인 SQL 앞단 계약

2026-10-24 · **D4-PREP-001/002·D4-FE-LEX-001/SPLIT-001/PARSE-001/SCALAR-001 ACCEPTED / 독립 앞단·scalar 제한 구현 / 모델·Q1 NOT_RUN**

[SQL 의미 정본](dev4-sql-design.md) · [개발 계획](../plans/dev4-preparation-plan.md) · [Oracle·로컬 소스 조사와 검토](../evidence/dev4-preparation-research-20261024.md)

<a id="scope"></a>
## 1. 적용 범위

DEV4 안에서 저장 엔진에 접근하지 않는 입력 처리·값 계산과 독립 검증 도구를 DEV3 완료 전에 개발할 수 있도록 분리한다. 새 제품 단계나 완료 gate를 만들지 않는다. DEV3 → DEV4의 **실제 엔진 통합·Q1 인수 의존성**은 유지한다. 이 계약은 기존 DEV4 문서의 모든 제품 코드를 DEV3 공급 뒤로 미뤘던 착수 조건을 아래 허용 범위에 한해서 대체한다. 최초 요청의 실행 범위는 계약·계획 확정이었다. 후속 개발 요청으로 [첫 lexer·이름](../implementation/dev4-implementation.md)을 제한 구현했으며 나머지는 아래 범위를 따른다.

| 구분 | 기존 작업 ID | 허용되는 산출물 |
| --- | --- | --- |
| 독립 제품 모듈 | 4.02, 4.03, 4.04 | lexer·이름·문장 경계, bounded AST/parser, SQL scalar 입력·NULL·typed predicate 계산 |
| 필요한 공통 부분만 | 4.14.1.1, 4.14.2, 4.13.1.2 | 앞단 memory 예약·해제, 앞단 호출별 취소/deadline 검사, lex/parse/value 진단 표현. session/engine 오류 변환 전체 완료는 아님 |
| 독립 시험 도구 | 4.16.1.1, 4.16.1.2, 4.16.3.2 및 4.16.3.1의 순수 판정 부분 | literal 기대값·case 목록·독립 의미 계산, 동결한 추상 상태 모델과 known-bad 거절. 실제 SQL worker·ACK 수집은 제외 |
| 설계만 선행 | 4.01, 4.05/4.06, 4.08, 4.11–4.15, 4.17/4.18의 나머지 | 공급 차이표, 후속 API·상태·spill·fault·성능 입력 준비. 아직 없는 engine API의 구현·실행 완료를 주장하지 않음 |

실제 catalog binding, scan/index cursor, 대상 RowId/observation 수집, DML/DDL 호출, transaction/snapshot/undo·commit/rollback, session/result runtime, temp I/O/spill, DB open/create 및 실행 CLI는 기존 [DEV4 진입 조건](../plans/dev4-implementation-plan.md#entry)의 실제 DEV3 공급 뒤에 구현·연결한다. 선행 모듈을 위해 가짜 영속 엔진이나 임시 제품 catalog를 만들지 않는다. 작은 literal row/type fixture는 독립 계산의 입력이며 실제 binding·저장 인수가 아니다.

<a id="oracle"></a>
## 2. Oracle 기준과 XADB의 명시적인 차이

참고 기준 버전은 **Oracle Database 19c SQL Language Reference**로 고정한다. 기존 DEV4가 이미 이 버전을 사용한다. 사용자가 제공한 로컬 `OracleManual`의 19c SQL 교육 자료도 대조하되 공식 문서와 개인 자료가 함께 있는 저장소이므로 설명이 다르면 공식 reference의 해당 조건을 따른다. 최신 Oracle 전체 문법·타입으로 자동 확대하지 않는다. Oracle 문서와 기존 XADB 계약이 다른 부분은 아래처럼 명시하며, 실제 Oracle 실행 비교는 아직 없다. 상세 URL·열람 범위는 [조사 기록](../evidence/dev4-preparation-research-20261024.md#oracle)에 둔다.

| 항목 | 이번 적용 |
| --- | --- |
| 이름 | 미인용 이름 대문자·인용 이름 대소문자 구분을 기준으로 한다. XADB 미인용 이름은 ASCII `[A-Za-z_][A-Za-z0-9_]*`, 인용 이름은 UTF-8·1–128 bytes다. Oracle의 문자집합별 대문자화·`$`/`#`·문자 시작 규칙·ROWID 예외를 그대로 도입하지 않음. XADB의 인용 이름 내 `""`와 control 거절도 기존 독자 계약으로 명시 |
| text/NULL | SQL 빈 Utf8 입력만 **Utf8 typed NULL**로 정규화한다. native non-NULL 빈 Utf8와 빈 binary는 보존. Oracle의 빈 문자열 의미를 참고하되 XADB의 원래 타입 검사를 생략하지 않음 |
| 조건식 | 비교·IS NULL → NOT → AND → OR 우선순위와 3값 논리. WHERE는 TRUE만 통과. Oracle의 AND/OR 왼쪽부터 실행을 보장한다고 설명하지 않음 |
| 문자열·숫자 | 단일 quote와 두 quote escape는 참고. Oracle q-quote·N-string·NUMBER·소수·지수·날짜·암묵 변환은 이번 지원 범위 밖. Int64 범위·UTF-8 byte 비교·명시 타입 유지 |
| 끝 공백 비교 | XADB는 literal·열·bind 모두 Utf8 byte 비교여서 `'a' = 'a '`는 FALSE다. Oracle19c는 두 text literal 비교에 blank-padding을 적용해 TRUE이며, 어느 한쪽이 VARCHAR2/NVARCHAR2이면 nonpadded 비교다. Oracle의 타입별 padding·collation 선택을 도입하지 않음 |
| 문법·namespace | 기존 BIGINT/BOOLEAN/VARCHAR/VARBINARY, `X'...'`, `?`, LIMIT, BEGIN·SET AUTOCOMMIT, 고정 PUBLIC·STORAGE DISK 유지. Oracle SQL 문법의 전체 호환이나 동일 bind 문법을 뜻하지 않음 |
| DDL·거래 | 지원 DDL의 구문 진입과 precommit 뒤 의미 검사를 분리하는 기존 계약 유지. 실제 precommit·두 outcome·UNIQUE·SQL Serializable는 엔진 통합 때 검증. Oracle 기본 격리·CLI 종료 의미로 변경하지 않음 |

현재 제품 의미는 [기존 SQL 정본](dev4-sql-design.md#grammar)이 우선한다. 조사에서 차이를 발견해도 기존 의미를 조용히 바꾸지 않는다. 의미 확장은 별도 D2 및 해당 기능의 독립 인수로 처리한다.

끝 공백 사례는 [Oracle19c 비교 규칙](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Data-Type-Comparison-Rules.html)의 Blank-Padded and Nonpadded Comparison Semantics와 대조한 **문서상 기대값**이다. Oracle 실행 결과가 아니며, XADB literal·명시 타입 row·parameter의 기대값은 같은 byte 비교로 고정한다.

<a id="interfaces"></a>
## 3. 모듈·입출력·소유권

제품 구현 위치는 기존 계획 패키지 `xadb-sql` 안이다. 이 계약만으로 폴더·Cargo member를 만들지 않는다. 최초 실제 코드와 함께 등록하며 선행 구현의 normal 의존성은 `xadb-foundation`만 소비한다. engine/platform/server/transport·OS 파일 I/O·전역 session 설정을 앞단에서 호출하지 않는다. 향후 같은 crate의 binder/executor가 engine을 소비해도 앞단 내부의 독립 경계는 유지한다. 별도 frontend crate·범용 parser framework·외부 parser 생성기 의존성은 도입하지 않는다.

다음은 **구현할 내부 경계의 의미·소유 계약**이다. 호출 가능한 API나 안정 Rust ABI가 아니다. 첫 leaf의 실제 Rust signature·private layout·자원 산식·fixture hash는 구현 전 F01/F04 동결 기록으로 대응시킨다. 이 문서의 채택을 F01–F05 전체 동결로 표시하지 않는다.

| 경계 | 입력 → 출력 | 불변식 |
| --- | --- | --- |
| lexer | 유효성 검사한 UTF-8 source → token kind와 원문 byte span, EOF | catalog 접근·타입 추론·숫자 값 변환 없음. token은 원문 범위로 표현하고 무제한 text 복제 금지 |
| 이름 정규화 | raw name과 quoted 여부 → 정규 이름·원문 span | 미인용 ASCII 대문자, 인용 bytes 보존. PUBLIC 해석은 구문 위치에서만 수행; 일반 이름을 전역 namespace로 바꾸지 않음 |
| parser | SQL bytes·공유 예약 입력·호출 제어 → owned ParsedStatement 또는 FrontendDiagnostic | 정확히 한 문장. caller buffer가 사라져도 성공 산출물은 유효. 실패 시 부분 AST·새 예약 전부 회수, 기존 산출물 보존 |
| batch 경계 인식 | 순차 byte chunk·명시 EOF → 완결 문장 source 또는 NeedMore/진단 | 파일을 직접 열지 않음. UTF-8/quote/comment/escape의 chunk 분할 상태 보존. `;`가 quote/comment 안이면 구분자가 아님 |
| SQL 값 입력 | raw literal 또는 원래 타입이 있는 bind 값·요구 타입/길이 → typed scalar | untyped NULL과 Utf8 typed NULL 구별. bind 전체 교체/metadata 관리는 후속 prepared owner |
| typed predicate | 검증한 typed expression·명시적인 typed row/parameter 입력 → TruthValue | 이름/ordinal의 실제 catalog 해석은 후속 binder. 검증 전 표현이나 다른 타입을 NULL 단락 평가로 통과시키지 않음 |
| 앞단 진단 | 검출 code·검출 모듈 구분·가능한 source span → 할당 실패에도 반환 가능한 작은 진단 | SYNTAX/UNSUPPORTED/NAME/TYPE/RANGE/NOT-NULL/LIMIT/CANCELLED/TIMEOUT, SQL 정본과 동일한 code. lexer/parser/value/evaluator 등 검출 모듈 구분을 보존하고 실제 variant는 F01에서 동결. SQL adapter가 이를 실제 호출 단계와 결합해 phase와 효과를 부여. 순수 진단이 session 상태·DDL outcome·commit receipt를 만들어 넣지 않음 |

ParsedStatement는 source bytes와 필요한 정규 이름/해석 값을 소유하고, AST 내부 참조는 안정된 arena index 또는 검증된 byte 범위로 표현한다. caller buffer 포인터·engine handle·page/root 주소·snapshot q를 저장하지 않는다. 반환 값의 무상 clone을 제공해 예약을 우회하지 않는다. AST 순회·폐기도 입력 길이에 비례한 call stack을 만들지 않는다.

<a id="diagnostic-effects"></a>
FrontendDiagnostic는 순수 함수의 실패 원인·위치를 나타내며 그 함수 자체는 DB 효과를 만들지 않는다. 이를 **호출 전체의 NoEffect나 거래 상태 보존으로 일반화하지 않는다.** SQL adapter가 실제 호출 단계·이미 진입한 transaction/statement·cleanup 결과를 사용해 [SQL 오류 효과 정본](dev4-sql-design.md#transactions)의 phase, statement_effect, transaction_state, certainty, retry_scope와 DDL outcome을 조립한다. 진단 code나 `bind`라는 phase만으로 거래 진입 여부를 추정하지 않는다.

| 소비 문맥 | SQL adapter의 책임 |
| --- | --- |
| transaction/statement 진입 전 lex/parse·binding 개수/타입 등 입력 검사 | lex/parse/bind에 대응하며 NoEffect·기존 Idle/Active와 이전 binding 보존. 실제 거래를 이미 시작한 뒤의 catalog bind/dependency 검사는 이 규칙에 포함하지 않음 |
| execute/fetch 중 값·predicate 평가, LIMIT/CANCELLED/TIMEOUT | 실제 execute/fetch phase를 붙이고 필요한 statement 복원·cursor 종료·snapshot 해제를 완료. ON implicit 거래의 terminal 오류는 거래 종료, 명시/OFF 거래의 복구 가능한 오류는 이번 statement 복원 뒤 Active 유지. 같은 호출에서 처음 만든 OFF 거래도 이 규칙 적용 |
| DDL precommit 이후의 값·타입·길이 검사 | 검출 단계의 phase와 DDL의 두 outcome을 보존. prior Committed 뒤 semantic validation 실패로 native DDL을 호출하지 않았다면 ddl=NotAttempted·Idle. 이전 거래의 Committed를 NoEffect·원래 Active로 되돌리지 않음 |

위 정리는 engine cleanup 완료를 전제로 한다. undo/정리 실패의 RecoveryRequired/Fenced와 이미 알려진 commit 결과가 앞단 진단보다 우선하며, 단순 진단 반환만으로 rollback·거래 종료 성공을 주장하지 않는다. lexer/parser/evaluator는 session runtime에 의존하거나 이 정리를 직접 수행하지 않는다.

<a id="syntax"></a>
## 4. 구문·AST 경계

문법의 단일 정본은 [DEV4 EBNF](dev4-sql-design.md#grammar)다. recursive descent와 명시 우선순위 계층을 사용하며 반복 list·AND/OR 연결은 반복문으로 처리한다. AST는 실행 plan이 아니다.

| 구문 산출물 | 보존할 정보·후속 책임 |
| --- | --- |
| 공통 | statement kind, source span, 정규 이름과 인용 여부, scalar/parameter의 출처. `?`는 실제 marker만 원문 출현 순서의 **1..N** 번호로 배정하며 SQL 정본의 prepared metadata와 동일하게 유지. 내부 배열 index와 혼동하지 않음 |
| INSERT/UPDATE/DELETE | 대상 이름·열/tuple 또는 assignment·predicate. 행·catalog·제약을 조회하거나 실제 mutation count를 성공 결과로 반환하지 않음 |
| SELECT | projection·predicate·order term·limit. ORDER BY 기본 ASC와 NULL 위치는 SQL 정본, 출력 alias/ordinal 해석·index 선택은 앞단 기능 아님 |
| DDL | 이름·열/타입·제약 정의, type_length의 **부호와 decimal 원문**. `VARCHAR(0)`·음수·Int64 초과 길이도 숫자 문법이 맞으면 보존; 범위·객체 존재·PK nullable/UNIQUE 등 의미 검사는 precommit 뒤 owner가 수행 |
| 거래 제어 | BEGIN/COMMIT/ROLLBACK/SET AUTOCOMMIT의 구문 분류만. 거래 생성·종료·설정 적용 없음 |

정수 부호는 lexer의 독립 punctuation이며 parser가 scalar/DDL type_length 위치에서 digits와 결합한다. Int64 최솟값을 양수 i64로 먼저 변환하지 않는다. scalar의 overflow는 RANGE지만 DDL type_length에는 같은 변환을 조기 적용하지 않는다. unsupported 숫자 형태·후속 문법은 해당 구문 위치에서 UNSUPPORTED로 구별한다. 알려진 후속 단어를 모든 identifier 위치에서 추가 reserved word로 바꾸지 않는다.

원문 span은 UTF-8 **byte** `[start,end)`, EOF는 `[len,len)`다. 잘못된 UTF-8는 SYNTAX와 처음 잘못된 byte 범위로 반환한다. 문법적으로 완결됐어도 뒤에 두 번째 문장/불필요 token이 있으면 오류다. optional 마지막 `;` 하나 뒤 whitespace/comment는 허용한다. 공백/comment만 있는 API 입력은 SYNTAX, batch의 빈 구간은 skip한다. batch EOF의 완결 단일 문장은 마지막 `;` 없이 전달할 수 있고 열린 quote/comment는 SYNTAX다. 이후 SQL 실행·EOF rollback은 CLI/engine owner의 후속 책임이다.

공백의 정확한 byte 집합, LF/CR/CRLF와 line comment 종료·batch 표시 좌표, 미지원 token 분류표·여러 오류가 겹칠 때 우선순위는 첫 lexer F01 입력에 명시한다. Unicode whitespace 전체를 지원한다고 추정하거나 native lexer의 기본값에 맡기지 않는다. 이 세부 입력이 미확정이면 해당 leaf는 구현에 들어가지 않는다.

실제 parser가 정상 구문을 반환해도 없는 열·잘못된 비교 타입·DDL 길이 검사를 끝냈다는 뜻은 아니다. 특히 `LIMIT 0`·거짓 predicate가 binder의 전체 이름/타입 검사를 생략하게 해서는 안 된다.

<a id="values"></a>
## 5. 값·타입·조건식

typed scalar는 `Int64 | Bool | Utf8 | Binary`라는 타입과 `NULL | non-NULL payload`를 함께 보존한다. 원문 `NULL`은 문맥 미정 값으로 남길 수 있으나 `''`는 처음부터 Utf8다. foundation의 `Value::Null`만으로 타입까지 표현하지 않으며 SQL wrapper가 타입을 보존한다. 기존 foundation Value/compare/전체 key order의 의미를 변경하지 않는다.

검사 순서는 원래 타입 확인 → SQL 입력에만 빈 Utf8 정규화 → nullability/byte 길이 검사다. BIGINT에 `''`를 대입하거나 Int64 parameter에 빈 Utf8를 전달하면 TYPE이다. native에서 읽은 non-NULL 빈 text는 재정규화하지 않는다. 임의 schema 타입 fixture로 값 계산을 검증하는 것과 실제 binder 인수를 구분한다.

TruthValue는 T/F/U 세 값이다. NOT의 3가지, AND·OR의 각 9가지 조합을 독립 truth table로 판정한다. NULL 비교는 U, IS NULL/IS NOT NULL은 T/F이고 WHERE는 T만 선택한다. 같은 타입의 non-NULL 비교만 허용하고 UTF-8/binary는 byte 순서다. predicate 결과와 index total order·sort의 NULL 위치를 같은 comparator 결과로 혼동하지 않는다. 순수 evaluator도 전체 타입 검증 뒤에만 단락 평가할 수 있다.

<a id="resources"></a>
## 6. 자원·취소·오류

기존 SQL 한도를 유지한다: 원문 1 MiB, token 65,536, AST node 8,192, nesting 64, marker 256, 가변 값 65,536 bytes, 정규 이름 128 bytes. 한도는 서로 독립적인 상한이며 모든 최대값의 조합을 8 MiB 안에서 반드시 수용한다는 약속이 아니다.

- 원문 한도에는 comment/공백을 포함한다. token count는 comment/공백·EOF를 제외하고 punctuation·marker를 포함한다. AST node는 저장하는 statement/clause·식·이름·scalar 노드를 모두 계산하며 list/edge capacity도 별도 byte 회계에 넣는다. 연속 NOT·괄호 등 입력 유래 중첩은 공통 깊이 한도를 소비한다.
- 원문 보관·token/AST arena·decoded payload·진단용 고정 공간을 **할당 전에** checked 산식으로 예약한다. 두 단계의 전체 source copy가 동시에 있으면 둘 다 charge한다. 성공 후 retained AST/source의 예약도 수명 종료까지 유지한다.
- 앞단의 compile/retained 공간은 기존 session compiled/bound 8 MiB 및 공유 DB SQL 64 MiB에서 함께 차감한다. 앞단용 추가 8 MiB/64 MiB를 만들지 않는다. caller가 공유 quota owner를 주입하며 제품 경로에서 호출마다 새 ledger를 만들지 않는다. 아직 DB가 없는 단위 fixture는 동일 계약의 시험 ledger를 명시적으로 제공한다.
- foundation `Quota`/`Reservation`을 소비한다. 여러 quota 중 뒤 예약이 실패하면 앞 예약을 반환하고 기존 AST/binding은 유지한다. quota 승인은 실제 allocation 성공이 아니므로 필요한 `try_reserve` 실패를 typed LIMIT로 처리한다. 실제 capacity와 임시 공존 peak를 포함하고 전역 OOM 복구를 보장한다고 쓰지 않는다. Vec 기반 원문 backing의 allocator 초과분에 한해서는 [D4-FE-SPLIT-001](#splitter-leaf)의 요청량 선예약·관측 초과분 게시 전 보정이 위 할당 전 예약의 구체화다. allocator 내부의 미관측 bytes까지 사전 예약한다는 의미를 대체한다.
- 호출별 cancel/deadline 입력은 새 호출에서 새로 받는다. source 검사·긴 token/주석·decode·parser/평가 loop에 bounded checkpoint를 두며 첫 leaf에서 최대 검사 간격과 allocation inventory를 동결한다. AST에 완료 호출의 cancel/deadline을 저장하지 않는다.
- 순수 앞단 자체의 오류·취소·폐기는 새 foreground 예약·DB I/O·숨은 commit 없이 끝나야 한다. 상위 SQL/engine owner의 statement undo·거래 종료에 필요한 I/O는 [오류 효과 경계](#diagnostic-effects)에 따라 별도로 수행한다. recursion/Drop overflow·unchecked capacity·부분 성공 반환을 금지한다. 전체 parser가 끝나지 않았으면 첫 문장 성공을 전체 입력 성공으로 반환하지 않는다.

<a id="handoff"></a>
## 7. 독립 검증과 DEV3 수신

독립 oracle는 production tokenizer/parser/type 비교·정규화 함수를 import하거나 복사하여 정답을 계산하지 않는다. source·case ID·입출력의 단순 관측 schema는 공유할 수 있다. literal AST 구조·이름 bytes·typed 결과·오류 span과 별도 작은 row bag/3값 evaluator가 기준이다. 상태 모델의 외부 action/outcome은 추상 입력이며 실제 engine ACK나 durability 증거로 계산하지 않는다.

DEV3와의 접점은 catalog/DDL, row/index, statement token/observation/undo, commit/fencing/recovery, temp resource port의 다섯 묶음으로 관리한다. 각 묶음의 실제 symbol·source·선택 시험 receipt·미충족 요구를 4.01.1에 기록한다. 앞단에는 catalog test double를 production adapter로 등록하지 않는다. 실제 연결 착수는 기존 DEV4 최소 저장 선행 증거를 수신한 뒤이며, 이 계약이 checkpoint/restore·반복 복구나 S1/Q1 조건을 면제하지 않는다.

앞단의 원래 단위 증거는 source와 범위 그대로 수신한다. 실제 binder/executor 연결로 변경된 코드·새 연결 경로의 관련 시험만 선택한다. DEV3 완료·전체 source fingerprint 변경·Q1 수합 자체를 이유로 무변경 앞단/하위 시험을 다시 실행하지 않는다. DEV3 S5의 전수 예외는 DEV4에 적용하지 않는다.

<a id="decision"></a>
## 8. D4-PREP-001 결정

| 항목 | 기록 |
| --- | --- |
| 분류·상태 | D2 · ACCEPTED. DEV4 독립 모듈의 착수 경계 구체화; 기존 SQL 의미·제품 품질·영속성 의무 유지 |
| 권한·담당 | 사용자의 DEV3와 병행할 DEV4 계약·계획 확정 요청, Oracle 참고 기준 지정. 작업·SQL 계약·통합 `/root` |
| 선택 | 순수 앞단과 독립 검증 도구의 개발 경로를 열고 실제 engine 통합의 선행은 유지 |
| 대안 | 전체 DEV4 대기는 독립 작업까지 지연. 가짜 engine 기반 executor 선행은 소유권·오류·복구 재작업 위험. 대형 공통 framework는 현재 소비자·필요가 없음 |
| 대체 범위 | SQL 정본 서두, DEV4 계획 entry/sequence, 작업 분류의 공통 착수·순서, bottom-up/stage 설명의 제품 착수 문구에서 본 계약의 허용 범위만 예외. 과거 결정·검토 identity는 보존 |
| 동결 범위 | 허용 ID·의존 금지·Oracle 적용/차이·입출력 소유·구문/값 경계·자원/오류·수신 조건. F01–05 전체 실제 Rust/fixture/profile 동결은 아님 |
| 실행 상태 | 코드·fixture·harness 추가 없음. 제품·모델·Oracle differential·Q1 모두 NOT_RUN |
| 검토·재검토 | [독립 검토 기록](../evidence/dev4-preparation-research-20261024.md#review). engine 접근/SQL 의미 확장·회계 우회·Oracle 차이 누락·경계 변경 시 D2 재검토 |
| 후속 교정 | [D4-PREP-002](#diagnostic-effect-decision)가 원 진단의 phase·NoEffect/상태 보존 문구를 한정 대체하고 Oracle 끝 공백 차이를 명시. 나머지 D4-PREP-001 범위와 최초 검토 기록은 보존 |

<a id="diagnostic-effect-decision"></a>
## 9. D4-PREP-002 — 진단과 호출 효과의 책임 교정

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-24 |
| 권한·담당 | 사용자의 새 계약·계획 리뷰와 후속 개선 요청. 작업·SQL 계약·통합 `/root` |
| 반례·선택 | ON SELECT의 fetch 중 predicate 취소를 원래 상태 보존으로 조립하면 implicit 거래/result가 남음. 순수 진단과 SQL 호출 효과를 분리하고 실제 adapter가 phase·cleanup·outcome을 소유 |
| 대안·이유 | 모든 진단을 NoEffect로 포장하는 방식은 SQL 정본과 충돌. evaluator에 session/engine을 주입하는 방식은 독립 앞단 경계를 깨므로 기각 |
| 대체 범위 | commit `0aa9e070a576c6e92d00818db06dd3d77945c518`의 D4-PREP-001 중 진단 phase 제한과 무조건 상태 보존 문구를 본 문서의 오류 효과 경계로 대체. 순수 cleanup의 I/O 금지 범위를 명확히 하고 기존 byte 비교의 Oracle 차이 사례 추가. SQL 의미 정본·DEV3 선행·단계/시험 정책은 유지 |
| 인수·실행 | 준비 계획의 FE10·FE11·FE14 및 후속 T03/T09/T11 연결 의무. 계약/계획 보강이며 제품·모델·Oracle 실행은 NOT_RUN |
| 검토 | `/root/dev4_contract_reassessment`의 독립 D2 정적 검토 수용, `/root` 반영 확인. [후속 리뷰·반영 기록](../evidence/dev4-preparation-research-20261024.md#review-followup); 실제 API·fixture 동결과 제품 검증은 후속 |
| 재검토 조건 | adapter가 진단에서 상태를 추정, cleanup 미완료를 정상 종료로 보고, phase/outcome 유실, Oracle/XADB 기대값 혼용 |

<a id="lexer-leaf"></a>
## 10. D4-FE-LEX-001 — 첫 lexer·이름 입력 동결

2026-10-24 · **D2 ACCEPTED / 제한 구현·선택 시험·최종 D1 수용**. 사용자의 독립 DEV4 개발·병렬 작업·단위 commit 요청에 따른 F01/F04의 첫 부분이다. 기존 D4-PREP-001/002의 의미·DEV3 수신 의무를 유지한다. 작업·통합·공통 API 담당은 `/root`다. 독립 검토·source/fixture hash·실행 기록은 [첫 앞단 증거](../evidence/dev4-frontend-20261024.md)에 남긴다.

### 책임과 실제 API

`crates/sql`의 첫 제품 코드는 foundation만 normal dependency로 소비한다. `frontend::{diagnostic,control,lexer,name}` 책임을 분리한다. lexer는 호출자가 빌린 source 위의 순차 token scanner이며 원문 복사·token 배열·heap allocation이 없다. 이후 parser가 source/arena를 소유하고 session8 MiB/DB64 MiB의 기존 quota를 예약한다. 따라서 이번 borrowed scanner의 heap allocation 실패/합산 quota 사례는 구조상 N/A이며 **FE11/12 전체 완료가 아니다**. 고정 크기 scanner·진단·128-byte 이름은 stack에 두고 크기를 검사한다. lexer마다 새 Quota를 만들지 않는다.

- `Span { start: usize, end: usize }`, `Diagnostic { code: Code, module: Module, span: Span }`는 작은 Copy 관측값이다. 첫 Code는 Syntax/Unsupported/Name/Limit/Cancelled/Timeout이고 Module은 Lex/Name이다. 후속 parser/value 진단은 해당 구현과 함께 추가한다. effect/outcome/session 상태는 없다.
- `CallControl::new(&Cancellation, &dyn Clock, Option<u64>)`는 호출별 제어를 빌린다. deadline은 해당 clock epoch의 nanoseconds이며 `now >= deadline`은 Timeout이다. 동시 관측 시 Cancelled가 우선한다. clock은 비차단·무할당 구현을 공급해야 한다. 제어를 장기 산출물에 보관하지 않는다.
- `Lexer::new(&[u8], &CallControl) -> Result<Lexer, Diagnostic>`는 원문 한도·전체 UTF-8를 검사한다. `next_token(&mut self) -> Result<Token, Diagnostic>`, `Token { kind: TokenKind, span: Span }`. 성공 EOF는 반복 호출에도 `[len,len)`이다. 오류 뒤 scanner는 같은 진단을 반환하는 terminal 상태다. 다른 호출의 새 scanner는 독립이다.
- `TokenKind`: Keyword(Keyword), Identifier, QuotedIdentifier, Digits, Text, Binary, Plus, Minus, Equal, NotEqual, Less, LessEqual, Greater, GreaterEqual, LeftParen, RightParen, Comma, Dot, Star, Semicolon, Marker, Eof. Keyword variant는 현행 EBNF의 CREATE/TABLE/PRIMARY/KEY/UNIQUE/STORAGE/DISK/BIGINT/BOOLEAN/VARCHAR/VARBINARY/INDEX/ON/DROP/INSERT/INTO/VALUES/UPDATE/SET/DELETE/FROM/SELECT/AS/WHERE/ORDER/BY/ASC/DESC/NULLS/FIRST/LAST/LIMIT/NOT/AND/OR/IS/NULL/TRUE/FALSE/BEGIN/COMMIT/ROLLBACK/AUTOCOMMIT/OFF다. PUBLIC·DEFAULT·RETURNING·PREPARE 등은 일반 Identifier로 두며 parser 위치에서 의미를 판정한다.
- `normalize_name(source: &[u8], token: Token, control: &CallControl) -> Result<Name, Diagnostic>`는 token 범위·경계·kind를 다시 검산한다. `Name`은 고정 `[u8;128]`/길이·원 span·quoted 여부를 소유하며 `as_bytes()`, `span()`, `is_quoted()`만 읽기로 제공한다. heap clone·catalog 접근·PUBLIC prefix 판정은 없다.

### byte 규칙과 오류 우선순위

whitespace는 SP/HT/LF/VT/FF/CR의 여섯 ASCII byte다. line comment는 CR 또는 LF에서 끝나며 CRLF는 두 whitespace로 소비한다. 표시 line/column은 후속 splitter/CLI 범위다. block comment는 첫 `*/`에서 닫히며 중첩하지 않는다. quote는 두 같은 quote로 escape하고 backslash는 특별 처리하지 않는다. contiguous `X'`/`x'`는 Binary token이며 hex 유효성·값 decoding은 후속 scalar 소유다. text/comment 안의 NUL은 lexically 보존한다.

정수 부호는 별도 token이다. decimal/exponent는 이번 scanner가 값으로 변환하지 않고 Digits/Dot/Identifier/부호 token으로 보존하며, scalar 위치의 Unsupported 판정은 parser에 남긴다. `!=`는 Lex/Unsupported와 두 byte span으로 거절한다. 나머지 미등록 punctuation·unquoted non-ASCII는 Lex/Syntax와 해당 첫 Unicode scalar span이다. 알려진 후속 단어를 추가 reserved word로 만들지 않는다.

진입 오류 순서는 호출 취소/deadline → 원문 1 MiB 한도 → 전체 UTF-8 → 좌측부터 lexical 검사다. 원문 초과는 `[1MiB,len)`이며 UTF-8는 `Utf8Error::valid_up_to/error_len`에 대응하는 최초 invalid sequence, EOF에서 잘린 sequence는 남은 byte 범위를 가리킨다. 미종료 quote/comment는 여는 delimiter부터 EOF까지다. token 65,536개를 허용하고 다음 nontrivia token 시작에서 Limit; EOF·comment·whitespace는 세지 않는다. 실패 token은 반환하지 않는다. name 정규화는 Name 모듈에서 빈/128bytes 초과·잘못된 형태·C0(U+0000..001F)/DEL(U+007F)/C1(U+0080..009F)을 Name 오류로 반환하며 span은 전체 원 name token이다. DEL 거절은 같은 control 정책의 명시적 구체화다.

제어 진단은 검사 중인 원문 offset `p`의 `[p,p)`다. scanner 진입은0, UTF-8 검사·lexical loop는 실제 원문 진행 위치, name 진입은 `min(token.start,source.len)`이며 decode 중에는 현재 raw 위치다. token 초과의 span도 다음 nontrivia 시작의 `[start,start)`로 고정한다. name은 caller token을 인증된 값으로 믿지 않는다. 취소/deadline을 먼저 검사한 뒤 역전/범위 초과·잘못된 UTF-8 경계/내용·kind/lexical 형태 불일치를 Name으로 거절한다. 오류 span은 `s=min(start,source.len)`, `e=max(s,min(end,source.len))`다. 이 규칙은 정상 token에서 전체 원 token span과 같다. 미인용 reserved keyword를 Identifier로 위조해도 Name으로 거절한다. name 밖 source의 유효성은 scanner 책임이다. 정규 이름의 identity는 `as_bytes()`이며 span·quoted metadata가 identity를 바꾸지 않는다.

입력 검사·긴 comment/quote/digits/name·keyword 검산은 최대256 source bytes 처리마다 제어를 검사한다(UTF-8 한 scalar 완결을 위한 최대3bytes 추가 허용). 새 token 진입과 반환 전에도 검사한다. heap allocation이 없어 cleanup은 고정 값 폐기뿐이다. 크기 검산은 Lexer512bytes 이하, Name192bytes 이하, Diagnostic64bytes 이하를 요구한다. 성공 결과·EOF라도 반환 전 제어 실패를 성공으로 숨기지 않는다.

비기본 `test-support` feature에만 `CallControl::with_observer(&dyn Fn(Module,Span))`를 제공한다. caller가 빌려 준 observer를 실제 checkpoint에서 취소/deadline 검사 직전에 호출하며 heap allocation이 없다. 기본 제품 build에는 이 field/호출이 없다. 시험은 clock 호출 횟수 대신 실제 진행 offset에서 취소/시간을 바꿔 긴 loop 내부 도달을 검증한다. observer는 정답 계산이나 제품 성공 판정이 아니며 시험 동작을 수행하는 도구다.

### 첫 선택 검증과 잔여

FE01/02의 token·span·정규 이름 literal, FE04의 source/token 경계±1, FE11의 진입/중간 cancel/deadline·실패 뒤 새 호출, FE13의 token/span/name/error/누락 EOF 변조 거절, FE15의 malformed corpus와 고정64seed×128operation 생성 입력을 작성한다. property 최소 profile과 30분 fuzz 확대 profile은 별도 선택하며 실행 증거를 혼합하지 않는다. 실제 fixture/case와 hash는 제품 구현 전에 보존한다. production 의미와 독립인 literal은 `crates/testing/reference`에, 실제 scanner 소비 시험은 `crates/sql/tests`에 둔다. reference의 normal dependency는 SQL/foundation을 추가하지 않는다.

splitter·단일 API 문장 수·AST/depth/marker 번호·scalar 검산·owned 자원·FE14 모델·엔진 통합은 후속 단위다. 첫 scanner가 여러 token/문장을 읽는 것은 단일 SQL API 성공을 뜻하지 않는다. 대안인 전체 원문/token 복사는 아직 parser 소비자가 없어 불필요한 할당과 예약을 만들고, 외부 parser framework는 현재 요구에 비해 의존성이 크므로 채택하지 않는다. 후속 parser·splitter에서 범위/진단/자원 계약을 바꾸면 독립 D2로 재검토한다.

<a id="splitter-leaf"></a>
## 11. D4-FE-SPLIT-001 — streaming 문장 경계와 원문 소유

2026-10-24 · **D2 ACCEPTED / 구현·선택 검증 / 최종 D1 제한 수용**. 기존 P1/FE03·04·11·12·13·15의 splitter 소비 부분이다. root가 계약·공통 자원·통합을, frontend_research가 splitter를, frontend_oracle이 독립 입력·관측 판정을, frontend_review가 별도 D2/D1을 맡는다. lexer 의미·원 시험 증거·DEV3 선행은 유지한다. 실제 동결 hash·원 실패·source별 실행과 검토는 [splitter 증거](../evidence/dev4-splitter-20261024.md)에 보존한다.

### API와 byte 경계

- `Splitter::new(session: &Quota, database: &Quota) -> Self`는 기존 session compiled/bound 및 DB SQL ledger의 handle만 복제한다. `push(&mut self, chunk: &[u8], control: &CallControl) -> Result<SplitStep, Diagnostic>`와 `finish(&mut self, control: &CallControl) -> Result<Option<StatementSource>, Diagnostic>`를 제공한다. 제어를 저장하지 않는다.
- `SplitStep { consumed: usize, outcome: SplitOutcome }`, `SplitOutcome::{NeedMore, Statement(StatementSource)}`. 성공 push는 첫 nonempty 문장 끝까지만 소비한다. consumed는 이번 chunk 상대 byte 수이며 caller는 suffix를 재제출한다. NeedMore는 chunk 전부를 소비했음을 뜻하며 EOF가 아니다. 오류는 terminal이므로 재개용 consumed를 제공하지 않는다. 실패 뒤는 입력·제어와 무관하게 원 진단을 반복한다.
- `StatementSource`는 bytes·예약을 함께 소유하며 `as_bytes()`, `span()`, `terminated()`만 제공한다. Clone·가변 bytes·예약 없는 Vec 추출은 없다. span은 batch 절대 byte `[start,end)`, 내용은 직전 외부 semicolon 직후부터 현재 외부 semicolon **포함**, EOF면 남은 모든 bytes다. leading/trailing whitespace/comment를 보존한다. 표시 line/column은 CLI 후속 책임이다.
- 이 source를 빌리는 lexer의 span은 source-local이다. 상위 adapter는 source span.start를 checked-add해 batch 절대 위치로 옮긴다. splitter가 lexer 진단의 모듈이나 원 local span을 조용히 바꾸지 않는다.
- whitespace·quote doubling·backslash·CR/LF·비중첩 comment는 §10과 같다. 외부 `;`는 괄호 깊이와 무관하게 경계다. whitespace/comment/semicolon만인 구간은 버리고 새 구간을 시작한다. pending `-`/`/`가 EOF에 남으면 일반 nontrivia다. NUL·미지원 punctuation·숫자·binary payload·token 수·문법은 여기서 판정하지 않는다. 정상 UTF-8만 보장하며 parser/lexer가 나머지를 판정한다.
- `finish`만 EOF다. 닫는 quote 후보로 끝난 입력·line comment는 완결, quote escape 뒤 다시 열린 quote·block comment·불완전 UTF-8는 Syntax다. `X'abc`의 열린 quote span은 실제 quote인 `[1,5)`이며 Binary token 시작을 추정하지 않는다. 정상 finish 뒤 반복 finish는 None(새 제어 검사 포함), 그 뒤 push는 빈 chunk도 Syntax terminal이다.
- chunk를 미리 전체 검사하지 않는다. `a;` 뒤 같은 chunk의 invalid UTF-8·초과 bytes는 첫 반환에 영향이 없다. 구분자/escape/UTF-8의 모든 byte 분할에서 동일한 문장·절대 span·terminal 진단을 내야 한다. pending delimiter/quote의 다음 byte는 lexical 상태만 재처리하며 append·UTF-8·offset을 중복 처리하지 않는다.

### 오류·한도·제어

진단 모듈에 `Split`을 추가한다. active 호출은 취소/deadline → 각 소비 byte의 offset checked-add/구간1 MiB 한도 → UTF-8 → 필요한 예약/할당 → 경계 처리 순이다. chunk 전체 길이나 아직 소비하지 않은 suffix로 실패하지 않는다. 반환 전에도 제어를 확인한다. failed 상태는 원 오류가 우선한다. finished 상태는 호출 제어 뒤 finish=None 또는 push=Syntax다.

구간 한도는 trivia·외부 semicolon을 포함하며 빈 구간도 적용한다. 외부 semicolon을 소비·skip/반환한 뒤에만 초기화한다. batch 총량은 제한하지 않고 absolute offset overflow를 checked 처리한다. 첫 초과 byte는 `[p,p+1)`, offset overflow는 `[p,p)`의 Limit다. 예약/할당 실패도 `[p,p)`의 Limit다. 잘못된 UTF-8는 표준 `Utf8Error::valid_up_to/error_len`에 해당하는 첫 sequence 범위, EOF carry는 carry 시작부터 EOF까지다. 열린 quote/comment는 여는 delimiter부터 EOF, finish 뒤 push는 EOF의 zero-width Syntax다.

byte scan과 성장 copy는 최대256 bytes마다 제어를 검사한다. scan은 다음 원문 절대 offset, copy는 현재 구간 시작+복사 완료 bytes의 zero-width span을 보고한다. copy 시작/끝 및 allocation 전후에도 검사하며 allocator 내부 latency의 실시간 상한은 보장하지 않는다. copy 검사 좌표는 이미 scan한 구간으로 되돌아갈 수 있다. 진입·반환도 검사한다. 취소/오류 시 미완결 buffer와 새 예약을 모두 폐기하고 이미 반환한 source는 유지한다. 폐기는 추가 예약·I/O 없이 고정 깊이로 끝난다.

### 공유 예약과 실제 capacity

초기 buffer는 allocation 없이 비어 있다. 첫 byte에256 bytes를 요청하고 이후 기존 capacity의2배(checked, 최대1 MiB)를 요청한다. allocator 실제 capacity가 요청보다 크면 그 capacity를 사용하지만 논리 source 길이는 여전히1 MiB다. 각 성장에서 기존 buffer를 그대로 유지한 채 새 요청 전체를 session → DB 순으로 예약하고 `Vec::try_reserve_exact`로 새 backing을 만든다. 실제 capacity 초과분은 session → DB 순으로 추가 예약한 뒤에만 복사·게시한다. 이는 transport의 기존 accounting과 같은 **요청량 선예약·관측 초과분 게시 전 보정**이다. allocator가 내부에서 잠시 점유하는 전체 bytes를 사전 예약하거나 RSS/OOM 전체를 보장한다는 뜻이 아니다. quota 승인과 실제 allocation 성공은 별개다.

성장 copy 중 old/new 실제 capacity의 합을 charge한다. 새 buffer 게시 후 옛 bytes를 먼저 폐기하고 옛 예약을 반환한다. 부분 실패에서도 새 bytes를 관련 lease보다 먼저 폐기하며, splitter는 기존 미완결 buffer까지 정리한다. backing 생성 직후부터 bytes를 첫 field, 모든 lease를 후속 field로 가진 동일 owner로 관리한다. 추가 예약·제어 실패의 조기 return에서도 이 field Drop 순서를 유지한다. returned source는 shrink/copy 없이 이동하여 실제 capacity charge를 수명 종료까지 유지한다. 빈 구간 skip·정상 빈 EOF·terminal 오류·Drop은 미완결 buffer charge를 반환한다. 모든 capacity/offset 산식과 `usize`→`u64` 변환은 checked다. 고정 bookkeeping은 caller 소유 stack 값이며 별도 heap control cell이 없다. Splitter≤512bytes, StatementSource≤192bytes를 검사한다.

비기본 test-support에서 splitter별 `fail_next_allocation()`은 다음 backing의 fallible allocation 지점에서 Limit를 주입한다. `set_allocation_padding(bytes)`는 다음 요청들의 allocator request에 checked padding을 더해 실제 초과 capacity 보정 경로를 강제한다. 정상 제품에는 둘 다 없다. base 요청만 선예약하고 관측한 전체 capacity를 반드시 추가 charge하므로 시험용 padding도 accounting을 우회하지 않는다. quota 실패는 실제 주입 ledger의 잔여량으로 유도한다. 기존 CallControl observer로 scan/copy의 실제 checkpoint와 동시 old/new 사용량을 관측한다.

같은 test-support의 `StatementSource::capacity_bytes()`와 `Splitter::buffer_capacity()`는 실제 backing capacity의 읽기 관측만 제공한다. allocator의 정확한 capacity 일치를 API 보장으로 가정하지 않고, 요청 이상·quota=실제 capacity 및 실제 공존 peak를 대조한다. offset overflow는 private 상태를 구성하는 splitter 내부 단위 시험으로 실제 checked 경로를 검증하며 제품에 offset 변경 API를 추가하지 않는다.

### 독립 입력·검토와 선택

제품 구현 전에 literal bytes·span·terminated·consumed/NeedMore/EOF·오류를 동결한다. 짧은 quote/comment/UTF-8 입력은 모든 byte partition, 256bytes 이하 literal/실패 입력은 whole/각 one-cut/bytewise로 검증한다. 1 MiB 한도 입력은 whole·명시한 한도/UTF-8 경계 cut만 선택하고 모든 one-cut을 적용하지 않는다. fixture가 실제 대상 길이·partition 건수와 생성 상한을 고정한다. 별도 수작업 기대값의 반복·offset 이동만 허용하며 제품 scanner로 기대값을 만들지 않는다. FE15 최소 생성은64seed×128개 독립 fragment이며 확대 profile은 별도 선택·NOT_RUN으로 보존한다.

자원 입력은 초기256·성장768(256+512)·retained source+다음 source의 잔여−1/동일/+1, session 성공 뒤 DB 실패, 실제 capacity 초과분 양쪽 실패, allocation 주입 실패, scan/copy 중 취소·deadline, 새 호출 제어, 두 splitter의 동일 DB 공유·실제 thread 합산, 모든 종료 후 baseline 회복을 포함한다. known-bad 관측은 quote 내 분할·chunk를EOF로 해석·CR/comment/UTF-8 carry·절대좌표·consumed/terminated·누락/중복 출력·불완전 관측을 거절한다. 자원 해제/검사 제거의 제품 mutant도 별도 source로 거절한다.

무변경 lexer/name의 기존 시험은 재실행하지 않는다. Module::Split 추가의 단순 관측 adapter 연결은 compile로 확인하며 새 splitter/자원 경로를 선택한다. 단일 SQL API의 다중 문장 거절·parser/AST/값·session runtime·Q1은 이 leaf의 완료로 대체하지 않는다. 전체 파일을 모아 반환하는 대안은 caller backpressure와 유지량 상한을 약화하고, 기존 lexer를 매 chunk 재시작하는 대안은 미완결/EOF 및 반복 복사 문제를 만들므로 채택하지 않는다.

<a id="parser-leaf"></a>
## 12. D4-FE-PARSE-001 — 공통 owned AST와 INSERT·거래 구문

2026-10-24 · **D4-FE-PARSE-001 D2 ACCEPTED / 채택 시 제품·시험 NOT_RUN**. P2와 해당 P0/P5 소비 부분이다. root가 AST/budget·자원 시험·통합, frontend_research가 parser, frontend_oracle이 독립 literal/판정·adapter, frontend_review가 D2/D1을 맡는다. P3 값 검산·P4 나머지 구문·실제 engine 연결은 별도이며 아래 임시 미공급 분류를 XADB의 영구 지원 범위로 해석하지 않는다.

### 소유 API와 AST

`Parser::new(session: &Quota, database: &Quota) -> Self`, `parse(&mut self, source: &[u8], control: &CallControl) -> Result<ParsedStatement, Diagnostic>`를 제공한다. Parser는 같은 quota handle만 보관하며 호출마다 새 control을 받는다. parse는 caller bytes를 예약한 source에 복사하고 AST를 소유한다. 성공·오류 뒤 같은 Parser를 재사용할 수 있다. 실패는 이번 호출의 source/AST만 폐기하며 이미 반환한 ParsedStatement를 변경하지 않는다. StatementSource를 소비하는 API·engine/catalog·session runtime은 없다. caller는 splitter/source와 parser에 기존 같은 session/DB ledger를 주입하며 두 source가 동시에 살아 있으면 둘 다 차감한다.

ParsedStatement는 `source() -> &[u8]`, `nodes() -> &[AstNode]`, `marker_count() -> u16`를 읽기로 제공하고 Clone·가변 AST·예약 없는 buffer 추출은 제공하지 않는다. nodes[0]은 root이며 성공 때 최소1개다. public AstNode는 private field와 `kind() -> &NodeKind`, `span() -> Span`, `first_child() -> Option<usize>`, `next_sibling() -> Option<usize>`만 제공한다. index는 해당 nodes slice의 지역 위치이며 engine identity/handle이 아니다. 생성·연결은 private builder만 수행하고 index는 범위 안, 연결은 비순환·중복 없음·root에서 모든 node 도달이다. 별도 edge Vec 없이 각 node가 first/last child와 next sibling을 보관해 O(1) append한다. 폐기와 순회는 재귀 소유를 사용하지 않는다.

이번 NodeKind는 `Insert`, `Begin`, `Commit`, `Rollback`, `SetAutocommit(bool)`, `Target`, `Columns`, `Rows`, `Tuple`, `Name(Name)`, `Scalar(ScalarSyntax)`다. ScalarSyntax는 `Integer { sign: Sign, digits: Span }`, `Text`, `Binary`, `Bool(bool)`, `Null`, `Parameter { ordinal: u16 }`이며 Sign은 None/Plus/Minus다. 이름은 기존 고정 Name을 소유한다. 모든 span은 owned source 안의 local UTF-8 byte 좌표다. raw scalar byte는 node.span으로 접근하며 Integer의 digits만 별도 span으로 보존한다. 부호와 digits 사이 whitespace/comment는 허용하고 전체 scalar span에는 포함한다.

preorder node 순서는 root Insert → Target → 이름(비인용/인용 prefix가 있으면 prefix도 먼저 보존) → Columns → 열 이름들 → Rows → 각 Tuple과 그 scalar들이다. root의 자식은 Target/Columns/Rows, Target의 자식은1개 이름 또는 prefix/실제 이름2개, Columns의 자식은 Name, Rows는 Tuple, Tuple은 Scalar다. 거래 제어는 root1개뿐이다. root.span은 첫 token 시작부터 마지막 구문 token 끝까지이며 optional `;`와 뒤 trivia를 제외한다. source()는 모든 원문을 보존한다. Target은 첫 이름부터 마지막 이름 끝, Columns/Tuple은 양쪽 괄호 포함, Rows는 VALUES부터 마지막 tuple 끝, Name/Scalar는 해당 raw token 또는 부호부터 digits 끝이다.

### 구문·값의 분리와 오류 표

P2는 INSERT INTO target `(columns)` VALUES tuple 목록과 BEGIN/COMMIT/ROLLBACK/SET AUTOCOMMIT ON/OFF만 구문 분석한다. 각각의 목록은 최소1개, 반복은 loop다. 추가 `;`·두 번째 문장·leading `;`·빈/trivia 입력은 Syntax이고 마지막 `;` 하나 뒤 trivia만 허용한다. INSERT의 열 중복·tuple별 arity/대상 열 수·존재/타입·engine mutation 한도는 후속 binding/execution 소유이며 P2의 새로운 구문 거절 조건으로 만들지 않는다. 이름 위치에는 Identifier/QuotedIdentifier만 허용하고 잘못된 token kind는 Parse/Syntax, 이름 내용 오류는 기존 Name 진단을 그대로 전파한다.

**ParsedStatement 성공은 scalar 값 검산 성공이 아니다.** P3의 SQL scalar 입력이 raw sign/digits를 Int64로 checked 변환하여 overflow를 Range로, binary의 짝수 hex, Text/Binary 각각의 decoded 길이≤65,536과 SQL empty text의 Utf8 typed NULL을 검산/변환한다. 따라서 P2는 Int64 초과 digits·홀수/비hex raw Binary·긴 raw payload를 소유할 수 있다. `''`는 Text이며 untyped Null로 조기 변환하지 않는다. §4의 scalar Range 요구는 P3 값 입력에 적용되고 DDL type_length의 raw 보존은 별도다. 정상 실행/BoundStatement 생성은 이 후속 검산을 반드시 소비해야 한다. 이름·scalar 외 node나 사용자 위조 AST를 값 검사 완료 증표로 쓰지 않는다.

| 문맥·입력 | 이번 진단과 span |
| --- | --- |
| BEGIN/COMMIT/ROLLBACK/SET/INSERT 외 첫 token | SELECT/UPDATE/DELETE/CREATE/DROP은 P4 미공급으로 Parse/Unsupported; 비인용 PREPARE/EXECUTE도 Unsupported. 다른 token은 Syntax. span은 그 token |
| 이름/PUBLIC | prefix 정규 bytes가 PUBLIC이면 제거하지 않고 Target 안에 보존. 다른 prefix는 prefix Name raw span의 Parse/Unsupported. unqualified public는 일반 이름. PUBLIC 뒤 dot 다음 이름 누락은 현재 token의 Syntax |
| scalar의 DEFAULT·SELECT | 비인용 DEFAULT 또는 Keyword SELECT는 그 token의 Parse/Unsupported. DEFAULT/RETURNING 등의 일반 이름 위치는 정상 이름 |
| INSERT target/열 목록 뒤 확장 | target 다음 및 닫힌 열 목록 다음 SELECT 또는 비인용 DEFAULT는 그 token의 Unsupported. AS는 target 다음에서만 Unsupported. 다른 예상 밖 token은 Syntax |
| 완결 INSERT 뒤 RETURNING | 닫힌 tuple 목록 뒤·optional `;` 전의 비인용 RETURNING만 해당 token Unsupported. 거래 구문 뒤, 아직 닫히지 않은 tuple 안, `;` 뒤 RETURNING은 Syntax |
| scalar의 `.5` | Dot 바로 다음 raw byte가 ASCII digit이면 Dot token span Unsupported, 그렇지 않은 Dot는 Syntax |
| scalar digits 다음 인접 `.` 또는 exponent identifier | trivia 없이 인접한 Dot는 그 Dot span Unsupported. 인접 Identifier가 e/E 하나 또는 e/E 뒤 ASCII digits만이면 그 Identifier span Unsupported(1e/1e+의 시작도 포함). 1efoo 등 다른 suffix는 Syntax |
| digits와 공백으로 분리한 Dot/exponent | 숫자 확장으로 합치지 않고 다음 delimiter 검사의 현재 token Syntax |
| 부호 다음 digits 외 token | Dot 바로 뒤 raw byte가 ASCII digit인 `.5`는 부호/trivia 여부와 무관하게 Dot span Unsupported(`+.5`, `- .5` 포함). 그 외 현재 token Syntax. `+?`, `-NULL`, `- -1`은 실패, `--1`은 line comment여서 현재 EOF에서 Syntax |
| N/q 바로 뒤 text quote | 비인용 N/n/Q/q Identifier 뒤 raw quote가 인접하면 그 Identifier span Unsupported. 그 외 scalar Identifier는 위 DEFAULT 이외 Syntax |
| 기타 잘린/예상 밖 구조 | 현재 token의 Parse/Syntax, EOF는 `[source.len,source.len)` |

같은 일반 단어를 전역 reserved로 추가하지 않는다. parser가 지원하지 않는 다른 확장을 모두 자동으로 Unsupported로 분류한다고 주장하지 않는다. lexer의 `!=`·미등록 punctuation 및 malformed UTF-8/quote/comment 진단은 원 모듈·span을 그대로 전파한다. 정상 scalar를 마친 뒤 delimiter 아닌 token은 인접 numeric extension 외에는 Syntax다. RETURNING 예외는 닫힌 INSERT tuple 목록 뒤에만 적용한다.

### 한도·자원·제어

진입 Parse 제어 → 기존 Lexer::new의 Lex 제어/1 MiB/전체 UTF-8 → source 예약·복사 → token scan/parse·AST 예약 → 반환 전 Parse 제어 순이다. 관측/할당 없는 호출 bookkeeping은 진입 제어 전에 수행하여 취소/UTF-8 실패 호출도 다음 호출용 주입 설정을 소비한다. 첫 source 예약 실패 span은 leading trivia와 무관하게 `[0,0)`다. 뒤 lexical 오류가 있어도 source 예약 실패가 먼저 나올 수 있다. 각 token scanner·name 진단은 그대로 보존하고 새 parser 자원/AST/marker·구문 오류는 Module::Parse다. 모든 Parse 제어 진단은 실제 처리 source offset p의 `[p,p)`이며 source copy는 복사 완료 byte 위치, arena 이동은 방금 옮긴 node.span.end를 사용하므로 좌표가 뒤로 갈 수 있다. source 외 단순 node 상한·marker 상한·quota/할당 실패는 해당 token/node 시작의 zero-width Limit다. 미완성 node의 초기 span은 도입 token 전체다(Insert/거래 keyword, Target의 첫 이름, Columns/Tuple의 여는 괄호, Rows의 VALUES). Target은 마지막 이름, Columns/Tuple은 닫는 괄호, Rows는 마지막 닫는 tuple, root는 마지막 구문 token을 소비한 직후 최종 span으로 갱신한다. root/Rows가 아직 끝나지 않은 성장에서는 초기 span을 유지한다. 257번째 marker를 AST에 넣기 전에 거절하며 ordinal은 원문 marker 출현 순서1..256이다.

statement/clause/Target/Name/Rows/Tuple/Scalar를 각각1node로 센다. INSERT의 수는 `4 + target_names + columns + tuples + total_scalars`, 거래 제어는1이다. 8,192개를 허용하고 다음 추가를 거절한다. marker/source/node 한도는 독립이다. P2의 정상 문법은 이 node 상한 아래에서 token65,536에 도달하지 않으므로 lexer의 원 token 증거를 수신하고 P2로 다시 실행하지 않는다. 입력 유래 재귀가 없어 유효 중첩은 단일 괄호 목록뿐이며 depth63/64/65 인수는 P3 predicate에 남긴다. 중첩 scalar 괄호를 추가 지원하거나 잘못된 Syntax를 Limit로 바꾸지 않는다.

원문은 known length만큼 한 번 요청하고 AST는16node부터 실제 capacity의2배(checked, 논리 최대8,192)를 요청한다. 실제 AstNode size≤256bytes를 검산한다. 모든 element count×size, capacity 초과분×size 및 usize→u64 산식은 checked다. 원문과 typed AST Vec 모두 §11의 요청량 session→DB 선예약, fallible allocation, 실제 초과 capacity session→DB 게시 전 보정을 적용한다. 이 결정은 §6의 Vec 원문 한정 보정을 **이번 typed AST backing에도** 확장한다. allocator 내부 transient RSS 전체 선예약/OOM 복구를 주장하지 않는다.

arena 성장에서 old/new 실제 backing을 함께 charge하고 node를 순서대로 move하며 한 node 이하(≤256bytes)마다 제어를 검사한다. source 복사는256bytes마다 검사한다. 모든 새 backing은 bytes/nodes Vec를 첫 field, lease를 후속 field로 보관하여 부분 실패에도 allocation을 먼저 폐기한다. old arena는 move 동안 allocation/lease를 유지하고 성공 후 폐기한다. 이름/AST에는 heap-owning child·recursive Drop이 없다. cleanup에 새 예약·DB I/O가 없다. 고정 Parser≤128bytes, ParsedStatement≤384bytes, AstNode≤256bytes를 검사한다.

비기본 test-support는 `Parser::fail_allocation_at(ordinal: usize)`로 **다음 parse 호출**의 source/arena backing allocation 순번1부터 실패를 주입한다(호출이 끝나면 설정 소멸). `set_allocation_padding(elements: usize)`는 요청 element 수를 checked 추가하여 실제 초과 회계 경로를 강제한다. `allocation_count()`는 직전/진행 호출에서 도달한 backing allocation 지점 수, ParsedStatement의 `source_capacity_bytes()`·`arena_capacity_bytes()`는 실제 allocation 관측이다. 기본 제품에는 없다. source 길이0은 capacity0을 유지하고 backing allocation·reservation·순번 소비가 없으며 padding도 적용하지 않는다. 순번은 양쪽 base 예약이 성공한 뒤 실제 fallible allocation 지점에 도달할 때 증가한다(주입 실패도 포함). base quota 선실패는 count를 증가시키지 않는다. ordinal0은 주입 끄기, 도달하지 못한 순번도 다음 호출로 이월하지 않는다. padding은 명시적으로0을 설정하기 전까지 호출 간 유지한다. 별도 parser-private budget은 u8/실제 AstNode 두 소비자만 지원하며 splitter의 이미 검증한 owner를 변경하지 않는다.

### 독립 입력과 선택 검증

literal AST는 전체 node 배열·순서·kind·Name·span·first_child/next_sibling·marker count를 정확히 대조한다. 생산 parser/normalizer로 기대값을 생성하지 않는다. 반복 fixture는 literal subtree의 index/span 이동만 허용한다. root만 따라가 orphan을 놓치는 판정·cycle/중복 연결·raw Text→Null·marker0/순서 누락·incomplete 성공을 known-bad로 거절한다. 최소64seed×128개 scalar fragment의 고정 생성과 별도 확대256×1024/30분 fuzz를 제공한다.

이번 선택에는 각 grammar 위치 잘림·정상/미지원·PUBLIC·source/node/marker/name 경계, 실패 후 재사용·retained AST와 새 parse의 session/DB 잔여−1/동일/+1, source/arena 각 allocation 실패·실제 초과분 회계 실패·old/new copy 취소/deadline·최대 flat AST Drop·동일 DB를 공유하는 caller 합산을 포함한다. 정확 입력/기대값·함수/수·hash는 제품 구현 전에 동결한다. 제품 검사 제거·lease 조기 해제/누수·capacity 회계 누락 mutant를 해당 새 경로 detector로 거절한다. 기존 lexer/name/splitter 완료 시험은 변경 없이 반복하지 않는다. parser 성공을 P3/P4·binding·실행·전체 FE11/12·Q1 완료로 승격하지 않는다.

### D2 결정·동결 기록

작업/계약/통합 담당은 `/root`, 독립 검토자는 `/root/frontend_review`다. 공개 tree/개별 heap node 대신 flat arena를 선택해 전체 node 수·quota·반복 Drop을 한 owner에서 검산한다. caller 원문 borrow 또는 StatementSource 소비 대신 예약한 원문 복사를 선택해 반환 AST의 수명을 독립시키고 현재 Quota에 없는 ledger identity API를 가정하지 않는다. 반복 목록은 tail ID로 O(1) 연결하며 각 append마다 처음부터 tail을 찾는 방식은 채택하지 않는다. 로컬 PostgreSQL의 raw parse/DB 분석 분리, SQLite의 token 소유, CUBRID의 tuple tail 복잡도 경고를 참고했다. 구체 source/revision은 이 leaf 실행 증거에 수합한다.

2026-10-24 독립 D2는 최종 검토 manifest `6aae5d3a42595e78ad334699cf3ed3de3f85dafddbcb41fce5041cd9dafd8dc3`와 연결된 전체 literal/recipe/control을 읽기 검토하고 수용했다. 필수 semantic258(구조24·raw scalar26·오류122·경계22·64seed×128), known-bad23/정상통제4, 자원22함수+private1, 제품 mutant5종이다. 검토 중 오류 문맥·초기 span·주입 수명, root 자원 fixture의 prefix byte 산식과 이동 중 예약 witness를 교정했으며 제품·시험 실행 전 원본을 보존했다. 30분 별도 확대는 valid grammar 생성 fuzz이고 임의 malformed-byte fuzz의 실행 증거로 쓰지 않는다. 필수 고정 malformed122는 별도다. 값 검산/새 AST variant·동결 오류 우선순위/실제 quota 반례가 바뀌면 이 결정을 재검토한다. §12 이전의 원 leaf 증거는 소급 변경하지 않는다.

구현 중 긴 exponent 후보 Identifier의 재검사 loop를 식별하여 [보충 자원 입력](../../tests/fixtures/dev4-parser/control-supplement.json)의 취소/deadline1함수와 검사 제거 mutant1종을 실행 전에 추가했다. 이 보충은 helper 코드 작성 **후**, 첫 compile/시험 **전**에 독립 D2 검토를 받았다. 원 구현 전 동결258semantic/22자원/5mutant를 소급 수정하지 않으며 합계 자원23함수·제품mutant6종으로 선택한다. 원 helper 초안과 두 동결 시점·hash는 실행 증거에 따로 보존한다.

<a id="scalar-leaf"></a>
## 13. D4-FE-SCALAR-001 — SQL scalar 입력·소유 경계

2026-10-24 · **D2 ACCEPTED / scalar 제한 구현·선택 검증 / 최종 D1 제한 수용**. P3의 scalar 입력을 먼저 구현하고 predicate AST·전체 타입 분석·3값 계산은 다음 leaf에 연결한다. 실제 binder/DDL/session·engine은 여전히 미구현이며 이 제한을 P3 전체 완료로 바꾸지 않는다. 담당/통합은 `/root`, 제품은 `/root/frontend_research`, 독립 literal은 `/root/frontend_oracle`, 독립 D2/D1은 `/root/frontend_review`다. 기준 commit은 `15e47f4df6bb2445bcc0d9fe3c5d06c9be9ffddc`이고 별도 DEV3 dirty를 수정·시험에 포함하지 않는다.

### 값 표현과 공개 입출력

현재 foundation에는 borrowed ValueRef가 없다. `Value::Null`은 타입이 없고 `Value::compare`는 NULL을 타입 비교보다 먼저 처리하며 owned String/Vec는 quota owner가 아니다. 이를 수정하거나 SQL의 정답 계산으로 사용하지 않는다. 이번 타입·소유는 SQL frontend 안에 두고 정상 dependency는 foundation 하나를 유지한다.

| 공개 항목 | 계약 |
| --- | --- |
| `ScalarType::{Int64,Bool,Utf8,Binary}` | 원래 논리 타입, Copy/Eq |
| `ScalarRef<'a>::{Null(ScalarType),Int64(i64),Bool(bool),Utf8(&str),Binary(&[u8])}` | Copy/Eq인 borrowed 입력/읽기 view. 직접 구성할 수 있으며 구성 자체는 검산·예약 성공이 아니다. caller 입력 allocation은 caller 소유이며 이 helper가 소급 회계하지 않음 |
| `DeclaredType::{Int64,Bool,Utf8(usize),Binary(usize)}`·`ScalarSpec { declared_type, nullable }` | 공개 명시 타입 fixture/후속 소비자 metadata. 가변 선언 길이는1..65,536, 고정 타입에는 길이가 없음. catalog 조회·DDL precommit을 대신하지 않음 |
| `OwnedScalar`·`as_ref()` | private 값/lease와 readonly borrowed view. Clone·값 반출·mutator가 없으며 값 backing은 lease보다 먼저 폐기. Int64/Bool/typed NULL은 heap 없음 |
| `DecodedScalar::{Value(OwnedScalar),Null,Parameter(u16)}` | raw 미정 NULL과1부터의 unresolved marker를 보존. SQL empty text는 Value(Null(Utf8)). 공개 variant는 caller도 구성하므로 AST provenance나 marker 유효성 capability가 아님 |
| `ScalarReader::new(&Quota,&Quota)` | 기존 session/DB ledger handle만 공유하며 새 quota를 만들지 않음 |
| `decode(&mut self,&ParsedStatement,node:usize,&CallControl)->Result<DecodedScalar,Diagnostic>` | 실제 immutable AST의 scalar node만 선택. statement/source borrow는 호출 동안만 유지하고 반환 값은 독립 수명. 원 source/AST lease는 별도 유지 |
| `copy_sql(&mut self,ScalarRef,ScalarSpec,Span,&CallControl)->Result<OwnedScalar,Diagnostic>` | 원래 타입 검사 후 SQL empty Utf8만 typed NULL로 정규화하여 목적 nullability/길이 검산·복사 |
| `copy_native` 같은 인자/반환 | 같은 검산·복사를 하되 native non-NULL empty Utf8를 정규화하지 않음 |

`OwnedScalar`는 **전역 scalar 저장 표현의 유효성**만 보장한다. `copy_*`와 `decode`가 같은 저장 타입을 반환해도 어떤 목적 열/parameter의 검산을 끝냈다는 capability는 아니다. 후속 실제 binder/predicate는 raw ScalarSyntax의 원래 타입(Text는 빈 값·초과 길이여도 Utf8, Integer는 Int64)과 전체 목적 metadata를 **raw decode 이전**에 검사해야 한다. 예를 들어 초과 길이 Text→Int64의 실제 대입 오류는 Type다. 문맥 없는 `decode` 자체가 그 Text에 Range를 반환하는 것은 별도 호출 의미다. 이런 실제 binder 연결은 아직 제공하지 않으며 decoder 성공을 전체 문장 검산으로 해석하지 않는다.

`copy_*`는 raw 미정 NULL/marker를 받지 않는다. 후속 typed 소비자가 전체 정적 타입 검사 뒤 유효 목적 타입을 사용해 미정 NULL을 typed NULL로 만들고 marker는 실제 binding으로 해소한다. 개별 값 검산을 bind_all·행 전체 원자성 인수로 확대하지 않는다.

### 검사 순서와 진단

Code에 `Type`, `Range`, `NotNull`, Module에 `Value`를 추가한다. 순수 진단이며 거래 상태/effect를 만들지 않는다. 이전 exhaustive 관측 adapter의 새 enum arm은 compile 연결이며 기존 runtime 사례를 반복할 근거가 아니다.

모든 호출은 관측 없는 injection bookkeeping 초기화 뒤 진입 제어를 검사한다. `decode`는 유효 index이면 해당 node의 시작, 범위 밖 index이면 source EOF에서 진입한다. 범위 밖은 Value/Syntax `[len,len)`, non-scalar는 그 node 전체 span의 Value/Syntax다. 그 외 값 구문/범위 진단은 scalar 전체 span, 자원 Limit는 scalar 시작의 zero-width다. 호출 제어 진단은 실제 raw byte 처리 위치의 zero-width다.

`copy_*`의 순서는 진입 제어→metadata 유효 범위→원래 타입→SQL만 empty 정규화→nullable→전역/목적 byte 길이→예약/복사→반환 전 제어다. 잘못된 metadata는 Range다. typed NULL·empty Utf8도 원래 타입이 다르면 Type가 우선한다. 같은 타입 NULL이고 nullable=false면 NotNull, 가변 payload가65,536 또는 목적 길이를 초과하면 Range다. coercion·절단·padding은 없다. native empty Utf8는 NOT NULL에도 실제 값이며 empty Binary·공백·U+0000도 보존한다.

`copy_*`에서 반환하는 모든 진단 span은 caller의 `at`를 수정하지 않는다. 원문이 없는 native/parameter에도 쓸 수 있으므로 clamp·offset 산식·reversed span 교정을 하지 않는다. 반면 test-support `CallControl` observer는 Module::Value와 **입력/복사 내부의 local byte progress `[p,p)`**를 받는다(진입0, 각 chunk 완료, 완료 입력 길이). 제어 오류를 반환할 때만 caller at로 매핑한다. 이 관측 위치는 SQL source 위치라는 주장이 아니며 내부 loop checkpoint를 오류 표시 위치와 분리한다.

정수는 checked 누적으로 MIN/MAX를 보존하며 첫 overflow digit에서 Range다. 음수 최솟값을 양수 i64로 먼저 만들지 않는다. 긴 leading zeros와 negative zero는 정상이다. Binary는 raw payload의 ASCII hex를 왼쪽부터 pair로 검산한다. pair 내 첫 비hex가 Syntax이고 유효 마지막 단독 nibble도 Syntax다. **두 nibble 모두 유효한65,537번째 byte가 성립할 때** Range가 된다. 따라서 그 pair의 비hex는 Syntax, 정상65,537byte 뒤의 비hex는 앞선 Range다. `X'0''0'`은 Syntax, `X''`는 빈 Binary다. Text는 quote doubling만 decode하고 Unicode/backslash/공백/NUL을 보존하며 decoded65,537byte에서 Range, 길이0이면 Utf8 typed NULL이다.

### 예약·제어·수명

Text/Binary는 검산·정확 decoded 길이 계산 후 단일 backing allocation으로 두 번째 decode를 수행한다. source 전체/최종 payload 전체의 숨은 UTF-8 재검사나 `parse::<i64>()`에 긴 loop를 위임하지 않는다. Text는 최대4byte UTF-8 scalar 또는 경계가 확인된≤256byte 조각을 검산하여 예약된 String에 넣는다. 모든 긴 정수·검산·decode/copy loop는 진입/종료와 최대256 raw/copied bytes마다 호출 제어를 검사한다. 최대4byte step은252byte 기준으로 검사해 초과를 피한다. cancel이 deadline보다 우선하며 관측한 checkpoint에서만 pending 값 오류보다 먼저 반환할 수 있다. 반환 값/reader에 완료 호출의 제어를 저장하지 않는다.

decoded/copied 길이를 정확히 요청하며 zero length·고정 값·NULL·marker에는 backing 예약/할당이 없다. session→DB base 선예약, fallible allocation, 실제 capacity 초과분 session→DB 게시 전 보정을 String/Vec 모두에 적용한다. checked byte 산식/usize→u64, old retained output+new+source/AST 합산을 유지한다. 추가 growth/realloc이나 무상 clone을 하지 않는다. allocation은 lease보다 먼저 폐기하고 부분 실패는 이번 예약만 회수한다. global allocator 내부 RSS/OOM 복구 보장은 아니다.

parser/splitter의 이미 검증한 budget을 범용화하지 않는다. scalar는 String/Vec backing·Value 진단·별도 호출 수명의 실제 소비자이며 작은 예약 절차 중복을 이번 범위에서 허용한다. 공유 불변식 변경 또는 추가 실제 소비자가 생기면 공통화 대안을 재검토한다. 고정 크기는 ScalarReader≤128, OwnedScalar≤256, DecodedScalar≤272, ScalarRef≤32bytes이며 입력 유래 재귀/Drop이 없다.

비기본 test-support의 `fail_allocation_at(usize)`, `set_allocation_padding(bytes)`, `allocation_count()`, OwnedScalar의 `capacity_bytes()`를 제공한다. 실패 ordinal은 다음 decode/copy 호출 한 번만 소비하고0은 끄기, 도달하지 못해도 이월하지 않는다. allocation count는 양쪽 base 예약 성공 뒤 실제 backing site에서 증가하며 주입 실패를 포함한다. padding은 명시0 전까지 유지하되 zero-length/fixed에는 적용하지 않는다. padding overflow는 base 예약 뒤 count1의 Limit이며 lease를 모두 반환한다.

### 검증·결정 기록

독립 literal의 실제 ID/수/hash와 root의 자원 adapter를 제품 작성 전에 동결한다. 최소64seed×128scalar fragment와 별도 확대256×1024/30분 grammar-valid scalar 입력 생성 profile을 구분한다. malformed raw Binary·범위 오류는 고정 corpus로 검사하며 유효 생성 fuzz를 임의 malformed-byte fuzz로 부르지 않는다. wrong type/typed NULL/정규화/marker/error/span/incomplete 관측을 known-bad로 거절하고 정상 통제를 함께 둔다.

자원 선택은 allocation site별 실패, session/DB base·actual capacity 경계±1, 뒤 예약 실패 정리, retained 원 source/AST/값, caller 폐기, long loop 양 pass의 중간 취소/deadline, 호출 재사용/주입 수명, 두 native caller 합산,256KiB stack의 최대 payload Drop과 checkpoint 최대 간격이다. 제품 mutant는 각 실제 long loop 검사 제거, actual capacity 미회계, 실패 lease 누수, typed NULL mismatch 수용, native empty 정규화를 해당 detector로 거절한다. 기존 완료 lexer/name/splitter/parser runtime은 반복하지 않는다.

D2는 foundation 변경/가짜 engine 대신 SQL-local readonly type+quota owner를 선택한다. 원래 타입·NULL 소유를 foundation에 합치는 대안은 현재 하위 의미를 바꾸므로 기각하고, AST source와 ScalarSyntax를 caller가 별개로 제공하는 대안은 forged 조합을 늘리므로 기각한다. PostgreSQL·SQLite의 checked 정수/입력 분리와 MariaDB의 empty 값 경계를 실제 revision으로 조사하되, MariaDB mode의 empty blob→NULL 및 넓은 숫자/coercion 문법은 도입하지 않는다. 실제 연구·독립 검토와 source별 실행 증거는 이 leaf 종료 기록에 수합한다. 의미/오류 순서·borrow/소유·quota·전체 타입 검사 경계가 바뀌면 D2를 다시 검토한다.

2026-10-24 `/root/frontend_review`가 manifest `b3e91f5de8983f09a07702dd0d7f01b77e642d98fce66d84009100451cae1b66`의 실제 파일·계약을 독립 D2로 수용했다. semantic7함수8,929call(고정/유한737·생성8,192), known-bad20/정상통제8, 자원16함수와 제품 mutant11범주를 제품 작성 전에 동결한다. 별도 확대2profile은 NOT_RUN이다. 검토 중 copy 관측 위치·목적 타입 검산 경계·최대 제어 간격·retained quota 경계·partial peak와 최종 제어를 보완했으며 제품/시험 FAIL을 소급 생성하지 않는다. 원 검토 사본·hash와 수용 후 동결은 artifacts/dev4-scalar-20261024에 보존한다.

[source별 scalar 실행 증거](../evidence/dev4-scalar-20261024.md)에 신규23함수·8,929semantic·known-bad20/통제8과 필수11범주/12제품 변조 거절을 보존한다. 정상 runtime은 한 번이며 확대2profile·predicate/실제 binding·Q1은 미완료다.

<a id="predicate-parser-leaf"></a>
## 14. D4-FE-PREDICATE-001 — 공통 predicate AST/parser

**D2 ACCEPTED / 2026-10-24 구현 전 입력 동결**. 4.03.1.2의 전체 조건식 구문을 독립 leaf로 공급한다. P3의 typed compile/eval·3값 논리(4.04.3), 실제 catalog binding·P4 문장 연결·Q1은 계속 미완료다. 기존 scalar나 SQL 의미를 바꾸지 않는다.

### 진입·AST·수명

`Parser::parse_predicate(&mut self, source: &[u8], control: &CallControl<'_>) -> Result<ParsedStatement, Diagnostic>`는 WHERE 없는 predicate 하나와 선택적인 마지막 세미콜론·trivia를 받는다. 기존 `parse`의 INSERT/거래 구문 허용 범위는 유지한다. 반환 source/flat arena는 기존 session/DB quota를 사용하며 caller 입력·Parser보다 오래 살아도 예약을 유지한다. 공개 raw 성공은 타입·존재·값 변환·행 선택 성공이 아니다.

`ComparisonOp::{Eq,NotEq,Lt,Le,Gt,Ge}`와 `NodeKind::{Predicate,Or,And,Not,Group,Compare(ComparisonOp),IsNull{negated:bool}}`를 추가한다. Predicate→Or 하나, Or→And 하나 이상, And→not-term 하나 이상, Not→not-term 하나, Group→Or 하나, Compare→Name/Scalar 두 개, IsNull→Name/Scalar 하나다. singleton Or/And도 항상 저장하고 Group 안에 Predicate를 추가하지 않는다. 전체 arena는 유일 root0에서 도달 가능한 preorder이며 기존 Name/Scalar 원문 표현을 재사용한다.

Predicate/Or/And span은 첫 자식 시작..마지막 자식 끝, Not은 NOT 시작..자식 끝, Group은 양쪽 괄호 포함, Compare는 lhs..rhs, IsNull은 lhs..마지막 NULL이다. 세미콜론·바깥 trivia는 AST span에서 제외하고 source에 보존한다. 부모의 최초 span은 첫 token 전체이며 자식 완료 뒤 확정한다. atom은 먼저 private Compare(Eq) placeholder를 등록하고 lhs를 읽은 뒤 실제 Compare/IsNull kind를 확정한다. 실패 tree는 반환하지 않는다. private `AstBuilder::set_kind`는 할당·재배치 없이 기존 node만 교체한다. 기존 `scalar`는 성공 span을 반환하도록 바꾸되 변환/진단/marker 순서는 유지한다.

P4 WHERE는 같은 ParseInput·source·arena·marker counter와 private predicate 메서드를 사용한다. fragment 진입점에만 Predicate root·optional semicolon·EOF 검사가 있다. 내부 predicate는 AND/OR가 아닌 다음 token을 lookahead에 남기며 각 문장/Group caller가 자신의 종료 문법을 검사한다. 실제 P4 연결은 아직 구현 증거가 아니다.

### 검사 순서·문법·오류

호출 순서는 injection 소비→Parse 진입 제어→Lexer 전체 UTF8/source 한도 검사→source copy→fragment 첫 peek다. 첫 token이 EOF이면 root/Or/And 할당 전에 Syntax EOF다. 따라서 빈 source+quota0은 Syntax·allocation0, trivia source의 예약 실패는 Limit다. 첫 peek의 Lex 오류를 먼저 전파하고, nonEOF token이면 Predicate→Or→And를 먼저 등록한다. 잘못된 첫 AND/세미콜론도 wrapper/atom 예약·node 한도가 구문 오류보다 먼저 실패할 수 있다. 내부 Or/And는 EOF/닫는 괄호도 먼저 wrapper를 등록한다.

우선순위는 비교/IS NULL→NOT→AND→OR다. AND/OR 형제 목록은 반복문으로 처리한다. NOT/Group만 공통 active depth를 소비하며 초기0,64허용,65번째 NOT/'(' token 확인 직후 재귀·해당 node 등록 전에 Parse/Limit zero-width 시작 위치로 거절한다. IS NOT NULL의 NOT는 깊이를 소비하지 않으며 sibling에서 부모 depth로 복귀한다. guarded recursive descent는 bounded frame만 보유하고 256KiB stack에서 최대 정상/실패를 검증한다. AST 폐기는 flat하고 재귀하지 않는다.

bare TRUE/NULL/이름/marker, `(a)=1`, `a=(1)`, 비교 chain은 Syntax다. 이름끼리·literal끼리·NULL/marker 상대, 타입이 다른 값·overflow 정수·raw 비hex Binary는 구문이 맞으면 raw AST로 보존한다. 전체 타입 검사는 후속 leaf가 담당한다.

| 구문 문맥 | 진단 |
| --- | --- |
| Name/Scalar operand 뒤 `.`, `(`, 이항 `+`, `-`, `*` | 해당 punctuation Unsupported. 공백 유무와 원래 operand 타입 무관. 값 시작 unary sign은 기존 정수 구문 |
| lhs 뒤 IN/BETWEEN/LIKE, 또는 NOT IN/NOT BETWEEN/NOT LIKE | 해당 word, 또는 NOT token의 Unsupported. NOT 뒤 다른 token은 NOT의 Syntax |
| IS/[NOT] 뒤 TRUE/FALSE/DISTINCT | 해당 token Unsupported. 나머지 NULL이 아닌 token은 Syntax |
| 값/Group 시작 SELECT | SELECT Unsupported |
| rhs 값 위치 '(' 뒤 SELECT | 한 token peek 뒤 SELECT Unsupported; 다른 token이면 원 '(' Syntax. peek의 Lex 오류는 우선 전파 |
| 인접 N/Q prefix text | prefix token Unsupported. 일반 n/q 열 이름은 허용 |
| IN/BETWEEN/LIKE/EXISTS/DEFAULT/RETURNING의 값 이름 위치 | 일반 Name. quoted 여부에 맞는 기존 정규화 |
| 위 operand suffix에 해당하지 않는 완결 비교 뒤 token, 또는 null-test terminator 뒤 token·두 번째 semicolon | Syntax. INSERT 전용 RETURNING 분류를 확대하지 않음 |
| `!=`·원 lexer/name 오류 | 원 module/code/span 그대로 |

함수 형태는 operand 뒤 '('에서 내부를 읽지 않고 거절한다. `EXISTS=1`은 이름 비교지만 `EXISTS(...)`는 '(' Unsupported다. IS NULL의 마지막 NULL은 operand가 아닌 terminator라 그 뒤 extra token은 Syntax다. NOT 복합 연산자 구분 또는 RHS '('의 한 token peek에서 발생한 lexical 오류를 숨기지 않는다. 이 명시표 밖의 모든 SQL 확장을 Unsupported로 인식한다고 주장하지 않는다. 구문 진단은 token 전체 span, EOF는 `[len,len)`, depth/node/marker Limit는 도입 token 시작의 zero-width다.

### 자원·독립 검증·잔여

원문1MiB·token65,536·AST8,192·marker256·nesting64 및 session8MiB/DB64MiB를 유지한다. 모든 wrapper와 IsNull도 node/capacity에 포함하며 별도 token/list backing은 없다. source/arena 선예약·actual capacity 보정·old/new peak·실패/Drop 반환은 기존 budget을 소비한다. ParseInput의 새 반복·중첩 진입/닫기와 최종 반환에 호출 제어를 검사한다. 원 helper의 bounded byte 검사와 node 이동 검사를 보존하며 AST span의 단조 증가를 가정하지 않는다. 완료 호출의 control을 보관하지 않는다.

독립 expected tree를 먼저 선택하고 literal/recipe로 SQL을 출력한다. 제품 tokenizer/parser/type/normalizer를 정답 계산에 사용하지 않는다. 전체 node 배열·payload·source·marker·code/module/span·complete를 비교하고, 임의 node 삭제/추가·연결·연산자·원문·typed 입력 경계 오류를 known-bad로 거절한다. 새 source별 제품 mutant는 우선순위·깊이·marker·EOF·node상한·반환 제어를 실제 detector로 검증한다.

필수 semantic profile은 고정 AST/원문/오류/경계와64seed×128fragment다. 64개의 composite parse와8,192fragment의 분모를 구분한다. 별도 확대0..255seed×1,024fragment·30분 grammar-valid 생성은 선택 가능하게 작성하고 NOT_RUN을 보존한다. 30분 profile을 임의 malformed-byte fuzz로 부르지 않는다. 자원9함수는 caller/output 수명·quota경계/partial peak·모든 신규 entry allocation site·미도달 주입 소비·취소/deadline·thread별 공유quota·256KiB stack/최대AST·실패후 재사용을 다룬다. 실제 입력/hash·원 실패와 최종 source별 검증은 채택 뒤 증거에 연결한다.

PostgreSQL/SQLite 실제 revision·선택 파일 hash/열람 범위를 보존한다. nary boolean·전체 의미 검산과 평가 분리는 참고하되 broader grammar·implicit coercion·NULL comparison rewrite는 도입하지 않는다. 별도 D2 reviewer가 계약/API/fixture/resource를 확인하기 전 제품 구현을 시작하지 않는다. 구현 뒤 D1에서 코드·원 source/결과와 잔여를 확인한다. 기존 P2/scalar runtime과 DEV3 제품을 다시 실행하지 않으며 새로운 함수/소비 경로만 선택한다. 문서 자동 검사는 push gate에서만 실행한다.

독립 검토자 `/root/frontend_review`가 계약·API·정확 literal/recipe·semantic adapter와 자원9함수 입력을 확인했고 `/root`가 채택했다. 원 검토 사본·이전 지적·최종 해시는 `artifacts/dev4-predicate-20261024/{d2-acceptance,freeze}.json`과 `frozen-inputs/`에 보존한다. 채택 시 제품 predicate parser·컴파일·시험은 미실행이었다.
