use super::*;

fn normalize_repo_full_name(full_name: &str) -> String {
    full_name.trim().to_ascii_lowercase()
}

pub(super) fn normalize_github_source_repo_key(source: &str) -> Option<String> {
    match github::parse_target_input(source).ok()? {
        github::TargetKind::Repo { owner, repo } => Some(format!(
            "{}/{}",
            owner.to_ascii_lowercase(),
            repo.to_ascii_lowercase()
        )),
        github::TargetKind::Owner { .. } => None,
    }
}

pub(super) fn ghcr_deployed_repo_keys(
    targets: Vec<crate::db::GithubWebhookServiceTarget>,
) -> std::collections::HashSet<String> {
    targets
        .into_iter()
        .filter_map(|target| crate::snapshot_worker::image_repo_from_image_ref(&target.image_ref))
        .filter_map(|image_repo| image_repo.strip_prefix("ghcr.io/").map(str::to_string))
        .map(|repo| repo.to_ascii_lowercase())
        .collect()
}

pub(super) fn preferred_ghcr_inspection_reference(tags: &[String]) -> Option<&str> {
    if tags.iter().any(|tag| tag == "latest") {
        return Some("latest");
    }
    tags.iter()
        .rev()
        .map(|tag| tag.trim())
        .find(|tag| !tag.is_empty())
}

fn push_warning_limited(warnings: &mut Vec<String>, message: String) {
    if warnings.len() >= 5 {
        return;
    }
    if warnings.iter().any(|existing| existing == &message) {
        return;
    }
    warnings.push(message);
}

pub(super) struct GhcrLinkedRepoProbeResult {
    pub(super) linked_repo_keys: std::collections::HashSet<String>,
    pub(super) probe_complete: bool,
}

struct GhcrPackageProbeOutcome {
    linked_repo_key: Option<String>,
    warning: Option<String>,
    probe_complete: bool,
}

async fn inspect_owner_ghcr_package_link(
    registry: crate::registry::HttpRegistryClient,
    owner: String,
    package_name: String,
    host_platform: String,
) -> GhcrPackageProbeOutcome {
    let image =
        match crate::registry::ImageRef::parse(&format!("ghcr.io/{owner}/{package_name}:latest")) {
            Ok(image) => image,
            Err(err) => {
                return GhcrPackageProbeOutcome {
                    linked_repo_key: None,
                    warning: Some(format!(
                        "skip invalid GHCR package ref {owner}/{package_name}: {err}"
                    )),
                    probe_complete: false,
                };
            }
        };
    let tags = match crate::registry::RegistryClient::list_tags(&registry, &image).await {
        Ok(tags) => tags,
        Err(err) => {
            return GhcrPackageProbeOutcome {
                linked_repo_key: None,
                warning: Some(format!(
                    "skip GHCR package {owner}/{package_name}: list tags failed ({err})"
                )),
                probe_complete: false,
            };
        }
    };
    let Some(reference) = preferred_ghcr_inspection_reference(&tags) else {
        return GhcrPackageProbeOutcome {
            linked_repo_key: None,
            warning: None,
            probe_complete: true,
        };
    };
    let source = match crate::registry::RegistryClient::get_oci_source(
        &registry,
        &image,
        reference,
        &host_platform,
    )
    .await
    {
        Ok(source) => source,
        Err(err) => {
            return GhcrPackageProbeOutcome {
                linked_repo_key: None,
                warning: Some(format!(
                    "skip GHCR package {owner}/{package_name}: read OCI source failed ({err})"
                )),
                probe_complete: false,
            };
        }
    };

    GhcrPackageProbeOutcome {
        linked_repo_key: source.as_deref().and_then(normalize_github_source_repo_key),
        warning: None,
        probe_complete: true,
    }
}

async fn resolve_owner_ghcr_linked_repo_keys(
    state: &Arc<AppState>,
    client: &github::GitHubClient,
    owner: &str,
    pat: &str,
    authenticated_login: Option<&str>,
) -> (Option<GhcrLinkedRepoProbeResult>, Vec<String>) {
    let package_names = match client
        .list_owner_container_package_names(owner, authenticated_login)
        .await
    {
        Ok(packages) => packages,
        Err(err) => {
            return (
                None,
                vec![format!("GHCR package metadata unavailable: {err}")],
            );
        }
    };

    if package_names.is_empty() {
        return (
            Some(GhcrLinkedRepoProbeResult {
                linked_repo_keys: std::collections::HashSet::new(),
                probe_complete: true,
            }),
            Vec::new(),
        );
    }

    let mut warnings = Vec::new();
    let mut auth_overrides = std::collections::HashMap::new();
    if let Some(login) = authenticated_login {
        let trimmed = login.trim();
        if !trimmed.is_empty() {
            auth_overrides.insert(
                "ghcr.io".to_string(),
                (trimmed.to_string(), pat.to_string()),
            );
        }
    }
    let registry = match crate::registry::HttpRegistryClient::new_with_basic_auth_overrides(
        state.config.docker_config_path.as_deref(),
        crate::registry::HttpRegistryClientOptions {
            per_host_concurrency: state.config.registry_per_host_concurrency,
            retry_max_attempts: state.config.registry_retry_max_attempts,
            retry_base_ms: state.config.registry_retry_base_ms,
            retry_max_ms: state.config.registry_retry_max_ms,
        },
        auth_overrides,
    ) {
        Ok(client) => client,
        Err(err) => {
            return (
                None,
                vec![format!("GHCR registry client unavailable: {err}")],
            );
        }
    };
    let host_platform =
        crate::registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
    let mut linked = std::collections::HashSet::new();
    let mut probe_complete = true;
    let max_in_flight = state
        .config
        .registry_per_host_concurrency
        .max(1)
        .min(package_names.len().max(1));
    let owner = owner.to_string();
    let mut pending = package_names.into_iter();
    let mut join_set = tokio::task::JoinSet::new();

    for _ in 0..max_in_flight {
        let Some(package_name) = pending.next() else {
            break;
        };
        join_set.spawn(inspect_owner_ghcr_package_link(
            registry.clone(),
            owner.clone(),
            package_name,
            host_platform.clone(),
        ));
    }

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(outcome) => {
                if let Some(repo_key) = outcome.linked_repo_key {
                    linked.insert(repo_key);
                }
                if let Some(warning) = outcome.warning {
                    push_warning_limited(&mut warnings, warning);
                }
                if !outcome.probe_complete {
                    probe_complete = false;
                }
            }
            Err(err) => {
                probe_complete = false;
                push_warning_limited(
                    &mut warnings,
                    format!("GHCR package inspection task failed: {err}"),
                );
            }
        }

        if let Some(package_name) = pending.next() {
            join_set.spawn(inspect_owner_ghcr_package_link(
                registry.clone(),
                owner.clone(),
                package_name,
                host_platform.clone(),
            ));
        }
    }

    (
        Some(GhcrLinkedRepoProbeResult {
            linked_repo_keys: linked,
            probe_complete,
        }),
        warnings,
    )
}

pub(super) fn ghcr_linked_selection_value(
    ghcr_linked_probe: Option<&GhcrLinkedRepoProbeResult>,
    full_name: &str,
) -> Option<bool> {
    let normalized_full_name = normalize_repo_full_name(full_name);
    let probe = ghcr_linked_probe?;
    if probe.linked_repo_keys.contains(&normalized_full_name) {
        return Some(true);
    }
    if probe.probe_complete {
        return Some(false);
    }
    None
}

pub(crate) async fn resolve_github_packages_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ResolveGitHubPackagesTargetRequest>,
) -> Result<Json<ResolveGitHubPackagesTargetResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let parsed = github::parse_target_input(&req.input).map_err(|e| {
        ApiError::invalid_argument("invalid input")
            .with_details(json!({"input": req.input, "error": e.to_string()}))
    })?;

    let deployed_repo_keys = ghcr_deployed_repo_keys(
        state
            .db
            .list_active_github_webhook_service_targets()
            .await
            .map_err(map_internal)?,
    );

    match parsed {
        github::TargetKind::Repo { owner, repo } => {
            let selected = state
                .db
                .get_github_packages_repo_selected(owner.as_str(), repo.as_str())
                .await
                .map_err(map_internal)?
                .unwrap_or(true);
            let full_name = format!("{owner}/{repo}");
            Ok(Json(ResolveGitHubPackagesTargetResponse {
                kind: "repo".to_string(),
                owner: owner.clone(),
                repos: vec![GitHubPackagesRepoSelection {
                    full_name: full_name.clone(),
                    selected,
                    visibility: Some("unknown".to_string()),
                    last_activity_at: None,
                    ghcr_linked: None,
                    deployed: deployed_repo_keys.contains(&normalize_repo_full_name(&full_name)),
                }],
                warnings: Vec::new(),
            }))
        }
        github::TargetKind::Owner { owner } => {
            let settings = state
                .db
                .get_github_packages_settings()
                .await
                .map_err(map_internal)?;
            let Some(pat) = settings.pat else {
                return Err(
                    ApiError::invalid_argument("pat is required before resolving owner")
                        .with_details(json!({"reason":"ghcr_pat_missing","owner":owner})),
                );
            };
            let client = github::GitHubClient::new(&pat).map_err(map_internal)?;
            let authenticated_login = client.get_authenticated_user_login().await.ok();
            let repos = client
                .list_owner_repos(&owner)
                .await
                .map_err(|e| map_github_owner_resolve_error(&owner, e))?;
            let (ghcr_linked_probe, ghcr_warnings) = resolve_owner_ghcr_linked_repo_keys(
                &state,
                &client,
                &owner,
                &pat,
                authenticated_login.as_deref(),
            )
            .await;
            // Default to "not selected", but keep existing tracked repos selected.
            let existing = state
                .db
                .list_github_packages_repos_selected_by_owner(owner.as_str())
                .await
                .map_err(map_internal)?;
            let mut existing_selected = std::collections::HashSet::<String>::new();
            for (repo, selected) in existing {
                if selected {
                    existing_selected.insert(repo.to_lowercase());
                }
            }
            Ok(Json(ResolveGitHubPackagesTargetResponse {
                kind: "owner".to_string(),
                owner: owner.clone(),
                repos: repos
                    .into_iter()
                    .filter_map(|r| {
                        // Avoid borrowing `full_name` across moving it into the response.
                        let full_name = r.full_name;
                        let visibility = if r.is_private { "private" } else { "public" };
                        let last_activity_at = r.pushed_at.or(r.updated_at);
                        let selected = {
                            let mut parts = full_name.split('/');
                            let ro = parts.next().unwrap_or_default().trim();
                            let rr = parts.next().unwrap_or_default().trim();
                            if ro.is_empty() || rr.is_empty() || parts.next().is_some() {
                                return None;
                            }
                            existing_selected.contains(&rr.to_lowercase())
                        };
                        Some(GitHubPackagesRepoSelection {
                            full_name: full_name.clone(),
                            selected,
                            visibility: Some(visibility.to_string()),
                            last_activity_at,
                            ghcr_linked: ghcr_linked_selection_value(
                                ghcr_linked_probe.as_ref(),
                                &full_name,
                            ),
                            deployed: deployed_repo_keys
                                .contains(&normalize_repo_full_name(&full_name)),
                        })
                    })
                    .collect(),
                warnings: ghcr_warnings,
            }))
        }
    }
}

#[allow(dead_code)]
pub(crate) fn urls_match(a: &str, b: &str) -> bool {
    let Ok(au) = Url::parse(a) else { return false };
    let Ok(bu) = Url::parse(b) else { return false };

    // GitHub webhook config URLs are effectively compared by the request destination we will
    // receive, not by exact `Url` string equality. Be tolerant of benign differences to avoid
    // re-creating equivalent hooks (e.g. trailing slashes, default port normalization).
    //
    // We intentionally ignore fragments because they are not sent to the server.
    let (Some(ah), Some(bh)) = (au.host_str(), bu.host_str()) else {
        return false;
    };

    if !au.scheme().eq_ignore_ascii_case(bu.scheme()) {
        return false;
    }

    if !ah.eq_ignore_ascii_case(bh) {
        return false;
    }

    if au.port_or_known_default() != bu.port_or_known_default() {
        return false;
    }

    fn normalize_path(path: &str) -> &str {
        if path.len() <= 1 {
            return path;
        }
        path.trim_end_matches('/')
    }
    if normalize_path(au.path()) != normalize_path(bu.path()) {
        return false;
    }

    au.query() == bu.query()
}

pub(crate) async fn sync_github_packages_webhooks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SyncGitHubPackagesWebhooksRequest>,
) -> Result<Json<SyncGitHubPackagesWebhooksResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    ensure_github_packages_sync_ready(&state).await?;

    let mut selected_repos: Vec<(String, String)> = state
        .db
        .list_github_packages_repos()
        .await
        .map_err(map_internal)?
        .into_iter()
        .filter(|r| r.selected)
        .map(|r| (r.owner, r.repo))
        .collect();
    if let Some(req_repos) = &req.repos {
        let allow = req_repos
            .iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect::<std::collections::HashSet<_>>();
        selected_repos.retain(|(o, r)| allow.contains(&format!("{}/{}", o, r).to_lowercase()));
    }

    let mut results = Vec::new();

    let dry_run = req.dry_run.unwrap_or(false);

    for (owner, repo) in selected_repos {
        let full = format!("{owner}/{repo}");
        if dry_run {
            results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "queued".to_string(),
                hook_id: None,
                conflict_hooks: None,
                message: Some("dryRun: would enqueue sync_repo job".to_string()),
            });
            continue;
        }

        let queued =
            ghcr_webhook_jobs::enqueue_sync_repo_job(&state, &full, &user.principal, "ui").await;
        match queued {
            Ok(enqueued) => results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "queued".to_string(),
                hook_id: None,
                conflict_hooks: None,
                message: Some(format!(
                    "jobId={}{}",
                    enqueued.job_id,
                    if enqueued.reused { " (reused)" } else { "" }
                )),
            }),
            Err(err) => results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "error".to_string(),
                hook_id: None,
                conflict_hooks: None,
                message: Some(err.to_string()),
            }),
        }
    }

    Ok(Json(SyncGitHubPackagesWebhooksResponse {
        ok: results.iter().all(|r| r.action != "error"),
        results,
    }))
}

pub(super) fn verify_github_signature(
    secret: &str,
    sig_header: &str,
    body: &[u8],
) -> anyhow::Result<()> {
    let header = sig_header.trim();
    let hex = header
        .strip_prefix("sha256=")
        .context("signature must start with sha256=")?;
    let tag = hex::decode(hex).context("invalid signature hex")?;
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    ring::hmac::verify(&key, body, &tag).map_err(|_| anyhow::anyhow!("signature mismatch"))?;
    Ok(())
}

pub(super) fn extract_repo_full_name(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("repository")
        .and_then(|v| v.get("full_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            payload
                .get("package")
                .and_then(|p| p.get("repository"))
                .and_then(|v| v.get("full_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

pub(super) fn extract_owner_login(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("organization")
        .and_then(|v| v.get("login"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            payload
                .get("repository")
                .and_then(|v| v.get("owner"))
                .and_then(|v| v.get("login"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            payload
                .get("sender")
                .and_then(|v| v.get("login"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

pub(super) fn github_webhook_repo_key(owner: &str, repo: &str) -> String {
    format!("ghcr.io/{owner}/{repo}").to_ascii_lowercase()
}

pub(super) fn github_webhook_repo_key_from_full_name(full_name: Option<&str>) -> Option<String> {
    let (owner, repo) = full_name?.split_once('/')?;
    Some(github_webhook_repo_key(owner.trim(), repo.trim()))
}

pub(super) fn github_webhook_audit_summary(
    repo: Option<&str>,
    delivery_id: &str,
    matched_service_ids: &[String],
    fallback_used: bool,
    reused_job_ids: &[String],
    fallback_reason: Option<&str>,
) -> serde_json::Value {
    let mut summary = json!({
        "source": "github_webhook",
        "repo": repo,
        "repos": repo.map(|value| vec![value]).unwrap_or_default(),
        "deliveryId": delivery_id,
        "deliveryIds": [delivery_id],
        "matchedServiceIds": matched_service_ids,
        "fallbackUsed": fallback_used,
        "reusedJobIds": reused_job_ids,
    });
    if let Some(reason) = fallback_reason
        && let Some(obj) = summary.as_object_mut()
    {
        obj.insert("fallbackReason".to_string(), json!(reason));
    }
    summary
}

pub(super) async fn append_github_webhook_audit_log(
    state: &Arc<AppState>,
    job_id: &str,
    now: &str,
    action: &str,
    audit: &serde_json::Value,
) {
    let _ = state.db.merge_job_summary_fields(job_id, audit).await;
    let _ = state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: now.to_string(),
                level: "info".to_string(),
                msg: format!("github webhook {action}: {audit}"),
            },
        )
        .await;
}

pub(super) async fn list_github_webhook_matched_services(
    state: &Arc<AppState>,
    repo_key: &str,
) -> Result<Vec<crate::db::GithubWebhookServiceTarget>, ApiError> {
    Ok(state
        .db
        .list_active_github_webhook_service_targets()
        .await
        .map_err(map_internal)?
        .into_iter()
        .filter(|target| {
            snapshot_worker::image_repo_from_image_ref(&target.image_ref)
                .is_some_and(|image_repo| image_repo.eq_ignore_ascii_case(repo_key))
        })
        .collect())
}

pub(super) async fn enqueue_github_webhook_check_job(
    state: &Arc<AppState>,
    now: &str,
    stale_threshold: time::Duration,
    target: &crate::db::GithubWebhookServiceTarget,
    audit: serde_json::Value,
) -> Result<(String, bool), ApiError> {
    let job_id = ids::new_check_id();
    let mut job_db = JobRecord::new_running(
        job_id.clone(),
        JobType::Check,
        JobScope::Service,
        Some(target.stack_id.clone()),
        Some(target.service_id.clone()),
        now,
    )
    .to_db();
    job_db.created_by = "github".to_string();
    job_db.reason = "webhook".to_string();
    job_db.summary_json = audit.clone();

    match state
        .db
        .insert_or_reuse_webhook_check_job_for_service(job_db, now, stale_threshold)
        .await
        .map_err(map_internal)?
    {
        crate::db::PendingJobUpsert::Inserted => {
            let host_platform =
                registry::host_platform_override(state.config.host_platform.as_deref())
                    .unwrap_or_else(|| "linux/amd64".to_string());
            spawn_check_job_task(
                state.clone(),
                job_id.clone(),
                JobScope::Service,
                Some(target.stack_id.clone()),
                Some(target.service_id.clone()),
                host_platform,
                now.to_string(),
                "webhook".to_string(),
                format!("github webhook check started: {audit}"),
                "github webhook check failed".to_string(),
                Some(audit),
            );
            Ok((job_id, false))
        }
        crate::db::PendingJobUpsert::Reused(existing) => Ok((existing.id.clone(), true)),
    }
}

pub(super) async fn enqueue_github_webhook_discovery_job(
    state: &Arc<AppState>,
    now: &str,
    stale_threshold: time::Duration,
    audit: serde_json::Value,
) -> Result<(String, bool), ApiError> {
    let job_id = ids::new_discovery_id();
    let mut job_db = JobRecord::new_running(
        job_id.clone(),
        JobType::Discovery,
        JobScope::All,
        None,
        None,
        now,
    )
    .to_db();
    job_db.created_by = "github".to_string();
    job_db.reason = "github_webhook".to_string();
    job_db.summary_json = audit.clone();

    match state
        .db
        .insert_or_reuse_webhook_discovery_job(job_db, now, stale_threshold)
        .await
        .map_err(map_internal)?
    {
        crate::db::PendingJobUpsert::Inserted => {
            let run_state = state.clone();
            let run_job_id = job_id.clone();
            let run_started_at = now.to_string();
            tokio::spawn(async move {
                append_github_webhook_audit_log(
                    &run_state,
                    &run_job_id,
                    &run_started_at,
                    "started discovery fallback",
                    &audit,
                )
                .await;

                let outcome = discovery::run_scan_for_job(run_state.as_ref(), &run_job_id).await;
                let finished_at =
                    now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
                match outcome {
                    Ok(resp) => {
                        let summary = merge_job_summary(json!({ "scan": resp }), Some(&audit));
                        let _ = run_state
                            .db
                            .finish_job(&run_job_id, "success", &finished_at, &summary)
                            .await;
                    }
                    Err(e) => {
                        let _ = run_state
                            .db
                            .insert_job_log(
                                &run_job_id,
                                &JobLogLine {
                                    ts: finished_at.clone(),
                                    level: "error".to_string(),
                                    msg: format!("discovery scan failed: {e}"),
                                },
                            )
                            .await;
                        let summary =
                            merge_job_summary(json!({ "error": e.to_string() }), Some(&audit));
                        let _ = run_state
                            .db
                            .finish_job(&run_job_id, "failed", &finished_at, &summary)
                            .await;
                    }
                }
            });

            Ok((job_id, false))
        }
        crate::db::PendingJobUpsert::Reused(existing) => Ok((existing.id.clone(), true)),
    }
}

pub(super) async fn record_github_packages_delivery(
    state: &Arc<AppState>,
    input: GitHubPackagesWebhookDeliveryRecordInput,
) -> Result<u32, ApiError> {
    state
        .db
        .record_github_packages_delivery(input)
        .await
        .map_err(map_internal)
}
