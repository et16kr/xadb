# DEV2 PF04 측정 profile B 도구(S1)

2026-10-26 · **S1 도구 구현·선택 검증·실제 연결 완료 / 독립 검토 r1 차단1건 수정·r2/r3 PASS_SCOPED_S1_TOOL / C3(S2–S4)·PF04 K0 인수(S5)·V3(S6) NOT_RUN / 영속 결정 대기 / 원 PF04 네 FAIL·DEV2 미완료 유지**

[채택 계획](../plans/dev2-pf04-resolution-plan.md#measurement-profile-b) · [source별 기록](dev2-pf04-profile-b-s1-20261026.json) · [K0-005 계약 등록](../implementation/dev2-implementation.md#k0-005-profile-b)

## 범위와 승인

사용자가 채택된 C3·profile B 계획의 S1(측정 profile B 도구 구현·선택 검증)부터 착수하도록 지시했다. 영속 처리(`PF04-DUR-001`)는 사용자가 따로 정한다. 결정 ID는 `PF04-PB-001`이다. 계획이 착수 때 받도록 정한 PMU 확인에서, 사용자는 `slot-env-v1`을 허용했다. 범위는 기존 `sudo -n → setpriv(CAP_PERFMON만) → /usr/bin/perf` 경로로 CPU2·CPU18의 `cycles`·`ref-cycles`만 slot의 GO→DONE 동안 계수하는 것이다.

작성·계약·통합은 이 Claude Code 세션이 맡았다. 작성자 외 D1/D2 검토자는 작성 맥락 없이 띄운 검토 subagent다. 제품 engine·worker·WAL·공개 형식과 guardrail 산식·정답 검사·분석기·recipe 형식은 바꾸지 않았다. 같은 작업 트리에서 다른 세션이 미커밋 DEV3 engine 변경을 진행 중이어서, 실제 연결용 build는 HEAD `9275154`와 S1 파일만 담은 깨끗한 export에서 만들었다.

## 구현

**controller slot gate(XADBTest, D2 runner 변경).** `run-performance`와 `replay-performance`가 `--gate`·`--gate-profile`·`--gate-profile-sha256`을 함께 받는다. 셋 중 일부만 주면 거절하고, 하나도 주지 않으면 기존 동작·bundle과 같다. 구현은 [gate 모듈](../../crates/testing/controller/src/performance/gate.rs)과 [bundle 연결](../../crates/testing/controller/src/performance/bundle.rs)에 있다.

- profile은 64KiB 이하 strict JSON이다. release build는 GO 대기 660초·ACK 대기 60초만 허용하고, self-test build는 시험을 위해 그보다 짧은 값만 허용한다.
- supervisor가 만든 owner-only FIFO 두 개(`controller.fifo`, `supervisor.fifo`)를 쓴다. supervisor가 읽고 있지 않으면 bundle을 만들기 전에 실패한다.
- slot마다 `Session::start` 전에 `READY ordinal run`을 보내고 같은 ordinal·run의 `GO`를 받은 뒤에만 worker를 시작한다. slot이 끝나면 `DONE`/`ACK`를 교환한다. 마지막 slot의 ACK, 또는 `ABORT` 뒤의 ACK만 환경 판정 SHA-256을 싣는다.
- 순서가 앞선 응답·한 줄을 넘는 응답, ordinal/run 불일치, EOF, deadline은 gate 실패다. 실패한 slot과 그 뒤 slot은 `NOT_RUN`으로 남는다. transcript에 종료 사유를 기록하고 판정 digest 없이 index를 저장한다. controller는 분석하지 않고 exit3로 끝난다.
- worker를 띄운 직후 Performance 요청 전에 `/proc/<pid>/stat`의 nice가0인지 확인한다. 아니면 그 slot은 failure이며 worker를 정리한다.
- gate를 쓴 bundle은 index `schema_version` 2와 `gate` binding(profile·판정 digest)을 쓰고 `gate-profile.json`·`gate-transcript.json`을 보존한다. verify는 recipe 순서로 유일하게 가능한 교환 전문을 다시 만들어 글자 단위로 대조한다. 판정 digest가 없으면 분석을 거절한다. gate를 쓴 원본을 gate 없이 replay하면 거절한다.

**supervisor·환경 판정기.** [dev2_profile_b.py](../../scripts/dev2_profile_b.py)와 동결 [profile](../../scripts/fixtures/dev2_profile_b_v1.json)이다. supervisor는 CPU4에 고정되고 controller는 `taskset -c 2`, nice0으로 실행한다.

- B1은 pair의 첫 slot 앞에서 10초 창 단위로 유휴를 확인하고 10분을 넘으면 ABORT한다. B2는 GO→DONE 동안의 `/proc/stat` tick 규칙과 `slot-env-v1`을 적용하고, DONE 시점 CPU2/18의 실행 task 목록을 남긴다. 주파수 ±3%와 공변량 중앙비[0.99, 1.01]는 campaign 단위로 판정한다.
- supervisor는 B2 INVALID slot이 나와도 20 slot을 모두 진행한다. 판정 파일을 쓰고 hash를 ACK로 보낸 뒤에만 bundle·worker 출력을 읽는다. controller 판정을 덮어쓰지 않고 `environment_validity`, campaign 분류, `pf04_decision`을 따로 낸다.
- 계획의 종료 상태표, 역할별 교체(최대2회, seed +1,000,000/+2,000,000), 역할 최종성, control→audit→replay 진입 조건, session 사이에 이어지는 3시간 예산, PF04 K0 판정표, C3 campaign의 교체 거절·순서·B2 미사용을 구현했다.

계획이 정하지 않은 세부는 다음처럼 구체화했다. 모두 [K0-005 계약 등록](../implementation/dev2-implementation.md#k0-005-profile-b)에 적었다.

| 항목 | 구체화 |
| --- | --- |
| ABORT 뒤 종료 | controller가 `DONE`을 보내고 supervisor가 판정 digest를 실은 ACK로 답한다. 프로토콜로 끝난 모든 gate bundle에 판정이 묶인다 |
| 공변량 중앙비 | 같은 pair의 baseline/candidate CPU2 cycles/ref 비율10개의 정확한 중앙값, 구간 양끝 포함. [모드별 진단](dev2-pf04-mode-attribution-20261025.md)의 paired 중앙과 같은 방식 |
| 다른 CPU 평균 | CPU2/18을 뺀 각 CPU busy 비율의 산술 평균 |
| 대기·상한 | 끝나는 시점이 10분을 넘을 수 있는 유휴 창은 시작하지 않고 ABORT. GO 전마다 남은 30분 시간이 slot deadline 60초보다 작으면 ABORT. GO 직전 controller 생존을 확인해 유휴 대기 중 사라진 controller에는 GO를 보내지 않음(첫 GO 전 NOT_RUN, 이후 TERMINATED) |
| gate 전 실패 | FIFO 생성·recipe 준비·장치 해석 등 supervisor 실패는 NOT_RUN으로 ledger에 정산하고 시도1회 사용. C3 recipe 입력 검사는 시도를 만들기 전 거절 |
| `slot-env-v1` | slot마다 disabled 상태의 CPU 범위 perf 하나를 준비하고 GO 직전 enable, DONE 직후 disable·SIGINT 종료. supervisor와 같은 CPU4에서 실행 |
| 장치 식별 | 파일시스템 UUID와 serial로 식별하고 kernel 장치 이름은 campaign마다 해석해 기록(아래 r1 실패) |
| 제품 결함 판별 | 판정 digest 뒤 native 증거에서 stderr의 `panicked at`, Ready/Performance가 아닌 응답, `Output.error`를 제품 결함으로 본다. 그 밖의 run failure(nice 불일치·전송·timeout·signal)는 TERMINATED |
| C3의 정답 FAIL | native 증거로 baseline 결함을 확인하지 못하면 candidate 기각. N-r5-1 문구의 baseline `check_output` 불일치와 다를 수 있는 명시적 차이이며, 계획의 “불확실이면 기각”을 따름. baseline 결함은 INCOMPLETE |
| C3 순서·recipe | 탐색 → 확인 → 두 guard, 앞 단계 COMPLETED 뒤에만 시작. recipe kind·모드와 서로 다른 두 build를 역할마다 확인. 단계별 통과 조건은 S3가 확인 |
| 3시간 예산 | `init`은 `--new-budget`·`--budget-from` 중 하나만 받음. `--new-budget`은 첫 session용 운영 규칙이며 predecessor 목록으로 감사. 이후 session은 lock을 잡고 닫은 이전 ledger를 hash와 함께 받아 그 누적 사용량에서 시작. 사용량 합계 검증 |
| 판정 도구 식별 | 판정·campaign 기록에 supervisor·window 스크립트 SHA-256 |
| 연결 역할 | `connection`(seed 2026102600)은 2 slot 뒤 ABORT하는 도구 연결 전용이다. 3시간 예산에 포함되고 1회뿐이며 PF04·C3 판정에 쓰지 않는다 |

## 변경별 선택 검증

변경하지 않은 하위 회귀·workspace 전체·완료 성능 campaign은 실행하지 않았다. 결과 원문은 세션 화면에서 옮긴 기록이며 cargo/unittest 원 로그 파일은 없다. 이 한계를 [source별 기록](dev2-pf04-profile-b-s1-20261026.json)에 함께 둔다.

| source | 실행 | 결과 |
| --- | --- | --- |
| controller r1 | 신규 [gate 시험](../../crates/testing/controller/tests/performance_gate.rs)7함수(self-test, mutant worker) | 6 PASS, 1 FAIL. 시험 assertion이 기존 단어 `Investigate`의 `gate`를 잡음 |
| controller r1 + 시험 수정 | 해당1함수 | PASS |
| controller r1 | 기존 `bundle_verifier_reanalyzes_raw_streams_and_rejects_missing_history_identity_and_failure`(바뀐 verify·index 경로를 지나는 유일한 기존 시험) | PASS |
| controller r1 | `cargo fmt -p xadbtest --check`, `cargo clippy -p xadbtest --all-targets --all-features -D warnings` | PASS |
| supervisor r1 | 신규 [Python 시험](../../scripts/test_dev2_profile_b.py)26함수 | 25 PASS, 실제 controller 연결1함수는 binary 경로가 없어 skip. 같은 r1에서 self-test controller·mutant worker로 그1함수 PASS |
| supervisor r2(장치 식별 수정) | 바뀐 경로의2함수(신규 장치 vector, 실제 controller 연결) | PASS. 나머지25함수는 r2가 바꾸지 않은 경로라 재실행하지 않음 |
| supervisor r3(검토 r1 반영) | 바뀐 경로의16함수: 공변량 방향, ledger·결정5(신규 예산 연계·C3 recipe 포함), gate 서비스8(신규 GO 전 controller 소실 포함), 신규 collector identity, 실제 controller 연결(닫힌 ledger 연계 포함) | PASS. 바뀌지 않은 경로의15함수는 재실행하지 않음. 그중 `test_campaign_validity_from_slots_abort_and_termination`은 바뀐 공변량 줄을 지나지만 모든 비율이 같아 결과가 변하지 않음 |
| supervisor r4(검토 r2 반영) | 바뀐 경로의16함수: ledger·결정5, 신규 session lifecycle2(gate 전 실패의 NOT_RUN 정산, `close`의 lock·합계 검증), gate 서비스8, 실제 controller 연결1 | PASS. 바뀌지 않은 경로의17함수는 재실행하지 않음. 현재 고유 함수는33개(이름 변경 1개를 포함하면 실행된 이름은34개) |
| supervisor r5(검토 r3 반영) | signal handler를 controller 시작 전에 설치·실패 경로 복원, 대체 판정 파일의 tool hash. 바뀐 경로의2함수(gate 전 실패 정산에 handler 복원 단언 추가, 실제 controller 연결) | PASS. 대체 판정 파일 경로는 전용 시험이 없음 |

controller 시험(7함수)은 gate ABORT·EOF·deadline·ordinal/run 불일치·한 번에 온 두 응답, 최종 ACK의 digest 누락·형식 오류, profile hash 불일치·release deadline 초과·미지 필드·FIFO가 아닌 파일·supervisor 부재, 교환 전문 변조(중복 READY)와 index 판정 digest 교체, 실행한 slot을 ABORT로 바꾼 기록, gate 없는 replay, nice1 worker, GO 지연400ms가 `external_ns`와 native journal 시간에 들어가지 않음을 확인한다. 같은 입력의 ungated bundle은 schema 1·10파일·`gate` 필드 없음이다.

Python 시험은 tick 1/2·nice/steal 0/1, B1 5%/10%/5% 경계, PMU coverage·scale·zero 분모, CPU18 cycles 5% 경계, ±3%·[0.99, 1.01] 경계, slot 하나 INVALID에도 20 slot 진행, 유휴 10분 초과·30분 상한 ABORT, 중복 READY·순서 오류·controller 종료, GO 전/후 collector 실패, 종료 상태표의 모든 행과 correctness FAIL 우선, 교체 초과·최종 역할 재실행·control 미통과 뒤 audit 거절·예산 소진, C3 교체 거절과 B2 미사용, PF04 판정표, 제품 결함과 도구 종료의 native 증거 구분, 가짜 perf의 정상·scale·소실·SIGINT 무시 정리, fixture perf의 특권 실행 거절, controller 순서 port의 literal vector(seed 20261018)를 확인한다. r3에서 닫힌 ledger만 예산을 넘기고 hash가 다르거나 닫히지 않은 ledger는 거절함, 연계된 사용량으로 예산 소진, C3 순서와 recipe kind·모드·두 build, 실제 collector의 CAP_PERFMON 외 capability·group 이탈·uid·명령 불일치 거절, 유휴 대기 중 controller 소실 시 GO 미전송을 추가했다.

## 실제 연결

| 시도 | 결과 |
| --- | --- |
| r1 init | **FAIL**(campaign·gate·perf·worker 미실행). profile이 장치 이름 `nvme1n1`을 고정했으나 재부팅(10:42 KST uptime 48분) 뒤 같은 serial `S5GYNG0R112060F`·UUID 디스크가 `nvme0n1`으로 열거됐다. 빈 session 디렉터리만 생겨 지웠고 실패 기록과 r1 binary를 보존했다 |
| r2 `connection-a0` | **TOOL_CONNECTED**. 성능 판정이 아닌 도구 연결 결과다 |

r2에서 공유 target 디렉터리로 다시 build하자 build script가 다시 돌지 않아 r1 source 지문이 binary에 남았다. 이 binary는 쓰기 전에 버리고 새 target 디렉터리에서 다시 build했다. 실제 사용한 controller·worker는 r2 export 지문 `343fc36e…`를 담는다.

r2 연결은 B1 유휴 창1개(10.0005초, busy CPU2 약1.31%·CPU18 약0.20%·다른 CPU 평균 약0.82%, 장치 io_ticks 0.46%)를 통과했다. 그 뒤 Off/Off 두 slot을 실제 release worker로 실행하고 셋째 READY에서 ABORT했다.

- 두 slot 모두 B2 VALID였다. CPU18 non-idle tick0, CPU2 nice+steal tick0, CPU18 cycles 비중 약0.21%/0.81%였다.
- `slot-env-v1` 네 행 모두 unscaled·coverage100%였다. CPU2 cycles/ref는 약2.0858/2.0854다. 실제 collector는 UID1000이고 capability 다섯 집합 모두 CAP_PERFMON(bit38)만 가졌다. 소유 group 소멸과 pidfd 종료를 확인했다.
- GO→DONE 창은 약30.35/30.33ms, 첫 slot DONE부터 둘째 slot GO까지 약58.2ms였다. 이 간격은 PMU 종료·다음 collector 준비를 포함한다.
- controller는 schema 2 bundle에 profile·판정 digest를 묶었다. 실행 slot2개, ABORT1개, NOT_RUN17개다. controller와 supervisor transcript의 12줄이 같다. 별도 `verify-performance` 재분석도 같은 comparison(validity INVALID, correctness UNKNOWN)을 냈다. 판정 이후 읽은 native 증거에서 제품 결함·도구 종료는0이다.
- 환경 판정은 계획된 ABORT 때문에 INVALID다(`aborted before GO: CONNECTION_STOP`). 연결 시험은 성능 판정에 쓰지 않는다.

campaign 시간은 약10.26초이고 ledger의 3시간 예산 사용량에 들어갔다. r2 도구는 gate 생성 직전부터 시간을 쟀다. r3 이후 도구는 시도 디렉터리를 만든 때부터 재므로 recipe 준비 시간도 예산에 들어간다(보수적 방향). 이 session은 r2 도구로 만든 첫 예산 session이며, 뒤 session은 이 ledger를 닫고 이어 받아야 한다. 연결 전에 같은 승인 경로로 perf CSV 형식만 확인하는 0.1초 probe를 slot 밖에서 1회 실행했다. 재부팅·서비스 재시작·host 설정 변경은0회다.

## preflight

같은 판정기 코드의 tick 규칙을 보존된 slot host 창 80개(Off 분류20·모드별 진단40·경계 대조20)에 다시 적용했다. 결과는 79/80 VALID다. INVALID 1개는 Off 분류 slot-03(CPU18 non-idle 2 tick)이다. 계획 검토의 계산과 같으며 재측정이 아닌 기술 보고다. 보존 창은 그 도구들의 slot 경계이고 profile B의 GO→DONE 창과 같지 않다.

## 한계와 다음 단계

- tick 규칙의 검출 한계는 계획과 같다. CPU2의 nice0 외부 작업은 worker·controller와 구별되지 않는다. bare metal이라 steal은 늘 0이다. CPU18 ≤1 tick은 약10–20ms의 형제 작업을 허용한다.
- 실제 연결은 2 slot만 실행했고 r2 도구를 썼다. r3–r5 수정과 20 slot 완주, 교체, 3시간 예산 소진·연계, replay gate는 실제 native에서 실행하지 않았다. 이 경로들은 위 self-test·Python 시험만 거쳤다.
- slot마다 sudo·perf를 GO 직전에 준비한다. 시작은 B2 시작 tick 관측 전에, 종료는 DONE tick 관측 뒤에 하지만, sudo 쪽 활동이 CPU4 밖에서 창과 겹치면 INVALID 비율만 높일 수 있다.
- bundle 증거를 판정 뒤 읽지 못하면 그 시도를 TERMINATED로 ledger에 남긴다. 이 경로의 전용 시험은 없다.
- r2 연결 session의 기록에는 판정 도구 hash 필드가 없다(r4에서 추가). 그 연결에 쓴 스크립트 hash는 외부 `export-manifest.json`에 있다.
- S2(C3 구현·선택 검증)부터 S7까지는 NOT_RUN이다. 원 e3 source의 PF04 네 FAIL, C1/C2 기각, P4 WAL 미채택, PF12/PF13 한정 완료와 DEV2/B2/D0 미완료를 유지한다. 영속 경로는 `PF04-DUR-001` 결정 대기다.

## 독립 검토

r1(`s1-review-r1.json`, SHA-256 `fcabac62abf179704e3db0e0aa9579b001c69af9f2283d62ace1bbaaeaad32e4`)은 **BLOCKED**였다. 차단1건은 3시간 예산이 session마다 0에서 시작해 S1·S3·S5 합산 상한을 강제하지 못한다는 지적이다. S5는 S4 뒤 새 build로 새 session을 만들어야 한다. 닫힌 이전 ledger를 hash로 받아 사용량을 잇도록 고쳤다. `init`은 `--new-budget`과 `--budget-from` 중 하나만 받으며, `--new-budget`을 첫 session에만 쓰는 것은 도구가 강제하지 않는 운영 규칙이다. 검토자는 실제 연결 수치, 두 build 지문, 모든 hash, preflight 79/80, 검토받은 계획 projection 불변을 원자료에서 다시 계산해 일치를 확인했다.

비차단12건은 다음처럼 처리했다. N1 재실행 생략 수(24→25)를 고쳤다. N2 GO 직전 controller 생존 확인, N4 공변량 방향(baseline/candidate)과 1.01 경계 시험, N5 collector identity 부정 시험, N6 10분을 넘는 유휴 창 금지, N7 연결 1회, N8 C3 순서·recipe 확인, N9 bundle 읽기 실패의 ledger 기록을 반영했다. N3은 명시적 차이로 계약에 적었고, N10은 위 한계에 적었다. N11은 이 검토·commit 뒤의 상태로 맞췄고, N12 표현(tick 분수, perf probe)을 고쳤다. r2(`s1-review-r2.json`, SHA-256 `0133a94c6e56d02c36ceecffa98bd2d5734851e0b9971cb42cbf5833de3d9417`)는 **PASS_SCOPED_S1_TOOL**이다. 예산 연계가 각 session이 닫힌 이전 ledger를 이어 받을 때 합산 상한을 강제함을 확인했고 회귀는 없었다. 비차단 지적 중 `close`의 lock·닫힌 ledger 기록 거절·합계 검증(R2-N1), gate 전 supervisor 실패의 NOT_RUN 정산(R2-N2), 판정 도구 hash 기록(R2-N7), 문서 정확도(R2-N3·N4·N5)를 r4에서 반영했다. 두 번째 `--new-budget`처럼 운영자가 명시적으로 우회하는 경우는 도구가 막지 못하며 predecessor 목록으로만 감사한다. C3 단계별 통과 조건(R2-N6)은 S3가 controller 판정으로 확인한다.

r3(`s1-review-r3.json`, SHA-256 `5e07b18419f692fd278fa396e18501a492e1e235cc1971688ecd7c0a4ac5ccf9`)는 r4 차분에 대해 **PASS_SCOPED_S1_TOOL**이다. 실패 경로의 자원 정리, 옛 ledger의 합계 검증 통과, `check_total`의 거짓 거절 없음(독립 모의 20,000개, 최대 오차1.46e-11)을 확인했다. 비차단4건 중 controller 시작 직후 signal 공백(R3-N2)과 대체 판정 파일의 tool hash(R3-N3)를 r5에서 고쳤고, source별 hash 보존(R3-N1)과 문구(R3-N4)를 반영했다. r5 차분은 위 2함수만 실행했고 별도 검토 회차는 두지 않았다.

외부 원본은 `/home/et16/xadb-dev2-evidence/pf04-profile-b-s1-20261026/`에 있다. 문서 자동 검사는 push gate에 남긴다.
