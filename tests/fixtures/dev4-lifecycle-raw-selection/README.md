# P5 raw mapping·selector 독립 정적 입력 v3

2026-10-24 · frontend_oracle · 최종 D2 검토 후보. 제품·compile·test·model 실행0.

이번 입력은 `action_at`, `index_of`, `resolve_selector`만 다룬다. 원 v1의125개 제안은
합산하지 않고 보존했다. 자료화 직전 v2 JSON의 byte-identical 사본이 recipe-source.json이다.
authoring.py는 그 안의 수기로 고정한 축과 tuple을 JSON으로 펼친 정적 작성 도구다.
제품·모델·codec·research의 expanded inventory를 import하거나 기대값으로 호출하지 않는다.
원 codec 시험을 실행하거나 그 PASS를 새 resolver 증거로 전용하지 않는다.

입력 v1/v2 디렉터리는 각각 byte불변으로 보존했다. v2의 독립 검토에서 FrozenU0를 caller guard 전에
반환하는 결함을 기존 입력이 검출하지 못함을 확인하여 정확히 세 local error를 추가했다.
FrozenUnissued(0)+actor2는 InvalidActor, actor0 부재는 MissingActor, actor0 engine0은
InvalidCallerScope다. v3은 여기에 SC06 종류별3개를 추가했다. row1(o1,n1), row2(o2,n1),
row3(o2,n2), next3에서 앞 nonce 중복보다 뒤 ordinal 중복을 먼저 반환해야 한다.
SC09 기존4개는 첫 요청 slot의 generation을0, 뒤 duplicate의 generation은 정상으로
배치하여 DuplicateSlot이 InvalidGeneration보다 먼저임을 확인한다. SC02의 기존1개는
첫 actor E0/뒤 duplicate E1로 배치하여 DuplicateActor가 InvalidCallerScope보다 먼저임을
직접 확인한다. 원 v1 대비 신규6개, 기존5개 입력 배치만 교정했고 기존 expected는 모두
동일하다. 상세 변경은 correction.json에 있다.

| 그룹 | 논리 입력 | 예정 API 호출 |
| --- | ---: | ---: |
| raw1533 |1533|3066: action_at/index_of 각1533|
| invalid_index |3|action_at3|
| invalid_fields |123|index_of123|
| selector_baseline |96|96|
| selector_history |9|9|
| selector_namespace |12|12|
| local_errors |115|115|
| readset_control |60|60|
| 합계 |1951|3484|

42개 family endpoint와12개 pivot은1533 내부의 coverage label이며 추가 호출이 아니다.
selectors.json의292개 중 baseline96은 기존 완결 `initial` literal을 읽는다. 나머지는
명시적인 **selector read-set fixture**다. State::default에서 지정된 scalar/table만 대입하고
각 record는 해당 schema의 Default에 명시된 field를 덮어쓴다. next_nonce의 지정 kind만
대입한다. tuple 순서는 kind,actor,slot,engine,session,nonce,generation이다.
`context_data=Completed`는 ContextData::Completed, `ParkedFetchDefault`는
ContextData::ParkedFetch(ParkedContext::default())를 뜻한다.
`poison_default_record_tables`는 각 지정 table에 정확히 한 개의 Default record를 넣는다.
전체 상태 invariant·canonical order·model 도달성은 이 입력의 주장이 아니며 full loader나
codec validation을 selector 전에 호출하면 국소 read-set 계약을 바꾸게 된다.

history9는 table순서와 ordinal을 뒤집고 caller와 다른 actor/과거 E/S를 보존한다.
Statement는 Retired/SessionClose, Result는 EngineFence/public_generation_live=false다.
Context는 ParkedFetchDefault와 Completed를 함께 둔다. Engine/Session table은 비어 있고
caller E/S와 current_engine도 다르다. 기대값은 원 identity의16bytes 그대로다.
SC10은 다른 namespace·actor·slot·engine/session·owner/fact/receipt의 concrete poison,
SC11은 FrozenU0..9에서 selected history/next_nonce poison을 읽지 않는30개 통제,
SC12는 ContextU6/U7에서 empty/poisoned slot table을 읽지 않는4개 통제를 포함한다.

selection-inventory.json은 전역 고유 ID1951개와 순서를 보존한다. 원1945개 사이의 상대 순서는
보존하고 신규 입력을 SC01..03/SC06에 추가했다. 미래 harness는
명시 ID 또는 그룹의 half-open 범위로 선택하며 각 API 방향마다 START/COMPLETED를 남긴다.
첫 실패에서 FAIL을 보존하고 이후는 NOT_RUN이다. raw 한 case의 action_at만 완료했다면
index_of 실패 뒤 action_at을 반복하지 않는다. 별도 원본 receipt가 완료 방향을 보존한다.

첫 신규 함수 그룹 자체를256KiB thread에서 실행하는 방안은 구현 시 검토한다. JSON 해석은
thread 밖에서 하고 작은 제품 호출·비교만 안에서 수행하면 별도 stack 시험으로 완료 입력을
반복할 필요가 없다. 현재 stack 측정이나 allocator 계측은 없으며 heap0는 구현 후 정적
근거와 구분해서 기록한다.

전체 ResolvedInput의 context/control/Env1·owner·receipt 조립, alias/EnvExcluded,
mandatory12trace51action 연결, 독립 edge checker, 각 모델 상태의1533 slot 분류 및 BFS는
후속이다. 이3484회는 아직 한 번도 호출하지 않은 계획 분모다. authoring의 구조적 개수·ID
검사는 정적 입력 작성 확인이며 제품 PASS·FE14 공간 완주·P5 완료가 아니다.
