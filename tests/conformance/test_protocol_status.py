"""Rust/Python/TypeScript share these status strings. Timing and UUIDs may differ."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk" / "python" / "src"))

from axiom_ai.result import TERMINAL_STATUSES


def test_python_terminal_statuses():
    expected = {
        "COMPLETED",
        "UNVERIFIED",
        "FAILED",
        "CANCELLED",
        "TIMED_OUT",
        "BUDGET_EXCEEDED",
        "STEP_LIMIT_EXCEEDED",
        "MODEL_BUDGET_EXCEEDED",
        "REJECTED",
        "RECOVERY_REQUIRED",
    }
    assert expected <= TERMINAL_STATUSES


def test_typescript_sdk_exports_same_protocol_version():
    version = (ROOT / "sdk" / "typescript" / "src" / "version.ts").read_text(encoding="utf-8")
    assert 'PROTOCOL_VERSION = "1.0"' in version
    assert 'SOURCE_FRAMEWORK = "axiom-typescript"' in version
    result = (ROOT / "sdk" / "typescript" / "src" / "result.ts").read_text(encoding="utf-8")
    for status in TERMINAL_STATUSES:
        assert f'"{status}"' in result


def test_worker_kinds_match_protocol_schema():
    worker_rs = (ROOT / "crates" / "axiom-protocol" / "src" / "worker.rs").read_text(encoding="utf-8")
    for kind in ("model.generate", "tool.execute", "state.snapshot", "goal.evaluate", "action.verify"):
        assert kind in worker_rs
    ts = (ROOT / "sdk" / "typescript" / "src" / "worker.ts").read_text(encoding="utf-8")
    py = (ROOT / "sdk" / "python" / "src" / "axiom_ai" / "worker.py").read_text(encoding="utf-8")
    for kind in ("model.generate", "tool.execute", "state.snapshot", "goal.evaluate", "action.verify"):
        assert kind in ts
        assert kind in py
