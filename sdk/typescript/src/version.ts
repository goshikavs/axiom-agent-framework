export const VERSION = "0.1.0-alpha";
export const PROTOCOL_VERSION = "1.0";
export const SOURCE_FRAMEWORK = "axiom-typescript";
export const WORKER_CAPABILITIES = [
  "model.generate",
  "tool.execute",
  "state.snapshot",
  "goal.evaluate",
  "action.verify",
] as const;
