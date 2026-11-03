# Authentication

Production mode is `GOSHIKQ_AUTH_MODE=oidc`. Local development may use `none` on loopback or `api_key` for service identities.

## Modes

| Mode | Who | Binding |
|---|---|---|
| `none` | Single local owner (`local-user`, role `owner`) | Loopback only unless `GOSHIKQ_ALLOW_INSECURE_BIND=true` |
| `api_key` | Hashed keys in `api_keys` | Bearer or `X-Api-Key` |
| `oidc` | JWT users + optional API keys | `Authorization: Bearer` |

`none` is not a production setting.

## OIDC / OAuth2

Validated with `jsonwebtoken` (issuer, audience, expiry, nbf, signature). Signing keys come from `GOSHIKQ_OIDC_JWKS_URL` (HTTPS, or loopback HTTP in tests). Production algorithms: RS256/RS384/RS512, ES256/ES384, PS256/PS384/PS512. `alg=none` and HMAC are rejected unless loopback HMAC test mode is on.

Unknown `kid` forces a JWKS re-fetch so key rotation does not wait for the cache TTL (`GOSHIKQ_OIDC_JWKS_CACHE_SECONDS`, default 300).

Goshikq does **not** implement OAuth token revocation. A valid JWT works until `exp`. Cut access by deleting `project_members` or revoking API keys.

RS256 suite: `python -m pytest tests/security/test_oidc_jwks.py`. Hosted IdP (Entra/Keycloak/Auth0) has not been run in this milestone.

Required environment:

```text
GOSHIKQ_AUTH_MODE=oidc
GOSHIKQ_OIDC_ISSUER=https://idp.example.com/realms/goshikq
GOSHIKQ_OIDC_AUDIENCE=goshikq
GOSHIKQ_OIDC_JWKS_URL=https://idp.example.com/realms/goshikq/protocol/openid-connect/certs
```

Claims used:

- `sub` — user identity (must match `project_members.subject`)
- `iss` / `aud` / `exp` / `nbf` — standard validation
- optional `goshikq_project_id`, `goshikq_tenant_id` — hints; membership in PostgreSQL is authoritative

Project selection: `X-Goshikq-Project-Id`, else the claim, else the default project. Missing membership is `403`.

### Loopback HMAC (tests only)

`GOSHIKQ_OIDC_ALLOW_TEST_HMAC=true` plus `GOSHIKQ_OIDC_HMAC_SECRET` is refused on non-loopback binds. Use `GOSHIKQ_OIDC_BOOTSTRAP_OWNER` to seed an owner on the default project at startup.

## API keys (service identities)

Keys are stored as SHA-256 hashes. Plaintext is returned once. Fields: `role`, `kind`, `expires_at`, `rotated_from`.

```http
POST /api/v1/api-keys
POST /api/v1/api-keys/{id}/rotate
POST /api/v1/api-keys/{id}/revoke
```

Expired or revoked keys fail authentication. In `oidc` mode a token starting with `axk_` is treated as an API key.

Studio never sends provider credentials to the browser. The Next.js proxy may attach `GOSHIKQ_STUDIO_API_KEY` server-side and forwards `Authorization`, `X-Api-Key`, and `X-Goshikq-Project-Id`.
