export type OrderStatus = "paid" | "return_started" | "refund_pending" | "refunded" | "refund_failed";
export type RefundStatus = "accepted" | "settled" | "rejected";
export type Fault = "none" | "processor_reject" | "delayed_settlement" | "never_settles" | "tool_error";

export type Order = {
  id: string;
  customer: string;
  amount_cents: number;
  currency: string;
  paid_at: string;
  status: OrderStatus;
  eligible: boolean;
  version: number;
  refund_id: string | null;
  return_status: string;
};

export type Refund = {
  id: string;
  order_id: string;
  amount_cents: number;
  status: RefundStatus;
  idempotency_key: string;
  created_at: string;
  settled_at: string | null;
  reject_reason: string | null;
};

export class SimulatedStore {
  orders = new Map<string, Order>();
  refunds = new Map<string, Refund>();
  refundsByKey = new Map<string, string>();
  fault: Fault = "none";
  settlementDelaySeconds = 0;
  authorizedLimitCents = 20_000;
  submitCount = 0;
  private acceptedAt = new Map<string, number>();

  snapshot(): Record<string, unknown> {
    this.maybeSettle();
    const orders: Record<string, Order> = {};
    for (const [k, v] of this.orders) orders[k] = { ...v };
    const refunds: Record<string, Refund> = {};
    for (const [k, v] of this.refunds) refunds[k] = { ...v };
    return {
      orders,
      refunds,
      authorized_limit_cents: this.authorizedLimitCents,
      fault: this.fault,
      _resources: Object.keys(orders).map((oid) => ({
        resource_id: `order:${oid}`,
        version: String(orders[oid]!.version),
        source_system: "orders-db",
      })),
    };
  }

  lookup(orderId: string): Record<string, unknown> {
    const order = this.orders.get(orderId);
    if (!order) return { found: false, order_id: orderId };
    return { found: true, ...order };
  }

  eligibility(orderId: string): Record<string, unknown> {
    const order = this.orders.get(orderId);
    if (!order) return { eligible: false, reason: "unknown_order" };
    if (!order.eligible) return { eligible: false, reason: "policy_ineligible", order_status: order.status };
    if (order.status === "refunded" || order.status === "refund_pending") {
      return { eligible: false, reason: `already_${order.status}`, refund_id: order.refund_id };
    }
    return { eligible: true, amount_cents: order.amount_cents, currency: order.currency };
  }

  submit(orderId: string, amountCents: number, idempotencyKey: string): Record<string, unknown> {
    this.submitCount += 1;
    const existing = this.refundsByKey.get(idempotencyKey);
    if (existing) {
      return { duplicate: true, accepted: true, settled: this.refunds.get(existing)?.status === "settled", refund: this.refunds.get(existing) };
    }
    const order = this.orders.get(orderId);
    if (!order) return { accepted: false, reason: "unknown_order" };
    if (amountCents > this.authorizedLimitCents) return { accepted: false, reason: "over_authorized_limit" };
    if (this.fault === "processor_reject") {
      const refund = this.record(orderId, amountCents, idempotencyKey, "rejected", "processor_declined");
      order.status = "refund_failed";
      order.version += 1;
      return { accepted: false, reason: "processor_declined", refund };
    }
    const pending = this.fault === "delayed_settlement" || this.fault === "never_settles";
    const refund = this.record(orderId, amountCents, idempotencyKey, pending ? "accepted" : "settled", null);
    order.refund_id = refund.id;
    order.status = pending ? "refund_pending" : "refunded";
    order.version += 1;
    if (pending && this.fault !== "never_settles") this.acceptedAt.set(refund.id, Date.now());
    return { accepted: true, settled: refund.status === "settled", refund };
  }

  getRefund(refundId: string): Record<string, unknown> {
    this.maybeSettle();
    const r = this.refunds.get(refundId);
    if (!r) return { found: false, refund_id: refundId };
    return { found: true, ...r };
  }

  private record(orderId: string, amount: number, key: string, status: RefundStatus, reason: string | null): Refund {
    const id = `rf-${this.refunds.size + 1}`;
    const refund: Refund = {
      id,
      order_id: orderId,
      amount_cents: amount,
      status,
      idempotency_key: key,
      created_at: new Date().toISOString(),
      settled_at: status === "settled" ? new Date().toISOString() : null,
      reject_reason: reason,
    };
    this.refunds.set(id, refund);
    this.refundsByKey.set(key, id);
    return refund;
  }

  private maybeSettle(): void {
    if (this.fault === "never_settles") return;
    const now = Date.now();
    for (const [id, t0] of [...this.acceptedAt]) {
      if ((now - t0) / 1000 >= this.settlementDelaySeconds) {
        const r = this.refunds.get(id);
        if (r) {
          r.status = "settled";
          r.settled_at = new Date().toISOString();
          const order = this.orders.get(r.order_id);
          if (order) {
            order.status = "refunded";
            order.version += 1;
          }
        }
        this.acceptedAt.delete(id);
      }
    }
  }
}

export function demoStore(fault: Fault = "none"): SimulatedStore {
  const store = new SimulatedStore();
  store.fault = fault;
  if (fault === "delayed_settlement") store.settlementDelaySeconds = 1.5;
  if (fault === "never_settles") store.settlementDelaySeconds = 1e9;
  store.orders.set("ORD-2048", {
    id: "ORD-2048",
    customer: "Ada Lovelace",
    amount_cents: 4800,
    currency: "USD",
    paid_at: "2026-04-01T10:00:00+00:00",
    status: "paid",
    eligible: true,
    version: 3,
    refund_id: null,
    return_status: "received",
  });
  store.orders.set("ORD-LOCKED", {
    id: "ORD-LOCKED",
    customer: "Grace Hopper",
    amount_cents: 9900,
    currency: "USD",
    paid_at: "2026-01-01T10:00:00+00:00",
    status: "paid",
    eligible: false,
    version: 1,
    refund_id: null,
    return_status: "received",
  });
  return store;
}
