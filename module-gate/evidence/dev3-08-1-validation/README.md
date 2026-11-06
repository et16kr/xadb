# DEV3 08-1 완료 후 설계·구현 검토

2026-11-07 KST · 기준 `be52fc679deca576ed455dcb698a7f33472a76a8`(PR #59 병합).
사용자의 08-1 검증·문제 수정·PR 요청에 따라 `/root`가 검토하고,
`/root/independent_review`가 별도 제품 코드 검토와 수정의 D2 검토를 맡았다.

## 범위와 판단

PR #53–#59의 [SMO 계약](../../../devdoc/contracts/dev3-tree-smo.md),
[제품 방향](../../../devdoc/plans/project-direction.md),
[일정](../../../devdoc/plans/dev3-schedule-baseline.md)을 구현과 대조했다.
정적 검토는 split candidate와 current-leaf 경로, A 자동 split, live unlink와 replay,
Base4 closure, U4 worker/controller와 원 실행 증거를 대상으로 했다.
전체 변경의 모든 경로를 새로 실행하거나 성능·전원 단절을 인수한 검토는 아니다.

- redo-only 완결 SMO가 사용자 rollback에도 남고, 행·CLR·Prune은 현재 anchor에서 다시 찾는 방향은 적절하다.
  RootReserved 동안의 구조 변경 거절과 read gate는 미완결 행 효과와 읽기 중 경로를 보호한다.
- 같은 cut의 canonical candidate 재계산, frame의 exact 비교, live/replay의 같은 전이 규칙은 임의 after image 채택을 막는다.
- XTR1 목록과 도달 집합을 분리하고 두 집합의 합을 allocator 소유 집합과 대조한다. unlink는 Free 권한을 만들지 않는다.
- 검토한 제품 경로에서 새로운 필수 정확성 결함은 확인하지 않았다. 아래 controller의 증거 확인 누락을 수정했다.
- 08-1의 2일 적립(35/86일 = 40.70%)은 제한된 공급 단위의 완료다. 08 전체·S1·대용량 성능 인수와 구별해야 한다.

## 발견과 수정

**N05의 장애 입력 확인 누락.** `check_produce`는 home write의 위치·길이·첫 1 byte 완료와
DWB sync를 확인했지만, 복구 전 실제 home page가 손상됐는지는 확인하지 않았다.
page byte 0은 XDP1의 고정 magic이다. 이미 유효한 page에 그 byte만 다시 쓰면 checksum은 유효하게 남는다.
독립 합성 입력에서 정상 checksum의 16 KiB page와 짧은 page를 모두 받아들였다.

이는 기존 native 실행의 실패나 제품 데이터 손상을 발견했다는 뜻이 아니다. 보존된 N05 data-file hash는
다른 cut과 달랐으며 현재 fixture가 이미 손상된 home을 만들 가능성과 양립한다. 결함은 판정기가 그 사실을
검증하지 않아 fixture 변경으로 장애가 사라져도 PASS할 수 있다는 데 있다.

D2 선택: 기존 trace 검사를 통과한 정확한 home file/offset에서 16,384 B를 읽고,
마지막 32 B와 나머지의 SHA-256이 달라야 복구로 진행한다. page codec의 checksum 배치와 같으며
Python stdlib로 독립 계산한다. production decoder를 정답 계산에 사용하지 않는다.
worker·제품·형식·fault inventory를 바꾸는 대안은 현재 fixture의 문제를 입증하지 못했으므로 채택하지 않았다.
재검토 조건은 page checksum 형식이나 N05 fault injection 방식 변경이다.

## 검증

실제 수정은 Python controller와 새 테스트 둘뿐이다. [source.json](source.json)에 변경 blob·hash·선택을 기록했다.

- 새 테스트 둘만 선택했다. [red.log](red.log)는 수정 전 부정 입력 3개를 거절하지 못한 결과,
  [green.log](green.log)는 수정 후 2 tests PASS다. 기존 8개 Python 시험은 반복하지 않았다.
- 명령: `PYTHONPATH=scripts python3 -B -m unittest -v` 뒤에 source.json의 두 exact ID를 전달했다.
- 제품 Rust는 기준 commit과 같으며 native worker 실행을 위해 `cargo test -p xadb-engine --lib --no-run --locked`로 빌드했다.
  `--no-run`은 제품 시험 실행 결과가 아니다.
- 수정 controller가 추가한 N05 분기를 실제 파일에서 확인하기 위해 `UNL-N05-L`, `UNL-N05-R`만 선택했다.
  나머지 cut·memory twin·workspace 시험은 반복하지 않았다. 두 사례 모두 produce·recover·reenter가 PASS했다(총 6 process).

## 후속 범위와 주의점

- 첫 extent 64 page/동적 55 page, root 축소 level 1→0, 빈 leaf만 unlink하는 한계는 계약에 명시돼 있다.
  일반 extent 성장·underfull merge·높은 root 축소가 완료된 상태가 아니다.
- 퇴역 page는 Allocated 상태다. 실제 Free/reuse(3.03.4) 없이 반복 split·delete 부하의 공간 재사용을 보장할 수 없다.
- CREATE Catalog Job 부채 때문에 일반 제품 table의 자동 split/unlink는 아직 막힌다.
  시험의 `retire_create!`를 제품 create/drain 공급(10-1)으로 해석하면 안 된다.
- UPDATE 입력 검사에서 새 cell 전체를 여유 공간과 비교하는 기존 07 편차는
  [알려진 편차](../../../devdoc/contracts/dev3-row-update.md#known-deviation-validate-capacity)다.
  기존 계약은 별도 수정 작업으로 분리했다. 이번에는 08-1에서 새로 발생한 결함으로 재분류하지 않는다.
- 매 연산 canonical WAL replay와 큰 page-image WAL 비용은 [기존 개선 계획](../../../devdoc/plans/dev3-implementation-plan.md#review-adoption-20261104)의
  A1 등 후속 대상이다. 이번 정확성 검토로 확장성·HA·TPS를 승인하지 않는다.
- native process abort는 전원 단절 시험이 아니다. 이번 새 실행은 N05의 실제 checksum 손상과 복구 경계만 추가 확인한다.

### 선택 native 실행 결과

[실행 계획](native-execution-plan.json), [process 결과](native-results.json), [완료 결과](native-complete.json),
[receipt·파일 hash](native-receipts.json), [로그](native.log)를 보존했다. produce는 LeafUnlink 115.76초,
RootCollapse 69.90초였고 모두 계획한 SIGABRT로 끝났다. recover·reenter는 모두 exit 0이다.
수정 controller가 복구 전에 실제 16 KiB home page의 checksum 불일치를 확인했고, 그 뒤 DWB 수리·READY tree·
퇴역 목록·bank·행 읽기와 재진입이 통과했다. 기존 N05가 손상을 만들지 못했다는 주장은 하지 않는다.

전체 source manifest·원 trace·test log·before image·실행 후 DB는
`/workspace/scratch/dev3-08-1-review/native/`에 보존하고 [artifact hash](native-artifacts.sha256)를 남겼다.
실행 계획의 source hash는 실행 시작 시 작업 트리 manifest이며, 후에 증거를 추가한 최종 commit hash가 아니다.
Rust source는 기준 main과 동일하다. Python 선택 시험은 로컬 결과이고 CI에 새로 등록된 시험이라고 주장하지 않는다.
별도 [D2 검토](review.md)는 자기 검토와 구별한다.
