# DEV4 — DEV3와 독립적인 SQL 앞단 계약

2026-10-24 · **D4-PREP-001/002 ACCEPTED / 제품·모델·Q1 NOT_RUN**

[SQL 의미 정본](dev4-sql-design.md) · [개발 계획](../plans/dev4-preparation-plan.md) · [Oracle·로컬 소스 조사와 검토](../evidence/dev4-preparation-research-20261024.md)

<a id="scope"></a>
## 1. 적용 범위

DEV4 안에서 저장 엔진에 접근하지 않는 입력 처리·값 계산과 독립 검증 도구를 DEV3 완료 전에 개발할 수 있도록 분리한다. 새 제품 단계나 완료 gate를 만들지 않는다. DEV3 → DEV4의 **실제 엔진 통합·Q1 인수 의존성**은 유지한다. 이 계약은 기존 DEV4 문서의 모든 제품 코드를 DEV3 공급 뒤로 미뤘던 착수 조건을 아래 허용 범위에 한해서 대체한다. 현재 요청의 실행 범위는 계약·계획 확정이며 제품 개발·시험 재개는 아니다.

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
- foundation `Quota`/`Reservation`을 소비한다. 여러 quota 중 뒤 예약이 실패하면 앞 예약을 반환하고 기존 AST/binding은 유지한다. quota 승인은 실제 allocation 성공이 아니므로 필요한 `try_reserve` 실패를 typed LIMIT로 처리한다. 실제 capacity와 임시 공존 peak를 포함하고 전역 OOM 복구를 보장한다고 쓰지 않는다.
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
