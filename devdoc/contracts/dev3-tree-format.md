# DEV3 공통 tree page 표현

2026-10-20 · **D3-TREE-F01 / 계약·production diff 독립 검토 수용 / 표현 국소 시험 완료 / 실제 tree·I/O 미완료**

[Page envelope](dev3-page-format.md) · [Key](dev3-key-format.md) · [PageRef·anchor](dev3-reference-format.md) · [소유·착수](../implementation/dev3-implementation.md#wave-2)

row/index/catalog의 leaf와 internal cell을 기존16 KiB page envelope에 넣는 무효과 표현 leaf다. file/buffer/WAL/transaction, tree insert/delete·root SMO·잠금·cursor/가시성·subtree key-range 검증은 수행하지 않는다. leaf value는 opaque bytes이며 row/version/catalog record 의미는 상위 decoder가 검증한다. 기존 DEV2·page/key/reference bytes는 바꾸지 않으며 새 development fixture만 사용한다.

## 공통 metadata와 cell bytes

PageKind1/2·3/4·5/6만 허용한다. leaf level0, internal level1–15다. slot0은 정확히32-byte metadata이고 이후 slot1..N은 논리 key 오름차순 entry다. 같은 key를 두 번 넣을 수 없다. leaf N=0을 허용하고 internal은 N≥1과 N+1 child가 필요하다. 정수 length/version·PageRef는 little endian, row/catalog 정렬 key와 physical key의 RowId는 big endian이다.

| metadata offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 4 | `XBT1` |
| 4 | 2 | version1; 다른 version은 UnsupportedFormat |
| 6 | 1 | key arity: row/catalog0, index1–4. caller의 owner schema arity와 같아야 함 |
| 7 | 1 | reserved0 |
| 8 | 24 | internal leftmost PageRef. leaf는 전부0 |

| leaf cell offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 2 | key length K |
| 2 | 2 | value length V |
| 4 | K | canonical key |
| 4+K | V | opaque value, V=0 허용. cell 길이는 정확히4+K+V |

| internal cell offset | bytes | 조건 |
| --- | --- | --- |
| 0 | 2 | key length K |
| 2 | 2 | reserved0 |
| 4 | K | canonical separator key |
| 4+K | 24 | right child PageRef. cell 길이는 정확히4+K+24 |

row/catalog key는 owner 범위의 nonzero u64를 정확히8-byte BE로 표현한다. 두 kind의 key를 서로 다른 owner 사이에서 비교할 capability는 아니다. index key는 기존 PhysicalKey 전체이며 metadata arity로 검증한다. K≤1,032이고 비교는 기존 PhysicalKey의 `(composite, RowId)` 비교다. raw physical bytes 전체 비교로 대체하지 않는다.

internal의 모든 child는 PageRef wire 규칙을 만족해야 한다. parent 자신의 `(FileId,page_no)` 또는 다른 child의 같은 주소를 가리키면 incarnation이 달라도 거절한다. 다른 FileId의 같은 page number는 별도 주소다. 이 검사는 실제 target 존재·allocator 소유를 인증하지 않는다.

## Byte 한도와 split 계약

공통 envelope의 cell+slot 예산은16,224 bytes다. metadata32+slot4를 빼면 entry 예산 `C=16,188`이다. entry별 `cell length+4-byte slot ≤ H=8,094`를 요구하므로 cell은 최대8,090 bytes다. row/catalog leaf value 최대8,078이며 index value 상한은8,086−K다. 한 page의 entry 비용 합계는 C 이하다. 최소 row/catalog leaf 비용16이므로 entry 최대1,011, metadata 포함 slot 최대1,012다. internal은 key8에서 비용40, index key에 따라 더 크다.

split 후보 입력은 canonical/sorted cell sequence이며 entry 비용 합계 `T≤C+H=24,282`다. 이는 한 정상 page에 허용 entry 하나를 추가한 overflow 범위를 포함한다. T≤2C만으로는 임의 indivisible entry를 두 page로 나눌 수 없으므로 더 약한2C 약속은 사용하지 않는다.

- leaf: N≥2. boundary k의 left=`[0,k)`, right=`[k,N)`다. 두 편이 비어 있지 않고 각각 C 이하인 boundary 중 `max(left bytes,right bytes)` 최소, 동률이면 작은 k를 선택한다. separator는 right 첫 key를 대여하며 right에 남는다.
- internal: N≥3. pivot p는1..N−2다. `[0,p)`와 `(p,N)`을 두 page에 두고 p의 separator를 parent로 승격한다. right page의 leftmost는 p의 right child다. 각 page에 separator가 최소 하나 남고 각각 C 이하인 후보 중 같은 최소-max/작은-p 규칙을 사용한다.
- 반환 SplitPlan은 index·left/right entry bytes·borrowed separator·optional right-leftmost PageRef다. page를 쓰거나 입력 순서를 바꾸지 않는다. 실제 separator propagation·parent 공간·page allocation·mini-transaction은 상위 SMO의 책임이다.

각 entry≤H, T≤C+H이면 leaf의 half-total 인접 boundary 중 하나는 양쪽≤C다. overflow internal은 weighted median entry를 제거하면 양쪽이 C 이하이고, 양 끝 entry≤H<T/2라 pivot이 양 끝이 아니다. T≤C에서는 아무 적법 nonempty pivot의 두 편이 이미 C 이하이다. 구현은 모든 허용 boundary/pivot을 검산하므로 존재 증명만 믿고 overflow 계산을 생략하지 않는다. 너무 적은 entry는 NotSplittable, 한 entry 초과는 EntryTooLarge, 후보 총합 초과는 NoSpace다.

## API·오류·수명

`storage::tree_page`는 pub(crate)다. 새 source root/dependency·heap allocation·I/O·lock을 추가하지 않는다. metadata/cell encoder는 caller-owned buffer의 필요한 prefix만 쓰고 성공 시 tail을 보존한다. 오류는 전체 buffer를 byte 단위 보존한다. full page encoder는 정확한16,384-byte output만 받는다.

합성 API는 caller가 준비한 `cells: &[&[u8]]`에 slot0 metadata도 포함하게 한다. `encode_tree_page(header, expected_arity, cells, output)`가 metadata·모든 entry·strict order·child 주소·공간을 먼저 검증한 다음 기존 encode_page를 호출한다. 이 선택은 encoder 안에1,012개 slice reference scratch나 새 page-size stack image를 숨기지 않는다. `encode_metadata`, `encode_leaf_cell`, `encode_internal_cell` helper가 caller scratch를 만든다. cell helper의 canonical key 검증·길이/공간 확인도 최초 쓰기 전에 끝낸다.

`decode_tree_page(input, TreeExpectation)`은 expected PageIdentity/kind/level/arity와 envelope·metadata·entry를 검증하고 borrowed TreePageView를 반환한다. expected identity/owner/arity는 안정화한 anchor/schema에서 받아야 하며 input header를 그대로 기대값으로 삼지 않는다. 잘못된 caller shape는 InvalidInput, 유효한 기대 level/arity와 저장값 불일치는 LevelMismatch/SchemaMismatch다. envelope 오류는 TreeError::Page(PageError), tree 구조 오류는 Corrupt, entry별 byte 상한 초과는 EntryTooLarge, metadata 미지원 version은 UnsupportedFormat이다.

TreePageView의 entry index는 metadata를 제외한0..N이다. entry view는 key와 leaf value 또는 internal right child를 대여/복사한다. lower_bound는 canonical query를 먼저 검증하고 첫 key≥query의 index와 equality를 반환한다. internal route_child는 첫 separator>query를 찾아 그 앞의 child를 선택한다. separator와 같은 query는 right child로 간다. leaf에서 child routing은 InvalidInput이다.

`child_expectation(child_index)`는 leftmost를0, separator i의 right를 i+1로 삼는다. parent의 DB/owner/generation·선택 edge PageRef·동일 family/arity·level−1을 담은 기대값을 돌려준다. 이 기대값으로 child decoder를 호출하여 stale incarnation·다른 owner·잘못된 kind/level/arity를 거절한다. 실제 child를 읽는 I/O나 subtree 범위 검증은 하지 않는다. root도 caller가 anchor에서 얻은 exact kind/level을 기대값에 넣어 검증한다.

## 독립 literal과 실행 전 분모

기본 header는 DatabaseId=[0x11;16], FileId=0x0000000100000002, page3/incarnation4, owner5/generation6, LSN7이다. internal leftmost는 같은 file의 page8/incarnation3, separator right는 page9/incarnation3이다. 독립 Python struct/hashlib로 아래 literal과 envelope 위치만 사용해 full-page digest를 계산한다. production tree/page encoder를 기대 bytes 생성에 쓰지 않는다.

| literal | hex |
| --- | --- |
| row/catalog leaf metadata | `5842543101000000000000000000000000000000000000000000000000000000` |
| index arity1 leaf metadata | `5842543101000100000000000000000000000000000000000000000000000000` |
| row/catalog leaf key1 value A | `08000100000000000000000141` |
| row/catalog leaf key258 value 00ff | `08000200000000000000010200ff` |
| index leaf NULL/RowId1 value A | `0900010000000000000000000141` |
| index leaf Bool(false)/RowId258 value 00ff | `0a0002000100000000000000010200ff` |
| row/catalog internal separator258/right9 | `080000000000000000000102020000000100000009000000000000000300000000000000` |
| index internal Bool(false)/RowId258/right9 | `0a00000001000000000000000102020000000100000009000000000000000300000000000000` |

고정 Rust test9개를 다음 분모로 실행한다. 테스트 수만으로 하위 범위를 추정하지 않도록 내부 검산 count도 assertion한다.

| test | 고정 입력·판정 |
| --- | --- |
| tree_golden_pages_and_cells | 3 family×leaf/internal=6 full golden page, literal cell/metadata bytes, leaf empty3, internal level15 정상3 |
| tree_leaf_lower_bound_matches_typed_oracle | row/catalog key1/3/9/258/MAX와 query1/2/3/4/9/10/257/258/259/MAX. index의 명시 typed-order20 scalar×RowId1/258/MAX=60 key와 각 scalar의 RowId2/MAX−1 gap query40; ordinal·u64 linear oracle와 대조 |
| tree_internal_routing_and_child_expectations | 3 family의 equal/right routing, leftmost·middle·rightmost, parent level1/15; exact child DB/file/page/incarnation/owner/generation/kind/level/arity 기대값 |
| tree_rejects_checksummed_structural_damage | 6 goldens 각각 metadata magic/version/reserved/arity, key length/truncation/trailing, key zero 또는 invalid physical scalar, 순서 역전/동일 key. leaf leftmost nonzero; internal reserved·child FileId/incarnation zero·self/duplicate 주소. envelope checksum을 다시 계산한 손상도 거절 |
| tree_enforces_entry_page_and_output_limits | 비용8,094 entry 수락/8,095 거절, 두 최대 entry의 정확한 page fill, row/catalog1,011 empty-value entry,1,012 거절, index key1,032 수락/1,033 거절, 총 page 비용 C/C+1 |
| tree_leaf_split_uses_bytes | 길이2..8에 비용집합[16,4096,8094]의 모든9,837 vector. T≤24,282는 독립 전체-boundary oracle와 같고 두 page 재인코딩 가능, 초과는 NoSpace. [8094,16,16,16,16]은 count midpoint와 다른 boundary1, [8094,8094,8094]는 tie boundary1 |
| tree_internal_split_promotes_separator | 최소3 separator의 pivot1·right-leftmost, 64 seed×길이3..40=2,432개의 arity2 index 길이 profile; 비용[52,552,1064], seed별 LCG로 선택. 독립 pivot 열거·승격 key 제외·child 보존·두 page 재인코딩 |
| tree_child_alias_and_kind_level_guards | 모든3 family의 self·duplicate 주소는 incarnation이 달라도 거절, 다른 file 같은 page번호 수락; stale/foreign child와 level/kind/arity 불일치 거절 |
| tree_encoders_preserve_outputs | metadata/cell/page의 잘못된 input·shape·size·space·마지막 entry 오류 각각 sentinel 전체 보존, 성공 prefix 뒤 tail 보존. malformed query 거절 |

internal profile은 state=seed(1..64), 각 cell마다 `state=state*6364136223846793005+1 mod2^64`, high32 mod3으로 위 비용을 선택한다. key 첫 열은 cell 순번 Int64이고 둘째 열은 그 비용을 만드는 Binary 비영 byte 길이0/500/1012다. RowId는 cell 순번+1, child page number는 cell 순번+20이다. decoder·split 계산은 production 입력을 검산하고 expected order·byte 합계·pivot은 typed ordinal과 입력 length에서 독립 계산한다. 이는 표현 unit/invariant 시험이며 실제 tree/SMO·I/O·장시간 V3를 대신하지 않는다.

독립 full-page checksum과 SHA-256은 다음과 같다. checksum은앞16,352 bytes의SHA-256으로tail에저장하고, full hash는checksum포함16,384 bytes전체다.

| fixture | checksum tail | full-page SHA-256 |
| --- | --- | --- |
| row leaf | `eca84cc85e166ede52841deb22d27397ff50cf6e52d35cee19d3f7137b0f77ae` | `c4de9a633e8021cb256f4a0254718c41f2854fdc39fe0c883e0dcb8e3cfa91ef` |
| row internal | `bd30079dd665fd4bd585c36a8b97138838befba5e2c960da3da331c8a1d38c4b` | `295d4f7e4275b9024d3ece4cee9465d23f6b6259104c0aaa860b859974f95c2c` |
| index leaf | `1fd2e7bca17bb41e5f858bb661ab8eee07bce9abb098ba7811e38aa8245315c1` | `d2dd2a32ffa8b0b8eb3fa8c145cdc540467166af5ff729898bece589eb1d2f9d` |
| index internal | `565b5b5b8061fbd6b08b0d427efc28c968f2000639dc66a25a1391f2edc3d93f` | `5d46cf75a9c53671e37ac99baec885e3c4e9c7def4d9ee5c155910667584bd1a` |
| catalog leaf | `0b04e62e52ea32a6846057edd977dc936b3c8d45aac35dd76b8407a264ddca6c` | `2c301c28ebc5ea5c191843cfdb29593e479a998352086bead3c2091afd11555c` |
| catalog internal | `977aefe78ea8f9ffeec46a69e513fbd4cfff13de573711e3a174c7da305c1147` | `54a1664f6ae5e019cbbea67e8a1bcafc01cf06d2ccac1e7e25bca18ed786c636` |

정렬 oracle의20 scalar 순서는 NULL; Bool(false,true); Int64(MIN,-2,-1,0,1,MAX); Utf8(빈문자열, NUL, a, a+NUL, é); Binary(빈값,00,0000,00ff,01,ff)다. 손상 분모는 공통8종×6 page=48, leaf/internal의역순·동일key각3family×2=12, leaf leftmost3, internal reserved/FileId0/incarnation0/self/duplicate 5종×3=15로 총78개다.
