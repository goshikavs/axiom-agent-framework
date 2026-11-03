/**
 * TypeScript refund agent. Tools and predicates run here; the Rust runtime owns the loop.
 *
 *   bun run start
 *   GOSHIKQ_REFUND_FAULT=tool_error bun run start
 *   GOSHIKQ_REFUND_FAULT=never_settles bun run start
 */
import { Agent, DecisionRef, Evidence, Goal, MockModel, Runtime, StateSnapshot, ToolError, Verifier, defineTool, lastToolResult, withEvidence } from "@goshikq/sdk";
import { z } from "zod";
import { demoStore, type Fault } from "./store.ts";

const orderId = process.env.GOSHIKQ_REFUND_ORDER || "ORD-2048";
const fault = (process.env.GOSHIKQ_REFUND_FAULT || "none") as Fault;
const STORE = demoStore(fault);

const lookupOrder = defineTool({
  name: "lookup_order",
  description: "Read the simulated order record.",
  schema: z.object({ order_id: z.string() }),
  jsonSchema: { type: "object", properties: { order_id: { type: "string" } }, required: ["order_id"], additionalProperties: false },
  effect: "READ_ONLY",
  execute: async ({ order_id }) => STORE.lookup(order_id),
});

const checkEligibility = defineTool({
  name: "check_eligibility",
  description: "Policy check against the order.",
  schema: z.object({ order_id: z.string() }),
  jsonSchema: { type: "object", properties: { order_id: { type: "string" } }, required: ["order_id"], additionalProperties: false },
  effect: "READ_ONLY",
  execute: async ({ order_id }) => {
    const row = STORE.eligibility(order_id);
    const order = STORE.orders.get(order_id);
    return withEvidence(
      row,
      [
        new Evidence({
          source_system: "orders-db",
          resource_id: `order:${order_id}`,
          resource_version: order ? String(order.version) : null,
          trust: "authoritative",
          access_scope: "refunds-read",
          keys: [`order:${order_id}`],
        }),
      ],
      new DecisionRef({
        id: `dec:eligibility:${order_id}`,
        kind: "eligibility",
        depends_on: [`order:${order_id}`],
        action: "act:submit_refund",
        payload: { resource_id: `order:${order_id}` },
      }),
    );
  },
});

const submitRefund = defineTool({
  name: "submit_refund",
  description: "Submit a refund. Honors idempotency_key; acceptance is not settlement.",
  schema: z.object({ order_id: z.string(), amount_cents: z.number().int() }),
  jsonSchema: {
    type: "object",
    properties: { order_id: { type: "string" }, amount_cents: { type: "integer" } },
    required: ["order_id", "amount_cents"],
    additionalProperties: false,
  },
  effect: "IDEMPOTENT_WRITE",
  idempotent: true,
  risk: "high",
  permissions: ["payments:refund"],
  requiredEvidence: ["order:{order_id}"],
  requiredInvariants: ["eligibility"],
  onMissingEvidence: "block",
  revalidationPolicy: "strict_contract",
  execute: async ({ order_id, amount_cents }, ctx) => {
    if (STORE.fault === "tool_error") {
      throw new ToolError("simulated processor outage", { code: "processor_unavailable", retryable: false });
    }
    const key = ctx.idempotencyKey || `refund:${order_id}:${amount_cents}`;
    return STORE.submit(order_id, amount_cents, key);
  },
  verifier: (_args, output, state) => {
    const payload = (output ?? {}) as Record<string, unknown>;
    const refund = (payload.refund ?? {}) as Record<string, unknown>;
    if (payload.accepted && !payload.settled) {
      return Verifier.waiting("processor accepted; settlement pending", {
        expected: { refund_status: "settled" },
        actual: refund.status ?? null,
        method: "authoritative-store",
      });
    }
    const orders = (state.get("orders") as Record<string, { status?: string; refund_id?: string }> | undefined) ?? {};
    const order = orders[String(_args.order_id)];
    const settled = order?.status === "refunded";
    return settled
      ? Verifier.verified({ expected: { order_status: "refunded" }, actual: order?.status, method: "authoritative-store" })
      : Verifier.failed("order not refunded in authoritative state", { expected: { order_status: "refunded" }, actual: order?.status });
  },
});

const getRefundStatus = defineTool({
  name: "get_refund_status",
  description: "Poll settlement. Required after an accepted (not settled) submit.",
  schema: z.object({ refund_id: z.string() }),
  jsonSchema: { type: "object", properties: { refund_id: { type: "string" } }, required: ["refund_id"], additionalProperties: false },
  effect: "READ_ONLY",
  execute: async ({ refund_id }) => STORE.getRefund(refund_id),
});

function refundCompleted(state: StateSnapshot) {
  const orders = (state.get("orders") as Record<string, { status?: string; refund_id?: string }> | undefined) ?? {};
  const target = orders[orderId];
  if (!target) return { satisfied: false, reason: `order ${orderId} missing`, expected: "present", actual: null };
  const refunds = (state.get("refunds") as Record<string, { status?: string }> | undefined) ?? {};
  const refund = refunds[target.refund_id || ""];
  const settled = target.status === "refunded" && refund?.status === "settled";
  return {
    satisfied: settled,
    expected: { "order.status": "refunded", "refund.status": "settled" },
    actual: { "order.status": target.status, "refund.status": refund?.status ?? null },
    reason: settled ? null : "refund has not settled against authoritative state",
  };
}

function refundNotDuplicated(state: StateSnapshot) {
  const refunds = Object.values((state.get("refunds") as Record<string, { order_id?: string; status?: string }> | undefined) ?? {});
  const matches = refunds.filter((r) => r.order_id === orderId && r.status !== "rejected");
  const ok = matches.length <= 1;
  return { satisfied: ok, expected: { max_refunds: 1 }, actual: matches.length, reason: ok ? null : "duplicate refund records" };
}

function amountWithinLimit(state: StateSnapshot) {
  const limit = Number(state.get("authorized_limit_cents") ?? 0);
  const refunds = Object.values((state.get("refunds") as Record<string, { amount_cents?: number }> | undefined) ?? {});
  for (const r of refunds) {
    if ((r.amount_cents ?? 0) > limit) {
      return { satisfied: false, expected: { max: limit }, actual: r.amount_cents, reason: "amount exceeds authorized limit" };
    }
  }
  return { satisfied: true, expected: { max: limit }, actual: "ok" };
}

function refundPolicy(messages: Record<string, unknown>[], _tools: Record<string, unknown>[], idx: number) {
  if (idx === 0) return { tool_calls: [["lookup_order", { order_id: orderId }] as [string, Record<string, unknown>]] };
  if (idx === 1) return { tool_calls: [["check_eligibility", { order_id: orderId }] as [string, Record<string, unknown>]] };
  const elig = lastToolResult(messages, "check_eligibility") || {};
  const inner = (elig.result as Record<string, unknown> | undefined) || elig;
  if (!inner.eligible) return { content: `Cannot refund ${orderId}: ${inner.reason}` };
  if (idx === 2) {
    return { tool_calls: [["submit_refund", { order_id: orderId, amount_cents: inner.amount_cents }] as [string, Record<string, unknown>]] };
  }
  const submitted = lastToolResult(messages, "submit_refund") || {};
  const payload = (submitted.result as Record<string, unknown> | undefined) || submitted;
  const refund = (payload.refund as Record<string, unknown> | undefined) || {};
  if (payload.accepted && !payload.settled && refund.id) {
    return { tool_calls: [["get_refund_status", { refund_id: refund.id }] as [string, Record<string, unknown>]] };
  }
  if (refund.status === "settled" || payload.settled) return { content: `Refund ${refund.id} settled for ${orderId}.` };
  return { content: `Refund did not settle: ${JSON.stringify(payload)}` };
}

const agent = new Agent({
  name: "ts_refund_agent",
  model: new MockModel({ policy: refundPolicy }),
  instructions: "Resolve valid customer refunds. Never claim success without tool evidence.",
  tools: [lookupOrder, checkEligibility, submitRefund, getRefundStatus],
  permissions: ["payments:refund"],
});

const goal = new Goal({
  description: `Resolve refund for ${orderId}`,
  success: refundCompleted,
  invariants: [refundNotDuplicated, amountWithinLimit],
  maxSteps: 10,
  externalWaitSeconds: fault === "never_settles" ? 8 : 20,
});

const runtime = new Runtime(agent, {
  stateProvider: {
    snapshot: () =>
      new StateSnapshot({
        state: STORE.snapshot(),
        version: JSON.stringify(Object.fromEntries([...STORE.orders].map(([id, o]) => [id, o.version]))),
        source: "simulated-orders+processor",
      }),
  },
  studioUrl: process.env.GOSHIKQ_STUDIO_URL || "http://127.0.0.1:3000",
});

const result = await runtime.run(goal, { input: { order_id: orderId, fault } });
console.log(result.summary());
console.log(`run_id=${result.runId}`);
console.log(`status=${result.status} verification=${result.verificationStatus}`);
if (result.studioUrl) console.log(`studio=${result.studioUrl}`);
if (result.failureReason) console.log(`failure=${result.failureReason}`);
if (!result.verified && fault === "none") process.exitCode = 1;
