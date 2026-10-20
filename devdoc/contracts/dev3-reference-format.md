# DEV3 PageRef·UndoRef·안정 anchor 표현

2026-10-20 · **D2 표현 계약 채택 / 국소 시험·독립 정적 검토 완료 / 실제 참조 수명·I/O 미완료**

[page envelope](dev3-page-format.md) · [tree 소유 계약](disk-storage-design.md#tree) · [undo 소유 계약](disk-storage-design.md#undo-lifecycle) · [작업 소유](../implementation/dev3-implementation.md#wave-2)

이 표현 leaf에는 I/O·allocator·reference pin·undo record·WAL 효과가 없다. 실제 링크·회수와 root 변경은 M01–M06 및 해당 WAL·자원 계약 이후의 의무다. 다음 layout은 raw Rust struct와 독립이며 모든 다중 byte 수는 little-endian이다.

## 공통 PageRef

정확히24 bytes다. `0..8=FileId(epoch:u32가 상위32bit, counter:u32가 하위32bit, 각각 nonzero)`, `8..16=page_no:u64(0 허용)`, `16..24=incarnation:u64(nonzero)`다. DB·owner·object generation은 담지 않으며 이를 검증하는 page load의 기대 identity가 별도로 필요하다. encode/decode는 정확한 길이만 수락하고 encode 오류는 output 전체를 보존한다.

독립 literal vector `FileId=0x0000000100000002,page=3,incarnation=4`는 `020000000100000003000000000000000400000000000000`이다.

## UndoRef

정확히40 bytes다. `0..24=PageRef`, `24..26=slot:u16`, `26..32=reserved zero`, `32..40=slot generation:u64(nonzero)`다. slot은 page envelope가 수용할 수 있는 최대 slot 수보다 작아야 한다. 이 보수적 상한은 `(16,352−128)/(slot directory4+최소 cell1)=3,244`이므로 slot0..3,243을 허용한다. 실제 undo record 크기의 더 작은 한도와 slot의 존재/종류/generation은 향후 undo page decoder가 검사한다.

slot은 안정된 directory index다. record 반환 때 남겨야 하는 tombstone·generation과 재사용은 undo 소유 계약에서 추가 정의하며 이 codec이 slot을 이동/재사용하지 않는다. absent UndoRef는 이 형식의 zero reference가 아니라 소유 record의 별도 presence tag로 표현해야 한다.

위 PageRef에 slot5/generation6이면 `02000000010000000300000000000000040000000000000005000000000000000600000000000000`이다. slot3,243 수락/3,244 거절, generation0 거절을 고정한다.

## 안정 anchor

anchor는 공통 kind11·level0 page이며 cell을 정확히 하나 가진다. 그 cell은 아래48 bytes다.

| offset | 크기 | 값 |
| --- | --- | --- |
| 0 | 4 | ASCII `XTA1` |
| 4 | 2 | version1 |
| 6 | 1 | tree의 leaf kind: row1/index3/catalog5 |
| 7 | 1 | root level0..15 |
| 8 | 24 | root PageRef |
| 32 | 8 | structural revision, nonzero |
| 40 | 8 | reserved0 |

root는 anchor 자신의 물리 주소와 달라야 한다. 같은 FileId/page_no이면 incarnation을 다르게 적어도 자기 주소이므로 거절한다. leaf kind가 row이면 root level0에서는 RowLeaf, level1..15에서는 RowInternal을 기대하며 index/catalog도 같은 방식이다. root page 확인은 **같은 DB·owner·owner generation**, exact root PageRef, kind·level을 모두 요구한다. page bytes 검증과 root의 frame pin/content latch 확보는 별개다. anchor 재설치/SMO·revision 발급·overflow를 이 codec에서 수행하지 않는다.

위 PageRef의 row leaf root, revision7 cell은 `585441310100010002000000010000000300000000000000040000000000000007000000000000000000000000000000`이다. anchor 자체 identity는 root와 다른 page_no8, 같은 DB/owner로 고정한 시험을 사용한다.

## API·고정 시험

모든 encoder는 사전 검증 후 caller buffer만 변경한다. decoder는 정확한 형식·길이·reserved·ID·slot/level 범위를 검사한다. 실패는 알려진 무효과이며 I/O retry/fence 의미를 추가하지 않는다. anchor는 page envelope의 identity/digest 검사를 반드시 경유한다.

실행 전 고정 corpus는 위3개 literal, 모든 짧은 길이·trailing byte, FileId 상/하위0·incarnation0·slot generation0·reserved bit·slot 경계, anchor0/2cell·잘못된 magic/version/kind/level/revision/reserved·self-root, row/index/catalog×root level0/1/15와 level16 거절, root DB/file/page/incarnation/owner/generation/kind/level 불일치다. 유효 checksum을 다시 만든 semantic 손상도 거절해야 한다. encode 실패는 sentinel output 전체 유지로 검사한다. 이것은 codec 입력 경계이며 실제 tree traversal·undo pin·파일 복구 인수를 대신하지 않는다.

root의 표현 시험 anchor4·reference2를 통과했고 `/root/dev3_model`이 계약·codec·시험을 별도 정적 검토하여 필수 지적 없음으로 보고했다. reviewer의 별도 실행은 없었다. 이 상태 문구는 이전 실행의 계약 hash를 바꾸지 않으며 동결 source receipt는 [구현 기록](../implementation/dev3-implementation.md#reference-leaf)에 별도로 연결한다.
