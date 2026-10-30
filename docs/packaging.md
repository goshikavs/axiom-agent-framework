# Packaging

Nothing is published to PyPI, npm, or crates.io in v0.9.1.

## Python (`sdk/python`)

- `pyproject.toml` with hatchling, version `0.1.0a1`
- Optional extras: `openai`, `anthropic`, `dev`
- Isolated install (temp venv **outside** the repo): `python -m pytest tests/packaging/test_isolated_install.py`
- Local smoke: `python -m build` then `pip install dist/*.whl` and `python -c "import axiom_ai"`

## TypeScript (`sdk/typescript`)

- ESM package `@axiom-ai/sdk` shipping TypeScript sources for Bun
- `engines.node: >=20`; Node consumers should use Bun or a TypeScript loader. A compiled `dist/` emit is not the supported path in this alpha
- Local smoke: `bun pm pack` then `bun add ./axiom-ai-sdk-0.1.0-alpha.tgz`

## Rust runtime

- Workspace version `0.1.0-alpha`
- `GET /health` reports `version`
- Platform: Windows x64 MSVC tested; Linux/macOS are expected to build but are not the recorded host for this milestone

## Studio

- `bun run build` in `apps/studio` for a production Next.js build
- Runtime URL: `AXIOM_STUDIO_RUNTIME_URL` or `AXIOM_RUNTIME_URL` (default `http://127.0.0.1:7400`)
- Migrations are additive (`0001`–`0006`); existing databases migrate on server start
