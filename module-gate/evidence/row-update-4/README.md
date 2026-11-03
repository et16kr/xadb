# Row UPDATE 4단계 native fault cut 실행 기록

[계약 §18](../../../devdoc/contracts/dev3-row-update.md#implementation-4)의 RUP-N01..N06 native 실행 기록이다. controller `scripts/check_dev3_row_update_native.py`가 debug test ELF 하나로 사례마다 세 process(produce, recover, reenter)를 돌렸다. 기대값은 동결 inventory `tests/fixtures/module-row-update/native-inventory.json`에서 온다. 검사는 [Row DELETE 3단계 controller](../row-delete-3/README.md)와 같다.

- `run/execution-plan.json`: worker 이름, 사례, source·ELF·input hash, 시간과 log 한도.
- `run/results.json`, `run/complete.json`: process마다의 종료 코드·시간, 사례마다의 pid·ACK q·WAL prefix 짝과 "뒤" 사례가 더한 group의 바이트 수.
- `run/<사례>/<phase>/receipt.tsv`: worker가 쓴 receipt. produce는 cut의 결과·q·RowId와 WAL witness, recover·reenter는 자원 receipt, READY q, 복구 step·CLR, B가 읽은 행이다.
- `run/<사례>/<phase>/files.json`: process가 끝난 뒤 namespace 파일마다의 크기와 SHA-256.
- `run/source-commit.txt`: 실행한 commit.

source hash는 controller가 실행 때 `git ls-files --cached --others --exclude-standard`의 파일마다 SHA-256을 적은 manifest의 SHA-256이다. 실행 tree는 `run/source-commit.txt`의 commit과 같고, 그 commit의 git object로 같은 manifest를 다시 만들면 hash가 같다. ELF, namespace image, effect trace, test log는 크기 때문에 남기지 않는다. controller가 실행 중에 effect trace와 WAL을 검사했고, image는 `files.json`의 hash로 남는다.
