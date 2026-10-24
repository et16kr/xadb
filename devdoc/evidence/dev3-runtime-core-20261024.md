# DEV3 원 WAL runtime/core 추출 선택 검증

2026-10-24 · **신규 필수7(static2·memory5) PASS, 최종 독립 D1 제한 수용**. [소유 계약](../contracts/dev3-storage-session.md#private-runtime-core)·[확정7개 입력](../plans/dev3-test-plan.md#private-runtime-core)·[source별 색인](dev3-runtime-core-20261024.json)을 따른다. G6 recovered owner의 선행 구조 추출이며 G6/첫 행/DEV3/S5 완료가 아니다. AI 소비자는1/3을 유지한다.

기존 자원을 private `WalRuntime`/`InstalledStorageCore`에 한 번씩 두고 원 Genesis/Fresh API의 명시적 전달과 DWB→Mutable 소비 이동을 연결했다. 역할·plan·fence·poison·reader claim 제한과 wire/상한은 유지한다. 기존16개 시험 지원/접근 파일은 기계적으로 적응했으며 완료 시험은 실행하지 않았다. 신규7개 exact 함수만 대상으로 한다. 진행 중인 G6 tag3/retention 의미 구현과 다른 DEV4 변경은 이번38개 owned source에서 제외했다.

## Source와 원 실패

모든 revision은 원 G5 `tests-r4/source`에 정확 owned overlay를 적용한 불변 보존본이다. 이전 source/ELF/원 실패는 덮어쓰지 않는다.

| revision | source SHA256 | 관측 |
| --- | --- | --- |
| product-r0 | `28933f5cc3a2f0d9e05d954cf2d5d99e0dceaf03e29fefd15ba3c2ebe6489fb7` | lib check E0609 두 필드 경로 FAIL; 제품 실행0 |
| product-r1 | `cac97f01e5fda3cd30e9cb765d5951dd27fc01ec84789b4493de8d3c1d1b0fd5` | 두 경로 수정 후 lib check PASS; 제품 실행0 |
| tests-r0 | `0cab71425a3d55b5750b87ede09317eb6e5632ae0f0d8673fc27f28c62711d0a` | 기존 scoped-thread 별칭의 private stage 접근 E0599 FAIL; 실행0 |
| tests-r1 | `a79efa9f5d544019becf85c4775aa7ff303136e9c89c22f5b84f63bd7f58d469` | 별칭 경로 수정 후 no-run build·fmt·Clippy PASS; 실행0 |
| tests-r2 | `28c59908571723d3d90e78b9837f3483480123151bbf8d75398f0711106cd9df` | 신규 static 측정값 출력을 assert 앞으로 이동; no-run build·변경 파일 fmt·Clippy PASS, static2 PASS |

r1 ELF는 `005ab7781f6c29bf230f3bef7361abe51819e889a14f759a04c200fcfec21bc7`이고 r2 ELF는 `7926acbd727812c73b0f7dacc2dee6282ab3016a18eaa1082aff7ed96ce547c0`(43,824,128bytes)다. 실제7개 입력 SHA는 `2913544daf59d6d6eac2d98cc5a93a533e7e07f2e9a5a6a791cade28d2562124`다. 단순 출력 순서 변경 전후에도 원 source/ELF를 구별한다.

## 정적 관측과 실행 경계

| 실제 타입 | runtime=Genesis | core=Fresh | Mutable | G5 source peak |
| --- | ---: | ---: | ---: | ---: |
| memory W8/File32 |376|560|848|47,908|
| native W16/File160 |512|696|984|49,068|

두 타입 모두 Empty 전환은65,424bytes, retained115,416bytes, 합180,840bytes이며64KiB/192KiB 한도를 유지한다. wrapper/inner의 실제 size/align, 원 소유자 크기, Result·인자 전달 phase, Fresh 생성/reader/role 상한과 G5 source census를 새 정적 ID로 검산했다. 이는 source-value 회계이며 machine stack/RSS 측정이나 native durability 실행이 아니다.

정적 원 결과 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/tests-r2/static-r0/result.json`)는 RC-ST01/02를 각각 한 번 실행한 PASS다. 최초 audit부터 final receipt까지 보수0.659초, 저장 뒤 실제 관측0.559033초이며 총10초 제한 안이다. 정적 수용 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/static-admission.json`) 뒤 메모리 원 결과 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/tests-r2/memory-r0/result.json`)의 RC-M01–05를 각각 한 번 실행하여5개 모두 PASS했다. 이동 중 효과0·원 backing/reader·fence·poison·선언된 busy fault를 확인했다. 전체 보수7.000초/실제 저장 뒤6.899573초로180초 안이며 완료 정적2는 반복하지 않았다.

## 준비와 보존

Source160MiB/ELF96MiB/공유 compile192MiB/records16MiB/native0의 준비 상한을 고정했다. 정적 전 회계 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/preparation-before-static.json`)는 고유 backing116,607,074/87,648,192/123,644,312/4,594,294/0bytes로 모두 한도 안이며 compile과 source/ELF의 inode 교집합은0이다. 같은 cache를 쓰는 compiler는 직렬화한다.

독립 검토 후 mutable G5 cache의 중복 실행 파일43,582,032bytes와 구형 operational-recovery incremental cache의 exact8개150,760,346bytes만 회수했다. 별도 원 ELF·source·raw·로그·영수증·진단 metadata는 유지했다. G5 보호67,963경로와 operational-recovery 보호43,268경로의 inode/bytes/SHA를 전후 대조했다. Read-only source hardlink의 nlink 증가를 원 bytes 변경과 구별한다. 원 준비 cutoff·실패·HOLD를 현재 cache 회계로 소급 변경하지 않는다.

초기 실행기/입력의 사전 audit 시간 누락은 제품 실행 전에 수정했고 원 v1을 보존했다. 현재 runner는 source/ELF/입력/fixture/tool binding과 이전 PASS를 검산하고 첫 audit·process·최종 기록을 전체 예산에 포함한다. Lower/workspace/native/compiler 캠페인·문서 verifier는 실행하지 않았다. 문서 검증은 pre-push gate이며 이번 요청으로 push하지 않는다.

## 최종 독립 검토

최종 D1 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/review/final-d1.md`)·기계 색인 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/review/final-d1.json`)은50개 소유 경로를 모두 검토하고 이 추출 단위를 `ACCEPTED_LIMITED_D1`으로 수용했다. 동시 G6/COMM 변경은 제외했다. generic reader의 소스 상한 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/review/generic-reader-source-proof.md`)은 고정 Rust/target에서 codec 실행과 반환 단계를 나눠 기존 예약량의 지배를 확인한 정적 근거이며, 제품 변경·추가 시험 실행은0이다. 실제 두 포트의 보수 static 식과 일반 타입의 단계별 증명을 구별한다.

최종 검토 중 일시적인 ENOSPC로 OCR 출력 저장이 실패했다. 빈 원 실패 파일과 별도 tmpfs 재수집 기록 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/review/ocr-enospc-recovery.json`)을 보존하며 원인을 추정하지 않는다. 저장 실패 뒤 대조 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/preservation-after-enospc.json`)는 소유 source38·기존 색인87·원 ELF2가 바뀌지 않았음을 확인했다. 별도 G6 미검증 source 복구는 이 커밋·검증 범위에 포함하지 않는다. 앞선 준비 cutoff를 이후 가용 공간의 보장으로 바꾸지 않는다.

최종 리뷰 후 준비 회계 (로컬 `../../artifacts/dev3/plan4-runtime-core-20261024/preparation-after-final-d1.json`)는 records4,715,523bytes를 포함한 다섯 범주가 원 상한 안임을 확인했다. 새 제품 시험을 실행한 결과가 아니다.
