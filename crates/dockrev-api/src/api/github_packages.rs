use super::*;

mod delivery_events;
mod target_resolution;

use delivery_events::emit_github_packages_delivery_event;
pub(super) use delivery_events::{
    github_packages_webhook_delivery_events, list_github_packages_webhook_deliveries,
};
#[allow(unused_imports)]
pub(super) use target_resolution::urls_match;
#[cfg(test)]
use target_resolution::{
    GhcrLinkedRepoProbeResult, ghcr_deployed_repo_keys, ghcr_linked_selection_value,
    normalize_github_source_repo_key, preferred_ghcr_inspection_reference,
};
use target_resolution::{
    append_github_webhook_audit_log, enqueue_github_webhook_check_job,
    enqueue_github_webhook_discovery_job, extract_owner_login, extract_repo_full_name,
    github_webhook_audit_summary, github_webhook_repo_key_from_full_name,
    list_github_webhook_matched_services, record_github_packages_delivery, verify_github_signature,
};
pub(super) use target_resolution::{resolve_github_packages_target, sync_github_packages_webhooks};

pub(super) async fn get_github_packages_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<GitHubPackagesSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let targets = state
        .db
        .list_github_packages_targets()
        .await
        .map_err(map_internal)?;
    let repos_total = state
        .db
        .count_github_packages_repos_total()
        .await
        .map_err(map_internal)?;
    let repos_selected_total = state
        .db
        .count_github_packages_repos_selected_total()
        .await
        .map_err(map_internal)?;

    Ok(Json(GitHubPackagesSettingsResponse {
        enabled: settings.enabled,
        callback_url: settings.callback_url,
        targets: targets
            .into_iter()
            .map(|t| GitHubPackagesTarget {
                input: t.input,
                kind: t.kind,
                owner: t.owner,
                warnings: t.warnings,
            })
            .collect(),
        repos_total,
        repos_selected_total,
        pat_masked: mask_if_some(&settings.pat),
        secret_masked: mask_if_some(&settings.webhook_secret),
    }))
}

pub(super) async fn put_github_packages_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<PutGitHubPackagesSettingsRequest>,
) -> Result<Json<PutGitHubPackagesSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    let _ = Url::parse(&req.callback_url)
        .map_err(|_| ApiError::invalid_argument("invalid callbackUrl"))?;

    let existing = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;

    let mut pat = req.pat;
    merge_secret(&mut pat, existing.pat);

    let mut webhook_secret = existing.webhook_secret;
    if webhook_secret.as_deref().unwrap_or_default().is_empty() {
        webhook_secret = Some(gen_webhook_secret().map_err(map_internal)?);
    }

    if req.enabled && pat.as_deref().unwrap_or_default().is_empty() {
        return Err(ApiError::invalid_argument(
            "pat is required when enabled=true",
        ));
    }

    let settings = GitHubPackagesSettingsDb {
        enabled: req.enabled,
        callback_url: req.callback_url,
        pat,
        webhook_secret,
        updated_at: Some(now.clone()),
    };

    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .map_err(map_internal)?;

    if let Some(req_targets) = req.targets {
        let mut targets = Vec::new();
        for t in req_targets {
            let kind = github::parse_target_input(&t.input).map_err(|e| {
                ApiError::invalid_argument("invalid target input")
                    .with_details(json!({"input": t.input, "error": e.to_string()}))
            })?;
            let (kind_str, owner) = match kind {
                github::TargetKind::Owner { owner } => ("owner".to_string(), owner),
                github::TargetKind::Repo { owner, .. } => ("repo".to_string(), owner),
            };
            targets.push(GitHubPackagesTargetDb {
                id: ulid::Ulid::new().to_string(),
                input: t.input,
                kind: kind_str,
                owner,
                warnings: Vec::new(),
                updated_at: Some(now.clone()),
            });
        }
        state
            .db
            .put_github_packages_targets(&targets, &now)
            .await
            .map_err(map_internal)?;
    }

    if let Some(req_repos) = req.repos {
        let repos = normalize_github_repo_selection(req_repos).map_err(|e| {
            ApiError::invalid_argument("invalid repos")
                .with_details(json!({"error": e.to_string()}))
        })?;
        state
            .db
            .put_github_packages_repos(&repos, &now)
            .await
            .map_err(map_internal)?;
    }

    Ok(Json(PutGitHubPackagesSettingsResponse { ok: true }))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListGitHubPackagesReposQuery {
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    per_page: Option<u32>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    selected_filter: Option<String>, // all|selected|unselected
}

pub(super) fn parse_selected_filter(v: Option<&str>) -> Result<Option<bool>, ApiError> {
    let Some(v) = v else { return Ok(None) };
    match v.trim() {
        "" | "all" => Ok(None),
        "selected" => Ok(Some(true)),
        "unselected" => Ok(Some(false)),
        _ => Err(ApiError::invalid_argument("invalid selectedFilter")),
    }
}

pub(super) async fn list_github_packages_repos(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ListGitHubPackagesReposQuery>,
) -> Result<Json<ListGitHubPackagesReposResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let selected_filter = parse_selected_filter(q.selected_filter.as_deref())?;

    let total = state
        .db
        .count_github_packages_repos_total()
        .await
        .map_err(map_internal)?;
    let selected_total = state
        .db
        .count_github_packages_repos_selected_total()
        .await
        .map_err(map_internal)?;
    let filtered_total = state
        .db
        .count_github_packages_repos_filtered(q.q.as_deref(), selected_filter)
        .await
        .map_err(map_internal)?;

    let offset = (page - 1).saturating_mul(per_page);
    let repos = state
        .db
        .list_github_packages_repos_page(q.q.as_deref(), selected_filter, per_page, offset)
        .await
        .map_err(map_internal)?;

    Ok(Json(ListGitHubPackagesReposResponse {
        page,
        per_page,
        total,
        filtered_total,
        selected_total,
        repos: repos
            .into_iter()
            .map(|r| GitHubPackagesRepo {
                full_name: format!("{}/{}", r.owner, r.repo),
                selected: r.selected,
                webhook_state: Some(r.webhook_state),
                webhook_job_id: r.webhook_job_id,
                hook_id: r.hook_id,
                last_sync_at: r.last_sync_at,
                last_audit_at: r.last_audit_at,
                last_op: r.last_op,
                last_error: r.last_error,
            })
            .collect(),
    }))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListGitHubPackagesWebhookDeliveriesQuery {
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    per_page: Option<u32>,
    #[serde(default)]
    decision: Option<String>,
    #[serde(default)]
    q: Option<String>,
}

pub(super) fn parse_delivery_decision_filter(
    input: Option<&str>,
) -> Result<Option<&'static str>, ApiError> {
    match input.unwrap_or("all") {
        "all" => Ok(None),
        "processed" => Ok(Some("processed")),
        "ignored" => Ok(Some("ignored")),
        "rejected" => Ok(Some("rejected")),
        _ => Err(ApiError::invalid_argument("invalid decision filter")),
    }
}

pub(super) async fn get_github_packages_webhook_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<GitHubPackagesWebhookOverviewResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let overview = ghcr_webhook_jobs::get_overview(&state)
        .await
        .map_err(map_internal)?;
    Ok(Json(overview))
}

pub(super) async fn trigger_github_packages_webhook_sync_all(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<TriggerGitHubPackagesWebhookSyncAllResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    ensure_github_packages_sync_ready(&state).await?;

    let queued = ghcr_webhook_jobs::enqueue_sync_all_job(&state, &user.principal, "ui")
        .await
        .map_err(map_ghcr_sync_enqueue_error)?;
    Ok(Json(TriggerGitHubPackagesWebhookSyncAllResponse {
        ok: true,
        job_id: queued.job_id,
        status: queued.status,
        reused: queued.reused,
    }))
}

pub(super) async fn trigger_github_packages_webhook_sync_repo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TriggerGitHubPackagesWebhookSyncRepoRequest>,
) -> Result<Json<TriggerGitHubPackagesWebhookSyncRepoResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    ensure_github_packages_sync_ready(&state).await?;

    let full_name = req.full_name.trim();
    if full_name.is_empty() {
        return Err(ApiError::invalid_argument("fullName is required"));
    }

    let queued = ghcr_webhook_jobs::enqueue_sync_repo_job(&state, full_name, &user.principal, "ui")
        .await
        .map_err(map_ghcr_sync_enqueue_error)?;
    Ok(Json(TriggerGitHubPackagesWebhookSyncRepoResponse {
        ok: true,
        job_id: queued.job_id,
        status: queued.status,
        reused: queued.reused,
    }))
}

pub(super) async fn ensure_github_packages_sync_ready(
    state: &Arc<AppState>,
) -> Result<(), ApiError> {
    let settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    if !settings.enabled {
        return Err(ApiError::invalid_argument(
            "github packages webhook is disabled",
        ));
    }
    if settings.callback_url.trim().is_empty() {
        return Err(ApiError::invalid_argument("callbackUrl is required"));
    }
    let _ = Url::parse(&settings.callback_url)
        .map_err(|_| ApiError::invalid_argument("invalid callbackUrl"))?;
    Ok(())
}

pub(super) async fn set_github_packages_repo_selected(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetGitHubPackagesRepoSelectedRequest>,
) -> Result<Json<SetGitHubPackagesRepoSelectedResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    let mut parts = req.full_name.split('/');
    let owner = parts.next().unwrap_or_default().trim();
    let repo = parts.next().unwrap_or_default().trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return Err(ApiError::invalid_argument("invalid fullName"));
    }

    state
        .db
        .upsert_github_packages_repo_selected(owner, repo, req.selected, &now)
        .await
        .map_err(map_internal)?;

    let mut job_id: Option<String> = None;
    if req.selected {
        let settings = state
            .db
            .get_github_packages_settings()
            .await
            .map_err(map_internal)?;
        let callback_ready =
            !settings.callback_url.trim().is_empty() && Url::parse(&settings.callback_url).is_ok();
        if settings.enabled && callback_ready {
            let full_name = format!("{owner}/{repo}");
            let queued = ghcr_webhook_jobs::enqueue_repo_job(
                &state,
                &full_name,
                ghcr_webhook_jobs::GhcrWebhookOp::Register,
                &user.principal,
                "ui",
            )
            .await
            .map_err(map_internal)?;
            job_id = Some(queued);
        }
    }

    Ok(Json(SetGitHubPackagesRepoSelectedResponse {
        ok: true,
        job_id,
    }))
}

pub(super) async fn delete_github_packages_repo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DeleteGitHubPackagesRepoRequest>,
) -> Result<Json<DeleteGitHubPackagesRepoResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;

    let mut parts = req.full_name.split('/');
    let owner = parts.next().unwrap_or_default().trim();
    let repo = parts.next().unwrap_or_default().trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return Err(ApiError::invalid_argument("invalid fullName"));
    }

    let existing = state
        .db
        .get_github_packages_repo(owner, repo)
        .await
        .map_err(map_internal)?;
    if existing.is_none() {
        return Err(ApiError::not_found("repo is not tracked"));
    }

    let job_id = ghcr_webhook_jobs::enqueue_repo_job(
        &state,
        &format!("{owner}/{repo}"),
        ghcr_webhook_jobs::GhcrWebhookOp::Unregister,
        &user.principal,
        "ui",
    )
    .await
    .map_err(map_internal)?;

    Ok(Json(DeleteGitHubPackagesRepoResponse { ok: true, job_id }))
}

pub(super) async fn bulk_set_github_packages_repos_selected(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<BulkSetGitHubPackagesReposSelectedRequest>,
) -> Result<Json<BulkSetGitHubPackagesReposSelectedResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    let selected_filter = parse_selected_filter(req.selected_filter.as_deref())?;
    let affected = state
        .db
        .bulk_set_github_packages_repos_selected(
            req.q.as_deref(),
            selected_filter,
            req.selected,
            &now,
        )
        .await
        .map_err(map_internal)?;

    Ok(Json(BulkSetGitHubPackagesReposSelectedResponse {
        ok: true,
        affected,
    }))
}

pub(super) async fn add_github_packages_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AddGitHubPackagesTargetRequest>,
) -> Result<Json<AddGitHubPackagesTargetResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    let settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let Some(pat) = settings.pat else {
        return Err(ApiError::invalid_argument("pat is required"));
    };

    let parsed = github::parse_target_input(&req.input).map_err(|e| {
        ApiError::invalid_argument("invalid target input")
            .with_details(json!({"input": req.input, "error": e.to_string()}))
    })?;

    let client = github::GitHubClient::new(&pat).map_err(map_internal)?;

    let (kind, owner, repos): (String, String, Vec<(String, String)>) = match parsed {
        github::TargetKind::Repo { owner, repo } => {
            ("repo".to_string(), owner.clone(), vec![(owner, repo)])
        }
        github::TargetKind::Owner { owner } => {
            let repos = client
                .list_owner_repos(&owner)
                .await
                .map_err(map_internal)?;
            let mut out = Vec::new();
            for r in repos {
                let mut parts = r.full_name.split('/');
                let ro = parts.next().unwrap_or_default().trim();
                let rr = parts.next().unwrap_or_default().trim();
                if ro.is_empty() || rr.is_empty() || parts.next().is_some() {
                    continue;
                }
                out.push((ro.to_string(), rr.to_string()));
            }
            ("owner".to_string(), owner, out)
        }
    };

    state
        .db
        .upsert_github_packages_target_by_input(&req.input, &kind, &owner, &[], &now)
        .await
        .map_err(map_internal)?;

    let repos_added = state
        .db
        .upsert_github_packages_repos_default_selected(&repos, &now)
        .await
        .map_err(map_internal)?;

    Ok(Json(AddGitHubPackagesTargetResponse {
        ok: true,
        kind,
        owner,
        repos_added,
    }))
}

pub(super) async fn remove_github_packages_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RemoveGitHubPackagesTargetRequest>,
) -> Result<Json<RemoveGitHubPackagesTargetResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let _ = state
        .db
        .delete_github_packages_target_by_input(&req.input)
        .await
        .map_err(map_internal)?;

    Ok(Json(RemoveGitHubPackagesTargetResponse { ok: true }))
}

pub(super) async fn github_packages_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let delivery_id = headers
        .get("X-GitHub-Delivery")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if delivery_id.is_empty() {
        return Err(ApiError::invalid_argument("missing X-GitHub-Delivery"));
    }

    let event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let received_at = now_rfc3339().map_err(map_internal)?;
    if event != "package" {
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "not_package_event"}),
        ));
    }

    let sig = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;

    if !settings.enabled {
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "disabled"}),
        ));
    }

    let Some(secret) = settings.webhook_secret else {
        return Err(ApiError::unauthorized()
            .with_details(json!({"reason":"webhook_secret_not_configured"})));
    };
    if verify_github_signature(&secret, &sig, &body).is_err() {
        return Err(ApiError::unauthorized().with_details(json!({"reason":"invalid_signature"})));
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => {
            let _ = record_github_packages_delivery(
                &state,
                GitHubPackagesWebhookDeliveryRecordInput {
                    delivery_id: delivery_id.clone(),
                    received_at,
                    owner: None,
                    repo: None,
                    event: Some(event),
                    action: None,
                    decision: "rejected".to_string(),
                    reason: Some("invalid_json".to_string()),
                    response_status: Some(400),
                    job_id: None,
                    job_ids: Vec::new(),
                },
            )
            .await;
            let _ = emit_github_packages_delivery_event(&state, &delivery_id).await;
            return Err(ApiError::invalid_argument("invalid json"));
        }
    };
    let action = payload
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if action != "published" {
        let repo_full_name = extract_repo_full_name(&payload);
        let owner = repo_full_name
            .as_deref()
            .and_then(|s| s.split('/').next().map(|v| v.to_string()))
            .or_else(|| extract_owner_login(&payload));
        let repo = repo_full_name
            .as_deref()
            .and_then(|s| s.split('/').nth(1))
            .map(|v| v.to_string());
        record_github_packages_delivery(
            &state,
            GitHubPackagesWebhookDeliveryRecordInput {
                delivery_id: delivery_id.clone(),
                received_at,
                owner,
                repo,
                event: Some(event),
                action: Some(action),
                decision: "ignored".to_string(),
                reason: Some("not_published".to_string()),
                response_status: Some(200),
                job_id: None,
                job_ids: Vec::new(),
            },
        )
        .await?;
        let _ = emit_github_packages_delivery_event(&state, &delivery_id).await;
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "not_published"}),
        ));
    }

    let repo_full_name = extract_repo_full_name(&payload);
    let owner = repo_full_name
        .as_deref()
        .and_then(|s| s.split('/').next().map(|v| v.to_string()))
        .or_else(|| extract_owner_login(&payload));
    let repo = repo_full_name
        .as_deref()
        .and_then(|s| s.split('/').nth(1))
        .map(|v| v.to_string());

    // NOTE: GitHub repo names are case-insensitive but case-preserving; compare in lower-case so we
    // don't mistakenly drop events due to casing differences between stored data and payloads.
    let mut selected_repos_lower = std::collections::HashSet::<String>::new();
    let mut selected_owners_lower = std::collections::HashSet::<String>::new();
    for r in state
        .db
        .list_github_packages_repos()
        .await
        .map_err(map_internal)?
        .into_iter()
        .filter(|r| r.selected)
    {
        selected_owners_lower.insert(r.owner.to_ascii_lowercase());
        selected_repos_lower.insert(format!("{}/{}", r.owner, r.repo).to_ascii_lowercase());
    }

    let should_trigger = if let Some(full) = &repo_full_name {
        selected_repos_lower.contains(&full.to_ascii_lowercase())
    } else if let Some(owner) = &owner {
        selected_owners_lower.contains(&owner.to_ascii_lowercase())
    } else {
        false
    };

    if !should_trigger {
        let delivery_exists = state
            .db
            .github_packages_delivery_exists(&delivery_id)
            .await
            .map_err(map_internal)?;
        if delivery_exists {
            let attempt_count = state
                .db
                .increment_github_packages_delivery_attempt(
                    &delivery_id,
                    &received_at,
                    owner.as_deref(),
                    repo.as_deref(),
                    Some(event.as_str()),
                    Some(action.as_str()),
                )
                .await
                .map_err(map_internal)?;
            let _ = emit_github_packages_delivery_event(&state, &delivery_id).await;
            return Ok(Json(
                json!({"ok": true, "ignored": true, "reason": "duplicate_delivery", "attemptCount": attempt_count}),
            ));
        }

        record_github_packages_delivery(
            &state,
            GitHubPackagesWebhookDeliveryRecordInput {
                delivery_id: delivery_id.clone(),
                received_at,
                owner,
                repo,
                event: Some(event),
                action: Some(action),
                decision: "ignored".to_string(),
                reason: Some("repo_not_selected".to_string()),
                response_status: Some(200),
                job_id: None,
                job_ids: Vec::new(),
            },
        )
        .await?;
        let _ = emit_github_packages_delivery_event(&state, &delivery_id).await;
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "repo_not_selected"}),
        ));
    }

    let is_new_delivery = state
        .db
        .insert_github_packages_delivery_if_new(
            &delivery_id,
            &received_at,
            owner.as_deref(),
            repo.as_deref(),
        )
        .await
        .map_err(map_internal)?;
    if !is_new_delivery {
        let attempt_count = state
            .db
            .increment_github_packages_delivery_attempt(
                &delivery_id,
                &received_at,
                owner.as_deref(),
                repo.as_deref(),
                Some(event.as_str()),
                Some(action.as_str()),
            )
            .await
            .map_err(map_internal)?;
        let _ = emit_github_packages_delivery_event(&state, &delivery_id).await;
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "duplicate_delivery", "attemptCount": attempt_count}),
        ));
    }

    let delivery_has_work = AtomicBool::new(false);
    let result: Result<serde_json::Value, ApiError> = async {
        let now = received_at.clone();
        let repo_key = github_webhook_repo_key_from_full_name(repo_full_name.as_deref());
        let stale_threshold = time::Duration::hours(2);

        let (job_ids, reused_job_ids, matched_service_ids, fallback_used, fallback_reason) =
            if let Some(repo_key) = repo_key.as_deref() {
                let matched_targets =
                    list_github_webhook_matched_services(&state, repo_key).await?;
                if matched_targets.is_empty() {
                    let fallback_reason = "no_managed_service_match".to_string();
                    let initial_audit = github_webhook_audit_summary(
                        Some(repo_key),
                        &delivery_id,
                        &[],
                        true,
                        &[],
                        Some(&fallback_reason),
                    );
                    let (job_id, reused) = enqueue_github_webhook_discovery_job(
                        &state,
                        &now,
                        stale_threshold,
                        initial_audit,
                    )
                    .await?;
                    let job_ids = vec![job_id.clone()];
                    let reused_job_ids = if reused {
                        vec![job_id.clone()]
                    } else {
                        Vec::new()
                    };
                    let final_audit = github_webhook_audit_summary(
                        Some(repo_key),
                        &delivery_id,
                        &[],
                        true,
                        &reused_job_ids,
                        Some(&fallback_reason),
                    );
                    if reused {
                        append_github_webhook_audit_log(
                            &state,
                            &job_id,
                            &now,
                            "reused discovery fallback",
                            &final_audit,
                        )
                        .await;
                    }
                    (
                        job_ids,
                        reused_job_ids,
                        Vec::new(),
                        true,
                        Some(fallback_reason),
                    )
                } else {
                    let mut seen_service_ids = std::collections::HashSet::<String>::new();
                    let matched_service_ids = matched_targets
                        .iter()
                        .filter_map(|target| {
                            if seen_service_ids.insert(target.service_id.clone()) {
                                Some(target.service_id.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let initial_audit = github_webhook_audit_summary(
                        Some(repo_key),
                        &delivery_id,
                        &matched_service_ids,
                        false,
                        &[],
                        None,
                    );
                    let mut job_ids = Vec::<String>::new();
                    let mut inserted_job_ids = Vec::<String>::new();
                    let mut reused_job_ids = Vec::<String>::new();

                    for target in &matched_targets {
                        let (job_id, reused) = enqueue_github_webhook_check_job(
                            &state,
                            &now,
                            stale_threshold,
                            target,
                            initial_audit.clone(),
                        )
                        .await?;
                        if reused {
                            if !reused_job_ids.contains(&job_id) {
                                reused_job_ids.push(job_id.clone());
                            }
                        } else if !inserted_job_ids.contains(&job_id) {
                            inserted_job_ids.push(job_id.clone());
                        }
                        if !job_ids.contains(&job_id) {
                            job_ids.push(job_id);
                        }
                    }

                    let final_audit = github_webhook_audit_summary(
                        Some(repo_key),
                        &delivery_id,
                        &matched_service_ids,
                        false,
                        &reused_job_ids,
                        None,
                    );
                    for job_id in &reused_job_ids {
                        append_github_webhook_audit_log(
                            &state,
                            job_id,
                            &now,
                            "reused check job",
                            &final_audit,
                        )
                        .await;
                    }
                    for job_id in &inserted_job_ids {
                        let _ = state
                            .db
                            .merge_job_summary_fields(job_id, &final_audit)
                            .await;
                    }

                    (job_ids, reused_job_ids, matched_service_ids, false, None)
                }
            } else {
                let fallback_reason = "owner_only_payload".to_string();
                let initial_audit = github_webhook_audit_summary(
                    None,
                    &delivery_id,
                    &[],
                    true,
                    &[],
                    Some(&fallback_reason),
                );
                let (job_id, reused) = enqueue_github_webhook_discovery_job(
                    &state,
                    &now,
                    stale_threshold,
                    initial_audit,
                )
                .await?;
                let job_ids = vec![job_id.clone()];
                let reused_job_ids = if reused {
                    vec![job_id.clone()]
                } else {
                    Vec::new()
                };
                let final_audit = github_webhook_audit_summary(
                    None,
                    &delivery_id,
                    &[],
                    true,
                    &reused_job_ids,
                    Some(&fallback_reason),
                );
                if reused {
                    append_github_webhook_audit_log(
                        &state,
                        &job_id,
                        &now,
                        "reused discovery fallback",
                        &final_audit,
                    )
                    .await;
                }
                (
                    job_ids,
                    reused_job_ids,
                    Vec::new(),
                    true,
                    Some(fallback_reason),
                )
            };

        if !job_ids.is_empty() {
            delivery_has_work.store(true, Ordering::Relaxed);
        }
        let primary_job_id = job_ids.first().cloned();
        state
            .db
            .update_github_packages_delivery_outcome(
                &delivery_id,
                &received_at,
                owner.as_deref(),
                repo.as_deref(),
                Some(event.as_str()),
                Some(action.as_str()),
                "processed",
                None,
                Some(200),
                primary_job_id.as_deref(),
                &job_ids,
            )
            .await
            .map_err(map_internal)?;
        let _ = emit_github_packages_delivery_event(&state, &delivery_id).await;

        let mut response = json!({
            "ok": true,
            "attemptCount": 1,
            "jobIds": job_ids,
            "matchedServiceIds": matched_service_ids,
            "reusedJobIds": reused_job_ids,
            "fallbackUsed": fallback_used,
        });
        if let Some(obj) = response.as_object_mut() {
            if let Some(job_id) = primary_job_id {
                obj.insert("jobId".to_string(), json!(job_id));
            }
            if let Some(reason) = fallback_reason {
                obj.insert("fallbackReason".to_string(), json!(reason));
            }
        }

        Ok(response)
    }
    .await;

    match result {
        Ok(response) => Ok(Json(response)),
        Err(err) => {
            if !delivery_has_work.load(Ordering::Relaxed) {
                let _ = state.db.delete_github_packages_delivery(&delivery_id).await;
            }
            Err(err)
        }
    }
}

#[cfg(test)]
mod resolve_metadata_tests;
