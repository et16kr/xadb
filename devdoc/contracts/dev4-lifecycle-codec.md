# DEV4 P5 수명 모델의 상태·입력 codec

2026-10-24 · **D4-LM-CODEC-001 ACCEPTED / 첫 schema·codec·독립 literal reader 구현·선택 검증·제한 D1 수용 / FE14 미완료**.

[선행 계획](../plans/dev4-preparation-plan.md#sequence)의 P5, 4.16.1.2·4.16.3.2를 위한
시험 전용 표현이다. [SQL 수명 계약](dev4-sql-design.md)과
[독립 앞단 계약](dev4-independent-frontend.md)의 실제 session/engine 의미를 대신하지 않는다.

## 결정과 범위

D4-LM-CODEC-001은 기존 사용자 위임 아래 내부 모델 형식과 검증 입력을 구체화한 D2다.
계약·통합 담당은 `/root`, wire 구현은 `/root/frontend_research`, 독립 입력은
`/root/frontend_oracle`, 독립 검토는 `/root/frontend_review`다. 2026-10-24 최종 D2에서
통합 의미·wire·자원·checkpoint 입력의 차단 모순을 해소하고 **현재 첫 자료형·state/input
codec·test-only literal reader만** 구현 범위로 수용했다. 후속 raw selector/index mapping,
checker API, 모델 전이·BFS·파일 재개는 해당 입력 동결 뒤 별도 구현한다.

일반 JSON이나 크기가 변하는 중첩 collection을 native 상태로 사용하는 대안 대신 bounded
flat record를 선택한다. 원 identity와 byte 상한을 보존하고 실제 capacity를 계산하기 위해서다.
실행 중 schema 생성이나 생산 SQL/engine 함수의 oracle 재사용은 없다. 형식·profile 변경,
필수 거절 누락이나 native 상한 초과는 재검토 조건이다. 채택은 시험 PASS가 아니다.

검토 원문은 로컬 `artifacts/dev4-lifecycle-model-20261024/review-final-d2-v1.md/.json`에
보존한다. JSON SHA-256은 `60a37149f45dd665494623c9c9aaa1c35bfa5a8692b3ac771dff280c5a1ab703`다.
owner manifest `owner-input-manifest-v1.json`의 SHA는
`2697d5dafcb76676af9de4156d16f1107b7800fdd66f58890e923242b47e4870`이며 입력26개·독립 fixture48개·
선행 리뷰13개의 실제 hash 일치를 확인했다. 원 후보·FAIL·이전 revision은 수정하지 않는다.

## 정본 입력과 실제 byte

[wire 필드·tag fixture](../../tests/fixtures/dev4-lifecycle-model/wire-schema.json)의 SHA는
`2edefc28473581f8e263c2c74db90a247c0973811787aed73bd26f7679ced1ec`다. 원 파일의 PROPOSED는
작성 당시의 상태이며 파일을 바꾸지 않고 이 결정의 첫 codec 범위에서 채택한다.
[독립 논리 literal](../../tests/fixtures/dev4-lifecycle-model/literals.json)과
[입력 보존 기록](../evidence/dev4-lifecycle-inputs-20261024.md)은 원 identity를 유지한다.

state는 `XADBLM02`/version2, header256·trailer SHA256 32bytes, 고정16section이다.
resolved input은 별도 version1/Env1이다. 정수는 little endian, padding 없음, bool은0/1,
reserved는0, Option은 absent tag 한 byte 또는 present tag+payload다. 길이에 자기 prefix를
포함하며 남거나 잘린 byte를 거절한다. 모르는 tag·reserved ReturnCode26/27도 거절한다.

정의 identity는 kind/actor/slot/engine/session/nonce/generation을 보존한다. 정렬은
`(kind,engine,session,actor,slot,nonce,generation)`의 숫자 순서다. Ref는 종류별 원 nonce이며
정렬된 배열 위치가 아니다. identity 재번호·alpha rename·old nonce 병합은 없다. raw input의
잘못된 kind/reserved/foreign target은 정상 state identity로 바꾸지 않는다.

Completed context는 exact ID와 issue ordinal만 가진21bytes다. ParkedFetch39/Commit35bytes는
원 API·대상·거래·deadline와 별도 reached/cancel을 보존한다. Completed의 삭제된 payload를
zero로 되살리거나 과거 provenance를 거기에서 재구성하지 않는다. incoming context Ref는 유지한다.

state obligation membership은12bytes, owner membership은 Option에 따라11..19bytes다.
input membership은 full Identity16·ResourceKind를 쓴다. recovery negative의 누락/추가/외부/
중복·순서와 invalid UTF8 bind bytes는 reader가 고치지 않는다. 구조 parse와 API 거절,
환경 제외, semantic checker 실패는 별도 분류다.

## 소유와 자원

reference는 순수 자료형·codec을 소유한다. 실제 파일·clock·process·BFS 조립은 후속 controller
책임이다. 기존 workspace sha20.10.9를 reference normal dependency로, serde_json1.0.145를
독립 literal 시험의 dev dependency로 사용한다. 새 버전·production 의미 의존은 없다.

State는 최대487개 flat record를 고정 table에 보관하고 record 내부의 heap collection은 없다.
두 heap Workspace를 fallible 예약 후 재사용한다. state wire 최대11,574bytes와 native
`size_of/capacity`를 구별한다. Workspace당 native64KiB·전체 stack256KiB·탐색 전체 소유2GiB는
실제 구현 검증을 요구한다. 코드의 산술만으로 측정 PASS를 선언하지 않는다.

codec은 caller buffer에 쓰며 encode/decode 중 heap allocation을 하지 않는다. 실패한 출력
buffer나 부분 decode workspace는 미검산 자료로 취급하고 visited/checkpoint에 게시하지 않는다.
전체 canonical bytes equality를 생략하고 hash만으로 같다고 판단하지 않는다.

## 현재 loader의 필수 거절

형식·checksum·header identity·정렬·중복·범위와 exact typed Ref 검산 뒤 다음 관계도 현재
codec 단위에서 검산한다. 후속 semantic checker가 아직 없다는 이유로 생략하지 않는다.

- Completed trailing payload, Parked API/status/필수 target 불일치, actor당 복수 Parked
- dangling incoming context, duplicate transaction obligation key
- Held/Transferred의 current holder 부재, Released의 current holder 잔존
- current slot/descriptor 연결 불일치와 terminal diagnostic payload 불일치
- durable/no-write receipt의 anchor 불일치
- DrainedConfirmed settlement의 기존 peer obligation·Published fact·RW/Recovery release 연결

독립 checker는 이후 이 loader helper를 정답 계산에 공유하지 않고 추가 state 의미와
pre/input/response/post 관계를 검산한다. raw input reader는 framing·known container tag·count만
검사하고 환경 가능성은 실제 모델의 admission gate 뒤에서 판단하도록 원 negative를 남긴다.

## 선택 검증과 남은 인수

첫 단위는 독립 상태29개 reader, state4/input7의 exact byte·전체 field 대조, 잘림·길이·count·
tag·reserved·정렬·Ref·hash·header 변조, 위 loader 교차 관계 거절, raw negative 보존과 실제
native capacity를 검증한다. 신규/수정 코드의 선택 사례만 실행하고 기존 P1–P4·DEV3·COMM
시험을 반복하지 않는다. 전체 문서 검사는 pre-push에만 수행한다.

정상 모델의 두 session·각 prepared2·active result1·길이0..6·raw1533 및 LM-M1~M5는 유지한다.
완료 ledger의 checker 수는 success+rejection+excluded, 응답 전이 수는 success+rejection이다.
실패·미마감 호출은 원 forensic 기록에 남긴다. cap·저장 부족·중단·미도달은 INCONCLUSIVE이며
작은 literal/codec PASS로 전체 queue 완주를 대신하지 않는다. whole BFS는10GiB 영속 보존
공간을 확보한 뒤에만 실행한다. 실제 SQL/engine·ACK/내구성·DEV4/Q1은 별도 미완료다.
