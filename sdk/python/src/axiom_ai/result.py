"""Run results as reported by the runtime (never by the model)."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

TERMINAL_STATUSES = {
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


@dataclass
class RunResult:
    run_id: str
    status: str
    verification_status: str
    trace_id: str
    final_output: str | None = None
    failure_reason: str | None = None
    step_count: int = 0
    input_tokens: int = 0
    output_tokens: int = 0
    usage_known: bool = False
    estimated_cost_usd: float | None = None
    cost_known: bool = False
    duration_ms: int | None = None
    studio_url: str | None = None
    tool_status: str | None = None
    dispatch_state: str | None = None
    business_operation_status: str | None = None
    raw: dict[str, Any] = field(default_factory=dict, repr=False)

    @property
    def verified(self) -> bool:
        """True only when the runtime completed the run AND the goal was verified against state."""
        return self.status == "COMPLETED" and self.verification_status == "VERIFIED"

    @property
    def succeeded(self) -> bool:
        return self.status == "COMPLETED"

    @property
    def terminal(self) -> bool:
        return self.status in TERMINAL_STATUSES

    @classmethod
    def from_api(cls, run: dict[str, Any], studio_url: str | None = None) -> RunResult:
        return cls(
            run_id=str(run["id"]),
            status=run["status"],
            verification_status=run.get("verification_status", "NOT_EVALUATED"),
            trace_id=run.get("trace_id", ""),
            final_output=run.get("final_output"),
            failure_reason=run.get("failure_reason"),
            step_count=int(run.get("step_count") or 0),
            input_tokens=int(run.get("input_tokens") or 0),
            output_tokens=int(run.get("output_tokens") or 0),
            usage_known=bool(run.get("usage_known", False)),
            estimated_cost_usd=run.get("estimated_cost_usd"),
            cost_known=bool(run.get("cost_known", False)),
            duration_ms=run.get("duration_ms"),
            studio_url=studio_url,
            tool_status=run.get("tool_status"),
            dispatch_state=run.get("dispatch_state"),
            business_operation_status=run.get("business_operation_status"),
            raw=run,
        )

    def summary(self) -> str:
        cost = f"${self.estimated_cost_usd:.6f}" if self.cost_known and self.estimated_cost_usd is not None else "unknown"
        tokens = f"{self.input_tokens + self.output_tokens}" if self.usage_known else f">= {self.input_tokens + self.output_tokens} (partial)"
        lines = [
            f"run_id:        {self.run_id}",
            f"status:        {self.status}",
            f"verification:  {self.verification_status}",
            f"steps:         {self.step_count}",
            f"tokens:        {tokens}",
            f"est. cost:     {cost}",
        ]
        if self.tool_status:
            lines.append(f"tool_status:   {self.tool_status}")
        if self.dispatch_state:
            lines.append(f"dispatch:      {self.dispatch_state}")
        if self.business_operation_status:
            lines.append(f"business_op:   {self.business_operation_status}")
        if self.failure_reason:
            lines.append(f"failure:       {self.failure_reason}")
        if self.final_output:
            lines.append(f"output:        {self.final_output}")
        if self.studio_url:
            lines.append(f"studio:        {self.studio_url}")
        return "\n".join(lines)
