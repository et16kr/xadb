# 08-1 U4 live unlink native fault cut 실행 기록

[계약 U4 결과](../../../devdoc/contracts/dev3-tree-smo.md#unlink-u4)의 UNL-N01..N06 native 실행 기록이다. controller
`scripts/check_dev3_unlink_native.py`가 debug test ELF 하나로 사례마다 process를 phase별로 돌렸다. 사례는 cut 여섯 ×
모양 둘(-L LeafUnlink, -R RootCollapse)의 12 이력이다. phase는 produce, recover, reenter이고, N06은 그 사이에 crash가
더 있다. 기대값은 동결 inventory `tests/fixtures/dev3-08-unlink/native-inventory.tsv`와 controller 자신의 규칙에서 온다.

- `run/execution-plan.json`: worker 이름, 사례, source·ELF·input·controller hash, 시간과 log 한도, 사례마다의 기대값.
- `run/results.json`: process마다의 종료 코드와 시간.
- `run/complete.json`: 사례마다의 결과, pid, WAL prefix와 cut이 더한 bytes, 복구 process의 log bytes.
- `run/<사례>/<phase>/receipt.tsv`: worker가 쓴 receipt. produce는 cut 위치, q, unlink 전 WAL prefix의 길이·SHA-256,
  complete LSN, bank high-water, 빈 leaf·root·anchor page다. recover·reenter는 자원 receipt, READY q, route·level·목록,
  high-water, B가 읽은 행 수, (recover만) ack q다.
- `run/<사례>/<phase>/files.json`: process가 끝난 뒤 namespace 파일마다의 크기와 SHA-256.
- `run/source-commit.txt`: 실행한 source commit.
- `review.md`: 독립 D2 검토 기록(최종 회차). `review-r1.md`는 1회차(CONDITIONAL, 필수 2) 기록이다.

source hash는 controller가 실행 때 `git ls-files --cached --others --exclude-standard`의 파일마다 SHA-256을 적은 manifest의
SHA-256이다. 실행 tree는 `run/`과 검토 기록을 더하기 전의 commit(`run/source-commit.txt`)과 같다. 그 commit에서 같은 manifest를 다시 만들면 hash가 같다. ELF,
namespace image, produce의 before image(pool arena·claim), effect trace, test log는 크기 때문에 남기지 않는다. controller가
실행 중에 effect trace와 WAL을 검사했고, image는 `files.json`의 hash로 남는다.
