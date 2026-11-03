"""Runtime facade: submit a run to the Goshikq runtime and serve its worker requests."""

from __future__ import annotations

import asyncio
import logging
from typing import Any

from ._env import branded
from ._version import PROTOCOL_VERSION, SOURCE_FRAMEWORK, __version__
from .agent import Agent
from .client import RuntimeClient
from .errors import RuntimeConnectionError
from .goal import Goal
from .result import RunResult
from .state import StateProvider
from .worker import Worker

log = logging.getLogger("goshikq_ai.runtime")


class Runtime:
    """Connects an :class:`Agent` to the Goshikq runtime.

    The runtime process owns the execution loop, policy, and persistence; this process
    only answers worker requests (model generation, tool execution, state snapshots, goal
    evaluation, action verification).
    """

    def __init__(
        self,
        agent: Agent,
        *,
        endpoint: str | None = None,
        api_key: str | None = None,
        state_provider: StateProvider | None = None,
        project_id: str | None = None,
        studio_url: str | None = None,
        poll_timeout_ms: int = 20_000,
    ) -> None:
        self.agent = agent
        self.state_provider = state_provider
        self.project_id = project_id or branded("PROJECT_ID")
        self.studio_url = (studio_url or branded("STUDIO_URL") or "").rstrip("/") or None
        self.poll_timeout_ms = poll_timeout_ms
        self.client = RuntimeClient(endpoint, api_key, actor=f"{SOURCE_FRAMEWORK}/{__version__}")
        self._last_worker: Worker | None = None

    async def run(
        self,
        goal: Goal,
        *,
        input: dict[str, Any] | None = None,
        idempotency_key: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> RunResult:
        """Create a run, serve it until it reaches a terminal state, return the runtime's verdict."""
        body: dict[str, Any] = {
            "agent": self.agent.to_definition(),
            "goal": goal.to_definition(),
            "input": input or {},
            "has_state_provider": self.state_provider is not None,
            "source_framework": SOURCE_FRAMEWORK,
            "metadata": {"sdk_version": __version__, "protocol_version": PROTOCOL_VERSION, **(metadata or {})},
        }
        if self.project_id:
            body["project_id"] = self.project_id
        if idempotency_key:
            body["idempotency_key"] = idempotency_key

        created = await self.client.create_run(body)
        run_id = str(created["run_id"])
        log.info("run %s created (status=%s, reused=%s)", run_id, created.get("status"), created.get("reused"))

        from .result import TERMINAL_STATUSES

        status = str(created.get("status") or "")
        if not created.get("reused") or status not in TERMINAL_STATUSES:
            return await self.attach(run_id, goal)
        return await self.result(run_id)

    async def create_deferred(
        self,
        goal: Goal,
        *,
        input: dict[str, Any] | None = None,
        idempotency_key: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Persist a coordinator run without spawning the executor."""
        body: dict[str, Any] = {
            "agent": self.agent.to_definition(),
            "goal": goal.to_definition(),
            "input": input or {},
            "has_state_provider": self.state_provider is not None,
            "source_framework": SOURCE_FRAMEWORK,
            "defer_start": True,
            "metadata": {"sdk_version": __version__, "protocol_version": PROTOCOL_VERSION, **(metadata or {})},
        }
        if self.project_id:
            body["project_id"] = self.project_id
        if idempotency_key:
            body["idempotency_key"] = idempotency_key
        return await self.client.create_run(body)

    async def attach(self, run_id: str, goal: Goal) -> RunResult:
        """Serve worker requests for an already-created run (child or deferred parent)."""
        worker = Worker(self.client, run_id, self.agent, goal, self.state_provider, poll_timeout_ms=self.poll_timeout_ms)
        self._last_worker = worker
        await worker.serve()
        return await self.result(run_id)

    def run_sync(self, goal: Goal, **kwargs: Any) -> RunResult:
        return asyncio.run(self.run(goal, **kwargs))

    async def result(self, run_id: str) -> RunResult:
        run = await self.client.get_run(run_id)
        return RunResult.from_api(run, self._studio_link(run_id))

    async def cancel(self, run_id: str, reason: str | None = None) -> dict[str, Any]:
        return await self.client.cancel_run(run_id, reason)

    async def check_connection(self) -> dict[str, Any]:
        try:
            return await self.client.ready()
        except RuntimeConnectionError:
            raise

    async def aclose(self) -> None:
        await self.client.aclose()

    async def __aenter__(self) -> Runtime:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.aclose()

    def _studio_link(self, run_id: str) -> str | None:
        return f"{self.studio_url}/executions/{run_id}" if self.studio_url else None
