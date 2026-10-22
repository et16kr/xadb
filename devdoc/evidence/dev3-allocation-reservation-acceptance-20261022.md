# DEV3 예약·취소 3.03.1 잔여 인수

2026-10-22 · D3-ALLOC-RESERVE-001 · 제한 단위 인수 수용 / 전체 DEV3 미완료

[소유 계약](../contracts/dev3-allocation-reservation.md) · [중첩 자원 검토](dev3-allocation-resource-review-20261022.md) · [현재 구현](../implementation/dev3-implementation.md#allocation-reservation-accepted) · [단계별 테스트 계획](../plans/dev3-test-plan.md#allocation-first)

사용자의 DEV3 재개·완료, 병렬 진행과 단계별 commit 요청으로 3.03.1의 미완료 인수를 수행했다. 총괄은 공통 코드·자원 계산·WAL4와 증거 통합, allocation_native는 실제 Linux adapter/runner·stack 관측, allocation_compiler는 compiler runner/probe, allocation_review는 별도 읽기 검토를 맡았다. 작업 경로와 최초 상태는 `artifacts/dev3/resume-20261022/coordination.json`에 보존한다. 다른 작업의 문서 변경은 이 단위에 포함하지 않는다.

## 실행 범위와 결과

각 결과는 아래 원래 source에서 실제 선택한 사례에만 적용한다. 현재 전체 소스의 재검증 PASS로 합성하지 않는다. [원래 runtime8 및 후속 선택 결과](dev3-allocation-reservation-partial-xadb-20261021.json)와 [원래 검토](dev3-allocation-reservation-partial-review-xadb-20261021.json)는 수정하지 않았다.

| 범위 | 실제 결과·판정 |
| --- | --- |
| 수정된 WAL stage helper4 | 정확4개 PASS. 논리/물리 prefix, 경계 무효과, gate/slot 보호, 선택 mapping을 검증. 다른 WAL/하위 시험 생략 |
| native2그룹/13DB | 실제 Linux writer로 profile1/2/3의1→8→63 예약/취소3DB, profile1 N8의 reserve/cancel×write-before/half/after·sync-before/after10DB PASS. 고정 frame/page/descriptor/item bytes와 실제 WAL prefix·불변 파일을 검산 |
| compiler baseline+6 | 정상 baseline과 release 성공. installed E0382, frame/operation/flush/slot E0499를 고정 primary span과 대조. 전체8.07초, 누적30초 한도 내 PASS |
| 새 checker | compiler8, native3(부정변형21), stack 최초3와 후속 cleanup1·signal1을 각 변경 시 선택 PASS. checker 검증을 제품 동작 PASS와 구분 |
| resource layout/합계 | r0 layout 및 r2 aggregate guard PASS. 실제 타입 합 constructor13484·operation18248bytes, source-owned64KiB guard와 generic padding 경계를 검증 |
| 변경 producer/validator | r3 AR02·AR04 PASS. 후속 header/descriptor 분리의 AR04만 r5 PASS(고정 의미 반례541개 및 추가 검산). preflight 추출의 AR03만 r6 PASS |
| 실제 bounded stack | r6 정확1개 PASS. pthread stack65536·guard4096에서 constructor→reserve63→reserved_page62→cancel63→observe/drop 및 handle/writer 반납 확인. 실제 stack은 외부 read-only GDB observer로 확인 |
| 정적 검사 | r6 관련 Rust fmt 및 engine 모든 target/feature Clippy PASS. 기능 시험을 실행한 것으로 세지 않음 |

native 실행은15.33초, 최대 child1.267초, DB 최대22225152bytes·합288063808bytes였다. child30초·개별24MiB·합312MiB 예산 이내다. native syscall 장애 주입 결과이며 전원 손실·일반 recovery 성공을 뜻하지 않는다. Linux 플랫폼 crate는 native 시험용 로컬 dev-dependency만 추가했으며 production dependency와 외부 package 버전은 유지한다.

## 실패와 변경의 연결

resource-r0는 VMA를 stack 크기로 잘못 간주한 assertion에서 제품 constructor 전에 실패했다. 별도 std-only 진단은 실제 pthread64KiB와 더 큰 VMA가 양립함을 확인했다. 실제 pthread stack을 관측한 observer-r0와 resource-r3/r4/r5는 SHA가 있는 중첩 경로에서 SIGSEGV로 실패했다. r3의 별도 단일 signal 진단도 FAIL이다. 원래 결과를 수정하거나 성공으로 소급 집계하지 않았다. resource-r1은 source 사본만 존재하며 NOT_RUN이다.

제품은 preflight·producer·header·reserved/finished 검사를 함수 경계로 나눠 큰 임시값이 동시에 살아 있는 시간을 줄였다. 검사 순서·오류 분류·WAL bytes·효과 인계 순서와 unwind fence는 유지했다. helper는 큰 Copy 인자를 borrow한다. caller 시험은 설치 owner를 thread 밖에서 Box로 준비하여 std closure의 이동 복사 비용을 줄였다. 예산64KiB는 높이지 않았다.

r6 최종 제품 후보 source는 `0f503243faa7cf120088bf2f1384040e325e55601491d6b2898e411d109379a6`, ELF SHA는 `a5bfa0b18bcd690e265765edb15d6186ae725a5177ba3af09204a7c0639996d5`다. 이전 native/compiler/WAL4와 이미 완료된 runtime 결과는 원래 source identity를 유지한다. 후속 diff의 직접 검증만 추가했으며 DEV0/1/2·전체 workspace/leaf wrapper와 문서 verifier는 실행하지 않았다. 문서 자동 검증은 push gate다.

## 독립 판정

`/root/allocation_review`는 최종 source·ELF·로그1005개 identity 대조(불일치0), 단계별 source diff, 원래 native/compiler/WAL4·runtime 결과와 변경 경로 검증을 읽고 **D1 ACCEPT_SCOPED_UNIT / D2 resource contract ACCEPT**로 판정했다. 실제 pthread64KiB 성공은 해당 Linux x86_64 debug/test 빌드와 concrete memory port의 정상 경로에 한정한다. generic adapter 내부, 모든 fault 경로, 다른 target/release stack이나 정확한 high-water를 측정한 주장으로 확대하지 않는다. 검토자는 제품·compiler·debugger·문서 verifier를 실행하지 않았다.

원본 검토는 `artifacts/dev3/resume-20261022/review/allocation-final-review-r0.md`이고 실행 색인에 SHA를 연결한다. 뒤의 index-only 보완 검토는 최종 파생 색인의 원본 참조 일치만 확인한다. 코드·원래 실행 상태는 바꾸지 않는다.

## 증거와 남은 범위

원시 source 사본·ELF·명령·고정 입력·로그·도구 및 결과 해시는 `artifacts/dev3/resume-20261022/`의 wal4, allocation-native-attempt1, allocation-compiler-result.json, allocation-borrows-l_9dgtov, resource-r0/r2/r3/r4/r5/r6, stack-diagnostic, review에 보존한다. [실행 색인](dev3-allocation-reservation-acceptance-20261022.json)은 각 원본 경로·source·SHA와 PASS/FAIL/NOT_RUN을 연결한다. 독립 검토는 실제 변경·예산·오류 순서·증거 범위를 읽고 판정하며 새 제품 시험을 실행하지 않는다.

이 단위는 선택된 최초 allocator page의 예약·취소까지다. payload INIT, 운영 dirty buffer·home/DWB flush, extent growth/free/reuse, 일반 recovery·READY·transaction ACK와 전체 DEV3 최종 S5 인수는 미완료다. 최종 S5에서는 사용자 예외 지시에 따라 확정된 DEV3 전체 inventory를 실행한다.
