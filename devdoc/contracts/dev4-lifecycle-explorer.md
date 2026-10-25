# DEV4 P5 결정적 BFS 탐색기

2026-10-25 · **D4-LM-BFS-001 / 독립 D2 변경 후 수용·종결 확인 / 설계 상한 INCONCLUSIVE, 깊이 5 불일치0(archive 없음) / durable checkpoint·FE14 미완료**.

[조립기](dev4-lifecycle-assembler.md)·[모델](dev4-lifecycle-model.md)·[독립 checker](dev4-lifecycle-checker.md)를
initial부터 깊이 6까지 결정적 BFS로 엮는 controller 쪽 탐색기다. 모든 raw slot을 분류하고 alias가 아닌 slot마다
모델과 checker를 한 번씩 호출한다. 이 단위는 메모리 안 단일 실행만 맡는다. 원 checkpoint wire v1–v3의 파일
게시·lease·재개 구현과 mutant 공간 탐색은 후속이며, 실행 중 process가 끝나면 남는 prefix 없이 INCONCLUSIVE다.

<a id="unarchived"></a>
**owner 결정(archive 없음)**: codec 계약은 10GiB 이상 영속 공간을 확보한 뒤에만 전체 BFS를 실행하게 하고,
resource 원문은 archive 없는 PASS를 금한다. 이 단위의 메모리 안 실행에는 그 실행 조건을 이 결정으로 대신한다.
archive가 없으므로 FIFO가 비어도 `DrainedUnarchived`이며 PASS가 아니다. 결과는 그 profile에서 모든 slot을
모델·checker로 판정했다는 사실·counter·SHA뿐이고, 기록한 ELF·manifest·profile로 다시 실행해야만 재현된다.
archive 있는 BFS와 그 10GiB 전제는 후속 durable 단위에 그대로 남는다.

## 원문

원문은 로컬 `artifacts/dev4-lifecycle-model-20261024/`에 있다. 모델 계약의 순위표를 그대로 쓰고 아래만 더한다.

| 원문 | 이 계약이 쓰는 범위 |
| --- | --- |
| `design-proposal-v3.md` §9 | 결정적 BFS·깊이 규칙·보호 cap·INCONCLUSIVE 기준 |
| `resource-proposal-v1.md` §2–§3 | cap 수치(state 500,000, raw slot 766,500,000, active wall 1,800초, 소유 용량 2GiB) |
| `checker-call-contract-v2.md` | alias 정의, model·checker 호출 수, 완료 counter 산식 |
| `checkpoint-wire-v1.md` §2, `checkpoint-wire-v3.md` §3–§5 | edge8·discovery32 형식, class·detail, counter 관계, FIFO 정규형 |
| `context-projection-v1.md` §6 | full canonical bytes equality로 중복을 합치는 Markov 근거(입력 해석이 상태에만 의존) |

## 경계

```rust
pub fn explore(profile: &Profile, hooks: Hooks, initial: &State) -> Report;
```

- 위치는 `crates/testing/controller/src/lifecycle/`이다. reference crate의 공개 API(`assemble_input`·
  `encode_input`·`transition`·`verify_edge`·`encode_state`·`decode_state`·`action_at`)만 쓴다.
- `Hooks`는 모델과 checker 함수 포인터다. 정상 실행은 reference의 `transition`·`verify_edge`이며 시험은 같은
  자리에 mutant 모델을 넣어 FAIL 경로를 확인한다. explorer는 결과를 고치거나 해석을 바꾸지 않는다.
- initial은 독립 byte vector `tests/fixtures/dev4-lifecycle-model/vectors/state-initial.bin`을 그 vector의 test
  descriptor hash로 decode한 상태다. canonical bytes는 같은 descriptor로 `encode_state`한 출력이며 initial의
  출력은 vector bytes와 같다. 그 descriptor field는 모든 상태에 같은 고정 header일 뿐 실행 identity가 아니다.
  실행 identity는 runner receipt의 source·fixture manifest, ELF SHA와 profile이다.
- `Profile`은 최대 깊이(정상 6), cap 넷, worker 수다. worker 수는 결과 bytes를 바꾸지 않는다. 정상 profile 밖의
  값(깊이·cap 확대)은 시험 또는 별도 승인 profile이며 정상 실행의 PASS로 세지 않는다.

<a id="depth-five"></a>
## 승인된 깊이 5 확대

정상 profile은 깊이 4 상태를 펼치는 중 state cap에 걸린다. 사용자가 2026-10-25 21:36 KST에 깊이 5 정확 비교를
승인했다("2"). 질문은 깊이 5 정확 비교와 그 예상(약 1–2시간, 12GB)을 제시했고 cap 수치는 보이지 않았다.
`Profile::depth_five`의 cap은 owner가 그 예상에 맞춰 정했다. 최대 깊이 5(깊이 5 상태는 발견·검산하되 펼치지
않음), state cap 16,000,000, raw slot cap 766,500,000, active wall 4시간, 소유 용량 24GiB다. 측정한 깊이 0–4
고유 상태 431,123개와 깊이마다 약 26배인 증가로 깊이 5를 약 1,100만 개, 상태당 약 1.1KB로 본 크기이며 실제
peak는 보고에 남긴다. 이 실행은 길이 6 경로를 검산하지
않으므로 FE14는 INCONCLUSIVE로 남고, 정상 profile의 INCONCLUSIVE도 그대로 보존한다. 깊이 6은 승인 범위
밖이다.

## 탐색 순서

1. initial을 state 0, 깊이 0으로 게시한다. 게시 전 `encode_state`의 검산이 state invariant 1회다.
2. FIFO는 discovery ID 순서다. head state의 깊이가 최대 깊이보다 작으면 raw slot 0..1532를 순서대로 분류하고,
   최대 깊이 상태는 펼치지 않는다(발견 때 invariant를 이미 검산했다).
3. slot마다 `assemble_input`을 호출한다. `ProfileLimit`은 profile 상한 도달로 INCONCLUSIVE다. 그 밖의 조립
   오류는 도달 상태에서 입력을 만들지 못한 결함으로 FAIL이다.
4. 같은 pre-state에서 앞선 slot과 `encode_input` bytes가 정확히 같으면 alias다. 가장 낮은 그런 slot이
   대표다. alias edge는 class Alias, detail은 대표 raw slot이고 대표의 successor만 복사하며(대표가 제외면
   없음) first discovery가 아니다. 모델·checker 호출은 0이다.
5. alias가 아니면 모델과 checker를 한 번씩 호출한다. 모델 오류, checker 거절, 조립 입력의 Envelope·ContextFresh
   제외(조립기 출력은 그 제외의 정당한 입력이 아니다), 제외가 아닌데 응답이 없는 edge, post-state 검산 실패는
   FAIL이다. 첫 반례의 pre-state bytes·raw slot·input bytes·post bytes·관측 edge·실패 이유와 initial부터의
   raw 경로를 보존한다.
6. class는 edge가 `Excluded`이면 EnvExcluded(detail은 아래 rule ID, successor 없음), 응답 code가 Ok·Eof·
   IgnoredCompleted이면 Success, 그 밖이면 API rejection이며 둘의 detail은 ReturnCode다. 모델 edge class가
   Rejected·NoEffect인지와 무관하게 응답 code로 정한다(checkpoint v2 tiny graph의 Signal InvalidContext가
   rejection인 것과 같다).
7. Success·API rejection의 successor는 post의 canonical bytes다. 같은 bytes가 이미 있으면 그 ID이고, 없으면 다음
   ID로 게시하며 이것이 first discovery다. 새 상태는 게시 전 invariant 1회를 검산한다. hash는 조회 가속이며 같은
   hash는 전체 bytes 비교로만 같다고 판정한다.

rule ID는 ContextFresh0·SignalNoDeadline1·BindN0 2·ExecuteCommit3·ReadOnlyCleanup4·CommitNoWrite5·
CommitProgress6·CommitPreattemptGate7·RecoveryPre8·RecoverySet9·Envelope10·ResumeWork11·FencedClose12·
PeerFence13이다.

## ledger와 counter

edge 하나는 checkpoint v1 §2의 edge8 u64다(pre bits0..18, raw 19..29, class 30..31: Success0·rejection1·
Alias2·EnvExcluded3, detail 32..42, successor+1 43..62, first 63). ledger SHA는 slot 순서의 LE u64 연결의
SHA-256이고 discovery SHA는 discovery32 frame과 wire bytes 연결의 SHA-256이다. edge8은 state ID를 19 bit에
담으므로 state cap이 2^19 이상인 profile은 같은 field를 넓힌 edge16(pre u32, raw u16, class u8, first u8,
detail u16, reserved u16, successor+1 u32, LE)을 쓰고 보고에 형식을 적는다. discovery32는 그대로다.

counter는 checkpoint v3 §3·§5를 따른다. checker 수 = success + rejection + excluded, checked transitions =
success + rejection, model 호출 수 = checker 수, alias의 model·checker 호출 0, 분류 slot 수 = 1533 × 펼친 상태
수(멈춘 상태의 부분 slot 제외). 전체와 Api 21개별 success·rejection·alias·excluded·checker, rule별 제외 수,
깊이 0..6별 unique·fully-expanded·invariant-checked를 낸다. 깊이별 invariant-checked는 unique와 같고 initial을
포함한다.

## 상한과 판정

| 상태 | 조건 |
| --- | --- |
| DrainedUnarchived | FIFO가 비고 모든 slot이 분류·검산됐으며 cap·실패가 없음. archive가 없어 PASS가 아님 |
| FAIL | 위 5의 FAIL 원인과 ProfileLimit 밖 조립 오류. 반례를 보존하고 즉시 멈춤 |
| INCONCLUSIVE | state·raw slot·active wall·소유 용량 cap, ProfileLimit, 할당 실패, 중단 |

ProfileLimit은 깊이 6 이하에서 나오지 않아야 하는 설계 상한 도달이므로 별도 이유로 보고한다. FE14의 판정은 이
상태와 별개다. 길이 6 witness(LM-M3·EF1·EF3 등)는 깊이 5 profile에서 닿지 않으므로 FE14는 INCONCLUSIVE이며
witness 도달 검사는 후속이다.

cap 검사는 새 state 게시 전(state·용량), 새 slot 시작 전(raw slot), 새 상태 결과를 합치기 전(wall)에 한다.
wall은 worker가 한 batch를 평가한 뒤 상태마다 보므로 batch 하나만큼 넘을 수 있고 그 시간도 보고에 남는다.
cap에 걸리면 그 상태의 남은 slot을 분류하지 않고 직전까지의 counter·SHA와 멈춘 frontier(head·현재 상태·
다음 slot)를 보고한다. 범위를 줄이거나 미분류 slot을 제외로 바꿔 PASS를 만들지 않는다. 소유 용량의 정의는
아래 보고 절에 있다. frontier의 head는 완전히 펼친 상태와 최대 깊이로 건너뛴 상태 수의 합이며, 멈춘 slot 수는
1533 × head + next_raw다(checkpoint v3 §4의 정규형과 같은 관계).

## 보고

`Report`는 profile, status와 이유, 깊이·API별 counter, ledger 형식과 ledger·discovery SHA, 멈춘 frontier,
경과 시간과 소유 용량 peak, 첫 반례를 담는다. arena는 다시 할당하지 않는 64MiB block이며 한 상태는 block을
넘지 않는다. 소유 용량은 arena·offset·depth·parent·visited index의 capacity에 worker마다 고정 allowance
32MiB를 더한 값이다. offset·depth·parent 배열이 커질 때의 옛·새 배열 공존분도 미리 charge한다. worker 쪽
할당(codec buffer·상태 3개·alias 표·batch successor)은 fallible이 아니므로 allowance로 추정해 charge하고, 그래도
실패하면 process가 끝나 receipt가 없는 INCONCLUSIVE다. allowance는 상한이 아닌 추정이므로 보고에 process의
peak RSS(`VmHWM`)를 함께 남긴다. ledger 형식과 rule ID 표는 `LEDGER_DESCRIPTOR` 문자열로 고정하고 그 SHA를
보고에 남기며, 형식이나 rule ID를 바꾸면 그 문자열과 SHA가 함께 바뀐다. runner는 profile마다
실행 전 attempt 기록을 먼저 남긴다. runner는 source·fixture manifest와 ELF SHA를 함께 receipt로 남긴다.
기본 보존 위치는 `artifacts/dev4-lifecycle-bfs/`다.

## 선택 검증

- edge8 6개와 discovery32 genesis가 checkpoint-wire-v1.json의 독립 byte vector와 같고, canonical initial이
  독립 initial vector와 같다.
- 작은 깊이 profile에서 worker 수 1과 여러 개의 ledger·discovery SHA·counter가 같다.
- 깊이 1 profile은 initial의 1533 slot을 모두 분류하고 counter 산식이 맞는다(독립 기대 상태 수는 없다).
- alias는 checker 호출이 없고 분류 slot·rule별 제외 합이 닫힌다.
- state·raw slot·wall·용량 cap이 정확한 지점에서 INCONCLUSIVE와 frontier를 낸다.
- mutant 모델 hook이 FAIL과 반례(post·경로 포함)를 내고 정상 모델은 같은 profile을 비운다. 모델 오류·조립 입력의
  Envelope 제외·응답 없는 edge도 각각 FAIL이다.
- 최대 깊이 상태는 펼치지 않고 invariant만 센다. edge16은 counter·discovery SHA를 바꾸지 않는다.

hash 충돌과 할당 실패는 시험으로 만들지 않았다. 충돌은 전체 bytes 비교로만 같다고 보는 구조로, 할당 실패는
fallible reserve 경로로 다룬다.

## 남은 의무

archive와 durable checkpoint(원 checkpoint wire의 구현, 10GiB 영속 공간), witness 도달 검사와 FE14 mutant 공간
탐색, 깊이 6 완주(승인 밖)가 남는다.
