Verdict: ACCEPT

# PR #66 독립 D2 추가 검토 — O0 4판 최종

2026-11-07. 검토자 `/root/overflow_design_review`; 작성·수정·통합 담당 `/root`와 별도 실행이다.
이 판정은 **O0 설계와 추가 oracle의 검토**이며 O1–O4 제품 동작의 수용이 아니다.

## 정확한 대상

기준 commit은 `928ec853348a0f197a0da0b65574eb386237499f`다. 그 기준 대비 4판 diff,
새 oracle, [초기 검토](review-codex-initial.md), [작성자의 결과 기록](review-codex.md)을 읽었다.
최종 파일을 `git hash-object`로 직접 확인했다.

| 파일 | 검토한 Git blob |
| --- | --- |
| `devdoc/contracts/dev3-row-overflow.md` | `b5dc2b7dc9577bfcbc8702302b4637df63123df8` |
| `module-gate/evidence/dev3-08-3-design/test_design_oracles.py` | `2dde44678fa9c1fd751d968d4e5d9e8366d70afc` |

## 필수 지적의 해소

- **R1 해소.** preflight의 source/schema 관측과 A 값 검산 후 reserve(plan)의 k 판정을 분리했다. 원 오류 순서를 유지하며 allocator 부족은 별도 OverflowCapacity로 처리한다. leaf split 길이 64, allocator 부족의 split0·효과0, Free k−1/k와 오류 우선순위 인수가 명시됐다.
- **R2 해소.** prepared-chunk의 성공 반환은 활성 ReservedRow다. C가 durable ChunkWrite 결과에서 draft·pending·LSN을 갱신하며 Forward/Cancel만 예약을 종료한다. 예약 중 flush는 준비 cut과 모든 page guard를 놓은 그룹 경계로 한정하고, 원 lease/control·전용 flusher owner/credit을 사용하며 Ready 뒤 재검산한다. 기존 flush 금지 계약의 좁은 대체를 명시했고 stage 중 flush·새 I/O owner를 허용하지 않는다. gate는 preflight에서 얻은 하나를 계속 유지하며 재획득하지 않는다. 이 구조는 O0에서 일관되며 Rust 수명·자원 상한·실제 오류 처리는 O3의 구현과 census로 검증해야 한다.
- **R3 해소.** C ReadStep, B Step, ReadSource와 LiveSource/LocatedSource, copy_selected/select_row의 전달 변경을 지정했다. 전체 digest/schema 확인 후에만 copied_slot을 채우며 FlushRequired·오류도 checked cleanup을 거친다. 부분 output의 유효 길이 0, 미완료 visible 행의 진행점 보존, 같은 행의 root부터 재시도와 point/cursor 새 사례가 기존 F02와 일치한다.

974,020 B를 행 root의 WAL 소계로 한정한 보완도 적절하다. 전체 거래 admission에는
기존 terminal·control·Aging 의무가 추가된다는 구분을 유지한다. 제품 방향·현재 지원
한도·실제 재사용의 후속 범위를 바꾸지 않았으며 불변식이나 인수 한도를 완화하지 않았다.

## oracle 검토

코드를 정적으로 읽고 산식·범위·음성 입력을 확인했다. 기존 제품 시험이나 이미 완료한
oracle를 검토자가 반복 실행하지 않았다. 실제 5개 PASS 및 보강한 2개 selector PASS는
작성자가 실행·전달하고 위 결과 기록에 보존한 증거다.

- frame 산식은 native header/body, XHG/XSG, inventory, whole-body Patch/INIT을 별도 항으로 합산한다. 459개 목록/반환 조합과 상한을 검사하며 제품 encoder를 기대값으로 사용하지 않는다.
- 길이 전수는 129,024개이고 debt 비교는 k/written/h의 108상태다. 반환 후 실제 frame와 잔여 cleanup이 예약을 넘지 않는지도 본다.
- owner 모델은 allocation과 소유 발생 횟수를 따로 센다. owner 미삭제·누락뿐 아니라 같은 leaf 범주 안의 두 chain이 한 page를 공유하는 mutant를 추가해 집합 합산만으로 중복을 숨기는 오류도 검출한다.
- projection fixture는 유효한 세 u64 PageRef를 사용한다. root 제외가 값 commitment 입력을 보존하는지 검산하는 국소 모델이며 실제 XRP1 codec을 시험한 것으로 주장하지 않는다.
- 이 모델은 source identity·page envelope·실제 Step/flush·실제 crash replay의 구현과 동일하다는 증거가 아니다. 결과 기록은 이 한계를 분명히 구분하고 R1–R3의 제품 회귀를 OVF-J03/J05에 남긴다.

**열린 필수 지적 0.** 현재 O0 수정과 새 설계 oracle는 수용한다. O1–O4 제품 시험,
native fault cut, Rust 수명/소유 구현 및 각 단계 census는 계속 NOT_RUN이며 이 판정으로
완료·적립하지 않는다. 최종 commit의 push gate는 작성자가 별도로 수행한다.
