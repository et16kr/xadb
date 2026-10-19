#!/usr/bin/env python3
"""Run the full local COMM C-V18 profile with an independent socket peer."""
from __future__ import annotations

import argparse
import gzip
import hashlib
import itertools
import json
import math
import os
from pathlib import Path
import platform
import random
import selectors
import shutil
import socket
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from check_dev0 import check_report_paths, digest, source_fingerprint, write_report
from comm_campaign import capture_source

ROOT = Path(__file__).resolve().parent.parent
PAYLOAD_SIZES = (0, 64, 4096, 65536, 1048576)
CONNECTION_COUNTS = (1, 16, 128)
FAMILIES = ("tcp4", "tcp6", "unix")
WORKLOADS = ("ECHO", "BULK")
INSTRUMENTATION = ("ALWAYS", "DETAILED")
PAIRS = 10
WARMUP_SECONDS = 5
MEASURE_SECONDS = 30
LATENCY_LIMIT_RISE = 0.15
THROUGHPUT_LIMIT_DROP = 0.10
PAYLOAD_QUOTA_BYTES = 64 * 1024 * 1024
ECHO_ACTIVE_BUDGET_BYTES = 24 * 1024 * 1024
BULK_ACTIVE_BUDGET_BYTES = 56 * 1024 * 1024
SLOW_PEER_MAX_FRAMES = 65_536
SLOW_PEER_TARGET_BYTES = 8 * 1024 * 1024
SLOW_PEER_MIN_FRAMES = 8
UNIX_LARGE_FRAME_MIN_FRAMES = 6
SLOW_PEER_RECEIVE_BUFFER_BYTES = 64 * 1024
SLOW_PEER_START_SECONDS = WARMUP_SECONDS
SLOW_PEER_PAUSE_SECONDS = 25
MAX_ECHO_PENDING_TICKETS = 4
MAX_RECEIPT_BYTES = 128 * 1024


def wire(payload: bytes) -> bytes:
    body = b"AXF1\x01\x00" + len(payload).to_bytes(4, "little") + payload
    return body + hashlib.sha256(body).digest()


def payload_for_sequence(sequence: int, size: int) -> bytes:
    return bytes(((sequence * 37 + index * 31 + 7) & 0xFF) for index in range(size))


def slow_peer_frame_limit(payload_bytes: int) -> int:
    return max(
        SLOW_PEER_MIN_FRAMES,
        min(SLOW_PEER_MAX_FRAMES, SLOW_PEER_TARGET_BYTES // (payload_bytes + 42)),
    )


def slow_peer_pressure_prefix_minimum(family: str, payload_bytes: int) -> int:
    # On Linux AF_UNIX buffers can hold only six 1 MiB echoes across the
    # transport's bounded TX/RX slots before the independent reader stalls.
    # Keep the burst cap independent from the minimum pressure prefix.
    if family == "unix" and payload_bytes >= 1_048_576:
        return UNIX_LARGE_FRAME_MIN_FRAMES
    return SLOW_PEER_MIN_FRAMES


def active_peer_count(connections: int, payload_bytes: int, workload: str, slow_peer: bool = False) -> int:
    budget = ECHO_ACTIVE_BUDGET_BYTES if workload == "ECHO" else BULK_ACTIVE_BUDGET_BYTES
    wire_bytes = payload_bytes + 42
    if workload == "ECHO":
        if slow_peer:
            slow_frames = slow_peer_frame_limit(payload_bytes)
            budget = max(wire_bytes * 2, budget - slow_frames * payload_bytes * 2)
        wire_bytes *= 2
    return min(connections, max(1, budget // wire_bytes))


def inspect_wire(data: bytes) -> bytes:
    if len(data) < 42 or data[:6] != b"AXF1\x01\x00":
        raise ValueError("invalid AXF1 header")
    size = int.from_bytes(data[6:10], "little")
    if len(data) != size + 42 or hashlib.sha256(data[:-32]).digest() != data[-32:]:
        raise ValueError("invalid AXF1 length or digest")
    return data[10:-32]


def percentile(samples: list[int | float], fraction: float) -> int | float | None:
    if not samples:
        return None
    ordered = sorted(samples)
    index = max(0, math.ceil(fraction * len(ordered)) - 1)
    return ordered[index]


def bootstrap_mean_interval(values: list[float], seed: int, resamples: int = 10_000) -> dict[str, float] | None:
    if len(values) < PAIRS:
        return None
    generator = random.Random(seed)
    count = len(values)
    means = [
        sum(values[generator.randrange(count)] for _ in range(count)) / count
        for _ in range(resamples)
    ]
    return {
        "lower_95": float(percentile(means, 0.025)),
        "upper_95": float(percentile(means, 0.975)),
    }


def matrix() -> list[dict[str, object]]:
    return [
        {"family": family, "connections": connections, "payload_bytes": size, "workload": workload, "slow_peer": False}
        for family, connections, size, workload in itertools.product(
            FAMILIES, CONNECTION_COUNTS, PAYLOAD_SIZES, WORKLOADS
        )
    ]


def slow_peer_matrix() -> list[dict[str, object]]:
    return [
        {"family": family, "connections": connections, "payload_bytes": size, "workload": "ECHO", "slow_peer": True}
        for family, connections, size in itertools.product(FAMILIES, CONNECTION_COUNTS, PAYLOAD_SIZES)
    ]


def read_line(process: subprocess.Popen[bytes], timeout: float) -> dict[str, object]:
    if process.stdout is None:
        raise ValueError("worker stdout is unavailable")
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    value = getattr(process, "_comm_line_buffer", bytearray())
    deadline = time.monotonic() + timeout
    try:
        while True:
            newline = value.find(b"\n")
            if newline >= 0:
                line = bytes(value[:newline])
                del value[: newline + 1]
                setattr(process, "_comm_line_buffer", value)
                return json.loads(line)
            if len(value) >= MAX_RECEIPT_BYTES:
                raise ValueError("worker receipt exceeded bounded limit")
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not selector.select(remaining):
                raise TimeoutError("worker receipt deadline")
            chunk = os.read(process.stdout.fileno(), min(16 * 1024, MAX_RECEIPT_BYTES - len(value)))
            if not chunk:
                if value:
                    raise ValueError("worker exited with an unterminated receipt")
                raise ValueError("worker exited before receipt")
            value.extend(chunk)
    finally:
        setattr(process, "_comm_line_buffer", value)
        selector.close()


def proc_snapshot(pid: int) -> dict[str, int]:
    try:
        stat = Path(f"/proc/{pid}/stat").read_text()
        status = Path(f"/proc/{pid}/status").read_text()
    except FileNotFoundError as error:
        raise RuntimeError(f"worker {pid} exited before the measurement snapshot") from error
    fields = stat[stat.rfind(")") + 2 :].split()
    if len(fields) < 22:
        raise ValueError("short /proc stat")
    high_water = next((line for line in status.splitlines() if line.startswith("VmHWM:")), None)
    if high_water is None:
        raise RuntimeError(f"worker {pid} exited before VmHWM could be sampled")
    high_water_kib = int(high_water.split()[1])
    return {
        # After stripping pid/comm, state is field 0; utime/stime are
        # original fields 14/15 and therefore indices 10/11 here.
        "cpu_ticks": int(fields[10]) + int(fields[11]),
        "rss_high_water_kib": high_water_kib,
    }


class Peer:
    def __init__(self, sock: socket.socket, payload_bytes: int, workload: str, measure_start: float, measure_end: float, index: int, active: bool, slow_reader: bool):
        self.sock = sock
        self.payload_bytes = payload_bytes
        self.workload = workload
        self.measure_start = measure_start
        self.measure_end = measure_end
        self.offset = 0
        self.frame_started = False
        self.awaiting_response = False
        self.sent_at_ns: int | None = None
        self.received = bytearray()
        self.closed = False
        self.latencies_ns: list[int] = []
        self.frames_per_second = [0] * MEASURE_SECONDS
        self.bytes_per_second = [0] * MEASURE_SECONDS
        self.socket_frames = 0
        self.socket_payload_bytes = 0
        self.echo_responses_observed = 0
        self.active = active
        self.index = index
        self.slow_reader = slow_reader
        self.slow_frames_sent = 0
        self.slow_write_blocked_since: float | None = None
        self.slow_burst_stopped = False
        self.slow_burst_stop_reason: str | None = None
        self.slow_partial_frame_bytes = 0
        self.cancel_target = False
        self.cancel_observed_latency_seconds: float | None = None
        self.sequence = 0
        self.response_sequence = 0
        self.packet = wire(payload_for_sequence(self.sequence, payload_bytes))
        self.last_progress = time.monotonic()

    def advance(self) -> None:
        self.sequence += 1
        self.packet = wire(payload_for_sequence(self.sequence, self.payload_bytes))


def modify_or_register(selector: selectors.BaseSelector, peer: Peer, events: int) -> None:
    try:
        selector.modify(peer.sock, events, peer)
    except KeyError:
        selector.register(peer.sock, events, peer)


def add_measurement(peer: Peer, at: float) -> None:
    if not peer.measure_start <= at < peer.measure_end:
        return
    second = int(at - peer.measure_start)
    if 0 <= second < MEASURE_SECONDS:
        peer.frames_per_second[second] += 1
        peer.bytes_per_second[second] += len(peer.packet) - 42


def close_peer(selector: selectors.BaseSelector, peer: Peer) -> None:
    if peer.closed:
        return
    try:
        selector.unregister(peer.sock)
    except (KeyError, ValueError):
        pass
    peer.closed = True
    peer.sock.close()


def connect_peers(
    family: str, ready: dict[str, object], count: int, slow_peer: bool = False,
) -> list[socket.socket]:
    result = []
    if family == "unix":
        address = str(ready["unix"])
        socket_family = socket.AF_UNIX
    else:
        host, port = str(ready["tcp"]).rsplit(":", 1)
        address = (host.strip("[]"), int(port))
        socket_family = socket.AF_INET if family == "tcp4" else socket.AF_INET6
    for index in range(count):
        sock = socket.socket(socket_family)
        if slow_peer and index == 0:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, SLOW_PEER_RECEIVE_BUFFER_BYTES)
        sock.settimeout(5)
        sock.connect(address)
        sock.setblocking(False)
        result.append(sock)
    return result


def prepare_peer_window(
    sockets: list[socket.socket], workload: str, payload_bytes: int, slow_peer: bool,
    measured_peer_count: int | None = None,
) -> tuple[selectors.BaseSelector, list[Peer], int, list[dict[str, object]]]:
    selector = selectors.DefaultSelector()
    measured_peer_count = len(sockets) if measured_peer_count is None else measured_peer_count
    active_count = active_peer_count(measured_peer_count, payload_bytes, workload, slow_peer)
    peers = [
        Peer(
            sock, payload_bytes, workload, 0.0, 0.0, index,
            index < active_count, slow_peer and index == 0,
        )
        for index, sock in enumerate(sockets)
    ]
    peers[-1].cancel_target = True
    socket_settings = [
        {
            "family": peer.sock.family,
            "send_buffer_bytes": peer.sock.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF),
            "receive_buffer_bytes": peer.sock.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF),
            "tcp_nodelay": peer.sock.getsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY)
            if peer.sock.family in (socket.AF_INET, socket.AF_INET6) else None,
        }
        for peer in peers
    ]
    for peer in peers:
        if peer.active and not peer.slow_reader:
            selector.register(peer.sock, selectors.EVENT_WRITE, peer)
    return selector, peers, active_count, socket_settings


def run_peer_window(
    prepared: tuple[selectors.BaseSelector, list[Peer], int, list[dict[str, object]]],
    workload: str, payload_bytes: int, pid: int, start_requested: float, slow_peer: bool,
) -> dict[str, object]:
    selector, peers, active_count, socket_settings = prepared
    measure_start = start_requested + WARMUP_SECONDS
    measure_end = measure_start + MEASURE_SECONDS
    for peer in peers:
        peer.measure_start = measure_start
        peer.measure_end = measure_end
        peer.last_progress = start_requested + SLOW_PEER_START_SECONDS if peer.slow_reader else start_requested
    worker_measure_start = None
    worker_measure_end = None
    controller_measure_start = None
    controller_measure_end = None
    drain_deadline = measure_end + 5.0
    preserve_cancel_target = False
    slow_start_at = start_requested + SLOW_PEER_START_SECONDS
    slow_stop_at = slow_start_at + SLOW_PEER_PAUSE_SECONDS
    try:
        while True:
            now = time.monotonic()
            if slow_peer and now >= slow_start_at:
                slow_owner = peers[0]
                if slow_owner.active and not slow_owner.closed and not slow_owner.slow_burst_stopped:
                    try:
                        selector.get_key(slow_owner.sock)
                    except KeyError:
                        selector.register(slow_owner.sock, selectors.EVENT_WRITE, slow_owner)
            if worker_measure_start is None and now >= measure_start:
                worker_measure_start = proc_snapshot(pid)
                controller_measure_start = proc_snapshot(os.getpid())
            if now >= measure_end and worker_measure_end is None:
                worker_measure_end = proc_snapshot(pid)
                controller_measure_end = proc_snapshot(os.getpid())
            if workload == "BULK":
                for peer in peers:
                    if not peer.closed and now >= measure_end and not peer.frame_started:
                        if peer.cancel_target:
                            if not peer.slow_reader:
                                modify_or_register(selector, peer, selectors.EVENT_READ)
                        else:
                            close_peer(selector, peer)
                if now >= drain_deadline:
                    unfinished = [peer for peer in peers if not peer.closed and peer.frame_started]
                    if unfinished:
                        raise TimeoutError("bulk peer could not finish a started frame before the drain deadline")
                    for peer in peers:
                        close_peer(selector, peer)
            else:
                for peer in peers:
                    if slow_peer and peer.slow_reader and now >= slow_stop_at and not peer.closed:
                        peer.slow_partial_frame_bytes = peer.offset
                        close_peer(selector, peer)
                        continue
                    if not peer.closed and now >= measure_end and not peer.awaiting_response and not peer.frame_started:
                        if peer.cancel_target:
                            if not peer.slow_reader:
                                modify_or_register(selector, peer, selectors.EVENT_READ)
                        elif peer.slow_reader:
                            close_peer(selector, peer)
                        else:
                            close_peer(selector, peer)
                if now >= drain_deadline:
                    unfinished = [
                        peer for peer in peers
                        if not peer.closed and (peer.frame_started or peer.awaiting_response or peer.received)
                    ]
                    if unfinished:
                        raise TimeoutError("echo peer did not finish its outstanding frame before the drain deadline")
                    for peer in peers:
                        close_peer(selector, peer)
            cancel_target = peers[-1]
            cancel_target_ready = (
                now >= measure_end
                and not cancel_target.closed
                and not cancel_target.frame_started
                and not cancel_target.awaiting_response
                and not cancel_target.received
            )
            if cancel_target_ready and all(peer.closed for peer in peers[:-1]):
                preserve_cancel_target = True
                break
            for peer in peers:
                if not peer.active or peer.closed:
                    continue
                if peer.slow_reader and peer.slow_frames_sent >= slow_peer_frame_limit(payload_bytes):
                    continue
                if peer.slow_reader and peer.slow_burst_stopped:
                    continue
                if peer.slow_reader and now < slow_start_at:
                    continue
                if peer.slow_reader and peer.slow_write_blocked_since is not None:
                    if now - peer.slow_write_blocked_since >= 0.5:
                        peer.slow_burst_stopped = True
                        peer.slow_burst_stop_reason = "socket_backpressure"
                        peer.slow_partial_frame_bytes = peer.offset
                        try:
                            selector.unregister(peer.sock)
                        except (KeyError, ValueError):
                            pass
                        continue
                if peer.slow_reader and peer.slow_frames_sent >= slow_peer_pressure_prefix_minimum(
                    "unix" if peer.sock.family == socket.AF_UNIX else "tcp", payload_bytes
                ):
                    if now - peer.last_progress >= 0.5:
                        peer.slow_burst_stopped = True
                        peer.slow_burst_stop_reason = "socket_backpressure"
                        peer.slow_partial_frame_bytes = peer.offset
                        try:
                            selector.unregister(peer.sock)
                        except (KeyError, ValueError):
                            pass
                        continue
                if now - peer.last_progress > 5.0:
                    raise TimeoutError(f"peer {peer.index} made no I/O progress for five seconds")
            timeout = min(0.02, max(0.0, measure_end - now)) if now < measure_end else 0.02
            for key, mask in selector.select(timeout):
                peer: Peer = key.data
                now = time.monotonic()
                if mask & selectors.EVENT_WRITE and peer.workload == "BULK":
                    if now >= measure_end and not peer.frame_started:
                        if peer.cancel_target:
                            modify_or_register(selector, peer, selectors.EVENT_READ)
                        else:
                            close_peer(selector, peer)
                        continue
                    try:
                        sent = peer.sock.send(peer.packet[peer.offset :])
                    except BlockingIOError:
                        if peer.slow_reader and peer.slow_frames_sent >= slow_peer_pressure_prefix_minimum(
                            "unix" if peer.sock.family == socket.AF_UNIX else "tcp", payload_bytes
                        ):
                            if peer.slow_write_blocked_since is None:
                                peer.slow_write_blocked_since = time.monotonic()
                        continue
                    if sent <= 0:
                        raise ValueError("bulk peer made no write progress")
                    peer.last_progress = time.monotonic()
                    if peer.slow_reader:
                        peer.slow_write_blocked_since = None
                    peer.frame_started = True
                    peer.offset += sent
                    if peer.offset == len(peer.packet):
                        completed = time.monotonic()
                        peer.offset = 0
                        peer.frame_started = False
                        peer.socket_frames += 1
                        peer.socket_payload_bytes += len(peer.packet) - 42
                        add_measurement(peer, completed)
                        peer.advance()
                        if completed >= measure_end:
                            if peer.cancel_target:
                                modify_or_register(selector, peer, selectors.EVENT_READ)
                            else:
                                close_peer(selector, peer)
                elif mask & selectors.EVENT_WRITE:
                    if now >= measure_end and not peer.frame_started:
                        if peer.cancel_target:
                            modify_or_register(selector, peer, selectors.EVENT_READ)
                        else:
                            close_peer(selector, peer)
                        continue
                    starting_frame = not peer.frame_started
                    try:
                        sent = peer.sock.send(peer.packet[peer.offset :])
                    except BlockingIOError:
                        if peer.slow_reader and peer.slow_frames_sent >= slow_peer_pressure_prefix_minimum(
                            "unix" if peer.sock.family == socket.AF_UNIX else "tcp", payload_bytes
                        ):
                            if peer.slow_write_blocked_since is None:
                                peer.slow_write_blocked_since = time.monotonic()
                        continue
                    if sent <= 0:
                        raise ValueError("echo peer made no write progress")
                    peer.last_progress = time.monotonic()
                    if peer.slow_reader:
                        peer.slow_write_blocked_since = None
                    if starting_frame:
                        peer.sent_at_ns = time.monotonic_ns()
                    peer.frame_started = True
                    peer.offset += sent
                    if peer.offset == len(peer.packet):
                        peer.offset = 0
                        peer.frame_started = False
                        peer.socket_frames += 1
                        peer.socket_payload_bytes += len(peer.packet) - 42
                        if peer.slow_reader:
                            peer.slow_frames_sent += 1
                            peer.sent_at_ns = None
                            peer.advance()
                            target_frames = slow_peer_frame_limit(peer.payload_bytes)
                            if peer.slow_frames_sent >= target_frames:
                                peer.slow_burst_stopped = True
                                peer.slow_burst_stop_reason = "frame_limit"
                                selector.unregister(peer.sock)
                        else:
                            peer.awaiting_response = True
                            selector.modify(peer.sock, selectors.EVENT_READ, peer)
                elif mask & selectors.EVENT_READ:
                    try:
                        chunk = peer.sock.recv(max(1, len(peer.packet) - len(peer.received)))
                    except BlockingIOError:
                        continue
                    if not chunk:
                        if peer.cancel_target and now >= measure_end:
                            raise ValueError("cancellation target closed before the explicit cancel request")
                        if peer.awaiting_response:
                            raise ValueError("peer EOF before complete echoed frame")
                        close_peer(selector, peer)
                        continue
                    peer.last_progress = time.monotonic()
                    peer.received.extend(chunk)
                    if len(peer.received) > len(peer.packet):
                        raise ValueError("peer received extra echo bytes")
                    if len(peer.received) == len(peer.packet):
                        observed = bytes(peer.received)
                        response_packet = wire(payload_for_sequence(peer.response_sequence, peer.payload_bytes))
                        if observed != response_packet or inspect_wire(observed) != response_packet[10:-32]:
                            raise ValueError("independent echo oracle mismatch")
                        completed = time.monotonic()
                        if (
                            peer.sent_at_ns is not None
                            and measure_start <= peer.sent_at_ns / 1e9
                        ):
                            peer.latencies_ns.append(time.monotonic_ns() - peer.sent_at_ns)
                        add_measurement(peer, completed)
                        peer.echo_responses_observed += 1
                        peer.response_sequence += 1
                        peer.awaiting_response = False
                        peer.sent_at_ns = None
                        peer.received.clear()
                        if not peer.slow_reader:
                            peer.advance()
                        if completed >= measure_end:
                            if peer.cancel_target:
                                modify_or_register(selector, peer, selectors.EVENT_READ)
                            else:
                                close_peer(selector, peer)
                        else:
                            selector.modify(peer.sock, selectors.EVENT_WRITE, peer)
    finally:
        for peer in peers:
            if not (preserve_cancel_target and peer is peers[-1] and not peer.closed):
                close_peer(selector, peer)
        selector.close()
    latency_samples = [sample for peer in peers for sample in peer.latencies_ns]
    return {
        "socket_frames_fully_written": sum(peer.socket_frames for peer in peers),
        "socket_payload_bytes_fully_written": sum(peer.socket_payload_bytes for peer in peers),
        "echo_responses_fully_observed": sum(peer.echo_responses_observed for peer in peers),
        "socket_settings": socket_settings,
        "active_peer_count": active_count,
        "established_peer_count": len(peers),
        "slow_peer": slow_peer,
        "slow_peer_minimum_pressure_prefix_frames": (
            slow_peer_pressure_prefix_minimum(
                "unix" if peers[0].sock.family == socket.AF_UNIX else "tcp", payload_bytes
            ) if slow_peer else None
        ),
        "slow_peer_burst_frame_limit": slow_peer_frame_limit(payload_bytes) if slow_peer else None,
        "slow_peer_offered_frames": sum(peer.slow_frames_sent for peer in peers),
        "slow_peer_burst_stop_reason": next(
            (peer.slow_burst_stop_reason for peer in peers if peer.slow_reader), None
        ),
        "slow_peer_partial_frame_bytes": sum(peer.slow_partial_frame_bytes for peer in peers),
        "socket_frames_fully_written_by_peer": [peer.socket_frames for peer in peers],
        "echo_responses_observed_by_peer": [peer.echo_responses_observed for peer in peers],
        "cancel_target_peer_index": peers[-1].index,
        "cancel_latency_seconds": None,
        "measured_frames_per_second": [sum(p.frames_per_second[i] for p in peers) for i in range(MEASURE_SECONDS)],
        "measured_payload_bytes_per_second": [sum(p.bytes_per_second[i] for p in peers) for i in range(MEASURE_SECONDS)],
        "latency_samples_ns": latency_samples,
        "latency_p50_ns": percentile(latency_samples, 0.50),
        "latency_p95_ns": percentile(latency_samples, 0.95),
        "latency_p99_ns": percentile(latency_samples, 0.99),
        "worker_cpu_ticks_start": None if worker_measure_start is None else worker_measure_start["cpu_ticks"],
        "worker_cpu_ticks_end": None if worker_measure_end is None else worker_measure_end["cpu_ticks"],
        "worker_cpu_seconds": None if worker_measure_start is None or worker_measure_end is None else (
            worker_measure_end["cpu_ticks"] - worker_measure_start["cpu_ticks"]
        ) / os.sysconf("SC_CLK_TCK"),
        "worker_cpu_percent_one_core": None if worker_measure_start is None or worker_measure_end is None else 100.0 * (
            worker_measure_end["cpu_ticks"] - worker_measure_start["cpu_ticks"]
        ) / os.sysconf("SC_CLK_TCK") / MEASURE_SECONDS,
        "worker_rss_high_water_kib": None if worker_measure_end is None else worker_measure_end["rss_high_water_kib"],
        "controller_cpu_ticks_start": None if controller_measure_start is None else controller_measure_start["cpu_ticks"],
        "controller_cpu_ticks_end": None if controller_measure_end is None else controller_measure_end["cpu_ticks"],
        "controller_cpu_seconds": None if controller_measure_start is None or controller_measure_end is None else (
            controller_measure_end["cpu_ticks"] - controller_measure_start["cpu_ticks"]
        ) / os.sysconf("SC_CLK_TCK"),
        "controller_cpu_percent_one_core": None if controller_measure_start is None or controller_measure_end is None else 100.0 * (
            controller_measure_end["cpu_ticks"] - controller_measure_start["cpu_ticks"]
        ) / os.sysconf("SC_CLK_TCK") / MEASURE_SECONDS,
        "controller_rss_high_water_kib": None if controller_measure_end is None else controller_measure_end["rss_high_water_kib"],
    }


def wait_for_cancel_eof(
    sock: socket.socket, requested_at: float, timeout: float, workload: str,
    payload_bytes: int, allow_buffered_echo: bool,
) -> dict[str, object]:
    selector = selectors.DefaultSelector()
    selector.register(sock, selectors.EVENT_READ)
    buffer = bytearray()
    full_frames = 0
    received_bytes = 0
    deadline = requested_at + timeout
    try:
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not selector.select(remaining):
                raise TimeoutError("cancellation EOF was not observed within one second")
            chunk = sock.recv(64 * 1024)
            if not chunk:
                break
            received_bytes += len(chunk)
            if workload != "ECHO" or not allow_buffered_echo:
                raise ValueError("unexpected bytes arrived after the measurement window")
            buffer.extend(chunk)
            while len(buffer) >= 10:
                if buffer[:6] != b"AXF1\x01\x00":
                    raise ValueError("invalid buffered echo header after cancellation")
                payload_size = int.from_bytes(buffer[6:10], "little")
                if payload_size != payload_bytes:
                    raise ValueError("buffered echo payload length changed after cancellation")
                frame_size = payload_size + 42
                if len(buffer) < frame_size:
                    break
                frame = bytes(buffer[:frame_size])
                expected = wire(payload_for_sequence(full_frames, payload_bytes))
                if frame != expected or inspect_wire(frame) != expected[10:-32]:
                    raise ValueError("independent buffered echo oracle mismatch after cancellation")
                del buffer[:frame_size]
                full_frames += 1
            if len(buffer) > payload_bytes + 42:
                raise ValueError("buffered cancellation echo exceeded one partial frame")
        partial_bytes = len(buffer)
        if partial_bytes and not allow_buffered_echo:
            raise ValueError("partial bytes arrived after a normal cancellation")
        if partial_bytes:
            expected = wire(payload_for_sequence(full_frames, payload_bytes))
            if len(buffer) >= len(expected) or buffer != expected[:len(buffer)]:
                raise ValueError("invalid partial echo frame at cancellation EOF")
        return {
            "latency_seconds": time.monotonic() - requested_at,
            "received_bytes": received_bytes,
            "full_echo_frames": full_frames,
            "partial_frame_bytes_at_eof": partial_bytes,
        }
    finally:
        selector.close()


def run_case(binary: Path, directory: Path, cell: dict[str, object], instrumentation: str, source_hash: str) -> dict[str, object]:
    family = str(cell["family"])
    workload = str(cell["workload"])
    connection_count = int(cell["connections"])
    payload_bytes = int(cell["payload_bytes"])
    slow_peer = cell.get("slow_peer", False)
    if not isinstance(slow_peer, bool):
        raise ValueError("slow_peer flag must be boolean")
    worker_connection_count = connection_count + (1 if slow_peer and connection_count == 1 else 0)
    directory.mkdir(parents=True)
    argv = [
        str(binary), family, instrumentation, workload, str(worker_connection_count),
        str(payload_bytes), "SLOW" if slow_peer else "NORMAL",
    ]
    unix_temp = None
    if family == "unix":
        unix_temp = tempfile.TemporaryDirectory(prefix="xcp-")
        argv.append(str(Path(unix_temp.name) / "private"))
    sockets: list[socket.socket] = []
    process: subprocess.Popen[bytes] | None = None
    try:
        process = subprocess.Popen(
            argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0
        )
        ready = read_line(process, 10)
        if ready.get("event") != "ready" or ready.get("source_sha256") != source_hash or ready.get("profile") != "release" or ready.get("pid") != process.pid:
            raise ValueError("wrong worker build/source identity")
        if (ready.get("family"), ready.get("workload"), ready.get("instrumentation"), ready.get("expected_connections")) != (
            family, workload, instrumentation, worker_connection_count
        ):
            raise ValueError("worker profile disagrees with requested cell")
        if ready.get("payload_bytes") != payload_bytes or ready.get("slow_peer") is not slow_peer:
            raise ValueError("worker payload size disagrees with requested cell")
        sockets = connect_peers(family, ready, worker_connection_count, slow_peer)
        armed = read_line(process, 10)
        if armed.get("event") != "armed":
            raise ValueError("worker did not arm after accepting all peers")
        peer_preparation_start = time.monotonic()
        prepared_peer_window = prepare_peer_window(
            sockets, workload, payload_bytes, slow_peer, measured_peer_count=connection_count
        )
        peer_preparation_seconds = time.monotonic() - peer_preparation_start
        start_requested = time.monotonic()
        if process.stdin is None:
            raise ValueError("worker stdin is unavailable for the start handshake")
        process.stdin.write(b"start\n")
        process.stdin.flush()
        started = read_line(process, 10)
        if started.get("event") != "started":
            raise ValueError("missing worker start receipt")
        start_ack_delay = time.monotonic() - start_requested
        peer_result = run_peer_window(
            prepared_peer_window, workload, payload_bytes, process.pid, start_requested, slow_peer
        )
        peer_result["peer_preparation_seconds"] = peer_preparation_seconds
        peer_result["measured_peer_count"] = connection_count
        peer_result["cancellation_helper_peer_count"] = worker_connection_count - connection_count
        target_index = int(peer_result["cancel_target_peer_index"])
        target_offered_frames = int(peer_result["socket_frames_fully_written_by_peer"][target_index])
        if process.stdin is None:
            raise ValueError("worker stdin is unavailable for the cancellation quiesce command")
        process.stdin.write(f"quiesce {target_offered_frames}\n".encode())
        process.stdin.flush()
        cancel_ready = read_line(process, 10)
        if (
            cancel_ready.get("event") != "cancel-ready"
            or cancel_ready.get("connection_index") != target_index
            or cancel_ready.get("phase_before") != "Open"
            or cancel_ready.get("terminal_error_before") is not None
            or cancel_ready.get("received_frames") != target_offered_frames
        ):
            raise ValueError("worker did not report a live cancellation target")
        if process.stdin is None:
            raise ValueError("worker stdin is unavailable for the cancellation command")
        cancel_requested = time.monotonic()
        process.stdin.write(b"cancel\n")
        process.stdin.flush()
        process.stdin.close()
        cancelled = read_line(process, 2)
        if (
            cancelled.get("event") != "cancelled"
            or cancelled.get("connection_index") != target_index
            or cancelled.get("phase_before") != "Open"
            or cancelled.get("phase_after") != "Closed"
            or cancelled.get("terminal_error_kind") != "Cancelled"
        ):
            raise ValueError("worker cancellation did not produce the Cancelled terminal reason")
        cancel_eof = wait_for_cancel_eof(
            sockets[-1], cancel_requested, 1.0, workload, payload_bytes,
            False,
        )
        sockets[-1].close()
        peer_result["cancel_latency_seconds"] = cancel_eof["latency_seconds"]
        peer_result["cancel_eof"] = cancel_eof
        closed = read_line(process, 10)
        if closed.get("event") != "closed" or closed.get("source_sha256") != source_hash:
            raise ValueError("missing worker close receipt or source mismatch")
        stdout, stderr = process.communicate(timeout=5)
        if process.returncode != 0:
            raise ValueError(f"worker exited {process.returncode}: {stderr.decode(errors='replace')[-2000:]}")
        totals = closed.get("totals", {})
        if int(closed.get("connections", 0)) != worker_connection_count or closed.get("cleanup") != {"payload_bytes": 0, "connection_count": 0}:
            raise ValueError("worker cleanup or connection accounting failed")
        failed_tickets = int(totals.get("failed_tickets", 0))
        allowed_failed_tickets = MAX_ECHO_PENDING_TICKETS if workload == "ECHO" and slow_peer else 0
        if totals.get("overflowed") or failed_tickets > allowed_failed_tickets:
            raise ValueError(
                "worker reported counter overflow or too many failed send tickets: "
                f"failed={failed_tickets}, allowed_cancellation_residue={allowed_failed_tickets}"
            )
        offered_frames = int(peer_result["socket_frames_fully_written"])
        received_frames = int(totals.get("received_frames", -1))
        if (not slow_peer and offered_frames != received_frames) or (slow_peer and received_frames > offered_frames):
            raise ValueError("independent peer and worker frame counts differ")
        offered_bytes = int(peer_result["socket_payload_bytes_fully_written"])
        received_bytes = int(totals.get("received_payload_bytes", -1))
        if (not slow_peer and offered_bytes != received_bytes) or (slow_peer and received_bytes > offered_bytes):
            raise ValueError("independent peer and worker payload byte counts differ")
        connection_details = closed.get("connection_details")
        if not isinstance(connection_details, list) or len(connection_details) != worker_connection_count:
            raise ValueError("worker omitted per-connection cancellation and pressure accounting")
        if workload == "ECHO":
            for index, detail in enumerate(connection_details):
                if not isinstance(detail, dict) or detail.get("connection_index") != index:
                    raise ValueError("worker connection accounting order changed")
                rx_frames = int(detail.get("received_frames", -1))
                sent_frames = int(detail.get("sent_frames", -1))
                flush_frames = int(detail.get("flushed_frames", -1))
                failed = int(detail.get("failed_tickets", -1))
                dropped = int(detail.get("dropped_echo_responses", -1))
                if rx_frames != sent_frames + dropped or sent_frames != flush_frames + failed:
                    raise ValueError(f"echo frame accounting mismatch for connection {index}")
                is_slow_owner = slow_peer and index == 0
                if not is_slow_owner and (failed != 0 or dropped != 0):
                    raise ValueError(f"unexpected echo cancellation residue for connection {index}")
                if is_slow_owner and failed > MAX_ECHO_PENDING_TICKETS:
                    raise ValueError("slow peer left more failed echo tickets than the bounded queue")
            if slow_peer:
                slow_detail = connection_details[0]
                offered_by_peer = peer_result["socket_frames_fully_written_by_peer"]
                if int(offered_by_peer[0]) < slow_peer_pressure_prefix_minimum(family, payload_bytes):
                    raise ValueError("slow peer did not send the minimum pressure prefix")
                if peer_result["slow_peer_burst_stop_reason"] not in {"socket_backpressure", "frame_limit"}:
                    raise ValueError("slow peer did not report a bounded burst stop reason")
                if (
                    int(slow_detail.get("pending_echo_backlog_peak", 0)) <= 0
                    or int(slow_detail.get("ticket_cap_saturations", 0)) <= 0
                    or int(slow_detail.get("would_block", 0)) <= 0
                    or int(slow_detail.get("peak_tx_queue_bytes", 0)) <= 0
                ):
                    raise ValueError("slow peer did not reach native TX queue/socket backpressure")
        if workload == "ECHO":
            observed = int(peer_result["echo_responses_fully_observed"])
            flushed = int(totals.get("flushed_frames", -1))
            if observed > flushed or (not slow_peer and observed != flushed):
                raise ValueError("independent peer and local echo flush accounting differs")
        if int(closed.get("diagnostics", {}).get("peak_payload_bytes", 0)) > PAYLOAD_QUOTA_BYTES:
            raise ValueError("transport peak payload exceeded its quota")
        cancel_latency = peer_result.get("cancel_latency_seconds")
        if not isinstance(cancel_latency, (int, float)) or not 0 <= cancel_latency < 1.0:
            raise ValueError("native cancellation did not reach the peer within one second")
        samples = peer_result.pop("latency_samples_ns")
        latency_file = directory / "latency-samples.json.gz"
        latency_file.write_bytes(gzip.compress(json.dumps(samples, separators=(",", ":")).encode()))
        record = {
            "status": "PASS",
            "cell": cell,
            "instrumentation": instrumentation,
            "source_sha256": source_hash,
            "command": argv,
            "worker_ready": ready,
            "peer": peer_result,
            "worker": closed,
            "cancelled": cancelled,
            "allowed_cancellation_failed_tickets": allowed_failed_tickets,
            "latency_samples": {
                "path": latency_file.name,
                "sha256": hashlib.sha256(latency_file.read_bytes()).hexdigest(),
                "count": len(samples),
            },
            "peer_started": started,
            "controller_start_requested_monotonic": start_requested,
            "start_ack_delay_seconds": start_ack_delay,
            "offered_admitted_completed_dropped": {
                "offered_frames": int(peer_result["socket_frames_fully_written"]),
                "offered_payload_bytes": int(peer_result["socket_payload_bytes_fully_written"]),
                "admitted_frames": int(totals["received_frames"]),
                "admitted_payload_bytes": int(totals["received_payload_bytes"]),
                "completed_frames": int(
                    peer_result["echo_responses_fully_observed"] if workload == "ECHO"
                    else totals["received_frames"]
                ),
                "dropped_frames": int(peer_result["socket_frames_fully_written"])
                - int(totals["received_frames"]),
            },
            "worker_stderr": stderr.decode(errors="replace"),
            "worker_exit_code": process.returncode,
            "measured_seconds": MEASURE_SECONDS,
            "slow_peer_pause_seconds": SLOW_PEER_PAUSE_SECONDS if slow_peer else None,
            "warmup_seconds": WARMUP_SECONDS,
        }
        return record
    except BaseException as error:
        for sock in sockets:
            try:
                sock.close()
            except OSError:
                pass
        if process is not None and process.poll() is None:
            process.kill()
        stderr = b""
        if process is not None:
            _, stderr = process.communicate(timeout=5)
        record = {
            "status": "FAIL",
            "cell": cell,
            "instrumentation": instrumentation,
            "source_sha256": source_hash,
            "command": argv,
            "error": str(error),
            "worker_exit_code": None if process is None else process.returncode,
            "worker_stderr": stderr.decode(errors="replace"),
        }
        write_report(directory / "validation.json", record)
        raise
    finally:
        if unix_temp is not None:
            unix_temp.cleanup()


def summarize_pairs(
    records: list[dict[str, object]], expected_cells: int, expected_pairs: int,
    expected_profile: list[dict[str, object]] | None = None,
) -> list[dict[str, object]]:
    grouped: dict[tuple[object, ...], dict[str, dict[str, object]]] = {}
    seen = set()
    allowed_cells = None if expected_profile is None else {
        tuple(cell[name] for name in ("family", "connections", "payload_bytes", "workload", "slow_peer"))
        for cell in expected_profile
    }
    if allowed_cells is not None and len(allowed_cells) != expected_cells:
        raise ValueError("expected profile does not contain the declared number of unique cells")
    for record in records:
        cell = record["cell"]
        assert isinstance(cell, dict)
        cell_key = tuple(cell[name] for name in ("family", "connections", "payload_bytes", "workload", "slow_peer"))
        if allowed_cells is not None and cell_key not in allowed_cells:
            raise ValueError(f"unexpected performance cell: {cell_key}")
        if record.get("pair") not in range(1, expected_pairs + 1):
            raise ValueError(f"invalid pair index: {record.get('pair')}")
        if record.get("instrumentation") not in INSTRUMENTATION:
            raise ValueError(f"invalid instrumentation mode: {record.get('instrumentation')}")
        key = (record["pair"], *cell_key)
        unique_key = (*key, record["instrumentation"])
        if unique_key in seen or record.get("status") != "PASS":
            raise ValueError(f"duplicate or failed performance receipt for {unique_key}")
        seen.add(unique_key)
        grouped.setdefault(key, {})[str(record["instrumentation"])] = record
    if len(records) != expected_cells * expected_pairs * 2:
        raise ValueError(f"unexpected mode run count: {len(records)}")
    if len(grouped) != expected_cells * expected_pairs:
        raise ValueError(f"incomplete paired profile: got {len(grouped)} cell pairs")
    if allowed_cells is not None:
        observed_cells = {tuple(key[1:]) for key in grouped}
        if observed_cells != allowed_cells:
            raise ValueError("profile cells do not match the frozen expected matrix")
    comparisons = []
    for key, modes in sorted(grouped.items(), key=lambda entry: str(entry[0])):
        if set(modes) != set(INSTRUMENTATION):
            raise ValueError(f"missing paired instrumentation mode for {key}")
        always_peer = modes["ALWAYS"]["peer"]
        detailed_peer = modes["DETAILED"]["peer"]
        always_worker = modes["ALWAYS"]["worker"]
        detailed_worker = modes["DETAILED"]["worker"]
        assert isinstance(always_peer, dict) and isinstance(detailed_peer, dict)
        assert isinstance(always_worker, dict) and isinstance(detailed_worker, dict)
        workload = key[4]
        slow_peer = key[5]
        payload_size = int(key[3])
        for label, record in (("ALWAYS", modes["ALWAYS"]), ("DETAILED", modes["DETAILED"])):
            worker = record.get("worker")
            peer = record.get("peer")
            if not isinstance(worker, dict) or not isinstance(peer, dict):
                raise ValueError(f"missing worker or peer receipt for {key} {label}")
            totals = worker.get("totals")
            diagnostics = worker.get("diagnostics")
            if not isinstance(totals, dict) or not isinstance(diagnostics, dict):
                raise ValueError(f"missing totals or diagnostics for {key} {label}")
            peak = diagnostics.get("peak_payload_bytes")
            if isinstance(peak, bool) or not isinstance(peak, int) or peak < 0:
                raise ValueError(f"invalid quota peak for {key} {label}")
            worker_frames = totals.get("measured_received_frames_per_second")
            worker_bytes = totals.get("measured_received_payload_bytes_per_second")
            peer_frames = peer.get("measured_frames_per_second")
            peer_bytes = peer.get("measured_payload_bytes_per_second")
            for bucket_label, frames, byte_counts in (
                ("worker", worker_frames, worker_bytes),
                ("peer", peer_frames, peer_bytes),
            ):
                if (
                    not isinstance(frames, list) or len(frames) != MEASURE_SECONDS
                    or not isinstance(byte_counts, list) or len(byte_counts) != MEASURE_SECONDS
                    or any(isinstance(value, bool) or not isinstance(value, int) or value < 0 for value in (*frames, *byte_counts))
                ):
                    raise ValueError(f"invalid {bucket_label} measurement buckets for {key} {label}")
                if any(byte_counts[i] != frames[i] * payload_size for i in range(MEASURE_SECONDS)):
                    raise ValueError(f"{bucket_label} frame/byte bucket mismatch for {key} {label}")
        always_totals = always_worker.get("totals", {})
        detailed_totals = detailed_worker.get("totals", {})
        if workload == "ECHO":
            always_frames = always_peer.get("measured_frames_per_second", [])
            detailed_frames = detailed_peer.get("measured_frames_per_second", [])
            always_bytes = always_peer.get("measured_payload_bytes_per_second", [])
            detailed_bytes = detailed_peer.get("measured_payload_bytes_per_second", [])
        else:
            always_frames = always_totals.get("measured_received_frames_per_second", [])
            detailed_frames = detailed_totals.get("measured_received_frames_per_second", [])
            always_bytes = always_totals.get("measured_received_payload_bytes_per_second", [])
            detailed_bytes = detailed_totals.get("measured_received_payload_bytes_per_second", [])
        always_frame_rate = sum(always_frames) / MEASURE_SECONDS
        detailed_frame_rate = sum(detailed_frames) / MEASURE_SECONDS
        no_healthy_peer = slow_peer and int(key[2]) == 1
        if no_healthy_peer:
            frame_drop = None
        elif always_frame_rate <= 0 or detailed_frame_rate <= 0:
            raise ValueError(f"no completed frames in paired profile for {key}")
        else:
            frame_drop = (always_frame_rate - detailed_frame_rate) / always_frame_rate
        always_rate = sum(always_bytes) / MEASURE_SECONDS
        detailed_rate = sum(detailed_bytes) / MEASURE_SECONDS
        if no_healthy_peer:
            byte_drop = None
        elif int(key[3]) == 0:
            byte_drop = None
        elif always_rate <= 0:
            raise ValueError(f"missing baseline payload throughput for {key}")
        else:
            byte_drop = (always_rate - detailed_rate) / always_rate
        p99_rise = None
        if workload == "ECHO" and not no_healthy_peer:
            always_p99 = always_peer["latency_p99_ns"]
            detailed_p99 = detailed_peer["latency_p99_ns"]
            if always_p99 is None or detailed_p99 is None or always_p99 == 0:
                raise ValueError(f"missing echo latency samples for {key}")
            p99_rise = (detailed_p99 - always_p99) / always_p99
        always_peak = int(always_worker.get("diagnostics", {}).get("peak_payload_bytes", 0))
        detailed_peak = int(detailed_worker.get("diagnostics", {}).get("peak_payload_bytes", 0))
        quota_pass = max(always_peak, detailed_peak) <= PAYLOAD_QUOTA_BYTES
        throughput_pass = no_healthy_peer or frame_drop <= THROUGHPUT_LIMIT_DROP and (
            byte_drop is None or byte_drop <= THROUGHPUT_LIMIT_DROP
        )
        comparisons.append({
            "pair": key[0],
            "family": key[1],
            "connections": key[2],
            "payload_bytes": key[3],
            "workload": workload,
            "slow_peer": slow_peer,
            "always_completed_frames_per_second": always_frame_rate,
            "detailed_completed_frames_per_second": detailed_frame_rate,
            "detailed_frame_throughput_drop_fraction": frame_drop,
            "always_payload_bytes_per_second": always_rate,
            "detailed_payload_bytes_per_second": detailed_rate,
            "detailed_byte_throughput_drop_fraction": byte_drop,
            "detailed_p99_rise_fraction": p99_rise,
            "always_peak_payload_bytes": always_peak,
            "detailed_peak_payload_bytes": detailed_peak,
            "quota_guardrail_pass": quota_pass,
            "throughput_guardrail_pass": True if no_healthy_peer else throughput_pass,
            "p99_guardrail_pass": True if no_healthy_peer else p99_rise is None or p99_rise <= LATENCY_LIMIT_RISE,
            "guardrails_applicable": not no_healthy_peer,
        })
    return comparisons


def summarize_pair_noise(
    comparisons: list[dict[str, object]], expected_pairs: int
) -> list[dict[str, object]]:
    grouped: dict[tuple[object, ...], list[dict[str, object]]] = {}
    for comparison in comparisons:
        key = tuple(comparison[name] for name in ("family", "connections", "payload_bytes", "workload", "slow_peer"))
        grouped.setdefault(key, []).append(comparison)
    summaries = []
    for key, pairs in sorted(grouped.items(), key=lambda entry: str(entry[0])):
        if len(pairs) != expected_pairs:
            raise ValueError(f"expected {expected_pairs} measured pairs for {key}, got {len(pairs)}")
        if not any(bool(pair["guardrails_applicable"]) for pair in pairs):
            summaries.append({
                "family": key[0],
                "connections": key[1],
                "payload_bytes": key[2],
                "workload": key[3],
                "slow_peer": key[4],
                "guardrails_applicable": False,
                "pair_count": len(pairs),
                "paired_frame_throughput_drop_mean": None,
                "paired_byte_throughput_drop_mean": None,
                "paired_completed_throughput_drop_mean": None,
                "completed_throughput_drop_95_ci": None,
                "frame_throughput_drop_95_ci": None,
                "byte_throughput_drop_95_ci": None,
                "paired_p99_rise_mean": None,
                "p99_rise_95_ci": None,
                "quota_guardrail_pass": all(bool(pair["quota_guardrail_pass"]) for pair in pairs),
                "throughput_guardrail_pass": True,
                "p99_guardrail_pass": True,
                "confidence_interval_method": "not applicable: no healthy peer exists in the one-connection slow-peer cell",
            })
            continue
        applicable_pairs = [pair for pair in pairs if bool(pair["guardrails_applicable"])]
        pair_no = [float(pair["detailed_frame_throughput_drop_fraction"]) for pair in applicable_pairs]
        byte_drops = [pair["detailed_byte_throughput_drop_fraction"] for pair in applicable_pairs]
        combined_drops = [
            max(frame, 0.0 if byte is None else float(byte))
            for frame, byte in zip(pair_no, byte_drops, strict=True)
        ]
        p99_rises = [
            float(pair["detailed_p99_rise_fraction"])
            for pair in applicable_pairs if pair["detailed_p99_rise_fraction"] is not None
        ]
        seed_material = json.dumps(key, separators=(",", ":")).encode()
        seed = int(hashlib.sha256(seed_material).hexdigest()[:16], 16)
        throughput_ci = bootstrap_mean_interval(combined_drops, seed)
        frame_ci = bootstrap_mean_interval(pair_no, seed ^ 0xA5A5A5A5)
        byte_values = [float(value) for value in byte_drops if value is not None]
        byte_ci = None if not byte_values else bootstrap_mean_interval(byte_values, seed ^ 0x5A5A5A5A)
        p99_ci = None if not p99_rises else bootstrap_mean_interval(p99_rises, seed ^ 0x13579BDF)
        all_quota = all(bool(pair["quota_guardrail_pass"]) for pair in pairs)
        if expected_pairs >= PAIRS and (throughput_ci is None or (key[3] == "ECHO" and p99_ci is None)):
            raise ValueError(f"missing paired confidence interval for {key}")
        summaries.append({
            "family": key[0],
            "connections": key[1],
            "payload_bytes": key[2],
            "workload": key[3],
            "slow_peer": key[4],
            "guardrails_applicable": True,
            "pair_count": len(pairs),
            "paired_frame_throughput_drop_mean": sum(pair_no) / len(pair_no),
            "paired_byte_throughput_drop_mean": None if not byte_values else sum(byte_values) / len(byte_values),
            "paired_completed_throughput_drop_mean": sum(combined_drops) / len(combined_drops),
            "completed_throughput_drop_95_ci": throughput_ci,
            "frame_throughput_drop_95_ci": frame_ci,
            "byte_throughput_drop_95_ci": byte_ci,
            "paired_p99_rise_mean": None if not p99_rises else sum(p99_rises) / len(p99_rises),
            "p99_rise_95_ci": p99_ci,
            "quota_guardrail_pass": all_quota,
            "throughput_guardrail_pass": (
                all(bool(pair["throughput_guardrail_pass"]) for pair in applicable_pairs)
                if throughput_ci is None else throughput_ci["upper_95"] <= THROUGHPUT_LIMIT_DROP
            ),
            "p99_guardrail_pass": (
                all(bool(pair["p99_guardrail_pass"]) for pair in applicable_pairs)
                if p99_ci is None else key[3] != "ECHO" or p99_ci["upper_95"] <= LATENCY_LIMIT_RISE
            ),
            "confidence_interval_method": "10,000 deterministic paired bootstrap resamples of the mean; nearest-rank 95% interval",
        })
    return summaries


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--smoke", action="store_true", help="run one 1-connection TCP4 echo case in each mode")
    parser.add_argument("--smoke-slow", action="store_true", help="run one 1-connection TCP4 slow-peer echo case in each mode")
    parser.add_argument(
        "--slow-smoke-family", choices=FAMILIES, default="tcp4",
        help="socket family used by --smoke-slow (default: tcp4)",
    )
    parser.add_argument("--report", type=Path, help="new output report path")
    args = parser.parse_args()
    check_report_paths([args.report])
    parent = ROOT / "artifacts" / "comm"
    parent.mkdir(parents=True, exist_ok=True)
    run_root = Path(tempfile.mkdtemp(prefix="performance-", dir=parent))
    source = run_root / "source"
    source_hash = capture_source(ROOT, source)
    if source_hash != source_fingerprint(source):
        raise RuntimeError("retained performance source identity mismatch")
    build_root = run_root / "build"
    build_log = run_root / "build.log"
    command = [
        "cargo", "build", "--release", "-p", "xadbtest-transport-worker",
        "--bin", "comm_performance", "--locked", "--offline", "--target-dir", str(build_root),
    ]
    build = subprocess.run(command, cwd=source, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    build_log.write_bytes(build.stdout)
    if build.returncode:
        raise RuntimeError(f"performance worker build failed; see {build_log}")
    binary = build_root / "release" / ("comm_performance.exe" if os.name == "nt" else "comm_performance")
    binary_hash = digest(binary)
    smoke_only = args.smoke or args.smoke_slow
    if smoke_only:
        cells = [{
            "family": args.slow_smoke_family if args.smoke_slow else "tcp4", "connections": 1,
            "payload_bytes": 1_048_576 if args.smoke_slow else 64,
            "workload": "ECHO", "slow_peer": False,
        }]
        if args.smoke_slow:
            cells[0]["slow_peer"] = True
        pair_count = 1
    else:
        cells = matrix() + slow_peer_matrix()
        pair_count = PAIRS
    manifest = {
        "schema_version": 1,
        "scope": "COMM C-V18 native workload and Always/Detailed paired profile; local only",
        "status": "RUNNING",
        "source_sha256": source_hash,
        "lockfile_sha256": digest(source / "Cargo.lock"),
        "contract_sha256": {
            "devdoc/contracts/comm-design.md": digest(source / "devdoc/contracts/comm-design.md"),
            "devdoc/plans/comm-implementation-plan.md": digest(source / "devdoc/plans/comm-implementation-plan.md"),
        },
        "environment": {"uname": list(platform.uname()), "python": sys.version, "cpu_count": os.cpu_count()},
        "profile": {
            "families": FAMILIES,
            "payload_sizes": PAYLOAD_SIZES,
            "connections": CONNECTION_COUNTS,
            "workloads": WORKLOADS,
            "slow_peer_cells": (1 if args.smoke_slow else 0) if smoke_only else len(slow_peer_matrix()),
            "pairs": pair_count,
            "warmup_seconds": WARMUP_SECONDS,
            "measure_seconds": MEASURE_SECONDS,
            "instrumentation_order": "alternates Always/Detailed order by pair",
        },
        "worker": {"path": str(binary.relative_to(run_root)), "sha256": binary_hash, "build_command": command, "build_log_sha256": hashlib.sha256(build_log.read_bytes()).hexdigest()},
        "case_count": len(cells) * pair_count,
        "mode_run_count": len(cells) * pair_count * 2,
        "smoke_only": smoke_only,
    }
    write_report(run_root / "manifest.json", manifest)
    records: list[dict[str, object]] = []
    run_index = 0
    try:
        for pair in range(1, pair_count + 1):
            order = INSTRUMENTATION if pair % 2 else tuple(reversed(INSTRUMENTATION))
            for cell_index, cell in enumerate(cells):
                for instrumentation in order:
                    run_index += 1
                    directory = run_root / "cases" / f"pair-{pair:02}" / f"cell-{cell_index:03}" / instrumentation.lower()
                    record = run_case(binary, directory, cell, instrumentation, source_hash)
                    record["pair"] = pair
                    record["run_index"] = run_index
                    write_report(directory / "validation.json", record)
                    records.append(record)
                    progress = {
                        "status": "RUNNING",
                        "completed_mode_runs": run_index,
                        "expected_mode_runs": len(cells) * pair_count * 2,
                        "last_run": str((directory / "validation.json").relative_to(run_root)),
                        "completed_status": record["status"],
                    }
                    (run_root / "progress.json.tmp").write_text(json.dumps(progress, indent=2) + "\n")
                    os.replace(run_root / "progress.json.tmp", run_root / "progress.json")
        comparisons = summarize_pairs(records, len(cells), pair_count, cells)
        pair_noise = summarize_pair_noise(comparisons, pair_count)
        guardrail_rows = pair_noise if not smoke_only else comparisons
        report = {
            "schema_version": 1,
            "scope": manifest["scope"],
            "status": ("SMOKE_PASS" if smoke_only else "PASS") if all(
                c["throughput_guardrail_pass"] and c["p99_guardrail_pass"] and c["quota_guardrail_pass"]
                for c in guardrail_rows
            ) else "FAIL",
            "source_sha256": source_hash,
            "manifest": str((run_root / "manifest.json").relative_to(ROOT)),
            "mode_run_count": run_index,
            "case_comparison_count": len(comparisons),
            "comparisons": comparisons,
            "paired_noise_summary": pair_noise,
            "artifacts": str(run_root.relative_to(ROOT)),
            "smoke_only": smoke_only,
            "official_distribution": False,
        }
        write_report(run_root / "validation.json", report)
        if args.report:
            write_report(args.report, report)
        print(f"COMM C-V18 {report['status']}: {run_root / 'validation.json'}", flush=True)
        return 0 if report["status"] in {"PASS", "SMOKE_PASS"} else 1
    except BaseException as error:
        failure = {
            "schema_version": 1,
            "scope": manifest["scope"],
            "status": "FAIL",
            "source_sha256": source_hash,
            "error": str(error),
            "completed_mode_runs": run_index,
            "expected_mode_runs": len(cells) * pair_count * 2,
            "artifacts": str(run_root.relative_to(ROOT)),
            "smoke_only": smoke_only,
        }
        write_report(run_root / "validation.json", failure)
        if args.report:
            write_report(args.report, failure)
        raise


if __name__ == "__main__":
    raise SystemExit(main())
