#[derive(Clone, Debug)]
pub struct GitHubPackagesSettingsDb {
    pub enabled: bool,
    pub callback_url: String,
    pub pat: Option<String>,
    pub webhook_secret: Option<String>,
    #[allow(dead_code)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GitHubPackagesTargetDb {
    pub id: String,
    pub input: String,
    pub kind: String,
    pub owner: String,
    pub warnings: Vec<String>,
    #[allow(dead_code)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GitHubPackagesRepoDb {
    pub owner: String,
    pub repo: String,
    pub selected: bool,
    pub webhook_state: String,
    pub webhook_job_id: Option<String>,
    pub hook_id: Option<i64>,
    pub last_sync_at: Option<String>,
    pub last_audit_at: Option<String>,
    pub last_op: Option<String>,
    pub last_error: Option<String>,
    #[allow(dead_code)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GitHubPackagesWebhookDeliveryDb {
    pub delivery_id: String,
    pub received_at: String,
    pub first_received_at: String,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub event: Option<String>,
    pub action: Option<String>,
    pub decision: String,
    pub reason: Option<String>,
    pub response_status: Option<u16>,
    pub job_id: Option<String>,
    pub job_ids: Vec<String>,
    pub attempt_count: u32,
}
