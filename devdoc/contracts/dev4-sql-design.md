# DEV4 — embedded 최소 SQL·Oracle 의미 상세 설계

2026-10-19 · **DEV4-DESIGN-001·REVIEW-002–006 ACCEPTED / 미구현·Q1 NOT_RUN**

[구현·검증 계획](../plans/dev4-implementation-plan.md) · [실제 소스 조사](../evidence/dev4-source-research.md) · [단계 범위](../plans/development-stages.json) · [완료 계약](../plans/stage-completion.json) · [DEV3 저장 계약](disk-storage-design.md) · [커널 구현 계약](implementation-contracts.md)

이 문서는 XADB의 비파티션 DISK table에 대한 SQL 의미와 embedded 실행 경계를 정한다. API 이름은 구현 예정이며 호출 가능한 제품을 뜻하지 않는다. DEV2 부분 구현과 DEV3 설계 채택을 영속 엔진 완료로 해석하지 않는다. [D4-PREP-001 독립 앞단](dev4-independent-frontend.md#scope)의 lexer/parser·값 처리와 독립 검증 도구는 해당 leaf 계약·oracle·profile 동결 뒤 DEV3와 병행 개발할 수 있다. 실제 엔진을 소비하는 구현·Q1은 DEV3 최소 선행 증거를 요구한다. 이번 계약·계획 작성은 제품 개발·시험 재개가 아니다.

공급 대상은 [DEV3-DESIGN-003](disk-storage-design.md#decision)의 mutable page·WAL·영속 undo와 statement token·table 단위 Serializable 검증이다. 이 보완 작업 중 해당 새 안과 별도 독립 검토 뒤 ACCEPTED 전환을 관측했다. 실제 공급 증거는 DEV4의 engine 소비 구현 착수 전에 확인한다. DEV2 전용 COW·전역 q·단일 거래 WAL frame을 DEV3 disk 경로의 계약으로 사용하지 않는다.

<a id="scope"></a>
## 1. 범위·원문·선택

필수 범위는 CREATE/DROP TABLE/INDEX, INSERT/UPDATE/DELETE, 단일 table SELECT·predicate·ORDER BY·LIMIT, PK/UNIQUE/NOT NULL, BEGIN/COMMIT/ROLLBACK·autocommit, typed prepare/bind/execute/fetch/close, embedded CLI다. Bool/Int64/Utf8/Binary와 NULL, statement rollback, read-your-writes, transaction snapshot + DEV3의 table 단위 보수적 validation SERIALIZABLE을 사용한다.

join·집계·subquery·window·DISTINCT·CTE·set 연산·산술/함수·암묵 cast·확장 타입·FK/CHECK·명시 SAVEPOINT·ALTER·PSM·trigger·partition SQL은 DEV7 또는 해당 후속 단계의 의무다. server/transport는 DEV5, driver는 DEV6이며 Q1에 요구하지 않는다. shared plan cache·고급 재최적화·JIT·vectorized 실행은 초기 구현에 넣지 않는다. 미지원 문법은 성공이나 빈 결과로 위장하지 않는다.

| 결정 | 채택할 선택과 이유 | 비교한 대안 |
| --- | --- | --- |
| SQL front end | 제한된 lexer + recursive-descent statement parser + precedence predicate parser, typed AST/BoundStatement | 외부 범용 parser는 XADB subset·오류·quota adapter 비용이 있어 지금 의존성을 추가하지 않음. 문법 확대 때 재검토 |
| 실행 | 작은 pull executor, scan/point/range + filter/project/sort/limit, DML은 engine statement API | VM/JIT·일반 optimizer framework는 현재 기능에 불필요한 상태·검증 비용 |
| prepared 실행 | 문맥/명시 hint로 prepare 시 타입 고정, owned binding, execute 시 dependency 검사, stale 명시 거절 | 실행마다 타입 변경·자동 reprepare는 metadata/오류 효과와 재실행 경합을 복잡하게 함 |
| ORDER BY | streaming 외부 merge sort를 기준선으로 제공 | RAM 전체 materialization은 큰 DISK 데이터에 부적합. top-N·index로 sort 제거는 별도 동등성 검증 뒤 |
| 제약·commit | 현재 DEV3의 부분 NULL·문장 최종 상태 UNIQUE와 COMMITTED/ABORTED/UNKNOWN·NotCommitted/RecoveryRequired 결과 보존 | SQL에서 UNIQUE를 재구현하거나 transaction 종료까지 검사를 미루지 않음 |
| Oracle 관련 의미 | 미인용 이름 대문자, SQL 빈 text 입력의 typed NULL 정규화, 명시적인 DDL 전 commit 경계 | SQL/native 값 표현 전체 변경이나 dialect 전환 flag를 추가하지 않음. 적용 한계는 아래 계약에 명시 |

SQL 초안의 어휘·타입·문법 제안 중 아래에 명시한 부분을 구체화한다. [초안](../reference/sql-language-draft.md)은 역사적 제안/예제이며 새로운 정본이 아니다. UNIQUE는 [DEV3-REVIEW-002](disk-storage-design.md#review-followup)의 문장 최종 상태 검사와 전체 statement rollback을 따른다. DEV4-DESIGN-001의 mutation별 검사 선택은 [DEV4-REVIEW-002](#oracle-review)로 대체한다. Oracle 19c의 이름·NULL·제약·DDL 의미를 선별 참고하며 전체 dialect/ABI/오류 호환은 약속하지 않는다. 기존 BIGINT/BOOLEAN/VARBINARY·LIMIT·`?`·고정 namespace 문법은 유지한다.

<a id="grammar"></a>
## 2. 유한 문법과 이름

아래 EBNF의 `[]`는 선택, `{}`는 반복이다. API의 한 SQL 입력은 정확히 한 문장과 선택적인 마지막 `;`만 받는다. CLI batch는 같은 lexer의 경계 규칙으로 문장을 하나씩 전달한다. 공백/주석만 있는 API 입력은 Syntax 오류, CLI의 빈 구간은 건너뛴다.

```text
statement := create_table | drop_table | create_index | drop_index
           | insert | update | delete | select | transaction_control
create_table := CREATE TABLE table_name '(' column_def {',' column_def}
                [',' PRIMARY KEY '(' columns ')']
                {',' UNIQUE '(' columns ')'} ')' STORAGE DISK
column_def := name type [NOT NULL | NULL]
type := BIGINT | BOOLEAN | VARCHAR '(' type_length ')'
      | VARBINARY '(' type_length ')'
type_length := ['+' | '-'] decimal_digits
create_index := CREATE [UNIQUE] INDEX index_name ON table_name '(' columns ')'
drop_table := DROP TABLE table_name
drop_index := DROP INDEX index_name
columns := name {',' name}
insert := INSERT INTO table_name '(' columns ')' VALUES tuple {',' tuple}
tuple := '(' scalar {',' scalar} ')'
update := UPDATE table_name SET name '=' scalar {',' name '=' scalar} [WHERE predicate]
delete := DELETE FROM table_name [WHERE predicate]
select := SELECT projection FROM table_name [WHERE predicate]
          [ORDER BY order_term {',' order_term}] [LIMIT limit_value]
projection := '*' | name [AS name] {',' name [AS name]}
order_term := name [ASC | DESC] [NULLS FIRST | NULLS LAST]
predicate := or_term
or_term := and_term {OR and_term}
and_term := not_term {AND not_term}
not_term := NOT not_term | '(' predicate ')' | comparison | null_test
comparison := value ('=' | '<>' | '<' | '<=' | '>' | '>=') value
null_test := value IS [NOT] NULL
value := name | scalar
scalar := literal | '?'
limit_value := nonnegative_integer | '?'
transaction_control := BEGIN | COMMIT | ROLLBACK | SET AUTOCOMMIT (ON | OFF)
table_name := [public '.'] name
index_name := [public '.'] name
```

`public` prefix도 일반 identifier 정규화 규칙을 적용한다. 단일 고정 namespace의 정규 이름은 `PUBLIC`이며 `public`, `PUBLIC`, `"PUBLIC"`를 같은 prefix로 받는다. `"public"`과 다른 schema는 Unsupported다. engine에는 prefix를 제거한 정규 이름 bytes를 전달한다. Oracle의 사용자별 schema를 구현한 것은 아니다. table/index 이름 namespace는 분리하고 index 이름은 DB 전체에서 유일하다. 열 참조는 입력 table의 단순 이름만 받는다. table alias·qualified column·출력 alias/ordinal을 ORDER BY에서 참조하는 문법은 미지원이다. projection의 중복 열/출력 이름은 허용하며 metadata ordinal로 구별한다.

- keyword와 unquoted name은 ASCII 대소문자를 구별하지 않는다. unquoted name은 `[A-Za-z_][A-Za-z0-9_]*`이고 ASCII 대문자로 저장한다. 열·table·index·출력 alias와 metadata 이름에 같은 규칙을 적용한다. 이 문법의 keyword는 reserved다.
- quoted name은 `"..."`, 내부 quote는 `""`로 표시하며 UTF-8 bytes를 정규화 없이 비교한다. 정규화 후 1–128 bytes, NUL과 Unicode C0/C1 control은 거절한다. [첫 앞단 동결](dev4-independent-frontend.md#lexer-leaf)은 DEL(U+007F)도 같은 control 거절 대상으로 명시한다. `t`, `T`, `"T"`는 같고 `"t"`는 다르다. native catalog의 byte 비교 자체는 바꾸지 않는다.
- 정수 literal은 선택적 `+`/`-`와 decimal digits이며 Int64 범위를 checked parsing한다. 최소값 `-9223372036854775808`도 허용한다. 소수·지수는 Unsupported, 정수 overflow는 Range다.
- boolean은 TRUE/FALSE, text는 `'...'` 및 `''` escape, binary는 `X'...'`의 짝수 hex digits, NULL은 값의 부재다. SQL의 빈 text literal `''`는 Utf8 typed NULL로 정규화하고 빈 binary `X''`는 실제 값으로 보존한다. backslash escape·Unicode 정규화를 하지 않는다. 공백과 text의 U+0000은 길이 있는 값으로 보존한다. identifier NUL과 구분한다.
- 입력은 유효한 UTF-8, 주석은 `--`부터 줄 끝과 중첩하지 않는 `/* ... */`다. 미종료 quote/comment는 Syntax다. literal·identifier·comment 안의 `?`/`;`는 parameter/구분자가 아니다.
- `!=`, DEFAULT, RETURNING, INSERT SELECT, IF EXISTS, CASCADE, descending/expression/partial index, SQL PREPARE/EXECUTE는 미지원이다. 알려진 후속 feature 시작 위치는 Unsupported, 나머지 잘못된 token/구조는 Syntax로 분류한다.

오류 위치는 원문 UTF-8의 0-based byte `[start,end)`를 정본으로 반환하며 EOF는 같은 시작/끝 offset이다. CLI의 1-based line/column은 표시용이며 원본 byte span을 보존한다. parse·bind는 단락 평가 전에 전체 문장을 검증한다. `LIMIT 0`이나 거짓 predicate도 없는 열/잘못된 타입 오류를 숨기지 않는다.

<a id="values"></a>
## 3. 타입·NULL·DDL

| SQL 타입 | 공통 값 의미 | 제약 |
| --- | --- | --- |
| BIGINT | signed Int64 | 범위 밖 거절, wrap/saturation/암묵 cast 없음 |
| BOOLEAN | false < true | 0/1·text 자동 변환 없음 |
| VARCHAR(n) | UTF-8 byte 비교 | n=1..65,536 bytes; SQL 빈 입력은 typed NULL, trailing space 보존, 절단/padding 없음 |
| VARBINARY(n) | unsigned byte 사전식 비교 | n=1..65,536 bytes; text 자동 변환 없음 |
| NULL | nullable typed 값 | 독립 열 타입이 아님; SQL 비교와 index NULL 순서를 구분 |

같은 논리 타입끼리만 비교·대입한다. 문자열 선언 길이가 달라도 비교 타입은 Utf8이며, 대입 길이는 목적 열에서 검사한다. untyped literal NULL은 상대/대입 문맥 타입을 사용한다. `NULL = NULL`은 UNKNOWN, `NULL IS NULL`은 TRUE다. 다른 non-NULL 타입의 비교는 결과를 보기 전에 Type 오류다.

<a id="text-boundary"></a>
SQL literal과 bind의 원래 타입을 먼저 확인하고 **길이 0인 Utf8 입력만 같은 타입의 NULL로 정규화**한 뒤 NULL 허용·길이·제약과 predicate를 평가한다. `''`는 untyped NULL이 아니므로 BIGINT 대입이나 Int64 parameter에 빈 문자열 bind를 허용하지 않는다. nullable Utf8 parameter의 빈 입력은 typed NULL로 저장하고 NOT NULL 목적 parameter이면 bind 실패·기존 binding 전체 보존이다. binary의 빈 값·공백·U+0000에는 이 규칙을 적용하지 않는다.

이 규칙의 소유자는 SQL 입력 경계다. foundation/native의 `Utf8("")` 표현과 byte 비교를 바꾸지 않는다. SQL/native 동등 fixture는 SQL 빈 text 입력에 대응하는 native 값을 명시적인 typed NULL로 제공한다. native API가 직접 기록한 non-NULL 빈 Utf8는 SQL fetch·filter·sort에서도 원래 값으로 보존하고, 다른 열을 UPDATE할 때 복사하는 기존 열에도 재정규화를 적용하지 않는다. 조회 때만 NULL로 바꾸면 저장된 index·NOT NULL·UNIQUE와 어긋나므로 금지한다. 그러한 native 직접 입력은 Oracle 빈 문자열 의미 보장 밖이며 별도 교차 경계 vector로 검사한다. 모든 native 입력까지 정규화를 강제하는 schema 정책은 이번 범위가 아니다.

일반 비교에서 어느 쪽이 NULL이면 UNKNOWN이며 WHERE는 TRUE만 통과시킨다. NOT은 T→F/F→T/U→U, AND는 F가 있으면 F·모두 T면 T·나머지 U, OR는 T가 있으면 T·모두 F면 F·나머지 U다. IS NULL은 항상 T/F다. foundation의 SQL 비교와 total key ordering을 혼동하지 않는다.

CREATE TABLE은 1–32개의 서로 다른 열과 최대 하나의 PK를 받는다. PK 열의 nullable 생략은 binder가 NOT NULL로 확정해 engine schema에 넘긴다. 명시 NULL과 PK가 겹치면 정의 오류다. PK/UNIQUE/명시 index를 합해 table당 8개, 각 index는 1–4개의 서로 다른 열, encoded key는 RowId 제외 1,024 bytes 이하라는 DEV3 한도를 적용한다. PK/UNIQUE 제약 소유 index는 catalog 내부 identity로 관리하며 SQL의 DROP INDEX 대상으로 노출하지 않는다. 명시 CREATE UNIQUE INDEX는 DROP INDEX로 제거할 수 있다.

PK는 NULL 불가·유일, UNIQUE는 **구성 열 전체가 NULL일 때만** 중복을 허용한다. 부분 NULL composite는 NULL 위치까지 포함한 tuple이 같으면 충돌한다. `UNIQUE(a,b)`의 `(1,NULL)` 두 행은 실패, `(NULL,NULL)` 두 행은 성공이다. SQL 빈 text 입력은 앞의 정규화 뒤 검사하므로 NULL 규칙을 따르고 빈 binary는 실제 key 값이다. row·모든 index·catalog를 같은 engine statement/commit에 묶는다. PK/UNIQUE 포함 CREATE TABLE은 비공개 table/schema/index 정의를 하나의 catalog 거래로 준비해야 하며 여러 공개 DDL 호출로 쪼개지 않는다. 이 native API가 준비되지 않으면 SQL에서 에뮬레이션하지 않고 선행 계약 미충족으로 남긴다.

<a id="ddl-boundary"></a>
지원하는 DDL은 SQL session이 아래 순서로 조립한다. native DDL은 계속 일반 DML transaction 밖의 독립 원자 거래이며 engine 안에 숨은 commit을 추가하지 않는다. autocommit ON/OFF와 BEGIN 여부에 관계없이 같은 순서이고 autocommit 설정은 유지한다.

1. 입력 UTF-8·lexer/parser·지원 문법·session/handle·열린 result·실행 진입 전 취소를 검사한다. Syntax/Unsupported/ResultBusy 또는 이 단계의 limit/cancel은 효과 없이 기존 거래를 유지한다. 지원 밖 타입/구문과 PUBLIC 이외 namespace는 이 단계의 Unsupported다. 지원 밖의 Oracle 문장을 읽었다는 이유로 commit하지 않는다. type_length는 부호와 decimal digits의 형식만 parse하며 값 변환·1..65,536 범위는 3단계에서 검사한다. 따라서 VARCHAR(0), 음수·큰 정수 길이는 문법 거절이 아닌 precommit 뒤 Range다.
2. Active 거래가 있으면 SQL이 engine commit을 명시 호출한다. COMMITTED일 때만 DDL로 진행한다. undo까지 완료한 ABORTED이면 Idle, 결과 불명 UNKNOWN이면 Fenced다. commit되지 않았음이 확실해도 undo가 완료되지 못한 NotCommitted/RecoveryRequired이면 Fenced이며 Aborted나 Unknown으로 합치지 않는다. 이 세 실패 모두 DDL은 미시도다. commit 호출 중 취소는 engine 결과가 우선한다.
3. 이전 거래를 종료한 뒤 새 catalog에서 이름·객체 존재·타입/길이·제약 정의 등 semantic validation과 DDL 준비를 수행한다. 이후 권한 기능이 도입되면 그 검사도 이 경계다. catalog/의미 오류나 build/drain 실패가 나도 앞선 COMMITTED는 유지한다. 앞선 commit 이후 DDL 호출 전 취소는 DDL 미시도·이전 거래 COMMITTED·Idle이다.
4. native 독립 DDL을 commit하고 그 결과를 반환한다. 성공은 DDL COMMITTED·Idle, undo까지 완료한 확정 실패는 DDL ABORTED·Idle, 결과 불명은 DDL UNKNOWN·Fenced다. commit되지 않았지만 undo 미완료인 NotCommitted/RecoveryRequired는 Fenced로 전달한다. DDL 성공 뒤 별도 빈 commit을 한 번 더 수행하지 않는다.

모든 direct DDL의 성공/오류 결과에는 `prior_transaction_outcome`과 `ddl_outcome`을 분리한다. 전자는 `NotPresent | NotAttempted | Committed | Aborted | NotCommittedRecoveryRequired | Unknown`, 후자는 `NotAttempted | Committed | Aborted | NotCommittedRecoveryRequired | Unknown`이다. NotCommittedRecoveryRequired는 certainty=NotCommitted와 cleanup 미완료·RecoveryRequired/Fenced를 함께 보존한다. 첫 단계에서 Active 거래를 그대로 둔 경우 전자는 NotAttempted이며 transaction_state=Active다. semantic validation 실패로 native DDL을 아직 호출하지 않았다면 후자는 NotAttempted다. commit identity/receipt는 engine이 실제 제공한 경우만 연결하며 무변경 거래에 WAL/q를 만들지 않는다. 원인 code와 각 거래의 certainty/fencing을 보존하고, DDL 오류를 이전 거래까지 rollback한 것으로 표시하거나 DDL 문자열 전체를 자동 재시도하지 않는다.

index build는 DEV3 admission close/drain과 bounded sort를 따른다. DROP TABLE은 소유 index와 함께 제거하며 old snapshot/파일 회수는 engine이 소유한다. prepare handle만으로 장기 snapshot을 잡아 DDL drain을 막지 않는다. DDL 뒤 기존 prepared handle은 다음 execute의 dependency 검사로 판단한다.

<a id="execution"></a>
## 4. binder·planner·executor와 DML

기존 배치대로 `xadb-sql → xadb-engine, xadb-foundation`, `xadb-cli → xadb-sql, xadb-foundation, xadb-platform`을 사용한다. engine은 SQL을 import하지 않고 SQL은 private page/WAL/lock에 접근하지 않는다. platform 효과는 CLI/worker가 주입하며 SQL은 public engine open/create와 효과 전달을 조립하는 좁은 facade를 제공한다. 새 crate·worker·빌드 경로는 실제 구현 시 등록한다.

| 소유 책임 | 입력→출력·보장 |
| --- | --- |
| lexer/parser | 원문→span 있는 AST; bounded 입력·깊이, 파일 I/O·catalog mutation 없음 |
| binder | 하나의 catalog snapshot에서 이름→ObjectId/Generation/SchemaEpoch·열 ordinal·타입·metadata. AST를 실행하며 이름을 다시 찾지 않음 |
| planner | BoundStatement→typed plan/node ID·dependency 목록. engine public scan/index API만 사용 |
| executor | engine statement token과 snapshot/own-write view 고정→open/next/close; row/page guard를 외부에 빌리지 않음 |
| session | transaction·statement/result handle·binding·quota·취소 상태. durable outcome은 engine 결과 그대로 전달 |
| engine | statement token/undo mark·row/index mutation·unique/row lock·validation·commit/UNKNOWN·복구·retention |

초기 SELECT 기준선은 `RowScan → Filter → Sort(필요 시) → Project → Limit`이다. 정렬에는 선택하지 않은 입력 열도 사용할 수 있다. NULL 기본 위치는 ASC LAST, DESC FIRST이며 명시 FIRST/LAST가 우선한다. 내부 index의 NULL-first encoding을 그대로 SQL 순서로 노출하지 않는다. ORDER BY 없이는 순서를 보장하지 않고 동률의 상대 순서도 SQL 보장이 아니다. 실제 sorter는 RowId를 내부 tie-breaker로 사용해 재현성을 얻되 사용자 계약으로 승격하지 않는다. LIMIT은 0..Int64::MAX, parameter는 non-NULL Int64여야 한다. OFFSET은 미지원이다.

최소 access path 선택은 full scan, index point, forward index range다. AND의 항에서 `(열 = literal/parameter)`의 leading equality prefix와 다음 한 열의 `<,<=,>,>=` bounds만 추출한다. `IS NULL`은 NULL equality bound로 사용할 수 있다. 일반 `열 = NULL`을 IS NULL lookup으로 바꾸지 않는다. NULL bind의 일반 비교는 TRUE 결과가 없음을 유지한다. OR/NOT·열끼리 비교는 보수적으로 scan하며 모든 residual predicate를 재평가한다. 모순 bound는 empty, inclusive/exclusive composite prefix는 DEV3의 key 계약으로 표현한다.

후보 우선순위는 full-key unique equality → 가장 긴 equality prefix+다음 range → 가장 긴 equality prefix → RowScan이며 동일 후보는 index ObjectId로 고정한다. 통계 기반 비용 optimizer·index-only scan·sort 제거는 이 기준선에 포함하지 않는다. index path도 row fetch와 residual filter를 거치고 read-your-writes delta를 포함한다. 시험 전용 force-scan은 비교 입력이며 공개 SQL hint가 아니다.

unique index라는 이유만으로 모든 lookup을 최대1행으로 줄이지 않는다. `IS NULL` bound로 구성 열 전체가 NULL이면 중복 허용 key이므로 모든 visible RowId를 반환한다. full-key가 유일성을 보장하는 경우에만 단건 성질을 사용한다. D4-V25는 point/range 선택·force-scan·native 경로에서 이 구분을 확인한다.

INSERT는 열 목록이 필수이고 omitted nullable 열은 NULL, omitted nonnullable 열은 NotNull이다. tuple 수/열 수·중복 열·타입/길이를 검사한 뒤 VALUES 순서로 처리한다. UPDATE의 SET 우변은 literal/parameter만 받으며 중복 목적 열은 Name 오류다. affected_rows는 INSERT 추가 수, DELETE 삭제 수, UPDATE 조건에 매칭한 수이며 동일 값 대입도 센다.

UPDATE/DELETE는 **engine의 한 begin_statement token 아래 snapshot+앞선 성공 statement의 own-write view**에서 대상 RowUID·필요한 원래 값·engine이 발급한 opaque 논리 행 버전 observation을 제한된 목록으로 먼저 모은다. scan과 mutation은 같은 token을 공유하며 SQL이 새로운 문장 snapshot을 중간에 만들지 않는다. 전체 대상 수·bytes가 transaction의 남은 256 mutation·8 MiB private·1 MiB 논리 payload 및 engine undo/rollback reserve를 넘으면 statement를 실패시킨다. 실제 interleaved WAL/CLR 크기를 논리 payload 한도와 같다고 계산하지 않는다. LIMIT 없는 DML을 내부 여러 commit으로 나누지 않는다. 대상 확정 후 native cursor를 닫되 statement token은 유지하고 RowId 오름차순으로 한 execute_statement에 넘겨 각 행을 한 번만 변경한 뒤 finish_statement로 최종 제약을 판정한다. scan 도중 자기 UPDATE가 index를 옮겨 행을 반복 방문하는 Halloween 문제를 막는다. [DEV3 행 버전 규약](disk-storage-design.md#observed-row-version)에 따라 잠금 획득 뒤 대기 여부와 무관하게 관측 버전을 검사한다. cursor를 닫아도 observation과 statement token의 결합을 유지하며 SQL이 RowUID/값만으로 observation을 재구성하지 않는다. 버전 변경·삭제이면 해당 행 변경 전에 DEV3의 전체 거래 SerializationConflict/undo 결과를 전달한다. SQL이 predicate를 숨겨 재실행하거나 commit까지 검사를 미루지 않는다.

UNIQUE는 DEV3처럼 **statement의 최종 row/index delta**를 기준으로 검사한다. engine이 old/new key 잠금으로 직렬화한 다른 거래의 최신 committed 항목과 own final delta에서 영향받는 old entry를 제외하고 최종 new key를 검사한다. SQL statement snapshot만으로 유일성을 판정하거나 transaction 종료까지 미루지 않는다. 일시적 중복이나 RowId 처리 순서만으로 최종 유일한 상태를 거절하지 않는다. 복구 가능한 statement 오류는 이번 문장의 논리 행·모든 index delta를 되돌리고 statement staging을 정리하여 이전 성공 statement를 보존한 뒤 반환한다. quota는 실제 반환한 자원만 해제하며, WAL/CLR·회수 전 undo/page·완결 SMO·거래 잠금과 남은 abort 예약은 [DEV3 자원 회계](disk-storage-design.md#rollback-accounting)를 따른다. SQL은 논리 rollback 성공을 근거로 engine ledger를 과거 값으로 초기화하지 않는다. 실패 affected_rows는 성공한 부분 행 수가 아니라 오류이며 effect=statement rollback이다. 전체 거래 conflict나 undo I/O 실패에는 각각 transaction abort 또는 RecoveryRequired/Fenced가 우선한다. DML 대상으로 쓴 native cursor는 mutation 전에 닫는다. 취소/할당 실패와 engine 오류에서 rollback mark의 소유자는 engine이며 SQL이 부분 역연산을 새 SQL로 수행하지 않는다. native key 교환 성공은 하위 회귀로 보존하되 현재 상수 SET 문법이 key 교환 SQL을 지원한다고 주장하지 않는다.

<a id="prepared"></a>
## 5. typed prepared 실행·metadata·수명

기본 API 책임은 `prepare(sql, parameter_types?)`, `bind_all(handle, values)`, `execute(handle)`, `fetch(result, max_rows, max_bytes)`, `close_result`, `close_statement`다. `execute_direct(sql)`은 동일 parse/bind/plan/execute 경로를 일회성으로 사용한다. typed prepare는 SELECT/INSERT/UPDATE/DELETE만 받으며 DDL/transaction control은 direct 경로다. SQL 문자열에 binding 값을 합성하지 않는다.

`?`는 lexical 왼쪽부터 1..N으로 번호를 붙이며 같은 값의 반복 사용도 각 occurrence에 따로 bind한다. 최대 256개다. 이름·타입 이름·DDL·열 목록·ORDER BY 이름에는 marker를 허용하지 않는다. assignment 대상, 비교 상대의 concrete 타입, LIMIT의 Int64 문맥으로 타입을 추론한다. hint는 생략하거나 정확히 N개의 optional type을 주며 conflicting hint는 Type이다. `? IS NULL`, `? = ?`, `? = NULL`처럼 타입이 결정되지 않는 marker는 명시 hint가 없으면 ParameterTypeUnknown으로 prepare 실패한다. NULL literal 자체와 parameter를 구분한다.

parameter metadata는 ordinal, 논리 타입, 허용 NULL, 최대 bytes, 사용 위치를 포함한다. 기본 가변 값 상한은 65,536 bytes이고 assignment는 목적 열 길이로 좁힌다. nullable=false 목적 열과 LIMIT의 marker는 NULL 불가다. result metadata는 ordinal, 출력 이름, 논리 타입, nullable, 선언 byte 길이(가변 타입), 원본 table identity/열 ordinal을 포함하며 affected count는 result column이 아니다. `*`는 prepare catalog의 열 순서로 펼친다. metadata는 prepare 이후 불변이다.

binding은 타입 tag·NULL·명시 byte 길이를 가진 owned 값이다. `bind_all`은 정확히 N개의 타입·UTF-8을 확인하고 [빈 text 정규화](#text-boundary), NULL/길이 검사·예약·복사/소유권 이전 뒤 한 번에 교체한다. 실패하면 이전 binding 전체를 보존하며 부분 교체하지 않는다. NULL도 선언 타입과 일치해야 한다. UTF-8 오류·count/type/length/NULL 오류는 mutation 전에 반환한다. 성공한 binding은 caller 원본 buffer의 수명과 독립이며 반복 execute 사이 유지한다. 실제 저장 제약과 key encoding 크기는 execute 때 다시 확인한다.

| statement 상태 | 허용 동작·다음 상태 |
| --- | --- |
| ReadyUnbound | bind 성공→ReadyBound; N=0이면 prepare 즉시 ReadyBound. execute는 BindCount 오류 |
| ReadyBound | bind 성공→교체; DML execute 성공/복구 가능한 오류 뒤 ReadyBound; SELECT execute 성공→ResultOpen |
| ResultOpen | 해당 result fetch/close만 실행. 같은 session의 새 execute·direct·bind·prepare·COMMIT은 ResultBusy. ROLLBACK/close_session은 result 취소 뒤 수행 |
| ResultOpen→EOF | 결과 완료 상태를 반환하고 pin/scratch 해제, statement ReadyBound. result token은 이후 EOF 조회 또는 close만 허용 |
| ResultOpen→복구 가능한 fetch 오류 | 미완료 terminal error 저장, pin/scratch 해제, statement ReadyBound. 이후 fetch는 같은 terminal error만 반환, 새 execute에는 사용 불가 |
| ResultOpen→close_result | 미완료이면 취소·해제, statement ReadyBound, result Closed. EOF 이후 close도 같은 Closed 상태이며 fetch는 HandleClosed |
| Invalid | stale 검사 실패→bindings·metadata는 조회 가능하나 재bind/execute 불가. close 후 새 prepare 필요 |
| Closed | terminal. 같은 session의 close는 멱등; 다른 사용은 HandleClosed. 다른 session/engine token은 InvalidHandle |

session당 활성 result는 하나다. **statement당 마지막 result descriptor 한 개만** 보존하며 EOF/error/closed 기록도 prepared 64개·8 MiB/session 예산 안에서 센다. 다음 execute 또는 statement close는 이전 terminal result를 폐기하고 generation을 전진시킨다. 과거 result는 InvalidHandle이다. direct 실행도 session당 마지막 terminal descriptor 한 개만 보존하고 다음 direct 실행에서 교체한다. 따라서 반복 SELECT/EOF가 descriptor를 누적하지 않는다.

handle은 session incarnation+slot generation을 검증하며 정수 재사용으로 과거 handle을 부활시키지 않는다. 멱등 close는 해당 slot의 현재 closed generation에만 적용하며 slot 재사용 뒤 과거 generation은 InvalidHandle이다. closed tombstone도 고정 slot 수 안에서 관리한다. session operation은 하나씩 직렬 실행하고 cancel token 신호만 별도 thread에서 허용한다. 실행 중 close/bind 경쟁은 Busy이며 내부 상태를 동시 변경하지 않는다.

prepare는 active transaction이면 그 catalog, idle이면 짧은 read-only snapshot으로 bind하고 pin을 즉시 해제한다. idle prepare가 autocommit OFF의 transaction을 시작하지 않는다. execute는 실제 실행 transaction과 engine statement token을 고정하고 그 catalog에서 DB/timeline/open instance, table ObjectId/Generation/SchemaEpoch, 사용한 index identity와 table의 index 정의 집합을 다시 검사한다. 단순 data commit만으로 plan을 무효화하지 않는다. index 정의 집합은 catalog descriptor로 대조하며 root/page 주소를 영구 plan에 저장하지 않는다. SELECT는 EOF/close/error까지 token의 snapshot을 유지하고 finish/취소 경로에서 해제한다. prepared handle 자체가 token을 보유하지 않는다.

drop/recreate·schema/index 정의 변경은 StalePlan으로 **첫 row 접근/효과 이전** 거절한다. 이름으로 자동 재bind하지 않는다. 검사와 실행은 동일 pinned transaction view를 사용해 DDL 사이의 TOCTOU를 막는다. 기존 transaction의 old catalog가 유효하면 그 view에 대해 판단하되 실제 offline DDL drain 규칙을 우회하지 않는다. 새 prepare는 최신 metadata를 반환하고 기존 handle metadata를 조용히 변경하지 않는다.

idle prepare의 짧은 catalog snapshot과 execute의 최초 transaction/statement snapshot은 [DEV3의 q 선택·horizon 등록 규약](disk-storage-design.md#snapshot-acquisition)을 소비한다. SQL이 published q를 먼저 읽어 보관했다가 나중에 pin을 요청하지 않는다. 기존 transaction view를 statement/result에 넘길 때도 engine guard의 소유를 연속 유지하며 raw q만으로 snapshot을 재생성하지 않는다. 실제 수신과 최초 획득/aging 경쟁은 D4-F02·T09에서 확인한다.

fetch는 owned row batch와 `complete` flag를 반환한다. max_rows=1..128, max_bytes=256 KiB 이내이며 현재 행이 caller 예산보다 크면 RowBufferTooSmall과 필요한 크기를 반환하고 위치를 전진시키지 않는다. 정상 EOF 또는 LIMIT 도달만 `complete=true`다. I/O·취소 오류나 조기 close는 미완료 결과이며 이미 전달한 prefix를 전체 성공으로 표시하지 않는다. `close_result`는 result의 자원만 정리하고 명시 transaction의 기존 DML을 commit하지 않는다. `close_statement`는 소속 result를 닫고 binding·plan을 해제한다. 명시 close_session/ROLLBACK은 engine undo 완료를 확인하며 실패는 호출자에게 반환한다. Drop은 hidden commit·새 allocation·panic 없이 이미 예약한 engine abort/수명 경로로 소유를 넘기거나 engine을 fence한다. undo/lock 소유를 단순히 버리거나 cleanup 미완료를 정상 rollback으로 표시하지 않는다. 구체 인계 API는 D4-F02에서 동결한다.

ROLLBACK은 active result를 닫고 generation을 전진시켜 기존 result token을 InvalidHandle로 만든다. 소유 statement는 ReadyBound로 돌아가며 새 transaction에서 dependency 검사를 다시 받아야 한다. close_session은 모든 statement/result를 무효화한다. terminal 오류 기록에는 code/effect만 제한된 크기로 보존하고 원문 row·spill·snapshot을 붙잡지 않는다.

<a id="transactions"></a>
## 6. session·transaction·오류 효과

새 session은 autocommit ON·Idle이다. BEGIN/OFF에서 시작하는 거래는 ReadWrite이며 read-only SQL만 실행해도 engine의 무변경 ReadWrite validation을 따른다. ON의 단독 SELECT만 ReadOnly를 사용한다.

현재 SQL의 격리 선택은 transaction snapshot + DEV3의 table read/write-set 검증 SERIALIZABLE뿐이다. 같은 table의 서로 다른 행 writer는 보수적으로 충돌할 수 있지만, 서로 다른 table이고 공통 catalog/읽기 의존이 없는 writer는 모두 commit할 수 있다. OFF/BEGIN의 read-only SQL만 수행한 ReadWrite 거래도 읽은 table이 바뀌면 종료 시 충돌할 수 있다. 이는 Oracle의 기본 READ COMMITTED와 다르다. DEV3-DESIGN-003은 native ReadCommitted의 문장별 snapshot·자기 변경 합성·행/키 잠금을 설계하지만, 관측 버전 변경 시 전체 abort하는 보수적 경계가 있어 Oracle과 동일하지 않다. 실제 native 두 isolation 검증 뒤 SQL isolation 문법/default와 오류·cursor 수명을 별도로 동결한다. 그 전에는 SQL flag만으로 Read Committed를 제공하거나 기존 snapshot을 문장마다 바꾸지 않는다. 후속 의무는 [구현 계획](../plans/dev4-implementation-plan.md#oracle-followup)에 남긴다.

lex/parse·handle·binding 개수/타입·session 상태 검사는 새 transaction을 만들기 전에 수행하며 이 단계 오류는 기존 Idle/Active를 유지한다. SELECT/DML execute가 실제 transaction을 시작한 뒤의 catalog bind/dependency 검사·statement/fetch 오류는 아래 Active 규칙을 따른다. OFF에서 이 호출이 처음 거래를 만들었어도 복구 가능한 오류 뒤 Active이며 transaction_state에 이를 반환한다. ON에서 만든 implicit 거래는 해당 실행의 terminal 오류면 종료하고, 기존 명시/OFF 거래는 복구 가능한 statement/fetch 오류 뒤 유지한다. API 진입 거절과 RowBufferTooSmall은 기존 거래/result를 유지한다. prepare 자체는 idle user transaction을 만들지 않는다. direct DDL은 [별도 전 commit 경계](#ddl-boundary)를 따르며 DDL 실패의 효과를 일반 DML 오류 규칙으로 축소하지 않는다.

| 현재 상태/입력 | 전이·관측 결과 |
| --- | --- |
| Idle + BEGIN | Active 명시 거래. nested BEGIN은 TxnState, 기존 거래 유지 |
| Idle ON + DML | implicit ReadWrite→한 statement→commit. COMMITTED 확인 뒤 성공 count 반환 |
| Idle ON + SELECT | implicit ReadOnly→cursor snapshot 유지→EOF/close/error에서 해제. durable commit receipt 없음 |
| Idle OFF + SELECT/DML | 첫 execute에서 Active 거래 시작. EOF/statement 성공 뒤도 Active |
| Active + COMMIT | 열린 result 없을 때 engine commit으로 종료. COMMITTED/ABORTED/NotCommittedRecoveryRequired/UNKNOWN 구분 |
| Active + ROLLBACK | 열린 result 취소 후 engine undo·abort-end 완료와 lock/pin 해제 확인→Idle. undo 실패는 NotCommitted/RecoveryRequired·Fenced. commit 시도 중에는 rollback 성공을 만들지 않음 |
| Idle + COMMIT/ROLLBACK | TxnState 오류 |
| Idle + SET AUTOCOMMIT | ON/OFF 설정; Active이면 TxnState로 거절 |
| Idle + DDL | prior=NotPresent, 독립 engine DDL. autocommit 설정 유지 |
| Active + DDL | 진입 거절이면 Active 유지. 이전 거래 commit 진입 뒤는 성공 때만 새 catalog에서 독립 DDL. 두 outcome 분리, 마지막 상태 Idle 또는 Fenced |
| close_session | result/statement 무효화·미commit 거래의 engine rollback 완료 확인. undo 실패는 RecoveryRequired·Fenced를 반환하고 engine의 복구 소유 유지. 이미 알려진 commit 결과를 변경하지 않음 |

SQL 오류는 stable code, phase(lex/parse/bind/execute/fetch/precommit/commit), source span(가능할 때), statement_effect, transaction_state, certainty, retry_scope, 원인 engine code를 가진다. direct DDL은 두 outcome과 각각의 certainty를 추가하며 precommit 뒤 Name/Type/Range 오류여도 이전 거래 Committed를 보존한다. foundation의 Effect/Certainty/RetryScope 의미를 보존하고 SQL 상세 필드는 adapter에서 제공한다. SQLSTATE·driver 매핑은 DEV6에서 이 의미를 보존해 고정하며 임의 SQLSTATE를 지금 약속하지 않는다.

| 코드 접두어 `XADB-SQL-` 뒤 이름 | 최초 검출 시점 | 효과·후속 처리 |
| --- | --- | --- |
| SYNTAX, UNSUPPORTED, NAME, TYPE, RANGE | parse/bind; 값 범위는 bind/execute | SELECT/DML은 미실행 또는 statement 복원·이전 성공 변경 유지. DDL은 별도 두 outcome으로 이미 끝난 precommit을 보존 |
| BIND-COUNT, PARAMETER-TYPE-UNKNOWN | bind/prepare | 효과 없음, 기존 binding/거래 유지 |
| NOT-NULL, UNIQUE, INDEX-KEY-TOO-LARGE | bind/execute/build | 이번 statement/build 전체 복원 |
| LIMIT, CANCELLED, TIMEOUT | 준비/execute/fetch, commit 전 | 일반 statement는 복원; implicit 거래는 종료. COMMIT 호출 중 pre-attempt 취소는 전체 undo 완료 뒤 ABORTED, undo 실패이면 RecoveryRequired·Fenced |
| TXN-STATE, RESULT-BUSY | API/session 진입 | 효과 없음, 기존 거래/result 유지. native DDL-in-txn은 engine 불변식이며 SQL direct DDL의 정상 거절 사유가 아님 |
| INVALID-HANDLE, HANDLE-CLOSED, STALE-PLAN | API/execute dependency 검사 | 효과 없음. stale statement만 Invalid |
| ROW-BUFFER-TOO-SMALL | fetch | cursor 유지, 같은 위치로 큰 budget 재요청 가능 |
| CONFLICT | row X lock 획득 뒤 논리 버전 검증(대기 유무 무관) 또는 commit validation | engine의 전체 undo 완료 후 ABORTED→Idle, 새 transaction 전체 재시도 판단. undo 실패/Fenced를 ABORTED로 낮추지 않음 |
| RESULT-UNKNOWN | CommitAttempted 뒤 불확실한 I/O/응답 | UNKNOWN 유지·engine Fenced, 자동 retry/rollback 금지 |
| ENGINE | RecoveryRequired 등 기타 typed engine 오류 | 원래 effect/certainty/fencing을 전달. NotCommitted/RecoveryRequired는 undo 미완료·Fenced이며 ABORTED나 UNKNOWN으로 합치지 않음. 손상·불변식·metadata 불명은 복구 가능한 SQL 오류로 낮추지 않음 |

cancel은 row/parse chunk·sort run·I/O wait의 bounded checkpoint에서 관측한다. 일반 DML 중 취소는 statement rollback 후 명시 거래를 유지할 수 있지만 COMMIT 진입 이후에는 engine commit 상태가 우선한다. sync 성공 뒤 취소 신호가 왔다고 committed를 ABORTED로 바꾸지 않는다. Fenced session은 새 execute를 받지 않고 close/진단만 허용한다. reopen은 새 engine/session/handle이며 outcome 확인·중복 요청 정책은 후속 서버 계약과 구분한다.

<a id="operation-context"></a>
SELECT의 statement/snapshot token 수명은 execute 호출의 취소·deadline 수명과 다르다. result는 snapshot·plan·읽기 위치를 보존하고, **각 execute/fetch/close 호출은 자기 operation generation·cancel token·deadline을 전달**한다. 완료된 execute/fetch의 deadline이나 cancel을 result에 영구 저장하지 않는다. 동일 호출이 중단/재개될 때에는 그 호출의 context를 유지하여 deadline을 연장하지 않지만, 이후 fetch에는 새 context를 사용한다. session close·거래 수명 만료는 별도 상위 cleanup 신호이며 context 교체로 지워지지 않는다. embedded API와 DEV5 adapter가 같은 규약을 소비하고 정확한 Rust signature는 F02에서 동결한다.

fetch terminal 오류는 위 result 상태표에 따라 cursor를 닫는다. 단, RowBufferTooSmall은 재시도 가능하여 cursor를 유지한다. engine이 transaction abort 완료를 확인하면 session은 Idle로, Fenced를 요구하면 Fenced로 전이하고 모든 active result를 무효화한다. 오류표의 statement 복원·거래 종료·lock 해제는 engine cleanup 완료가 전제이며 undo I/O 실패에는 RecoveryRequired/Fenced가 우선한다. statement를 ReadyBound로 돌리는 것으로 engine fencing을 해제하지 않는다.

<a id="recovery-boundary"></a>
SQL은 commit 결과·statement 원자성·취소 후 효과를 소비하며 page redo·undo·CLR을 소유하지 않는다. DEV2 전용 private/COW에서는 미공개 변경을 버리지만, 공급 대상 DEV3-DESIGN-003에서는 WAL-before-page·pageLSN·영속 UndoRef·redo-only CLR·checkpoint/로그 보존·doublewrite와 반복 장애가 필수다. rollback이 I/O 오류로 끝나지 못하면 engine의 RecoveryRequired/Fenced를 보존하고 statement 복원 성공/ABORTED로 답하지 않는다. ARIES의 WAL 역방향 undo chain과 DEV3의 영속 undo chain을 동일 알고리즘이라고 하지 않으며 Oracle 내부 구현과의 동일성도 주장하지 않는다. crash 뒤에는 [DEV3 영속 예약 인계](disk-storage-design.md#recovery-reservation)가 bootstrap/loser 예약을 복원한 실제 증거를 수신한다. SQL이 복구용 reserve를 새로 배정하거나 engine의 open 거절을 정상 rollback으로 바꾸지 않는다. 실제 공급 API·fault schedule은 D4-F01/F02/F04에서 다시 동결하고 SQL의 원자성·ACK 보존·UNKNOWN 의무를 실제 disk 경로로 검증한다.

<a id="resources"></a>
## 7. bounded 실행·정렬·진단

아래는 초기 개발 한도이며 제품 최대 용량/SLA의 측정 결과가 아니다. 모든 count/length 산술은 할당 전에 checked 연산하고 초과는 typed error다. 데이터/statement가 크다고 조용히 잘라 성공시키지 않는다.

| 자원 | 초기 상한·회계 |
| --- | --- |
| 한 SQL 입력 | UTF-8 1 MiB, token 65,536개, AST node 8,192개, nesting 64(괄호·연속 NOT 등 모든 parser 재귀 포함). CLI batch 전체는 streaming |
| 열·projection·ORDER BY | table 32열, projection/정렬 각각 32항, projected canonical row 128 KiB |
| prepared | session당 64개, parameter 256개/statement. SQL text·AST/plan·metadata·binding 합계 8 MiB/session |
| 활성 실행 | session당 1개·workspace 8 MiB. 그 안에 sort run 4 MiB·반환 batch 256 KiB 포함 |
| SQL 총 memory | DB당 compiled/bound+active SQL 예약 합계 64 MiB. engine의 buffer/metadata/private/system budget과 별도로 더해 RSS를 관측 |
| temporary sort disk | query당 256 MiB, DB당 1 GiB. 해당 볼륨의 여유/전체 disk admission에도 반영, engine retained-data quota에서 숨기지 않음 |
| sorter 파일 | fan-in 8, query당 열린 임시 FD 최대 10, SQL 전체 임시 FD 64. OS/engine FD 예산과 함께 합산 |
| transaction | DEV3의 256 mutation·논리 payload 1 MiB·private 8 MiB, user snapshot/pin 32 유지. undo/CLR/WAL 예약은 engine 소유이며 SQL이 별도 추가 용량처럼 중복 배정하지 않음 |

sorter는 정렬 key+RowId+projected values를 bounded run으로 기록하고 fan-in 8 merge를 반복한다. header·key·값을 포함한 spill record는 최대 256 KiB이고 확장한 정렬 key가 이 한도를 넘으면 Limit이다. merge는 입력별 현재 record 하나와 출력 buffer만 유지해 최대 record에서도 4 MiB sort 예산 안에 들어가도록 예약한다. 전체 run 목록은 디스크 manifest에서 순회하고 메모리에 모든 run/file 이름을 모으지 않는다. merge 단계의 입력+출력 공존 peak를 disk quota에 포함한다. 한 row가 workspace/결과 한도를 넘으면 Limit이며 끝없는 retry를 하지 않는다. 정렬 완료 전 결과를 반환하지 않고 이후 fetch는 bounded batch로 읽는다. LIMIT top-N 최적화는 필수가 아니므로 LIMIT이 작아도 정렬 quota 초과가 가능하다.

임시 파일은 engine data/undo 파일·WAL과 분리한 소유 run namespace에 배타 생성한다. 파일 format/version·record length·checksum·count를 검사하며 corrupt/truncated spill을 정렬 성공으로 처리하지 않는다. spill은 durable DB 상태가 아니므로 commit dependency나 recovery redo에 넣지 않는다. query/result close·취소·오류 때 닫고 삭제하며 삭제 실패 bytes는 실제 회수까지 quota에 남긴다. crash 뒤에는 writer lease 확보 후 소유 manifest로 확인한 임시 namespace만 bounded cleanup한다. 알 수 없는 파일은 지우지 않는다. 임시 codec byte vector와 cleanup API는 D4-F03에서 구현 전에 고정한다.

prepare·execute·plan/node ID에 parse/bind/plan 시간, rows/loops, visited row/page, predicate 통과 수, sort run/spill bytes·merge pass·memory peak, storage I/O와 commit outcome을 연결한다. parameter 원문 값을 기본 진단에 넣지 않는다. 계측 off/상시/상세와 유실을 구분하며 미계측을 0으로 보고하지 않는다. PF10의 실제 release 측정과 독립 결과는 [검증 계획](../plans/dev4-implementation-plan.md#performance)을 따른다.

<a id="aggregate-pressure"></a>
### SQL 합산 예약과 교체 peak

위 DB당 memory/disk/FD 상한은 session 수나 후속 연산자 수를 곱해 늘리는 예산이 아니다. 각 query의 sort/merge/input/output와 retained prepared/binding/result가 동시에 존재하는 peak를 합산한다. `bind_all`의 이전 binding을 보존한 채 새 binding을 준비하는 구간도 old+new 공존량을 먼저 예약하며, 부족하면 기존 binding을 그대로 유지하고 거절한다. spill 전환도 run 생성/merge에 필요한 실제 workspace·FD·입출력 파일 공존량 안에서만 가능하고, 추정 cardinality가 실제 자원 한도를 대신하지 않는다.

여러 session의 SQL memory 또는 임시 disk/FD가 동시에 부족할 때 무제한 대기/재시도하지 않는다. 첫 effect 전 거절 또는 이미 실행 중인 statement/result의 기존 오류·cleanup 규약을 적용한다. 이미 보유한 자원은 그 소유 수명이 끝날 때 반환하며 실패한 unlink의 bytes는 계속 charge한다. cleanup에 필요한 상태·작업 공간은 해당 실행 admission에 포함하여 새 foreground quota 확보에 의존하지 않는다. engine의 rollback/flush 예약은 별도 보존한다. 조인·hash aggregate·parallel operator를 추가할 때도 이 합산식을 확장하고 실제 spill 의미·취소·cleanup을 별도 동결한다. 이번 DEV4에서 그 연산자들을 구현한 것으로 보지 않는다.

<a id="capacity-boundary"></a>
### 7.1 초기 처리 범위와 저장 경로 인수

| 현재 선택 | 관측 가능한 한계·계측 | 확대 전에 필요한 것 |
| --- | --- | --- |
| table 단위 Serializable | 같은 table의 서로 다른 행 writer도 충돌 가능. 성공 TPS와 시도 TPS·충돌률을 분리 | finer validation 또는 SQL ReadCommitted의 별도 의미/동시성 검증 |
| 거래당256 mutation·논리1 MiB·대상 선확정 | 한도를 넘는 UPDATE/DELETE는 Limit이며 문장 전체 실패. 앞선 성공 statement는 유지 | 큰 거래·spill 대상 목록·undo/CLR reserve의 별도 계약; 몰래 여러 commit으로 분할하지 않음 |
| 규칙 기반 access path·항상 필요한 외부 sort | 편향·낮은 선택도에서 index가 scan보다 비쌀 수 있고 작은 LIMIT도 정렬 quota 초과 가능 | 실제 node/storage 비용과 독립 동등 결과를 근거로 비용 선택·top-N·sort 제거 검토 |
| DEV3의 짧은 적용 직렬화·bounded DWB batch | SQL 동시성/응답 비용에 gate·flush 대기가 포함됨 | parse/bind와 gate/lock/flush/commit 비용 분리. SQL에서 저장 guard·reserve 우회 금지 |

위 한계는 오류 효과가 명확한 초기 개발 profile이며 일반 DBMS의 처리 용량이나 SLA가 아니다. D4-F02/F05는 [DEV3 변경 프로토콜](disk-storage-design.md#mutation-protocol), [자원 예약](disk-storage-design.md#reserve-accounting), [batch flush](disk-storage-design.md#doublewrite-batch)의 실제 공급 revision·증거를 확인한다. SQL의 transaction/statement token은 engine이 보유한 undo·lock·예약 소유와 함께 종료/인계되어야 한다. 낮은 자원에서 cancel/close/rollback을 요청한 뒤 정상 종료와 Fenced를 구별하는 시험을 포함한다. 미구현 F01–F05·모델·disk 인수는 이 문서 보완만으로 완료되지 않는다.

<a id="vectors"></a>
## 8. 구현 전에 보존할 독립 의미 vector

모두 **PLANNED_NOT_RUN**이다. 각 행은 새 fixture/독립 case이며 native oracle에 기대값을 literal로 넣고 SQL evaluator/production key codec으로 만들지 않는다. 기본 fixture는 `t(id BIGINT NOT NULL, k BIGINT, s VARCHAR(6), b BOOLEAN, v VARBINARY(2), PRIMARY KEY(id), UNIQUE(k)) STORAGE DISK`와 SQL 입력 `(1,10,'가나',FALSE,X'00FF'), (2,NULL,'',TRUE,X''), (3,NULL,NULL,NULL,NULL)`이다. native catalog 이름은 `T/ID/K/S/B/V`이며 두 번째 행의 S는 typed NULL, V는 길이 0의 Binary다. fixture의 DDL/초기 행은 먼저 commit하고 각 case를 시작한다.

| ID | 입력·조건 | 독립 기대값·효과 |
| --- | --- | --- |
| D4-V01 | `SELECT id FROM t WHERE k = NULL` / `WHERE k IS NULL ORDER BY id` | 각각 빈 결과 / `[2,3]` |
| D4-V02 | `WHERE NOT (k = 10) ORDER BY id` / `WHERE k = 10 OR k IS NULL ORDER BY id` | 각각 빈 결과 / `[1,2,3]`; T/F/U 3×3 AND/OR와 NOT 전체 vector 추가 |
| D4-V03 | `ORDER BY k ASC, id` / `ORDER BY k DESC, id` | id 순서 `[1,2,3]` / `[2,3,1]`; 명시 NULLS FIRST/LAST도 검사 |
| D4-V04 | 새 행에 `'가나다'` / `'가나'` / `''` / `X''` | VARCHAR(6) 앞 세 대입은 각각 Range / 성공 / typed NULL; 빈 Binary는 별도 실제 값 |
| D4-V05 | `BEGIN; UPDATE t SET b=TRUE WHERE id=1; INSERT INTO t(id,k) VALUES(4,20),(5,10)` | INSERT 전체 UNIQUE 실패, 4·5 둘 다 없음. 앞선 id=1의 b=TRUE 유지, COMMIT 가능 |
| D4-V06 | `UPDATE t SET k=20 WHERE id=1 OR id=2` | 문장 최종 집합 UNIQUE 실패, 전체 statement 복원, 원래 k=10/NULL 유지. 특정 mutation의 검사 시점에 기대값을 묶지 않음 |
| D4-V07 | `prepare('SELECT id FROM t WHERE id >= ? ORDER BY id LIMIT ?'); bind [Int64(2),Int64(1)]` | metadata 둘 다 Int64, LIMIT NULL 불가; 결과 `[2]`. 문자열 `"2"` bind는 Type·기존 binding 유지 |
| D4-V08 | `prepare('SELECT id FROM t WHERE ? IS NULL')` | hint 없으면 ParameterTypeUnknown; Int64 hint + typed NULL이면 `[1,2,3]` multiset |
| D4-V09 | ReadyBound에서 잘못된 개수/UTF-8/길이 bind, 열린 result에서 재bind/execute, EOF 후 재execute | 실패 binding 원자 보존 / ResultBusy / 같은 binding으로 새 snapshot 실행 |
| D4-V10 | prepare 뒤 idle에서 DROP/같은 이름 CREATE, 또는 선택 index DROP | StalePlan, 이전 metadata 유지, effect 없음. 새 prepare만 새 identity 사용 |
| D4-V11 | 같은 q에서 BEGIN한 두 session의 같은 table 서로 다른 행 UPDATE; 별도 fixture의 공통 catalog/읽기 의존 없는 서로 다른 table UPDATE | 같은 table은 첫 COMMITTED·두 번째 CONFLICT/ABORTED, 독립 table은 모두 COMMITTED. row/unique lock 충돌은 별도 구분. ACK history와 reopen 대조 |
| D4-V12 | text parameter에 quote·semicolon·`--` 포함, caller buffer 즉시 변경/해제 | SQL 재해석 없음, bind 시 owned bytes 그대로 비교 |
| D4-V13 | `LIMIT 0` + 존재하지 않는 열, Int64 최소/최대±1, index encoded key 1024/1025 | 이름 오류 숨김 없음, 정수·encoded key 경계의 성공/거절 정확히 구분 |
| D4-V14 | fetch prefix 뒤 cancel/spill I/O 오류/close, 재open, stale result 재사용 | 미완료 표시, pin/FD/quota 회수; 이전 결과 token 거절. 과거 prefix를 전체 성공으로 판정하지 않음 |
| D4-V15 | 한 prepared SELECT를 execute→EOF로 10,000회 반복하며 과거 token 보관, close/slot 재사용 | statement당 terminal descriptor 1개, 전체 memory 상한 유지. 이전 generation은 InvalidHandle, 현재 closed generation의 close만 멱등 |
| D4-V16 | BEGIN→UPDATE 성공→SELECT fetch 취소→COMMIT; ON implicit UNIQUE/stale 오류; OFF 첫 execute stale 오류 | 각각 result 해제·앞선 UPDATE commit 가능 / Idle·pin 해제 / Active 유지·effect 없음·ROLLBACK 가능. RowBufferTooSmall은 cursor 위치 유지 |
| D4-V17 | 별도 `u(a BIGINT,b BIGINT,UNIQUE(a,b)) STORAGE DISK`에 `(1,NULL)` 두 행 / `(NULL,1)` 두 행 / `(NULL,NULL)` 두 행 / `(1,NULL),(2,NULL)` | 독립 새 fixture별 앞 둘은 UNIQUE·문장 전체 복원, 뒤 둘은 성공. SQL/native/index build/reopen의 같은 의미 |
| D4-V18 | 빈 catalog의 별도 fixture에 `CREATE TABLE t (id BIGINT) STORAGE DISK` 뒤 `t`, `T`, `"T"`, `"t"` 조회와 `SELECT id AS alias FROM t` prepare | 앞 셋 같은 identity, 마지막 Name. metadata 이름은 ID/ALIAS. `public`, `PUBLIC`, `"PUBLIC"` 같은 prefix, `"public"`은 Unsupported |
| D4-V19 | 기본 fixture의 `WHERE s IS NULL ORDER BY id` / `WHERE s = ''`; `? IS NULL`에 Utf8 hint+empty bind; NOT NULL text 목적 empty bind; `''`의 BIGINT 대입 | `[2,3]` / 빈 결과; TRUE이므로 전체 행 통과; NULL 검증 실패·이전 binding 보존; Type. `'' = ''`도 UNKNOWN, 공백·U+0000·빈 Binary는 NULL 아님 |
| D4-V20 | native로 `Utf8("")`를 별도 nullable indexed text 열에 직접 기록, SQL scan/index/force-scan·fetch·`IS NULL` 대조 | fetch는 non-NULL 빈 text 그대로, IS NULL은 해당 행 제외. 조회 정규화로 NULL index 결과가 달라지는 mutant 거절. 이 native 입력은 Oracle 빈 text 보장 밖 |
| D4-V21 | BEGIN→UPDATE t 성공→문법적으로 유효한 동명 CREATE TABLE t 실패→재시작; 별도 case로 VARCHAR(0/-1/65537/Int64 초과 길이), 잘못된 문법/미지원 namespace/ResultBusy | 동명 case prior=Committed·ddl=NotAttempted·Name·Idle, UPDATE는 재시작 후 보존. 길이 case도 prior=Committed·Range·Idle. 뒤 진입 거절 case는 prior=NotAttempted·ddl=NotAttempted·Active, 기존 변경 미commit |
| D4-V22 | DDL precommit의 conflict/undo 실패/UNKNOWN, 또는 precommit 성공 뒤 DDL 전 cancel/DDL build 실패/DDL undo 실패/DDL UNKNOWN | 각각 prior=Aborted/NotCommittedRecoveryRequired/Unknown이고 ddl=NotAttempted·Idle/Fenced/Fenced. 성공 뒤는 prior=Committed 유지, ddl=NotAttempted/Aborted/NotCommittedRecoveryRequired/Unknown·Idle/Idle/Fenced/Fenced. undo 실패의 확정 NotCommitted와 cleanup 미완료 보존. 원래 SQL 전체 자동 재시도 금지 |
| D4-V23 | S1 성공→S2 중간 key/quota 실패 또는 최종 UNIQUE 실패→문장 rollback 완료→COMMIT ACK→kill/reopen/복구 재장애 | S1의 row/index만 보존, S2는 없음. 별도 case로 문장 rollback 중 COMMIT 전 kill이면 S1도 포함한 해당 미commit 거래 전체 없음 |
| D4-V24 | DML commit durable 뒤 응답 전 kill, DDL precommit durable 뒤 DDL 호출 전 kill, DDL durable 뒤 응답 전 kill | 외부 응답은 UNKNOWN일 수 있으나 각각 거래 전체 / 이전 DML만 / 이전 DML+DDL 보존. 두 commit identity와 복구 cut를 분리하며 반복 reopen에서도 누락·중복 없음 |
| D4-V25 | 기본 fixture에서 `WHERE k IS NULL`; 별도 `u(id BIGINT NOT NULL,a BIGINT,b BIGINT,PRIMARY KEY(id),UNIQUE(a,b))`에 `(1,NULL,NULL),(2,NULL,NULL)`을 넣고 `WHERE a IS NULL AND b IS NULL ORDER BY id` | 각각 id bag `[2,3]` / ordered `[1,2]`. unique point/range·force-scan·native·reopen 모두 동일하며 단건 조기 종료 mutant 거절 |
| D4-V26 | T1이 UPDATE/DELETE 대상을 관측한 뒤 T2가 같은 행 변경/commit/잠금 해제, 그 뒤 T1이 즉시 X lock 획득 | DML execute에서 해당 행의 첫 변경 전 전체 거래 conflict·undo 완료 후 ABORTED, T1의 앞선 변경도 소멸·T2 ACK 보존. 대기 경로·삭제·동일 값의 새 버전·own write·split 변형을 T09와 native DS06에서 대조 |
| D4-V27 | S1 성공 뒤 새 row/key 잠금을 얻고 WAL을 쓴 S2가 실패→statement rollback을 반복; WAL floor를 다른 owner가 pin | S1 값 유지, S2 값 없음. 실제 잔류 WAL/CLR·undo/page·잠금은 계속 charge하며 다음 statement admission이 사전 거절될 수 있음. 이미 예약한 전체 ROLLBACK은 진행하고 COMMIT 시도는 기존 certainty/실패 규약을 유지. ledger 과거값 복원·소비 CLR 예약 재사용 mutant 거절 |
| D4-V28 | SELECT execute 완료 뒤 그 호출 deadline만 경과하고 유효 result를 fetch; 다음 fetch의 중단/재개·취소 | 최초 snapshot/위치 유지, 새 fetch는 새 operation context 사용. 이전 호출 cancel/deadline 영향0, 현재 fetch 취소는 미완료 result 해제와 기존 거래 효과를 따름. session/transaction 만료는 별도 우선 |

추가 필수 교차 조합·crash cut·부정 oracle·CLI/PF10은 구현 계획의 Q1 행렬이 소유한다. 위 예시만 통과해 Q1을 완료할 수 없다.

<a id="decision"></a>
## 9. DEV4-DESIGN-001 결정 기록

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당·권한 | 작업·SQL 계약·통합 `/root`. 사용자의 DEV4 설계 요청과 DP01의 기존 D2 위임. 제품 방향·단계 선행·내구성 의무 유지 |
| 선택·대안 | 1절의 유한 문법·typed pull 실행·owned binding·명시 stale 거절·외부 sort. 공개 소스의 책임 분리를 참고하되 코드는 도입하지 않음 |
| 적용·대체 | 원래 DEV4 기본 설계. 당시 mutation별 UNIQUE 검사·소문자 이름·빈 text 보존·Active DDL 거절 선택은 아래 DEV4-REVIEW-002로 대체된다. 나머지 범위와 최초 검토의 identity는 보존 |
| 조사·검증 | [소스 조사](../evidence/dev4-source-research.md)와 [인수 계획](../plans/dev4-implementation-plan.md). 문서 자동 검사는 push 전용, 제품 실행 NOT_RUN |
| 독립 검토 | `/root/dev4_design_review`가 원문·최종 diff·기존 소유 계약·실제 소스 증거를 읽기 전용으로 검토. P2 2건 수정 후 필수 수정 지적 없음·통합 가능. 계약/통합 담당 `/root` 반영 확인. 범위·수정은 [작업 기록](../plans/dev4-implementation-plan.md#work-record) 참조 |
| 재검토 조건 | 실제 engine API로 단일 statement/DDL 원자성 구현 불가, 자원 상한 위반, SQL/native 의미 불일치, schema race, DEV7 확장 또는 측정으로 새 optimizer/정렬 구조 필요 |

기본 설계 채택과 leaf의 실행 입력 동결을 구분한다. 실제 Rust signature·오류 mapping vector·spill codec·worker protocol·실행 recipe·fault schedule은 D4-F01–F05로 남아 있으며 미동결 항목을 구현 완료/PASS로 표시하지 않는다.

<a id="oracle-review"></a>
## 10. DEV4-REVIEW-002 — Oracle·ARIES 리뷰 반영

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당·권한 | 작업·SQL/커널 경계 계약·통합 `/root`. 사용자의 Oracle·ARIES 리뷰 요청과 후속 수정 지시, DP01의 D2 위임 |
| 선택 | DEV3 부분 NULL/문장 최종 UNIQUE 정합성, 대문자 이름, SQL 빈 text 입력 정규화, 두 outcome을 가진 DDL 전 commit, DEV3 table 단위 Serializable 수신·후속 SQL READ COMMITTED, rollback/commit/recrash 교차 vector |
| 비교·이유 | 이전 의미를 유지하면 최신 DEV3 및 Oracle 사용 기대와 어긋남. 모든 native 값 변경·Oracle 전체 dialect 도입·SQL 안 CLR 구현은 범위를 불필요하게 넓힘. SQL 입력/조립 경계와 하위 소유 책임을 구분 |
| 적용·대체 | DEV4-DESIGN-001의 위 의미만 대체. [커널 구현 계약](implementation-contracts.md#section-4)의 DDL 거절·숨은 commit 금지는 native API에 유지하고 명시 SQL 조립을 연결. DEV3-DESIGN-003의 변경된 공급 경계를 소비하되 DEV2 실제 구현·DEV3 원문·단계 순서·기존 실행 receipt는 변경하지 않음 |
| 근거·인수 | [Oracle/ARIES 조사 보완](../evidence/dev4-source-research.md#oracle-aries), D4-V17–24·D4-T01–14, D4-F01/F02/F04. Oracle 실행 비교·XADB Q1·제품 실행 NOT_RUN |
| 독립 검토 | `/root/dev4_oracle_review`가 기준 사본 대비 14개 문서/JSON의 DEV4 변경과 최신 DEV3 공급 경계를 읽기 전용 검토. 지적 반영 후 D2 채택을 막는 P1/P2 없음. 계약/통합 담당 `/root` 확인. [보완 작업 기록](../plans/dev4-implementation-plan.md#oracle-work-record) 참조 |
| 재검토 조건 | SQL/native 정규화 경계 오용, DDL 두 거래의 결과 유실, 실제 공급 API 불일치, READ COMMITTED 도입, 저장 방식 변경, Oracle 대상 버전·호환 범위 확대 |

사내 개발은 새 fixture를 사용하며 이전 대소문자/빈 값의 자동 변환을 구현하지 않는다. 현재 build의 reopen·ACK·constraint 검증은 유지한다. 준비된 설계의 채택은 미동결 leaf나 실행 결과를 PASS로 바꾸지 않는다.

<a id="architecture-review"></a>
## DEV4-REVIEW-003 — 저장 공급과 처리 범위 보완

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당·권한 | 작업/SQL 계약/통합 `/root`; 사용자의 DBMS 비교 리뷰 후 문서 수정 지시 |
| 선택·이유 | 기본 계층·문법 유지. all-null unique lookup의 다중 결과를 명시하고 초기 처리 한계를 인수/계측에 연결. DEV3-REVIEW-004의 실제 공급·예약/cleanup 증거를 수신 |
| 대체·대안 | 기존 UNIQUE·오류·DDL·isolation 의미를 바꾸지 않는 구체화. READ COMMITTED 기본 전환·큰 DML 분할 commit·고급 optimizer 도입은 이번 범위 밖 |
| 인수·독립 검토 | [비교·보완 기록](../evidence/dev3-dev4-architecture-review.md), D4-V25·T04/05/09/11/14. `/root/dev34_architecture_review` 최종 설계 채택 PASS·필수 P1/P2 없음, `/root` 반영 확인. 제품·Q1·PF10 NOT_RUN |
| 재검토 | all-null lookup 누락, 저장 공급 미충족, quota/cleanup 경계의 의미 변경, 실제 충돌률/flush 대기로 처리 목표 미달 |

<a id="conflict-reserve-review"></a>
## DEV4-REVIEW-004 — 행 관측값 전달과 복구 예약 공급

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-18 |
| 담당·권한 | 작업/SQL 계약/통합 `/root`; 사용자의 리뷰 후 수정 지시 |
| 선택·이유 | DML 대상 목록에 engine의 opaque observation을 유지하고 대기 없는 잠금 획득도 execute 중 검증. DEV3의 crash 뒤 예약 복원·반복 장애 증거를 수신하여 cleanup 결과를 전달 |
| 대안·대체 | RowUID/원래 값만 전달하거나 commit validation에만 의존하는 해석을 대체. SQL 자체 재검사/rollback ledger는 만들지 않음. REVIEW-002/003의 Serializable·statement 원자성·오류 효과와 SQL 범위 유지 |
| 인수·독립 검토 | D4-V26·T09, R2·T10/T11, F02/F04. `/root/dev34_conflict_reserve_review` 최종 독립 설계 검토 PASS·필수 P1/P2 없음, `/root` 반영 확인; [공통 작업 기록](../plans/dev3-implementation-plan.md#conflict-reserve-review). Q1·제품·성능 NOT_RUN |
| 재검토 | 실제 observation API/오류 효과 불일치, SQL의 숨은 재실행, 복구 예약 공급 미충족, SQL isolation 확대 |

<a id="mathematical-review"></a>
## DEV4-REVIEW-005 — 문장 rollback 회계와 호출별 취소 수명

| 항목 | 기록 |
| --- | --- |
| 분류·일자·담당 | D2 · 2026-10-19 · 작업/계약/통합 `/root` |
| 권한·선택 | 사용자의 DEV3·4·5 수학적 재검토·문제 즉시 수정 요청과 DP01 위임. 4절의 quota 전체 복원 표현을 실제 반환 회계로 대체하고 6절의 operation context·D4-V27/V28 추가. SQL 값·격리 의미 유지 |
| 대안·대체 | 4절의 quota 전체 원복 표현을 실제 반환 회계로 수정. 논리 rollback 후 물리 비용이 남는 [DEV3 보존식](disk-storage-design.md#rollback-accounting)을 적용. cursor에 완료 요청의 context를 고정하면 이전 deadline이 다음 fetch를 취소하므로 snapshot과 호출 context의 소유를 분리 |
| 상태·인수·독립 검토 | ACCEPTED. D4-V27/V28·T04/T08/T10/T11·F02/F04/F05에 반영. `/root/dev345_independent_review`가 원문·반례·세 단계 계약/계획/인수 diff를 독립 검토하고 축소 계산을 재실행: 필수 P1/P2 잔여 없음. `/root` 반영 확인. 이전 review identity·실행 결과 유지 |
| 참고 근거 | 2026-10-19 [Oracle 19c Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/transactions.html)의 문장 원자성·[읽기 일관성](https://docs.oracle.com/en/database/oracle/oracle-database/19/cncpt/data-concurrency-and-consistency.html), MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4`의 `storage/innobase/include/read0types.h` ReadView/purge를 대조. XADB ledger·Serializable·snapshot 모델은 자체 계약으로 유지 |
| 실행·재검토 | 산술 반례·축소 계산만 실행. Q1·제품·복구·성능은 NOT_RUN. 잔류 비용 누락·snapshot/operation 수명 혼동 또는 실제 leaf 반례가 생기면 재검토 |

<a id="extensibility-review"></a>
## DEV4-REVIEW-006 — 동시 SQL 자원 인수 보완

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-19 |
| 담당·권한 | 작업/SQL 계약/통합 `/root`; 사용자의 확장 한계 점검·선제 보완 요청과 DP01 위임 |
| 선택·대안 | 기존 DB 합산 quota·spill/cleanup 계약에 binding 교체 peak·cleanup 예약과 다중 session 경계 사례를 명시. query별 상한만 검사하거나 추정 cardinality로 실제 quota를 대체하는 대안은 제외 |
| 적용·대체 | 기존 의미·수치·stage DAG 유지. D4-T07/T08/T11/T14와 F02/F03/F05의 구체화이며 큰 DML·추가 SQL 연산자 도입은 별도 |
| 근거·검토 | [소스 조사·반례·독립 검토](../evidence/commercial-extensibility-research.md). `/root/extensibility_review`의 독립 D2 설계 검토 PASS·잔여 필수 P1/P2 없음, 계약/통합 담당 `/root` 반영 확인. 제품·Q1·PF10 NOT_RUN |
| 재검토 | 다중 연산자·parallel query·큰 bind/LOB 추가, cleanup 자원 순환 또는 실제 합산 peak 초과 |
