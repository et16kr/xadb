# DEV2 PF04 사용자·커널 CPU 분리 관측

2026-10-25 · **선택6함수·실제2연결 완료 / 최종 독립 D1/D2 한정 채택 / PF04·DEV2 미완료**

[선행 계획](../plans/dev2-pf04-resolution-plan.md#cpu-user-kernel) · [source·결과 색인](dev2-pf04-cpu-split-20261025.json) · [C2 기각·WAL 판단](dev2-pf04-single-writer-20261025.md)

## 구현과 실행 범위

계획을 `b33f1e4`로 먼저 commit하고 독립 계획 검토 뒤 수집 도구만 수정했다. 담당·통합은 `/root`, 독립 검토자는 `/root/pf04_evidence_review`다. 기본 `user-v1`의 원6개 event를 유지하고, 고정 `user-kernel-v1`에 `cycles:k`, `instructions:k`를 추가했다. 선택은 CLI→수집 argv→parser 예상 집합에 전달되며 실패·RUSAGE_ONLY 결과에도 남는다. 임의 event·다른 TID·시스템 전체 수집을 추가하지 않았다. 원 CAP_PERFMON·일반 사용자 worker·소유 process/collector identity·pidfd·deadline·정리 경로는 유지했다.

실제 diff에 대응하는 기본 parser·fake 연결2함수와 신규4함수만 실행하여6 PASS다. 신규 범위는 독립 literal8개 정상/누락/부분/미지원/중복/예상 밖 event, 잘못된 profile의 입력·출력·spawn 전 거절, CLI→fake perf→parser, 실패/미지원 profile 보존이다. fake 출력은 실제 PMU 증거가 아니다. Python4파일 문법 검사는 실행 시험과 구분한다. 완료한 Rust hook·privilege/lifecycle·BPF·하위 회귀·전체 workspace·원 PF04 인수는 반복하지 않았다.

구현과 외부 `observe.py`는 실제 실행 전 독립 검토 PASS를 받았다. 새 실행기는 기존 P1 Runner를 사용하며 P3b의 변경 없는 idle/affinity와 payload 전달 함수만 재사용한다. C2의 미검증 timeout race가 있는 일회성 실행기는 재사용하지 않았다. worker CPU2는 payload 전달 전에, 실제 perf collector CPU3는 준비 ACK·identity 확인 뒤 READY 전에 확인했다.

## 실제 관측

P3의 보존 release ELF `ea1f0f7c5f2d58a61199f399ae6961f290449dea488a9187ebf11505bc501c51`, 제품 source `7ced0a5622825b971a4a146cdd8d1ce4a5ef01eefe633e59deb6b338390a5329`를 재사용했다. 새 build·제품 수정은 없다.128행·warmup64·측정512·seed20261018, Off→Always 각1회·총2slot을 완료했다. 두 slot 모두 시작 전10초 CPU2/18·disk busy5% 이하를 첫 관측에서 만족했고,60초 deadline 안에 정리됐다. worker·sudo/perf group 종료 및 pin한 collector 종료를 확인했다. 재시작·서비스·sysctl·governor 변경은 없다.

| 관측 | Off | Always |
| --- | ---: | ---: |
| 사용자 instructions | 13,571,051 | 14,863,461 |
| 커널 instructions | 20,775,069 | 20,589,750 |
| 사용자 cycles | 14,351,225 | 15,278,608 |
| 커널 cycles | 20,829,631 | 20,939,151 |
| 사용자 ref-cycles | 20,118,925 | 21,304,250 |
| context switches | 1,039 | 1,043 |
| CPU migrations | 0 | 0 |
| page faults | 27 | 29 |
| 각 event runtime ns | 25,973,037 | 26,326,191 |
| 각 event reported coverage | 100.00% | 100.00% |
| 원 측정 thread CPU ns | 28,412,021 | 29,039,299 |
| 경계 rusage user ns | 15,857,000 | 10,235,000 |
| 경계 rusage system ns | 12,597,000 | 18,843,000 |
| 원 측정 elapsed ns | 2,758,469,037 | 2,760,140,569 |
| 원 cycle p99 ns | 5,517,289 | 5,584,315 |

8개 모두 count·runtime·reported coverage를 원문으로 보존했다.100.00%는 perf의 보고 정밀도이며 완벽한 동시 측정의 별도 증명이 아니다. PMU는 제어 bracket을 포함한다. bracket은 각각2,761,485,636ns·2,764,133,301ns, worker 경계의 원 측정 대비 excess는8,376ns·5,501ns다. event runtime·CPU clock·rusage·wall bracket을 같은 시간량이라고 강제하지 않는다.

두 실행의 exact 진단 suffix·envelope identity·build·원 elapsed를 확인한 뒤 임시 copy만 기존 path로 정규화하여 독립 oracle로 검산했다. 원 output을 변경하지 않았다. 각512개 측정 거래의 값·old reader·ACK/영속 image가 통과했으며, 전체 관측 commit578은 setup/warmup을 포함한다. 이 결과의 ACCEPTED는 진단 정답 범위이며 PF04 인수 합격이 아니다.

## 해석과 다음 판단

이번 두 구간의 커널 instruction/cycle 수는 비슷했지만 rusage system 시간은12.597ms에서18.843ms로 달랐다. 사용자 instruction 수가 증가한 구간에서 rusage user 시간은 오히려 감소했다. 따라서 rusage user/system 배분만으로 해당 하드웨어 작업량의 증가·감소를 판단하지 않는다. 커널 PMU는 target TID가 실행되는 privilege domain을 센다. 특정 syscall·별도 journal task의 비용, 순수 workload CPU 또는 GHz가 아니다.

새 profile은 기존 user-only 관측에서 빠졌던 커널 counter를 실제로 제공한다. 그러나 두 번의 고정 순서 연결에는 같은 profile의 별도 대조나 반복·무작위화가 없으므로 안정적 모드 비용, 수집 영향, 과거 실패 pair의 원인을 추정하지 않는다. 현재 커널 count가 비슷하다는 이유로 과거 kernel 변동을 배제하지도 않는다. parser의 ratios는 null이며 성능 효과·영향 screen은 NOT_EVALUATED다.

다음 판단은 설치 커널의 CPU accounting과 perf event runtime 구현을 실제 revision으로 읽어 시간·count가 달라질 수 있는 경계를 확인하는 것이다. 이미 완료한 두 slot이나 기각 C1/C2를 다시 실행하지 않는다. 이후에도 원인 구분이 필요하면 새 관측 변경과 관련 사례·측정 예산을 먼저 동결해야 한다. WAL 선확보/data sync를 이번 상대 계측 비용의 해결책으로 채택할 근거는 추가되지 않았다. 원 네 campaign FAIL·PF04·DEV2/B2/D0 미완료를 유지한다.

## 보존·검토

외부 루트는 `/home/et16/xadb-dev2-evidence/pf04-cpu-split-20261025`다. baseline/수정 source·patch·선택 목록·시험 원 로그·고정 driver·입력·idle·identity·PMU raw·worker output·영속 파일·oracle 결과와 분석을 보존했다. `artifact-manifest.json`은121파일을 연결하며 SHA-256은 `14c5f457dbb6f06604c07d4170f4ce3bbfb653c413fc33a55a895e5c17d18ff5`다. 자기 자신·최종 검토 receipt·Python cache는 제외한다.

실행 전 코드 검토 receipt `code-review.json`의 SHA-256은 `53c0f22cf1bcf76d5bf1cbc36bb1e73a7ad98f9529061c3bcfee17fe905e5d65`다. 최종 독립 검토 **PASS_SCOPED_D1_D2_USER_KERNEL_PROFILE_AND_RETAINED_TWO_CONNECTIONS**를 채택했다. 차단0개이며 원자료·해석·문서 범위다. receipt `final-review.json`의 SHA-256은 `9ce1a5dea0cf8555bb007e88872c0bd467dc3afda7500e9fca7f02195077bb40`다. 안정적 비용·과거 실패 원인·PF04/DEV2 완료 승인이 아니다. 문서 verifier는 push gate에 남겼고 push는 하지 않았다.
