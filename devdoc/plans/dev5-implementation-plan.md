# DEV5 — 서버 구현·검증 계획

2026-10-18 · **DEV5-DESIGN-001 ACCEPTED 설계 대응 / 제품·B3·PF11 NOT_RUN**

[서버 상세 설계](../contracts/dev5-server-design.md) · [소스 조사·검토](../evidence/dev5-source-research.md) · [단계 JSON](development-stages.json) · [완료 계약](stage-completion.json)

이 계획은 XADB-012 서버 조립과 XADB-033 실제 client 인수의 순서·선행·동결 입력을 정한다. 구현될 파일·runner·명령이 존재한다고 주장하지 않으며 빈 package나 fingerprint/clean-export 경로를 추가하지 않는다. DEV6 driver 완성은 B3의 선행 조건이 아니다.

<a id="entry"></a>
## 1. 착수와 완료의 구분

| 단계 | 필요한 실제 입력 | 현재 판단 |
| --- | --- | --- |
| 설계·byte vector·모델 준비 | DEV4/COMM 채택 계약, 단계·제품 방향 | 이번 작업 범위. 코드 실행 증거 아님 |
| protocol/auth 모듈 구현 | D5-F01/F02 동결, 실제 DEV1/COMM 공급 API와 해당 leaf 회귀 | Q1 없이 stub으로 모듈 시험 가능하나 B3 통합 PASS 불가 |
| 실제 서버 조립 | 현재 revision의 S1→Q1, C1 실제 실행 증거; D4-F01/F02/F04/F05·C1 소유권/취소/clock/peer port 공급 | 기존 부분 구현·dirty 코드만으로 충족했다고 판단하지 않음 |
| B3 완료 | 아래 모든 필수 inventory·수치 profile 동결 및 SC03/KU14/PA08/PF11 실제 PASS, Q1/C1·하위 회귀, 독립 검토 | NOT_RUN; 미도달·미실행·UNKNOWN을 PASS로 변경 금지 |

DEV3의 mutable page·WAL·영속 undo·CLR·doublewrite·table SERIALIZABLE 공급을 실제 확인한다. DEV2 volatile/COW leaf나 mock SQL/echo ACK를 B3 durability로 대체하지 않는다. current-build recovery·restore와 과거 ACK 보존을 검증하며 사내 교차 버전은 NOT_REQUIRED_INTERNAL_FRESH_BUILD다. 안정 릴리스 때 VERSION_RELEASE가 추가된다.

<a id="freeze"></a>
## 2. 구현 전 동결할 leaf

모두 **OPEN_NOT_IMPLEMENTED**다. 설계에서 정한 의미·한도는 유지하고 실제 signature/입력/receipt와 실행 recipe를 아래 gate 전에 D2 검토한다. 코드 없이 그럴듯한 명령이나 byte 기대값을 완료 증거로 만들지 않는다.

| ID | 동결할 내용 | 닫는 시점·필수 산출 |
| --- | --- | --- |
| D5-F01 | XSQ1 body별 exact field order·handle/metadata/outcome enum·timeout prefix·버전 거절, 독립 byte fixtures | wire codec 구현 전. 정상/최소/최대/손상 literal bytes·기대 오류·allocation inventory; SQL1 MiB/bind8 MiB/UTF-8 경계 |
| D5-F02 | 실제 platform peer credential port, UID/principal map grammar·reload epoch, SQL authorization callback·DDL 단계·권한 효과 | auth/SQL adapter 구현 전. dependency/unsafe/feature 검토, 실패·revoke race vector, guard/thread 소유 가능성 |
| D5-F03 | request/worker/result/engine abort ownership·reserved cleanup slot·writer fence·shutdown supervisor | session/scheduler 구현 전. state transition table·lock order·heap/stack/FD 최악 계산·bounded model 전이/깊이 |
| D5-F04 | 독립 client/controller·SQL/native literal oracle·ACK history·socket/WAL/reopen fault hooks·receipt | 실제 server runner 구현 전. build/fault manifest, 각 cut reachability와 known-bad 검출, Q1/C1 공급 revision |
| D5-F05 | PF11 fixture·source/binary/profile/환경·worker/admission 규모·실제 recipe·사전 통계 계획 | baseline 실행 전. 아래 profile·paired order/seed·원시 sample·metric map·재현 bundle |

frame/UID/권한 callback의 실제 하위 계약이 부족하면 해당 소유자가 먼저 구현·검토하고 소비한다. server가 private fd·SQL AST 내부·engine WAL/undo 필드를 직접 읽는 우회를 만들지 않는다.

<a id="sequence"></a>
## 3. 구현 순서·책임 배치

| 순서 | 작업·소유 책임 | 인수 후 다음 입력 |
| --- | --- | --- |
| D5-I01 | protocol 모델·독립 byte decoder/fixture, XSQ1 codec·bounded reassembly | D5-F01·T01/02/13. AXF1과 logical-body 한도 분리 |
| D5-I02 | peer identity·정책 loading/reload·권한 callback | F02·T03/04. 인증 없는 SQL 효과 0과 DDL prior outcome |
| D5-I03 | session registry·request generation·worker scheduling·예약/cleanup 소유 | F03·T05/08/09/12, 모델과 synthetic budget |
| D5-I04 | DEV4 prepared/direct/fetch/metadata adapter·DDL/거래·stale 권한 재확인 | 실제 Q1/C1 공급 뒤 T06/07/10/11, mock 종료 |
| D5-I05 | start/recovery/listener·cancel/disconnect/shutdown·외부 ACK runner | F04·T03–13 전체, current-build disk reopen/재장애 |
| D5-I06 | PF11 계측·부하/원인 구분·비교·replay, 단계 통합 | F05·T14, 실제 하위 회귀·별도 검토·B3 판정 |

계획된 `crates/server` 내부에서 protocol, auth, session, scheduler, lifecycle, diagnostics 책임별로 코드를 둔다. `main.rs/lib.rs`는 조립/등록만 담당한다. 실제 path와 테스트 위치는 구현 착수 시 정하며 지금 Cargo·package·dependency·fingerprint·clean export 목록에 미래 파일을 넣지 않는다. 독립 client는 제품 driver를 정답 계산에 공유하지 않고 XADB-032의 이력·replay 기반을 사용한다. 제품 코드의 wire encoder와 oracle decoder를 공유해 거짓 PASS를 만들지 않는다.

단일 구현 담당+별도 검토를 기본으로 한다. 병렬 구현을 실제 선택하면 F01/F02 공통 계약을 먼저 통합하고 protocol/auth와 독립 oracle의 파일 소유를 분리한다. session과 scheduler가 같은 state transition을 따로 정의하지 않으며 공유 Cargo/README/JSON은 통합 담당만 최신본에 순차 반영한다.

<a id="acceptance"></a>
## 4. 필수 인수 행렬

모든 행은 **PLANNED_NOT_RUN**이다. 기본 fixture는 DEV4 D4-V01–25와 같은 DISK table/값이며 새 fixture에서 각 이력을 수행한다. expected rows·metadata·effect·state·certainty는 literal/독립 모델로 고정하고 production SQL·key codec·server 상태 기계를 정답 생성에 사용하지 않는다.

| ID | 필수 입력·반례 | 합격 조건·연결 |
| --- | --- | --- |
| D5-T01 | XSQ1 header/kind/version/flags·짧은 body·count/length overflow·UTF-8·NULL·Int64 min/max | 독립 byte vector 일치, 손상은 SQL 전에 거절, allocation 한도. SC03 |
| D5-T02 | 모든 header/body split·복수 AXF1 묶음·1 MiB SQL+envelope·8 MiB bind·fragment 256/257·중복 offset·중간 EOF/cancel | 전체 한 번 실행 또는 효과0, 실패 bind는 이전 binding 보존, reservation 회수. quota 거절 close와 semantic bind 오류 구분. SC03 |
| D5-T03 | peer UID/claimed UID 불일치·unknown principal·잘못된 DB·권한 없음·version mismatch·실제 owned Unix connect·두 server DB open | 인증 전 SQL/catalog/txn 효과0, writer lease 단독, pre-recovery Ready 불가. Unix mode/path 소유 검사, TCP SQL Unsupported. SC03 |
| D5-T04 | read/write/ddl/connect 각각 거절, prepare 후 revoke, fetch/Describe 재확인, reload 성공/손상/중복/epoch 경합 | 권한 선형화점 이후 승인한 동작만 완료; 다음 동작 새 epoch. DDL 앞 UPDATE는 prior=Committed·ddl=NotAttempted·PermissionDenied로 reopen 보존, prior commit 권한 거절은 Active 유지. SC03 |
| D5-T05 | session 교차 token·다른 connection·stale generation·ID duplicate/overflow·pipelining·Reply(N) 먼저/CancelAck(N) 나중 수신 | 다른 세션 영향0, 재실행0, request terminal 단일 확정, live/closed/tombstone quota 상한. client는 두 응답 후 N+1을 보내며, 완료 Ack ticket 회수가 늦었다고 정상 다음 Cancel을 거절하지 않음. SC03 |
| D5-T06 | prepare/bind/Describe/execute/fetch/close 전체, type/count/NULL/empty text/binary·잘못된 rebind·caller buffer 해제·N=0 | DEV4 metadata/NULL/원자 binding/handle lifecycle과 일치, 문자열 SQL 합성 mutant 거절. SC03·PA08 |
| D5-T07 | DROP/recreate·index 변경 후 execute, schema와 권한 동시 변경, old catalog 거래, 10,000회 result EOF/close/reexecute | dependency·권한 재확인과 세션 격리, 마지막 descriptor만 보존, stale 첫 효과0, pin/plan/bind 누수0. SC03·PA08 |
| D5-T08 | 32session/4worker 포화·global quota·output 10초 정지·partial RX trickle·Cancel flood | byte/slot/FD/pin bound 준수, 신규 SQL 생산 중단, control 진행, 다른 runnable 세션 진행, 실패 종류/기한/자원 회수 관측. SC03 |
| D5-T09 | queued/running/precommit/CommitAttempted/sync/publish/terminal/send의 cancel·timeout·FIN/reset | Reply(N)→CancelAck(N)→N+1/Cancel(N+1) 정상 trace와 늦은 Cancel(N)을 교차 검사. late cancel이 다음 request/COMMITTED에 영향0, Requested와 완료 구별, ABORTED/UNKNOWN/NotCommittedRecoveryRequired 정확한 mapping. SC03·KU14 |
| D5-T10 | ON/OFF/BEGIN·실패 S2 뒤 S1 commit·SELECT prefix 취소·DDL prior/ddl outcome·권한·undo 오류 | DEV4 D4-V05/16/21–24의 wire 결과와 native/literal/reopen 일치. 부분 statement/DDL 성공 위장0. PA08·KU14 |
| D5-T11 | 같은 table 충돌·독립 table 성공·unique 부분/전체 NULL·scan/index/force-scan·동시 schema | 선언한 Serializable history·D4-V11/17/25와 일치. 성공TPS·시도TPS/충돌률 분리. PA08 |
| D5-T12 | startup/recovery 실패·빈 서버/Active/result/TX 중 shutdown·Drop/panic·worker 포화 cleanup·기한 초과 | reply enqueue 뒤 COMM drain, 미commit undo 또는 recovery 소유, 재사용 금지/fence 정확, 자기 endpoint만 제거. grace 실패를 성공으로 표시하지 않음. SC03 |
| D5-T13 | 아래 crash cut·잘못된 server/client decoder·미도달 hook·빈/유실 ACK history·mutant | actual client ACK 보존, 확정 abort 비가시, UNKNOWN 원자성, 반복 reopen·row/index/catalog 일치, oracle가 known-bad 거절. KU14·PA08 |
| D5-T14 | below PF11 profile·queue/worker/WAL/output 경계·trace loss·계측 off/on·slow peer | 외부 latency와 내부 원인 연결, 비용/회귀 수치·독립 정확성·새 세션 replay 충족. PF11 |

### 4.1 필수 crash/fault cut

D5-T13은 최소한 다음 cut를 실제 current-build engine/server hook에 연결한다. 모든 cut는 controller가 도달 receipt를 받은 뒤 소유 candidate만 종료하며 독립 history/fixture는 밖에 둔다. hook 미도달·runner error·timeout은 PASS가 아니다.

| cut | 장애 시점 | 외부/복구 기대 |
| --- | --- | --- |
| D5-C01 | 완결 request decode 후 SQL admission 전 | SQL 효과0, terminal 미수신은 일반 client 관점 UNKNOWN일 수 있음 |
| D5-C02 | S1 성공·S2 변경/statement undo 중 COMMIT 전 | 해당 미commit 거래 전체 없음. S1도 소멸 |
| D5-C03 | commit record 시도 전·시도 중·sync 오류 | engine certainty별 판정, ACK 없음만으로 abort 단정 금지; 부분 row/index/catalog 없음 |
| D5-C04 | durable sync 후 publish 전/후, terminal 작성 전 | ACK가 없어도 committed 전체 복원. 두 하위 cut 각각 실행 |
| D5-C05 | reply enqueue 전/후·부분 frame·local flush 후 client terminal 수신 전 | transport 성공을 외부 ACK로 오인하지 않음, commit 보존, 자동 재실행0 |
| D5-C06 | 외부 client가 완결 COMMITTED/ABORTED를 history에 보존한 뒤 | COMMITTED 전부 보존, 확정 ABORTED 비가시; 두 번 reopen 후 동일 |
| D5-C07 | DDL precommit durable 뒤 native DDL 전·DDL durable 뒤 reply 전 | 각각 이전 거래만/이전 거래+DDL 보존, 두 identity/outcome 구분 |
| D5-C08 | cancel/disconnect cleanup·rollback CLR·recovery redo/undo 중 재장애 | 완료 cleanup만 ABORTED, 나머지 RecoveryRequired/Fenced, 복구 반복 후 원자성·ACK 보존 |

process kill, 파일 API short write/sync fault, 하위 전원 장애 모델을 별도 축으로 실행한다. 실제 장치 power-loss 시험을 process kill로 대체하지 않는다. K/DEV3 전체 fault inventory 중 영향받는 필수 회귀를 보존하며 이 8개 행만으로 하위 전체 인수를 충족했다고 하지 않는다.

### 4.2 모델·부정 사례·실제 동시성

D5-M01은 세션2·worker1·세션당 ordinary2·취소1·disconnect1, 축소 RX/TX/cleanup slot 각1의 유한 상태 모델이다. assembly/queue/run/commit-attempt/durable/terminal/TX/cleanup 상태, revoke epoch 2개와 shutdown을 포함한다. ID 재사용 금지·terminal 유일성·unauthorized effect0·정리 소유 보존·late cancel 격리를 검사한다. 독립 expected state와 전이 목록·탐색 깊이/상태 수 상한은 F03에서 실행 전 동결하고 유한 공간 완주 여부를 기록한다. cutoff를 PASS로 처리하지 않는다.

D5-M02는 output slot 포화+Cancel+shutdown의 wait-for graph를 확인한다. SQL worker가 TX 공간을 기다리지 않고 I/O owner가 worker slot을 기다리지 않는 불변식, cleanup 예약과 engine blocking 종료의 구분을 확인한다. 실제 OS I/O 종료의 실시간 상한을 모델로 증명하지 않는다.

known-bad 최소 집합은 auth 우회, prior DDL outcome 유실, stale handle 재활성, 잘못된 fragment 순서 수락, 일부 bind 교체, affected_rows를 COMMITTED로 표시, local flush를 ACK로 계산, sync 전 ACK, ACK된 행 유실, late cancel이 새 요청 취소, undo 실패를 ABORTED로 표시, TX 대기 중 worker 점유다. 모델·byte oracle·실제 process의 해당 검출 증거를 따로 남긴다.

DP03의 V2 property 64 seeds×최대128 operations, V3 256×최대1,024와 실제 thread stress 30분, 관련 protocol/parser target당 fuzz 30분 및 보존 corpus 전체, 비결정 fault cell 3회 새 fixture를 적용한다. 구현 전 seed/operation/환경을 동결하며 단순 반복 unit test를 해당 인수로 대체하지 않는다.

<a id="performance"></a>
## 5. PF11 사전 profile

기준 환경은 DP06의 첫 qualification 조합을 따르고 실제 kernel/CPU/장치/flush/ext4 mount·toolchain·lockfile·feature·release binary hash는 F05에서 고정한다. 공식 profile 미구현 상태에서는 현재 로컬 결과와 구별한다. workload 데이터는 새 fixture이며 row count/생성 seed/hash를 실행 전 등록한다.

| ID | workload·관측 질문 |
| --- | --- |
| D5-P01 | prepared PK lookup, client1/4/16/32, 결과1행; 외부 roundtrip과 queue·worker·SQL·output 비용 |
| D5-P02 | 독립 table writer와 같은 table 경합 writer, autocommit·명시 거래(16행), client4/16; 성공/시도 TPS·p50/p95/p99·conflict·WAL sync |
| D5-P03 | buffer4배 이상 DISK working set의 scan·ORDER BY/spill·128행 fetch, client1/4; client 수신속도와 server output/pin/disk 한도 |
| D5-P04 | 정상 client4+느린 reader1(10초 no-read), cancellation burst, 32session 포화; 정상 client 진행·거절·cleanup·quota peak |

P01/P02/P03은 off/상시/상세 계측을 구별하고 동일 의미의 release binary를 비교한다. 각 cell은 30초 warmup+60초 측정, baseline/candidate 순서를 seed로 AB/BA 무작위화한 **10 paired blocks**다. P02의 종료 시 확정 결과/충돌/UNKNOWN 수를 검증하고 fixture 상태를 block마다 재설정한다. raw 요청 latency·offered/admitted/completed rate·거절·timeout·CPU·RSS·queue/worker/output·I/O·trace loss를 보존한다. P03 행/byte 결과는 완결 수신 기준으로 센다.

상시 계측 비용 한도는 off 대비 성공 throughput 감소5% 이하 및 p99 증가10% 이하, 둘 다 paired block bootstrap 95% CI(고정 seed·10,000 resamples)의 불리한 끝이 한도 안이어야 한다. 해당 workload의 p99/throughput CI가 경계를 걸치면 INCONCLUSIVE다. 상세 trace는 비용을 보고하되 상시 모드 한도로 위장하지 않고 실제 진단/replay를 확인한다. 개선 주장은 별도 독립 confirmation 10 paired blocks에서 재검증한다. 반복 시행 중 유리한 block만 고르지 않는다.

P04는 정상 client가 각 1초 관측 구간마다 한 번 이상 완료하는지(5초 startup 제외, 60초), quota 상한과 output timeout 뒤 cleanup 결과를 확인한다. 이 값은 명시된 합성 부하/환경의 개발 인수이며 고객 SLA가 아니다. OS scheduling·engine stall로 실패하면 원인을 기록하고 사후 기준을 완화하지 않는다. cleanup의 정상 완료와 RecoveryRequired/프로세스 실패를 구분하고 후자를 누수 없는 정상 성공으로 세지 않는다.

PF11 통과에는 수치 비용뿐 아니라 진단이 실제 병목(queue, worker CPU, WAL wait, output 중 사전 주입한 원인)을 구분하고 동일 bundle을 새 세션에서 재실행하는 증거가 필요하다. 유효하지 않은 환경·원시 sample 유실·정확성 실패·미도달 경로는 PF PASS가 아니다. 목표 TPS/고객 latency SLA와 전체 OPS1 benchmark campaign은 별도다.

<a id="handoff"></a>
## 6. DEV6·후속 이관

DEV6에는 협상 capability, exact codec fixtures, parameter/result metadata, effect/certainty/DDL outcomes, request cancel races, timeout·buffer/handle 소유권, actual B3 server recipe를 넘긴다. native/JDBC/ODBC는 각각 실제 서버에서 검증한다. DEV5 independent client를 제품 driver 완료로 세지 않는다.

TCP SQL·TLS/인증서·원격 identity/password·객체별 ACL·multi-DB·LOB chunk·durable request lookup·session resume는 후속 계약과 실제 인수를 거친 뒤 capability에 추가한다. 현재 미지원이라는 사실로 작성 대상의 NET/SEC/SES/LOB 기능을 삭제하거나 PRODUCT_QUALIFIED의 누락을 N/A로 바꾸지 않는다. Unix peer 방식은 동일 OS 계정 범위이며 일반 원격 접근 제어 완료가 아니다.

<a id="work-record"></a>
## 7. 설계 작업 기록

| 항목 | 기록 |
| --- | --- |
| 분류·권한·담당 | DEV5-DESIGN-001 D2. 사용자 DEV5 설계 요청·DP01 위임. 작업/계약/통합 `/root` |
| 기준 identity | HEAD `e50e0793acf54c115d4715f71126f6ae45c74008`, dirty 파일51개. 사본 `/tmp/xadb-dev5-design-base-0za1z36k`, identity.json SHA-256 `a9d921f8b61998f51751603b2fdbc0e1735fe058bd689230b851f5dc97f290c9` |
| 독점 수정 | 새 dev5-server-design.md·이 계획·dev5-source-research.md |
| 공유 수정 | devdoc 및 contracts/plans/evidence README·documents.json, development-stages Markdown/JSON·stage-completion DEV5/SC03, KU14/PA08 design refs, 작성 목록의 DEV5 원문 안내 |
| 중복 확인 | DEV2 engine/transaction/durable·foundation/platform/network·transport·시험 및 기존 evidence/색인 dirty 변경 존재. 이 작업은 제품 코드·Cargo·기존 실행 evidence·구현 상태를 수정하지 않음. 공유 문서 최신본에 DEV5 항목만 순차 추가 |
| 순서·생성물 | 계약/소스 조사→구현/인수 계획→JSON/색인→별도 읽기 검토→지적 반영. 새 HTML/문서 검사 산출물 없음 |
| 동시 상태 변경 관측 | 작업 중 COMM 구현 기록·단계/패키지/색인의 PARTIAL_IMPLEMENTATION 변경을 관측. 이 변경과 진행 중 DEV2 제품 코드는 그대로 보존하며 C1 전체 증거로 승격하지 않음 |
| 검토 | `/root/dev5_design_review`: 새 문서3개·공유 문서12개의 DEV5 변경과 상위 계약·실제 source/baseline identity 대조. 취소 ACK/terminal 순서 지적 반영 후 채택 가능·잔여 필수 P1/P2 없음. `/root` 계약/통합 담당 반영 확인. 동시 COMM·제품 변경 승인 아님 |
| 검증 상태 | 실제 local source revision/origin·선택 코드·hash 확인. 제품/성능/모델/복구 실행 NOT_RUN. DOC-002에 따라 문서 자동 검사는 push 전용; 현재 실행하지 않음 |

설계 작성·검토 과정에서는 commit/push·배포·제품 시험을 수행하지 않았다. 설계 채택·실제 leaf 동결·제품 인수는 별개이며, 기존 FAIL·중단 기록을 다시 쓰지 않는다.
