#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["asyncssh==2.24.1", "pyte==0.8.2", "websockets==15.0.1"]
# ///
"""Qualify local release images through real SSH, HAProxy and blue/green backends.

Build first with `make bastion-test-images`. All resources belong to a unique
Compose project and are removed even on failure. Reports survive in target/.
"""

from __future__ import annotations

import argparse
import asyncio
import contextlib
import hashlib
import json
import os
from pathlib import Path
import socket
import signal
import statistics
import subprocess
import tempfile
import time
import uuid

import asyncssh
import pyte
from websockets.asyncio.client import connect as websocket_connect

ROOT = Path(__file__).resolve().parents[2]
COMPOSE = ROOT / "docker-compose.bastion-test.yml"


def emit(event, **fields):
    print(json.dumps({"event": event, "time": time.time(), **fields}), flush=True)


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def distribution(values):
    values = sorted(values)
    if not values:
        return {}
    return {
        "n": len(values),
        "p50_ms": statistics.median(values) * 1000,
        "p95_ms": values[int((len(values) - 1) * 0.95)] * 1000,
        "p99_ms": values[int((len(values) - 1) * 0.99)] * 1000,
        "max_ms": max(values) * 1000,
    }


class Stack:
    def __init__(self, output: Path):
        self.output = output
        self.project = "late-bastion-test-" + uuid.uuid4().hex[:8]
        self.temp = tempfile.TemporaryDirectory(
            prefix="late-bastion-", dir=os.environ.get("TMPDIR")
        )
        self.state = Path(self.temp.name)
        self.empty_env = self.state / "empty.env"
        self.empty_env.write_text("")
        self.env = os.environ.copy()
        self.ports = {
            name: free_port() for name in ("RAW", "SSH", "DIRECT", "API", "CONTROL")
        }
        self.env.update(
            {
                f"BASTION_TEST_{key}_PORT": str(value)
                for key, value in self.ports.items()
            }
        )
        self.env["BASTION_TEST_STATE_DIR"] = str(self.state)
        host_key = asyncssh.generate_private_key("ssh-ed25519")
        (self.state / "host_key").write_bytes(host_key.export_private_key("openssh"))
        # Deliberately disposable key, readable by the unprivileged image UID.
        (self.state / "host_key").chmod(0o644)
        self.active = "blue"

    async def compose(self, *args, check=True):
        proc = await asyncio.create_subprocess_exec(
            "docker",
            "compose",
            "--env-file",
            str(self.empty_env),
            "-p",
            self.project,
            "-f",
            str(COMPOSE),
            *args,
            cwd=ROOT,
            env=self.env,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT,
        )
        out, _ = await proc.communicate()
        with (self.output / "compose.log").open("ab") as log:
            log.write(("\n$ compose " + " ".join(args) + "\n").encode() + out)
        if check and proc.returncode:
            raise RuntimeError(
                f"compose {' '.join(args)} failed: {out.decode(errors='replace')[-5000:]}"
            )
        return out.decode(errors="replace")

    async def control(self, command):
        reader, writer = await asyncio.open_connection(
            "127.0.0.1", self.ports["CONTROL"]
        )
        writer.write((command + "\n").encode())
        await writer.drain()
        response = await asyncio.wait_for(reader.read(), 5)
        writer.close()
        await writer.wait_closed()
        return response.decode()

    async def health(self):
        for _ in range(60):
            try:
                reader, writer = await asyncio.open_connection(
                    "127.0.0.1", self.ports["API"]
                )
                writer.write(
                    b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                )
                await writer.drain()
                status = await asyncio.wait_for(reader.readline(), 2)
                writer.close()
                await writer.wait_closed()
                if b"200" in status:
                    return
            except (OSError, TimeoutError):
                pass
            await asyncio.sleep(1)
        raise RuntimeError("backend API did not become healthy")

    async def route(self, target):
        other = "green" if target == "blue" else "blue"
        for backend in ("tunnel", "api"):
            await self.control(f"set server {backend}/{target} state ready")
            await self.control(f"set server {backend}/{other} state maint")
        self.active = target
        await self.health()

    async def start(self):
        await self.compose("config", "--quiet")
        # Both replicas migrate the same cold database, as on a fresh rollout.
        await self.compose("up", "-d", "--wait", "--wait-timeout", "180")
        await self.health()
        (self.output / "endpoints.json").write_text(
            json.dumps(
                {
                    "project": self.project,
                    "ports": self.ports,
                    "state_dir": str(self.state),
                },
                indent=2,
            )
        )
        emit("stack_ready", project=self.project, ports=self.ports)

    async def samples(self):
        ids = (await self.compose("ps", "-q")).split()
        proc = await asyncio.create_subprocess_exec(
            "docker",
            "stats",
            "--no-stream",
            "--format",
            "{{json .}}",
            *ids,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL,
        )
        out, _ = await proc.communicate()
        with (self.output / "resources.jsonl").open("ab") as log:
            log.write(
                json.dumps(
                    {"time": time.time(), "containers": out.decode().splitlines()}
                ).encode()
                + b"\n"
            )

    async def close(self):
        try:
            try:
                logs = await self.compose("logs", "--no-color", check=False)
                (self.output / "containers.log").write_text(logs)
            finally:
                await self.compose("down", "-v", "--remove-orphans", "--timeout", "3")
        finally:
            self.temp.cleanup()


class Client:
    def __init__(self, number, conn, process, token):
        self.number, self.conn, self.process, self.token = number, conn, process, token
        self.screen = pyte.Screen(120, 40)
        self.parser = pyte.ByteStream(self.screen)
        self.bytes = 0
        self.resets = 0
        self.last_reset = 0.0
        self.tail = b""
        self.error = None
        self.changed = asyncio.Event()
        self.reader = asyncio.create_task(self.read())

    async def read(self):
        try:
            while data := await self.process.stdout.read(65536):
                # Keep only an incomplete prefix: a completed sequence in the
                # preceding chunk must not be counted a second time.
                scan = self.tail[-7:] + data
                resets = scan.count(b"\x1b[?1049h")
                if resets:
                    self.resets += resets
                    self.last_reset = time.monotonic()
                self.tail = (self.tail + data)[-131072:]
                self.bytes += len(data)
                self.parser.feed(data)
                self.changed.set()
            self.error = "SSH output ended"
        except Exception as error:
            self.error = repr(error)
        finally:
            self.changed.set()

    async def send(self, data):
        self.process.stdin.write(data)
        await asyncio.wait_for(self.process.stdin.drain(), 5)

    async def wait(self, predicate, seconds=30):
        async with asyncio.timeout(seconds):
            while not predicate():
                if self.error or self.conn.is_closed():
                    raise RuntimeError(
                        f"client {self.number}: {self.error or 'SSH disconnected'}"
                    )
                self.changed.clear()
                await self.changed.wait()

    async def probe(self, sequence):
        # Escape pending overlays/composition, select Home and open its composer.
        for data in (b"\x1b", b"\x1b", b"1", b"i", b"\x15"):
            await self.send(data)
            await asyncio.sleep(0.08)
        marker = f"bastion_probe_{self.number}_{sequence}"
        started = time.monotonic()
        await self.send(marker.encode())
        await self.wait(lambda: marker in "\n".join(self.screen.display), 5)
        elapsed = time.monotonic() - started
        await self.send(b"\x15\x1b")
        return elapsed

    async def upgrade(self):
        for data in (b"\x1b", b"\x1b", b"5", b"q", b"r"):
            await self.send(data)
            await asyncio.sleep(0.08)

    async def close(self):
        self.conn.close()
        with contextlib.suppress(Exception):
            await asyncio.wait_for(self.conn.wait_closed(), 3)
        self.reader.cancel()
        await asyncio.gather(self.reader, return_exceptions=True)


async def connect_client(stack, index, output, direct=False):
    key = asyncssh.generate_private_key("ssh-ed25519")
    if index < 3:
        key_path = output / f"client-{index}.key"
        key_path.write_bytes(key.export_private_key("openssh"))
        key_path.chmod(0o600)
    loop = asyncio.get_running_loop()

    async def dial():
        sock = socket.socket()
        sock.setblocking(False)
        try:
            await loop.sock_connect(
                sock, ("127.0.0.1", stack.ports["DIRECT" if direct else "RAW"])
            )
            if not direct:
                await loop.sock_sendall(
                    sock,
                    f"PROXY TCP4 198.18.{index // 250}.{index % 250 + 1} 127.0.0.1 12345 5222\r\n".encode(),
                )
            return await asyncssh.connect(
                sock=sock,
                username="qualification",
                client_keys=[key],
                known_hosts=None,
                config=None,
                agent_path=None,
                login_timeout=15,
                keepalive_interval=5,
                keepalive_count_max=6,
            )
        except BaseException:
            sock.close()
            raise

    # Qualify returning sessions during rollouts, rather than driving the
    # compulsory first-visit tour in every soak client. This also exercises
    # exec-only teardown against the real packaged backend for every account.
    conn = await dial()
    try:
        await asyncio.wait_for(conn.run("late-cli-token-v1", check=True), 15)
        fingerprint = key.get_fingerprint()
        assert fingerprint.startswith("SHA256:") and "'" not in fingerprint
        sql = (
            'UPDATE users SET settings = settings || \'{"clubhouse_tutorial_done":true,"landing_page":"home"}\'::jsonb WHERE fingerprint = \''
            + fingerprint
            + "'"
        )
        await stack.compose(
            "exec", "-T", "postgres", "psql", "-U", "postgres", "-c", sql
        )
    finally:
        conn.close()
        await conn.wait_closed()
    started = time.monotonic()
    conn = await dial()
    try:
        result = await asyncio.wait_for(conn.run("late-cli-token-v1", check=True), 15)
        token = json.loads(result.stdout)["session_token"]
        process = await conn.create_process(
            term_type="xterm-256color",
            term_size=(120, 40),
            env={"TERM_PROGRAM": "bastion-qualification"},
            encoding=None,
        )
        client = Client(index, conn, process, token)
        await client.wait(lambda: client.bytes > 100, 30)
        client.first_frame = time.monotonic() - started
        return client
    except BaseException:
        conn.close()
        await conn.wait_closed()
        raise


async def pair_roundtrip(stack, token):
    async with websocket_connect(
        f"ws://127.0.0.1:{stack.ports['API']}/api/ws/pair?token={token}", open_timeout=5
    ) as ws:
        await ws.send(json.dumps({"event": "heartbeat"}))
        # A successful upgrade and server message prove the stable token routes
        # to the current backend. Detailed control semantics have Rust coverage.
        response = await asyncio.wait_for(ws.recv(), 5)
        return json.loads(response)


async def campaign(args):
    loop = asyncio.get_running_loop()
    task = asyncio.current_task()
    # SIGINT already cancels asyncio.run's main task. Give SIGTERM the same
    # opportunity to collect evidence and remove only our Compose project.
    with contextlib.suppress(NotImplementedError):
        loop.add_signal_handler(signal.SIGTERM, task.cancel)
    output = (
        Path(args.output).resolve()
        if args.output
        else ROOT
        / "target"
        / "bastion-qualification"
        / (time.strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:6])
    )
    output.mkdir(parents=True, exist_ok=False)
    stack = Stack(output)
    clients, probe_times, recovery_times = [], [], []
    summary = {
        "status": "failed",
        "clients": args.clients,
        "duration_secs": args.duration,
        "faults": [],
    }
    emit("report_directory", path=str(output))
    try:
        await stack.start()
        summary["images"] = await stack.compose("images", "--format", "json")
        summary["git_head"] = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip()
        summary["diff_sha256"] = hashlib.sha256(
            subprocess.check_output(["git", "diff", "HEAD"], cwd=ROOT)
        ).hexdigest()
        # Baseline uses the same backend and a single local SSH account at a time.
        direct = await connect_client(stack, 0, output, direct=True)
        summary["direct_first_frame_ms"] = direct.first_frame * 1000
        await direct.close()
        # A modest ramp avoids making account creation a synchronized DB flood.
        for first in range(0, args.clients, 5):
            batch = await asyncio.gather(
                *(
                    connect_client(stack, i, output)
                    for i in range(first, min(first + 5, args.clients))
                ),
                return_exceptions=True,
            )
            clients.extend(client for client in batch if isinstance(client, Client))
            errors = [
                repr(error) for error in batch if isinstance(error, BaseException)
            ]
            if errors:
                raise RuntimeError(f"connection ramp failed: {errors}")
            emit("ramp", connected=len(clients))
        summary["first_frame"] = distribution(
            [client.first_frame for client in clients]
        )
        summary["initial_pair"] = await pair_roundtrip(stack, clients[0].token)
        started = time.monotonic()
        next_fault = (
            started + args.duration / (args.faults + 1) if args.faults else float("inf")
        )
        faults, sequence = 0, 0
        while time.monotonic() - started < args.duration:
            failures = [
                (client.number, client.error)
                for client in clients
                if client.error or client.conn.is_closed()
            ]
            if failures:
                raise RuntimeError(f"unexpected SSH disconnects: {failures}")
            if time.monotonic() >= next_fault and faults < args.faults:
                old = stack.active
                target = "green" if old == "blue" else "blue"
                await stack.compose(
                    "up", "-d", "--wait", "--wait-timeout", "120", target
                )
                counts = [client.resets for client in clients]
                fault_at = time.monotonic()
                await stack.route(target)
                graceful = faults % 2 == 0
                await stack.compose(
                    "kill", "-s", "SIGTERM" if graceful else "SIGKILL", old
                )
                if graceful:
                    await asyncio.gather(*(client.upgrade() for client in clients))
                await asyncio.gather(
                    *(
                        client.wait(
                            lambda c=client, before=count: c.resets > before, 30
                        )
                        for client, count in zip(clients, counts)
                    )
                )
                recovered = [client.last_reset - fault_at for client in clients]
                recovery_times.extend(recovered)
                result = {
                    "kind": "graceful" if graceful else "kill",
                    "recovery": distribution(recovered),
                }
                summary["faults"].append(result)
                emit("fault_recovered", **result)
                await pair_roundtrip(stack, clients[0].token)
                await stack.compose("stop", "-t", "2", old)
                faults += 1
                next_fault = started + args.duration * (faults + 1) / (args.faults + 1)
                await asyncio.sleep(1)
            sequence += 1
            active = clients[: max(1, args.clients // 5)]
            probe_times.extend(
                await asyncio.gather(*(client.probe(sequence) for client in active))
            )
            await stack.samples()
            emit(
                "sample",
                elapsed_secs=round(time.monotonic() - started),
                connected=len(clients),
                active=len(active),
                total_bytes=sum(client.bytes for client in clients),
                probes=distribution(probe_times[-len(active) :]),
            )
            await asyncio.sleep(
                min(10, max(0, args.duration - (time.monotonic() - started)))
            )
        if faults != args.faults:
            raise RuntimeError(
                f"only exercised {faults}/{args.faults} scheduled faults"
            )
        summary.update(
            status="passed",
            input_response=distribution(probe_times),
            recovery=distribution(recovery_times),
            first_frame=distribution([client.first_frame for client in clients]),
        )
    except BaseException as error:
        summary["error"] = repr(error)
        for client in clients:
            (output / f"client-{client.number}.ansi").write_bytes(client.tail)
            (output / f"client-{client.number}.txt").write_text(
                "\n".join(client.screen.display)
            )
        raise
    finally:
        await asyncio.gather(
            *(client.close() for client in clients), return_exceptions=True
        )
        cleanup_errors = []
        try:
            # Capture socket counts after all owned clients have disconnected.
            if clients:
                await asyncio.sleep(2)
                await stack.samples()
                processes = {}
                for service in ("bastion", stack.active):
                    processes[service] = await stack.compose(
                        "exec",
                        "-T",
                        service,
                        "sh",
                        "-c",
                        'printf "fds="; ls /proc/1/fd | wc -l; printf "tasks="; ls /proc/1/task | wc -l; cat /proc/1/status',
                        check=False,
                    )
                (output / "after-disconnect.json").write_text(
                    json.dumps(processes, indent=2)
                )
        except Exception as error:
            cleanup_errors.append(repr(error))
        try:
            await stack.close()
        except Exception as error:
            cleanup_errors.append(repr(error))
        finally:
            for key in output.glob("client-*.key"):
                key.unlink(missing_ok=True)
            if cleanup_errors:
                summary.update(status="failed", cleanup_errors=cleanup_errors)
            (output / "summary.json").write_text(json.dumps(summary, indent=2))
            emit(
                "finished", status=summary["status"], path=str(output / "summary.json")
            )
        if cleanup_errors:
            raise RuntimeError(f"qualification cleanup failed: {cleanup_errors}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--clients", type=int, default=100)
    parser.add_argument("--duration", type=int, default=2700)
    parser.add_argument("--faults", type=int, default=10)
    parser.add_argument("--output")
    args = parser.parse_args()
    if args.clients < 1 or args.duration < 1 or args.faults < 0:
        parser.error("clients/duration must be positive and faults nonnegative")
    asyncio.run(campaign(args))
