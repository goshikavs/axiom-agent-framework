"""Read GOSHIKQ_* first, then legacy AXIOM_* names."""

from __future__ import annotations

import os


def branded(*suffixes: str, default: str | None = None) -> str | None:
    for suffix in suffixes:
        for prefix in ("GOSHIKQ_", "AXIOM_"):
            value = os.environ.get(f"{prefix}{suffix}")
            if value:
                return value
    return default
