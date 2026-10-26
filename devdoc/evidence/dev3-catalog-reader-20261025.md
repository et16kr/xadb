# XADB 원 pool의 typed Catalog 읽기

2026-10-25 · **D2 수용·신규 선택 검증 완료 / 최종 독립 D1 수용 / 실제 비어 있지 않은 Catalog·첫 행·DEV3/S5 미완료**

[소유 계약](../contracts/dev3-retention-live.md#typed-catalog-read) · [source별 기계 기록](dev3-catalog-reader-20261025.json)

**D3-PAGE-LIFETIME-CATALOG-001**은 기존 authoritative owner와 SharedPool의 allocator0·anchor7·leaf8 전체3pin을 빌려 typed physical Catalog record를 제공한다. `RetentionReader::read_catalog`가 반환한 guard 안에서만 `lookup(ObjectId)`의 borrowed record를 사용할 수 있다. stored retired/uncommitted record도 읽으며 가시성·정의 anchor 존재·CREATE 성공은 후속 거래 소비자가 판단한다. 값 전체를 검증하므로 검색 경로 밖의 손상도 원 CatalogError와 공통 fence로 처리한다. 별도 pool·heap·I/O·WAL·page copy는 추가하지 않았다.

작업은 `/root/page_lifetime`, D2와 통합은 `/root`, 최종 독립 D1은 `/root/dev3_review`다. 기존 `lookup_catalog_empty`의 빈 Catalog 판정은 변경하지 않았다. 원 pool에서 수행한 시험은 실제 생성된 빈 Catalog를 소비하며, 비어 있지 않은/retired/uncommitted/양수 LSN의 입력은 독립 literal 순수 page다. 이를 실제 nonempty resident mutation이나 복구 성공으로 확대하지 않는다.

| 선택 검증 | 실제 결과 |
| --- | --- |
| 신규 제품 | 고유5 PASS: 전체3pin·해제·foreign core·공통 fence·nonempty physical version/부재·검색 밖 손상·typed 자원 |
| 빌림 수명 | baseline1와 positive/negative6 PASS. record가 살아 있는 guard release/drop은 정확 E0505, guard가 살아 있는 Owner 재대여는 E0499와 원 primary span을 대조 |
| 새 도구 | probe 생성·marker·해제 위치의 고유3 unittest PASS |
| 정적 | r1 source의 engine lib/tests all-features Clippy 및 소유 파일 rustfmt PASS |
| 반복/미실행 | 완료 제품 재실행0. native owner·nonempty dirty flush/eviction·Row/Undo INIT·일반 복구·S5는 NOT_RUN |

원 source fingerprint는 `eb3ac3507c8a1d4231a9f7c6a50b8b3709bee5b3b6c53304a957aa14088ba01d`, source manifest 파일의 SHA-256은 `c886e231560ecaf5f938258755a28900d0bfb847822749baf1c401bd0c34d442`, ELF는 `fe82d8f4c92116ef3d1874da70b679fb234aaa4fcfbca628fde7293558c8665b`다. 최초 r0 compile은 qualified const generic 두 인자의 braces 누락으로 실패했고 제품 시험은 실행되지 않았다. 실패 원로그를 보존한다. 빌림 checker는 r1과 동일한 표준 source fingerprint를 기준으로 각 probe patch·로그의 지문을 보존한다. checker 파일 자체의 hash는 별도로 결합한다.

typed source-value 합10,216byte는 고정 예약16,384byte 안에 있고, 기존 Catalog nested28,676(memory)/29,700(native)보다 작다. 현 source의 peak63,168/63,304·metadata123,072/123,208·whole188,608/188,744는 원 상한 안이다. machine stack/RSS나 일반 다중 스레드 F05 인수로 표시하지 않는다.

원 증거는 `artifacts/dev3/catalog-reader-20261025/`에 있다. r0/r1 source 각각4,834개 member의 hash를 검산한 뒤 gzip으로 보존하고 expanded290,914,451byte를 제거했다. compiler의 disposable source/target166,111,433byte와 이 담당 전용 target113,837,601byte도 정리했다. 원 ELF·실패·raw log·probe patch는 보존했고 다른 작업 cache는 건드리지 않았다. 최종 산출물은 약67MiB다.

다음 구현은 실제 Catalog CREATE/CLR와 Row/Undo INIT, 원 등록·load·history·flush·복구의 canonical 공급이다. 본 읽기 API가 그 권한을 만들지는 않는다.

독립 D1은 제품·수명 코드, 제품5/컴파일7/도구3 원 로그·ELF·소유 파일 hash 및 archive4,834개 항목을 대조하여 제품 차단 결함 없음으로 보고했다. 최초 receipt의 source manifest SHA를 source fingerprint로 부른 라벨 오류를 지적했고, 원 receipt-r1.json을 보존한 뒤 두 알고리즘을 분리했다. 이 증거 보완에는 제품 시험 재실행이 필요하지 않았다.
