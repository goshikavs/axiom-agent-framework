"""RS256 JWKS interoperability: issuer, audience, expiry, signature, rotation, isolation.

HMAC test mode is not enabled. Signing keys come from a loopback JWKS HTTP server.
"""

from __future__ import annotations

import base64
import json
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

import httpx
import pytest

from tests.helpers import free_port, start_server, stop, wait_ready

cryptography = pytest.importorskip("cryptography")
from cryptography.hazmat.primitives import hashes  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import padding, rsa  # noqa: E402

ISSUER = "http://127.0.0.1/idp"
AUD = "axiom"


def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def int_b64url(n: int) -> str:
    length = (n.bit_length() + 7) // 8
    return b64url(n.to_bytes(length, "big"))


class KeyPair:
    def __init__(self, kid: str) -> None:
        self.kid = kid
        self.private = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        pub = self.private.public_key().public_numbers()
        self.jwk = {
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": kid,
            "n": int_b64url(pub.n),
            "e": int_b64url(pub.e),
        }

    def mint(self, sub: str, extra: dict | None = None, alg: str = "RS256", exp_delta: int = 3600) -> str:
        header = {"alg": alg, "typ": "JWT", "kid": self.kid}
        payload = {
            "sub": sub,
            "iss": ISSUER,
            "aud": AUD,
            "exp": int(time.time()) + exp_delta,
            "nbf": int(time.time()) - 5,
        }
        if extra:
            payload.update(extra)
        h = b64url(json.dumps(header, separators=(",", ":")).encode())
        p = b64url(json.dumps(payload, separators=(",", ":")).encode())
        signing_input = f"{h}.{p}".encode()
        if alg == "none":
            return f"{h}.{p}."
        sig = self.private.sign(signing_input, padding.PKCS1v15(), hashes.SHA256())
        return f"{h}.{p}.{b64url(sig)}"


class JwksServer:
    def __init__(self) -> None:
        self.keys: list[dict] = []
        self.port = free_port()
        parent = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                body = json.dumps({"keys": parent.keys}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_args) -> None:
                return

        self.httpd = HTTPServer(("127.0.0.1", self.port), Handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    def start(self) -> None:
        self.thread.start()

    def stop(self) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}/jwks"


@pytest.fixture
def jwks_runtime():
    k1 = KeyPair("k1")
    jwks = JwksServer()
    jwks.keys = [k1.jwk]
    jwks.start()
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(
        port,
        {
            "AXIOM_AUTH_MODE": "oidc",
            "AXIOM_OIDC_ISSUER": ISSUER,
            "AXIOM_OIDC_AUDIENCE": AUD,
            "AXIOM_OIDC_JWKS_URL": jwks.url,
            "AXIOM_OIDC_BOOTSTRAP_OWNER": "alice",
            "AXIOM_OIDC_JWKS_CACHE_SECONDS": "300",
        },
    )
    try:
        wait_ready(url)
        yield url, k1, jwks
    finally:
        stop(proc)
        jwks.stop()


def auth(token: str) -> dict[str, str]:
    return {"Authorization": f"Bearer {token}"}


def test_rs256_whoami_and_membership(jwks_runtime):
    url, k1, _jwks = jwks_runtime
    token = k1.mint("alice")
    with httpx.Client() as c:
        r = c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5)
        assert r.status_code == 200, r.text
        body = r.json()
        assert body["subject"] == "alice"
        assert body["role"] == "owner"
        assert body["auth_mode"] == "oidc"


def test_rejects_wrong_issuer(jwks_runtime):
    url, k1, _ = jwks_runtime
    token = k1.mint("alice", extra={"iss": "https://evil.example"})
    with httpx.Client() as c:
        assert c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5).status_code == 401


def test_rejects_wrong_audience(jwks_runtime):
    url, k1, _ = jwks_runtime
    token = k1.mint("alice", extra={"aud": "other-api"})
    with httpx.Client() as c:
        assert c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5).status_code == 401


def test_rejects_expired(jwks_runtime):
    url, k1, _ = jwks_runtime
    token = k1.mint("alice", exp_delta=-120)
    with httpx.Client() as c:
        assert c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5).status_code == 401


def test_rejects_invalid_signature(jwks_runtime):
    url, k1, _ = jwks_runtime
    token = k1.mint("alice")
    tampered = token[:-4] + ("AAAA" if not token.endswith("AAAA") else "BBBB")
    with httpx.Client() as c:
        assert c.get(f"{url}/api/v1/whoami", headers=auth(tampered), timeout=5).status_code == 401


def test_rejects_hmac_when_jwks_mode(jwks_runtime):
    url, _k1, _ = jwks_runtime
    header = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    payload = b64url(
        json.dumps(
            {"sub": "alice", "iss": ISSUER, "aud": AUD, "exp": int(time.time()) + 60},
            separators=(",", ":"),
        ).encode()
    )
    token = f"{header}.{payload}.c2ln"
    with httpx.Client() as c:
        assert c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5).status_code == 401


def test_rejects_alg_none(jwks_runtime):
    url, k1, _ = jwks_runtime
    token = k1.mint("alice", alg="none")
    with httpx.Client() as c:
        assert c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5).status_code == 401


def test_key_rotation_unknown_kid_refreshes(jwks_runtime):
    url, k1, jwks = jwks_runtime
    k2 = KeyPair("k2")
    jwks.keys = [k2.jwk]
    token = k2.mint("alice")
    with httpx.Client() as c:
        r = c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5)
        assert r.status_code == 200, r.text
        old = k1.mint("alice")
        assert c.get(f"{url}/api/v1/whoami", headers=auth(old), timeout=5).status_code == 401


def test_role_mapping_and_unknown_subject(jwks_runtime):
    url, k1, _ = jwks_runtime
    alice = k1.mint("alice")
    bob_sub = f"bob-{uuid.uuid4().hex[:8]}"
    bob = k1.mint(bob_sub)
    with httpx.Client() as c:
        denied = c.get(f"{url}/api/v1/whoami", headers=auth(bob), timeout=5)
        assert denied.status_code == 403, denied.text
        members = c.post(
            f"{url}/api/v1/members",
            headers=auth(alice),
            json={"subject": bob_sub, "role": "viewer"},
            timeout=5,
        )
        assert members.status_code == 200, members.text
        who = c.get(f"{url}/api/v1/whoami", headers=auth(bob), timeout=5)
        assert who.status_code == 200, who.text
        assert who.json()["role"] == "viewer"
        forbidden = c.post(
            f"{url}/api/v1/api-keys",
            headers=auth(bob),
            json={"name": "nope"},
            timeout=5,
        )
        assert forbidden.status_code == 403


def test_cross_tenant_denied(jwks_runtime):
    url, k1, _ = jwks_runtime
    alice = k1.mint("alice")
    with httpx.Client() as c:
        proj = c.post(
            f"{url}/api/v1/projects",
            headers=auth(alice),
            json={"name": "Tenant B", "slug": f"tenant-b-{uuid.uuid4().hex[:8]}"},
            timeout=5,
        )
        assert proj.status_code == 201, proj.text
        b_id = proj.json()["id"]
        carol_sub = f"carol-{uuid.uuid4().hex[:8]}"
        c.post(
            f"{url}/api/v1/members",
            headers=auth(alice),
            json={"subject": carol_sub, "role": "operator"},
            timeout=5,
        )
        carol = k1.mint(carol_sub)
        r = c.get(
            f"{url}/api/v1/runs",
            headers={**auth(carol), "X-Axiom-Project-Id": b_id},
            timeout=5,
        )
        assert r.status_code in (401, 403)


def test_jwt_revocation_limitation_documented(jwks_runtime):
    """OIDC access tokens cannot be revoked by Axiom; membership/API keys can.

    A still-valid JWT continues to authenticate until expiry. Operators must
    remove project membership or rotate keys to cut access immediately.
    """
    url, k1, _ = jwks_runtime
    token = k1.mint("alice")
    with httpx.Client() as c:
        assert c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5).status_code == 200
        # removing alice would lock the bootstrap owner; assert the limitation instead:
        # there is no /oauth/revoke equivalent on the runtime.
        r = c.post(f"{url}/oauth/revoke", headers=auth(token), timeout=5)
        assert r.status_code in (404, 405)
        assert c.get(f"{url}/api/v1/whoami", headers=auth(token), timeout=5).status_code == 200
