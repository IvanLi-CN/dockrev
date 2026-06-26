use super::*;

fn to_api_github_packages_webhook_delivery(
    delivery: GitHubPackagesWebhookDeliveryDb,
) -> GitHubPackagesWebhookDelivery {
    let full_name = match (&delivery.owner, &delivery.repo) {
        (Some(owner), Some(repo)) => Some(format!("{owner}/{repo}")),
        _ => None,
    };

    GitHubPackagesWebhookDelivery {
        delivery_id: delivery.delivery_id,
        received_at: delivery.received_at,
        first_received_at: delivery.first_received_at,
        owner: delivery.owner,
        repo: delivery.repo,
        full_name,
        event: delivery.event,
        action: delivery.action,
        decision: delivery.decision,
        reason: delivery.reason,
        response_status: delivery.response_status,
        job_id: delivery.job_id,
        job_ids: delivery.job_ids,
        attempt_count: delivery.attempt_count,
    }
}

fn to_github_packages_webhook_delivery_event_value(
    delivery: GitHubPackagesWebhookDeliveryDb,
) -> serde_json::Value {
    let delivery = to_api_github_packages_webhook_delivery(delivery);
    serde_json::to_value(GitHubPackagesWebhookDeliveryEvent {
        event_type: "github_packages_delivery_event".to_string(),
        delivery,
    })
    .unwrap_or_else(|_| json!({ "type": "github_packages_delivery_event" }))
}

fn resolve_github_packages_delivery_events_after_id(
    headers: &HeaderMap,
    query_after_id: i64,
) -> i64 {
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    let header_after_id = last_event_id.parse::<i64>().unwrap_or(0);
    std::cmp::max(header_after_id, query_after_id).max(0)
}

pub(super) async fn emit_github_packages_delivery_event(state: &Arc<AppState>, delivery_id: &str) {
    let delivery = match state.db.get_github_packages_delivery(delivery_id).await {
        Ok(Some(delivery)) => delivery,
        Ok(None) => return,
        Err(err) => {
            tracing::warn!(delivery_id = %delivery_id, error = %err, "load github packages delivery for sse failed");
            return;
        }
    };

    let payload_json = match serde_json::to_string(
        &to_github_packages_webhook_delivery_event_value(delivery.clone()),
    ) {
        Ok(payload_json) => payload_json,
        Err(err) => {
            tracing::warn!(delivery_id = %delivery_id, error = %err, "serialize github packages delivery sse payload failed");
            return;
        }
    };

    if let Err(err) = state
        .db
        .insert_github_packages_delivery_event(delivery_id, &delivery.received_at, &payload_json)
        .await
    {
        tracing::warn!(delivery_id = %delivery_id, error = %err, "persist github packages delivery sse event failed");
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitHubPackagesWebhookDeliveryEventsQuery {
    #[serde(default)]
    after_id: i64,
}

pub(crate) async fn github_packages_webhook_delivery_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<GitHubPackagesWebhookDeliveryEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let mut after_id = resolve_github_packages_delivery_events_after_id(&headers, q.after_id);

    if after_id <= 0 {
        after_id = state
            .db
            .get_github_packages_delivery_events_last_id()
            .await
            .map_err(map_internal)?;
    }

    let sse_state = state.clone();
    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().comment("keep-alive"));
        loop {
            let rows = match sse_state
                .db
                .list_github_packages_delivery_events_since(after_id, 200)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    let evt = GitHubPackagesWebhookDeliveryEventsErrorPayload {
                        event_type: "github_packages_delivery_events_error".to_string(),
                        error: e.to_string(),
                    };
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .event("github_packages_delivery_events_error")
                            .data(serde_json::to_string(&evt).unwrap_or_else(|_| json!({
                                "type": "github_packages_delivery_events_error",
                                "error": "serialize_error",
                            }).to_string())),
                    );
                    break;
                }
            };

            if rows.is_empty() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            for row in rows {
                after_id = row.id;
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(row.id.to_string())
                        .event("github_packages_delivery_event")
                        .data(row.payload_json),
                );
            }
        }
    };

    let sse = Sse::new(stream).keep_alive(edge_proxy_safe_keepalive());

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp_headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );

    Ok((resp_headers, sse))
}

pub(crate) async fn list_github_packages_webhook_deliveries(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ListGitHubPackagesWebhookDeliveriesQuery>,
) -> Result<Json<ListGitHubPackagesWebhookDeliveriesResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let decision = parse_delivery_decision_filter(q.decision.as_deref())?;
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let total = state
        .db
        .count_github_packages_deliveries_total()
        .await
        .map_err(map_internal)?;
    let summary = state
        .db
        .summarize_github_packages_deliveries()
        .await
        .map_err(map_internal)?;
    let filtered_total = state
        .db
        .count_github_packages_deliveries_filtered(decision, search)
        .await
        .map_err(map_internal)?;
    let offset = (page - 1).saturating_mul(per_page);
    let deliveries = state
        .db
        .list_github_packages_deliveries_page(decision, search, per_page, offset)
        .await
        .map_err(map_internal)?;

    Ok(Json(ListGitHubPackagesWebhookDeliveriesResponse {
        page,
        per_page,
        total,
        filtered_total,
        summary,
        deliveries: deliveries
            .into_iter()
            .map(to_api_github_packages_webhook_delivery)
            .collect(),
    }))
}
