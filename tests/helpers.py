"""Shared helpers for spawning goshikq-server in integration tests."""

from __future__ import annotations

import os
import socket
import subprocess
import time
from pathlib import Path

import httpx
import pytest

ROOT = Path(__file__).resolve().parents[1]
DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:goshikq@127.0.0.1:55432/goshikq")


def find_server() -> Path | None:
    # Prefer the explicit MSVC x64 target even if GOSHIKQ_SERVER_BIN points at a stale
    # `target/debug` image (common while the long-lived :7400 process holds that file).
    x64 = ROOT / "target" / "x86_64-pc-windows-msvc" / "debug" / "goshikq-server.exe"
    if x64.exists():
        return x64
    env = os.environ.get("GOSHIKQ_SERVER_BIN") or os.environ.get("AXIOM_SERVER_BIN")
    if env and Path(env).exists():
        return Path(env)
    candidates = [
        ROOT / "target" / "x86_64-pc-windows-msvc" / "debug" / "goshikq-server.exe",
        ROOT / "target" / "x86_64-pc-windows-msvc" / "release" / "goshikq-server.exe",
        ROOT / "target" / "debug" / "goshikq-server.exe",
        ROOT / "target" / "release" / "goshikq-server.exe",
        ROOT / "target" / "debug" / "goshikq-server",
        ROOT / "target" / "release" / "goshikq-server",
        ROOT / "target" / "x86_64-pc-windows-msvc" / "debug" / "axiom-server.exe",
        ROOT / "target" / "debug" / "axiom-server.exe",
        ROOT / "target" / "debug" / "axiom-server",
    ]
    for p in candidates:
        if p.exists():
            return p
    return None


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def start_server(port: int, extra_env: dict[str, str] | None = None) -> subprocess.Popen:
    binary = find_server()
    if binary is None:
        pytest.skip("goshikq-server binary not found")
    env = os.environ.copy()
    env.update(
        {
            "DATABASE_URL": DATABASE_URL,
            "GOSHIKQ_BIND": f"127.0.0.1:{port}",
            "GOSHIKQ_AUTH_MODE": "none",
            "RUST_LOG": env.get("RUST_LOG", "warn"),
        }
    )
    if extra_env:
        env.update(extra_env)
    kwargs: dict = {"env": env, "cwd": str(ROOT), "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    return subprocess.Popen([str(binary)], **kwargs)


def wait_ready(url: str, timeout: float = 30.0) -> None:
    deadline = time.monotonic() + timeout
    with httpx.Client() as client:
        while time.monotonic() < deadline:
            try:
                r = client.get(f"{url}/health", timeout=1.0)
                if r.status_code < 500:
                    return
            except httpx.HTTPError:
                time.sleep(0.2)
        raise TimeoutError(f"server at {url} did not become ready")


def stop(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    proc.kill()
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.terminate()
