# Troubleshooting

See the table in the root README. Additional notes:

- Windows ARM64: compile `--target x86_64-pc-windows-msvc` (or `cargo +stable-x86_64-pc-windows-msvc`) after `vcvarsall.bat x64`.
- Embedded Postgres data: `.goshikq/postgres` (gitignored). Delete it to re-initialize.
- If Studio shows "Runtime unreachable", confirm `GET http://127.0.0.1:7400/health`.
