#!/usr/bin/env python3
"""Literal fake slot-env-v1 perf, only for profile B control/cleanup tests; never PMU evidence."""
import os
import signal
import sys
import time

args = sys.argv[1:]
# Independent literal expectations, never imported from the collector.
assert args[:3] == ['stat', '-C', '2,18'] and '-A' in args and '--no-scale' in args
assert args[args.index('-e') + 1] == 'cycles,ref-cycles' and '--delay=-1' in args
behavior = os.environ.get('PROFILE_B_FAKE_PERF', 'normal')
counts = [int(v) for v in os.environ.get('PROFILE_B_FAKE_COUNTS', '1000,40,500,30').split(',')]
_, ctl, ack = args[args.index('--control') + 1].replace('fifo:', 'fifo,', 1).split(',')
stopping = False


def finish(_signal, _frame):
    global stopping
    stopping = True


signal.signal(signal.SIGINT, signal.SIG_IGN if behavior == 'ignore-sigint' else finish)
coverage = '50.00' if behavior == 'scaled' else '100.00'
enabled = False
with open(ctl) as commands, open(ack, 'wb', buffering=0) as replies:
    for command in commands:
        if command == 'enable\n' and behavior == 'lost':
            raise SystemExit(4)
        replies.write(b'ack\n')
        if command == 'disable\n' and enabled:
            while not stopping:
                time.sleep(.001)
            rows = [('CPU2', 'cycles'), ('CPU18', 'cycles'), ('CPU2', 'ref-cycles'), ('CPU18', 'ref-cycles')]
            for (cpu, event), count in zip(rows, counts):
                print(f'{cpu};{count};;{event};100000;{coverage};;', flush=True)
            break
        if command == 'enable\n':
            enabled = True
