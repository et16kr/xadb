# DEV3 Catalog CREATE 순수 전이 검증

2026-10-25 · **순수 바이트 전이 선택 검증 완료 / 실제 CREATE·복구 연결·F05·S5 미완료**

[실제 CREATE 계약](../contracts/dev3-catalog-create.md) · [source별 기록](dev3-catalog-transition-20261025.json) · [AI 개발 방법](../policies/ai-development-method.md)

Root512/Job1024 v2, 원 Tx/Aging descriptor의 전이·잔여 credit, Catalog Missing Undo와 page after, scope2 native/redo를 안정된 before에서 계산한다. 생산자와 관측자는 같은 순수 의미 검산을 사용한다. Forward의 borrowed schema는 엄격히 해석한 XCT1 정의만 빌리며 header·identity·actor·Undo·물리 pair는 원 before 문맥에서 다시 만든다. 이 모듈의 결과는 WAL stage, 등록, 설치 또는 retirement 권한이 아니다.

독립 D1에서 TerminalFinish의 실제 terminal outcome/q/Status 관계와 RollbackStart의 실제 Live Missing Undo 검산이 보완됐다. 보완 후 독립 읽기를 수용했고, borrowed schema의 독립 검토도 후보 header를 권한으로 사용하지 않는 범위에서 수용했다. 원 검토 메모는 artifact의 reviews에 보존한다.

## 실행 범위와 결과

- r3 source `3c71915f035789cbdd5523af266d9fe8dd4e337fc0a363218ed947e00ef32bdb`, ELF `003c111b080da955d0f35e2c114329fc0866ba5eb13be577a1afd4f2e17d8a50`: 새 core/projection/layout 108개 PASS.
- r5 source `eab8a3869fbd80d0a43adfd43ae027c6af3d7f7bffb6a03cc172b41b9bb62542`, ELF `19347986c5c400a8764bb4e77cbc460ab2a682a6410e6ad673701da0f5e94da8`: 전이 첫24개 PASS, R04 시험 어댑터 오류로 중단, 나머지47개 NOT_RUN.
- r6 source `6ddd0ba2dade756c15b022eea770622b7d3b5cacc2faa4bc7e2986620dfbb311`, ELF `d5e324c5144e53dbb6136fb40dcf35838d4b6b7a4fac912b94e741ea6337eab3`: 실패한 R04와 미실행47개, 총48개 PASS. 앞선24개는 반복하지 않았다.

R04는 변경된 Job ordinal `-1`을 API의 terminal ordinal로 읽어 제품 호출 전에 실패했다. 원 manifest의 명시 terminal을 읽도록 해당10개 adapter만 수정했다. 원 raw324개·8,857,480B, generator, manifest, 기대 페이지·frame과 제품 코드는 바꾸지 않았다. 원 FAIL과 수정 전 harness를 보존한다. 결합 결과는 전이72개(positive46/negative26)의 서로 다른 source별 결과이며, 단일 source 전수 결과로 표시하지 않는다.

입력은 독립 고정 정수·offset serializer에서1회 생성하고 별도 입력 auditor로1회 검산했다. Positive는 실제 produce의 전체 after8page·frame을 원 golden과 비교하고, observe는 product 출력 대신 원 golden을 소비한다. negative는 before identity/credit/terminal/Undo 관계와 frame 변경을 거절하는지 본다. 사례10초·선택 전체120초·출력8MiB·증거64MiB 상한과 exact selector를 중앙 제어기에 고정했다. 별도108은 사례10초·전체60초였다.

## 보존과 남은 작업

artifact `artifacts/dev3/catalog-create-20261025`에 immutable export의 파일별 SHA, 실제 ELF, build 원문, exact selector별 stdout/stderr·판정, 원 실패를 보존한다. r0/r1의 형식·borrow 컴파일 오류, r2의 기존64KiB cap 오류와29항목 compiler-only 크기 관측도 남긴다. r4는 새 fixture 폴더 복사 단계의 제어기 오류이며 컴파일·제품 실행0이다. 통과한 export는 다른 진행 중 B2 코드도 포함하므로 이 commit 전체의 runtime 검증으로 확대하지 않는다.

사용이 끝난 실패 export는 모든 파일 내용·SHA를 확인한 압축본으로 보존한 뒤 펼친 사본을 지웠다. 재사용 중인 전용 target과 실행 ELF는 유지한다. 원64KiB/192KiB/256KiB 자원 상한은 변경하지 않았다.

다음 단계는 원 owner의 CREATE admission·stage·whole install, installed Q·snapshot·참조 pin, 현재 v3 base/replay와 loser cleanup, 정확 source/metadata/loan census 및 해당 compiler/native/동시성 시험이다. DEV3 전체 완료와 최종 S5는 아직 주장하지 않는다. 완료된 하위 시험·workspace wrapper는 실행하지 않았다.

커밋에 포함할 경로만 별도 index tree로 export하여 컴파일했다. 첫 export는 Profile2 의존 경로 누락으로 실패했고 `core-index-r0`에 보존했다. 의존 경로를 포함한 `core-index-r1` source `5fddd5a3b06b69a4890ae88d44f43c2f4e829711b889b7cd65334907d620df29` 컴파일은 PASS다. 실제 소비자 연결 전 typed reexport의 unused 경고는 남아 있으며 zero-warning gate 통과로 표시하지 않는다. 완료한 시험은 이 확인에서 반복하지 않았다.
