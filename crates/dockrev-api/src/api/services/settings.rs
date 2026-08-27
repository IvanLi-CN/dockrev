use super::*;

pub(in crate::api) async fn get_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(stored) = settings else {
        return Err(ApiError::not_found("service not found"));
    };
    Ok(Json(ServiceSettingsResponse {
        auto_rollback: stored.settings.auto_rollback,
        backup_targets: stored.settings.backup_targets,
        repo_url: stored.settings.repo_url,
        auto_update_policy: stored.auto_update_policy,
    }))
}

pub(in crate::api) async fn put_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<ServiceSettingsRequest>,
) -> Result<Json<PutServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let current_settings = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let (repo_url, repo_url_auto_disabled) = match req.repo_url {
        Some(repo_url) => {
            let repo_url = normalize_repo_url_input(repo_url.as_deref())?;
            let repo_url_auto_disabled = repo_url.is_none();
            (repo_url, repo_url_auto_disabled)
        }
        None => (
            current_settings.settings.repo_url.clone(),
            current_settings.repo_url_auto_disabled,
        ),
    };
    let auto_update_policy = req
        .auto_update_policy
        .clone()
        .unwrap_or(current_settings.auto_update_policy.clone());
    crate::auto_update::validate_policy_for_scope(&auto_update_policy, "service")?;
    let updated = state
        .db
        .put_service_protection_settings_with_repo_auto_disabled(
            &service_id,
            req.auto_rollback,
            repo_url.as_deref(),
            repo_url_auto_disabled,
            &now,
        )
        .await
        .map_err(map_internal)?;
    if !updated {
        return Err(ApiError::not_found("service not found"));
    }
    state
        .db
        .put_auto_update_policy("service", &service_id, &auto_update_policy, &now)
        .await
        .map_err(map_internal)?;
    state
        .management_events
        .publish_change(
            "services",
            "service",
            service_id,
            serde_json::json!({ "operation": "settings_updated" }),
        )
        .await;
    Ok(Json(PutServiceSettingsResponse { ok: true }))
}
