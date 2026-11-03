use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use goshikq_storage::*;
use uuid::Uuid;

use crate::auth::{Permission, Role, Scope};
use crate::dto::*;
use crate::error::ApiError;
use crate::state::AppState;

// ---- health -------------------------------------------------------------------------

#[utoipa::path(get, path = "/health", tag = "system", responses((status = 200, body = HealthResponse)))]
pub async fn health(State(st): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_seconds: (chrono::Utc::now() - st.started_at).num_seconds(),
    })
}

#[utoipa::path(get, path = "/ready", tag = "system", responses((status = 200, body = ReadyResponse), (status = 503, body = ReadyResponse)))]
pub async fn ready(State(st): State<AppState>) -> (StatusCode, Json<ReadyResponse>) {
    let (code, db) = match st.db.ping().await {
        Ok(()) => (StatusCode::OK, "ok"),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    };
    let auth_mode = st.config.auth_mode.as_str();
    (
        code,
        Json(ReadyResponse {
            status: if code == StatusCode::OK {
                "ready".into()
            } else {
                "not_ready".into()
            },
            database: db.into(),
            active_runs: st.runtime.active_run_count(),
            auth_mode: auth_mode.into(),
        }),
    )
}

// ---- projects -----------------------------------------------------------------------

#[utoipa::path(get, path = "/api/v1/projects", tag = "projects", responses((status = 200, body = Vec<ProjectRow>)))]
pub async fn list_projects(
    State(st): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<ProjectRow>>, ApiError> {
    scope.require(Permission::ProjectRead)?;
    if scope.kind == crate::auth::SubjectKind::User {
        return Ok(Json(st.db.list_projects_for_subject(&scope.subject).await?));
    }
    let all = st.db.list_projects().await?;
    Ok(Json(
        all.into_iter()
            .filter(|p| p.id == scope.project_id)
            .collect(),
    ))
}

#[utoipa::path(get, path = "/api/v1/whoami", tag = "system", responses((status = 200, body = WhoAmI)))]
pub async fn whoami(State(st): State<AppState>, scope: Scope) -> Json<WhoAmI> {
    Json(WhoAmI {
        subject: scope.subject,
        actor: scope.actor,
        role: scope.role.as_str().into(),
        kind: format!("{:?}", scope.kind).to_ascii_lowercase(),
        project_id: scope.project_id,
        tenant_id: scope.tenant_id,
        auth_mode: st.config.auth_mode.as_str().into(),
    })
}

#[utoipa::path(post, path = "/api/v1/projects", tag = "projects", request_body = CreateProjectRequest, responses((status = 201, body = ProjectRow)))]
pub async fn create_project(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectRow>), ApiError> {
    scope.require(Permission::ProjectWrite)?;
    if req.name.trim().is_empty() || req.slug.trim().is_empty() {
        return Err(ApiError::bad_request("name and slug are required"));
    }
    if !req
        .slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ApiError::bad_request(
            "slug must be lowercase letters, digits, and hyphens",
        ));
    }
    let org = st.db.get_project(scope.project_id).await?.organization_id;
    let p = st
        .db
        .create_project(org, req.name.trim(), req.slug.trim())
        .await
        .map_err(|e| match e {
            StorageError::Sqlx(sqlx::Error::Database(d)) if d.is_unique_violation() => {
                ApiError::conflict("slug already exists")
            }
            other => other.into(),
        })?;
    st.db.upsert_member(p.id, &scope.subject, "owner").await?;
    st.db
        .audit(
            scope.project_id,
            &scope.actor,
            "project.created",
            "project",
            &p.id.to_string(),
            serde_json::json!({"slug": p.slug}),
        )
        .await?;
    let _ = st
        .db
        .record_security_event(
            Some(p.id),
            scope.tenant_id,
            &scope.actor,
            "project.created",
            "ok",
            serde_json::json!({"slug": p.slug}),
        )
        .await;
    Ok((StatusCode::CREATED, Json(p)))
}

// ---- API keys -----------------------------------------------------------------------

#[utoipa::path(get, path = "/api/v1/api-keys", tag = "projects", responses((status = 200, body = Vec<ApiKeyPublic>)))]
pub async fn list_api_keys(
    State(st): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<ApiKeyPublic>>, ApiError> {
    scope.require(Permission::ApiKeyManage)?;
    let rows = st.db.list_api_keys(scope.project_id).await?;
    Ok(Json(rows.into_iter().map(ApiKeyPublic::from).collect()))
}

#[utoipa::path(post, path = "/api/v1/api-keys", tag = "projects", request_body = CreateApiKeyRequest, responses((status = 201, body = ApiKeyCreated)))]
pub async fn create_api_key(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<ApiKeyCreated>), ApiError> {
    scope.require(Permission::ApiKeyManage)?;
    if req.name.trim().is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }
    let role = req.role.as_deref().unwrap_or("operator");
    if Role::parse(role).is_none() {
        return Err(ApiError::bad_request(
            "role must be viewer|operator|approver|admin|owner",
        ));
    }
    let kind = req.kind.as_deref().unwrap_or("service");
    let expires_at = req
        .expires_in_seconds
        .map(|s| chrono::Utc::now() + chrono::Duration::seconds(s.max(1)));
    let project_id = match (st.config.auth_mode, req.project_id) {
        (crate::config::AuthMode::None, Some(id)) => {
            let _ = st.db.get_project(id).await?;
            id
        }
        _ => scope.project_id,
    };
    let token = format!("axk_{}", Uuid::new_v4().simple());
    let row = st
        .db
        .create_api_key_full(project_id, req.name.trim(), &token, role, kind, expires_at)
        .await?;
    st.db
        .audit(
            scope.project_id,
            &scope.actor,
            "api_key.created",
            "api_key",
            &row.id.to_string(),
            serde_json::json!({"name": row.name, "project_id": project_id, "role": role}),
        )
        .await?;
    let _ = st
        .db
        .record_security_event(
            Some(project_id),
            scope.tenant_id,
            &scope.actor,
            "api_key.created",
            "ok",
            serde_json::json!({"id": row.id, "role": role}),
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(ApiKeyCreated {
            id: row.id,
            project_id: row.project_id,
            name: row.name,
            token,
            created_at: row.created_at,
            role: row.role,
            kind: row.kind,
            expires_at: row.expires_at,
        }),
    ))
}

#[utoipa::path(post, path = "/api/v1/api-keys/{id}/revoke", tag = "projects", params(("id" = Uuid, Path)), responses((status = 200, body = ApiKeyPublic)))]
pub async fn revoke_api_key(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiKeyPublic>, ApiError> {
    scope.require(Permission::ApiKeyManage)?;
    let row = st.db.revoke_api_key(scope.project_id, id).await?;
    let _ = st
        .db
        .record_security_event(
            Some(scope.project_id),
            scope.tenant_id,
            &scope.actor,
            "api_key.revoked",
            "ok",
            serde_json::json!({"id": id}),
        )
        .await;
    Ok(Json(row.into()))
}

#[utoipa::path(post, path = "/api/v1/api-keys/{id}/rotate", tag = "projects", params(("id" = Uuid, Path)), responses((status = 201, body = ApiKeyCreated)))]
pub async fn rotate_api_key(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<ApiKeyCreated>), ApiError> {
    scope.require(Permission::ApiKeyManage)?;
    let token = format!("axk_{}", Uuid::new_v4().simple());
    let row = st.db.rotate_api_key(scope.project_id, id, &token).await?;
    let _ = st
        .db
        .record_security_event(
            Some(scope.project_id),
            scope.tenant_id,
            &scope.actor,
            "api_key.rotated",
            "ok",
            serde_json::json!({"from": id, "to": row.id}),
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(ApiKeyCreated {
            id: row.id,
            project_id: row.project_id,
            name: row.name,
            token,
            created_at: row.created_at,
            role: row.role,
            kind: row.kind,
            expires_at: row.expires_at,
        }),
    ))
}

#[utoipa::path(get, path = "/api/v1/members", tag = "projects", responses((status = 200, body = Vec<ProjectMemberRow>)))]
pub async fn list_members(
    State(st): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<ProjectMemberRow>>, ApiError> {
    scope.require(Permission::MemberManage)?;
    Ok(Json(st.db.list_members(scope.project_id).await?))
}

#[utoipa::path(post, path = "/api/v1/members", tag = "projects", request_body = UpsertMemberRequest, responses((status = 200, body = ProjectMemberRow)))]
pub async fn upsert_member(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<UpsertMemberRequest>,
) -> Result<Json<ProjectMemberRow>, ApiError> {
    scope.require(Permission::MemberManage)?;
    if req.subject.trim().is_empty() {
        return Err(ApiError::bad_request("subject is required"));
    }
    if Role::parse(&req.role).is_none() {
        return Err(ApiError::bad_request(
            "role must be viewer|operator|approver|admin|owner",
        ));
    }
    let row = st
        .db
        .upsert_member(scope.project_id, req.subject.trim(), &req.role)
        .await?;
    let _ = st
        .db
        .record_security_event(
            Some(scope.project_id),
            scope.tenant_id,
            &scope.actor,
            "member.upserted",
            "ok",
            serde_json::json!({"subject": row.subject, "role": row.role}),
        )
        .await;
    Ok(Json(row))
}

#[utoipa::path(delete, path = "/api/v1/members/{subject}", tag = "projects", params(("subject" = String, Path)), responses((status = 204)))]
pub async fn delete_member(
    State(st): State<AppState>,
    scope: Scope,
    Path(subject): Path<String>,
) -> Result<StatusCode, ApiError> {
    scope.require(Permission::MemberManage)?;
    st.db.delete_member(scope.project_id, &subject).await?;
    let _ = st
        .db
        .record_security_event(
            Some(scope.project_id),
            scope.tenant_id,
            &scope.actor,
            "member.deleted",
            "ok",
            serde_json::json!({"subject": subject}),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/v1/security-events", tag = "projects", responses((status = 200, body = Vec<SecurityEventRow>)))]
pub async fn list_security_events(
    State(st): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<SecurityEventRow>>, ApiError> {
    scope.require(Permission::AuditRead)?;
    Ok(Json(
        st.db.list_security_events(scope.project_id, 100).await?,
    ))
}

// ---- agents -------------------------------------------------------------------------

#[utoipa::path(get, path = "/api/v1/agents", tag = "agents", params(PageQuery), responses((status = 200, body = Paginated<AgentRow>)))]
pub async fn list_agents(
    State(st): State<AppState>,
    scope: Scope,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<AgentRow>>, ApiError> {
    scope.require(Permission::RunRead)?;
    let page = Page::new(q.limit, q.offset);
    let items = st.db.list_agents(scope.project_id, page).await?;
    Ok(Json(Paginated {
        items,
        total: None,
        limit: page.limit,
        offset: page.offset,
    }))
}

#[utoipa::path(post, path = "/api/v1/agents", tag = "agents", request_body = SaveAgentRequest, responses((status = 201, body = AgentDetail)))]
pub async fn save_agent(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<SaveAgentRequest>,
) -> Result<(StatusCode, Json<AgentDetail>), ApiError> {
    scope.require(Permission::RunWrite)?;
    if req.agent.name.trim().is_empty() {
        return Err(ApiError::bad_request("agent.name is required"));
    }
    if !req.agent.model.contains('/') {
        return Err(ApiError::bad_request(
            "agent.model must be provider-qualified (e.g. mock/deterministic)",
        ));
    }
    for t in &req.agent.tools {
        jsonschema::validator_for(&t.input_schema).map_err(|e| {
            ApiError::bad_request(format!(
                "tool `{}` has an invalid input_schema: {e}",
                t.name
            ))
        })?;
    }
    let def = serde_json::to_value(&req.agent).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let (agent, _version) = st
        .db
        .save_agent_definition(
            scope.project_id,
            &req.agent.name,
            &req.agent.description,
            &req.source_framework,
            &def,
            &req.agent.model,
        )
        .await?;
    st.db
        .audit(
            scope.project_id,
            &scope.actor,
            "agent.saved",
            "agent",
            &agent.id.to_string(),
            serde_json::json!({"name": agent.name}),
        )
        .await?;
    let detail = agent_detail(&st, scope.project_id, agent).await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

#[utoipa::path(get, path = "/api/v1/agents/{id}", tag = "agents", params(("id" = Uuid, Path)), responses((status = 200, body = AgentDetail)))]
pub async fn get_agent(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<AgentDetail>, ApiError> {
    scope.require(Permission::RunRead)?;
    let agent = st.db.get_agent(scope.project_id, id).await?;
    Ok(Json(agent_detail(&st, scope.project_id, agent).await?))
}

async fn agent_detail(
    st: &AppState,
    project_id: Uuid,
    agent: AgentRow,
) -> Result<AgentDetail, ApiError> {
    let versions = st.db.list_agent_versions(agent.id).await?;
    let (run_count, verified_runs, failed_runs, last_run_at) =
        st.db.agent_run_stats(project_id, agent.id).await?;
    Ok(AgentDetail {
        agent,
        versions,
        run_count,
        verified_runs,
        failed_runs,
        last_run_at,
    })
}

// ---- capability manifests -----------------------------------------------------------

#[utoipa::path(get, path = "/api/v1/capabilities", tag = "integrations", responses((status = 200, body = Vec<goshikq_protocol::FrameworkCapabilityManifest>)))]
pub async fn list_capabilities() -> Json<Vec<goshikq_protocol::FrameworkCapabilityManifest>> {
    Json(goshikq_protocol::FrameworkCapabilityManifest::builtin())
}

// ---- integrations -------------------------------------------------------------------

#[utoipa::path(get, path = "/api/v1/integrations", tag = "integrations", responses((status = 200, body = Vec<IntegrationRow>)))]
pub async fn list_integrations(
    State(st): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<IntegrationRow>>, ApiError> {
    scope.require(Permission::ProjectRead)?;
    Ok(Json(
        st.db
            .list_integrations(scope.project_id)
            .await?
            .into_iter()
            .map(|mut i| {
                i.config = goshikq_core::redaction::redact_value(&i.config);
                i
            })
            .collect(),
    ))
}

#[utoipa::path(post, path = "/api/v1/integrations", tag = "integrations", request_body = UpsertIntegrationRequest, responses((status = 200, body = IntegrationRow)))]
pub async fn upsert_integration(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<UpsertIntegrationRequest>,
) -> Result<Json<IntegrationRow>, ApiError> {
    scope.require(Permission::ProjectWrite)?;
    if req.framework.trim().is_empty() {
        return Err(ApiError::bad_request("framework is required"));
    }
    if !["connected", "configured", "error", "planned"].contains(&req.status.as_str()) {
        return Err(ApiError::bad_request(
            "status must be connected|configured|error|planned",
        ));
    }
    let row = st
        .db
        .upsert_integration(
            scope.project_id,
            req.framework.trim(),
            &req.adapter_version,
            req.framework_version.as_deref(),
            &req.status,
            &serde_json::json!(req.capabilities),
            &serde_json::json!(req.unsupported_capabilities),
            &req.config,
            req.last_error.as_deref(),
        )
        .await?;
    st.db
        .audit(
            scope.project_id,
            &scope.actor,
            "integration.upserted",
            "integration",
            &row.id.to_string(),
            serde_json::json!({"framework": row.framework}),
        )
        .await?;
    Ok(Json(row))
}

// ---- workflows ----------------------------------------------------------------------

#[utoipa::path(get, path = "/api/v1/workflows", tag = "workflows", params(PageQuery), responses((status = 200, body = Paginated<WorkflowRow>)))]
pub async fn list_workflows(
    State(st): State<AppState>,
    scope: Scope,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<WorkflowRow>>, ApiError> {
    scope.require(Permission::WorkflowRead)?;
    let page = Page::new(q.limit, q.offset);
    let items = st.db.list_workflows(scope.project_id, page).await?;
    Ok(Json(Paginated {
        items,
        total: None,
        limit: page.limit,
        offset: page.offset,
    }))
}

#[utoipa::path(get, path = "/api/v1/workflows/{id}", tag = "workflows", params(("id" = Uuid, Path)), responses((status = 200, body = WorkflowRow)))]
pub async fn get_workflow(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowRow>, ApiError> {
    scope.require(Permission::WorkflowRead)?;
    Ok(Json(st.db.get_workflow(scope.project_id, id).await?))
}

/// Validates against the shared workflow schema; invalid definitions are rejected with
/// the full issue list so the builder can highlight them unless `draft` is true.
#[utoipa::path(post, path = "/api/v1/workflows", tag = "workflows", request_body = CreateWorkflowRequest,
    responses((status = 201, body = WorkflowRow), (status = 422, body = WorkflowValidation)))]
pub async fn create_workflow(
    State(st): State<AppState>,
    scope: Scope,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<WorkflowRow>), ApiError> {
    scope.require(Permission::WorkflowWrite)?;
    let issues = req.definition.validate();
    if !req.draft && !issues.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_workflow",
            "workflow failed validation",
        )
        .with_details(serde_json::json!({"issues": issues})));
    }
    let def =
        serde_json::to_value(&req.definition).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let row = st
        .db
        .create_workflow_full(
            scope.project_id,
            &req.definition.name,
            req.definition.version as i32,
            &req.definition.description,
            "draft",
            &def,
        )
        .await
        .map_err(|e| match e {
            StorageError::Sqlx(sqlx::Error::Database(d)) if d.is_unique_violation() => {
                ApiError::conflict("workflow name+version already exists")
            }
            other => other.into(),
        })?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(put, path = "/api/v1/workflows/{id}", tag = "workflows", request_body = UpdateWorkflowRequest, responses((status = 200, body = WorkflowRow)))]
pub async fn update_workflow(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> Result<Json<WorkflowRow>, ApiError> {
    scope.require(Permission::WorkflowWrite)?;
    let issues = req.definition.validate();
    if !req.draft && !issues.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_workflow",
            "workflow failed validation",
        )
        .with_details(serde_json::json!({"issues": issues})));
    }
    let def =
        serde_json::to_value(&req.definition).map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(
        st.db
            .update_workflow_draft(
                scope.project_id,
                id,
                &req.definition.name,
                &req.definition.description,
                &def,
            )
            .await?,
    ))
}

#[utoipa::path(post, path = "/api/v1/workflows/validate", tag = "workflows", request_body = CreateWorkflowRequest, responses((status = 200, body = WorkflowValidation)))]
pub async fn validate_workflow(Json(req): Json<CreateWorkflowRequest>) -> Json<WorkflowValidation> {
    let issues = req.definition.validate();
    Json(WorkflowValidation {
        valid: issues.is_empty(),
        issues,
    })
}

#[utoipa::path(post, path = "/api/v1/workflows/{id}/publish", tag = "workflows", responses((status = 200, body = WorkflowRow), (status = 422, body = WorkflowValidation)))]
pub async fn publish_workflow(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowRow>, ApiError> {
    scope.require(Permission::WorkflowWrite)?;
    let row = st.db.get_workflow(scope.project_id, id).await?;
    let def: goshikq_core::workflow::WorkflowDefinition =
        serde_json::from_value(row.definition.clone()).map_err(|e| {
            ApiError::bad_request(format!("stored definition is not a workflow: {e}"))
        })?;
    let issues = def.validate();
    if !issues.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_workflow",
            "cannot publish an invalid workflow",
        )
        .with_details(serde_json::json!({"issues": issues})));
    }
    let (published, _) = st
        .db
        .publish_workflow(scope.project_id, id, &scope.actor)
        .await?;
    Ok(Json(published))
}

#[utoipa::path(get, path = "/api/v1/workflows/{id}/versions", tag = "workflows", responses((status = 200, body = Vec<WorkflowVersionRow>)))]
pub async fn list_workflow_versions(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<goshikq_storage::WorkflowVersionRow>>, ApiError> {
    scope.require(Permission::WorkflowRead)?;
    let _ = st.db.get_workflow(scope.project_id, id).await?;
    Ok(Json(
        st.db.list_workflow_versions(scope.project_id, id).await?,
    ))
}

#[utoipa::path(post, path = "/api/v1/workflows/{id}/duplicate", tag = "workflows", request_body = DuplicateWorkflowRequest, responses((status = 201, body = WorkflowRow)))]
pub async fn duplicate_workflow(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Json(req): Json<DuplicateWorkflowRequest>,
) -> Result<(StatusCode, Json<WorkflowRow>), ApiError> {
    scope.require(Permission::WorkflowWrite)?;
    if req.name.trim().is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }
    let row = st
        .db
        .duplicate_workflow(scope.project_id, id, req.name.trim())
        .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(post, path = "/api/v1/workflows/{id}/run", tag = "workflows", request_body = RunWorkflowRequest, responses((status = 201, body = crate::dto::CreateRunResponse)))]
pub async fn run_workflow(
    State(st): State<AppState>,
    scope: Scope,
    Path(id): Path<Uuid>,
    Json(req): Json<RunWorkflowRequest>,
) -> Result<(StatusCode, Json<crate::dto::CreateRunResponse>), ApiError> {
    scope.require(Permission::WorkflowExecute)?;
    let wf = st.db.get_workflow(scope.project_id, id).await?;
    let version = req
        .version
        .or(wf.published_version)
        .ok_or_else(|| ApiError::bad_request("workflow has no published version"))?;
    let snap = st
        .db
        .get_workflow_version(scope.project_id, id, version)
        .await?;
    let def: goshikq_core::workflow::WorkflowDefinition =
        serde_json::from_value(snap.definition.clone()).map_err(|e| {
            ApiError::bad_request(format!("published definition is not a workflow: {e}"))
        })?;
    let issues = def.validate();
    if !issues.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_workflow",
            "published workflow failed validation",
        )
        .with_details(serde_json::json!({"issues": issues})));
    }
    if def.input_schema.get("type").and_then(|v| v.as_str()) == Some("object") {
        if !req.input.is_null() && !req.input.is_object() {
            return Err(ApiError::bad_request(
                "workflow input must be a JSON object",
            ));
        }
    }
    let goal_desc = if def.goal.description.trim().is_empty() {
        def.name.clone()
    } else {
        def.goal.description.clone()
    };
    let create = goshikq_protocol::CreateRunRequest {
        project_id: Some(scope.project_id),
        agent: goshikq_protocol::AgentDefinition {
            name: format!("workflow:{}", def.name),
            description: def.description.clone(),
            model: "mock/deterministic".into(),
            instructions: "Execute the pinned workflow graph. Do not invent side effects.".into(),
            tools: def
                .nodes
                .iter()
                .filter(|n| n.kind == goshikq_core::workflow::WorkflowNodeKind::Tool)
                .filter_map(|n| {
                    let name = n
                        .config
                        .get("tool")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())?;
                    let handler = n
                        .config
                        .get("handler")
                        .and_then(|v| v.as_str())
                        .unwrap_or("echo");
                    if matches!(handler, "echo" | "fail") || n.config.get("result").is_some() {
                        return None;
                    }
                    Some(goshikq_protocol::ToolSpec {
                        name: name.into(),
                        description: n.label.clone(),
                        input_schema: if n.input_schema.is_null() {
                            serde_json::json!({"type": "object"})
                        } else {
                            n.input_schema.clone()
                        },
                        output_schema: None,
                        effect: n
                            .config
                            .get("effect")
                            .and_then(|v| v.as_str())
                            .and_then(goshikq_protocol::EffectCategory::parse)
                            .unwrap_or(goshikq_protocol::EffectCategory::ReadOnly),
                        risk: goshikq_protocol::RiskLevel::Low,
                        timeout_ms: n.timeout_ms.unwrap_or(30_000),
                        retry_policy: Default::default(),
                        idempotent: n
                            .config
                            .get("idempotent")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true),
                        requires_approval: n
                            .config
                            .get("requires_approval")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        required_permissions: vec![],
                        preconditions: vec![],
                        has_verifier: false,
                        revalidation_policy: Default::default(),
                        evidence_coverage: None,
                    })
                })
                .collect(),
            execution_limits: goshikq_protocol::ExecutionLimits {
                max_steps: def.max_steps.max(8),
                deadline_seconds: def.timeouts.run_seconds.unwrap_or(120),
                ..Default::default()
            },
            retry_policy: Default::default(),
            context_policy: serde_json::json!({}),
            metadata: serde_json::json!({}),
            permissions: def.required_permissions.clone(),
        },
        goal: goshikq_protocol::GoalDefinition {
            description: goal_desc,
            success_predicate: None,
            invariants: vec![],
            max_steps: Some(def.max_steps),
            deadline_seconds: def.timeouts.run_seconds,
            external_wait_seconds: def.timeouts.wait_seconds,
            budget: None,
            verification_policy: match def.goal.verification_policy.as_str() {
                "none" => goshikq_protocol::VerificationPolicy::None,
                _ => goshikq_protocol::VerificationPolicy::Required,
            },
            metadata: serde_json::json!({"workflow_id": id, "workflow_version": version}),
        },
        input: req.input,
        has_state_provider: false,
        source_framework: "goshikq-workflow".into(),
        idempotency_key: req.idempotency_key,
        metadata: serde_json::json!({
            "workflow_id": id,
            "workflow": {
                "id": id,
                "version": version,
                "name": def.name,
                "definition": def,
            },
            "coordination": { "mode": "supervisor", "max_depth": 8, "max_delegations": 32, "prevent_cycles": true },
            "worker_attach_seconds": st.config.worker_attach_timeout.as_secs(),
        }),
        defer_start: false,
        parent_run_id: None,
        task_id: None,
        correlation_id: None,
        trace_id: None,
    };
    let (run, reused) = st.runtime.create_run(scope.project_id, create).await?;
    Ok((
        if reused {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(crate::dto::CreateRunResponse {
            run_id: run.id,
            status: run.status,
            trace_id: run.trace_id,
            reused,
        }),
    ))
}

// ---- models -------------------------------------------------------------------------

#[utoipa::path(get, path = "/api/v1/models", tag = "models", responses((status = 200, body = ModelsResponse)))]
pub async fn models(
    State(st): State<AppState>,
    scope: Scope,
) -> Result<Json<ModelsResponse>, ApiError> {
    scope.require(Permission::MetricsRead)?;
    let adapters = std::fs::read_to_string("config/model-adapters.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let capture = match st.config.capture_model_io {
        goshikq_runtime::CaptureMode::Full => "full",
        goshikq_runtime::CaptureMode::Redacted => "redacted",
        goshikq_runtime::CaptureMode::None => "none",
    };
    Ok(Json(ModelsResponse {
        pricing: (*st.runtime.cfg.pricing).clone(),
        capture_model_io: capture.into(),
        adapters,
    }))
}
