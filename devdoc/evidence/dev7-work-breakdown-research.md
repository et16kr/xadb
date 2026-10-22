# DEV7 세부 분류 — ARIES·Oracle·로컬 DBMS 조사

2026-10-22 · **소스·공식 문서 열람과 기존 계약의 분류 근거 / 제품·외부 DBMS 비교 실행 없음**

[36개 작업 분류](../plans/dev7-work-breakdown.md) · [64개 요구 배정](../plans/dev7-implementation-plan.md#coverage) · [기존 설계](../contracts/dev7-sql-extensions-design.md) · [최초 조사](dev7-source-research.md) · [DESIGN-002 검토](dev7-review-002.md)

<a id="scope"></a>
## 1. 조사 질문과 작업 범위

사용자의 “DEV3처럼 DEV7도 세부 항목으로 나누고 ARIES·Oracle·로컬 DBMS를 참고한 뒤 commit” 요청을 수행한다. 기존 7개 구현 묶음에서 공급·오류·자원·복구 책임이 다른 부분을 찾아 36개 분류와 하위 배정 단위로 펼쳤다. 기존 DESIGN-001/002를 재채택하거나 새로운 제품 의미를 결정하는 작업은 아니다.

착수 XADB HEAD는 `def35fb1e274040eb55d78fa68948ba1610166c1`, 작업 트리는 clean이었다. 작업/통합 담당은 `/root`이며 D0 문서 분류로 관리한다. 이번 자료 열람·담당 대조를 독립 검토 PASS나 제품 동등성 증거로 쓰지 않는다. 과거 소스·검토·실행 기록은 당시 identity 그대로 보존한다.

작업 중 DEV4/5 분류 commit으로 HEAD가 이동하고 DEV3/6의 동시 문서 변경도 관측했다. 해당 변경을 보존하며 공유 README/JSON에는 DEV7 참조만 추가한다. 이번 commit의 범위는 새 DEV7 분류·조사, 기존 DEV7 계획, 공유 색인/단계 문서의 DEV7 변경이다.

<a id="primary"></a>
## 2. ARIES와 Oracle 공식 자료

아래 URL은 2026-10-22 직접 열람했다. 페이지의 설명을 모든 XADB 의미로 복사하지 않고 이미 채택된 소유 계약의 작업 경계를 확인하는 데 사용했다.

| 자료·읽은 부분 | 관측 | 기존 계약의 분류에 적용 |
| --- | --- | --- |
| Mohan 외, [ARIES 원문](https://cs-people.bu.edu/mathan/reading-groups/papers-classics/aries.pdf), TODS 17(1), 1992, 인쇄 p.94/PDF 첫 페이지 초록 | partial rollback, history redo, rollback 변경의 logging·chain과 반복 장애에서 bounded logging을 함께 다룸 | 7.16 savepoint/CLR와 7.26 자원·재장애, 7.36 fault 인수를 연결. SQL 문법만 작성해 rollback 완료로 볼 수 없음. 이번에 확인한 범위는 첫 페이지이며 prepare의 상세 근거는 아래 Oracle·PostgreSQL 및 기존 DESIGN-002 조사에 둠 |
| Oracle 26ai [Data Type Comparison Rules](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/Data-Type-Comparison-Rules.html), Character Values·Blank-Padded/Nonpadded | 양쪽 문자 타입에 따라 padding 비교가 달라짐 | 7.01/03 비교 문맥 → 7.08 join → 7.13 optimizer로 인계. Oracle의 전체 implicit conversion 규칙을 새로 채택하지 않음 |
| Oracle 26ai [Transaction Processing and Control](https://docs.oracle.com/en/database/oracle/oracle-database/26/lnpls/transaction-processing-and-control.html), SAVEPOINT·Implicit Rollbacks | 명시 marker·숨은 statement marker·예외/host의 rollback 범위를 구별 | 7.16 marker 공급과 7.20 CALL 원자성·결과 spool을 별도 배정. Oracle의 marker 한도/호출 예외 동작을 XADB의 한도·atomic CALL 계약으로 오인하지 않음 |
| Oracle 26ai [Autonomous Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/26/lnpls/autonomous-transactions.html), Transaction Context·Savepoints·Avoiding Errors | autonomous는 부모와 별도 거래/marker·commit 수명을 가지며 부모가 보유한 자원과 교착 가능 | 7.21의 자식 거래와 부모 rollback/child UNKNOWN 인수. 7.20 일반 중첩 CALL과 분리하고 Oracle의 기본 격리를 XADB에 적용하지 않음 |
| Oracle 26ai [Distributed Transactions Concepts](https://docs.oracle.com/en/database/oracle/oracle-database/26/admin/distributed-transactions-concepts.html), Prepared Response·Steps in Prepare Phase·Commit Phase | prepared 응답에는 복구 가능한 변경과 장애 후 잠금 보장이 필요하며 coordinator 결정과 참여자 처리가 구별됨 | 7.31 native prepared/복구와 7.32 journal/TM 연결을 분리. Oracle의 commit point/SCN 구조는 이식하지 않으며 XADB의 원자성·전역 격리 구분 유지 |
| Java SE 21 [XAResource](https://docs.oracle.com/en/java/javase/21/docs/api/java.transaction.xa/javax/transaction/xa/XAResource.html), start/end·prepare·recover·forget | TM/RM API·flag, prepared/heuristic recover와 heuristic forget의 범위를 구별 | 7.31.1–5·7.32.3/4·7.33.3으로 배정. StandardXa 정상 retirement와 XADB의 별도 확장 ACK profile을 합치지 않음 |

기존 CMU ARIES URL은 이번 접속에서 timeout이 발생하여 위 대학 호스팅의 원문 첫 페이지를 열람했다. 해당 PDF 후반의 추가 조회도 실패했으므로 논문 전체를 이번에 다시 읽었다고 기록하지 않는다. 기존 조사 기록의 상세 열람을 이번 새 관측으로 재표시하지 않는다.

<a id="local"></a>
## 3. 실제 로컬 checkout과 열람 위치

[로컬 source catalog](../reference/opensource-references.md)에 따라 origin·HEAD·작업 트리를 확인했다. 아래 8개는 모두 clean이었고 origin은 catalog의 공식 저장소와 일치했다. 경로는 `/home/et16/work/<checkout>/` 아래 상대 위치다. fetch/pull/checkout·소스 수정·빌드·시험을 수행하지 않았다.

| 대상·실제 HEAD | 읽은 소스·줄 범위 | 관측과 분류 연결 |
| --- | --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `postgres/src/backend/executor/nodeHashjoin.c` 15–58; `src/backend/utils/cache/plancache.c` 14–80; `src/backend/access/transam/twophase.c` 14–75 (뒤 두 파일도 postgres 아래) | hash batch/skew·cache dependency/metadata·prepared lock/WAL/checkpoint 책임을 대조. 7.07/08·7.14·7.31 분리. hash best-effort 메모리 초과나 결과 metadata 자동 변경 허용을 XADB에 가져오지 않음 |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `mariadb/sql/sp_head.cc` 315–342·1630–1685 | COMMIT/ROLLBACK 분류와 routine security 전환·EXECUTE 재확인/실패 복원. 7.20의 실행 문맥·권한/거래 제어 인수로 연결. 서버 thread 구조는 복제하지 않음 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `cubrid/src/query/serial.c` 1180–1265, redo 호출 1190–1208 재확인 | serial 변경의 redo→page 갱신·system operation과 non-cycle 발급 범위를 관측. 7.23 범위 예약/발급/재시작을 일반 행 rollback과 분리. 이 발췌로 전체 sequence 내구성을 인증하지 않음 |
| MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613` | `mysql-server/sql/xa/sql_xa_commit.cc` 90–155 | attached/detached dispatch·one-phase/prepared 분기·metadata 잠금 실패의 재시도 경계. 7.31 상태/resolve와 7.33 adapter 분리. 후속 GTID 오류의 unilateral rollback 분기를 XADB의 prepared 결정 불변 규칙으로 가져오지 않음 |
| SQLite `43a48f77585a88940c727f33682151418f544059` | `sqlite/src/window.c` 15–103; `sqlite/src/fkey.c` 19–100 | window rewrite·partition 임시 보관, immediate/deferred FK와 statement/commit 판정. 7.10 window·7.17 deferred ledger를 각각 7.07/16에 연결. VM 이식·제약 identity를 잃는 counter만의 구현은 채택하지 않음 |
| ClickHouse `1d76716b7580f78d0375c626736e49a3e13f099c` | `clickhouse/src/Interpreters/GraceHashJoin.cpp` 300–342 | join별 temp child scope·spill 계측·threshold 입력을 읽음. 7.07 공통 자원과 7.08 hash operator를 구분. 컬럼 저장 엔진이나 별도 제품 저장 모드는 도입하지 않음 |
| TiDB `328460a9af032ec6b53823724f87837a6d2d5857` | `tidb/pkg/planner/core/plan_cache.go` 145–184·400–444 | schema 변화 시 준비 상태 재처리와 prepared 권한/잠금 확인을 관측. 7.13 비용 최적화와 7.14 cache 수명·권한·metadata를 별도 인수. 분산 KV 구조 도입 아님 |
| FoundationDB `487620d5aa41152b1f034f4e932005a87eb2947f` | `foundationdb/fdbclient/NativeAPI.cpp` 4715–4780 | commit 전달 가능성·결과 불명과 idempotency에 따른 확인 경로를 읽음. 7.29/30/32/33의 certainty와 효과 재시도 구분. 해당 제품의 재시도 방식·분산 구조를 그대로 채택하지 않음 |

catalog에 기록된 `/home/et16/work/duckdb`와 `/home/et16/work/rocksdb`는 이번 확인 시 존재하지 않았다. 두 저장소의 과거 revision을 이번 열람 revision으로 인용하지 않고 조사/근거 수에서 제외한다. catalog의 2026-10-18 확인 기록은 당시 증거로 보존한다.

<a id="application"></a>
## 4. 분류 판단과 범위 보존

| 판단 | 반영 | 새로 결정하지 않은 것 |
| --- | --- | --- |
| 공통 의미와 operator를 구별 | 7.01 descriptor/codec, 7.06 typed plan, 7.07 자원, 7.08–14 기능/계획 | 새 type ID·wire byte·optimizer rule·라이브러리 선택 |
| SQL 문법과 native effect를 구별 | 7.15–17 거래/제약 → 소비 SQL, 7.26 저장 예약 → 7.27 LOB | DEV3 전체를 재구현하는 별도 WAL/recovery, S1 기본 한도의 암묵 확대 |
| 호출과 독립 효과를 구별 | 7.20 CALL·7.21 autonomous·7.22 trigger·7.23 sequence·7.24 job | 모든 효과가 CALL rollback으로 취소된다는 의미 |
| 영속 prepared와 분산 결정을 구별 | 7.31 RM의 상태/복구 → 7.32 coordinator/TM → 7.30.3 Atomic2pc | local Serializable+2PC만으로 전역 Serializable 보장 |
| 기능 공급과 실제 소비를 구별 | 7.33 driver·7.34 언어/runtime·7.35 외부 교환 | 예제/로컬 mock을 실제 외부 업무 인수로 승격 |
| 보류도 책임을 남김 | R10-03 내부/R10-05는 7.35.3, D-LOGGING 유지 | logging 상세 조사·설계·시험 재개, 보류의 N/A 처리 |
| 시험 준비와 실행을 구별 | 7.36의 case/fixture/oracle를 각 기능과 함께 작성, 원래 source/case 결과 취합 | DEV3 S5 전수 예외를 DEV7로 확대, 무변경 시험 반복 |

36개 분류는 기존 요구의 탐색·배정 단위이고 전체 overload/operation/variant inventory의 동결 완료가 아니다. F01–F10·상위 DAG·SC07–11/SQL_EXT1 상태·보류는 유지한다. 분류만으로 구현률이나 성능 개선을 계산하지 않는다.

## 5. 수행·미수행 기록

수행한 것은 기존 owner 계약 열람, 공개 원문·로컬 소스 조사, 분류표/64개 요구 연결·문서 색인의 편집과 담당 diff 대조다. 제품 코드·빌드 manifest·외부 checkout·과거 증거를 변경하지 않았다. 제품 시험·compiler/Clippy·Oracle/외부 DBMS 비교 실행·새 모델 실행·문서 자동 검사는 수행하지 않았다. 문서 검사는 기존 `scripts/git-hooks` pre-push gate에서 수행한다. 사용자 요청의 commit은 이 문서군의 Git 이력으로 식별하며 push는 이번 범위에 포함하지 않는다.
