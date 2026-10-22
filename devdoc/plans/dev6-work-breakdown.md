# DEV6 — 18개 세부 작업 분류와 공급·인수 계획

2026-10-22 · **분류·하위 작업 계획 / D6-WORK-BREAKDOWN-002 D2 ACCEPTED·독립 검토 완료 / 제품 구현·시험·CLIENT1 NOT_RUN**

[DEV6 구현 계획](dev6-implementation-plan.md) · [클라이언트 의미 정본](../contracts/dev6-client-design.md) · [이번 ARIES·Oracle·로컬 소스 조사](../evidence/dev6-work-breakdown-research-20261022.md) · [완료 계약](stage-completion.json)

DEV3의 [공급·완료 경계와 작업 크기](dev3-implementation-plan.md#remaining-work)를 적용해 DEV6의 기존 D6-I01–I05를 6.01–6.18로 나눈다. 번호는 실행 순서나 동일한 작업량이 아니다. 18개 책임과 기존 55개 하위 묶음의 ID를 유지하고, 큰 묶음은 아래 네 자리 ID의 완료 단위로 추가 분해한다. 분해된 부모는 집계용이며 자식과 작업량을 이중 계산하지 않는다. 실제 API symbol·파일·runner 이름은 구현 때 정한다. 제품 의미는 DEV6-DESIGN-001·REVIEW-002와 DEV4/5·COMM 정본을 따른다. F01–F07의 미동결 항목을 이 분류만으로 확정하지 않는다.

[재검토·개선 근거](../evidence/dev6-work-breakdown-review-20261022.md)는 최초 분류 조사와 별도다. 이번 보완은 ODBC의 직접 선행, 큰 작업의 완료 단위, 공통 책임의 집계와 [검증 예산](dev6-implementation-plan.md#execution-budget)을 다룬다. 55개를 최종 구현·시험 inventory의 분모로 사용하지 않는다.

현재 모든 하위 작업은 **계획만 작성한 상태**다. 기존 DEV3 구현이나 과거 DEV6 정적 검토를 신규 client 구현·시험 결과로 승계하지 않는다. 새로운 제품 경로·package·dependency는 실제 구현과 함께 등록한다.

<a id="catalogue"></a>
## 1. 분류·기존 작업·인수 연결

아래 F/T/C/M/P는 [DEV6 구현 계획](dev6-implementation-plan.md)의 D6-F/T/C/M/P ID이며 R2-V는 REVIEW-002 vector다. SC04·SC05·SC06은 각각 XADB-034·035·036의 실제 단계 인수다.

| ID·분류 | 기존 묶음·의미 정본 | 핵심 공급 | 인수 연결 |
| --- | --- | --- | --- |
| [6.01 공통 계약·지원표](#work-01) | I01–I04, [scope/protocol](../contracts/dev6-client-design.md#protocol) | wire/API/error·ABI 동결 목록과 server 공급 대조 | F01–F04, T01/05/12/13/14 |
| [6.02 wire codec·협상](#work-02) | I01, [protocol](../contracts/dev6-client-design.md#protocol) | bounded Rust codec·독립 corpus 일치 | F01, T01, R2-V01 |
| [6.03 접속·세션 수명](#work-03) | I01/I02, [state](../contracts/dev6-client-design.md#state) | 검증된 연결·실패 정리·Broken/Fenced 처리 | T02/04/08, C01 |
| [6.04 호출 진행·취소·deadline](#work-04) | I01, [state](../contracts/dev6-client-design.md#state) | public operation guard·wire ID·terminal/Ack 장벽 | T03/04, C01/02/08, M01/02, R2-V01/02 |
| [6.05 오류·outcome·진단](#work-05) | I02–I04, [errors](../contracts/dev6-client-design.md#errors) | lossless typed outcome와 driver별 진단 | T11/12, C03/05/06, R2-V02/03 |
| [6.06 값·인코딩·바인딩](#work-06) | I02, [values](../contracts/dev6-client-design.md#values) | owned value·원자 BindAll·독립 변환 vector | T06/07, C02 |
| [6.07 prepare·metadata·재실행](#work-07) | I02, [values](../contracts/dev6-client-design.md#values) | immutable descriptor·expected-result·stale 거절 | T05/07/09, C02 |
| [6.08 fetch·결과 수명](#work-08) | I02, [native/transactions](../contracts/dev6-client-design.md#transactions) | owned batch·EOF/prefix 구분·cached row 무효화 | T08/09, C07, R2-V04/05 |
| [6.09 거래·모드 전환·DDL](#work-09) | I02–I04, [transactions](../contracts/dev6-client-design.md#transactions) | native/raw와 JDBC/ODBC 조립별 상태 전이 | T05/10/11, C03/05/06/08, M03, R2-V03/04 |
| [6.10 Native 공개 API·C ABI](#work-10) | I02, [native](../contracts/dev6-client-design.md#native) | Rust facade·generation registry·좁은 FFI | F03, T06–12/15, M02, SC04 |
| [6.11 Java wire·접속 기반](#work-11) | I03, [jdbc](../contracts/dev6-client-design.md#jdbc) | 순수 Java codec·Selector·URL/service/DataSource | F01/02/05, T01–04/13, M01/02 |
| [6.12 JDBC API·결과·진단](#work-12) | I03, [jdbc](../contracts/dev6-client-design.md#jdbc) | 실제 JDK21 method별 지원과 상태/거래 조립 | T05–13, C02/06/07, M03, R2-V02–05, SC05 |
| [6.13 ODBC handle·descriptor·탐지](#work-13) | I04, [odbc](../contracts/dev6-client-design.md#odbc) | ENV/DBC/STMT 수명·attrs/함수표·진단 | F03/04/05, T02/10/12/15, M02, SC06 |
| [6.14 ODBC 변환·binding·부분 buffer](#work-14) | I04, [odbc](../contracts/dev6-client-design.md#odbc) | Execute시 복사·bound fetch·SQLGetData | T06–09/14/15, C02/07, R2-V06, SC06 |
| [6.15 자원 회계·계측](#work-15) | I01–I05 공통, [resources](../contracts/dev6-client-design.md#resources) | 효과 전 quota·cleanup reserve·상관 가능한 측정 | F03/04/07, T02/07/08/12/18, P01–04 |
| [6.16 독립 oracle·모델·복구](#work-16) | I01/I05, [faults](dev6-implementation-plan.md#faults) | literal 기대값·known-bad·외부 ACK/reopen | F06, T01–16, C01–08, M01–03 |
| [6.17 빌드·설치·산출물 identity](#work-17) | I02–I05, [resources](../contracts/dev6-client-design.md#resources) | 고정 toolchain·ABI probe·clean export·설치 | F05, T15/17, SC04–06 |
| [6.18 성능·증거 종합·최종 인수](#work-18) | I05, [performance](dev6-implementation-plan.md#performance) | P01–04 비교/replay·SC별 잔여 의무·CLIENT1 | F07, T18, SC04→SC05→SC06, CLIENT1 |

6.01·6.05·6.09는 공통 **의미**의 소유 분류다. 그 계약을 소비하는 순수 Java 구현은 6.11/12에 있고 Rust 구현 완료를 선행으로 요구하지 않는다. 6.13/14는 native core API를 소비하지만 C facade를 경유하도록 강제하지 않는다. 6.15/16/17/18의 준비 산출물은 처음부터 필요한 작업에 공급하며 최종 분류 번호 때문에 뒤로 미루지 않는다.

<a id="entry-exit"></a>
## 2. 공통 착수·완료 기록

| 항목 | 실제 작업에 기록할 내용 |
| --- | --- |
| 소유 | 하위 작업 ID, 작업/계약/통합 담당과 독립 검토자, 기준 commit·dirty digest, 좁은 owned paths·공유 수정 순서 |
| 배정·견적 | 실제 구현을 소유하는 가장 작은 완료 단위와 driver/API 적용 범위, 구현·검토·시험 준비의 예상 범위/불확실성, 별도 기계 실행 시간. 아직 F02/F04 inventory가 없으면 미산정으로 남기고 임의의 동일 일수·진행률을 부여하지 않음 |
| 선행 수신 | 해당 **하위 산출물**의 symbol·revision·증거 범위, 실제 COMM/foundation 공급, 필요한 DEV5 D6-W01 공급. 상위 분류 전체 완료를 임의의 선행으로 추가하지 않음 |
| 구현 전 동결 | 소비하는 F01 bytes·F02 상태/오류·F03/04 ABI/수명·F05 환경·F06 fault·F07 측정 항목만 명확히 식별. 일부 동결을 전체 F-ID 완료로 표시하지 않음 |
| 구현과 함께 추가 | 정상·경계·오류·해제의 test/fixture/oracle/harness와 필요한 real I/O/thread/C/Java/DM/fault 입력. production codec을 정답 계산에 사용하지 않음 |
| 효과 전 자원 | 해당 경로의 실제 allocation/변환/retained bytes·FD/handle peak, quota 예약·반환, 진단/취소 reserve. 계산과 실패 입력은 allocation 또는 송신 전 준비 |
| 검증 선택 | 마지막 관련 검증 source와 실제 diff를 대조하여 새 코드·변경 경로의 필요한 사례만 선택. 변경 없는 하위 코드·완료 사례·전체 wrapper를 반복하지 않음 |
| 공급 완료 | 산출물·관련 검증 결과·독립 검토와 남은 통합 의무를 함께 기록. fixture peer의 성공은 실제 SC 인수가 아니며 NOT_RUN/FAIL/중단 원본을 보존 |

**시험 실행 범위는 [루트 사용자 지시](../../AGENTS.md)가 우선한다.** DEV3 최종 S5의 전수 예외를 DEV6으로 확대하지 않는다. 아래 case 연결과 기존 T01–T18·V3·하위 회귀 문구는 인수 의무의 위치이며 무변경 코드 재실행 명령이 아니다. 새 구현에 필요한 모델·실제 복구·확대 사례는 각각 구현·선택하고, 최종 인수는 원래 source/범위가 명확한 기존 결과와 변경 영향 및 미실행 의무를 대조한다. 누락을 PASS로 바꾸거나 새 fingerprint만으로 재실행하지 않는다. 계획 작성으로 제품 개발·시험을 시작하지 않으며 문서 자동 검사는 push gate에서만 실행한다.

<a id="ownership"></a>
### 공통 계약·실제 구현·통합 증거의 배정

하나의 구현 산출물에는 주 담당 완료 단위를 하나만 둔다. 공통 의미 담당은 mapping·전이·독립 기대값을 공급하고, driver 구현 담당은 그 의미를 실제 호출에 구현한다. 같은 결과를 여러 인수 조건에 연결할 수 있으나 구현량이나 시험 실행 횟수로 중복 집계하지 않는다. 서로 다른 언어·ABI 구현의 시험을 하나의 PASS로 합치지도 않는다.

| 주제 | 공통 공급·대조 | 실제 구현 주 담당 | 통합·집계 |
| --- | --- | --- | --- |
| 진단 | 6.05.1/2 의미, 6.05.3 mapping 대조표 | Rust 6.10.1, C 6.10.3, JDBC 6.12.3.2, ODBC 6.13.2.3 | 6.05.3이 각 구현의 T12/15 증거를 연결; 같은 facade를 다시 구현하지 않음 |
| 거래·모드 | 6.09.2 API별 상태 전이·부분 outcome | native 6.09.1, JDBC 6.12.3.1, ODBC 6.13.3.4 | 6.09.3 의미 통합, 6.16.4 실제 복구. Java는 native 구현 완료 대신 동결 의미를 수신 |
| 자원·계측 | 6.15.1 driver별 회계 기반·reserve, 6.15.3 공통 관측 규약 | allocation·cleanup·span을 생산하는 각 기능 leaf | 6.15.2/3은 경로별 계산·측정 누락을 대조. 별도 harness 구현만 별도 작업량 |
| 독립 검증 | 6.16.1 corpus, 6.16.2 모델, 6.16.3 runner | production과 분리된 XADBTest 담당; 제품 hook은 해당 제품 owner | 6.16.4는 driver별 실제 실행 증거, 6.18.3은 잔여 인수 의무 집계 |
| 빌드·설치 | 6.17.1 환경 입력 | 6.17.2 recipe·등록, 6.17.3 driver별 설치·load | 공유 manifest/export는 통합 담당 순차 반영. early load는 전체 T17 완료가 아님 |

6.11.2·6.12.3·6.13.1/2/3·6.16.2/3·6.18.2는 여러 독립 산출물을 포함하므로 아래 자식 단위로 배정한다. 6.04의 한 호출 guard·cancel latch·deadline 불변식은 분해 후에도 하나이며 각 소비 API 조립의 통합 완료 조건으로 유지한다. 6.15/17 및 6.16.4/6.18.3은 `(작업 ID, driver, 적용 API/case)`로 공급·완료 범위를 구분한다. 파일이나 함수마다 ID를 추가하지 않는다.

<a id="work-01"></a>
## 3. 6.01 공통 계약·지원표

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.01.1 wire 공급 대조 | DEV4/5·COMM 정본 → D5-F01/D6-F01 공동 field/enum/profile·expected-result·metadata/outcome 목록. 미공급 body와 DEV5 담당을 명시하고 독립 bytes 작성이 가능한 범위를 동결 |
| 6.01.2 API·오류·상태 목록 | 채택된 DEV6 계약 → native operation/JDK21 overload/ODBC function·attr별 Supported/Conditional/Unsupported/LocalNoOp, lower error·SQLSTATE/code·상태/부수 효과·case 연결. unknown 항목을 빈 성공으로 채우지 않음 |
| 6.01.3 ABI·빌드 경계 목록 | 6.01.2·저장소 RB04/06 → F03 Rust/C signature·FFI/OS 조립/unsafe 경계, F04 DM/header·descriptor/length 단위·변환 목록. 실제 구현 전 D2 동결·독립 검토; 이번 분류는 ABI 확정 아님 |

T01/05/12/13/14의 입력 분모를 준비하며 F02의 전체 method inventory 미작성 상태를 유지한다. API/형식 변경이 필요하면 해당 owner를 먼저 수정한다.

<a id="work-02"></a>
## 4. 6.02 wire codec·협상

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.02.1 header·body codec | 6.01.1·6.15.1·6.16.1의 관련 입력 → Rust encode/decode. 길이/overflow·UTF-8·enum·fragment·metadata count를 allocation 전 검사하고 literal 정상/손상 bytes 일치(T01) |
| 6.02.2 협상·호환 거절 | 6.02.1 → Hello/Welcome version/capability/profile 검산. 불일치·잘림·필수 옵션 오류에서 session/SQL handle 공개0, 자동 downgrade0(T01/02) |
| 6.02.3 송수신 admission 연결 | 6.02.1·6.04.1의 ID 계약·실제 transport 공급 → partial I/O와 FIRST enqueue 수락을 구분. local encode/quota 실패·Backpressured 재개 뒤 ID gap/중복 소비0(R2-V01/C01) |

Rust/Java/독립 decoder의 공통 corpus를 공급한다. Java 구현은 6.11이 별도로 검증하며 하나의 decoder 성공을 다른 구현의 PASS로 복사하지 않는다.

<a id="work-03"></a>
## 5. 6.03 접속·세션 수명

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.03.1 endpoint·접속 | 6.02.1/2·실제 transport/platform 조립·6.05.1 → socket/principal/database 검증, connect/handshake timeout·인증 실패 정리. 완결 Welcome 이후에만 연결 공개(T02) |
| 6.03.2 연결 상태·해제 | 6.03.1·6.04.1·6.15.1 → Ready/진행/Broken/Fenced/closed의 owner와 명시 close/local abort/Drop 분리. Drop의 숨은 commit·blocking·신규 allocation0(T02/08) |
| 6.03.3 종료 통합 | 6.03.2·6.04.2/3·6.08.3·6.09.1의 해당 공급 → active 호출/거래·server 종료·권한 철회/만료 시 handle과 자원 정리. 연결 폐기를 rollback 성공으로 보고하지 않음(T02/04/08) |

후속 C/JDBC/ODBC는 각 API의 close/disconnect 차이를 유지한다. native 세션 하나의 검증으로 전체 driver 종료 의미를 인수하지 않는다.

<a id="work-04"></a>
## 6. 6.04 호출 진행·취소·deadline

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.04.1 상태 primitive | F01/F02·6.05.1·6.15.1 → public operation generation/guard·wire request ID reserved/accepted·control ticket. I/O 전에 수명/짧은 lock 순서를 검토하고 M01/02의 관련 전이와 연결 |
| 6.04.2 요청 진행·취소 장벽 | 6.04.1·6.02.3·6.03.1 → 송수신 owner·wake/control slot·중복 cancel 합침·terminal/Ack 두 순서. 늦은 Cancel(N)이 Fetch(N+1)에 영향0(T03/C01/08) |
| 6.04.3 복합 호출·시간 예산 | 6.04.2·6.07/08/09의 해당 API 조립 공급 → BindAll→Execute와 Close→COMMIT→SET 전체 guard/cancel latch·남은 deadline 유지. 요청 없는 구간의 cancel도 다음 송신을 차단하고 이전 완료 효과 보존(T04/C02/06, R2-V02) |

6.04.1은 prepare·거래 구현을 기다리지 않는 기반이다. 6.04.3은 각 소비 API가 생길 때 따로 통합한다. client timeout/CancelAck가 서버 cleanup 완료라는 해석은 금지한다.

<a id="work-05"></a>
## 7. 6.05 오류·outcome·진단

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.05.1 공통 outcome 표현 | 6.01.2·6.15.1 → code/state/effect/certainty/retry/engine cause·DDL 두 outcome·모드 조립 단계별 결과와 고정 진단 여유. 정상·오류 literal 기대값(T12) |
| 6.05.2 우선순위·연결 상태 | 6.05.1·6.04.2·6.09의 해당 생산 경로 → 완결 COMMITTED 보존, 유실 Unknown·확정 NotCommitted/Fenced 분리. timeout/40001로 Unknown을 낮추는 mutant 거절(T11/12, C03/05/06) |
| 6.05.3 driver 진단 mapping·대조 | 6.05.1/2의 해당 의미 → C typed getter·JDBC exception/accessor/chain·ODBC snapshot/vendor fields의 기대값과 누락 대조표를 먼저 공급. 구현은 [주 담당](#ownership)에 귀속하고 실제 T12/15 증거를 후속 연결. 반복 조회 불변·quota 중 결과 보존을 확인 |

표현을 먼저 공급하고 각 생산자·adapter의 실제 mapping을 그 구현과 함께 닫는다. 진단 문자열 파싱으로 성공·retry 가능성을 정하지 않는다.

<a id="work-06"></a>
## 8. 6.06 값·인코딩·바인딩

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.06.1 값·변환 | F02·6.05.1·6.15.1 → Bool/Int64/Utf8/Binary·typed NULL·byte length와 strict 인코딩. min/max/overflow·surrogate·NUL·빈 binary/빈 text 차이를 literal로 확인(T06) |
| 6.06.2 owned binding | 6.06.1·F03 소유권 → 전체 검사·quota 예약·복사 후 원자 교체, 실패시 기존 vector 보존. 개수0/256/257·Unbound·rebind 실패·caller 변경을 검증(T07) |
| 6.06.3 BindAll 실행 연결 | 6.06.2·6.02.1/3·6.04.1/2 → BindAll terminal 뒤에만 Execute. local 변환/BindAll 실패 뒤 이전 binding 실행0, SQL 문자열 치환0(T07/C02). 6.07.2와 결합할 때 6.04.3의 해당 복합 호출도 인수 |

JDBC setter/getter 복사는 6.12, ODBC descriptor 등록 뒤 Execute마다 읽기는 6.14에서 구현한다. 공통 타입 의미가 메모리를 읽는 시점까지 같다는 뜻은 아니다.

<a id="work-07"></a>
## 9. 6.07 prepare·metadata·재실행

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.07.1 prepare·describe | 6.02.1/3·6.03.1/2·6.04.1/2·6.05.1·F02 → statement class·parameter/result immutable metadata·hints 처리. 타입 미결정·ordinal·duplicate label의 독립 기대값(T07/09) |
| 6.07.2 실행 종류·binding 연결 | 6.07.1·6.06.3·6.04.1/2 → Any/Rows/Update 기대 종류를 wire에 전달하고 6.04.3의 해당 복합 호출을 함께 통합. 실제 DEV5가 DML/DDL precommit 전 mismatch를 거절함을 확인(T05/C02) |
| 6.07.3 재실행·stale·해제 | 6.07.1/2·6.08.3 → DROP/recreate/index 변경·권한 철회 후 첫 효과0, 자동 reprepare0, 원래 descriptor 보존·명시 해제(T08/09) |

SQL 분류기를 client에 복제하지 않는다. 6.07.2의 server 효과0 인수는 실제 D6-W01 공급 이후이며 local 검사만으로 대신하지 않는다.

<a id="work-08"></a>
## 10. 6.08 fetch·결과 수명

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.08.1 결과 표현·소유권 | F02/F03·6.05.1·6.06.1·6.15.1 → owned row/metadata·quota lease와 result 상태. network/page buffer 주소가 반환 값에 남지 않고 caller 보유 bytes를 회계(T06/08) |
| 6.08.2 bounded fetch·완료 판정 | 6.08.1·6.07.2·6.04.2 → one batch·EOF/partial/prefix 실패 구분, fetch별 새 deadline. 일부 행 수신을 전체 query 성공으로 보고하지 않음(T08/C07) |
| 6.08.3 무효화·재사용 | 6.08.1/2·거래 종료 전이 계약 → close/reexecute/rollback/연결 폐기와 local cached row/offset 정리. 정상 implicit EOF 마지막 batch 보존과 no-op/진입 거절의 row 보존을 구별(R2-V04) |

6.08.3의 상태 primitive는 6.09가 소비하고 실제 거래 종료와 함께 통합 검증한다. JDBC None/Rows/Update 소진은 6.12.2, ODBC row 소비/partial copy는 6.14가 소유한다.

<a id="work-09"></a>
## 11. 6.09 거래·모드 전환·DDL

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.09.1 native/raw 거래 | F02·6.04.2·6.05.1·6.07.2·6.08.3 → BEGIN/COMMIT/ROLLBACK/SET과 authoritative state. native Active+SET 거절·commit/rollback cursor 정리·hidden commit0(T05/10) |
| 6.09.2 adapter 모드 조립 의미·대조 | F02·native/raw 거래와 6.04의 동결 상태 계약 → Close terminal 뒤 Idle/Active 재판정, 필요시 COMMIT→SET·각 단계 결과의 공통 기대값. 실제 조립은 6.12.3.1/6.13.3.4에 귀속하고 6.04.3의 해당 복합 호출을 통합(T10/C06/M03, R2-V03/04). Java 준비에 6.09.1의 Rust 구현 완료를 요구하지 않음 |
| 6.09.3 실패·DDL 통합 | 대상 생산 경로(native 6.09.1, JDBC 6.12.3.1, ODBC 6.13.3.4)·해당 6.09.2 의미·실제 B3/Q1/S1·6.16.3의 적용 도구 → S1 성공/S2 실패 후 commit·전체 abort/fence·DDL prior/ddl 두 outcome·ACK 유실. 현재 build reopen에서 독립 이력과 대조(T11/C03/05/08). Native 인수에 JDBC/ODBC 완료를 요구하지 않음 |

6.09.2는 의미와 상태 전이의 공통 분류이고 실제 JDBC/ODBC 코드는 6.12.3/6.13.3에 배정한다. 클라이언트에서 WAL/undo/recovery를 새로 구현하지 않는다.

<a id="work-10"></a>
## 12. 6.10 Native 공개 API·C ABI

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.10.1 Rust facade | F03·6.02–09의 제공된 관련 leaf → connect/prepare/bind/execute/fetch/거래/cancel/diagnostics를 좁은 API로 조립. 실제 B3 연동 전후의 검증 범위를 구별(T01–12) |
| 6.10.2 registry·수명 | F03·6.04.1·6.15.1 → kind+generation+parent 검산, stale/wrong parent/overflow 폐기·free/cancel 경쟁. facade 전체를 기다리지 않고 수명 primitive를 먼저 검증(T15/M02) |
| 6.10.3 C header·FFI·실제 C 호출 | 6.10.1/2·6.17.1 → ABI version·size/align/offset/symbol·pointer/length·panic/fence·전용 free. 유효 외부 포인터 전제의 실제 C harness와 적용 가능한 sanitizer/Miri 증거(T15/17) |

SC04는 Native/C의 필수 사례, 관련 6.15–18 증거와 실제 DEV5 공급 뒤 인수한다. C ABI package의 unsafe 예외를 native core 전체로 확대하지 않는다.

<a id="work-11"></a>
## 13. 6.11 Java wire·접속 기반

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.11.1 순수 Java codec | F01/F02·6.16.1·6.17.1·Java 자원 계산 → 동일 wire corpus에 대한 독립 Java encode/decode. Rust/JNI 의존 없이 length/encoding/fragment 경계를 검증(T01) |
| 6.11.2 Selector·operation owner | 아래 6.11.2.1–3으로 배정. 순수 Java 진행·취소 구현과 실제 thread 검증을 분리 기록하되 전체 operation 불변식을 함께 인수 |
| 6.11.3 URL·driver 등록·DataSource | 6.11.1/6.11.2.2·F02/F05 → jdbc:xadb:unix와 properties 검증·service loader·DriverManager·pool 없는 DataSource. unknown/duplicate·다른 vendor URL·실패 정리(T02/13/17). 6.11.2.3의 모든 소비 API 경합 검증을 등록/접속 구현의 선행으로 삼지 않음 |

구현은 Rust/C 완료와 독립적으로 진행 가능하지만 실제 server 공급 없이 SC05를 인수할 수 없다. socket/취소 방식을 Oracle 구현과 동일하다고 주장하지 않는다.

| 완료 단위 | 직접 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.11.2.1 I/O·wake 기반 | 6.11.1·F01/F02의 진행 계약·6.15.1 Java 회계 → SocketChannel/Selector·partial TX/RX·wakeup·채널 종료 정리. 독립 peer로 조각/정지/EOF 검증(T01/02); 아직 취소 API 전체 완료 아님 |
| 6.11.2.2 operation·취소·시간 | 6.11.2.1·6.04의 동결 의미·M01/02 입력 → guard·wire ID 예약/소비·cancel latch·terminal/Ack·남은 deadline. R2-V01/02 및 T03/04의 해당 반례; 복합 API가 생길 때 각각 통합 |
| 6.11.2.3 Java thread 경합 | 6.11.2.2 및 해당 소비 API → cancel/finish/close·요청 사이 경쟁·park/resume을 실제 Java thread로 검증. 모델만으로 PASS 불가. 미구현 API의 의무는 별도 잔여로 유지 |

<a id="work-12"></a>
## 14. 6.12 JDBC API·결과·진단

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.12.1 statement·binding·metadata | F02 method inventory·6.11.1/6.11.2.2·6.12.3.2의 해당 진단·6.06/07 의미 → Statement/PreparedStatement setter 복사·immutable descriptor. inherited SQL overload·미지원 옵션은 명시 오류(T05–09/13). URL/DataSource 전체 완료는 local API 구현 선행 아님 |
| 6.12.2 ResultSet·결과 소진 | 6.12.1·6.08 의미 → next/getter/wasNull·label 첫 ordinal·closed/EOF·None/Rows/Update·getMoreResults. false/null/-1 일치와 cached row 무효화·마지막 batch 예외(T06/08/09/13, R2-V04/05) |
| 6.12.3 거래·예외·abort | 아래 6.12.3.1–3의 주 담당 완료를 합산. 진단 기반은 statement·거래보다 먼저 공급할 수 있고 전체 묶음을 선행으로 사용하지 않음 |

SC05는 해당 Java 구현의 전체 지원표·복구/자원/설치 증거와 실제 SC04 뒤 인수한다. Java API 준비나 실제 통합의 착수를 SC04 완료까지 모두 지연하지 않는다.

| 완료 단위 | 직접 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.12.3.1 거래·모드 조립 | 6.11.2.2·6.12.1/2의 해당 result 수명·6.12.3.2·6.09.2 의미 → commit/rollback/setAutoCommit·단계별 outcome. T10/11·C06·R2-V02/03/04와 각 복합 API guard 검증 |
| 6.12.3.2 예외·outcome facade | F02·6.05의 해당 의미·6.15.1 Java reserve → SQLException subclass·vendor accessor·chain·불변 diagnostics. literal로 먼저 검증하고 각 API 생산 경로에서 T12/13 연결; 거래·ResultSet 전체 완료를 기다리지 않음 |
| 6.12.3.3 close·abort | 6.11.2.2·6.12.3.2·동결 수명 계약 → close/abort·Executor 거절·호출 깨움·handle 무효화. local 경로부터 공급하고 active result/거래 종료는 각 생산 경로와 통합(T02/08/10/13). 연결 폐기를 rollback 확인으로 보고하지 않음 |

JDK21 reflection 목록은 6.01.2가 공급하고 6.11/12의 각 구현 담당이 자기 signature의 도달·오류를 연결한다. 전체 지원표 대조는 6.18.3의 SC05 집계에서 수행하며 6.12.3 하나에 모든 JDBC 검증량을 귀속하지 않는다.

<a id="work-13"></a>
## 15. 6.13 ODBC handle·descriptor·탐지

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.13.1 ABI·handle·접속 | 아래 6.13.1.1–3으로 ABI/수명·문자열·실제 접속을 배정. local 준비에 native core 전체 완료를 요구하지 않음 |
| 6.13.2 descriptor·지원 탐지·진단 | 아래 6.13.2.1–3으로 별도 공급. 실행·거래 API가 공통 descriptor·진단을 소비하며 지원 탐지 전체 완료를 불필요한 장벽으로 추가하지 않음 |
| 6.13.3 실행·거래·정리 | 아래 6.13.3.1–4로 배정. 직접 실행·매개변수 없는 prepare는 6.14 전체 완료를 기다리지 않으며 parameter 실행만 6.14.1을 소비 |

ODBC는 native core의 동결 API를 소비한다. C facade 완성을 별도 구현 선행으로 추가하지 않는다. 실제 driver manager 호출과 native ABI 검증을 분리 기록한다.

| 완료 단위 | 직접 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.13.1.1 ABI·handle 수명 | F03/F04·6.17.1 ODBC 입력·6.15.1 → LP64/header probe·ENV/DBC/STMT registry·parent/generation·free/cancel 상태. stale/active child/재사용 경합(T15/M02). 포인터 handle의 이전 참조와 주소 재사용·tombstone 예산을 F04에서 설명하고 token만으로 해결됐다고 가정하지 않음 |
| 6.13.1.2 접속 문자열 | F04 grammar·A/W 단위 → DSN/property parser·duplicate/escape/미지원 옵션·encoding의 독립 vector(T02/14). socket 또는 SQL 실행 불필요 |
| 6.13.1.3 실제 접속 | 6.13.1.1/2·6.13.2.3의 진단·native connect/close → 실제 DM A/W 접속·실패 자원 회수(T02/15). 전체 값 변환은 선행 아님 |
| 6.13.2.1 descriptor·attribute | 6.13.1.1·F02/F04 → implicit descriptor·row/parameter binding 상태·Get/Set attrs·필수 필드. local 기본값/범위/수명(T14/15). 실행 연결은 소비자에서 완료 |
| 6.13.2.2 지원 탐지 | 6.13.1.1·F02/F04 지원표 → GetInfo/GetFunctions/GetTypeInfo의 실제 구현 capability 대조(T12/14). 미완성 항목을 가짜 지원으로 표시하지 않고 적용 범위·남은 의무를 유지 |
| 6.13.2.3 진단 snapshot | 6.13.1.1·6.05 의미·6.15.1 reserve → SQLGetDiagRec/Field·record/vendor outcome·교체/반복 조회. literal T12/15를 먼저 공급하고 실제 오류 생산 경로에서 추가 연결 |
| 6.13.3.1 기본 실행·prepare | 6.13.1.3·6.13.2.1/3·6.13.3.3 수명 기반·native direct/prepare/execute/result API → SQLExecDirect·parameter0 Prepare/Execute·metadata·Rows/Update 상태(T05/07/09). 6.14.1/3 없이 매개변수 없는 SELECT를 실제 DM에서 연결 가능 |
| 6.13.3.2 parameter 실행 | 6.13.3.1·6.14.1·native BindAll API·6.04의 복합 호출 계약 → 실행 시 입력 복사·BindAll 뒤 Execute·복합 guard(T06/07/C02). local 변환 실패 뒤 과거 binding Execute0 |
| 6.13.3.3 결과·binding 정리 | 6.13.1.1·6.13.2.1/3·6.08 수명 계약과 native close API → CloseCursor/MoreResults/FreeStmt의 상태 primitive를 먼저 공급. 각 실행/fetch 연결 시 CLOSE/UNBIND/RESET_PARAMS·cached row/offset 무효화를 검증(T08/R2-V04). parameter·부분 조회 전체 완료를 primitive 선행으로 삼지 않음 |
| 6.13.3.4 거래·접속 종료 | 6.13.1.3·6.13.2.1/3·6.13.3.3·native 거래 API·6.09 의미 → SQLEndTran/autocommit/Disconnect 조립. Active Disconnect 거절·ON EndTran no-op(T10/11/C06). result/parameter 생산 경로의 해당 조합은 생길 때 통합 |

매개변수 없는 실제 조회 경로는 6.13.1.3 → 6.13.3.1 → 6.14.2 → 6.13.3.3의 close 호출로 연결한다. 6.13.3.3의 상태/close 기반은 조회 전에 준비한다. 6.14.3은 이후 부분 조회를 추가하고 6.14.1은 parameter 입력 경로를 별도로 공급한다. 따라서 이 경로의 기반·소비·통합을 서로의 전체 완료 조건으로 되돌리지 않는다.

<a id="work-14"></a>
## 16. 6.14 ODBC 변환·binding·부분 buffer

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.14.1 input descriptor·Execute 복사 | F04·6.13.1.1·6.13.2.1/3·6.06 의미 → BindParameter 등록·Execute마다 caller 값 읽기/복사·NULL/length·A/W 단위. pointer 값1→2 재실행과 변환 실패 뒤 Execute0(T06/07/C02); 실제 실행은 6.13.3.2에서 연결 |
| 6.14.2 bound fetch | F04 출력 변환표·6.13.1.1·6.13.2.1/3·6.13.3.3 수명 기반·native result API → BindCol/Fetch·NULL indicator·truncation·전체 길이·guard bytes. 6.14.1 입력 parameter 완료는 선행 아님. local row vector 뒤 6.13.3.1의 실제 결과와 연결하고 row 소비·close 후 binding 보존 검증(T08/14/15) |
| 6.14.3 SQLGetData·부분 copy | 6.14.2·F04 → unbound ordinal 순서·offset·남은 변환 bytes·UTF 경계·terminator. 가변형 길이0 probe와 고정형 복사, NULL/빈 값/NO_DATA·HY090/HY009를 구별(T14/C07, R2-V06) |

입력/출력 type 조합별 지원·거절을 빠짐없이 F04에 연결한다. SC06은 실제 DM 환경의 자체 증거와 SC05 뒤 인수하며 다른 driver 성공을 승계하지 않는다.

<a id="work-15"></a>
## 17. 6.15 자원 회계·계측

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.15.1 회계·reserve 기반 | DEV6 resources와 대상 driver의 관련 F02/F03/F04; Rust는 기존 foundation quota → driver별 reservation/lease·진단/control reserve·실패 원자성 입력. codec/registry/binding보다 먼저 필요한 부분을 공급하며 다른 driver의 미동결 ABI 때문에 전체를 막지 않음 |
| 6.15.2 경로별 peak·실패 회수 | 각 leaf 구현과 함께 → 변환 전후·binding 교체·frame/result·retained batch/handle의 동시 bytes/FD 계산과 관련 실패 사례. 한도 초과 효과 전 거절·종료 뒤 live resource0(T02/07/08/12/15) |
| 6.15.3 계측·상관·압력 | 해당 driver 진행 경로·F07 → API/encode/copy/TX/RX/decode/반환·request ID 상관, trace loss·UNKNOWN·slow consumer/cancel 압력 관측(T18/P01–04) |

6.15.1/2는 마지막 성능 단계까지 미룰 수 없다. Java 전체 heap/RSS·caller buffer와 driver owned bytes를 구별하며 측정 없이 한도 준수·성능 개선을 주장하지 않는다.

<a id="work-16"></a>
## 18. 6.16 독립 oracle·모델·복구

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.16.1 corpus·독립 기대값 | 채택 의미의 준비는 즉시 가능, bytes는 F01/F02 동결 후 → literal wire/type/row/error·protocol peer·known-bad. production codec/parser와 기대값 계산 분리·잘못된 driver 거절 |
| 6.16.2 유한 모델 | 아래 M01/M02/M03별 완료 단위로 배정. 기존 bound·필수 R2 witness를 보존하고 미도달/초과를 미완료로 보고 |
| 6.16.3 외부 ACK·fault runner | 아래 ledger·fault 제어·negative self-test로 배정. driver가 필요한 hook/cut만 먼저 수신하되 적용 의무의 누락을 완료로 세지 않음 |
| 6.16.4 driver별 실제 복구 | 해당 driver leaf·실제 B3/Q1/S1/C1·6.16.3의 적용 도구/self-test → ACK된 commit 보존, Unknown과 abort/CLR 중 재장애·DDL/API 조립 부분 성공을 독립 DB 이력과 대조(T16/C01–08) |

6.16.1/2/3의 준비를 모든 driver 완료까지 미루지 않는다. 실제 복구는 SC04·SC05·SC06별로 소비하며 ARIES를 참고했다는 사실이나 fixture ACK를 disk 내구성 증거로 사용하지 않는다.

| 완료 단위 | 직접 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.16.2.1 M01 | F02 request/admission/terminal/Ack 상태 → 동결 전이·독립 모델·R2-V01 witness·기존 bound 완주 |
| 6.16.2.2 M02 | F02/F03/F04의 해당 handle/operation 수명 → 재사용·free·늦은 cancel·요청 없는 구간 latch·R2-V02 witness. 언어/ABI별 적용 차이 명시 |
| 6.16.2.3 M03 | F02·6.09.2의 모드·result 의미 → close 뒤 Idle/Active·commit/SET 부분 성공·cached rows·R2-V03/04 witness. M03 준비가 M01/02 공급을 막지 않음 |
| 6.16.3.1 관측·외부 ledger | F06·DEV5 관측 계약 → terminal decode/application 관측/controller receipt의 독립 기록·연결. 기록 유실·부분 기록은 ACK 아님(C03/04) |
| 6.16.3.2 fault 제어·도달 | F06·해당 DEV5/COMM/engine hook·6.16.3.1 관측 포트 → partial I/O·kill/reopen/recrash·C01–08별 reachability와 fresh fixture. 제어기 구현과 각 cut 적용을 구분 |
| 6.16.3.3 negative self-test | 6.16.3.1/2의 해당 기능 → 잘못된 driver/binary/history·ACK 누락·미도달 hook을 실제로 거절. 적용 self-test 전 해당 6.16.4 결과를 인수하지 않음 |

모델 입력·ledger 포트의 준비와 제품 API 통합 검증은 별도 완료다. 6.16.4 실행 카드는 driver×적용 cut와 관련 T/R2를 명시하고, 공통 도구의 같은 source 시험을 driver마다 반복하지 않는다. 각 driver의 실제 관측·복구 증거는 별도로 필요하다.

<a id="work-17"></a>
## 19. 6.17 빌드·설치·산출물 identity

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.17.1 환경·빌드 입력 동결 | F05 및 대상별 경계(native/C는 F03, ODBC는 F03/F04, Java는 F01/F02) → Rust/C compiler·JDK21 vendor/patch·Maven/plugin lock·unixODBC/header/config·OS/libc/profile 중 해당 입력. ABI probe/recipe 준비를 각 adapter 구현 전에 공급하며 Java가 C/ODBC 동결을 기다리지 않음 |
| 6.17.2 package·export 등록 | 실제 source/package 추가와 함께 → native/C header/library·JAR/service descriptor·ODBC library의 source fingerprint·clean export·dependency/feature 입력. 빈 경로 선등록0 |
| 6.17.3 별도 prefix 설치 | 해당 driver 산출물·6.17.1/2 → 깨끗한 export의 build/load·C/JDK/DM 호출·symbol/notice/checksum·예제. 누락 입력/stale binary/잘못된 DM을 거절(T15/17) |

설치 recipe 성공과 SC의 SQL/복구 성공을 구별한다. 각 driver의 설치 검증을 전체 세 driver 완성까지 일괄 지연하지 않는다.

<a id="work-18"></a>
## 20. 6.18 성능·증거 종합·최종 인수

| 하위 작업 | 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.18.1 profile·inventory 준비 | 기존 F07/P01–04·F02 지원표 → workload/seed/order·warmup/GC·통계·guardrail·실제 명령·미실행 분모. 성능 측정 전에 동결하고 새 확대 사례는 해당 기능 구현과 함께 작성 |
| 6.18.2 실제 비교·replay | 아래 driver/profile별 실행·분석·재현 단위로 배정. [실행 예산](dev6-implementation-plan.md#execution-budget)을 먼저 명시하고 FAIL/누락을 제외해 비율 개선 금지 |
| 6.18.3 SC별 인수·CLIENT1 종합 | 각 driver 필수 leaf와 6.16.4/17/18.2의 적용 증거 → SC04→SC05→SC06 순서. T01–18·C01–08·M01–03·P01–04·R2-V01–06을 지원표와 대조하고 미실행/미해결 의무가 있으면 CLIENT1 미완료 |

6.18.3은 기존 시험을 무조건 다시 실행하는 wrapper가 아니다. [실행 범위](#entry-exit)에 따라 원래 결과/source·변경 영향·필요한 새 검증을 연결한다. 완료 항목 수/18·하위 항목 수를 진행률로 환산하지 않고 제공 capability·검증 범위·막힌 소비자·잔여 의무를 보고한다.

| 완료 단위 | 직접 선행 → 산출물·완료 조건 |
| --- | --- |
| 6.18.2.1 driver/profile 실행 | 대상 correctness·6.15.2/3·해당 6.17 산출물·F07 → `(driver, P-ID, cell 묶음)`별 독립 정답·raw sample·source/환경 manifest. 경쟁 build/stress 없이 사전 고정한 비교 순서·run 수 이행 |
| 6.18.2.2 통계·비용 판정 | 6.18.2.1의 해당 bundle·F07 → p99/throughput·copy/retained bytes·paired interval·계측 비용과 FAIL/누락/불확실성 별도 보고. 실제 sample 없는 수치 생성0 |
| 6.18.2.3 재현·증거 연결 | 6.18.2.1/2의 해당 bundle → 독립 정답·manifest·recipe·재현 결과. 기존 재현 의무를 driver별 인수에 연결하며 전체 campaign을 자동 반복하는 wrapper로 만들지 않음 |

SC04/05/06은 각각 자기 driver의 6.16.4·6.17.3·6.18.2 적용 증거를 수신한다. 이미 충족한 사례는 원래 source/범위로 연결하고 미실행·실패 의무만 명시하여 CLIENT1에 합산한다.

<a id="sequence"></a>
## 21. 우선 공급과 합류 순서

| 묶음 | 먼저 공급할 범위 | 후속 작업·합류 |
| --- | --- | --- |
| A 공통 입력 | 6.01의 관련 계약·6.16.1/2 기대값/모델 준비·6.15.1 회계·6.17.1 환경·6.18.1 profile 준비 | 모든 leaf의 필요한 부분에 공급. 전체 F01–07 동결을 모든 준비 작업의 공통 장벽으로 만들지 않음 |
| B Native 기반 | 6.05.1·6.02.1/2·6.04.1·6.03.1·6.10.2, 이어 6.02.3/6.04.2 | 값/prepare/fetch/거래 연결에 필요한 좁은 API 공급 |
| C Native 실행 | 6.06·6.07·6.08·6.09와 6.04.3·6.05.2·6.03.3를 소비 경로별 통합, 6.10.1/3 | 실제 DEV5 공급·Native/C의 관련 6.15–18 증거 뒤 SC04. JDBC/ODBC 전체 완료 불필요 |
| D adapter 구현 | 공통 계약 뒤 6.11→6.12, F03/F04 뒤 6.13/14의 독립 local 모듈 | Java는 Rust 완료 불필요. ODBC 실제 연결은 native core 공급 필요. 실제 Java/DM 통합 준비가 끝나는 대로 검사하되 SC05는 SC04 뒤, SC06은 SC05 뒤 인수 |
| E 증거 종합 | driver별 6.15.2/3·6.16.3/4·6.17.2/3·6.18.2/3 | source/variant가 명확한 증거와 잔여 의무 대조. 모든 필수 증거가 충족되어야 CLIENT1 |

위 묶음은 고정 직렬 단계가 아니다. 6.08.1/3 상태 primitive를 먼저 공급한 뒤 6.09와 통합하고, 6.04.1/2가 있으면 복합 API별 6.04.3을 닫는 방식으로 순환 선행을 피한다. ODBC의 기본 실행은 6.13.3.1, parameter 실행은 6.13.3.2가 소유한다. 6.14.2는 입력 parameter 대신 descriptor·진단·결과 수명 기반을 직접 수신한다.

<a id="priority"></a>
### 조기 공급·통합 우선순위

다음은 A–E 안에서 선택할 작업 순서이며 새 gate가 아니다. 실제 서버 통합은 [기존 진입 조건](dev6-implementation-plan.md#entry)의 B3/Q1/S1/C1·D6-W01 공급 이후다. 그 전에는 local/fixture 범위만 기록한다. Native/C·Java·ODBC의 준비는 필요한 계약 뒤 겹칠 수 있으며 SC 인수 순서를 개발 전체의 직렬 대기로 확대하지 않는다.

| 우선순위 | 공급·연결할 범위 | 드러낼 위험·완료 경계 |
| --- | --- | --- |
| 1 계약·환경·수명 | 6.01의 필요한 F01/F02·독립 bytes, 6.15.1 reserve, 6.17.1 JDK/DM/ABI, 6.04.1·6.10.2·6.13.1.1, 6.16.2.1/2 | field/표준/ABI 불일치·quota·generation/취소 소유권을 효과 전 확인. F03/F04 미동결 부분은 계속 OPEN |
| 2 작은 실제 연결 경로 | 각 driver의 접속 → 매개변수 없는 SELECT 또는 parameter0 prepare → fetch → 명시 close. ODBC는 6.13.3.1/6.14.2/6.13.3.3, JDBC는 6.11과 6.12의 해당 범위 | 각 경로의 정상·필수 오류/정리와 실제 C/Java/DM 호출. 로딩·I/O·result 수명 문제를 조기에 발견; 전체 지원표·SC 완료 아님 |
| 3 거래·장애 경계 | 해당 DML/거래 공급 뒤 COMMIT/응답 유실·실제 reopen, 취소와 terminal/Ack·정지 출력, mode 조립의 부분 성공; 6.16.3/4 해당 cut | UNKNOWN·알려진 commit·실제 rollback 완료를 독립 관측. 진단·fault runner가 마지막 작업까지 지연되지 않게 공급 |
| 4 지원표 확대 | 남은 statement·metadata·type/NULL·parameter·A/W·부분 buffer·미지원 overload·수명 조합 | 각 새 기능에 관련 정상/오류/해제/자원 시험 동반. 1–3의 미완료 의무를 완료로 승계하지 않음 |
| 5 확대 검증·종합 | driver별 아직 남은 fault/thread/fuzz/압력·P01–04·설치/재현·SC 인수 | [실행 선택·예산](dev6-implementation-plan.md#execution-budget)에 따라 증거를 채우고 SC04→SC05→SC06 집계. 무변경 코드 전체 재시험 금지 |

배포용 산출물이 완성되기 전에도 해당 6.17.2 source/package가 생기면 JAR service load·C symbol/header·ODBC DM load의 좁은 확인을 수행한다. 전체 T17 설치 검증이나 SQL 성공으로 승격하지 않는다. 기술적 위험을 먼저 확인하는 순서이며 성능 최적화·지원표 전체 확장을 이 작은 경로의 선행으로 만들지 않는다.

<a id="work-size"></a>
### 작업 크기와 배정 기준

범위가 큰 이유는 Java의 독립 transport/상태 구현, ODBC의 descriptor·포인터·A/W 조합, 외부 process·disk를 연결하는 fault 도구, 장시간 측정의 서로 다른 비용이다. 이를 모두 같은 하위 작업 하나의 기간으로 계산하지 않는다. 착수 카드에는 다음 정보를 함께 기록한다.

| 항목 | 작성 기준 |
| --- | --- |
| 분량 | API/전이/타입·오류 조합과 공급 산출물, 새 harness 수, 실제 runtime/ABI의 범위. 구현·검토·도구 준비 예상은 범위와 근거·불확실성을 표시하고 F02/F04 전에는 미산정 허용 |
| 직접 선행·우선순위 | 수신할 정확한 자식 ID/계약과 막힌 소비자, 위 우선순위에서 위험을 줄이는 이유. 상위 묶음 전체를 일괄 선행으로 쓰지 않음 |
| 구현 담당 | 해당 artifact를 생산하는 주 담당 하나. 공통 의미 담당과 통합 담당·독립 검토자는 별도 열로 기록. 현재 미래 구현 담당은 미배정 |
| 실행 시간 | 사람/agent 작업량과 기계 점유 시간을 분리. 모델·fault·stress·성능의 cell/반복/환경·직렬 자원은 구현 계획의 예산 카드에 연결 |
| 완료 | 독립적으로 수신 가능한 산출물·선택 검증·검토 결과와 남은 통합 의무. 부모 집계와 자식 완료를 중복 계산하지 않음 |

첫 해당 모듈의 실제 작업·검토·재작업 및 실행 시간이 생기면 후속 견적에 반영한다. 이번 문서에는 관측 없는 일수·생산성 향상률·고정 전체 기간을 넣지 않는다. 검증 가능한 완료 단위가 이미 작은 6.02.2 같은 항목까지 개수를 맞추려고 분해하지 않는다.

[기존 PAR01–06](dev6-implementation-plan.md#parallel)의 조건부 병렬 가능성은 유지한다. 이 분류가 미래 agent 배정이나 이번 병렬 제품 개발의 승인을 뜻하지 않는다. 실제 착수 때 좁은 파일 소유를 정하고 wire 정본·공유 manifest/lock·export·문서 JSON은 통합 담당이 순차 반영한다. 독립 검토는 DP01에 따라 구현자와 다른 실행이 맡는다.

<a id="decision"></a>
## 22. D6-WORK-BREAKDOWN-001

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22; 기존 계약의 공급·인수 배정 상세화 |
| 권한·담당 | 사용자의 DEV3 같은 세부 분류·ARIES/Oracle/로컬 DBMS 참고·한 번 commit 요청. 작업/계약/통합 `/root` |
| 기준 | HEAD `def35fb1e274040eb55d78fa68948ba1610166c1`, 착수 worktree clean |
| 선택·대안 | 기존 I01–05를 유지하면서 18개 책임과 하위 산출물로 전개. driver별 큰 묶음만 유지하면 취소/거래/자원/복구 누락을 추적하기 어렵고, 함수/파일별 분할은 하나의 상태 전이를 흩뜨리므로 제외 |
| 의미·상태 | API/형식/수치·task DAG·SC 순서·제품 범위는 기존 정본 유지. F01–F07·detail_expansion_required·CLIENT1 상태 미승격. 현재 사용자 시험 제한을 DEV6 계획 읽기에 명시 |
| 조사 | [실제 읽은 원문·revision/hash·분류 적용](../evidence/dev6-work-breakdown-research-20261022.md); 타 DBMS 코드/이력·의존성 도입 없음 |
| 검토·검증 | `/root/dev6_breakdown_review`가 새 분류/조사 전체와 연결 문서의 DEV6 변경분·정본·인용 자료를 읽기 전용 독립 검토: 필수 수정 지적 P1/P2 없음·정적 D2 채택 가능. 18/55개 분류·공급 순환 없음·인수 연결·driver별 선행·로컬6개 revision/hash 일치를 확인했고 `/root`가 반영 확인. [검토 범위](../evidence/dev6-work-breakdown-research-20261022.md#review). 제품 변경·시험·문서 자동 검사·push 없음 |
| 재검토 | owner/API 변경, 순환 선행, case 미배정, 자원/독립 기대값 준비 지연, 지원표·환경 확대 |

001은 최초 채택·검토 당시의 기록이다. 아래 002가 하위 배정·ODBC 선행·집계·실행 예산의 해당 내용을 보완·대체하며 최초 조사와 검토 결과를 새 보완의 승인으로 사용하지 않는다.

<a id="decision-002"></a>
## 23. D6-WORK-BREAKDOWN-002

| 항목 | 기록 |
| --- | --- |
| 분류·상태·일자 | D2 · ACCEPTED · 2026-10-22; 독립 검토 반영 |
| 권한·담당 | 사용자의 DEV6 분배 검증 및 후속 수정·개선·완료 뒤 한 번 commit 요청. 작업/계약/통합 `/root` |
| 기준 | HEAD `9f79b31b99f558f4e9a98b8bd752220b851e7bbb`; 기존 README·documents.json 순서 변경은 이번 수정·commit에서 분리 보존 |
| 선택·대안 | 18개 책임·55개 기존 묶음 ID 유지, 큰 묶음만 네 자리 완료 단위 추가. 전체 재번호·함수별 분할은 참조/통합 비용 때문에 제외. 매개변수 없는 ODBC 조회에 입력 binding·부분 조회 전체 완료를 요구하는 선행을 좁힘 |
| 책임·분량 | 공통 의미/구현 주 담당/증거 집계를 구분. driver·모델·fault·성능 단위의 실제 분량과 기계 실행 시간을 분리하고 작은 실제 연결·장애 경로를 우선 공급 |
| 보존 범위 | API/SQL/wire 의미·F01–07 미동결·모델 bound·시험/성능 기준·SC04→05→06·CLIENT1 NOT_RUN 유지. DEV3 S5 전수 예외 확대 없음 |
| 근거·대체 | [재검토 기록](../evidence/dev6-work-breakdown-review-20261022.md). 001의 해당 배정·선행을 대체하고 나머지 책임·기존 역사 증거는 보존 |
| 검토·검증 | 별도 실행 `/root/dev6_allocation_review`가 원문·DEV6 diff·근거를 읽기 전용 검토하여 필수 수정 P1/P2 없음·정적 D2 채택 가능으로 판단. ODBC/Java의 직접 선행·의무 보존·집계·예산 가정·시험 제한과 source identity를 확인했고 `/root`가 반영 확인. [검토 범위·한계](../evidence/dev6-work-breakdown-review-20261022.md#review). 제품 변경·제품 시험·문서 자동 검사·push 없음 |
| 재검토 | F02/F04 전개 후 큰 완료 단위 잔류, 실제 critical path/자원 충돌, 도구 준비 지연·중복 구현, 측정 분모/예산 변경 또는 새로운 상태 반례 |
