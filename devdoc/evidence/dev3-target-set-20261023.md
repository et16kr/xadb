# DEV3 전체 대상 pin 집합 — 선택 검증

2026-10-23 · **D2 ACCEPTED / LOCAL_SELECTED_VERIFICATION / 제한 단위 최종 D1 수용**.
작성·통합 `/root`, 독립 검토 `/root/pool_design_review`, 방식
`open-code-review-delegate`. [source·receipt 색인](dev3-target-set-20261023.json)과
[소유 계약](../contracts/dev3-task-05-buffer-flush.md#bounded-target-set)이 정확 범위를 정한다.

## 공급 범위

기존 `SharedPool`의 단일 directory·pin accounting·DPT를 사용해 정확1..64개 요청을 함께
확보한다. `reserve_targets`의 기존1..2 slice도 같은 generic core를 사용하며 기본 guard 크기는
유지한다. 전체 중복/lookup/정렬/quota 확인 뒤 pin을 한 번에 증가하고, 실패·unwind·release는
모든 대상의 count와 잠금을 정리한다. 실제 dirty 설치 권한은 원 sealed permit이 소유한다.

이 단위는 실제 일반 load/registration·N+1 dirty/flush/evict/reload·큰 resident backing을
공급하지 않는다. 시험 bootstrap의 페이지를 일반 disk I/O 인수로 세지 않는다.
실제 live Begin/undo/terminal·현재-build 복구·tree/행 거래·전체 DEV3/S5는 미완료다.

## Source별 실행

| source | 변경·결과 | 실행/보존 범위 |
| --- | --- | --- |
| r0 `6087470c9cbdfe243b427c4123a47e622799ec4e279064401a0e4adc60e9df47` | compile FAIL, thread closure가 `Receiver`를 shared borrow하여 E0277 | 원 source·진단 보존, runtime NOT_RUN |
| r1 `aadb52ef68364808418203591f857d3fd695e1280f50fea7e1062632f4f2150f` | closure 소유 이동 수정, compile PASS | 새 runtime15·직접 관련4·static2·fmt/engine Clippy PASS |
| r2 `5fa194a252b4acd2c3f0ff6df1be5981aca2306acdedd9b46cca6a42c970288f` | compiler runner provenance·foreign diagnostic 보완 | 새 checker3 PASS, Rust는 r1과 동일 |
| r3 `b95c1a8da67207ea9d79a941091fb5143d8441861674e6bf2a3fca25c4164e3f` | 같은 error 안의 다른 함수 instantiation 위조 거절 | 변경 diagnostic checker1·compiler7 PASS, Rust는 r1과 동일 |

제품 runtime19는 r1 ELF `dba580a348eaf20a8326583487cc85af9e245d5d4e6d29bc8d742b77fdf3da52`에서
정확 selector별1개씩 실행했다. 합1.143초이며 각30초·합180초 안이다. 원 static 결과는 같은
r1의 별도2개 receipt다. r2/r3의 Python 수정이나 source 지문 변화 때문에 제품 시험을 반복하지 않았다.
기준 제품 commit은 `8ed6eed`, r2/r3 당시 HEAD는 별도 COMM 문서 commit `5d320fd`다.

[동반 manifest](../../crates/engine/tests/fixtures/dev3-target-set/manifest.json)는 사전 inventory이며
`AUTHORED_NOT_RUN`은 입력 동결 당시 상태로 보존한다. 실행 상태는 이 증거와 원 receipt가 소유한다.
새15는 N3/N64 순위·bytes, 비인접 alias, 마지막 missing, pin quota와 선행 owner 보존,
late Busy read/write, poison/stale cleanup, release 원자성, fence-before-unlock·실제 thread
reader/writer 배제, stamp/인덱스와 기존 wrapper를 확인한다. 관련4는 바뀐 공통 core의
reverse rank·atomic rejection·stale release와 실제 allocator reserve/cancel 소비자다.
완료된 무변경 하위 회귀·전체 wrapper·workspace 시험은 실행하지 않았다.

compiler7은 baseline1·N3 정상 해제1·owner 재대여/소비된 pins/Sync 거절3·N0/N65 capacity 거절2다.
55.600초로 합120초 안이며 test binary는 실행하지 않았다. baseline과 capacity는 `cargo test --lib
--no-run`, 나머지는 `cargo check --tests`를 사용한다. 정확 단일 오류 code/primary path/line와
해당 `reserve_target_capacity::<N>`을 확인한다. baseline 실패·다른 오류·다른 함수의 숫자는
기대한 거절의 PASS가 아니다. 각 probe의 수정 patch·source hash·compiler JSON과 stderr를 보존했다.

## 자원과 리뷰

N2/3/8/64 call source-value 식은1712/1856/2576/10640바이트다. 이전 default의 보수적
2352에 N64−N2 증가8928을 더해도11280≤16384다. 같은 carrier를 다른 식으로 세므로 두 식을
서로 더하지 않는다. 기존 pin56/read104/write128·pool17208과 default CALL3072를 유지한다.
성공해 유지 중인 capacity 합102의 retained metadata는5440바이트다. 진행/실패/전환/cleanup
callchain은 동시 수만큼 별도 과금하고 caller 요청·page loan도 별도다. whole live owner/RSS/OOM
충분성으로 일반화하지 않는다.

활성 fixture 한도2MiB, case/review 결과 한도16MiB다. 기록 시 작은 결과·review·compiler 진단은
852,631바이트이고, 별도 source export/ELF/compiler cache는3,174,014,419바이트다.
실제 경로/종류/bytes는 JSON의 `resource.tool_retention`과 원 `tool-retention.json`에 보존한다.

OCR delegate D2와 r1 runtime gate 뒤 신규15+관련4를 실행했다. runtime 검토 중 compiler runner에
TARGET-RUNNER-001(동결 fixture와 실제 실행 helper의 출처 결합), TARGET-RUNNER-002(정확 단일
진단의 함수 instantiation) 지적이 나왔다. r2/r3에서 보완하고 관련 checker만 선택했다.
최종 compiler gate는 r3에 대해 별도 수용됐다. 원 pending 판정·코드 검토·coverage와 후속 수용을
함께 보존하며 최종 D1은 `D1_ACCEPTED_LIMITED_BOUNDED_TARGET_SET`으로 수용했다.
원문은 `artifacts/dev3/target-set-20261023/review/final-d1.json`이며 SHA-256은
`9c5346e9f485a775bb1c9aeff4a2b1b95123b3c5a8cc7e1d078c8b52e8b7e617`이다.
OCR25항목 중 staged24를 검토했고 관련 제외16을 수동 검토했다. unrelated1과 공유 문서의
DEV2 unstaged hunk3은 범위 밖이다. 후속 상태 메타데이터의 검토는 같은 디렉터리
`final-metadata.json`으로 연결한다. review의 unrelated DEV2 항목은 사유와 함께
제외하며 해당 변경을 이 commit에 포함하지 않는다.

## 남은 인수

S5 확대576은 N1..64×4순서256, N64의 late64×read/write128,
N64의 stale64×release/read/write192로 작성했으며 **NOT_RUN**이다. 중간 mandatory와 새 compiler/
checker도 최종 S5 inventory에 포함된다. 일반 I/O나 새 영속 효과가 없어 이번 단위의 새 native
DB fault/reopen은 해당하지 않으며 이전 recovery 결과를 새 실행으로 재사용하지 않는다.
문서 자동 검사는 push 전용이므로 실행하지 않았다. push·전체 DEV3 인수 승격은 하지 않았다.
