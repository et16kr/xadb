# DEV4 P5 전체 입력 조립

2026-10-25 · **D4-LM-ASM-001 독립 D2 수용 / 구현·선택 시험·독립 D1 수용 / 모델 전이·FE14 미완료**.

[선행 계획](../plans/dev4-preparation-plan.md#sequence)의 P5에서 [원시 번호·selector](dev4-lifecycle-raw-selection.md)와
[상태·입력 codec](dev4-lifecycle-codec.md)을 잇는 시험 전용 순수 기능이다. 실제 SQL/engine API,
admission, 환경 가능성, 전이, 응답, alias, BFS를 제공하지 않는다.

## 결정과 정본 입력

이 결정은 기존 준비 자료의 연구 제안·owner 선택·read-set·검토를 하나의 소유 계약으로 묶는다.
원문은 로컬 `artifacts/dev4-lifecycle-model-20261024/`에 보존한다. 아래 표는 조립기 원문만 정리한다.
뒤 문서는 앞 문서가 명시적으로 미결정·오기로 남긴 항목만 닫으며, 상위 정본인 identity-transition·
input-layout·codec 정렬 규칙을 바꾸지 않는다. 원 파일은 수정하지 않는다.

| 순위 | 원문 | 적용 범위 |
| --- | --- | --- |
| 1 | `wire-proposal-v2.md` §3·§4 | Env1 byte·class 고정 조합·family 선택표·fence census |
| 2 | `assembler-proposal-v1.md` §2–§8 | prefix/context/control·Env1·owner recipe·recovery mutation |
| 3 | `assembler-owner-scope-v2.md` | A1 Bind·A2 인증 실패·A3 기존 implicit·A4 상한·A5 decision 출처 |
| 4 | `assembler-readset-v2.md`, `-v3.md`, `assembler-owner-addendum-v3.md` | S0–S5 오류 순서·조건부 read-set·generation offset·CommitFact NoWrite·peer O 생략 |

준비 기록은 [입력 준비 증거](../evidence/dev4-lifecycle-assembly-inputs-20261025.md)이며 선택 검증은
[source별 증거](../evidence/dev4-lifecycle-assembler-20261025.md)에 둔다. 채택과 정적 작성은 실행 PASS가 아니다.

## 공개 경계

```rust
pub fn assemble_input(state: &State, action: &RawAction, out: &mut ResolvedInput)
    -> Result<(), AssemblyError>;
pub enum AssemblyError { InvalidAction(RawActionError), Selector(SelectorError),
    InvalidState, ProfileLimit, Capacity }
```

State는 항상 불변이다. 성공만 out을 유효하게 만들며 모든 scalar·Option·Bind·Env와 initialized
count를 교체한다. 오류의 out은 부분·미검산이므로 encode·비교·모델·게시에 쓰지 않는다. 오류는
API ReturnCode나 EnvExcluded가 아니다. 전체 State 유효성은 caller 전제이며 이 함수는 읽는 참조만
검사한다. 원 State clone·heap allocation·model/checker/codec 호출은 없다.

## 오류 단계

| 단계 | 검사 | 오류 |
| --- | --- | --- |
| S0 | `index_of(raw)`의 필드 | InvalidAction, State 읽기 전 |
| S1 | target family는 기존 `resolve_selector`, 그 외 caller 유일·E1..7/S1..8 | Selector 또는 InvalidState |
| S2 | continuation family의 pending 참조·종류·원 private 연결 | InvalidState, 정상 wrong-kind/absence는 unmatched |
| S3 | 인증한 P/R·descriptor·선택 T·owner/O/receipt 관계, old result 연결, symbolic birth | InvalidState |
| S4a | 사용 namespace를 IdentityKind 순서로 primitive→ordinal/nonce 중복→next=max+1, 선택 slot의 next_result_generation1..13 | InvalidState |
| S4b | 사용 후보 block의 cap: Engine7, Resource54(block 끝 nonce), 나머지6, Result generation12 | ProfileLimit |
| S5 | phase 순서 nonce 배정·정렬·count/summary·recovery negative·고정 출력 공간 | Capacity |

사용하지 않는 namespace는 읽지 않는다. selector가 S1에서 검사한 selected namespace는 그 동결
오류를 유지한다. 유효 저장 next=cap+1은 새 후보가 필요할 때만 ProfileLimit이다. identity primitive는
kind별 envelope와 nonce 범위만 보며 engine/session table을 조회하지 않는다.

새 Result 후보 generation은 인증한 Execute Success Select에서 `slot.next_result_generation +
retirement_offset`이다. slot descriptor가 없으면 offset0, 있으면 slot→descriptor→issued Result의
exact statement·actor·slot·E/S 연결과 두 public_generation_live 일치를 S3에서 검사한 뒤 live면1,
이미 retired면0이다. old descriptor의 Open/Eof/Error/Closed 상태는 offset을 바꾸지 않는다.

## prefix·context·control

Prepare만 slot/shape/N, SetAutocommit만 mode를 채우고 나머지는255다. fresh family는
`(Context, caller actor, slot255, attached E/S, next Context, gen0)` 후보를 쓴다. entry
Cancelled는 `(Absent,true,false)`, entry Timeout은 `(ReachedAtEntry,false,true)`이고 해당 outcome은
Prepare/Execute/CloseResult/CloseStatement/Begin/SetAutocommit1/2, BindAll/CommitAtomic8/9,
Fetch3/4, Rollback/CloseSession2/3이다. ParkFetch/BeginCommit은 outcome1만 FutureAtEntry다.
Reopen/Inspect/Signal/Recovery는 context None·control0이다. Resume은 matching ParkedFetch,
CommitFact/Finish는 matching ParkedCommit의 원 context와 deadline·cancel·reached를 복사하며
불일치·부재는 context None·control0이다. 다른 종류의 payload는 읽거나 복사하지 않는다.

Env는 entry class와 Prepare/Bind/Begin/Set/Park/BeginCommit/Inspect/Signal/Reopen에서 None이다.
CloseResult/CloseStatement Normal은 LocalCleanup이다. Fact 후보는 caller attached E(Recovery는
current engine), next Fact다. Env가 있는 모든 raw class는 미소비 fallback이라도 Fact를 쓴다.
Recovery의 Env transaction은 None이고 fresh_engine은 `(Engine, engine=nonce=next Engine)`이다.

## Bind payload (A1)

SuccessA/B와 entry control만 인증한 statement의 parameter_count를 본다. N1이면 A·entry=Utf8 `61`,
B=Utf8 `62`, N0·미인증이면 empty다. Count0 empty, Type Int64 `0100000000000000`, NotNull
TypedNullUtf8 길이0, Range Utf8 `6161`, SyntaxUtf8 Utf8 `ff`, Limit Utf8 `61`, Count2는
Utf8 `61`,`62`이다. N0의 unsupported class를 조립기가 제외하지 않는다.

## 인증·continuation과 transaction 선택

statement 인증은 같은 caller의 exact issued identity, Current 또는 CurrentClosed, slot backlink다.
result 인증은 같은 caller의 exact public current result와 descriptor·statement·slot 연결이다.
인증한 record의 깨진 연결은 InvalidState다. 인증은 admission이 아니며 Busy/Fenced/cached를
예측해 입력을 비우지 않는다. matching ParkedFetch는 fence로 public result가 retired되어도 원 private
statement·result·descriptor·transaction 연결과 T.parked_context 역참조를 검사해 사용한다.

A2: retired·foreign·unissued·malformed target의 인증 실패와 continuation의 부재·다른 종류는
transaction None, owner/obligation empty, census 없음이다. raw class·fact·명시 receipt fallback은
남는다. 인증 대상이 없는 direct Rollback·CloseSession과 CommitAtomic Incomplete/Unknown/
PublicationPending은 caller T가 None이어도 census를 수행한다.

Execute는 인증 뒤 actor.transaction, 없으면 ON Select=implicit RO, ON Dml=implicit RW,
OFF=OffStarted RW의 prospective `(Transaction, caller scope, next Transaction)`이다. Fetch/
CloseResult는 descriptor.transaction, CloseStatement는 slot descriptor의 원 transaction,
Commit/Rollback/CloseSession은 actor.transaction, continuation은 Parked.transaction이다.

## Env1 class

class 조합은 wire-v2 §3 표를 따른다. Resume work class는 Rows0·Eof1·ReadError2·Conflict3·
RowBufferTooSmall4(NextWork−1)이며 무신호 Resume은 Fetch 대응 recipe다. 무신호 Rows/RowBufferTooSmall은
raw cleanup이 Incomplete여도 owner empty이고 cleanup byte는 그대로다(proposal §7). pending 신호가 있으면
Complete는 C(result) release, Incomplete는 census다. 새 receipt는 선택
transaction의 원 actor/E/S, 없으면 caller scope이며 새 anchor는 receipt engine이다. CommitAtomic
Success의 changed는 선택 RW의 prior_successful_write이며 transaction이 없으면 no-write with identity다.
Finish Committed의 unmatched 또는 progress 불일치는 Published+기존 receipt(없으면 None)다.
기존 receipt는 T.receipt의 유일 row·transaction·engine·scope와 kind별 anchor shape를 검사한다.

A3에 따라 Execute Success Dml은 선택 T가 ImplicitReadWrite/RW이면 prospective·existing 모두
Published와 새 durable receipt·anchor다. existing이면 live RW(T)만 Released, 연결 O는 Committed,
Recovery는 제외한다. StalePlan/ValueError/TerminalCancelled/TerminalTimeout은 prospective implicit
RW만 Aborted와 RW3 birth+release, existing implicit RW는 owner0·commit255다.

## owner·obligation recipe

`C(R)`·`L(P)`·`RW(T)`·`R(T)`·`O(E)`와 family별 union은 proposal §7·wire §4 표를 따른다.
선택은 pre-state의 original holder·kind·disposition으로 하며 같은 holder의 같은 kind live owner가
둘이면 InvalidState다. before는 current holder의 full identity, Released after는 None이다.
같은 resource에 서로 다른 최종 요구가 생기면 InvalidState다. T에 기존 O가 있으면 그 T의
RW/Recovery row는 O를 연결하고 연결 row가 있을 때 O row를 포함한다. handoff·recovery는 기존 O를
요구하며 새 O를 만들지 않는다. CommitFact NoWrite3/4와 CommitAtomic WithoutIdentity1은 모두
live RW(T)를 release한다. wire-v2의 "WithoutIdentity는 None"은 receipt를 뜻한다.

Execute·Fetch·무신호 Resume의 Conflict Complete와 Rollback Complete는 RW(T)와 T를 가리키는
현재 Open descriptor의 cursor를 release한다. 이미 Eof/Error/Closed인 descriptor는 제외한다.
Incomplete는 cause의 cursor를 release하지 않는다.

fence census는 engine의 life≠Ended·access RW·progress None/BeforeAttempt/Attempted/DurableConfirmed
transaction과 이번 prospective RW를 **transaction full identity key 순서**로 처리한다
(identity-transition-v1의 "exact identity 숫자순", wire-v2의 정수 tuple 정렬). prospective는 caller
session·actor와 stored next nonce의 key로 놓인다. tuple(E,T,birth session)과
T.obligation이 일치하는 기존 O를 재사용하고 없으면 새 O를 그 순서로 번호 매긴다. 새 O는 유지 RW/Recovery
(before=after)와 새 Recovery birth를 모두 넣는다. 기존 O의 cause는 변경 row만 넣는 count0 원인 갱신이고,
변경 없는 peer 기존 O는 생략한다. peer Plan/Binding/cursor는 읽지 않는다. 반복 fence는 기존 O에
두 번째 Recovery를 발급하지 않으므로 기존 O에 live Recovery owner가 없으면 InvalidState다.

O의 known_decision(A5)은 row 역할을 먼저 정한 뒤 아래 순서로 고른다.

1. handoff(Finish NCR의 이미 Fenced 포함)·recovery는 기존 O 값이다.
2. publication 확인은 Committed다: Execute Success Dml의 existing implicit RW, CommitFact Published drain,
   아래 owner 결정의 CommitAtomic Success changed RW.
3. exact cause의 새 O·기존 O 원인 갱신은 raw class로 NCR→NotCommittedRecoveryRequired,
   Unknown→Unknown, PublicationPending→Committed다.
4. peer 새 O는 그 T.decision, 변경이 있는 peer 기존 O는 기존 O 값이다.
5. 그 밖의 새 decision 사실 없는 직접 release는 기존 O 값이다.

count/summary는 envelope 안의 연결 row만 계산한다. 새 resource nonce는 RW3→cursor3→Recovery
(census 순서) 배정 뒤 출력에서 full identity key로 정렬한다. obligation도 identity key 순서다.
Recovery의 MissingFirst(index0 삭제)/ExtraNeverIssued(first 복사·nonce255·끝 추가)/
ForeignEngineFirst(first의 engine255·제자리)/DuplicateFirst(first를 index1 삽입)는 정렬 뒤 obligation
vector에만 적용하며 재정렬·중복 제거하지 않는다. owner bytes는 Exact와 같다. O(E)가 empty면 원
outcome을 보존한 empty다.

## owner 결정

조립기 원문이 직접 정하지 않은 항목은 이 결정의 owner 선택으로 고정한다.

- CloseSession Complete는 engine admission이 Admitting일 때만 whole-abort 경로다. Fenced와
  RecoveryConfirmedRetired는 LocalReleaseAndHandoff(cleanup3·commit255·actor-local release와 origin
  session의 Outstanding O handoff)다. 회복된 engine에서 Aborted 결정을 새로 공급하지 않기 위해서다.
- Finish NCR은 wire-v2 문구대로 admission이 Fenced일 때만 cleanup3·handoff이며 Admitting과
  RecoveryConfirmedRetired는 cleanup2·census다. Fenced에서 parked T가 이미 끝나 O가 없으면
  (NoWriteValidated·Published) 넘길 O가 없으므로 같은 header를 row 없이 공급한다. 이 class는 진행 순서
  밖이라 모델이 제외한다(2026-10-25 [checker 차등 대조](../evidence/dev4-lifecycle-checker-20261025.md)에서
  도달 상태의 InvalidState로 발견). 조립 시험의 독립 case는 없고 `differential_tests`의 다섯째 고정 경로가
  이 분기의 시험 근거다.
- 무신호 Resume Eof+Incomplete는 wire-v2가 admission 뒤 제외할 조합이며 원문이 공급 recipe를 정하지
  않는다. 일반 Incomplete 규칙대로 census를 공급한다. ReadError/Conflict+Incomplete도 census다.
- CommitAtomic Success changed RW의 Published 공급은 publication 확인으로 보아 연결 기존 O를
  Committed로 둔다. no-write·validation abort·terminal control의 직접 release는 기존 O 값이다.

## 자원

입력 storage는 owner54·obligation7·Bind2다. 계획 저장소는 기존 owner54와 birth8을 그대로 합칠 수
있도록 owner62·obligation14·birth16의 고정 배열을 쓴다. 초과는 표식만 남기고 S3–S4 검사를 모두
통과한 뒤 S5 Capacity로 보고한다. owner54/O7은 container 한도이며 정상 도달 최대가 아니다.
제품 경로는 recursion·heap container·State clone이 없다. 신규 호출은 첫 실행부터 256KiB worker에서
실행하며 같은 사례의 release/stack 반복과 allocator 계측은 하지 않는다.

## 독립 입력과 남은 의무

입력은 [fixture 디렉터리](../../tests/fixtures/dev4-lifecycle-assembly/README.md)의 검토 후보1658,
준비 단계에서 미검토로 남은 rich304, 이번에 독립 작성한 supplement 입력이며 원 파일은 byte 동일하게
보존한다. rich 후보의 정정은 별도 `rich/correction.json`에 원 기대 hash·근거와 함께 두고 시험이 원 hash를
확인한 뒤 해당 기대값만 대체한다. 기대값은 제품·모델·codec 출력으로 만들지 않는다. 신규 함수의
선택 시험만 실행한다.

admission·환경 가능성·소비, full input alias, 독립 edge checker, 원51 action model replay,
전체 BFS·재개와 FE14는 후속이다. 전체 BFS의 10GiB 영속 공간 요구는 미충족이다.
