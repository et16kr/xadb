# DEV2 PF04 warmup 전 관측 경계

2026-10-25 · **구현·선택11함수·실제4연결 완료 / 최종 독립 D1/D2 한정 채택 / PF04·DEV2 미완료**

[선행 계획](../plans/dev2-pf04-resolution-plan.md#prewarmup-boundary) · [source·검증 기록](dev2-pf04-prewarmup-20261025.json) · [선행20회 UNKNOWN](dev2-pf04-off-classification-20261025.md)

## 구현한 경계

계획을 `12096d4`로 먼저 commit하고 D2 계획 검토 뒤 XADBTest worker와 Python 수집 경계를 수정했다. 담당·통합은 `/root`, 독립 검토는 `/root/pf04_evidence_review`다. 기존 활성 진단이 warmup 뒤 START/GO 교환을 하여 비활성 인수와 측정 직전 상태가 달라지는 관측 한계를 다룬다. 이 차이가 과거 비용 실패의 원인이라고 확정한 것은 아니다.

K0 전용 `prewarmup-continuous-v1`과 `prewarmup-wait-v1`을 명시적으로 선택한다. 두 모드 모두 setup 뒤 warmup 앞에서 ARM/ARMED로 PMU를 한 번 enable한다. continuous는 warmup 뒤 로컬 clock/rusage만 읽고 측정에 진입하며 wait는 같은 위치에서 START/GO를 추가 교환한다. wait의 두 번째 교환에서는 PMU를 다시 enable하지 않는다. 기본 measurement 모드의 HELLO v1·START/GO와 비활성 경로를 유지한다. 잘못된 모드·부분 환경 설정·durable 선택·wire identity 불일치는 거절한다.

새 PMU 범위는 **warmup+drain+measurement+control bracket**이다. setup·최종 정답 값 수합/직렬화는 제외되며, 원 loop의 thread CPU/elapsed/latency 측정 위치는 유지한다. PMU에서 warmup이나 경계 비용을 추정하여 빼지 않는다. 새 출력 suffix 두 가지는 원 성능 인수 checker가 거절하며, 정답 adapter만 정확한 suffix와 build identity를 확인한 뒤 메모리 사본의 path를 정규화한다. 원 output byte는 보존한다.

기존 P1의 socket 상한·교환 deadline·전체 실행 한도·소유 group/pidfd 정리와 일반 사용자 worker/CAP_PERFMON 수집 경로를 재사용했다. 새 crate·외부 의존·source root·영속 형식·engine/WAL 변경은 없다. 총8개 변경 파일은 실행 기록에 각각 hash로 연결한다.

## 변경에 대응한 선택 검증

동시 DEV3 미커밋 변경을 제외한 `12096d4` export에 이번8개 파일만 덮어쓴 source에서 검증했다. source 지문은 `759d9952d8a45bffc342eb852ee6f29cc84e3991f7a633becb895cbc09071604`, release worker SHA-256은 `14d25271a4927a2eac0ce7293afd67bd18c561bf2e34c75eda74f8c8e4909eb2`다. 최종 저장소8파일과 검증 export의 일치를 읽기만 확인했다. 이를 동시 변경을 포함한 전체 workspace 검증으로 확대하지 않는다.

| 선택 범위 | 실제 결과 | 변경과의 관계 |
| --- | --- | --- |
| Python6함수 | PASS | 신규5: 모드/HELLO·STOP 시간/상한·enable1회·ARM/START/warmup 실패 정리·ARM 뒤 취소. 수정 observe의 legacy fake 연결1 |
| Rust exact5함수 | 각각 PASS | 신규3: 명시 설정·arm/begin 상태/순서·잘못된 ARM ACK. 수정 disabled/legacy 분기2 |
| compiler·Clippy·release·fmt | PASS | worker library/출력 adapter no-run compile, 관련 worker Clippy, release build, 수정 Rust4파일 fmt |
| 실제 연결4개와 각 출력 oracle1개 | 모두 PASS | 새 모드2와 변경된 legacy active/disabled 각1. 새 출력의 값·counter·identity 검산 |

미변경 exchange/권한/하위 제품 시험·전체 workspace·원 PF04 인수·기존20회는 반복하지 않았다. source 지문 변화만으로 시험 범위를 넓히지 않았다. synthetic fake PMU 결과와 실제 hardware PMU 결과는 별개다.

## 실제 연결 결과

실행 전 driver와 입력을 동결하고 독립 preflight를 수용했다.4건 총1.532522243초, 실패·중단·재시도0이다. worker CPU2는 payload 전에, 실제 perf collector CPU3는 READY 전에 identity와 함께 확인했다. slot60초·수집30초·oracle5초와 정리 예약을 두었고 전체 예산은240초였다. 기능 검증이므로 idle screen이나 성능 비교를 하지 않았다.

| 경로 | PMU 범위 | 수집·정답·정리 |
| --- | --- | --- |
| prewarmup-continuous-v1 | warmup 포함 |8event 모두 reported100.00%,65,536회 완료·정답 PASS, 소유 정리 PASS |
| prewarmup-wait-v1 | warmup 포함 |8event 모두 reported100.00%,65,536회 완료·정답 PASS, 소유 정리 PASS |
| legacy active | 기존 measurement bracket |8event 모두 reported100.00%,65,536회 완료·정답 PASS, 소유 정리 PASS |
| disabled | PMU 없음 | 기존 output path,65,536회 완료·정답 PASS, 소유 정리 PASS |

새 두 경로의 ARM 뒤부터 loop 시작까지는 각각895,570ns와1,975,800ns이며 그 안에 원 warmup894,918ns와914,365ns가 들어간다. 이는 **시간 범위 포함 검산**이다. 한 번씩의 수치를 경계 효과나 성능 차이로 판정하지 않는다. 각 STOP은 원 elapsed와 결합되고, ARMED 송신 하한≤armed≤loop start≤loop end≤STOP 수신을 충족한다. wait의 GO 송신 하한≤loop start도 확인했다. 총11개 소유 group·실제 collector3개의 종료 기록과 capture overflow/error 없음이 남아 있다. 재부팅·서비스·host 설정 변경은 없다.

`go_ns`와 `armed_go_ns`는 `send()` 직전의 송신 측 하한이다. 실제 peer 수신·ACK 완료 시각이나 정확한 응답 지연으로 읽지 않는다. 실패 실행의 `phases`는 완전한 chronology가 아닐 수 있으며 이번 실패 fixture의 부분 raw와 소유 정리 증거로 범위를 한정한다. reported100.00%도 perf의 보고 정밀도이며 loop-only 또는 완벽한 동시 hardware 측정 증명이 아니다. continuous에도 로컬 관측 호출이 남으므로 원 비활성 인수와 완전히 같은 상태라고 주장하지 않는다.

## 보존·채택 범위와 후속

외부 루트는 `/home/et16/xadb-dev2-evidence/pf04-prewarmup-20261025`다. source·원 diff·선택 목록·원 로그·실행 파일·입력·raw output·PMU·oracle·정리·읽기 집계를 보존했다. `manifest.json`은5,050파일368,809,390byte를 결합하며 SHA-256은 `26ea0987b2899769c8419a7b337387292f5081c05a895d9dc4d5ef832172d3f2`다. 자기 자신·후속 최종 검토 receipt·Python cache·실행하지 않은 target build cache는 제외한다. 실제 실행한 lib-test·정답 oracle·release worker는 포함한다.

계획·구현 정적·native preflight receipt와 hash는 [기록](dev2-pf04-prewarmup-20261025.json)에 있다. 최종 원자료·문서 검토 **PASS_SCOPED_D1_D2_IMPLEMENTATION_FUNCTIONAL_EVIDENCE_AND_DOCUMENTS**를 한정 채택했다. 차단0개이며 `outcome-review.json`의 SHA-256은 `2ffc8fb16e8d6a84263fe6fa4ec02fb1d88f04a5daaab50b35360ebc37a91ec3`다. 원자료와 manifest 전 항목의 독립 검산을 포함하며 성능 효과·인수 또는 추가 실행 승인은 아니다. 문서 verifier는 push gate에 남기며 push는 하지 않았다.

이 단위는 새 관측 도구의 기능을 검증했다. **경계의 성능 효과 NOT_EVALUATED, 원 PF04 네 FAIL·DEV2/B2/D0 INCOMPLETE**를 유지한다. 다음 구분 실험은 같은 ELF·warmup 포함 범위의 두 새 모드10쌍을 순서 균형으로 대조하는 것이다. 별도 입력·품질·예산·결과별 조치와 무변경 코드 시험 예외를 먼저 동결해야 하며, 이번4연결이나 이전 소진한 “허용”을 추가 성능 실행 승인으로 쓰지 않는다.
