# License review (v0.9)

Runtime crates remain Apache-2.0 (`Cargo.toml` workspace license). Third-party crates, PyPI, and npm packages keep their upstream licenses.

Do not add Cursor or AI assistants as authors, co-authors, maintainers, or copyright holders.

Attribution for vendored protocol docs stays with their authors. No copyleft inversion was introduced in this cycle. A full `cargo license` / `pip-licenses` dump should be generated at publish time, not invented here.
