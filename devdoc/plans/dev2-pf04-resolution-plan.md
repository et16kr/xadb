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

<a id="p2-k0-owner"></a>

## P2 실행 계약: K0 관측 소유권 후보 하나

2026-10-25 사용자의 후속 “이어서 진행해줘”에 따라 P2를 시작한다. D2 작업·계약·통합 담당 `/root`, 독립 검토자 `/root/pf04_evidence_review`이며 본 구체 계약을 검토·commit한 뒤 구현한다. P1 실제 PMU 도구의 완료는 이 후보의 성능 효과나 PF04 원인 증거가 아니다.

### 가설·대안·불변식

현재 snapshot/cursor는 `Arc<Counters>`와 활성 collector의 Arc를 각각 복제한다. 후보 C1은 K0 전용 `Diagnostics` 내부 owner를 `Off(Arc<Counters>) | Active(Arc<Collector>)`로 만들고 Active collector가 기존 Counters Arc를 한 번 소유하게 한다. store/snapshot/cursor는 이 owner 하나를 공유한다. 활성 snapshot마다 두 번째 clone/drop 한 쌍을 제거하는 대신 counter 접근의 한 단계 간접 참조가 생기므로 실제 이득은 미확정이다.

공통 `metrics::Counters`의 구조·constructor·포화 갱신, transaction/recovery 소비자는 변경하지 않는다. Off는 기존 Counters Arc만 사용하며 collector/queue 추가 할당과 진단 logical bytes는0이다. Active collector에 추가되는 Counters handle은 `size_of<Collector>`를 통해 별도 진단 budget에 포함한다. 기존 row quota charge·pin 수·오류 순서·행/ID/old-root 값·snapshot/cursor의 store 독립 수명, 정확 started/completed/failed·drop/포화·event 순서를 유지한다. counter 감소·표본 누락·sync/부하 변경은 후보가 아니다.

공통 Counters에 Diagnostics를 넣는 대안은 거래/복구의 크기·quota를 바꾸므로 제외한다. 별도 K0 wrapper Arc를 추가하는 대안은 Off에도 할당·부가 charge를 만들므로 제외한다. 이번은 C1 하나, 구현 수정은 계약/정확성 결함 보완만 허용한다. 측정 수치를 본 뒤 다른 배치·알고리즘을 추가하지 않는다. 성능 이득이 부족하면 제품 변경을 채택하지 않고 원 source·후보 diff·실패/불충분 결과를 보존해 기각을 commit한다.

### 실행 전 고정한 범위

- 기준선은 commit `77d919f`의 깨끗한 export다. 후보도 같은 export에 이번 명시한 source diff만 적용하며 병행 DEV3 작업 트리는 포함하지 않는다. baseline 실행은 변경한 K0 경로의 상대 비교 대조군이며 기존 제품 인수 재검증으로 세지 않는다. `metrics.rs`, WAL/transaction/recovery를 포함한 무변경 하위 시험·workspace wrapper는 실행하지 않는다.
- 정확성은 변경된 Diagnostics owner/constructor를 직접 사용하는 기존 진단 unit4개, engine performance 사례5개를 선택한다. store보다 오래 사는 snapshot/cursor/row view의 값·오류·정확 진단·drop·quota 최종 해제와 동시 read/drain 수명을 새 사례로 검증한다. constructor의 invalid capacity 실패와 최종 owner drop에서 Counters의 Weak upgrade 불가·Quota=0을 확인하고, 관측용 Diagnostics 강한 clone은 실제 최종 view/owner 해제 전에 내려 자원 회수를 가리지 않게 한다. 관련 Rust fmt와 engine/worker의 scoped compile/Clippy를 수행한다. 문서 자동 검사는 push gate에 남긴다.
- P1 PMU 연결은 Detailed의 baseline/candidate 각1회만 새 구간을 관측한다. 원 workload와 sampling은 바꾸지 않고 worker/runner/ELF/source·raw counter/rusage/제어 bracket을 보존한다. 기존 retained-output adapter/check_output은 input.mode를 직접 검산하고 Side.mode를 사용하지 않으므로 checker source 변경 없이 새 Detailed 출력의 scalar/진단 정답을 확인한다. 원 raw·exact 진단 suffix 확인·임시 copy의 path 정규화는 그대로 유지한다. PMU 결과는 원 성능 인수나 효과 채택 표본에 넣지 않는다. PMU 한 쌍과 C1 전체 비교는 Collector 배치·간접 참조·컴파일 결과 변화까지 포함하므로 clone/drop만의 독립 기여로 해석하지 않는다.
- release toolchain·features·target·최적화 설정을 baseline/candidate에서 같게 유지한다. workload는 기존 K0 정책의128행, old snapshot 유지, capacity256, warmup4096, 측정65536, stride16, fixture seed20261018이다. controller/analyzer·성능 한도는 변경하지 않는다.
- 모든 campaign은10쌍,5AB+5BA다. CPU2에 controller/worker를 고정하고 sibling CPU18을 관측한다. campaign 직전10초 동안 CPU2/18 busy가 각각5% 이하일 때만 실행한다. 자기 소유 build/test는 측정과 겹치지 않는다. 다른 작업과의 공유 host라는 한계 및 경계/종료 busy·load/process 관측을 보존하며 CPU governor/sysctl/서비스를 바꾸지 않는다. 유휴 조건이 없으면 측정 시작 없이 최대10분 기다리고 NOT_RUN으로 남긴다. 이미 시작한 실패 campaign을 유리한 결과를 얻기 위해 반복하지 않는다.

### 측정 예산·판정·중단

1. C1 탐색 Detailed10쌍1campaign만 실행한다(order seed2026102501). correctness PASS·validity VALID·guardrails PASS이고 elapsed paired 중앙 비율이1 미만이면 후보/분석 규칙을 그대로 동결하고 확인으로 간다. 아니면 중단·기각한다.
2. 새 Detailed 확인10쌍1campaign(order seed2026102502)을 사용한다. 탐색 run ID와 겹치지 않으며 exploration contract/hash·후보 ELF를 결합한다. 주 지표는 기존 `measurement.elapsed_ns`; 기존 분석기의 run-level 순서통계 구간 `[1,8]` 상한이0.95 미만인 IMPROVED와 correctness PASS·validity VALID·guardrails PASS를 모두 요구한다. 구간의 IID 가정과 표시 nominal coverage1002/1024를 통계적 보장으로 확대하지 않는다.
3. 확인이 통과한 경우에만 Off/Always의 각각10쌍 guard campaign을 한 번씩 실행한다(order seed2026102503/2026102504). 이들은 효과 선택용 후보 탐색에 피드백하지 않는 사전 고정 guard이며 recipe kind는 기존 Exploration을 그대로 사용한다. 두 모드 모두 correctness PASS·validity VALID·guardrails PASS여야 채택한다. 부수 효과가 좋아 보여도 Detailed 확인 실패를 구제하지 않는다.
4. 모든 모드의 pair별 guard는 기존 elapsed/CPU/external1.15배, sample p99 1.25배, RSS 증가4MiB다. incomplete/missing·실패·drift를 제거하거나 한도를 바꾸지 않는다. 주 지표 하나의 확인1회만 채택 근거이며 여러 mode에서 유리한 결과 하나를 고르는 규칙이 아니다.
5. 최대4campaign80실행과 별도 PMU2실행이다. 확인 실패/불확실·guard 실패 시 추가 tuning/확인 없이 기각하고, 변경 관련 정답 결과와 원 실패를 보존한다. 기존 종료한3후보 예산에 이번을 붙이지 않는다. P2 한정 채택도 원 PF04 네 campaign FAIL이나 영속 경로를 PASS로 바꾸지 않는다. 새 instrumentation audit는 이번 예산에 포함하지 않는다.

최종 reviewer가 계약·후보 diff·선택 시험·모든 실행 원자료와 채택/기각을 독립 확인한 뒤 단계 commit한다. P2 계획 상태: **독립 설계 검토 PASS 채택 / 계획 commit `a64ca58` 뒤 실행·후보 기각 종료**. 검토는 계획만 대상으로 하며 구현·성능의 통과가 아니다. receipt는 `/home/et16/xadb-dev2-evidence/pf04-k0-owner-20261025/p2-plan-review.json`, SHA-256 `e7c1c90fa901d2feb9743539745e82b229411d98076099c8b71c63c6fd60f002`다. 검토 P2 본문 projection SHA-256은 `f1132d8a0589fa80a0e58b07e11d333fbd05631bc423cf22aaef6159b1205f03`이며 이 상태 문장 앞까지를 고정한다. 차단·미해결 지적0개다.

<a id="p3-io-cause"></a>

## P3 실행 계약: 영속 구간의 journal·block 관측

2026-10-25 후속 “이어서 해주세요”에 따라 P3를 시작한다. D2 담당 `/root`, 독립 검토 `/root/pf04_evidence_review`. 본 계약의 독립 검토·선행 commit 뒤 관측 도구를 구현한다. P2 C1 기각은 유지하고 제품/WAL·fsync 의미·측정 workload·인수 기준은 바꾸지 않는다.

### 질문·환경·관측 의미

현재 fixture는 `/dev/nvme1n1p2`의 공유 ext4이며 시스템과 journal을 공유한다. 다른 디렉터리는 격리가 아니다. `/tmp`·`/dev/shm`의 tmpfs는 collector 기록에만 쓰며 제품 fixture는 기존 ext4에 둔다. 미사용 NTFS 장치를 mount/포맷하거나 재부팅·서비스/전력/governor/sysctl/mount 옵션 변경을 하지 않는다.

기존 worker fsync→journal transaction/phase 관측에 동일 boot·CLOCK_MONOTONIC(`nsecs(monotonic)`) 시간축의 실제 block request 시작→완료를 추가해, 대기가 FLUSH/FUA를 포함한 어느 요청 구간과 겹치는지 구분한다. 동시에 P1 PMU/rusage로 현재 Off/Always의 CPU 작업을 별도로 관측한다. 이전 실패 pair를 새 관측으로 설명했다고 주장하지 않는다. request 시작→완료는 커널 요청 경과 시간이며 순수 NVMe service time이 아니다. 공유 journal·device 요청의 시간 겹침은 해당 WAL만의 소유나 배타적 인과 귀속이 아니다. 누락·requeue·중복·불완전 경계는 명시하고 비율 합산으로 가리지 않는다.

### 구현·권한·경계

- XADBTest의 새 Python 수집기/분석기·BPF source·선택 시험으로 제한한다. P1 Runner를 조립/상속하여 socket·PMU·worker의 기존 bounded lifecycle을 재사용한다. 제품과 기존 도구의 무관한 경로를 수정하지 않는다. 새 source는 구현과 함께 빌드 색인에 등록한다.
- worker를 일반 사용자로 생성해 stdin 앞에서 기다리게 한 뒤 BPF의 전체 attach 알림·정확 probe 수·살아 있는 collector를 확인하고 payload를 전달한다. P1 HELLO/START/STOP으로 원 측정구간을 연결하며 worker의2초 교환 deadline은 늘리지 않는다. 수집은 warmup/후처리도 포함하므로 분석에서 원 측정 timestamp와 실제 source WAL inode를 사용해 양 경계 안의 완전한512개 fsync pair를 선택한다. boot_id와 clock domain·초기 time namespace 및 monotonic offset0의 근거를 raw에 결합한다.
- BPF는 worker의 ext4 sync enter/exit와 jbd2 wait enter/exit, 대상 filesystem의 journal phase, 대상 disk의 raw `block_rq_issue`/`block_rq_complete`/`block_rq_requeue`를 관측한다. BTF와 실행 kernel에 맞는 필드·flag 의미를 확인한다. request pointer는 수집 내부의 일시적 대응 키이며 영속 ID가 아니다. start/end 재사용·오류/partial/requeue가 있는 연결은 정상 요청으로 추정하지 않는다. 개별·NVMe batch·internal FLUSH가 공통 complete tracepoint를 지나는 실제 source/BTF를 대조한다. 0byte flush·partial completion·partition/whole-disk dev·FUA flag를 구분한다. cross-CPU 출력 순서를 신뢰하지 않고 monotonic timestamp와 요청별 시작 세대로 대응하며 동일시각 모호성은 거절한다. race-safe count()의 event별 kernel count와 종료 후 저장 행 수를 정확히 대조하고 유실·warning·map overflow는 분석 거절 사유다. capture 경계의 미완료 요청도 명시하며 대응이 불완전하면 전체 비율을 거절한다.
- 설치된 root 소유 sudo/python3/setpriv/bpftrace와 검토된 전용 Python supervisor만 명시 호출하고 최소 환경·고정 프로그램/인자·새 private tmpfs spool을 사용한다. supervisor는 임의 명령을 받지 않으며 고정 BPF source·PID/device 정수 인자만 받는다. BPF attach에는 root가 필요하며 worker는 승격하지 않는다. 전역 hook은 worker PID, filesystem dev 또는 disk dev로 일찍 필터한다. 고정 memory/map·record 크기와 wall deadline을 적용하고 link를 pin하지 않는다. root collector의 전체 attach 후 준비 실패·취소·정상 종료와 독립 watchdog의 parent-death 정리를 선택 검증한다.
- 일반 사용자 runner→sudo/root supervisor→별도 session의 root BPF topology와 각 UID/PID/starttime을 기록한다. supervisor가 직접 생성한 unreaped BPF leader와 pidfd를 유지하고 모든 signal을 직접 보낸다. runner는 pipe로 STOP/CANCEL을 요청하며 EOF도 parent 상실로 처리한다. root supervisor 내부 독립 monotonic deadline은 생성부터55초이며 cleanup TERM/INT→KILL·수거에 최대3초를 더 쓴다. setpriv의 parent-death KILL로 supervisor 강제 사망도 제한하되 실제 일반 사용자 parent 사망 사례에서 검증한다. 정상 종료는 producer disable ACK→고정1초 drain→collector INT→수거다. drain을 CPU barrier로 주장하지 않으며 종료 후 count·행·미완료 상태를 대조한다. 오류에서는 직접 소유 child/group만 bounded 종료하고 원자료·실패/정리 상태를 보존한다. slot의 Python 수집 deadline은 유휴 대기 뒤 worker 생성 전부터60초이며 오류 원자료 보존·cleanup에는 추가8초까지 허용한다. 유휴 대기10분은 이 per-slot deadline 밖이다. raw 각64MiB, map 최대8192 key, event 최대200,000행이다. 제한을 넘으면 FAILED/INCONCLUSIVE이고 기존 결과를 덮어쓰지 않는다.

### 변경별 시험과 고정 native 예산

새 분석기의 수동 vector로 interval clip/union·중첩·동일 timestamp·request 재사용/중복/고아/requeue·오류·512개/구간/inode 불일치·count/loss·알 수 없는 record 거절을 검증한다. 수집기의 fixed argv/필터·identity·deadline/취소·준비 실패·소유 cleanup·원자료 보존을 새 사례로 선택한다. native helper는 정상·attach 뒤 payload 전 실패·활성 취소·runner EOF 각1개다. 구현 검토에서 root supervisor 강제 사망의 PDEATHSIG 설정 전 race와 독립 deadline의 별도 증거가 필요해, 고정 parent 검증 launch를 보완하고 supervisor 강제 사망·실제55초 watchdog 각1개를 추가했다. helper 총6개이며 앞선4개는 반복하지 않고 제품 연결4slot은 유지한다. 구현 결함 때문에 실패한 경우 실제 수정 diff/실패 사례만 다시 검증하고 최초 실패를 보존한다. 기존 제품·하위 회귀·전체 wrapper·완료 P1/P2 시험은 반복하지 않는다.

clean export 기준선은 `5253caa`이며 병행 DEV3 변경을 제외한다. 제품 source는 그대로 두고 이번 새 도구만 결합한 release worker를 사용한다. 실행 input은 기존 영속 seed20261018·128행·warmup64·측정512다. 새 도구의 실제 연결·영향을 검증하는 native는 아래4slot만 허용하며 원 제품 인수 반복으로 세지 않는다.

| slot | 제품 observations | 추가 BPF | 목적 |
| --- | --- | --- | --- |
| 0 | Off | 꺼짐 | 같은 P1 구간/PMU의 현재 대조 |
| 1 | Off | 켜짐 | fsync/journal/request 연결 |
| 2 | Always | 켜짐 | 동일 관측에서 CPU·진단 작업 구분 |
| 3 | Always | 꺼짐 | 반대 순서의 현재 대조 |

모든 slot에서 P1 PMU를 수집하고 raw 진단 path와 원 scalar/ACK/image 정답을 기존 독립 checker로 검산한다. 관측자는 verdict를 제품에 넣지 않는다. CPU2 worker, CPU3 runner/collector, sibling18을 관측한다. 각 slot 전10초 CPU2/18·대상 disk busy 각각5% 이하일 때만 실행하고 자기 소유 build/test를 겹치지 않는다. 유휴 조건은 최대10분 대기 후 NOT_RUN, 시작한 실패 slot은 재실행하지 않는다. 뒤 slot 진행은 앞 slot의 정답/수집/정리 완결에만 의존하며 유리한 성능 수치 때문에 선택하지 않는다.

이는 성능 최적화의10쌍 확인이 아닌 도구의4개 신규 연결 사례다. 모드별 traced/control elapsed·기존 durable cycle-duration p99(ns)는1.25배, CPU는2배를 넘으면 coarse 영향/환경 screen FAIL로 표시한다. 한 쌍씩의 screen PASS도 순수 tracing overhead 추정이나 안정적 모드 비용의 입증이 아니다. screen FAIL이면 해당 추적의 대응 사실만 기록하고 untraced 비중으로 일반화하지 않는다. 원 PF04 한도를 이 screen으로 대체하지 않는다. kernel request 겹침·PMU 한 쌍으로 P4 WAL 변경을 자동 채택하지 않는다.

종료 산출물은 source/binary/kernel/device/recipe·원자료·정답·필수512 대응/누락·CPU 관측·영향 screen·cleanup과 독립 검토다. 충분히 연결되지 않은 원인은 INCONCLUSIVE로 기록하고, 그 원인을 구분하려면 무엇을 추가로 관측해야 하는지 구체화해 P3 commit한다. P4는 충분한 근거와 별도 형식/복구 계약 없이는 시작하지 않는다.

P3 계획 상태: **PASS_PLAN_ONLY 채택**. 계획 채택 당시 구현·실행 전이었으며 이후 실행은 [P3 증거](../evidence/dev2-pf04-io-cause-20261025.md)를 따른다. 독립 검토자가 clock·완료 경로·root child 소유·EOF·예산 보완본의 차단0을 확인했다. 전용 supervisor의 control pipe는 BPF에 상속하지 않고 BPF child에만 parent-death KILL을 적용한다.

## 검토·상태 기록

- P0 계획: `/root/pf04_evidence_review`의 `PASS_PLAN_ONLY`를 채택했다. R1 권한 부재 대체 경로, R2 raw PMU·FIFO deadline, R3 시험 소유 구분을 반영했고 차단/미해결 지적은0개다. 검토 뒤 상태·receipt만 기록하고 계획 commit을 생성한다.
- 검토 receipt: `/home/et16/xadb-dev2-evidence/pf04-analysis-20261025/plan-review.json`, SHA-256 `f0b15ad3063f9f02716c2d3f1d812bf9c8048c02e3d840b20affc50f9c3571c2`. 검토 본문 projection SHA-256 `903f30baec57dfb17855debd644e725887b5a9471b2ca32e42dfe64dc69d2124`는 상단 상태와 본 절을 제외한다. 계획 채택은 구현·시험 실행 증거가 아니다.
- P1 도구 구현·선택 검증: 계획 commit `82c14ba` 뒤 수행했다. [source별 실행 기록](../evidence/dev2-pf04-window-20261025.md)을 따르며 당시 일반 사용자 PMU는 UNAVAILABLE이었다. 승인된 후속의 [실제 PMU 수집·선택 검증](../evidence/dev2-pf04-pmu-20261025.md)을 별도 연결하며 최종 독립 검토는 각 기록의 채택 상태를 따른다. P0 검토 projection은 계획 commit 시점의 본문에 해당한다.
- P2: 계획 commit `a64ca58` 뒤 C1 구현·선택 검증·확인 측정을 수행했다. [후보 기각 증거](../evidence/dev2-pf04-k0-owner-20261025.md)의 확인 구간 상한0.990642가 동결0.95 미만 조건을 충족하지 못해 제품을 원복하고 후보 평가를 종료했다. Off/Always guard는 NOT_RUN, 최종 검토는 증거 기록을 따른다. P2 종료 당시 P3–P4는 진입 전이었으며 별도 구체 검토를 유지한다.
- 원 PF04 네 실패와 DEV2/B2/D0 INCOMPLETE: 유지.

- P3: 계획 commit `360961b` 뒤 [도구·선택 관측](../evidence/dev2-pf04-io-cause-20261025.md)을 완료했다. helper 새 경계2개를 추가해6개로 한정 검증하고 제품 연결4slot은 유지했다. 정답과512 fsync/wait 연결은 확인했으나 요청 전체 비율은 INCONCLUSIVE·Always 영향 screen FAIL이다. P3 후속 provenance 관측과 원 CPU 한도 설명이 남고 P4는 시작하지 않았다.


<a id="p3-flush-provenance"></a>

## P3 후속: 물리 완료와 flush 순서 완료 구분

2026-10-25 지속 목표 “이 작업을 완료”에 따른 D2 후속이다. 담당·통합 `/root`, 독립 검토 `/root/pf04_evidence_review`. 앞선 P3의 두 요청 비율 INCONCLUSIVE와 Always 영향 FAIL은 보존한다. 계획을 먼저 commit하고 도구의 실제 변경 범위만 검증한다. 제품 원인·개선 단계로 진행하기 위한 정보이며 새 성능 합격을 만들기 위한 반복이 아니다.

실행7.0.0-38 kernel의 적용 `blk-flush.c`에서 `blk_flush_complete_seq`는 입력 seq를 누적하고 다음 단계가 DONE이면 `blk_flush_restore_request` 뒤 `blk_mq_end_request`를 호출한다. `blk_flush_restore_request`가 `RQF_FLUSH_SEQ`를 지우고 saved_end_io를 복원한다. 새 `fentry:vmlinux:blk_flush_complete_seq`가 현재 BTF에 존재함을 확인했다. 함수 진입의 rq pointer·seq_before·action·error·rq_flags·cmd_flags·data_len·end_io·saved_end_io를 수집하고, 기존 ISSUE/COMPLETE에도 rq_flags와 end_io를, COMPLETE에는 cmd_flags도 추가한다. free 뒤 rq를 읽는 fexit는 사용하지 않는다. probe16개·clock/identity/권한/한도/worker 경계와 원 workload는 앞 계약을 유지한다.

분석은 이번 kernel의 `RQF_FLUSH_SEQ=1<<1`, `REQ_FSEQ_ACTIONS=7`에 결합한다. seq_before와 action은0..7의 서로 겹치지 않는 mask이고 action=0·다중 bit도 합법이다. error=0이며 `(seq_before | action)==7`인 실제 DONE 전환 marker만 요청 세대별1:1로 소비해 같은 pointer의 다음 COMPLETE와 연결한다. marker 후 다른 ISSUE·중복 marker·동일 timestamp·error·필드 불일치·순서 역전·미완료는 unknown으로 남긴다. 논리 완료는 rq_flags에서 FLUSH_SEQ만 지워지고 end_io가 saved_end_io와 같으며 cmd_flags/data_len이 marker와 일치하는0byte 성공인 경우에만 구분한다. marker 없는0byte 완료는 예전처럼 고아다. 물리 요청 구간과 논리 종료를 별도 반환하고 원 event/kernel count를 줄이지 않는다. physical/logical/unknown 완료 census의 합은 모든 COMPLETE 행 수와 일치해야 한다. v2 schema marker를 명시하고 과거 v1 원자료의 판정은 보존한다.

journal wait의 transaction과5개 지점을 연결해 start→locking→flushing→logging→end의4구간과 wait의 앞/뒤 잔여 겹침을 산출한다. 순서·transaction 재사용·누락·동일시각·경계를 별도로 다룬다. commit sequence 공개가 end_commit trace보다 먼저이므로 정상 wait가 end marker 전에 반환할 수 있다. end_commit 대비 반환 시차는 부호를 보존하며 순수 scheduler 지연이라고 단정하지 않는다. phase/request가 겹쳐도 공유 journal의 배타적인 WAL 소유를 뜻하지 않는다. 이상·경계가 남으면 해당 분모를 거절하며 전체 순수 device service time을 주장하지 않는다.

새 선택 vector는 v2 정상/고아0byte/유효 logical completion/marker 충돌·잘못된flag/end_io·오류/partial·재사용·경계, v1 보존, journal 단계 clip·순서/누락/불완전 transaction이다. root 수명 코드는 바꾸지 않으므로 이전 native owner6개는 반복하지 않는다. fixed probe count/source 연결만 새 실제 관측에서 확인한다. 외부 driver는 CPU2/18 정확8필드·비음수delta·양수total/time·disk delta와 per-process affinity를 실행 전에 검증하도록 보강하고 선택 부정 vector를 갖춘다.

native 예산은 새16-probe/data 경로를 검증하는 Off/control→Off/trace→Always/trace→Always/control 각1개, 총4slot이다. 제품은 앞 P3의 보존5253caa source/ELF 그대로 사용한다. 재빌드·기존 제품/하위 시험은 없다. 실행 전 source/hash·probe/clock·기존 고정 input·CPU2/18와 disk idle5%·deadline60초·coarse screen을 별도 recipe로 동결한다. started 실패는 재시도하지 않고 원자료를 보존한다. helper 새 시험이 필요하면 실제 수정/새 경계에 한정하며 예산을 실행 전에 기록한다. 영향 screen의 elapsed/p99 1.25·CPU2배는 기존대로이며 모드별 한 쌍으로 원 PF04 인수를 대체하지 않는다.

결과를 통해 물리 요청과 journal 대기의 위치를 구분한 뒤 P4 WAL 후보를 채택/기각한다. K0·영속 CPU 한도는 별개 완료 조건이다. 이 후속 관측만으로 목표 완료나 PF04 PASS를 선언하지 않는다.

후속 계획 상태: **PASS_PLAN_ONLY 채택 / 채택 당시 구현·실행 전**. 이후 실행은 [후속 증거](../evidence/dev2-pf04-flush-provenance-20261025.md)를 따른다. 독립 검토 차단0개. receipt는 `/home/et16/xadb-dev2-evidence/pf04-flush-provenance-20261025/plan-review.json`, SHA-256 `401defe67b60b3d0632f56fb4c1938796b538ec959782089c2578823f3b2f727`이다. 본 anchor부터 이 상태 문장 직전까지의 검토 projection SHA-256은 `daaa7627e6b0a2f6cf4669224f9eafd1c74d1cf20fa474b4203002aa6c9b14aa`다. 계획 채택은 구현·실행 통과가 아니다.

<a id="p3-stack-output"></a>

## P3 수집 오류 보완: 출력 버퍼를 호출별 stack으로 제한

후속 계획 commit `f157846` 뒤 source-r1..r4의 신규11개 단위 사례(관련 변경 재검증 포함12회)와 BPF codegen을 확인하고 고정 native를 실행했다. Off/control과 Off/trace는 수집·정답을 통과했다. Always/trace는 제품 scalar·ACK/image 검산과 수집 종료 뒤 원 kernel count와 행 수가 불일치해 **FAILED**다. 마지막 Always/control은 시작하지 않았다. 원자료는 `/home/et16/xadb-dev2-evidence/pf04-flush-provenance-20261025`에 보존하고 원 FAIL을 재분류하지 않는다.

Always 원행10198/10199의 COMPLETE는 timestamp·pointer·전체 payload가 동일하다. ISSUE는 kernel2712/저장2711, COMPLETE는 kernel3299/저장3300이다. 설치 bpftrace0.25가 만든 IR에서 두 probe는 같은 CPU별 `__bt__fmt_str_buf[cpu][0]`를 공유하고 각각 format ID11/12와56byte를 출력한다. 공식 v0.25.0 소스와 기본 `on_stack_limit=32`도 대조했다. 같은 CPU의 중첩 호출이 공유 버퍼를 덮는 경로는 관측과 일치하는 유력 원인이다. 당시 CPU/IRQ 중첩은 수집하지 않았으므로 단일 원인 확정으로 쓰지 않는다.

D2 선택은 고정 BPF config의 `on_stack_limit=128`이다. 현재 최대72byte의 모든 printf payload를 호출별 BPF stack에 놓아 이 공유 경로를 제거한다. 커널 전역·설치 bpftrace·worker·root owner 수명·count/output 검증·schema와16probe를 바꾸지 않는다. raw를 중복 제거하거나 kernel count를 맞추는 대안은 증거를 왜곡하므로 기각한다. 별도 BPF 프로그램/사용자 수집기 교체는 이번 결함보다 범위가 커 제외한다.

수정 뒤 선택 검증은 실제 compiler가 만든 optimized IR에서 printf 공용 scratch 참조가 없고 각 출력이 stack allocation을 쓰는지 확인하는 신규 정적 사례1개와, 새 수집기의 실패했던 Always/trace 연결1개다. 미실행 Always/control1개를 이어서 실행해 해당 모드의 coarse 영향 screen을 완성한다. Off의 완료 연결과 기존 root owner6개·하위/제품 인수는 반복하지 않는다. 원 Always 실패와 새 source의 검증을 별도 디렉터리/recipe/hash로 구별하며 제품 ELF·input·idle·deadline·quality 기준은 유지한다. 새 trace도 실패하면 이 보완 예산에서 반복하지 않는다. 새 소스 native는 총2slot이며 실패한 동일 코드를 다시 돌려 통과를 찾는 실행이 아니다.

이 보완은 바뀐 도구의 오류 경로를 검증하기 위한 한정 예산이다. 원4slot의 started 실패 무재시도 규칙을 불리한 성능 값의 재시도 허용으로 확대하지 않는다. 상위 사용자 지시의 수정 코드 필수 시험 범위에 해당하며, Off/Always의 서로 다른 수집기 source를 합쳐 하나의 성능 확인 표본으로 쓰지 않는다. P4와 K0/영속 CPU 원인 종료 검토는 계속 남는다.

Off/trace의 원 count·연결 검사 통과는 보존한다. 다만 같은 공유 scratch 수집기여서 count가 맞아도 조용한 payload 오염이 전혀 없었다는 증명은 아니다. 이 해석 한계를 추가하고 완료 사례를 반복하거나 원 판정을 소급 변경하지 않는다.

수집 오류 보완 계획 상태: **PASS_PLAN_ONLY 채택 / 채택 당시 수정 전**. 이후 변경 검증은 [후속 증거](../evidence/dev2-pf04-flush-provenance-20261025.md)를 따른다. 독립 검토 차단0개. receipt `/home/et16/xadb-dev2-evidence/pf04-flush-provenance-20261025/stack-plan-review.json`의 SHA-256은 `43f62dfc789a63e9da8ec1c7a806e03767fa5f828e60525ad3d61572d74a93f1`, 본 anchor부터 상태 문장 직전까지의 검토 projection은 `ef290419c358a4456feb992a24bb63357b621fbcef286b24704c5feb2456fbad`다.

<a id="p4-wal-decision"></a>

## P4 판단: PF04 해결책으로서 WAL 변경은 미채택

P3b는 `688b9af`에서 [한정 검증·독립 검토](../evidence/dev2-pf04-flush-provenance-20261025.md)를 마쳤다. 수정 Always의 fsync512개와 물리 요청·journal phase를 연결했지만, 공유 장치의 대기 겹침은 원 PF04 상대 비용 실패의 인과 증명이 아니다. 고정 계측 CPU 비용을 둔 채 Off/On 공통 sync 시간을 줄이면 상대 비용은 오히려 커질 수 있다. 따라서 이번 PF04 해결책으로 WAL 선확보/data sync를 채택하지 않는다. 전체 WAL 성능 개선을 영구 기각한다는 뜻은 아니다.

설치 커널 Ubuntu `7.0.0-38.38`의 보존 소스 `fs/ext4/fsync.c`에서 data sync도 `i_datasync_tid`의 journal commit을 기다린다. `fs/ext4/inode.c:ext4_do_update_inode`는 on-disk 크기가 달라지면 `need_datasync=1`로 기록하며 delayed-allocation write-end/writeback도 크기 갱신을 수행한다. PostgreSQL 로컬 revision `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의 `XLogFileInitInternal`은 segment 준비·sync를 별도 수행한다. XADB의 현재 물리 EOF·유효 tail·quota·rotation·backup·복구 계약을 그대로 둔 한 줄 치환과 같지 않다. 원 소스와 해석 범위는 후속 증거에 연결한다. 이 읽기 전용 판단에 제품 시험을 실행하지 않는다.

<a id="c2-single-writer"></a>

## C2: K0 단일 쓰기 소유자의 통계 갱신

2026-10-25 후속 “이어서 해주세요”에 따른 D2 후보다. 작업·계약·통합 담당 `/root`, 독립 검토자 `/root/pf04_evidence_review`. 본 계획을 검토·commit한 뒤 구현한다. C1의 Arc 소유권 통합과 종료된 탐색은 재개하지 않는다. 새로운 근거는 실제 mutation callsite의 단일 writer 소유권이며 C2 하나만 허용한다. PMU 이벤트 확장은 구현하지 않는다.

### 소유권과 보존할 의미

`VolatileStore`는 복제되지 않으며 Create/Drop/Insert/Replace/Delete는 각각 `&mut self`에서만 시작·완료한다. 이 다섯 label의 started/completed/failed는 writer가 하나이므로 relaxed atomic load→checked_add→store로 정확히 갱신할 수 있다. 다른 thread의 snapshot은 별도 atomic 관측만 수행한다. Snapshot/Lookup/Schema/Read/Scan/NextPage는 여러 작성자가 가능하므로 기존 CAS 갱신을 유지한다. 포화 때 값은 MAX를 유지하고 saturated를 공개하며 panic을 completed로 세지 않는다.

단순 `&mut Diagnostics`는 clone된 collector와 경쟁할 수 있으므로 충분하지 않다. 비 Clone writer wrapper가 Diagnostics 하나를 소유하고, store만 이 wrapper를 보유한다. snapshot/cursor에는 clone 가능한 shared view만 전달한다. 내부 MutationOperation5개와 SharedOperation6개를 분리하여 shared 갱신으로 mutation label을 선택할 수 없게 한다. shared view의 constructor는 모듈 내부로 좁힌다. wrapper는 추가 Arc·heap allocation·quota charge를 만들지 않는다. 기존 공통 Counters, row/quota/root/ID/오류/공개 순서, dropped/emitted/sequence/queue의 원자 경로, clock·부하·WAL은 유지한다. 별도 포화 전 fast/wrap 경로나 unsafe는 도입하지 않는다.

### 변경별 검증과 비교 예산

- 기준선은 `688b9af`의 깨끗한 export, 후보는 그 export에 C2의 diagnostics/substrate와 관련 시험 diff만 적용한다. 병행 DEV3 변경은 포함하지 않는다. release 설정·target·toolchain은 양쪽에서 같게 한다. 후보가 기각되면 diff·source·원 결과를 보존하고 제품을 기준선으로 복원한다.
- 새 unit은 writer label별 성공/실패·MAX-1/MAX 포화·started 후 panic/미완료·writer 종료 뒤 shared view 수명과 동시 shared 읽기/drain을 검증한다. 새 API가 mutation label을 shared 경로로 기록하지 못하고 writer가 Clone이 아닌 경계는 compiler 반례로 확인한다. 기존 `performance`의 mode별 값/오류/quota, 모든 label의 성공/실패, concurrent snapshot/read3개만 직접 변경 경로로 선택한다. 새 wrapper와 shared 호출 형태가 바뀌는 기존 diagnostics4개는 관련 경계로 선택한다. 실행 전 실제 diff에 맞춰 exact selector를 기록한다. 무변경 하위/전체 workspace·완료 사례 반복은 없다. 관련 fmt·engine/worker Clippy는 시험 실행과 구분한다.
- 비교 입력은 기존128행·old snapshot·capacity256·warmup4096·측정65536·stride16·seed20261018 그대로다. controller/analyzer·인수 한도도 유지한다. 각각10쌍,5AB+5BA로 실행한다. C2의 새로운 탐색은 Detailed1campaign(order seed2026102511), 확인은 새 Detailed1campaign(2026102512)뿐이다. 탐색의 correctness PASS·validity VALID·guardrails PASS 및 elapsed 중앙 비율<1일 때만 확인한다. 확인은 기존 순서통계 구간 상한<0.95인 IMPROVED와 세 품질 판정을 모두 요구한다. C1의 기준을 낮추지 않는다.
- 확인이 통과한 경우에만 Off/Always 각10쌍 guard(2026102513/2026102514)를 한 번씩 실행한다. pair별 elapsed/CPU/external1.15, sample p99 1.25, RSS 증가4MiB를 그대로 적용한다. 최대4campaign80native 실행이며 실패/불확실이면 추가 tuning·재확인 없이 중단·기각한다. 이번에는 별도 PMU native를 추가하지 않는다. 유리한 부수 지표나 실패 pair 제거로 채택하지 않는다.
- CPU2 controller/worker, sibling18 관측, 각 campaign 직전10초 CPU2/18 busy 각각5% 이하, 최대10분 유휴 대기를 유지한다. CPU key/길이/비음수 delta/양의 합을 검증하고 자기 소유 build/test와 측정을 겹치지 않는다. 프로세스·원 output·실패·deadline·source/ELF를 보존한다. 실행 wrapper의 timeout 때 원 stdout/stderr를 저장하고 소유 group을 bounded 정리한다. 재시작·서비스/전력/governor/sysctl 변경은 없다.

후보 확인 통과는 C2의 한정 개선 증거다. 원 PF04 네 FAIL·DEV2 미완료를 자동 해제하지 않으며 영속 CPU/tail 실패도 별개다. 최종 독립 D1/D2가 소유권·반례·선택 시험·모든 원 측정과 채택/기각을 확인한 뒤 단계 commit한다. 동일 코드 인수를 다시 돌릴 명목으로 본 계획을 확장하지 않는다.

C2 계획 상태: **PASS_PLAN_ONLY 채택 / 채택 당시 구현·실행 전**. 이후 [후보 평가](../evidence/dev2-pf04-single-writer-20261025.md)에서 확인 기준 미달로 제품 미반영·기각했다. 독립 검토 차단0개. receipt `/home/et16/xadb-dev2-evidence/pf04-single-writer-20261025/plan-review.json`의 SHA-256은 `ac13e33f7cab54184a653d7847808918f96d7fc4126f2e06cf103ff40683e8c7`, P4 anchor부터 이 상태 문장 직전 본문의 검토 projection은 `7c9d96fec4da315d185fe72889d0b10a8004b52222e575cd7254dfb83c67f3dc`다.

<a id="cpu-user-kernel"></a>

## 영속 CPU 후속: 사용자·커널 PMU 분리

C2는 `c2822d9`에서 기각·미반영 기록을 마쳤다. 남은 원 영속 CPU 한도 FAIL의 관측 공백을 다룬다. D2 담당·통합 `/root`, 독립 검토 `/root/pf04_evidence_review`. 계획을 먼저 commit한 뒤 도구만 보완한다. 제품·WAL·부하·판정 한도와 종료된 C1/C2 탐색을 변경하지 않는다.

기존 PMU는 user-only cycles/instructions여서 같은 thread의 커널 CPU를 구분하지 못한다. 새 고정 `user-kernel-v1` profile은 `cycles:u`, `cycles:k`, `instructions:u`, `instructions:k`, `ref-cycles:u`, `context-switches`, `cpu-migrations`, `page-faults`8개를 같은 enable/disable bracket에서 수집한다. 원6개는 기본 `user-v1`으로 유지한다. 임의 event 문자열·시스템 전체·다른 thread 수집은 허용하지 않는다. profile 선택을 argv/결과에 기록하고 parser의 예상 event 집합과 결합한다. raw unscaled·각 runtime/coverage·missing/unsupported와 원 rusage를 모두 보존하며 partial/0/missing에서 정상 비율을 합성하지 않는다.

기존 CAP_PERFMON, 일반 사용자 worker, fixed trusted tools·identity·pidfd·deadline·정리 정책은 유지한다. 새 privilege/sysctl/재시작·서비스 변경은 없다. PMU kernel count는 해당 task가 실행 중인 privilege-domain 관측이며 syscall별 비용·다른 task의 journal 작업·순수 workload CPU 또는 GHz로 해석하지 않는다. 두 번의 새 연결로 안정적 On/Off 비용이나 과거 실패의 단일 원인을 확정하지 않는다. rusage user/system 분할도 PMU와 같다고 강제하거나 차이를 보정하지 않는다.

선택 검증은 새 profile의 literal 정상/부재/부분 coverage/중복/다른 profile event 거절·잘못된 선택의 spawn 전 거절·CLI→fake perf→최종 parser 연결이다. 기본 parser와 fake 연결은 실제 예상집합/호출 연결 변경에 대응하는 기존2개만 선택한다. Rust hook, 기존 privilege/lifecycle suite·BPF·하위 회귀·제품 인수는 반복하지 않는다. fake 출력은 독립 literal profile로 제공하며 실제 kernel PMU라고 표시하지 않는다.

Native 예산은 새 profile의 durable Off1회→Always1회 **총2slot**이다. P3의 보존 release ELF `ea1f0f7c5f2d58a61199f399ae6961f290449dea488a9187ebf11505bc501c51`, source `7ced0a5622825b971a4a146cdd8d1ce4a5ef01eefe633e59deb6b338390a5329`,128행·warmup64·measure512·seed20261018을 재사용한다. rebuild·BPF 추가·원 인수 재실행은 없다. CPU2 worker·CPU3 runner/perf, sibling18 관측과 시작 전10초 CPU2/18·disk busy 각각5% 이하, 유휴 대기 최대10분을 유지한다. worker affinity를 payload 전 확인한다. 총60초 deadline과 기존 owned cleanup·원자료 보존을 적용하고8event full coverage·정답·identity·원512거래 검산을 요구한다. 실패·미지원·유휴 부족이면 원 사유를 남기고 남은 slot도 NOT_RUN으로 종료하며 시작한 같은 소스를 반복하지 않는다. Off/Always 순서나 미리 정한 입력을 수치 때문에 바꾸지 않는다.

결과는 새로운8event 연결과 source별 관측 범위로 기록한다. 같은 collector의 별도 control과10쌍 확인이 없으므로 영향/모드비용 screen 또는 PF04 합격을 만들지 않는다. 원 실패와 미확정 원인은 유지한다. user/kernel counter가 무엇을 구분했고 무엇을 구분하지 못했는지 기록·독립 검토한 뒤 단계 commit한다.

CPU 분리 계획 상태: **PASS_PLAN_ONLY 채택 / 채택 당시 도구 수정·실행 전**. 독립 검토 차단0개. receipt `/home/et16/xadb-dev2-evidence/pf04-cpu-split-20261025/plan-review.json`의 SHA-256은 `38d36549d547b91609ffd187592b6d92aa91f6bdc3c43c28e5ec63e357e3ea7c`, 본 anchor부터 상태 문장 직전 projection은 `f1d0423a5a505ee049b4c82390c2cb02387f493bfbac1929b1b99feb97ad981a`다.

CPU 분리 실행: 선행 계획 commit `b33f1e4` 뒤 도구를 구현하고 선택6함수·실제2연결을 완료했다. [관측 증거](../evidence/dev2-pf04-cpu-split-20261025.md)의 원자료·해석·최종 검토를 따른다.8event reported coverage와 진단 정답을 확인했으며 원 PF04·DEV2 완료 판정은 변경하지 않았다.

CPU 집계 읽기 후속: `038236b` 뒤 [설치 커널 source 조사](../evidence/dev2-pf04-cpu-accounting-20261025.md)로 rusage tick 비율 보정·원 thread CPU·perf context runtime 경로를 구분했다. 제품/수집 변경·새 시험0이며 이 의미 차이를 원 전체 CPU FAIL의 무효화나 원인 해결로 처리하지 않는다.
