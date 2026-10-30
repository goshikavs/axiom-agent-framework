import { cn } from "@/lib/utils";

const RUN: Record<string, string> = {
  CREATED: "bg-slate-800 text-slate-300",
  PLANNING: "bg-sky-950 text-sky-300",
  VALIDATING_ACTION: "bg-sky-950 text-sky-300",
  WAITING_FOR_APPROVAL: "bg-amber-950 text-amber-300",
  EXECUTING: "bg-blue-950 text-blue-300",
  RECONCILING_STATE: "bg-indigo-950 text-indigo-300",
  VERIFYING_GOAL: "bg-violet-950 text-violet-300",
  REPLANNING: "bg-orange-950 text-orange-300",
  PAUSED: "bg-amber-950 text-amber-300",
  RECOVERY_REQUIRED: "bg-fuchsia-950 text-fuchsia-300",
  COMPLETED: "bg-emerald-950 text-emerald-300",
  UNVERIFIED: "bg-amber-950 text-amber-200",
  FAILED: "bg-red-950 text-red-300",
  CANCELLED: "bg-slate-800 text-slate-400",
  TIMED_OUT: "bg-red-950 text-red-300",
  BUDGET_EXCEEDED: "bg-red-950 text-red-300",
  STEP_LIMIT_EXCEEDED: "bg-red-950 text-red-300",
  MODEL_BUDGET_EXCEEDED: "bg-red-950 text-red-300",
  WAITING_FOR_EXTERNAL_STATE: "bg-amber-950 text-amber-300",
};

const VERIFY: Record<string, string> = {
  VERIFIED: "bg-emerald-950 text-emerald-300",
  FAILED: "bg-red-950 text-red-300",
  INCONCLUSIVE: "bg-amber-950 text-amber-200",
  NOT_EVALUATED: "bg-slate-800 text-slate-400",
};

const TIPS: Record<string, string> = {
  UNVERIFIED: "The run finished without an authoritative Axiom outcome verifier. Importing a trace is not verification.",
  VERIFIED: "An Axiom verifier matched expected vs observed authoritative state.",
  NOT_EVALUATED: "No Axiom outcome verifier ran for this run.",
  INCONCLUSIVE: "A verifier ran but could not confirm or deny the goal from observed evidence.",
  COMPLETED: "Runtime reached a terminal success status. Independent of goal verification.",
  FAILED: "Runtime or verification recorded a failure.",
  RECOVERY_REQUIRED: "The previous runtime process left this run active; no action was re-executed automatically.",
  WAITING_FOR_EXTERNAL_STATE: "Waiting for an authoritative snapshot of an external operation.",
  DISPATCH_UNCERTAIN: "Whether the write reached the external system is unknown.",
  OUTCOME_CONFIRMED: "Authoritative state shows the external operation settled.",
  OUTCOME_PENDING: "The processor accepted the request; settlement is not yet observed.",
};

export function StatusBadge({ value, kind = "run" }: { value: string; kind?: "run" | "verify" }) {
  const map = kind === "verify" ? VERIFY : RUN;
  return (
    <span
      title={TIPS[value] ?? `${kind} status ${value}`}
      className={cn("inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[11px] tracking-wide", map[value] ?? "bg-slate-800 text-slate-300")}
    >
      {value}
    </span>
  );
}
