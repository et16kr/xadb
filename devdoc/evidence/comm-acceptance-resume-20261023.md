# COMM 미완료 인수 재개·수신 도달 뒤 프로세스 종료

2026-10-23 · **선택 신규 인수 PASS / D1·D2 독립 검토 수용 / 전체 C1 INCOMPLETE**

[구현 기록](../implementation/comm-implementation.md#acceptance-resume-20261023) · [인수 정본](../plans/comm-implementation-plan.md#acceptance) · [시험 범위 우선 지시](../../AGENTS.md)

## 작업과 소유

사용자가 DEV3와 별도로 COMM 미완료 시험의 진행을 요청했다. 작업·계약·통합 담당은 `/root`, 별도 검토자는 `/root/comm_acceptance_review`다. 기준 commit은 `b9d3641`이며 착수 때 DEV2 리뷰 문서와 세 문서 색인의 미commit 변경을 확인해 보존했다. DEV3 코드·시험·공용 계약·dependency·AXF1 bytes를 변경하지 않았다.

소유 구현은 `crates/testing/transport-worker/src/bin/comm_fault.rs`, `scripts/check_comm_rx_kill.py`, `scripts/test_check_comm_rx_kill.py`다. 공유 문서는 COMM 구현/계획과 문서·증거 색인의 이번 항목만 순차 반영했다. 실제 package의 자동 binary 발견을 사용하며 새 crate·manifest·lockfile 변경은 없다. 생성물과 원 receipt는 Git에서 제외된 `artifacts/comm/`의 각 실행 디렉터리에 보존한다.

## 기존 결과의 재확인

아래는 **새 제품 실행 없이** 원본 report·source·generator·build 로그·실행 binary·실행 로그·mutant patch의 hash를 대조한 결과다. 검토자도 별도로 대조했다. `manifest.json`/`running.json`의 RUNNING은 최초 불변 기록이며, 완료 여부는 최종 receipt로 판단한다. 예전 결과를 현재 소스의 PASS로 승계하지 않는다.

| 원 산출물 디렉터리 (`artifacts/comm/` 아래) | 원 source의 실행 결과 | 이번 처리 |
| --- | --- | --- |
| `codec-soak-kzf9ebjf` | codec fuzz 1,800,000ms·652,857,421 iterations PASS | 원본 보존, 재실행 없음 |
| `events-soak-nsv3qhg2` | event fuzz 1,800,000ms·2,724,323,328 iterations·26 event counts PASS | 원본 보존, 재실행 없음 |
| `threads-soak-2pqjta0u` | thread stress 1,800,000ms·11,406,306 rounds·182,500,896 attempts PASS | 원본 보존, 재실행 없음 |
| `mutants-41sc9ect`, `mutants-i7g5gm4a` | 각 원 source에서 11개 production mutant KILLED | 두 source의 원본을 각각 보존, 재실행 없음 |
| `model-syt39xg8` | depth7·36,480,163 states·106,165,909 transitions까지; 최종 receipt 없음 | INCOMPLETE. depth8 PASS로 계산하지 않음 |
| `performance-_3rzyq83` | mode별 receipt74개 PASS / 예상2,700개; 최종 aggregate 없음 | C-V18 INCOMPLETE. 기존 receipt를 보존하며 이번 재실행 없음 |
| `native-fault-matrix-dev1/run.log` | PASS 관측264·N/A8 뒤 `UnexpectedEof`, 시험 전체 FAIL | source/binary identity 미보존. 진단 이력으로만 보존하며 현 source의 cell PASS로 사용하지 않음 |

앞의 세 장시간 실행과 첫 mutant 실행의 source SHA-256은 `7837e52f9908ec502b862e4a688925e93872c5ea4010f31746d24a13d7fbeca7`, 두 번째 mutant 실행은 `3a46f227161d7a32119d099eef404a683b01c0d8f43d0e563d6c00d52cd3fecd`다. 현재와의 관련 차이는 scalar key/namespace·peer 거절 분류·sender 준비 실패 분기 등이다. AXF1 codec 부분은 유지되며, sender/network의 변경 시험은 [10/23 별도 선택 결과](../implementation/comm-implementation.md#review-fixes-20261023)에 귀속한다. 저장소 전체 지문 변경을 무변경 코드의 재시험 사유로 사용하지 않았다.

원본 audit는 `artifacts/comm/resume-20261023-m9ihn844/historical-audit.json`이며 SHA-256은 `9ac1f228927b274bf31ee5bc5f83d1fdb251349c1f88f9cc8408ea782eef9a0e`다. baseline·소유 경로·source별 실행 전 선택 근거도 같은 디렉터리에 남겼다.

## 신규 RX-prefix/process-kill 셀

기존 focused kill은 peer의 성공한 `sendall`·SIGKILL·reap만 증명했고 worker의 실제 partial RX 도달은 증명하지 않았다. 새 engine-free fixture는 실제 연결의 누적 read와 validated frame, 실제 `take_received` payload를 출력한다. 독립 Python peer가 이 기록을 검산한 뒤 해당 PID를 SIGKILL한다.

| stage | 독립 AXF1 prefix | fault 전 필수 관측 |
| --- | --- | --- |
| `header_partial` | 5bytes | read5·validated0·공개 payload 없음 |
| `body_partial` | 17bytes (header10+body7) | read17·validated0·공개 payload 없음 |
| `digest_partial` | 105bytes (마지막 digest byte 전) | read105·validated0·공개 payload 없음 |
| `complete` | 전체106bytes | read106·validated1·실제 payload64bytes가 `00..3f`와 일치 |

세 socket family × 네 stage × 독립 fresh fixture3회로 **신규36셀 PASS**다. 모든 셀에서 ready와 prefix의 source/profile/target·실제 PID·case nonce를 확인하고 `/proc/PID/exe` hash를 ready 확인 뒤와 SIGKILL 직전에 대조했다. prefix 수신 이후 signal 시각, `exit=-9`, reap과 peer EOF를 확인했다. 이번36셀은 모두 EOF였으며 checker는 EOF/reset을 구분해 기록한다. packet fragmentation 제어·Rust Drop 실행·quota 반환·정상 종료·SQL 또는 내구성 인수를 주장하지 않는다.

```bash
python3 -B scripts/check_comm_rx_kill.py
# 변경/실패 셀만 고르는 예:
python3 -B scripts/check_comm_rx_kill.py --cell tcp4/header_partial/0
```

각 셀을 개별 선택할 수 있으며 중복 선택을 거절한다. 이 명령은 기존 leaf·하위 회귀·soak·model·mutant·성능 시험을 호출하지 않는다. 사용 전 실제 diff와 선택 범위의 관계를 확인한다.

## Source별 검증과 수정

| 순서 | 실제 source와 선택 실행 | 결과와 증거 |
| --- | --- | --- |
| 최초 checker | 신규6 method | PASS. 원 tool output을 `checker-first-result.json`에 전사했다. 별도 raw stdout 파일은 미보존이며 사후 생성 로그로 가장하지 않는다 |
| 첫 native | source `0334ce2f74c715923f2ba5f617729134f99a8e8cab20131330c642de0611a113` | 신규36셀 PASS. `rx-kill-tttmoxj0/{manifest,validation}.json`, 실행 binary와 source 보존. 이 버전은 parsed ready/prefix를 보존하며 raw stdout은 미수집 |
| 독립 검토 수정 | 잘못된 TCP endpoint 타입의 명시적 FAIL 보존, bounded raw stdout reader 추가 | 새 부정3 method·변경된 mock-controller1 method만 실행해4 PASS. `checker-delta-{selection,result}.json`·`checker-delta.log` |
| reader 실제 연결 | source `98089cf4c9f721bd2a34279774cd782a5f717201586102d99601b284392f5844` | `tcp4/header_partial/0`만1 PASS. `rx-kill-odgq0g2j/`의 raw `stdout.jsonl`·cell receipt·binary·source 보존. 나머지35셀은 반복하지 않음 |
| 최종 port 수정 | 0/-1/65536 port의 명시적 FAIL 보존 | 변경된 endpoint method만1 PASS. `checker-port-{selection,result}.json`·`checker-port.log`. 다른 checker/native는 반복하지 않음 |
| 정적 검사 | 첫 native 사본의 `cargo clippy -p xadbtest-transport-worker --bin comm_fault --locked --offline -- -D warnings`, 새 Rust 파일의 rustfmt | PASS. 이후 Rust worker는 무변경. 시험 실행과 구분하며 결과는 원 tool output에 보존 |

checker는 **source별6→4→1 실행**이다. 서로 겹치는 method를11개의 신규 시험으로 합산하지 않는다. 최종 파일에는 총9 method가 있으며 변경 없는 method의 기존 결과를 보존했다. raw reader는 malformed/미완결 JSON 원 bytes를 보존하며 크기·대기를 제한한다. wrong source/PID/profile/nonce/binary, 미도달·과도한 prefix, partial의 조기 공개, complete payload 불일치, duplicate receipt, 비정상 endpoint, signal/reap/peer 종료 증거 누락은 PASS가 아니다. 부정 controller 시험은 합성 receipt/mock을 사용하며 OS 동작은 위 native 결과로 따로 증명한다.

| 최종 소유 파일 | SHA-256 |
| --- | --- |
| `crates/testing/transport-worker/src/bin/comm_fault.rs` | `af6cf95b2939fc162bcb77eaf1ad7cd8a7dc859b3cacba085976b918d3e39904` |
| `scripts/check_comm_rx_kill.py` | `bd48647b12993977b8c0f38a36150206e314276f46d4999c28d3430b4bec219e` |
| `scripts/test_check_comm_rx_kill.py` | `0dd508c48be3216e5be8035f57a6fd9f042018d5b20c32b6776152c2ee2f2aec` |

검토자는 원본 evidence와 최종 diff·실제 결과·source별 hash·선택 범위를 읽기 전용으로 대조했다. endpoint 타입과 port 범위 지적을 수정하고 관련 시험만 실행한 뒤 **D1 worker/D2 독립 peer·checker 수용 가능, 잔여 차단 지적 없음**으로 판정했다. `/root`가 계약·통합 담당으로 반영했다. 검토자는 제품 시험을 실행하거나 파일을 편집하지 않았다. 문서 자동 검사는 [push gate](../policies/documentation-policy.md#push-gate)에 남긴다.

## 남은 인수

[C-V01–18 정본](../plans/comm-implementation-plan.md#acceptance)의 분모는 유지한다. 이 증거의 신규36셀은 **C-V09의 RX-prefix/process-kill 부분만** 닫는다.

| 대상 | 남은 범위·다음 공급 |
| --- | --- |
| C-V01–06 | 기존 focused/선택 결과를 원 source에 유지. 전체 변형의 source별 coverage 수합은 미완료이며 신규 코드가 생기면 해당 오류/인접 경계만 선택 검증 |
| C-V07–13·15 | RX-prefix kill 외 전체 native fault 조합, fd 재사용/reset/trickle/slow peer·TX/Connecting/Draining와 공정성·취소/종료 관측의 source/binary/도달 receipt. 과거 FAIL 로그를 완료 증거로 대체하지 않음 |
| C-V14 | thread stress의 원 PASS와 분리해 depth8 모델의 완결 receipt·전체 witness가 필요. depth7 결과를 승격하거나 무변경 모델을 이번 작업에서 다시 실행하지 않음 |
| C-V16·17 | 원 source의11 mutants 및30분 codec/event PASS를 보존. 후속 관련 코드 diff가 있을 때 필요한 사례와 기존 완료 evidence의 관계를 확인 |
| C-V18 | 74/2,700 mode receipt와 smoke를 보존. 전체 workload/slow peer·10쌍·Always/Detailed·계측 비용·독립 통계 수합은 미완료이며 이번 실행 없음 |
| C1 전체 | 위 coverage·원 FAIL·미실행·현재 지원 경계의 독립 수합. DEV3 S5 전수 예외를 COMM/하위 회귀로 확대하지 않음 |

전체 C1은 **INCOMPLETE**를 유지하며 DEV5 Serving·제품 qualification의 기존 미완료 상태도 승격하지 않는다. 시험 완료를 위해 의미 없는 소스 변경을 만들거나, 완료한 시험을 반복하거나, 중단된 시험을 새 PASS로 표시하지 않는다.
