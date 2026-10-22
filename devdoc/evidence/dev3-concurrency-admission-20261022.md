# DEV3 적용 admission·version gate 인수

2026-10-22 · D3-CONCURRENCY-ADMISSION-001 · 3.10.1.a.1 제한 단위 인수 수용 / 전체 DEV3 미완료

[소유 계약](../contracts/dev3-concurrency-admission.md) · [실행 색인](dev3-concurrency-admission-20261022.json) · [현재 구현](../implementation/dev3-implementation.md#concurrency-admission) · [시험 계획](../plans/dev3-test-plan.md)

사용자의 DEV3 완료·병렬 진행·단계별 commit 요청에 따라 실제 적용/content admission과 exclusive version mutex를 공급했다. 구현 담당은 core와 test-only seam, 시험 담당은 독립 배열 oracle·실제 thread·fixture, 총괄은 공통 fence 연결·compiler/runtime runner·자원 검증·실행, 별도 검토자는 D2와 D1·원본 증거를 읽었다. Commit 설치 전이는 cfg(test) 모의 명세이며 실제 WAL/status/DPT 또는 durable Commit 권한을 제공하지 않는다.

## 공급과 실제 결과

고정34-slot은 Foreground32·Maintenance1·DedicatedFlusher1로 나뉘며 역할 간 대여하지 않는다. Owner와 자식 guard는 non-Clone·non-Send/non-Sync이고 독점 borrow로 중복 admission을 막는다. 실제 version mutex는 try-lock, 짧은 state 진입과 Drop 회계는 blocking lock이다. unwind에서 공통 SessionFence를 먼저 설정한 뒤 gate와 admission을 반환한다. 오류 정리 중 처음 발견한 poison/손상/fence도 CallerMustResolve로 반환하며 원래 fatal cause는 보존한다. snapshot·pin·tree/page rank·거래 잠금·FIFO는 이 단위의 공급이 아니다.

| 실행 범위 | 결과와 원래 source |
| --- | --- |
| GA08 compiler12 | 원래 baseline·release 성공2, 의도한 code/primary span의 거절10 PASS. 누적13.596초/30초. source `f1d64dbc73190b96e38c7098dd6de6a52b6adee35d4ddb67a1ca5cc39a8c3ec3`; runtime ELF가 아닌 compiler metadata |
| 새 compiler checker4 | 입력·변조·선택·누적 budget의 새 분기 PASS. 이전 공통 helper 시험은 반복하지 않음 |
| 새 runtime checker4 | 좁은 선택·fixture census·정확한 실행/완료 마커·source/ELF build receipt 연결 PASS. 제품 실행과 구별 |
| mandatory GA01–07 | 정확7개 그룹·45개 named case PASS, 총5.205초/120초. GA01=10, GA02=2, GA03=4, GA04=4, GA05=10, GA06=12, GA07=3 |
| 독립 oracle | 위 GA07 안의 부정 입력20개 PASS. 잘못된 상태7·action trace5·선택/중복4·재검사/해제순서 witness4를 거절 |
| 자원 | 위 GA07 안의 concrete layout27개,34-owner+33-content+1-version 실제 동시 보유와 완전 반환 PASS. source 논리 retained7248·nested temporary896·constructor7832bytes 검토 |
| S5 확대 |4개 ignored selector×128=512개 고정 schedule 작성. NOT_RUN. fixture census를 실행 성공으로 세지 않음 |

runtime source는 `e9eda90aaa91015823fddbc7302fb45dcb5ab995aed7327ea707929eb3273cd4`, 보존 ELF SHA는 `e5c30e106298cca29b7a39a3892d469b928a4422c996cf0616ebb9b1d7c5012f`다. 실제 두 scoped worker와 controller의 채널 ACK로 interleaving을 제어한다. postcheck 직전/실제 gate 획득/거절과 unwind fence-before-release를 관측한다. blocked Drop은 실제 state lock 보유 중5초 noncompletion, lock 반환 뒤 completion과 정확한 회계를 확인한다. 이는 OS 공정성·상위 작업 진행 보장이 아니다.

actual layout은 test core760·owner24·apply16·content8·version40·State600·State mutex608bytes다. test-only hook·installation 상태가 포함된 target-layout이며 source 논리 상한의 전제를 검산한다. 컴파일러 실제 stack·OS 자원 전체나 상위 caller34 제한을 측정한 결과가 아니다. 전체64KiB ingress·전역 F05 완료는 주장하지 않는다.

## 실패·수정과 범위 유지

첫 runtime-r0 빌드는 시험의 Result 임시가 owner borrow를 오래 유지하여 E0505로 실패했다. 제품 시험은 실행하지 않았고 원래 source·진단·FAIL을 보존했다. 해당 임시값의 scope만 줄인 runtime-r1에서 최초7개 그룹을 실행했다.

r1 fmt는 PASS, Clippy는 test-only 설치 guard의 nested if 한 곳을 지적하여 FAIL이었다. 이후 같은 short-circuit 순서의 let-chain으로 정리했다. 독립 검토는 검사 순서·성공/실패 회계·잠금/Drop 수명이 동일함을 확인했다. 최종 source `cb1dbccd476680bbaba89f922d6e039f17c6c6da7102958012c6f40a941e95b2`의 fmt·engine all-target/all-feature Clippy는 PASS이며 별도 receipt에 연결한다. 이전45개 runtime을 재실행하거나 최종 source의 새 PASS로 표시하지 않는다. GA08 이후의 명시 cleanup 보강도 API/lifetime 변경이 없어 원래 compiler 결과를 보존했다.

DEV0/1/2·전체 workspace/leaf·하위 wrapper와 문서 verifier는 실행하지 않았다. 문서 검증은 push gate이며 이 작업은 로컬 단계 commit이다. 원본 PASS/FAIL/NOT_RUN은 [실행 색인](dev3-concurrency-admission-20261022.json)의 해당 source·명령·로그·도구 identity와 함께 보존한다.

## 판정과 다음 소비

계약 D2와 core·시험·runner 입력의 독립 source 검토를 수용했다. 실제 runtime 증거의 독립 감사는 source manifest·ELF 원본/보존본·build artifact·checker/log의562개 hash 일치를 확인했다. compiler 증거의 별도 감사201개도 원본 identity를 유지한다. 최종 quality source의527개 hash도 일치했다. `/root/allocation_review`는 **D1 ACCEPT_SCOPED_UNIT / D2 F05 target-layout 조건 충족**으로 판정했다. 원본은 `artifacts/dev3/resume-20261022/review/guard-final-review-r0.md`(SHA `c424fee49c58e757f733473abd77212f8b232c617653150cfb5f7d097b801e79`)이며 최종 정적 검사와 함께 실행 색인에 연결한다.

이 단위의 성공은3.10.1.a 전체,3.10 동시성 전체 또는 DEV3 완료가 아니다. 다음 실제 소비는 같은 저장 owner의 dirty frame·content latch·typed WAL stage/apply 연결이다. 실제 page rank·snapshot/publication·transient pin·FIFO, dirty/flush·Commit·recovery·READY와 최종 DEV3 S5는 남아 있다.
