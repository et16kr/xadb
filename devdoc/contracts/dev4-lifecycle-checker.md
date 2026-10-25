# DEV4 P5 독립 edge checker

2026-10-25 · **D4-LM-CHECK-001 구현·선택 시험 PASS / 모델 차등 불일치0 / 독립 D2 수용·D1 제한 수용 / alias·BFS·FE14 미완료**.

[수명 모델](dev4-lifecycle-model.md)의 관측 하나 `(pre, input, edge, post)`가 명세가 허용하는 결과인지
판정하는 시험 전용 관계 checker다. 모델이 만든 값을 기대값으로 쓰지 않고 사전 상태와 원문 규칙만으로
기대 class·응답·후상태를 다시 계산한다. 실제 SQL/engine 연결, BFS·checkpoint, alias 판정은 범위 밖이다.

## 정본 입력

```rust
pub fn verify_edge(pre: &State, input: &ResolvedInput, edge: &Edge, post: &State)
    -> Result<(), CheckFailure>;
```

`CheckFailure`는 처음 어긋난 부분 하나다. PreState(필요한 참조가 없는 비도달 상태)·Input(raw profile 밖
입력)·Class(제외로 숨긴 합법 입력 포함)·Response·ContextEvent·Controls·ConsumedParts·Authenticated·
PostState{table, reason} 순서로 비교한다. CheckerCapacity는 checker 자신의 고정 table이 넘친 결함이며
비도달 상태와 구별한다. 같은 원문 순위표(모델 계약 §정본 입력의 순위1–5)를 따른다.

## 독립성

checker는 구현에 참여하지 않은 별도 작성자가 모델 코드를 읽지 않고 썼다. 모델과 공유하는 것은 state·
input record 선언과 `Edge` 타입뿐이며 모델 전이·helper와 입력 조립기를 호출하지 않는다. 기대 후상태는
`pre ⊕ delta`를 table별로 비교하며 heap·재귀·State clone이 없다.

공급 Env1은 checker가 wire-v2 §3–§4와 조립 계약에서 독립으로 다시 계산한 recipe와 집합으로 비교한다.
다르면 `Excluded(Envelope)`(recovery row는 `RecoverySet`)만 허용한다. `Excluded(RecoveryPre)`는 old engine
Fenced·두 shell Closed·parked 없음·fresh engine 전제가 어긋날 때 받는다. fact·fresh engine identity 불일치는
ENV-RECOVERY-PRE와 공급 envelope 규칙이 함께 해당하므로 RecoveryPre와 Envelope를 모두 받는다. 기대 owner/obligation 변화는 그
recipe에서 만들며 공급 row나 관측 post에서 역생성하지 않는다.

환경 제외도 판정한다. `Excluded(rule)`은 입장한 입력이 그 rule의 조건을 실제로 만족하고 아무것도 바뀌지
않을 때만 받는다. 조건이 여럿이면 계약에 우선순위가 없으므로 해당하는 rule 모두를 받는다. 합법 입력을 제외로
분류하면 Class 실패다.

## 해석 선택

원문이 열어 둔 항목은 아래처럼 정했다. 모델과의 차등 대조에서 어긋난 항목은 모델 계약의
[차등 판정](dev4-lifecycle-model.md#checker-adjudication)과 같은 결론이다.

- 제외 edge는 응답·context event·consumption·authenticated가 없고 controls는 기본값이다.
- 응답 decision 필드는 child 인증 뒤 호출이 선택한 transaction(Execute는 caller 또는 새 T, Fetch 계열은
  descriptor T, CloseStatement는 slot descriptor T, session family는 caller T, continuation은 parked T)에서,
  그 전에는 caller의 후상태 transaction에서 읽는다.
- `EngineFenced`는 attached engine의 first_cause를 engine cause로 싣는다.
- Ok/Eof/IgnoredCompleted 밖의 모든 code는 Diagnostic을 가진다(control-plane과 InvalidContext 포함).
- Fetch·ParkFetch·ResumeFetch의 거절·cached·완료는 rows/complete `0/1`·`false/true`, ParkFetch park
  성공과 control-plane은 NotApplicable이다.
- Inspect·Reopen 성공은 certainty Known, CommitFact·Recovery 성공은 NotApplicable이다.
- child를 인증하는 API의 child 인증 뒤 Rejected는 인증한 statement 또는 result와 wire-v2 §4가 선택한
  transaction(Execute는 caller의 사전 T, Fetch 계열은 descriptor T, CloseStatement는 slot descriptor T 또는
  None, BindAll·InspectStatement는 None)을 authenticated targets로 남긴다. child 인증 실패는 세 필드 None,
  그 밖의 edge는 관측 없음이다(v5 §1).
- Recovery가 끝낸 transaction의 completion_fact는 바꾸지 않는다. recovery fact는 transaction이 없어 codec의
  completion fact 연결 규칙을 만족하지 못하며, 복구 사실은 obligation의 recovery_fact가 기록한다.
- 첫 fence는 engine의 현재 public result를 EngineFence로 retire하고 slot counter+1, ResultOpen slot을
  ReadyBound로 바꾸며 `active_public_result`를 지운다. descriptor는 상태·위치를 유지하고
  `blocked_by_fence`를 받는다. peer는 life RecoveryPending과 새 obligation 연결만 받는다.
- CloseSession Incomplete는 Rollback Incomplete처럼 session을 열어 둔다(owner 결정 2026-10-25). 입장한
  Complete만 caller의 transaction·parked 연결을 지운다.
- 열린 implicit ReadOnly cursor를 닫는 CloseStatement는 CloseResult처럼 그 transaction을 끝낸다.
- implicit ReadOnly의 Rollback·whole abort는 Aborted decision 없이 끝낸다. Execute·CommitAtomic·Finish
  abort는 commit progress를 바꾸지 않는다.
- obligation disposition은 그 transaction의 RW/Recovery owner 전체에서 유도하고 latest_fact는 obligation
  row가 소비된 envelope에 있을 때마다 전진한다.
- fact next_work는 Execute fact, atomic terminal control, pending continuation control, atomic Fetch
  UndoIncomplete9에서 NotConsumed다. CommitFact fact context는 parked commit context이며 receipt/anchor는
  Receipt4를 소비할 때만 기록한다.
- 이미 Fenced인 engine의 Finish NCR은 PeerFence cause와 NCR decision/knowledge이고, Unknown·
  PublicationPending은 기존 obligation의 count0 cause 갱신이다.
- Bind payload bytes와 Park/BeginCommit의 outcome·deadline 대응은 다시 검산하지 않는다. entry bit 없는
  entry class outcome은 `Excluded(Envelope)`다.

## 호출 계약

로컬 원문 `artifacts/dev4-lifecycle-model-20261024/checker-call-contract-v2.md`의 호출 후보를 따른다.
alias가 아닌 raw slot마다 모델 전이 1회와 `verify_edge` 1회를 호출하며 완료 ledger의 checker 수는
`success + rejection + excluded`다. checker 거절은 원 반례·입력·관측을 보존하는 탐색 실패이며 호출했다는
사실만으로 PASS가 아니다. BFS·checkpoint 결합은 후속 단위다.

<a id="selection"></a>
## 선택 검증

`checker_tests`는 원 literal 29 edge 수용, 5 known-bad 거절, literal 파생 표적 변형 거절, 환경 제외 판정,
fenced close, fence·continuation 변형, literal 상태에서 조립한 모든 입력의 total 판정, 손상 입력의 panic
없음, checker 자기 walk 후상태의 codec 유효성을 본다. 자기 walk의 기대값은 checker 자신의 시험 전용
materialize이며 모델 일치가 아니다.

`differential_tests`는 모델 출력을 checker로 판정한다. literal 29 상태, 판정 이력과 드문 family의 고정 경로 7개,
결정적 walk(`XADB_DIFF_WALKS`, `XADB_DIFF_DEPTH`)의 방문 상태마다 조립기가 해석한 모든 raw slot이
대상이다. 조립 오류는 종류별로 세고 InvalidState·Capacity는 실패다. walk는 여덟 걸음 중 하나꼴로 거절
edge도 따라가며 마지막 상태도 판정한다. 조립 입력에서 Envelope·ContextFresh 제외가 나오면 실패이고, 대신
literal 상태와 고정 경로의 마지막 상태에서 context nonce·공급 transaction·fact identity를 바꾼 입력으로 두
제외를 대조한다. 판정 입력 영역은 조립기 출력과 이 손상 입력이며 일반 Envelope 전수가 아니다. 불일치는
자동 모델 버그가 아니라 원문으로 판정할 대상이다. `XADB_DIFF_COVERAGE`는 API·outcome·class별 수를,
`XADB_DIFF_REPLAY="raw,…:raw"`는 한 관측의 바뀐 table을 출력한다.

## 남은 의무

alias, BFS·checkpoint(기본 보존 위치 `artifacts/dev4-lifecycle-bfs/`, 약 10GiB), FE14
완주와 mutant 공간 탐색이 남는다.
