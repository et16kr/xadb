# DEV2 PF04 보존 실행 파일의 주요 경로 검토

2026-10-25 · **읽기 조사 / 제품·도구 변경과 새 시험0 / PF04 FAIL·DEV2 미완료**

[기록](dev2-pf04-hotpath-20261025.json) · [앞선 CPU 집계 조사](dev2-pf04-cpu-accounting-20261025.md) · [해결 계획](../plans/dev2-pf04-resolution-plan.md)

## 판단

보존된 release 실행 파일의 K0 측정 루프·Replace·Snapshot·Read·상세 기록·Snapshot 해제와 영속 관측 시작·완료를 한정해서 읽었다. **이 범위에서 원 실패를 해결할 만한 새로운 큰 중복 작업은 확인하지 못했다.** 최적화 가능성이 전혀 없다는 증명은 아니다. 기계어에 있는 명령 수를 실행 횟수·시간 비중으로 바꾸거나 새로운 성능 profile로 보고하지 않는다.

따라서 이미 기각한 C1/C2를 합치거나, 작은 정리를 근거로 기존 인수를 다시 실행하지 않는다. off-CPU 스케줄링 수집기도 지금 추가하지 않는다. 원 K0 실패는 경과 시간과 thread CPU·실행 전반의 p50가 함께 증가했으므로 off-CPU 구간만으로 설명할 수 없고, 그 수집 결과가 바로 결정할 수정도 아직 없다.

## 실제 읽은 대상

외부 보존 위치는 `/home/et16/xadb-dev2-evidence/pf04-hotpath-review-20261025`다. 두 ELF의 SHA-256을 기존 build receipt와 대조했고, `objdump`의 고정 주소 범위7개와 제품/worker source5개를 보존했다. 실행 파일을 새로 build하거나 실행하지 않았다.

| 대상 | 보존 ELF의 SHA-256 | 주요 확인 위치 |
| --- | --- | --- |
| K0 baseline release | `b554e895ab3026f5d393c5566ac2cbef3b406801c659ef4da9b0800a2096fb45` | iterations `0x73cd0`, step `0x73f50`, replace `0x8e800`, snapshot `0x8f000`, read `0x8fc40`, record_event `0x91390`, Snapshot drop `0x74940` |
| 영속 P3 release | `ea1f0f7c5f2d58a61199f399ae6961f290449dea488a9187ebf11505bc501c51` | Observations::start `0xbf660`, Span::finish `0xbfb90`, Span::record `0xbfbd0` |

두 build receipt의 source fingerprint는 `7ced0a5622825b971a4a146cdd8d1ce4a5ef01eefe633e59deb6b338390a5329`다. source 읽기 기준은 `5253caa`이며 현재 `78517fa`와의 관련5파일 diff도 보존했다. diagnostics는 동일하고 나머지4파일은 dependency alias/import 정리다. 현재 전체 제품과 보존 ELF가 동일하다는 주장은 하지 않는다. 최초 수집 보조 명령은 현재 source의 byte 동일성 검사에서 substrate의 import 차이로 중단했고, 이후 원 source와 현재 diff를 분리 기록했다. 제품 시험 실패나 재실행이 아니다.

## 확인한 경로와 한계

| 항목 | 기계어·source 확인 | 판단 |
| --- | --- | --- |
| K0 시작·완료 counter | Replace `0x8e82a/0x8e92e`, Snapshot `0x8f03a/0x8f1da`, Read `0x8fc7e/0x8fcee`의 포화 검사·CAS 재시도 | 기존에 확인한 성공 연산당2회 갱신과 일치한다. CAS 재시도 빈도나 시간은 측정하지 않았다. 정확한 시작/완료·panic·동시 reader·포화 의미를 유지해야 한다 |
| Detailed full queue | 세 호출 경로가 full hint를 읽고 drop counter를 갱신한다. Read는 `0x8ff2f`부터이며 full이면 `record_event` 호출을 우회한다 | 기존 PF12 최적화가 실제 ELF에도 있다. full queue마다 lock을 잡는다는 새 원인 가설을 채택하지 않는다 |
| Snapshot 소유권 | 활성 collector의 추가 clone은 `0x8f160`의 원자 증가이며 drop 경로에는 대응 참조 해제가 있다 | 기존 C1 대상이다. snapshot/cursor가 store보다 오래 사는 의미 때문에 단순 삭제할 수 없고, C1 확인 실패를 재해석하지 않는다 |
| 영속 Span | start의 requested/items가0이면 해당 갱신을 건너뛴다. record의 complete bytes도0이면 우회하며 max는 현재 값과 비교 후 필요한 경우 CAS한다 | 기존 zero/max 최적화가 실제 ELF에 남아 있다. finish의 정상 경로는 record 뒤 반환하며, unwind 분기를 정상 이중 기록으로 세지 않는다 |
| NamespaceWait/Hold의 숨은 items | 두 callsite가 items=1을 넘기지만 공개 snapshot의 items는 None이다 | 작은 별도 정리 후보일 뿐이다. 전역 counters_saturated의 의미 보존을 먼저 증명해야 하며, 5% 개선이나 원 K0/영속 실패 해결 근거가 없어 이번에 구현·측정하지 않는다 |

K0 공통 할당·quota·row 접근을 줄이면 Off와 On 모두에 영향을 준다. 공통 비용 감소를 계측의 상대 비용 한도 해결로 계산하지 않는다. C1 약2.46%·C2 약2.92%의 확인 결과는 각 동결 계약에서 기각된 상태를 유지한다. 두 수치를 더해서 새 개선 효과로 주장하지 않는다.

## 다음 작업의 경계

남은 문제는 원 실행의 CPU/처리 속도 변동과 영속 tail/CPU 초과의 원인 구분이다. 기존 PMU 도구가 있으므로, 추가 도구를 만들어야만 원인을 조사할 수 있는 상태는 아니다. 동일 ELF를 대상으로 한 고정 진단을 계속하려면 [AGENTS.md](../../AGENTS.md)의 무변경 코드 시험 제한과 적용 범위를 먼저 명확히 해야 한다. 현재 명시 예외는 별도 COMM와 최종 DEV3 S5이며 PF04 환경 진단으로 확대하지 않는다.

필요한 후속은 원 source·입력·길이·한도를 보존하고, 특정 가설과 실행 수·중단 조건을 미리 정한 원인 구분이다. 성공할 때까지 인수를 반복하거나 실패 표본을 빼는 방식은 선택하지 않는다. 이번 기록은 그 실행의 승인·성능 개선·DEV2 완료 증거가 아니다. 구체 후속 범위와 독립 검토는 연결 계획·기계 판독 기록에 남긴다.

최종 독립 검토 **PASS_SCOPED_D1_D2_STATIC_EVIDENCE_AND_CONDITIONAL_PLAN**를 채택했다. 설계 차단0개이며 사용자 예외는 여전히 대기다. receipt는 `/home/et16/xadb-dev2-evidence/pf04-hotpath-review-20261025/final-review.json`, SHA-256 `a23723dc2f9e6031a16c5df02d1ed660b80744bacdc418cd4ad39387362e0a8a`다. 보존 manifest18파일의 지문과 준비된 후속 proposal 지문은 연결 JSON을 따른다.
