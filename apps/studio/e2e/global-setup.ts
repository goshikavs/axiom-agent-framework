import { randomUUID } from "node:crypto";

const RUNTIME = (process.env.AXIOM_RUNTIME_URL || "http://127.0.0.1:7400").replace(/\/$/, "");
export const FIXTURE_AGENT = "pw-missing-telemetry";

function otlpMissing(traceId: string) {
  const now = `${Date.now()}000000`;
  return {
    resourceSpans: [
      {
        resource: {
          attributes: [
            { key: "service.name", value: { stringValue: FIXTURE_AGENT } },
            { key: "axiom.source_framework", value: { stringValue: "otel" } },
          ],
        },
        scopeSpans: [
          {
            scope: { name: "axiom.pw-fixture", version: "otel-genai-v1" },
            spans: [
              {
                traceId,
                spanId: randomUUID().replace(/-/g, "").slice(0, 16),
                name: "chat",
                kind: 3,
                startTimeUnixNano: now,
                endTimeUnixNano: now,
                status: { code: 1 },
                attributes: [
                  { key: "gen_ai.operation.name", value: { stringValue: "chat" } },
                  { key: "gen_ai.request.model", value: { stringValue: "fixture-model" } },
                  { key: "gen_ai.agent.name", value: { stringValue: FIXTURE_AGENT } },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
}

export default async function globalSetup() {
  const traceId = randomUUID().replace(/-/g, "");
  const resp = await fetch(`${RUNTIME}/v1/traces`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(otlpMissing(traceId)),
  });
  if (!resp.ok) {
    throw new Error(`failed to seed missing-telemetry fixture: ${resp.status} ${await resp.text()}`);
  }
  process.env.PLAYWRIGHT_MISSING_TRACE = traceId;
}
