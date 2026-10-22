# DEV7 작업 분배 검토 — ARIES·Oracle·로컬 DBMS 대조

2026-10-22 · **D7-WBS-002 배정·인수 보완 채택·독립 정적 검토 PASS / 제품 구현·시험 실행 없음**

[검토 대상](../plans/dev7-work-breakdown.md) · [구현·인수 계획](../plans/dev7-implementation-plan.md) · [설계 정본](../contracts/dev7-sql-extensions-design.md) · [이전 분류 조사](dev7-work-breakdown-research.md)

## 판정과 범위

**36개 상위 분류는 유지하는 것이 적절하다. 초기115개 하위 ID에는 추가 분해와 인계 구체화가 필요했다.** 아래 WBR-01–06은 초기 문서를 대상으로 한 발견 기록이다. 후속 사용자 요청에 따라135개 하위 작업·공급/인수 연결·위험 우선 준비로 [보완했다](#applied). 구현 시간·재작업·통합 비용을 측정하지 않았으므로 이 안이 가능한 모든 대안 중 최적이라고 증명하지 않는다.

사용자는 처음 분배·분량·우선순위·내용·분류의 검토를 요청했고, 이어 발견 사항의 즉시 수정과 완료 후 한 번의 commit을 요청했다. `/root`가 자료를 읽고 조사 기록을 작성한 뒤 D2 배정·인수 보완을 반영한다. 아래 최초 판정과 입력 지문은 보존하고 후속 적용·독립 검토는 끝부분에 구별해 기록한다. 기존 DESIGN-001/002와 당시 독립 검토 기록은 보존한다.

| 확인 항목 | 결과와 한계 |
| --- | --- |
| 요구 누락·중복 | capability inventory의 DEV7 대상 64개와 계획의 64행을 대조했다. 요구 ID 누락·추가·중복 없음 |
| 분류 수 | 표의 상위 36개·하위 115개 ID를 확인했다. 하위 ID 중복 없음. 7.35.3의 보류 추적을 포함하므로 115개 모두가 실행 가능한 구현 작업은 아님 |
| 책임 방향 | 값→관계→최적화, engine 거래→SQL 소비, native RM→coordinator/driver, 공통 spill→operator 방향은 타당 |
| 상세 명세 상태 | F01–F10 미동결과 `detail_expansion_required=true`를 사실대로 남김. 이 자체를 결함으로 보지 않음. 해당 leaf의 착수에 필요한 입력만 먼저 구체화해야 함 |
| 실행 상태 | 코드·빌드·시험·외부 DBMS 비교 실행 없음. 구조적 coverage는 operation/variant의 완전성이나 SQL_EXT1 PASS가 아님 |

검토 시 HEAD는 `9f79b31b99f558f4e9a98b8bd752220b851e7bbb`였다. 시작할 때 `devdoc/README.md`와 `devdoc/documents.json`에 기존 변경이 있었으며 보존했다. 아래 지문은 검토 입력을 식별하고 제품 source/export 검증을 뜻하지 않는다.

| 입력 | SHA-256 |
| --- | --- |
| `plans/dev7-work-breakdown.md` | `0d00f45fc7b44b87fcc8b2a8a6531275b327379316226cbe30d09b25e37f21e8` |
| `plans/dev7-implementation-plan.md` | `439c25f71a2931c8a7a5d7a7b29769939cf9c1787e69ad26a3980fb5c4e21b6f` |
| `contracts/dev7-sql-extensions-design.md` | `8c50d0a03c15e98f75ddbf4debe2b6d57b7769ca211a56a35f7dff282fa3c31d` |
| 보고서 작성 전 tracked `git diff --binary HEAD` | `bc0798e725d2baf1e20c5b0052996a729e9096669332965135980e334189df37` |

<a id="findings"></a>
## 보완 권고

### WBR-01 — 일부 하위 ID도 여전히 여러 독립 작업을 포함한다

**해당 작업을 배정하기 전에 분해할 항목이다.** 번호가 세 자리라는 이유로 하나의 구현·검토 단위가 되지는 않는다. 현재 계획도 추가 variant 분해를 허용하지만, 실제 배정 단위라고 부르는 표에는 아래 차이가 남아 있다.

| 현재 하위 작업 | 권장 분해 경계 | 분리 이유 |
| --- | --- | --- |
| 7.05.3 regex·hash/crypto | regex 지원/실행 상한; hash 함수; 암호 API·key/nonce·인증 실패 | 의존 라이브러리·오류·보안 검증이 다름 |
| 7.08.2 hash/merge/index join | 알고리즘별 구현과 인수; 공통 비교 문맥과 spill 공급은 공유 | 같은 SQL 결과라도 상태·자원·skew·정렬/index 전제가 다름 |
| 7.20.1 구문/overload·frame·제어/예외 | 이름/overload 바인딩; frame/제어 실행; 예외/unwind | 정적 의미와 실행 수명·거래 오류 전파를 개별 검토할 수 있어야 함 |
| 7.20.2 cursor/record/array·dynamic SQL·권한 | 복합 값; cursor 수명; dynamic SQL 바인딩·실행 권한 | 값 표현·자원 해제·권한 전환의 실패 경계가 다름 |
| 7.28.2 exact predicate/연산 | topology 검증·관계 predicate; 거리; buffer/overlay 연산 | contains와 union/buffer를 같은 크기로 간주하기 어려움. 선택 library와 지원표에 따라 실제 분해 |
| 7.33.2 Callable/OUT/multi-result·batch/keys | 호출 결과 수명; batch 원자성/부분 결과; generated keys; 각 driver 연결 | 기능별 공급과 C/JDBC/ODBC의 API·runtime 검증이 다름 |
| 7.33.3 LOB/spatial·XAResource | LOB; spatial 매핑; XAResource; 해당 driver별 인수 | XA TM 연동을 공간 값 구현 완료에 묶을 이유가 없음 |
| 7.34.2 PDO/.NET, 7.34.3 framework·TCP/TLS/auth | PDO와 .NET 각각; framework별 업무; 원격 전송 공급 수신·연결 인수 | 별도 runtime/빌드/metadata 계약과 DEV5/COMM 공급이 섞여 있음 |
| 7.35.2 HDFS/Hive | 실제 지원할 HDFS 파일 경로와 Hive 교환 경로별 adapter·업무 | 파일 입출력 성공이 Hive 타입/원격 업무 완료를 대신하지 않음 |

새 최상위 번호나 함수별 문서를 늘릴 필요는 없다. 기존 36개 아래에서 한 의미·수명·실패 경계를 검토 가능한 단위로 나누고, 관련 구현·fixture·oracle·필수 시험을 함께 배정하는 편이 낫다. 반대로 7.23 sequence, 7.31 RM, 7.32 coordinator처럼 같은 불변식을 공유하는 부분을 무조건 다른 담당에게 흩뜨리지 않는다.

### WBR-02 — 공급과 인수의 연결이 대분류 행에 머물러 있다

**해당 leaf 착수 전 구체화가 필요하다.** 문서는 전체 분류 완료를 장벽으로 삼지 말라고 올바르게 명시하지만, 직접 선행·완료 조건은 대부분 7.xx 행에 붙고 7.xx.y별로 구별되지 않는다. 인수 Txx는 여러 구현을 묶으므로 Txx 하나를 모든 하위 작업의 종료 조건으로 쓰면 대기·중복 인수가 생긴다.

예를 들어 7.32.3의 실제 표준 TM 연결은 7.31 native뿐 아니라 7.33의 실제 XA driver/연결 공급을 필요로 한다. 이 공급을 7.33.3의 LOB/spatial 전체와 구별해야 한다. 7.16.2의 deferred 복원과 7.17.3의 deferred ledger도 marker/복원 인터페이스 공급과 실제 ledger 통합 인수를 구별해야 서로 전체 완료를 기다리지 않는다. 이것은 현재 문서에 확정된 순환이 있다는 판정이 아니라, 좁은 공급 원칙을 실제 하위 작업표로 내리지 않은 문제다.

각 배정 leaf에 `필요한 공급 ID·소비 API/계약·산출물·공급 완료 사례·소비 경로 통합 사례·관련 F 항목·공유 파일 소유자`를 붙이는 것을 권고한다. 7.12/7.22의 기준 DML와 trigger 합류를 구별한 기존 설명은 유지한다. 모든 F 전체를 먼저 동결하는 새 장벽은 만들지 않는다.

### WBR-03 — 저장 자원과 XA 복구의 위험을 더 일찍 해소한다

**우선순위 개선 권고다. 기존 내용의 누락으로 판정하지 않는다.** 현재도 첫 effect 전 예약, 7.31 native 선행, 초기 fault 입력 준비를 명시한다. 다만 선택 순서 표에서는 7.26이 주로 순서3, 7.31이 순서4에 나타나므로 이 두 작업의 설계·실현 가능성 확인을 별도 초기 공급으로 표시하는 편이 안전하다.

7.26.1의 소비 opcode별 산식은 관련 큰 DML·cascade·trigger·queue·LOB 효과 구현 전에 받아야 한다. 64 MiB 변경 목표가 WAL 256 MiB 안에서 된다고 단정할 수 없다. old/new·undo·forward WAL·CLR·retained 자원·재장애 공존량을 실제 opcode로 계산해야 한다. 현재 자료만으로 불가능하다고 단정하지도 않는다.

7.31의 prepare 상태·checkpoint 인계·잠금/validation fence 재구성·예약 resolve 경로는 7.15–17의 engine 계약을 구체화할 때 함께 검토하는 것이 좋다. 실제 구현은 해당 S1/거래 공급 후 진행하고 SC11 상위 인수 선행은 유지한다. coordinator·공간·PSM 전체를 먼저 완성할 필요는 없다. 이 판단은 ARIES의 CLR/부분 rollback·prepared 보호와 Oracle의 prepare 자원 확보, PostgreSQL의 WAL/checkpoint/recovery 연결을 근거로 한 XADB 일정 권고다.

### WBR-04 — 증분 refresh는 이름만 있는 예약 leaf를 넘어야 한다

**7.18.4를 배정하기 전 필요한 보완이다.** full refresh와 incremental을 구분한 것은 맞지만, 현재 7.18의 완료 조건과 T13/C04는 증분 경로의 변경분 공급·지원 질의·진척 보존을 특정하지 않는다. F04에 남겨 둔 상세 동결을 실제 이 작업의 산출물로 연결해야 한다.

최소한 지원 질의 형태·변경분을 얻는 방법·적용 경계·중복/누락 방지·동시 DML/재시작·full refresh와의 독립 대조를 작업 범위에 명시해야 한다. Oracle도 log-based와 PCT/LPCT 증분을 구별한다. PostgreSQL의 concurrent refresh는 새 결과 전체를 만든 뒤 기존 결과와 diff를 구하는 경로이므로, 그 코드만 참고하여 base 변경분 기반 증분 유지까지 해결했다고 볼 수 없다.

D-LOGGING은 그대로 보류한다. 이 보고서는 WAL 추출·구독 설계를 재개하지 않는다. 후속 F04가 선택하는 방식이 보류 기능을 실제로 필요로 한다면 그 의존성과 차단 상태를 기록해야 하며, 단지 “증분”이라는 명칭 때문에 모든 구현을 D-LOGGING에 종속시키지도 않는다.

### WBR-05 — 성능 사례의 크기와 성공/한도 거절을 구별한다

**F10 동결 전 보완 권고다.** P01의 1 GiB table·8 MiB workspace·256 MiB/query temp 조건은 읽기 데이터 크기만으로 충족 여부를 판단할 수 없다. 넓은 행 1 GiB를 모두 보관해야 하는 window/spool이라면 현재 temp 한도에 맞지 않을 수 있다. 반대로 좁은 projection·선택적 입력이면 같은 table에서 성공 가능한 workload가 된다.

workload별 row 폭·cardinality·key/peer skew·projection·중간 결과/파일 공존 peak를 먼저 적고, 성공해야 하는 셀과 한도 초과를 올바르게 거절해야 하는 셀을 구분한다. 현재 P01 전체가 불가능하다는 판정은 아니다. 실제 저장/임시 한도를 묵시적으로 늘리거나 실행 실패 뒤 profile을 축소해서 해결하지 않는다. PostgreSQL의 hash join best-effort 초과도 XADB의 엄격한 quota 근거가 될 수 없다.

### WBR-06 — 공통 값 의미와 물리 codec의 작업 소유자를 구별한다

**7.01.2의 배정 전에 명시할 사항이다.** 해당 행의 소유는 foundation·SQL인데 산출물에는 persistent/index/hash/spill codec이 함께 있다. 설계 정본은 foundation의 공유 값 의미, engine의 page/index/LOB, SQL의 관계 실행을 구별한다. 현재 제품 계층 위반이 구현됐다는 뜻은 아니지만 작업 소유 표만으로는 engine 저장 형식 변경 책임이 보이지 않는다.

공통 canonical value/key 의미의 소유자는 하나로 유지한다. engine의 row/index/WAL 수용, SQL temp envelope/spill 수용, server/client wire 수용에는 각 소비 소유자와 같은 독립 vector의 연결을 붙인다. 별도 serializer가 값을 제각각 정의하게 하거나 모든 물리 형식을 foundation으로 모으는 분해는 피한다.

<a id="allocation"></a>
## 분량과 분류 평가

아래 크기는 일정 추정치가 아니라 작업 분해 필요성이다. 실제 인일은 F별 지원표와 신규/재사용 코드·fault 수·환경 준비·검토 비용이 정해진 뒤 산정한다. 번호 개수나 문서 줄 수를 작업률로 사용하지 않는다.

| 대상 | 평가 | 권장 배정 방식 |
| --- | --- | --- |
| 7.01–05 값·타입·식 | 큰 묶음. 정수 범위와 decimal 산술·Unicode·tzdb는 분량 차이가 큼 | 공통 descriptor 후 family별 세로 연결. crypto/regex는 별도 책임 |
| 7.06–07 typed plan·실행 자원 | 공통 공급이며 후속 다수가 소비 | 필요한 node와 최소 실행 수명부터 공급. 단일 계약 소유자가 통합 |
| 7.08–11 관계 연산 | 큰 묶음. 특히 join 알고리즘·window frame·재귀 상태 | 기준 실행과 해당 최적화/큰 입력 경로를 분리. 7.11의 set/재귀/pivot 분리는 유지 |
| 7.12 DML | 원자성·반환·trigger/cascade 합류가 큰 고위험 작업 | 기준 DML→제약/trigger 합류→큰 거래 공급 연결. 관련 복구 인수를 함께 배정 |
| 7.13–14 최적화·cache | 정확성 보호와 성능 선택이 섞임 | rule별 의미 검증, cache 권한/metadata 수명은 필수. 비용 탐색/generic-custom 성능 확대는 기준 실행 뒤 |
| 7.15–17 격리·savepoint·제약 | 작은 문법 작업으로 볼 수 없는 고위험 engine 확장 | native history/복구 공급과 SQL 노출 분리. deferred 통합 경계를 명시 |
| 7.18–19 catalog·refresh·PART/NG | 여러 lifecycle과 별도 core 공급을 가짐 | view/DDL/full/incremental을 따로 인수. PART/NG를 SQL 팀의 숨은 core 개발로 전가하지 않음 |
| 7.20–22 PSM·package/autonomous·trigger | 가장 큰 영역 중 하나 | WBR-01대로 PSM 추가 분해. package state와 autonomous 별도 하위 작업은 유지 |
| 7.23–24 sequence·queue/scheduler | sequence 분리는 좋음. queue/job/관리통신은 다른 효과 | native sequence/queue 공급을 일반 PSM 전체와 분리. 세 하위 효과별 인수 유지 |
| 7.25 온라인 변경 | capture→switch→회수·복구가 큰 고위험 상태 기계 | durable job 상태의 책임은 일관되게 유지하고 전이별 산출물·fault를 분담 |
| 7.26–28 저장·LOB·공간 | 저장 예약과 기하 알고리즘은 종류가 다른 큰 작업 | 저장 산식 선행, LOB 수명별 연결, 공간 지원표/library 선택 후 연산군 분해 |
| 7.29–30 외부 routine·link | 격리·remote certainty·peer 의미가 핵심 | pure/effectful 및 ReadOnly/IndependentRemote/Atomic2pc의 기존 분리 유지 |
| 7.31–32 RM·coordinator | 복구·진행 보장이 큰 고위험 작업 | native RM 수직 경로 우선, 표준 TM 실제 연결, 확장 ACK 별도. 두 책임은 합치지 않음 |
| 7.33–35 client·언어·교환 | 현재 표에 비해 실제 분량이 특히 큼 | 기능×실제 adapter/runtime 중 지원 셀을 명시. 구현·업무·clean build·실패 인수까지 포함 |
| 7.36 검증·자원·성능 | 말기 단일 작업으로 배정하면 안 됨 | 기존 원칙대로 기능마다 oracle/harness를 동반하고 마지막에는 남은 통합과 증거만 취합 |

7.11·7.21·7.24·7.28은 상위 제목의 응집도가 낮아 보여도 하위 작업을 독립 배정하는 색인으로는 유지 가능하다. 이를 이름을 맞추려고 합치거나 36개 전체를 다시 번호 매길 이득은 작다. 7.35.3은 완료율 분모에서 구현 완료로 계산하지 않고 보류 의무로 별도 표시해야 한다.

<a id="priority"></a>
## 권장 우선순위와 진행 방식

기존 상위 DAG·필수 범위를 유지하는 선택 순서 제안이다. 실제 공급이 없는 작업을 mock으로 인수하거나 현재 중단된 제품 개발을 재개하라는 지시가 아니다.

| 순서 | 먼저 닫을 내용 | 다음 작업에 주는 가치 |
| --- | --- | --- |
| 0 — 관련 leaf 준비 | 필요한 F01/F02/F03/F06/F07/F08/F09 입력, 독립 vector, library/실제 runtime 선택, 7.26.1 산식, 7.31 복구·해소 설계 | 큰 재작업과 외부 환경 막힘을 구현 전에 드러냄. 모든 F의 일괄 완료 요구 아님 |
| 1 — 공통 공급 | 7.01–05의 필요한 family, 7.06/07 최소 경로, 관련 7.15–17 native·7.18 dependency 공급 | 공통 값·실행 수명·rollback 경계를 고정 |
| 2 — 실제 최소 통합 | 기준 join/aggregate/DML·full refresh, driver 한 경로의 해당 type 왕복, 7.26의 필요한 opcode 공급 | SQL→저장/복구→client의 실제 인계 문제를 빨리 발견 |
| 3 — 기능별 확장 | window/재귀/나머지 관계·PSM/trigger/sequence/queue·LOB/공간, 준비된 7.31 native | 필요한 선행을 받은 leaf별 진척. 기준선 이후 해당 최적화를 병행하되 공유 파일 소유 조정 |
| 4 — 고급/외부 통합 | 온라인 변경·증분 refresh·PART/NG 해당 leaf, 실제 XAResource/TM·coordinator·Atomic2pc, 언어/교환 adapter | 준비된 native/API/실제 환경을 결합. 기본 ReadOnly link 등 독립 leaf까지 기다릴 필요 없음 |
| 5 — 남은 인수 | 미완료 성능/자원/실패 사례·증거 연결·독립 검토 | 이미 검증한 무변경 코드의 시험을 반복하지 않고 남은 의무를 확인 |

한 담당에게 큰 영역 몇 개씩 같은 수로 나누는 방식보다, 공유 descriptor/거래/wire의 계약 소유자와 실제 leaf 구현 소유자를 구별하는 편이 낫다. 진행 중인 구현 수를 늘리는 것보다 공급된 leaf를 관련 인수·검토까지 닫는 것을 우선한다. 작업 계약에는 구현량뿐 아니라 oracle/fixture·fault·runtime 설치·검토/통합 부담을 포함한다. 이번에는 실제 담당 배치나 병렬 구현을 수행하지 않았다.

<a id="sources"></a>
## 외부 원문과 로컬 소스 근거

2026-10-22 열람했다. 원문의 기능이 XADB의 필수 범위를 자동으로 늘리지 않으며, 자료 비교를 성능·동등성 실행 증거로 쓰지 않는다.

| 원문·읽은 부분 | 검토에 사용한 내용 |
| --- | --- |
| [ARIES, Mohan 외, TODS 17(1), 1992](https://www.cs.cmu.edu/~15849g/readings/mohan92.pdf), §5.2 p.117–118, §5.3 p.120–121, §5.4 p.121, §6.1 도입/Fig.9 p.122–123 | CLR chain과 반복 undo의 bounded logging, prepare의 영속 잠금 정보, checkpoint/restart 인계. 7.16/26/31의 분리·조기 검토 근거. 논문의 marker/lock 해제 선택을 XADB에 그대로 적용하지 않음 |
| [Oracle 26ai 비교 규칙](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/Data-Type-Comparison-Rules.html), blank-padded/nonpadded | 7.01/03의 비교 문맥을 공통으로 두되 join/index/optimizer에서 소비 검증해야 함 |
| [Oracle 26ai 호출 해석](https://docs.oracle.com/en/database/oracle/oracle-database/26/lnpls/subprogram-invocation-resolution.html) | scope·signature 해석이 PSM 실행 frame·예외 수명과 별도 작업임을 확인 |
| [Oracle 26ai 분산 거래](https://docs.oracle.com/en/database/oracle/oracle-database/26/admin/distributed-transactions-concepts.html), Prepared Response·Steps in Prepare Phase | prepare 응답 전에 복구 가능한 변경·자원·장애 후 보호가 필요. XADB RM native 선행을 지지하며 Oracle commit-point/SCN 구조는 도입하지 않음 |
| [Java SE 21 XAResource](https://docs.oracle.com/en/java/javase/21/docs/api/java.transaction.xa/javax/transaction/xa/XAResource.html), RM/TM 설명·recover·forget | 실제 driver/TM 경계를 독립 배정. 정상 완료와 heuristic/forget을 구분한 기존 DESIGN-002 유지 |
| [Oracle 26ai materialized view refresh](https://docs.oracle.com/en/database/oracle/oracle-database/26/dwhsg/refreshing-materialized-views.html), §7.1 | complete와 log/PCT/LPCT incremental의 공급 차이. 7.18.4의 명시적 공급/인수 필요 |

ARIES PDF는 웹 도구에서 timeout이 반복되어 같은 CMU 원문을 HTTP로 받아 텍스트를 추출해 위 절을 읽었다. 원문 PDF SHA-256은 `6131bcee36c21e669d41204ef2b2320cb458b5d3bd9b7cf3e7758afe8ed3495f`다. 논문 전체를 정독했다거나 새로운 복구 모델을 실행했다고 주장하지 않는다.

아래는 [로컬 catalog](../reference/opensource-references.md)를 출발점으로 origin·HEAD·작업 트리를 이번에 다시 확인한 실제 revision이다. 모두 공식 catalog origin과 일치하고 clean이었다. 경로는 `/home/et16/work/<이름>/` 기준이다. fetch·checkout·수정·빌드·시험은 하지 않았다.

| checkout·revision | 실제 열람 위치 | 판단에 사용한 부분 |
| --- | --- | --- |
| postgres `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/backend/access/transam/twophase.c` 1–135, 1145–1235, 2090–2180; `src/backend/utils/cache/plancache.c` 1–75; `src/backend/executor/nodeHashjoin.c` 1–100; `src/backend/commands/matview.c` 142–164, 310–364, 560–593 | prepared WAL flush·checkpoint 경합·복구 callback, cache invalidation 별도 책임, hash batch의 best-effort 한계, 전체 재생성 후 concurrent diff |
| mariadb `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `sql/sp_head.cc` 290–365, 1605–1705 | 거래 제어 구문 분류와 routine 실행 권한 전환/복원은 다른 책임 |
| cubrid `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/query/serial.c` 1170–1270 | serial redo→page 변경·system operation·발급/예약 범위 구분. sequence의 독립 책임을 지지하며 이 발췌로 전체 내구성을 인증하지 않음 |
| mysql-server `3b99be40f8a31d9396cad10accfd3ba51647d613` | `sql/xa/sql_xa_commit.cc` 80–165 | attached/detached·one-phase/prepared와 metadata 잠금 재시도. 후속 unilateral rollback 분기는 XADB에 채택하지 않음 |
| sqlite `43a48f77585a88940c727f33682151418f544059` | `src/fkey.c` 1–115 | statement/commit의 immediate/deferred 판정 구분. identity를 잃는 단순 counter는 복사하지 않음 |
| clickhouse `1d76716b7580f78d0375c626736e49a3e13f099c` | `src/Interpreters/GraceHashJoin.cpp` 295–355 | operator의 temp child scope·계측·spill threshold. 공통 자원과 join 알고리즘 분리 근거 |
| tidb `328460a9af032ec6b53823724f87837a6d2d5857` | `pkg/planner/core/plan_cache.go` 140–188, 400–445 | schema 재처리와 prepared 권한/lock 검사를 cache 성능과 구분 |
| foundationdb `487620d5aa41152b1f034f4e932005a87eb2947f` | `fdbclient/NativeAPI.cpp` 4700–4785 | 전달 가능/commit 불명·idempotency 확인 경계. 외부 effect를 단순 재시도 작업으로 취급하지 않을 근거 |

`/home/et16/work/duckdb`와 `/home/et16/work/rocksdb`는 이번에도 존재하지 않았다. 두 제품을 이번에 읽은 소스 수에 넣지 않는다. Oracle은 위 공식 문서를 읽었으며 Oracle 제품 소스를 열람하거나 서버를 실행하지 않았다.

## 수행 범위와 후속 적용

수행한 검증은 요구/작업 ID의 읽기 전용 집계, 문서 의미·의존성·분량의 정적 대조, 공개 원문과 로컬 소스 조사다. 제품 시험·문서 검증 gate·fmt/Clippy·성능 측정·push는 실행하지 않는다. 계획·인수의 후속 편집과 사용자 요청 commit은 아래 적용 범위를 따른다.

실제 의미/API 변경은 후속 leaf에서 DP01의 D2 검토를 따른다. 초기115개를 임의로 완료 처리하거나 모든 항목의 상세 문서를 한 번에 만들지 않는다. 시험 실행은 계속 변경 코드에 해당하는 필수 사례로 제한하며 DEV3 S5 예외를 DEV7로 확대하지 않는다.

<a id="applied"></a>
## D7-WBS-002 적용과 독립 검토

| 결정 필드 | 내용 |
| --- | --- |
| ID·제목·일자·상태 | D7-WBS-002 · DEV7 하위 배정·공급·인수 보완 · 2026-10-22 · ACCEPTED |
| 분류·담당 | D2 · 작업/계약/통합 `/root`, 별도 검토 `/root/dev7_wbs_review` |
| 위임 | 사용자의 개선 즉시 반영·완료 후 한 번 commit 요청, DP01의 기존 위임 |
| 적용 | 기존 36개 분류·64개 요구 유지,115→135개 하위 작업·공급/통합 인수 구체화. 새 제품 의미·byte/API·상위 DAG·수치 한도 채택 없음 |
| 대안 | 기존115개 그대로 배정; 상위36개 전체 재편; 큰 하위 책임만 추가 분해 중 마지막을 선택 |
| 선택 이유 | 서로 다른 실패/자원/runtime 검증을 분리하면서 기존 인수·요구 추적을 유지. 개수 균등화·파일 수 확대는 목표가 아님 |
| 소유 경로 | `plans/dev7-work-breakdown.md`, `plans/dev7-implementation-plan.md`, 이 증거 문서 |
| 공유 편집 | `README.md`, `plans/README.md`, `evidence/README.md`, `documents.json`의 DEV7 설명/등록만 `/root`가 반영 |
| 중복·통합 순서 | 조사 중 DEV3/DEV4 계획·공유 색인의 동시 변경을 관측해 보존. DEV7 계획→인수→조사/색인→읽기 전용 독립 검토→이번 변경만 commit 순서 |
| 대체 관계 | 현재 분류표의 초기115개 배정만 보완. 하위 ID 이동은 계획의 대응표, 당시 조사는 기존 증거 그대로 유지. DESIGN-001/002를 대체하지 않음 |
| 검증·한계 | 자료/계획 정적 검토와 diff 확인. 코드·테스트·문서 gate 실행 및 제품 PASS 없음 |
| 재검토 조건 | 공급 순환·불필요한 전체 완료 장벽, 실제 한도 불충족, 지원 runtime/기하 library 변경, 증분 방식이 보류 공급을 요구함, 후속 F 동결의 의미 변경 |

| 발견 | 반영 위치·결과 |
| --- | --- |
| WBR-01 큰 하위 작업 | regex/hash/crypto, join 알고리즘, PSM, geometry 연산, client/언어/교환을 나누어 하위20개 추가. 기존 상위 기능 범위 유지 |
| WBR-02 하위 인계 | [하위 공급표](../plans/dev7-work-breakdown.md#leaf-handoffs)에 native/소비 인수 구분. XAResource→실제 TM과 savepoint→deferred 합류를 명시. 배정별 variant/관련 사례·공유 소유자 기록 요구 |
| WBR-03 위험 우선 | [순서0–3](../plans/dev7-work-breakdown.md#supply-map)에 자원 산식·XA 복구/해소 계약 준비를 앞당김. 실제 하위 공급과 상위 인수 선행 유지 |
| WBR-04 증분 refresh | 7.18.4 지원/변경분 계약·실제 공급 구현과7.18.5 적용/인수로 분리. F04·T13·C04에 적용 cut·중복/누락·동시 변경/재시작 의무 연결. D-LOGGING 보류 유지 |
| WBR-05 workload 한도 | F10·P01 설명에 row 폭/cardinality/projection/skew·공존 peak와 정상/한도 거절 셀 구분. 기존 성공/성능 의무의 대체 금지 |
| WBR-06 codec 소유 | 7.01.1/2의 공유 의미,7.01.3 engine 저장,7.01.4 SQL temp,7.01.5 wire 매핑을 구별. consumer가 같은 독립 vector로 합류 |

독립 검토자 `/root/dev7_wbs_review`가 계획2개 전체·실제 diff·owner 계약·이 보고서 적용 기록과 ARIES §5.2–5.4, Oracle refresh/prepare, 로컬 PostgreSQL·MariaDB·SQLite 관련 원문을 직접 대조했다. 최초 P2 지적은 7.18.4가 계약만 작성하고 .5는 실제 공급을 기다리는 구현 소유 공백이었다. .4에 선택한 변경분 공급 구현·독립 공급 인수를 배정하고 계획 F04/대응표를 함께 수정했다. 재검토 결과는 **정적 계획 검토 PASS, 남은 P1/P2 없음**이다. 검토자는 파일 수정·빌드·시험·문서 gate·commit을 하지 않았다.

| 검토 snapshot | SHA-256 |
| --- | --- |
| `plans/dev7-work-breakdown.md` | `dacc04df135631ea9edca8dd4033d0ba4ba4c427558e5bba66ef377fa045b1c4` |
| `plans/dev7-implementation-plan.md` | `5a9367e12823e50e05811a454f9f93a26bed07ae01879ae7c10e9eac87a2bfab` |

통합 담당 `/root`가 위 지문 일치를 확인한 뒤 채택/검토 상태와 이 결과 기록만 갱신했다. 사용자 요청 commit에는 이번 DEV7 계획·보고서·공유 색인의 해당 변경만 포함하고 동시 DEV3/4/5/6 변경은 보존한다. F01–F10 상세 동결·SC07–SC11·SQL_EXT1은 계속 미완료이며 제품 구현·시험은 재개하지 않는다.
