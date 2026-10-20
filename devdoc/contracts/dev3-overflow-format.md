# DEV3 row overflow 표현

2026-10-20 · **D3-OVERFLOW-F01 / 계약·production diff 독립 검토 수용 / 표현 국소 시험 완료 / 실제 chain I/O 미구현**

[page envelope](dev3-page-format.md) · [PageRef](dev3-reference-format.md) · [row 저장 계약](disk-storage-design.md#tree) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 계약은 2 KiB 초과·128 KiB 이하 canonical row bytes의 chunk 표현이다. row의 각 value 64 KiB 제한·값 타입·schema 검사는 상위 row codec의 책임이다. 실제 page 할당·pin·읽기·WAL·row 공개·undo/horizon 소유와 회수는 포함하지 않는다. 이미 공개한 chain의 payload와 연결은 불변으로 취급하고 교체는 새 chain을 만든다. 이 codec 자체가 동시성이나 불변성을 집행하지는 않는다. undo record의 별도 metadata를 이 row 형식에 암묵적으로 넣지 않는다.

## Head와 chunk bytes

정수는 little endian이다. row record가 보존할 `OverflowHead`는 정확히64 bytes다.

| offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 24 | root PageRef |
| 24 | 4 | total row bytes: 2,049..131,072 |
| 28 | 2 | chunk count = ceil(total / 16,092), 1..9 |
| 30 | 2 | reserved0 |
| 32 | 32 | 전체 canonical row bytes의 SHA-256 |

각 chunk는 공통 kind7·level0 page의 유일한 cell이다. cell은 아래128-byte header와 payload로 구성한다. 모든 chunk는 같은 DB/owner/owner generation, 같은 head를 가진다.

| cell offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 4 | `XOV1` |
| 4 | 2 | version1 |
| 6 | 2 | chunk index, 0..count−1 |
| 8 | 2 | head와 같은 count |
| 10 | 2 | reserved0 |
| 12 | 4 | head와 같은 total row bytes |
| 16 | 4 | payload bytes |
| 20 | 4 | reserved0 |
| 24 | 24 | head와 같은 root PageRef |
| 48 | 24 | 마지막 chunk는 전부0, 그 외는 유효 next PageRef |
| 72 | 32 | head와 같은 전체 row digest |
| 104 | 24 | reserved0 |
| 128 | payload bytes | row의 해당 연속 구간 |

page envelope의 cell 예산16,224에서 slot4와 chunk header128을 뺀 최대 payload는16,092 bytes다. 마지막 외 모든 payload는 정확히16,092 bytes이고 마지막은 `total−index×16,092`다. 빈 chunk·중간의 짧은 chunk·trailing bytes·다른 count/total/digest/root는 거절한다. 이 고정 분할은 같은 head의 여러 비정규 분할을 허용하지 않는다.

index0의 현재 PageRef는 root와 정확히 같아야 한다. 뒤 chunk는 root와 같은 물리 주소 `(FileId,page_no)`를 가질 수 없다. next도 현재 또는 root와 같은 물리 주소면 incarnation이 달라도 거절한다. 이 국소 검사는 chain 전체에서 다른 주소가 반복되는지까지 보장하지 않는다. 전체 검산기는 최대9개 방문 주소를 저장해 중복 주소와 순서·길이·전체 digest를 별도로 검사한다. allocator 소유와 실제 target 존재 검증은 page load/allocator의 별도 의무다.

## API·검증 경계

`storage::overflow`는 engine 내부다. 새 heap allocation·I/O·lock·quota 소유를 만들지 않는다. encoder는 caller buffer를 받으며 모든 오류에서 output 전체를 보존한다. cell encoder는 필요한 prefix만 쓰고 성공 시 tail을 유지한다. head/page encoder는 각각 정확히64/16,384-byte output을 요구한다.

`OverflowHead::new(root,total,digest)`는 total을 검사하고 count를 계산한다. `encode/decode`는 위64-byte 형식과 정확한 count를 확인한다. digest의 특정 값 자체는 금지하지 않으며 실제 전체 payload와 대조해야 한다.

`encode_overflow_cell(head,index,current,next,payload,output)`은 위치·link·길이를 먼저 검사한다. `encode_overflow_page(header,head,index,cell,output)`은 caller가 준비한 cell을 검산하고 기존 page encoder에 단일 cell로 전달한다. page-size scratch를 숨기지 않는다.

`decode_overflow_page(input,expected_identity,head,index)`는 기존 envelope decoder로 DB/PageRef/owner/kind를 검사한 뒤 cell과 head를 대조한다. 기대 head는 안정된 row/undo 소유자에게서, 기대 PageRef는 앞서 검증한 head/next에서 얻어야 한다. 저장된 header를 그대로 기대 identity로 사용하지 않는다. 반환 view는 header·index·next·payload를 대여하며 frame pin을 대신하지 않는다.

`OverflowChainVerifier`는 expected root identity와 head에서 시작한다. `push(view)`는 다음 정확한 PageRef·DB/owner·head·순서와 이미 방문한 물리 주소를 검사하고 streaming SHA-256을 갱신한다. 마지막 push에서 총 byte 수와 전체 digest를 확인한다. 오류는 검산기 상태를 보존하며 caller가 다른 올바른 입력으로 재시도할 수 있다. 마지막 뒤 추가 push는 InvalidInput이다. `finish(&self)`는 count개를 모두 검산한 경우만 성공하고 성공·실패 모두 검산기를 소비하지 않는다. 이 검산기는 bytes를 보관하지 않으므로 실제 materialization은 caller의 별도 예약된 출력에 수행한다.

오류는 `InvalidInput`, `UnsupportedFormat`, `Corrupt`, `HeadMismatch`, `OrderMismatch`, `Incomplete`, `Page(PageError)`로 구분한다. 잘못된 caller header/shape/output 크기는 InvalidInput, 저장된 형식/길이/링크는 Corrupt, 유효한 기대 head와의 차이는 HeadMismatch다. 오류의 I/O certainty나 fencing은 이 무효과 codec에서 결정하지 않는다.

## 실행 전 고정 시험

8개 test를 고정한다. 기대 header bytes와 full-page SHA는 독립 Python `struct/hashlib`로 계약에서 계산해 구현 전 기록한다. production encoder로 기대값을 만들지 않는다.

기본 literal은 DB=[11;16], FileId=0x0000000100000002, root page3/incarnation4, owner5/generation6, pageLSN7과 아래2,049-byte payload를 사용한다. 독립 계산 스크립트와 원본 bytes는 `artifacts/dev3/overflow-format/`에 보존했다.

- payload SHA-256: `22f0a97231b1c2f1c8e4390f637461035e336295817ba70929f902e11b5e9809`
- head: `020000000100000003000000000000000400000000000000010800000100000022f0a97231b1c2f1c8e4390f637461035e336295817ba70929f902e11b5e9809`
- page checksum: `1f97d1d77b1d98246096de2d96fc2675fe08524a5d377fae0534a26538c02a3b`
- full-page SHA-256: `cd8499269433bc243136e100edb13f7328d0563d2a74f1098f23ec1cfc4634d5`

| test | 분모·판정 |
| --- | --- |
| overflow_head_literal_and_limits | 독립64-byte literal, total2,049/16,092/16,093/131,072 정상4, total0/2,048/131,073/MAX 거절4, 모든64 잘린 길이·trailing1·count/reserved/FileId/incarnation 손상 |
| overflow_page_literal | 독립1-chunk page 전체 digest·cell header literal, payload2,049 bytes의 `(i×17+3) mod256`, 정확한 tail과 대조 |
| overflow_chain_fixed_boundaries | total2,049/16,092/16,093/32,184/32,185/131,072의6 chain, 각 chunk의 위치·next·payload·finish 검산, 총 chunk18 |
| overflow_rejects_checksummed_damage | 첫/중간/마지막 chunk 각각 magic/version/index/count/reserved10/total/payload length/reserved20/root/digest/reserved104/truncation12종=36; 각 page checksum을 다시 계산해도 거절. 마지막 chunk의 trailing byte와 빈/2-cell envelope2개는 별도 거절 |
| overflow_rejects_links_and_identity | 잘못된 first root, 뒤 root주소, next self/root의 같은/다른 incarnation, 마지막 nonzero next·중간 missing next; DB/file/page/incarnation/owner/generation/kind mismatch7종 |
| overflow_verifier_requires_complete_order_and_digest | incomplete·skip·duplicate·다른 owner/head·마지막 후 추가 push·유효 page checksum을 가진 payload 손상으로 전체 digest 불일치; 오류 뒤 올바른 입력으로 완료. 4-chunk root3→page4→page5→page4의 마지막 incarnation4/5 두 경우는 local/index/next 검사를 통과해도 방문 물리 주소 중복으로 거절 |
| overflow_streaming_profiles | seed1..16, total=2,049+(seed×7,919 mod129,024), payload byte=`(index×31+seed×7) mod256`; 입력 전체의 독립 SHA-256과 각 입력 slice를 검산, decoder 출력으로 기대 payload를 생성하지 않음 |
| overflow_encoders_preserve_outputs | 잘못된 total/index/current/next/payload/공간/header/마지막 cell 손상은 sentinel 전체 보존, head/cell/page 성공 크기와 cell tail 보존 |

단위 표현 검증은 실제 disk chain 손상 주입·DS03/04·WAL/overflow 소유·32 transaction 자원표·장시간 V3를 대신하지 않는다.
