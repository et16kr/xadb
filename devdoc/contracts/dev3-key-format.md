# DEV3 복합 key 표현

2026-10-20 · **D2 표현 계약 채택 / 제한된 실행·독립 정적 검토 완료 / index 실행 미완료**

[소유 설계](disk-storage-design.md#keys) · [착수 소유](../implementation/dev3-implementation.md#wave-2)

이 F01 leaf는 무효과 byte 표현과 비교만 정한다. index schema·잠금·statement-final unique·tree·disk 실행의 완료를 뜻하지 않는다. 기존 foundation scalar key의 의미와 bytes는 변경하지 않는다. 두 실제 소비자(기존 scalar key와 새 composite codec)가 같은 encoder를 사용하도록 정확한 길이·caller buffer 쓰기·prefix 검증을 foundation에 둔다.

## Byte 계약

| 값 | canonical scalar bytes |
| --- | --- |
| NULL | `00` |
| Bool | `01 00` 또는 `01 01` |
| Int64 | `02` 뒤 signed 값을 unsigned로 해석하고 최상위 bit를 반전한 8 bytes, big-endian |
| Utf8 | `03` 뒤 원래 UTF-8 bytes의 `00`을 `00 ff`로 escape, 끝에 `00 00` |
| Binary | `04` 뒤 같은 escape와 terminator |

scalar prefix decoder는 Bool의 다른 값, 알려지지 않은 tag, 잘린 숫자·escape·terminator, 비정규 UTF-8 및 decoded payload 64 KiB 초과를 거절한다. string 내부 `00 xx`에서 `xx`는 `00` 또는 `ff`만 허용한다. UTF-8 code point 중간의 escaped NUL도 거절한다. 이 decoder는 정확히 첫 scalar의 byte 길이와 NULL 여부를 반환하며 뒤 bytes의 의미는 해석하지 않는다.

Composite는 schema 순서의 scalar bytes를 그대로 이어 붙인다. 열 수는 소유 index schema에서 전달하며 1–4다. decoder는 그 수만큼 소비한 뒤 남은 bytes가 없어야 한다. escaped composite 상한은 **1,024 bytes**이며 별도 length prefix·column count를 key 정렬 bytes에 넣지 않는다. 비교는 unsigned byte lexicographic이고 typed tuple 순서와 같다. NULL은 각 column에서 가장 먼저, type 순서는 NULL·Bool·Int64·Utf8·Binary다. 서로 다른 schema의 비교가 index 간 비교를 허용하는 capability는 아니다.

물리 index key는 composite 뒤에 nonzero `RowId`의 8-byte big-endian 표현을 붙인 것이다. composite 한도에는 RowId를 포함하지 않으므로 물리 key 상한은 1,032 bytes다. decoder는 마지막 8 bytes를 RowId로 분리하고 나머지 전체를 위 규칙으로 검증한다. 비교는 `(composite bytes, RowId 숫자)` 순서다. 가변 길이 composite의 byte 접미사에 RowId를 붙였다는 이유로 **물리 key 전체 raw byte 비교를 사용하지 않는다**. 예를 들어 서로 다른 열 수의 key나 prefix 경계를 임의로 혼합하지 않는다.

NULL 개수는 검증한 scalar prefix에서만 계산한다. `has_null`은 PK 입력 검증에, `all_null`은 unique 충돌/잠금 예외 판단에 공급하는 정보다. 부분 NULL은 예외가 아니다. 이 codec은 PK·unique 여부를 모르며 statement 원자성·다른 거래와의 충돌을 검사하지 않는다.

## API·효과

foundation `encoded_key_len`은 정확한 scalar 크기를 검사한다. `encode_key_into`는 정확히 그 크기의 caller buffer만 수락한다. 기존 `encode_key`는 같은 helper를 사용하여 bytes 호환성을 유지한다. `inspect_key_prefix`는 allocation 없이 canonical prefix를 검사한다.

engine `encode_composite`와 `encode_physical_key`는 전체 값·열 수·길이를 먼저 검사한 뒤 caller buffer의 필요한 prefix만 쓴다. 부족한 buffer, 범위 밖 열 수, 값/키 크기 오류는 **전체 output을 보존**한다. 여분 tail도 보존한다. 반환 view는 output을 빌리며 그 수명 안에서만 유효하다. decoder도 allocation·I/O·lock·quota 소유 효과가 없다. typed RowId 생성은 발급·유일성 확보와 다르다.

## 실행 전 고정 입력

| 입력 | 기대값·판정 |
| --- | --- |
| `[NULL,NULL]` | `0000`, all-null=true |
| `[Int64(1),NULL]` | `02800000000000000100`, has-null=true, all-null=false |
| `[Utf8("a\0b"),Binary(00 ff)]` | `036100ff6200000400ffff0000` |
| `[Bool(false),Int64(-1),Utf8(""),NULL]` | `0100027fffffffffffffff03000000` |
| `[NULL,NULL]`, RowId=258 | `00000000000000000102` |
| Int64 edge vectors | MIN→`020000000000000000`, -1→`027fffffffffffffff`, 0→`028000000000000000`, MAX→`02ffffffffffffffff` |
| Binary 1,021 nonzero bytes | scalar/composite 1,024 bytes 수락; 1,022개는 거절 |
| Binary 510 NUL + 1 nonzero | escaped 1,024 bytes 수락; NUL 하나 추가하면 거절 |
| 열 수·RowId | 0/5열·zero RowId 거절; 1/4열·u64::MAX RowId 수락 |
| 순서 oracle | 명시 순서의 20 scalar 값으로 두 열의 모든 400 tuple을 만들고 모든 pair의 tuple ordinal과 byte 비교를 대조. RowId 1/258/MAX를 추가한 물리 비교도 대조 |
| 손상 | 각 golden의 모든 truncation, unknown tag, Bool=2, escape=01/fe, 미완결·과잉 UTF-8, column 수 불일치, trailing byte, zero RowId |
| 실패 효과 | 크기/열/값 실패 각각 sentinel output 전체 유지, 성공 시 여분 tail 유지 |

이는 표현 입력이며 고정 seed가 모델/실제 인덱스 인수를 대체하지 않는다. 독립 검토에서 의미 오류를 찾으면 수정 이유·실행 이전/이후를 구분해 기록한다.

동결 입력으로 key5·scalar3과 기존 page/P01을 실행했다. 별도 reviewer `/root/dev3_model`의 정적 검토를 root가 확인했다. 실제 소스·원본 receipt·국소 회귀 범위는 [구현 기록](../implementation/dev3-implementation.md#key-leaf)을 따른다. 실행 receipt의 계약 hash는 실행 전 초안 bytes를 가리키며, 이 완료 상태 문구를 당시 hash에 소급 반영하지 않는다.
