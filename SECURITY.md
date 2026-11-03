# Security policy

Report vulnerabilities privately to the repository owner (GitHub: goshikavs). Do not open public issues with exploit details.

This project is `0.1.0-alpha`. Known residual risks are listed in `docs/security/threat-model.md`. Prompt injection is not solved. `GOSHIKQ_AUTH_MODE=none` is local development only.

`cargo audit` ignores `RUSTSEC-2023-0071` (`rsa` 0.9, Marvin) because it is pulled by unused `sqlx-mysql` and Goshikq does not RSA-decrypt untrusted ciphertexts. See `.cargo/audit.toml`.
