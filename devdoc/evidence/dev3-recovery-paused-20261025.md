# DEV3 plan4 복구 작업 중간 저장·자료 정리

2026-10-25 · **사용자 요청으로 중지 / WIP · 복구 인수·DEV3/S5 미완료**

사용자는 “정리한 뒤 이번 진행 종료”를 선택하고 disk full 대응의 정리 대상을 `artifacts`로 지정했다. 신규 개발·빌드·제품 시험을 중단하고 현재 소스와 미완료 검토 사항을 보존한다. 이 기록은 복구 기능의 최종 D1이나 실행 수용이 아니다. [source별 상태 색인](dev3-recovery-paused-20261025.json)을 따른다.

## 완료한 단위와 미완료 범위

- 직전 완료 commit `c2b1691a652c45c559975bfdf35ffdd9183c840d`는 [빈 거래 live·prefix](dev3-retention-live-20261025.md)의 원 source별 필수75 PASS다. 완료 시험을 반복하지 않았다.
- 현재 중간 저장은 engine 수정17·신규15 및 fixture 문서2개의 복구 owner·인계·검증 후보다. 최신 phase 분리와 consumed WAL hash 변경, oracle 초안은 **compile/runtime NOT_RUN**이다.
- 원 복구 static2는 두 사례 모두 **FAIL**, 실제 제품 효과0·**F05 HOLD**다. 이전 L/B10 source의 `cargo check` PASS는 해당 과거 source에만 적용하며 최신 후보로 승계하지 않는다.
- `plan4_recovery_tests_faults.rs`와 `plan4_retention_recovery_test_observation.rs`는 모듈에 연결하지 않은 draft다. 입력48개는 후보이며 S5 inventory는 동결하지 않았다. 제안 E09도 구현·등록 완료가 아니다.
- 독립 검토자는 미완료 checkpoint 보존을 허용했다. source 검토를 최종 복구 인수로 바꾸지 않는다. 원 FAIL·HOLD와 검토 기록을 보존한다.

## 재개 시 남은 사항

sealed prefix의 digest 불일치는 `ContentMismatch`, offset/LSN 불일치는 `OrderMismatch`로 분리하는 지적이 남아 있다. 실제 bind/history phase의 source census 재측정, L/B10의 ATT/Aging·credit·base/control bytes·effects·정확 cause oracle 보완과 독립 검토도 필요하다. 이후에만 exact inventory와 source/ELF-bound effect admission을 동결하고 관련 사례를 선택한다. 사용자의 명시적인 재개 전에는 이 목록을 실행 지시로 사용하지 않는다.

## artifacts 정리와 증거 보존

정리 기록은 [정리 디렉터리](../../artifacts/cleanup-20261025/)의 `cargo-cache-cleanup.json`, `build-products-cleanup.json`, `build-products-removed.jsonl`, `evidence-archive.json`에 둔다. Cargo 캐시와 재생성 가능한 ELF·컴파일 산출물을 삭제하며, 실행 파일의 경로·크기·SHA를 기록한다. 사용이 끝난 시험 DB·소스 사본·결과·실패 로그는 `used-evidence.tar.zst`로 압축하고 원본과 내용 비교가 성공한 뒤 풀린 사본을 제거한다. 보관 위치는 원 source/결과의 역사적 정체성을 바꾸지 않는다.

기존 문서의 “보존 ELF/원 경로”는 당시 실행 시점 기록이다. 이번 정리 이후 삭제한 ELF는 현존하지 않고, 압축 대상으로 옮긴 증거는 archive 안의 원 경로에서 찾아야 한다. 해당 파일을 복원하지 않고 기존 경로가 현재 검증 가능하다고 주장하지 않는다. 새 실행·PASS로 재분류하지도 않는다. 진행 중이던 DEV3 두 단위와 현재 DEV4 수명 자료는 원 경로를 유지하고 별도 COMM 정리 기록도 보존한다.

범위 정정 전 루트 `target` 캐시 8.08GiB를 이미 삭제했으며 사용자에게 알렸다. 그 이후 삭제·압축은 `artifacts` 안으로 제한했다. 제품 소스·fixture 원본·Git 이력과 다른 작업의 미커밋 변경은 공간 정리 대상으로 삼지 않았다. 정리 중 신규 제품 시험·빌드0, push0이며 문서 검사는 pre-push 단계에 남긴다.

압축 원본 내용 비교 PASS 후236개 과거 자료 root의 풀린 사본을 제거했다. 압축 파일 2.57GiB, 정리 후 전체 artifacts 6.57GiB다. 실제 삭제·관측 공간 값과 archive SHA는 위 JSON 색인에 기록한다.
