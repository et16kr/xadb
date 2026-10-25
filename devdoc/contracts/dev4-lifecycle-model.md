# DEV4 P5 수명 모델 전이

2026-10-25 · **D4-LM-MODEL-001 독립 D2 수용 / 구현·선택 시험·독립 D1 제한 수용 / [독립 checker](dev4-lifecycle-checker.md) 차등 판정 반영 / BFS·FE14 미완료**.

[전체 입력 조립](dev4-lifecycle-assembler.md)이 만든 `ResolvedInput`을 받아 사전 상태에서 응답과 후상태를
만드는 시험 전용 추상 모델이다. 실제 SQL/session/engine, ACK·내구성, BFS·checkpoint는 이 결정의 범위가
아니다. 모델은 정답 생성기가 아니며 [독립 checker](dev4-lifecycle-checker.md)는 이 코드를 재사용하지 않는다.

## 정본 입력

원문은 로컬 `artifacts/dev4-lifecycle-model-20261024/`에 있다. 아래 순서로 적용하며 뒤 문서는 앞 문서가
명시적으로 후보·미결정으로 남긴 항목만 닫는다. 아래 "owner 결정" 절은 원문이 정하지 않은 항목이다.

| 순위 | 원문 | 범위 |
| --- | --- | --- |
| 1 | [SQL 정본](dev4-sql-design.md#prepared)의 prepared·transaction·operation-context | 제품 의미. 모델이 바꾸지 않음 |
| 2 | `design-proposal-v3.md` §2–§7 | fence·reopen/recovery·gate 순서·관측 schema·환경 가능성 |
| 3 | `owner-decisions-v5.md`, `identity-transition-v1.md`, `context-projection-v1.md`, `context-admission-v1.md` | 호출 기록·commit 경계·Bind tag·shared fence·resource 범위·발급/retire·B projection |
| 4 | `owner-final-addendum-v2.md`, `wire-proposal-v2.md` §3·§4, `literal-design-v1.md` §4 root 선택·§5 | 같은 호출 RW3 보존·소비 bit·Env1 class·transaction 선택·응답 후보 행 |
| 5 | [`literals.json`](../../tests/fixtures/dev4-lifecycle-model/literals.json) 29 edge/12 trace/5 mutant | 독립 기대값. 순위1–4와 충돌하면 조용히 고르지 않고 재검토한다 |

## 공개 경계

```rust
pub fn transition(pre: &State, input: &ResolvedInput, post: &mut State, edge: &mut Edge)
    -> Result<(), ModelError>;
```

제외(`EdgeClass::Excluded`)는 `post == pre`이고 응답이 없다. `ModelError`는 사전 상태의 필요한 참조가
깨졌거나 고정 table이 넘칠 때만이며 API 거절·제외가 아니다. 이때 post는 pre로 되돌리고 edge는 기본값이며
게시하지 않는다. 공급 Env1이 사전 상태와 맞지 않는 경우(fact/birth/receipt nonce, before holder, 새 O의
transaction, 선택 transaction, continuation identity, Env1 부재·다른 kind, Recovery fresh engine identity)는
모델 오류가 아닌 `Excluded(Envelope)`다. 모델이 검산하는 envelope 항목은 이것뿐이다. work_kind·work_class·cleanup·commit_class처럼 raw class가 정하는 고정
조합 scalar와 그 밖의 row 값은 조립기 출력을 그대로 신뢰하므로, 판정 입력 영역은 조립기 출력과
[checker 차등 대조](dev4-lifecycle-checker.md#selection)가 정한 손상 입력이다. 성공 뒤 모든 table을 codec의 canonical 순서로
정렬한다. 모델은 heap·재귀·State clone이 없다.

`Edge`는 literal edge와 같은 관측이다. class는 Admitted(입장한 fresh 호출과 그 값/실행 오류, cached
terminal과 멱등 close 포함)·Rejected(입장 전 거절)·NoEffect(상태 불변인 context 없는 호출과 control-plane
응답)·NoContextIssue(context 없이 상태를 바꾸는 Signal/Reopen/CommitFact/Recovery)·Continuation(일치한
Parked를 끝내는 Resume/Finish)·Excluded(rule)다. controls는 entry cancel/deadline의 NotChecked·No·Yes와
이번 호출이 관측한 cancel/deadline이다. consumed_parts는 Work1·Cleanup2·Receipt4·CommitProgress8·
RecoverySet16의 실제 적용 영역이다. authenticated targets는 child 인증 실패의 None과 인증 뒤 거절의
인증된 참조만 남긴다([차등 판정](#checker-adjudication)).

## gate 순서

fresh context API는 envelope → child 인증 → Closed shell → parked Busy → shared fence → API 상태 →
entry Cancelled → entry Timeout → 환경 가능성 → admit이다. 거절은 manager를 보존하고 fresh context를
Completed로 기록해 next Context를 한 번 전진시킨다. 제외는 아무것도 기록하지 않는다.

| gate | 결과 |
| --- | --- |
| envelope | input.context가 caller의 현재 session/engine·next Context가 아니거나 nonce가 6을 넘으면 `Excluded(ContextFresh)` |
| child | 같은 caller의 exact current statement(Current·CurrentClosed) 또는 public live result가 아니면 `InvalidHandle` |
| shell | Closed shell은 CloseSession이면 멱등 Ok, 그 밖은 `SessionState` |
| parked | actor에 Parked가 있으면 `Busy` |
| fence | attached engine이 Admitting이 아니면 `EngineFenced`; CloseStatement·CloseSession·Inspect는 예외 |
| entry | Cancelled가 Deadline보다 먼저, `Cancelled`·`Timeout` phase Entry |

continuation은 actor pending 일치(없음 NoPendingOperation, 다른 종류 ContinuationKind) → continuation
identity(다르면 `Excluded(Envelope)`) → Resume의
fence terminal → pending Cancel → pending Deadline → work다. Inspect·Signal·Reopen·환경 fact는 새 context가
없다. 조립기가 공급한 Env1의 transaction은 선택 transaction과 같아야 하며 다르면 `Excluded(Envelope)`다.

## 응답 행

literal이 고정한 행은 그대로이고 나머지는 owner 결정이다. 오류는 같은 내용의 Diagnostic을 가진다.
Execute Select 성공은 rows0, Dml 성공은 affected_count1이다. rows/complete는 Fetch·Resume의 거절·cached·
완료 응답에만 0/1·false/true로 붙고, ParkFetch 성공·control-plane 응답·그 밖의 family는 NotApplicable이다. context 없는 Inspect·Reopen의 gate 거절은 class Rejected, Signal의 InvalidContext·
IgnoredCompleted와 control-plane 응답은 NoEffect다. transaction이 없는 호출의 decision 필드는 actor의 현재
transaction에서, 그것도 없으면 NotApplicable·NoDecisionObserved에서 읽는다.

| 경우 | code / phase / effect / retry / origin |
| --- | --- |
| 진입 거절 | InvalidHandle·HandleClosed·SessionState·TxnState·BindCount(ReadyUnbound)·StalePlan(Invalid) / Entry / NoEffect / Never |
| 진입 대기 거절 | Busy·ResultBusy·SlotBusy / Entry / NoEffect / Operation; EngineFenced / Entry / NoEffect / AfterRecovery, engine cause는 attached engine의 first_cause |
| entry control | Cancelled·Timeout / Entry / NoEffect / Operation / Cancelled·Timeout |
| control-plane | NoPendingOperation·ContinuationKind / NotApplicable / NoEffect / Never, certainty NotApplicable |
| Signal | InvalidContext(Never)·IgnoredCompleted·Ok / NotApplicable / NoEffect, certainty NotApplicable |
| Bind 값 오류 | BindCount·Type·NotNull·Range·SyntaxUtf8 / Bind / NoEffect / Never; Limit / Bind / NoEffect / Operation |
| Execute 오류 | StalePlan / Execute / NoEffect / Never; ValueError·Cancelled·Timeout / Execute / StatementRestored / Operation |
| Fetch 오류 | RowBufferTooSmall / Fetch / NoEffect / Operation; ReadError·Cancelled·Timeout / Fetch / CursorReleased / Operation |
| Conflict | Execute·Commit은 StatementRestored, Fetch·Resume은 CursorReleased / WholeTransactionDecision / Conflict |
| commit abort | CommitAtomic ValidationAborted·TerminalCancelled·TerminalTimeout와 Finish Aborted는 Conflict·Cancelled·Timeout / Commit / StatementRestored / WholeTransactionDecision |
| 환경 fact | CommitFact 성공은 Ok / Commit / NoEffect / NotApplicable, Recovery 성공은 Ok / Recovery / NoEffect / NotApplicable; 둘 다 certainty NotApplicable |
| cleanup 미완료 | RecoveryRequired / 해당 phase / RecoveryRequired / AfterRecovery / UndoIncomplete |
| commit 불확실 | ResultUnknown / Commit / RecoveryRequired / AfterRecovery / CommitUnknown, certainty Unknown |
| publication 미완료 | ResultUnknown / Commit / RecoveryRequired / AfterRecovery / PublicationPending, certainty Unknown |
| Resume fence terminal | EngineFenced / Fetch / NoEffect / AfterRecovery, engine cause는 first_cause, rows0·complete false |
| 성공 | Ok(Fetch EOF는 Eof) / Prepare·Bind·Execute·Fetch·Close·Commit·Rollback·Recovery, Begin·Set는 Entry, Reopen·Inspect·Signal은 NotApplicable |

성공 effect는 NoEffect가 기본이며 Dml StatementApplied, 열린 cursor를 닫은 Fetch EOF·CloseResult·
CloseStatement는 CursorReleased, Rollback Complete는 StatementRestored다. public_transaction은 후상태
actor.transaction의 life(없으면 Idle)이고 decision·client_knowledge·commit_progress·completion(receipt)·
delivery는 이번 호출이 다룬 transaction의 후상태 record에서 읽는다. recovery_obligations는 이번에 소비한
Env1의 obligation row다.

## 전이 규칙

발급·retire·resource phase·owner birth/release/handoff·같은 호출 RW3·fence는 identity-transition과
owner v5를 따른다. 소비한 Env1의 owner/obligation row는 그대로 owner·fact membership으로 기록하며
결과에서 역생성하지 않는다. birth는 저장 next부터 연속 nonce block이어야 하고 기존 owner row의 before는
현재 holder와 같아야 한다. owner.last_fact는 disposition이나 holder가 실제 바뀔 때만 갱신한다.

| family | API 상태 행 | admit 효과 |
| --- | --- | --- |
| Prepare | ResultBusy → SlotBusy(slot이 Vacant/Closed 아님) | Closed slot의 이전 statement를 SlotReuse로 retire, 새 statement·Plan, N0이면 Empty binding·Binding |
| BindAll | ResultBusy → HandleClosed → StalePlan | N1 SuccessA/B와 N0 SuccessA/B·Count0은 성공(N0은 Empty)으로 binding 교체(old Replaced·Binding Released). N1 Count0·Count2와 N0 Count2는 BindCount 오류, N1 값 오류는 해당 code이며 무효과. N0 Type/NotNull/Range/SyntaxUtf8/Limit은 `Excluded(BindN0)` |
| Execute | ResultBusy → HandleClosed → StalePlan → BindCount | 거래 선택/생성, 이전 terminal result Reexecute retire·descriptor 제거, outcome recipe |
| Fetch | HandleClosed → cached Eof/Error | Rows 위치+1, Eof·terminal·ReadError cursor release·Error terminal, Conflict whole abort·result Rollback retire, UndoIncomplete9 RW fence(fact next_work NotConsumed) |
| ParkFetch | Fetch와 같음 | Open이면 원 deadline·target으로 Parked, cached는 park 없이 Completed |
| CloseResult | current Closed 멱등 | C(R) release·Closed, 열린 implicit RO 종료 |
| CloseStatement | current Closed 멱등 | L(P) release, result StatementClose retire, binding Released, statement CurrentClosed·slot Closed |
| Begin·SetAutocommit | TxnState(Active) | ExplicitBegin RW·RW3 / mode 설정 |
| CommitAtomic | ResultBusy → TxnState(Idle) | changed RW는 Published·durable receipt, no-write는 NoWriteValidated(identity 유무), abort/fence class |
| BeginCommit | CommitAtomic과 같음 | BeforeAttempt, ParkedCommit |
| CommitFact | pending 일치 | BeforeAttempt→Attempted(changed)/NoWriteValidated(no-write), Attempted→DurableConfirmed, DurableConfirmed→Published·drain |
| FinishCommit | pending 일치 | Published/NoWriteValidated의 Committed 전달, BeforeAttempt의 Aborted(engine Admitting일 때만)·NCR(Fenced면 기존 O로 정확 handoff, 아니면 census), Attempted의 Unknown, DurableConfirmed의 PublicationPending |
| Rollback | TxnState(Idle) | Complete는 열린 result Rollback retire와 whole abort, Incomplete는 RW fence |
| CloseSession | Closed 멱등 | Complete는 Admitting이면 whole abort, 그 밖은 local release와 handoff이며 child 전부 SessionClose retire, slot Vacant, shell Closed. Incomplete는 Admitting의 RW만이며 Rollback Incomplete처럼 session을 열어 둔 채 RW fence만 적용 |
| Reopen | SessionState(Open) → Busy → engine 조건 | Same은 attached engine이 current이고 Admitting, Fresh는 attached engine이 Retired이고 successor가 current일 때만이며 아니면 EngineFenced. 새 session은 current engine에 붙고 autocommit On·slot generation 1 |
| Recovery | `Excluded(RecoveryPre/RecoverySet)` | old engine Retired·successor, 새 engine Admitting·current, obligation RecoveryConfirmed |

환경 제외는 N0 Bind 값 오류(BindN0), Idle ON implicit RW Dml 밖의 Execute Unknown·PublicationPending
(ExecuteCommit), RW가 아닌 거래의 cleanup 미완료(ReadOnlyCleanup), no-write에 불가능한 commit class와 그
반대(CommitNoWrite), 진행 순서 밖 fact/Finish(CommitProgress), pending 신호·peer fence 뒤 pre-attempt
fact(CommitPreattemptGate), peer fence 아래 BeforeAttempt Finish Aborted(PeerFence), 이미 Fenced인 engine의
CloseSession Incomplete(FencedClose, v3 ENV-FENCED-CLOSE), 신호 없는 Rows/Buffer/Eof+Incomplete(ResumeWork),
deadline 없는 parked의 Deadline 신호(SignalNoDeadline), recovery 전제와 obligation·owner 정확 집합
(RecoveryPre·RecoverySet), family와 다른 Env1 kind·transaction(Envelope)이다.

## owner 결정

- cached EOF/Error와 멱등 close는 Admitted이며 소비0·manager 불변이다. cached EOF의 effect는 NoEffect다.
- 입장한 Execute는 outcome과 무관하게 slot의 이전 terminal result를 Reexecute로 retire하고 descriptor를 지운다.
- fence의 cause transaction은 class의 decision(NCR·Unknown·Committed)과 RecoveryPending을 받는다. 새 O를
  받은 peer RW transaction은 progress·decision을 그대로 두고 life만 RecoveryPending이다.
- CommitFact Attempted는 진행을 Attempted로, decision을 원 불확실성인 Unknown으로 둔다(v5 "원 불확실성").
- Recovery는 확인한 obligation의 transaction을 Ended로 둔다. CloseSession과 Reopen은 slot을 Vacant로 비운다.
- 이미 Fenced인 engine의 Finish NCR은 PeerFence cause의 RecoveryRequired와 LocalReleaseAndHandoff cleanup이다.
- 신호 없는 Finish Aborted는 commit validation 실패로 보아 Conflict다.
- Closed shell의 비-close fresh 호출은 SessionState, context envelope 불일치와 next Context>6은 `Excluded(ContextFresh)`다.
- 일치 pending이 없는 CommitFact·Finish·Resume은 NoPendingOperation, 다른 종류면 ContinuationKind다.
- 기존 O의 cause 원인 갱신은 known_decision·latest_fact와 latest_reason·reason_mask(1 << reason)를 바꾼다.
- 같은 값을 다시 Signal하면 상태가 그대로이므로 NoEffect다.

<a id="checker-adjudication"></a>
독립 checker와의 차등 대조와 그 독립 검토(2026-10-25)에서 원문이 정하지 않았거나 모델이 원문과 어긋난
열두 항목을 아래처럼 정했다. 각 항목의 근거와 판정 이력은 [checker 증거](../evidence/dev4-lifecycle-checker-20261025.md)에 있다.

- InspectStatement의 child 인증 실패도 authenticated targets None을 기록한다(위 Edge 규칙의 누락 교정).
- `EngineFenced`(gate 거절·Reopen·Resume fence terminal)의 engine cause는 attached engine의 first_cause다.
- 첫 fence가 retire한 public result의 slot이 ResultOpen이면 ReadyBound로 바꾼다. descriptor·owner·위치와
  `blocked_by_fence`는 그대로다(v3 §2 "ReadyBound 표시만으로 owner를 해제하거나 fence를 지우지 않는다").
- CloseSession Incomplete는 Rollback Incomplete처럼 실패한 close다. shell·session은 Open, actor는
  RecoveryPending transaction과 parked 연결을 유지하고 SessionClose retire·local release가 없다. 첫 fence
  효과와 Env1 census row만 적용한다. SQL 정본의 "close_session은 모든 statement/result를 무효화한다"는
  성공한 close의 효과이고 undo 실패는 RecoveryRequired·Fenced 반환과 engine 복구 소유 유지다. wire-v2 §4는
  Incomplete에서 local 성공을 발명하지 않게 하며, identity-transition은 첫 fence가 result만 retire하고
  statement를 cleanup·진단용으로 보존한다고 정한다. statement를 owner 해제 없이 retire하면 plan·binding
  owner가 남겨지므로 뒤의 fenced CloseSession Complete가 local release와 handoff를 수행한다. 순위 밖인
  design-proposal-v2의 "CloseSession + UndoIncomplete | child token 무효화"는 첫 fence의 result retire로
  만족된다.
- atomic Fetch UndoIncomplete9는 자기 next work를 지정하지 않으므로 fact next_work는 NotConsumed다.
  공급 work class 9는 fact outcome에 남고 Work1+Cleanup2 소비는 그대로다. 이때 NotConsumed는 class가 next
  work label을 지정하지 않는다는 뜻이고 Work1은 공급 work class 9를 적용했다는 뜻이다. Resume의 Conflict·
  ReadError+Incomplete는 공급 next_work를 기록한다.
- fenced CloseSession Complete의 local release·handoff는 transaction의 종료나 미완료를 확인하지 않으므로
  completion_fact를 두지 않는다. fenced Finish NCR handoff는 NCR을 확인하므로 completion_fact다. 근거는
  literal-design §4 root 선택의 "transaction.completion_fact는 그 종료/미완료를 확인한 consumed fact"다.
- Recovery 성공은 CommitFact·Signal처럼 API가 아닌 외부 fact이므로 certainty NotApplicable이다.
- 열린 cursor를 닫는 CloseStatement는 그 transaction이 Active인 implicit 거래일 때만 끝내고 completion_fact를
  둔다. fence 뒤 RecoveryPending transaction의 기존 completion_fact를 덮지 않는다.
- Recovery가 끝낸 transaction은 completion_fact를 바꾸지 않는다. fence로 RecoveryPending이 된 peer도 None으로
  남는다. codec loader는 completion_fact가 그 transaction을 가리키는 fact이기를 요구하는데 recovery fact는
  transaction이 없는 engine 이력이다(v3 §3 "복구 확인 fact는 별도 engine history"). 복구로 끝난 사실은
  obligation의 recovery_fact가 기록한다.
- child를 인증하는 API(statement 대상 BindAll·Execute·CloseStatement·InspectStatement, result 대상 Fetch·
  ParkFetch·CloseResult)의 child 인증 뒤 Rejected 응답(shell·parked·fence·API 상태·entry control)은
  authenticated targets에 인증한 statement 또는 result identity와 transaction을 남긴다(v5 §1). transaction은
  wire-v2 §4의 선택을 따른다. Execute는 caller의 사전 transaction, Fetch·ParkFetch·CloseResult는 descriptor의
  transaction, CloseStatement는 slot descriptor가 있으면 그 transaction이며, 없거나 선택이 없는 BindAll·
  InspectStatement는 None이다.
  child 인증 실패는 세 필드 모두 None이고 child가 없는 API와 Rejected가 아닌 edge는 관측이 없다. 원 literal
  `sel.entry_cancel`은 이 규칙보다 앞서 targets를 생략했으므로(순위5와 순위3의 충돌) 두 시험이 r1s0·t1로
  정정해 대조한다.
- continuation(ResumeFetch·CommitFact·FinishCommit)의 공급 context identity가 actor의 Parked context와 다르면
  `Excluded(Envelope)`다. 이 검사는 pending 일치(NoPendingOperation·ContinuationKind) 바로 뒤, Resume fence
  terminal과 pending control보다 앞이다. identity는 Env1이 아닌 input prefix에 있지만(wire-v2 §3) 공급 입력이
  사전 상태와 맞지 않는 경우의 같은 규칙으로 보며 조립기는 항상 Parked identity를 공급한다.
- child 인증 뒤 거절 응답의 decision·client_knowledge·commit_progress·completion·delivery는 인증 참조의
  transaction(wire-v2 §4 선택, 위 항목)에서 읽고 그것이 없을 때만 caller의 현재 transaction에서 읽는다. 끝난
  implicit ReadOnly의 Eof result를 가진 caller가 새 거래를 연 뒤 그 result를 다루는 거절이 예다. BFS 깊이 4에서
  발견했다([BFS 증거](../evidence/dev4-lifecycle-explorer-20261025.md)).

소비 bit는 실제로 적용한 영역만 센다.

| 경우 | bit |
| --- | --- |
| Execute 성공(implicit 아님), Fetch/Resume Rows·RowBufferTooSmall | Work1 |
| Execute StalePlan·ValueError·Conflict·Incomplete, Fetch/Resume Eof·ReadError·Conflict·Incomplete | Work1+Cleanup2 |
| atomic·pending Cancelled/Timeout, Rollback, Close*, commit abort, CloseSession, cleanup만의 Incomplete, Finish Aborted·NCR | Cleanup2 |
| Execute Unknown | Work1+CommitProgress8 |
| Execute implicit commit·PublicationPending | Work1+Receipt4+CommitProgress8 |
| CommitAtomic/CommitFact Published·DurableConfirmed·NoWrite(identity)·PublicationPending, Finish Committed(receipt)·PublicationPending | Receipt4+CommitProgress8 |
| CommitAtomic/CommitFact Unknown·Attempted·NoWrite(identity 없음), Finish Committed(receipt 없음)·Unknown | CommitProgress8 |
| Recovery | Cleanup2+RecoverySet16 |

## 범위 밖 관측

InspectSession·InspectStatement의 진단 필드(context-projection §3)는 응답에 싣지 않는다. 두 호출은 상태를
바꾸지 않으므로 관측은 그 상태의 projection이며 독립 checker가 상태에서 직접 읽는다.

## 선택 검증

원 literal 29 edge의 pre/input→response/post/class/context_event/controls/consumed_parts를 비교하고 12 trace
51 occurrence를 모델 후상태로 연쇄 실행한다. 5 known-bad의 bad response/post가 모델 출력과 다른지 확인한다.
literal 29 상태와 결정적 walk의 모든 raw1533이 모델 오류 없이 끝나고 후상태가 codec loader 검산을 통과하는지
본다. literal 밖 family는 원문에서 손으로 유도한 고정 trace(복구·fresh reopen·same reopen·commit 연속·atomic
commit·Bind·CloseStatement 재사용·FencedClose·PeerFence)로 확인한다. 응답·후상태의 정답 판정은
[독립 checker](dev4-lifecycle-checker.md)와의 차등 대조가 맡으며 FE14 완주가 아니다.

## 남은 의무

alias, BFS·checkpoint, FE14 완주와 mutant 공간 탐색은 후속이다. 전체 BFS에는 10GiB 영속 공간이
필요하다.
