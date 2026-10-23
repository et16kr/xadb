# DEV3 현재 membership의 Status home load

2026-10-23 · **구현·선택 검증 완료, 제한 단위 최종 독립 D1 수용**. [P3 소유 계약](../contracts/dev3-task-05-buffer-flush.md#operational-status-p3)과 [source·receipt 색인](dev3-status-load-20261023.json)을 따른다. 기준 DEV3 commit은 `1962fd9`이며 별도 COMM commit `57077b3` 위에서 계속 작업했다. 일반 pool·live Status 변경·READY·전체 DEV3/S5 완료가 아니다.

## 실제 공급

기존 `OperationalStatus::load_status(PageRef)`가 현재 RAM allocator와 descriptor/terminal/high-water/WAL prefix를 검증하고 원 FILEMAP의 실제 home을 읽는다. allocator가 dirty여도 유효한 DPT/requirement를 보존한다. 현재 Status는 clean·unpinned이어야 하며, 읽은 페이지의 identity·immutable Free body·nonzero LSN≤synced prefix를 검산한 뒤 두 번째 frame의 generation과 publication을 교체한다. 같은 현재 주소는 I/O 없이 거절한다. empty slot에서도 준비 세대+1을 발급한다.

실제 INIT A→flush A/allocator→B cancel/INIT→B만 flush한 상태에서 A→B→A를 읽는다. RAM allocator는LSN6, allocator home은LSN2인 동안 현재 membership이 권한이다. 원 allocator bytes/stamp/DPT/requirement·descriptor·WAL과 home 파일들은 load로 바뀌지 않는다. 후속 allocator flush→prepare→다음 주소 INIT도 연결한다. live reader는 recovery의 exact INIT WAL census를 재구성하지 않으며 일반 N-frame eviction/pressure를 공급했다고 하지 않는다.

## source별 선택과 실패 보존

원본 경로는 `artifacts/dev3/status-load-20261023`이다. 각 source export·ELF·build JSONL·정확 selector·stdout/stderr·hash를 별도로 보존한다. 제품 코드가 변하지 않은 기존 INIT/flush/recovery와 하위 wrapper는 재실행하지 않았다. `before()`·membership helper는 visibility만 확장했으며 기존 reader/allocator의 알고리즘은 바꾸지 않았다.

- r0: compile FAIL5(type path, private accessor, read return, test lifetime). 원 진단을 보존하고 수정했다.
- r1/r2: compile PASS. r2 static2의48208/48304는 원 owner를 빠뜨린 부분식 관측이다. 독립 검토 P3-LOAD-R01로 실행 gate를 거절했으며 현재 한도 충족 증거로 사용하지 않는다.
- r3: S5 profile 인수 u8/u32 compile FAIL1. r4에서 수정했다.
- r4: compile 및 수정된 resource static2 PASS. 새 실제 owner와 전체 metadata phase를 포함했다.
- r5: compile, padded owner static1, 새 native checker3·변경 compiler inventory1 PASS. runtime27은20 PASS/7 FAIL 원본을 보존한다. Clippy unused import1 FAIL도 보존한다.
- r6: native provenance 검산과 compiler 비공개 타입 probe를 보완했다. 원 static/제품 runtime은 반복하지 않았다.
- r7: compile PASS, 추가 checker2·변경 compiler inventory1 PASS. 수정3·신규1 runtime4 PASS, native1은 실제3load·전체15파일 대조·FDpeak5/final0 PASS, compiler3+baseline PASS. fmt·engine all-target/all-feature Clippy PASS. 최종 독립 D1은 이번 제한 load 단위로 수용됐다.

r5의 RawSnapshot에는 allocator뿐 아니라 global payload publication도 포함된다. A/B 교체 뒤 이를 그대로 기대한 단언을 실제 allocator 필드와 구별해 수정했다. 또 port의 `open_existing_with_metadata`가 open 한 번으로 처리하는 metadata를 별도 I/O로 잘못 세어14개 ordinal을 작성했다. 실제 source는4open+6read의10개다. 존재하지 않는10..13의4 FAIL을 NOT_APPLICABLE로 철회하고 원 FAIL을 보존한다. 같은 오류의 S5 16개는 NEVER_RUN이며 PASS가 아니다. 실제10tuple의 독립 작성 schedule oracle를 새로 추가했다. 최종 중간 inventory는 runtime24, 확대 S5는43이다. 이 정정은 품질 기준이나 실제 fault 지점을 줄이는 것이 아니다.

## 자원·독립 리뷰

load component는 원 MutableStorageOwner·bound·reader2·File4·snapshot2·permit2·result2·descriptor1536·carrier4096·sequential helper16384·registry3072를 포함한다. memory49056/native49176≤65536이다. 기존 retained60672+P+C를 다시 더하는 보수 식은 dirty110920/111040≤196608, flush65536을 포함해176456/176576≤262144다. 기존 bound를 일부 중복 과금한다. padded owner는69056으로 admission을 거절한다. caller operation114688·두 resident page·기존 flush workspace의 크기는 바꾸지 않았다. source-value 상계이며 OS stack/RSS나 일반 OOM 인증이 아니다.

`/root/pool_design_review`가 `open-code-review-delegate`의 deterministic file/rule 선정과 독립 의미 검토를 사용했다. 설계 D2·r2 거절·r5 unit gate 및 후속 source별 gate를 각각 보존한다. 최종 D1은 staged43개(제품·시험30+문서13)를 수용했다. OCR reviewable27개 전부와 제외된 관련16개를 수동 검토했고, 별도 DEV2 파일1개와 공유3개 문서의 unstaged DEV2 hunk는 제외했다. 제품·시험30개는 r7과 byte 일치한다. 원문 `review/final-d1.md`·`review/final-d1.json`과 파일별 coverage/hash는 source·receipt 색인에 보존한다. private storage permit compiler probe는 외부 타입 경계 E0603을 검증한다. 처음 제안한 field-level E0451을 실제 실행 증거라고 하지 않는다.

[S5 준비 inventory](../../tests/fixtures/dev3-status-load/manifest.json)의 profile2/3 실제 load2, 32 A/B round1, 2profile×10I/O×2error/panic40은 작성·NOT_RUN이다. 일반 replacement/pressure·live Status 변경과 전체 DEV3/S5는 남아 있다. 문서 검사는 push 전용이므로 실행하지 않았으며 push도 하지 않는다. 별도 COMM/DEV2 변경은 이 commit에 포함하지 않는다.
