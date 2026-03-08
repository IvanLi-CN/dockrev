use super::*;

pub(super) async fn list_ignores(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ListIgnoresResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let rules = state.db.list_ignore_rules().await.map_err(map_internal)?;
    Ok(Json(ListIgnoresResponse { rules }))
}

pub(super) async fn create_ignore(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateIgnoreRequest>,
) -> Result<(StatusCode, Json<CreateIgnoreResponse>), ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    if req.scope.kind != "service" {
        return Err(ApiError::invalid_argument("scope.type must be 'service'"));
    }
    if req.scope.service_id.is_empty() {
        return Err(ApiError::invalid_argument(
            "scope.serviceId must not be empty",
        ));
    }

    let rule_id = ids::new_ignore_id();
    let rule = IgnoreRule {
        id: rule_id.clone(),
        enabled: req.enabled,
        scope: IgnoreRuleScope {
            kind: req.scope.kind,
            service_id: req.scope.service_id,
        },
        matcher: IgnoreRuleMatch {
            kind: req.matcher.kind,
            value: req.matcher.value,
        },
        note: req.note,
    };
    state
        .db
        .insert_ignore_rule(&rule, &now)
        .await
        .map_err(map_internal)?;

    Ok((StatusCode::CREATED, Json(CreateIgnoreResponse { rule_id })))
}

pub(super) async fn delete_ignore(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DeleteIgnoreRequest>,
) -> Result<Json<DeleteIgnoreResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let deleted = state
        .db
        .delete_ignore_rule(&req.rule_id)
        .await
        .map_err(map_internal)?;

    Ok(Json(DeleteIgnoreResponse { deleted }))
}
