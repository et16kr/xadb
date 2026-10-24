# DEV4 독립 앞단 — Oracle·로컬 DBMS 조사와 계약 검토

2026-10-24 · **읽기 조사·독립 D2 검토 완료 / D4-PREP-001 ACCEPTED / 제품·모델·Oracle 실행 NOT_RUN**

최초 조사·채택 기록은 아래 1–4절에 보존한다. 후속 사용자 리뷰에서 발견한 진단 효과 충돌과 [D4-PREP-002 교정](#review-followup)은 별도 기록이며 독립 D2 재검토를 거쳐 ACCEPTED로 반영했다. 제품·모델·Oracle 실행은 계속 NOT_RUN이다.

[채택 대상 계약](../contracts/dev4-independent-frontend.md) · [개발 계획](../plans/dev4-preparation-plan.md) · [기존 DEV4 조사](dev4-source-research.md) · [참고 소스 목록](../reference/opensource-references.md)

## 1. 질문과 작업 정체성

사용자는 DEV3 진행 중 미리 할 DEV4 작업을 물은 뒤 계약·계획 확정을 요청했고 Oracle을 참고 표준으로 지정했다. 후속으로 `/home/et16/work/OracleManual` clone을 알려 주어 로컬 SQL 교재도 대조했다. 제품 구현·시험 재개·commit/push 요청은 아니다.

| 항목 | 기록 |
| --- | --- |
| 작업/계약/통합 | `/root`, D2. 별도 검토 `/root/dev4_preparation_review` |
| 기준 HEAD | `2f262d27cd726bd48e645644bf27c77f52398b6b` |
| 기준 사본 | `/tmp/xadb-dev4-preparation-base-668zfp_o`, `identity.json` SHA-256 `13de21ae656550db71f7e0085117d4d6a6300fc8490076db70f3906e9d07919c` |
| 기존 변경 | DEV3 mixed Status/Undo INIT의 engine·fixtures·scripts·계약/증거와 공유 README/JSON 수정. 해당 구현·시험·상태는 이번 검토/수정 범위 밖 |
| 동시 작업 관측 | 편집 중 HEAD가 DEV3 작업 commit `b807eb4fae0dabb4c9a18a3c85fddb63245664bc`로 이동했고 원 DEV3 변경이 commit된 상태를 확인. 최신 공유 파일의 DEV3 내용은 보존하며 이번 작업은 DEV4 문서 diff만 남김 |
| 독점 작성 | 독립 앞단 계약·준비 계획·이 조사 기록 |
| 공유 수정 | 기존 SQL 정본·DEV4 계획/분류·bottom-up/stage·scope JSON의 착수 예외, 문서/폴더 색인·소스 목록. 최신 파일에서 좁은 문구/필드만 변경 |
| 순서 | 공식/로컬 조사 → 범위·계약·선택 시험 계획 → 정본/단계 연결 → 독립 검토/수정 → 채택 기록. 문서 검사 생성물 갱신 없음 |

<a id="oracle"></a>
## 2. Oracle 기준 확인

기준은 Oracle Database 19c 공식 SQL Language Reference다. 아래는 2026-10-24 실제 web 열람이며 전체 매뉴얼·Oracle binary 실행 검증은 아니다. 내용은 필요한 의미만 요약했다.

| 공식 원문·읽은 부분 | 확인과 XADB 적용 |
| --- | --- |
| [Database Object Names and Qualifiers](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Database-Object-Names-and-Qualifiers.html), Naming Rules | 미인용 대문자·인용 대소문자 구분과 COMPATIBLE≥12.2의 일반 이름 128 bytes. XADB의 ASCII/선두 `_`·내부 `""`·고정 PUBLIC·control 거절은 독자 차이로 명시 |
| [Nulls](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Nulls.html), 문자 길이0·comparison/conditions | 빈 문자값과 NULL, 일반 비교 UNKNOWN·WHERE 제외. XADB typed NULL·native empty 보존 경계는 기존 계약 유지 |
| [Logical Conditions](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Logical-Conditions.html), NOT/AND/OR 표 | 독립 3값 표의 의미 근거. XADB oracle는 실제 production 함수와 분리 |
| [About SQL Conditions](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/About-SQL-Conditions.html), Condition Precedence | 비교·NOT·AND·OR 우선순위. AND/OR의 왼쪽부터 실행을 보장한다고 해석하지 않음 |
| [Comparison Conditions](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Comparison-Conditions.html), 비교 연산 표 | Oracle의 `!=`·`^=`·`<>`는 같은 부등 비교다. 로컬 교재의 별도 `<>` 우선순위 행을 채택하지 않는 보강 근거; XADB는 기존 `<>` 지원 범위 유지 |
| [Literals](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Literals.html), Text Literals | 단일 quote·두 quote escape 확인. q-quote/N-string 등의 전체 지원은 XADB에 추가하지 않음 |
| [COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/COMMIT.html), Purpose | 구문상 유효한 DDL 전 commit과 성공 뒤 commit 설명. XADB parser의 DDL 길이 의미 검사 지연과 실제 SQL 조립의 분리 근거. Oracle 도구의 정상 종료 동작을 XADB CLI에 도입하지 않음 |

### 로컬 OracleManual의 사용과 차이 해결

`OracleManual/README.md`는 공식 문서와 개인 자료를 함께 모으는 저장소라고 설명하며 실습 환경을 Oracle19c로 표시한다. 로컬 파일은 교육·예제의 보조 자료로 사용하고 공식 SQL reference와 XADB 채택 계약을 대신하지 않는다. SQL chapter3의 IS NULL/조건식/정렬과 chapter10의 거래 종료 설명을 확인했다.

| 대조 지점 | 판단 |
| --- | --- |
| chapter11 명명 규칙의 `1~30자` | 공식19c의 COMPATIBLE≥12.2 일반 이름128 bytes와 구별. XADB 기존128 **bytes** 유지; 로컬 요약을 새 한도로 채택하지 않음 |
| chapter3 우선순위 표의 `<>` 별도 행 | 공식 조건식 표는 비교 연산을 같은 층으로 둔다. XADB의 비교→NOT→AND→OR grammar 유지 |
| chapter3 ORDER BY alias/ordinal·OFFSET/FETCH | Oracle 문법 비교 사례로 보존. DEV4의 단순 열 정렬·LIMIT 범위를 확장하지 않고 미지원 corpus 후보로 사용 |
| chapter10 DDL/정상 도구 종료의 자동 commit 요약 | XADB 지원 DDL의 구문/semantic 경계·두 outcome 및 CLI EOF rollback 정본 유지. 요약으로 오류 후 이전 commit을 지우거나 EOF commit을 추가하지 않음 |

교육 예제의 기대 행 수를 XADB 실행 정답으로 옮기지 않는다. 이번에는 예제를 실행하거나 원본 파일을 수정하지 않았다.

<a id="local-sources"></a>
## 3. 로컬 DBMS 소스에서 확인한 경계

| 대상·읽은 책임 | 관측·채택/기각 |
| --- | --- |
| PostgreSQL raw_parser·grammar 주석 | raw parse와 DB 상태 의존 분석의 분리, grammar 단계 DB 접근/가변 설정 경계. XADB에서도 parser가 catalog/transaction을 호출하지 않는 구성을 채택. PostgreSQL 문법·이름 접기·C allocator는 복제하지 않음 |
| SQLite tokenizer·parser loop·complete 상태 | token 단위 scan·입력 길이·interrupt, quote/comment 상태를 고려한 문장 완결 판별. XADB streaming 경계/한도 시험에 반영. NUL-terminated 입력·SQLite 방언·완결=구문 정당성 주장은 도입하지 않음 |
| MariaDB quote lexing·string literal 구성 | quote 해석과 빈 문자열 처리가 sql_mode/연결 문맥의 영향을 받는 구현을 확인. XADB는 고정 방언·SQL 입력 경계로 한정하며 native 값 전체나 전역 mode에 정규화를 넣지 않음 |
| CUBRID parser 선언·pt_resolve_names | parse tree의 생성/해제 API와 이름 해석의 별도 책임을 확인. XADB AST 소유와 후속 binder 경계에 참고. CUBRID parser 전체가 저장소로부터 독립적이라고 주장하지 않음 |

foundation 실제 `Value::Null`은 타입을 담지 않고 `Value::compare`는 NULL이면 UNKNOWN을 먼저 반환한다. SQL은 별도 타입을 보존하고 타입 검증을 선행해야 한다. `Quota/Reservation`은 논리 예약 수명이며 heap 전체 OOM 보장이 아니므로 새 앞단은 capacity·동시 공존 공간·fallible allocation을 별도로 확인해야 한다. 제품 파일을 변경하거나 기존 시험을 재실행하지 않았다.

### 실제 checkout·파일 identity

모든 아래 경로의 기준은 `/home/et16/work`다. revision·origin·작업 트리를 실제 조회했고 listed file 전체 SHA-256과 **읽은 줄 범위만** 기록한다. 다른 파일/전체 저장소 검토·빌드·실행 증거가 아니다. 해시는 동일 파일 identity를 위한 것이며 시험 PASS가 아니다.

| checkout | 실제 revision | origin | 상태 |
| --- | --- | --- | --- |
| `postgres` | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | [공식 소스/자료](https://github.com/postgres/postgres) | clean |
| `sqlite` | `43a48f77585a88940c727f33682151418f544059` | [공식 소스/자료](https://github.com/sqlite/sqlite) | clean |
| `mariadb` | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | [공식 소스/자료](https://github.com/MariaDB/server) | clean |
| `cubrid` | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | [공식 소스/자료](https://github.com/CUBRID/cubrid) | clean |
| `OracleManual` | `269894a0eaf96cb3eae9db23b8e83b4575df7117` | [사용자 제공 매뉴얼 모음](https://github.com/elpisk/OracleManual) | clean |

| 파일 | 열람 줄 | SHA-256 |
| --- | --- | --- |
| `postgres/src/backend/parser/parser.c` | 18–88 | `6456c41096afcae3af42ca2d376fbcecae2367c9d6eee80d959bf5fc4e5ee316` |
| `postgres/src/backend/parser/gram.y` | 16–38 | `9915bd2d6050b83c97850e08156c18ebf99e5578bb9520ce05b985817d01fccb` |
| `sqlite/src/tokenize.c` | 270–335,600–665 | `fc0679949c68463f8a82a3f4f78cc17d720bd94a356ee6652b2bb1f6c6bacd22` |
| `sqlite/src/complete.c` | 14–114 | `83584978e86b201455660c6663772fc1882c43bca155e9653d68cdacb140b375` |
| `mariadb/sql/sql_lex.cc` | 2375–2410 | `50fdd43c30914172d2440b9b2b891ed610eb1fe6445a8e9358cbdf654feffd0b` |
| `mariadb/sql/sql_class.cc` | 2890–2949 | `ee6c5715d0f8500588a119c7d2853cfb8db0466f6e190501ed14a9b2a593c11c` |
| `cubrid/src/parser/parser.h` | 60–100 | `6b4f90d3a57c4f957a58abfa1274a6bfb02195d57820616b0b7767a4bd9a85ad` |
| `cubrid/src/parser/name_resolution.c` | 9410–9478 | `681551ae8b99ef23c1a974517fbc8a98890994bbe1fb15baab784e22c1e4eaab` |
| `OracleManual/README.md` | 전체 | `ca770da851e2037d10adc1ba77831cfc6627778f1f3e8cc886328199be01b57e` |
| `OracleManual/SQL/19c_sql_ch03_new.md` | 1–50,173–198,244–258,296–337 | `aab15ce2d351ce0ba5d59671e6afede6cfa0366e48968ded6c2c71d426d2edbf` |
| `OracleManual/SQL/19c_sql_ch10_new.md` | 244–304 | `17a730dad0b319433103d55308e54b54ebe6cf18d53caf1c80cab2f5379d2a7f` |
| `OracleManual/SQL/19c_sql_ch11_new.md` | 29–39 | `3c3990e4317bbd2d0e7876a25c0bb84413292dec2ef2894dc4dda7fe5e921cca` |

<a id="review"></a>
## 4. 독립 D2 검토와 실행 상태

검토자는 구현/문서 작성자와 별도인 `/root/dev4_preparation_review`다. 새 계약·계획·조사 문서3개와 기존 정본/JSON/색인15개의 DEV4 변경을 읽기 전용 검토하여 **D4-PREP-001 채택 가능·남은 P1/P2 없음**으로 판정했다. SQL 계약·통합 담당 `/root`가 반영을 확인하여 ACCEPTED로 전환했다. 동시 DEV3 변경 자체는 검토 범위 밖이다.

초기 검토에서 P0의 `4.14.1.1/2`가 temp disk/FD 작업으로 오해될 수 있음을 지적해 `4.14.1.1·4.14.2`로 명시했다. whitespace/newline·line comment·오류 우선순위의 실제 F01 동결 의무를 추가했다. 작성자 대조에서 발견한 marker 번호도 기존 정본의1..N으로 수정했다. 전역 stage entry 규칙의 좁은 예외·D4-0의 소비 leaf별 동결과 함께 최종 검토자가 재확인했다. DDL 숫자 원문·typed NULL·allocation/retained charge·실제 I/O와 모델의 구분은 유지한다.

검토자는5개 checkout의 HEAD/origin/clean, 기록한12개 파일 hash·열람 범위와 기준 identity hash를 직접 대조했고 Oracle19c 공식 문서 및 로컬 OracleManual의 차이 처리도 확인했다. 선택적 보완으로 제안한 OracleManual 링크 명칭을 자료 모음으로 구별하고 Comparison Conditions 공식 근거를 추가했다. 이 검토는 계약·계획의 정적 판단이며 실제 Rust API·fixture/profile 동결과 제품 실행 검증은 후속 의무다.

이번 실행은 문서·소스 읽기, revision/hash 기록, 문서 편집·의미 검토뿐이다. 제품 코드/fixture/oracle/harness 구현·제품 시험·모델 실행·Oracle differential·compiler/Clippy·문서 자동 검사·commit/push는 **미실행**이다. DEV3 및 Q1 실행 상태를 승격하지 않는다.

<a id="review-followup"></a>
## 5. 후속 리뷰와 D4-PREP-002 교정

사용자는 commit `0aa9e070a576c6e92d00818db06dd3d77945c518` 이후 새 계약·계획을 기본 계획·일반 DBMS/Oracle·내용 충분성에 비추어 다시 검토하도록 요청했고, 결과를 받은 뒤 개선을 지시했다. 작업·SQL 계약·통합 담당은 `/root`다. 최초 채택과 “남은 P1/P2 없음”은 그 시점의 판단으로 보존하며 후속 P2 발견을 소급 삭제하지 않는다.

| 항목 | 후속 관측·반영 |
| --- | --- |
| 독립 재검토 | `/root/dev4_contract_reassessment`가 원 계약·계획과 SQL 정본을 읽기 전용 대조. 진단 효과에 P2 한 건 확인; 독립 범위·DEV3 선행·자원·leaf 동결·시험 구조는 대체로 일치 |
| 원 반례 | FrontendDiagnostic 전체의 phase를 lex/parse/bind로 제한하고 기존 Idle/Active 보존을 요구했으나, 동일 범위의 typed predicate는 실행 중 취소를 반환할 수 있음. ON SELECT fetch 취소에는 cursor/snapshot 해제와 implicit 거래 종료가 필요하므로 SQL 정본과 충돌 |
| 계약 반영 | [진단 효과 경계](../contracts/dev4-independent-frontend.md#diagnostic-effects)에 순수 진단과 SQL adapter의 phase·cleanup·effect/state/outcome 소유를 분리. NoEffect·기존 상태 보존은 진입 전 검사로 한정. DDL prior commit과 cleanup 실패의 RecoveryRequired/Fenced 우선, 순수 앞단의 I/O 금지가 engine undo를 막지 않도록 명시 |
| 계획 반영 | FE10에 끝 공백 비교 기대값, FE11에 순수 진단의 취소/실패, FE14에 implicit fetch 취소 뒤 거래/result 유지 mutant를 연결. [후속 실제 연결 시나리오](../plans/dev4-preparation-plan.md#diagnostic-handoff-tests)에 기존 T03/T09/T11·owner·관측 의무를 배정하고 NOT_RUN 유지 |
| Oracle 보강 근거 | [Oracle19c Data Type Comparison Rules](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/Data-Type-Comparison-Rules.html)의 Blank-Padded and Nonpadded Comparison Semantics를 후속 리뷰에서 열람. 두 text literal의 `'a' = 'a '`는 TRUE, 한쪽 이상 VARCHAR2/NVARCHAR2이면 nonpadded 비교. XADB의 기존 Utf8 byte 비교에서는 FALSE이며 문서상 기대값으로 구분; Oracle binary 실행은 없음 |
| 구조 대조 근거 | [PostgreSQL18 Parser Stage](https://www.postgresql.org/docs/18/parser-stage.html)의 raw parsing과 transaction 내 catalog 의미 분석 분리 설명을 열람. XADB 독립 앞단 방향의 근거이며 PostgreSQL 전체 구현·SQL 호환성 주장 아님 |
| 선택·대체 | [D4-PREP-002 결정](../contracts/dev4-independent-frontend.md#diagnostic-effect-decision). 원 오류 효과 문구만 한정 교정하고 기존 SQL 의미·기능·자원 한도·DEV3 선행·단계 완료·시험 실행 정책은 유지 |
| 변경 범위 | 기존 앞단 계약·준비 계획·이 기록과 README 진입점. 기존 문서 ID·경로·역할 유지로 `documents.json` 등록 추가 없음. 동시 작업 중인 `crates/engine` 변경은 읽기 검토·수정·검증·commit 범위 밖 |
| 교정 중 지적·반영 | 독립 검토에서 원 P2 해소를 확인하고, 후속 DDL 시나리오 담당을 잘못 적은 `4.06/4.13`을 지적. 실제 owner인 `4.10.2/4.10.3·4.13.1.2`로 수정하고 native DDL 호출 전 검증 실패의 prior=Committed·ddl=NotAttempted·Idle을 명시. 작성자 대조에서 진단의 검출 모듈 구분도 보존해 parser 안쪽 lexer 오류의 lex/parse 유실을 방지 |
| 교정 독립 검토 | `/root/dev4_contract_reassessment`가 실제 3문서 diff·SQL 정본과 공식 Oracle/PostgreSQL 원문을 대조하고 위 최종 변경까지 확인하여 **D4-PREP-002 수용·추가 필수 수정 없음**으로 판정. `/root`가 반영을 확인하고 ACCEPTED로 전환. 자기 검토를 독립 검토로 계산하지 않음 |
| 이번 실행 | 문서 읽기·편집·diff 대조·독립 정적 검토. 제품·모델·Oracle 실행, compiler/Clippy·문서 자동 검사·commit/push는 이번 교정에서 미실행 |

이는 원 반례에 대한 계약·계획 교정이며, 실제 Rust API·fixture/hash·모델 탐색 profile은 기존 leaf 동결 의무로 남는다. 이번 문서 작업에는 제품 테스트를 선택하지 않으며 문서 자동 검사는 기존 push gate에서 수행한다.
