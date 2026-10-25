#!/usr/bin/env python3
"""Synthetic owned child for the diagnostic runner; no product engine use."""
import json
import os
import resource
import socket
import sys
import time

settings = json.load(sys.stdin)
behavior = settings.get('behavior', 'normal')
if behavior == 'exit':
    raise SystemExit(7)
if behavior == 'hang':
    time.sleep(120)
if behavior == 'overflow':
    os.write(1, b'x' * (5 * 1024 * 1024))
    time.sleep(120)
if behavior == 'descendant':
    if os.fork() == 0:
        time.sleep(120)
        os._exit(0)
    time.sleep(120)
token = os.environ['XADBTEST_WINDOW_TOKEN']
boundary = os.environ.get('XADBTEST_WINDOW_BOUNDARY', 'measurement')
with socket.socket(socket.AF_UNIX) as peer:
    peer.connect(os.environ['XADBTEST_WINDOW_SOCKET'])
    stream = peer.makefile('rwb', buffering=0)
    def send(line):
        stream.write((line + '\n').encode())
    def receive():
        return stream.readline(512).decode().strip()
    def usage():
        r = resource.getrusage(resource.RUSAGE_THREAD)
        return [int(r.ru_utime * 1e9), int(r.ru_stime * 1e9), r.ru_minflt, r.ru_majflt, r.ru_nvcsw, r.ru_nivcsw]
    source = settings['source']
    tid = os.getpid() + (1 if behavior == 'bad_identity' else 0)
    hello = f'HELLO 1 {token} {os.getpid()} {tid} {source} synthetic'
    if boundary != 'measurement':
        hello = f'HELLO 2 {token} {os.getpid()} {tid} {source} synthetic {boundary}'
    send(hello)
    ready = receive()
    assert ready.startswith(f'READY {token} '), ready
    mode = ready.split()[-1]
    if behavior == 'partial':
        stream.write(b'S')
        time.sleep(120)
    armed = None
    if boundary != 'measurement':
        send(f'{"WRONG" if behavior == "bad-arm" else "ARM"} {token}')
        assert receive() == f'ARMED {token} {mode}'
        armed = time.monotonic_ns()
        if behavior == 'warmup-fail':
            raise SystemExit(7)
        if behavior == 'warmup-hang':
            print('ARMED_WARMUP', flush=True)
            time.sleep(120)
        sum(v * v for v in range(10000))
    if boundary != 'prewarmup-continuous-v1':
        send(f'{"WRONG" if behavior == "bad-start" else "START"} {token}')
        assert receive() == f'GO {token} {mode}'
    if behavior == 'active-hang':
        # Privilege-path cancellation must occur after the actual perf attach.
        print('ACTIVE_WINDOW', flush=True)
        time.sleep(120)
    start = time.monotonic_ns()
    before = usage()
    measured = time.monotonic_ns()
    # Small actual CPU work followed by a voluntary switch.
    sum(v * v for v in range(10000))
    time.sleep(.002)
    elapsed = time.monotonic_ns() - measured
    after = usage()
    end = time.monotonic_ns()
    stop = dict(phase='STOP', token=token, start_ns=start, end_ns=end,
                elapsed_ns=elapsed, before=before, after=after)
    if armed is not None:
        stop['armed_ns'] = armed
    send(json.dumps(stop))
    assert receive() == f'DONE {token} {mode}'
print(json.dumps({'path': 'synthetic.diagnostic_window.v1', 'completed': True}))
