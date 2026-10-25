# DEV2 PF04 같은 경계의 모드별 실행 작업과 CPU 비용

2026-10-25 · **디버깅40slot/20쌍 완료 / 원 PF04 FAIL·DEV2 미완료 유지**

[기계 판독 기록](dev2-pf04-mode-attribution-20261025.json) · [동결 계획](../plans/dev2-pf04-resolution-plan.md#mode-attribution) · [앞선 경계 대조](dev2-pf04-boundary-comparison-20261025.md)

## 결과와 해석

동일한 보존 release ELF의 continuous 관측 경계에서 Always/Off10쌍·Detailed/Off10쌍을 한 번 실행했다. Off 표본은 각 비교에서 별도로 실행했다.40개 모두 정답·수집 품질·소유 종료 조건을 만족했고 전체 417.476283초, 재시도0이다.

| On 모드 | CPU On/Off paired 중앙 | 정렬[1,8] 구간 | CW/WC 중앙비 | 최악 pair | pair별23/20 초과 |
| --- | ---: | ---: | ---: | ---: | ---: |
| always | 1.092334187 | 1.085905265–1.100748448 | 1.004348757 | 1.104277470 | 0/10 |
| detailed | 1.122806355 | 1.110987781–1.131064472 | 0.995275631 | 1.133934999 | 0/10 |

원 CPU 한도23/20을 참조한 **중앙 구간의 기술 분류**는 Always `MEDIAN_INTERVAL_WITHIN_REFERENCE`, Detailed `MEDIAN_INTERVAL_WITHIN_REFERENCE`다. 원 인수는 개별 pair 기준이므로 중앙 구간 분류가 원 판정을 대체하지 않는다. 각 pair의 초과 여부·최악값·불리한 자료를 모두 보존한다. 구간의1002/1024는 IID 가정의 명목 coverage이며 순차 실행의 독립성을 보증하지 않는다. **원 PF04 네 campaign FAIL과 DEV2/B2/D0 미완료, 성능 인수 NOT_APPLICABLE을 유지한다.**

user instructions의 paired 중앙은 Always 약+5.32%, Detailed 약+7.63%이고 user CPI는 각각 약+3.65%, +4.67%다. 추가 명령 수와 명령당 비용 변화가 함께 관측됐으며, user cycles/reference cycles 비율은 각각0.999316·1.000361로 이번 모드 간 비교에서 거의 같았다. 이 수치를 더해 loop CPU를 분해하거나 특정 원자 연산의 시간 비중으로 환산하지 않는다.

[앞선 source 분석](dev2-pf04-analysis-20261025.md)에서 확인한 정상 loop의 counter 갱신 호출393,216회, Detailed 이벤트196,608건 중256건 보존·196,352건 drop이라는 추가 작업과 양립한다. 이번 실제 출력에서도 성공한 논리 started/completed counter 증가 합393,216과 Detailed 보존/drop 작업량을 독립 검산했다.393,216은 실제 CAS 명령의 시도·재시도 횟수 실측이 아니다. 다만 과거 실패의 약25–30% CPU 증가를 설명하는 느린 실행 대역은 이번에 나타나지 않았다. 알려진 비용과 과거 초과를 구분하며, 기각한 C1/C2를 되살리거나 효과를 합산할 근거로 사용하지 않는다.

## 실제 입력·정답·수집

계획 `091a38b`와 준비 `c21b96e`를 먼저 commit했다. 사용자가 현재 작업은 개발 회귀가 아닌 디버깅임을 정정한 `590991d`와 기존 원인 분석·해결 지시에 따라 진행했으며, 새 예외 허가를 요청하거나 받았다고 기록하지 않는다.

- 전40slot은 `prewarmup-continuous-v1`, seed20261018·128행·old snapshot·capacity256·warmup4096·measure65536·stride16이다. 입력은 mode만 다르며 Off는 원 request와 byte가 같다. 각 모드의 pair 순서는 CW5/WC5이고 두 종류 pair의 block 순서도 AD5/DA5다.
- ELF SHA-256 `14d25271a4927a2eac0ce7293afd67bd18c561bf2e34c75eda74f8c8e4909eb2`, build source `759d9952d8a45bffc342eb852ee6f29cc84e3991f7a633becb895cbc09071604`를 유지했다. 병행 DEV3 변경은 포함하지 않았고 worker·oracle 새 build0, 제품·기존 수집기 변경0이다.
- 기존 Rust 검산기의 `Side.mode`가 Off라는 사실만으로 새 adapter가 필요하다고 한 추정을 실제 호출 경로 확인으로 철회했다. `check_output`은 request의 input.mode로 세 모드의 snapshot·counter·event/drop/sequence를 검산한다. 새 driver가 각 slot의 의도한 mode·동결 input byte/hash를 독립 결합하고, 기존 continuous exact case를 fresh output마다 한 번 실행했다. 원 raw는 바꾸지 않으며 정확한 diagnostic path를 확인한 뒤 메모리 사본에서만 원 oracle path를 사용한다.
- worker CPU2, runner/실제 perf CPU3, sibling CPU18을 유지했다. 각 slot 앞 유휴10초 확인은 한 번이며 CPU2/18·disk busy≤5%였다. 측정 중 host 원자료도 보존하며 시작 전 유휴가 측정 중 무간섭을 보장한다고 주장하지 않는다.
- 8event×40=320행은 raw unscaled count·runtime·reported100%이며 migration0·측정 rusage major fault0을 확인했다. PMU는 warmup+drain+measurement+control bracket이다. loop CPU/latency와 범위가 다르며 정확한 domain CPU 분해나 GHz로 환산하지 않는다.
- 새 출력40개의 정답, 독립 검산의2,621,440 scalar·163,840 latency·활성20run의220 label counter delta·Detailed2,560개 이벤트/10개 drop delta와 소유 group120개·실제 collector40개 종료를 보존한다. source·ELF·입력·driver·분석기 지문을 전/매 slot 뒤 확인하고 최종 지문 뒤에도 취소·deadline을 검사했다.900초 전체·60초 slot 한도를 충족했고 취소·실패·중단 slot은 없다.

## 같은 pair의 보조 관측

다음은 같은 pair의 On/Off 비율을 각 모드10쌍에서 취한 중앙이다. PMU와 loop의 범위 차이는 위와 같다. p50은 median, p99는 nearest-rank이며 정수분수 원값·10개 비율·구간·분모0의 null과 이유는 `analysis.json`에 남겼다.

| 지표 | Always/Off 중앙 | Detailed/Off 중앙 |
| --- | ---: | ---: |
| elapsed_ns | 1.092328309 | 1.122791926 |
| latency_p50_ns | 1.086580087 | 1.127705628 |
| latency_p99_ns | 1.084000000 | 1.084000000 |
| instructions:u | 1.053185367 | 1.076304360 |
| instructions:k | 1.014768880 | 1.035872816 |
| cycles:u | 1.091675228 | 1.126576257 |
| cycles:k | 1.111663890 | 0.866223533 |
| ref-cycles:u | 1.092516481 | 1.128541542 |
| user_cpi | 1.036546289 | 1.046707780 |
| kernel_cpi | 1.112251075 | 0.844478046 |
| user_cycles_per_ref | 0.999315678 | 1.000361030 |

해석은 실행 작업 증가와 명령당 비용 변화를 함께 확인하는 범위다. PMU CPI만으로 cache·원자 연산 경합·주파수 원인을 특정하지 않으며, 서로 다른 지표의 중앙값을 더하거나 빼서 정확한 비용 기여도를 만들지 않는다. 현재 모드 차이가 과거 큰 실행 속도 대역이나 영속 tail/CPU 실패를 설명했다고 주장하지 않는다.

| 모드 | 모드 내 pair | 전체 pair | CPU On/Off | 원 CPU 참조 한도 | p99 On/Off |
| --- | ---: | ---: | ---: | --- | ---: |
| always | 0 | 1 | 1.104277470 | 이하 | 1.084000000 |
| always | 1 | 2 | 1.095391624 | 이하 | 1.084000000 |
| always | 2 | 4 | 1.094704182 | 이하 | 1.084000000 |
| always | 3 | 7 | 1.089964193 | 이하 | 1.084000000 |
| always | 4 | 9 | 1.086221124 | 이하 | 1.080000000 |
| always | 5 | 10 | 1.083519120 | 이하 | 1.160000000 |
| always | 6 | 12 | 1.088199665 | 이하 | 1.079681275 |
| always | 7 | 14 | 1.085905265 | 이하 | 1.084000000 |
| always | 8 | 17 | 1.100748448 | 이하 | 1.124481328 |
| always | 9 | 19 | 1.095811422 | 이하 | 1.044000000 |
| detailed | 0 | 0 | 1.127529660 | 이하 | 1.084000000 |
| detailed | 1 | 3 | 1.121197587 | 이하 | 1.084000000 |
| detailed | 2 | 5 | 1.126519682 | 이하 | 1.084000000 |
| detailed | 3 | 6 | 1.121960947 | 이하 | 1.084000000 |
| detailed | 4 | 8 | 1.123651763 | 이하 | 1.120000000 |
| detailed | 5 | 11 | 1.116388345 | 이하 | 1.120000000 |
| detailed | 6 | 13 | 1.110987781 | 이하 | 1.124481328 |
| detailed | 7 | 15 | 1.133934999 | 이하 | 1.079681275 |
| detailed | 8 | 16 | 1.108283509 | 이하 | 1.079681275 |
| detailed | 9 | 18 | 1.131064472 | 이하 | 1.119521912 |

## 준비 검증과 보존

외부 `run.py`는 새 mode/input 결합과40slot 조립을 추가했고, 기존 수집·정리 owner를 재사용했다. 분석기는 공통40slot 품질 gate·mode별 정확 분수·개별 초과/최악값·보조 분모 처리를 추가했다. 새 선택4함수의 r1은3PASS/1합성 기대FAIL이었다. 순서 차이 gate가 먼저 걸린 fixture만 수정해 해당1함수를 r2에서 PASS했다. 독립 preflight가 발견한 비양수 retained CPU 나눗셈 경계는 r3에서 검사 순서를 보완하고 새1함수/20vector로 PASS했다. **5개 고유 함수·6회 실행이며 원 FAIL을 보존했다.** 완료한 다른 시험을 반복하지 않았고 실제 fresh diagnostic output40개의 정답 검산은 별도다.

원 source·준비 history·입력·freeze·각 slot·분석은 `/home/et16/xadb-dev2-evidence/pf04-mode-attribution-20261025`에 있다. manifest는 598파일·56,025,679byte, SHA-256 `4259974b62507048147bf8e60a90fa50336ddd693dcae7c4eb821a1098411c23`다. manifest self와 최종 outcome review는 제외하고 연결 JSON에서 별도로 결합한다. 독립 원자료 검토는 새 worker/oracle 실행 없이 수행했으며 최종 독립 검토 **PASS_SCOPED_D1_D2_MODE_ATTRIBUTION_EVIDENCE_AND_DOCUMENTS**를 채택했으며 차단0개다. receipt 경로·지문은 연결 JSON에 기록한다.

## 이번 조사 종료와 남은 해결 의무

이번 고정 campaign은 종료했다. 자동 반복·원 인수 재실행·임계값 변경·기각 C1/C2 조합·새 profiler·host tuning을 추가하지 않았다. 모드별 비용과 추가 작업을 관측했지만, 새 의미 보존 제품 수정이나 과거 실패의 확정 원인으로 이어지는 근거는 아직 부족하다. 원인·해결책 UNKNOWN을 유지한다. 다음 조사는 어떤 코드 또는 실행 조건이 원 큰 CPU 대역과 연결되는지 구별할 새 근거를 먼저 정해야 하며, 기존 정상 진단의 반복만으로 실패를 닫지 않는다. 영속 tail·CPU 원인 구분도 별도로 남아 있다.
