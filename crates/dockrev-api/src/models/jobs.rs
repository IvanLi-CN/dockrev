use serde_json::Value;

use crate::api::types::{JobListItem, JobScope, JobType};

#[derive(Clone, Debug)]
pub struct JobRecord {
    pub id: String,
    pub r#type: JobType,
    pub scope: JobScope,
    pub stack_id: Option<String>,
    pub service_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub allow_arch_mismatch: bool,
    pub backup_mode: String,
    pub summary_json: Value,
}

impl JobRecord {
    pub fn new_running(
        id: String,
        r#type: JobType,
        scope: JobScope,
        stack_id: Option<String>,
        service_id: Option<String>,
        now: &str,
    ) -> Self {
        Self {
            id,
            r#type,
            scope,
            stack_id,
            service_id,
            status: "running".to_string(),
            created_at: now.to_string(),
            started_at: Some(now.to_string()),
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: Value::Object(Default::default()),
        }
    }

    pub fn to_db(&self) -> JobListItem {
        JobListItem {
            id: self.id.clone(),
            r#type: self.r#type.clone(),
            scope: self.scope.clone(),
            stack_id: self.stack_id.clone(),
            service_id: self.service_id.clone(),
            status: self.status.clone(),
            created_at: self.created_at.clone(),
            created_by: "unknown".to_string(),
            reason: "unknown".to_string(),
            started_at: self.started_at.clone(),
            finished_at: self.finished_at.clone(),
            allow_arch_mismatch: self.allow_arch_mismatch,
            backup_mode: self.backup_mode.clone(),
            summary_json: self.summary_json.clone(),
        }
    }
}
