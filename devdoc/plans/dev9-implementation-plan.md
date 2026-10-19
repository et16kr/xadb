# DEV9 — 제품 인수 구현·검증 계획

2026-10-19 · **DEV9-DESIGN-002 대응 / 독립 수정본 검토 PASS / leaf 동결·구현·SC16·SC17 NOT_RUN**

[상세 설계](../contracts/dev9-qualification-design.md) · [조사·검토](../evidence/dev9-source-research.md) · [단계 정본](development-stages.json) · [완료 계약](stage-completion.json)

<a id="entry"></a>
## 1. 착수와 인수 진입

XADB-046의 목록·계약·독립 모델·fixture 준비는 조기 착수할 수 있다. XADB-047의 제품 인수에는 실제 CLIENT1·SQL_EXT1·OPS1·P1과 해당 기능별 하위 gate를 요구한다. global index를 선언하면 GLOBAL_INDEX_QUALIFICATION도 필요하다. 이전 단계 설계 채택·다른 source의 PASS·이관한 시험·volatile substrate 결과를 현재 candidate의 제품 증거로 받지 않는다.

순서는 `소유 계약/요구 배정 → 세부 leaf·profile 동결 → independent oracle/negative checker → runner/packaging 연결 → 실제 public 경로 통합 → 현재 build 복구·운영·성능 → 독립 검토·인수`다. 제품 실행기와 package recipe는 아직 없으며 DEV9 `run_entrypoint=null`, SC16/17·PRODUCT_QUALIFIED·공식 배포는 NOT_RUN이다. 실행하지 않은 명령을 제시하지 않는다.

<a id="freeze"></a>
## 2. 구현·실행 전 동결 의무

다음 입력은 모두 **OPEN_NOT_IMPLEMENTED**다. 설계 채택은 literal schema·실제 환경·수치·corpus 동결을 뜻하지 않는다. 각 작업은 관련 F 입력이 동결되기 전 구현/인수 경계를 넘지 않는다.

| ID | 소유 | 동결할 입력 | 차단할 작업 |
| --- | --- | --- | --- |
| D9-F01 | 계약 소유자·DEV9 | 104개 부모의 모든 semantic leaf, 공개 grammar/API/overload/property/limit/문서 inventory, 적용성·D3 제외 근거 | SC16 |
| D9-F02 | DEV6/7/8/PART/NG·DEV9 | 지원 조합과 거절 조합·환경 값·동등성 분할·필수 다중 축 셀·수/분모·coverage closure | SC16·matrix 생성 |
| D9-F03 | XADBTest·build | baseline/candidate/profile/receipt literal schema, canonical digest·중복/크기/경로 규칙·artifact 역할·상태 전이, RunSlot 분모·candidate campaign/dispatch 원장·전체 attempt 대조 | aggregator·receipt 생산 |
| D9-F04 | 독립 oracle·기능 소유자 | typed expected·합법 history·오류 효과·각 case의 독립 assertion, known-bad·dialect mapping | 셀 correctness 실행 |
| D9-F05 | engine/server/RP·XADBTest | 실제 crash/fault cut·도달 witness·재장애 schedule·독립 ACK 보존 경계·연속 이력·prepared obligation·목적/branch/target별 복원 closure | 복구·전환·PITR 인수 |
| D9-F06 | 성능 담당·DEV8 | workload/data/부하·장비·FS/runtime·계측·한도·반복/기간·TPS/p99/RSS/I/O/RPO/RTO·통계/중단 기준 | candidate 성능 확인 |
| D9-F07 | XADBTest·platform | worker/event/artifact/oracle의 수치 예산·timeout·process/경로 소유·cleanup·비밀값 처리·원장/observer의 장애 분리·봉인·보존 | runner·대규모 campaign |
| D9-F08 | build/package·platform | DP06의 첫 배포 대상과 실제 Linux target/package·toolchain/lockfile/provider·설치 소유 manifest·rollback·재현 recipe·license inventory | 설치·공식 배포 |
| D9-F09 | SQL/client/운영 문서 소유자 | 지원표·한도·오류·실행 예제·runbook·release note와 case 연결 | R16-04·SC17 |
| D9-F10 | 통합·독립 검토 | evidence 신뢰 등급·승격 규칙·DP06 보호 실행/보존·blocker 범위·retest/무효화·버전 applicability·새 기준선 변경 절차 | PRODUCT_QUALIFIED·공식 사내 배포·출시 판단 |

소유자는 역할 표만 두지 않고 실제 구현 작업 계약에 담당 식별자·수정 경로·공유 변경·기준 지문을 등록한다. D2 범위의 routine 결정은 기존 위임으로 진행하며 제품 기능 축소·SLA·외부 출시만 해당 D3 판단을 적용한다.

<a id="sequence"></a>
## 3. 구현 묶음과 합류

| 묶음 | 연결 | 구현 책임 | 합류 조건 |
| --- | --- | --- | --- |
| D9-W01 | XADB-046 / SC16 | 기능 소유 계약과 public surface에서 leaf·variant·case·oracle 추적을 작성 | F01/02/04, 아래 104개 배정과 양방향 누락 0 |
| D9-W02 | XADB-046 / SC16 | baseline/candidate manifest·receipt validator·aggregator와 known-bad | F03/07/10, 빈집합·stale·누락·기준 변경 거절, R2-V01 |
| D9-W03 | XADB-047 / SC17 | 실제 engine/SQL/server/client/peer·운영 조합 runner | W01/02, 실제 선행 gate와 F05, T11–T22·R2-V02/03 |
| D9-W04 | XADB-047 / SC17 | Linux 설치/제거·fresh build·manual/runbook·version applicability | F08/09/10, T23–T28·R2-V04 |
| D9-W05 | XADB-047 / SC17 | 성능/capacity/soak와 최종 bundle·독립 판정 | W03/04, F06, T29–T34·P01–P05 |

공통 schema·계약 동결 뒤 기능 소유자별 fixture 작성은 분리할 수 있으나 최종 합류는 동일 candidate manifest에서 수행한다. 빌드/의존 배치 변경은 실제 구현 시 [저장소 정책](../policies/repository-build-plan.md)과 JSON을 갱신한다. 아직 없는 runner·crate·fixture 경로를 구현 목록에 추가하지 않는다.

<a id="coverage"></a>
## 4. 104개 요구의 소유와 DEV9 수신

아래 연속 ID 범위는 현재 [기능 정본](capability-inventory.json)의 모든 104개 부모를 정확히 배정한다. 부모 배정은 세부 leaf 동결이나 실제 시험 완료가 아니다. 기존 `required=true`, 보류·개별 source 조사 의무를 유지한다. DEV9 case는 소유 단계의 상세 case 전체를 수신하고 제품 조합을 추가한다.

| 요구 범위 | 수 | 기능 소유·필수 수신 | DEV9 추가 인수 |
| --- | --- | --- | --- |
| R01-01–12 | 12 | DEV4/7, SC02·SC07 | T11/12/14, SQL/DDL×driver×거래/복구 |
| R02-01–07 | 7 | DEV1/7, SC07·타입/함수 leaf | T11/12, 비교 문맥·NULL·overload·형 변환·metadata |
| R03-01–06 | 6 | DEV2/3/7, 실제 격리·statement/savepoint·XA | T13–T17, 동시 history·응답 유실·CLR·외부 의무 |
| R04-01–06 | 6 | DEV3 S1, 실제 page/buffer/space/temp/압축 공급 | T14/15/21·P02, 한도·DISK I/O·회수·복구 |
| R05-01–05 | 5 | PART P1와 DEV7 SQL 노출 | T14/18/21, routing/pruning/lifecycle×unique×복제 |
| R06-01–05 | 5 | DEV3/PART/DEV7, 선언 시 NG/global gate | T14/18, 제약·index lifecycle·table 전체 의미 |
| R07-01–11 | 11 | DEV7 SC08·객체별 leaf | T12/14/17, routine/trigger/sequence/queue·외부 효과 |
| R08-01–04 | 4 | DEV7 SC09·DEV6 LOB 경로 | T11/12/15/21, LOB/spatial·취소·restore·자원 |
| R09-01–09 | 9 | DEV6 SC04–06·DEV7 SC10/11 | T11–T13/17/22, native/C/JDBC/ODBC/도구·ABI/runtime |
| R10-01–05 | 5 | DEV7 SC10/11, adapter/capture 개별 보류 포함 | T17/19/22, 외부 routine/link/adapter·격리·원자성 |
| R11-01–07 | 7 | DEV8 SC12·H1/L1/M1와 해당 공급 | T16–T18/21, fencing·schema·sequence·failback·복구 |
| R12-01 | 1 | DEV2/3/8, SC13·offline/online/incremental/PITR | T15/16/17/20, closure·재장애·새 commit/재시작 |
| R13-01–05 | 5 | DEV5/8, SC03/14·보안 provider | T19/20/22/24, 인증/권한·TLS/key·감사·자료 노출 |
| R14-01–11 | 11 | DEV5/8, SC14·도구/관측·이관 leaf | T18–T22/27/28, 설정·운영중 업무·논리 이관·runbook |
| R15-01–06 | 6 | DEV3/8, SC15·PF08/14·P01–07 | T29–T32·P01–P05, 등록 workload·현재 candidate 확인 |
| R16-01–04 | 4 | DEV1/9, 아래 R16 상세·버전 정책 | T23–T28, 설치·재현·문서·버전 적용성 |
| 합계 | 104 | 선행 완료 조건 유지 | 누락/미분류/required 미통과 0 |

R16의 직접 인수 범위는 다음과 같다.

| 요구 | 필수 설계·실행 | 근거·판정 |
| --- | --- | --- |
| R16-01 | 일반 사용자 prefix 설치·다중 instance·시작/종료·실패 정리·uninstall·소유 파일/DB 분리 | F08, T23/24. package 설치 성공만으로 server 기능을 인수하지 않음 |
| R16-02 | clean source export·고정 build·fresh fixture·현재 build crash/reopen·재현 manifest | F03/08, T25. 이전 사내 DB 자동 삭제/변환 금지 |
| R16-03 | 개발/최초 stable/후속 same-major별 적용성·기준선 목록·현재 build 복구 유지 | F10, T26. cross-version N/A를 PASS로 계산하지 않음 |
| R16-04 | 지원 기능/차이/한도/오류 표·SQL/API 예제·운영 runbook·release note와 실제 case 연결 | F01/09, T27/28. 작성 목록만으로 완료하지 않음 |

<a id="acceptance"></a>
## 5. 필수 시험 묶음

아래 T01–T34는 실행 전 세부 case/variant로 확장할 인수 의무이며 모두 NOT_RUN이다. 각 묶음에 실제 입력·독립 oracle·도달 witness·통과 기준·담당을 F 단계에서 동결한다. production이 출력한 성공 문자열을 정답으로 삼지 않는다.

| ID | 시험과 독립 기대값 |
| --- | --- |
| D9-T01 | 104개 부모/공개 항목 양방향 추적. 부모/overload/API/property 하나 삭제·중복·미분류 시 SC16 거절 |
| D9-T02 | required leaf 0·case 0·빈 family·없는 oracle/owner·dangling reference·미동결 기준을 거절 |
| D9-T03 | 적용 조건식·지원/거절 조합·고위험 다중 축 coverage. 미구현 required를 N/A로 바꾸거나 pairwise로 숨기면 거절 |
| D9-T04 | baseline/criterion 변경 뒤 이전 결과, 다른 candidate·target·provider·feature·package를 섞은 결과 거절 |
| D9-T05 | receipt 누락/절단/중복/변조·미등록 schema·artifact hash/size 불일치·원시 측정 없음·종료 marker 없음 거절 |
| D9-T06 | exit 0이나 READY 로그만 있는 worker, 미도달 fault, skipped/timeout, expected error 뒤 잘못된 거래 효과를 PASS로 만들지 않음 |
| D9-T07 | FAIL→반복 후 PASS로 은폐, 탐색 결과 재사용, 실행 후 허용치 완화·구현자의 자기 승인 거절. 공통 worker/oracle 수정 뒤 새 candidate와 이전 PASS 혼합 거절; 동일 identity의 일시적 인프라 복원만 오염 closure 재실행 허용 |
| D9-T08 | controller/worker kill·event overflow·artifact ENOSPC·수집/봉인 중 장애. incomplete 판정과 소유 process/FD/path 회수 |
| D9-T09 | negative oracle corpus: 중복/유실 행·NULL/collation 오류·부분 commit·불법 history·prepared 유실·중복 apply를 거절 |
| D9-T10 | 보호 CI 미구현을 보호 실행으로 표기·위조 producer·과거 gate receipt 승계·N/A를 PASS로 집계하는 보고서를 거절 |
| D9-T11 | SQL/type/function/metadata·public API의 정상/경계/오류를 모든 지원 client/runtime에서 독립 typed vector로 대조 |
| D9-T12 | bag/order/tie·collation·NULL/빈 text·수치·datetime·LOB/spatial·CALL·batch/partial fetch의 의미와 자원 수명 |
| D9-T13 | commit/취소/timeout/close/연결 유실·재접속·pool 재사용. 외부 ACK 이력과 UNKNOWN 합법 결과, 자동 중복 실행 거절 |
| D9-T14 | DDL/DML/제약/partition/index·장기 snapshot·동시 statement 실패와 crash. 독립 관계/history와 구조 무결성 대조 |
| D9-T15 | WAL-before-page·undo/CLR·checkpoint/REBASE·예약 복원·회수의 모든 필수 cut, 복구 중 재장애 뒤 같은 합법 상태 |
| D9-T16 | offline/online/incremental backup·archive·PITR의 독립 closure. 실제 복원·새 commit·다시 crash/reopen, 손상/누락 거절 |
| D9-T17 | XA PREPARED/외부 결정×복제/backup/PITR×crash. vote/decision 보존·in-doubt 유지, clone endpoint 격리와 재접속 |
| D9-T18 | H1/L1/M1·fencing/승격/failback·schema/sequence·partition 이동·오래된 peer/ledger. 원점별 ACK·apply·conflict oracle |
| D9-T19 | 인증/권한 철회×prepared/cursor/job·TLS/key 교체×재접속. 과거 권한으로 금지된 새 효과를 허용하지 않음 |
| D9-T20 | backup/archive/WAL/undo/spill/export/diagnostic의 선언한 보호 범위·key closure·감사. 민감값 누출/키 누락·손상 거절 |
| D9-T21 | 긴 거래/retention/RP 지연·OOM/ENOSPC·query spill·큰 result·구조적 limit. backpressure·오류 효과·drain 후 live 자원 복귀 |
| D9-T22 | 설정 변경·start/stop·maintenance·외부 routine/link/adapter·취소. process/endpoint 격리와 정상 업무 동시 수행 |
| D9-T23 | 각 Linux package의 일반 사용자 설치/시작·다중 instance·운영 경로. 잘못된 ABI/runtime/권한/port의 사전 거절 |
| D9-T24 | 부분 설치/제거 실패·symlink/경로 탈출·기존 DB/backup/config·다른 instance 보호. 소유 manifest 밖 변경 0 |
| D9-T25 | clean export 두 번 build와 실제 artifact/의존/license manifest·새 fixture·현재 build 복구. 재현 주장에 맞는 digest 대조 |
| D9-T26 | 내부 fresh/최초 stable/이후 stable applicability를 별도 검사. 적용 시 모든 earlier same-major 기준선의 old→new·변환 crash·새 쓰기 복구 |
| D9-T27 | 공개 기능/오류/한도/설정/driver 문서와 inventory 양방향 대조. 일반 사용자 예제·runbook을 candidate에서 실행 |
| D9-T28 | 선언 source별 논리 이관·기능적 재작성 예제의 schema/data/권한/routine·누락/충돌/실패 보고. 독립 예상 업무 결과·restore drill |
| D9-T29 | 등록 workload마다 correctness·부하/환경 유효성·throughput/latency/resource/오류 기준을 독립 원시 자료로 재계산 |
| D9-T30 | warm/cold·Off/Always/Detailed·동시 운영 workload·fixed arrival/포화 campaign 분리. timeout/대기·재시도 누락 반례 |
| D9-T31 | RAM 초과 dataset·실제 I/O·모든 구조적 limit·soak/drain·retention/RP/backup 압력. 크기 축소·누출 은폐 거절 |
| D9-T32 | RPO/RTO의 장애 시작/외부 ACK/복구 업무 완료 cut·prepared 의무·원점별 손실 대조. READY만으로 RTO 종료 금지 |
| D9-T33 | 동일 candidate의 전체 분모·하위 gate·필수 셀·미결함·수치·문서·독립 검토를 재집계. 하나의 필수 비통과도 승격 차단 |
| D9-T34 | 제품 인수·공식 사내 배포·외부 출시 분리. 개발 N/A/로컬 검증/설계 채택이 출시 승인으로 승격되지 않음 |

<a id="review-002-cases"></a>
### 5.1 DESIGN-002 추가 반례와 수신

다음은 T 묶음의 의무를 구체화하는 추가 case family다. fixture·literal schema·실제 cut은 해당 F에서 동결하고 모든 세부 slot을 실행한다. 표의 예상 판정은 **설계 반례이며 실행 결과가 아니다**.

| ID·연결 | 반례·정상 대조와 필수 판정 |
| --- | --- |
| D9-R2-V01 · T02/05/07/08/33 | 비결정적 필수 3회 중 2회 PASS만 제출, 한 receipt를 다른 반복/shard에 중복 배정, 실패 attempt 또는 campaign 전체 삭제, worker 시작 뒤 controller kill·원장 끝 유실을 거절. 모든 기대 slot·등록 attempt가 대조되고 인프라 INVALID 대체 근거까지 남은 완전한 bundle만 수락. 실제 제품 FAIL 뒤 새 campaign의 PASS는 해소가 아님 |
| D9-R2-V02 · T15/16/17/32 | exact-cut/PITR에서 target 직전·정확한 commit·직후, 같은 timestamp·다른 branch의 같은 q, `backup Lend > target LSN`이지만 q는 같은 경우, target 미도달·뒤 source CLR/commit 혼입을 검사. 정확한 target 상태는 수락하고 잘못된 prefix는 거절. prepared XID·잠금/가시성·resolve 예약과 외부 격리를 확인. source DB/live WAL/control 접근을 차단한 상태에서 독립 closure 복원→새 ACK→재장애→재대조 |
| D9-R2-V03 · T08/09/13/15/17 | 대체 witness 없는 DB와 ACK observer 동시 유실, 관측한 성공의 기록 삭제, 뒤 UPDATE로 가려진 ACK 거래 유실, 첫 reopen에서 보인 UNKNOWN 효과가 두 번째 reopen에서 사라지는 history를 거절. 실제 무응답 요청의 합법 UNKNOWN과 관측 인프라 손실을 구분. 목표 cut 앞뒤 관측 witness·전체 연속 이력·prepared 효과를 갖춘 정상 대조 포함 |
| D9-R2-V04 · T10/23/25/34 | 다른 OS/FS·debug/fault artifact·로컬 grade·보호 job 부정 시험 누락·증거 보존 미구현인 결과를 첫 공식 사내 배포로 승격하는 입력을 거절. DP06 대상·동일 candidate release/V3·보호 권한·보존이 확인된 입력만 해당 승격 가능. internal fresh-build N/A를 버전 호환 PASS로 바꾸지 않음 |

<a id="faults"></a>
## 6. Fault·상태·실제 실행

DEV2/3/6/7/8·COMM/PART/NG의 해당 필수 case와 V2/V3 프로파일은 수신 의무다. DEV9에서 작은 신규 smoke로 줄이지 않는다. 새 parser/manifest/aggregator의 malformed·경계 입력, 실제 thread·process·file 효과와 negative fixture를 구현하고 적용 가능한 V2/V3 범위를 D2 검토로 고정한다. V3의 seed/operation·thread/fuzz 기간 등은 [DP03](../policies/development-policy.md#section-4)과 해당 단계의 더 구체적인 계약을 따른다.

fault 축은 allocation·quota·read/write/short write·sync/rename/dirsync·ENOSPC·timeout/cancel·process kill·복구 중 kill·network loss/delay·fencing/provider/key 실패·artifact 수집 실패다. stage owner가 실제 도달 지점을 공급한다. 각 셀은 새 소유 fixture로 실행하고 reached/expected cut·관측 결과·recovery image·cleanup을 저장한다. 비결정적 셀의 반복 수는 기존 프로파일 이상으로 사전 고정하고 seed/schedule/반복별 RunSlot에 배정한다. candidate 전체 campaign·dispatch·attempt를 [원장 계약](../contracts/dev9-qualification-design.md#attempt-completeness)으로 대조한다. 장애·PITR의 기대 상태와 observer 보존은 [복구 목적별 계약](../contracts/dev9-qualification-design.md#recovery-target)과 [관측 계약](../contracts/dev9-qualification-design.md#recovery-observation)을 따른다.

독립 검증은 (1) 외부 API 결과/ACK/history, (2) 논리 row/object 관계와 transaction 효과, (3) 구조/무결성·진행점·retention·자원, (4) 재시작/복원 이후 같은 검증으로 구성한다. production decoder의 self-consistency만으로 충분하지 않다. 독립 검사기의 메모리·임시 파일·시간이 한도를 넘으면 결과를 UNKNOWN/INVALID로 남긴다. 시험 인프라 실패를 제품 정상으로 바꾸지 않는다.

<a id="performance"></a>
## 7. 제품 성능·운영 profile

DEV9의 P01–P05는 DEV8 P01–P07과 PF08/14를 제품 후보에 연결하는 추가 묶음이다. 기존 수치·최소 기간·반복·sample 수·24시간 soak·자원 기준을 축소하지 않는다. workload별 추가 목표와 Linux 환경 matrix는 F06 미동결 상태이며 실행 전 결정해야 한다.

| 묶음 | 내용 | 필수 판정 |
| --- | --- | --- |
| D9-P01 | 등록 SQL/routine/driver·읽기/쓰기·운영 혼합의 fixed arrival와 포화 | 의미/부하 유효성, attempted/admitted/completed/durable success·오류/재시도·p50/p95/p99·backlog; workload별 기준 전부 |
| D9-P02 | DEV3 buffer4배 및 DEV8 process RAM1.5배 이상 별도 데이터·spill/space/limit | 실제 접근과 I/O·peak/live ledger·자원 거절 뒤 회복; 사이즈나 내구성 축소 금지 |
| D9-P03 | H1/L1/M1·backup/archive/PITR·XA/DDL와 업무 동시 수행 | ACK/protection/외부 의무·RPO/RTO·tail/throughput·retention·conflict를 각각 평가 |
| D9-P04 | DEV8의 각 필수 topology/mode별 24시간 soak와 시간별 drain, 반복 복구 | 기존 RSS slope·live 누출·tail/TPS guardrail, 시간 추세·실패/재시작·cleanup 기록 보존 |
| D9-P05 | 실제 배포 artifact와 계측 역할, baseline/candidate 최종 확인·새 세션 재현 | DP07 최소10쌍·독립 run/CI·다중 비교/중단 규칙, 탐색 분리·기준 자기 변경 거절 |

DEV8 기본 warmup60초·측정180초·10쌍 AB/BA, p99 최소 표본과 계측 비용 기준을 참조한다. 비교 불가·표본 부족·신뢰구간 경계 교차를 임의 PASS로 바꾸지 않는다. 설치/일반 기능 smoke를 24시간 soak로 늘리지 않고, 실제 장시간 의무가 있는 P04만 해당 환경에서 수행한다. competing build/stress와 시간/CPU/cache drift를 관측하고 사전 무효화 규칙을 적용한다.

<a id="completion"></a>
## 8. 완료 bundle과 현재 상태

최종 bundle은 baseline/candidate/profile manifest, clean source와 artifact hash, 104개 부모→확장 leaf→필수 셀/RunSlot 분모, candidate 전체 campaign/dispatch 원장, 실제 하위 gate receipt, 각 attempt의 원시 결과·ACK/history·fault 도달·복구/운영/성능 자료·cleanup, runner/oracle 부정 self-test, 설치/license/문서/이관 예제, 미결함 목록과 독립 검토를 포함한다. 비적용 항목과 근거·권한을 별도로 보존한다. 민감 자료는 접근 제어와 redaction 상태를 표시하고 원본 없는 요약만으로 필수 관측을 충족하지 않는다.

SC16은 전체 leaf/variant 동결 전 NOT_RUN이다. SC17은 같은 candidate의 전체 required 셀/RunSlot·실제 CLIENT1/SQL_EXT1/OPS1/P1와 해당 global gate·현재 build 복구·운영·성능·R16이 완결되기 전 NOT_RUN이다. 공식 사내 배포에는 [DP06](../policies/development-policy.md#section-7)의 첫 대상·보호 release/V3·보존 의무를 추가 적용한다. 기존 stage status, evidence_refs=[], run_entrypoint=null은 이번 설계로 바꾸지 않는다. F01–F10·T01–T34·R2-V01–04·P01–P05는 계획이며 실행 결과가 아니다.

현재 수정본의 독립 설계 검토와 채택 기록은 [2차 조사·검토](../evidence/dev9-source-research.md#review-002)에 둔다. DESIGN-001 당시 기록은 [최초 검토](../evidence/dev9-source-research.md#review)로 보존한다. 문서 검사는 pre-push 훅에서 실행하며 이번 편집 중 제품 시험·문서 검증기를 실행하지 않는다. 기존 DEV2/COMM 코드·실행 증거와 무관한 변경은 작업 범위 밖이다.
