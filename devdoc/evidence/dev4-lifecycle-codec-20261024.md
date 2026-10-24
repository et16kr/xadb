# DEV4 P5 수명 상태·입력 codec의 선택 검증

2026-10-24 · **첫 schema·codec·독립 literal reader 구현·선택 검증 / 최종 독립 D1 제한 수용 / 모델·FE14 미완료**.

[codec 계약](../contracts/dev4-lifecycle-codec.md) D4-LM-CODEC-001과
[기존 독립 입력 보존](dev4-lifecycle-inputs-20261024.md)을 연결했다. 계약·공통 자료형·통합은
`/root`, codec은 `/root/frontend_research`, 독립 reader는 `/root/frontend_oracle`, 별도 검토는
`/root/frontend_review`가 맡았다. DEV3/COMM의 동시 작업은 수정 범위에서 제외했다.

## 구현 범위와 D2

`crates/testing/reference/src/lifecycle`에 flat state·typed nonce·raw input, caller buffer를 쓰는
state/input codec과 필수 교차 참조 검산을 추가했다. test-only reader는 원 논리 JSON을 직접
펼쳐 typed expected를 만든다. codec/model 결과를 golden으로 사용하지 않는다. 파일·시간·
process·BFS나 실제 SQL/engine adapter는 추가하지 않았다.

최종 D2 입력의 실제 hash를 대조해 owner manifest를 동결했다. wire-v2·projection B·identity·
resource와 checkpoint의 관계를 수용하되 현재 구현 GO는 첫 codec 단위다. 후속 raw selector/
index mapping·checker API·모델 전이·BFS 입력은 별도 선행으로 남겼다. 원 입력·checkpoint-v1
검토의 수정 요구와 후속 보완을 보존하며 채택을 실행 PASS로 분류하지 않는다.

원 47개 정적 fixture와 manifest는 변경하지 않았다. 실행 시 Git 내보내기에서 읽을 수 있도록
원 authoring manifest/selection의 동일 사본 두 개를 추가했다.
[vector manifest](../../tests/fixtures/dev4-lifecycle-model/independent-vector-manifest.json)의 SHA는
`e58a94580d9f1edcababa0a36458582c70b798a4346a6d36f309391deec25cad`,
[선택 입력](../../tests/fixtures/dev4-lifecycle-model/vector-selection.json)은
`8dc9e4325150f53663ed4d5e11426da2e7270675b43e59dc3220a598b0d42911`이다.
원 fixture의 작성 당시 상태 표시는 보존하고 이 두 사본을 새 의미 기대값으로 만들지 않았다.

## 실제 source별 실행

원자료는 `artifacts/dev4-lifecycle-model-20261024/` 아래에 있다. 각 실행은 선택 목록·소스
사본/hash·컴파일 로그·실제 ELF와 stdout/stderr를 보존한다. 단일 신규 함수씩 정확히 실행하고
실패에서 멈췄다. 완료한 무변경 함수와 prefix를 반복하지 않았다.

| 실행 | 선택과 실제 결과 | 보존·한계 |
| --- | --- | --- |
| `schema-compile-r1` | 새 enum 변환 compile FAIL | `ResultState::Error`와 연관 타입 `Error` 모호성 원 소스·로그 보존 |
| `schema-compile-r2` | 명시적 `InvalidTag` 반환으로 compile PASS | runtime 실행0 |
| `workspace-r1` | 새 workspace 함수1 PASS | native State10,536bytes·두 heap capacity21,072bytes·256KiB thread stack에서 생성/324 membership copy/재사용·예산−1 거절. 실제 allocator exhaustion은 주장하지 않음 |
| `codec-compile-r1` | 신규 codec/교차 검산 compile PASS | runtime 실행0 |
| `codec-suite-r1` | 독립 reader29상태 함수1 PASS; state byte 함수 FAIL; 나머지12 NOT_RUN | 정상 ParkedFetch를 Commit continuation으로만 허용한 결함. 원 실패·ELF·source 유지 |
| `codec-suite-r2` | 실패 경로 수정 후 선택13함수 PASS | state vector는 start1로 남은3개만 encode/decode·전체 field 대조. input7개 양방향·reader cycle3/정상1·codec10함수의 오류/경계/보존 사례 |
| `quality-r1` | 해당 reference lib/tests Clippy `-D warnings` PASS; fmt FAIL | 테스트 target은 compile만 하고 사례 실행0. 두 module 선언 순서의 원 fmt 출력 보존 |
| `quality-r2` | 선언 순서만 정리한 두 파일 fmt PASS | runtime/Clippy 반복0; 나머지 formatter 결과는 원 r1 범위로 유지 |

state4개 중 initial의 encode/decode·field 비교는 r1 실패 함수의 완료 prefix이며 새 r2 결과에
합산하지 않는다. 원 로그는 함수 단위다. 보존된 case 순서·고정 입력·유일한 parked transaction
링크와 실패 지점에서 initial 완료 후 두 번째 vector 실패였음을 귀속한 근거를
`codec-suite-r1/failure-scope.json`에 기록했다. r2는 정확 start1과 각 완료 ID를 출력한다.
실패 함수를 소급 PASS로 바꾸지 않았으며 reader29상태·workspace의 완료 함수도 재실행하지 않았다.

codec10함수의 정적 오류 inventory는557개 거절 입력과 정상 통제를 포함한다. 전체 함수가
r2에서 통과했으며 raw malformed target·invalid UTF8·recovery 순서/중복을 보존하고, 길이·tag·
reserved·Ref·Completed/Parked·slot/terminal·receipt/owner·fact membership·Drained 연결과
caller buffer를 검산한다. 이 숫자는 모델 전이 수나 도달 상태 수가 아니다. 작성자의 최초
558개 표기는 독립 D1 정적 재계수에서 input cap33개를 확인해557개로 정정했다. 시험 입력·
결과는 바꾸지 않고 정정 산식과 원 표기를 `codec-inventory-correction-v1.json`에 보존했다.

## 검토 중 보완과 원 실패

독립 검토의 `artifacts/` 실행 입력 의존을 체크인 fixture로 옮겼고, Open public descriptor가
ResultOpen slot에 연결되어야 하는 역방향 검사를 추가했다. 정상 EOF/Error/Closed와 fence 뒤
private descriptor를 함께 통제했다. 독립 reader의 envelope 참조 순환도 base/value/envelope의
같은 참조 stack으로 검산한다. 원 후보는 `literal-pre-export-fix`, `codec-pre-d1-open-slot`,
`literal-pre-envelope-cycle`에 보존했다. 첫 항목은 정확 경로 수정의 역변환으로 복원한 bytes가
작성자의 원 SHA와 일치함을 명시했다. 정적 지적을 실제 실행 실패라고 표현하지 않는다.

r1의 실제 ParkedFetch 실패 뒤 transaction/context 양방향 검산을 Fetch와 Commit 양쪽에
적용했다. 독립 기대값은 수정하지 않고 직접 오류 경계5개를 추가했다. source별 결과와
선택의 실제 diff 대응을 유지했다. 최종 독립 D1은 이 변경·원 실패·선택 증거와 정확26경로를
확인한 뒤 첫 codec leaf를 제한 수용했다. `review-codec-d1-v1.json` SHA는
`39a49c6ada281959a4c0658c83bdedb7167f1770fc4086d0d93e2ae7f5257801`이다.
원 source snapshot358항목·보존 ELF3개를 대조했고 실행은 live checkout/shared target임을
명시했다. 최종 module 선언 순서만 바뀐 차이를 새 runtime 실행으로 표시하지 않는다.

## 미완료 항목

workspace의 예산 거절은 실행 확인했으나 실제 `try_reserve_exact`의 allocator 오류 경로는
**NOT_RUN**이다. 오류를 `WorkspaceError::Allocation`으로 직접 전달하고 지역 Vec를 해제하는
경로는 정적으로 검토했다. 원 D2의 allocation 실패 확인 기대는 유지하며, 예산 거절을
allocator 실패 시험으로 바꾸지 않는다. 이 한계 아래 첫 codec leaf만 독립 D1의 통합 대상으로 삼는다.

이 기록은 수명 모델의 상태 표현과 입력 codec 범위다. raw1533 resolver·독립 relation checker·
실제 모델 전이·LM-M1~M5 검출·정상 길이0..6 queue 완주·filesystem checkpoint/process-cut은
미실행이다. 전체 BFS용10GiB 영속 보존 공간도 미충족이다. 작은 codec의 성공을 FE14·P5·
실제 SQL/engine·내구성·DEV4/Q1 완료로 승격하지 않는다. 문서 verifier·push는 실행하지 않았다.
