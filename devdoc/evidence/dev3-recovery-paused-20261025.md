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

정리 기록은 로컬 `artifacts/cleanup-20261025/`의 `cargo-cache-cleanup.json`, `build-products-cleanup.json`, `build-products-removed.jsonl`, `evidence-archive.json`에 둔다. Cargo 캐시와 재생성 가능한 ELF·컴파일 산출물을 삭제하며, 실행 파일의 경로·크기·SHA를 기록한다. 사용이 끝난 시험 DB·소스 사본·결과·실패 로그는 `used-evidence.tar.zst`로 압축하고 원본과 내용 비교가 성공한 뒤 풀린 사본을 제거한다. 보관 위치는 원 source/결과의 역사적 정체성을 바꾸지 않는다.

기존 문서의 “보존 ELF/원 경로”는 당시 실행 시점 기록이다. 이번 정리 이후 삭제한 ELF는 현존하지 않고, 압축 대상으로 옮긴 증거는 archive 안의 원 경로에서 찾아야 한다. 해당 파일을 복원하지 않고 기존 경로가 현재 검증 가능하다고 주장하지 않는다. 새 실행·PASS로 재분류하지도 않는다. 진행 중이던 DEV3 두 단위와 현재 DEV4 수명 자료는 원 경로를 유지하고 별도 COMM 정리 기록도 보존한다.

범위 정정 전 루트 `target` 캐시 8.08GiB를 이미 삭제했으며 사용자에게 알렸다. 그 이후 삭제·압축은 `artifacts` 안으로 제한했다. 제품 소스·fixture 원본·Git 이력과 다른 작업의 미커밋 변경은 공간 정리 대상으로 삼지 않았다. 정리 중 신규 제품 시험·빌드0, push0이며 문서 검사는 pre-push 단계에 남긴다.

압축 원본 내용 비교 PASS 후236개 과거 자료 root의 풀린 사본을 제거했다. 압축 파일 2.57GiB, 정리 후 전체 artifacts 6.57GiB다. 실제 삭제·관측 공간 값과 archive SHA는 위 JSON 색인에 기록한다.

## 문서 검사·push 요청의 정적 검사 보완

이후 사용자가 문서 검사와 push를 요청했다. 첫 전송 후보 `27ae7fe0b209a510b90815e37a18c75bf1868dab`의 pre-push는 fmt를 통과했지만, 위 중간 저장 코드의 미사용 import·중복 literal 모듈·큰 오류 소유자·인자 수·중첩 조건문에 대한 Clippy 경고로 전송을 차단했다. 원 출력은 `artifacts/push-20261025/push-qt6Jos.log`에 보존한다. 이 시도는 문서 검사에 도달하지 않았다.

push에 필요한 보완은8개 Rust 파일의 import와 시험 전용 re-export 정리, 동일한 단락 평가·오류 반환을 유지하는 조건문3곳의 let-chain 서식, 개별 항목에 이유를 명시한 lint 허용이다. 큰 오류는 원 writer lease·caller loan·inline owner를 그대로 반환하는 계약 때문에 boxing하지 않았다. literal 파일은 서로 다른 부모의 `Oracle` 타입에 결합하므로 각 모듈 선언에만 중복 허용을 명시했다. `observe_home`의 별도 image/home 대여와 match/repair mask도 기존 인자로 유지했다. 포괄적인 workspace lint 완화는 없다.

검증 선택은 이 diff의 정적 성격에 한정한다. 기능·알고리즘·평가 순서·할당·시험 기대값은 변경하지 않아 제품 시험을 추가 실행하지 않으며, 전송할 commit의 fmt·전체 target/feature Clippy·문서 검사는 기존 pre-push 훅에서 수행한다. 이후 실제 gate 출력은 `artifacts/push-20261025/`에 commit별로 남긴다. 이번 정적 검사와 push를 복구 개발 재개나 기능 인수로 해석하지 않는다. 원 static2 FAIL·F05 HOLD와 최신 runtime NOT_RUN, DEV3/S5 미완료는 유지한다.

별도 검토자 `/root/push_lint_review`는 `27ae7fe` 위 Rust8파일 diff(SHA-256 `817bcc6288c79e8e4049feb01c2ecf78e45dc73beac4f99a31db906877a76e8c`)를 읽기 전용으로 확인해 차단 지적 없이 제한 D1을 수용했다. 조건값의 Copy·동일 평가/반환 순서, 실제 자원 검사를 남기는 re-export 정리, 항목별 lint 이유와 oracle 계산·assertion 불변을 확인했다. 새 기능 시험이나 완료 시험 반복은 요구하지 않았으며 `/root`가 이 제한 보완을 반영했다. 최종 정적 gate 통과 여부는 실제 push 출력으로 별도 확인한다.

두 번째 후보 `5d2df62a3aa56ed18e2c33cce7a109f6b9db1fef`는 fmt PASS 뒤 안쪽 `retention_replay_cells` 재수출1개의 unused import로 Clippy FAIL했다(`push-Qlvrx5.log`). 소비자 없는 재수출만 추가 제거했고 함수 구현은 유지했다. 같은 검토자가 이 후속 diff(SHA-256 `50e570de44d44fdac31babc493534944c8377052536c54bab1ff6f15b6cf77d0`)를 제한 D1으로 수용했으며 기능 시험은 추가하지 않았다.

같은 commit의 별도 임시 export에서 실행한 문서 검사도 FAIL했다(`documents-5d2df62a.log`). Git에 포함되지 않는 `artifacts` 대상 링크는 표시 문구와 원 경로를 보존한 로컬 증거 표기로 바꾸고, 이 문서의 상태 JSON을 evidence README에 추가했다. 원자료·기록된 결과·계약 의미와 검사기 기준은 바꾸지 않았다. 두 로그 모두 위 `artifacts/push-20261025/`에 보존하며 최종 후보의 검증은 기존 훅에 남긴다.
