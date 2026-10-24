# DEV4 P5 원시 action 번호·target 선택

2026-10-25 · **D4-LM-RAW-001 ACCEPTED / 구현·선택 시험·제한 D1 완료 / 전체 resolver·FE14 미완료**.

[선행 계획](../plans/dev4-preparation-plan.md)의 P5와 [첫 codec](dev4-lifecycle-codec.md)을
잇는 시험 전용 순수 기능이다. 실제 SQL/engine API나 handle 인증을 제공하지 않는다.

## 결정과 입력

기존 사용자 위임 아래 `/root`가 계약·통합을, `/root/frontend_research`가 구현을,
`/root/frontend_oracle`가 독립 입력·시험을 맡는다. 별도 `/root/frontend_review`가
2026-10-24 최종 D2에서 아래 세 API만 수용했다. 고정 raw1533 분모를 줄이지 않고
전체 ResolvedInput 조립과 모델 실행에서 먼저 쓸 수 있는 작은 기능으로 분리했다.

- `action_at(u16)`: 0..1532를 전체 RawAction으로 해석
- `index_of(&RawAction)`: 유효한 정규 필드의 원 번호를 반환
- `resolve_selector(&State, actor, kind, selector)`: 정확한 RawIdentity16을 선택

검토 원문은 로컬 `artifacts/dev4-lifecycle-model-20261024/review-raw-selector-d2-v1.md/.json`이다.
검토 JSON SHA는 `acdc3df977263b49eebdfa823489b4d43ab9ccf0d5041377fa7cffaf3c8d5794`,
실제33입력 hash를 대조한 `raw-selector-owner-freeze-v1.json` SHA는
`bc2fdd1986bbf4b7e3b66f9817e58354ebedad13d296e2f12666633e2ad6e076`이다.
owner scope v2가 연구 v1의 복합 오류 순서·비교30쌍·51action 후속 구분을 명시한다.
채택과 정적 자료 작성은 실행 PASS가 아니다.

## 번호와 정확 selector

family는 기존 Api tag0..20 순이다. 차원은 actor → slot/selector → shape → N → outcome이며
오른쪽이 빠르다. 없는 차원은 생략하고 SetAutocommit은 actor → mode → outcome이다.
Prepare의 N은 shape 뒤이며 Bind Count0=2·Count2=10, Resume outcome은 작업5×cleanup2의
순서를 유지한다. Recovery actor는255, Inspect outcome은0, unused 선택 필드는 None이다.
범위 밖 번호는 OutOfRange, 잘못된 RawAction 필드는 InvalidFields다. modulo 재순환은 없다.

[독립 번호 입력](../../tests/fixtures/dev4-lifecycle-raw-selection/raw-descriptors.json)은
1533개 전체 tuple을 고정한다. 양끝42·내부 pivot12는 그 안의 coverage label이며 추가 실행이 아니다.

Issued0..5는 요청 종류의 **issue_ordinal=k+1**이다. table 위치·현재 actor·현재 token으로
바꾸지 않고 retired/foreign/과거 session·engine을 보존한다. 범위·중복 등 국소 검산 뒤
요청 ordinal이 없으면 gap도 포함해 U0=16 zero bytes다. ordinal 연속성은 여기서 검사하지 않는다.

FrozenUnissued0..9는 고정 wrong-kind·foreign engine/session·slot2·generation255·각 slot의
다음 generation·zero generation·kind254를 포함한다. E/S는 caller ActorRecord의 값이다.
Context U6/U7은 고정 generation1/2이며 slot을 읽지 않는다. U1은 Context 요청에도
Statement/slot0/generation1이다. malformed raw identity를 정상 state identity로 고치지 않는다.

## 읽는 범위와 오류 순서

전체 State의 도달성이나 불변식은 caller의 별도 검산 책임이다. 공개 unvalidated 자료형에는
아래 필요한 구조만 검사하고 오류를 반환한다. unrelated table을 추가 검사하지 않는다.

1. InvalidActor → InvalidSelector → 요청 actor의 MissingActor/DuplicateActor → caller E/S 범위.
2. Issued: 선택 namespace의 전체 primitive 범위 → 전체 DuplicateOrdinal → 전체 DuplicateNonce
   → selected-kind next_nonce=max retained nonce+1 → ordinal 검색.
3. handle U6/U7: 요청 caller/slot의 MissingSlot/DuplicateSlot → 해당 kind generation1..13.

Actor E/S는1..7/1..8이다. Issued는 ordinal·nonce1..6, actor0/1, E/S1..7/1..8,
handle slot0/1·generation1..12 또는 Context slot255·generation0이다. 원 kind를 검산한다.
Engine/Session table lookup과 model/codec/checker 호출은0이다. issued duplicate 비교는
ordinal15쌍과 nonce15쌍의 최대30쌍이다. heap allocation·State clone 없이 작은 값과 참조만 쓴다.
실제 stack 실행과 allocator 계측은 정적 산식과 구별한다. 구조 오류를 EnvExcluded로 바꾸지 않는다.

## 독립 입력과 후속 의무

[정적 manifest](../../tests/fixtures/dev4-lifecycle-raw-selection/manifest.json)의 SHA는
`bc1de24bd9c61ac7e485bffeb3b5c2dff94bf4a0882ddc15c30d106299fe9eba`다.
[선택 inventory](../../tests/fixtures/dev4-lifecycle-raw-selection/selection-inventory.json)는
논리1951개·예정 호출3484회(action_at1536/index_of1656/selector292)를 구분한다.
원 v1/v2 후보와 조기 U0 반환·오류 순위 witness 공백을 보존하고, v3에서 신규6개와
기존5개 입력 배치를 보완했다. 원 기대값 변경·삭제는 없다.

baseline96은 기존 initial literal을, 나머지 selector는 명시적인 read-set fixture를 사용한다.
후자는 필요한 필드만 고정하며 full-state invariant 통과나 모델 도달성을 주장하지 않는다.
독립 기대값을 제품 함수·codec·모델 결과에서 만들지 않는다. 새 함수의 선택 시험만 실행하고
완료 codec·P1–P4·DEV3·COMM 시험을 반복하지 않는다. 세부 실행은
[source별 기록](../evidence/dev4-lifecycle-raw-selection-20261025.md)에 남긴다.

full input의 context/control/Bind/Env1·owner·receipt 조립, mandatory51action 연결,
admission·alias·EnvExcluded, 독립 relation checker, 모델 전이·BFS·FE14는 후속이다.
target bytes가 같다는 것만으로 full input alias를 선언하지 않는다. 전체 모델·실제
SQL/engine·내구성·DEV4/Q1 인수를 이 단위의 결과로 대신하지 않는다.
