# DEV3 — 조사 결과와 구현·검증 계획

2026-10-18 · **D2 기본 설계 ACCEPTED / production 구현 미착수 / S1·제품 실행 NOT_RUN**

[상세 설계](../contracts/disk-storage-design.md) · [실제 소스 조사](../evidence/dev3-source-research.md) · [단계 완료 정본](stage-completion.json) · [DEV2 선행 계획](dev2-implementation-plan.md)

<a id="entry"></a>
## 1. 착수와 완료의 구분

이 작업은 DEV3 설계·소스 조사·시험 의무 구체화다. DEV2가 아직 부분 구현이어도 계약 준비는 가능하다. DEV3 production leaf의 시작에는 DEV2의 최소 선행 요건과 해당 task 선행 계약·증거가 필요하다. K0 volatile 시험 통과로 durable 의존성이 충족됐다고 하지 않는다. 설계 문서의 D2 채택, leaf 검증, S1 완료, 공식 배포는 별도 상태다.

DEV2의 공통 API·identity·값/오류·statement rollback·coarse validation·L0·recovery·checkpoint를 사용한다. 실제 K2–K5에서 결정할 namespace/형식/anchor가 아래 가정과 다르면 소비 구현 전에 양쪽 소유 계약을 조정하고 독립 검토한다. DEV2 완료 조건을 DEV3 page 구현에 역으로 종속시키지 않는다.

| DEV2 공급 계약 | DEV3 소비와 검증 |
| --- | --- |
| K0/K1 row/catalog·snapshot·private batch·statement 경계 | disk read/scan와 private index delta의 동일 의미; cursor 변경 감지 |
| K2 durable L0·writer epoch·파일 namespace | pack identity, data-before-commit dependency, UNKNOWN fencing, feature/format 거절 |
| K3 q chain·STABILIZE·epoch resume | page closure 검증과 bounded recovery. 이전 OS cache 채택→두 번째 장애 사례 |
| K4 coarse serializable validation·실제 thread 이력 | concurrent unique/phantom/schema conflict. 최신 root overwrite로 lost update 금지 |
| K5 exact cut·CURRENT/WRITER·pin/retire·restore | chunked disk inventory, previous와 replay roots 보존, source WAL 없는 exact-cut restore |

<a id="sequence"></a>
## 2. 구현 순서

S0–S5는 DEV3 내부 작업 묶음이며 상위 DEV 단계나 기존 task ID를 대체하지 않는다. 모든 구현은 기존 package·effect 경계를 사용한다. leaf마다 의미·oracle·fault profile을 먼저 동결하고 실제 코드가 생길 때만 소스 경로를 등록한다.

| 묶음 | 기존 task 연결 | 구현·독립 기대값 | 완료 조건과 남길 의무 |
| --- | --- | --- | --- |
| S0 계약·oracle 준비 | XADB-028/021/022/029 공통 | 독립 composite key/NULL/row/index 모델, page/pack/manifest byte vector, corrupt corpus, resource profile | D3-F01–F03 동결. oracle만 PASS여도 disk 실행 NOT_RUN |
| S1 디스크 표현 | XADB-028 | page/cell/key/root·inventory codec, COW 변환과 독립 byte/구조 vector. DEV2의 XADB-009/010/027 선행 확인 | production에서 사용할 표현 구현을 실제 byte buffer로 검증. 파일 I/O·S1 제품 gate는 미완료 의무로 남김 |
| S2 page·pack·buffer·tree 경로 | XADB-021 | XADB-028의 표현 위에 실제 파일·checksum/parser·PageGuard·clock·row overflow/locator·index scan·private delta 연결 | XADB-005/008/028 선행 충족 후 구현. 실제 short I/O·full pin·취소·scan equality 검증; 공통 commit 인수는 다음 묶음 |
| S3 다중 객체 영속 통합 | XADB-022 | pack+inventory sync→공통 WAL→전체 root; offline index build와 실제 reopen·dependency 검증 | XADB-009/021 선행 충족. 모든 table/index/catalog 같은 q, ACK/UNKNOWN, crash cut와 DEV2 회귀. 이 이후만 durable disk capability 주장 |
| S4 checkpoint·회수·maintenance | XADB-029 | inventory streaming, previous/replay closure, backup/restore, whole-pack GC, offline compact | XADB-022/028 선행 충족. 오래된 snapshot·중간 replay root 생존, 두 checkpoint 뒤 회수, ENOSPC/중단·재복구 |
| S5 전체 S1 인수 | XADB-029 및 네 task의 필수 variant 전체 | 4배 이상 working set, 독립 이력·실제 I/O·PF09·자원 상한, 하위 회귀 | stage-completion의 모든 필수 결과·독립 검토·재현 bundle. 하위 leaf 일부로 전체 완료 금지 |

기존 task DAG 순서 **XADB-028 → XADB-021 → XADB-022 → XADB-029**를 유지한다. S1의 표현 구현은 S2 파일 엔진을 요구하지 않는 codec/자료구조 경계다. 표현 leaf의 로컬 검증을 XADB-028의 모든 후속 통합 의무 완료나 DEV3 S1 gate로 확대하지 않는다. task의 필요한 최소 선행 leaf와 남은 통합 의무를 착수 계약에 각각 기록한다.

<a id="acceptance"></a>
## 3. 시험 의무와 구체 variant

모든 항목은 **NOT_RUN**이다. testcase 목록은 새 실행기의 입력으로 동결해야 하며 이 표가 존재한다는 사실은 실행 증거가 아니다. 독립 기대값은 production codec/tree/replay·기존 제품의 출력 복사로 만들지 않는다.

| 인수 ID | 필수 variant·검출할 반례 |
| --- | --- |
| DS01, KU09 | 빈 tree/한 행/다층 tree, 순차·역순·난수 삽입, leaf/internal/root split, 삭제와 root 축소, inline 2 KiB 전/동일/후와 128 KiB row, overflow 누락/순환. 모든 index point/range/full scan을 독립 row 값과 대조 |
| DS06, KU09 | PK NULL/중복, Unique의 부분·전체 NULL과 중복 허용, Nonunique RowId 순서, 1–4열·최대 8 index, UTF-8·빈 값·NUL bytes·정수 양끝·1024-byte escaped key 경계. key update, 없는 행, 동일 statement 중복/교환 실패 후 이전 statement 유지 |
| DS02, RC-T12, KU10 | 두 table·모든 index·catalog/locator를 한 q로 변경. 각 dependency file write/sync/dirsync, WAL append/sync, publish/reply 전후 crash. page-first 장벽 생략·일부 index 누락·다중 root 별도 공개 mutant 검출 |
| DS03, RC-T14, KU10 | row·index·locator lookup·inventory 각각 buffer보다 큰 데이터, 부하·recovery·checkpoint·build·GC에서 bounded RAM; recovery VERIFY는 disk spill 없는 양방향 lookup으로 검사. 파일 수/메타데이터 증가가 RAM 증가로 직결되지 않음. 전체 locator 상주·recovery 전체 로드 mutant 검출 |
| DS04, RC-T13 | old reader를 유지하며 update/delete/drop·eviction·checkpoint·GC. 최신 root에는 없지만 retained WAL 중간 q가 참조한 page 보존. pin/mark/Retiring 경합·stale cache·file identity 재사용 mutant 검출 |
| DS05 | CURRENT old/new 선택·WRITER rotation·fallback, 공유 pack 손상, missing inventory chunk, repeated recovery와 두 번 장애/STABILIZE. exact C backup을 source WAL 없이 restore→새 ACK→재시작. RECOVER_OK/REJECT_CORRUPTION/REJECT_IO 구분 |
| DS03/04/05 | compact 중 읽기 drain timeout·spill/pack ENOSPC·WAL UNKNOWN·publish 직후 crash. 논리 digest 동일, q 전진, previous/backup pin 유지, checkpoint 두 번 뒤 삭제량·고유 disk bytes 대조 |
| DS06, KU09 | offline build에서 중복·oversized key·취소·quota·drain timeout·재시작, drop/recreate handle. 결과 tree는 재open 후 재build 없이 사용 |
| DEV2 회귀 | snapshot/RowUID·오류 effect·statement rollback·serialization·ACK/UNKNOWN·WAL chain·checkpoint/restore의 적용 variant를 새 disk 경로에서 재실행. 기존 volatile와 lower regression도 유지 |
| PF09 | 동일 의미의 fixture/recipe·baseline/candidate·독립 결과·반복/통계·계측 비용. COW write amplification, reopen I/O, file count, 유지보수 공간을 포함 |

reference는 key 정렬을 typed tuple과 자체 byte 비교로 계산하고 literal vector로 교차 검산한다. production key를 decode해서 기대 순서를 계산하지 않는다. index 검사에는 잘못된 RowId 연결, 중복/누락 entry, NULL 정책 오류를 가진 known-bad 출력도 입력한다. 외부 controller의 응답 관측과 transaction별 witness row를 남겨 나중 overwrite가 ACK 유실을 숨기지 못하게 한다.

<a id="faults"></a>
## 4. fault·동시성 profile

S0에서 named operation마다 before/after와 일부 효과 후 error를 정의한다. effect port의 실제 호출 receipt·잔존 bytes·파일 길이·sync 확인 범위를 함께 기록한다. controller가 fault 도달을 확인한 후 소유 worker를 종료한다. 임의 sleep·자기 보고만으로 특정 cut을 달성했다고 하지 않는다.

| 경계 | 필수 cut·schedule |
| --- | --- |
| page buffer | map 등록/Loading I/O/완료/최초 pin/마지막 unpin/eviction, 같은 page 동시 load, 전 frame pin과 cancel, 재사용 frame generation 오인 |
| tree/private | 각 path-copy/split/overflow 예약·statement undo mark·index old 제거/new 삽입. 실패 후 transaction의 이전 private 상태 |
| pack | create·short/zero write·seal·file sync·directory sync, symlink/다른 owner·이미 존재 파일·읽기 중 checksum/length 오류 |
| commit | dependency inventory 마지막 chunk, 전후 root 검증, WAL 첫 byte·partial/full-length torn·sync, publication 직전/직후, 외부 ACK 관측 전/후 |
| checkpoint/recovery | anchor pin과 commit/rotation 경합, manifest/CURRENT 각 전환, fallback 재선택, VERIFY 반복 crash와 spill 파일 누적 0, STABILIZE 전/중/후의 두 번째 장애, RESUME WRITER 선택 |
| reclaim/compact | mark 중 새 commit/pin, revision 재검사, Retiring 전/후 새 pin, unlink/dirsync, compact old/new root·두 checkpoint·backup pin |

공통 DP03의 V2 property 64 seeds·seed당 최대 128 operations, V3 256 seeds·최대 1,024 operations, V3 실제 thread stress와 target별 fuzz 각 30분, 비결정적 fault 셀 3회 반복 기준은 [개발 운영 방침](../policies/development-policy.md#section-4)을 따른다. leaf에서 지원되는 한도 안의 정확한 operation 수·반복·timeout·실제 cut 목록을 정한다. 미지원 harness를 PASS/N/A로 바꾸지 않는다. 필수 사례의 timeout·미도달·receipt 유실은 실패 또는 미검증이며 정상 복구를 손상 거절로 사후 바꾸지 않는다.

process kill, syscall fault, power-loss model, 실제 장치 qualification은 분리한다. OS cache가 살아 있는 process kill만으로 전원 장애 내구성을 주장하지 않는다. 성공 sync bytes 보존을 포함한 장치 모델과 실제 Linux/FS/mount/장치 identity를 D3-F04/F05에 기록한다.

<a id="performance"></a>
## 5. 용량·성능·메모리 검증

기본 profile은 buffer 64 MiB이며 16 MiB·4 MiB의 축소 profile은 경계·fault 탐색용이다. **각 profile의 해당 accessed working set ≥ buffer 4배**를 적용한다. 기본 용량 사례는 row/index의 접근 대상 256 MiB 이상, 이어 1 GiB corpus를 사용한다. 압축되지 않은 fixture 명목 크기만 비교하지 않고 실제 접근한 고유 page bytes·조회된 index 범위를 기록한다. locator·inventory에도 RC-T14의 전체 configured page-buffer 4배 기준을 유지한다. locator는 row tree의 영속 해석 경로를 큰 corpus에서 검사한다. inventory 인수는 global page buffer를 4 MiB 등으로 설정한 별도 사전 profile에서 실제 유효 inventory 16 MiB 이상과 그 참조 pack closure를 생성한다. 이에 필요한 disk quota·파일 수·시간을 D3-F05에 미리 고정하고, fixture를 만들지 못하면 NOT_RUN으로 남긴다. metadata 8 MiB 안의 locator cache 1 MiB, inventory decoder buffer 64 KiB의 별도 4배 pressure 사례는 추가 시험이며 전체 page-buffer 기준을 대신하지 않는다.

| workload | 비교할 비용·필수 관측 |
| --- | --- |
| 순차/랜덤 insert·read·update·delete | 1/2/4 writer와 readonly reader, 균등/고정 편향. 성공 TPS와 conflict 비율을 함께 보고 |
| point/range/full scan | 선택도·limit·중복 key·hot/cold 범위, page reads·hit/miss·index→row 비용·독립 값 동일성 |
| 오래된 snapshot + 쓰기 | retained/live ratio·pin wait·quota 거절. pin을 몰래 만료해 성능 개선으로 집계하지 않음 |
| checkpoint/backup/restore/reopen | wall time·CPU·읽기/쓰기·sync·scratch/RSS·동시 foreground 지연 |
| delete churn + compact | dead/live bytes·파일 수·전체 재작성 bytes·두 checkpoint 후 실제 회수, 공간 부족 시 사전 거절 |

engine FileIo counter는 syscall byte/operation을 보여 주고 외부 `/proc`/소유 process I/O 관측은 실제 장치 I/O와 OS cache 영향을 구분한다. engine cache miss만으로 장치 read가 있었다고 주장하지 않는다. warm/cold cache 조건은 소유 fixture·격리 환경에서 준비하고 공유 host 전체 cache drop은 사용하지 않는다.

RSS budget은 환경의 고정 runtime baseline + buffer·metadata·전체 private·scratch·diagnostic 예약 + 사전 고정 allocator margin으로 정한다. peak와 장기 기울기를 둘 다 확인한다. dataset 증가 시 bound 초과, leaked pin/file handle, 누적 메모리 증가를 검출해야 한다. logical reservation 성공을 전체 OOM 회복으로 해석하지 않는다.

성능 프로파일은 각 case의 데이터·seed·기간·warmup·반복·통계·회귀 수치·계측 on/off 비용 한도를 실행 전에 확정한다. 초기 구조 선택은 성능 PASS가 아니다. 기본 선택보다 나은 후보의 채택에는 동일 durability와 independent result, source/binary/recipe identity, 새 세션 재현 및 DP07 검토가 필요하다.

<a id="freeze"></a>
## 6. 구현 전 동결 항목

| ID | 정확한 산출물·담당 경계 | 현재 상태·닫는 시점 |
| --- | --- | --- |
| D3-F01 | storage/durable: page/pack/PageRef/inventory/feature/WAL extension의 field offsets·magic·길이·digest 범위, 독립 정상/손상 hex vectors | 구조·크기 기준 결정, byte vector 미작성. S1 표현 codec 구현 전 |
| D3-F02 | api/catalog/transaction: IndexSpec·PK/NULL/오류 번호·range bounds·cursor revision·build/compact 상태표·Rust signature | 의미는 상세 설계에 정의, 실행 API/vector 미동결. S2 구현 전 |
| D3-F03 | reference/checker: 독립 tuple/row/index/history 모델·negative mutants·case/variant 전체 목록·예상 결과 | DS01–06에 배정, oracle 구현/검증 미실행. 해당 leaf production 구현 전 |
| D3-F04 | platform/durable: pack+WAL+CURRENT/WRITER effect receipts, fault/corruption 판정과 반복 장애 schedule | 명명 경계 정의, 실제 hook/장애 모델 미동결. S3 구현 전 |
| D3-F05 | storage/performance: cache/metadata/FD·disk reserve·RSS margin·I/O timeout·외부 I/O 관측·대형 inventory의 실제 disk quota·환경 digest·PF09 수치 | 공학적 기본값만 존재, 실제 profile 미실행. 해당 용량 leaf 구현/실행 전 |
| D3-F06 | integration: DEV2 실제 API/format·회귀 evidence, stage/task 선행 증거·clean build·모듈 의존 검사·실제 runner와 보호 검토 경로 | DEV2 partial, durable 선행 미충족. production 착수·S1/배포 시 각각 확인 |

후속 PART/NG/SQL/서버의 미구현을 위 leaf의 새 선행 조건으로 추가하지 않는다. 반대로 D3-F04가 미동결인데 crash 시험을 통과했다고 하거나 D3-F06 없이 DEV3 구현을 시작하지 않는다. 새로운 실행 스크립트 이름이나 artifact 경로를 이미 존재하는 entrypoint로 등록하지 않는다.

<a id="review"></a>
## 7. 작업 소유·검토·검증 기록

| 항목 | 기록 |
| --- | --- |
| 분류 | D2: storage 내부 형식·API·인수 계약 구체화 |
| 담당·계약·통합 | `/root` |
| 기준 | `7f656e6aa47c114772cb758e4c1a1af2b16712db`, 착수 시 `git status --short` 빈 출력 |
| 독점 수정 | `contracts/disk-storage-design.md`, `contracts/disk-test-matrix.json`, 이 계획, `evidence/dev3-source-research.md` |
| 공유 수정 | `plans/development-stages.{md,json}`, `plans/stage-completion.json`, 문서 README·documents.json. 담당 `/root`가 순차 반영 |
| 계약 영향 | DEV2의 불변식 유지. DEV3 page/locator·key·자원·retention 구체화. 과거 실행 evidence와 제품 코드 불변 |
| 중복 확인 | 착수 시 clean tree, 구현 subagent 없음. 검토 중 루트 README·project-direction·phase1-product-baseline·capability-inventory·architecture-overview의 제품 소개 5곳에 외부 동시 수정이 나타남. 이 작업에서 작성·승인한 변경이 아니므로 보존하고 D2 소유 범위에서 제외. 독립 검토자는 읽기 전용 |
| 반영 순서 | 소스 조사→storage 계약→구현 계획/기존 인수 JSON→색인→독립 검토/수정→문서 검사 재생성 |
| 생성물 | `document-validation.json`, `python3 devdoc/verify_documents.py`, 담당 `/root` |
| 독립 검토 | `/root/dev3_design_review`; 원문·diff·소스 증거와 기존 DEV2/복구 계약에서 출발. 최초 P2 1건 수정 후 최종 설계 검토 PASS. 계약/통합 담당 `/root`가 반영 확인 |
| 검증 | `python3 devdoc/verify_documents.py`: 45 Markdown, 오류 0. 기존 문서 검사 회귀 `python3 -m unittest discover -s devdoc -p 'test_*.py'`: 27 tests PASS. 최종 기록 반영 후 같은 문서 검사를 재생성한다. DBMS source build/benchmark·DEV3 실행·S1 승격은 수행하지 않음 |

### 독립 검토 반영

| 항목 | 지적·수정·확인 범위 |
| --- | --- |
| P2 recovery scratch | 최초안의 외부 정렬 VERIFY는 정상 DB의 크기와 반복 crash에 따라 임시 disk가 부족해질 수 있었다. 추가 disk spill 없는 row→index lookup과 index→row 역검사·strict order/unique/count 검사로 변경. DS03/05에 VERIFY 반복 crash와 scratch 누적 0을 연결 |
| RC-T14 기준 | 전체 configured page-buffer 4배 조건을 row/index/locator/inventory 모두 유지. 작은 전용 cache의 pressure 시험은 추가 사례임을 명시하고 대형 inventory quota를 사전 profile에 배정 |
| 선행 DAG·공통 기준 | XADB-028→021→022→029의 실제 관계와 표현 leaf/후속 통합 구분, DP03의 실제 V2/V3 수치를 대조 |
| 조사 증거 | 독립 검토자가 7개 HEAD·tracked clean·모든 기록 file hash·외부 inventory hash를 재계산해 일치 확인 |
| 검토 한계 | 설계 계약의 일관성 검토. 실제 page 구현·fault suite·성능·S1 인수의 PASS가 아님. 외부 동시 제품 소개 수정은 이 검토의 승인 대상에서 제외 |

새 설계의 승인으로 기존 historical receipt를 갱신하지 않는다.

후속 통합: 사용자가 README 변경과 DEV3 설계를 함께 commit하도록 명시 요청했다. README와 같은 제품 소개 문구를 맞춘 관련 4개 문서의 기존 변경도 함께 포함한다. 해당 소개 편집은 저장 엔진·인수 계약을 바꾸지 않는 D0로 확인했으며, 위 독립 D2 검토의 원래 범위와 구별한다.
