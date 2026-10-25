# DEV2 PF04 warmup 뒤 관측 경계 대조

2026-10-25 · **디버깅10쌍 완료 / UNKNOWN_NO_FROZEN_LARGE_BOUNDARY_SHIFT / 원 PF04 FAIL·DEV2 미완료**

[기계 판독 기록](dev2-pf04-boundary-comparison-20261025.json) · [동결 계획](../plans/dev2-pf04-resolution-plan.md#boundary-comparison) · [도구 구현·선택 검증](dev2-pf04-prewarmup-20261025.md)

## 결과

동일 ELF·Off 입력에서 warmup 뒤 바로 측정하는 C와 START/GO 뒤 측정하는 W를10쌍 대조했다. **원 loop CPU의 W/C paired 중앙은1.002127138**, 정렬 비율의0-based[1,8] 구간은 **0.991354900–1.015730013**이다. CW/WC 순서별 중앙비1.010741214는 동결 순서 불안정 기준에 들지 않았다. 구간 전체가1.10보다 크거나10/11보다 작아야 하는 주 효과 기준을 충족하지 않아 **UNKNOWN_NO_FROZEN_LARGE_BOUNDARY_SHIFT**다.

이는 이번 조건에서 사전 정의한 큰 경계 효과를 관측하지 못했다는 뜻이다. 작은 차이의 부재나 과거 원인 배제를 증명하지 않는다. 원 PF04 네 campaign FAIL과 DEV2/B2/D0 미완료를 유지하며 성능 인수 PASS로 승격하지 않는다. 구간의1002/1024 coverage는 독립·동일분포를 가정한 명목값이며 이번 순차 실행의 독립성을 보증하지 않는다.

## 실행 근거와 범위

계획 `8444a45`, 준비·검토 `d6dc26c` 뒤 사용자가 현재 작업은 개발 회귀가 아닌 디버깅임을 정정했다. [AGENTS.md](../../AGENTS.md)와 계획에 그 구분을 `590991d`로 반영했다. 기존 원인 분석·해결 지시 안의 미실행 진단이므로 별도 예외 허가를 요구하지 않았다. 이전의 추가 허가 필요 판단과 receipt는 역사 자료로 남기고 적용 범위만 정정했다.

원 `run.py`는 보존했다. 실행한 `run-debug.py`는 예외 승인 검사 대신 디버깅 적용 범위 확인을 사용하고 준비 manifest 이름만 바꿨다. 두 치환을 되돌리면 원 driver와 byte가 같다는 `debug-scope-diff.json` 및 독립 검토를 보존했다. 수집·입력·순서·한도·정리·판정 계약 변경은 없다.

- C=`prewarmup-continuous-v1`, W=`prewarmup-wait-v1`, Off, seed20261018,128행, old snapshot 유지, capacity256, warmup4096, measure65536, stride16이다.
- 순서는 CW/WC/WC/CW/CW/CW/WC/WC/WC/CW다. 같은 release ELF SHA-256 `14d25271a4927a2eac0ce7293afd67bd18c561bf2e34c75eda74f8c8e4909eb2`, build source `759d9952d8a45bffc342eb852ee6f29cc84e3991f7a633becb895cbc09071604`를 사용했다. 병행 DEV3 수정은 포함하지 않았다.
- 전체20slot을 **208.675208초**에 한 번 완료했다. 한도는900초, slot60초, 재시도0이며 실패·취소·미실행 slot은 없다. 각 slot 앞10초 유휴 확인도 한 번뿐이다.
- worker CPU2, runner/실제 perf CPU3와 sibling CPU18·disk 관측을 보존했다. 시작 전 CPU2/18·disk busy는 모두5% 이하였다. 측정 중 host 관측까지 유휴라고 주장하지 않으며 일부 slot의 disk 활동도 원자료에 남겼다.
- source·ELF·입력·driver·준비 지문을 시작 전과 각 slot 뒤 대조했다.8event×20=160PMU행은 raw unscaled count·runtime·reported100%를 보존한다. migration0, 원 STOP의 측정 rusage major fault0을 확인했다. warmup까지 major fault가 없었다는 주장은 아니다.
- 새 출력20개에 정확한 Rust adapter를 각각 한 번 적용했다. 독립 읽기 검산은1,310,720 read scalar와 old/final·counter,81,920 latency, 보조 pair 수치390개를 확인했다. 소유 group60개와 실제 collector20개의 종료 기록도 확인했다.
- 새 build0, 완료 제품·도구 시험 재실행0, 재부팅·서비스 재시작·host 설정 변경0이다. 기존 CAP_PERFMON 수집 경로를 사용했다.

## 같은 pair의 측정

CPU 비율을 주 지표로 고정했고 p50/p99·PMU·rusage는 보조 관측이다. p50은 표본 median, p99는 nearest-rank이며 원 raw와 정수분수 비율은 `analysis.json`에 보존한다. 분모0 비율은 null과 이유를 남겼다.

| pair | 순서 | C CPU ns | W CPU ns | CPU W/C | p50 W/C | p99 W/C |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 0 | CW | 14,460,880 | 14,563,072 | 1.007066790 | 1.000000000 | 1.004000000 |
| 1 | WC | 14,515,092 | 14,448,908 | 0.995440332 | 1.000000000 | 1.004000000 |
| 2 | WC | 14,571,968 | 15,763,665 | 1.081780100 | 0.962500000 | 1.000000000 |
| 3 | CW | 14,729,004 | 14,449,229 | 0.981005165 | 1.000000000 | 1.037344398 |
| 4 | CW | 14,387,933 | 14,476,168 | 1.006132570 | 1.038961039 | 1.004000000 |
| 5 | CW | 14,345,443 | 14,571,097 | 1.015730013 | 1.038961039 | 1.004000000 |
| 6 | WC | 14,451,493 | 14,373,486 | 0.994602149 | 1.000000000 | 1.000000000 |
| 7 | WC | 14,423,801 | 14,602,756 | 1.012406924 | 1.038961039 | 1.041493776 |
| 8 | WC | 14,559,345 | 14,433,478 | 0.991354900 | 0.962500000 | 1.041493776 |
| 9 | CW | 14,508,379 | 14,481,128 | 0.998121706 | 1.000000000 | 1.004000000 |

보조 paired 중앙은 elapsed1.002124052, p50 1.000000000, p99 1.004000000, user instructions1.000125736, user cycles1.001486143, user CPI1.001361521, user cycles/reference cycles0.999644714다. kernel instructions1.056735679, kernel cycles1.173305766이며 작은 절대 kernel count의 변화와 전체 loop CPU를 구분한다. pair2의 CPU1.081780100도 제외하지 않았으며 큰 경계 효과 기준을 충족시키지 않는다.

C의 `armed→loop start`에서 worker warmup 시간을 뺀 잔여는511–1,523ns, W는1,060,000–1,063,456ns다. 이는 추가 동기화가 실제로 진입 전 시간을 늘렸음을 보여 주는 기술 관측이다. drain·경계 작업을 포함하므로 순수 IPC service time은 아니다. PMU 자체는 두 모드 모두 **warmup+drain+measurement+control bracket**을 포함한다. 이 잔여나 warmup 시간을 PMU에서 빼서 loop-only count를 만들지 않는다. cycles/reference cycles를 GHz로 해석하거나 rusage user/system으로 정확한 domain CPU를 분해하지 않는다.

## 보존·검토와 다음 조사

외부 원자료는 `/home/et16/xadb-dev2-evidence/pf04-boundary-comparison-20261025`에 있다. `execution/slot-00`부터 `slot-19`, 원 summary·analysis·준비 history·선택 분석기 시험 결과를 보존한다. manifest는 **315파일·27,936,904byte**, SHA-256 `033d0797372e4f91901ecd5314be7070b763a2658d5ed4c0513c33c2139a6528`다. 최종 outcome review는 자기 참조를 피하기 위해 manifest에서 제외하고 연결 JSON에서 별도로 결합한다.

독립 원자료 검토 **PASS_SCOPED_RETAINED_RAW_RECALCULATION**은 새 native·oracle·시험 실행 없이 완료했다. 최종 독립 검토 **PASS_SCOPED_D1_D2_BOUNDARY_DIAGNOSTIC_EVIDENCE_AND_DOCUMENTS**를 채택했으며 차단0개다. receipt 경로·지문은 연결 JSON에 기록한다. 기존 분석기 r1의4함수와 r2의 새3함수 선택 PASS는 각각 원 source의 준비 증거로 유지하고 다시 실행하지 않았다.

이 경계 대조를 자동 반복하지 않는다. 다음에 필요한 구분은 같은 경계에서 모드에 따라 늘어나는 실행 작업과 실행 간 CPU 변동이다. 이번 Off-only 결과로 Always/Detailed 비용이나 영속 tail/CPU 문제를 해결했다고 판단하지 않는다. 다음 진단은 해당 입력·정답 검산·관측 질문·예산을 먼저 동결하고 기존 디버깅 지시에 따라 진행한다.
