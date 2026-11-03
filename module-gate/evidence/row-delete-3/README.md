# Row DELETE 3단계 native fault cut 실행 기록

[계약 §19](../../../devdoc/contracts/dev3-row-delete.md#implementation-3)의 RDL-N01..N06 native 실행 기록이다. controller `scripts/check_dev3_row_delete_native.py`가 debug test ELF 하나로 사례마다 세 process(produce, recover, reenter)를 돌렸다. 기대값은 동결 inventory `tests/fixtures/module-row-delete/native-inventory.json`에서 온다.

- `run/execution-plan.json`: worker 이름, 사례, source·ELF·input hash, 시간과 log 한도.
- `run/results.json`, `run/complete.json`: process마다의 종료 코드·시간, 사례마다의 pid·ACK q·WAL prefix 짝.
- `run/<사례>/<phase>/receipt.tsv`: worker가 쓴 receipt. produce는 cut의 결과·q·RowId와 WAL witness, recover·reenter는 자원 receipt, READY q, 복구 step·CLR, B가 읽은 행이다.
- `run/<사례>/<phase>/files.json`: process가 끝난 뒤 namespace 파일마다의 크기와 SHA-256.
- `review.md`: 독립 D2 검토 기록.

source hash는 controller가 실행 때 `git ls-files --cached --others --exclude-standard`의 파일마다 SHA-256을 적은 manifest의 SHA-256이다. 실행 tree는 이 기록을 더하기 전의 commit과 같다. 그 commit에서 같은 manifest를 다시 만들면 hash가 같다. ELF, namespace image, effect trace, test log는 크기 때문에 남기지 않는다. controller가 실행 중에 effect trace와 WAL을 검사했고, image는 `files.json`의 hash로 남는다.
