# COMM 성능 시험의 시간 제한·조합별 분할

2026-10-23 · **D2 분할 실행기·적용 범위 정정 수용 / 선택3조합60mode BATCH_PASS / C1 INCOMPLETE**

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
