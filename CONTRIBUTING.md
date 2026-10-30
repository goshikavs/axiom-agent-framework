# Contributing

Maintainer: [goshikavs](https://github.com/goshikavs).

1. Open an issue before large design changes.
2. Keep the vertical slice runnable: runtime + Python SDK + mock model + Studio.
3. Do not mark planned features as available. Update `docs/IMPLEMENTATION_STATUS.md`.
4. Tests: `cargo test --workspace`, `pytest` in `sdk/python`, `bun test` in `sdk/typescript`.
5. On Windows without `link.exe` on PATH, use `scripts/with-msvc.ps1`.
6. Do not commit `.env`, credentials, or captured production traces.
7. Third-party framework adapters must declare supported and unsupported capabilities explicitly.

License: Apache-2.0.
