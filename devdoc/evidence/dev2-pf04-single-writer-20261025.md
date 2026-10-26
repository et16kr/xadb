# DEV2 PF04 C2 단일 쓰기 통계 후보 평가

2026-10-25 · **C2 확인 기준 미달·제품 미반영 / 독립 D1/D2 기각 기록 한정 채택 / PF04·DEV2 미완료**

[동결 계획](../plans/dev2-pf04-resolution-plan.md#c2-single-writer) · [source·실행 기록](dev2-pf04-single-writer-20261025.json) · [선행 I/O 관측](dev2-pf04-flush-provenance-20261025.md)

## 결정

사용자의 “이어서 해주세요”에 따라 계획을 `18eb879`로 먼저 commit하고 C2를 구현·평가했다. 작업·계약·통합 담당은 `/root`, 독립 검토자는 `/root/pf04_evidence_review`다. `688b9af`의 깨끗한 export와 그 위에 C2만 적용한 export를 비교했다. 병행 DEV3 수정은 포함하지 않았다.

C2는 `VolatileStore`의 Create/Drop/Insert/Replace/Delete 다섯 label에 한해 포화 counter를 atomic load→checked_add→store로 갱신했다. 이 연산은 `&mut self`의 단일 작성자만 수행한다. 비 Clone `MutationDiagnostics`와 내부 5/6 label 타입으로 writer 소유권을 제한했고, Snapshot/Read 등 공유 경로는 기존 CAS를 유지했다. wrapper의 추가 Arc·할당·charge는 없고 공통 Counters, dropped/emitted/queue/sequence, row/quota/오류/공개 순서는 유지했다. 이전 C1 Arc 통합을 재도입하지 않았다.

확인 측정의 중앙 비율은 **0.9707905(약2.92% 감소)**지만 구간 상한 **0.9765757**이 동결된 **0.95 미만** 조건을 충족하지 못했다. C2는 **REJECTED**다. 외부 후보 source·patch·실패/불충분 판정을 보존하고 제품에는 반영하지 않았다. 현재 세 대상 파일이 기준선과 같음을 hash로 확인했으며 기각 뒤 무변경 제품 시험을 반복하지 않았다. 작은 개선을 5% 이상의 효과나 PF04 합격으로 바꾸지 않는다.

## 변경 범위의 검증

| 대상 | 실제 선택·결과 |
| --- | --- |
| 기존 diagnostics4함수 | 내부 label 타입·공유 완료 helper 변경에 대응,4 PASS |
| 신규 diagnostics3함수 | 5개 mutation label 성공/실패와 MAX 경계, panic 미완료·writer 종료 뒤 shared 수명, 단일 writer와 shared producer/동시 drain 보존,3 PASS |
| 기존 performance3함수 | mode별 값/ID/오류/quota,11label 성공/실패, concurrent reader loss accounting,3 PASS |
| 신규 performance1함수 | 실제 Store의512 replace와 다른 thread의512 old-snapshot read·store 종료 뒤 값 보존,1 PASS |
| compiler 반례2개 | writer clone은 E0599, shared 경로의 mutation label은 E0308 단일 오류로 거절; 실행 시험 없음 |
| 관련 fmt·engine/worker Clippy | PASS. 전체 target compile과 실제 시험 실행은 구분 |
| release build | 같은 toolchain/target/config의 baseline·candidate 각각 PASS |
| 외부 일회성 실행기 새 사례 | 정상 종료·이미 존재하는 별도 PG 자식의 timeout 정리2개, idle 정상·6개 잘못된 표본 거절 PASS. 아래 미검증 race를 포함한 일반 수명 보장 아님 |

제품 시험은 고유11함수(기존7+신규4)다. 무변경 하위/전체 workspace wrapper, 별도 PMU, 원 PF04 campaign 재실행은0회다. 기존 bounded collection·invalid capacity 사례도 변경과 무관하여 반복하지 않았다. compiler 반례는 후보 사본에 잘못된 호출을 임시 주입하고 원 byte를 복원한 뒤 release build했다. 원 source/hash·선택 명령·로그를 보존했다.

## 고정 비교

128행·old snapshot·capacity256·warmup4096·measure65536·stride16·fixture seed20261018을 유지했다. 각 campaign은10쌍,5AB+5BA이며 같은 baseline controller가 두 worker를 실행했다. CPU2 controller/worker, sibling18을 관측하고 시작 전10초 각각 busy5% 이하를 확인했다. 자체 build/test와 측정을 겹치지 않았고 재시작·host 설정 변경은 없다. 공유 host의 외부 간섭을 완전히 격리한 결과는 아니다.

| campaign | 실행 | elapsed 중앙 비율 | 순서통계 구간 | 정답/유효성/guard | 결정 |
| --- | ---: | ---: | --- | --- | --- |
| Detailed 탐색 | 10쌍 | 0.9721452 | [0.9590664,0.9852442] | PASS/VALID/PASS | 중앙값<1로 확인 진입 |
| Detailed 확인 | 새10쌍 | 0.9707905 | [0.9681296,0.9765757] | PASS/VALID/PASS | NO_MATERIAL_CHANGE, C2 기각 |
| Off guard | 0 | 해당 없음 | 해당 없음 | NOT_RUN | 동결 중단 규칙 |
| Always guard | 0 | 해당 없음 | 해당 없음 | NOT_RUN | 동결 중단 규칙 |

총40native 실행이며 최대80회를 채우기 위한 추가 측정은 하지 않았다. elapsed/CPU/external1.15배·sample p99 1.25배·RSS 증가4MiB 한도는 그대로다. 확인의 worst elapsed1.011711, CPU1.011659, external1.010046, p99 1.036900, RSS 증가61,440bytes다. 구간은 기존 run-level 정렬 비율 index[1,8], nominal coverage1002/1024이며 IID 가정을 공유 host의 실제 보장으로 확대하지 않는다. 분석기의 `cause_status=UNRESOLVED`, `promotion=INCOMPLETE`와 원 PF04 FAIL을 유지한다.

## P4 WAL 판단과 남은 원인

이번 PF04 해결책으로 WAL 선확보/data sync는 미채택했다. P3b에서 fsync·journal·block 요청의 연결은 확인했지만, 공통 대기를 줄이는 것이 Off/On 상대 계측 비용 FAIL을 해결한다는 근거는 없다. 추가 CPU가 고정되어 있으면 공통 대기를 줄일 때 상대 비용이 커질 수도 있다.

설치 커널 Ubuntu `7.0.0-38.38`의 보존 소스는 data sync도 `i_datasync_tid`의 journal commit을 기다리고, on-disk 파일 크기 변경을 data-sync 필요 metadata로 처리함을 보여 준다. PostgreSQL 로컬 `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`의 segment 준비·sync도 확인했다. XADB의 물리 EOF/유효 tail·quota·rotation·backup·복구 계약을 그대로 둔 한 줄 교체와 다르다. 소스 경로/hash와 실제 revision은 `wal-source-review.json`에 보존했다. 이는 제품 전체의 WAL 성능 개선을 영구 기각한 결정이 아니다.

영속 진단의 같은 모드 내 user instructions는 거의 일정하지만 thread CPU와 user/system 분할은 달랐다. 기존 user-only PMU만으로 커널 CPU의 변동을 설명할 수 없다. 또 durable Off에도 거래 통계와 clock은 남으며 Always 추가 범위에는 WAL3개뿐 아니라 namespace wait/hold도 포함된다. 다음 관측은 user/kernel CPU·cycles를 같은 구간에 대응하는 좁은 보완이어야 한다. 이 C2 측정으로 원 영속 CPU·tail 실패를 설명했다고 주장하지 않는다. C1/C2 재확인이나 후보 조합을 이번 종료 예산에 추가하지 않는다.

## 보존과 실행기의 한계

외부 루트는 `/home/et16/xadb-dev2-evidence/pf04-single-writer-20261025`다. source archive·patch·두 release·원 output·recipe·선택 로그·compiler 반례·소유권 검토를 보존한다. `source-manifest.json`은 양쪽9,656파일을 연결한다. 최종 artifact manifest와 독립 검토 receipt는 연결 JSON에 기록한다.

실행기 검토 통지가 도착했을 때 두 campaign은 이미 정상 종료되어 기각 결정까지 완료됐다. `command.py`는 timeout 정리에서 session을 한 번만 스캔하므로 스캔 뒤 controller가 새 worker를 생성하는 race를 막지 못한다. 정리 도중 예외에 대한 보장도 완전하지 않다. 기존 helper 반례 통과를 이 경계의 검증으로 표시하지 않는다. 실행 byte를 사후 교체하거나 실패를 숨기지 않으며 이 일회성 도구는 재사용·지원 도구로 채택하지 않는다. 실제 성능 관련8개 command는 모두 exit0·timeout 없음, 최장0.779초 미만으로 완료했다. 독립 검토가40개 history kind5 정상 종료·completion과 원자료를 확인했다. kind5는 controller의 child exit 관측·group 정리·wait/reap 성공을 필요로 하는 경로다. 개별 worker PID 목록·현재 부재를 직접 보존한 census를 뜻하지 않는다. 문제의 timeout 경로를 사용했다고 주장하지 않는다.

문서 verifier는 push gate로 남겼으며 push는 수행하지 않았다. 원 PF04 네 campaign FAIL, DEV2/B2/D0 미완료는 유지한다.

최종 독립 검토는 **PASS_SCOPED_D1_D2_C2_REJECTION_WAL_NONADOPTION**으로 채택했다. 차단0개이며 제품 성능·PF04 합격이나 외부 실행기 일반 수명 승인이 아니다. receipt `/home/et16/xadb-dev2-evidence/pf04-single-writer-20261025/final-review.json`, SHA-256 `a5cc314093ef1c4ba997ea59da2f8031f1feb71c1ad39182c8dca21d34f49280`를 보존한다.
