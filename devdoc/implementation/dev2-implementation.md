# DEV2 구현 기록

**이관 상태:** 아래의 API·동작·제약 설명을 현재 코드와 함께 유지한다. 이전 실행 횟수·검토 기록은 새 XADB qualification이 아니다. K0-005 통합 후 r3 실행에서28개 check와 DEV0·DEV1 회귀를 확인했고, 별도 성능 진단 r1에서35개 시험과60개 release 실행을 확인했다. 최초 성능 비교의 sample-p99 guardrail FAIL은 유지한다. 현재 적용 source와 한계는 [구현 상태](implementation-status.json)와 [증거 색인](../evidence/README.md)을 따른다.


2026-10-18 · **구현 진행 중 / DEV2·B2·D0 미완료**

[소유 설계](../contracts/dev2-kernel-design.md) · [구현 순서](../plans/dev2-implementation-plan.md) · [완료 의무](../plans/stage-completion.json)

## DEV2-P0-001 — 독립 이력 판정의 첫 실행 경계

| 항목 | 결정 |
| --- | --- |
| 작업·계약·통합 담당 | `/root` |
| 범위 | XADB-004의 작은 row/transaction oracle와 literal known-bad self-test. 실제 engine·native worker·내구성 시험과 별개 |
| 소유 경로 | `crates/testing/reference/`, `crates/testing/protocol/src/history.rs`, 관련 시험; 이 구현 기록 |
| 공유 변경 | `/root`가 Cargo workspace/lock, protocol 모듈 선언, 저장소 배치 JSON/Markdown, 문서 색인/검사 결과를 순차 반영 |
| 순서 | 이력 계약 → 독립 검토 → reference 구현·literal 반례 → 하위 회귀·독립 build → 최신 문서 검사 |
| 대안 | commit 응답 순서만 정렬하면 응답 역전과 begin 구간을 오판한다. worker snapshot 번호를 신뢰하면 혼합 snapshot을 놓친다 |
| 선택 이유 | controller의 invocation/response 사이에서 pending 연산의 가능한 선형화 순서를 유한 탐색한다. snapshot은 Begin 선형화에서 고정하고 private row map은 모델이 독립 계산한다 |
| 의존 | reference normal dependency는 순수 protocol만. foundation/engine/platform·production codec/값 연산을 사용하지 않음. 새 외부 dependency 없음 |
| 재검토 | native stream 연결, catalog/schema/ID·statement quota·pin·recovery 선택 범위 확장 시 해당 leaf 계약과 검토 추가 |

### P0 실행 전에 고정한 의미

protocol의 이력은 controller가 보존한 입력·관측이다. worker verdict나 snapshot token을 포함하지 않는다. P0 상태는 사전에 고정한 table/row key와 scalar row이며 catalog/schema/ID 발급은 다음 K0 leaf에서 확장한다. 키는 `(table, row)`의 nonzero u64 쌍, row는 scalar 배열이다. NULL과 absent row를 구별한다. scan은 table의 전체 행을 key 오름차순으로 반환하는 P0 관측이다. native paging은 K0에서 별도 연결한다.

연산 alphabet은 `Begin(ReadOnly|ReadWrite)`, `Read`, `Scan`, `Statement([Insert|Replace|Delete])`, `Commit`, `Rollback`이다. 외부 사건은 `Invoke(actor, request, operation)`, `Return(actor, request, observation)`, `Crash`다. 끝까지 응답 없는 요청은 Crash로 닫힌 경우만 허용하며 그 외는 INVALID다. Crash 이전 request의 늦은 Return도 INVALID다. request는 전체 이력에서 유일하고 actor마다 한 요청만 in-flight다. Begin 응답 후 같은 actor의 다음 요청을 호출한다. invocation/response 경계를 넘어 응답 순서가 달라지는 것은 허용한다. 형식 오류는 INVALID, 관측을 설명할 선형화가 없으면 REJECTED, 탐색 상한 초과는 INCOMPLETE다. 빈 assertion 이력은 INVALID다.

Begin의 snapshot+자기 변경만 읽는다. statement는 전체 복사본에서 입력 순서대로 적용하여 오류 시 진입 상태를 유지한다. Replace/Delete의 없는 행과 Insert의 중복 행은 명시 오류다. ReadOnly 쓰기는 거절한다. 성공한 쓰기 statement는 private 상태를 만들며 commit은 시작 version과 현재 version이 같을 때만 전체 반영한다. 최종 논리 행이 현재 committed 행과 같으면 version을 증가시키지 않는다(DEV2-P0-NET-ZERO-002). 무변경 ReadWrite도 version 검사를 한다. ReadOnly는 기존 snapshot에서 종료한다. Rollback/실패한 commit은 private 변경을 버린다.

Commit의 fault 선택은 worker 응답이 아닌 **시험 controller의 사전 입력**이다. `None`은 UNKNOWN을 허용하지 않는다. `BeforeAttempt`는 ABORTED, `Uncertain`은 첫 write 시도 이후의 불확실성으로 전체 포함/미포함 두 branch와 Fenced를 만든다. Uncertain은 최종 논리 변경이 있는 ReadWrite에만 적용한다. Fenced 뒤 기존 ReadOnly/ReadWrite 모두 Read/Scan과 Rollback은 가능하다. Begin/Statement/Commit은 Fenced 오류이며 commit 거절은 private 상태를 유지한다. Uncertain 분기는 정상 version validation을 통과하고 최종 논리 변경이 있는 writer에만 적용한다. Crash는 private/pending 상태를 버리고 모델의 durable 상태로 다시 연다. 일반 commit의 ACK가 유실된 pending 요청은 Crash 전에 전체 선형화되거나 미실행일 수 있다. Fenced 상태의 미공개 UNKNOWN은 Crash 뒤만 관측된다. 실제 I/O receipt나 process kill의 증거를 이 self-test로 대신하지 않는다.

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

**D2 / 후속 구현·시험 이관 / r2 현재 소스의 해당 leaf 로컬 검증.** 초기 초안 이후의 controller 보완과 실제 worker·부정 시험을 추가했다. 아래는 이관한 현재 API·오류·수명 계약이며, 실제 후속 이관 검증은 [r2 보고서](../evidence/dev2-xadb-20261018-r2.json)에 기록한다. DEV2·K0 전체 완료를 뜻하지 않는다.

controller가 소유하는 새 artifact directory에 candidate binary 사본/hash, 제한된 raw stdout/stderr, append-only invocation/response journal을 보존한다. worker cwd는 별도 하위 directory이고 expected 값·최종 verdict·journal 경로를 인자로 보내지 않는다. 같은 OS 사용자 권한의 로컬 분리는 보호 CI나 보안 sandbox로 주장하지 않는다. 호출자는 typed action만 invoke하고 수신 observation의 의미를 독립 oracle로 판정한다. transport의 정상 종료만으로 제품 PASS를 만들지 않는다.

request1..1,024를 controller가 발급하고 actor0..31당 pending1·전체pending32를 제한한다. nonblocking stdin/stdout/stderr를 단일 loop로 진행해 큰 request 쓰기 중에도 응답을 수집한다. 완전한 request 전송 전에 온 동일 요청 응답, 알 수 없는/중복 request, 잘못된 run/actor, unsolicited response, 부분 frame EOF와 budget 초과는 fatal이다. 정상 응답 역전은 허용하고 request ID로 매칭한다. 응답을 완전하게 읽고 envelope를 검사한 즉시 journal write+flush를 마친 뒤 관측 완료로 다루며, 그 전에는 다음 worker kill을 허용하지 않는다. controller crash 내구성을 주장하지 않아 journal fsync는 별도 의무로 끌어오지 않는다.

journal에는 단조 event sequence와 session-relative monotonic time, Invoke/완전 전송/Observe를 구분해 남긴다. Invoke write+flush는 첫 stdin byte보다 먼저이며, 완전 전송 뒤의 Sent와 완전 frame+matching 검증 뒤 Observe도 순서대로 write+flush한다. journal budget/쓰기 실패 뒤 정상 후속 invoke나 계획된 crash-cut kill handshake를 진행하지 않는다. 위 kill 제한은 검증용 계획 crash에 해당하며 protocol/timeout/I/O 오류의 즉시 abort cleanup kill을 막지 않는다. 기록에 실패한 response는 ACK 증거가 아니고 cleanup 성공이나 늦은 응답이 최초 오류를 지우지 않는다.

입력·stdout·journal 각각64MiB, stderr1MiB, frame 전체4MiB, 요청마다invoke 시점부터5초·run60초를 적용한다. 기본값을 줄인 timeout/byte 경계 fixture는 별도 test-only profile임을 명시한다. decoder와 pending/inbox의 논리 한도는 process RSS 보장이 아니다. poll은 stream별 유한 quantum으로 반복해 한 stream이 다른 stream의 deadline 처리를 굶기지 못하게 한다. 종료는 stdin 닫기→bounded drain/leader-exit 확인→group signal 후 leader reap 순서로 처리하며 cleanup 실패를 정상 완료로 바꾸지 않는다. abort/Drop도 자기 child만 정리한다. protocol/timeout/I/O 실패 뒤 session을 재사용하지 않는다.

정상 종료 drain grace는5초이며 run60초에 포함한다(남은 run 시간이 짧으면 그 시간이 먼저 만료). SIGKILL→reap는 기존 DEV0와 같이 협력적 Linux의 실제 정리이며 uninterruptible kernel wait나 process group을 탈출한 descendant의 강제 시간 보장을 주장하지 않는다. pipe를 유지한 descendant 때문에 grace가 끝나면 timeout을 남기며 정상 실행으로 승격하지 않는다. 기존 `OwnedChild::finish`의 kernel wait 한계는 유지하고 아직 실행하지 않은 sandbox/cgroup 자격으로 바꾸지 않는다.

초기 부정 corpus는 wrong run/request/actor, duplicate/unsolicited/reversed response, partial/oversized/malformed frame, missing response/late response, continuously flooding stdout/stderr, stdin을 읽지 않는 worker, leader 종료 후 pipe를 보유한 descendant, journal 쓰기 실패다. 실제 native worker와 정상 literal 이력, 복수 actor pending/역전, 중간 응답 뒤 controller-owned kill을 대조한다. 이 controller leaf도 K0 catalog oracle 전체·bounded concurrent 공간·PF/bundle·durability 완료를 대신하지 않는다.

실제 worker 통합 시험은 `xadbtest-engine-worker`의 dev-dependency로 기존 `xadbtest`, `tempfile`, `serde_json`을 사용한다. worker normal 실행은 이 controller를 링크하지 않는다. controller에 engine 의존을 추가하는 대안보다 기존 process boundary를 유지하며 `tests/support`의 test-only scalar 기대값 generator를 기존 trusted process와 실제 controller가 함께 소비한다. production 모델을 공유하지 않는다. V2 64×128 requests·별도 최대 seed1,024·V3 256×1,024 profile은 앞선 worker leaf와 같으며 실제 controller 경로에서도 따로 실행한다. 실행 전 이름·source hash를 wrapper가 고정하고 각 session의 worker 사본·journal·stdout/stderr·manifest·completion을 보존한다. 이 dev 간선과 native controller는 K0의 catalog 전체 oracle를 대체하지 않는다. 의존 경계/실제 native 통합이 바뀔 때 이 선택을 재검토한다.

이관한 구현은 두 반례의 수정과 회귀 시험을 포함한다. ACK와 초과 stdout이 같은 read에 도착하면 ACK까지 버리는 문제는 남은 허용 byte까지 먼저 decode/Observe하고 다음1byte로 초과를 검출하도록 수정했다. invoke 전 도착한 완전·부분 응답을 미래 request에 연결하는 문제는 admission 전에 읽을 수 있는 stream을 모두 drain하고, frame 첫 byte의 Sent high-water를 보존해 수정했다. 회귀 corpus에 양쪽·여러 quantum drain을 포함했다. invoke 진입에서 request deadline을 시작해 admission/encode도 포함하고 manifest/journal format version을 명시했다. 실제 worker의 Insert/Read 뒤 계획 kill도 `Observe→KillIntent→KillConfirmed`와 별도 `planned-kill.json` hash receipt로 보존한다. 정상 종료 receipt와 계획 kill은 제품 verdict·내구성 증거가 아니다.

## DEV2-K0-004 — catalog 이력과 native recipe 연결

**D2 / 후속 구현·시험 이관 / r2 현재 소스의 해당 leaf 로컬 검증.** P0 거래 이력 checker와 native transport를 유지하며 순차 catalog 이력을 독립 판정한다. `xadbtest`가 `xadbtest-reference`를 normal dependency로 소비한다. engine/foundation의 역방향 의존이나 production row/compare/codec 공유는 추가하지 않는다. 작은 독립 BTreeMap 모델을 사용하는 선택은 현재 K0 profile에 한정한다. profile·공개 결과·K1 연결을 확장할 때 다시 검토한다.

reference는 protocol의 입력·관측 타입만 공유하고 별도 `BTreeMap`과 명시적인 snapshot 복사로 catalog·schema·row·cursor 상태를 계산한다. 정상/오류 observation, namespace/epoch/local ID, NULL/absence, drop/recreate와 old reader, cursor 고정 순서·EOF·canonical byte 수를 자체 해석한다. alias의 소비·해제 규칙도 검사한다. Metrics의 attempted operation/returned row/pin은 독립 계산하고, resident byte는 감소 가능한 gauge로0..64MiB 범위만 검사하고, 누적 row-ref 복사량은 단조성만 검사한다. 후자에 대한 정확한 비용 비교는 별도 PF 소유이며 전체 metric을 독립 재현했다는 주장을 하지 않는다.

첫 native recipe는 actor0의 순차 요청이며 Initialize는64MiB resident·32pin으로 고정한다. reference 실행 profile은1,024 requests·논리 입력/관측8MiB, 성공 상태별4table·table당4columns·전체128rows·scalar payload1KiB, 동시에 보유한 snapshot8개·cursor8개로 제한한다. profile 밖은 명시 INVALID이고 축소하거나 일부만 PASS시키지 않는다. schema/type와 stale/없는 행의 제품 오류는 공개 catalog root를 변경하지 않는다. Create/Insert 실패 때 예약한 alias는 영구 소비되고 attempted-op counter는 증가할 수 있다. 잘못된·재사용·해제된 alias는 제품 오류가 아니라 fatal protocol/INVALID이며 이후 정상 operation을 허용하지 않는다. 제품의 더 큰 한도·quota 실패·동시 actor 검증은 기존 literal/transport 시험에 남기며 이 작은 모델의 검증 범위로 합치지 않는다. snapshot/cursor 복사 수와 각 root의 입력 한도를 함께 검사해 큰 무제한 복사를 시작하지 않는다.

controller의 versioned recipe는 worker 경로/hash·run identity·typed action을 입력으로 받아 현재 streaming session을 실행한다. 외부 journal의 Invoke/Sent/Observe를 별도 reader로 확인하고 그 입력·관측을 reference에 전달한다. recipe/worker/외부 history identity가 다르거나 요청·응답·종료 기록이 빠지면 INVALID/ERROR로 남긴다. worker에는 예상 observation이나 verdict를 전달하지 않는다. 파일에서 재실행한 recipe도 같은 독립 판정을 거치며 결과를 바꾼 known-bad 이력을 거절한다. 이 replay는 K0의 현재 행/catalog 의미만 소유하며 XADB-014a/PF의 전체 bundle·통계·성능 비교 완료는 별도 의무다.

검증 profile은 실행 전에 literal schema/ID/scan 경계와 각 의미별 wrong observation을 고정하고, V2 seed0..63×128 requests·별도 최대1,024·V3 seed0..255×1,024를 사용한다. 정상 이력 수용뿐 아니라 stale table 부활, 새 snapshot 혼합, 삭제/실패 변경 반영, 잘못된 identity·scalar·페이지 순서/EOF/byte 수, duplicate/missing history의 거절을 요구한다. 실제 worker와 controller를 통과한 동일 recipe의 외부 journal을 비교한다. profile/오류 우선순위·자원 상한·공개 결과 타입은 코드 추가 전에 검토하며, K1 거래/동시 이력 통합 또는 한도 확장 때 재검토한다. 기존 결정을 대체하지 않는 K0 하위 확장이다.

K0-004 입력 예산은 논리 비용으로 고정한다. 요청/관측 pair마다512bytes, 모든 schema의 table 이름 UTF-8 bytes와 각 column의64bytes+이름 bytes, 입력 cell마다32bytes+Utf8/Binary payload bytes, 반환 row마다128bytes+각 cell의 같은 비용, ProductError code의 UTF-8 bytes를 합산한다. Scan Page의 모든 row를 포함하고 입력·관측을 둘 다 센다. 기타 숫자/enum/alias/envelope/고정 metric은 pair의512bytes에 포함된다. checked 합산으로8MiB를 넘으면 INVALID이며 raw journal의64MiB 한도와 별개다. 이는 RSS/allocator capacity 보장이 아니다. input schema는 최대4columns·각 이름 최대129bytes(정상128+1의 오류 사례), input cell은 최대4개·payload1KiB로 제한한다. 반환 관측은 위 전체8MiB 예산에서 먼저 검사하고 잘못된 값은 기대 관측과 불일치로 REJECTED 처리한다. 정상 상태의4table·128rows·snapshot8·cursor8 한도를 clone 전에 검사한다. protocol의 frame4MiB에 더해 recipe 파일 자체는 parse 전에16MiB(허용 마지막 byte 다음1byte로 초과 확인), 최대1,024 action으로 고정하고 unknown/duplicate field·version은 거절한다. JSON escaping이 이 별도 파일 한도를 넘으면 논리 예산 내라도 parse 전 실행 ERROR다.

oracle 판정 결과는 `ACCEPTED`(모든 pair의 의미 일치), `REJECTED`(정상 profile의 관측 불일치), `INVALID`(typed 입력·protocol·profile·논리 예산 위반)로 구분하며 첫 실패 위치와 이유를 남긴다. 빈 typed trace는 INVALID다. 첫 Initialize는 nonzero DB/timeline/epoch,64MiB/32pin의 고정 profile이며 실패한 초기화를 정상 recipe로 승격하지 않는다. native recipe parser·파일 byte 한도·identity·미완료 journal·transport·I/O 실패는 실행 ERROR로 남는다. CLI 종료 코드는 ACCEPTED=0, REJECTED=1, INVALID=2, 실행 ERROR=3이며 ERROR를 oracle 결과로 합치지 않는다.

오류 우선순위는 envelope/run/request/actor 및 protocol alias 검사를 먼저 한다. Create는 table alias 예약→schema 이름·열수·중복→동일 이름 확인 순서, Lookup은 snapshot 확인→table alias 예약→이름 조회, Insert는 table alias 확인→row alias 예약→현재 table 유효성→열수·각 열의 타입/nullability 순서다. Replace/Delete는 table·row alias→현재 table→row 소속 table→행 존재를 확인하며 Replace만 이후 schema를 검사한다. Read는 snapshot·table·row alias→해당 snapshot의 table→row 소속 table→행 조회 순서다. OpenScan은 snapshot·table alias→cursor 한도/예약→snapshot의 table, NextPage는 cursor alias→limit1..1,024, Release/Close는 active alias 확인 후 자원 소유만 해제한다. 제품 오류 ID는 기존 K0-001의 InvalidInput/SchemaMismatch/TableExists/TableNotFound/StaleHandle/RowNotFound/LimitExceeded를 그대로 대조한다. 이 고정 작은 profile에서 예상하지 않는 allocation/quota 오류는 정상 관측으로 허용하지 않는다. 큰 값·전체 제품 table/pin/cursor 한도는 기존 직접 literal 시험에 남긴다.

Lookup의 검색 이름도 UTF-8 bytes를 논리 비용에 더하고 최대129bytes로 제한한다. Metrics.operations는 Create/Drop/Insert/Replace/Delete가 실제 engine 호출에 진입한 횟수(제품 오류 포함), rows_read는 성공 Read(Some)와 반환 page의 행 수, snapshot_pins는 snapshot/cursor가 공유하는 고유 pin 수다. 별도 검토자는 resident gauge·alias 소비·입력 예산/결과형/오류 순서 및 Lookup 비용 누락을 지적했고 위 계약에 반영했다. 다른 착수 차단 모순이 없음을 확인해 계약·통합 담당이 D2 입력을 채택한다. 이 검토는 아직 없는 코드나 실행 결과의 승인 증거가 아니다.

유효한 요청에 대한 잘못된 response envelope는 관측 불일치이므로 reference에서 REJECTED다. native reader의 framing·matching 위반은 실행 ERROR다. 입력 요청의 envelope/order/alias 위반은 INVALID로 구분한다. native reader와 reference는 같은 `CatalogInputBudget`을 소비해 각 decoded pair를 보유하기 전에 논리 예산을 확인하며 예산 실패 뒤 재사용을 거절한다. 이 공유는 입력 예산 계약에만 해당하고 native framing이나 production 정답 계산을 reference에 가져오지 않는다.

## DEV2-K0-005 — 성능 작업의 미구현 제안 인계

**이관 당시 PROPOSED 기록.** 아래 제안은 이관 시점에는 구현 NOT_RUN·검토 미완이었다. 현재 채택 계약과 담당·구현·검증은 뒤의 K0-005 재개 계약과 실제 결과를 따른다. 이관 제안 자체를 실행 증거로 사용하지 않는다.

전용 구현 계획은 엔진 `metrics.rs`/`substrate.rs`와 전용 시험·worker `performance.rs`, controller `performance.rs`/하위 모듈·전용 시험으로 분리한다. 공통 protocol/Cargo/lib/main/scripts/docs와 platform 수집 adapter는 통합 담당이 순차 반영할 영역이다. 아직 없는 경로를 IMPLEMENTED로 등록하지 않는다.

### K0-005 공통 계약 제안 — 독립 검토 전

두 담당의 입력을 합친 최초 범위는 한 thread의 실제 row replace→새 snapshot/read다. 기존5개 기본 Metrics는 off에도 남는 비용이며 새 diagnostics만 Off/Always/Detailed로 구분한다. API operation label11개(Create/Drop/Insert/Replace/Delete/Snapshot/Lookup/Schema/Read/Scan/NextPage)의 시작·완료·실패 counter를 고정 배열에 기록한다. off는 미수집을 None으로 표시한다. 상세 event는 sequence·operation·성공/실제 ErrorCode이며 clock/span이라고 표시하지 않는다. hot path는 할당·출력·대기 없이 try_lock으로 사전 할당 queue에 기록하고, 포화·경합·poison 시 drop을 공개한다. capacity0..1,024, 기본256, row quota와 분리한 diagnostics 메모리 상한을 적용한다. drain 뒤 queue capacity를 유지한다. counter/sequence의 포화는 saturated 상태로 공개하고 wrap하지 않는다. 기존 기본 counter의 의미는 바꾸지 않는다.

protocol은 별도 `performance` 소유 모듈의 엄격한 version1 input/output을 native engine Action/Observation에 연결한다. native Initialize 뒤 두 번째 요청 한 번만 허용하는 독립 fixture batch로 제한하고, 기존 세션의 행·alias를 소비하거나 변경하지 않는다. worker 내부의 새 volatile store(고정64MiB/32pins, DB `[2;16]`·timeline7·epoch3)를 사용한다. input은 seed, row_count1/128, pin_old, diagnostics mode/capacity, warmup4,096, measure65,536, latency_stride16이다. 각 구간 row index는 `i % row_count`, 초기 row 값은 index의 i64 값이다. 매 iteration 시작에 LCG `state = state * 6364136223846793005 + 1442695040888963407`를 u64 wrapping으로 적용하고 i64로 해석해 replace→새 snapshot/read→drop한다. warmup 끝 state를 measurement로 넘긴다. old snapshot은 setup 후 warmup 전에 고정한다. 제품 오류는 숨기지 않고 해당 실행을 실패로 남긴다. 직접 엔진 시험은 같은 정상/오류 fixture의 mode별 의미·ID·quota 결과를 대조한다.

worker output은 입력 echo, setup/warmup/measure 각각의 elapsed ns, 실제 완료 수·오류, measurement read 값65,536개 전부, 최종/old row 최대128개, 매16번째 iteration의 latency ns4,096개, 측정 전후 기본/추가 counter, bounded 상세 event/drop/saturation을 담는다. 입력·결과는 기존4MiB frame과5초 request·60초 run 한도를 유지한다. 측정 전 상세 queue를 drain하고 counter는 전후 delta를 사용한다. 측정 후 correctness 조회·직렬화 시간은 measurement 안에 넣지 않는다. 외부 controller는 값/완료량을 독립 scalar 산술로 대조하고 외부 Invoke→Observe 시간 및 process 정상 종료를 별도 확인한다. worker clock과 controller clock을 서로 빼지 않는다. Linux `/proc/self/schedstat`의 실행 ns delta는 single-thread CPU 관측, `/proc/self/status` VmHWM은 전체 process peak RSS로 정의한다. 수집 불가·역행·불완전은 None/UNSUPPORTED이며0으로 대체하지 않는다. VmHWM을 측정 구간만의 RSS라고 하지 않는다.

최초 성능 fixture는 row_count128·pin_old=true다. correctness edge는1/128행·pin 유무와 cap0/1/256을 포함한다. 비교는 actual release의 off↔always, off↔detailed 두 campaign으로 시작하며 최초 동일 구현의 계측 비용을 보고한다. 개선 PASS를 목표로 하지 않는다. 모든 campaign은 실행 전 고정한10 paired runs, AB5/BA5 순서를 seed로 섞어 보존한다. 주 비용은 measure_ns, 최소 유용 효과5%, median ratio의 distribution-free interval은 정렬된10개 비율의 두 번째/아홉 번째(coverage1002/1024)다. interval 전체가0.95 미만이면 IMPROVED,1.05 초과면 REGRESSED, 전체가[0.95,1.05] 안이면 NO_MATERIAL_CHANGE, 나머지는 INCONCLUSIVE다. 원시 sample p99는 정렬된4,096개 중 index4,055이며 전체 latency 모집단 p99로 표시하지 않는다. 사전 guardrail은 measure elapsed/CPU/외부 request elapsed 각각+15%, 표본p99+25%, process peak RSS+4MiB이며 comparison은 각 paired guardrail의 최악값을 사용한다. 필요한 관측이 없으면 guardrails UNKNOWN·promotion INCOMPLETE다. AB/BA 층의 주 비용 ratio median 차이가0.10을 넘으면 drift INVALID다.

recipe JSON64KiB, strict version1/unknown·duplicate field 거절, 한 campaign의10쌍과 단일 fixture/분석 정책을 원문 hash로 묶는다. source/binary/build/config·fixture/semantic/path identity, worker 사본, 원본 recipe, 각 실행의 native journal/raw streams/종료 receipt·원시 관측, 가설·재실행 지시를 소유 bundle에 보존한다. 비교 결과축은 validity/correctness/performance_effect/guardrails/cause_status/promotion을 분리한다. ratio는 checked 정수 ppm을 쓰며0 분모/overflow는 성공 판정이 아니다. baseline/candidate의 계측 모드·binary/source 차이만 명시적으로 허용하며 fixture·의미·부하·실제 경로·완료량 mismatch는 INVALID다. 한 pair의 실패도 삭제하지 않는다. 탐색 후보는최대16개, 고정 candidate의 최종 확인은1 campaign이며 탐색·확인 run ID는 겹치지 않는다. winner 재사용·중복 확인·기준 자기 변경·누락 run의 거짓 개선을 반례로 거절한다. 로컬 파일 분리는 보호 CI 권한 분리로 주장하지 않는다.

고정 분석 vector는 동등/개선/악화/불확실·order drift·우연한 탐색 winner 재사용, 잘못된 identity/부하/경로·요청/관측 누락·timeout 은폐다. bundle만 남긴 별도 cwd에서 baseline/candidate를 새 process로 재실행해 정답과 비교를 다시 산출한다. PF01/03/04/05/07/13의 현재 K0 variant를 목표로 하며 WAL/sync·중첩·병렬 clock·내구성 의무와 PF12 실제 국소 최적화는 별도 후속이다. row-ref 불필요 복사는 관측 후 검토할 가설일 뿐 현재 최적화나 효과로 기록하지 않는다.

### 재개 전 해결할 항목

아래는 앞의 제안에 아직 반영해야 할 검토 의견이다. 미완 제안으로 보존하며 실행 증거·채택 계약으로 해석하지 않는다.

- `/proc` CPU/RSS 수집은 platform adapter가 소유한다. root가 `crates/platform/src/performance.rs`와 module 조립·worker→platform normal 간선·배치 원문을 함께 반영할 계획이다. worker 내부 직접 OS 수집으로 배치 정책을 우회하지 않는다.
- 기존 Metrics의3개 누적 counter만 checked delta를 사용하고 resident_bytes/snapshot_pins는 before/after gauge로 다룬다. VmHWM은 sampling 시점까지의 process peak다. warmup→event drain→before metrics→CPU/Instant 시작→측정→Instant/CPU 끝→after metrics→event drain→최종 correctness 조회의 cut를 반영한다.
- 모드별 수집 범위와 completed의 오류 포함 여부, 제안한 diagnostics64KiB 논리 비용식·drain 반환 메모리 범위를 확정한다. 아직 수치만으로 총 RSS 상한을 주장하지 않는다.
- ratio ppm을 floor로 계산해 경계 초과를 PASS시키지 않도록 checked cross-product 또는 보수적 경계 비교를 확정한다. 예를 들어10,500,009/10,000,000을1.05 이하로 분류하면 안 된다.
- 최초 두 계측 비교는 InstrumentationAudit, candidate 채택 확인은 단일 Confirmation, 새 세션 재현은 ReplayAudit로 구분한다. ReplayAudit는 원본 recipe/확인/실패를 보존하고 새 run IDs로 재실행·재분석하되 promotion=INCOMPLETE로 고정한다. 이 구분을 wire/recipe·분석 규칙에 반영해야 한다.

K0 성능·bundle 의무와 K1 트랜잭션·K2 WAL·K3 복구·K4 동시성·K5 checkpoint/restore는 남은 작업이다. 이번 이관은 이 기능을 새로 구현하거나 DEV2 완료를 선언하지 않는다.

### K0-005 재개 계약 — 2026-10-18

**D2 / ACCEPTED / 착수 시 고정한 계약.** 사용자가 병렬 진행안을 승인했다. 위 이관 제안의 미결 입력은 이 절에서 구체화한다. 실제 구현·시험 결과는 후속 기록으로 구분하며 이 계약의 채택만으로 K0·DEV2 전체 완료를 뜻하지 않는다. 기준 commit은 `b4d1d8c`, 실행 소스 지문은 `3fa1b7958cc5597ba99be6ba81d75ea65360657d318d22d59e6e2b633f76a540`이다. r2의 DEV0 8·DEV1 8·DEV2 28개 PASS와 같은 지문임을 재확인했다. 착수 때 별도 DEV4 설계 문서의 미커밋 변경이 있으며 그 원문·색인을 보존한다.

| 담당 | 소유·공유 변경 |
| --- | --- |
| 작업·계약·통합 `/root` | protocol performance 형식과 Action/Observation 연결, platform 수집, Cargo/공통 build identity/lib/main 조립, scripts·배치·증거·이 절. 공유 문서는 최신 원문에서 좁게 편집 |
| 구현 `/root/dev2_engine_performance` | engine diagnostics/metrics/substrate와 전용 시험, engine-worker performance 모듈과 전용 시험 |
| 구현 `/root/dev2_controller_performance` | controller performance 모듈·하위 파일과 전용 시험. production의 정답 계산은 공유하지 않음 |
| 독립 검토 `/root/dev2_performance_review` | 계약·부정 vector·최종 diff·실제 증거의 읽기 전용 검토 |

같은 작업 트리에서는 위 소유 경로만 병렬 편집한다. 공통 계약 → 공통 wire/adapter → 두 구현 → 조립/독립 검토 → 소스 변경을 멈춘 통합 검증 → 증거·문서 등록 순서다. 각 구현의 Cargo 검사는 별도 `/tmp` target을 사용하고 root만 최종 workspace/stage 검사를 실행한다. 성능 측정 동안 다른 빌드·시험은 중지하고 환경을 보존한다. 다른 작업의 source 변경이 생기면 최종 지문을 다시 고정한다.

새 diagnostics는 별도 `diagnostics` 모듈과 `VolatileStore::new_with_diagnostics`/`diagnostics`/`drain_diagnostics`로 제공한다. 기존 `new`는 Off다. Off는 모든 추가 관측을 None으로 반환하며 queue/추가 counter 저장소를 할당하지 않는다. Always는11개 operation의 started/completed/failed만, Detailed는 같은 counter와 종료 event를 수집한다. completed는 Ok와 Err의 정상 반환 전체, failed는 Err 부분집합이며 panic은 완료로 꾸미지 않는다. event는 종료 시점 operation·실제 ErrorCode와 sequence만 포함하고 시간 span이 아니다. sequence 포화 뒤 새 event는 drop하며 중복 sequence를 발급하지 않는다. 진단 mutex 경합·poison은 product root와 분리된 관측 손실로 집계하며 정상 제품 동작을 바꾸지 않는다.

diagnostics의 engine 소유 논리 bytes는 고정 collector 크기 + Arc header + 실제 예약 queue capacity×event 크기이며 최대64KiB다. 별도 예산으로 초기 준비하고 기존 row quota에 charge하지 않는다. Snapshot/Cursor에 추가하는 동일 크기 참조 필드의 기존 pin 회계는 모든 mode에서 같으며 source build별 quota vector로 검증한다. cap0..1,024(기본256), Always/Off는 queue를 만들지 않는다. drain은 반환 buffer를 먼저 제한적으로 준비한 다음 try_lock으로 이동하고 원래 queue capacity를 유지한다. 단일 반환 backing은 추가로 최대 cap×event 크기이고 caller가 여러 반환을 보유하는 메모리는 engine 상한 밖이다. allocation 실패는 수집 오류, 경합·poison은 unavailable로 표시한다. 이 숫자는 RSS/OOM 보장이 아니다. counter saturation은 공개하며 정확 delta로 해석하지 않는다.

측정 입력은 version1·seed·row_count(1 또는128)·pin_old·mode·capacity·warmup4096·measure65536·latency_stride16이다. 측정 loop의 값·latency 출력 buffer는 측정 전에 준비한다. warmup → event drain → before metrics/diagnostics → CPU/Instant 시작 → replace·snapshot·read·view/snapshot 해제 반복 → Instant/CPU 종료 → after metrics/diagnostics → event drain → 최종/old 행 정답 조회 순서다. 반환값 저장과 표본 clock 비용은 명시적으로 측정 안에 있다. 기존3개 누적 counter만 checked delta로, resident/pins는 before/after gauge로 해석한다. setup/warmup/measure 시간과 실제 완료수·오류를 분리하고 누락 값·실패를 성공 관측으로 채우지 않는다. `/proc`는 platform adapter만 읽으며 CPU는 main thread schedstat 실행 ns delta, RSS는 수집 시점 process VmHWM bytes다. 미지원·읽기 오류·역행은 None이다.

worker 출력은 위 입력·실제 경로 `k0.replace_snapshot_read.v1`·build source/toolchain/target/profile identity, 구간 시간·완료수·오류, measurement 값65,536개·최종/old 값·latency4096개, before/after 기본/추가 counter, bounded event/drop/saturation, CPU/RSS 관측을 포함한다. wire는 strict version1/unknown·duplicate field 거절이며 기존4MiB frame·5초 요청·60초 run 한도를 유지한다. Performance는 actor0의 Initialize 바로 다음 request2에서만 허용하고 한 session에 한 번으로 종료한다. 기존 catalog oracle/recipe는 이 확장을 지원하는 것처럼 수용하지 않고 INVALID로 거절한다. 공유 build helper는 controller와 worker의 실제 build 지문을 동일 산술로 기록하며 worker가 자기 품질 verdict를 제출하지 않는다. 실제 release build receipt·binary hash는 root의 실행 증거에 묶고 실행 전후 source/binary identity를 대조한다.

비교는 고정10쌍·AB5/BA5의 seed shuffle이며20개 run ID 전체가 유일해야 한다. ratio의 numerator/denominator 원본을 보존하고 정렬·threshold·guardrail·drift는 checked u128 cross-product(필요시 overflow 거절)로 판정한다. ppm은 표시 전용이다. 10 ratio의 median은5·6번째의 평균이고 interval은2·9번째다. coverage1002/1024는 독립·동일분포 pair 가정 아래 nominal이며 drift 검사가 그 가정을 증명하지 않는다. AB/BA 각5개 median(3번째)의 절대 ratio 차이가0.10 초과면 INVALID다. 정확 경계0.95/1.05는 NO_MATERIAL_CHANGE 구간에 포함하고, 1.0500009·1.1500009·1.2500009를 반올림해 허용하지 않는다. 기존 +15% elapsed/CPU/외부 Invoke→Observe, +25% sample p99, +4MiB process RSS guardrail을 모든 pair에 적용한다. 외부 시간은 controller journal의 동일 clock에서 계산한다.

InstrumentationAudit(동일 release의 Off↔Always, Off↔Detailed)와 ReplayAudit는 promotion=INCOMPLETE다. Confirmation은 같은 계측 mode/capacity·fixture/분석 계약, 탐색 candidate≤16·새 run·확인 시도1을 요구한다. 탐색/이전 확인/현재20개 run의 중복·누락·기준 변경을 거절하며 실제 독립 증거가 부족하면 INCOMPLETE다. 최초 audit를 최적화 채택 증거로 쓰지 않는다. recipe는64KiB strict JSON이고 원문 hash·build/config·fixture/semantic/path·controller/oracle/분석 identity·환경·실제 명령·가설/다음 실험을 bundle에 보존한다. worker 사본·native journal/raw streams/종료 receipt를20개 모두 보존하고 실패도 삭제하지 않는다. verify는 원본에서 재판정하고 replay는 원본 recipe/hash/결과를 보존한 채 별도 cwd·새 process/run ID로 다시 실행하는 ReplayAudit다. 로컬 identity/이력 검사는 보호 CI 권한 분리를 대신하지 않는다.

필수 반례는 모드별 CRUD/old snapshot/ID/오류/quota 동일성,11개 operation 성공/실패 counter, queue cap0/1/256·경합·poison·포화·drain 수명, 잘못된 seed/warmup/old row·값/완료량 누락, CPU None·gauge 감소·saturation, 통계 동등/개선/악화/불확실/정확 경계/drift, identity·unknown/duplicate/초과 입력·누락 이력·timeout 은폐·run 재사용·재현의 승격 위조다. root는 release 두 audit와 별도 cwd replay, 독립 build·부정 시험·기존28-check와 DEV0/1 회귀를 실행한다. PF01/03/04/05/07/13의 이 capability만 기록하고 PF12 국소 최적화, bounded concurrent 전수,30분 stress/fuzz, WAL/내구성 및 K0/DEV2 전체는 미완료로 유지한다.

독립 검토 `/root/dev2_performance_review`가 재개 계약·미결 반례를 확인하고 착수 차단 의견 없음으로 검토했다. `/root`가 D2 입력을 채택한다. diagnostics snapshot은 각 atomic의 개별 관측이며 동시 단일 시점 snapshot이 아니다. 현재 단일 thread fixture에서만 exact delta를 검증한다. 이 결정은 구현 diff·실행 결과의 후속 독립 검토를 대신하지 않는다.

K0-005 최종 통합 검토에서 재현 단위를 구체화했다. campaign은 worker 두 사본·receipt와 함께 실행한 controller 사본 및 baseline/candidate build config 원문을 보존하며 config hash를 side/receipt에 대조한다. 원래 실행 controller와 재분석 analyzer identity는 결과에서 구별한다. 실제 검증은 campaign 전체를 별도 cwd에 복사하고 그 안의 controller로 새 process/run IDs를 만든다. 상위 check-session은 fingerprint 대상 source 원문도 보존한다. 각 latency 표본은 measurement 시간 이하여야 하며 setup+warmup+measurement 합은 외부 Invoke→Observe 시간 이하인지 checked/u128 포함 관계로 검사한다. 서로 다른 시계 timestamp를 빼지 않으며 equality는 허용한다. AB/BA drift만 발생한 완전한 실행은 성능 validity=INVALID를 그대로 보존한다. 이때 wrapper 실행 정합성 PASS는 효과·guardrail·승격 PASS를 뜻하지 않으며 나머지 증거 누락/실행 실패/정답 오류는 wrapper FAIL이다.

Detailed의 고정 single-thread fixture에서는 측정 전 completed counter 합 다음 sequence부터 Replace→Snapshot→Read 순서로 cap개 이벤트를 보존해야 한다. 이 fixture에 수집 경합이나 다른 drain 호출자는 없으므로 정확한 sequence·operation·길이·drop delta로 warmup 미배출과 잘못된 operation 라벨도 거절한다. 일반 concurrent diagnostics API의 lossy 수집 허용을 이 fixture의 증거 누락 허용으로 확장하지 않는다.

### K0-005 구현 범위

engine의 [diagnostics](../../crates/engine/src/diagnostics.rs)와 기존 substrate 경로를 연결했고, [worker fixture](../../crates/testing/engine-worker/src/performance.rs)는 별도 성능용 모형이 아닌 실제 `VolatileStore` API를 실행한다. [controller 분석·재현](../../crates/testing/controller/src/performance.rs)은 별도 scalar 기대값과 exact ratio 산술을 소유한다. CPU/RSS 수집은 platform에 두고 kernel/reference가 OS 관측을 직접 소비하지 않는다. 기존 catalog 이력 경로는 Performance 입력·관측을 거절한다.

실행 진입점은 `python3 -B scripts/check_dev2_performance.py`다. 기존 DEV0·DEV1·28-check 회귀와 고정 성능 반례 35개, 새 target release build, Off↔Always·Off↔Detailed 각10쌍, 별도 cwd의 보존 controller ReplayAudit10쌍을 순서대로 수행한다. CLI `performance-profile`, `prepare-performance`, `run-performance`, `verify-performance`, `replay-performance`는 원본 recipe/hash와 receipt/config를 구체 입력으로 사용한다. 완전한 campaign의 `resume.md`에 재검증·재현 명령을 남기고 저장된 comparison을 신뢰하지 않은 재분석을 수행한다.

독립 구현 검토에서 configured capacity의 양쪽 해석, controller/config 원문 보존, 불가능한 시간 포함 관계, Detailed event의 sequence·operation 검증을 보완했다. `/root/dev2_performance_review`는 수정된 코드·반례·runner를 읽고 통합 실행 GO를 확인했다. 실행기 첫 시도 `artifacts/dev2-performance/check-287gdq9o`는 기존 전수 회귀 소요에 비해 짧은 상위 timeout을 발견해 중단했고 `execution-interruption.json`에 보존했다. 제품 request/run 한도를 바꾸지 않고 하위 회귀를 기다리는 상위 command 한도만1,800초로 조정했다. 중단한 결과는 현재 소스 PASS로 사용하지 않는다.

### K0-005 실제 실행 결과 — 2026-10-18

[성능 진단 원본 보고서](../evidence/dev2-performance-xadb-20261018-r1.json)의 source는 `6e58ed3534a1c996a0d5bf9ea56e3f4e81d6dc95bc2b67fc704589f12b4a0e8f`다. 전후·retained source 지문이 같고 wrapper19개 check와 필수 시험35개가 PASS했다. [DEV2 r3](../evidence/dev2-xadb-20261018-r3.json)의28개 check 및 DEV0·DEV1 각8개 회귀도 같은 source에서 PASS했다. workspace fmt·Clippy·all-features/default·fresh source build와 V2/V3 이력 검사는 해당 원본 로그에 묶인다. 보존한 release worker/controller와 각 config/receipt hash를 대조했고 새 cwd의 campaign 사본에서 같은 controller로 재현했다.

| 비교(각10쌍·20run) | 정답 / validity | measurement 중앙 비율(candidate/Off) | 효과 | guardrail |
| --- | --- | --- | --- | --- |
| Off↔Always 최초 audit | PASS / VALID | 약1.01328 | NO_MATERIAL_CHANGE | FAIL: 최악 pair의 표본 p99 약1.44569배 |
| Off↔Detailed 최초 audit | PASS / VALID | 약1.04266 | INCONCLUSIVE | FAIL: 최악 pair의 표본 p99 약2.76491배 |
| Off↔Always ReplayAudit | PASS / VALID | 약1.01507 | NO_MATERIAL_CHANGE | PASS |

세 campaign의 원시20run씩과 원본 replay 계보·새 run IDs·실패 축을 보존했다. 이 표의 비율은 표시용 근삿값이며 판정에는 원본 정수 비율을 사용했다. 최초 audit의 p99 실패는 후속 replay PASS로 지우지 않는다. 모든 promotion은 INCOMPLETE이며 CPU·elapsed·외부 시간·RSS의 현재 한도 통과만으로 p99 한도나 원인 규명을 대신하지 않는다. 실제 원인·국소 최적화(PF12), 통합 PF 인수, bounded concurrent 전수,30분 stress/fuzz, K1–K5와 영속성은 남은 의무다. K0·DEV2 전체는 INCOMPLETE다.

최종 독립 검토 `/root/dev2_performance_review`는 현재/보존 source 지문, wrapper19개·필수 시험35개 및 하위28/8/8개 실행 로그 hash, controller/worker/config/receipt/index와60개 실행 원시 파일 hash를 읽기 전용으로 대조했다. 모든 run ID의 유일성, journal↔stdout·envelope·65,536개 값·최종/old 행·counter·Detailed sequence/drop·시간 포함 관계를 별도로 확인했다. 원본 Always campaign은 별도 cwd 사본과 replay의 original에서 byte-identical이며 median/interval·최악 pair 표본 p99의 Fraction 재계산이 원본 보고서와 일치했다. 통합 차단 결함 없이 실행 기능·재현 leaf 통합 GO다. 최초 성능 budget 실패·원인 미규명·승격 INCOMPLETE는 그대로 남으며 성능 인수 GO를 뜻하지 않는다. `/root`가 해당 검토와 원본 보고서 등록을 확인했다.

## DEV2-K0-006 — 장시간 thread와 native parser 검증 확장

**D2 / 구현 중 / 장시간 실행 사용자 요청으로 중단.** 사용자 DEV2 완료·병렬 진행 요청에 따라 기존 K0-005를 기준으로 현재 capability의 V3 미실행 의무를 진행한다. 작업·계약·통합 담당은 `/root`, 독립 검토자는 `/root/dev2_completion_review`다. 기준 commit은 `b8b1e60a314d397198e2f2c41205f9e468a6decf`, 착수 기준 소스 지문은 `6e58ed3534a1c996a0d5bf9ea56e3f4e81d6dc95bc2b67fc704589f12b4a0e8f`다. 착수 초기 보였던 K0-005 미커밋 변경은 해당 commit에 반영되었으며 최신 clean 상태를 다시 확인했다. 과거 원본 증거는 수정하지 않는다.

| 소유 | 수정 범위·순서 |
| --- | --- |
| `/root/dev2_k0_stress` | 신규 `crates/engine/tests/stress.rs`: 실제 단일 writer와 immutable snapshot reader 장시간 시험·짧은 smoke |
| `/root/dev2_k0_fuzz` → `/root` | 신규 `crates/testing/protocol/tests/fuzz.rs`와 필요한 전용 support: native Request/Response mutation fuzz·보존 corpus |
| `/root` | 신규 `scripts/check_dev2_extended.py`·그 runner 반례 시험, 이 기록과 필요한 문서/증거 등록. 공유 파일은 최신 원문에 순차 반영 |
| 독립 검토 | 각 입력 profile → 구현 diff·기존 반례 → smoke → 고정 source의 실제 장시간 증거. 구현자가 자신의 작업을 독립 승인하지 않음 |

기존 제품·wire·정답 의미와 dependency는 바꾸지 않는다. 짧은 seed 회귀만 반복하는 대안은 DP03의 시간 기반 의무를 충족하지 않는다. 새 외부 fuzz dependency 대신 구체 Request/Response 경계에 유계 mutation generator와 독립 literal vector를 추가한다. 이는 coverage-guided fuzz나 모든 입력의 전수 증명이 아니다. K0 writer는 `&mut VolatileStore` 하나이며 K4 다중 transaction 동시성·내구성 검증으로 표시하지 않는다.

장시간 target은 `k0_real_thread_stress`, `native_request_fuzz`, `native_response_fuzz` 세 개다. 각각 실제 monotonic 반복 구간 최소1,800초, 외부 process timeout2,400초를 고정한다. 빌드·sleep·다른 target의 경과 시간을 합산하지 않는다. 입력 corpus/seed/generator와 source hash를 실행 전에 보존하고 각 target의 원시 출력·완료량·elapsed·exit status·binary hash를 연결한다. 같은 source의 release/no-default-features 시험 실행 파일을 source-only export에서 빌드하고 별도 cwd에서 실행한다. 세 target의 병렬 실행은 정확성 검증이며 성능 비교로 쓰지 않는다. 짧은 smoke는 별도 시험명과 결과이며 V3 완료로 승격하지 않는다. timeout·panic·missing receipt·0 tests·시간 미달·필수 lane 미도달은 FAIL다.

별도 미완료 의무는 P0의 bounded concurrent program 공간 전수, controller catalog/performance recipe·config/bundle JSON의 해당 V3 parser target, 후속 영속 codec의 V3, K0 PF12와 최초 sample-p99 실패 원인/해결이다. 이 leaf를 실행해도 K0·DEV2 전체를 완료 처리하지 않는다. 기존 V2/V3 seed 회귀와 DEV0/1 하위 회귀는 유지한다. 문서 자동 검사는 push 훅에서만 수행한다.

### 실행 전 동결한 구체 profile

thread generator는 `k0-thread-v1`, seed0부터0..255 순환, Off/Always/Detailed(cap0/1/256)의5 lane이다. 각 lane×seed 최소1,280 round를 모두 방문하고 각 lane 최소256회, 실제 reader4개 각각의 연산과 writer 연산을 확인한다. 16행의 독립 Int64 scalar 기대값, barrier를 통한 replace/delete/drop-recreate, old snapshot/cursor/page/view 수명, pin4→4→0, 동일 empty-catalog capacity의 resident 회복을 대조한다. reader와 writer의 diagnostics 수집·drain을 겹치고 join 후 정확한 operation counter·event sequence·drained+dropped 회계를 확인한다. Off의 관측 없음과0을 구별한다. 시작·종료 receipt와 실제 lane/seed/thread 작업량을 교차 검증한다.

native mutation generator는 `k0-native-mutation-v1`/SplitMix64, Request seed `0x5841444252455131`, Response seed `0x5841444252455331`이다. Request17·Response21 literal variant(Performance mode와9개 product error 포함), strict malformed corpus, 17개 mutation operator, 13개 공통 family(Response는 product_code 추가)를 선언한다. body 길이·UTF-8·identity·unknown/duplicate field/tag·모든 짧은 prefix·4MiB 한도/초과·escape 팽창·연속 frame·valid prefix 뒤 손상 suffix·poison/EOF/progress를 검사한다. 입력 cap은8,388,612bytes다. 초기 corpus는 Request322·Response223 cases이며 각variant와family/operator 실제 방문·완료량 합계를 확인한다. literal 정상/거절 기대값과 별도 raw framing/identity 검사를 사용하며 arbitrary mutation의 blocking/stream 대조가 공통 JSON decoder 의미의 독립 증명은 아님을 명시한다. 실패 input·chunk PRNG state·seed·iteration을 별도 파일에 보존하고 재현할 수 있다.

별도 검토자는 착수 profile과 thread 최종 diff를 검토했다. 처음 thread smoke에서 생성 직후 empty root184bytes와 create/drop 이후192bytes를 잘못 비교했다. 소유 구현이 보유하는1-slot 배열의8bytes를 독립 확인하고 fixture를 동일 create/drop-empty 상태로 맞췄다. 제품 quota 한도를 늘리거나 실패를 삭제하지 않았다. Native 최초 smoke는 valid text mutation이 Performance의 `seed` 필드명까지 바꾼 generator 오류로 실패했다. 원문·binary·입력·실패 로그는 `artifacts/dev2-extended/first-fuzz-generator-failure-0te6rm16`에 보존했고 문자열 값 위치만 바꾸도록 수정했다. 두 native smoke는 수정 뒤 PASS다. helper agent 중단 뒤 `/root`가 native 구현·검증을 이어받았으며 최종 독립 리뷰는 별도로 수행한다.

runner 반례는0 tests·누락/중복/외국 identity·시간 미달·불가능한 duration·필수 작업량 누락·가짜 exit0·crash·timeout·spawn 실패·자손 process cleanup과 원문 로그 보존을 확인한다. timeout 상태를 다음 poll에서 잃는 초기 오류를 반례로 잡아 고쳤다. long 실행은 `--nocapture`로 profile/panic을 즉시 보존하며 exact 시험명·1개 실행·최종1PASS/0FAIL/0ignored와 고정 JSON receipt를 함께 확인한다. 원래 실패를 후속 PASS로 지우지 않는다.

### 사용자 실행 순서 변경 — K1–K5 구현 우선

2026-10-18 사용자가 **“K1~k5구현 완료 될 때 까지 테스트 금지”**를 명시했다. 이를 현재 작업의 실행 순서에 우선 적용한다. K1–K5 production 구현을 모두 연결하기 전에는 unit/integration/property/stress/fuzz/benchmark/회귀 및 문서 검증을 실행하지 않는다. 코드 작성과 독립 정적 검토는 병렬 진행한다. 기존 단계별 실행 의무를 삭제하거나 통과로 간주하지 않으며, 구현 완료 후 통합 source에서 검증을 수행한다.

실행 중이던 세 K0-006 target은 약464초에 중단했다. runner PID553303에 SIGINT를 전달했고 runner가 자식 process group을 종료·회수했다. [원본 보고서](../evidence/dev2-extended-xadb-20261018-r1.json)는 runner의 FAIL을 그대로 보존한다. 개별 `runner interrupted`·exit−9·실제 경과는 `artifacts/dev2-extended/check-s6196li5/target-results.json`, source/binary/raw log는 같은 디렉터리에 남겼다. 30분 의무를 충족하지 않았고 하위 회귀는 시작하지 않았다. 이는 사용자 요청에 따른 실행 중단이며 제품 결함 발견이나 PASS를 뜻하지 않는다. 추가 bounded exhaustive 실행도 보류한다.

## DEV2-K1–K5-001 — 영속 커널 구현 계약

**D2 / 구현 연결·독립 정적 검토 완료 / 로컬 부분 검증, DEV2 인수 미완료.** 작업·통합 담당은 `/root`, 독립 검토자는 `/root/dev2_completion_review`다. 기존 DEV2의 private/COW·coarse validation·L0 AXF1·exact checkpoint 계약을 구현하며 DEV3의 mutable page/undo를 앞당겨 넣지 않는다. 기준 commit은 K0-006과 같고 중단 실행의 retained source는 `artifacts/dev2-extended/check-s6196li5/source-inputs`에 보존한다. 별도 작업의 `disk-storage-design.md` 변경은 수정하지 않는다. 사용자 지시에 따라 아래 구현이 모두 연결되기 전에는 테스트를 실행하지 않는다.

| 담당 | 구현 소유·공유 변경 |
| --- | --- |
| `/root/dev2_k0_stress` | `crates/engine/src/transaction.rs`, `transaction/`, `publication.rs`, paired private quota에 필요한 `catalog.rs`·`storage/volatile.rs`. 기존 K0 API는 유지. 루트가 engine `lib.rs`를 연결 |
| `/root/dev2_performance_analysis` | `crates/foundation/src/namespace.rs`, `crates/platform/src/namespace.rs`와 각 `lib.rs` 연결. 기존 nix의 디렉터리 feature만 필요한 경우 추가 |
| `/root` | `crates/engine/src/durable/`, `database.rs`, engine `lib.rs`, foundation codec의 streaming digest 경계, 문서·빌드 inventory와 실제 통합 |
| 독립 검토 | 착수 계약과 각 diff를 정적으로 검토. 검토 중 실행 시험을 추가하지 않음 |

### K1·K4 트랜잭션 경계

새 `VolatileDatabase`와 `Transaction`은 K0의 단일 작업 driver와 분리한다. 고정 snapshot·읽기 전용/읽기 쓰기·statement COW 준비·read-your-writes·명시 rollback·coarse start-q validation을 제공한다. private allocation은 거래별8MiB와 전체64MiB에 함께 예약하고, 둘째 예약 실패 때 첫 예약을 RAII로 반환한다. statement의 새 root가 모두 준비된 뒤 private root를 교체하므로 오류는 이전 statement 결과를 보존한다. RowId·TxnId는 실패나 rollback 뒤 재사용하지 않는다. active transaction과 user pin 각각 최대32다.

commit은 coordinator 아래 validation·resident root 준비를 마치고, 내부 callback에서 WAL을 준비·동기화한 뒤 publication lock 안에서는 Arc 교체만 한다. 해제는 lock 밖이다. callback은 public API가 아니며, 최종 `Engine`은 private `Transaction`을 감싼 durable transaction만 반환하여 volatile commit 우회를 막는다. 읽기 전용과 무변경 읽기 쓰기는 WAL/q를 생성하지 않으며 후자는 start-q를 검사한다. 기존 K0의9개 오류 번호는 변경하지 않고 transaction 오류·효과·certainty를 별도 타입으로 제공한다. 취소는 commit 시도 전만 abort이며 시도 후에는 기존 결정을 마친다.

### K2–K5 형식 동결 제안

모든 record는 기존 AXF1 외피와 SHA-256 검사를 사용한다. 내부 공통 prefix는 아래43bytes다. 숫자는 little endian이며 지원 형식은1, required features는1만 허용한다. 0 DB/timeline/epoch·unknown tag·초과 길이·trailing bytes는 거절한다. 이 표는 실행된 golden vector가 아니다.

| offset | bytes | 의미 |
| --- | --- | --- |
| 0 | 4 | ASCII `XDK1` |
| 4 | 2 | engine format=1 |
| 6 | 8 | required features=1 |
| 14 | 1 | kind: Header1, Batch2, Seal3, Resume4, Writer5, Current6, Manifest7, Chunk8 |
| 15 | 16 | DatabaseId 원본 bytes |
| 31 | 8 | timeline |
| 39 | 4 | writer epoch |

`Link`는 epoch(u32),segment(u64),end_offset(u64),file_size(u64),q(u64),prefix_digest(32) 순이다. rotation은 seal까지 완결된 이전 파일, resume은 완결 prefix 끝과 남겨 둔 tail의 원래 파일 크기를 구별한다. `Anchor`는 epoch(u32),segment(u64),header_digest(32),offset(u64),prefix_digest(32),q(u64)다. optional은 0/1 한 byte이며 payload가 없는 0 뒤에 padding을 허용하지 않는다.

Header는 segment·start_q·origin(genesis/rotation/resume/restore)·이전 Link·resume digest를 가진다. Batch는 txn local(u64)·prev_q(u64)·q(u64)·change count(u16)와 Create/Drop/Put/Delete를 저장하고 `XDCM`,반복 q(u64),count(u16),body bytes(u32) trailer로 끝낸다. DDL은 단독 변경, DML은 행별 최종 after-image/tombstone으로 최대256개다. object handle은 object/generation/schema epoch 각각u64, row 번호는u64다. schema 이름 길이는u16, 열수u16, 각 열은 이름·type(u8)·nullable(0/1)이다. row는 cell count(u32) 뒤 NULL0/Bool1/Int642/Utf83/Binary4 tag와 원본 scalar bytes다. 가변 값은u32 길이 뒤 원본 bytes다. 값64KiB·row128KiB·전체 payload1MiB를 유지한다.

Seal은 끝 offset·q·prefix digest, Resume은 새 segment와 이전 Link, Writer는 segment·header digest다. Current는 current와 optional previous manifest generation·digest만 선택한다. Manifest는 generation·cut q·epoch high-water·replay Anchor·schema·chunk inventory, Chunk는 generation·index와 분할하지 않은 row after-image를 저장한다. 이름은 형식/identity로부터 결정하며 임의 경로를 payload에서 받지 않는다. chunk 크기1MiB·manifest1MiB·파일 inventory4,096개를 적용한다. bootstrap은 C=0의 빈 manifest, restore는 원본 WAL에 연결하지 않는 새 timeline·header·anchor를 선택한다.

WAL source를 truncate/덮어쓰기하지 않고 검증 prefix를 STABILIZE한 뒤 새 epoch·WRITER를 선택한다. checkpoint는 coordinator에서 root와 정확한 anchor를 함께 capture하고 lock 밖에서 chunk/manifest를 쓴다. 마지막 CURRENT 선택만 coordinator 아래 수행한다. current/previous 전체 closure와 backup/build/replay pin이 없는 파일만 Retiring으로 바꿔 회수한다. selector 결과 불명은 fence하고 old/new closure를 유지한다. 검증은 K1–K5 구현 완료 후 수행하며 그 전까지 durability·recovery·동시성·checkpoint 인수는 NOT_RUN이다.


### K1–K5 구현 연결과 정적 검토 — 최초 실행 전 기록

현재 [트랜잭션](../../crates/engine/src/transaction.rs)·[publication](../../crates/engine/src/publication.rs), [WAL](../../crates/engine/src/durable/wal.rs), [복구](../../crates/engine/src/durable/recovery.rs), [체크포인트 생성](../../crates/engine/src/durable/checkpoint.rs)·[보존 ledger](../../crates/engine/src/durable/retention.rs)·[archive](../../crates/engine/src/durable/backup.rs) 소스가 있다. [Engine](../../crates/engine/src/database.rs)과 [유지 작업](../../crates/engine/src/database/maintenance.rs)은 create/open·durable transaction·checkpoint·backup·restore_new·collect_garbage를 연결한다. Linux namespace 효과는 foundation port와 platform adapter에 분리했다. 이 절은 최초 빌드·실행 전 설계 기록이다. 이후 native durable 이력·계측 연결과 최종 통합 정적 검토를 완료했고, 아래 실행 기록부터 로컬 검증을 시작했다.

독립 검토자는 각 단계의 실제 diff에서 namespace exclusion/파일 identity, private quota/빌린 읽기 수명, commit sync/publication, frame·replay closure, exact-cut checkpoint/retention/archive를 검토했다. NULL 중심 행의 decoded allocation 증폭은 record당2MiB 상한으로 제한했고 checkpoint packing은 wire1MiB와 decoded2MiB를 함께 적용한다. 복구 replay는 테이블별 최종 배열을 한 번 준비하여 old+intermediate+final 중복 보유를 피한다. 복구 임시 plan은 소비형 API에서 lease까지 해제한 후 새 epoch를 구성한다. 이 정적 검토를 실행 PASS·내구성 자격으로 사용하지 않는다.

관리 메모리는 실행 전 다음 값으로 구체화했다. Engine의 전체 resident 상한64MiB 중8MiB를 control용으로 선예약하고, 그 하위 quota에서 retention entry·system pin·manifest·name/inventory·Engine 고정 allocation을 회계한다. 일반 root·old root와 core 고정 allocation은 나머지 최대56MiB를 공유한다. `control_reserved_bytes`와 `control_used_bytes`는 구별하며 이 값은 RSS가 아니다. 작은 설정에서 관리 예약에 실패하면 READY 전에 QuotaExceeded로 거절한다. 일반 행이 여유를 모두 사용해도 GC와 checkpoint 선택을 위한 관리 예산을 사용할 수 있게 하기 위한 초기 계약 구체화이며, 실행 실패 뒤 fixture를 줄인 결과가 아니다. private 거래별8MiB/전체64MiB와 공통 scratch16MiB는 별도다.

checkpoint build는 coordinator에서 root·q·정확한 WAL anchor·generation·build pin을 함께 capture한다. immutable 파일 생성은 coordinator 밖에서 진행하고 파일 slot을 먼저 예약한다. selector 직전에는 builder scratch를 반환하고 current/previous closure와 새 파일의 합집합을 Selection pin으로 보존한다. CURRENT 선택 오류는 fence와 합집합 pin을 유지한다. GC는 coordinator/ledger에서 Retiring 승인을 마친 뒤 namespace 직렬화 mutex만 보유하여 unlink·directory sync한다. unlink가 승인된 파일은 이후 pin을 받지 않는다. Drop은 파일 삭제·동기화·namespace 조회를 수행하지 않는다. 높은 orphan epoch/generation은 durable high-water가 그 값을 보존하기 전까지 회수하지 않는다.

backup은 선택된 checkpoint C의 manifest/chunk만 복사하며 WAL을 포함하지 않는다. 현재 구현은 복사 동안 namespace mutex를 보유하므로 커밋이 그 I/O를 기다릴 수 있고, 고정 snapshot 읽기는 계속 가능하다. 원본 검증 오류와 대상 I/O 오류를 분리한다. restore는 빈 대상에서 같은 DatabaseId, 원본과 다른 nonzero timeline, 원본 high-water보다 큰 epoch를 선택하고 모든 checkpoint record를 새 namespace로 재인코딩한다. manifest provenance에는 원본 DB/timeline/generation/manifest digest를 기록하고 후속 정상 checkpoint에도 유지한다. manifest의 common epoch와 capture anchor epoch는 같다. 대상 CURRENT와 WRITER가 모두 영속화되기 전 정상 Engine을 반환하지 않는다.

사용자 실행 금지는 K1–K5 구현 연결·독립 정적 검토 완료까지 지켰다. 그 이후 필요한 검증을 시작했으며, 새 변경·실패·미해결 문제 또는 최종 고정 source의 증거 결합에 필요한 검사만 수행한다. 이전 K0 실행 결과는 변경된 K1–K5 소스의 검증 결과가 아니다.


### K1–K5 로컬 구현 검증과 단계별 commit — 2026-10-18

K1–K5 production API 연결과 독립 정적 검토를 마친 뒤 테스트를 재개했다. 사용자가 요구한 순서는 구현 완료 전 실행 금지였으며, 현재 단계는 구현 후 검증이다. 아래 로그는 `artifacts/dev2-kernel-20261018-integration-r1/`의 개발 중 로컬 source 실행이다. 최종 동결 export·V3·성능·하위 회귀를 모두 통과한 DEV2 완료 증거로 사용하지 않는다. 원본 실패는 보존한다.

| 실제 실행 | 결과·정확한 한계 |
| --- | --- |
| `engine-unit-01.log`, `transaction-01.log` | engine 21개·transaction 7개 PASS. 이후 수정의 최종 통합 회귀는 별도 |
| `cache-loss-03.log` | cache-loss 15개 중 12 PASS·3 FAIL. 두 건은 restore 새 timeline의 RowUID를 source RowUID와 혼동한 fixture, 한 건은 실제 checkpoint+GC scratch 예약 초과 |
| `cache-loss-04-restore.log` | 두 restore/backup fixture 수정 후 해당 2개 PASS |
| `cache-loss-04-concurrent.log`, `cache-loss-05-concurrent.log` | 추가 한 페이지 가정 fixture 실패를 보존하고 bounded scan EOF 반복으로 수정. 실제 checkpoint 중 commit·rotation·GC 한 건 PASS |
| `native-fault-01.log` | durable 2개와 fault 4개 PASS. 당시 33개 case의 재개 후 이미지에 대한 독립 판정은 consistency ACCEPTED, strict INCOMPLETE이며 사전 crash image/provenance를 갖춘 최종 반복은 별도 |
| `disk-codec-v2-01.log`, `durable-wire-v2-01.log` | disk 64×128 변형 입력 및 durable/fault 4 lane 각각 64×128 변형 입력·고정 corpus PASS. coverage-guided fuzz와 V3 30분 자격은 아님 |
| `independent-oracle-02.log` | 독립 oracle 41개·bridge 34개, 총 75개 self-test PASS. production 장애 실행 PASS를 대신하지 않음 |
| `kernel-threads-v2-01.log` | 64 seed에서 실제 Linux I/O와 2 writer/4 reader, commit 4,096·충돌 4,096·고정 old snapshot 및 자유 reader 원자성 PASS. power-loss 시험은 아님 |

복구 시 CURRENT/WRITER가 없으면 불완전 생성/선택을 명시 거절하고 덮어쓰지 않는다. GC는 Retiring 승인 후 중단되면 RAII로 fence하여 부분 승인 ledger를 정상 운영으로 남기지 않는다. 복구 workspace는 고정 4,096-entry 선예약 대신 실제 제한된 inventory 수만큼 예약하고, 재탐색 중 개수가 그 수를 넘으면 grow하지 않고 거절한다. 이 변경은 16MiB scratch 한도를 늘리지 않고 checkpoint와 GC의 작은 DB 동시 실행에서 확인한 과다 예약을 줄인다. `/root/dev2_completion_review`가 실제 production diff와 위 실패 원인을 독립 검토해 GO를 확인했다. fixture의 source/new-timeline ID 구별과 scan paging 수정도 별도 기록했다.

단계별 commit은 사용자의 추가 지시에 따른다. 검증된 단위를 commit하고 다음 단위로 진행한다. 장시간·성능·최종 동결 검증 전에는 DEV2/B2/D0 완료나 공식 배포를 선언하지 않는다.

### DEV2-P0-NET-ZERO-002 — 최종 논리 상태와 무변경 commit

D2 / 채택 / 2026-10-18. 사용자 DEV2 완료 및 단계별 commit 요청에 따라 `/root/dev2_completion_review`가 구현하고 `/root`가 계약 담당·독립 실제 diff 검토·통합을 맡았다. dirty flag는 같은 값 replace·insert/delete·변경 후 원복에서 불필요한 q 증가와 UNKNOWN을 허용했다. start-version 검증 뒤 최종 논리 행을 비교하는 대안을 선택했다. 오래된 RW는 최종 행이 현재 행과 같아도 충돌하며, 명시 BeforeAttempt abort는 유지한다. 기존 dirty-flag shortcut을 대체한다. no-op/conflict/fault 의미가 바뀌면 재검토한다.

commit `ea145b9`의 history 22 PASS/extended 1 ignored, bounded smoke 1 PASS/exhaustive 1 ignored는 `p0-net-zero-01.log`에 남겼다. 전수 profile은 256 초기 상태·actor별 13 program·19 interval class·6 critical order·70 serial order, 총 9,604,608건(ACCEPTED 7,003,648/REJECTED 2,600,960)이며 전수 실행은 아직 NOT_RUN이다. Insert/Delete·다중 mutation·rollback·UNKNOWN·crash·catalog는 이 유한 공간에서 제외하고 별도 literal/integration 의무로 남긴다.


### 고정 source 검증 체크포인트와 사용자 중지 — 2026-10-18

[원본 연결과 중단 receipt](../evidence/dev2-kernel-xadb-20261018-r1.json)의 retained source는 `8e4665f374b450904a25b4c84911e7fc6a2a13c2ad0fca360f371e8a0adebb88`이다. 이 export에는 별도 미완료 성능 초안과 K0 후보도 포함되어 있으므로 이후 선택적 commit의 전체 HEAD와 같다고 주장하지 않는다. source·retained 실행 파일·사전 manifest·실제 execution·재개 전 crash image를 독립 bridge에 묶었다. engine 22개, transaction 7개, cache-loss 15개, ext4 fixture의 동시성 64 seeds(4,096 commit/4,096 conflict)가 PASS했다. 독립 oracle/bridge 75개와 kernel runner 반례 8개도 PASS했다. 이 결과는 해당 고정 source와 시험 범위에 귀속된다.

native 캠페인 첫 회는 4개 시험·33개 case와 controller 소유권 부정 사례가 모두 PASS했고 strict 독립 판정 33 ACCEPTED, 누락·중복·미연결 run 0이다. 두 번째 회는 디스크 여유 0의 ENOSPC로 1 PASS/3 FAIL, 세 번째 회는 NOT_RUN이다. 전체 검증은 FAIL/미완료이며 첫 회 PASS로 3회 의무를 대신하지 않는다. 최종 report 저장도 ENOSPC로 실패해 원래 0-byte report.json을 보존했고, 실제 원본 로그·manifest·첫 execution/summary에서 사후 receipt를 별도로 작성했다. 첫 시도에서 발견한 신규 성능 worker 빌드 오류 3개와 그 수정도 `kernel-frozen-01.log`·`artifacts/d2k-5wk_abx0/`에 보존한다.

사용자는 이번 진행 단위의 결과를 commit하고 일단 멈추도록 요청했다. 원본 source·실행 파일·crash image·journal·로그를 유지하고 이번 두 시도의 재생성 가능한 `target/` 캐시만 지워 약702MiB를 확보했다. 이후 캠페인·장시간·성능 실행은 시작하지 않는다. source-only extended runner 반례는 SHA binding 보완 후 11개 PASS지만 30분 target이나 P0 전수 실행은 아니다. native transport/fault 구현과 검증 기반을 commit하고, 미검증 성능 초안·미채택 K0 최적화 후보는 별도 작업 트리에 보존한다. DEV2/B2/D0 완료·성능 채택·공식 배포는 여전히 미완료다.

<a id="dev2-review-20261019"></a>
## DEV2 코드 리뷰 — 재시작용 용량 예약 (2026-10-19)

**D1 / 기존 자원·복구 계약의 결함 수정 / 로컬 검증 PASS.** 사용자 코드 리뷰·즉시 수정 요청에 따라 `/root`가 구현·통합하고 `/root/dev2_independent_review`가 소유 계약·원래 반례·실제 diff를 독립 검토했다. 기준 commit은 `fe8d3f5d2a7744a1c54a26fa9d30c656b974fa7c`이며 착수 미커밋 diff의 SHA-256은 `eb3abb812f0adc05517663b33c8cddd6b71477e7ecf87445432077e115d97761`이다. 이 digest는 당시 untracked 파일의 identity를 포함하지 않는다. 기존 성능 초안·K0 후보·COMM 변경과 이후 동시 작업은 보존한다.

소유 변경은 [WAL admission](../../crates/engine/src/durable/wal.rs), [빠른 경계 시험](../../crates/engine/src/database/capacity_tests.rs), [실제 용량 시험](../../crates/testing/engine-worker/tests/durable_capacity.rs), 이 기록이다. 이 작업에서 공유 `database.rs`에 추가한 것은 시험 모듈 연결이다. `persist()`의 let-chain 정리와 `io.rs`/`wal.rs`의 lint 허용 주석·attribute는 동시 변경이며 이 수정의 소유 범위에 넣지 않는다. 새 crate·의존성·실행 source root는 없으며 기존 `crates` fingerprint/clean-export 범위에 들어간다. 공개 형식과 256 MiB/4,096개 하드 상한, WAL 보존·STABILIZE·UNKNOWN·checkpoint 선택 계약은 유지한다. 수정 뒤 소스를 동결하고 해당 사본에서 통합·하위 회귀를 실행했다. 문서 자동 검사는 push gate에서만 수행한다.

### 발견·수정

- **P1 — ACK 뒤 WAL 용량 부족으로 재시작 실패.** 이전 append는 보존 WAL의 하드 상한까지 허용하지만 open은 새 epoch의 203-byte RESUME header를 더한다. 실제 Linux 파일에서 268,418,948 bytes 보존 상태에 16,135-byte Binary 교체를 commit하면 q=4,088 ACK 후 202 bytes만 남고, 다음 open이 `LimitExceeded: retained WAL budget`으로 실패했다. 원래 DB와 로그는 `artifacts/dev2-review-20261019/capacity-before-db/`와 `capacity-before-02.log`에 보존한다.
- **같은 원인의 파일 inventory 경계.** normal metadata 생성과 회전 없는 append가 다음 open의 4-slot 검사 여유를 보존하지 않았다. 정상 admission은 예약된 checkpoint slot까지 합쳐 재시작용 4개를 남기며, 기존 open이 여유를 사용한 뒤에는 파일을 추가하지 않는 write도 재검사한다.
- **회전 선행 효과.** 회전이 필요한 batch는 seal+새 header+batch+재시작 header를 첫 seal write 전에 함께 검사한다. 한도 거절은 Aborted/unfenced이며 WAL·WRITER·q를 바꾸지 않는다. 하드 상한 확대나 필수 WAL 삭제로 회피하지 않았다.

이 여유는 정상 write/metadata admission 뒤 다음 한 번의 open을 위한 것이다. 유지 작업 없이 반복 open하거나 실패한 metadata 시도를 무한히 허용한다는 뜻은 아니다. recovery 자체는 기존 하드 상한을 사용하고 소스 파일을 보존한다. 여유를 소비한 후에는 checkpoint/GC로 용량을 회수해야 한다.

### 다른 DBMS와 대조한 책임

공개 자료는 리뷰 질문의 근거이며 XADB와 같은 구현·SQL·성능이라는 주장이 아니다. 로컬 checkout은 읽기 전용으로 사용했고 아래 실제 HEAD와 clean worktree를 확인했다.

| 자료·실제 revision | 읽은 경계·XADB 적용 |
| --- | --- |
| [Oracle 26ai COMMIT](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/COMMIT.html), 2026-10-19 열람 | WAIT 성공 응답과 redo 영속화, 성공 응답 유실의 불확실성을 구분. XADB sync→publish→reply와 UNKNOWN 처리를 대조 |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`, `/home/et16/work/postgres` | `src/backend/access/transam/xact.c:1520–1564`, `RecordTransactionCommit`의 `XLogFlush` 후 commit 상태 갱신. 동기·비동기 정책을 구분해 읽음 |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4`, `/home/et16/work/mariadb` | `storage/innobase/log/log0log.cc:70–86,2041–2104`, `set_capacity`의 margin과 `checkpoint_margin`/`log_free_check`의 쓰기 전 용량 관리. XADB 재시작 여유 누락을 대조 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5`, `/home/et16/work/cubrid` | `src/transaction/log_page_buffer.c:6513–6678`, `logpb_remove_archive_logs`의 checkpoint·backup 등 필요 범위 보존. XADB current/previous closure·system pin·GC 승인 순서를 대조 |
| SQLite `43a48f77585a88940c727f33682151418f544059`, `/home/et16/work/sqlite` | `src/wal.c:2250–2281`, `walCheckpoint`의 reader 보존과 WAL→DB sync 순서. XADB immutable checkpoint/retention 검토에 사용하며 동일 알고리즘으로 해석하지 않음 |

### 실행 범위

원본 결과는 `artifacts/dev2-review-20261019/`에 보존한다. 테스트 작성 첫 빌드는 존재하지 않는 `LinuxDirectory::open` 호출로 실패했고 `open_existing`으로 바로잡았다(`capacity-before.log`). 제품 결함 반례는 그 후 실제 실행인 `capacity-before-02.log`와 `capacity-unit-before.log`다. 원래 실패를 후속 PASS로 덮어쓰지 않는다.

| 실행 | 관측·한계 |
| --- | --- |
| 빠른 용량 경계 3개 | 수정 전 3 FAIL → 수정 후 3 PASS. 202/203-byte, 회전 전 무효과, checkpoint 예약/비회전 write의 파일 여유. 합성 accounting 입력이며 하드 상한을 축소하지 않음 |
| 실제 256 MiB WAL 경계 | 수정 전 ACK 후 open 실패 → 수정 후 write 거절·기존 ACK/q 복원·checkpoint 2회·GC·새 write 성공. 실제 파일 I/O/reopen이며 power-loss qualification 아님 |
| 실제 파일 inventory 경계 | 4,092개에서 ACK 후 reopen으로 4,094개가 되어도 기존 table/q 보존. 다음 write는 무효과로 거절 |
| transaction·cache-loss 회귀 | 7개·15개 PASS. 기존 모든 선언 fixture cut와 rotation/checkpoint/backup/restore/GC를 포함 |
| 실제 durable/native fault | 2개·4개 PASS. 이번 실행은 local 1회이며 formal manifest/독립 bridge·V3 3회 반복의 완료로 주장하지 않음 |
| DEV2 P0/K0 및 DEV0·DEV1 | 고정 source에서 `python3 scripts/check_dev2_reference.py --with-substrate` PASS. DEV2 검사 28개, native 323개·catalog 325개 세션, DEV0·DEV1 각각 8개 검사 PASS. 실행 전후 source SHA-256 동일 |

고정 source는 `artifacts/dev2-review-20261019/source/`, SHA-256 `972ee1aa3ed58fd3f6902643543f369482d2e4fc85879aa56c13f3bef6a6488f`다. 이 snapshot에는 별도 작업의 변경도 포함되며 현재 전체 작업 트리와 같다고 추정하지 않는다. 리뷰 수정의 로컬 결과로 DEV2/B2/D0 완료·성능 인수·공식 배포 상태를 승격하지 않는다.

하위 회귀의 최종 보고서는 `artifacts/dev2-review-20261019/dev2-regression.json`(SHA-256 `9ff013d412abefd513bef7d09fc4a5d28b5aef4818212c62d60087c723071dae`)이며 stdout은 `dev2-regression.log`에 보존한다. 독립 검토자는 수정 코드·원래 반례·완료된 집중 시험·고정 source 지문, DEV0/DEV1의 보존 결과와 최종 DEV2 보고서·로그·native/catalog inventory의 지문을 직접 대조했다. 최종 판정은 **D1 수정 통합 차단 지적 없음 / PASS**이며 `/root`가 반영을 확인했다. 검토자는 재시험이나 파일 수정을 하지 않았다.

<a id="dev2-tests-20261019"></a>
## DEV2-TEST-20261019 — 남은 로컬 시험 실행

**실행·최종 독립 검토 완료 / DEV2·B2·D0·성능 인수 INCOMPLETE.** 사용자 “DEV2의 남은 테스트 완료” 요청에 따라 `/root`가 실행·증거 결합을 맡고 `/root/dev2_test_review`가 원래 계약·반례·diff·원시 결과를 독립 검토했다. 원본 연결은 [실행 receipt](../evidence/dev2-completion-xadb-20261019-r1.json)에 있다. 동결 source는 `47c84c22f9cf2f5720ba3f39fb310f888a0ca86c7313a9bcd7a7d47294861991`이며 이후 다른 작업의 COMM·push 검사 변경을 이번 검증으로 승격하지 않는다. 제품 소스와 기존 미커밋 변경을 보존했다.

| 실행 범위 | 실제 결과 |
| --- | --- |
| 영속 커널 release | engine 25개·transaction 7개·cache-loss 15개·독립 Python 101개 PASS. native 33셀을 fresh fixture 3회 실행해 strict 99 ACCEPTED, 누락·중복 0 |
| 기존 V3 | K0 thread·native Request/Response·durable/fault 4-codec·disk codec·durable thread의 6 target 모두 실제 1,800초 이상 PASS |
| P0 전수 | 동결 scalar grammar의 9,604,608건 = ACCEPTED 7,003,648 + REJECTED 2,600,960. invalid·incomplete·missing 0. 원래 제외 범위 유지 |
| 추가 JSON V3 | catalog recipe·performance recipe·Index/BuildReceipt 구조 parser 각 1,800초 이상 PASS. 12 mutation operator와 실제 작업량 확인 |
| 추가 literal·변이 | 정상 JSON 4개·중복/수치 위반 18개, 3 tests PASS. ACK sync 생략·old-prefix sync 생략·조기 WAL 회수의 빌드된 mutant 3개를 지정한 의미 위반으로 검출. UNKNOWN full-length 손상 잔존 이미지 6셀은 Corruption 거절·원본 bytes 보존 |
| 하위 회귀 | 고정 사본의 DEV0 8·DEV1 8·DEV2 P0/K0 28 check PASS. default/all-features·보존 corpus·256 seeds×1,024 operations와 독립 build 포함 |
| 성능 정답·반례 | release 8개 검사 그룹의 필수 35개 named test PASS. 6개 K0 campaign 120회와 영속 audit/replay 40회의 정답·원시 이력 재검증 완료 |

추가 시험은 `artifacts/dev2-completion-20261019-*`의 보존 harness와 명시한 `cfg(test)` 모듈만 붙인 사본에서 실행했다. 원래 제품 파일 inventory와 injection 외 차이를 대조하고 원본·overlay·harness·binary·log 지문을 연결했다. D2의 대안은 새 제품 API를 노출하거나 이미 통과한 시험만 재실행하는 것이었으며, 기존 private parser와 실제 제품 경로를 직접 검사하는 시험 overlay를 선택했다. 의존성을 추가하지 않았다. JSON 장시간 결과는 구조 parser의 검증이며 전체 bundle verifier fuzz가 아니다. config는 원래 parser가 없는 bounded bytes/hash 계약을 확인했다. 손상 6셀은 명시적으로 동결한 잔존 이미지의 거절이며 실제 장치의 부분 영속 증거가 아니다. 시험 도구·parser·형식이 바뀌면 이 profile과 oracle 범위를 재검토한다.

### 성능 측정과 남은 인수

모든 correctness job과 build를 마친 뒤 CPU2에서 같은 입력·기준·10 paired runs를 사용했다. 각 원시 실행을 보존하고 bundle에서 결과를 다시 계산했다. 아래는 candidate/baseline 경과 시간 중앙 비율이며 한도 판정은 원본 정수 비율과 모든 필수 지표를 따른다.

| 비교 | 중앙 비율 | 정답·유효성 | 비용/회귀 한도 |
| --- | --- | --- | --- |
| K0 Off/Always audit | 1.1002 | PASS / VALID | PASS |
| K0 Off/Detailed audit | 1.2836 | PASS / VALID | FAIL |
| K0 Always bundle 재현 | 1.1011 | PASS / VALID | FAIL |
| 기존 exclusive replace 후보 탐색 | 0.1612 | PASS / VALID | PASS |
| 후보의 새 최종 확인 | 0.1620 | PASS / VALID | PASS |
| 후보의 bundle 재현 | 0.1609 | PASS / VALID | PASS |
| 영속 Off/Always audit | 0.9876 | PASS / VALID | FAIL — cycle p99 |
| 영속 bundle 재현 | 0.9973 | PASS / VALID | FAIL — CPU·cycle p99 |

PF12의 후보는 착수 시 존재하던 `VolatileStore::replace`의 exclusive root/table 경로다. 기준선은 같은 source에서 `substrate.rs`만 이전 구현으로 되돌렸고, 나머지 제품 파일은 동일하다. 측정 전 고정한 행 참조 복사량 baseline 8,388,608·candidate 0을 원시 frame 60개에서 확인했다. 최종 확인은 후보 1개·confirmation attempt 1회·새 20 run이며 탐색·확인·재현의 run ID를 재사용하지 않았다. 원인 counter와 end-to-end 개선의 이 범위는 확인됐지만, 기존 analyzer의 `cause_status`·`promotion`을 변경하거나 전체 성능 인수를 PASS로 바꾸지 않았다. 최초 K0 p99 실패와 이번 계측 한도 실패를 모두 유지한다.

### 원본 실패와 재검증

첫 live 하위 회귀는 제품 check 이후 동시 COMM 변경에 따른 source 지문 불일치로 FAIL했다. 원본을 보존하고 같은 동결 사본에서 하위 회귀를 다시 완료했다. 최초 K0 기준선 build는 live workspace와 공유 target을 사용해 선언한 baseline과 다른 embedded source의 실행파일을 만들었고 독립 checker가 비교를 INVALID/FAIL로 거절했다. 원래 20회 이력·binary·잘못된 receipt를 유지했다. 빈 전용 target의 실제 재빌드 후 실행 preflight에서 baseline 지문 `f8782b03…`와 복사량을 확인하고 새 탐색·새 최종 확인·재현을 수행했다. 이 provenance 수정을 기존 실패의 PASS 재표기로 사용하지 않는다. 초기 JSON harness 상수 경로 오류·early-reclaim mutant의 compile 실패도 별도 보존하고 mutant 검출 수에 합산하지 않았다.

공식 대상 환경·보호 CI·공식 배포와 DEV2 전체 인수는 완료하지 않았다. 문서 자동 검사는 push 훅에서 수행하며 이번 편집 중에는 실행하지 않았다.

최종 독립 검토자는 K0 120회·영속 40회의 원시 이력·정답·지문·계보와 정확한 유리수 통계를 대조해 오류 0을 확인했다. 영속 이력의 23,120 ACK 거래와 source/restore/archive bytes, PF12 60개 원시 frame의 복사량도 독립 기대값과 일치했다. 로컬 시험 실행·증거 정합성 검토는 PASS이며 `/root`가 반영을 확인했다. 비용 한도 실패와 전체 인수 INCOMPLETE는 유지한다.


<a id="dev2-instrumentation-fix-20261019"></a>
## DEV2-PF-FIX-20261019 — 계측 비용과 CPU 관측 수정

**D2 / 사용자 실패 해결 요청 / 로컬 실행 결과는 아래 범위에 한정.** `/root`가 구현·통합하고 `/root/dev2_test_review`가 원래 실패·계약·실제 diff·원시 증거를 독립 검토했다. [수정 receipt](../evidence/dev2-instrumentation-fix-xadb-20261019-r1.json)는 앞 절의 실패를 보존한 채 새 실행과 연결한다. 착수 source `b630facc2d6d63e864131390f82e7ee20ae5d7a3c8d60d5ce40a7d874d71ee31`, 최종 동결 source `e3bccf1a865b75deafa9eeac551f17591df406da1c41ccbdf9ec909c9c8e0f1a`에 귀속하며, 이후 COMM 등 동시 변경의 전체 검증으로 확대하지 않는다.

**2026-10-19 보존 상태 갱신:** disk full 뒤 사용자가 `xadb/artifacts`를 삭제했다고 확인했다. 아래 수치는 삭제 전 실제 실행의 역사 기록이며, 현재 원시 bundle의 재현 가능성을 뜻하지 않는다. 최종 e3 소스·ext4 kernel 원본·하위 회귀 원본은 외부 검증 사본에 남아 있고, 하위 자료 11,280개 파일은 해시를 대조한 277MB 압축본으로 추가 보존했다. 삭제된 성능 원시 자료와 보조 시험은 새 실행으로 다시 연결해야 한다. 30분 전에 중단된 6개 V3는 미완료로 유지한다. 구체 경로와 해시는 위 receipt의 `artifact_availability_after_cleanup`을 따른다.

### 원인과 구현 선택

- K0 Detailed의 가득 찬 이벤트 큐에서 매번 mutex를 얻고 별도 sequence를 증가시키던 비용을 줄였다. mutex 소유자만 full 힌트와 emitted를 갱신하고 drop은 별도 포화 counter로 집계한다. 실제 큐 길이로 용량을 재검사하며 accepted sequence는 emitted+dropped+1의 검사된 합이다. drop의 gap·비차단 손실·정확한 완료시도 회계·공개 포화 표시는 유지한다. stale full은 공개된 drop만 허용하며 제품 publication과 분리된다.
- 영속 관측은 0-byte/item 증가와 이미 충분한 max에 불필요한 원자 쓰기를 생략한다. 실제 WAL sync, clock sample, histogram과 실패 집계는 유지한다. TLS 집계·unsafe·새 큐 구조보다 작은 변경을 선택했다. drain 빈도·경합·공개 metric 의미가 달라지면 복잡성의 비용을 재검토한다.
- 별도 정확성 결함으로 CPU reader를 고쳤다. `/proc/self/schedstat`의 캐시된 main-task 합 대신 안전한 `CLOCK_THREAD_CPUTIME_ID`로 호출 thread의 구간 경계를 읽는다. 기존 nix 0.30.1의 time feature만 추가했고 package/version/lockfile·전이 의존성·공개 타입은 바꾸지 않았다. 음수·나노초 범위·u64 overflow는 unavailable로 보존한다. 구간·catalog·bundle의 CPU 정의도 함께 수정했다. 이 절 이후 새 실행에는 앞 절의 schedstat 설명을 적용하지 않는다.

코드 소유 범위는 diagnostics.rs, durable/observation.rs, platform/Cargo.toml과 performance.rs, controller의 performance.rs와 performance/bundle.rs, check_dev2_performance.py의 필수 시험 목록이다. 기존 crates/scripts fingerprint·clean export 범위를 사용하며 공개 저장 형식·SQL·격리·내구성 정책은 바꾸지 않았다. 사내 새 fixture와 current-build recovery를 사용했다.

### CPU 반례와 조사 범위

실제 host는 Linux `7.0.0-38-generic`이다. CPU2에서 15ms CPU 구간을 40회 대조한 r2 probe의 schedstat delta는 POSIX thread clock보다 최대 1,001,910ns 작았다. 다른 순서·affinity의 r1 20회도 원문 보존한다. 이는 해당 측정의 관측 오차이며 과거 모든 latency 실패의 원인을 증명하지 않는다. parent가 기다리는 동안 child thread가 20ms CPU를 소비하는 새 반례는 정확한 이전 구현에서 실행 후 assertion FAIL(101), 수정 구현에서 PASS했다.

원문 대조는 upstream Linux v7.0의 [proc_pid_schedstat](https://raw.githubusercontent.com/torvalds/linux/v7.0/fs/proc/base.c), [CPU clock sample](https://raw.githubusercontent.com/torvalds/linux/v7.0/kernel/time/posix-cpu-timers.c), [task_sched_runtime](https://raw.githubusercontent.com/torvalds/linux/v7.0/kernel/sched/core.c)을 읽었다. upstream tag와 배포판의 실제 kernel binary를 동일 revision이라고 주장하지 않는다. raw libc의 workspace unsafe를 피하고 기존 safe platform adapter를 선택했으며, 지원 platform/clock 가용성이 달라지면 재검토한다.

### 탐색·환경 실패의 보존

collector 탐색은 사전 한도 3개였다. candidate1은 full 힌트와 no-op 원자 쓰기 제거, candidate2는 emitted/drop 분리, candidate3는 독립 검토에서 발견한 두 counter의 마지막 동시 overflow 표시 누락을 수정하고 hot path를 분리했다. 여섯 audit의 결과와 비용 한도 실패, 후보2의 검토 반례를 보존한다. 이후 CPU observer 수정은 별도 정확성 결함의 수리이며, 이전 CPU 수치를 소급해 PASS로 바꾸지 않는다. CPU를 고친 공통 baseline에서 두 collector 파일만 이전 구현으로 되돌려 비용 효과를 별도로 비교한다.

초기 후보2 native 실행은 긴 사본 경로가 Linux local socket 경로 상한을 넘어 실패했다. 짧은 /tmp 사본의 kernel 결과는 tmpfs에 한정한다. 이를 확인한 뒤 같은 동결 소스를 /home/et16의 짧은 root로 복사하고 TMPDIR까지 ext4로 지정해 kernel·native 3회 전체를 별도로 검증했다. tmpfs의 PASS를 disk qualification으로 대체하지 않는다. 각 원본과 filesystem 확인을 보존한다. 최종 하위 회귀 첫 회는 user disk quota errno122로 보고서 저장도 실패했다. 원본 log/artifact를 유지했고 완료된 이번 작업의 재생성 가능한 target 캐시만 삭제했다. 완료된 증거의 동일 worker 바이트는 경로·권한·해시를 보존하는 hardlink로 중복 제거했다. 동결 소스·명령을 바꾸지 않고 하위 회귀를 새 directory에서 재실행했다. 이 환경 복구는 성능 비교 재시도가 아니다.

최종 성능은 자체 correctness/build 작업과 CPU2의 sibling CPU18에서 진행 중이던 다른 작업의 soak가 끝난 뒤 실행했다. 다른 공유 host 작업은 원본 environment에 기록한다. 전용 무부하 장비라는 주장은 하지 않는다. workload·seed 20261018·5AB/5BA의 10쌍·wall/CPU/external 1.15·sample p99 1.25·RSS 4MiB 기준은 그대로다. 최종 확인 1회와 별도 bundle replay 1회를 동결했으며 유리한 결과를 고르는 반복은 하지 않는다.

### 새 source의 실제 실행

| 범위 | 결과 |
| --- | --- |
| ext4 kernel/current-build recovery | 18개 검사 그룹 PASS. engine 29·transaction 7·cache-loss 15·thread 64 seeds·native 33셀×3회 strict 99 ACCEPTED·독립 oracle 101·runner 반례 8 |
| DEV0·DEV1·DEV2 P0/K0 하위 회귀 | 각각 8·8·28 check PASS. 실제 native/catalog inventory와 실행 전후 같은 source hash 확인 |
| 성능 정답·반례 | release 8개 그룹의 필수 36개 named test PASS. CPU old-reader 반례 FAIL과 수정 PASS를 별도로 연결 |

다음 중앙 비율은 candidate/baseline 경과 시간이며, 비용 한도는 모든 pair의 원래 정수 비율과 필수 지표로 판단한다. 같은 CPU observer를 사용하는 두 collector의 비교와 Off 대비 계측 비용을 구분한다.

| 비교 | 중앙 비율 | 유효성 / 정답 | 비용 한도 |
| --- | --- | --- | --- |
| K0 Off/Always audit | 1.0919 | VALID / PASS | PASS |
| K0 Off/Detailed audit | 1.1260 | VALID / PASS | PASS |
| K0 Always bundle 재현 | 1.0910 | VALID / PASS | PASS |
| K0 Detailed bundle 재현 | 1.1276 | VALID / PASS | FAIL |
| collector 비용 탐색 | 0.8875 | VALID / PASS | PASS |
| collector 새 최종 확인 | 0.8854 | VALID / PASS | PASS |
| collector bundle 재현 | 0.8877 | VALID / PASS | PASS |
| 영속 Off/Always audit | 0.9995 | VALID / PASS | FAIL |
| 영속 bundle 재현 | 0.9974 | VALID / PASS | FAIL |

최종 campaign 실행은 PASS, 사전 동결한 로컬 성능 인수는 INCOMPLETE다. 자동 analyzer의 cause_status/promotion은 원문 그대로 보존하며 stage 완료로 승격하지 않는다.

### 남은 한도 실패와 해석 한계

- K0 Detailed replay의 pair 4(0-based, BA): wall 17,079,804/14,454,060ns=1.181661, 외부 구간 23,264,909/20,129,430ns=1.155766으로 1.15 한도를 초과했다. 같은 pair의 CPU 16,265,331/14,380,860ns와 p99는 PASS다. wall−CPU 차이의 증가가 non-CPU 대기를 시사하지만 scheduler trace가 없어 공유 host를 확정 원인으로 삼지 않는다.
- 영속 audit pair 7: cycle p99 12,317,483/5,573,277ns=2.210097로 1.25 한도를 초과했다.
- 영속 replay pair 3: cycle p99 12,297,716/5,448,021ns=2.257281. pair 6: CPU 8,047,409/6,967,256ns=1.155033로 각각 한도를 초과했다.

계측 Off의 cycle p99도 audit에서 5.42–12.27ms, replay에서 5.45–12.40ms로 달랐다. 이 변동과 paired 결과만으로 계측 코드 또는 공유 환경에 원인을 확정하지 않는다. collector 최종 확인의 경과 시간 중앙 비율 0.885365(약11.46% 감소)는 같은 CPU observer와 workload에서 얻은 별도 개선 증거다. 이 국소 개선과 계측 전체 인수의 FAIL을 구분한다. 추가 코드 결함으로 단정하거나 한도를 완화하지 않았으며, 재시도해서 유리한 결과를 고르는 대신 유휴 Linux/ext4 환경의 원인 분리 측정을 남긴다. 사용 가능한 전용 host/유휴 시간대는 사용자에게 확인 중이다.

최종 독립 검토자는 7개 파일의 구현·반례·불변식과 ext4 99셀, 하위 28개 검사/DEV0·DEV1 각8개, K0 140회·영속40회의 원시 값·정답·지문·정확 유리수 통계·계보를 대조했다. 영속 ACK 거래23,120건의 TxnId/q·전체 afterimage·source/archive/restore 이미지가 일치하고, run180개/campaign9개 ID는 모두 유일하다. 보존 사본11,076개·4,004개의 원본/사본 bytes·hash·mode·전체 inventory와 추가30,622개 해시 참조도 일치했다. 판정은 구현·실행/보존 증거 GO / 전체 성능 인수 INCOMPLETE이며 `/root`가 반영을 확인했다. 남은 세 campaign 실패와 원인 해석의 한계를 유지한다.

기존 source의 9개 30분 V3와 P0 전수를 새 source에 소급하지 않는다. 새 source의 full V3·공식 대상 환경·보호 CI·배포는 NOT_RUN이며 DEV2/B2/D0 전체 인수는 INCOMPLETE다. 문서 자동 검사는 push 훅의 책임으로 남겼고 편집 중 실행하지 않았다. commit/push는 하지 않았다.


<a id="dev2-resumed-tests-20261019"></a>
## DEV2 새 시험과 사용자 요청 일시 중지

사용자가 disk full 뒤 `xadb/artifacts`를 삭제했다고 확인했다. 삭제된 원시 자료의 과거 PASS를 현재 검증으로 사용하지 않고, 남아 있는 e3 소스와 kernel·하위 회귀 원본에 새 실행을 연결했다. [새 실행·일시 중지 receipt](../evidence/dev2-resumed-tests-xadb-20261019-r1.json)에 실제 경로·해시·독립 검토와 34개 필수 ID 및 D0 경계 7개의 매핑을 기록했다. 전체 작업 트리의 동시 변경을 검증했다는 뜻은 아니다.

| 실행 범위 | 실제 결과 |
| --- | --- |
| e3 30분 V3 | 11개 종료, 11개 target PASS. 기존 kernel/native 6개, JSON 3개, 영속 성능 요청·응답 codec 2개 |
| 격리·publication | 3개 시험 각각 새 fixture 3회; write skew·empty scan phantom·두 table publication 확인, 의미 변이 2개 검출 |
| 복구 보완 | read 경계 63개×전후×3회=378개 실제 SIGKILL; 756개 crash/reopen 이미지 독립 해독. unsynced 생존 모델 12셀, retire/fence 3회와 이미지 9개 |
| 필수 성능 정답 | 36개 named test PASS; PF02/PF07 보완 4개 PASS. CPU old-reader 반례 컴파일 성공 뒤 지정 assertion 실패, 수정 구현 PASS |
| kernel 의미 변이 | ACK 이전 sync 누락·이전 prefix sync 누락·조기 회수 3개를 지정 assertion에서 검출 |
| P0·하위·kernel 원본 | e3 P0 전수9,604,608건, 하위28개/DEV0·DEV1각8개, ext4 native99셀의 남아 있는 원본을 연결 |

6개 V3 시험 자체는 모두 1,800초 이상 정상 종료했다. 이후 집계기가 동결 소스 아래 생성된 Python bytecode 한 파일을 지문에 포함하면서 마지막 assertion에서 실패했다. 원본 204개 파일과 시험 바이너리의 바이트는 바뀌지 않았다. 생성 캐시를 별도 경로로 옮겨 보존하자 e3 지문이 복원됐고, 독립 검토자가 해당 캐시를 가상 추가해 오류 당시 지문을 재현했다. target PASS와 원래 집계기 FAIL을 구분하며 집계기 실행을 PASS로 바꾸지 않는다.

보조 시험의 첫 torn-image export는 출력 상위 폴더가 없어 실패했고, 별도 CPU 반례의 첫 준비는 missing_docs로 컴파일 실패했다. 이 준비 실패를 보존했다. 이후 실제 새 실행 결과만 채택했으며 컴파일 실패를 의미 변이 검출로 세지 않는다. 복구 모델과 test-only preemption은 실제 장치 전원 장애나 물리 sector tear를 증명하지 않는다.

실패 원인 조사용 traced 영속 실행 2개는 독립 정답 검사를 통과했다. 해당 부하·trace 아래 측정 cycle wall의99.2216%·99.5747%가 fsync 호출 안에서 소요됐다. 이 수치로 장치 자체 또는 삭제된 이전 실패 pair의 원인을 확정하지 않으며 성능 인수에 사용하지 않는다.

사용자의 “지금 실행중인 테스트가 끝나면 일단 중지” 요청에 따라 위 실행 결과를 보존한 뒤 일시 중지했다. 새 성능9개 campaign은 준비만 했고 **NOT_RUN**이다. 이전 K0 Detailed replay와 영속 audit/replay의 비용 한도 FAIL, PF12/PF13 및 DEV2/B2/D0 전체 인수 **INCOMPLETE**를 유지한다. `/root/dev2_test_review`의 독립 검토는 이번 범위의 정확성 증거에 한정한다. 재개할 때 보존 지문·선행 결과·준비된 집계기의 연결을 확인하고 사전 고정한 유휴 조건을 만족한 뒤 성능 실행을 시작해야 한다.

원본·새 로그는 외부 경로에 보존했다. 중복 worker는 경로·해시·권한을 유지한 hardlink로 정리했고, 하위 원본11,280개 파일은 검증한277MB 압축본도 남겼다. 공식 환경·보호 CI·배포 자격은 이번 로컬 증거의 범위가 아니다. 편집 중 문서 자동 검사는 실행하지 않았으며 commit/push하지 않았다.

<a id="dev2-performance-followup-20261020"></a>
## DEV2 성능 실행 재개와 남은 인수

**2026-10-20 · D2 / 고정 소스의 로컬 실행 완료, DEV2 인수 INCOMPLETE.** 사용자 재개 요청에 따라 준비된 성능 9개 campaign을 모두 실행했다. [후속 실행 receipt](../evidence/dev2-performance-followup-xadb-20261020-r1.json)는 실제 180개 run, 보존 bundle, 독립 검토와 원래 실패를 연결한다. 제품 코드는 변경하지 않았다. e3 동결 소스와 현재 작업 트리의 공통 DEV2 파일은 같지만, COMM 성능 파일 3개 추가와 동결 Python 캐시 6개 부재로 전체 지문은 다르다. 이를 전체 현재 소스 검증으로 확대하지 않는다.

실행 전에 실제 ext4 UUID·장치 serial과 CPU2/18을 확인했다. 각 campaign 전 10초간 두 CPU busy와 해당 장치 I/O busy가 각각 5% 이하이고 소유 DEV2/COMM 작업이 없을 때만 시작했다. 이 경계 관측은 측정 내내 전용 장비였음을 증명하지 않는다. workload·seed·10쌍(5AB/5BA)과 wall/CPU/external 1.15·표본 p99 1.25·RSS 4MiB 한도는 유지했다. 공유 host 설정을 변경하지 않았다.

| 비교 | 경과 시간 중앙 비율 | 유효성 / 정답 | 비용 한도 |
| --- | --- | --- | --- |
| K0 Off/Always audit | 1.0903 | VALID / PASS | FAIL |
| K0 Off/Detailed audit | 1.1243 | VALID / PASS | FAIL |
| K0 Always bundle 재현 | 1.0934 | VALID / PASS | PASS |
| K0 Detailed bundle 재현 | 1.1258 | VALID / PASS | PASS |
| collector 비용 탐색 | 0.8791 | VALID / PASS | PASS |
| collector 새 최종 확인 | 0.8759 | VALID / PASS | PASS |
| collector bundle 재현 | 0.8742 | VALID / PASS | PASS |
| 영속 Off/Always audit | 0.9976 | VALID / PASS | FAIL |
| 영속 bundle 재현 | 1.0019 | VALID / PASS | FAIL |

위 비율은 표시용 근삿값이다. 원시 정수 비율로 모든 pair를 판정했고, K0 Always pair5와 Detailed pair0/2/6, 영속 audit의 p99 pair0/1/4, 영속 replay의 CPU pair1/9와 p99 pair2/3에서 한도를 초과했다. 재현이 성공해도 최초 실패를 지우지 않는다.

집계기는 첫 4개 campaign을 마친 뒤 기존 준비 파일의 `exploratory` 값이 실제 recipe enum과 달라 종료했다. worker 실행 전 입력 해석 실패였으며 원본 실패와 80개 run을 보존했다. 별도 continuation에서 `exploration` 및 영속 결과 경로 처리를 고치고 남은 5개만 실행했다. 이미 측정한 campaign은 다시 실행하지 않았다. CPU·장치 유휴 조건의 경계/부정 시험 8개, 실제 controller의 recipe 해석과 worker identity 거절, 실행 전후 source·binary·runner 지문 확인을 거쳤다. 원래 6개 V3 집계기 실패와 과거 삭제 자료의 제한도 유지한다.

### 개별 인수와 원인 분리

`/root/dev2_closure_review`는 K0 원시 160개 실행(본 실행 140개와 대조 20개)의 LCG 읽기값·최종 행·old snapshot을 별도로 계산했다. 영속 본 실행 40개와 대조 20개의 cycle p99·비용 비율도 원시 값에서 재계산했다. 보존된 9개 bundle의 verifier 결과, 정확한 유리수 통계와 실패 pair가 일치했다. `/root`는 다음 범위의 개별 종료를 채택했다.

- **PF12 완료:** 포화 K0 collector의 국소 개선. 최종 확인 약12.41%, 별도 재현 약12.58% 경과 시간 감소와 비용 한도 PASS. 각 측정의 진단 시도196,608건=보존256건+공개 drop196,352건을 확인했다. 단일 thread·측정 중 drain 없음 조건에서 full-queue 분기가 drop 경로의 mutex·sequence RMW를 생략한다는 소스 경로와 관측을 연결했다. 이는 하드웨어 lock 횟수 실측이나 개별 하위 변경의 분리 효과가 아니다. 비교 소스의 영속 관측 변경은 이 K0 경로에서 호출되지 않으며 영속 개선을 주장하지 않는다. 같은 e3 소스의 기존 독립 정답·포화/동시 drain·복구·하위 회귀·11개 V3 증거를 함께 연결했다.
- **PF13 완료:** 현재 capability의 보존 bundle 재현 기능. 별도 cwd와 새 process/run ID로 baseline/candidate를 실행하고 정답·계보·실패를 재산출했다. 비용 한도 FAIL을 그대로 재현·보존한 결과도 포함한다.
- **PF04 미완료:** 계측 비용 한도. 실제 측정과 실패 공개는 완료했지만, 네 campaign의 FAIL을 성능 인수 PASS로 바꾸지 않는다. 자동 analyzer의 원래 cause_status/promotion도 변경하지 않는다.

원인 분리용으로 동일 e3 binary·동일 Off 설정의 대조를 K0와 영속 경로에서 각각 고정10쌍 실행했다. 정답은 모두 PASS다. K0는 비용 한도 PASS였고, 영속은 양쪽 모두 관측을 꺼도 pair8 p99가12,389,622/5,480,140ns=약2.26082배로 FAIL했다. 반대 방향 고지연도 있었다. 관측 On이 없어도 tail 변동이 생긴다는 증거이며, 장치·scheduler 원인 확정이나 기존 실패 취소 근거는 아니다. 대조 실험은 성능 인수에 사용하지 않는다.

추가 K0 진단20run은 소유한 UID1000 worker PID에만 PMU를 붙여 수행했다. Off/Detailed 측정 구간 중앙값은14.584/16.415ms, 전체 worker의 instructions 중앙값은118.761/127.412백만이었다. 전체 worker cycles/task-clock은 양쪽 약5.200cycle/ns였고 context switch는 중앙0·최대1이었다. PMU 범위에는 setup·warmup·출력도 포함되므로 측정 loop만의 원인 지표로 쓰지 않는다. 이 진단에서 이전 audit의 경과·CPU 시간 outlier는 재현되지 않았지만, pair5 표본 p99는361/251ns=약1.43825배로 높았다. 이 진단은 인수용 비교가 아니며 p99 관측도 보존한다. 영속 동일Off 실패 pair의 p99 cycle은 두 쪽 모두99.96% 이상 commit 구간에 포함되지만, 그 내부의 sync·kernel·scheduler 원인은 아직 분리하지 못했다. 어느 진단도 기존 비용 한도 실패를 대체하지 않는다.

남은 작업은 **PF04의 원인 구분과 해결을 검증하고 최종 DEV2/B2/D0 종료 증거를 반영하는 것**이다. 11개 V3나 복구 전수를 미실행으로 되돌리지 않는다. 공식 Ubuntu24.04 환경·보호 CI·공식 배포는 로컬 종료와 별도인 NOT_RUN 상태다. 문서 자동 검사는 push 훅에서만 수행한다.

<a id="dev2-fsync-diagnostic-20261020"></a>
## DEV2 영속 fsync 대기 구간 진단

**2026-10-20 · D2 / 진단 실행 완료, PF04·DEV2 인수 미완료.** [진단 receipt](../evidence/dev2-fsync-diagnostic-xadb-20261020-r1.json)는 실행 전에 고정하고 독립 검토한 Off/On/On/Off 4개 실행과 원시 추적을 연결한다. 같은 e3 release worker, seed20261018, warmup64·측정512를 사용했다. 제품 코드·공유 host 설정은 변경하지 않았다. 두 수집기의 enable ACK 뒤에만 worker 입력을 전달했고, worker의 fsync/openat/close·CPU 전환과 해당 PID를 깨우는 사건을 같은 CLOCK_MONOTONIC으로 관측했다. wake 수집기의 부수적인 perf metadata에는 다른 process 정보가 포함될 수 있으며, 모든 원시 byte를 worker 전용이라고 주장하지 않는다.

| 고정 순서 | 측정 경과(ms) | cycle p99(ms) | 정답·추적 연결 |
| --- | --- | --- | --- |
| Off | 2,805.190 | 12.656 | PASS |
| On | 2,733.538 | 5.861 | PASS |
| On | 2,813.969 | 6.590 | PASS |
| Off | 2,799.417 | 6.299 | PASS |

기존 독립 checker가 각 실행의 578개 실제 ACK·행 값·old reader·WAL·backup·restore와 측정 구간의 file_sync512회·q66→578을 확인했다. 열린 FD의 수명과 `/proc` 경로 증거를 이용하여 최초 source WAL의 fsync579회에서 header·DDL·fixture·warmup67회를 제외한512회를 각 cycle에 연결했다. 2,048개 모두 `fsync 시간 ≤ commit 시간 ≤ cycle 시간`을 만족했다. 단순히 마지막512회를 선택하지 않았다.

이 실행들에서는 전체 측정 cycle 시간의 **97.352–98.538%가 fsync 중 worker 깨움 이전의 off-CPU 구간**이었다. 깨움 완료부터 CPU 재개까지의 runnable 구간은 전체의0.254–0.535%였다. 두 번째 On 실행의 가장 느린6개 cycle에서는 runnable 구간이3.475%였으며, 첫 번째 On 실행의8,356ns는 wake 사건이 모호해 미분류로 보존했다. 이 구분은 해당 추적에서 CPU 재개 대기가 전체 지연의 주된 비중이 아니었음을 보여 준다. 깨움 이전 대기는 filesystem·journal·장치 작업 등을 포함할 수 있어 장치 원인으로 단정하지 않는다. 과거 실패 pair의 원인을 소급 확정하지도 않는다.

사전 고정한 거친 영향 확인 조건인 이전 동일Off 측정 최댓값2,778,464,859ns의1.25배 이하는4개 모두 충족했다. 서로 다른 시점의 비교이므로 순수한 추적 overhead 추정이나 인수 비용 한도 PASS가 아니다. 16개 analyzer 사전 사례는 누락된 switch-in·잘못된 clockid·손실·외부 PID 등 검출과 모호한 wake의 미분류 보존을 확인했다. 두 수집기를 붙인 직후의 의도한 실패에서 입력 미전달·process 정리도 확인했다. 본 실행 종료 뒤 worker·수집기 잔류는 없었다.

기존 필수34개 ID 중31개 scoped 검토와 PF12/PF13 완료에 더해, 남은 항목은 **PF04의 네 campaign 비용 한도 실패 및 최종 DEV2/B2/D0 종료**다. 이 숫자는 증거 범위의 개수이며 남은 공수의 비율이 아니다. 이번 진단은 특정 제품 결함을 확정하지 않았으므로 임의 코드 수정이나 추가 인수 반복으로 이어가지 않는다. 측정 환경의 사용 조건 또는 새로 검토한 구분 가설을 확보한 뒤 다음 실행을 정하고, 현재까지의 실패와 원래 예산을 유지한다.

<a id="dev2-pf04-hypothesis-review-20261020"></a>
## DEV2 PF04 후속 가설 검토

**2026-10-20 · D2 / 읽기 전용 조사·독립 검토 완료, PF04·DEV2 인수 미완료.** [가설 검토 receipt](../evidence/dev2-pf04-hypothesis-review-xadb-20261020-r1.json)에 두 후보 설명과 채택하지 않은 이유를 남겼다. 새 제품 실행, 제품 코드 수정, 공유 host 설정 변경은 없다.

K0의 관측 활성 snapshot은 기본 Counters 외에 collector Arc를 별도로 소유한다. 현재 측정 loop에서 추가 clone/drop 65,536쌍이 실행되는 경로를 소스와 동결 binary에서 확인했지만, 그 지점의 시간 기여는 분리하지 못했다. shared Counters에 통합하면 transaction·영속 복구의 크기/할당량에도 영향을 주며, 별도 wrapper는 소유권·할당·간접 참조를 추가한다. snapshot/cursor의 독립 수명과 Off 동작도 보존해야 한다. 기존 최악 CPU 실패를 같은 baseline에서 해소하려면 Always 약8.26%, Detailed 약11.46%의 전체 candidate 시간 감소가 필요하며, 이는 요구량이지 이 후보의 예상 효과가 아니다. 이미 소진한 후보 3개 탐색 예산을 늘릴 근거로 삼지 않았다.

영속 경로는 설치된 Ubuntu kernel package `linux 7.0.0-38.38`과 맞는 공식 source archive·patch를 대조했다. 사용한 원본·패치의 SHA-256과 관련 9개 파일, tag `Ubuntu-7.0.0-38.38`의 commit `740b32353f43e9b00d0350d6fa7b29a6667deabd`를 receipt에 기록했다. 분석 입력은 checksum으로 연결한 archive 재구성이며, 설치 binary의 재현 build나 실제 kernel 호출 stack을 확인한 것은 아니다.

현재 파일시스템에는 fast_commit 기능이 없고 WAL inode에 sync flag가 없다. 이 조건에서 소스가 가리키는 일반 fsync 경로는 file data 대기 후 journal transaction 완료를 기다린다. `max_batch_time`에 따른 지연은 `jbd2_journal_stop`에서 동기 handle·이전과 다른 writer·미완료 batching 시간 등의 조건이 충족될 때 발생한다. 모든 fsync에 무조건 더하는 지연이 아니다. 현재 mount·기능 관측은 추적 이후의 값이며 당시의 함수 경로를 확정하지 않는다.

기존 2,048개 cycle을 깨운 실행 문맥의 PID별로 다시 집계했다. journal thread PID470이 깨우기 전 대기가 각 실행의 cycle 시간 합계 중96.843–98.085%였고 해당 구간 중앙값은5.213–5.251ms였다. PID0 문맥이 깨우기 전 대기의 중앙값은23.284–30.057µs였다. PID0은 idle/interrupt 문맥이며 timer의 증거가 아니다. PID470은 해당 filesystem journal thread로 확인했지만, 깨운 PID만으로 특정 journal transaction·wake 지점·장치 I/O 원인까지 식별하지 못한다. 따라서 **`max_batch_time=0`으로 약5ms 대기가 해결된다는 설명은 현재 근거로 지지되지 않는다.** 다른 writer나 겹친 작업에 batching이 영향을 줄 가능성까지 부정한 것은 아니다.

`/root/dev2_closure_review`는 원본 archive와 Ubuntu patch의 해당 11개 hunk를 독립 재구성하여 최종 9개 파일을 확인하고, 원시 trace에서 2,048개 cycle의 구간별 개수·합·중앙값·범위·미분류를 다시 계산했다. 차이와 차단 지적은 없었고 `/root`가 한계를 포함한 결론을 반영했다. 남은 PF04 네 campaign의 FAIL은 그대로다. 다음 실행은 측정 환경의 사용 조건을 확인하거나 journal 완료 대기를 더 구분할 관측을 사전에 정하고 별도 검토한 뒤 결정한다. 공유 root filesystem remount나 유리한 결과를 얻기 위한 반복 측정을 이 조사로 정당화하지 않는다.

<a id="dev2-journal-diagnostic-20261020"></a>
## DEV2 실제 journal transaction 대기 진단

**2026-10-20 · D2 / 고정 Off 1회 실행, PF04·DEV2 인수 미완료.** [진단 receipt](../evidence/dev2-journal-diagnostic-xadb-20261020-r1.json)는 이전 깨움 PID 관측에서 구분하지 못했던 실제 대기 함수·journal transaction ID와 단계를 연결한다. 동일 e3 release worker, seed20261018, warmup64·측정512를 사용했고, 실행 전에 소스·실행기·분석기·계획을 고정해 별도 검토했다. 제품 코드와 공유 host 설정은 변경하지 않았다.

**추적 영향·환경의 거친 확인 조건은 FAIL이다.** 측정 경과 시간은4,173,318,806ns로, 사전 고정한 이전 동일Off 최댓값2,778,464,859ns의1.25배를 넘었다. 다른 시점의 비교라 순수 추적 overhead로 분리할 수 없다. 아래 시간 비율은 이번 계측 실행 안에서만 유효하며, 계측 없는 실행이나 과거 실패 pair의 원인을 설명하는 비율로 사용하지 않는다. 재시도는 하지 않았다.

기존 독립 checker의 ACK·행 값·old reader·WAL·backup·restore 정답 검사는 PASS다. 최초 source WAL의 FD 수명과 경로로 측정 fsync512회를 연결했고, 각각 실제 `jbd2_log_wait_commit` 호출과 서로 다른 TID512개를 확인했다. 각 TID의 start/locking/flushing/logging/end marker가 모두 존재하며 미분류 phase는 없다. cycle 시간 합계4,172,604,214ns 중 journal wait가4,095,552,972ns(98.153%), logging→end와 겹친 wait가4,041,633,116ns(96.861%)였다. logging→end는 여러 I/O와 후처리를 포함하는 넓은 구간이며 장치 기록 시간으로 읽지 않는다.

같은 journal PID의 `io_schedule` 함수 경과 구간이 worker wait와 겹친 시간은4,016,216,287ns(전체 cycle의96.252%)였다. kernel stack에는 `__wait_on_buffer`와 `jbd2_journal_commit_transaction`이 나타났다. 이 경과 시간에는 CPU 재개 등도 포함되며, 시간상 겹침만으로 각 I/O가 그 worker TID의 소유라고 확정하지 않는다. 장치 service time, 특정 commit record 대기 지점, kernel 결함 또는 기존 PF04 실패 원인을 확정한 결과가 아니다.

BPF hook은 전역에 연결되며 앞단 PID/dev 조건으로 실제 worker의 wait 함수, 해당 filesystem dev의 phase, 해당 journal PID의 `io_schedule`에 해당하는 기록만 해석했다. perf와 BPF 시계는 모두 명시적 CLOCK_MONOTONIC이다. 12개 probe의 전체 연결과 실제 수집기 생존을 확인한 뒤에만 입력을 보냈다. 종료 시 실제 BPF PID에 producer 중지 신호를 보내 acknowledgment를 받고1초간 출력 배출 후 정지했다. 커널 집계와 출력된 WAIT_IN606·WAIT_OUT606·PHASE3,035·IO_IN1,214·IO_OUT1,214개가 일치했다. 이 전체 수집 개수와 측정512개의 분모는 구분한다.

수집 로그는 소유한 tmpfs 공간에 기록하여 대상 ext4 journal에 로그 append를 추가하지 않았다. 제품 fixture는 ext4에 유지했고, 수집기 종료 뒤 byte/hash가 같은 사본을 보존하고 임시 공간을 제거했다. 실제 수집기에 독립 watchdog180초·강제 종료 유예5초를 두었으며, 부모 종료 helper에서는 짧은6초·유예2초 설정의 실제 root BPF/perf가 부모 종료 뒤 약5.985초에 모두 끝났다. 17개 분석 사전 사례와 정상/입력 전 실패 helper를 확인했다. 초기 header 오판·종료 시 출력 누락·timeout wrapper에 대한 중복 신호·helper 보존 권한 실패도 원본대로 남겼다. 이번 결과는 제품 수정이나 추가 인수 반복의 근거가 아니며, 남은 PF04 네 campaign과 최종 DEV2/B2/D0 인수 상태는 유지한다.

`/root/dev2_closure_review`는 원시 기록에서 512개 cycle/TID/단계/I/O 겹침의 모든 행과 합계를 독립 재계산하고, 보존된 perf binary를 다시 해독하여 export와 일치함을 확인했다. 보존 byte에 독립 정답 checker를 적용하고 receipt에 연결된 파일44개·tmpfs 사본8개·고정 입력17개의 해시, 소유 PID/PGID·BPF link 정리도 확인했다. 차단 지적은0개이며 `/root`는 **추적 자료 한정 승인과 확인 조건 FAIL**을 함께 반영했다. 추적 연결 완료를 PF04 또는 DEV2 완료로 승격하지 않는다.

<a id="dev2-journal-binary-mapping-20261020"></a>
## DEV2 journal 대기 위치의 binary 대응

**2026-10-20 · D2 / 정적 분석, 추가 제품·수집 실행0회.** [대응 receipt](../evidence/dev2-journal-binary-mapping-xadb-20261020-r1.json)는 앞선 추적의 두 `jbd2_journal_commit_transaction` return offset을 설치 kernel binary와 연결한다. `/boot`의 image를 읽기 전용 복사하여 배포된 추출기로 ELF를 꺼냈고, GNU build ID `338e542e384c46228f7a378babc0f9bb2b6a5997`이 실행 kernel notes와 일치했다. image·System.map은 설치 package의 MD5 항목과 일치하고, 여섯 함수의 map/runtime 주소 차이는 동일했다. 현재 boot 시작은 보존된 추적 시작보다 이전이다. build ID는 이번에 확인했으며 원래 수집 때 직접 저장한 값은 아니다. 재현 build나 배포 서명 검증을 주장하지 않는다.

ELF에는 symbol table·DWARF가 없어 줄 번호를 직접 조회하지 않았다. 함수 시작에서 디코드한 기계어의 호출 대상·인자·분기·해제 흐름을 이미 검토한 동일 버전 source와 대조했다. 처음 임의 byte 위치에서 시작한 부분 disassembly는 명령 경계가 어긋나므로 증거에서 제외했다.

| 기존 stack return offset | 대응하는 대기 | 구분 근거 |
| --- | --- | --- |
| +3587 | `io_bufs`의 임시 metadata buffer | `__wait_on_buffer` 뒤 `cond_resched`, buffer 해제와 shadow list 재배치가 이어짐 |
| +4778 | commit record buffer | `journal_submit_commit_record`의 출력 `cbh`를 읽어 dirty 해제·대기·uptodate 검사·참조 해제하는 inline 경로 |

이 대응은 기존 두 stack에 귀속된 겹침15,661,457ns와4,000,554,830ns의 **코드 위치**를 구분한다. 새로운 시간 측정은 아니다. `io_schedule` 경과 시간의 장치 service time 해석, 각 I/O의 worker TID 소유, 과거 실패 pair 원인 또는 kernel 결함은 여전히 확정하지 않는다. 앞선 추적 영향·환경 확인 FAIL과 PF04 네 비용 한도 FAIL도 유지한다. barrier·sync·mount 설정 변경이나 추가 인수 반복을 이 정적 분석으로 정당화하지 않는다.

`/root/dev2_closure_review`는 압축 frame 경계를 독립적으로 찾아 동일 ELF byte를 재구성하고, 함수 시작에서 다시 디코드해 보존본과 일치함을 확인했다. 두 호출의 실제 상대 주소와 대상, 인자·buffer 수명 흐름, artifact20개와 source/추적 입력2개의 연결을 확인했으며 차단 지적은0개다. `/root`는 정적 코드 대응에 한정하여 반영했다. 다음 비교 실행에는 사용할 host·경로, 다른 writer와 journal을 공유하는지, CPU·스토리지 동시 작업을 통제할 수 있는 시간대가 필요하다.

<a id="dev2-pf04-analysis-20261025"></a>
## DEV2 PF04 원인 분석과 해결 후보

**2026-10-25 · 기존 증거 재분석 / 해결 후보 PROPOSED / 제품 변경·새 시험0.** [분석 기록](../evidence/dev2-pf04-analysis-20261025.md)과 [source·자료 색인](../evidence/dev2-pf04-analysis-20261025.json)은 보존 K0 80개·영속60개 실행 자료의 시간 재계산과 독립 검토를 연결한다. K0의 계측 추가 비용과 실행별 p50/경과 시간의 두 대역, 영속 Off/Off tail 변동과 별도의 미해결 CPU 실패를 구분했다. 원인 관측 보완과 K0 소유권 비용 절감을 우선 후보로 제안하며, WAL 공간 준비/data sync는 추가 근거와 복구 설계가 필요한 후순위다. 원 실패 원인의 확정·수정 효과·현재 source의 검증으로 확대하지 않는다. 기존 한도·실패와 PF12/PF13의 한정 완료를 보존하며 PF04·DEV2/B2/D0는 미완료다.

<a id="dev2-pf04-window-20261025"></a>

## PF04 P1 측정 구간 관측 도구 — 2026-10-25

[계획](../plans/dev2-pf04-resolution-plan.md)을 `82c14ba`로 먼저 커밋한 뒤 platform 관측 primitive·worker private gate·bounded runner를 구현했다. [선택 검증과 원 실패](../evidence/dev2-pf04-window-20261025.md)는 source별로 보존한다. 신규24개 시험 함수와8개 native 연결 사례를 변경 범위에 따라 검증했으며 최종 source 전수를 주장하지 않는다. 최초 일반 사용자 PMU는 권한 제한으로 UNAVAILABLE이었고 RUSAGE_ONLY 수집·도구 실패 정리만 당시 범위에서 확인했다. P2–P4·원 PF04 네 FAIL·DEV2 미완료는 유지한다. 최종 독립 검토는 증거 문서의 채택 상태를 따른다.

후속 사용자 승인으로 [실제 PMU 수집 경로](../evidence/dev2-pf04-pmu-20261025.md)를 추가했다. `scripts/dev2_measurement_window.py`의 `--perf-privilege sudo-cap-perfmon`은 설치된 perf에만 CAP_PERFMON을 주고 worker의 일반 사용자 상태를 유지한다. 실제 synthetic 구간의6개 counter와 coverage100%, 권한·signal/deadline·pidfd 정리를 source별 선택 검증했다. 기본 호출은 기존 무특권 경로다. 기존 native/Rust/제품 시험은 반복하지 않았고, PMU 자료는 진단용으로서 PF04 인수 PASS가 아니다.


<a id="dev2-pf04-k0-owner-20261025"></a>

## 2026-10-25 P2 K0 관측 소유권 후보 기각

[구체 계획](../plans/dev2-pf04-resolution-plan.md#p2-k0-owner)을 독립 검토하고 `a64ca58`로 먼저 commit했다. 활성 Collector가 Counters Arc를 소유해 snapshot/cursor의 별도 clone/drop을 줄이는 C1만 구현했다. [선택12함수·실제 PMU2·paired40실행](../evidence/dev2-pf04-k0-owner-20261025.md)의 정답·자원·guard는 통과했지만 확인 elapsed 중앙 비율0.975373, 구간 상한0.990642로 동결 채택 조건0.95 미만에 미달했다. C1은 기각하고 두 제품 파일을 원래 byte로 돌렸다. 후보 구현·시험·raw는 원 source로 보존하며 현재 제품에 최적화를 적용하지 않았다.

Off/Always guard는 확인 실패 중단 규칙에 따라 NOT_RUN이고 완료 시험·무변경 하위 회귀는 반복하지 않았다. 최종 독립 검토 상태는 증거 기록을 따른다. P2 후보 평가를 종료하며 P3/P4·원 PF04 네 FAIL·DEV2/B2/D0 INCOMPLETE는 유지한다.


<a id="dev2-pf04-io-cause-20261025"></a>

## 2026-10-25 P3 영속 I/O 원인 관측

`360961b` 계획 commit 뒤 [새 BPF 수집·분석 도구와 source별 선택 검증](../evidence/dev2-pf04-io-cause-20261025.md)을 구현했다. 제품은 고정5253caa clean export이며 제품 변경0이다. 도구 unit 고유16·native owner6·제품 연결4slot의 정답을 검증했고 기존 완료 제품 시험은 반복하지 않았다. 두 추적의512개 fsync가 원 측정구간과 대응하며 fsync 중 journal wait 겹침은99.0435%/99.2930%다.

0byte 미연결 완료의 논리 종료 여부를 구분할 kernel state가 없어 전체 요청 비율은 INCONCLUSIVE다. Always trace/control p99 screen은1.92731배로 FAIL이다. 다음 단계는 flush 순서 provenance·transaction별 phase 연결이며 이를 확인하기 전 P4 WAL 후보를 자동 채택하지 않는다. 원 PF04 네 FAIL·DEV2/B2/D0 INCOMPLETE는 유지하고 독립 최종 검토 상태는 증거 기록을 따른다.


<a id="dev2-pf04-flush-provenance-20261025"></a>

## 2026-10-25 P3 flush 완료 구분과 출력 버퍼 보완

계획 `f157846`·오류 보완 계획 `f18700d` 뒤 [16-probe 수집·세대별 분석과 source별 검증](../evidence/dev2-pf04-flush-provenance-20261025.md)을 완료했다. 실제 compiler에서 출력15개를 호출별 stack에 배치하고 공유 scratch 참조0을 확인했다. 원 Always count 불일치 FAIL은 보존하며 변경 source의 Always 완료2932개(물리2347·논리585)와512 fsync/phase를 연결했다. 실제5연결은 원3+수정2이고 제품/하위 인수 반복은0이다.

수정 Always의 fsync 중 장치 요청 겹침은 약98.90%이며 journal wait의 대부분이 logging→end 구간에 있다. 원 Off 수집기의 잠재 scratch 오염 한계와 원 PF04 네 FAIL·DEV2/B2/D0 INCOMPLETE를 유지한다. 한 쌍의 영향 screen PASS를 상대 비용 인수로 쓰지 않는다. P4 후보 채택/기각과 K0/영속 CPU 원인 종료 검토가 남는다.


<a id="dev2-pf04-single-writer-20261025"></a>

## DEV2 PF04 C2 단일 쓰기 통계 후보 — 2026-10-25

[계획](../plans/dev2-pf04-resolution-plan.md#c2-single-writer)을 `18eb879`로 먼저 commit한 뒤 깨끗한 `688b9af` export에서 C2를 구현했다. 비 Clone writer와 내부5/6label 구분으로 mutation 통계만 atomic load/store를 사용했고 shared CAS·포화·event·quota 의미는 유지했다. 선택11함수·compiler 반례2개·관련 fmt/Clippy와 release build가 PASS였다.

[평가 기록](../evidence/dev2-pf04-single-writer-20261025.md)의 탐색/확인 각10쌍에서 정답·유효성·guard는 통과했으나 확인 중앙0.9707905·구간 상한0.9765757로 동결0.95 미만 조건에 미달했다. C2는 기각하여 제품에 반영하지 않았고 Off/Always guard·추가 PMU는 실행하지 않았다. 원 후보/source/raw와 외부 일회성 실행기의 미검증 timeout 정리 race를 보존한다. 독립 검토 범위와 receipt는 연결 증거를 따른다.

P4 WAL 선확보/data sync도 PF04 상대 비용의 해결책으로 채택할 근거가 부족하여 미채택했다. 기존 P3b 연결을 원 비용 실패의 인과 증거로 확대하지 않는다. 원 PF04 네 FAIL·영속 CPU/tail 원인 미확정·DEV2 미완료는 유지하며, C1/C2 재확인을 이번 예산에 추가하지 않는다.


<a id="dev2-pf04-cpu-split-20261025"></a>

## DEV2 PF04 사용자·커널 CPU 분리 — 2026-10-25

계획 `b33f1e4` 뒤 수집 도구의 고정 `user-kernel-v1`8event profile을 추가하고 기본6event와 기존 권한·정리 경로를 유지했다. 실제 변경 범위의 선택6함수와 보존 release ELF의 새 Off/Always 연결2개를 완료했다.8개 모두 reported full coverage,512개 거래의 진단 정답 검산과 소유 process 종료를 확인했다.

[관측 증거](../evidence/dev2-pf04-cpu-split-20261025.md)는 커널 count와 rusage user/system 분할을 구별한다. 이 두 연결로 안정적 모드 비용이나 과거 실패 원인을 판정하지 않는다. 코드·원자료·해석의 독립 검토 상태는 연결 증거를 따른다. 원 PF04 네 FAIL·DEV2/B2/D0 미완료, C1/C2 기각·WAL 미채택은 유지한다.

<a id="dev2-pf04-prewarmup-20261025"></a>

## DEV2 PF04 warmup 전 관측 경계 — 2026-10-25

계획 `12096d4` 뒤 K0 private gate에 명시적인 prewarmup continuous/wait 두 모드를 구현했다. 둘 다 warmup 앞에서 PMU를 한 번 enable하며 continuous는 warmup 뒤 추가 IPC 없이, wait는 추가 START/GO 뒤 측정에 진입한다. 새 PMU는 warmup+drain+measurement+control bracket이고 기존 loop 지표·원 인수 path와 구분한다. 기본 모드와 비활성 경로는 유지한다.

[선택 증거](../evidence/dev2-pf04-prewarmup-20261025.md)는 Python6·Rust5·compile/Clippy/release/fmt와 실제4연결의 정답·정리 PASS를 보존한다. 동시 DEV3 변경을 제외한 source의 검증이며 최종 독립 검토 상태는 연결 증거를 따른다. 구현 검증 당시 경계 효과는 미평가였다. 후속 사용자 정정에 따라 PF04 원인 진단은 개발 회귀 제한과 구분하며 별도 예외 승인 없이 아래 동결 대조를 진행했다.


<a id="dev2-pf04-boundary-comparison-20261025"></a>

## DEV2 PF04 warmup 뒤 관측 경계 대조 — 2026-10-25

[동결 디버깅10쌍](../evidence/dev2-pf04-boundary-comparison-20261025.md)을208.675208초에 완료했다. 새 출력20개·PMU160행·소유 process 정리와 독립 원자료 검산을 확인했다. CPU W/C 중앙1.002127138, 구간0.991354900–1.015730013로 큰 경계 효과는 UNKNOWN이다. 작은 효과·과거 원인 배제나 성능 인수 완료를 뜻하지 않는다. 원 PF04 네 FAIL·DEV2/B2/D0 미완료, 기존 후보 기각과 완료 증거를 유지한다. 다음에는 모드별 추가 작업과 실행 간 변동을 구분할 진단을 동결한다.
