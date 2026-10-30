"""Native multi-agent programming model.

Child completion is never treated as parent-goal verification. The parent worker
evaluates the parent :class:`Goal` against an authoritative :class:`StateProvider`.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Any, Literal

from .agent import Agent
from .client import RuntimeClient
from .goal import Goal
from .result import RunResult
from .runtime import Runtime
from .state import StateProvider
from .worker import Worker

CoordinationMode = Literal["sequential", "parallel", "supervisor"]
FailureMode = Literal["fail_fast", "partial"]


@dataclass
class CoordinationPolicy:
    mode: CoordinationMode = "supervisor"
    max_depth: int = 4
    max_delegations: int = 16
    prevent_cycles: bool = True
    failure_mode: FailureMode = "fail_fast"
    max_child_retries: int = 0
    require_parent_verification: bool = True

    def to_dict(self) -> dict[str, Any]:
        return {
            "mode": self.mode,
            "max_depth": self.max_depth,
            "max_delegations": self.max_delegations,
            "prevent_cycles": self.prevent_cycles,
            "failure_mode": self.failure_mode,
            "max_child_retries": self.max_child_retries,
            "require_parent_verification": self.require_parent_verification,
        }


@dataclass
class AgentTask:
    agent: Agent
    goal: Goal
    input: dict[str, Any] = field(default_factory=dict)
    role: str = "worker"
    requires_approval: bool = False
    state_provider: StateProvider | None = None
    idempotency_key: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class GroupResult:
    parent: RunResult
    children: list[RunResult]
    hierarchy: dict[str, Any] = field(default_factory=dict)

    @property
    def verified(self) -> bool:
        return self.parent.verified


class AgentGroup:
    """Coordinator plus child agents sharing one parent run and correlation id."""

    def __init__(
        self,
        supervisor: Agent,
        *,
        policy: CoordinationPolicy | None = None,
        endpoint: str | None = None,
        api_key: str | None = None,
        state_provider: StateProvider | None = None,
        project_id: str | None = None,
        studio_url: str | None = None,
        poll_timeout_ms: int = 20_000,
    ) -> None:
        self.supervisor = supervisor
        self.policy = policy or CoordinationPolicy()
        self.state_provider = state_provider
        self.project_id = project_id
        self.studio_url = studio_url
        self.poll_timeout_ms = poll_timeout_ms
        self.parent_runtime = Runtime(
            supervisor,
            endpoint=endpoint,
            api_key=api_key,
            state_provider=state_provider,
            project_id=project_id,
            studio_url=studio_url,
            poll_timeout_ms=poll_timeout_ms,
        )
        self.client: RuntimeClient = self.parent_runtime.client

    async def run(
        self,
        parent_goal: Goal,
        tasks: list[AgentTask],
        *,
        input: dict[str, Any] | None = None,
        idempotency_key: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> GroupResult:
        if len(tasks) > self.policy.max_delegations:
            raise ValueError(f"{len(tasks)} tasks exceed max_delegations={self.policy.max_delegations}")
        parent_meta = {
            "coordination": self.policy.to_dict(),
            "role": "supervisor",
            **(metadata or {}),
        }
        created = await self.parent_runtime.create_deferred(
            parent_goal,
            input=input,
            idempotency_key=idempotency_key,
            metadata=parent_meta,
        )
        parent_id = str(created["run_id"])
        children: list[RunResult] = []

        async def one(task: AgentTask) -> RunResult:
            body: dict[str, Any] = {
                "agent": task.agent.to_definition(),
                "goal": task.goal.to_definition(),
                "input": task.input,
                "has_state_provider": (task.state_provider or self.state_provider) is not None,
                "source_framework": "axiom-python",
                "role": task.role,
                "requires_approval": task.requires_approval,
                "metadata": task.metadata,
            }
            if task.idempotency_key:
                body["idempotency_key"] = task.idempotency_key
            child = await self.client.create_task(parent_id, body)
            child_id = str(child["child_run_id"])
            if child.get("approval_id"):
                # Held until a human posts /api/v1/approvals/{id}/decision.
                return await self.parent_runtime.result(child_id)
            rt = Runtime(
                task.agent,
                endpoint=self.client.endpoint,
                api_key=self.client.api_key,
                state_provider=task.state_provider or self.state_provider,
                project_id=self.project_id,
                studio_url=self.studio_url,
                poll_timeout_ms=self.poll_timeout_ms,
            )
            rt.client = self.client
            return await rt.attach(child_id, task.goal)

        if self.policy.mode == "parallel":
            results = await asyncio.gather(*[one(t) for t in tasks], return_exceptions=True)
            for r in results:
                if isinstance(r, BaseException):
                    if self.policy.failure_mode == "fail_fast":
                        raise r
                    continue
                children.append(r)
        else:
            for t in tasks:
                r = await one(t)
                children.append(r)
                if not r.succeeded and self.policy.failure_mode == "fail_fast":
                    break

        await self.client.start_run(parent_id)
        parent = await self.parent_runtime.attach(parent_id, parent_goal)
        hierarchy = await self.client.hierarchy(parent_id)
        return GroupResult(parent=parent, children=children, hierarchy=hierarchy)
