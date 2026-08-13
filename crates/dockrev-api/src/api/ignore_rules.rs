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
    publish_ignore_management_event(&state, &rule.scope.service_id, &rule_id, "ignore_created")
        .await;

    Ok((StatusCode::CREATED, Json(CreateIgnoreResponse { rule_id })))
}

pub(super) async fn delete_ignore(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DeleteIgnoreRequest>,
) -> Result<Json<DeleteIgnoreResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let deleted_service_id = state
        .db
        .delete_ignore_rule(&req.rule_id)
        .await
        .map_err(map_internal)?;
    if let Some(service_id) = deleted_service_id.as_ref() {
        publish_ignore_management_event(&state, service_id, &req.rule_id, "ignore_deleted").await;
    }

    Ok(Json(DeleteIgnoreResponse {
        deleted: deleted_service_id.is_some(),
    }))
}

async fn publish_ignore_management_event(
    state: &Arc<AppState>,
    service_id: &str,
    rule_id: &str,
    operation: &str,
) {
    let stack_id = state
        .db
        .get_service_stack_id(service_id)
        .await
        .ok()
        .flatten();
    let mut entities = vec![crate::management_events::ManagementEventEntity {
        entity_type: "service".to_string(),
        id: service_id.to_string(),
    }];
    if let Some(stack_id) = stack_id.as_ref() {
        entities.push(crate::management_events::ManagementEventEntity {
            entity_type: "stack".to_string(),
            id: stack_id.clone(),
        });
    }
    state
        .management_events
        .publish_immediate(
            "services",
            entities,
            json!({
                "operation": operation,
                "ruleId": rule_id,
                "serviceId": service_id,
                "stackId": stack_id,
            }),
        )
        .await;
}
