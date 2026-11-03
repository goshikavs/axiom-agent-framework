//! Request scope: identity, tenant/project, and role-based permissions.
//!
//! `GOSHIKQ_AUTH_MODE=none` is local development only (loopback unless
//! `GOSHIKQ_ALLOW_INSECURE_BIND=true`). Production uses `oidc` (JWT + optional API keys)
//! or `api_key` for service identities.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::config::AuthMode;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Viewer,
    Operator,
    Approver,
    Admin,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "viewer" => Some(Role::Viewer),
            "operator" => Some(Role::Operator),
            "approver" => Some(Role::Approver),
            "admin" => Some(Role::Admin),
            "owner" => Some(Role::Owner),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Operator => "operator",
            Role::Approver => "approver",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    pub fn allows(self, perm: Permission) -> bool {
        use Permission::*;
        match self {
            Role::Viewer => matches!(
                perm,
                ProjectRead | RunRead | WorkflowRead | ApprovalRead | SharedRead | MetricsRead
            ),
            Role::Operator => {
                Role::Viewer.allows(perm)
                    || matches!(
                        perm,
                        RunWrite | WorkflowExecute | TelemetryWrite | A2A | SharedWrite
                    )
            }
            Role::Approver => Role::Viewer.allows(perm) || perm == ApprovalDecide,
            Role::Admin => {
                Role::Operator.allows(perm)
                    || Role::Approver.allows(perm)
                    || matches!(
                        perm,
                        ProjectWrite | WorkflowWrite | ApiKeyManage | MemberManage | AuditRead
                    )
            }
            Role::Owner => true, // includes RetentionPurge; admin cannot purge evidence
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    ProjectRead,
    ProjectWrite,
    RunRead,
    RunWrite,
    WorkflowRead,
    WorkflowWrite,
    WorkflowExecute,
    ApprovalRead,
    ApprovalDecide,
    ApiKeyManage,
    MemberManage,
    TelemetryWrite,
    A2A,
    SharedRead,
    SharedWrite,
    MetricsRead,
    AuditRead,
    RetentionPurge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    User,
    Service,
    ApiKey,
    Local,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub project_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub actor: String,
    pub subject: String,
    pub role: Role,
    pub kind: SubjectKind,
}

impl Scope {
    pub fn require(&self, perm: Permission) -> Result<(), ApiError> {
        if self.role.allows(perm) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "role `{}` cannot {}",
                self.role.as_str(),
                match perm {
                    Permission::ApprovalDecide => "decide approvals",
                    Permission::ApiKeyManage => "manage API keys",
                    Permission::MemberManage => "manage members",
                    Permission::RunWrite => "mutate runs",
                    Permission::WorkflowWrite => "edit workflows",
                    Permission::WorkflowExecute => "execute workflows",
                    Permission::ProjectWrite => "mutate projects",
                    Permission::A2A => "invoke A2A",
                    Permission::TelemetryWrite => "write telemetry",
                    Permission::SharedWrite => "write shared context",
                    Permission::AuditRead => "read security events",
                    Permission::RetentionPurge => "purge retained runs",
                    _ => "perform this action",
                }
            )))
        }
    }
}

fn bearer_or_api_key(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .or_else(|| {
            parts
                .headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        })
}

fn header_get<'a>(parts: &'a Parts, names: &[&'static str]) -> Option<&'a str> {
    for name in names {
        if let Some(v) = parts
            .headers
            .get(*name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(v);
        }
    }
    None
}

fn header_project(parts: &Parts) -> Option<Uuid> {
    header_get(parts, &["x-goshikq-project-id", "x-axiom-project-id"])
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn header_actor(parts: &Parts) -> Option<String> {
    header_get(parts, &["x-goshikq-actor", "x-axiom-actor"]).map(|s| s.chars().take(128).collect())
}

async fn scope_from_api_key(
    state: &AppState,
    token: &str,
    header_actor: Option<String>,
    claimed_project: Option<Uuid>,
) -> Result<Scope, ApiError> {
    let key = state
        .db
        .authenticate_api_key(token)
        .await?
        .ok_or_else(|| ApiError::unauthorized("invalid, expired, or revoked API key"))?;
    if let Some(claimed) = claimed_project {
        if claimed != key.project_id {
            return Err(ApiError::forbidden(
                "X-Goshikq-Project-Id does not match the API key project",
            ));
        }
    }
    let role = Role::parse(&key.role).unwrap_or(Role::Operator);
    let actor = header_actor
        .map(|a| format!("{}:{a}", key.name))
        .unwrap_or_else(|| key.name.clone());
    Ok(Scope {
        project_id: key.project_id,
        tenant_id: None,
        actor,
        subject: format!("apikey:{}", key.id),
        role,
        kind: if key.kind == "user" {
            SubjectKind::ApiKey
        } else {
            SubjectKind::Service
        },
    })
}

async fn scope_from_jwt(
    state: &AppState,
    token: &str,
    claimed_project: Option<Uuid>,
) -> Result<Scope, ApiError> {
    let claims = state.oidc.validate(token).await?;
    let claim_project = claims
        .goshikq_project_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    let project_id = claimed_project
        .or(claim_project)
        .unwrap_or(goshikq_storage::DEFAULT_PROJECT_ID);
    let member = state.db.get_member(project_id, &claims.sub).await?;
    let member = member.ok_or_else(|| ApiError::forbidden("not a member of this project"))?;
    let role = Role::parse(&member.role).unwrap_or(Role::Viewer);
    let tenant_id = claims
        .goshikq_tenant_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .or({
            state
                .db
                .get_project(project_id)
                .await
                .ok()
                .map(|p| p.organization_id)
        });
    Ok(Scope {
        project_id,
        tenant_id,
        actor: claims.sub.clone(),
        subject: claims.sub,
        role,
        kind: SubjectKind::User,
    })
}

impl FromRequestParts<AppState> for Scope {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header_actor = header_actor(parts);
        let claimed_project = header_project(parts);
        match state.config.auth_mode {
            AuthMode::None => Ok(Scope {
                project_id: goshikq_storage::DEFAULT_PROJECT_ID,
                tenant_id: Some(goshikq_storage::DEFAULT_PROJECT_ID),
                actor: header_actor.unwrap_or_else(|| "local-user".into()),
                subject: "local-user".into(),
                role: Role::Owner,
                kind: SubjectKind::Local,
            }),
            AuthMode::ApiKey => {
                let token = bearer_or_api_key(parts)
                    .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
                scope_from_api_key(state, &token, header_actor, claimed_project).await
            }
            AuthMode::Oidc => {
                let token = bearer_or_api_key(parts)
                    .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
                if token.starts_with("axk_") {
                    scope_from_api_key(state, &token, header_actor, claimed_project).await
                } else {
                    scope_from_jwt(state, &token, claimed_project).await
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_cannot_approve() {
        assert!(!Role::Operator.allows(Permission::ApprovalDecide));
        assert!(Role::Approver.allows(Permission::ApprovalDecide));
        assert!(!Role::Approver.allows(Permission::RunWrite));
        assert!(Role::Owner.allows(Permission::ApprovalDecide));
        assert!(Role::Admin.allows(Permission::ApprovalDecide));
        assert!(Role::Admin.allows(Permission::ApiKeyManage));
    }

    #[test]
    fn viewer_is_read_only() {
        assert!(Role::Viewer.allows(Permission::RunRead));
        assert!(!Role::Viewer.allows(Permission::RunWrite));
        assert!(!Role::Viewer.allows(Permission::WorkflowExecute));
    }

    #[test]
    fn only_owner_can_purge_retention() {
        assert!(Role::Owner.allows(Permission::RetentionPurge));
        assert!(!Role::Admin.allows(Permission::RetentionPurge));
        assert!(!Role::Operator.allows(Permission::RetentionPurge));
    }
}
