# DEV3 page envelope 형식

2026-10-20 · **D3-LEAF-001 F01 부분 동결 / 개발 형식 v1**

[저장 설계](disk-storage-design.md#page) · [구현·소유·검토](../implementation/dev3-implementation.md#wave-1)

이번 형식은 page envelope의 무효과 byte 변환만 정의한다. cell의 row/index/undo 의미, 정렬 key·child level·참조 대상의 존재, allocation ownership, WAL·DWB·파일 내구성은 소비 leaf에서 별도로 검증해야 한다. envelope가 유효해도 tree나 DB가 유효하다는 뜻은 아니다. 기존 DEV2 형식은 바꾸지 않으며 새 development fixture만 사용한다.

## F01 bytes

page 크기는 정확히 16,384 bytes다. header는 128 bytes, 마지막 32 bytes는 앞의 16,352 bytes에 대한 SHA-256이다. 정수는 little endian이며 struct dump를 사용하지 않는다.

| offset | bytes | 필드·조건 |
| --- | --- | --- |
| 0 | 4 | magic `XDP1` |
| 4 | 2 | format 1; 다른 version은 UnsupportedFormat |
| 6 | 1 | kind: row leaf1/internal2, index leaf3/internal4, catalog leaf5/internal6, overflow7, undo8, transaction status9, allocation10, tree anchor11, retention root12/jobs13 |
| 7 | 1 | internal level1–15; 나머지0. 최대 tree 높이16 |
| 8 | 16 | 기존 `DatabaseId` bytes; 전부0 거절 |
| 24 | 8 | FileId: high32 writer epoch·low32 counter, 각각 nonzero. 발급/영속 예약·counter 소진과 wrap 거절은 이후 issuer 책임 |
| 32 | 8 | page number; 0 허용 |
| 40 | 8 | page incarnation; 기존 Generation 의미, 0 거절·wrap 금지 |
| 48 | 8 | owner ObjectId; 0 거절 |
| 56 | 8 | owner Generation; 0 거절 |
| 64 | 8 | PageLsn; 0은 아직 logged update 없는 INIT 표현. WAL validation은 별도 |
| 72 | 2 | slot count |
| 74 | 2 | lower = 128 + 4 × count |
| 76 | 2 | upper = 마지막 cell 시작, 빈 page이면16,352 |
| 78 | 50 | reserved, 전부0 |
| 128 | 4 × count | slot 순서대로 `(offset:u16,length:u16)` |
| lower | upper − lower | 미사용 공간, 전부0 |
| upper | 16,352 − upper | cell bytes |
| 16,352 | 32 | SHA-256 |

v1은 compact한 canonical 표현이다. slot0의 cell은 digest 직전에 끝나며 slot1은 slot0 직전에 끝나는 식으로 역방향 연속 배치한다. cell은 빈 byte string을 허용하지 않는다. offset/length overflow, directory 침범, overlap, gap, trailing cell 공간은 거절한다. slot 순서는 논리 순서이며 물리 주소 순서와 다르다. 삭제·갱신 소비자는 compact한 새 image를 만들어야 한다. fragmentation 지원은 후속 명시 형식 결정이다.

## F02 무효과 API

`storage::page`는 engine 내부 `pub(crate)` 모듈이다. 외부 public API로 저장 표현을 노출하지 않는다. 실제 S2 소비가 연결될 때까지 이 모듈에만 이유를 기록한 `dead_code` 허용을 둔다.

- `encode_page(header, cells, output)`은 caller-owned 정확한 크기의 buffer를 받는다. header·크기·합계·cell 조건을 전부 확인한 다음에만 output을 바꾼다. 오류이면 output은 byte 단위 그대로다. 새 allocation·I/O·lock·숨은 quota 사용은 없다.
- `decode_page(input, expected_identity, expected_kind)`는 정확한 길이·version·checksum·canonical 구조와 호출자의 DB/file/page/incarnation/owner/kind를 확인하고 borrowed `PageView`를 반환한다. 입력을 바꾸지 않으며 allocation·I/O가 없다.
- `PageView::cell(slot)`은 검증한 input의 slice만 대여한다. 존재하지 않는 slot은 None이다. mutable input과 함께 살아남을 수 없으며 frame pin·content latch를 대신하지 않는다.
- `PageError`는 InvalidInput, UnsupportedFormat, Corrupt, IdentityMismatch, KindMismatch, NoSpace로 구분한다. 모두 무효과이며 자동 재시도·DB fencing 정책은 상위 소유자 책임이다.

독립 fixture는 Python 표준 `struct`·`hashlib`로 이 표에서 생성한 고정 binary와 SHA-256이며 production encoder로 기대 bytes를 생성하지 않는다. 정상 empty와 여러 cell, page/owner identity mismatch, 모든 잘린 길이, 단일 byte 손상, checksum을 다시 계산한 의미 손상, 정확한 공간 경계와 실패 output 보존을 검사한다. 이는 V1/V2 표현 시험이며 V3 30분 fuzz·실제 I/O·DS 인수를 대신하지 않는다.

V2 생성 profile은 seed1–64 각각128 operations, LCG `state=state*6364136223846793005+1 mod 2^64`, 반환 high32다. 매 operation의 cell 수는 첫 반환값 mod12, 각 길이는1+다음값 mod300, 각 byte는 다음값 low8이다. 매4번째 operation은 reserved byte를 손상시키고 checksum을 재계산하여 구조 거절을 확인한다. 기대 cell은 입력 vector에서 얻으며 codec 출력에서 계산하지 않는다. 정확한 최대 page/slot 경계는 별도 고정 시험으로 보완한다. 실행 뒤 범위를 축소하지 않는다.
