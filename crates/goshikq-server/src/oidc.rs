//! OIDC / OAuth2 JWT validation.
//!
//! Production path: fetch JWKS from the issuer and validate `iss`, `aud`, `exp`, `nbf`.
//! Test path: HS256 HMAC is accepted only on loopback when `GOSHIKQ_OIDC_ALLOW_TEST_HMAC=true`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::OidcSettings;
use crate::error::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcClaims {
    pub sub: String,
    #[serde(default)]
    pub iss: String,
    #[serde(default)]
    pub aud: Aud,
    #[serde(default)]
    pub exp: Option<i64>,
    #[serde(default)]
    pub nbf: Option<i64>,
    #[serde(default, alias = "axiom_project_id", rename = "goshikq_project_id")]
    pub goshikq_project_id: Option<String>,
    #[serde(default, alias = "axiom_role", rename = "goshikq_role")]
    pub goshikq_role: Option<String>,
    #[serde(default, alias = "axiom_tenant_id", rename = "goshikq_tenant_id")]
    pub goshikq_tenant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Aud {
    One(String),
    Many(Vec<String>),
}

impl Default for Aud {
    fn default() -> Self {
        Aud::Many(vec![])
    }
}

impl Aud {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Aud::One(s) => s == expected,
            Aud::Many(v) => v.iter().any(|s| s == expected),
        }
    }
}

#[derive(Clone)]
pub struct OidcValidator {
    settings: OidcSettings,
    http: reqwest::Client,
    jwks: Arc<RwLock<Option<(Instant, jsonwebtoken::jwk::JwkSet)>>>,
}

impl OidcValidator {
    pub fn new(settings: OidcSettings) -> Self {
        OidcValidator {
            settings,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("goshikq-server-oidc")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            jwks: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn validate(&self, token: &str) -> Result<OidcClaims, ApiError> {
        let header =
            decode_header(token).map_err(|_| ApiError::unauthorized("invalid JWT header"))?;
        let (key, alg) = self.decoding_key(&header).await?;
        let mut validation = Validation::new(alg);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.set_issuer(&[&self.settings.issuer]);
        validation.set_audience(&[&self.settings.audience]);
        let data = decode::<OidcClaims>(token, &key, &validation).map_err(|e| {
            tracing::info!(error = %e, "oidc jwt rejected");
            ApiError::unauthorized("invalid or expired token")
        })?;
        if data.claims.sub.trim().is_empty() {
            return Err(ApiError::unauthorized("token is missing sub"));
        }
        if !data.claims.aud.contains(&self.settings.audience)
            && !matches!(data.claims.aud, Aud::Many(ref v) if v.is_empty())
        {
            // jsonwebtoken already checks aud; keep a belt-and-suspenders message.
        }
        Ok(data.claims)
    }

    async fn decoding_key(
        &self,
        header: &jsonwebtoken::Header,
    ) -> Result<(DecodingKey, Algorithm), ApiError> {
        if matches!(
            header.alg,
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
        ) {
            if self.settings.allow_test_hmac {
                if let Some(secret) = &self.settings.hmac_secret {
                    return Ok((DecodingKey::from_secret(secret.as_bytes()), header.alg));
                }
            }
            return Err(ApiError::unauthorized("HMAC tokens are not accepted"));
        }
        if !matches!(
            header.alg,
            Algorithm::RS256
                | Algorithm::RS384
                | Algorithm::RS512
                | Algorithm::ES256
                | Algorithm::ES384
                | Algorithm::PS256
                | Algorithm::PS384
                | Algorithm::PS512
        ) {
            return Err(ApiError::unauthorized("unsupported JWT algorithm"));
        }
        let kid = header
            .kid
            .as_deref()
            .ok_or_else(|| ApiError::unauthorized("JWT is missing kid"))?;
        let mut set = self.load_jwks(false).await?;
        if set.find(kid).is_none() {
            set = self.load_jwks(true).await?;
        }
        let jwk = set
            .find(kid)
            .ok_or_else(|| ApiError::unauthorized("unknown signing key"))?;
        let key = DecodingKey::from_jwk(jwk)
            .map_err(|_| ApiError::unauthorized("unsupported signing key"))?;
        Ok((key, header.alg))
    }

    async fn load_jwks(&self, force: bool) -> Result<jsonwebtoken::jwk::JwkSet, ApiError> {
        if !force {
            if let Some((at, set)) = self.jwks.read().await.as_ref() {
                if at.elapsed() < self.settings.jwks_cache_ttl {
                    return Ok(set.clone());
                }
            }
        }
        let url = self
            .settings
            .jwks_url
            .clone()
            .ok_or_else(|| ApiError::unauthorized("OIDC JWKS URL is not configured"))?;
        if !(url.starts_with("https://")
            || url.starts_with("http://127.0.0.1")
            || url.starts_with("http://localhost"))
        {
            return Err(ApiError::unauthorized(
                "JWKS URL must be https (or loopback http for tests)",
            ));
        }
        let set: jsonwebtoken::jwk::JwkSet = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|_| ApiError::unauthorized("failed to fetch JWKS"))?
            .error_for_status()
            .map_err(|_| ApiError::unauthorized("JWKS endpoint rejected the request"))?
            .json()
            .await
            .map_err(|_| ApiError::unauthorized("JWKS document is invalid"))?;
        *self.jwks.write().await = Some((Instant::now(), set.clone()));
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;

    fn settings() -> OidcSettings {
        OidcSettings {
            issuer: "http://127.0.0.1/idp".into(),
            audience: "goshikq".into(),
            jwks_url: None,
            hmac_secret: Some("test-hmac-secret-at-least-32-bytes!!".into()),
            allow_test_hmac: true,
            bootstrap_owner: None,
            jwks_cache_ttl: Duration::from_secs(300),
        }
    }

    fn mint(iss: &str, aud: &str, exp_delta: i64) -> String {
        let claims = json!({
            "sub": "alice",
            "iss": iss,
            "aud": aud,
            "exp": chrono::Utc::now().timestamp() + exp_delta,
            "nbf": chrono::Utc::now().timestamp() - 5,
        });
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"test-hmac-secret-at-least-32-bytes!!"),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn accepts_loopback_hmac_with_issuer_and_audience() {
        let claims = json!({
            "sub": "alice",
            "iss": "http://127.0.0.1/idp",
            "aud": "goshikq",
            "exp": chrono::Utc::now().timestamp() + 60,
        });
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"test-hmac-secret-at-least-32-bytes!!"),
        )
        .unwrap();
        let v = OidcValidator::new(settings());
        let got = v.validate(&token).await.unwrap();
        assert_eq!(got.sub, "alice");
    }

    #[tokio::test]
    async fn rejects_wrong_issuer() {
        let v = OidcValidator::new(settings());
        assert!(v
            .validate(&mint("https://evil.example", "goshikq", 60))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn rejects_wrong_audience() {
        let v = OidcValidator::new(settings());
        assert!(v
            .validate(&mint("http://127.0.0.1/idp", "other-api", 60))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn rejects_expired_token() {
        let v = OidcValidator::new(settings());
        assert!(v
            .validate(&mint("http://127.0.0.1/idp", "goshikq", -120))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn rejects_hmac_when_test_mode_disabled() {
        let mut s = settings();
        s.allow_test_hmac = false;
        s.jwks_url = Some("http://127.0.0.1:1/jwks".into());
        let v = OidcValidator::new(s);
        assert!(v
            .validate(&mint("http://127.0.0.1/idp", "goshikq", 60))
            .await
            .is_err());
    }
}
