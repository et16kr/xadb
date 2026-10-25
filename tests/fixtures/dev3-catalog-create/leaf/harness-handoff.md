# CREATE scope2 독립 leaf harness 전달

2026-10-25 · `/root/dev3_review` · **작성·정적 입력 대조 완료, coherent compile 및 제품 실행 전.** 제품 encoder/decoder·Cargo·test runner 실행0, frozen raw 재생성0. 새 Rust 파일에 한정한 rustfmt만 실행했다.

- 소유 source: `crates/engine/src/storage/semantic_catalog/leaf_tests.rs`, 42,626B, SHA256 `2b0f5a30e724f7385f90f426d0be1eb3bf99dff3f8f4c676441be8793f218d09`.
- 선택 정본: `harness-input-plan.json`, SHA256 `cc88e026feacafab5729371f4e02a899a8d999074b4f77b027b0cc40292d15b2`.
- parent의 `semantic_catalog.rs`에 `#[cfg(test)] mod leaf_tests;` 연결이 필요하다. 이 파일은 여기서 수정하지 않았다.

## 정확 선택 범위

libtest 예상 prefix는 `storage::semantic_catalog::leaf_tests::`다. 함수명108개를 plan에 전부 기록했고 새 source의 macro 선언108개와 정적으로 대조했다. 실제 ELF의 libtest inventory는 아직 확인하지 않았다.

| layer | 정상 | 거절 | 합계 |
| --- | ---: | ---: | ---: |
| core | C01..20 =20 | NC01..35 중 NC09 제외 =34 |54|
| projection | P01..06 =6 | NP01..12 =12 |18|
| wire layout | W01..17 =17 | NW01..19 =19 |36|
| 전체 |43|65|108|

각 case는 별도 test function이다. 실패 시 이미 완료한 case를 다시 실행할 필요 없이 exact 이름으로 남은 선택을 구성할 수 있다. 최초 prefix 선택의 `--test-threads=1 --nocapture`를 제안하며, raw/libtest 결과와 각 `D3_CREATE_LEAF BEGIN/PASS`를 함께 보존한다. marker만으로 libtest 실패를 PASS로 바꾸지 않는다.

NC09와 NS01..03은 stable semantic consumer 입력이므로 이번108개에 포함하지 않는다. 입력 수령 후 각각 `D3_CREATE_LEAF NOT_RUN ... stable-semantic-consumer-unconnected`를 기록한다. ignored 빈 함수나 fake semantic assertion을 만들지 않았다. 이4개 파일을 해시 확인하는 행위를 제품 의미 검증으로 세지 않는다.

## 입력·oracle 경계

repository의 기존 `CARGO_MANIFEST_DIR/../..` 고정 fixture 경로 관례를 사용한다. 원 r0/r1의 manifest, index, generator, input-plan 총8개 지문을 수령 검산하고, **실제 parse하는 index bytes**도 해당 고정 SHA와 대조한다. raw120개/1,296,921B의 길이·SHA·중복 key를 검산한다. 단일 프로세스는 immutable OnceLock corpus를 공유한다. 개별 read는262,144B 상계이며 raw 합계는 고정 수치 이상으로 늘어나지 않는다. source export에는 원 fixture subtree가 포함돼야 한다.

Core 정상의 operation/sequence/q/cursor/current/next와 before/after digest를 고정 기대값과 대조한다. expected scalar와 digest 상수는 independent manifest 및 원 independent raw의 고정 field에서 옮겼으며 제품 출력으로 만들지 않았다. negative의 원 계약은 REJECT이고 새 오류 우선순위를 임의 도입하지 않는다. 실제 Error는 각 결과에 기록한다.

Projection은 별도 Namespace/Object/value presence를 공급한다. P01/P06/NP11/NP12의 None을 Some(empty)로 바꾸지 않는다. 기대 digest는 XCP1 원 raw 전체 SHA다. Wire는 frozen section offset/length/count, owner/prior/header, 원 core bytes, descriptor sentinel과 borrowed slice를 대조한다. 이 sentinel frame을 semantic history, credit, grant 또는 producer 성공으로 부르지 않는다.

NW patch는 고정 원 frame에 한 번 치환하고 XWF header/full checksum만 재봉인한다. 그 전체 결과를 기존 `materialized_sha256`와 먼저 대조한 뒤 실제 layout decoder의 거절을 확인한다. NW19도 r1 원6bytes/정확offset17442를 소비한다.

Typed Profile2를 공급하기 위한 context는 control-format 표의 ID1/generation1/DB01..10/timeline1 및 채택된 base2MiB를 직접 byte로 구성한4096B다. 전체 SHA256은 `b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24`로 별도 고정했고 제품 Profile encoder를 호출하지 않는다. original Profile decoder 성공은 입력 context 표현이며 runtime authority가 아니다.

## 실행 전 입장

parent가 이 harness/oracle을 독립 검토하고 coherent source·ELF·실제108개 inventory·입력 지문을 결합한 뒤 실행한다. 제안 한도는 selected 전체60s, exact case10s, 단일 worker, 출력8MiB, 실행 로그·영수증64MiB다. runtime timeout/output/artifact 상계는 parent controller가 적용해야 하며 harness 자체가 강제한다고 주장하지 않는다. compile/export/cache는 parent 단위의 기존 별도 상계·정리 정책을 따른다.

원 r0/r1 raw/manifest/index/generator/input-plan SHA는 작성 후에도 유지됐다. 새 제품 source API 변경이나 원 fixture 수정이 필요하지 않았다. 현재 정본은 작성 완료를 뜻하며 제품 PASS는 아니다.
