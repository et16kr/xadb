# PR #67 추가 설계·코드 검토와 신규 시험

2026-11-07 · 기준 `cb48f496ff33adb428e611c553d3a415d2687787` → PR head `899b1ac9c8209065191075ce6972d1b97cb45a9b` · 추가 시험 D1 ACCEPT · 실행 결과는 아래 기록

## 검토 결론과 범위

사용자의 PR 설계·방향성·코드 검토, 기존 시험 외 신규 검증 및 문제 수정 요청에 따른다. `/root`는 overflow chunk/chain codec, canonical stream/digest/window/materialization, XTR1, O1 계약과 SCOPE-002의 관계를 검토했다. 별도 `/root/independent_review`는 Job3 상태·budget·LSN·ledger/census, opcode/shape/native/redo, Base4 certificate/provenance와 소비 경계를 읽기 전용으로 검토했다. **이번 검토와 신규 시험에서 제품 코드의 필수 수정 결함은 발견하지 못했다.** 제품 코드는 바꾸지 않고 신규 시험을 PR에 추가한다.

- 뒤에서 앞으로 chunk를 쓰는 선택은 현재 frame/loan 한도 안에서 전체 행 buffer나 미사용 page 예약을 추가하지 않는다. root 없는 chunk와 Job pending 칸은 이 선택과 맞는다.
- 새 형식을 여는 O1과 C 전이·제품 경로를 여는 O2/O3의 구분을 유지한다. replay의 새 opcode 거절과 Base4 role 7 소비 거절, 아직 inline만 받는 제품 경로를 성공으로 보고하지 않는다.
- `cleanup_started()`는 chain 반환 뒤 cursor 0인 상태도 정리 시작으로 센다. Job·census·ledger의 기존 O1 신규 사례가 이 규칙을 직접 검사하므로 중복 실행하지 않았다.
- SCOPE-002의 최종 목표를 이번 PR이 달성한 것은 아니다. whole-page WAL 비용(A2), 장시간 TerminalReadGate와 allocator 공유 지점(A4), 첫 extent/128 KiB profile, 실제 Free·재사용(14-2)은 계약 §9의 후속 의무다. O1의 합격을 확장성·동시성·내구성 인수로 승격하지 않는다.
- Reserved overflow ledger의 `forward_steps()` 적용과 role 7의 page-image/소유 closure는 계약 §14대로 O2에 남는다. 이미 미구현으로 명시되고 O1에서 거절되는 기능을 이번 결함으로 분류하지 않는다.

## 추가한 시험

| 정확한 case | 독립 기대값과 추가 검증 |
| --- | --- |
| `storage::row_record::stream::tests::pr67_maximum_row_matches_independent_bytes_and_digest` | 64 KiB Binary + 65,522 B Binary로 정확히 128 KiB canonical row. 제품 encoder 없이 literal header와 입력 bytes로 기대 row를 구성, Python SHA-256 고정값과 대조. 9개 window를 쓰기 순서인 역순으로 읽고 마지막 출력 tail 보존·길이 필드 분할·131,073 B 거절을 확인 |
| `storage::row_record::stream::tests::pr67_utf8_stream_exhaustive_pairs_and_scalar_boundaries` | 모든 65,536개 두 byte payload를 12개 split 위치마다 검사(786,432조합). 별도 scalar 경계 12개는 156개 split 조합. 빈 push·손상 후 실패 상태 유지 포함. 기대값은 다른 row decoder가 아닌 `std::str::from_utf8` |
| `storage::overflow::tests::pr67_maximum_pending_suffixes_preserve_walk_on_rejected_identity` | 9-chunk chain의 nonempty suffix 9개, 총 45개 chunk 위치에서 다른 incarnation 거절 뒤 올바른 page 재시도. expected edge·미완료 상태·최종 주소 목록을 독립적으로 확인 |

최대 row의 독립 계산(표준 Python, 제품 encoder/hash 호출 없음):

```python
import hashlib, struct
row = (struct.pack('<I', 2)
       + b'\x04' + struct.pack('<I', 65536) + bytes(range(256)) * 256
       + b'\x04' + struct.pack('<I', 65522) + bytes([0xa5]) * 65522)
assert len(row) == 131072
assert hashlib.sha256(row).hexdigest() == (
    'bdfdc6b6322454723e0f03494121b082f358a1a2a21ab7b9bab6a0453c7f9b2e')
```

## 실행 증거

실행 명령: `CARGO_TARGET_DIR=/tmp/xadb-pr67-target cargo test -p xadb-engine --lib pr67_ --locked` (Rust 1.95.0). 선택 범위는 위 새 사례 3개뿐이다. 기존 120개·census 322개·전체 workspace 제품 시험을 반복하지 않는다.

실행 상태: **PASS — 3 passed, 0 failed, 0 ignored, 2,471 filtered out; 0.52 s**. 최초 빌드는 3분 5초였다. [원 실행 로그](tests-codex.log)의 SHA-256은 `a66d4bdaf8a8c8127c7ea9547cee6416aec8e52ca5642d1dddeec9514121a22e`다. 시험 binary SHA-256은 `280a84b04074b3d7b087836df5d259bd394e41d29d7c339f85b75a2e9744a9da`다. 제품 source는 위 원 PR head 그대로이며 추가 test source는 아래 두 Git blob이다. 실행 후 새 source 변경·실패가 없어 같은 시험을 반복하지 않았다. Build target과 임시 binary는 증거를 보존한 뒤 정리한다.

## 별도 D1 검토

`codex-pr67/root/independent_review`가 추가 diff, owner 계약, literal 기대값과 gate selector 매핑을 확인하여 ACCEPT, 필수 지적 0건. 최대 row의 길이·SHA를 별도 Python으로 다시 계산했다. 기존 제품 시험은 재실행하지 않았다. 기존 review와 source별 PASS 기록은 그대로 보존한다.

| 검토한 파일 | Git blob |
| --- | --- |
| `crates/engine/src/storage/overflow_tests.rs` | `d4ec2f6473e25d971143da03a08fbb872f2f0053` |
| `crates/engine/src/storage/row_record/stream_tests.rs` | `ff6f4a82468af3a0e1c694234b13855ea3185b30` |

기존 module-gate record에 위 두 blob의 새 D1 review와 정확한 selector 3개를 추가한다. 과거 review의 blob을 최신 hash로 소급 덮어쓰지 않는다. push의 fmt·workspace Clippy·문서 검사는 commit snapshot에서 훅이 실행하며 제품 시험과 별도다.
