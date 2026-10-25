# DEV2 PF04 해결 단계와 커밋 계획

2026-10-25 · **D2 계획 검토 채택 / 실행 전 계획 커밋 우선 / PF04·DEV2 미완료**

[원인 분석](../evidence/dev2-pf04-analysis-20261025.md) · [계측 계약](../contracts/ai-performance-engineering.md) · [기존 구현](../implementation/dev2-implementation.md#dev2-pf04-analysis-20261025) · [개발 정책](../policies/development-policy.md)

## 요청·담당·적용 범위

사용자가 “1단계부터 시작”, “단계별 commit”, 이어서 “일단 계획서를 commit하고 시작”하도록 지시했다. 먼저 본 계획과 앞선 분석·색인을 커밋한 뒤1단계 구현을 시작한다. 작업·계약·통합 담당은 `/root`, 계획과 구현의 작성자 외 독립 검토자는 `/root/pf04_evidence_review`다. 이번 착수 범위는1단계이며2–4단계는 아래 진입 조건에 따라 진행 여부와 구체 profile을 확정한다. push는 이번 요청에 포함하지 않는다.

기존 네 campaign FAIL, e3 원 source와 PF12/PF13 한정 완료를 유지한다. 기존 계측 비용 한도·정답·내구성을 낮추지 않는다. 동일 코드의 완료 시험·하위 회귀·전체 workspace 실행을 반복하지 않는다. 변경한 도구의 신규 시험과 직접 변경한 연결부만 선택 검증한다. 검증 후 추가 변경이 생기면 해당 diff·실패 사례만 추가 검증하고 앞선 결과를 보존한다.

## 단계와 종료 조건

| 단계 | 산출물·범위 | 종료·커밋 조건 |
| --- | --- | --- |
| P0 계획 | 본 계획, 원인 분석과 자료 지문·독립 검토, 문서 색인 | D2 계획 독립 검토를 반영한 문서 commit. 제품·도구 변경이나 시험 실행보다 먼저 수행 |
| P1 측정 구간 관측 도구 | 명시적 진단 실행의 측정 시작/끝 연결, worker identity, CPU/전환 관측과 선택적 PMU, bounded 외부 runner·실패 정리·증거 | 아래 P1 계약과 선택 시험 통과, 원 실패/미지원 보존, 최종 독립 D1/D2 검토, 구현·증거를 별도 commit. PF04 인수 승격 아님 |
| P2 K0 비용 절감 | P1의 구간 한계와 기존 source 비용을 바탕으로 snapshot 계측 소유권의 좁은 대안 검토 | 별도 D2 후보·예산 동결 후 관련 수명/동시성/정확 통계/자원 시험과 실제 변경 경로의 성능 비교. 효과 미달이면 기각 기록을 commit; 무조건 최적화하지 않음 |
| P3 영속 대기 원인 구분 | 저장소·journal 공유 조건, WAL fsync→journal→block 요청/FLUSH/FUA의 관측과 CPU 실패 구분 | 새 관측 도구의 영향·정리·source 연결을 먼저 검증. 측정 환경과 새로운 검증 범위를 확정한 후 진행. 확인/미확정 원인을 구분해 commit |
| P4 조건부 WAL 개선·종료 검토 | P3에서 충분한 근거가 있을 때만 공간 선확보/data sync 등 대안 설계; PF04·DEV2 종료 증거 | 형식·tail·quota·ACK/fence·복구 계약과 변경 관련 시험, 상대 계측 비용 검증, 독립 검토. 불필요하면 설계 기각을 기록하며 인수 FAIL을 자동 PASS하지 않음 |

P2의 새로운 탐색은 이전 종료한3후보 예산에 추가하지 않는다. 새 캠페인의 가설·후보 상한·확인1회·중단 규칙과 DP07 최소10쌍을 별도 동결한다. COMM의5+5 예외는 적용하지 않는다. P3/P4의 미정 입력은 P1 완료로 승인된 것으로 취급하지 않는다.

## P1 설계 선택

### 활성화와 배치

- 기본 경로는 기존 worker/fixture/profile을 유지한다. 명시적 진단 환경값이 있을 때만 새 관측 경계를 사용한다. 진단 도구는 engine/WAL에 계측 의존성을 추가하지 않는다.
- Linux calling-thread rusage·TID·monotonic 관측 primitive는 platform의 기존 performance 영역, 두 worker가 공유하는 private 진단 wire/Unix socket 상태와 실제 측정 연결은 engine-worker, 자식 process/perf 조립·판정·보존은 scripts의 XADBTest 도구가 소유한다. 여러 소비자를 조립하는 시험 절차를 platform 공통 API로 올리지 않는다. 새 crate·외부 의존성을 추가하지 않는다. 필요한 기존 nix 기능 추가는 D2 범위에 포함한다. `unsafe_code=forbid`를 유지한다.
- 새 구현 파일은 구현과 함께 저장소 배치 목록에 등록한다. 계획 커밋 때 빈 source나 실행 가능한 것처럼 보이는 placeholder를 만들지 않는다.
- 진단이 활성화된 output에는 기존 인수 path와 다른 명시적 진단 path를 쓴다. 기존 checker가 해당 output을 기존 인수 PASS로 받지 않는 것을 직접 확인한다. 원 protocol 형식과 기존 결과는 변경하지 않는다.

### 경계와 관측 의미

1. worker가 준비 단계에 소유된 로컬 Unix socket으로 연결한다. runner는 OS peer PID와 자신이 생성한 child PID, worker PID/TID·process starttime·source·workload·실행 token을 대조한다. 단일 main-thread driver만 지원하며 TID=child PID를 확인한다. 불일치하면 측정에 진입하지 않는다.
2. runner는 자신의 worker TID만 대상으로 perf stat을 disabled 상태로 준비한다. `--no-inherit`와 `--no-scale`을 사용해 후손을 제외하고 unscaled count를 수집한다. 기본 모드는 OS 권한 변경·sudo·공유 host tuning을 자동 수행하지 않는다. 후속 사용자 승인에 따른 명시적 특권 수집은 아래 보완 계약을 따른다. 준비 단계에서 확인된 미지원/권한 거절만 원 오류·`UNAVAILABLE`과 명시적 `RUSAGE_ONLY`로 보존하고 진행한다. attach 이후 enable/disable ACK 실패·중간 소실은 진단 실패다.
3. 기존 warmup 뒤 worker가 START를 보내고 runner가 PMU 모드에서는 perf enable ACK를 확인한 뒤, 준비 단계에서 확정한 RUSAGE_ONLY 모드에서는 해당 모드를 명시해 진행을 허용한다. worker는 추가 경계 monotonic timestamp와 calling-thread rusage를 얻고 기존 측정 loop를 수행한다. 측정 loop의 workload·warmup·길이·표본화·원 clock은 바꾸지 않는다.
4. 기존 측정 endpoint 직후 경계 timestamp와 rusage를 얻고 STOP을 전달한다. runner는 PMU 모드에서는 perf disable ACK를 확인하고, RUSAGE_ONLY에서는 해당 모드의 종료 응답을 준다. 경계 protocol 실패는 진단 실패이며 기존 제품 오류·부분 완료를 숨기지 않는다.
5. PMU가 덮는 구간에는 시작/종료 handshake 등의 부가 작업이 포함될 수 있다. worker 경계·원 측정 길이·runner enable/disable bracket을 모두 보존한다. 이를 loop만의 정확한 PMU 수치나 순수 CPU 주파수로 표시하지 않는다.

관측 후보는 user cycles·instructions·reference cycles, context switches·CPU migrations·page faults와 rusage의 voluntary/involuntary switches·faults다. raw count·실행 시간/coverage와 missing reason을 보존한다. multiplexing·미지원·권한 거절·0분모에서 정상 비율을 합성하지 않는다. cycles/reference-cycles는 사용 가능한 동일 구간 counter의 비율이며 GHz나 원 실패의 원인 판정으로 자동 변환하지 않는다.

### 한도·오류·보존

- socket 환경값은 경로·32자리 hex token이 모두 있어야 한다. line protocol은 버전·phase가 고정되며 최대512bytes다. 각 교환은 절대2초 deadline으로 제한하고 반복 read가 deadline을 연장하지 않게 한다. perf FIFO의 open/write/ACK도 nonblocking I/O와 교환별 절대2초 deadline을 적용한다.
- runner 전체 기본 deadline은30초, 최대60초다. 소유 worker/perf process group만 종료하며 TERM 뒤 최대2초, 필요 시 KILL 뒤 최대2초로 정리한다. runner의 예외·SIGINT/SIGTERM에서도 finally 정리를 수행한다. 외부 SIGKILL 같은 비협조 종료의 한계는 명시한다.
- 단일 실행 입력 최대64KiB, stdout 최대4MiB, stderr/perf 자료 각각1MiB, control line512bytes로 제한한다. deadline·용량 초과·worker 실패·부분 output·정리 실패는 따로 남기고 성공으로 절단하지 않는다.
- 결과 디렉터리는 새로 생성하고 덮어쓰지 않는다. 입력·worker SHA-256·source 기대값·runner/hash·명령·원 output·PMU raw·phase bracket·미지원·실패·정리 상태를 연결한다. 합격 판정은 도구 계약 범위이며 `performance_acceptance=NOT_APPLICABLE`을 고정한다.
- 실제 경계 비용은 측정값으로 보존한다. 인위적으로 빼거나 한도를 낮춰 인수 결과를 만들지 않는다. 수집 영향이 충분히 분리되기 전에는 원인 확정이나 성능 개선을 주장하지 않는다.

<a id="p1-privileged-perf"></a>

## P1 후속: 승인된 실제 PMU 수집

2026-10-25 사용자는 sudo 권한이 있으면 해결을 진행해야 한다고 정정했고, 일시 중지 뒤 “이어서 해주세요”로 재개했다. 기존 일반 사용자 실행의 UNAVAILABLE은 원 source의 증거로 보존한다. 새 경로는 D2이며 작업·계약·통합 담당 `/root`, 별도 검토자 `/root/pf04_evidence_review`다.

- 명시적 `--perf-privilege sudo-cap-perfmon`에서만 `sudo -n → setpriv → 설치된 /usr/bin/perf`를 사용한다. 세 도구의 실제 경로·상위 디렉터리는 root 소유이며 group/other 쓰기가 없어야 한다. 임의 perf fixture의 특권 실행은 거절한다.
- worker는 일반 UID/GID·기존 그룹/capability 상태를 유지하며 caller의 permitted/effective/ambient는0이어야 한다. perf는 호출자 UID/GID, 보조 그룹 없음, CAP_PERFMON만 inheritable/permitted/effective/bounding/ambient에 보유하고 no-new-privs를 적용한다. sudo supervisor의 EUID0·전체 capability는 이 launcher의 준비·감독 범위에 존재하며 perf 자체 권한과 구분해 기록한다.
- `PATH=/usr/bin:/bin`, `LC_ALL=C`, 임시 작업 HOME, `PERF_CONFIG=/dev/null`의 최소 환경을 사용한다. 사용자 perf config·loader 환경을 상속하지 않는다. sysctl, 설치 파일 capability, 부팅/서비스 설정을 변경하지 않는다.
- perf의 UID/GID/capability·명령·소유 session/group을 READY 전에 확인한다. 실제 collector는 sudo supervisor와 별도로 pidfd를 보관하여 PID 재사용 없이 종료를 확인하고, group이 달라 거절될 때도 해당 직접 자식을 종료한다. launcher 정리 뒤 pin한 collector가 남아 있으면 pidfd KILL 후 추가 최대2초로 종료를 확인한다. stdin DEVNULL, stdout/stderr pipe, 새 session으로 controlling TTY 없이 실행한다. 정리 확인은 소유 group과 pin한 직접 collector 범위이며, pin 전에 거절한 미확인 복수/중첩 topology의 외부 descendant까지 보장하지 않는다. sudo의 임의 policy plugin이나 적대적 descendant를 봉쇄하는 sandbox는 아니며 외부 SIGKILL에서 runner finally가 실행된다고 주장하지 않는다.
- 신규 선택 검증은 명령/권한 반례, 임의 실행 파일 거절, 실제 PMU synthetic 구간, 준비 중 취소·identity 거절, 활성 상태 SIGINT/SIGTERM/deadline 정리다. 기본 호출 인자 연결이 변경되므로 기존 fake PMU 연결 사례1개만 관련 회귀로 선택한다. Rust hook·완료 native·원 성능 campaign·문서 verifier를 반복하지 않는다.
- 실제 구간 counter·coverage·rusage·bracket과 sudo/perf/worker identity·종료 상태를 보존한다. `performance_acceptance=NOT_APPLICABLE`, PF04 네 실패·DEV2 미완료는 유지한다. 실행 전 diff·선택 이유·source를 보존하고 최종 독립 검토 뒤 후속 commit한다.

## P1 필수 선택 검증

| 범위 | 신규·변경 시험 | 원래 시험과의 관계 |
| --- | --- | --- |
| platform primitive | calling-thread TID·단조 clock·rusage 값/누락 처리 | 신규 primitive 시험만 실행. 기존 platform 전체 시험은 실행하지 않음 |
| worker private gate | disabled·잘못된 config·정상 순서·잘못된 ACK/token·oversize·EOF·절대 deadline | 신규 진단 module만 실행. 기존 worker 전체 시험은 실행하지 않음 |
| runner parser·통계 | literal PMU 정상/미지원/권한 오류/중복/누락/multiplexing·0분모·phase 역행/identity 위조·byte bound | 신규 독립 literal 기대값과 fake peer/perf로 확인; 기존 성능 analyzer 재실행 아님 |
| process 수명 | 실제 synthetic child의 정상·중간 실패·timeout·signal·종료 정리, attach 실패와 unavailable 보존 | 새 진단 도구의 실제 subprocess 경로. synthetic helper가 제품 engine을 호출하지 않음 |
| worker 연결 | 변경한 K0/영속 hook의 disabled/active·부분 실패·진단 path·기존 acceptance 거절과 독립 기대값 | 신규 선택 사례로 각 연결부를 확인. 원 fixture·LCG·old snapshot·최종 값·영속 ACK/파일을 독립 oracle로 대조한다. 기존 oracle를 재사용하는 경우 exact 진단 path/identity를 먼저 확인한 명시적 진단 adapter만 임시 copy의 path를 정규화하고 원 output은 보존한다. 결과는 진단 정답 범위이며 성능 인수 PASS가 아니다. 원 성능9campaign·하위 wrapper·전체 recovery는 실행하지 않음 |
| 실제 PMU | 새 synthetic helper에서 가능한 경우 real perf enable/disable·구간을 확인 | 권한/지원 부재는 UNAVAILABLE로 보존; fake 성공을 실제 PMU PASS로 표시하지 않음. 성능 인수 아님 |
| 정적·리뷰 | 수정 Rust의 fmt, 관련 package compile/Clippy, 새 source/증거 독립 검토 | Clippy는 제품 시험 실행이 아님. 문서 자동 검사는 push gate에서만 수행 |

구현자는 시험 전에 diff와 선택 사례의 관계를 기록한다. 최초 실행 결과를 source별로 보존하며, 수정 후에는 그 diff와 실패 사례에 필요한 것만 선택한다. P1 native 실행은 새 관측 경계·도구를 검증하는 목적이며 기존 K0/영속 성능 인수를 재판정하지 않는다.

## 검토·상태 기록

- P0 계획: `/root/pf04_evidence_review`의 `PASS_PLAN_ONLY`를 채택했다. R1 권한 부재 대체 경로, R2 raw PMU·FIFO deadline, R3 시험 소유 구분을 반영했고 차단/미해결 지적은0개다. 검토 뒤 상태·receipt만 기록하고 계획 commit을 생성한다.
- 검토 receipt: `/home/et16/xadb-dev2-evidence/pf04-analysis-20261025/plan-review.json`, SHA-256 `f0b15ad3063f9f02716c2d3f1d812bf9c8048c02e3d840b20affc50f9c3571c2`. 검토 본문 projection SHA-256 `903f30baec57dfb17855debd644e725887b5a9471b2ca32e42dfe64dc69d2124`는 상단 상태와 본 절을 제외한다. 계획 채택은 구현·시험 실행 증거가 아니다.
- P1 도구 구현·선택 검증: 계획 commit `82c14ba` 뒤 수행했다. [source별 실행 기록](../evidence/dev2-pf04-window-20261025.md)을 따르며 당시 일반 사용자 PMU는 UNAVAILABLE이었다. 승인된 후속의 [실제 PMU 수집·선택 검증](../evidence/dev2-pf04-pmu-20261025.md)을 별도 연결하며 최종 독립 검토는 각 기록의 채택 상태를 따른다. P0 검토 projection은 계획 commit 시점의 본문에 해당한다.
- P2–P4: 진입 전. 구체 대안·새 실행 profile은 별도 검토한다.
- 원 PF04 네 실패와 DEV2/B2/D0 INCOMPLETE: 유지.
