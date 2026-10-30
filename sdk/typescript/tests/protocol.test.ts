import { describe, expect, test } from "bun:test";
import { PROTOCOL_VERSION, SOURCE_FRAMEWORK, TERMINAL_STATUSES, WORKER_CAPABILITIES } from "../src";

describe("cross-language protocol constants", () => {
  test("terminal statuses match the runtime contract", () => {
    for (const s of [
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
    ]) {
      expect(TERMINAL_STATUSES.has(s)).toBe(true);
    }
  });

  test("worker capabilities are the five ADR-001 kinds", () => {
    expect([...WORKER_CAPABILITIES]).toEqual([
      "model.generate",
      "tool.execute",
      "state.snapshot",
      "goal.evaluate",
      "action.verify",
    ]);
  });

  test("source framework and protocol version", () => {
    expect(SOURCE_FRAMEWORK).toBe("axiom-typescript");
    expect(PROTOCOL_VERSION).toBe("1.0");
  });
});
