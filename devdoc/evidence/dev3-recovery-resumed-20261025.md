# DEV3 중단된 plan4 복구 작업 재개

2026-10-25 · **중단 단위 완료 / 필수50 source별 PASS / 독립 D1·D2 한정 수용 / 전체 DEV3·S5 미완료**.

사용자의 “진행중에 중단된 작업 만 완료” 지시에 따라
[중간 저장](dev3-recovery-paused-20261025.md)의 복구 제품·독립 시험·검토 작업을 재개한다.
기준 commit은 `f22e956`이며 별도 DEV2/COMM/DEV4 작업은 이 단위에 포함하지 않는다.
[복구 owner 계약](../contracts/dev3-plan4-recovery-owner.md)을 따른다.
개별 ID·실패 이력·source/ELF/input과 원 receipt SHA는 [기계 색인](dev3-recovery-resumed-20261025.json)에 있다.

## 범위와 담당

총괄 `/root`는 source 고정·빌드·선택 실행·native worker·문서·통합을 맡는다.
`/root/recovery_implementation`은 복구 제품 보완과 native controller,
`/root/recovery_tests`는 미완성 독립 기대값·선택/장애 사례,
`/root/recovery_review`는 별도 D1/D2 검토를 맡는다. 파일별 소유와 예산은 로컬
`artifacts/dev3/plan4-recovery-resumed-20261025/resume-scope.json`에 기록한다.

중단 때의 static2 FAIL·F05 HOLD와 최신 source 미실행 기록은 그대로 보존한다.
직전 fresh75와 pure272의 PASS는 원 source 범위이며 다시 실행하지 않는다.

## 제품 수정과 static 선택

sealed prefix의 offset/LSN과 digest 오류를 `OrderMismatch`/`ContentMismatch`로 분리했다.
Pending home이 이미 같거나 최신이라 쓰기를 생략한 경로에서도 실제 파일 sync·재검산을
거친 뒤 Empty를 공개하도록 보완했다. 사용한 caller scratch와 기존 자원 한도는 유지한다.

원 실패 뒤 bind/replay의 실제 함수 수명 분리와 위 변경에 대응하는 static2만 선택했다.
`static-r0`는 현재 제품 후보와 기준 commit의 시험 소스를 고정한 별도 export다.
source `05187d64be7cf27f79e533f3292ae3c119f0d88e021cb0bd8256932ded72609b`,
ELF `bb513fb924998c2eccc7b83090489e72e52db37b8a78fc1826954e13c01e9a8f`이며
build·원 stdout/stderr와 결과는 위 로컬 단위의 `static-r0/`에 보존한다.

| 실제 타입 | local 최대 | bind | metadata | whole |
| --- | ---: | ---: | ---: | ---: |
| memory |65320|64491|93624|159160|
| Linux native |65456|64627|93760|159296|

source-value 한도65,536/196,608/262,144byte를 통과했다. caller backing·compiler stack·RSS와
구분하며 adapter 생성·acquire·read/write/sync·spawn은0이다. 결과 SHA는
`bacc4946b2ecbc5d76514ae1005c689cc6600c1f084075f9de7f2385a8031f12`다.
독립 검토 `review/f05-static-r0.json`은 source·ELF·receipt를 직접 재해시해 이 정적 범위만 수용했다.
별도 의미/native 입력과 실행을 대신하지 않는다.

## 선택 실행과 원 실패 보존

원48에 consumed-frame 변조 E09와 sealed 경계 오류 구분 E10을 더한 필수50은
static2/memory45/native3이다. ATT·Aging 전체 bank, 예약 credit의 한 번 복원,
base/control의 전체 bytes, 원 pool의 bind 및 live 인계, 실제 cleanup·flush·재진입을
검산한다. memory의 파일·event 비교는 프로세스 안에서 수행하고 marker/stdout을 보존한다.
전체 파일 snapshot 보존은 실제 native 실행에 해당한다.

| 실행 source | 결과와 범위 |
| --- | --- |
| `static-r0` | 위 static2 PASS; adapter/I/O 효과0 |
| `semantic-r0` | 시험용 `RetentionPhase` import 누락으로 compile FAIL; runtime0 |
| `semantic-r1` | import 보완 뒤 compile PASS; runtime0 |
| `semantic-r2` | B4 PASS 뒤 L01의 독립 epoch 예상값 FAIL. 별도 선택으로 V4/S15/P6/E9 PASS, 합38 고유 PASS |
| `semantic-r3` | XCP UUID 위치를 `8..24`에서 명세의 `16..32`로 수정한 L6와 신규 E10 PASS. 완료38 제외 |
| `final-r4` | native worker·controller의 자원/종료 oracle 보완 뒤 N01–N03 PASS. static2/memory45 반복0 |

`semantic-r2` source는 `e362ff2990afe9a384a142a9ce876c3cdc717a24e5c0beee6473d6a152073675`,
ELF는 `144036a89b6bac723302ac821db34598f4aa12a82465f7fba2993b5a146819be`다.
`semantic-r3` source는 `1909f4dd63d89c591e91feb75c206e5c0bd84a3526966cf92f952c4d5a987265`,
ELF는 `be38634fe2e28cfe28725c9bfe87e4a4468b6517c1bf99a10e738c430f0f5cb4`다.
독립 lineage는 제품·타입 수명이 바뀌지 않은 static2를 원 증거로 연결하며 새 source의 재실행 PASS로 표시하지 않는다.

새 native controller 자체 시험 HN01–09는9 PASS, engine process0이다. r3의 최초 native N01은
시험의 FD 한도5 assertion에서 FAIL, N02/N03은 NOT_RUN이다. 원 trace의 최대 File은3이고
플랫폼 소스는 caller directory1+lease directory1+flock1+File3=FD6을 요구한다. 이는 원 source
설계의6과 일치한다. Writer wrapper Drop 뒤 남은 File은 원 lease Arc를 유지하므로 마지막
File close만 허용하는 oracle로 보완한다. 원 FAIL과22,204,832byte snapshot을 보존한다.

r3 Clippy는 시험 소스의 인접 `if` 줄바꿈과 non-Drop flusher의 명시 `drop` 두 건에서 FAIL했다.
후속은 동일 지점의 lexical scope 종료와 줄바꿈만 바꾸며 완료 S/V 시험을 반복하지 않는다.
문서 자동 검사는 push gate에 남기고 이 작업에서 실행하지 않는다.

## Native 후속과 최종 검증

`final-r4` source는 `ee40b448904aeba355192abed146011fbff46ea645f64f527e4559ce8fc2827d`,
ELF는 `688511fd4f32230d27f90b5a153adec6dd44142c300d14a6545b3475ca0a0389`다.
같은 input `5e68a22a62e576f81c7011ff99777ec24c8250461535118c826643635e2c8e3b`로
서로 다른 PID67576/67585/67645의 producer/recover/reentry를 실행했다. 각 단계 File peak3,
실제 DB FD peak6, 종료 시 File0이며 entropy는1/0/0이다. 재진입 home write0을 trace로 확인했다.
새 epoch·base·control 공개는 정상이며 전체 쓰기0으로 표시하지 않는다.

원 실패를 포함한 native 누적은10.891초(worker7.731015초/감사3.159985초), 보존 DB 이력은
117,207,592byte로30초·worker29초·128MiB 한도를 지켰다. 원 inventory의 최종 감사 예약1초를
매 호출 전에 남겼으며 사전/단계 사이/실패 감사도 전체30초에 합산했다. 최초 controller가
이 예약을 모든 감사의 상한으로 구현했던 부분도 수정했으며 원 실패를 새 PASS로 바꾸지 않았다.

첫 실패의 working DB와 snapshot15개를 원 inode·길이·SHA와 전체 stream bytes로 두 담당이
각각 대조한 후, snapshot을 보존하고 동일한 working 사본22,204,832byte만 제거했다.
`native-duplicate-retirement-r0.json`은 독립 검토·제거 경로·원 identity를 보존한다.
재시도는 원 결과와 보존24개 파일·제거 receipt를 gate에 묶었으며 완료된 native ID는0개였음을
확인한 뒤 실패한 N01과 미실행 N02/N03만 선택했다. 최종 native 결과 SHA는
`6bdbe9d8da60c5d1eeb3cd99843a4f8d5c698713ae05acda703f5a1468f133e6`다.

controller 후속은 변경 HN02/04/06/07/08/09와 신규 HN10–14의11개 PASS다.
무변경 HN01/03/05의 원 결과를 유지하므로 도구 시험은 source별 고유14개, engine process0이다.
최종 engine all-targets/all-features Clippy와 변경 Rust 파일 fmt를 통과했다. 이들은 compile/style
검사이며 제품 시험 재실행이 아니다. 원 compile·fmt·Clippy 실패는 모두 별도 receipt에 남겼다.

정적2+memory45+native3의 source별 고유50 PASS이며 성공한 제품 ID 재실행은0이다.
새 실행의 L01/N01 실패2건과 원 중단 때 static2 FAIL·F05 HOLD는 별도로 보존한다.
일반 Undo·행 거래·Catalog CREATE/CLR·확대 profile/crash inventory·전체 DEV3/S5는 이번 중단
단위 범위가 아니다. 구현자와 다른 `/root/recovery_review`가 전체 source·계약·입력·원 로그·결과·
문서를 검토하고 최종 독립 D1·D2를 한정 수용했다. 로컬 `review/final-d1-d2-r4.json`의 SHA는
`f2d29c189c5446aba2c5f4b7d210820aa94c0f3c41341704700a9a71f52f0d24`이며,
`review/native-final-r4-readback.json`은 보존84개 파일과 실제 trace·자원을 별도로 대조했다.

최종 준비 공간의 고유 inode 합계는621,196,916byte로1GiB 안이며, source146,007,874,
ELF244,862,312, cache111,270,656, native117,207,592, records1,848,482byte다.
각 개별 한도와 외부 pinned fixture5,800,420byte도 지켰고 최종 기록 여유64KiB를 남겼다.
후속 독립 재계수에서는 검토 기록 증가를 포함한621,210,167byte를 확인했으며 한도 안이다.
`final-preparation-audit.json`과 source별 lineage는 기계 색인에 연결한다. 이 완료는 사용자 지시의
중단된 복구 구현·독립 시험·검토 세 역할에 한정되며 별도 DEV2/DEV4 작업은 포함하지 않는다.
