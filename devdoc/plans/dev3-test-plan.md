# DEV3 — 단계별 필수 테스트와 구현 동반 테스트 추가 계획

2026-10-22 · **D3-TEST-PLAN-001 D2 ACCEPTED / 독립 검토 완료 / 신규 테스트 코드·제품 실행 없음**

[개발 문서](../README.md) · [구현 단계 S0–S5](dev3-implementation-plan.md#sequence) · [16개 상세 설계](../contracts/dev3-task-designs.md) · [현재 구현·잔여 인수](../implementation/dev3-implementation.md#dev3-work-breakdown) · [사용자 실행 기준](../../AGENTS.md)

**중간 단계는 변경 기능의 필수 테스트만 실행하고, DEV3 최종 인수 S5에서 DEV3 전체 테스트를 수행한다.** “추가 테스트”는 각 기능 구현과 함께 새로 작성할 테스트·fixture·독립 기대값·검증 도구를 뜻한다. 선택적인 추가 시험이라는 뜻이 아니다. 새 테스트를 작성하는 시점과 큰 조합을 실제 실행하는 시점을 구분한다.

이 계획의 단계는 기존 DEV3 내부 S0–S5이며 제품 완료 gate S1과 다르다. 3.01–3.16은 작업 ID다. 현재 이미 완료된 표현·모델 시험은 원래 source·범위의 증거로 유지하고, 새로 계획한 시험의 실행 결과는 아직 없다. 본 요청으로 제품 개발·테스트 실행을 재개하지 않는다.

<a id="selection"></a>
## 1. 중간 단계의 실행 선정

| 상황 | 실행할 필수 범위 | 실행하지 않을 범위 |
| --- | --- | --- |
| 새 기능·새 상태 전이 | 아래 작업별 필수 사례와 그 기능의 직접 producer→consumer 연결 | 같은 단계의 다른 작업, 기존 하위 leaf 전체 |
| 수정·결함 보완 | 수정 전 반례, 수정한 분기의 정상·경계·실패 및 직접 영향 사례 | 관련 코드가 같은 기존 완료 사례 |
| 공통 API·형식·소유권 변경 | 실제 의미가 바뀐 소비 경로의 사례와 해당 수명/compiler probe | 소비자라는 이유만으로 해당 crate 전체 |
| 테스트·oracle·harness 추가/수정 | 새 테스트와 변경 checker의 정상 입력·known-bad·경로 미도달 거절 | 기존 제품 suite를 도구 시험의 대용으로 실행 |
| 문서·목록·계획만 수정 | 읽기 검토. 문서 자동 검사는 push gate에서만 | 제품 시험·compiler·Clippy·문서 verifier의 편집 중 실행 |
| 중간 묶음·commit·병렬 통합 완료 | 새로 연결하거나 수정한 경로만 | workspace·전체 leaf·DEV0/1/2 wrapper, source 전체 hash 변경만을 이유로 한 반복 |
| 최종 S5 인수 진입 | [DEV3 최종 inventory](#final-suite)의 전수 실행 | DEV3와 무관한 workspace 전수로의 확대 |

실행 전 `마지막 관련 검증 source → 실제 코드 diff → 영향 불변식 → 정확한 case/variant`를 한 줄씩 연결한다. 마지막 commit이 아니라 **그 사례를 마지막으로 검증한 source**와 비교한다. 기존 구현에 남아 있던 미검증 변경은 최신 diff가 비어 보여도 원래 source 차이와 미실행 의무를 근거로 선정한다. 새 테스트는 자신의 코드·기대값·경로 도달도 검증하며 이전에 통과한 시험의 반복으로 대신하지 않는다.

[DOC-004 push gate](../policies/documentation-policy.md#doc-004)의 workspace Clippy는 push 직전의 정적 검사다. 테스트 코드를 컴파일·검사하지만 기능 시험을 실행하지 않으며 아래 중간 단계의 제품 테스트 목록에 전체 suite를 추가하는 의미가 아니다. 작업 중에는 변경 package/target의 필요한 정적 검사만 선택한다.

아래 “중간 필수”는 해당 작업을 처음 구현할 때 닫아야 할 최소 기능 경계다. 작은 후속 수정마다 행 전체를 재실행하는 목록이 아니다. 바뀐 제한에는 경계−1/동일/+1을, 새 실패 상태에는 상태·효과·소유권 확인을 포함한다. 준비 중 무효과 거절과 write 시도 후 부분/불명·fence는 서로 다른 결과이므로 하나의 임의 I/O 실패로 합치지 않는다. 발견한 결함의 재현 시험은 그 수정과 함께 필수로 추가한다.

구현 전 미동결 형식·API·자원 예약은 그대로 선행 의무다. 필수만 실행한다는 이유로 F05 계산, commit/abort 성공 전 영속 장벽, 실제 I/O·실제 thread의 핵심 확인을 생략하지 않는다. 아직 없는 상위 복구/공개 기능은 해당 후속 단계의 미완료 의무로 남기고 하위 구현의 완료 범위를 제한한다.

기존 계약이 특정 모델·property를 해당 effect의 선행 증거로 요구하면 **그 변경 대상의 최소 고정 profile**은 중간 필수에 포함한다. 새/수정 property의 해당 V2 profile은64 seed·최대128 operation이며, 변경한 bounded model의 완료에는 선언한 유한 공간 완주가 필요하다. 변경 parser/codec target의 기존 보존 corpus도 해당 V2 중간 필수이며 그 target의 corpus 전체를 실행한다. 작은 known-bad로 checker를 검증한 것을 모델·property·codec 전체 PASS로 승격하지 않는다. 이를 이유로 무변경 M01–M06 전체나 다른 property/parser target을 반복하지 않으며 30분 fuzz·확대 corpus 등 최종 profile과 구별한다.

<a id="stages"></a>
## 2. 단계별 필수 실행 목록

각 단계에서는 **그때 새로 작성·수정한 작업에 해당하는 항목만** 실행한다. 예를 들어 S2에서 WAL 한 분기를 고쳤다고 S2 전체 allocator·buffer·tree 시험을 다시 돌리지 않는다.

| 단계 | 중간에 실행할 필수 테스트 | 구현과 함께 추가할 테스트 산출물 | 최종 S5에 모아 실행할 범위 |
| --- | --- | --- | --- |
| S0 계약·oracle | 새/수정 oracle의 독립 정상 입력과 대표 known-bad, 잘못된 결과·미도달을 PASS로 처리하지 않는지 확인 | literal bytes·typed 이력·edge multiset·자원 회계 기대값, case별 선택·결과 기록. 제품이 없으면 oracle까지만 판정 | 전체 mutant·전체 모델 fixture·최종 source/binary 결합 검산 |
| S1 표현 leaf | 변경 codec의 독립 정상 vector, 길이/한도 경계, 손상·identity·tag 거절, 해당 target의 기존 보존 corpus, 변경 borrow/권한 API의 compiler probe | NT01 및 해당 소비자의 새로운 표현·semantic vector | 모든 지원 표현 조합·전체 target corpus·30분 fuzz/확장 property profile |
| S2 실제 기반 | 새 파일/할당/WAL/flush/tree/undo 경로의 정상·경계·핵심 실패, 실제 Linux I/O와 좁은 연결, 자원/guard 반환 | NT02–06·NT09·NT10의 기반 부분. 실제 effect 순서·pin/예약·thread barrier 기록 | 모든 batch/profile/fault 조합·큰 working set·장시간 부하 |
| S3 거래·영속 통합 | 새 row/index/catalog의 statement rollback, unique, 두 격리 수준의 핵심 충돌, commit ACK/UNKNOWN, 해당 복구 장벽·reopen | NT07–12, NT15의 audit 연결. 외부 ACK 이력과 source별 복구 fixture | 다중 객체 전체 조합·전체 fault cut·thread stress·모든 복구 profile |
| S4 checkpoint·회수·backup | capture 누락/선택 보호, 회수 전 보존/해제 후 재사용, begin-WAL 제거 뒤 undo, 작은 exact-C backup→restore→새 ACK→reopen | NT13–15와 NT03의 free/reuse 연결, NT16의 계측·큰 fixture 준비 | 전체 보존 조합·장기 churn·대형 backup·규모별 회수·PF09 |
| S5 최종 전수 인수 | DEV3의 확정된 전체 inventory를 최종 후보에서 실행하고 독립 판정 | NT16의 전수 선택표·집계·재현 bundle 완성 | 별도 후속으로 미루지 않음. 필수 미실행/미해결 FAIL이면 DEV3 미완료 |

기존 [우선 실행 묶음 1–5](dev3-implementation-plan.md#work-priority)은 작업 선택 순서로 유지한다. S0의 기대값 준비는 모든 단계에 동행하고, 3.10.1 guard는 소비자보다 먼저, 3.12의 fault 입력은 각 쓰기 경로와 함께 준비한다. 단계 번호를 기다리느라 보호 장치를 늦추지 않는다.

<a id="task-tests"></a>
## 3. 16개 작업별 필수 목록과 새로 추가할 테스트

`NT01–NT16`은 이 계획의 **테스트 개발 묶음 ID**다. 현재 존재하는 Rust test 이름·runner·신규 파일 경로가 아니다. 실제 구현 때 기존 시험에 동일 보장이 있으면 재사용하고, 빠진 사례·fixture·관측만 추가한다. 한 시험이 여러 의무를 충족하면 같은 source/입력/판정 결과를 연결하고 ID마다 같은 DB 실행을 복제하지 않는다. 각 상세 설계의 전체 인수 표는 최종 inventory의 원문이며 아래 중간 목록으로 삭제되지 않는다.

### 3.1 표현·파일·할당·WAL

| 작업·주 단계 | 중간 필수: 해당 구현/변경 때 실행 | 구현과 함께 추가할 테스트 | S5에서 실행할 전체 범위 |
| --- | --- | --- | --- |
| [3.01 형식·정체성](../contracts/dev3-task-01-formats.md), S1 | 수정 codec의 독립 literal·해석 값, 길이/최대/overflow, checksum·reserved·unknown tag, 다른 namespace/owner/generation 거절. 권한/borrow API 변경 probe | **NT01**: 새 kind/body/opcode마다 literal과 hash를 맞춘 의미 손상 vector, checked 산술 경계, raw bytes로 변경 권한을 얻지 못하는 compiler 사례 | 지원 codec/semantic 결합 전체, 잘림·손상 위치 변형, 보존 corpus와 parser별 확장 profile |
| [3.02 파일·읽기](../contracts/dev3-task-02-files-reads.md), S2 | 선택된 파일의 정상 read, path 충돌·열린 handle identity/길이 불일치, short read/오류 후 fence, 검증되지 않은 후보 거절. 새 실제 I/O에는 Linux 확인 | **NT02**: 독립 namespace/FILEMAP fixture, handle 교체·부분 read·selector 실패 입력, 실제 호출/bytes·lease 해제 관측 | 선택·길이·file/extent/profile 전체 변형, read 비용·FD 경계와 큰 manifest |
| [3.03 할당](../contracts/dev3-task-03-allocation.md), S2/S4 | 현재 3.03.1은 [잔여 필수](#allocation-first). 이후 각각 token/generation/INIT 원자 인계, growth 실패의 실제 bytes/소유 보존, 최신 eligibility 후 free→새 generation·INIT 및 stale 참조 거절 | **NT03**: native adapter·compiler probe·64KiB 증명, reserve/consume/cancel 독립 ledger, INIT와 FILEMAP 인계 cut, free/reuse·MAX generation fixture | allocator 상태/owner·growth·reuse fault 전체, 모든 profile과 실제 inventory 4배 |
| [3.04 WAL](../contracts/dev3-task-04-wal.md), S2 | 새 opcode의 before/after·owner chain, 준비 실패 무효과, stage 뒤 partial apply fence, LSN 순서, short write/sync 불명의 effect·certainty와 slot 소유 | **NT04**: opcode별 literal semantic 그룹과 잘못된 delta, interleaved owner 입력, append/apply/DPT 등록 barrier 및 I/O effect recorder | opcode·frame 경계·owner 교차·tail/corruption·fault cut 전체. Commit 공개는 NT11, CLR 의미는 NT09와 연결 |

### 3.2 buffer·tree·index·catalog

| 작업·주 단계 | 중간 필수: 해당 구현/변경 때 실행 | 구현과 함께 추가할 테스트 | S5에서 실행할 전체 범위 |
| --- | --- | --- | --- |
| [3.05 dirty·flush](../contracts/dev3-task-05-buffer-flush.md), S2 | dirty 첫 recLSN과 redirty 보존; WAL sync→DWB image sync→pending→home sync→전체 retirement 순서; 두 file 중 부분 실패에서 pending 보존; reuse 금지; 작은 buffer에서 flusher 진행 | **NT05**: effect 순서 독립 trace, redirty/copy barrier, 다중 file pending·retirement fault와 오래된 image 재적용 mutant, bounded 진행 관측 | 설계 3.05-C01–3.05-C16의 해당 전체 변형, batch1/8/64×profile×fault, eviction/oldest/WAL-service·장시간 pressure와 PF09 |
| [3.06 tree·행](../contracts/dev3-task-06-tree-rows.md), S2/S3 | empty→insert/search, split/root 변경·삭제 경계, inline↔overflow, row의 before/after·undo 연결, split/delete 사이 cursor 순서·누락/중복, locator stale 거절 | **NT06**: 독립 ordered tuple/history, split/compaction/overflow fixture, logical row identity와 statement token, cursor 재탐색·caller guard 수명 시험 | T06-01–T06-16과 선언한 전체 변형, 최대 row·tree 높이·이동/overflow 손상·4배 row/locator |
| [3.07 index](../contracts/dev3-task-07-indexes.md), S3 | row↔presence/scan 일치; PK NULL; 부분 NULL 중복 거절·전체 NULL 허용; statement key 교환/최종 중복·이전 statement 보존; build 성공과 취소/실패의 staging 소유 | **NT07**: 독립 key→row 기대값, unique/key-lock barrier, statement-final 결과, offline build·cleanup·drain 중 flusher 진행 입력 | 전체 index 수/키 경계·point/range/full scan·unique 경합·external sort run/FD·build fault 조합 |
| [3.08 catalog·DDL](../contracts/dev3-task-08-catalog-ddl.md), S3 | 정의/이름·열 위치 경계, create/drop의 catalog·객체 원자성, 실패 rollback, 옛 snapshot과 drop/recreate generation, index build와 catalog 공개의 동일 commit 장벽 | **NT08**: 독립 catalog 상태표·이름 충돌·잘못된 position, DDL 전후 객체 집합, create/drop cut와 orphan/retired 소유 fixture | table/index/DDL 객체 조합·전체 실패 cut·과거 snapshot·복구/audit 결합 |

### 3.3 undo·동시성·commit·recovery

| 작업·주 단계 | 중간 필수: 해당 구현/변경 때 실행 | 구현과 함께 추가할 테스트 | S5에서 실행할 전체 범위 |
| --- | --- | --- | --- |
| [3.09 undo·rollback](../contracts/dev3-task-09-undo-rollback.md), S2/S3 | head·두 previous edge·incoming의 같은 그룹; statement mark 복원/이전 statement 보존; 현재 tree의 logical undo가 타 거래·완결 SMO 보존; CLR effect+progress·예약; durable AbortEnd 뒤 잠금 해제; 실패 fence | **NT09**: 독립 edge multiset/typed 이력, 새 대상(row/index/catalog)별 undo fixture, split 뒤 rollback·재삽입 예약, ledger가 mark로 되감기지 않는 반복 실패 입력 | 3.09-C01–3.09-C17 및 선언 변형, chunk/release·target 교차·first/middle/last CLR 재장애·저공간 조합 |
| [3.10 동시성](../contracts/dev3-task-10-concurrency.md), S2 primitive/S3 통합 | 새 guard/rank·모든 pin 준비 또는 반환; q 선택+등록·status11/frontier10; pin 인계 공백 없음; FIFO 준비 중 flusher; RC 문장 안정/다음 문장 새 commit; Serializable 의존 충돌; 즉시 X-lock 뒤 관측 버전 비교·victim 전체 undo | **NT10**: 소유권 compiler probe, sleep에 의존하지 않는 실제 thread barrier, q10 기대값·lock/observe 이력·ticket 진행, deadlock/cancel fixture | C10-01–C10-16 및 선언 변형, 두 isolation×충돌 종류×진입 경로, 실제 thread 장시간 stress |
| [3.11 commit](../contracts/dev3-task-11-commit.md), S3 | commit 전/첫 byte 시도 후 오류 구분, WAL sync 뒤 Committed→frontier→ACK, no-force/steal, 다중 객체 전부 또는 전무, UNKNOWN/ACK 외부 관측, ACK 뒤 reopen 보존 | **NT11**: engine 밖 ACK/UNKNOWN witness와 독립 원자 이력, sync/status/frontier/reply barrier, terminal owner·credit 원자 인계·오류 fixture | 모든 commit cut·다중 객체/거래·WAL/DWB 상태 조합, 정상/거절 복구 분리. 미동결 terminal 형식은 먼저 동결 |
| [3.12 recovery](../contracts/dev3-task-12-recovery.md), S3 | 각 장벽의 [아래 필수 인수](#recovery-minimum). 새로 연결한 장벽과 직접 앞뒤 인계를 검증하고 무변경 앞 장벽 전체를 재실행하지 않음 | **NT12**: source prefix·ATT/DPT·obligation의 독립 snapshot, 대상별 redo 기대값, 두 슬롯 closure·epoch/WRITER·새 프로세스 fixture, 장벽 도달/재장애 controller | 장벽·namespace·quota·fault 전체와 반복 reopen, BEGIN-WAL 제거/부분 CLR, 고정 peak와 정상 startup 규모 비교 |

### 3.4 checkpoint·회수·backup·종합

| 작업·주 단계 | 중간 필수: 해당 구현/변경 때 실행 | 구현과 함께 추가할 테스트 | S5에서 실행할 전체 범위 |
| --- | --- | --- | --- |
| [3.13 checkpoint](../contracts/dev3-task-13-checkpoint.md), S4 | 완료 등록 prefix의 ATT/DPT/ownership 동일 cut; CommitAttempted 미해결 시 capture 금지; 누락 검출; current/previous와 old/new 합집합 보호; floor→Retiring→unlink/dirsync·charge 반환 | **NT13**: capture와 수정/flush thread barrier, 독립 prefix inventory·chunk digest·floor, 누락/조기삭제 mutant, selector 불명 fixture | 3.13-C01–3.13-C15 및 전체 변형, fuzzy capture×dirty/owner·fallback×fault·WAL 회수 전체 |
| [3.14 회수](../contracts/dev3-task-14-reclamation.md), S4 | q/rollback/pin 보존 중 free 금지·해제 후 실제 reuse; edge/cursor/count 동일 그룹; Committed(q) stamp; queue 작업 소유와 final Free 구별; begin-WAL 제거 뒤 Active abort; quota 거절 후 기존 abort 진행 | **NT14**: 독립 ref graph·보존 이유/실제 반환량, release role/chunk cut·generation ABA, old overflow 별도 retirement·owner 해제 후 admission fixture | R14-01–R14-17 및 전체 변형, 장기 reader/churn·retention owner·slot/page/extent·재장애 조합 |
| [3.15 audit·backup](../contracts/dev3-task-15-backup-integrity.md), S0 준비/S3 audit/S4 backup | 작은 고정 DB의 row↔index/catalog/allocation/undo audit 및 대표 손상 검출; drain timeout에 기존 commit/rollback/flusher 진행; exact-C 복사→source WAL 없는 restore→새 ACK→reopen·source 보존 | **NT15**: typed 전체 기대값·known-bad DB, stream audit/FD 계측, drain·copy·선택·새 timeline namespace 인계 fixture와 실제 destination sync 관측 | 전체 무결성 규칙·손상 종류·backup/restore cut·대형 DB·취소·재장애. namespace 인계 미동결 동안 restore 성공을 주장하지 않음 |
| [3.16 자원·성능·최종 인수](../contracts/dev3-task-16-qualification.md), 전 단계/S5 | 새 opcode의 forward/cleanup/abort F05와 자원−1/동일/+1; 작은 fixture에서 계측·집계 정상/known-bad; 변경 회계의 중복/누락·잔여량 보존 | **NT16**: opcode 자원표와 독립 ledger, 실제 RSS/FD/pin/I/O 관측, 4배 fixture·PF09 workload·전수 inventory와 누락/오집계 거절 checker | [FT01–FT10 전체](#final-suite), 최대·대형 profile, PF09와 증거 종합. 중간 단계마다 성능 전수 실행하지 않음 |

인수 표의 case 번호는 각 작업 문서 안에서만 해석한다. “전체 변형”의 실제 입력·cut·seed·개수는 소유 계약의 분모를 유지하여 실행 전에 고정하며 사후 결과에 맞춰 축소하지 않는다. 위 핵심 목록은 동일 source의 기존 시험을 반복하거나, 아직 없는 모든 소비자 구현을 현재 작업의 선행으로 만드는 지시가 아니다.

<a id="design-review-009"></a>
### 3.5 DEV3-REVIEW-009에서 보완한 구현 동반 반례

[2026-10-22 설계 비교 리뷰](../evidence/dev3-task-design-review-20261022.md)의 보완을 해당 NT 묶음에 포함한다. 아래는 **새 테스트 명세**이며 실행 결과가 아니다. 해당 경로를 구현할 때 작은 핵심 반례를 중간 필수로 작성·선택하고, 전체 profile/cut 변형은 S5 inventory에 포함한다. 이번 문서 수정으로 제품 시험을 실행하지 않는다.

| 리뷰 항목 | 해당 작업·중간 필수 반례 | S5 연결 |
| --- | --- | --- |
| F1 flush claim | NT03/05/14: copy 뒤 Pending 전·retirement 뒤 RAM finalize 전 free/reuse 차단, claim/free 동시 등록과 안전한 취소 후 진행. 3.05-C16·R14-17·T06-06 | FT04/05/06/07, batch/profile/cut 전체 |
| F2 전체 own UNIQUE | NT07/10: S1 own INSERT/UPDATE의 키와 S2 충돌 시 S2만 rollback; S1 DELETE/key 이동 뒤 S2 키 재사용·같은 full key 재검사는 정상. 3.07-T10 | FT03/05, NULL·격리·statement 조합 |
| F3 table/index drop | NT08:0/1/8 종속 index와 table의 같은 DDL commit, 옛 snapshot 보존, 마지막 집합 검산과 공개 사이 create/drop 경합. 3.08-T6 | FT03/04/07, 모든 그룹/commit cut |
| F4 부분 rollback 뒤 재undo | NT09: S1 성공→S2 mark rollback→S3 변경→전체 abort에서 S2 재undo0, StatementFinish 전후 crash·BEGIN/옛 CLR WAL 회수. 3.09-C17 | FT03/04/07, 대상/chain/cut 전체 |
| F5 REDO 검사 단계 | NT12: capture 전 terminal owner가 ATT에 없고 일부 status/count page만 최신인 그룹에서 필요한 page만 redo, ledger 재채택0·runtime before 검사 오용 거절. 3.12.4 | FT04/07, 혼합 page 상태·opcode 전체 |
| F6 부분 rollback 중 crash | NT09/12: Aborting(Statement) loser가 mark 이후 남은 전체 거래 undo·AbortEnd까지 완료. mark에서 Active/READY 조기 성공 mutant 거절. 3.09-C17·3.12.7 | FT04/07, 부분/종료 직후·재장애 |
| F7 restore 잔여 의무 | NT15/16: loser0이지만 terminal queue/credit이 남은 backup을 source WAL 없이 복원; 새 admission 전 동일 소유·예약 인계, credit0 초기화 거절. 3.15-T9 | FT04/06/07, queue/namespace·새 ACK/reopen |
| F8 backup drain 진행 | NT15: 기존 rollback/terminal이 공유12MiB를 요구할 때 drain이 workspace/turn을 독점하지 않음; backup 자신의 checkpoint도 진행. 3.15-T6 | FT05/07, 기존 작업·timeout/cancel 조합 |

<a id="design-review-010"></a>
### 3.6 DEV3-REVIEW-010의 설치 순서·drain·backup cut

[16개 설계 2차 리뷰](../evidence/dev3-task-design-review-20261022-r2.md)의 세 보완을 구현 동반 명세로 추가한다. 이번에는 테스트 코드를 작성하거나 실행하지 않는다. 해당 새 경로 구현 시 작은 반례가 중간 필수이며, 전체 변형은 기존 S5 DEV3 inventory에 포함한다.

| 리뷰 항목 | 해당 작업·중간 필수 반례 | S5 연결 |
| --- | --- | --- |
| R2-F1 Commit 설치 순서 | NT04/05/10/11/12: Commit@c sync 뒤 설치 전 정지→같은 status page의 다른 slot update/CLR/aging 시도. 물리 Idle에도 후속 stage 차단, 전체 설치 뒤 재개, 기존 image flush의 redirty 보존·대기 maintenance의 turn 양도·오류 fence. 3.11-T11과 각 cut의 winner/status·terminal 이력 검산 | FT03/04/05/07, 공유 page·생산 opcode·취소/fault 전체 |
| R2-F2 build drain | NT07/08/10: 기존 rollback이 공유12MiB/turn을 요구할 때 build 대기, 자기 거래 drain 방지, 닫힌 외부 admission 아래 catalog commit/실패 cleanup 진행. 3.07-T11 | FT03/05/07, timeout/cancel·동시 drain·staging 전체 |
| R2-F3 최종 backup cut | NT13/15: drain 뒤 checkpoint가 WAL L0→L1을 전진시켜도 논리 C 불변. manifest/base ledger는 최종 sync된 L1 cut, restore 첫 LSN>L1·새 unflushed ACK/reopen. 3.15-T12 | FT04/06/07, checkpoint/manifest cut·retained owner 조합 |

<a id="allocation-first"></a>
## 4. 지금 남아 있는 3.03.1의 필수 테스트

현재 재개 순서의 첫 작업은 이미 부분 구현된 예약·취소다. [부분 실행 원본](../evidence/dev3-allocation-reservation-partial-xadb-20261021.json)과 [독립 검토 remaining](../evidence/dev3-allocation-reservation-partial-review-xadb-20261021.json)을 기준으로 아래만 닫는다.

| 필수 작업 | 추가할 것·판정 | 반복하지 않을 것 |
| --- | --- | --- |
| native 2그룹/13 Linux DB | 기존 계약의 adapter/runner와 독립 실제 bytes·오류·fence 판정 구현 후 해당 13DB 실행. 현재 실행 가능한 완성 runner로 가정하지 않음 | DEV2 native99셀·DEV3 전체 I/O leaf |
| compiler baseline+6 probe | baseline의 compile 성공과 6개 probe 각각의 예상 성공/거절을 판정하는 runner. 위반5개는 의도한 이유로 거절하고 owner 종료 뒤 반환1개는 성공 | 기존 무변경 codec·세션·DWB compiler 전체 |
| component64KiB | generic nested-local의 실제 동시 생존량·수명과 caller/owner/scratch 경계를 측정·증명. 수정이 필요하면 그 경로 사례 추가 | 계산표만으로 stack/RSS 실제 PASS 처리 |
| 변경 WAL stage 4시험 | 원래 미검증 WAL 변경과 현재 source를 대조한 뒤 관련 4개 exact case. 공통 코드가 추가로 바뀌면 직접 영향 case만 추가 | genesis WAL 전체 runtime/native suite |
| 관련 quality·독립 검토 | 변경 target/feature의 필요한 compile·Clippy, 새 결과와 기존 source별 증거 취합 | workspace Clippy/test·전체 wrapper |

runtime8 및 r3/r4에서 이미 확인한 선택 사례는 관련 코드가 같으면 중간 단계에서 재실행하지 않는다. 새로운 native/compiler 코드와 실제 미검증 변경은 검증한다. 이 단위의 성공으로 INIT·home flush·일반 recovery·거래 ACK를 완료 처리하지 않는다. S5에서는 최종 DEV3 inventory에 포함된 예약·취소 시험도 전수 대상이다.

native 13DB는 새로 부풀린 전수 행렬이 아니라 기존 제한 단위의 고정 필수 입력이다. 정상3DB는 profile1/2/3의 reserve/cancel 1→8→63 cycle, 장애10DB는 profile1·N8의 reserve/cancel×write-before/half/after·sync-before/after다. compiler는 정상 baseline과 installed owner 재사용·frame/operation/flush/WAL slot 중첩 변경 거절, owner 종료 뒤 반환 성공을 구별한다.

남은 WAL4의 실제 기존 시험명은 `wal_slot_writes_exact_logical_and_physical_prefixes`, `wal_slot_rejects_boundaries_before_effect`, `wal_io_releases_gate_and_protects_slot`([WAL 시험](../../crates/engine/src/storage/wal_io_tests.rs)), `bootstrap_wal_requires_selected_mapping`([bootstrap 시험](../../crates/engine/src/storage/wal_io/bootstrap_tests.rs))다. 실행 시 실제 모듈을 포함한 전체 이름으로 선택하고 원래 미검증 helper diff와 대응한다.

<a id="recovery-minimum"></a>
## 5. 복구 3.12의 장벽별 중간 필수

새 장벽의 핵심 정상·거절·소유 인계만 먼저 확인한다. 실제 crash/reopen이 아직 연결되지 않은 부분은 메모리 포트의 상태 전이 증거로 표시하고, 그 소비자가 연결될 때 아래 실제 reopen 의무를 닫는다. 공통 controller를 준비하되 완성되지 않은 하위 결과를 성공 복구로 집계하지 않는다.

| 하위 작업 | 중간 필수 사례 | 함께 추가할 입력·관측 |
| --- | --- | --- |
| 3.12.1 SELECT/BOOT_RESERVE | 정상 selected closure; profile·자원 부족·손상 시 첫 DB write 전 거절; admission 닫힘 | namespace 후보·write 미시도·bootstrap 예약 ledger |
| 3.12.2 DWB_REPAIR | 선택 pending의 두 file 복원·모두 sync 뒤 retirement; pending 없는 손상 거절과 완전 INIT+연속 WAL 예외 | 실제 torn image·file별 sync·retirement cut, raw retired image mutant |
| 3.12.3 ANALYSIS/RESERVE_RESTORE | capture 뒤 prefix에서 winner/loser·잔여 obligation 재구성; stale home 배제; REDO 전 정확히 한 번 인계 | 실제 파일 bytes/page token/소비 CLR를 별도 계산한 ledger |
| 3.12.4 REDO | 다중 page 중 일부 home만 반영된 그룹의 대상별 skip/apply; incarnation mismatch·corrupt complete WAL 거절; WAL-before-page | 부분 반영 page 집합·전체 그룹 skip mutant·source 보존 hash |
| 3.12.5 REBASE | 두 슬롯/명시 previous·공통 fallback에서 완전 closure 보존; selector 불명 old/new 합집합; 고정 peak·새 WAL 의존 없음 | 슬롯 선택 전후 재진입 fixture·charge/retention 이력 |
| 3.12.6 LOG_OPEN | epoch/header sync와 WRITER 영속 선택 전 CLR 금지; 선택 직후 재시작에서 같은 새 epoch 발견 | 실제 namespace/sync trace·WRITER 선택 cut |
| 3.12.7 UNDO/VERIFY | loser의 남은 chain만 undo; CLR 효과+progress 재적용에서 중복 없음; AbortEnd 전 성공 금지 | 부분 CLR·winner ACK·남은 예약의 독립 기대값 |
| 3.12.8 STABILIZE/READY | 장벽 실패 시 READY 금지; 성공 뒤 새 ACK→두 번째 reopen 보존; 연결한 경로의 재장애·소형 full audit | 외부 admission/ACK 관측, controller cut 도달 기록·재현 bundle |

각 장벽의 모든 slot/owner/batch/profile 조합과 반복 장애 캠페인은 S5에 둔다. 다만 이번에 구현한 영속 선택 자체가 특정 중간 cut에서 깨지는지 확인하는 핵심 사례는 최종까지 미루지 않는다. 제품의 복구 성공을 선언하는 실제 연결에는 새 프로세스·현재 build의 복구가 필요하다.

<a id="test-delivery"></a>
## 6. 추가 테스트를 구현에 동반하는 방법

1. **구현 전:** 해당 NT 묶음에서 정확한 요구·정상/오류 기대값·독립 fixture를 정한다. 아직 runner가 없으면 그 구현도 작업 산출물에 포함한다. 신규 파일 경로는 실제 구현 때 저장소 목록에 등록한다.
2. **구현 중:** 제품 코드와 함께 좁은 test case·fault hook·thread barrier·oracle를 추가한다. production encoder/replay/ledger 결과를 다시 정답으로 사용하는 시험은 피한다. test-only hook은 배포 feature와 분리한다.
3. **중간 인수:** 위 필수 사례만 정확히 선택한다. 새 harness는 작은 정상/known-bad로 검증하고, 필수 분기·cut의 실제 도달과 실행 case 수를 확인한다. 아직 실행하지 않은 큰 변형은 S5 예정으로 남긴다.
4. **병합 시:** source·fixture·case·결과와 남은 S5 목록을 함께 넘긴다. 새 연결 코드의 직접 시험은 실행하되 완료한 하위 시험을 반복하지 않는다.
5. **S5 전:** 미작성 테스트·미확정 profile·지원되지 않는 runner를 먼저 완성한다. 중간에서 시험을 생략한 것과 아직 테스트를 작성하지 않은 것을 구분한다. 미작성/미검증 harness가 남으면 전수 실행 준비 완료가 아니다.

| 테스트 종류 | 함께 추가할 최소 장치 | 중간 실행 | S5 실행 |
| --- | --- | --- | --- |
| 단위·component | 독립 입력/기대값, resource/owner 관측, exact case 선택 | 변경 분기 필수 입력 | DEV3 전체 지원 case |
| compiler | 정상 baseline, 의도한 타입/수명 거절 probe와 오류 사유 판정 | 변경 API probe | DEV3 전체 capability/수명 probe |
| native·fault·reopen | 실제 Linux adapter, cut 도달, 효과/확실성, 외부 이력·새 fixture | 변경 영속 경계의 핵심 cut | 선언한 전체 cut/variant 및 반복 규칙 |
| thread·진행 | 실제 barrier·ticket/lock/gate trace, timeout과 미도달 구분 | 변경 경쟁 경계의 결정적 schedule | 전체 schedule·영향 suite의 장시간 profile |
| property·fuzz·모델 | 고정 seed/corpus/generator hash·known-bad, 모델의 독립 graph | 새 규칙/변경 checker의 고정 사례와 핵심 반례; [§1](#selection)의 해당64 seed·변경 model 선언 공간·변경 parser/codec의 기존 보존 corpus 의무 유지 | 전체 선언 유한 공간·확장 seed/corpus/fuzz budget |
| 성능·큰 자원 | correctness oracle·요청/시도/성공 분모·실제 bytes/FD/RSS 계측 | 작은 fixture의 계측/집계 검증과 새 opcode 한도 | 실제 4배 working set·PF09 전체 workload/profile |

기존 runner가 case 하나를 골라도 내부에서 전체 leaf를 실행한다면 중간 실행용으로 쓰지 않는다. 해당 구현과 함께 좁은 선택 기능 또는 전용 test 진입점을 제공하고, 선택한 case 밖이 실행되지 않는지도 도구 시험으로 확인한다. case 이름을 잘못 지정해 0개 실행한 결과는 PASS가 아니다.

`--exact`도 시험 함수 안의 전체 profile/fault 반복까지 줄여 주지는 않는다. 신규 시험은 중간 필수 입력과 최종 확대 입력을 서로 독립적으로 선택 가능한 test/case ID로 나눈다. 기존 시험이 이미 과도하게 묶여 있으면 해당 변경과 함께 분리하고 기대값·원래 전체 분모를 보존한다.

현재 진입점의 실제 공급은 다음과 같다. 아래는 읽기로 확인한 현황이며 실행 결과가 아니다.

| 현존 진입점 | 실제 범위·중간 선택 주의 |
| --- | --- |
| `xadb-engine`의 `--lib` 단위 test | 표현·무효과 semantic·제한 WAL/할당·clean buffer. 실제 전체 test 경로를 넣은 `--exact` 선택 가능; 함수 내부 loop 범위도 확인 |
| `xadbtest-engine-worker`의 `page_io`, `read_buffer`, `genesis_wal`, `storage_session`, `doublewrite_install` test target | 실제 제한 native 공급. target와 정확한 함수로 선택 가능하나 일반 dirty/commit/recovery 인수가 아님 |
| [check_dev3_leaf.py](../../scripts/check_dev3_leaf.py) | 현재 `--report`·`--with-regressions`만 있고 개별 case selector 없음. 중간 실행에 사용하지 않으며 이 기존 leaf만으로 S5 전수 충족 불가 |
| 기존 [buffer](../../scripts/check_dev3_buffer_borrows.py)·[WAL](../../scripts/check_dev3_wal_borrows.py)·[storage](../../scripts/check_dev3_storage_borrows.py)·[DWB](../../scripts/check_dev3_doublewrite_borrows.py) compiler runner | 기존 수명 경계에 한정하며 현재 개별 probe selector 없음. 새 변경에 필요한 probe만 고를 수 있도록 해당 구현 때 제공 |
| 신규 NT 묶음의 미공급 영역 | allocation native/compiler, 운영 dirty/flush, live tree/undo·index/DDL, 일반 commit/recovery/audit 등의 시험은 해당 기능과 함께 추가. 기존 표현/모델 결과로 대신하지 않음 |

<a id="final-suite"></a>
## 7. S5 — DEV3 최종 전수 테스트

2026-10-22 후속 사용자 지시에 따른 **DEV3 최종 인수의 전수 실행 예외**다. 중간 단계의 변경 범위 원칙은 유지한다. 최종 후보 source/binary·fixture·환경·case inventory를 고정하고 아래 전체를 한 최종 인수 캠페인으로 수행한다. 하나의 거대한 명령일 필요는 없으며 case별 job으로 나눠 같은 inventory를 완주한다. 계획 작성일에 실행하거나 자동 예약하지 않는다.

필수 기능·시험이 미구현이면 해당 의무를 inventory에서 빼지 않고 NOT_RUN으로 남긴다. 실행 가능한 것만 모아 전수 완료라고 판정하지 않는다. 누락된 기능·시험과 필요한 profile 동결을 먼저 완료해야 최종 DEV3 완료 판정이 가능하다.

| 최종 묶음 | 전수 범위 | 주요 원문·산출물 |
| --- | --- | --- |
| FT01 표현·component·compiler | DEV3 3.01–16의 구현된 계약 시험과 모든 지원 형식/semantic/수명 case. 중간에서 통과한 DEV3 사례도 최종 대상 | 각 작업 인수 표·실제 test inventory, NT01–16 |
| FT02 모델·oracle | M01–M06의 선언 fixture/유한 공간·진행/반복 bootstrap graph, 필수 negative mutants와 checker 부정 입력 | 상태/전이/미탐색 수·counterexample·hash. cap 도달은 INCONCLUSIVE |
| FT03 기능·원자성 | DS01/06·KU09/10의 row/tree/index/catalog·unique·statement/거래 원자성·두 isolation 전체 | typed tuple/history·row↔index·catalog 검산 |
| FT04 영속·복구 | DS02/05·RC-T12–14의 WAL/DWB/home/namespace/selector·8장벽·CLR·ACK/UNKNOWN·restore 재개, 선언 fault 전부 | 실제 cut 도달·외부 ACK·새 프로세스 reopen, 정상 복구와 손상/I/O 거절 분리 |
| FT05 동시성·수명 | DS04/06의 snapshot 등록/공개·pin/ABA·lock/observe·deadlock·FIFO 진행 전체 schedule | 실제 thread 및 각 영향 suite의 최종 stress profile |
| FT06 자원·크기 | DS03/RC-T14·opcode F05·4/16/64MiB buffer·row/index/locator/allocation 각각4배·최대 입력/정상 admitted 복구 | 실제 bytes·pin/FD/RSS·quota/잔여 ledger와 진행. 64GiB qualification 준비를 지원 완료로 가정하지 않음 |
| FT07 checkpoint·회수·backup·audit | capture/fallback/floor·begin-WAL 제거·stamp/cut/release/reuse·exact-C source-WAL 없는 restore와 모든 무결성 규칙 | NT13–15 전체 입력, 긴 reader·Active 거래·장기 churn·재장애 |
| FT08 PF09 | CRUD/scan·1/2/4 writer/reader·두 isolation/충돌 분해·DWB1/8/64·checkpoint/reopen/audit·aging·backup | 같은 의미/환경의 correctness 선행, 성공/시도/거절·latency·I/O·공간·계측 비용 |
| FT09 재현·증거 | 모든 필수 의무/case와 source/binary/fixture/log 대응, 잘못된 binary·누락/미도달·오집계 거절, 재현 bundle | 독립 검토·전체 inventory의 결과 누락0. 제품 실행 PASS와 도구 판정 분리 |
| FT10 하위 공급 수신 | DEV3가 실제 수정한 공통 공급·adapter와 DEV3 전용 연결의 필수 수신 사례 | DEV0/1/2 전체 workspace/wrapper를 자동 전수 실행하지 않음. 무변경 하위 공급은 원래 증거와 동일성 확인 |

FT01–FT10은 실행 분모를 나누는 목록이며 기능별 독립 suite를 중복 실행하는 요구가 아니다. 한 실제 case를 DS/RC/KU에 여러 번 연결할 수 있다. 단, 입력·feature·환경·판정이 다르면 다른 variant이며 이름만 같다고 결과를 재사용하지 않는다. full audit는 그 fixture의 논리 상태 전체를 확인하는 검사이고, 매번 저장소 테스트 전체를 돌린다는 뜻이 아니다.

최종 수치 profile은 [DP03](../policies/development-policy.md#section-4)의 해당 확장 기준을 유지한다. property는 해당 확장256 seed·최대1024 operation, parser/codec target별 fuzz30분, 영향 동시성 suite의 실제 thread stress30분, 비결정적 필수 fault 셀3회이며 필요한 구체 입력/시간·환경은 실행 전에 동결한다. 모델의 고정 유한 공간 전수는 seed/stress로 대체하지 않는다. 중간 단계에 이 확대 profile 전체를 반복하지 않는다. 소유 계약의 더 구체적인 분모가 있으면 그 분모를 등록하며 실패 후 줄이지 않는다.

최종 실행 중 결함을 고치면 원래 실패를 보존하고 수정 반례와 실제 영향 case를 다시 실행한다. 무관한 완료 job까지 처음부터 반복하지 않는다. 최종 판정은 전수 기준 source와 이후 diff의 영향 검토·선택 재검증을 함께 사용하고, 다른 source 결과를 전부 마지막 binary에서 다시 실행했다고 표현하지 않는다. 영향 범위를 확인할 수 없는 부분은 미해결로 남기고 범위를 먼저 확정한다.

<a id="receipts"></a>
## 8. 실행 명세와 결과 기록

| 실행 전에 적을 항목 | 내용 |
| --- | --- |
| 작업/요구/시험 | 3.xx·NTxx·상세 설계 case·DS/RC/KU/PF 의무 |
| 코드 변화 | 마지막 관련 검증 source, 현재 commit/dirty digest, 실제 파일·symbol diff, 영향받는 직접 소비자 |
| 선택 | 중간 필수 또는 S5 전수, 정확한 case/variant·예상 개수, 선정 이유, 무변경 생략 사례의 원래 증거 |
| 입력·명령 | fixture/corpus/seed·fault cut/schedule, 실제 존재하는 실행 진입점과 exact selector, feature·build·환경 |
| 판정·한도 | 독립 기대값, 정상/거절/UNKNOWN 구분, memory/disk/FD·시간·종료 기준과 관측 방법 |

결과에는 실행한 case 수·실제 cut/barrier 도달·binary/source/fixture/log identity·원래 오류와 판정을 남긴다. `PASS`, `FAIL`, `NOT_RUN`, `INCONCLUSIVE`를 구분한다. 기존 결과를 인용하거나 중간에서 무변경을 생략한 것은 **기존 증거 재사용**이지 새 PASS가 아니다. 최종까지 미실행인 필수 항목은 전체 DEV3 완료를 막는다.

Rust 단위 시험은 실제 package/target과 전체 test 경로를 확인한 뒤 `--exact`를 쓰는 좁은 명령으로 고정한다. native/compiler/controller는 구현된 선택 인자만 사용한다. 이 계획에서 미구현 runner의 명령이나 미래 파일 경로를 실행 가능한 것으로 제시하지 않는다. 새 테스트 도구의 case 선택·누락 검출도 NT 묶음의 일부다.

<a id="decision"></a>
## 9. D3-TEST-PLAN-001 결정 기록

| 항목 | 기록 |
| --- | --- |
| 분류·상태 | D2 · ACCEPTED. 단계별 실행 시점과 테스트 개발 범위 구체화이며 제품 의미·전체 필수 인수 분모 변경 없음 |
| 일자·권한 | 2026-10-22. 사용자의 단계별 필수 목록·구현과 함께 추가할 테스트 요청 및 DEV3 완료 시 전수 실행 지시 |
| 소유 | 작업·계약·통합 `/root`. 3.01–08 조사 `/root/dev3_specs_review`, 3.09–16 조사 `/root/dev3_buffer_design`. 독립 검토 `/root/dev3_tree_design` |
| 기준·보존 | HEAD `a285043b91571cef83dacda49fcf2a63e99160d2`; 기존 변경 사본 `/tmp/xadb-dev3-test-plan-52farjdu`, 시작 tracked diff SHA-256 `0399f6c1cafab635b6001f9f640e94553ebb2f10d0aa99b90967f4ecd8220b98`. 이전 상세 설계·분해·원본 실행 결과 보존 |
| 선택·대안 | 중간은 변경 기능의 최소 필수, 신규 테스트는 구현과 동반, 큰 조합/장시간/전수는 최종 S5. 매 단계 전수와 모든 테스트 개발을 마지막으로 미루는 방식을 채택하지 않음 |
| 적용·대체 | 이전 무변경·전수 금지는 중간 단계에서 유지. 최신 사용자 지시에 따라 **DEV3 최종 인수 전수만 예외**로 구체화. DEV0/1/2·전체 workspace 전수로 확장하지 않음 |
| 검토 | `/root/dev3_tree_design`이 새 계획 전체와 기준 사본 대비 9개 연결 파일을 읽기 전용 검토하여 PASS. 변경 parser/codec의 보존 corpus 중간 의무를 명료화한 뒤 재확인했고 미해결 필수 수정 없음. compiler 위반5/성공1·실제 runner 범위·16개 작업/복구8장벽·과거 증거 보존 확인. 검토한 본문 SHA-256 `7bd899a4f6a668da82e06ddae15752c6cf937aec5535dfeeb523739d6d79438e`는 이 수용 기록 반영 전 후보의 identity |
| 실행 경계 | 테스트 추가 구현·제품 시험·compiler/Clippy 미실행. 검토자는 문서 자동 검사도 하지 않음. 후속 사용자 요청에 따른 전체 문서 검증·문제 수정·commit/push는 별도 전달 작업이며 문서 검증 결과는 [보고서](../document-validation.json)와 실제 push gate를 따름 |
| 재검토 | 신규 opcode/API·fault/자원 경계, 누락된 필수 의무, exact 선택 불가, 잘못된 재사용/집계, 사용자 실행 범위 변경 |
