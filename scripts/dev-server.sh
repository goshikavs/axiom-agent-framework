#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ ! -f .env && -f .env.example ]]; then
  cp .env.example .env
fi
export DATABASE_URL="${DATABASE_URL:-embedded}"
exec cargo run -p axiom-server
