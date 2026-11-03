use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::{Db, DbResult, WorkflowNodeExecutionRow};

impl Db {
    pub async fn list_workflow_nodes(
        &self,
        project_id: Uuid,
        run_id: Uuid,
    ) -> DbResult<Vec<WorkflowNodeExecutionRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeExecutionRow>(
            "SELECT * FROM workflow_node_executions WHERE project_id = $1 AND run_id = $2 ORDER BY started_at NULLS LAST, node_id",
        )
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_workflow_node(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        node_id: &str,
    ) -> DbResult<Option<WorkflowNodeExecutionRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeExecutionRow>(
            "SELECT * FROM workflow_node_executions WHERE project_id = $1 AND run_id = $2 AND node_id = $3",
        )
        .bind(project_id)
        .bind(run_id)
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_workflow_node(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        workflow_id: Uuid,
        workflow_version: i32,
        node_id: &str,
        status: &str,
        attempt: i32,
        input: &Value,
        output: Option<&Value>,
        error: Option<&str>,
        child_run_id: Option<Uuid>,
        approval_id: Option<Uuid>,
        wait_until: Option<DateTime<Utc>>,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
    ) -> DbResult<WorkflowNodeExecutionRow> {
        Ok(sqlx::query_as::<_, WorkflowNodeExecutionRow>(
            r#"INSERT INTO workflow_node_executions
                 (id, project_id, run_id, workflow_id, workflow_version, node_id, status, attempt, input, output, error, child_run_id, approval_id, wait_until, started_at, completed_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
               ON CONFLICT (run_id, node_id) DO UPDATE SET
                 status = EXCLUDED.status,
                 attempt = EXCLUDED.attempt,
                 input = EXCLUDED.input,
                 output = EXCLUDED.output,
                 error = EXCLUDED.error,
                 child_run_id = COALESCE(EXCLUDED.child_run_id, workflow_node_executions.child_run_id),
                 approval_id = COALESCE(EXCLUDED.approval_id, workflow_node_executions.approval_id),
                 wait_until = EXCLUDED.wait_until,
                 started_at = COALESCE(workflow_node_executions.started_at, EXCLUDED.started_at),
                 completed_at = EXCLUDED.completed_at
               RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(run_id)
        .bind(workflow_id)
        .bind(workflow_version)
        .bind(node_id)
        .bind(status)
        .bind(attempt)
        .bind(input)
        .bind(output)
        .bind(error)
        .bind(child_run_id)
        .bind(approval_id)
        .bind(wait_until)
        .bind(started_at)
        .bind(completed_at)
        .fetch_one(&self.pool)
        .await?)
    }
}
