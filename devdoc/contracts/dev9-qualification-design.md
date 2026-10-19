# DEV9 — 제품 기능·운영·성능 인수 설계

2026-10-19 · **DEV9-DESIGN-001 / D2 ACCEPTED / 독립 수정본 검토 PASS / 구현·SC16·SC17·PRODUCT_QUALIFIED NOT_RUN**

[구현·검증 계획](../plans/dev9-implementation-plan.md) · [조사·독립 검토](../evidence/dev9-source-research.md) · [기능 목록](../plans/capability-inventory.json) · [단계 완료 계약](../plans/stage-completion.json)

<a id="scope"></a>
## 1. 범위와 기존 계약

DEV9는 Linux/Rust 디스크 관계형 DBMS인 XADB의 선언한 기능·도구·지원 환경·운영·성능을 하나의 제품 후보에서 인수한다. SC16/XADB-046은 기준선과 세부 요구 목록을 동결하고, SC17/XADB-047은 그 목록의 실제 실행 증거와 남은 결함을 판정한다. 설계·fixture·독립 모델 준비는 선행 단계 완료 전 가능하다. 실제 제품 인수에는 CLIENT1·SQL_EXT1·OPS1·P1과 그 하위 공급 증거가 필요하다.

[제품 방향](../plans/project-direction.md), [하위 계층 개발](../plans/bottom-up-development.md), [DEV3 저장](disk-storage-design.md), [DEV6 client](dev6-client-design.md), [DEV7 확장](dev7-sql-extensions-design.md), [DEV8 운영](dev8-operations-design.md)의 의미를 재정의하지 않는다. Oracle 문서·ARIES·공개 DBMS·추가 로컬 자료는 조사 입력이며 XADB의 정답·코드·SQL 호환성·성능 보증이 아니다. 저장 엔진은 DISK 하나이며 buffer/cache/workarea와 K0 개발 substrate는 유지한다.

현재 104개 capability는 세부 leaf 수가 아니다. 기존 required 요구와 DEV7 R10 등의 보류를 유지하며, 구현이 어렵거나 시험이 실패했다는 이유로 제외하지 않는다. global index의 물리 구현 선택과 R06-04의 table 전체 uniqueness·partition key 없는 조회 능력을 구분한다. native global index를 선언하면 GLOBAL_INDEX_QUALIFICATION이 필수이고, local fan-out 대응은 동일 업무·오류·동시성·비용 기준을 별도로 만족해야 한다.

PRODUCT_QUALIFIED, 공식 사내 배포, EXTERNAL_RELEASE는 서로 다른 판정이다. 외부 출시·고객 SLA·제품 방향 변경은 기존 DP01 D3 권한을 따른다. 이번 설계는 기준선의 내용·수치·실행을 동결하지 않으며 `baseline_frozen=false`, `inventory_complete=false`, 제품 실행 `NOT_RUN`을 유지한다.

<a id="ownership"></a>
## 2. 책임과 공급

| 소유 | DEV9에 공급할 계약·증거 | DEV9의 책임 |
| --- | --- | --- |
| DEV0/1, XADBTest | process 소유·receipt·negative checker·Linux effect·quota | 실제 runner/oracle identity, self-test, 환경/실패 분류 확인 |
| DEV2/3, PART/NG | 거래·WAL/undo·page/index·공간·partition·복구 | S1/P1 및 해당 global gate, 실제 DISK 경로와 복구 조합 수신 |
| DEV4/5, COMM | SQL 의미·session·권한·wire·취소·외부 ACK | Q1/B3/C1과 public 경로, 부분 결과·응답 유실·재접속의 통합 판정 |
| DEV6 | native/C/JDBC/ODBC별 API·수명·오류·metadata | CLIENT1 및 지원 runtime/ABI별 동일 업무·회수 증거 |
| DEV7 | 타입·함수·PSM·LOB/spatial·외부 거래 | SQL_EXT1, 모든 기능별 공급과 CALL/XA·비교 문맥의 교차 시험 |
| DEV8 | H1/L1/M1·backup/PITR·보안·도구·성능 | OPS1, topology/provider별 실행과 운영중 workload·RPO/RTO |
| DEV9 | 제품 기준선·조합·설치·문서·증거 판정 | SC16/17, R16-01–04, 전체 누락 검사와 독립 최종 검토 |

하위 계약의 실패는 소유 단계로 반환한다. DEV9에서 커널 내부를 직접 수정하거나 다른 단계의 미실행을 문서 참조로 충족하지 않는다. 하위 성능·복구 의무는 상위 smoke로 대체하지 않는다. XADBTest controller/reference와 실제 worker·CLI·server·driver를 조립하며 engine에 qualification 의존성을 추가하지 않는다. 새 crate·빈 경로는 만들지 않고 [저장소 정책](../policies/repository-build-plan.md)의 실제 구현 시 배치를 따른다.

<a id="identity"></a>
## 3. 기준선·후보·campaign 정체성

| 개념 | 고정하는 내용 | 변경 시 처리 |
| --- | --- | --- |
| BaselineId | required leaf/variant, XADB 계약 revision, 지원 환경, workload, 허용 결과·수치, 제외 근거, 문서 범위의 digest | 새 baseline. 이전 실패와 변경 이유 보존, D2/D3 분류 재검토 |
| CandidateId | clean source/export digest, toolchain·lockfile·외부 의존/provider, target/feature/build recipe, 모든 배포·client·시험 artifact digest를 묶은 manifest | 새 candidate. 최종 SC17은 이 identity의 필수 셀 전부 요구 |
| ProfileId | 실제 장비·OS/kernel/FS/장치·runtime, 자원·fault·topology·계측·시간/seed·반복·통계 규칙 | 새로운 조건을 기존 PASS에 끼워 넣지 않음 |
| CampaignId / AttemptId | baseline/candidate/profile, case/variant, 입력·schedule·oracle, 실행 전후 identity와 원시 자료 | 재시도는 새 attempt. 실패를 덮어쓰지 않음 |

같은 candidate는 모든 플랫폼에서 같은 binary를 뜻하지 않는다. 대상별 server/client/native library/JAR와 debug/fault worker의 역할·hash를 하나의 manifest에 실행 전에 등록한다. 고의 결함·fault 전용 artifact의 결과로 배포 artifact의 정상·성능 시험을 대신하지 않는다. 배포 파일을 재포장하거나 provider를 바꾸어 bytes가 달라지면 identity도 바꾼다. 이후 추가한 빌드가 이전 manifest에 자동 포함되지 않는다.

SC16은 candidate 실행 전 baseline을 동결한다. 개발 탐색은 가능하지만 그 결과를 본 뒤 통과 기준을 유리하게 조정한 동일 campaign은 인수에 쓰지 않는다. 선행 gate의 과거 PASS는 개발 참고이며, 최종 candidate의 source/build와 맞는 필수 하위 회귀·통합 증거를 다시 갖춘다. 변경 없는 모듈의 과거 결과를 재사용하려는 최적화는 별도 증거 재사용 계약 전까지 허용하지 않는다.

<a id="inventory"></a>
## 4. 요구 분해와 누락 방지

추적 관계는 `Rxx-yy → semantic leaf → applicable variant → case → oracle/criterion → attempt → receipt → 판정`이다. 한 case가 여러 leaf를 입증할 때는 leaf별 assertion과 관측이 있어야 한다. 많은 case 수나 높은 평균 성공률로 누락을 덮지 않는다.

각 semantic leaf는 안정 ID·상위 요구 ID·소유 단계/계약 절·사용 사례와 입력 domain·공개 인터페이스·정상/오류/거래 효과·한도·자원·보안·복구 의무·적용 조건을 가진다. 함수 overload, cast 방향, collation, API 호출 상태, 길이 단위, NULL, 오류 효과가 다르면 명시적으로 분리한다. 순서 미정 결과는 bag으로, 순서 계약이 있는 결과는 순서와 동률 조건까지 비교한다. API 이름만 같은 항목을 한 leaf로 합치지 않는다.

각 variant의 적용 분류는 다음과 같다.

| 분류 | 의미 | 인수 효과 |
| --- | --- | --- |
| REQUIRED | 선언한 기능과 지원 조합 | 해당 cell의 VERIFIED_PASS 필수 |
| NOT_APPLICABLE | 계약·지원 대상 조건상 성립하지 않는 조합, 근거·조건식·소유 검토 포함 | PASS로 세지 않음. unknown 조건식은 SC16 차단 |
| EXCLUDED_BY_DIRECTION | 기존 사용자 제외 또는 새 D3 결정으로 제외 | 결정 ID 필수. required를 담당자가 임의 전환하지 못함 |
| UNRESOLVED | 세부 의미·소유·지원 조합 미정 | SC16 차단 |

지원하지 않는 조합의 명시적 거절도 별도 negative case다. 그 성공으로 required positive capability를 완료하지 않는다. `DEFERRED`, `UNSUPPORTED`, `NOT_RUN`은 필수 요구의 면제 사유가 아니다. R16-03의 개발 빌드 교차 버전 비적용은 [버전 정책](../policies/version-compatibility-policy.json)의 정확한 applicability를 사용한다.

목록 완결성은 104개 부모의 담당 배정만으로 성립하지 않는다. 공개 grammar·타입/함수/overload 목록·driver API/metadata·설정/property·오류/limit·운영 명령·매뉴얼 지원표의 양방향 대조를 요구한다. 구현에만 있는 공개 항목도 누락으로 보고한다. 입력 domain 분할과 대표값·경계값의 포괄 근거를 기록하고 신규 공개 항목·계약 변경 시 baseline을 무효화한다. 테스트가 발견한 목록만으로 정본을 생성하지 않는다.

<a id="matrix"></a>
## 5. 제품 조합 행렬

축은 SQL/객체/타입·client/API/runtime·거래/격리·DISK/partition/index·권한·topology/protection·운영 사건·fault·환경/자원/계측이다. 각 축의 실제 값과 합법/거절 조합을 F02에서 고정한다. 모든 가능한 숫자·문자열을 나열한다는 뜻이 아니라 계약으로 분할한 유한한 필수 셀 전체를 실행한다.

최소 필수 교차 경계는 다음과 같다. 실제 세부 셀은 기능 공급에 따라 확대하며 아래 이름을 단일 시험으로 세지 않는다.

| 묶음 | 함께 입증할 경계 |
| --- | --- |
| M01 SQL↔driver | NULL/빈 text·수치·날짜·collation·LOB·metadata·부분 fetch·batch·CALL·오류가 지원 경로마다 보존 |
| M02 transaction↔storage | statement 실패/savepoint·동시 제약·DDL·partition 이동·index 변경과 crash/recovery, row/index/catalog 일치 |
| M03 commit↔connection | 성공 ACK·commit 시도 전 거절·결과 불명·취소·연결 유실·재접속·close, 자동 중복 실행 없음 |
| M04 recovery↔operations | backup/archive/PITR·undo/CLR·checkpoint/회수·복구 중 재장애·복원 후 새 commit과 재시작 |
| M05 replication↔external obligation | H1/L1/M1·prepared XA/외부 결정·writer fencing·보호 모드·failback·오래된 peer·schema/sequence |
| M06 security↔lifecycle | 역할 철회·prepared/cursor·TLS/key 교체·backup/진단/export·실행중 운영 job·재시작 |
| M07 resources↔load | 큰 데이터·bounded buffer/spill·긴 거래·복제 지연·retention·OOM/ENOSPC·취소·drain·성능 |
| M08 installation↔documentation | 설치 package·소유 경로·다중 instance·설정·매뉴얼 예제·runbook·현재 build 복구 |

고위험 조합은 pairwise로 줄이지 않는다. 예를 들어 `XA PREPARED × strict H1 × backup/PITR × crash`, `partition 이동 × unique × 복제 × 장기 snapshot`, `취소 × 부분 fetch × 권한 철회 × pool 재사용`은 명시적 다중 축 셀이다. 나머지 조합의 축약은 동등성 근거·누락 영향·소유자의 독립 검토로 사전 고정한다. 채택된 required 셀의 실행을 확률 표본으로 대체하지 않는다. 불가능한 조합은 조건식과 negative 거절 사례로 설명한다.

<a id="receipts"></a>
## 6. 실행·receipt·판정 모델

다음은 구현 전 F03에서 literal schema·한도·직렬화·중복 규칙을 동결할 필수 필드군이다. 현재 실행 가능한 새 포맷을 선언하는 것은 아니다.

| 필드군 | 필요한 정보 |
| --- | --- |
| identity | schema version, baseline/candidate/profile/campaign/attempt ID, leaf/variant/case ID, 실제 producer artifact hash |
| input | fixture/seed/schedule/oracle hash, 선행 gate receipt, expected outcome·성능 criterion ID |
| observation | 명령·exit/signal·시작/종료·fault 도달·외부 ACK/거래 이력·typed result/error·효과·자원/cleanup·원시 측정 참조 |
| integrity | artifact의 소유 상대 경로·크기·digest, 실행 전후 source/build 지문, 완결 event 수/종료 marker, 누락/overflow |
| verdict | validity, correctness, performance/guardrail, applicability, 독립 판정 이유와 검토 참조 |

판정 상태는 구별한다. `VERIFIED_PASS`는 올바른 identity·유효한 실행·독립 기대값/모든 기준 충족·필수 증거 완결을 뜻한다. `FAIL`은 계약 위반, `UNKNOWN`은 정답/효과 판정 불충분, `NOT_RUN`은 미실행, `INVALID`는 환경·입력·실행 조건 위반이다. `NOT_APPLICABLE`은 사전 적용성 판정이며 시험 성공이 아니다. 실제 요청 outcome인 UNKNOWN은 기대된 허용 이력으로 독립 판정할 수 있지만, 시험 판정 UNKNOWN은 통과가 아니다.

빈 목록·중복 ID·미등록 variant·없는 oracle·누락된 fault witness·hash 불일치·중간 종료·stderr만 있고 receipt 없음·지원하지 않는 필수 schema는 성공으로 읽지 않는다. 예상 오류 시험은 정확한 오류와 이후 상태·자원·거래 효과를 모두 입증해야 통과한다. exit 0만으로 성공하지 않는다. receipt 해시 검사는 내용 정합성일 뿐 신뢰된 생산자·보호 CI 권한을 증명하지 않는다. 로컬 검증과 보호 환경의 증거 등급을 유지한다.

실행은 `PREPARED → RUNNING → COLLECTING → SEALED → EVALUATED` 순서다. 취소/timeout/수집 실패는 incomplete attempt로 봉인하고 worker·자식·FD·임시 파일 정리 결과를 남긴다. 동일 attempt를 재개하여 빠진 로그를 합성하지 않는다. 검증 가능한 immutable chunk의 전송 재시도는 허용하되 content/sequence 중복·hole을 확인한다. 필수 artifact가 빠지면 셀을 통과시키지 않는다.

retry는 실패 원인을 지우지 않는다. 동결된 인프라 무효화 규칙과 별도 검토 없이 FAIL을 INVALID로 바꾸지 못한다. 한 번이라도 동일 candidate의 유효한 제품 실패가 있으면 그 failure가 해결되기 전 SC17을 닫지 않는다. 제품뿐 아니라 worker·runner·oracle를 포함한 source/artifact 변경은 새 CandidateId와 전체 필수 SC17 실행을 요구한다. 기대값·적용성·기준·지원 profile의 baseline 내용 변경도 새 BaselineId의 SC16과 전체 필수 SC17을 요구하며 이전 PASS를 새 identity에 다시 붙이지 않는다.

영향 셀만 재실행할 수 있는 경우는 candidate/baseline/oracle bytes와 동결한 ProfileId를 바꾸지 않고 일시적 인프라 장애를 같은 조건으로 복원한 경우뿐이다. 사전 무효화 규칙·독립 영향 검토로 실제 오염된 셀과 공유 fixture/환경의 의존성 closure를 확정하고 모두 새 attempt로 실행한다. 이때 유지하는 결과도 동일한 기존 identity와 무오염 근거를 가져야 한다. oracle 코드·기대값 오류 수정을 인프라 복원으로 분류하지 않는다. 알려진 flaky case를 통과할 때까지 반복해 마지막 PASS만 채택하지 않는다.

<a id="runner"></a>
## 7. 실행기와 증거의 경계

controller가 case 선택·소유 process/endpoint·fault schedule·timeout·artifact 수집을 맡고, worker는 실제 public API 또는 명시된 native 공급 port만 호출한다. reference/validator는 production 비교·codec·복구 알고리즘을 정답 계산에 공유하지 않는다. 공통 단순 event schema와 독립 의미 계산은 구별한다. worker가 baseline·허용치·expected를 자기 출력으로 대체하지 못하게 입력 manifest를 실행 전 고정한다.

소유 run 디렉터리와 process identity로 kill/reap/cleanup 대상을 제한한다. 외부 service·기존 DB·사용자 설치 경로는 test default로 사용하지 않는다. import/restore/PITR clone은 외부 routine·DB link·TM/RM·복제 peer를 기본 격리하고 재생이 외부 의무를 중복 수행하지 않도록 owner 계약을 적용한다. 설정·SQL/bind·dump·인증 정보의 redaction과 접근 제어를 갖추며 비밀값을 보고서 identity로 사용하지 않는다.

case fan-out, 동시 worker, event queue, history, artifact bytes, 비교 작업 메모리·CPU/time을 F07에서 제한한다. 원시 이력은 bounded chunk로 소유 저장소에 streaming하고 oracle의 외부 정렬/partition 검증도 예산을 명시한다. quota/ENOSPC/event loss는 시험을 UNKNOWN/INVALID 또는 FAIL로 판정하며 자료를 조용히 버리고 PASS하지 않는다. cleanup 중 실패도 receipt에 남기고 다음 case의 fixture를 오염시키지 않는다.

검증기는 known-bad corpus로 먼저 시험한다. ACK 유실·부분 commit·중복 apply·잘못된 NULL/collation·오류 효과·누락 셀·stale binary·거짓 p99·receipt 삭제를 거절해야 한다. 외부 DBMS의 출력 일치만으로 독립 oracle 의무를 충족하지 않는다. 서로 다른 dialect의 차이는 XADB 명세에 따라 설명하고 원시 관측을 보존한다.

<a id="recovery"></a>
## 8. 복구와 운영 인수

ARIES의 analysis/redo/undo, 이력 재실행, CLR, 재장애, fuzzy image copy의 복구 시작점은 조사 근거다. 실제 XADB의 in-place page·WAL-before-page·독립 undo-store·doublewrite·recovery reservation·REBASE는 [DEV3](disk-storage-design.md#recovery)가 소유한다. DEV9는 다른 DBMS의 page/LSN 구현을 이식하지 않고 그 불변식이 제품 public 경로와 운영에서도 유지되는지 확인한다.

필수 crash cut은 commit append/sync/publish/reply, dirty page/undo flush, checkpoint selector, retention 이전/삭제, CLR와 undo 진행, bootstrap 예약 복원, backup/archive manifest, apply+진행점, fencing/승격, prepared vote/decision 앞뒤다. 제품 owner가 가진 모든 필수 cut을 수신하며 DEV9 목록만으로 축소하지 않는다. process kill, 주입 I/O 실패, 모델링한 전원 장애, 실제 장치 시험의 범위를 별도 표시한다. kill 시험으로 전원 장애를 입증하지 않는다.

외부에 관측한 성공 ACK는 해당 보호 모드에서 남아야 한다. abort/실패는 계약대로 효과가 없거나 명시된 이전 효과만 유지해야 하며 UNKNOWN은 합법 이력 집합으로 검사한다. prepared 거래·coordinator 결정은 일반 loser로 rollback하거나 새 writer에서 잊지 않는다. row/index/catalog/partition·sequence·LOB·권한·replication 진행점은 같은 owner 계약에 맞게 확인한다.

backup 파일/checksum 검사는 필요한 선행 단계다. 인수에는 별도 빈 소유 경로로 실제 restore/PITR → 논리·구조 무결성 대조 → 새 거래 commit → 재시작 → 재대조가 필요하다. current source 파일을 몰래 참조하는 restore를 금지하고 독립 closure만으로 복원한다. 운영 clone의 identity/외부 obligation 격리, online backup consistency end와 목표 cut, WAL/undo/control/key 누락, 오래된 snapshot/consumer가 막는 회수도 포함한다. backup 존재와 서버 READY 로그만으로 복구 완료를 판정하지 않는다.

RPO는 선언한 장애/보호 모드의 외부 ACK cut과 복구된 cut·의무 차이로, RTO는 동결한 사건 시작부터 독립 업무 probe가 정상 완료하는 시점까지 측정한다. detection·fencing·replay·undo·검증·reconnect 시간을 나누되 전체 시간을 빠뜨리지 않는다. 단조 시간과 서로 다른 host의 시계 오차를 명시한다. async와 M1은 원점별 cut·conflict 상태를 보존하고 UNKNOWN을 손실 0으로 바꾸지 않는다.

<a id="performance"></a>
## 9. 성능·용량·장시간 인수

[성능 계약](ai-performance-engineering.md)과 [DEV8 P01–P07](../plans/dev8-implementation-plan.md#performance)의 수치·기간·guardrail을 수신한다. DEV9는 새 수치로 낮추지 않는다. 실제 workload별 TPS/latency/RSS/I/O/RPO/RTO 목표와 장비·지원 설정을 F06에서 실행 전에 고정하며 미측정 목표를 고객 SLA로 약속하지 않는다.

단일 기능·혼합 SQL·native/JDBC/ODBC·운영중 backup/replication·복구·장시간·capacity를 구분한다. DEV3의 최소 buffer 4배와 DEV8의 process 사용 가능 RAM 1.5배 이상의 별도 dataset 의무를 보존한다. disk bytes 크기만으로 실제 접근 working set을 입증하지 않고 I/O·cache·spill witness를 포함한다. 구조적 limit의 직전/정확한 경계/초과와 회수 뒤 재사용을 검증한다.

fixed arrival latency와 포화 throughput, warm/cold, 계측 Off/Always/Detailed를 분리한다. offered/admitted/completed/commit/error/timeout/retry와 queue/backlog를 보존한다. timeout을 빼거나 sync·격리·보호를 낮춘 비교는 거절한다. workload별 correctness와 각 기준을 모두 통과해야 하며 평균으로 실패를 상쇄하지 않는다. 신규 capability의 목표 달성과 기존 baseline 대비 개선은 별개다. 탐색·최종 확인, 독립 run 단위 통계와 다중 비교·중단 정책을 유지한다.

<a id="distribution"></a>
## 10. 설치·재현·문서와 버전

R16-01/02는 Linux 대상 package·CPU/ABI·runtime·의존 library·provider·filesystem을 F08에서 명시한다. clean export에서 고정 toolchain/lockfile/recipe로 build하고 binary·symbol·dependency/license/notice 목록과 package 파일 소유 manifest를 연결한다. 두 번 build하여 동일 artifact를 주장하려면 byte digest를 실제 대조한다. 시간·경로 metadata 등 비결정적 부분은 기록하고 재현 recipe만 확인한 결과를 bit-identical build로 부르지 않는다. 프로젝트 license 선택 미결정은 외부 배포 blocker로 유지한다.

설치는 일반 사용자 소유 prefix와 DB/config/log 분리를 기본으로 한다. preflight는 권한·공간·경로 충돌·runtime·port/instance·필수 OS effect를 검사한다. 기존 DB를 자동 삭제·초기화하지 않으며 symlink/경로 탈출·다른 instance의 lock을 거절한다. package 설치 중 실패는 소유 manifest에 한정해 정리하고 시작 실패/부분 설정을 정상 설치로 표시하지 않는다. 제거는 기본적으로 설치 소유 binary만 대상으로 하고 사용자 data/backup/config 보존을 검증한다. system service나 시스템 설정 변경은 명시된 설치 모드에서만 수행한다. 다른 DBMS의 전역 kernel tuning 수치를 복사하지 않는다.

R16-04는 SQL·타입·함수/API·오류·지원표·한도·관리·backup/restore·보안·성능·이관 예제·release note를 요구한다. 매뉴얼의 실행 예제와 runbook은 candidate의 일반 사용자 경로에서 시험한다. 기능·설정·오류 ID에 대한 양방향 추적, 실제 지원하지 않는 문법/도구의 미지원 표시, 실패 복구·UNKNOWN 재시도 금지·데이터 보존을 포함한다. 작성 목록을 매뉴얼 완성으로 보지 않는다.

R16-03은 기존 버전 정책의 적용성만 연결한다. 사내 개발 빌드는 fresh DB/fixture이고 이전 사내 버전 호환은 `NOT_REQUIRED_INTERNAL_FRESH_BUILD`다. 최초 안정 릴리스에 이전 고객 기준선이 없으면 `NOT_APPLICABLE_NO_RELEASE_BASELINE`이며 PASS가 아니다. 안정 릴리스가 존재하면 같은 major의 모든 이전 안정 기준선에서 old client/data/catalog/WAL/backup→new, 변환 중 crash와 새 쓰기의 재복구·미지원 old reader의 무변경 거절을 요구한다. 역방향 호환·downgrade·rolling upgrade를 자동 보장하지 않는다. converter의 실제 구현은 해당 소유 계약에서 별도 구체화한다.

<a id="gates"></a>
## 11. SC16·SC17과 승격

SC16의 통과 조건은 `BaselineFrozen ∧ InventoryComplete ∧ RequiredLeafCount>0 ∧ Unresolved=0`이다. 104개 부모의 배정, public surface와 leaf의 양방향 완결성, 모든 적용성·수치·독립 oracle·case 연결, 독립 검토가 필요하다. SC16 완료가 기능 구현을 뜻하지 않는다.

SC17/PRODUCT_QUALIFIED는 SC16과 같은 baseline에서 다음을 모두 요구한다.

1. 동일 candidate manifest에 결합된 CLIENT1·SQL_EXT1·OPS1·P1, 하위 회귀와 필요한 global/기능별 gate가 완결됨.
2. 모든 REQUIRED 셀의 유효한 VERIFIED_PASS, 현재 build 복구·운영·성능·설치·문서 증거가 완결됨.
3. 미해결 유효 실패·미실행·UNKNOWN·필수 artifact 누락·required 보류·제품 인수 blocker가 0임.
4. oracle/runner/aggregator의 부정 self-test와 실제 독립 검토를 통과함. 담당자의 자기 검토를 독립 검토로 기록하지 않음.

집계식은 비어 있는 집합의 논리적 참을 사용하지 않고 leaf/cell 수와 부모별 분모를 확인한다. 외부 출시 판정은 여기에 VERSION_RELEASE 적용성·고객 기준선, 라이선스/배포 정책·출시 blocker와 권한 있는 사람의 결정을 더한다. capability 목록의 `eligibility_rule`은 외부 출시 조건이며 내부 PRODUCT_QUALIFIED와 혼합하지 않는다. 어떤 상태도 문서 검사나 참고 DBMS의 성공으로 승격하지 않는다.

<a id="decision"></a>
## 12. DEV9-DESIGN-001 결정 기록

| 항목 | 결정 |
| --- | --- |
| 분류·담당 | D2 ACCEPTED, 작업·계약·통합 `/root`; 별도 실행 `/root/dev9_design_review`의 수정본 검토 PASS와 반영 결과는 조사 기록에 보존 |
| 근거 | 사용자의 DEV9 설계·Oracle/ARIES/로컬 소스 참고 요청, 기존 SC16/17·104 capability·DP01/03/06/07 |
| 선택 | 요구/variant의 동결과 candidate identity에 결합된 독립 제품 인수. R16의 설치·재현·문서와 버전 적용성 포함 |
| 대안 | 단계별 PASS만 합산하면 서로 다른 소스와 미시험 조합이 섞임. 외부 DBMS 출력만 정답으로 쓰면 dialect/오류 차이를 숨김. 전체 조합 무조건 나열은 무한 입력·불가능 조합을 다루지 못하므로 명세 domain 분할과 고위험 명시 조합을 선택 |
| 비용 | 세부 inventory·runner·독립 oracle·실제 장시간/복구 campaign·지원 환경별 artifact 유지 비용. 구현 전 F01–F10 동결로 구체화 |
| 대체 범위 | 기존 DEV9 개요를 상세화. 하위 의미·제품 범위·버전 정책·단계 순서·실행 상태는 유지 |
| 미완료 | 세부 leaf/variant/corpus/수치·환경·schema/runner·실제 package·현재 candidate 증거, 고객 출시 판단 |
| 재검토 | public surface/기능 조합 변경, evidence 누락 허용, false PASS 반례, 지원 환경/의존/provider 변경, 신규 안정 기준선 |

문서 설계 채택과 제품 실행은 별개다. [구현 계획](../plans/dev9-implementation-plan.md#completion)의 입력이 동결되고 실제 증거가 생기기 전까지 SC16·SC17·PRODUCT_QUALIFIED와 공식 배포는 NOT_RUN이다.
