# DEV3 고정 등록 코어·세대 pin 선택 검증

2026-10-23 · **제한 단위 D2·제품 D1 수용 / 일반 pool·DEV3/S5 미완료**.

[소유 계약](../contracts/dev3-task-05-buffer-flush.md#registration-core) · [실제 구현](../implementation/dev3-implementation.md#registration-core) · [실행 색인](dev3-registration-core-20261023.json)

기준 `4c1fa5b`의 clean worktree에서 기존 SharedPool가 즉시 소비하는 등록 lookup과 pin token을 변경했다. 계약·제품·통합 `/root`, 별도 읽기 검토 `/root/pool_design_review`다. 실제 기능은 fixed 주소 등록의 단일 조회·Copy 요청·세대 pin 검산이며 동적 load/eviction 권한은 공급하지 않는다.

## 실행 source와 범위

최종 r6 source SHA-256은 `0ab017577c3545b8fa25d01874057ad5f4721a979edbedd4a7bd81822df5d984`, 보존 ELF는 `e4bcc9ad2f4c859fc169d07154e7e0b8f4ce567c48cb636088754d52c24c4635`다. `artifacts/dev3/registration-core-20261023/source-r6`에 source export, 같은 폴더에 binary·빌드 receipt·선택 inventory·원 로그를 보존했다. 문서 변경은 source fingerprint 범위 밖이다. 새 source export가 기존 결과를 재실행한 것은 아니다.

| 증거 | 실제 선택·판정 | 범위 |
| --- | --- | --- |
| r6 신규 runtime | exact11 PASS | 역순 두 pin·Copy 요청, 전부 또는 없음, stale 두 번째 token의 count 보존, read/write/finalize 획득·관측과 invalid fixed 공개 거절. read 요청/bytes/observe는 각각 fresh pool의3개 variant |
| r6 INIT 직접 소비 | CF07-01·CF12-01 PASS | 실제 INIT 이미지·두-pin reader·공개와 concurrent reader 경계 |
| r6 allocator 직접 소비 | FN04·FN05·FR01·FR02 PASS | redirty·retired finalize 보존과 변경 자원 admission의 경계 |
| r6 Status 직접 소비 | BA09·BR02·BR03·BR05 PASS | Copy capture 권한 조합·flusher 선행·redirty·retired target 전환 |
| r6 static | CF17-01 PASS | reader3608≤3648·dirty193600≤196608 및 INIT concrete layout; 제품 I/O0 |
| r5 static | dirty_shared_resource_after_closed_init PASS | 변경 dirty carrier·타입·source ledger, shared hot165472/cold154944≤196608. 해당 코드의 이후 무변경 시험은 생략 |
| r6 native | profile1 DB1·worker1 PASS | actual reserve→Status INIT→allocator/Status 순차2batch·62 logical flush effects. 독립 full bytes/WAL/descriptor·FD/소유 검사 |
| r6 compiler | semantic2+control1 PASS | release positive·storage_borrow E0505/line10 negative·실제 baseline. 수명 compiler 증거이며 제품 runtime 아님 |
| r6 정적 품질 | engine all-target/all-feature Clippy·workspace fmt check·diff check PASS | Clippy는 test target을 정적으로 검사하며 시험을 실행하지 않음 |

새 runtime11과 관련 소비10을 합친21개 사례는 r6에서 성공했다. 정적 두 case의 source는 r5/r6로 나뉜다. r5 CF17-01은 당시 J3496 식이 산출한 관측이며 caller identity2개 누락을 해소한 최종 자원 증거가 아니다. r6에서 그 식과 READ_CALL을 수정한 후 관련 CF17-01만 다시 확인했다. r3의 별도 신규 layout receipt는 실행 당시 source hash를 보존하지 못했으므로 탐색 관측으로만 남기고 source 결합된 PASS 수에 합산하지 않는다. 최종 S5의 확정 inventory에서는 그 테스트도 별도 실행 대상이다.

새 unit timeout은 각30초였다. 기존 소비 runner는 동결된120초 한도, native runner는30초이며 계획의60초 외부 상한 안에 있다. native 실제2.4184초·DB22,128,448bytes, replacement peak bound22,132,544≤24MiB다. wrapper File peak2/final0, 경계 관측 FD peak4/final0(151관측)을 구별한다. 연속 OS peak·전역 disk 선확보·장치 power-loss 인수가 아니다. compiler3개 합계5.352초이며 다른 native fixture/build 경로와 격리했다. PF09 성능 수치로 사용하지 않는다.

## 원 실패·검토·남은 인수

compile-r0는 consuming install의 unit 반환과 Copy 요청의 남은 역참조(E0308/E0614), build-r2는 test helper visibility(E0603)로 실패했다. 원 로그를 보존하고 수정 후 compile/no-run을 완료했다. runtime FAIL을 숨기거나 성공할 때까지 반복한 결과는 없다. r5/r6 source·ELF를 구분하며 완료한 무변경 사례·하위 wrapper·전체 leaf·모델은 재실행하지 않았다.

독립 검토에서 full snapshot의 상주 비용을 compact token으로 줄였고, Status caller identity 수명·관측 경로의 구조 검산·read3개 entry의 독립 반례를 반영했다. r6 source/ELF·정확 선택·원 로그에 대해 D2 실행 gate와 최종 제품 D1 PASS를 받았다. 같은 모델 계열 간 통계적 독립성은 주장하지 않는다.

일반 mutable directory·consuming pool 전환·load/evict/reload·live Status·tree/undo/행 거래·일반 recovery/READY·최종 S5는 미완료다. 이전 복구·flush 결과는 원 source 범위의 증거로 유지한다. 이 단위에서 문서 verifier·push는 실행하지 않았다.
