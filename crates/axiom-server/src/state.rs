use std::sync::Arc;

use axiom_runtime::Runtime;
use axiom_storage::Db;

use crate::config::Config;
use crate::oidc::OidcValidator;
use crate::otlp_limit::OtlpLimiter;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub runtime: Arc<Runtime>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub otlp: Arc<OtlpLimiter>,
    pub oidc: Arc<OidcValidator>,
}
