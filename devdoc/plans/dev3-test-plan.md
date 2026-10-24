# DEV3 — 단계별 필수 테스트와 구현 동반 테스트 추가 계획

2026-10-22 · **D3-TEST-PLAN-001 D2 ACCEPTED / 독립 검토 완료 / 신규 테스트 코드·제품 실행 없음**

같은 날 [항목별 재점검](../evidence/dev3-test-plan-review-20261022.md)으로 중간 필수 경계·중복 제거·실행 분모를 보완했다. 최초 결정과 그 검토 identity는 보존하며, 후속 **D3-TEST-PLAN-002의 상태·검토 범위**는 [새 결정 기록](#review-decision)을 따른다.

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

<a id="minimum-boundaries"></a>
### 1.1 필수 사례를 고르는 단위

“작게 실행”은 **서로 다른 결과를 만드는 경계를 작은 fixture로 확인**한다는 뜻이다. 다음 기준으로 해당 변경에 필요한 case만 선정한다.

- 정상·사전 무효과 거절·effect 후 실패/fence·정리/소유 반환은 서로 다른 경계다. fault 하나로 이 네 결과를 대표하지 않는다. checksum 오류와 checksum을 다시 맞춘 의미 오류도 구분한다.
- 새 limit/분기·상태 인계를 처음 구현할 때는 최소·최대·필요량−1/동일/+1, 취소/Drop/unwind의 적용되는 경계를 닫는다. 최대 입력 한 건·split 한 번·두 sort run의 merge는 중간 필수이고, 수많은 최대 입력·대형 working set·장기 반복은 S5다. 적용되지 않는 경계는 이유를 적는다.
- 모델, 메모리 port, 실제 Linux I/O, 실제 thread, 새 프로세스 reopen은 증거 종류가 다르다. 동일 불변식이라도 서로를 대체하지 않는다. 상위 공급이 없으면 그 통합 의무의 소비 작업과 완료 시점을 적고 미완료로 인계한다.
- 하나의 scenario가 여러 NT/DS/RC 의무를 만족하면 **주 실행 owner 하나와 의무별 assertion**을 등록한다. 같은 fixture·cut·schedule·feature·환경·기대값의 실행 결과를 연결하고, 호출자별 실제 연결이나 다른 fault 경계를 같은 시험으로 간주하지 않는다.
- 테스트 코드·공통 fixture·oracle·generator·빌드 feature도 영향 분석 대상이다. 제품 파일이 같아도 이들이 바뀌어 기대값/경로가 달라진 사례는 새 검증 대상이다. 전체 source hash 변경만으로 무관한 사례를 선정하지 않는다.

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
| [3.10 동시성](../contracts/dev3-task-10-concurrency.md), S2 primitive/S3 통합 | 새 guard/rank·모든 pin 준비 또는 반환; q 선택+등록·status11/frontier10; pin 인계 공백 없음; FIFO 준비 중 flusher; RC 문장 안정/다음 문장 새 commit·독립 행 writer 성공; Serializable 의존 충돌; 즉시/대기/already-held X-lock 뒤 관측 버전 비교·victim 전체 undo | **NT10**: 소유권 compiler probe, sleep에 의존하지 않는 실제 thread barrier, q10 기대값·lock/observe 이력·ticket 진행, deadlock/cancel fixture | C10-01–C10-16 및 선언 변형, 두 isolation×충돌 종류×진입 경로, 실제 thread 장시간 stress |
| [3.11 commit](../contracts/dev3-task-11-commit.md), S3 | commit 전/첫 byte 시도 후 오류 구분, WAL sync 뒤 Committed→frontier→ACK, no-force/steal, 다중 객체 전부 또는 전무, UNKNOWN/ACK 외부 관측, 후속 overwrite로 가려지는 ACK 유실 검출, ACK 뒤 reopen 보존 | **NT11**: engine 밖 ACK/UNKNOWN witness와 독립 원자 이력, sync/status/frontier/reply barrier, terminal owner·credit 원자 인계·오류 fixture | 모든 commit cut·다중 객체/거래·WAL/DWB 상태 조합, 정상/거절 복구 분리. 미동결 terminal 형식은 먼저 동결 |
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

<a id="item-review"></a>
### 3.7 16개 항목 재점검: 중간 필수의 빠진 경계와 중복 방지

아래는 기존 소유 계약의 인수를 중간 실행 선택에 구체화한 보완표다. 새 제품 의미나 별도 전수 suite를 추가하지 않는다. **각 행도 해당 경로를 작성·수정할 때만** 적용하며, 동일 보장의 기존 case가 있으면 연결하고 부족한 assertion/fixture만 추가한다. §3.1–3.6과 이 표의 합집합을 §6.1에서 중복 없이 등록한다.

| 항목·계약 근거 | 중간에 놓치지 않을 필수 사례·판정 | 비용·중복 제어와 S5 확대 |
| --- | --- | --- |
| 3.01 / R2–R6 | 신뢰 expected를 입력 header로 위조한 사례, checksum 유효 의미 손상·trailing bytes, encoder 거절 시 caller output의 계약상 보존. round-trip만으로 정답 판정 금지 | 공통 envelope는 NT01, 소비자에는 새 kind의 의미·authority 인계만 추가. 변경 target의 보존 corpus는 유지하고 무관한 codec 전수 제외 |
| 3.02 / §5–6 | 경로 이름만 같고 실제 열린 handle/namespace가 다른 입력·alias/lease 충돌, 실패 뒤 FD/lease 반환, 원래 I/O cause와 common fence 관측 | native 호출 한 번의 trace를 NT04/12의 같은 연결에 재사용. 큰 manifest/FD 전체 조합은 S5; 해당 limit의 작은 경계는 중간 |
| 3.03 / R1–R6·§6 | §4 잔여 유지. INIT의 wrong claimant·중복 token 소비·generation MAX, growth 선택 전 주소 참조 금지·부분 bytes 소유, cancel 뒤 generation/recLSN/retained WAL 보존 | reserve/cancel 13DB와 WAL4를 다른 wrapper로 반복하지 않음. flush claim/free 경합은 NT05/14와 공동 case, 큰 inventory는 S5 |
| 3.04 / R1–R7·§5 | Busy/stale prepare·LSN/offset 소진·owner chain 불일치, short/Interrupted 뒤 진행·zero/invalid count 뒤 fence, stage/apply/DPT 사이 unwind. 물리 Idle과 Commit 의미 설치 완료를 구별 | opcode 의미마다 작은 vector, 공통 append/sync fault는 동일 호출 경로일 때 공유. Commit 설치 반례 주 owner는 NT11 |
| 3.05 / C02–C12·C16 | counter 소진·전체 pin 실패 cleanup, 중복 주소/batch 거절, copy/finalize redirty 경쟁, **한 page의 부분 batch deadline**, hot-page 압력에서 flush 완료와 oldest recLSN 진전을 별도 관측 | 다중 file 실패는 작은 2-file fixture. claim/free는 NT03/14에 같은 receipt 연결. batch1/8/64×profile×명시 fault의 확정 행렬·장시간 pressure는 S5 |
| 3.06 / T06-02–14 | 유효 checksum의 child/level/범위 손상, 비대칭 entry split·마지막 row 삭제/root 축소, 2047/2048/2049·최대 row와 overflow 누락/순환, **자기 mutation 뒤 cursor 무효화**, locator cache on/off 결과 동일 | 작은 강제 split·소수 chunk로 경계 도달. observation은 NT10, split 뒤 undo는 NT09와 공동 case. 4배 집합은 S5 |
| 3.07 / T1–T11 | typed tuple 순서·range 포함/제외 경계, full-key 제거/재추가의 옛 snapshot, 두 writer의 commit/abort 뒤 UNIQUE 재검사, 작은 **2-run external merge**·단계별 취소/cleanup과 닫힌 admission 아래 catalog 공개 | codec vector는 NT01 수신, 반복 실패 ledger는 NT09/16과 공동. 작은 spill로 merge 분기를 먼저 닫고 전체 key/index/profile·대형 build는 S5 |
| 3.08 / R1–R7·§7 | duplicate/UTF-8 byte 이름·nullable PK·parent generation/schema mismatch·열 위치·index 수 경계; 실패 create 후 정의/소유 보존, 단독 index drop과 table+종속 index drop 구별 | build 알고리즘은 NT07, NT08은 catalog 공개/undo·staging 인계만 연결. 기존 REVIEW-009의0/1/8 index drop 필수는 유지 |
| 3.09 / C01–C17 | 같은 UndoRef를 두 role이 참조하면 count2, creator는 status edge 아님; net0 remove/add의 제거 존재·underflow 검산; Missing/tombstone 구별; 반복 statement 실패 뒤 실제 ledger 누적과 abort 예약 보존 | 작은 몇 회 반복으로 다음 admission 거절까지 도달. 부분 rollback suffix 재undo 방지는 기존 C17과 연결. 긴 chain/모든 CLR cut 조합은 S5 |
| 3.10 / C10-01–16·DS06 | 즉시/대기/already-held의 **세 X-lock 경로**와 same-bytes UPDATE/DELETE, own-write 재관측·stale token 거절; RC 독립 행 writer 둘 다 성공, Serializable 독립 table 성공과 absence/range/write-skew/catalog 충돌 구별 | 실제 barrier로 각 독립 분기 도달. 중간에30분 stress를 추가하지 않음. q10·pin 인계는 NT09/11/14와 같은 실행을 연결 |
| 3.11 / T1–T11·§9 | T1 ACK 뒤 T2 overwrite로 최종 값만 같아도 T1 유실을 검출; 확정 ABORT/UNKNOWN 별도 기대값. read-only/empty transaction 경로는 **F02 동결 후** 잠금·등록 해제와 규정된 outcome 검증 | ACK witness/checker는 NT15와 공동, status11/frontier10은 NT10과 공동. 서로 다른 최초 byte/status/frontier/reply cut는 합치지 않음 |
| 3.12 / R1·R4–R8·§10 | 낮춘 foreground 한도+원래 recovery profile은 기존 ACK/부분 CLR loser 보존. 부족/미지원 profile은 write0 거절 후 지원 설정 재개. 불완전 active tail과 **완전 길이 손상/gap**의 거절 구분; empty DPT 시작점은 F01 후 검증 | §5의8장벽 인계에 배정. 전원 상실 모델·process kill·주입 I/O의 증거 구별. 정상 startup에 full audit를 넣지 않음; 규모 비교는 S5 |
| 3.13 / C01–C15 | previous metadata는 정상이지만 WAL/undo/FILEMAP closure가 불완전한 fallback 거절; 최신 home을 이전 image로 되감지 않음. unlink 뒤 dirsync 불명/열린 old inode의 charge 보존 | capture 등록 경쟁은 NT04/05/11과 공동. 작은 마지막 chunk 누락/중복도 중간 필수, 큰 inventory와256MiB/1GiB 비교는 S5 |
| 3.14 / R14-01–17 | count0이어도 외부 소유/queue 작업이 남으면 final Free 금지, 같은 target 두 role의 release cursor 정확히 한 번·반환 chunk 재읽기 금지, generation MAX, 진단 잘림을 전수 완료로 오인 금지 | begin-WAL 없는 abort는 NT09/12/13, old overflow는 NT06/09, claim/free는 NT05와 공동. 작은 quota로 거절→소유 해제→실제 reuse→admission 확인; 장기 churn은 S5 |
| 3.15 / T1–T12 | 감사 규칙마다 작은 정상/known-bad: index 누락/유령·catalog/allocator 오염·edge multiplicity/queue 누락; checksum 재봉인. restore의 nonempty destination·누락 파일·pageLSN/high-water·profile 거절과 source 보존 | 정상 C·손상 clone·restore→새 ACK→reopen의 fixture 기반 공유. T9/T10/T12는 한 scenario에 각각 assertion 가능. destination sync/cancel cut는 유지, 큰 audit/backup은 S5 |
| 3.16 / R1–R7·§3–8 | 각 자원의 forward/cleanup/abort 동시 peak와 별도 타입 크기/stack/RSS, 실제 admission 성공 수·거절 수. selector의0건/추가 실행/누락/중복 receipt·잘못된 binary·미도달·timeout을 성공 집계하지 않는 known-bad | 공통 ledger oracle·case 목록·build 재사용. correctness/계측 검사 뒤 PF09. 비용 절감은 중복 실행/fixture 준비에서 하고4배 요구·최종 budget은 유지 |

이 표의 “공동”은 이름이 비슷한 시험을 삭제하는 규칙이 아니다. 같은 실행이 각각의 계약 assertion과 실제 소비 경로를 충족하는지 확인한다. 예를 들어 모델의 claim/free 반례와 실제 thread 경합은 별도 실행이며, 상위 호출자가 새로 연결되면 그 연결 사례가 필요하다.

<a id="allocation-first"></a>
## 4. 3.03.1의 필수 테스트 배정과 완료 기록

아래는 부분 구현 당시 [실행 원본](../evidence/dev3-allocation-reservation-partial-xadb-20261021.json)과 [독립 검토 remaining](../evidence/dev3-allocation-reservation-partial-review-xadb-20261021.json)에 배정한 필수 작업이다. 현재는 아래2026-10-22 인수 기록까지 완료했으며 현재 재개·무변경 재실행 목록이 아니다. 후속 선택은 [현재 우선순위](dev3-implementation-plan.md#work-priority)와 실제 diff를 따른다.

| 필수 작업 | 추가할 것·판정 | 반복하지 않을 것 |
| --- | --- | --- |
| native 2그룹/13 Linux DB | 기존 계약의 adapter/runner와 독립 실제 bytes·오류·fence 판정 구현 후 해당 13DB 실행. 현재 실행 가능한 완성 runner로 가정하지 않음 | DEV2 native99셀·DEV3 전체 I/O leaf |
| compiler baseline+6 probe | baseline의 compile 성공과 6개 probe 각각의 예상 성공/거절을 판정하는 runner. 위반5개는 의도한 이유로 거절하고 owner 종료 뒤 반환1개는 성공 | 기존 무변경 codec·세션·DWB compiler 전체 |
| component64KiB | generic nested-local의 실제 동시 생존량·수명과 caller/owner/scratch 경계를 측정·증명. 수정이 필요하면 그 경로 사례 추가 | 계산표만으로 stack/RSS 실제 PASS 처리 |
| 변경 WAL stage 4시험 | 원래 미검증 WAL 변경과 현재 source를 대조한 뒤 관련 4개 exact case. 공통 코드가 추가로 바뀌면 직접 영향 case만 추가 | genesis WAL 전체 runtime/native suite |
| 관련 quality·독립 검토 | 변경 target/feature의 필요한 compile·Clippy, 새 결과와 기존 source별 증거 취합 | workspace Clippy/test·전체 wrapper |

2026-10-22 구현 결과는 [3.03.1 인수 기록](../evidence/dev3-allocation-reservation-acceptance-20261022.md)에 연결한다. native는 `dev3_allocation_native.py`의13개 child 선택, compiler는 `check_dev3_allocation_borrows.py`의 baseline+6 입력, 자원은 `allocation_component_layout_inventory`와 `check_dev3_allocation_stack.py`가 선택하는 `allocation_component_runs_with_observed_64k_stack`을 공급했다. 새 checker의 개별 selftest도 원래 source 결과와 함께 S5 inventory 입력이다. 중간 실행의 원래 FAIL을 지우지 않으며 이 목록은 아직 미구현된 DEV3 전체 시험을 대체하지 않는다.

runtime8 및 r3/r4에서 이미 확인한 선택 사례는 관련 코드가 같으면 중간 단계에서 재실행하지 않는다. 새로운 native/compiler 코드와 실제 미검증 변경은 검증한다. 이 단위의 성공으로 INIT·home flush·일반 recovery·거래 ACK를 완료 처리하지 않는다. S5에서는 최종 DEV3 inventory에 포함된 예약·취소 시험도 전수 대상이다.

native 13DB는 새로 부풀린 전수 행렬이 아니라 기존 제한 단위의 고정 필수 입력이다. 정상3DB는 profile1/2/3의 reserve/cancel 1→8→63 cycle, 장애10DB는 profile1·N8의 reserve/cancel×write-before/half/after·sync-before/after다. compiler는 정상 baseline과 installed owner 재사용·frame/operation/flush/WAL slot 중첩 변경 거절, owner 종료 뒤 반환 성공을 구별한다.

남은 WAL4의 실제 기존 시험명은 `wal_slot_writes_exact_logical_and_physical_prefixes`, `wal_slot_rejects_boundaries_before_effect`, `wal_io_releases_gate_and_protects_slot`([WAL 시험](../../crates/engine/src/storage/wal_io_tests.rs)), `bootstrap_wal_requires_selected_mapping`([bootstrap 시험](../../crates/engine/src/storage/wal_io/bootstrap_tests.rs))다. 실행 시 실제 모듈을 포함한 전체 이름으로 선택하고 원래 미검증 helper diff와 대응한다.

<a id="recovery-minimum"></a>
## 5. 복구 3.12의 장벽별 중간 필수

[첫3.12.1.a 선택 단위](../contracts/dev3-recovery-select.md#inventory)는 초기 control·header-only WAL의 닫힌 inspection으로 한정한다. 해당 신규 unit/native/compiler와 helper 수신만 선택하며 아래3.12.1 전체·3.12.1–4 복원·BOOT_RESERVE 성공으로 세지 않는다. actual nonempty producer prefix의 거절과 성공 복구 인수를 구분한다. [구현·재개 상태](../implementation/dev3-implementation.md#recovery-select)

후속 [현행 WAL prefix 분석](../contracts/dev3-recovery-prefix.md)은 실제 reserve/cancel·Status INIT를 canonical RAM 상태로 재구성하는 제한 공급이다. 신규 prefix/scanner/native/compiler/checker와 변경 pure helper만 검증한다. Pending metadata·혼합 home 독립성의 읽기 전용 분석을 Pending repair·물리 REDO 성공으로 집계하지 않는다. scanner의 EOF 전수·긴 반복·확대 profile 조합은 별도 S5 selector에 유지한다. [선택 실행](../evidence/dev3-recovery-prefix-20261022.md)은 신규231의 완료 단언(완결 job223·실패 job 내8)과 native31·compiler8+control1·checker15·변경 RS6을 source별로 보존하며 제한 단위 최종 D1을 수용했다.

[실제 닫힌 복구](../contracts/dev3-recovery-effects.md)는 제한 구현설계 D2와 첫 실행 gate 뒤 새 BOOT·source 안정화·Pending repair·예약 한 번 인계·page별 REDO와 직접 변경 수신만 [선택 검증](../evidence/dev3-recovery-effects-20261023.md)했다. 초기 semantic75 후보에서 exact393/16함수(semantic·BOOT·named104+fault289), native31/95worker, compiler9+control1, checker30을 동결했다. 결과는 완결 job386·원 실패 job 내 완료 단언7과 source별 원 FAIL/최소 재선택을 구별한다. 제한 단위 최종 독립 D1을 수용했다. 기존 완료 prefix/SELECT·하위 wrapper를 반복하지 않았으며 S5 runtime matrix3504/alias36/추가3468·native matrix63/alias7/추가56은 작성·NOT_RUN으로 유지한다.

새 장벽의 핵심 정상·거절·소유 인계만 먼저 확인한다. 실제 crash/reopen이 아직 연결되지 않은 부분은 메모리 포트의 상태 전이 증거로 표시하고, 그 소비자가 연결될 때 아래 실제 reopen 의무를 닫는다. 공통 controller를 준비하되 완성되지 않은 하위 결과를 성공 복구로 집계하지 않는다.

| 하위 작업 | 중간 필수 사례 | 함께 추가할 입력·관측 |
| --- | --- | --- |
| 3.12.1 SELECT/BOOT_RESERVE | 정상 selected closure; 낮춘 신규 admission과 원래 recovery profile 분리; 부족/미지원 설정은 첫 DB write 전 거절 후 지원 설정으로 재개; 손상 profile은 별도 거절; admission 닫힘 | namespace 후보·write 미시도·bootstrap 예약 ledger·이미 admitted된 의무의 보존 |
| 3.12.2 DWB_REPAIR | 선택 pending의 두 file 복원·모두 sync 뒤 retirement; pending 없는 손상 거절과 완전 INIT+연속 WAL 예외 | 실제 torn image·file별 sync·retirement cut, raw retired image mutant |
| 3.12.3 ANALYSIS/RESERVE_RESTORE | capture 뒤 prefix에서 winner/loser·잔여 obligation 재구성; stale home 배제; REDO 전 정확히 한 번 인계 | 실제 파일 bytes/page token/소비 CLR를 별도 계산한 ledger |
| 3.12.4 REDO | 다중 page 중 일부 home만 반영된 그룹의 대상별 skip/apply; incarnation mismatch·corrupt complete WAL/gap 거절과 허용 active tail 구분; empty DPT 시작점·WAL-before-page | 부분 반영 page 집합·전체 그룹 skip mutant·source 보존 hash·F01 시작점 기대값 |
| 3.12.5 REBASE | 두 슬롯/명시 previous·공통 fallback에서 완전 closure 보존; selector 불명 old/new 합집합; 고정 peak·새 WAL 의존 없음 | 슬롯 선택 전후 재진입 fixture·charge/retention 이력 |
| 3.12.6 LOG_OPEN | epoch/header sync와 WRITER 영속 선택 전 CLR 금지; 선택 직후 재시작에서 같은 새 epoch 발견 | 실제 namespace/sync trace·WRITER 선택 cut |
| 3.12.7 UNDO/VERIFY | loser의 남은 chain만 undo; CLR 효과+progress 재적용에서 중복 없음; AbortEnd 전 성공 금지 | 부분 CLR·winner ACK·남은 예약의 독립 기대값 |
| 3.12.8 STABILIZE/READY | 장벽 실패 시 READY 금지; 성공 뒤 새 ACK→두 번째 reopen 보존; 연결한 경로의 재장애·소형 full audit | 외부 admission/ACK 관측, controller cut 도달 기록·재현 bundle |

각 장벽의 모든 slot/owner/batch/profile 조합과 반복 장애 캠페인은 S5에 둔다. 다만 이번에 구현한 영속 선택 자체가 특정 중간 cut에서 깨지는지 확인하는 핵심 사례는 최종까지 미루지 않는다. 제품의 복구 성공을 선언하는 실제 연결에는 새 프로세스·현재 build의 복구가 필요하다.

<a id="direction-integration"></a>
### 5.1 방향 리뷰의 통합 목표별 시험 배정

[D3-DIRECTION-001](dev3-implementation-plan.md#direction-review-adoption)의 완료 조건을 기존 NT owner에 배정한다. 아래 DIR01–05는 명세 추적 ID이며 실행 가능한 test 이름·추가 완료 결과가 아니다. 실제 구현과 함께 [case inventory](#case-inventory)에 source·정확 selector·fixture·oracle·중간/S5 변형을 등록한다. 기존 인수·seed·cut·최종 budget은 줄이지 않고 같은 실행의 assertion은 중복 실행 없이 연결한다.

| ID·owner | 구현 동반 중간 필수 | 최종 S5 확대·판정 경계 |
| --- | --- | --- |
| DIR01 · NT12/03/05 | 실제 reserve/cancel·Status INIT/flush producer의 정상 prefix, WAL만 durable, 두 home 혼합, Pending/torn, retired raw 슬롯, repair 재장애를 새 프로세스에서 복원·검산. 미지원 prefix 거절·닫힌 admission·원 WAL 보존을 확인 | 장벽별 전체 cut/profile/batch 조합. 제한 inspection을 READY/ACK 결과로 집계하지 않음 |
| DIR02 · NT05/09 | 단일 주소 권한·기존 clean reader 전환/접근 배제, 실제 frame 수를 넘는 작은 집합의 eviction/reload, 반복 Status 변경, stale completion/redirty·pin/claim 경쟁 | batch1/8/64·다중 file·전체 page 종류/압력 profile. 단순 target 상수 증액을 pool 인수로 보지 않음 |
| DIR03 · NT06/09/10/11/12/15 | 실제 영속 table/anchor의 작은 행 거래에서 no-force ACK 보존·steal loser 제거·statement 이전 결과 보존/full abort·UNKNOWN·부분 CLR 재장애. 정상 admitted quota 경계의 완료·REBASE/epoch 인계 peak와 새 ACK 뒤 재시작을 독립 이력/ledger로 확인 | 전체 row/index/catalog·isolation·CLR/REBASE 조합과 긴 chain. 설정 부족/손상 거절과 정상 복구 성공 분모를 분리; terminal queue 등 미공급을 N/A 처리하지 않음 |
| DIR04 · NT01/03/12/16 | 새 인수 profile/성장 코드가 생길 때 tuple 지원/거절·checked 산술·FILEMAP 배치·intent bound·실제 예약 한도와 작은 fixture 생성기 known-bad를 확인 |64GiB 전체 회계·실제 allocator/row/index/locator 각각4배 접근·대형 생성/장치 I/O. 문서 산술을 제품 지원이나 실제 공간 증거로 세지 않음 |
| DIR05 · NT04/05/10/11/16 | 새 계측 경로의 정상/실패/취소·UNKNOWN·충돌/거절 분모, 구간 중복·trace 잘림·실제 batch 수를 작은 고정 입력으로 검산 | PF09 전체 workload·지연/처리량·계측 비용과 장기 자원 profile. 미계측0·성공만의 분모로 비용을 숨기지 않음 |

새 연결의 필수 사례를 구현 시 실행하되 기존 하위 시험을 통째로 다시 호출하지 않는다. 현 시점 DIR01–05는 미작성/미실행이며 문서 반영만으로 제품 테스트를 시작하지 않는다. S5의 확정 DEV3 inventory 전수 예외는 그대로 유지한다.

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
| 신규 NT 묶음의 미공급 영역 | allocation native/compiler/resource는 위 기록에 공급; 운영 dirty/flush, live tree/undo·index/DDL, 일반 commit/recovery/audit 등의 시험은 해당 기능과 함께 추가. 기존 표현/모델 결과로 대신하지 않음 |

<a id="case-inventory"></a>
### 6.1 의무에서 실제 실행 case까지의 목록

NT16의 inventory는 **기능 구현과 함께 채우는 실행 명세**다. 이 문서가 완성 runner나 확정 case 개수를 제공한 것은 아니다. 기존 시험 목록/receipt 형식을 우선 사용하며 별도 범용 orchestration 도구를 선행으로 만들지 않는다.

| 필수 필드 | 등록할 값 |
| --- | --- |
| identity·소유 | 안정된 case/variant ID, 주 NT owner 하나, 연결할 모든 상세 case/요구·DS/RC/KU/PF·FT ID. ID 없는 인수 행은 문서 anchor+요구/행 이름으로 연결 |
| 적용·시점 | 실제 코드/의미 분기·선행 capability, 중간 필수 또는 S5 확대. 공통 case 참조·미공급 통합 의무·비적용 사유와 계약 근거 |
| 실제 선택 | package/target/전체 test 이름 또는 지원 selector, 함수 내부 parameter/loop 포함 **실제 실행될 ID 집합과 개수**, build/feature·runner revision |
| 입력·정답 | fixture/seed/corpus/generator hash, typed 독립 기대값·assertion별 검출 대상, 정상 복구/예상 거절 구분, fault model/cut/schedule와 도달 조건 |
| 변형·반복 | profile·batch·대상·경계값의 명시 셀, 결정적/비결정적 구분·반복 index, 동치 합침의 근거와 검토. 미확정은 미확정으로 남김 |
| 비용·결과 | build/fixture/실행/audit/cleanup 예상 비용과 산정 근거, 실제 경과/CPU·disk/RSS/FD 상한, timeout 처리, source/binary/환경/receipt identity |

실행 셀은 `case + variant + seed/cut/schedule + 반복 index + fixture/build/feature/환경 identity`로 구별한다. 같은 source에서 같은 셀을 여러 NT/FT가 요구하면 한 번 실행하고 assertion별 결과를 연결한다. 서로 다른 입력·환경·반복은 별도 셀이다. V2 seed가 V3 목록에 들어 있다는 이유만으로 operation 수·profile이 다른 실행을 같다고 합치지 않는다. compiler 정상 baseline도 같은 probe crate/feature/toolchain이면 함께 준비하되 각 probe의 실제 성공/의도한 거절은 확인한다.

확정 전 양방향 대조를 한다. **상세 요구·인수 행·DS01–06 required_variants·RC-T12–14·KU09/10·PF09 → case**, 그리고 **case → 실제 selector와 독립 assertion**이 모두 있어야 한다. REVIEW-009/010과 §3.7의 보완도 포함한다. 실행 가능한 함수만 발견하여 만든 목록은 미구현 의무를 빠뜨리므로 전체 inventory가 아니다. 계획 단계의 미정 수치는 최종 실행 전에 owner와 독립 검토자가 동결한다.

selector/집계기 구현 때에는 작은 가짜 결과로 `0건·오타·예상 밖 실행·중복/누락 ID·wrong binary·cut 미도달·timeout·중단·손상 receipt`의 거절과 정상 수락을 검증한다. 단순 합계가 아니라 **예상 ID 집합과 관측 ID 집합의 일치**를 판정한다. 의도한 corruption 거절은 그 거절 case의 성공일 수 있지만, 정상 reopen 의무의 실패를 대신할 수 없다. compile-fail probe는 baseline 성공 후 의도한 타입/수명 오류를 확인하며 무관한 syntax/import/toolchain 실패는 성공이 아니다.

<a id="status-home-flush"></a>
### 6.2 3.03.2.b Status home flush의 확정된 신규 입력

[D3-STATUS-FLUSH-001 소유 계약](../contracts/dev3-status-home-flush.md#inventory)의 구현 동반 입력이다. NT03/NT05를 연결하며 기존 .a·allocator-only의 무변경 사례를 중간에 반복하지 않는다. [source별 선택 실행](../evidence/dev3-status-home-flush-20261022.md)은 필수43·compiler6·native3의 실제 결과와 원 FAIL을 보존한다. 입력 채택과 actual layout·독립 D1·실행 결과를 구분하며 S5 추가588은 NOT_RUN이다.

| 선택 분모 | 실제 입력과 시점 |
| --- | --- |
| 중간 필수43 | BN6+BA10+BH10+BR6+PF8+RF3. Rust42행과 Python RF03 witness1행을 구별; named variants는 각 행에 정확히 연결 |
| RF03 | valid witness+known-bad12, 합계12개의 제품 mutant 실행으로 세지 않음 |
| 최종 S5 추가588 | physical570=3profiles×AS/SA×(before31+after31+prefix11+short11+Interrupted11) 중 PF01–05/PF08의 exact alias6을 제외한564; cancel1/2/8/31×profiles3×AS/SA history24 |
| unique runtime/witness631 | mandatory43+S5추가588.637은 alias 전 합계이며 실행수나 unique 분모로 사용하지 않음 |
| compiler semantic6+control1 | BC00 positive consuming/Before reuse/release/resplit, BC01–05 move/borrow/privacy/raw-authority 거절. 별도 무수정 baseline1은 semantic6에 포함하지 않음 |
| native3 | profile1/2/3의 새 actual DB, exact current workflow·WAL/DWB/two homes와 namespace/FD/effect 검산. DB별30초/24MiB·총72MiB; 현재-build recovery 증거와 구별 |

[독립 fixture](../../tests/fixtures/dev3-status-home-flush/manifest.json)와 [runtime 선택표](../../crates/engine/tests/fixtures/dev3-status-home-flush/runtime/manifest.json), [compiler 입력](../../crates/engine/tests/fixtures/dev3-status-home-flush/compiler/manifest.json)을 source/binary/helper hash에 결합한다. `XADB_STATUS_FLUSH_CASES`는 exact case를 선택하며 mandatory와 ignored S5 selector가 분리된다. BEGIN→variant→CASE 및 COMPLETE의 실제 ID/순서·개수와 libtest 결과를 함께 검산한다.0개 실행·다른 binary·누락/중복·timeout은 실패다.

필수 aggregate120초/S5 aggregate1200초/compiler선택30초에 build/setup은 포함하지 않고 별도 기록한다. actual thread의 각 ACK/release는5초로 제한하며 watchdog 종료를 성공으로 세지 않는다. namespace FileIdentity 교체, 실제 whole retirement 뒤 poison, original lower cause/effect/certainty, full bytes와 known/possible exact selected references, failed continuation의 I/O0을 필수로 검산한다. 일반 header/checksum 실패를 checksum-valid semantic 손상이나 healthy zero/exact-home 경계의 대용으로 쓰지 않는다. native worker/harness의 실제 profile별 tuple은 별도 동결하고 미작성·미실행은 PASS로 승격하지 않는다.

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

<a id="cost-control"></a>
### 7.1 조합·준비·실행 시간의 낭비 방지

“전체 변형”은 위 inventory에 동결한 **필수 셀 전체**다. 모든 테스트에 `opcode×profile×batch×격리×fault×seed`를 무조건 곱하지 않는다. 소유 계약의 명시 조합은 그대로 유지하고, 추가 조합은 상호작용하는 상태·자원·실패 경계와 근거를 적어 선택한다. 이미 요구된 셀/유한 모델 공간/보존 corpus를 pairwise나 대표 한 건으로 축소하려면 동등 검출 근거의 별도 D2 검토가 필요하며, 이번 보완은 그런 축소를 승인하지 않는다.

| 비용 발생 지점 | 실행 계획 |
| --- | --- |
| 같은 불변식의 다중 분류 | NT/DS/RC/FT는 같은 case를 참조. 모델·native·thread·reopen은 구별하고, 동일 native 시나리오의 다중 assertion은 한 실행으로 집계 |
| 반복 build·fixture 생성 | 동일 source/toolchain/feature의 binary를 한 번 준비. 동결한 fresh fixture 원본은 불변으로 보관하고 mutation/fault마다 격리한 사본 사용. 앞 case가 변경한 DB를 다음 독립 case의 초기값으로 재사용하지 않음 |
| 중간 대형 경로 확인 | 작은 실제 split·overflow·spill/merge·quota fixture로 분기와 정답 확인. 시험용 한도 조정은 production 한도 증명을 대신하지 않으며 실제 최대값 사례와 F05를 유지 |
| 대형4배/audit/backup | 동일 profile의 fixture가 각각의 실제 접근 집합4배 조건을 모두 만족할 때 준비물을 공유. zero padding·sparse apparent size·반복 I/O는 집합 크기에 산입하지 않음. 각 fault 후 필요한 audit는 유지하고 같은 불변 결과의 audit 중복만 제거 |
| 큰 profile 준비 실패 | 기본8GiB로 allocator16/64MiB buffer의4배를 만족할 수 없으므로64GiB qualification의 형식/회계 동결과 host 실제 공간을 먼저 확인. 부족하면 NOT_RUN/미완료이며 축소 fixture로 PASS하지 않음 |
| 장애 반복 | 결정적 셀은 정한 입력/schedule을 한 번 완주하되 소유 계약의 명시 반복은 유지. 비결정적 필수 fault 셀은3회·각 fresh fixture. 실패 후 성공할 때까지 자동 재시도하거나 모든 결정적 셀을 관행적으로3회 반복하지 않음 |
| 긴 fuzz/stress·PF09 | 정확성·harness known-bad를 먼저 확인하고 장시간 profile은 S5에서 실행. CPU/I/O/RSS 간섭을 받는 성능 job과 대형 fixture/audit·부하 job을 겹치지 않음 |
| 조기 실패·수정 후 재검증 | 실패 case와 자료를 보존하고 그 결과에 의존하는 비싼 job만 보류. 무관한 준비/검토는 진행 가능. 미실행은 NOT_RUN, 탐색 한도/미도달은 INCONCLUSIVE 또는 해당 실패로 남김. 수정 후 실제 영향 셀만 다시 실행 |

실행 전에 job별 `준비 + 셀별 예상 시간×반복 + audit + cleanup`을 합산하고 disk/FD/RSS 동시 peak를 기록한다. 예측 근거가 없으면 미측정으로 표시하고 작은 필수 실행에서 얻은 실제 시간으로 갱신한다. 현재 총 실행 시간이나 절감률을 수치로 주장할 근거는 없다. §7 profile에 해당하는 fuzz target 수를 F, 동시성 suite 수를 T라 하면 **fuzz/stress의 최소 관측 시간 합만30×(F+T)분**이고 build·fixture·property·fault·PF09 시간은 별도다. 이는 병렬 실행 후의 wall time 추정이 아니다. owner 계약의 더 긴 budget은 유지한다.

timeout은 성공을 위한 축소 허가가 아니다. 동결한 한도 도달의 원인·남은 의무를 기록하고, 최종 전체 inventory를 닫기 전까지 DEV3 인수는 미완료다. 소유 계약상 성능 비교가 필요한 변경에는 DP07의 동일 조건 최소10쌍 등 해당 의무를 적용하고, 일반 correctness case에 임의10회 반복을 붙이지 않는다.

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

<a id="review-decision"></a>
## 10. D3-TEST-PLAN-002 — 항목별 누락·중복·비용 보완

| 항목 | 기록 |
| --- | --- |
| 분류·상태 | D2 · ACCEPTED. `/root/dev3_test_plan_review` 독립 읽기 검토 PASS·통합 담당 반영 확인. 기존 계약의 실행 선택·추적 명세 보완이며 제품 의미·최종 필수 셀/budget 축소 없음 |
| 일자·권한·소유 | 2026-10-22. 사용자의 DEV3 테스트 설계·항목별 필수/낭비/누락 점검 및 개선 요청. 작업·계약·통합 `/root` |
| 선택·대안 | 작은 독립 경계는 중간 필수, 큰 데이터/긴 관측/명시 전체 조합은 S5 유지. 중복은 공통 실행/fixture 준비를 공유하여 제거. 모든 조합의 무조건 곱과 위험 경계를 대표 정상 한 건으로 대체하는 방식은 채택하지 않음 |
| 적용·대체 | D3-TEST-PLAN-001의 원칙·최종 예외와 REVIEW-009/010을 유지하고 §1.1·3.7·5·6.1·7.1 및 NT10/11을 상세화. 최초 결정/검토 identity·과거 실행 결과는 원래 범위로 보존 |
| 검토·기준 | [재점검 기록](../evidence/dev3-test-plan-review-20261022.md)의 기준 source·항목별 근거와 독립 검토를 따름. 기존001의 PASS를 이번 보완에 승계하지 않음 |
| 실행 | 문서만 수정. 테스트/harness 구현·제품 시험·compiler/Clippy·문서 자동 검사·commit/push 없음. 후속 구현/최종 인수 상태 변경 없음 |
| 재검토 | 새 의미 분기·runner 내부 숨은 전수·필수 셀 누락/중복·oracle/fixture 영향 누락·자원/시간 예상 초과·source 변화로 동일성 확인 불가 |


<a id="allocator-residency-inventory"></a>
## 11. allocator 동일 주소 reload의 구현 동반 inventory

2026-10-23 [소유 계약](../contracts/dev3-allocator-residency.md)의 중간 선택은 신규 runtime19·Fixed mode 직접 소비2·static4와 native1/compiler3/checker10이다. 정확 selector와 원 FAIL/NOT_RUN·마지막 성공 source는 [실행 색인](../evidence/dev3-allocator-residency-20261023.json)이 보존한다. 최종 S5에 ignored `storage::wal_io::bootstrap::doublewrite_install::mutable_storage::allocator_residency::tests::reload_s5_mixed_count_epoch_history`를 추가한다(count1..8×4회,32round/64reload). 확대는 작성·NOT_RUN이며 이번 선택으로 최종 DEV3 전수를 대신하지 않는다. 무변경 하위 wrapper는 중간에 반복하지 않는다.

<a id="post-durable-snapshot-inventory"></a>
## 12. 영속화 이후 snapshot 경합 수정의 inventory

2026-10-23 D3-OCR-001 수정에 동반한 [신규5 case와 정확 selector](../evidence/dev3-code-review-20261023.md#fix-inventory)는 NT03 소유, NT04/05·FT03/05/09 연결이다. reserve/cancel 각각의 flusher content lock 경합2·잠금 해제 뒤 정상2·Status 예약 wrapper1을 profile1/count1·실제 thread ACK로 고정한다. 5건 모두 중간 필수이며 최종 S5에 포함한다. 기존 dirty53·Status flush43의 분모와 과거 결과는 유지한다. 수정 전 반례 FAIL과 수정 후5 PASS는 [후속 실행 기록](../evidence/dev3-code-review-20261023.md#fix-followup)에 보존하며, 관련 diff가 없는 중간 단계에서는 반복하지 않는다. 이 추가가 native crash 복구나 전체 DEV3/S5 완료를 뜻하지 않는다.


<a id="operational-registration-followup"></a>
### 일반 page 등록 코어 P1 후속 inventory

2026-10-23 [D3-OPERATIONAL-PAGES-001 P1](../contracts/dev3-task-05-buffer-flush.md#operational-registration)은
기존 directory의 슬롯별 publication과 세대 준비를 직접 검증한다. 신규 selector는
`storage::dirty_frame::registration::tests::slot_wrong_generation_does_not_publish`,
`slot_concurrent_publication_has_one_exact_winner`, `slot_exclusive_replacement_keeps_cells_independent`다
(뒤 두 이름도 같은 module 경로). 최종 S5 확정 inventory에 이3개를 포함한다.

중간에는 source별 exact selection의 static5·runtime20·Linux2만 실행했다. 기존 CF06-08은
공유 경로에서 이제 불가능한 generation0 주입 대신 잘못된 kind를 주입하며, generation
거절은 새 selector가 publication 전 무효과를 검사한다. 기존 Status CF12의 공개 전 C1–C5와
공개 후 reader를 변경 소비자로 선택했다. [원 증거](../evidence/dev3-operational-registration-20261023.md)를
보존하며 하위 wrapper·무변경 시험·S5는 반복하지 않는다. P2/P3의 consuming API·반복 INIT·
실제3주소/2frame 및 반복 live Status inventory는 해당 구현과 함께 추가해야 한다.

<a id="allocator-eviction-followup"></a>
## allocator 실제 비상주 구간의 중간·S5 배정

2026-10-23 [소유 계약](../contracts/dev3-allocator-residency.md#eviction-lifecycle)의 새
`eviction::tests`에서 중간 필수18건과 layout1건을 exact 선택한다. 실제 Linux 이력1건과
새 compiler3+baseline1을 동반한다. 변경된 종전 reload/Fixed 경로7건·추가 자원4건의
source별 결과는 [선택 증거](../evidence/dev3-allocator-eviction-20261023.md)를 따른다.
완료한 동일 코드 시험·하위 wrapper를 재실행하지 않는다.

최종 S5에는 기존 확정 inventory와 함께
`storage::wal_io::bootstrap::doublewrite_install::mutable_storage::allocator_residency::eviction::tests::eviction_s5_three_slot_history`
를 추가한다. ignored32round/64reload는 작성·NOT_RUN이며 중간 선택에 포함하지 않는다.
다른 주소·N+1·Status 반복 변경·pressure의 일반 pool 시험 의무는 그대로 남는다.

<a id="operational-status-followup"></a>
## P2 live owner의 변경 범위 검증

[P2 계약](../contracts/dev3-task-05-buffer-flush.md#operational-status-p2)의 consuming owner,
반복 INIT·generation claim·trusted population 변경에 대해 새 literal·actual owner·Linux·
compiler fixture를 함께 공급한다. [선택 증거](../evidence/dev3-operational-status-20261023.md)는
source별 완료 결과와 compile 실패를 보존한다. r2 static6/runtime2를 r4/r5의 별도 경계 추가나
전체 source 지문 변경만으로 반복하지 않는다. closed CF15-01/02는 달라진 population·INIT
권한의 직접 소비자라 좁혀 선택한다. 전체 wrapper·하위 회귀·S5는 실행하지 않는다.

live owner 인수 당시 반복 sequence current-build recovery는 미구현이었다. 후속
[반복 이력 복구](#operational-recovery-followup)가 새 producer→별도 process 복구·partial-prefix/fault cut를
공급했다. 원 live owner/closed recovery PASS를 새 실행 증거로 대신 쓰지 않는다. P3 load 통합은 남아 있다.
순수 reader LSN 의미 시험은 실제 P3 A reload로 세지 않는다. 긴 전체 조합·pressure는 S5
배정을 유지하며 이번 선택 실행을 DEV3 전수 인수로 승격하지 않는다.

최종 S5 확정 inventory에는 이번 신규14개 runtime selector와
`storage::wal_io::bootstrap::doublewrite_install::mutable_storage::status_init::status_flush::tests::native::operational::worker_operational_status_native`,
compiler manifest의semantic5, 새 checker4를 포함한다. runtime14의 정확 이름은
[해당 Rust 입력](../../crates/engine/src/storage/wal_io/operational_status_tests.rs)의
`operational_status_resource_gate`를 제외한14개 `#[test]`이며 정적 gate1은 별도로 포함한다.
source별 선택 원문은 [receipt 색인](../evidence/dev3-operational-status-20261023.json)의
r2/r4/r5/r7 selection에 연결한다. 기존 closed CF15-01/02는 원 inventory와 중복 집계하지 않는다.
첫 extent 전체 주소·여러 profile·long history 확대는 아래 복구 후속 inventory에서 작성 상태와
실행 결과를 구별한다. P3 실제 load/교체·반복 변경 확대는 해당 구현과 함께 추가한다.

<a id="operational-recovery-followup"></a>
## 반복 Status 복구의 변경 범위와 S5 inventory

[F09](../contracts/dev3-recovery-effects.md#operational-status-recovery)의 immutable Status survivor·
주소별 census·REDO·topup 변경에 대해 신규65개 boundary/11selector와 직접 소비자
ES003·ES005·PPL004만 선택했다. source별57+8 PASS를 합산하며 같은 source의 완료 시험을
다시 실행하지 않는다. native4개 이력/14worker, 새 compiler EC09–11+baseline 및 checker의
원 실패/수정을 [증거](../evidence/dev3-operational-recovery-20261023.md)에 보존한다.

정확 selector·case ID·시간/공간 한도는 [구현 동반 inventory](../../tests/fixtures/dev3-operational-recovery/manifest.json)가 소유한다. 최종 S5에는 mandatory65·native4·compiler3·새 checker11·변경 compiler checker2·static2와 다음 확대2379개를 포함한다. 기존 소비자3개는 원 inventory와 중복 집계하지 않는다.

| 확대 selector | 실제 작성 범위 | 상태 |
| --- | --- | --- |
| `s5_profiles` | profile2/3 각각63개 INIT·62개 cancel; profile1의 같은 전체 extent는 mandatory 경계에 포함 | NOT_RUN |
| `s5_long_history` | profile1,2pages,4096cancel,work4098/LSN8196 | NOT_RUN |
| `s5_home_pending_matrix` | 3profile×8home subset×6historical Pending =144 | NOT_RUN |
| `s5_home_fault_matrix` | 3profile×home subset별 dirty target 합12×31call×Before/After =2232 | NOT_RUN |

확대 fault vector는 source protocol의 BOOT/source39개 뒤 dirty target마다31개로 사전 작성한다. 실제 성공 trace에서 역산하지 않으며 fault 위치의 exact event tuple·누적 effect·held failed owner·source/clean home 보존을 검산한다. 기존 하부 BOOT39 fault를 이 확대 분모로 다시 세지 않는다. 각 ignored selector는 명시 선택하며 준비 fixture는 총256MiB, 확대 보존물1GiB, 실행 합1320초 한도를 유지한다. 이 S5 준비는 실행 PASS가 아니며 일반 recovery/READY·P3·전체 DEV3는 미완료다.

<a id="status-load-followup"></a>
## P3 실제 Status home load의 선택 inventory

[소유 계약](../contracts/dev3-task-05-buffer-flush.md#operational-status-p3)·[실행 증거](../evidence/dev3-status-load-20261023.md)·
[정확 manifest](../../tests/fixtures/dev3-status-load/manifest.json)를 따른다. 중간 runtime24는
실제 A/B/empty/다음 INIT, dirty allocator 보존, 잘못된 membership/손상/누출 claim/fence와
source에서 작성한10개 I/O 오류 지점을 포함한다. native1은 fresh Linux A→B→A, exact selected
read·전체 파일 독립 oracle·FD/Drop을 확인한다. compiler3+별도 baseline, resource static3,
새 native checker5와 변경 compiler inventory checker1을 구별한다. 기존 완료 시험은 반복하지 않는다.

S5 확대43은 profile2/3 load2·32round1·2profile×10call×2error/panic40의 ignored selector다.
최종 S5에는 중간 inventory도 포함하며 이 작성 상태는 NOT_RUN이다. 최초14call 초안은
handle-bound open의 metadata를 이중 계산했다. 존재하지 않는4 runtime FAIL과16 S5 NEVER_RUN의
철회 사유/원본을 보존하고 성공으로 바꾸지 않는다. 수정된 실제10tuple을 별도 oracle로 확인한다.


<a id="begin-preparation-followup"></a>
## 최초 빈 거래 Begin 준비의 선택 inventory

[소유 계약](../contracts/dev3-obligation-format.md#begin-preparation)과
[정확 manifest](../../crates/engine/tests/fixtures/dev3-status-begin/manifest.json)를 따른다.
새 producer/observer/materializer의 필수12개는 독립 raw literal 전체 일치, 슬롯/세대/현재 epoch와
local 경계, page prior와 global predecessor 구별, 정확 초기 예약량, 각 section 손상,
비선택 home 보존, 모든 frame proper prefix/trailing, 입력 불변을 검산한다.
layout1은 caller loan을 별도로 표시한 component 상계를 확인한다.
[선택 결과](../evidence/dev3-status-begin-20261023.md)의 r1에서만 실행했으며 기존 RollbackStart와
하위 wrapper·무변경 시험은 반복하지 않았다. 순수 준비에는 native I/O/borrow probe가 해당하지 않는다.

S5에는 필수12와 static1에 더해 profile3×slot71×generation3의639조합을 작성했다.
`begin_s5_profiles_slots_and_generations`는 ignored/NOT_RUN이며 raw vector를 순차 생성·재사용한다.
각 필수30초·전체120초, S5 전체120초와 fixture1MiB·시험 결과보존8MiB의 사전 한도를 유지한다.
source export·ELF·compiler/cache의 별도 보존은 소유 계약의 범위 명시를 따른다.
실제 발급·directory census·stage/apply·rollback/terminal·현재-build 복구에는 별도 native/fault/compiler
inventory와 F05가 필요하며 이 순수 시험을 그 실행 증거로 승격하지 않는다.

<a id="bounded-target-set-followup"></a>
## 전체 대상 pin 집합의 선택 inventory

[소유 계약](../contracts/dev3-task-05-buffer-flush.md#bounded-target-set)과
[정확 manifest](../../crates/engine/tests/fixtures/dev3-target-set/manifest.json)를 따른다.
새 필수15는 N3/N64 정상, 전체 duplicate/missing/quota, late Busy/poison/stale,
원자 release·unwind fence-before-unlock·실제 reader/writer thread·stamp/인덱스와 기존 wrapper를
확인한다. static2와 직접 소비자4만 추가 선택하며 무변경 하위 wrapper는 실행하지 않는다.
새 compiler는 baseline1+positive1+borrow negative3+capacity negative2를 분리한다.
새 checker3는 입력/실행기 provenance·정확 단일 진단/instantiation·const assertion 위조를 검산한다.
원 실패와 수정 후 해당 사례만의 재검증은 [source별 결과](../evidence/dev3-target-set-20261023.md)를 따른다.

최종 S5에는 중간 새15·새 static1·compiler semantic6·checker3와
`target_set_s5_expanded`의 N1..64×4순서256, N64×late64×read/write128,
N64×stale64×release/read/write192의576조합을 포함한다. 관련4·기존 layout1은 원 inventory와
중복 집계하지 않는다. 확대는 ignored/NOT_RUN이며 일반 load/pressure 증거가 아니다.
각 runtime30초·합180초, compiler120초·checker30초·S5 합180초,
활성 fixture2MiB·case/review16MiB와 별도 source/ELF/compiler/cache 보존을 유지한다.


<a id="resident-status-followup"></a>
## 여러 Status 상주 수명의 선택 inventory

[소유 계약](../contracts/dev3-task-05-buffer-flush.md#resident-status-pool),
[최종 manifest](../../crates/engine/tests/fixtures/dev3-resident-status/manifest.json)와
[source별 결과](../evidence/dev3-resident-status-20261023.md)를 따른다. 신규31은 실제 INIT/flush/
퇴출/load, index/claim/permit·generation·forgotten reservation/pin·activation retry·read shape/
alias/Busy/poison/semantic·unwind·실제 thread 정리를 확인한다. 공통 binder/INIT/reader/flush/indexed
loader의 직접 소비자4만 관련 선택하며 완료된 하위 wrapper는 반복하지 않는다.

static2는 mock/native actual Writer의 자원 census, compiler9는 baseline1+positive2+negative6,
checker8은 borrow provenance/진단2+native protocol/full-image/dirty-home3+실패 retention/build-log2+
최종 정리 시간1이다. native1은 새3frame/4주소 producer→별도 recovery→reentry의3PID다.
최종 S5에는 신규31·static2·compiler semantic8·checker8·native1과 별도6 ignored selector를 포함한다.
확대는 profile2/3×N4/8/64, 모든 victim146·victim×late-content8200개이며 NOT_RUN이다.
관련4는 원 inventory와 중복 집계하지 않는다. N64+1은 첫 extent 범위 밖의 잔여다.
각 runtime30초/합240초·native history30초/합120초·DB24MiB/보존96MiB·compiler180초·checker30초,
활성 input8MiB/case-review32MiB와 별도 source/ELF/cache 보존을 유지한다.


<a id="status-pressure-followup"></a>
## Status 자동 상주 후보 선택의 구현 동반 입력

2026-10-24 [소유 계약](../contracts/dev3-task-05-buffer-flush.md#status-residency-pressure)과
[확정 manifest](../../tests/fixtures/dev3-status-pressure/manifest.json)에 신규11함수33scenario,
static3·checker7·native1이력3PID를 등록한다. [source별 증거](../evidence/dev3-status-pressure-20261024.md)는
원 r8 ledger oracle FAIL·완료한 producer/recovery와 수정 checker의 저장 결과 재검산·새 reentry를
구분한다. 같은 실행 ELF를 쓰며 완료 시험을 반복하지 않았다. artifact runner 자체시험3은 제품 인수와 별도다.

최종 S5에는 `empty_pressure_s5_profiles_slots_and_obstruction`의 profile1/2/3×frame4/8/64×모든
후보 위치×ordinary/MAX generation438을 포함한다. Empty slot의 MAX도 실제 설정하여 다음 후보로
넘어감을 검산한다. 현재는 ignored·NOT_RUN이며 S5 전수 inventory에서 누락하지 않는다. frame64는
첫 extent 안의62상주+1empty·63주소를 쓰므로65주소 N+1·다음 extent 인수를 대신하지 않는다.
중간에는 새 selector의 필수 사례만 선택하며 기존 load/flush/복구·전체 wrapper를 재실행하지 않는다.

<a id="empty-transaction-followup"></a>
## 빈 거래 통합의 준비 inventory

[§17](../contracts/dev3-obligation-format.md#empty-abort-preparation),
[정확 manifest](../../crates/engine/tests/fixtures/dev3-empty-abort/manifest.json),
[source별 증거](../evidence/dev3-empty-transaction-20261023.md)를 따른다. 새 순수 AbortEnd 필수13과
static1, 공통 native/page binding을 바꾼 직접 RollbackStart 소비자2만 선택한다. exact 빈 예약,
상태·chain·실제 pageLSN 하한, 전체 native/redo/page 결합, 모든 proper prefix와 입력 보존,
Begin→RollbackStart→AbortEnd bytes 연결을 검산한다. 관련2는 S5 원 inventory와 중복 집계하지
않는다. 새13/static1은 S5에 포함하고 `empty_abort_s5_all_profile_slot_generations`의 profile3×
slot71×Free generation3(1/19/MAX−1)인639조합은 ignored/NOT_RUN으로 배정한다. 실제 owner/
예약/설치·native·복구·통합 F05 inventory는 아직 미동결이며 이 준비 결과로 대신하지 않는다.
각 case30초/전체120초, 활성 fixture1MiB·case/review8MiB와 별도 source/ELF/cache 보존을 적용한다.

같은 통합의 [owner 인계 계약](../contracts/dev3-obligation-format.md#empty-transaction-handoff)은
[별도 manifest](../../crates/engine/tests/fixtures/dev3-empty-transactions/manifest.json)의 필수13과
static1, compiler baseline1+새probe7, 새checker2를 선택한다. 성공·Before retry·foreign binding,
forgotten reservation/read gate/pin/actual capture·zero-pin flush claim, poison·기존/cleanup fence와
bank/원owner/retained wide Result의 독점 수명을 검산한다. 기존 완료된 순수 AbortEnd·Rollback
시험은 다시 실행하지 않는다. S5의 profile3×capacity4 인계12조합은 authored ignored/NOT_RUN이다.
이 인계는 실제 거래·복구 시험을 대체하지 않는다. 앞 단위와 같은30초/case·120초/runtime 및
180초/compiler·8MiB case 증거 한도를 적용하고 source/ELF/cache를 별도 보존한다.

실제 live 설치는 [§19](../contracts/dev3-obligation-format.md#empty-transaction-live)와
[live manifest](../../crates/engine/tests/fixtures/dev3-empty-transactions/live-manifest.json)의
신규22·static1, 공유 exact budget 추출의 직접 소비자2를 선택한다. 독립 전체 WAL/page bytes,
2/32/33거래·발급 gap/소진, aggregate admission, bank↔home·중복·resident/disk census,
stage별 오류/unwind·실제 WAL write/sync·short write, durable 뒤 잘못된 terminal/receipt/stamp를
검산한다. pending 필드 추가의 handoff static1은 변경된 layout의 직접 소비자로 한 번 실행한다.
새 compiler는 sequential live 호출과 private install 생성 거절2개+baseline이며 기존7개는
반복하지 않는다. sealed E0451의 정확4span checker1과 부정 입력을 선택한다. source별 결과는
[기존 증거](../evidence/dev3-empty-transaction-20261023.md#live-installation)에 이어 보존한다.
S5 profile3×depth(1/2/8/32)12조합은 authored ignored/NOT_RUN으로 추가한다. live flush/load와
native 현재-build recovery/reentry inventory는 후속 구현과 함께 작성하며 이 결과로 대체하지 않는다.

[§20 live history/flush](../contracts/dev3-obligation-format.md#empty-transaction-flush)는
[history manifest](../../crates/engine/tests/fixtures/dev3-empty-transactions/history-manifest.json)의
신규11개 함수와 Linux native1, 변경된 resource static3을 선택한다. exact4이미지·interleaved
거래·checksum-valid 가짜 home4·semantic14·prefix5·원장 역사 불일치·read count/short/
Interrupted·6개 DWB ordinal의 before/after12·retired finalize 재개를 검산한다. private History
compiler2는 [선택 runner](../../scripts/check_dev3_empty_history_borrows.py)로 실행하며
정확 E0451의2개 span과 실제 completion을 검증하는 신규 checker1을 포함한다. 이미 완료한
변경 없는 lower 시험은 실행하지 않는다. S5에는 위 inventory와 authored ignored
`empty_history_s5_profiles_and_selected_states`의 profile3×상태4인12조합을 추가한다.
source별 원 FAIL·수정에 따른 재선택은 [기존 증거](../evidence/dev3-empty-transaction-20261023.md#live-history-flush)를
따르며 live load·closed recovery 결과로 확대하지 않는다.

[§21 live load](../contracts/dev3-obligation-format.md#empty-transaction-load)는
[load manifest](../../crates/engine/tests/fixtures/dev3-empty-transactions/load-manifest.json)의
신규 runtime9·static1·native1과 실제 census 추출의 직접 소비자1을 선택한다. Active/Aborting/
Aborted 교체와 원 actor 계속 진행, admission, 알려진 read 오류 retry, EOF/invalid count/손상,
과거 Free, 최종 read drift/unwind, forgotten pin/poison/flush claim을 검산한다. 새 compiler2만
`--case load_sequence --case sealed_load`로 선택하고 공통 verify 변경의 checker2를 실행한다.
S5에는 `empty_load_s5_profiles_states_and_replacement`의12조합을 추가하되 중간 실행하지 않는다.
원 source별 실패·완료 선택은 [증거](../evidence/dev3-empty-transaction-20261023.md#live-load)를 따른다.


[빈 거래 closed recovery F10](../contracts/dev3-recovery-effects.md#empty-transaction-recovery)은
[recovery manifest](../../crates/engine/tests/fixtures/dev3-empty-transactions/recovery-manifest.json)의 새13함수104variant를 선택한다.
Active/Aborting/Aborted3, interleaved2, capacity3, mixed12, semantic24, ordering5, historical Pending6,
intact conflicts9, tail15, Pending exact3, bank census9, seal drift5, required body read8이다.
직접 수정된 System typed callback의 mixed9와 public readonly scope1, 변경 census2만 기존 수신 사례로 선택한다.
완료된 unchanged lower/전체 wrapper는 실행하지 않는다. 실패한 job의 완료 case는 원 source에서 보존하고
수정된 ERP003만 다시 선택한다.

Linux actual producer→새PID recovery→재진입은 native6history21worker로 동결했다. 첫5history17worker와
추가 slot partial1history4worker는 서로 다른 source 영수증이며 완료5를 반복하지 않는다. native worker는
`dev3_empty_recovery_native.py --select`의 정확 history만 선택하고 독립 전체 파일 oracle와 inode/FD를 검산한다.
compiler EC12–14와 baseline만 선택하며 EC00–11은 반복하지 않는다. 새 native checker4·compiler selection checker1 및
변경 census checker1·frozen manifest checker1을 구별한다. ignored `empty_recovery_s5_profiles_states_and_homes`의48조합은 최종 S5에 전수 배정한다.
원 source별 결과와 실패·자원 한도는 [통합 증거](../evidence/dev3-empty-transaction-20261023.md#closed-recovery)를 따른다.

<a id="undo-bootstrap-followup"></a>
## Undo bootstrap 선행 inventory

[plan3 소유 계약](../contracts/dev3-status-bootstrap.md#undo-bootstrap)과
[raw 입력](../../tests/fixtures/dev3-undo-bootstrap/manifest.json),
[compiler 입력](../../crates/engine/tests/fixtures/dev3-undo-bootstrap/compiler/manifest.json)을 따른다.
새 UB01–05의33assertion(5함수), 실제 Linux creator1, compiler4+baseline1과 신규 checker만 선택한다.
기존 plan1/2의 완료 설치·fault·borrow 시험은 반복하지 않는다. static/literal4를 실제 설치4건으로 세지 않는다.
S5의 `undo_bootstrap_s5_plan_matrix`는 profile3×wire6×high5=90에서 exact mandatory alias12를 뺀
78조합이며 [tuple 목록](../../tests/fixtures/dev3-undo-bootstrap/s5-matrix.json)으로 고정했다.
중간에는 ignored/NOT_RUN을 유지하고 최종 DEV3 inventory에는 새 필수33과 추가78을 모두 포함한다.
원 compile/Clippy FAIL과 source별 선택 결과는 [증거](../evidence/dev3-undo-bootstrap-20261024.md)에 보존한다.
이 선택은 free root INIT·mixed pool·current-build plan3 recovery의 인수를 대신하지 않는다.
