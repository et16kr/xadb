#!/usr/bin/env python3
"""Literal fake perf, only for new control/cleanup tests; never real PMU evidence."""
import json
from pathlib import Path
import sys
import signal
import time

args = sys.argv[1:]
assert '--no-scale' in args and '--no-inherit' in args
behavior = json.loads(Path.cwd().parent.joinpath('input.bin').read_text()).get('perf', 'normal')
if behavior in ('permission', 'restricted'):
    print('No permission to enable cycles:u event.' if behavior == 'permission' else 'No supported events found.\nAccess to performance monitoring and observability operations is limited.', file=sys.stderr)
    raise SystemExit(255)
_, ctl, ack = args[args.index('--control') + 1].replace('fifo:', 'fifo,', 1).split(',')
tid = int(args[args.index('--tid') + 1])
stopping = False
def finish(_sig, _frame):
    global stopping
    stopping = True
signal.signal(signal.SIGINT, signal.SIG_DFL if behavior == 'sigint-no-output' else finish)
with open(ctl) as commands, open(ack, 'wb', buffering=0) as replies:
    for command in commands:
        if command == 'enable\n' and behavior == 'lost':
            raise SystemExit(4)
        replies.write(b'ack\n')
        if command == 'disable\n' and getattr(sys, '_enabled', False):
            # Real perf waits on /proc existence, which includes zombies. It
            # must receive explicit finalization; never exit just on state Z.
            while not stopping:
                time.sleep(.001)
            assert Path(f'/proc/{tid}/stat').exists()
            if behavior == 'empty-finalization':
                break
            for index, event in enumerate(('cycles:u', 'instructions:u', 'ref-cycles:u', 'context-switches', 'cpu-migrations', 'page-faults')):
                print(f'{100 + index};;{event};100000;100.00;', flush=True)
            break
        if command == 'enable\n':
            sys._enabled = True
