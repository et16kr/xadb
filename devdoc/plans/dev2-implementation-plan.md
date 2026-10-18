# DEV2 — 조사 결과와 구현·검증 계획

2026-10-18 · **D2 기본 설계·착수 순서 채택 / leaf 동결 의무 유지 / P0/K0 일부 구현 / 영속 경로·제품 시험 NOT_RUN**

[상세 커널 설계](../contracts/dev2-kernel-design.md) · [개발 순서](bottom-up-development.md) · [단계 JSON](development-stages.json) · [완료 계약](stage-completion.json)

DEV02는 저장소의 DEV2다. 목표는 **직접 typed API로 실제 행을 변경하고, 성공 응답을 받은 거래가 crash/reopen 뒤 보존되는 최소 영속 커널**이다. K0–K5를 차례로 통합하며 첫 착수 단위는 XADB-004 독립 정답 모델과 XADB-027 row/catalog다. 현재 P0/K0 일부와 native worker가 있다. streaming controller 초안 코드도 있으나 전용 기능 검증은 미실행이다. 영속 엔진·완료된 controller 인수·공식 배포는 없다.

<a id="research"></a>

## 1. 조사 범위와 현재 상태

기존 DEV2 설계와 P0/K0 일부 구현을 이관했다. XADB의 현재 source fingerprint로 DEV0·DEV1 회귀와 DEV2 reference/substrate/frame/worker 검증을 새로 실행했다. 영속 경로·동시 이력 탐색·streaming controller·성능 인수·30분 stress/fuzz는 미완료다. [현재 실행 증거](../evidence/README.md)

| 대상 | 현재 상태와 남은 일 |
| --- | --- |
| P0 reference | 독립 순차 이력 모델·known-bad 반례 구현; bounded concurrent exploration 미실행 |
| K0 production | 실제 volatile row/catalog·snapshot·quota·native worker 일부 구현 |
| K1–K5 | 영속 transaction/WAL/recovery/checkpoint/restore의 통합·실패 시험 필요 |
| D0/B2/PF | 경계·전체 내구성·성능 인수 의무 유지; leaf PASS로 완료되지 않음 |
| 외부 원문 | Linux fsync/rename와 Rust File locking의 계약을 [상세 설계 근거](../contracts/dev2-kernel-design.md#basis)에 연결 |

<a id="sequence"></a>

## 2. 구현 순서와 작은 통합 단위

| 순서 | task·소유 범위 | 구현할 결과 | 다음으로 가기 위한 증거 |
| --- | --- | --- | --- |
| P0 | XADB-004; 계획 `xadbtest-reference`, protocol의 순수 history 타입 | row/거래 reference, literal history, known-bad corpus | SC01: 정상 이력 수용, 6종 wrong history 거절. production 의미 import 없음 |
| K0 | XADB-027 + XADB-032 시작; 계획 engine api/catalog/storage::volatile, engine-worker | 실제 create/drop·insert/read/update/delete·schema·ID, bounded native protocol | KU02, D0 초기 경계: 실제 production 자료구조·잘못된 handle·한도·독립 기대값 |
| K1 | XADB-006; transaction/publication | private overlay, statement rollback, pin, quota, volatile 내부 publication | KU03, PA02/03: read-your-writes·비공개·old snapshot·다중 table 원자성·자원 복원 |
| K2 | XADB-007; durable::wal + foundation/ platform namespace port | lease·bootstrap·epoch reservation·L0 frame·실제 append/sync/publication | KU04, RC-T01/26: partial write·sync 불명·ACK-before-sync mutant 거절. recovery 완료 주장은 아직 불가 |
| K3 | XADB-008; durable::recovery + XADB-032 process history | production reopen, tail/손상 분류, 새 epoch 연결 | KU05/06, PA05/07, RC-T02–04: 외부 ACK→reopen·재복구→새 ACK→재crash |
| K4 | XADB-010; transaction/publication·실제 thread driver | coarse serializable 검증, statement/cancel 경합 | KU07, RC-T05: lost update/write skew/phantom·DDL 충돌·old reader 수명 |
| K5 | XADB-009; durable::checkpoint/retention + XADB-032 | exact-cut checkpoint·fallback closure·backup/restore·GC | KU08, PA06, RC-T06–11: selector별 crash·조기 회수 mutant·빈 환경 restore |
| 완료 | XADB-032 통합 + XADB-014a bundle + DEV2 PF 누적 | 전체 native history·하위 회귀·성능 관측/재현 | B2/D0 모든 의무와 PF01–07/12/13의 실제 결과. 필수 미실행/미정 0 |

XADB-032는 마지막에 시험을 붙이는 작업이 아니다. K0에서 시작해 모든 단계의 실제 API·native 이력을 확장한다. P0에서 전체 미래 동작을 구현할 필요는 없지만 SC01의 known-bad 이력 판정과 K0 operation의 독립 기대값을 먼저 고정한다. K1의 statement atomicity를 K4까지 미루지 않으며 K4는 동시 실행 검증을 확장한다.

각 K 단계에는 현재 capability의 성능 계측·비교·재현도 함께 구현한다. product task가 metric을, XADB-032가 독립 비교/replay를, XADB-014a가 제한된 증거 bundle을 담당한다. PF별 정확한 배정은 최신 [단계 JSON](development-stages.json)의 `kernel_build_steps.performance`를 따른다. 완료 행은 이 결과를 누적 확인하는 단계이며 계측을 처음 구현하는 시점이 아니다. XADB-014b 자동 축소·XADB-015 AI workflow pilot은 후속이며 K0의 선행 gate가 아니다.

각 단계는 D1/D2 해당 검토와 V2의 현재 capability 통합을 완료한다. K2에 아직 없는 K5를 요구하지 않지만, K2만 통과해 DEV2 전체를 닫지 않는다. 신규 crate는 실제 구현과 함께 추가하고 Cargo workspace·허용/실제 의존·build identity·clean export·문서 검사를 갱신한다. 현재 실행 소스 루트 `crates/tests/scripts` 안에 추가해도 각각의 inventory 누락을 확인한다.

<a id="acceptance"></a>

## 3. 누락 없이 추적할 인수 의무

[stage-completion.json](stage-completion.json)의 DEV2가 완료 의무의 정본이다. 아래 표는 구현·시험 배정이며 시험 항목을 새로 제외하는 표가 아니다.

| 의무 | 설계 연결·필수 반례 |
| --- | --- |
| SC01 | 독립 직렬 이력: lost update, write skew, phantom, partial batch, ACK 유실, 부당 UNKNOWN 허용 |
| KU02–03, PA02–03 | 실제 row/catalog·고정 snapshot·stale handle·다중 row 오류·quota ledger·old reader/drop-recreate |
| KU04, RC-T01/26 | 바이트 한도·truncate/손상·형식 거절·write/sync uncertainty·sync 전 ACK mutant |
| KU05–06, PA05/07, RC-T02–04 | 외부 ACK/ABORT/UNKNOWN, STABILIZE·복구 중 crash, process crash→새 ACK→power-loss model, 정상 복구/손상 거절/I/O 거절 구분 |
| KU07, RC-T05 | 2개 이상 actor의 고정 snapshot·coarse conflict, 실제 thread, statement/cancel의 전후 경계 |
| KU08, PA06, RC-T06–11 | exact cut·replay anchor, CURRENT/WRITER 상태표, fallback 재선택·chain, pin/retire 경합, early-delete, WAL 없는 exact-cut restore |
| PF01–07/12/13 | 아래 성능 개발 계획. 단순 TPS 출력이나 correctness PASS로 대체 불가 |

D0는 [7개 경계](phase1-product-baseline.md#section-7)를 실제 코드/fixture로 확인한다. RowUID와 물리 주소 분리, snapshot 기반 opaque 접근, row/catalog와 후속 저장의 공통 commit, index scope와 매체 분리, versioned format/capability 거절, 미래 DISK의 O(N) 상주 강제 금지, 독립 경계 검사다. DEV2에는 index/DISK 구현이 없으므로 설계 경계 검증만 하고 실제 지원 PASS를 주지 않는다.

<a id="profiles"></a>

## 4. correctness·fault 검증 프로파일

DP03의 V2 property 64 seeds×최대 128 operations, V3 256 seeds×최대 1,024 operations를 유지한다. V2 seed가 V3에 포함되고 최대 길이 고정 사례도 별도로 둔다. 영향받는 codec fuzz target마다 V3 최소 30분, 동시성 suite 실제 thread stress 최소 30분, 비결정 fault cell은 fresh fixture 3회다. 결정적 cell은 선언 조합 전부를 실행한다. 아래는 **프로파일 설계**이며 실제 seed list·generator hash·명령은 구현 전에 고정한다.

| 프로파일 | 입력·판정 |
| --- | --- |
| 작은 bounded model | 2 actor, 2 table, table당 최대 2 row, 값 {NULL,0,1}, actor당 최대 4 operation. operation alphabet·legal transition·부분순서 감소 규칙·상태/전이 상한을 P0에서 동결하고 전수 완료 여부 기록 |
| 순차 경계 | empty DB/C=0, 한 행, 값 0/최대/최대+1, schema 오류, stale handle, epoch/counter/CommitSeq overflow, table/drop/recreate |
| statement | 앞 statement 성공→다음 statement의 첫/중간/마지막 mutation 실패→앞 변경 유지; quota 해제·RowId 비재사용 |
| 동시성 | 두 writer가 같은 start root, writer와 readonly scan, DROP과 old reader, checkpoint와 update, pin과 Retiring, cancel과 CommitAttempted |
| 실제 I/O/process | owned worker+실제 파일로 commit/응답/crash/reopen. controller history 보존 후 worker만 kill. process kill을 power loss 증거로 표시하지 않음 |
| persistence model | production effect port 아래 volatile/durable bytes와 namespace를 분리. process crash는 OS cache 유지, power-loss model은 미sync 효과 선택 소실/partial persistence. 성공 sync는 보존, rename와 dirsync cut 분리. 두 종류 장애를 한 연속 이력에서 조합 |
| codec | 모든 짧은 frame prefix, 경계 길이·checked overflow, valid-length torn body/trailer, unknown version/kind/feature, 중복 q/TxnId, 잘못된 chain, missing dependency |
| budget | 각 예약 직전/직후 fault, old pin 장기 유지, WAL 가득 참, diagnostics 포화·collector 부재, recovery workspace 부족 |

model의 유한 공간을 완주하지 못하면 `INCOMPLETE`이며 seed test로 전수 완료를 대신하지 않는다. 실제 kernel/FS/device가 없는 fault model은 그 모델 범위의 결과다. DP06 첫 공식 대상 Ubuntu 24.04/x86_64/ext4는 별도 실제 환경 qualification이 필요하다. 현 host만 보고 공식 대상 PASS를 만들지 않는다.

필수 fault point 집합은 다음과 같다. 각 leaf에서 operation ID·전/후 cut·실제 effect 여부를 manifest에 열거하고 미도달·receipt 유실을 FAIL로 판정한다.

| 범위 | 필수 cut |
| --- | --- |
| commit | validation 전/후, prepare 각 allocation, batch 첫 write 전, body/trailer partial write, sync 전/후·실제 sync 뒤 error, publish 전/후, 응답 송신 전/후, controller ACK 기록 후 |
| namespace/ID | lock 경쟁, bootstrap file sync/rename/dirsync, epoch 예약과 WRITER의 각 선택 단계, segment seal/header/dirsync, 기존 tail→새 epoch 연결 |
| recovery | select/load/decode/각 batch apply/verify/STABILIZE의 각 파일·namespace sync/resume의 중간 종료, 새 거래 ACK 뒤 두 번째 crash, 잘못된 resume tip·중복/누락 epoch |
| checkpoint | chunk create/write/sync/name sync, manifest sync/name sync, CURRENT temp/sync/rename/dirsync와 rotation 경합, anchor 전/내/후 파일 누락, fallback 재선택, retiring/삭제/삭제 dirsync |
| restore | 파일 일부 복사·inventory 검증·새 timeline/epoch·selector 선택, 완료 전/후 재시작; 원본 bytes 보존 |

known-bad는 정답을 틀리게 계산하는 reference self-test와 실제 product 경로를 틀리게 만든 test-only mutant를 구분한다. wrong binary/fixture/format, fault 미도달, 이전 build receipt 재사용, worker가 verdict를 위조하는 경우도 거절해야 한다. default/release 배포 조합에는 fault/mutant/test-support를 넣지 않는다.

[장애 결과 계약](../contracts/dev2-kernel-design.md#fault-outcomes)에 따라 각 variant의 `RECOVER_OK`/`REJECT_CORRUPTION`/`REJECT_IO`/`REJECT_INCOMPLETE_CREATE`를 실행 전에 고정한다. 정상 crash의 full-length torn tail도 현재는 명시 거절할 수 있다. STABILIZE 전/파일별 중간 장애는 미sync bytes의 실제 잔존 형태별로 결과를 고정하며, 장벽 전체 성공 이후 RESUME cut의 정상 reopen과 구분한다. 이 검출 PASS를 정상 reopen·ACK 보존 성공 수에 합산하지 않는다. 기대 결과가 `RECOVER_OK`인 fixture의 오류·timeout·미도달은 FAIL이다. 과거 단일 장애/객체 모델 결과는 새 STABILIZE·anchor·dual-selector 상태표의 실행 증거가 아니다.

<a id="performance"></a>

## 5. 성능 개발과 진단

[AI 성능 개발 계약](../contracts/ai-performance-engineering.md)의 정본을 유지한다. 초기 측정 항목은 transaction 준비/validation/coordinator 대기, WAL encode/write/sync, publication, row-ref 복사량, resident/private/pinned bytes, checkpoint bytes/시간, replay bytes/거래 수, diagnostics drop이다. 단위·분모·clock domain·source/binary·소유 API와 연결한다. exclusive 시간이 아닌 중첩 span을 합산하지 않고 공유 작업은 link로 표현한다.

K0에서는 row/catalog의 실제 비용·bounded collection·fixture/recipe identity·독립 비교/replay부터 제공한다. K1은 transaction/snapshot/rollback 비용, K2는 WAL/sync와 내구성 생략 거절, K3는 reopen/recovery, K4는 contention·중첩 실행, K5는 checkpoint/retention과 누적 회귀를 추가한다. 각 단계에서 필요한 PF variant와 XADB-014a bundle recipe를 동결하며, 아직 없는 WAL collector 전체를 K0의 선행 조건으로 만들지 않는다.

초기 합성 workload 후보는 (a) 1 actor 한 행 durable update, (b) 4 actor 두 table 이체+hot-row conflict, (c) 1/1,024/8,192행 table의 한 행 변경과 장기 snapshot, (d) checkpoint 중 쓰기와 reopen이다. fixture 한도를 넘는 크기는 입력 단계에서 거절하며 작게 바꿔 실행한 결과를 원래 workload 결과로 기록하지 않는다. 기준선은 같은 의미를 만족하는 첫 실제 L0 구현이다.

| PF | 구현할 판정 |
| --- | --- |
| PF01/02 | metric catalog·단위/범위·clock domain, 중첩/병렬/공유 작업 고정 vector와 잘못된 합산 거절 |
| PF03/04 | off/상시/상세, 포화·collector 없음에도 정답/내구성 동일. 실제 release의 TPS/p99/CPU/RSS/drop·기록량 비교 |
| PF05/06 | 서로 다른 fixture/의미/부하·잘못된 binary·미도달 경로의 비교 INVALID. sync 생략/요청 누락·timeout 은폐 mutant 거절 |
| PF07 | 동등/개선/악화/불확실·순서 drift·다수 후보 선택 편향의 고정 분석 vector. 사후 기준 변경 거절 |
| PF12 | 실제 관측한 병목에 대한 좁은 수정 1건의 원인 지표+end-to-end+독립 정답·관련 fault/recovery. 개선이 없으면 그대로 기록 |
| PF13 | 다른 세션에서 bundle만으로 baseline/candidate·recipe·가설 이력 재현 |

성능 복잡화 후보의 최종 확인은 DP07 최소 10 paired runs를 적용한다. 탐색과 최종 확인을 분리하고 순서 무작위화·warmup/기간·통계·다중 확인 정책·중단 규칙을 실행 전에 고정한다. 처리량·p99·CPU·RSS의 허용 회귀와 계측 비용의 **수치**는 목표 환경·fixture를 정한 PF leaf에서 동결한다. 임의의 보편적 1%나 제품 TPS 목표를 이 설계의 측정 결과로 만들지 않는다. 미정 수치·미실행 recipe는 PF/B2 완료를 차단하지만 XADB-004/K0 착수는 차단하지 않는다.

<a id="freeze"></a>

## 6. 구현 전 동결 항목과 완료 차단 조건

| ID | 동결할 산출물 | 시점·담당 | 현재 |
| --- | --- | --- | --- |
| D2-F01 | K0 operation/schema/error vector, RowUID/epoch 산술, reference operation alphabet, SC01 known-bad corpus | P0/K0 구현 전 계약·oracle 담당 | 기본 의미 채택; 기계 실행 vector 없음 |
| D2-F02 | 작은 모델 전체 공간·seed 목록/generator hash, native protocol envelope·ACK 기록/kill handshake·wire 예산 | 해당 P0/XADB-032 leaf 구현 전 | 설계값 존재; runner 없음 |
| D2-F03 | fs port signature/receipt, lock 수명, STABILIZE의 기존 파일·namespace 보장, bootstrap·WRITER/CURRENT·rotation의 모든 crash cut | K2/K3 구현 전 계약·platform 담당 | STABILIZE 순서·L0 선택 상태표 구체화; 실제 adapter와 이전 I/O 오류 뒤 sync 보장·전체 실행 vector 필요 |
| D2-F04 | row/WAL/segment/epoch byte offset·magic·golden hex·독립 decoder·fault profile와 variant별 기대 결과 | K2 codec 구현 전 durable·oracle 담당 | full-length torn 거절과 정상 복구 구분; binary 형식·실제 자동 복구 범위 미동결 |
| D2-F05 | replay anchor/digest·manifest/selector·fallback 재선택·restore 시작점 vector·GC 전이 | K5 구현 전 durable·storage 담당 | CURRENT만 선택·anchor 검증 종료 규약 구체화; 같은 segment의 두 anchor·C=0·빈 새 epoch·회수 후 reopen vector 필요 |
| D2-F06 | allocator/Arc/Vec/진단 allocation inventory, peak·RSS 측정 방법, 실제 환경/feature·시간/디스크 상한 | 해당 leaf 구현 전·B2 완료 전 | 논리 budget 기본값 채택; 실측 없음 |
| D2-F07 | PF fixture·recipe·비용/회귀 수치·반복/통계/선택편향 규칙·최종 확인 입력 지문 | 각 PF leaf 구현·실행 전 | workload 후보만 존재; PF 전체 NOT_RUN |
| D2-F08 | 최신 DEV0/1 검증과 독립 검토, 지원 환경 identity, 실제 DEV2 runner·증거 목록 | K0 착수 전 하위 증거 확인; B2/배포 시 각각 검증 | 별도 DEV0/1 작업 진행 중, DEV2 entrypoint=null |

이 목록은 미래 전체 제품의 미정을 K0 선행 조건으로 바꾸지 않는다. 각 leaf가 사용할 의미·vector·profile만 먼저 확정한다. 반대로 미완성 항목을 설계 승인만으로 PASS시키지 않는다. 구현 때 새 `dev2-contract.json`이 필요하면 실제 checker가 소비할 값을 이 문서에서 옮기고 정본/상태를 연결한다. 현재 미소비 JSON의 긴 사본을 추가하지 않는다.


## 7. 변경·검토·검증 기록

| 항목 | 현재 기록 |
| --- | --- |
| 분류 | D2 내부 API·형식·검증 구조 설계. V0 문서 변경만 수행 |
| 작업·계약·통합 담당 | `/root` |
| 별도 검토 | `/root/dev2_design_review`; 원래 소유 계약과 실제 소스에서 출발, 읽기 전용 |
| 수정 범위 | 신규 상세 설계와 이 계획, 소유 문서의 짧은 연결, 단계 JSON의 설계 참조, 폴더 README·문서 catalog |
| 공유 파일 | `devdoc/documents.json`, `devdoc/document-validation.json`은 현재 파일을 다시 읽고 필요한 항목만 통합. 다른 DEV0/1 변경 보존 |
| 제품 변경 | 없음. 기존 제품 코드·checker·historical evidence 수정 없음 |
| 검토 결과 | 소유 계약·실제 소스·수정본 별도 대조 완료. 아래 반례를 보완했고 기본안 채택의 추가 차단 사항 없음. F01–08은 구현 전 의무로 유지 |
| 문서 검사 | `python3 devdoc/verify_documents.py` PASS, 오류 0. [생성 결과](../document-validation.json). 제품 시험은 NOT_RUN |

| 별도 검토에서 확인한 반례·누락 | 반영한 설계 |
| --- | --- |
| rotation된 새 segment의 ACK를 다음 open이 orphan으로 누락 | 새 active segment의 WRITER 선택·directory sync를 append 전에 완료 |
| root C를 pin한 뒤 더 나중 WAL tail로 replay 시작점을 선택 | coordinator 아래 C·replay 위치·high-water·retention pin 동시 획득 |
| 원본/복원 DB 사이 같은 숫자 identity와 오래된 handle 혼용 | DB/timeline namespace와 open instance token 검증, restart 후 handle 재획득 |
| previous checkpoint의 권위 부재·metadata 불명 후 write 계속 | CURRENT에 previous identity/digest, metadata 결과 불명은 engine Fenced |
| worker가 제시한 snapshot token을 oracle가 그대로 신뢰 | 외부 begin 구간과 가능한 committed 상태로 독립 검증 |
| 다중 actor의 정상 응답 역전을 오류로 판정 | request ID와 actor/transaction별 요구 순서만 검사 |
| 최신 단계 계획의 초기 bundle·성능 배정 누락 | XADB-014a/032와 K별 product/PF ownership 연결 |

검토는 제품 구현·실행의 증거가 아니다. dual-selector 전체 상태 공간, codec vector, fault/성능 profile의 실제 동결과 실행은 각 leaf에서 검토해야 한다.

지금 구현을 시작한다면 P0의 SC01 known-bad reference와 K0 API vector부터 진행한다. DEV2 전체의 성공 기준은 위 단계별 실제 증거이며, 현재 설계 문서의 완성 여부와 구분한다.

<a id="recovery-review"></a>

## 8. Recovery 리뷰 반영과 검증 배정

[DEV2-DESIGN-002](../contracts/dev2-kernel-design.md#recovery-review-decision)는 사용자 요청에 따른 D2 문서 개선이다. 작업·계약·통합 담당은 `/root`, 독립 검토자는 `/root/dev2_recovery_review`이며 검토자는 읽기 전용이다. 기준 revision은 `546b1f98d849bc238d9747f7a8c5c480ddbad1a4`, 착수 때 작업 트리는 clean이었다. 제품 코드·실행 checker·과거 evidence는 변경하지 않는다.

| 범위·공유 변경 | 소유·통합 순서 |
| --- | --- |
| `contracts/dev2-kernel-design.md`, `recovery-logging-design.md`, `implementation-contracts.md` | `/root`가 의미와 순서를 먼저 구체화 |
| `contracts/recovery-logging-contract.json`, `recovery-test-matrix.json`, 이 계획 | `/root`가 불변식·기존 28개 case 안의 필수 variant와 NOT_RUN 상태 연결 |
| `contracts/README.md`, `plans/README.md`, `documents.json` | `/root`가 기존 문서의 탐색 설명 갱신; 신규 경로 없음 |
| `document-validation.json` | 최종 통합 원문으로 `python3 devdoc/verify_documents.py` 실행하여 생성 |

| 반례·결정 | 필수 구현/시험 연결 |
| --- | --- |
| 이전 미sync 완결 frame을 cache에서 채택한 뒤 새 WAL만 sync | RC-T04: process crash/cache 유지→STABILIZE→새 ACK→power-loss model/cache 소실. 장벽 생략 mutant 검출 |
| STABILIZE sync/namespace 결과 불명 뒤 개방 | RC-T04: 파일별 전/후/일부 효과 뒤 error; READY·새 WRITER 선택 없음, 원본 보존 |
| checkpoint가 이전 predecessor를 영구 pin하거나 과거 tip으로 회귀 | RC-T06/07/09: manifest anchor 검증 종료·current/previous에서 실제 tip 도달; build/선택 중 commit·rotation |
| CURRENT 선택 불명 또는 GC 경합으로 old/new 경로 유실 | RC-T07/08/10: 합집합 pin, coordinator 선택 경계, Retiring 이후 pin 거절, 회수 후 실제 reopen |
| current 손상 후 fallback 사용·다음 checkpoint가 손상 후보 재등록 | RC-T09: fallback 재선택의 모든 cut, previous 없음 진단, 새 checkpoint 뒤 재장애 |
| backup manifest의 source WAL anchor를 restore가 요구 | RC-T11: source WAL 없는 C 복원, 새 timeline/base-q=C anchor, 새 ACK 뒤 restart |
| full-length torn 검출을 자동복구 성공으로 집계 | RC-T03: `RECOVER_OK`와 명시 거절의 고정 fixture·별도 verdict, 무조건 tail 삭제 mutant 검출 |


독립 검토자는 계약·diff·원래 재장애 반례와 필수 variant의 기대 결과를 대조했다. STABILIZE 전/중간의 미sync frame 손상과 장벽 전체 성공 뒤 RESUME 장애를 같은 `RECOVER_OK`로 묶지 않도록 RC-T04를 수정하고, 문장 연결을 정리한 뒤 미해결 채택 차단 의견 없음으로 검토를 완료했다. 계약·통합 담당이 DEV2-DESIGN-002를 ACCEPTED로 반영했다. 별도 작업의 `reference/review/` 문서와 해당 탐색 등록은 보존하며 이 검토 범위에서 제외했다.

`python3 devdoc/verify_documents.py`와 `git diff --check`는 PASS다. 문서 검사 결과는 [생성 보고서](../document-validation.json)에 기록한다. 제품·fault·전원 장애·성능 시험, F03–05의 실행 vector는 NOT_RUN이며, 이 문서 반영으로 기존 단계 완료 상태를 바꾸지 않는다.

<a id="review"></a>
## 독립 검토와 실행 상태

이관 범위의 D2 독립 검토는 [이관 기록](../document-migration.json)에 기록한다. 현재 DEV2 일부 구현의 실제 검증은 [DEV2 구현](../implementation/dev2-implementation.md)과 [증거 색인](../evidence/README.md)을 따른다. 계획 채택·문서 검토는 영속성 구현·실행 증거가 아니다.
