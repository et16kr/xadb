# COMM 성능 시험의 시간 제한·조합별 분할

2026-10-23 · **D2 시험 도구 수용 / 같은 source10조합200mode BATCH_PASS / 후속2조합40mode RUNNING / 모델·C1 INCOMPLETE**

[시간 제한 정본](../plans/comm-implementation-plan.md#time-bounded-batches) · [원 C-V18](../plans/comm-implementation-plan.md#performance) · [이전 잔여 대장](comm-acceptance-resume-20261023.md)

## 범위와 선택

사용자가 단일8시간 이상 시험은 일단 skip하고, 나누어 할 수 있으면 진행하되 당일23시 전에 마치도록 지시했다. 작업·계약·통합 담당 `/root`, 별도 검토 `/root/comm_acceptance_review`. 기준 commit `b9d3641`에서 기존 COMM 신규 파일·문서와 DEV2 리뷰의 dirty 범위를 보존했다. DEV3·기존 성능 worker·원 workload/통계 oracle·dependency는 수정하지 않았다.

소유 파일은 새 `scripts/check_comm_performance_batch.py`와 `scripts/test_check_comm_performance_batch.py`다. 기존 `check_comm_performance.py`를 변경하지 않고 실제 `run_case`와 원10쌍 통계 검산을 소비한다. 공유 계획·구현·증거·색인은 이번 항목만 순차 반영한다. 작업 baseline·실행 전 diff/선택·일정 판단은 `artifacts/comm/batch-20261023-9w_e63a4/`에 보존한다.

| 대상 | 이번 선택과 이유 |
| --- | --- |
| 전체 C-V18 단일 묶음 | 2,700mode×35초=nominal94,500초(26시간15분)에 추가 준비/정리가 필요. 단일 묶음 실행은 건너뛰고 원 분모 유지 |
| 원 cell37 | TCP IPv6·1연결·65,536bytes·BULK·정상 peer. 기존 원74는 pair1 cell0..36의 두 mode여서 완료 사례와 겹치지 않음 |
| 새 조합별 batch | cell37만10쌍20mode. nominal700초, build/timeout/정리/기록을 포함한 보수3,320초. batch budget3,600초·deadline `2026-10-23T23:00:00+09:00` |
| 깊이8 모델 | 6시간 상한은8시간보다 짧지만 해당 source는 무변경. 이번 재실행 근거가 아니므로 미완료를 보존하고 실행하지 않음 |
| 기존30분3개·11 mutant·원74 mode | 원 source 완료 결과를 보존하며 반복하지 않음. source가 다른 결과를 현재 전체 PASS로 합산하지 않음 |

## 분할·시간 경계

`--cell-index`는 원135조합의 번호이며 누락·중복·범위 밖 입력을 거절한다. `--deadline`은 timezone이 필요하고 이미 지난 시각을 거절한다. 배치 예산이8시간 이상이면 거절하며, 선택 작업의 보수 상한이 deadline 또는 wall budget에 맞지 않으면 build 전에 `SKIPPED_TIME_BUDGET`·완료0으로 기록한다. 실행 중에는 다음 mode와 최종 기록 reserve가 들어갈 시간을 다시 확인한다.

mode별120초 one-shot alarm은 기존 `run_case`의 예외 정리로 이어져 worker kill/reap·FAIL receipt·Unix 임시 경로 정리를 수행한다. active timer가 있으면 덮어쓰지 않고 거절하며 handler를 복구한다. 완료 mode와 시도 mode를 구별한다. 완료20개·원10쌍 통계·guardrail을 모두 충족해도 `BATCH_PASS`이며 C1은 INCOMPLETE다. 누락·중복·잘못된 cell/source/binary·guardrail 실패는 batch PASS가 아니다.

```bash
python3 -B scripts/check_comm_performance_batch.py \
  --cell-index 37 --deadline 2026-10-23T23:00:00+09:00 --batch-seconds 3600
```

후속 조합은 보존 source export 안의 같은 runner로 실행할 수 있다. 이를 통해 live DEV3 변경과 분리한 동일 source의 작은 조합별 receipt를 모을 수 있다. 원 실행기 전체를 자동 호출하거나 이미 완료한 셀을 자동 반복하지 않는다. 별도 전체 coverage 수합 없이 C1을 승격하지 않는다.

## 실행 증거

새 checker8 method의 첫 job은 **7 PASS·1 FAIL**이었다. FAIL은 합성 `run_case`가 실제 helper와 달리 출력 디렉터리를 만들지 않은 시험 준비 오류였다. 해당 fixture를 고친 그 method만 다시 실행해 PASS를 확인했고 나머지7개는 반복하지 않았다. 원 FAIL과 source별 결과는 `checker-{selection,result}.json`·`checker.log`, 후속은 `checker-fix-{selection,result}.json`·`checker-fix.log`에 보존한다.

검증에는 실제로 ready 전에 멈춘 dummy executable을 Popen하고0.5초 alarm으로 종료한 신규 사례가 포함된다. `exit=-SIGKILL`·실제 PID 소멸·reap·FAIL cell receipt·Unix scratch 제거와 alarm/handler 복구를 확인했다. 이는 timeout controller의 검증이며 제품 workload 완료를 주장하지 않는다. 나머지는 합성 receipt·mock으로 selector/deadline/skip·누락/중복·guardrail·source/binary와 attempted/completed 경계를 검사한다.

실행 전 D2 독립 검토는 선택 범위·time budget·분모·정리·실패 보존과 원 FAIL을 읽기 전용으로 확인해 수용했다. 실행 전 검토 뒤 controller에는 uname/Python/CPU 수/load average 기록만 추가했으며 scheduling·timeout·oracle 함수는 유지했다. 최종 source는 아래 실제 native batch가 사용했다.

| 실제 실행 | 결과·범위 |
| --- | --- |
| cell37 원10쌍·Always/Detailed 교대 순서 | 신규20mode 전부 PASS, attempted20/completed20. 5초 warmup+30초 측정·독립 peer/worker frame/byte·취소·자원 반환·raw latency sample 유지 |
| 10쌍 통계·throughput guardrail | 처리량 하락 평균 `-0.0104185%`, paired bootstrap95% 구간 `[-0.1682898%, +0.1490708%]`. 10% 하락 한도 PASS. 이 workload의 관측이며 개선으로 주장하지 않음 |
| payload quota | 관측 최대131,156bytes,64MiB 상한 PASS. 실제 close cleanup의 payload/connection baseline 복귀 확인 |
| p99 | BULK workload이므로 p99 비교 미적용. `null`을 p99 측정 성공으로 표시하지 않음 |
| 전체 판정 | **BATCH_PASS / C1 INCOMPLETE**. 원135조합/2,700mode 분모 유지. 다른 source의 원74와 합쳐 현재94개 완료로 표시하지 않음 |
| 시간 | 계획 시각 `2026-10-23T03:05:38.846387+00:00`(KST12:05:38). 최종 receipt를 확인한 clock 시각 `03:17:31 UTC`(KST12:17:31). 성능 실행을23시 전에 완료했고 검토·기록도 같은 낮 시간에 진행 |

실제 원본은 `artifacts/comm/performance-batch-bpz4yqzw/`의 plan/manifest/progress/validation·보존 source·release worker·build log·20개 개별 `validation.json`·latency sample gzip이다. 별도 새 report `artifacts/comm/batch-20261023-9w_e63a4/native-result.json`도 원 aggregate와 동일하다. workload·계측 코드·완료한 기존 시험을 수정하거나 반복하지 않았다.

| identity | SHA-256 |
| --- | --- |
| 실행 source | `b28bc3963ae88df1c949b109d5d6c46c0d670c15bc6d9564b725846dc5aad892` |
| release worker | `50b47c564913120769a045de3aa5722c367e245a79756491dd6575de504f6530` |
| 원 aggregate report | `3406ed3f84bb4d9302bc50da40cbc7db78437631e634c00a4e05e23c36f61ca1` |
| 새 batch controller | `2333b26e03aeef9a489baf5e133d7d64bab1739b64785a98bfc0a7d17fa0b27c` |
| 새 checker | `9de9828c7b10acbc980d9fa1e7d683688e245054cab7c862887773536a132040` |

최종 독립 검토자는 source·generator·binary·build·20개 mode/latency 원자료와 peer/worker 회계를 대조했고, 처리량 drop 평균과10,000회 bootstrap95% 구간을 원 bucket에서 독립 재계산해 일치를 확인했다. 선택20mode BATCH_PASS와 최종 문서·계획·구현·색인을 수용하며 차단 지적은 없다. `/root`가 계약·통합 담당으로 반영했다. 검토자는 파일 편집·제품 시험 재실행을 하지 않았다. 문서 자동 검사는 push 시점에 남기며 제품·C1 완료 상태를 바꾸지 않는다.

<a id="standalone-continuation"></a>
## 별도 미완료 시험 작업의 적용 범위 정정·후속 실행

사용자가 계속 진행하도록 요청한 뒤, 작업 담당은 코드 diff가 없다는 이유로 cell38/39를 보류하고 무변경 시험 예외를 질문했다. 당시 별도 검토도 같은 제한 적용을 전제로 보류했다. 사용자는 **"그런데 당신은 테스트만 하잖아요 개발중이 아닌데"**라고 현재 작업의 범위를 바로잡았다. 개발 중 불필요한 하위 회귀 반복 제한을 별도 미완료 COMM 인수의 미실행 조합까지 확대했던 해석을 정정한다. 현재 미완료 인수는 코드 수정 없이 진행하며 완료 사례 반복·품질 기준 축소는 허용하지 않는다.

원 준비/보류는 `artifacts/comm/continue-20261023-c86sfi4a/{selection,scope-review}.json`에, 이후 사용자 원문·적용 범위 정정은 `scope-clarification.json`에 각각 보존했다. 이전 보류를 실행 실패/PASS로 바꾸지 않으며 사용자 정정 이후에만 실행했다. 검토자 `/root/comm_acceptance_review`도 새 사용자 범위에서 진행 가능함을 확인했고, 작업·계약·통합 담당은 `/root`다. 현재 phase는 **제품 코드·시험 실행기 변경 없는 시험 작업**이다.

후속 선택은 원0-based cell38/39, 즉 TCP IPv6·1연결·1MiB payload의 정상 ECHO/BULK 두 조합이다. 완료37·다른 source의 원74와 겹치지 않는다. 원10쌍40mode·5초 warmup+30초 측정·guardrail·전체2,700 분모를 유지한다. nominal1,400초(23분20초), 보수5,920초, wall budget7,200초·deadline23:00 KST다. 직전 보존 source의 runner를 실행해 source `b28bc3963ae88df1c949b109d5d6c46c0d670c15bc6d9564b725846dc5aad892`와 worker `50b47c564913120769a045de3aa5722c367e245a79756491dd6575de504f6530`를 유지했다. DEV3의 동시 제품 수정은 사본과 분리하고 보존했다.

같은 해석 혼동을 막기 위해 기존 [AGENTS.md](../../AGENTS.md)의 별도 COMM 절에 사용자 정정을 좁게 반영했다. 개발 중 회귀 제한·완료 시험 반복 금지와 다른 DEV의 시험 범위는 유지한다. root 지침·계획·이 증거의 정정은 별도 검토자가 D2 범위에서 수용했다. 실제 제품 코드·실행기·oracle 수정은 없고 문서·지침만 구체화했다.

| 후속 실제 실행 | 결과·범위 |
| --- | --- |
| cell38·ECHO | 원10쌍20mode PASS. throughput 하락 평균 약-0.0332%,95% 구간[-0.2894%,+0.1586%]로10% 한도 PASS. p99 상승 평균 약-0.0784%,95% 구간[-1.5131%,+0.8884%]로15% 한도 PASS |
| cell39·BULK | 원10쌍20mode PASS. throughput 하락 평균 약-0.0861%,95% 구간[-0.2334%,+0.0735%]로10% 한도 PASS. p99 미적용 |
| quota·정답·정리 | ECHO peak2,097,236bytes/BULK1,048,618bytes로64MiB 상한 PASS. 모든 mode의 peer/worker frame·byte·payload, 취소·exit0·cleanup0 검증 PASS |
| 배치·source별 수합 | attempted40/completed40·**BATCH_PASS / C1 INCOMPLETE**. 같은 source/worker의 기존37과 중복 없이37–39 세 조합60mode. 다른 source 원74는 합산하지 않음 |
| 시간 | 계획 `2026-10-23T04:57:51.906742+00:00`(KST13:57:51). 실제 최종 receipt 완료 확인 clock `05:21:23 UTC`(KST14:21:23). deadline23시 전 |

후속 원본 root는 `artifacts/comm/performance-batch-bpz4yqzw/source/artifacts/comm/performance-batch-f9gbbced/`다. plan/manifest/progress/validation·보존 source·release worker·build log와40개 mode/latency raw를 유지했다. 외부 사본 `artifacts/comm/continue-20261023-c86sfi4a/native-result.json`은 원 aggregate와 동일하고 SHA-256은 `3b15c47e0c0ba36e5cd5d8438da2f8c770f796f1b8d11c911a695e7a11b8d902`다. 같은 디렉터리의 `completion-audit.json`은 같은 source/worker60개의 고유 `(cell,pair,mode)`와 완료 확인 시각·quota·비중복을 기록한다. 음수 하락률·상승률을 성능 개선 주장으로 일반화하지 않는다.

후속40mode·원자료·통계·최종 기록은 별도 검토자가 대조하여 수용했고 차단 지적은 없다. `/root`가 계약·통합 담당으로 반영했다. 검토자는 제품 시험·문서 자동 검사·파일 편집을 하지 않았다. 전체 C1·남은 model/native fault/성능 조합은 미완료다. 이미 완료한 시험·실행기 unit 시험·하위 wrapper·DEV3 시험은 반복하지 않았다. 문서 검사는 push gate에 남긴다.

<a id="model-continuation"></a>
## 미완료 모델 시험 후속 실행·실패 보존

사용자가23시 전 남은 시험을 계속 진행하도록 확인해 같은 보존 source의 `check_comm_model.py`로 `two_connections_two_slots_depth_eight_exhaustive_model --exact --ignored --nocapture` 한 시험만 실행했다. 계획·선택은 `artifacts/comm/deadline-continuation-20261023-cvn24yo0/selection.json`에 보존했다. 기존 깊이8·360,000,000 states·5,000,000,000 transitions·6시간 예산과 runner21,660초를 유지했고 성능 시험과 동시 실행하지 않았다. 완료한 smoke·key 회귀·기존 장시간 시험은 반복하지 않았다.

2026-10-23 14:29:05 KST에 시작한 실행은 **exit101·FAIL**로 즉시 종료했다. depth1–3 뒤 `[Enqueue(0), Enqueue(0), Cancel, Enqueue(0)]`에서 실제 sender 오류 `Cancelled`와 모델 기대값 `Backpressured`가 달라 counterexample을 남겼다. libtest 경과는0.02초이며 깊이8 완료·PASS가 아니다. 제품·모델·checker 수정이나 재실행은 아직 없고 원 실패를 보존한다. 취소 뒤 terminal 오류 우선순위와 모델 기대값의 차이는 후속 분석 대상으로 남긴다.

원본 root는 `artifacts/comm/performance-batch-bpz4yqzw/source/artifacts/comm/model-l9w9_i1i/`이며 manifest·보존 source·build 출력·test-binary·execution.log·validation.json을 유지한다. 실제 source SHA-256은 `b28bc3963ae88df1c949b109d5d6c46c0d670c15bc6d9564b725846dc5aad892`, binary는 `b37688ea6820b150e47260479bfb6bc679b4f877ef3daa762d111575e5660c93`, 실행 log는 `c94ce5fe8510dab265ff2ff5984869ffe63ab4f612b07e76b4ccace833ef05f7`다. 외부 사본은 위 선택 디렉터리의 `model-result.json`이다. frozen runner의 `origin.revision/tracked_diff`는 바깥 live 저장소 메타데이터이므로 실제 실행 source의 검증 주장은 snapshot fingerprint·generator·binary hash에만 귀속한다. 전체 C1은 INCOMPLETE다.

별도 검토자 `/root/comm_acceptance_review`는 source·generator·contract·binary·build·log와 외부 사본 및 실패 반례를 대조했다. 모델의 full-ticket 검사가 취소/terminal 검사보다 앞서는 반면 현행 sender와 수용된 오류 계약은 취소/terminal을 우선하므로 모델 oracle의 우선순위 불일치라는 근거가 충분하다고 판단했다. 이 분석으로 원 FAIL을 바꾸지 않으며 모델 수정·관련 반례 검증은 별도 D2 작업이다. 검토자는 시험 재실행·편집을 하지 않았다.

<a id="model-oracle-fix"></a>
## 모델 oracle 수정과 관련 반례 선택 검증

사용자가 다시 미완료 시험의 진행을 요청해 `crates/transport/tests/exploration/mod.rs`의 독립 oracle만 수정했다. 제품 sender·AXF1·연결/slot/payload·event inventory·BFS/key·깊이8·360M states/5B transitions/6시간 예산은 유지한다. 작업·계약·통합 담당은 `/root`, 별도 검토자는 `/root/comm_acceptance_review`다.

| D2 결정 | 내용 |
| --- | --- |
| 문제·선택 | 원4-event 반례에서 모델이 full-ticket을 먼저 검사했다. 현행 오류 계약에 맞게 cancel→기존 terminal 원인→non-Open Closed→Open full Backpressured 순서로 정정 |
| witness | `full_queue_rejected`는 취소되지 않은 Open의 full queue에서만 인정. 취소·종료를 capacity rejection 증거로 세지 않음 |
| 대안·근거 | 제품을 오래된 모델에 맞춰 되돌리는 대신 수용된 terminal 오류 계약을 독립 상태 값으로 표현. production transition을 oracle에 import하지 않고 literal 오류 기대값과 실제 SUT를 각각 검산 |
| 새 관련 회귀 | 두 연결×세 payload에서 Open full·Draining·취소·완결 ticket 보유 Closed·Send/Frame/Drain/Idle timeout과 이후 cancel을 확인. 원 `[Enqueue(0), Enqueue(0), Cancel, Enqueue(0)]`도 그대로 포함. 거절 무효과·witness 유무와 SUT replay의 quota/ticket/기존 terminal 원인 보존 확인 |
| 실행 범위 | 새3 method만 실행한 첫 job은2 PASS·Idle fixture1 FAIL. 다른 연결의 더 이른 idle deadline도 먼저 진행하도록 fixture를 고친 그1 method만 PASS. 검토에서 원 반례 literal 추가를 요구해 변경한 cancel method1개만 다시 PASS. 완료한 나머지 method·기존 smoke/key·하위 회귀는 반복하지 않음 |
| 원 증거 | `artifacts/comm/model-oracle-fix-20261023-3lwgi943/`의 selection·regression-result/log·build·source, 후속 `fixture-fix-eyes1rd4/`와 `counterexample-fix-6f6b_46q/`에 각 FAIL/PASS/source/binary를 구별 |
| 정적 확인 | 첫 fixture 수정 source의 scoped model Clippy와 최종 파일 rustfmt 확인 PASS. 그 뒤 cancel method에 원 literal 반례만 추가했으며 최종 build·변경 method PASS를 별도 보존 |
| 안전 감시 | artifact의 `guarded_model.py`가 직접 Popen한 test binary를100ms마다 관측하고1초마다 RSS/가용 메모리를 기록. 가용8GiB 이하·runner21,660초·22:15 KST hard stop이면 kill/reap하고 INCOMPLETE로 보존. 사용자23시 전 검토·기록 여유를 확보 |
| 감시 신규 검증 | 정상 종료·실제 timeout kill/reap·강제 memory 조건 kill/reap의3개 신규 control 사례 PASS. 독립 검토에서 Popen 직후 running 기록 실패의 정리 공백을 지적해 lifetime contextmanager로 보완하고 실제 child에 해당 실패를 주입한1개만 추가 PASS. 원 guard3 source/hash와 결과는 별도 보존하고 반복하지 않음. 제품 모델의 공간 완주 증거와 구분 |
| 상태·재검토 | D2 최종 검토 수용, 잔여 차단 지적 없음. 원 FAIL은 유지하고 깊이8 실행 결과는 별도 판정. 오류 우선순위·witness·PID/정리·원자료 누락·예산 변경 시 재검토 |

최종 관련 회귀 binary의 source는 `e77352d99360bda79253b951d335b221d2dbc3a65e005e1269ea4a7ffdc9305b`, binary는 `11c9e21101c27340c9657efc26c82073e53c9e7e73edf08eb258c7b6f38ec02e`, oracle 파일은 `61fdeaf26692242d80af7afaaae4597aabad37dabeb7ca0b5117661ff7326075`다. 이 source/binary를 깊이8 한 시험에 사용하며 기존 성능 source와 혼합하지 않는다. 모델과 성능 측정은 동시 실행하지 않는다.

별도 검토자는 최종 oracle/source/binary와3→변경1→변경1의 실제 회귀 결과, guard의 원3·변경1 사례/source를 읽기 전용으로 대조하여 수용했다. `/root`가 계약·통합 담당으로 반영했다. 14:42:53 KST에 같은 최종 binary의 깊이8 한 시험을 시작했으며 원본은 같은 artifact 아래 `exhaustive-sat1r5ra/`다. 당시 RUNNING이었으나18:45 재개 시점에는 최종 validation과 PID142876/감시 프로세스가 없었다. 최종 완결 layer7은36,480,163 states/106,165,909 transitions이고 마지막 자원 관측은 경과1,273.53초·RSS7,970,772KiB·available45,706,932KiB다. 원 로그를 보존하고 `interruption-audit.json`에 **INCOMPLETE_INTERRUPTED**로 기록했다. 종료 원인은 미확정이며 OOM·8GiB guard 발동·담당자의 예방 중단 어느 것으로도 단정하지 않는다.

후속으로 `check_comm.py`의 test-support 필수 inventory에 새 Rust method3개를 등록했다. `test_check_comm.py`의 신규 method1개만 선택 실행해 등록 전 FAIL과 등록 후 PASS를 `inventory-{red,green}.{json,log}`·각 원 source에 보존했다. 각 이름 누락/ignored 거절·완전 집합 수락과 default inventory 제외를 검사하며 제품·모델 시험 실행으로 세지 않는다. 별도 검토자가 실제 method 이름·source/log hash·검사 경계를 대조해 D2 수용했다. 이 inventory 후속 편집은 이미 실행 중인 모델의 동결 source에 포함되지 않으므로 전수 결과를 새 inventory source의 실행으로 소급하지 않는다.

<a id="model-cache-fix"></a>
## 모델 상태 저장 구조의 선택 수정·시간 부족 skip

마지막 level에서 큰 HashSet의 재할당 중 구/신 table이 겹칠 자원 위험을 줄이기 위해 `model.rs`의 `seen`만 표준 `BTreeSet<[u8;80]>`로 바꿨다. `seen`은 insert의 중복 여부와 len만 사용하며 순회하지 않는다. 따라서 정확한 key의 동등성·부모/event 순서·첫 대표 trace·BFS·키 버전·깊이8·상태/전이/6시간 예산은 유지한다. 대안은 원 대형 table과 안전 중단에만 의존하는 것이었다. 노드의 빈 공간/포인터와 비교 비용 때문에 새 구조의 실제 대규모 메모리 사용량이나6시간 내 완주는 아직 보장하지 않는다. 제품 코드는 수정하지 않았다.

새 `ordered_layer_dedup_preserves_exact_hash_reference` 한 method가 깊이1–4의 모든 생성 key에서 HashSet/BTreeSet의 insert 결과와 개수, 첫 대표 trace/상태의 순서 및 고정 layer 수20/266/3,162/35,310을 비교해 PASS했다. 기존 smoke/key/terminal 회귀를 반복하지 않았다. 이 이름을 test-support 필수 inventory에 추가하고 별도 신규 Python method1개로 누락/ignored 거절을 RED1 FAIL→GREEN1 PASS로 확인했다. 다른 checker method는 반복하지 않았다. 관련 model Clippy와 rustfmt 확인도 PASS다.

원 증거는 같은 artifact의 `cache-fix-fm0u0zek/`에 있다. source는 `45fbfcb6c19069f76121c09a4b4d11a7a9a3ac95f88205c4c8aed18f14d8db54`, binary는 `0321d85586832ce5ee6cd1033c9873d409ecef446c95f3e3835887d4c9e0eb44`, model 파일은 `119f7cf82719632f51aaa6a232f1fb275715f3e9e7d04a878951fe6c18f1f1e8`다. 별도 검토자 `/root/comm_acceptance_review`가 diff·원 source/binary/build/log·신규 선택 결과를 읽기 전용으로 대조해 **D2 수용, 차단 지적 없음**으로 판정했고 `/root`가 반영했다. 검토자는 시험·편집을 하지 않았다.

새 source의 깊이8 재개 계획은18:45 KST의 실제 남은 시간에서6시간+정리/검토가23시 전에 들어가지 않으므로 **SKIPPED_TIME_BUDGET**이다. 필수 범위·시간 예산을 줄여 PASS로 바꾸지 않는다. 앞선 HashSet 실행에 대한 예방 중단 계획은 실제 수행하지 않았으며 그 실행의 미확정 종료 원인을 이 계획으로 대체하지 않는다. 전체 모델과 C1은 INCOMPLETE다.

<a id="evening-continuation"></a>
## 저녁 시간의 미실행 성능 조합

18:45 재개 시각과 모델 프로세스 부재를 확인한 뒤 기검토 후보의 첫4셀48/49/58/59를 선택했다. TCP IPv6·1MiB payload의16/128연결 ECHO/BULK이며 완료37–39·다른 source의 원74와 중복되지 않는다. 보존 source `b28bc3963ae88df1c949b109d5d6c46c0d670c15bc6d9564b725846dc5aad892`의 원 batch runner로 원10쌍80mode·warmup5초/측정30초·guardrail·2,700 분모를 유지한다. 모델과 성능은 동시 실행하지 않았다.

선택·원 모델 중단 관측은 `artifacts/comm/evening-20261023-de7y58__/selection.json`과 위 `interruption-audit.json`에 보존한다. 성능 단독 nominal2,800초·보수11,120초, batch budget21,600초(<8시간), 실행 deadline22:30 KST로23시 전 검토/기록30분을 남긴다. 실제 원본 root는 `artifacts/comm/performance-batch-bpz4yqzw/source/artifacts/comm/performance-batch-9utevpx5/`이며 외부 report는 `artifacts/comm/evening-20261023-de7y58__/batch-01-result.json`이다. 별도 검토자가 선택·비중복·시간·원 분모를 수용했다. 19:37:46 KST clock에서 최종 **BATCH_PASS·attempted80/completed80**를 확인했으며 전체 C1은 INCOMPLETE다.


| cell·연결/활성 peer | workload | completed throughput 하락 평균·95% 구간 | p99 상승 평균·95% 구간 |
| --- | --- | --- | --- |
| 48·16/11 | ECHO | +0.3954% [-0.3529%, +1.0927%] | -0.1828% [-1.3520%, +0.9988%] |
| 49·16/16 | BULK | +0.0724% [-0.1624%, +0.2818%] | 미적용 |
| 58·128/11 | ECHO | +0.2306% [-0.9668%, +1.4491%] | +0.3866% [-1.1313%, +1.9218%] |
| 59·128/55 | BULK | -0.0736% [-0.3050%, +0.1329%] | 미적용 |

원 실행기의 in-flight 제한 아래 ECHO 활성 peer11, BULK 활성16/55를 유지한 관측이다. 처리량10%·ECHO p99 15% 한도와64MiB payload quota를 통과했다. 동시 global payload 관측 최대는12,583,416bytes이며 개별 queue peak 합과 구별한다. 관측 cancel 최대 약2.121ms·exit0·cleanup의connection_count/payload_bytes=0을 확인했다. 음수 하락률을 일반 성능 개선으로 해석하지 않는다.

원 aggregate와 외부 사본의 SHA-256은 `8438bc10582a441fba8f7476207718c91a6f2b402bbb6bf4ca5a1797bef3d921`다. 별도 검토자가 앞선40개의 review manifest 불변을 확인하고 새 pair06–10의40개 원자료와 전체10쌍 평균·10,000회 bootstrap95% 구간을 독립 재계산해 일치를 확인했다. source/binary/byte/frame·latency sample·취소/정리·분모를 수용하며 차단 지적은 없다. 실제 review 목록과140개 고유(cell,pair,mode)의 수합은 같은 외부 디렉터리의 `batch-01-{partial-review,completion-audit}.json`에 보존한다. 같은 source/worker의7조합140mode이며 다른 source의 원74는 합산하지 않는다.

후속은 원 미실행68/69/78(Unix·1MiB·1연결ECHO/BULK·16연결ECHO)3조합60mode다. UTC10:38:04(KST19:38:04)에 계획했고 nominal2,100초·보수8,520초가22:30 실행 마감에 들어가는 것을 확인해 원10쌍·측정·통계·source/worker를 유지한다. 원본은 `artifacts/comm/performance-batch-bpz4yqzw/source/artifacts/comm/performance-batch-ws2l1z6h/`, 선택/외부 report는 위 외부 디렉터리의 `batch-02-{selection,result}.json`이다. 모델·이전 성능 프로세스가 종료한 뒤 실행했고20:13:46 KST clock에서 최종 BATCH_PASS·attempted60/completed60을 확인했다.


| cell·연결/활성 peer | workload | completed throughput 하락 평균·95% 구간 | p99 상승 평균·95% 구간 |
| --- | --- | --- | --- |
| 68·1/1 | ECHO | -0.0007% [-0.2339%, +0.2326%] | -0.1050% [-0.3865%, +0.1829%] |
| 69·1/1 | BULK | -0.0001% [-0.1010%, +0.1007%] | 미적용 |
| 78·16/11 | ECHO | -0.0753% [-0.3249%, +0.1240%] | -1.5533% [-3.3226%, -0.1134%] |

원 throughput10%·ECHO p99 15%·64MiB quota 한도를 모두 통과했다. 동시 global payload 최대12,583,416bytes, 관측 cancel 최대약3.133ms이며60개 모두worker exit0·cleanup connection_count/payload_bytes=0이다. 앞30개 검토 때의 cancel 최대2.128ms와 전체60개의 최대를 구별한다. 연결16/활성11의 원 in-flight 제한을 유지한다.

원/외부 aggregate SHA-256 `718149a88648bb4c8beaee42efa2be1c79855b9cef2bfe8a2045e2a978560cba`를 확인했다. 별도 검토자는 앞30개의 manifest `e14943e27ad30096f96825a0618ed5a01ad5d56888af65c175a92f34af77b6ad` 불변과 추가30개의 manifest `232d4414c5c3f9daed05a0d39ffb03117fc6ff83d615153e84580a8eb2a9271e`·원자료를 대조했다. 전체10쌍의3개 처리량 평균/95% 구간과2개ECHO p99 평균/95% 구간을10,000 bootstrap으로 독립 재계산해 일치했으며 차단 지적은 없다. 목록·수합은 같은 외부 디렉터리 `batch-02-{partial-review,completion-audit}.json`에 보존한다. 같은 source/worker의10조합200개 고유 mode이며 다른 source의 원74는 합산하지 않는다.

다음 미실행79/88은Unix·1MiB·16연결BULK/128연결ECHO의2조합40mode다.20:14:08 KST 계획에서 보수5,920초가22:30까지가용8,151.9초 안에 들어가며 원10쌍·warmup5초/측정30초·guardrail을 유지한다. 이전 배치 종료 뒤 시작했고 현재 RUNNING이다. 원본은 `artifacts/comm/performance-batch-bpz4yqzw/source/artifacts/comm/performance-batch-zabbon_7/`, 선택/외부 report는 `artifacts/comm/evening-20261023-de7y58__/batch-03-{selection,result}.json`이다. 별도 검토자가 완료10셀·역사 결과와 비중복·시간·분모·동일source/binary 계획을 수용했다.
