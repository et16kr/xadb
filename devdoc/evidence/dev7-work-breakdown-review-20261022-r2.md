# DEV7 작업 분배 후속 검토 — 제약 설치·작업 수명·분량

2026-10-22 · **D7-WBS-003 채택·독립 정적 검토 PASS / 제품 구현·시험·성능 측정 NOT_RUN**

[현재 작업 분류](../plans/dev7-work-breakdown.md) · [구현·검증 계획](../plans/dev7-implementation-plan.md) · [설계 정본](../contracts/dev7-sql-extensions-design.md) · [이전135개 분배 검토](dev7-work-breakdown-review-20261022.md)

## 판단과 범위

사용자가 DEV7 잔여 작업의 분배·분량·우선순위·내용·분류를 ARIES·Oracle 및 `~/work`의 DBMS 소스에 비추어 다시 검토하고 발견 사항을 즉시 반영한 뒤 한 번 commit하도록 요청했다. 이번 시작 HEAD는 `f6cb181`이며 이미 D7-WBS-002의36개 분류·135개 하위 작업이 있었다. 이전 검토를 새로 수행한 것처럼 재기록하지 않는다.

**36개 대분류와 native→SQL→server/client 공급 방향은 유지하는 편이 적절하다.** 값 의미와 물리 codec, RM과 coordinator, 기준 실행과 최적화, logging 보류를 구분한 기존 선택은 유지한다. 전체 번호를 다시 매기거나 동일 개수로 담당을 나누는 것은 이득이 작다. 다만135개도 균등한 구현 작업이 아니며, 아래의 설치/상태/실행 책임이 덜 드러나 있었다. 이5개 책임을 분리해140개로 보완하고 배정·비용 기준을 추가한다.

이는 현재 계약 아래 더 명확한 배정이라는 판단이다. 구현량·속도·실패율 측정 없이 유일한 최적 분배나 실제 인일을 증명하지 않는다. 140개에는 보류7.35.3이 포함되며 작업 수를 완료율로 사용하지 않는다. F01–F10·SC07–11·SQL_EXT1, 실제 S1/B3/CLIENT1/PART/NG 공급과 D-LOGGING의 미해결 상태는 유지한다.

<a id="findings"></a>
## 발견·반영

| ID·평가 | 기존 배정의 문제 | 반영한 책임·인수 |
| --- | --- | --- |
| R2-01 / P2 제약 설치 인계 | 7.17은 신규 DML 검사,7.18.1은 일반 DDL로 읽혀 기존 위반 데이터의 검증/설치 공개가 두 담당 사이에 남음. 새로운 DML만 검사하는 구현으로는 기존 행의 무결성을 인증할 수 없음 | 7.18.1 catalog 기반→7.17.5 기존 데이터 검증·동시 DML 보호·설치/삭제 통합. 실패 old metadata·precommit과 DDL outcome·공개/reopen·cache/optimizer 소비를 T11/T13/C04에 연결. 지원 상태는 F03/F04에서 동결하며 Oracle NOVALIDATE/RELY 등의 새 SQL 모드를 채택하지 않음 |
| R2-02 / P2 FK 분량 | 7.17.2에 보호 probe와 cascade가 같이 있어 조회 보호 공급과 다중 객체 effect/rollback 인수가 하나의 큰 완료 조건이 됨 | .2는 parent/child probe, 새 .4는 cascade worklist·cycle/중복 target·최종 제약/rollback. S1·7.16·해당7.26을 수신하고 trigger 합류는7.22에서 추가. native 공급에 PSM 전체 선행을 추가하지 않음 |
| R2-03 / P2 queue/job 실행 | .24.1/2에 거래 queue, 재시도 상태, due/lease, 실제 worker가 묶이고 완료 조건에만 dead-letter 등이 나타남 | 새7.24.4가 retry/dedup/dead-letter의 영속 상태·실패 원인별 rollback/확정 경계를 소유. .2는 job/due/lease/attempt, 새 .5는 실제 worker의 호출·권한·취소·stale 결과·회수/재시작. T18/C03/M05의 관련 사례를 별도 선택. native 공급·실제 실행 중 하나로 양쪽 PASS를 표시하지 않음 |
| R2-04 / P2 온라인 복구 순서 | 7.25.1/2 성공 전이 뒤 .3의 “복구”를 배정하면 durable job/owner와 각 상태의 restart 분기 책임을 말기에 몰 수 있음 | 새7.25.4가 상태/owner/recovery 공급·통합 담당. .1–3은 실제 build/capture·switch·abort/retire를 소유하고 각 effect와 함께 fault를 작성. .4 상태표는 초기 공급일 뿐 전체 복구 인수가 아님. switch 전 old·switch 후 new 유지와 stale owner/재장애를 T19/C04에 연결 |
| R2-05 / P2 우선순위 표시 | 본문은 native 선행을 구분하지만 Mermaid는 CALL→7.21–24 전체 연결이라 sequence·queue도 PSM 전체 대기처럼 보임 | sequence·queue·job 상태를 S1의 좁은 공급으로 분리하고 해당 job의 PSM 호출만 worker에 연결. 온라인 상태 준비도 순서0에 표시. 상위 task 인수 DAG는 변경하지 않음 |
| R2-06 / 배정·비용 개선 | 135개라는 수와 정성적 “큼”만으로는 배정 완료 범위·환경 및 검증 부담을 비교하기 어려움 | [배정 기준](../plans/dev7-work-breakdown.md#allocation)에 공통 공급/의미 알고리즘/거래 통합/영속 lifecycle/외부 환경/검증의 성격과 닫을 경계를 명시. P profile의 셀당 기본 최소30분과 별도 준비·표본 확보·replay 비용을 드러냄. 새 시간 제한이나 시험 실행 지시가 아님 |

7.18.1이 다시7.17 전체를 기다리는 순환을 만들지 않도록 catalog 기반 공급과 제약 설치 통합을 구분했다. queue의 row 잠금과 scheduler의 lease fencing도 같은 의미로 공통화하지 않는다. 온라인 복구를 독립 모듈 하나가 대신 수행한다고 가정하지 않고, .4가 전이 소유자의 실제 처리를 합류시킨다.

## 유지한 분류와 분량 판단

| 대상 | 판단 |
| --- | --- |
| 7.01–05 값·식 | 공통 의미/codec 소유 분리는 유지. decimal/문자/tzdb·regex/crypto를 단순 정수 범위와 같은 분량으로 세지 않고 family/operation별 배정 |
| 7.06–14 관계·optimizer/cache | 기준 실행과 알고리즘·검사 전제를 분리한 기존 구성이 적절. window frame·재귀 상태가 크더라도 독립 expected와 지원 variant로 먼저 좁히고 문법마다 새 ID를 기계적으로 만들지 않음 |
| 7.15–19 거래·제약·catalog | native 격리/savepoint를 유지하고 R2-01/02의 제약 공급·설치 책임을 보완. full/incremental refresh 분리와 실제 PART/NG 수신 유지 |
| 7.20–25 PSM·객체 | 이미 나뉜 PSM frame/예외/cursor·package/autonomous·sequence를 유지. 큐 재시도·scheduler worker·온라인 상태/복구가 서로 다른 실패 단위이므로5개 추가 중3개를 이 영역에 배정 |
| 7.26–28 저장·LOB/공간 | opcode별 forward/undo/CLR와 공존 peak의 선행을 유지. LOB 수명과 기하 dependency/연산은 다른 분량이므로 지원 셀별 배정. 공간 파일 adapter까지 engine 한 담당의 작은 부속 작업으로 세지 않음 |
| 7.29–32 외부 효과·XA | worker/link mode·RM/coordinator 분리 유지. prepare 전에 예약·복구/resolve가 성립해야 한다는 ARIES 비교의 핵심을 유지하며 native와 실제 TM 인수 분리 |
| 7.33–36 client·교환·검증 | adapter/runtime 곱·설치/권한·단절·독립 oracle 비용을 포함. 환경 준비는 초기, 실제 통합은 공급 뒤. 7.36을 말기의 테스트 작성 작업으로 사용하지 않음 |

우선순위는 준비된 실제 공급→정확성/복구 위험→소비자 해제→작은 실제 통합 완료 순으로 적용한다. 전체 일정·실제 인력 수는 아직 산정할 수 없으며, native/SQL/client라는 서로 다른 증거를 하나의 “완료”로 합치지 않는다.

<a id="sources"></a>
## 이번에 읽은 원문

2026-10-22 열람. 문서와 소스의 책임 분리를 비교했으며 DBMS 실행·벤치마크·동등 SQL 인증은 수행하지 않았다. 아래 외부 기능 목록이 XADB 필수 기능을 자동 확장하지 않는다.

| 자료·읽은 부분 | 적용·비채택 |
| --- | --- |
| [ARIES, Mohan 외, TODS 17(1), 1992](https://www.cs.cmu.edu/~15849g/readings/mohan92.pdf), §5.2 p.117–118·§5.3 p.120 | statement/savepoint 복원과 CLR의 undo-next 연결, prepare의 영속 보호 정보를 확인. cascade/queue/온라인 전이의 실패 경계와 복구를 첫 effect부터 배정하는 근거. 논문의 savepoint 소멸/잠금 해제 선택을 XADB 의미로 복사하지 않음 |
| [Oracle26ai Data Integrity](https://docs.oracle.com/en/database/oracle/oracle-database/26/adfns/data-integrity.html), §13.16·13.16.2 | 현재 DML enforcement와 기존 데이터 validation은 다른 책임이며 위반 데이터가 있으면 validated 설치가 실패함. 7.17.5 배정 근거; Oracle의4개 상태·assertion·추가 문법 채택 아님 |
| [Oracle26ai AQ Introduction](https://docs.oracle.com/en/database/oracle/oracle-database/26/adque/aq-introduction.html), Dequeue Modes·Retry with Delay | transaction dequeue와 retry count/delay/exception 처리가 구분됨. rollback 뒤 재시도 상태 갱신의 경합 설명을7.24.4의 독립 동결/인수 이유로 사용. Oracle counter 갱신 방식이나 buffered queue를 도입하지 않음 |
| [Oracle26ai Scheduler](https://docs.oracle.com/en/database/oracle/oracle-database/26/admin/scheduling-jobs-with-oracle-scheduler.html), Creating Jobs·Making Steps Restartable | job 정의/enable·권한과 실제 실행·recovery 후 재시작 정책은 별도. 상태와 worker 배정 근거; Oracle chain·external executable 전체를 XADB에 추가하지 않음 |
| [Oracle26ai Managing Tables](https://docs.oracle.com/en/database/oracle/oracle-database/26/admin/managing-tables.html), online redefinition의 sync/finish·Terminating/Cleaning Up | build 이후 동시 DML 동기화·종료 대기·abort 정리가 다른 단계임을 확인. .25.4의 전체 수명 책임 근거; Oracle의 finish 후 rollback 기능을 XADB의 switch 후 cleanup에 섞지 않음 |

ARIES는 웹 도구의 timeout 후 같은 원문을 HTTP로 받아 텍스트를 추출해 위 부분을 읽었다. PDF SHA-256은 `6131bcee36c21e669d41204ef2b2320cb458b5d3bd9b7cf3e7758afe8ed3495f`다. 전체 논문 정독이나 새 복구 모델 실행으로 보고하지 않는다.

[로컬 catalog](../reference/opensource-references.md)에서 시작해8개 checkout의 origin/HEAD/작업 트리를 확인했다. 모두 catalog origin/revision과 일치하고 clean이었다. 아래4개는 이번에 실제 소스 내용을 읽은 대상이다. `/home/et16/work/duckdb`·`rocksdb`는 없었다. 그 외4개의 이전 조사 결과를 이번 소스 열람으로 세지 않는다. 외부 checkout을 수정·fetch·build·test하지 않았다.

| checkout·실제 revision | 이번 주 담당의 열람 위치 | 판단에 사용한 책임 |
| --- | --- | --- |
| postgres `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/backend/commands/tablecmds.c` 5905–5926,14178–14276; `src/backend/commands/indexcmds.c` 1660–1718,1795–1859; `src/backend/access/transam/twophase.c` 1148–1228 | 기존 행 FK 검증의 별도 scan·relation lock 전제, concurrent index의 준비/유효성·old snapshot 대기·cache invalidation, prepare 데이터의 복구 가능 크기 확인·WAL flush |
| mariadb `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `sql/event_scheduler.cc` 276–336; `sql/event_data_objects.cc` 934–985 | worker의 job load/실행/종료와 due 계산·disabled/completion 상태를 분리. MariaDB의 thread 정책을 XADB bounded worker 계약으로 대체하지 않음 |
| cubrid `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/transaction/log_manager.c` 7740–7788,8000–8075 | rollback 종료 LSA와 logical compensate/system operation의 다음 복원 위치. SQL 소비자마다 별도 undo 엔진을 만들지 않고 공통 복원 공급에 합류할 근거 |
| sqlite `43a48f77585a88940c727f33682151418f544059` | `src/fkey.c` 19–95 | statement/commit 제약 경계·insert/delete/update별 검사와 단순 deferred counter의 기존 위반 데이터 한계. counter 알고리즘 복사 대신 설치 검증/ledger 책임 분리 |

<a id="decision"></a>
## D7-WBS-003 결정·소유·검토

| 항목 | 기록 |
| --- | --- |
| ID·일자·분류·상태 | D7-WBS-003 · 2026-10-22 · D2 · ACCEPTED |
| 작업·계약·통합 | `/root`; 별도 읽기 전용 검토 `/root/dev7_distribution_review`. AGENTS와 DP01의 독립 검토 의무에 따른 별도 AI 실행이며 제품 병렬 구현은 없음 |
| 위임 | 사용자의 즉시 개선·완료 뒤 한 번 commit 요청과 DP01의 기존 위임. 제품 개발·시험 재개 아님 |
| 기준·동시 변경 | 시작 HEAD `f6cb181`; 편집 전 보존 snapshot HEAD `f6c965b9b133ffc083532215f98905f89e00c26f`, 해당 tracked diff SHA-256 `ae8f90f322b7abd8e97e19fd9355464edd2844baa08b93155b82eb2682132b0a`. 조사 중 다른 담당의 DEV4/5 commit과 DEV3/6·공유 색인 변경을 관측; 최신 파일의 DEV7 부분만 편집/commit |
| 소유 경로 | `plans/dev7-work-breakdown.md`, `plans/dev7-implementation-plan.md`, 이 새 증거. 공유 `README.md`, `plans/README.md`, `evidence/README.md`, `documents.json`은 DEV7 소개/등록만 총괄 반영 |
| 대안 | 135개 유지;36개 전체 재편; 구별되는 실패/설치/실행5개만 추가 분해. 마지막을 선택해 기존 추적·범위와 원자성 책임 보존 |
| 대체 범위 | D7-WBS-002의 해당 하위 배정/인계만 보완. 이전115→135 이동·검토 지문·제품 DESIGN-001/002·상위 DAG·수치 한도·보류를 대체하지 않음 |
| 합류 | 계획2개→조사/색인→독립 diff/원문 검토→지적 반영→이번 변경만1회 commit. 문서 gate는 push 직전 |
| 검증·한계 | 문서·요구/하위 ID의 정적 대조와 diff 확인만 수행. 제품/test/harness 수정·테스트·fmt/Clippy·문서 verifier·성능 측정·push 미실행. 미실행 사례를 PASS로 승격하지 않음 |
| 재검토 조건 | 하위 공급 순환, 설치 상태와 optimizer 가정 불일치, queue retry/worker outcome 혼동, 온라인 복구 소유 공백, 실제 variant/runtime/실행 비용이 배정 범위를 넘음 |

독립 검토자 `/root/dev7_distribution_review`가 두 계획·이 증거 초안·공유 색인의 DEV7 변경과 owner 계약을 읽고, ARIES §5.2/5.3·PDF 지문, Oracle 제약/AQ/Scheduler, PostgreSQL·MariaDB·CUBRID·SQLite 관련 소스를 별도 대조했다. 초기 검토의 catalog→제약 설치 순환 방지, Oracle 추가 문법 비채택, native cascade와 trigger 인수 분리, 온라인 상태표와 실제 복구 인수 구별을 반영했다. 최종 결과는 **정적 계획 검토 PASS, 남은 필수 P1/P2 없음**이다. 검토자는 파일 수정·시험·빌드·문서 verifier·commit을 하지 않았다.

| 검토 snapshot | SHA-256 |
| --- | --- |
| `plans/dev7-work-breakdown.md` | `ea27fdbfb2e46d09ad40ea28ef5e378523635f9060e4e329b69d5be409e721bf` |
| `plans/dev7-implementation-plan.md` | `a5a01097159428b678a84b4e70f02610a28ecce55b40bf89da511f5e170ac34f` |
| 이 증거 초안 | `488ce0202c44d210730b689f98476d9d50c5ab4582cc33e9a9f1ad97f246ae10` |

총괄은 위 계획 지문 일치를 확인했다. 검토 뒤에는 WBS의 채택 표시, 이 검토 결과 기록, PostgreSQL 추가 열람 범위1148–1228과 WAL flush 확인 기록만 갱신했다. 정적 집계는 대분류36·중복 없는 하위 ID140·요구64개이며 제품 시험 PASS가 아니다. 이번 DEV7 문서 변경만 한 번 commit하고 동시 작업은 보존한다.
