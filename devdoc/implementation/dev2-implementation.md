# DEV2 구현 기록

**이관 상태:** 아래의 API·동작·제약 설명을 현재 코드와 함께 유지한다. 이전 실행 횟수·검토 기록은 새 XADB qualification이 아니다. 새 실행 여부는 [구현 상태](implementation-status.json)와 [증거 색인](../evidence/README.md)을 따른다.


2026-10-18 · **구현 진행 중 / DEV2·B2·D0 미완료**

[소유 설계](../contracts/dev2-kernel-design.md) · [구현 순서](../plans/dev2-implementation-plan.md) · [완료 의무](../plans/stage-completion.json)

## DEV2-P0-001 — 독립 이력 판정의 첫 실행 경계

| 항목 | 결정 |
| --- | --- |
| 작업·계약·통합 담당 | `/root` |
| 범위 | XADB-004의 작은 row/transaction oracle와 literal known-bad self-test. 실제 engine·native worker·내구성 시험과 별개 |
| 소유 경로 | `crates/xadbtest-reference/`, `crates/xadbtest-protocol/src/history.rs`, 관련 시험; 이 구현 기록 |
| 공유 변경 | `/root`가 Cargo workspace/lock, protocol 모듈 선언, 저장소 배치 JSON/Markdown, 문서 색인/검사 결과를 순차 반영 |
| 순서 | 이력 계약 → 독립 검토 → reference 구현·literal 반례 → 하위 회귀·독립 build → 최신 문서 검사 |
| 대안 | commit 응답 순서만 정렬하면 응답 역전과 begin 구간을 오판한다. worker snapshot 번호를 신뢰하면 혼합 snapshot을 놓친다 |
| 선택 이유 | controller의 invocation/response 사이에서 pending 연산의 가능한 선형화 순서를 유한 탐색한다. snapshot은 Begin 선형화에서 고정하고 private row map은 모델이 독립 계산한다 |
| 의존 | reference normal dependency는 순수 protocol만. foundation/engine/platform·production codec/값 연산을 사용하지 않음. 새 외부 dependency 없음 |
| 재검토 | native stream 연결, catalog/schema/ID·statement quota·pin·recovery 선택 범위 확장 시 해당 leaf 계약과 검토 추가 |

### P0 실행 전에 고정한 의미

protocol의 이력은 controller가 보존한 입력·관측이다. worker verdict나 snapshot token을 포함하지 않는다. P0 상태는 사전에 고정한 table/row key와 scalar row이며 catalog/schema/ID 발급은 다음 K0 leaf에서 확장한다. 키는 `(table, row)`의 nonzero u64 쌍, row는 scalar 배열이다. NULL과 absent row를 구별한다. scan은 table의 전체 행을 key 오름차순으로 반환하는 P0 관측이다. native paging은 K0에서 별도 연결한다.

연산 alphabet은 `Begin(ReadOnly|ReadWrite)`, `Read`, `Scan`, `Statement([Insert|Replace|Delete])`, `Commit`, `Rollback`이다. 외부 사건은 `Invoke(actor, request, operation)`, `Return(actor, request, observation)`, `Crash`다. 끝까지 응답 없는 요청은 Crash로 닫힌 경우만 허용하며 그 외는 INVALID다. Crash 이전 request의 늦은 Return도 INVALID다. request는 전체 이력에서 유일하고 actor마다 한 요청만 in-flight다. Begin 응답 후 같은 actor의 다음 요청을 호출한다. invocation/response 경계를 넘어 응답 순서가 달라지는 것은 허용한다. 형식 오류는 INVALID, 관측을 설명할 선형화가 없으면 REJECTED, 탐색 상한 초과는 INCOMPLETE다. 빈 assertion 이력은 INVALID다.

Begin의 snapshot+자기 변경만 읽는다. statement는 전체 복사본에서 입력 순서대로 적용하여 오류 시 진입 상태를 유지한다. Replace/Delete의 없는 행과 Insert의 중복 행은 명시 오류다. ReadOnly 쓰기는 거절한다. 성공한 쓰기 statement는 dirty 상태를 만들며 commit은 시작 version과 현재 version이 같을 때만 전체 반영한다. 무변경 ReadWrite도 version 검사를 한다. ReadOnly는 기존 snapshot에서 종료한다. Rollback/실패한 commit은 private 변경을 버린다.

Commit의 fault 선택은 worker 응답이 아닌 **시험 controller의 사전 입력**이다. `None`은 UNKNOWN을 허용하지 않는다. `BeforeAttempt`는 ABORTED, `Uncertain`은 첫 write 시도 이후의 불확실성으로 전체 포함/미포함 두 branch와 Fenced를 만든다. Uncertain은 dirty ReadWrite에만 적용한다. Fenced 뒤 기존 ReadOnly/ReadWrite 모두 Read/Scan과 Rollback은 가능하다. Begin/Statement/Commit은 Fenced 오류이며 commit 거절은 private 상태를 유지한다. Uncertain 분기는 정상 version validation을 통과한 dirty writer에만 적용한다. Crash는 private/pending 상태를 버리고 모델의 durable 상태로 다시 연다. 일반 commit의 ACK가 유실된 pending 요청은 Crash 전에 전체 선형화되거나 미실행일 수 있다. Fenced 상태의 미공개 UNKNOWN은 Crash 뒤만 관측된다. 실제 I/O receipt나 process kill의 증거를 이 self-test로 대신하지 않는다.

초기 한도는 최대 actor 8, 동시 pending 8, events 4,096, initial rows 128, row columns 32, scalar bytes 65,536, mutations/statement 256, 전체 history 입력 비용 8 MiB(scalar payload에 cell 32·key/mutation 32·event 64 bytes를 더한 논리 단위)다. 탐색은 event별 closure를 중복 제거하고 전체 방문 상태 1,000,000개·전이 4,000,000개를 한도로 한다. event별 frontier 4,096개·보유 Vec/String capacity×실제 요소 크기와 BTree entry별 16-slot+1 KiB 여유를 합한 64 MiB, 이력당 단조 clock 30초를 추가 상한으로 둔다. 이전/다음 frontier와 최대 두 scratch 모델, caller 입력은 별도이며 이 수치는 process RSS/OOM 보장이 아니다. 각 limit 위반은 명시 INVALID/INCOMPLETE이며 잘린 입력을 PASS하지 않는다. 이 숫자는 oracle의 시험 예산이며 제품 용량이 아니다.

### 검증 프로파일

- Literal 정상: read-your-writes, NULL/없는 행, statement 중간 오류, rollback, 고정 snapshot, coarse conflict, 응답 역전, UNKNOWN 전체 포함·미포함, ACK 없는 pending commit의 두 결과.
- Literal known-bad: lost update, write skew, phantom/mixed snapshot, partial batch, ACK 유실, 근거 없는 UNKNOWN. actor/request 위조·누락·중복, 무조건 거절하는 checker도 정상 corpus로 검출한다.
- V2 순차 생성 이력: seed `0..63`, seed당 128 operations. V3는 `0..255`, 1,024 operations. 각 profile에 최대 길이 고정 사례를 별도로 유지한다. generator와 seed hash는 실행 결과에 연결한다.
- 계획의 2 actor×2 table×table당 2 row×{NULL,0,1}×actor당 최대 4 operation의 작은 concurrent 공간은 별도 실행 전에 정확한 program alphabet·초기 상태·interleaving 수를 동결한다. 이 문서의 scalar/행 한도만으로 전체 공간 전수 완료를 주장하지 않는다.
- reference 독립 build, Cargo dependency 대조, default/all-features, DEV0/1 회귀, 문서 검사. parser/fuzz·실제 thread/I/O·제품 mutant는 현재 P0의 실행 증거가 아니며 해당 leaf에 남긴다.

최종 행만 같으면 덮어써진 ACK 거래의 물리 보존은 증명할 수 없다. P0의 ACK 유실 반례는 복구 후 관측 가능한 손실이다. XADB-032/K3는 독립 commit identity·decoder·이력 witness를 추가하여 이 한계를 해소해야 하며 P0 결과를 ACK 집합 보존의 전체 증명으로 사용하지 않는다.

P0 당시 제품 구현·SC01 전체 인수·K0–K5·PF는 완료하지 않았다. 후속 구현과 실행 증거는 아래에 누적하며 과거 기록을 덮어쓰지 않는다.

### P0 실제 구현과 검증

`xadbtest-protocol::history`는 in-process typed 이력이며 native wire/protocol parser가 아니다. `xadbtest-reference::check`가 각 외부 사건 사이의 pending 연산을 중복 제거한 유한 closure로 탐색한다. statement는 전체 private map 복사본에서 계산한다. production 자료구조·값 비교·codec을 호출하지 않는다. accepted는 입력 이력을 설명할 경로가 있다는 뜻이며, 모든 미래 이력이나 미관측 저장 상태를 인증하지 않는다.

```bash
python3 scripts/check_dev2_reference.py
```


| 실행 | 결과 |
| --- | --- |
| 기본 P0 시험 | 18 PASS, 확장 profile 1개는 이 명령에서 ignored |
| 명시적 V3 profile | ignored 시험 별도 실행 1 PASS; 256 seeds×1,024 operations |
| V2 profile | 64 seeds×128 operations, 각 이력의 관측 오염 거절 |
| 최대 길이 | 별도 seed u64::MAX의 1,024 operations |
| DEV0·DEV1 회귀 | 각각 필수 8/8 PASS; [DEV0](../evidence/README.md), [DEV1](../evidence/README.md) |
| build·정적 검사 | workspace fmt/Clippy/all-features·default build, fresh source build, reference 독립 빈 target build PASS |
| 누락 시험 방어 | wrapper의 고정 필수 이름 목록과 실제 PASS 출력 대조; `0 tests` 결과를 거절하는 별도 probe PASS |

첫 wrapper 실행 `check-a_gmqit0`는 모든 target의 offline Cargo metadata가 미캐시 `r-efi`를 요구하여 FAIL이었다. 새 dependency를 추가하지 않고 실제 검증 대상 `x86_64-unknown-linux-gnu`를 필터로 고정했다. 수정 후 `check-8v4lgfjx`는 PASS였지만 아래 메모리 회계 검토 수정을 포함하지 않아 최종 근거로 승계하지 않았다. 두 실행은 원래 artifact에 보존했다.



## DEV2-K0-001 — 실제 행·catalog substrate


이 leaf는 행 저장·schema·ID·COW 수명·자원 회계와 독립 literal 기대값까지 구현한다. native worker와 P0 catalog 연결, K0 PF·bundle·bounded concurrent 전수 검사는 별도 남은 K0 의무다. 해당 결과까지 통합하기 전에 K0 완료나 K1 완료를 주장하지 않는다. 기존 K0부터의 PF 의무를 제외하지 않는다.

| 경계 | 고정 입력·의미 |
| --- | --- |
| identity | nonzero DatabaseId 128-bit, nonzero timeline u64, epoch 1..=u32::MAX. Generation/SchemaEpoch=1. K0는 fixture가 제공하며 영속 예약은 K2/3. ObjectId/RowId 각각 `epoch<<32 | local`, local=1부터, 실패·drop 뒤 재사용 없음, overflow 거절 |
| handle | 불변 instance Arc의 pointer equality + DB/timeline + ObjectHandle. 생성자는 비공개. 같은 숫자 ID가 있어도 다른 store handle은 stale. RowUID에도 DB/timeline·table/generation·row를 포함 |
| schema | DB당64 table, 1–32열, Bool/Int64/Utf8/Binary·nullable, 이름1–128 UTF-8 bytes/NUL 없음/byte case-sensitive/중복 거절. 변환·ALTER·index 없음 |
| row | canonical 크기 `u32 column_count + 각 값 tag u8 + 고정 scalar 또는 u32 length+payload`; 값64KiB·row128KiB. 이 산술은 크기 검사이며 WAL byte format 동결을 대신하지 않음 |
| 변경 | create/drop/insert/replace/delete는 각각 exclusive `&mut VolatileStore` 원자 연산. 완성한 새 root만 교체. 입력/준비 실패 시 old root 유지, 이미 발급한 ID는 소비된 채 유지 |
| snapshot | 최대32 owning pin. snapshot/cursor/row view는 같은 pin을 공유하며 마지막 view 해제 때 반환. old reader는 drop/recreate 전 객체를 계속 읽고 새 snapshot의 old handle은 stale |
| scan | 고정 snapshot의 RowId 오름차순 cursor, page 요청1–1,024행, canonical payload 최대256KiB. cursor가 pin과 table root를 소유하며 내부 위치를 외부 token으로 노출하지 않음. EOF 명시 |
| resident | root/table row-ref 배열·schema·row의 고유 allocation마다 foundation Reservation. 기본64MiB, 공유 Arc는 한 번 회계. old snapshot의 자원도 마지막 소유자까지 회계. 입력 Vec/String의 실제 capacity와 root/table/schema/row/instance/counter/pin Arc의 header(2×usize)를 포함. scan page Vec<RowView>도 같은 ledger로 과금하고 마지막 page 소유자가 반환 |
| 반환 값 | 값 복사 대신 opaque owning RowView에서 열 값을 빌림. RowUID는 물리 offset/address로 사용하지 않음 |
| 숨은 allocation | Vec 준비는 try_reserve, Arc·이미 구성된 caller 입력·allocator overhead·OS OOM은 완전 fallible 보장 밖. quota는 명시 ledger이며 RSS 한계와 구분 |
| 계측 | 누적 logical operation·read rows·copied row references와 현재 resident/pins. 시간·release 성능 비교는 남은 K0 PF leaf에서 clock domain·recipe와 연결 |

초기 오류는 `XADB-ENGINE-0001` InvalidInput, `0002` SchemaMismatch, `0003` TableExists, `0004` TableNotFound, `0005` StaleHandle, `0006` RowNotFound, `0007` LimitExceeded, `0008` QuotaExceeded, `0009` IdentityExhausted다. 이 substrate 오류의 공개 root 효과는 Unchanged/Known이며 같은 입력 자동 retry를 요구하지 않는다. 후속 거래·I/O 오류는 해당 leaf에서 effect/확실성/retry를 확장한다.

검증은 손계산 ID·schema/value/row 경계, 잘못된 instance/table, create/drop/recreate와 old view, 정렬/paging/EOF, 모든 변경의 작은 quota 실패 전후 ledger·root, ID overflow/실패 뒤 비재사용, 실제 COW sharing과 복사량을 포함한다. 새 dependency는 engine→foundation뿐이고 OS·XADBTest normal/build 의존은 금지한다. 정책 JSON·workspace·문서 ID는 실제 구현과 함께 `/root`가 최신 파일에서 좁게 통합한다.

Snapshot/Cursor/RowView 자체는 caller가 소유하는 값이며 새 heap allocation 없이 기존 Arc를 공유한다. boxed wrapper 등 caller가 추가한 메모리는 caller 책임이다. Pin의 새 Arc allocation과 page backing은 engine resident에 포함한다. foundation Quota의 고정 ledger handle과 allocator bookkeeping은 runtime/allocator overhead로 구분하며 전체 process RSS로 표현하지 않는다. page는 Vec를 꺼내 예약보다 오래 소유하게 하는 API를 제공하지 않는다.

K0 leaf의 최종 증거 프로파일은 literal 12개·내부 ID/COW 2개, 순차 생성 seed `0..63` 각각128 mutation positions와 매 위치 독립 전체 scan 대조, 별도 u64::MAX seed 1,024 positions, V3 seed `0..255` 각각1,024 positions로 고정한다. 실제 reader thread 2개가 writer의1,000번 교체 전 snapshot을 유지하는 barrier 시험을 포함한다. 이 유한 thread 시험은 V3 30분 stress를 대신하지 않는다. `python3 scripts/check_dev2_reference.py --with-substrate`가 실행 전에 generator hash·seed·필수 시험 이름을 보존하고, 빈 target의 engine/reference binary를 따로 빌드·보존·실행한다. DEV0/1 회귀·default/all-features·의존 경계와 전후 source hash를 함께 검사한다. native 이력·PF·전수 concurrent 탐색은 `NOT_RUN`이다.


### K0 substrate 실제 검증



## DEV2-K0-002 — native 스트림 경계


wire는 4-byte big-endian body 길이 + UTF-8 JSON이다. **header를 포함한 전체 frame 최대4MiB, body는1..=4MiB−4**다. header 초과·부분 header/body·JSON/UTF-8 오류·unknown field/tag·version 불일치는 명시 오류다. 길이를 먼저 검사하고 제한된 backing만 예약하며, incremental decoder는 한 frame과 consumed byte 수를 반환한다. caller는 미소비 suffix를 다음 feed에 전달하며 decoder가 tail을 숨겨 보관하지 않는다. 모든 decode 실패 뒤 poisoned 상태로 재사용을 거절한다. EOF에서 부분 frame은 오류이고 frame 경계 EOF만 정상이며 완료 뒤 feed/finish도 거절한다. 직렬화도 capped writer로 실제 escape 이후 wire 크기를 검사한다. 요청을 모두 읽고 검증하기 전에 worker가 effect를 수행하지 않는다. schema/value/row의 제품 의미는 engine이 검사하고 protocol codec을 oracle로 사용하지 않는다. 새 외부 dependency 없이 기존 고정 serde_json을 protocol에서 사용한다.

envelope는 `version=1`, nonzero 16-byte run ID, run 전체 유일 request ID(정수값1..=1,024, 요청 수도최대1,024), actor0..31과 action/observation이다. controller는 pending32·actor당 pending1을 제한하고 응답을 invocation에 매칭한다. 다른 actor의 응답 역전은 허용한다. 총 입력·stdout·history 각각64MiB, stderr1MiB, 요청당5초·run60초를 초기 native 실행 한도로 둔다. 예산을 초과하거나 응답이 유실·중복·위조되면 ERROR/INVALID/TIMEOUT이며 PASS로 절단하지 않는다.

K0 alphabet은 Initialize(DB/timeline/epoch/quota), Create/Lookup/Drop table, Insert/Replace/Delete, Snapshot/ReleaseSnapshot, Read, OpenScan/NextPage/CloseScan, Metrics다. table/row/snapshot/cursor alias는 controller가 고른 정수값1..=1,024이며 worker의 실제 opaque 객체와 대응한다. alias를 제품 handle로 재구성하지 않는다. 생성 성공은 실제 논리 identity를 관측값에 포함한다. snapshot32·cursor32·각 alias 종류의 누적1,024개를 제한하고 alias를 run 안에서 재사용하지 않는다. snapshot 해제 후 cursor의 owning pin은 계속 살아 있다. protocol 실패와 제품의 Unchanged/Known 오류를 구별한다. worker에 기대값·최종 verdict를 전달하지 않는다.

Initialize는 actor0의 첫 request1에만 허용하고 run ID는 그때 고정한다. 성공 뒤 재Initialize와 다른 run ID는 fatal protocol 오류다. 생성 alias는 validation을 통과해 시도한 때 영구 소비하고 product 오류·release/drop 뒤에도 재사용하지 않는다. 누락·잘못된 종류·0 alias와 중복 request ID는 protocol 오류이며 오류 뒤 worker는 추가 요청의 effect 없이 종료한다. K0 native는 최대32 actor envelope를 지원하되 최초 이력 비교는 순차 operation에 한정한다. P0 transaction oracle의8 actor 한도를 native32 actor 검증 완료로 확대 해석하지 않는다.

이 leaf의 codec 검증은 손으로 쓴 길이/JSON vector, 모든 짧은 frame prefix,1/최대/최대+1 wire, unknown version/field/tag, 잘린 UTF-8·중복 field, 한 byte와 여러 frame chunk, 직렬화 escape 팽창·실패한 decoder 재사용 거절을 포함한다. 생성 frame chunk 분할은 V2 seed0..63×128, V3 seed0..255×1,024와 별도 최대 길이 사례를 실행 전에 고정한다. parser 30분 fuzz와 실제 worker streaming/process mutant는 다음 실행 의무이며 codec self-test만으로 native 완료를 주장하지 않는다.



### K0 native worker 연결 — leaf 검증 완료

실제 `xadbtest-engine-worker`를 추가한다. normal dependency는 engine, 값 전환에 필요한 foundation, 순수 protocol이며 platform은 실제 I/O를 시작하는 K2까지 planned다. 제품에 기대값·verdict를 넣지 않는다. RowUID의 table generation/schema epoch도 실제 공개 getter로 읽어 관측하며 상수를 합성하지 않는다. Create 응답의 schema는 성공한 요청 schema이고, Lookup은 실제 snapshot schema를 읽는다.

Initialize가 제품 오류로 실패하면 그 ProductError frame을 flush한 뒤 worker가 exit2로 종료한다. 직접 Session도 poisoned가 되어 재Initialize나 후속 effect를 거절한다. 빈 stdin도 정상 실행으로 취급하지 않는다. Drop/Delete 이후 table/row alias의 실제 capability는 old snapshot/absent 검증을 위해 보유하며 같은 alias를 다시 생성하지 못한다. ReleaseSnapshot/CloseScan은 소유 자원만 해제하고 alias는 소비된 채 남는다. 이 byte/type adapter의 단독 시험은 streaming controller의 외부 이력·deadline·process cleanup 검증을 대신하지 않는다.

worker 하위 leaf는 adapter literal6개와 trusted process smoke4개를 먼저 확인한다. 실제 subprocess의 생성 profile은 seed0..63 각각128 wire requests, V3 seed0..255 각각1,024 requests, 별도 u64::MAX seed1,024 requests로 동결한다. Initialize/Create/Insert 뒤 Replace(정상·타입 오류 교대)→Snapshot→Read→ReleaseSnapshot의4-request cycle과 마지막 Metrics로 정확한 길이를 채운다. 매 변경은 production을 import하지 않는 scalar 기대값으로 다음 Read와 대조한다. 각 child의 I/O5초·EOF 종료5초 한도와 RAII kill/reap를 적용하며 stdout 응답을 stdin EOF 전에 읽는다. 이것은 알려진 단일 worker의 smoke/property이고, 악의적 worker/descendant·append-only 외부 history·response matching/timeout mutant를 검증할 실제 controller와 구분한다.

추가 run byte 경계는 실제64KiB NUL 문자열의 JSON escape를 이용해 입력 또는 응답 누적64MiB를 각각 넘기도록 고정한다. 각 frame은 합법4MiB 이하이며 전체 request1,024 이내다. 한도 안 응답을 모두 읽고 다음 frame이 누적 한도를 넘을 때 새 응답 없이 exit2인지 검사한다. 내부 작은 한도로 바꿔 실행한 결과를 실제64MiB 검증으로 기록하지 않는다.



## DEV2-K0-003 — 외부 streaming controller

**D2 / 초안 코드 이관 / 기능 인수 미실행.** `crates/xadbtest/src/native.rs`의 native session 초안도 snapshot에 포함되어 이관됐다. workspace 빌드·Clippy 대상에는 포함되지만 이 모듈의 독립 process mutant·외부 이력 통합 시험은 아직 연결되지 않았으며 실행 보고서는 `streaming_controller: NOT_RUN`이다. 아래는 완료해야 할 계약이며 현재 검증된 보장이 아니다. `xadbtest::native`는 기존 Linux `OwnedChild`의 unreaped leader/process-group 정리를 재사용한다. 기존 전체 stdout fixture controller의 판정 경로는 유지한다. 새 외부 dependency·production normal dependency는 없다.

controller가 소유하는 새 artifact directory에 candidate binary 사본/hash, 제한된 raw stdout/stderr, append-only invocation/response journal을 보존한다. worker cwd는 별도 하위 directory이고 expected 값·최종 verdict·journal 경로를 인자로 보내지 않는다. 같은 OS 사용자 권한의 로컬 분리는 보호 CI나 보안 sandbox로 주장하지 않는다. 호출자는 typed action만 invoke하고 수신 observation의 의미를 독립 oracle로 판정한다. transport의 정상 종료만으로 제품 PASS를 만들지 않는다.

request1..1,024를 controller가 발급하고 actor0..31당 pending1·전체pending32를 제한한다. nonblocking stdin/stdout/stderr를 단일 loop로 진행해 큰 request 쓰기 중에도 응답을 수집한다. 완전한 request 전송 전에 온 동일 요청 응답, 알 수 없는/중복 request, 잘못된 run/actor, unsolicited response, 부분 frame EOF와 budget 초과는 fatal이다. 정상 응답 역전은 허용하고 request ID로 매칭한다. 응답을 완전하게 읽고 envelope를 검사한 즉시 journal write+flush를 마친 뒤 관측 완료로 다루며, 그 전에는 다음 worker kill을 허용하지 않는다. controller crash 내구성을 주장하지 않아 journal fsync는 별도 의무로 끌어오지 않는다.

journal에는 단조 event sequence와 session-relative monotonic time, Invoke/완전 전송/Observe를 구분해 남긴다. Invoke write+flush는 첫 stdin byte보다 먼저이며, 완전 전송 뒤의 Sent와 완전 frame+matching 검증 뒤 Observe도 순서대로 write+flush한다. journal budget/쓰기 실패 뒤 정상 후속 invoke나 계획된 crash-cut kill handshake를 진행하지 않는다. 위 kill 제한은 검증용 계획 crash에 해당하며 protocol/timeout/I/O 오류의 즉시 abort cleanup kill을 막지 않는다. 기록에 실패한 response는 ACK 증거가 아니고 cleanup 성공이나 늦은 응답이 최초 오류를 지우지 않는다.

입력·stdout·journal 각각64MiB, stderr1MiB, frame 전체4MiB, 요청마다invoke 시점부터5초·run60초를 적용한다. 기본값을 줄인 timeout/byte 경계 fixture는 별도 test-only profile임을 명시한다. decoder와 pending/inbox의 논리 한도는 process RSS 보장이 아니다. poll은 stream별 유한 quantum으로 반복해 한 stream이 다른 stream의 deadline 처리를 굶기지 못하게 한다. 종료는 stdin 닫기→bounded drain/leader-exit 확인→group signal 후 leader reap 순서로 처리하며 cleanup 실패를 정상 완료로 바꾸지 않는다. abort/Drop도 자기 child만 정리한다. protocol/timeout/I/O 실패 뒤 session을 재사용하지 않는다.

정상 종료 drain grace는5초이며 run60초에 포함한다(남은 run 시간이 짧으면 그 시간이 먼저 만료). SIGKILL→reap는 기존 DEV0와 같이 협력적 Linux의 실제 정리이며 uninterruptible kernel wait나 process group을 탈출한 descendant의 강제 시간 보장을 주장하지 않는다. pipe를 유지한 descendant 때문에 grace가 끝나면 timeout을 남기며 정상 실행으로 승격하지 않는다. 기존 `OwnedChild::finish`의 kernel wait 한계는 유지하고 아직 실행하지 않은 sandbox/cgroup 자격으로 바꾸지 않는다.

초기 부정 corpus는 wrong run/request/actor, duplicate/unsolicited/reversed response, partial/oversized/malformed frame, missing response/late response, continuously flooding stdout/stderr, stdin을 읽지 않는 worker, leader 종료 후 pipe를 보유한 descendant, journal 쓰기 실패다. 실제 native worker와 정상 literal 이력, 복수 actor pending/역전, 중간 응답 뒤 controller-owned kill을 대조한다. 이 controller leaf도 K0 catalog oracle 전체·bounded concurrent 공간·PF/bundle·durability 완료를 대신하지 않는다.
