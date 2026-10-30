export type SdkOrigin = {
  label: string;
  native: boolean;
  controlsExecution: boolean;
};

export function sdkOrigin(framework: string | null | undefined): SdkOrigin {
  switch (framework) {
    case "axiom-python":
      return { label: "Native Python SDK", native: true, controlsExecution: true };
    case "axiom-typescript":
      return { label: "Native TypeScript SDK", native: true, controlsExecution: true };
    case "axiom":
      return { label: "Native Axiom runtime", native: true, controlsExecution: true };
    case "langgraph":
      return { label: "LangGraph", native: false, controlsExecution: false };
    case "openai-agents":
      return { label: "OpenAI Agents SDK", native: false, controlsExecution: false };
    case "otel":
      return { label: "External OTLP", native: false, controlsExecution: false };
    case "mcp":
      return { label: "MCP", native: false, controlsExecution: false };
    default:
      return { label: framework || "unknown", native: false, controlsExecution: false };
  }
}

export function mcpPayload(payload: unknown): Record<string, unknown> | null {
  if (!payload || typeof payload !== "object") return null;
  const p = payload as Record<string, unknown>;
  if (p._mcp && typeof p._mcp === "object") return p._mcp as Record<string, unknown>;
  const output = p.output;
  if (output && typeof output === "object" && "_mcp" in output) {
    return (output as Record<string, unknown>)._mcp as Record<string, unknown>;
  }
  const evidence = p._evidence ?? (output && typeof output === "object" ? (output as Record<string, unknown>)._evidence : null);
  if (Array.isArray(evidence) && evidence.some((e) => typeof e === "object" && String((e as { source_system?: string }).source_system ?? "").startsWith("mcp:"))) {
    return { server_id: String((evidence[0] as { source_system?: string }).source_system), inferred: false };
  }
  return null;
}
