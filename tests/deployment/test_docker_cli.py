"""Docker smoke is required for v1.0. This host documents availability only."""

from __future__ import annotations

import os
import shutil

import pytest


def test_docker_engine_or_skip():
    if shutil.which("docker") is None:
        pytest.skip("Docker CLI is not installed on this host; image smoke is CI-only and has not run")
    if os.environ.get("GOSHIKQ_DOCKER_SMOKE") != "1":
        pytest.skip("Docker is installed but GOSHIKQ_DOCKER_SMOKE=1 was not set; do not claim a local smoke test")
