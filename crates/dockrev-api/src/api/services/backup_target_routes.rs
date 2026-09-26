use super::*;

pub(crate) async fn get_service_backup_targets(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceBackupTargetsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    Ok(Json(
        load_service_backup_targets_response(&state, &service_id).await?,
    ))
}

pub(crate) async fn put_service_backup_targets(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<PutServiceBackupTargetsRequest>,
) -> Result<Json<PutServiceBackupTargetsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    Ok(Json(
        save_service_backup_targets_response(&state, &service_id, req).await?,
    ))
}
