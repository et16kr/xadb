# XADB 초기 SQL 명세 초안

2026-10-18 · **문법·세부 의미 제안 / SQL 엔진 미구현 / 예제 시험 미실행**

[사용자 매뉴얼](user-manual-draft.md) · [구조화된 예제·기대 결과](sql-language-draft.json) · [채택한 초기 SQL 범위](../plans/development-stages.md#section-4) · [커널 계약](../contracts/implementation-contracts.md)

이 문서는 DEV4의 유한한 SQL 범위를 구현 전에 검토할 수 있도록 구문과 관측 결과를 구체화한다. 테이블·색인, 한 테이블 조회, INSERT·UPDATE·DELETE, 트랜잭션을 다룬다. 여기서 선택한 철자·NULL unique·문자 길이·정렬·세션 제어의 상세는 **신규 제안**이다. 기존 설계로 채택된 statement rollback·초기 SERIALIZABLE·트랜잭션 안 DDL 거절·결과 불명 보존은 그대로 따른다.

후속 [DEV4 상세 설계](../contracts/dev4-sql-design.md)가 최소 SQL의 소유 계약과 prepared 실행을 구체화한다. 이 초안의 예제는 당시의 미실행 제안으로 보존하며 현행 Q1 기대값으로 사용하지 않는다. [DEV4-REVIEW-002](../contracts/dev4-sql-design.md#oracle-review)는 [현재 DEV3](../contracts/disk-storage-design.md#keys)의 부분 NULL·문장 최종 UNIQUE 검사를 소비하고, 미인용 이름 대문자·SQL 빈 text 입력의 NULL 정규화·DDL 전 commit을 채택한다. 아래 소문자 이름·빈 text 보존·Active DDL 거절은 대체된 제안이다. 초안 전체가 채택됐거나 예제가 실행됐다는 뜻은 아니다.

<a id="section-1"></a>

## 1 범위와 표기

아래 예제의 기대 결과는 명세를 위한 수작업 기대값이며 실행 결과가 아니다. 구문을 구현할 때 parser뿐 아니라 실제 DISK 저장·거래·오류·복구 경로와 연결해 검증한다.

초기 범위는 CREATE/DROP TABLE, CREATE/DROP INDEX, SELECT, INSERT, UPDATE, DELETE, BEGIN, COMMIT, ROLLBACK과 autocommit 제어다. join·subquery·집계·window·DISTINCT·CTE·UNION·함수·산술식·암묵 cast·명시 SAVEPOINT·ALTER·외래 키·CHECK·PSM·partition SQL은 이 초안의 실행 범위 밖이다. 후속 기능 요구에서 삭제하는 것은 아니다.

문법에서 대괄호는 생략 가능, 중괄호는 반복, 세로줄은 선택을 뜻한다. 대문자는 keyword, 소문자는 문법 요소다. `<...>`는 실제 입력 문자열이 아니다. 이 표기는 SQL 입력으로 실행하지 않는다.

<a id="section-2"></a>

## 2 이름과 literal과 타입

### 이름 규칙

keyword와 따옴표 없는 이름은 대소문자를 구별하지 않고 이름은 ASCII 소문자로 정규화한다. 따옴표 없는 이름은 ASCII 문자 또는 밑줄로 시작하고 이후 문자·숫자·밑줄을 사용한다. keyword는 일반 이름으로 사용하지 않는다.

큰따옴표 이름은 UTF-8 내용을 정규화 없이 정확히 비교한다. 이름 안의 큰따옴표는 두 번 써서 표현한다. 빈 이름·NUL·제어 문자는 거절한다. 이 초안의 내부 개발 한도 제안은 정규화 후 128 bytes이며 안정 릴리스 한도를 확정한 것이 아니다. `orders`, `ORDERS`, `"orders"`는 같은 이름이고 `"Orders"`는 다른 이름이다.

사용자 테이블·색인의 기본 namespace는 `public`으로 제안한다. `orders`와 `public.orders`는 같은 대상이다. CREATE SCHEMA와 다른 사용자 namespace 생성은 후속이다. 시스템 namespace의 조회는 그 기능이 제공될 때만 허용한다. 명시 트랜잭션은 고정 snapshot의 catalog에 이름을 bind하며 오래된 객체 identity를 새 동명 객체로 조용히 연결하지 않는다.

### literal

| 입력 | 의미 |
|---|---|
| `12`, `-12`, `+12` | 범위 내 signed 64-bit 정수. 소수·지수 표기는 초기 미지원 |
| `TRUE`, `FALSE` | BOOLEAN |
| `'문자열'`, `''` | UTF-8 문자열과 빈 문자열. 작은따옴표는 `''`로 escape |
| `X'00FF'` | 짝수 개 hex digit의 binary. `X''`는 길이 0 |
| `NULL` | 열 타입에 맞추어 해석할 값의 부재 |

문자열에는 backslash escape나 암묵 Unicode 정규화를 적용하지 않는다. 잘못된 UTF-8·홀수 hex·범위 밖 정수는 명시적 오류다. `--`부터 줄 끝까지와 중첩하지 않는 `/* ... */` 주석을 지원하는 안이다. 문자열·인용 이름·주석 안의 세미콜론은 문장 구분자로 취급하지 않는다. batch에서 주석과 공백만 있는 구간은 실행 문장으로 세지 않는다.

### 열 타입

| 제안 구문 | 값·길이 의미 |
|---|---|
| `BIGINT` | signed 64-bit 정수 |
| `BOOLEAN` | TRUE·FALSE |
| `VARCHAR(n)` | 최대 n **UTF-8 bytes**의 가변 문자열. trailing space 보존, padding 없음 |
| `VARBINARY(n)` | 최대 n bytes의 가변 binary. padding 없음 |

`n`은 명시해야 하며 초기 사내 구현 후보 범위는 1–65,536이다. 현재 공통 기반의 값 상한과 연결한 제안이며 최종 제품의 문자·LOB 한도가 아니다. 문자 개수와 byte 길이를 혼동하지 않는다. `VARCHAR(6)`에는 `'가나'`가 들어가지만 `'가나다'`는 길이 오류다. 길이를 넘는 값을 잘라 저장하지 않는다.

non-NULL 값의 비교와 대입은 같은 논리 타입끼리만 허용한다. `BIGINT` 열에 `'12'`를 넣거나 BOOLEAN과 정수를 비교하면 타입 오류다. `VARCHAR(n)`끼리의 비교는 선언 길이와 무관하게 UTF-8 byte 순서를 따른다. binary도 byte 순서를 따른다. BOOLEAN의 정렬은 FALSE가 TRUE보다 앞선다. 길이는 저장할 열의 한도로 검사한다.

<a id="section-3"></a>

## 3 테이블과 색인 정의

**제안 문법**

```text
CREATE TABLE table_name (
  column_name type [NOT NULL | NULL]
  {, column_name type [NOT NULL | NULL]}
  [, PRIMARY KEY (column_name {, column_name})]
  {, UNIQUE (column_name {, column_name})}
) STORAGE storage_medium;

storage_medium := DISK

CREATE [UNIQUE] INDEX index_name ON table_name (column_name {, column_name});
DROP INDEX index_name;
DROP TABLE table_name;
```

STORAGE를 명시하여 배치 기본값을 추측하지 않게 한다. DISK 테이블은 영속 저장을 요구한다. NULL 여부를 생략하면 nullable이다. PRIMARY KEY는 한 테이블에 최대 하나이며 해당 모든 열에 NOT NULL을 요구한다. PRIMARY KEY 선언은 nullable 기본값을 non-null 제약으로 강화한다. 열에 명시한 NULL과 PRIMARY KEY가 충돌하면 정의 오류로 거절한다. 제약에 없는 열, 중복 열·객체 이름, 빈 열 목록을 거절한다. 기본값·identity·생성 열·임시 테이블 구문은 후속이다.

**UNIQUE의 NULL 제안:** 키에 NULL이 하나라도 있으면 다른 행과의 unique 충돌을 만들지 않는다. non-NULL 복합 키는 모든 구성 값이 같을 때 충돌한다. 빈 문자열은 실제 값이므로 반복하면 충돌한다. 이 규칙은 행·unique index·복구에 동일하게 적용한다. primary key의 NULL은 항상 거절한다.

초기 색인은 복합 오름차순 key를 제안한다. SQL의 NULL 정렬은 4절을 따르며 내부 key byte 순서와 자동으로 같다고 보지 않는다. 일반 테이블·색인 수·복합 key 전체 길이 등 자원 한도는 초기 저장 기준선에서 따로 정하고 범위 초과를 명시적으로 거절한다.

DROP TABLE은 소유 색인·제약과 함께 원자적으로 처리한다. PRIMARY KEY·UNIQUE 제약을 뒷받침하는 색인은 DROP INDEX만으로 제약을 몰래 제거할 수 없다. 지원하지 않는 종속 객체가 있으면 거절하며 CASCADE·IF EXISTS는 초기 범위에 넣지 않는다.

DDL은 활성 트랜잭션이 없는 상태에서 독립 원자 작업으로 실행한다. DML/SELECT로 시작된 트랜잭션이나 명시 BEGIN 안에서는 DDL을 거절하고 기존 트랜잭션을 유지한다. 먼저 commit하는 숨은 동작은 없다. 초기 offline 색인 생성은 기존 DB 전체 admission/drain 계약을 따른다.

<a id="section-4"></a>

## 4 조회

**제안 문법**

```text
SELECT projection
FROM table_name
[WHERE predicate]
[ORDER BY column_name [ASC | DESC] [NULLS FIRST | NULLS LAST]
          {, column_name [ASC | DESC] [NULLS FIRST | NULLS LAST]}]
[LIMIT nonnegative_integer];

projection := * | column_name [AS output_name] {, column_name [AS output_name]}
predicate := comparison
           | value IS [NOT] NULL
           | NOT predicate
           | predicate AND predicate
           | predicate OR predicate
           | (predicate)
comparison := value comparison_operator value
comparison_operator := = | <> | < | <= | > | >=
value := column_name | literal
```

SELECT는 한 테이블의 열을 선택한다. `SELECT 1`처럼 FROM 없는 조회와 임의 expression projection은 초기 미지원으로 구분한다. WHERE를 생략하면 모든 행이 대상이다. 연산 우선순위는 비교/IS NULL → NOT → AND → OR이며 괄호로 변경할 수 있다. `!=` 같은 별도 alias는 이 초안에서 제공하지 않는다.

NULL이 포함된 일반 비교는 UNKNOWN이다. WHERE는 TRUE인 행만 선택한다. `NOT UNKNOWN`도 UNKNOWN이며 `FALSE AND UNKNOWN`은 FALSE, `TRUE OR UNKNOWN`은 TRUE다. 모든 열 이름과 타입은 실행 전에 검증하므로 `1 = 0 AND missing_column = 1`도 존재하지 않는 열의 오류를 숨기지 않는다.

ORDER BY의 열은 입력 테이블의 열을 가리킨다. 출력 alias·열 번호로 정렬하는 문법은 후속이며 선택하지 않은 입력 열로 정렬할 수는 있다. ASC가 기본이다. NULL 위치를 생략하면 ASC는 LAST, DESC는 FIRST로 제안한다. 문자열은 locale 정렬이 아니다.

ORDER BY가 없으면 행 순서를 보장하지 않는다. ORDER BY가 있어도 동률 행의 상대 순서를 보장하지 않는다. 재현 가능한 LIMIT 예제는 마지막 정렬 열에 unique key를 포함한다. LIMIT 0은 빈 결과이며 음수·범위 밖 값·OFFSET은 거절한다. LIMIT으로 결과가 제한됐다는 사실과 전송 중 잘린 결과는 구분한다.

<a id="section-5"></a>

## 5 행 추가와 변경과 삭제

**제안 문법**

```text
INSERT INTO table_name (column_name {, column_name})
VALUES (literal {, literal}) {, (literal {, literal})};

UPDATE table_name SET column_name = literal {, column_name = literal}
[WHERE predicate];

DELETE FROM table_name [WHERE predicate];
```

INSERT는 열 목록을 명시한다. 생략된 nullable 열에는 NULL을 넣고 생략된 NOT NULL 열은 오류다. 열·값 개수가 다르거나 같은 열을 반복 지정하면 거절한다. 초기 UPDATE 우변은 literal만 허용하며 산술·함수·다른 열 참조는 후속이다.

WHERE가 없는 UPDATE·DELETE는 모든 행을 대상으로 한다. XADB SQL이 자동으로 조건을 추가하지 않는다. 도구가 경고나 정책을 제공하더라도 SQL 자체의 의미와 구분한다. 결과의 affected row count는 UPDATE의 조건에 맞은 행 수, DELETE의 삭제 행 수, INSERT의 추가 행 수로 제안한다. 값을 같은 값으로 바꿔도 매칭된 UPDATE 행에 포함한다.

한 문장 중 어느 행에서 타입·길이·unique·NOT NULL 오류가 나더라도 그 문장의 모든 변경을 되돌린다. 명시 트랜잭션의 이전 성공 문장까지 지우지는 않는다. autocommit 문장의 실패는 그 문장의 암시적 트랜잭션을 종료한다. 오류 결과의 문장 효과·거래 상태를 확인한다.

대상 행은 문장 시작 시점의 해당 트랜잭션 view에서 정하고, 최종 문장 상태에 대해 제약을 검사하는 안이다. unique 값 갱신·복합 key의 경계는 독립 예제와 동시성 시험으로 검증해야 한다. 하나의 문장 실행 순서에 따라 임의로 다른 결과를 내지 않는다.

<a id="section-6"></a>

## 6 트랜잭션과 세션 제어

**제안 문법**

```text
BEGIN;
COMMIT;
ROLLBACK;
SET AUTOCOMMIT ON;
SET AUTOCOMMIT OFF;
```

새 세션은 autocommit ON을 기본안으로 한다. ON에서 BEGIN 밖의 SELECT/DML은 문장별 트랜잭션이다. BEGIN은 명시 트랜잭션을 시작하고 COMMIT/ROLLBACK이 종료한다. 종료 뒤에는 기존 autocommit 설정을 유지한다.

OFF에서는 트랜잭션이 없을 때 첫 SELECT/DML이 트랜잭션을 시작한다. COMMIT/ROLLBACK 뒤에는 다음 SELECT/DML까지 새 트랜잭션을 시작하지 않는다. idle 상태의 DDL은 OFF에서도 독립 원자 DDL로 확정되며 사용자에게 그 효과를 표시한다. 기존 활성 트랜잭션을 먼저 확정하는 의미는 아니다.

활성 상태의 BEGIN·autocommit 변경은 상태 오류이며 기존 변경을 유지한다. idle 상태의 COMMIT·ROLLBACK은 상태 오류로 제안하며 성공한 commit/rollback 결과를 만들어 내지 않는다. 새 접속은 과거 접속의 트랜잭션을 이어받지 않는다.

초기 격리는 기존 SERIALIZABLE 계약을 사용한다. 타입·제약 같은 복구 가능한 문장 오류와 전체 abort가 필요한 충돌을 구분한다. 내부 불변식 위반·영속 I/O 결과 불명은 커널의 쓰기 차단·복구 계약을 따르며 일반 문장 오류처럼 계속 실행하지 않는다.

### 오류 분류 제안

| 제안 코드 | 발생 예 | 효과·재시도 |
|---|---|---|
| `XADB-SQL-SYNTAX` | 구분자·literal·열 목록 형식 오류 | 문장 미실행, 수정 후 새 요청 |
| `XADB-SQL-UNSUPPORTED` | join·함수 등 초기 범위 밖 | 문장 미실행, 지원 범위 확인 |
| `XADB-SQL-NAME` | 없는 객체·열 또는 중복 정의 | 문장 미실행, 권한상 허용된 정보만 설명 |
| `XADB-SQL-TYPE` | 정수 열에 문자열 대입 | 이번 문장 효과 없음/rollback, 이전 문장 유지 |
| `XADB-SQL-RANGE` | 정수·길이·자원 범위 초과 | 잘라 저장하지 않음, 이번 문장 효과 복원 |
| `XADB-SQL-NOT-NULL` | NULL 불가 열에 NULL | 이번 문장 rollback |
| `XADB-SQL-UNIQUE` | non-NULL unique key 충돌 | 이번 문장 rollback |
| `XADB-SQL-TXN-STATE` | 중첩 BEGIN·활성 상태의 mode 변경 | 기존 트랜잭션 유지 |
| `XADB-SQL-DDL-IN-TXN` | 활성 트랜잭션 안 DDL | DDL 미실행, 기존 트랜잭션 유지 |
| `XADB-SQL-CONFLICT` | commit 검증 충돌 | 전체 ABORTED, 전체 업무 재시도 판단 |
| `XADB-SQL-RESULT-UNKNOWN` | commit 응답·영속 결과 확인 불가 | UNKNOWN 유지, 자동 재전송 금지 |

이 코드는 공개 오류 이름 제안이며 실제 DEV1의 `XADB-FOUND-*`와 같은 구현 코드가 아니다. SQLSTATE·driver별 매핑은 동일 효과를 보존하도록 별도 명세한다. 메시지 문자열 대신 오류 코드·효과·확실성·재시도 단위를 프로그램 분기에 사용한다.

<a id="section-7"></a>

## 7 예제와 기대 결과

모든 사례는 **PLANNED_NOT_RUN**이다. 별도 언급이 없으면 다음 fixture를 새로 구성한 독립 사례이며, 각 actor는 autocommit ON·활성 트랜잭션 없음 상태의 새 세션에서 시작한다. DISK에서 같은 논리 결과를 확인해야 한다.

<!-- sql-example: SQL-01 -->
**문법 제안·실행 미검증**

```sql
CREATE TABLE orders (
    order_id BIGINT NOT NULL,
    customer_name VARCHAR(80),
    quantity BIGINT NOT NULL,
    paid BOOLEAN NOT NULL,
    PRIMARY KEY (order_id)
) STORAGE DISK;
INSERT INTO orders (order_id, customer_name, quantity, paid)
VALUES (1001, '김하나', 2, FALSE),
       (1002, NULL, 1, TRUE),
       (1003, '', 3, FALSE);
```

기대: 테이블과 기본 키가 생기고 INSERT의 affected row count는 3이다. fixture를 만들면서 기존 동명 객체를 임의 삭제하지 않는다.

<!-- sql-example: SQL-02 -->
**문법 제안·실행 미검증**

```sql
SELECT order_id, quantity
FROM orders
WHERE paid = FALSE
ORDER BY order_id;
```

기대: `(1001, 2)`, `(1003, 3)` 순서다.

<!-- sql-example: SQL-03 -->
**문법 제안·실행 미검증**

```sql
SELECT order_id FROM orders WHERE customer_name IS NULL ORDER BY order_id;
SELECT order_id FROM orders WHERE customer_name = '' ORDER BY order_id;
SELECT order_id FROM orders WHERE customer_name = NULL ORDER BY order_id;
```

기대: 순서대로 `[1002]`, `[1003]`, 빈 결과다. 마지막 비교는 모든 행에서 UNKNOWN이므로 선택된 행이 없다.

<!-- sql-example: SQL-04 -->
**문법 제안·실행 미검증**

```sql
SELECT order_id FROM orders
ORDER BY customer_name ASC NULLS LAST, order_id ASC LIMIT 2;
```

기대: 빈 문자열 행 1003, `'김하나'` 행 1001 순서다. NULL 행은 뒤에 온다.

<!-- sql-example: SQL-05 -->
**문법 제안·실행 미검증**

```sql
BEGIN;
UPDATE orders SET quantity = 5 WHERE order_id = 1001;
SELECT quantity FROM orders WHERE order_id = 1001;
ROLLBACK;
SELECT quantity FROM orders WHERE order_id = 1001;
```

기대: UPDATE는 매칭 행 1개, 두 조회는 각각 5와 2다. 다른 actor의 동시 변경이 없는 독립 fixture 조건이다.

<!-- sql-example: SQL-06 -->
**문법 제안·실행 미검증**

```sql
BEGIN;
UPDATE orders SET quantity = 5 WHERE order_id = 1001;
INSERT INTO orders (order_id, customer_name, quantity, paid)
VALUES (1004, '이둘', 1, FALSE), (1001, '중복', 1, FALSE);
SELECT order_id, quantity FROM orders ORDER BY order_id;
ROLLBACK;
```

기대: INSERT는 `XADB-SQL-UNIQUE`, 신규 1004 행도 남지 않는다. SELECT는 `(1001, 5)`, `(1002, 1)`, `(1003, 3)`이다. 앞선 UPDATE는 트랜잭션 안에 유지되며 마지막 rollback에서 복원된다. 이 사례의 controller는 예상 오류 뒤 계속하도록 명시해야 한다. CLI의 기본 stop-on-error 스크립트로 후속 문장 실행을 가정하지 않는다.

<!-- sql-example: SQL-07 -->
**문법 제안·실행 미검증**

```sql
CREATE TABLE tags (
    tag_id BIGINT NOT NULL,
    tag VARCHAR(6),
    PRIMARY KEY (tag_id),
    UNIQUE (tag)
) STORAGE DISK;
INSERT INTO tags (tag_id, tag) VALUES (1, NULL), (2, NULL), (3, '');
INSERT INTO tags (tag_id, tag) VALUES (4, '');
INSERT INTO tags (tag_id, tag) VALUES (5, '가나다');
```

기대: 첫 INSERT는 3행이다. 다음은 빈 문자열 unique 충돌, 마지막은 9-byte 문자열의 길이 오류다. 각 오류 문장의 행은 남지 않는다. 예상 오류를 처리하는 독립 controller 절차로 실행할 계획이다.

<!-- sql-example: SQL-08 -->
**문법 제안·실행 미검증**

```sql
BEGIN;
UPDATE orders SET quantity = 5 WHERE order_id = 1001;
CREATE INDEX orders_quantity ON orders (quantity);
SELECT quantity FROM orders WHERE order_id = 1001;
ROLLBACK;
```

기대: DDL은 `XADB-SQL-DDL-IN-TXN`, 색인은 생성되지 않고 SELECT는 5다. DDL 거절이 앞선 변경을 commit하지 않는다. 오류 뒤 조회는 예상 오류를 처리하는 controller에서 진행한다.

추가 계획은 정수 경계·binary round trip·타입 불일치·0행 변경·autocommit 전이·두 actor의 commit 충돌·commit 응답 유실·실제 재시작이다. [JSON 사례 목록](sql-language-draft.json)에 입력과 독립 기대 결과를 보존하며 복구 시험은 실제 엔진이 준비된 뒤 수행한다.

### 리뷰로 보강한 경계 사례

아래도 **미실행 계획**입니다. 각 사례는 새 fixture에서 독립 실행하며 예상 오류 뒤 계속하는 controller를 명시합니다. SELF_CONTAINED는 다른 사례의 테이블이 없는 소유 환경입니다. 단일 매체 사례는 DISK로 각각 실행하고, SQL-25는 같은 DB의 디스크 저장 경로를 함께 검증합니다.

| ID | 입력·관측의 핵심 | 독립 기대 결과 |
|---|---|---|
| SQL-18 | 한 INSERT에 정상 행과 NOT NULL 위반 행 | 둘 다 미반영, 원래 3행 유지 |
| SQL-19 | VARCHAR(6)의 한글 6/9 bytes, VARBINARY(2)의 0/2/3 bytes | 6-byte text·빈 binary 허용, 길이 초과 각각 오류, 자동 잘림 없음 |
| SQL-20 | 복합 UNIQUE(a,b)에 한 구성 값이 NULL인 중복과 non-NULL 중복 | NULL 포함 중복 허용, (1,'a') 중복 거절 |
| SQL-21 | 같은 값 UPDATE, LIMIT 0, DESC의 기본 NULL 위치 | 매칭 count=1, 빈 결과, ID 순서 1002→1001→1003 |
| SQL-22 | 인용 대문자 이름, unquoted 접근, DROP/recreate | 이름 구별, 새 identity·빈 테이블; 이전 행 재등장 금지 |
| SQL-23 | autocommit OFF의 idle DDL, idle ROLLBACK, ACTIVE BEGIN | DDL 독립 확정, 두 상태 오류, 마지막 거래 종료 확인 |
| SQL-24 | 중복 paid 값에 UNIQUE INDEX 생성 | XADB-SQL-UNIQUE, 원래 행·catalog 유지, 임시 자원·작업 소유 admission 해제 |
| SQL-25 | DISK 각각 quantity=2→5를 한 거래로 변경 후 재시작 | ACK 시 (5,5), UNKNOWN 시 (2,2) 또는 (5,5); (2,5)/(5,2) 금지 |

SQL-24의 오류 코드 연결은 이 개정의 제안이며 커널의 독립 DDL·drain·원자 공개 계약을 바꾸지 않습니다. 정확한 SQL 입력·actor schedule·기대 자료는 JSON에 있습니다. statement 오류 뒤 transaction effect와 실제 데이터·색인·자원 상태를 함께 확인합니다.

<a id="section-8"></a>

## 8 후속 확장과 확정 순서

SQL 구현 착수 전에 이 초안의 선택 사항을 상세 계약으로 검토한다. 특히 UNIQUE의 NULL 정책, 복합 key 한도, UTF-8 byte 길이, NULL 정렬과 index 접근의 일치, 문장 최종 상태 제약, autocommit 상태 전이, DDL 거절 효과를 먼저 확정한다.

뒤이어 join·집계·subquery·window·정밀 수치·시간 타입 등의 명세를 같은 방식으로 확장한다. 각 문법에는 정상 예제, 빈 결과·NULL·경계, 실패 효과, 동시 실행, 권한, 결과 출력, 저장·복구의 필요한 사례를 연결한다. 상세 표현만 늘려 아직 없는 기능을 초기 SQL 지원 목록에 넣지 않는다.

public namespace 예약, SQL 철자·오류 코드 이름은 이 초안의 제안이다. 구현된 public API나 고객 호환 기준선으로 자동 승격하지 않는다. 문서 검사와 예제 목록의 일치는 parser·DB 의미 검증을 대신하지 않는다.
