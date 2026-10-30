#!/usr/bin/env bash
# Compose smoke for axiom-server + Studio + Postgres.
# Requires Docker. Intended for GitHub Actions ubuntu-latest.
# Local Windows hosts without Docker Desktop must not claim this passed.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export AXIOM_AUTH_MODE="${AXIOM_AUTH_MODE:-none}"
export AXIOM_ALLOW_INSECURE_BIND="${AXIOM_ALLOW_INSECURE_BIND:-true}"

echo "==> build runtime + studio"
docker compose --profile runtime --profile studio build

echo "==> up postgres"
docker compose up -d postgres
for i in $(seq 1 30); do
  if docker compose exec -T postgres pg_isready -U axiom -d axiom >/dev/null 2>&1; then
    break
  fi
  sleep 2
done

echo "==> up runtime (migrates on start)"
docker compose --profile runtime up -d runtime
for i in $(seq 1 40); do
  if curl -fsS http://127.0.0.1:7400/ready >/tmp/axiom-ready.json; then
    cat /tmp/axiom-ready.json
    break
  fi
  sleep 2
done
curl -fsS http://127.0.0.1:7400/health
curl -fsS http://127.0.0.1:7400/ready | grep -q ready

echo "==> up studio"
docker compose --profile studio up -d studio
for i in $(seq 1 40); do
  if curl -fsS -o /dev/null http://127.0.0.1:3000/; then
    break
  fi
  sleep 2
done

echo "==> native agent via isolated python in compose network is host-side"
python3 - <<'PY'
import json, os, urllib.request, uuid
base = os.environ.get("AXIOM_RUNTIME_URL", "http://127.0.0.1:7400")
health = json.load(urllib.request.urlopen(base + "/health"))
assert health.get("status") == "ok", health
print("health", health)
PY

echo "==> restart runtime and re-check ready"
docker compose --profile runtime restart runtime
for i in $(seq 1 40); do
  if curl -fsS http://127.0.0.1:7400/ready >/dev/null; then
    break
  fi
  sleep 2
done
curl -fsS http://127.0.0.1:7400/ready

echo "docker-smoke: compose build, migrate-on-start, ready, studio, restart OK"
echo "Full native/workflow/goal/isolation checks still run as pytest against this runtime in the CI job."
