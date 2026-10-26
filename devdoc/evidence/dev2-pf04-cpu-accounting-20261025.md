# DEV2 PF04 CPU 시간 집계 의미 조사

2026-10-25 · **보존 source 읽기·새 실행0 / 독립 D1/D2 한정 채택 / 원 PF04 FAIL 유지**

[사용자·커널 PMU 관측](dev2-pf04-cpu-split-20261025.md) · [source 색인](dev2-pf04-cpu-accounting-20261025.json) · [후속 계획](../plans/dev2-pf04-resolution-plan.md#cpu-user-kernel)

## 결론

`038236b`의 새8event 연결 뒤, user/system 시간과 counter를 구분하기 위해 설치 커널과 같은 Ubuntu source package를 읽었다. **rusage의 영역별 시간은 이 환경에서 timer tick의 user/system 비율을 scheduler 누적 실행 시간에 맞춰 보정하는 경로다. 원 PF04의 전체 CPU 지표는 별도 `CLOCK_THREAD_CPUTIME_ID` 경로다.** 영역별 배분의 한계가 원 전체 CPU FAIL을 무효화하지 않는다.

새 제품·수집 도구 수정, build·시험·worker·PMU/BPF 실행, 재시작·host 설정 변경은 모두0이다. 담당은 `/root`, 독립 검토는 `/root/pf04_evidence_review`다. 읽기 조사이며 새 성능 표본이나 원인 재현이 아니다.

## 실제 source와 host

외부 루트 `/home/et16/xadb-dev2-evidence/pf04-cpu-accounting-20261025`에 kernel12파일을 보존했다. 기존 보존 `linux_7.0.0.orig.tar.gz`와 Ubuntu `linux_7.0.0-38.38` diff에서 필요한 부분만 추출·`--fuzz=0` 적용했다.7파일의 patch와 적용 로그를 보존했다. source package의 기록 tag는 `Ubuntu-7.0.0-38.38`, peeled commit은 `740b32353f43e9b00d0350d6fa7b29a6667deabd`다. 개별 최종 파일·archive/diff SHA-256은 `source-manifest.json`에 있다. 소스를 build하거나 실행 kernel 메모리와 byte 비교한 것은 아니다.

host는 `7.0.0-38-generic`, 직전 두 PMU 연결과 같은 boot ID다. `/boot/config-7.0.0-38-generic`은 `CONFIG_HZ=1000`, `CONFIG_VIRT_CPU_ACCOUNTING_GEN=y`, `CONFIG_NO_HZ_FULL=y`, `CONFIG_CONTEXT_TRACKING_USER_FORCE`와 `CONFIG_IRQ_TIME_ACCOUNTING`은 꺼짐이다. `/sys/devices/system/cpu/nohz_full`은 newline1byte로 CPU 목록이 없고, 관련 boot parameter도 없다. 설정 파일·sysfs 원 byte와 읽은 결과를 보존했다.

`drivers/base/cpu.c:nohz_full_show`는 full-dynticks mask가 없거나 비어 있으면 빈 목록을 출력한다. `kernel/time/tick-sched.c:tick_nohz_init`의 full mask 순회와 `kernel/context_tracking.c:ct_cpu_track_user`·강제 활성화 분기를 함께 읽었다. 이 구성과 빈 mask는 CPU2의 generic vtime/context tracking 비활성 경로를 뒷받침한다. live static-key/per-CPU 메모리를 직접 읽은 증거로 확대하지 않는다.

## 세 종류의 시간과 count

| 관측 | 실제 source 경로 | 해석 |
| --- | --- | --- |
| `RUSAGE_THREAD` user/system | `kernel/sys.c:getrusage` → `kernel/sched/cputime.c:task_cputime_adjusted`·`task_cputime`·`cputime_adjust` | 누적 tick 비율을 scheduler runtime에 맞추고 두 성분의 단조 증가를 보장. syscall은 timeval로 반환하므로 표시 단위는 마이크로초지만 영역별 정확도가1µs라는 뜻은 아님 |
| 원 전체 thread CPU | `kernel/time/posix-cpu-timers.c:THREAD_CLOCK`·`cpu_clock_sample` → `kernel/sched/core.c:task_sched_runtime` | `CPUCLOCK_SCHED`가 실행 중인 task의 pending runtime까지 갱신하여 반환. rusage의 tick 기반 user/system 배분과 다른 경로 |
| perf event runtime/coverage와 count | `kernel/events/core.c:__perf_update_times`·`perf_event_time`·`__perf_event_time_ctx`·`ctx_sched_out` | context 시간과 event ACTIVE/INACTIVE 상태로 enabled/running을 계산. u/k event의 runtime은 각 영역의 CPU 시간을 직접 측정한 값이 아니며, 하드웨어 count와 단위·범위가 다름 |

`account_process_tick`은 vtime 비활성일 때 `TICK_NSEC`를 당시 user/system 상태에 배정한다. `cputime_adjust`는 누적 `stime/(stime+utime)` 비율을 `sum_exec_runtime`에 적용하고 이전 user/system 값보다 작아지지 않게 보정한다. 따라서 두 rusage snapshot의 차이는 이 측정 구간에만 속한 정밀한 privilege-domain 타이머가 아니다. 과거 누적 비율·timer 관측 시점의 영향을 받을 수 있다. 실제 각 tick이 어느 상태를 보았는지는 이번 자료에 없다. 또 vtime 비활성 경로에서 `task_cputime`가 false를 반환하면 `task_cputime_adjusted`는 이미 누적된 `p->se.sum_exec_runtime`을 사용하고 `task_sched_runtime`을 따로 호출하지 않는다. 원 thread clock의 pending runtime 갱신과도 달라 두 총시간의 경계까지 강제 일치시키지 않는다. 이 차이를 관측된 수치 차이의 정확한 원인으로 분해한 것은 아니다.

제품 source는 같은 보존 ELF의 기준 commit `5253caa`에서 platform `performance.rs`, worker `durable_performance.rs`·`measurement_window.rs`를 추출했다. `thread_usage()`는 `getrusage(RUSAGE_THREAD)`, `thread_cpu_ns()`는 `CLOCK_THREAD_CPUTIME_ID`를 호출한다. durable worker의 원 `cpu_ns`는 loop 전후 `thread_cpu_ns()` 차이다. 진단 gate의 rusage는 그 바깥 경계에서 별도로 수집한다. 이 source 연결 때문에 rusage 분할만의 한계를 원 CPU guardrail 결함으로 주장하지 않는다.

## 관측에 적용한 판단

직전 두 연결의 kernel instructions는20,775,069·20,589,750, rusage system delta는12,597,000ns·18,843,000ns였다. user instructions는13,571,051·14,863,461인데 rusage user delta는15,857,000ns·10,235,000ns였다. 소스는 이처럼 rusage 분할과 하드웨어 작업량이 같은 방향·비율로 움직여야 하는 계약이 없음을 설명한다. 두 값의 차이를 보정하거나 과거 FAIL pair의 정확한 기여도로 환산하지 않는다.

각 slot의 u/k event가 동일 runtime을 보고한 것도 user와 kernel 각각 그 시간만큼 실행됐다는 의미가 아니다. 같은 event context 시간에 기초한 enabled/running이며, count 필터와 분모를 혼동하지 않는다. runtime과 원 CPU의 차이, cycles/ref-cycles 비율을 GHz·정확한 PMU 누락·커널 추가 CPU로 변환하지 않는다.

**확인된 것은 관측값의 의미와 오해할 수 있는 경계다.** 원 전체 CPU 한도·p99 FAIL의 재현 원인, frequency/C-state·scheduler/IRQ·장치 공유의 기여, 안정적 계측 비용과 제품 개선 효과는 미확정이다. C1/C2 기각·WAL 미채택·원 네 campaign FAIL·DEV2/B2/D0 미완료를 유지한다.

## 다음 실행의 조건

현재8event 도구로 user/kernel 작업량을 구분할 수 있으므로 rusage 분할을 이유로 반복 실행하지 않는다. 후속 성능 수정은 실제 제품 hot path의 별도 후보와 유지할 의미·최소 효과를 먼저 확정해야 한다. 환경 원인을 더 구분하려면 target thread의 실제 schedule-in/out 구간과 CPU 전력/주파수 관측의 대응을 새 수집 변경으로 설계하고, 영향·손실·필요 권한·종료·고정 실행 예산부터 검토해야 한다. per-CPU 값을 자동으로 해당 task에 귀속하거나 governor 변경·재부팅을 선행 해결책으로 삼지 않는다.

이 문서는 그 추가 실행을 시작한 기록이나 반복 인수 허가가 아니다. 새 가설·변경 근거 없이 완료한 인수·두 연결·기각 후보를 다시 돌려 PASS를 찾지 않는다. 최종 독립 검토와 보존 manifest는 연결 JSON에 기록하며 문서 verifier는 push gate로 남긴다.

최종 독립 검토 **PASS_SCOPED_D1_D2_CPU_ACCOUNTING_SOURCE_ANALYSIS**를 채택했다. 차단0개이며 source·host·제품 호출 연결과 위 의미 구분 범위다. receipt `/home/et16/xadb-dev2-evidence/pf04-cpu-accounting-20261025/final-review.json`, SHA-256 `cf6af0f1acde8d7a332c0fd988a0ba99fe697bb213bbc24cb628394f2d98f869`다. 원인 확정·원 PF04/DEV2 완료 승인이 아니다.
