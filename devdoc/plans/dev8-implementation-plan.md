# DEV8 — 복제·운영 구현·검증 계획

2026-10-19 · **DEV8-DESIGN-001 대응 / 독립 설계 검토 PASS / leaf 동결·제품 실행·OPS1 NOT_RUN**

[상세 설계](../contracts/dev8-operations-design.md) · [조사·검토](../evidence/dev8-source-research.md) · [단계 정본](development-stages.json) · [완료 계약](stage-completion.json)

<a id="entry"></a>
## 1. 선행 증거와 착수

설계·독립 모델·fixture 준비는 현재 가능하다. production 통합은 현재 source identity에 대한 DEV3 S1·DEV5 B3와 하위 DEV0/1/2·Q1/C1의 실제 공급 증거를 요구한다. client 전환·remote SQL/TLS는 해당 DEV6 CLIENT1 경로, LOB/PSM/XA/확장 타입은 DEV7 leaf, partition/global index 조합은 PART/NG의 실제 공급 증거를 추가한다. 계획 문서와 이관한 PASS는 실행 증거가 아니다.

순서는 `leaf 의미/형식/수치 동결 → 독립 모델·known-bad → native engine 공급 → RP/운영 구현 → 실제 server/client/peer 통합 → 현재-build 복구·자원·성능 → 독립 검토`다. XADB-042–045의 부분 완료와 DEV8 전체 OPS1을 구분한다. DEV8 runner는 아직 없고 `run_entrypoint=null`이며 예정 명령을 실행한 것처럼 쓰지 않는다. 초기 H1 성공으로 L1/M1·온라인 백업·보안·전체 workload를 완료하지 않는다.

<a id="freeze"></a>
## 2. 구현 전에 동결할 입력

F01–F10은 **OPEN_NOT_IMPLEMENTED**다. 아키텍처 결정을 미완료로 되돌리는 뜻이 아니라 실제 signature·literal byte·case/profile의 미구체화 의무다. 완료 전 `detail_expansion_required=true`를 유지한다.

| ID | 담당 경계 | 동결할 내용과 착수 차단 |
| --- | --- | --- |
| D8-F01 | engine | durable WAL/capture/snapshot/retention/apply/backup port; logical supplement의 mini-transaction 원자성; 추가 opcode·FILEMAP/undo/allocator·현재-build redo/CLR; RP pin을 포함한 REBASE/epoch/재장애 예약 산식·admission·source WAL 회수. 관련 engine 변경 전 |
| D8-F02 | transport/server/client | protocol profile·identity·fragment/checksum/length·credit·handshake·오류·commit ticket와 local outcome/protection mapping; literal vectors·허용 client별 C/JDBC/ODBC 상태. wire/driver 구현 전 |
| D8-F03 | RP/운영 | H1/L1/M1 topology, ConfigEpoch 전환·보호 peer 교체 cut, fence provider의 재시작 차단과 증명 검증, 원점/인과/충돌 domain·해결 정책·sequence·DDL 지원 조합. 역할/충돌 구현 전 |
| D8-F04 | capture/apply | spool checkpoint/manifest·진행점·source pin 인계·filtered/no-op·schema/LOB/prepared 상태·origin ledger·원자 apply·중복/compaction/reseed. capture/receiver 구현 전 |
| D8-F05 | backup/archive | offline/online·full/differential/cumulative, page copy와 allocation/drop/reuse, parent/dependency/key closure, archive seal/검증/retention, format·취소/재개·disk cap. backup 구현 전 |
| D8-F06 | recovery | target q/time·시계/index·complete WAL cut, base consistency 조건, parent/fork/new timeline·LSN high-water·prepared·반복 restore와 namespace atomicity. PITR 구현 전 |
| D8-F07 | security | TLS/KDF/crypto/key provider의 DP04 검토·버전·지원 matrix, principal/role/password/LDAP·certificate rotation/revoke·auditable effect/intent/outcome·redaction·nonce/key closure. security 구현 전 |
| D8-F08 | operations/tools | 설정 registry·job state·admin API/CLI/화면/exit code·view/schema·SNMP/exporter·진단·import/export/이관/비교·directory/service/container의 실제 명령·UX 인수 목록. 각 도구 구현 전 |
| D8-F09 | 검증/자원 | 30개 요구→operation/variant→생산 경로→독립 expected→case의 전수 목록, state model 유한 공간·fault reachability·negative checker, memory/disk/FD/task·cleanup 산식과 실제 측정. runner/관련 인수 전 |
| D8-F10 | 성능/통합 | 환경·release/feature·fixture/seed·workload·기간/반복·RPO/RTO/latency/RSS/I/O 예산·통계·계측 비용·경쟁 부하 금지·재현 recipe. 첫 측정 전 |

F01/F03에는 H1 `install_standby`/`follow_redo`의 source identity·active/prepared 보존과 **protected-XA의 PREPARED vote/coordinator decision/의무 해제 보호**를 포함한다. F04는 snapshot 이전 열린 거래·capture 활성화 cut, portable origin row/version mapping, tombstone와 retired ledger frontier를 동결한다. F05/F06는 DEV7 prepared-backup profile·일반 PITR clone의 외부 endpoint 격리·원 RM/TM identity의 안전한 인계를 요구한다. F07는 WAL/undo/archive/spill/진단까지 매체별 protection scope를 명시한다.

<a id="sequence"></a>
## 3. 구현 묶음과 합류

| 순서 | 주 작업 | 공급·완료 범위 |
| --- | --- | --- |
| D8-I01 | XADB-042/043 공통 engine port·retention·supplement·독립 모델 | F01/09, 실제 S1; T01–04·T18. 하위 native 복구·예약 선행 |
| D8-I02 | XADB-042 physical stream·remote durable ACK·H1 역할 전환 | I01·F02/03·실제 B3/C1, T05–11. H1 한정 인수 |
| D8-I03 | XADB-042 capture/spool·L1 snapshot/catch-up·logical apply | I01·F04, T12–17·T19. schema/type/LOB/XA별 실제 공급 |
| D8-I04 | XADB-042 M1 원점/인과·충돌·해결·병렬 apply | I03·F03/04, T20–22. optional 최적화도 정확성/순서 의무 유지 |
| D8-I05 | XADB-043 online/incremental/archive·PITR/restore | I01·F05/06, T23–29·RC-T24/25. source 접근 없는 복원·재장애 |
| D8-I06 | XADB-044 remote 보안·key/audit·설정/job·운영 도구 | F07/08, 실제 B3와 해당 client; T30–37. I02/I05가 실제 공급하는 관리 작업과 합류 |
| D8-I07 | XADB-045 workload/capacity·soak·PF08/14·전체 인수 | I02–I06 및 해당 DEV6/7/PART/NG, T38–40·P01–P07. 모든 필수 조합과 하위 회귀 |

공통 port·wire·format·결정 담당은 하나로 두고 소비 구현에서 복제 정의하지 않는다. 실제 병렬 개발을 할 때 [소유 정책](../policies/parallel-development-policy.md)에 따라 기준 revision+dirty digest, owned_paths, 공유 조립 파일 담당, 합류 순서를 먼저 정한다. 이번 작업은 설계 작성과 독립 검토이며 미래 구현자는 미배정이다. 제품 경로·crate는 구현할 때만 추가한다.

<a id="coverage"></a>
## 4. 30개 필수 요구 배정

`capability-inventory.json`에서 DEV8이 planned_stages에 있는 R11–R15의 모든 요구를 보존한다. 각 행은 책임 배정이며 전체 API/variant 작성 완료나 PASS가 아니다. BACKUP-01–08, REPL-01–12, SEC-01–09, OPS-01–11과 관련 TOOLS/PERF 항목을 F05/07/08/09에서 세분한다.

| 요구 | 주 책임·인수 | 설계 절·시험 |
| --- | --- | --- |
| R11-01 단일·다중 writer | XADB-042 / SC12 | [topology](../contracts/dev8-operations-design.md#topology), T06–11·T20/21 |
| R11-02 동기·비동기 | XADB-042 / SC12 | [ACK](../contracts/dev8-operations-design.md#ack), T05–08 |
| R11-03 충돌 | XADB-042 / SC12 | [동기화](../contracts/dev8-operations-design.md#synchronization), T20/21 |
| R11-04 초기/부분/재동기화 | XADB-042 / SC12 | [동기화](../contracts/dev8-operations-design.md#synchronization), T15/16/19 |
| R11-05 schema·sequence | XADB-042 / SC12 | [동기화](../contracts/dev8-operations-design.md#synchronization), T17 |
| R11-06 전환·failback·주소 | XADB-042 / SC12 | [전환](../contracts/dev8-operations-design.md#failover), T09–11 |
| R11-07 복제 관리 | XADB-042/044 / SC12/14 | [운영](../contracts/dev8-operations-design.md#operations), T33/34 |
| R12-01 백업·복구·PITR | XADB-043 / SC13 | [백업](../contracts/dev8-operations-design.md#backup)·[PITR](../contracts/dev8-operations-design.md#pitr), T23–29 |
| R13-01 계정·인증 | XADB-044 / SC14 | [보안](../contracts/dev8-operations-design.md#security), T30 |
| R13-02 역할·권한 | XADB-044 / SC14 | [보안](../contracts/dev8-operations-design.md#security), T30/31 |
| R13-03 전송·인증서 | XADB-044 / SC14 | [보안](../contracts/dev8-operations-design.md#security), T31 |
| R13-04 데이터 보호·키 | XADB-044 / SC14 | [보안](../contracts/dev8-operations-design.md#security), T32 |
| R13-05 감사 | XADB-044 / SC14 | [보안](../contracts/dev8-operations-design.md#security), T32 |
| R14-01 catalog·운영 view | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T34 |
| R14-02 설정·시작/종료 | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T33/37 |
| R14-03 interactive/batch CLI | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T35 |
| R14-04 logical export/import | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T36 |
| R14-05 이종 이관 | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T36; source별 schema/value/권한·routine 차이 명시 |
| R14-06 비교·선택 sync | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T36 |
| R14-07 plan·통계·진단 | XADB-044/045 / SC14/15 | [운영](../contracts/dev8-operations-design.md#operations), T34/40 |
| R14-08 cache 제어 | XADB-044/045 / SC14/15 | [운영](../contracts/dev8-operations-design.md#operations), T34/38 |
| R14-09 monitor·SNMP | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T34/37 |
| R14-10 보호된 진단 묶음 | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T34/35/40 |
| R14-11 directory·외부 tool | XADB-044 / SC14 | [운영](../contracts/dev8-operations-design.md#operations), T30/37 |
| R15-01 업무 corpus | XADB-045 / SC15 | [성능](#performance), T38·P01–07 |
| R15-02 throughput·tail | XADB-045 / SC15 | [성능](#performance), T38 |
| R15-03 구조적 한도 | XADB-045 / SC15 | [자원](../contracts/dev8-operations-design.md#resources), T39 |
| R15-04 큰 데이터·유한 자원 | XADB-045 / SC15 | [자원](../contracts/dev8-operations-design.md#resources), T18/39 |
| R15-05 혼합·운영중 workload | XADB-045 / SC15 | [성능](#performance), T38/39 |
| R15-06 사전 등록·판정 | XADB-045 / SC15 | [성능](#performance), T40·PF08/14 |

<a id="tests"></a>
## 5. 독립 시험과 실패 반례

다음 T01–T40은 시험 책임 묶음이다. 하나의 test function이나 전체 fault 셀 수가 아니다. 각 항목을 mode/topology/type/client/phase별 case로 펼치고 expected·생산 경로·reachability·원시 결과를 연결한다. 모든 실행은 NOT_RUN이다.

| ID | 실행과 독립 기대값 |
| --- | --- |
| D8-T01 | port/format identity: DB/timeline/epoch/profile·LSN·q·origin의 혼합/길이/overflow/잘린 fragment·digest 충돌을 효과 전에 거절; 알려진 literal bytes와 독립 decoder |
| D8-T02 | WAL+logical supplement의 모든 append/sync/mini-transaction cut. data만 또는 change event만 살아남는 mutant 거절; rollback/CLR를 업무 DML로 내보내는 오류 검출 |
| D8-T03 | retention 등록과 snapshot cut 사이 삭제 race, pin 전진 전 crash, 불명 selector old/new closure. 재시작 후 owner 복원 전에 unlink하는 mutant 검출 |
| D8-T04 | 네트워크 대기 중 commit/page/retention latch를 보유하지 않는 schedule, data credit 고갈 때 ACK/cancel/control 진행과 cleanup. 실제 thread·FD/task 종료 |
| D8-T05 | async·strict, receive-only/write-only/durable/apply별 ACK 차이. peer restart·OS cache·torn/missing dependency가 durable ACK를 통과하지 못함 |
| D8-T06 | local commit 후 remote wait timeout/cancel/disconnect. client별 local=Committed/protection 실패와 reply 유실 Unknown; rollback/자동 replay/async 성공 강등 0 |
| D8-T07 | 지정 synchronous peer 교체 도중 crash/partition. 기존 ACK cut 이전이 끝나기 전 ConfigEpoch 전환/새 peer 보호 ACK 금지 |
| D8-T08 | source와 target 동시 restart·연속 prefix 검증·duplicate/gap·old epoch 응답. 모든 관측 성공 ACK가 지정 durable copy에 존재 |
| D8-T09 | planned switchover와 failover: old primary 정지/재시작 금지 증명, 거짓 fence receipt·heartbeat만으로 승격·이중 writer 반례 거절 |
| D8-T10 | strict peer 상실·뒤처진 async peer·불명 ACK high-water·divergent suffix·failback. 손실/불명 표시, split-brain 없음, 잘못된 zero-loss 주장 거절 |
| D8-T11 | native/C/JDBC/ODBC 주소 전환·TLS/role 검사·응답 유실·기존 handle·진행 거래. 새 연결과 transaction retry 분리, 실제 driver evidence |
| D8-T12 | 장기 열린 거래·savepoint/statement rollback·LOB 조각·schema·prepared의 spool checkpoint. source 시작 WAL 삭제 뒤 commit/abort/restart 결과 동일 |
| D8-T13 | spool payload/manifest/sync/pin 전진/compaction 사이 crash. fallback closure·재개 위치 검증, 손상 spool+삭제 WAL은 NeedsReseed이며 거짓 재개 금지 |
| D8-T14 | 원자 apply+origin ledger+frontier의 모든 fault cut. 같은 transaction 재전송은 효과 1회, 다른 digest 정지; 효과/진행점 따로 commit하는 mutant 검출 |
| D8-T15 | snapshot C와 stream pin 원자 취득, 복사/설치/catch-up 중 쓰기·crash·취소. source C와 이후 이력으로 만든 독립 target과 동일 |
| D8-T16 | table/partition filter·세대 변경·cross-object 거래·FK/trigger dependency. 선택 효과의 원자성, 불완전 closure 사전 거절, gap/no-op 구별 |
| D8-T17 | DDL/drop-recreate·sequence·trigger·PSM·LOB/spatial·XA prepare/commit/abort. schema barrier·origin 보존·비거래 high-water·외부 효과 재실행 0; 실제 DEV7/PART/NG 공급 |
| D8-T18 | source WAL/capture/receive/archive full·receiver 정지·retention pin·반복 local crash. 정상 admission 차단과 예약된 rollback/recovery 진행; cleanup 뒤 소유 ledger 잔존 여부 |
| D8-T19 | 중단된 초기화·손상 target·긴 outage 뒤 reseed. old target/backup 보존, staging 해제, 검증 전 serving 금지 |
| D8-T20 | 2/3 writer의 분리된 영역·겹치는 key·update/delete·unique/FK·순환 전달·인과 역순. 원점별 모델로 missing/duplicate/conflict를 독립 판정 |
| D8-T21 | conflict-domain drain/fence·동일 resolution ID·새 ConfigEpoch·한 참여자 유실·해결 중 새 변경·재장애. 모든 affected replica 일치 전 reopen 금지 |
| D8-T22 | 직렬/병렬 apply 비교: dependency·schema/sequence/unique 경합과 head-of-line·starvation. 단순 정답 모델과 동일 origin frontier/효과; 최적화 비용은 별도 |
| D8-T23 | offline exact-cut 회귀와 online page image+LSN+FILEMAP. copy 중 갱신/새 page/재사용 시도·장기 거래·torn page·max pageLSN. full independent audit |
| D8-T24 | full/differential/cumulative parent chain·변경 추적 손상·page 재사용·free/할당/key 변화. 누락 parent 거절, 추적 불명 full scan, restore 결과 동일 |
| D8-T25 | archive write/sync/rename/manifest/read-back·동일 이름 충돌·누락/손상 segment·retention 삭제 경합. complete frontier에 hole 없음 |
| D8-T26 | q/time PITR: 동일 timestamp·clock 역행/jump·target 앞뒤 commit·uncommitted/부분 rollback·prepared. base end>target 거절, 명시 inclusive cut의 독립 결과 |
| D8-T27 | 새 timeline/LSN·CLR·control 선택과 반복 restore crash. source suffix 혼입/높은 pageLSN으로 redo 누락/epoch 무한 누적 mutant 거절 |
| D8-T28 | 원본 DB/WAL 차단, 별도 빈 환경 restore→새 ACK→두 번째 crash/reopen. backup만으로 closure 완결 확인; key/namespace/filemap 누락 거절 |
| D8-T29 | 손상 범위별 media rebuild와 RPO/RTO drill. 잘못된 파일/backup 혼합·부분 성공을 Complete로 보고하는 오류 거절; RC-T24/25 연결 |
| D8-T30 | account/role/LDAP/password lock·만료·revoke·owner/routine·bootstrap recovery의 허용/거절 matrix. 미인증 효과·권한 우회·secret 유출 0 |
| D8-T31 | 실제 모든 remote client/RP/admin의 TLS hostname/CA/EKU·만료/폐기·rotation·재접속·평문 downgrade 거절, 권한 변경 중 기존 작업 정리 |
| D8-T32 | key rotation/삭제·backup/spool/replica closure·tag/nonce·provider 오류·감사 intent/outcome crash·sink full/보존/조회/redaction. commit 뒤 audit 실패의 거짓 rollback 금지 |
| D8-T33 | config candidate 검증/준비/선택·restart pending·job resume/cancel·동일 ID 다른 입력·중복 제출. 일부 설정만 적용/완료 job 중복 실행 0 |
| D8-T34 | view/metric sample epoch·unit/reset/stale·lag 위치·cardinality·plan/stat/cache 권한·무효화·bundle redaction. 실제 상태/원시 이력과 공개 응답 일치 |
| D8-T35 | interactive/batch CLI·관리 화면의 실제 작업·취소·오류·진행률·machine output/exit code·재접속. 자동화된 API 성공만으로 화면 인수를 대체하지 않음 |
| D8-T36 | snapshot export/import·타입/NULL/encoding/LOB·객체 의존·routine/권한 이종 mapping·실패 행·partial batch·resume·cutover·선택 sync의 stale before-version 거절 |
| D8-T37 | SNMP/exporter/trap·directory SQL backend·외부 관리 tool·service/container volume와 secret·drain/kill/reopen. 실제 runtime·network path 인수 |
| D8-T38 | 모든 workload의 정답·ACK·durability를 먼저 확인하고 동일 의미 paired 측정. 각 cell의 오류/충돌/retry/timeout·p50/95/99·TPS·I/O/CPU/RSS를 보존 |
| D8-T39 | 구조적 capacity min/max/초과와 buffer 초과·RAM 초과 dataset·24시간 혼합 soak·취소/재시작. 제한 초과의 명시 오류·누출/무한 backlog/FD 증가 검출 |
| D8-T40 | PF08의 숨긴 원인 5개 성능 과제·PF14 장시간 회귀/기준 자기 갱신·잘못된 binary/recipe/미도달/누락 case. 보호된 독립 판정·새 세션 replay와 거짓 개선 거절 |

다음 교차 반례는 해당 묶음의 필수 variant이며 별도 선택 시험으로 생략하지 않는다.

| 연결 | 필수 반례·기대값 |
| --- | --- |
| T08/15/23 | H1 base에 active T의 변경을 복사→standby 설치/재시작→source COMMIT(T). T의 값 보존; 일반 restore의 loser undo/new timeline을 호출하는 mutant 거절 |
| T08/09/17 | RM PREPARED vote→다른 RM commit→primary 상실, coordinator decision→첫 participant dispatch→primary 상실. 지정 standby에 의무 closure 보존; 미지원 profile은 첫 효과 전 거절 |
| T12/15 | snapshot C 이전 시작/변경, C 이후 commit한 거래와 capture 최초 활성화. 전체 살아남은 변경이 정확히 한 번 적용; C+1부터만 읽는 mutant 거절 |
| T14/18 | retained/retired ledger 경계·source crash/reconnect·다른 digest·중복 payload. 검증 불가 과거 replay를 성공으로 합성하지 않고 RetiredHistory/공통 checkpoint/reseed로 처리 |
| T16/20 | source T7을 target T19로 적용한 후 source T8의 before=T7·동일 값 재쓰기·delete/reinsert. portable version mapping으로 정상 거래와 concurrent 충돌을 구별 |
| T23/26/27/28 | prepared 포함 backup의 profile 거절/수락, standalone PITR clone과 살아 있는 원 RM/TM. 자동 재접속/resolve/외부 재실행0; 검증된 fence·identity 인계 없이는 외부 admission 금지 |
| T32 | page 암호화 중 WAL/archive/temp/diagnostic의 평문 노출·restart nonce 재사용. 전체 protection 표면의 선언과 실제 bytes/권한 검사 일치 |

<a id="faults"></a>
## 6. Fault·상태 모델·oracle 의무

F09에서 최소 다음 축의 필수 조합을 등록한다. 무제한 Cartesian product를 모두 실행했다는 주장을 하지 않으며 제외한 셀은 계약상 불가능한 이유와 리뷰를 남긴다. 모드별 핵심 전이의 모든 crash cut과 필수 기능 조합은 생략할 수 없다.

- actor: source, receiver, apply, archive/backup job, fence provider, client, 외부 ACK observer.
- 장애: append/short-write·sync 오류/UNKNOWN·torn/잘린/누락/교체된 파일·rename/dirsync, process kill와 반복 restart, 양방향/단방향 partition·drop/duplicate/reorder·느린 peer, quota/FD/OOM 경계·cancel·시계 변화.
- 순서: local commit↔remote ACK, peer 교체↔promotion, spool↔pin↔GC, snapshot↔DDL↔catch-up, apply↔ledger, page copy↔WAL/archive, restore branch 선택↔CLR, key/audit↔commit.

독립 reference는 `(origin, q, schema, value)` 이력·ACK 관측·허용 cut·충돌 정책으로 결과를 계산한다. physical pageLSN이나 production redo 성공을 정답으로 사용하지 않는다. row↔index/unique/catalog/allocator/undo full audit와 별개로 외부 결과를 비교한다. physical source와 target이 같은 bug를 재현해도 동일 출력만으로 PASS하지 않는다. 알려진 잘못된 ACK·조기 pin 해제·ledger 선행·누락 WAL·잘못된 timeline·거짓 fence·은폐된 timeout을 각각 checker 부정 입력으로 둔다.

유한 모델은 최초 `노드 3(primary/지정 standby/async), 거래 2, ConfigEpoch 2, crash 2회, link up/down`을 상태 변수로 하며 ACK/승격/재시작 전이를 전수 탐색한다. logical 모델은 원점 2·key 2·transaction 2·rollback 1·중복/재정렬·conflict resolution을 포함한다. spool/backup 모델은 old/new manifest·payload durable 여부·pin/GC·재장애를 포함한다. 정확한 reachable state/transition 수와 상한은 F09에서 동결한다. sample seed를 전수 탐색 PASS로 부르지 않는다.

V2는 고정 64 seed×최대128 operation과 보존 corpus·경계 고정 사례, V3는 256 seed×최대1024 operation, 영향받는 real-thread suite와 parser/codec target마다 각각 최소30분을 적용한다. 비결정적 fault 셀은 새 fixture로 최소3회 반복한다. fault 미도달·시간 초과·receipt 누락은 PASS가 아니다. 24시간 soak는 별도 P05이며 30분 thread/fuzz를 대신하지 않는다.

<a id="performance"></a>
## 7. 성능·용량·장시간 profile

P01–P07은 최초 engineering acceptance 목표다. 제품 SLA·성능 실측이 아니며 실행 전 F10에서 실제 하드웨어·FS·device flush 의미·CPU/kernel·network topology/지연·release binary·feature·client·fixture와 모든 수치를 고정한다. DP06의 첫 배포 대상과 개발 host 관측을 구분한다. 필요한 환경/공간이 없으면 NOT_RUN으로 남기고 작은 결과로 대체하지 않는다.

| ID | 사전 workload | 필수 수치·판정 |
| --- | --- | --- |
| D8-P01 | H1 async/strict, 1/8/32 session, 1 KiB·64 KiB mutation, 1 GiB DB·64 MiB buffer | 외부 성공 ACK 누락0·중복0, 실제 disk witness·queue/remote wait 측정. mode 간 추가 비용을 공개하되 서로 다른 내구성 결과를 개선 비교로 쓰지 않음 |
| D8-P02 | L1/M1, 장기 거래와 짧은 거래 병행·LOB·DDL/sequence·receiver10분 중단 후 catch-up | spool cap·원점 순서/충돌 정답 충족, 정지 중 성장/압력 거절 관측. 재개 후 1 GiB backlog를 10분 안에 해소하는 최초 목표; 미달은 별도 원인 기록 |
| D8-P03 | 1 GiB full·incremental·archive/PITR, 일반 DML과 online backup 병행 | 독립 restore 정답·closure 누락0. 같은 fixture 로컬 restore/readiness 120초 이내, peak staging/FD/메모리 ≤등록 cap. 처리량·backup 영향은 각각 보고 |
| D8-P04 | H1 planned/unplanned 전환, source/peer 동시 장애·failback | strict 성공 ACK의 RPO=0; async는 외부 ACK 집합과 선택 cut의 정확한 손실/불명 표시. local harness에서 fence 요청부터 새 client query 성공까지 RTO60초 이내. fence 불가 셀은 승격 거절이 정답이고 RTO 충족으로 세지 않음 |
| D8-P05 | 등록한 각 핵심 topology/mode에서 24시간 OLTP+DDL/backup/archive/복제+주기 취소/restart | 지속적 논리 owner/FD/task 누출0. 매 1시간 고정 drain 뒤 ledger 기준선 복귀; 마지막6시간 RSS slope≤1 MiB/시간·동일 구간 p99/TPS 악화≤10%. allocator high-water와 live charge 별도 |
| D8-P06 | capacity와 RAM 초과 data, 실제 buffer spill·긴 transaction·큰 result·LOB·partition | dataset ≥buffer4배인 기준선 및 별도로 ≥해당 DB process 사용 가능 RAM1.5배. 실제 read/write·peak·backpressure·cleanup, 데이터 크기 축소로 RAM 초과 시험을 통과 처리하지 않음 |
| D8-P07 | Off/Always/Detailed·원인 진단·보호된 baseline/candidate 비교 | Always CPU/median latency 악화≤5%, sample-p99 악화≤10%; 상세 비용 별도. acceptance event loss0, 새 세션 bundle replay, PF08/14의 거짓 개선/기준 변경 거절 |

P04 async 손실 허용치는 장애 셀마다 source의 외부 ACK cut·candidate durable cut과 장애 시간/부하를 사전 정의한다. async라는 이름만으로 시간 RPO 상한을 보증하지 않는다. Unknown cut을 수치0으로 대체하지 않는다. multi-writer는 원점별 RPO와 충돌/해결 상태를 보고하고 local success를 전역 convergence로 계산하지 않는다.

성능 측정은 warmup60초·측정180초·10쌍 AB/BA를 초기 기본으로 한다. 비교 대상은 같은 격리/내구성·보호·결과 의미이며 optional 최적화는 DP07에 따라 사전 선택한 주 지표·paired95% CI와 모든 guardrail을 사용한다. throughput 분모에 오류·재시도·timeout을 보존하고 completed-only latency 외 queue/timeout 분포를 별도 보고한다. run당 완료 operation 10,000개 미만이면 p99를 NOT_ESTIMABLE로 표시하고 사전 profile 재설계 전 인수를 닫지 않는다. soak의 시간 추세와 성능 비교 표본을 혼합하지 않는다.

원시 timestamp·count·histogram·CPU/I/O/RSS/FD·ownership ledger·복구 시간·계측 mode와 경쟁 process를 저장한다. 측정 중 경쟁 build/stress를 금지한다. 장비 간/서로 다른 DBMS의 수치는 의미·schema·driver·내구성 차이를 먼저 기록하고 단순 TPS 순위로 설계 선택을 정당화하지 않는다. 외부 DBMS 제품 실행은 이번 설계에 포함하지 않는다.

PF08은 CPU·대기·queue·불필요 작업·I/O의 숨긴 원인 과제 5개에서 재현→구분 실험→후보→독립 확인을 기록한다. PF14는 장시간 tail/backlog/메모리/복구 회귀와 candidate의 기준 자기 갱신을 검출한다. 탐색과 최종 확인 run을 분리하고 baseline/threshold/oracle 수정 권한을 candidate와 분리한다. 올바른 기능을 제거하거나 sync를 생략한 후보는 빠르더라도 거절한다.

<a id="completion"></a>
## 8. 완료 bundle과 남은 의무

bundle에는 source revision+dirty digest·clean export·binary/runtime/feature·fixture/seed/oracle/profile hash, 30개 요구와 실제 expanded variant, case→production 경로, 외부 ACK·원시 이력·fault reachability·전체 결과, 하위 gate receipt와 독립 검토를 포함한다. 표에 예정 expected를 적은 것은 실행 receipt가 아니다. 실제 실행한 native/driver/fence/provider 환경과 모사한 환경을 구별한다.

OPS1은 SC12–15, RC-T24/25, PF08/14와 실제 기능별 DEV6/7/PART/NG 조합의 모든 필수 leaf를 만족해야 한다. F01–F10의 미동결, H1만 구현, online/incremental/M1 미실행, TLS/키/감사/provider 누락, 미측정 RPO/RTO·soak와 부정 checker는 전체 완료를 막는다. DEV7 R10의 보류를 이 단계 결과로 해제하지 않는다. 고객 릴리스·SLA는 별도 판단이다.

문서 자동 검사는 [pre-push](../policies/documentation-policy.md#push-gate)에서만 수행한다. 이번 문서 작성 중에는 실행하지 않는다. 제품 코드·fixture·runner를 추가하지 않았으므로 제품 시험도 NOT_RUN이다.
