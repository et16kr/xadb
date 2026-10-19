# DEV7 — Oracle·Altibase·로컬 DBMS 조사와 설계 검토

2026-10-19 · **문서/소스 열람·독립 아키텍처 설계 검토 PASS / DBMS 비교 실행·제품 시험 NOT_RUN**

[상세 설계](../contracts/dev7-sql-extensions-design.md) · [구현·검증 계획](../plans/dev7-implementation-plan.md) · [로컬 소스 catalog](../reference/opensource-references.md)

<a id="scope"></a>
## 1. 실제 조사 범위와 한계

사용자의 DEV7 설계와 Oracle·Altibase·`~/work` DBMS 참고 요청에 따라 공개 문서와 관련 소스 부분을 읽었다. 아래는 전체 소스 감사·제품 실행·dialect 동등성 증거가 아니다. 알고리즘/책임의 연구이며 코드를 복사하거나 외부 dependency를 도입하지 않았다. 연구용 checkout과 기존 이관 소스는 수정하지 않았다.

catalog에 있던 DuckDB `/home/et16/work/duckdb`와 RocksDB `/home/et16/work/rocksdb`는 조사 시점에 존재하지 않았다. 읽지 않은 두 소스의 과거 revision을 이번 조사 근거로 사용하지 않는다. 나머지 공개 DBMS 8개는 실제 HEAD·origin·clean 작업 트리를 확인했다. FoundationDB는 KV의 결과 불명 경계만 참고하며 XADB 저장 엔진 대체안이 아니다.

Altibase는 `/home/et16/work/altibase_manula`의 **공식 ALTIBASE/Documents 문서 checkout**을 발견하여 7.3 영문 Markdown의 관련 절을 읽었다. origin은 `git@github.com:ALTIBASE/Documents.git`, HEAD는 `f2cbd3cb1baef2c235453d1bee1bbd8ea5a3bc1a`, 작업 트리는 clean이다. 공개 7.3 웹 색인은 검색했으나 직접 open은 HTTP429였으므로 그 본문을 열람했다고 하지 않는다. `/home/et16/work/official-altibase`는 설치 배포물 디렉터리이며 Git 소스가 아니다. 해당 서버를 시작하거나 기존 DB에 연결하지 않았다. Oracle은 26ai 공식 문서를 웹에서 읽었으며 Oracle 소스 접근/실행은 없다.

<a id="oracle"></a>
## 2. Oracle 공식 문서

| 자료 / 확인일 | 읽은 내용 | XADB 선택·검증 연결 |
| --- | --- | --- |
| [26ai SQL Data Types](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/Data-Types.html), 2026-10-19 | NUMBER의 precision/scale, FLOAT와 binary floating 구별, DATE/TIMESTAMP/interval, 문자·LOB의 타입 구분 | 숫자 이름만으로 표현을 추정하지 않고 typed descriptor·cast·key/wire를 함께 고정. Oracle 전체 범위/implicit conversion 복제 아님; T01–04 |
| [26ai Analytic Functions](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/Analytic-Functions.html), 2026-10-19 | grouping 이후 window, peer 동률·ROWS/RANGE/GROUPS, 기본 frame, 결과 정렬과 window 정렬의 구별 | peer/frame 독립 vector와 total ordering 없는 결과의 인수 방식; T07 |
| [26ai Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/26/cncpt/transactions.html), 2026-10-19 검색 발췌 | savepoint·autonomous/distributed transaction의 구분 | 자식의 독립 commit과 부모 rollback을 혼동하지 않음; T12/T16. 전체 문서 완독 근거 아님 |
| [26ai Autonomous Transactions](https://docs.oracle.com/en/database/oracle/oracle-database/26/lnpls/autonomous-transactions.html), 2026-10-19 | autonomous transaction의 독립성·주 거래와의 수명 | 일반 CALL과 autonomous 경계를 별도 설계, child outcome·self-lock 검증; T16 |
| [26ai Distributed Transactions Concepts](https://docs.oracle.com/en/database/oracle/oracle-database/26/admin/distributed-transactions-concepts.html), 2026-10-19 | distributed transaction·2PC phase와 역할 | durable decision과 참여자/응답 유실의 구별. Oracle의 commit-point 최적화를 그대로 채택하지 않고 단순 coordinator/RM 모델; T25/T26 |

Oracle의 SQL 사용성은 설계 입력이다. XADB의 기본 SERIALIZABLE·stale prepared 거절·자원 한도·오류 envelope는 자체 계약이다. Oracle 응답을 독립 XADB 정답으로 저장하지 않았다.

<a id="altibase"></a>
## 3. Altibase 7.3 공식 문서 checkout

아래 경로는 `Manuals/Altibase_7.3/eng/` 아래이며 링크는 실제 읽은 commit에 고정한다. 문서의 메모리/혼합 저장 제품 구조는 XADB에 채택하지 않는다.

| 문서·열람 위치 | 관측과 설계 판단 |
| --- | --- |
| [General Reference-1.Data Types & Altibase Properties](https://github.com/ALTIBASE/Documents/blob/f2cbd3cb1baef2c235453d1bee1bbd8ea5a3bc1a/Manuals/Altibase_7.3/eng/General%20Reference-1.Data%20Types%20%26%20Altibase%20Properties.md), FLOAT/NUMBER 877–946행 | FLOAT는 decimal precision 설명, NUMBER는 인자 유무에 따라 NUMERIC/FLOAT 의미를 구분한다. XADB FLOAT를 binary로 오인하지 않도록 타입 이름·표현·cast 의무를 분리; F01/T01 |
| [Stored Procedures Manual](https://github.com/ALTIBASE/Documents/blob/f2cbd3cb1baef2c235453d1bee1bbd8ea5a3bc1a/Manuals/Altibase_7.3/eng/Stored%20Procedures%20Manual.md), autonomous pragma 6911–6945행 | 독립 transaction의 lock/savepoint/rollback/commit과 부모와의 deadlock 가능성을 설명한다. 별도 transaction/quota·parent-child wait 검출 채택; F05/T16 |
| [DB Link User's Manual](https://github.com/ALTIBASE/Documents/blob/f2cbd3cb1baef2c235453d1bee1bbd8ea5a3bc1a/Manuals/Altibase_7.3/eng/DB%20Link%20User%27s%20Manual.md), 439–485행 | remote statement/simple commit/2PC mode와 로그·recovery phase가 분리돼 있다. XADB는 independent remote와 atomic2pc를 명시하고 simple commit을 원자 인수로 채택하지 않음; F07/T24/T26 |
| [API User's Manual](https://github.com/ALTIBASE/Documents/blob/f2cbd3cb1baef2c235453d1bee1bbd8ea5a3bc1a/Manuals/Altibase_7.3/eng/API%20User%27s%20Manual.md), XA 655–692행 | RM/TM·in-doubt/heuristic 용어와 거래 역할. session 단절과 durable prepared 상태를 별도로 추적; F07/T25 |
| [External Procedures Manual](https://github.com/ALTIBASE/Documents/blob/f2cbd3cb1baef2c235453d1bee1bbd8ea5a3bc1a/Manuals/Altibase_7.3/eng/External%20Procedures%20Manual.md), 270–301행 | 별도 agent process와 internal mode가 구별된다. XADB는 별도 process와 bounded ABI만 채택, internal load 제외; T23 |
| [Spatial SQL Reference](https://github.com/ALTIBASE/Documents/blob/f2cbd3cb1baef2c235453d1bee1bbd8ea5a3bc1a/Manuals/Altibase_7.3/eng/Spatial%20SQL%20Reference.md), 420–446·866–885행 | geometry의 7 subtype과 WKT/WKB·확장 SRID 표현/내부 형식 구분. XADB도 교환 codec과 내부 codec을 분리하며 exact/index 동등성을 별도 인수; F06/T21/T22 |

<a id="local"></a>
## 4. 로컬 공개 소스의 실제 revision과 열람

모든 경로는 `/home/et16/work/<checkout>/` 기준이다. origin은 [catalog](../reference/opensource-references.md)의 해당 공식 GitHub origin과 같으며 실제 조회했다. 줄 범위는 이 commit의 조사 위치이지 현재 XADB 소스 위치가 아니다.

| 대상 / 실제 HEAD | 읽은 부분 | 관측·XADB 판단 |
| --- | --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `postgres/src/backend/executor/nodeHashjoin.c` 1–120, `utils/cache/plancache.c` 1–100, `access/transam/twophase.c` 1–85 (뒤 두 경로도 `src/backend/` 아래) | hash batch/spill·skew, generic/custom cache와 dependency invalidation, prepared WAL/checkpoint/locks. XADB는 hard quota·metadata 고정·prepared 보호 복구를 채택; best-effort 메모리 초과와 PostgreSQL backend 구조는 도입하지 않음 |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `mariadb/sql/sp_head.cc` 1630–1685, recursion/security symbol 검색 | routine security 전환 뒤 EXECUTE 재검사·실패 시 복원. XADB definer/invoker·권한 철회·cache/PSM 경계의 근거; thread 구조 복제 없음 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `cubrid/src/query/serial.c` 391–445·1010–1090 | serial cache 유무·범위 진행과 catalog 반영 경계를 읽음. XADB sequence durable reservation/발급은 자체 계약으로 설계하며 이 발췌만으로 CUBRID 전체 내구 동작을 증명하지 않음 |
| MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613` | `mysql-server/sql/iterators/hash_join_iterator.cc` 370–455, spill symbol 검색 | NULL join key와 match flag, disk chunk·열린 파일 수 제한. outer unmatched·spill FD 인수에 반영 |
| SQLite `43a48f77585a88940c727f33682151418f544059` | `sqlite/src/window.c` 1–95, `src/fkey.c` 1–100 | window rewrite의 clause 위치·공유 정렬, immediate/deferred FK와 statement 복원. XADB는 VM 이식 없이 typed plan/독립 제약 ledger로 구현; counter만으로 오류 identity를 잃지 않음 |
| ClickHouse `1d76716b7580f78d0375c626736e49a3e13f099c` | `clickhouse/src/Interpreters/GraceHashJoin.cpp` 300–342, bucket/spill symbol 검색 | child temporary scope·spill 계측·명시 threshold. XADB도 합산 quota·실제 spill 지표를 사용; 컬럼 저장 엔진 채택 아님 |
| TiDB `328460a9af032ec6b53823724f87837a6d2d5857` | `tidb/pkg/planner/core/plan_cache.go` 145–184·400–444 | schema version 변경에 따른 준비 상태 무효화와 prepared 권한 확인. XADB는 cache hit의 execute 권한 검사·결과 metadata 고정 유지 |
| FoundationDB `487620d5aa41152b1f034f4e932005a87eb2947f` | `foundationdb/fdbclient/NativeAPI.cpp` 4715–4780 | 전달됐을 수 있는 commit의 불명 상태와 별도 idempotency 판단. XADB에서는 확인 없는 재시도를 채택하지 않고 UNKNOWN/Prepared/terminal outcome을 분리 |

<a id="decisions"></a>
## 5. 설계에 반영한 비교 결론

| 쟁점 | 채택 | 배제·재검토와 필요한 증거 |
| --- | --- | --- |
| 넓은 SQL와 작은 초기 실행기 | typed relational plan+pull execution·정렬 기준선+bounded hash/spill | 무제한 hash/partition materialization. T05–10/T30과 실제 큰 DISK workload |
| planner/cache | dependency·권한·semantic profile을 key/검사에 포함, 명시 reprepare와 다음 execute 전 물리 재최적화 | metadata 바뀐 handle의 자동 성공·결과 반환 중 재시작. T10의 race |
| 값·확장 저장 | type/codec/key/wire 동결, 큰 LOB용 별도 D7-STORAGE 공급 | 1 MiB 거래 상한을 그대로 둔 채 큰 LOB 성공 주장. T01–04/T12/T20 |
| transaction 효과 | 일반 CALL·savepoint·autonomous·sequence·remote effect를 구분 | 모든 statement 실패가 모든 외부 효과를 되돌린다는 주장. T15–18/T23–26 |
| 참조 무결성 | parent/child writer의 공통 보호와 최신 constraint probe | snapshot 조회만 한 FK. T11의 두 거래 반례 |
| XA | durable prepared+잠금/undo/validation fence+coordinator decision+retry receipt | connection 종료 시 prepared abort, 순차 remote commit, timeout을 abort 판정으로 사용. C05–C08 |
| 기능 보존 | 64개 요구를 작업/시험에 명시 배정; API/overload 동결은 OPEN | scheduler·online 변경·언어 adapter·CDC 등을 어렵다는 이유로 누락. 전체 gate에서 누락 검사 |

이 선택은 공개 소스들이 모두 동일한 구현이라는 주장이 아니다. XADB의 stage/오류·영속성·자원 계약에 맞춰 연구 결과에서 필요한 책임만 선택했다. 라이선스/배포 의무가 있는 코드를 가져오려면 별도 DP04 판단이 필요하다.

<a id="review"></a>
## 6. 작업 identity·독립 검토·실행 상태

기준 XADB HEAD는 `b7dcf29589a8c91f88d1e17aaff603f7f964e795`다. 조사 중 관측한 `git diff --binary HEAD`의 SHA-256은 `975919471d048644728a4e89cffe852d6beaef6d3616c296dcfba32e701c13f1`이며 **그 순간의 tracked diff만** 식별한다. untracked 파일을 포함한 제품 source fingerprint나 clean export identity가 아니다. 작업 트리는 기존 engine/testing/COMM 등 변경을 포함한다. 이번 설계는 그 변경을 수정·실행·승격하지 않는다.

소유 파일은 새 DEV7 계약·계획·이 조사 기록, 공유 편집은 문서 README·documents.json·development-stages·stage-completion·capability/feature inventory의 관련 DEV7 참조다. 공유 파일은 `/root`가 최신 원문에 필요한 항목만 반영한다. 기존 feature inventory 변경과 DEV6 조사·COMM 진행 기록은 보존한다. 제품 source·과거 실행 증거·빌드 root/manifest를 변경하지 않는다.

독립 D2 검토자 `/root/dev7_design_review`가 DEV7 원문·기존 owner 계약·관련 diff와 반례를 읽기 전용으로 검토했다. 1차 결과는 수정 필요이며 아래 5건을 `/root`가 반영했다. 같은 별도 검토자가 최신 계약·계획·기록·DEV7 완료 JSON을 재검토하여 **설계 수준 PASS, 필수 P1/P2 잔여 없음**을 보고했다. 계약/통합 담당 `/root`가 반영을 확인하고 DEV7-DESIGN-001을 아키텍처 설계 범위에서 ACCEPTED로 전환했다. 담당 자기 검토를 독립 PASS로 쓰지 않는다.

| 검토 ID / 중요도 | 반례·수정 | 검증 연결 |
| --- | --- | --- |
| D7-R01 / P1 | 두 RM의 서로 반대 read/write 순서는 각각 local Serializable이어도 전역 cycle을 만든다. 2PC 원자성/peer 격리/전역 격리를 분리하고 미검증 전역 수준은 effect 전 거절 | F07·T24–26·M04 |
| D7-R02 / P1 | Prepared를 기다리는 DDL/backup이 resolver까지 닫으면 해소 불가. engine owner·Preparing/close 선형화·예약 resolve lane·backup Busy·shutdown 영속 인계·일반 victim/만료 rollback 제외 | F07·T25/T26·C05/C07 |
| D7-R03 / P2 | 여러 statement/동적 trigger가 기존 B 잠금 뒤 A를 요구하면 전역 정렬 불가. 알려진 신규 closure만 정렬하고 기존 잠금은 유지하며 wait-for/deadlock 적용 | F03·T11 |
| D7-R04 / P2 | CALL의 DML+결과 prefix 뒤 후속 오류가 발생할 수 있음. 성공 전 bounded spool/OUT 비공개·spool REF CURSOR 소유·성공 뒤 late-fetch 실패 의미·LOB pin 공급을 명시 | F05/F06/F08·T15/T28 |
| D7-R05 / P2 | 기존 R10-05 상세와 R10-03 logging 내부는 D-LOGGING 보류. 새 설계도 보류를 유지하고 T27/C10은 외부 Oracle/JDBC adapter와 HDFS/Hive 교환으로 한정. 필수 예약 자체는 삭제하지 않음 | coverage·completion, capability의 DEFERRED_BY_USER·D-LOGGING 유지 |

hash skew fallback의 미공개 partition 한정·중복 반환 금지와 trigger의 문장 최종 UNIQUE/FK도 담당 검토에서 명확히 했다. 이 기록은 문서 의미 검토이며 반례 프로그램·제품 모델/복구를 실행한 결과가 아니다.

재검토 PASS는 64개 요구와 보류 의무의 배정, 위 경계 보완과 아키텍처 채택에 한정한다. F01–F10 상세 동결·구현·SC07–SC11·SQL_EXT1·성능·배포 인수를 포함하지 않는다. 검토자는 파일 수정·문서 자동 검사·제품 시험을 수행하지 않았다.

이번에 수행한 것은 repository 계약/공개 소스·문서 열람과 설계 작성이다. 제품 build/test, Oracle/Altibase DB 실행, 성능 측정, bounded model 실행, 문서 자동 검사, commit/push·배포는 수행하지 않았다. pre-push hook은 기존 `scripts/git-hooks` 설정을 확인했으며 변경하지 않았다.
