use std::{borrow::Cow, time::Duration};

use anyhow::Context as _;
use lettre::{
    AsyncSmtpTransport, AsyncTransport as _, Message, Tokio1Executor,
    message::{Mailbox, MultiPart, SinglePart, header::ContentType},
};
use serde::Serialize;
use serde_json::{Value, json};
use url::Url;
use web_push::{
    ContentEncoding, HyperWebPushClient, SubscriptionInfo, Urgency, VapidSignatureBuilder,
    WebPushClient as _, WebPushError, WebPushMessageBuilder,
};

use crate::{
    api::types::{
        JobLogLine, NotificationSettings, NotificationTestChannel,
        ServiceDigestTagsSnapshotResponse,
    },
    state::AppState,
};

const MAX_TEST_SUMMARY_CHARS: usize = 512;
const MAX_TEST_DEBUG_RAW_MESSAGE_CHARS: usize = 1024;
const TELEGRAM_MAX_MESSAGE_CHARS: usize = 4096;
const MAX_JOB_SERVICE_URLS: usize = 10;
const MAX_JOB_ERROR_CHARS: usize = 1024;
const MAX_NEW_VERSION_SERVICE_URLS: usize = 10;
const MAX_GHCR_REPOS: usize = 10;
const MAX_GHCR_REPO_ERROR_CHARS: usize = 256;
const NEW_VERSION_NOTIFY_SETTLE_TIMEOUT: Duration = Duration::from_secs(3);
const NEW_VERSION_NOTIFY_SETTLE_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub async fn notify_job_updated(
    state: &AppState,
    job_id: &str,
    status: &str,
    now_rfc3339: &str,
    summary: &Value,
) -> anyhow::Result<()> {
    let payload = json!({
        "jobId": job_id,
        "status": status,
        "ts": now_rfc3339,
        "summary": summary,
    });
    send_all(
        state,
        Some(job_id),
        now_rfc3339,
        Some(&payload),
        NotifySendMode::Default,
    )
    .await?;
    Ok(())
}

pub async fn notify_new_versions_discovered(
    state: &AppState,
    check_job_id: &str,
    reason: &str,
    now_rfc3339: &str,
    services_checked: u32,
    discovered_services: &[NewVersionDiscoveredService],
) -> anyhow::Result<()> {
    if discovered_services.is_empty() {
        return Ok(());
    }

    let settings = state.db.get_notification_settings().await?;
    if !is_event_enabled(&settings, NotificationEventKind::NewVersionDiscovered)
        || !has_enabled_delivery_channel(&settings)
    {
        return Ok(());
    }

    let discovered_total = discovered_services.len();
    let discovered_services =
        revalidate_new_version_discovered_services(state, discovered_services).await?;
    if discovered_services.is_empty() {
        log_new_version_notification_skip(
            state,
            check_job_id,
            now_rfc3339,
            format!(
                "new-version notification skipped: all {} services no longer have matching active candidates",
                discovered_total
            ),
        )
        .await;
        return Ok(());
    }

    let discovered_services =
        settle_new_version_discovered_services(state, &discovered_services).await?;

    let mut reserved = Vec::<ReservedNewVersionNotification>::new();
    for item in &discovered_services {
        let pending = crate::db::NewVersionNotificationPending {
            id: crate::ids::new_notification_id(),
            service_id: item.service_id.clone(),
            job_id: check_job_id.to_string(),
            reason: reason.to_string(),
            image_ref: item.image_ref.clone(),
            image_tag: item.current_tag.clone(),
            current_tag: item.current_tag.clone(),
            current_display_tag: item.current_display_tag.clone(),
            candidate_tag: item.candidate_tag.clone(),
            candidate_display_tag: item.candidate_display_tag.clone(),
            candidate_digest: item.candidate_digest.clone(),
            created_at: now_rfc3339.to_string(),
        };
        match state.db.reserve_new_version_notification(&pending).await? {
            crate::db::NewVersionNotificationReserveResult::Reserved(record_id) => {
                reserved.push(ReservedNewVersionNotification {
                    record_id,
                    service: item.clone(),
                });
            }
            crate::db::NewVersionNotificationReserveResult::SkippedDuplicate => {}
        }
    }

    if reserved.is_empty() {
        log_new_version_notification_skip(
            state,
            check_job_id,
            now_rfc3339,
            format!(
                "new-version notification skipped: all {} services already have active records",
                discovered_services.len()
            ),
        )
        .await;
        return Ok(());
    }

    let revalidated_reserved_services = reserved
        .iter()
        .map(|item| item.service.clone())
        .collect::<Vec<_>>();
    let active_reserved_ids =
        revalidate_new_version_discovered_services(state, &revalidated_reserved_services)
            .await?
            .into_iter()
            .map(|service| service.service_id)
            .collect::<std::collections::HashSet<_>>();

    let mut sendable_reserved = Vec::<ReservedNewVersionNotification>::new();
    for item in reserved {
        if active_reserved_ids.contains(&item.service.service_id) {
            sendable_reserved.push(item);
        } else {
            state
                .db
                .finalize_new_version_notification(&item.record_id, &[], None, now_rfc3339)
                .await?;
        }
    }

    if sendable_reserved.is_empty() {
        log_new_version_notification_skip(
            state,
            check_job_id,
            now_rfc3339,
            format!(
                "new-version notification skipped: all {} services no longer have matching active candidates",
                discovered_total
            ),
        )
        .await;
        return Ok(());
    }

    let reserved_services = sendable_reserved
        .iter()
        .map(|item| item.service.clone())
        .collect::<Vec<_>>();
    let send_result = send_new_versions(
        state,
        check_job_id,
        now_rfc3339,
        services_checked,
        &reserved_services,
    )
    .await;

    let results = match send_result {
        Ok(results) => results,
        Err(err) => {
            let err_text = err.to_string();
            for item in &sendable_reserved {
                let _ = state
                    .db
                    .finalize_new_version_notification(
                        &item.record_id,
                        &[],
                        Some(err_text.as_str()),
                        now_rfc3339,
                    )
                    .await;
            }
            return Err(err);
        }
    };

    let sent_channels = successful_delivery_channels(&results);
    let last_error = failed_delivery_error(&results);
    for item in &sendable_reserved {
        let _ = state
            .db
            .finalize_new_version_notification(
                &item.record_id,
                &sent_channels,
                last_error.as_deref(),
                now_rfc3339,
            )
            .await?;
    }
    Ok(())
}

pub async fn notify_ghcr_webhook_anomaly(
    state: &AppState,
    now_rfc3339: &str,
    event: GhcrWebhookAnomalyEvent<'_>,
) -> anyhow::Result<()> {
    if event.counts.total() == 0 {
        return Ok(());
    }
    send_ghcr_webhook_anomaly(state, now_rfc3339, event).await?;
    Ok(())
}

pub async fn send_test(
    state: &AppState,
    now_rfc3339: &str,
    message: &str,
    channel: Option<NotificationTestChannel>,
) -> anyhow::Result<Value> {
    let results = send_all(
        state,
        None,
        now_rfc3339,
        None,
        NotifySendMode::Test {
            channel,
            message: message.to_string(),
        },
    )
    .await?;
    Ok(results)
}

#[derive(Clone, Debug)]
enum NotifySendMode {
    Default,
    Test {
        channel: Option<NotificationTestChannel>,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotificationEventKind {
    Update,
    NewVersionDiscovered,
    GhcrWebhookAnomaly,
}

#[derive(Clone, Debug)]
pub struct NewVersionDiscoveredService {
    pub stack_id: String,
    pub service_id: String,
    pub image_ref: String,
    pub current_tag: String,
    pub current_display_tag: String,
    pub candidate_tag: String,
    pub candidate_display_tag: String,
    pub candidate_digest: String,
}

pub fn extract_new_versions_discovered(summary: &Value) -> Vec<NewVersionDiscoveredService> {
    let Some(items) = summary
        .get("newVersions")
        .and_then(|v| v.get("services"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        let Some(stack_id) = item.get("stackId").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(service_id) = item.get("serviceId").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(image_ref) = item.get("imageRef").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(current_tag) = item.get("currentTag").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(candidate_tag) = item.get("candidateTag").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(candidate_digest) = item.get("candidateDigest").and_then(|v| v.as_str()) else {
            continue;
        };
        let current_display_tag = item
            .get("currentDisplayTag")
            .and_then(|v| v.as_str())
            .unwrap_or(current_tag);
        let candidate_display_tag = item
            .get("candidateDisplayTag")
            .and_then(|v| v.as_str())
            .unwrap_or(candidate_tag);
        out.push(NewVersionDiscoveredService {
            stack_id: stack_id.to_string(),
            service_id: service_id.to_string(),
            image_ref: image_ref.to_string(),
            current_tag: current_tag.to_string(),
            current_display_tag: current_display_tag.to_string(),
            candidate_tag: candidate_tag.to_string(),
            candidate_display_tag: candidate_display_tag.to_string(),
            candidate_digest: candidate_digest.to_string(),
        });
    }
    out
}

#[derive(Clone, Debug)]
pub struct GhcrWebhookAnomalyRepo {
    pub owner: String,
    pub repo: String,
    pub state: String,
    pub last_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GhcrWebhookAnomalyCounts {
    pub missing: u32,
    pub conflict: u32,
    pub error: u32,
}

impl GhcrWebhookAnomalyCounts {
    pub fn total(self) -> u32 {
        self.missing + self.conflict + self.error
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GhcrWebhookAnomalyEvent<'a> {
    pub job_id: &'a str,
    pub status: &'a str,
    pub counts: GhcrWebhookAnomalyCounts,
    pub repos: &'a [GhcrWebhookAnomalyRepo],
}

fn is_event_enabled(settings: &NotificationSettings, event: NotificationEventKind) -> bool {
    match event {
        NotificationEventKind::Update => settings.event_update_enabled,
        NotificationEventKind::NewVersionDiscovered => settings.event_new_version_enabled,
        NotificationEventKind::GhcrWebhookAnomaly => settings.event_ghcr_webhook_anomaly_enabled,
    }
}

fn should_send_channel(
    mode: &NotifySendMode,
    enabled: bool,
    channel: NotificationTestChannel,
) -> bool {
    match mode {
        NotifySendMode::Default => enabled,
        NotifySendMode::Test {
            channel: Some(target),
            ..
        } => *target == channel,
        NotifySendMode::Test { channel: None, .. } => enabled,
    }
}

#[derive(Clone, Debug)]
struct ReservedNewVersionNotification {
    record_id: String,
    service: NewVersionDiscoveredService,
}

fn new_version_candidate_matches_current_state(
    item: &NewVersionDiscoveredService,
    current: &crate::db::CurrentNewVersionNotificationTarget,
) -> bool {
    current.image_ref == item.image_ref
        && current.image_tag == item.current_tag
        && current.candidate_digest.as_deref() == Some(item.candidate_digest.as_str())
}

async fn revalidate_new_version_discovered_services(
    state: &AppState,
    discovered_services: &[NewVersionDiscoveredService],
) -> anyhow::Result<Vec<NewVersionDiscoveredService>> {
    let service_ids = discovered_services
        .iter()
        .map(|service| service.service_id.clone())
        .collect::<Vec<_>>();
    let current_targets = state
        .db
        .list_current_new_version_notification_targets(&service_ids)
        .await?;
    let current_by_service = current_targets
        .into_iter()
        .map(|target| (target.service_id.clone(), target))
        .collect::<std::collections::HashMap<_, _>>();

    Ok(discovered_services
        .iter()
        .filter(|service| {
            current_by_service
                .get(&service.service_id)
                .is_some_and(|current| {
                    new_version_candidate_matches_current_state(service, current)
                })
        })
        .cloned()
        .collect())
}

#[derive(Clone, Debug, Default)]
struct NewVersionNotificationSettleTarget {
    image_repo: Option<String>,
    current_digest: Option<String>,
    current_resolved_tag: Option<String>,
    candidate_resolved_tag: Option<String>,
}

async fn settle_new_version_discovered_services(
    state: &AppState,
    discovered_services: &[NewVersionDiscoveredService],
) -> anyhow::Result<Vec<NewVersionDiscoveredService>> {
    if discovered_services.is_empty() {
        return Ok(Vec::new());
    }

    let host_platform =
        crate::registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
    let settle_targets =
        load_new_version_notification_settle_targets(state, discovered_services).await?;
    let deadline = tokio::time::Instant::now() + NEW_VERSION_NOTIFY_SETTLE_TIMEOUT;

    loop {
        let (settled, pending) = settle_new_version_discovered_services_once(
            state,
            discovered_services,
            &settle_targets,
            &host_platform,
        )
        .await?;
        if !pending || tokio::time::Instant::now() >= deadline {
            return Ok(settled);
        }
        tokio::time::sleep(NEW_VERSION_NOTIFY_SETTLE_POLL_INTERVAL).await;
    }
}

async fn load_new_version_notification_settle_targets(
    state: &AppState,
    discovered_services: &[NewVersionDiscoveredService],
) -> anyhow::Result<std::collections::HashMap<String, NewVersionNotificationSettleTarget>> {
    let mut out = std::collections::HashMap::new();
    let mut stack_cache =
        std::collections::HashMap::<String, Option<crate::api::types::StackRecord>>::new();

    for item in discovered_services {
        if out.contains_key(&item.service_id) {
            continue;
        }

        let stack = if let Some(cached) = stack_cache.get(&item.stack_id) {
            cached.clone()
        } else {
            let loaded = state.db.get_stack(&item.stack_id).await?;
            stack_cache.insert(item.stack_id.clone(), loaded.clone());
            loaded
        };

        let image_repo = crate::snapshot_worker::image_repo_from_image_ref(&item.image_ref);
        let target = stack
            .as_ref()
            .and_then(|stack| stack.services.iter().find(|svc| svc.id == item.service_id))
            .map(|service| NewVersionNotificationSettleTarget {
                image_repo: image_repo.clone().or_else(|| {
                    crate::snapshot_worker::image_repo_from_image_ref(&service.image.reference)
                }),
                current_digest: service
                    .image
                    .digest
                    .as_deref()
                    .and_then(crate::snapshot_worker::normalize_digest),
                current_resolved_tag: service.image.resolved_tag.clone(),
                candidate_resolved_tag: service
                    .candidate
                    .as_ref()
                    .and_then(|candidate| candidate.resolved_tag.clone()),
            })
            .unwrap_or_else(|| NewVersionNotificationSettleTarget {
                image_repo,
                ..NewVersionNotificationSettleTarget::default()
            });
        out.insert(item.service_id.clone(), target);
    }

    Ok(out)
}

async fn settle_new_version_discovered_services_once(
    state: &AppState,
    discovered_services: &[NewVersionDiscoveredService],
    settle_targets: &std::collections::HashMap<String, NewVersionNotificationSettleTarget>,
    host_platform: &str,
) -> anyhow::Result<(Vec<NewVersionDiscoveredService>, bool)> {
    let mut pending = false;
    let mut settled = Vec::with_capacity(discovered_services.len());

    for item in discovered_services {
        let target = settle_targets.get(&item.service_id);
        let image_repo = target.and_then(|target| target.image_repo.as_deref());
        let (current_display_tag, current_pending) = settle_new_version_display_tag(
            state,
            image_repo,
            &item.current_tag,
            target.and_then(|target| target.current_resolved_tag.as_deref()),
            Some(item.current_display_tag.as_str()),
            target.and_then(|target| target.current_digest.as_deref()),
            host_platform,
        )
        .await?;
        let (candidate_display_tag, candidate_pending) = settle_new_version_display_tag(
            state,
            image_repo,
            &item.candidate_tag,
            target.and_then(|target| target.candidate_resolved_tag.as_deref()),
            Some(item.candidate_display_tag.as_str()),
            Some(item.candidate_digest.as_str()),
            host_platform,
        )
        .await?;
        pending |= current_pending || candidate_pending;
        settled.push(NewVersionDiscoveredService {
            current_display_tag,
            candidate_display_tag,
            ..item.clone()
        });
    }

    Ok((settled, pending))
}

async fn settle_new_version_display_tag(
    state: &AppState,
    image_repo: Option<&str>,
    raw_tag: &str,
    existing_resolved_tag: Option<&str>,
    existing_display_tag: Option<&str>,
    digest: Option<&str>,
    host_platform: &str,
) -> anyhow::Result<(String, bool)> {
    let raw_tag = raw_tag.trim();
    let digest = digest.map(str::trim).filter(|digest| !digest.is_empty());
    let image_repo = image_repo.map(str::trim).filter(|repo| !repo.is_empty());
    let needs_inference = !raw_tag.is_empty() && !crate::ignore::is_strict_semver(raw_tag);
    let stable_display =
        best_notification_display_tag(raw_tag, &[existing_resolved_tag, existing_display_tag]);

    let mut inferred = None;
    let mut in_flight_reason = None;
    if needs_inference && let (Some(image_repo), Some(digest)) = (image_repo, digest) {
        inferred = infer_notification_display_tag_from_snapshot(
            state,
            image_repo,
            digest,
            host_platform,
            raw_tag,
        )
        .await?;
        in_flight_reason = state
            .snapshot_worker
            .in_flight_reason(image_repo, digest, host_platform)
            .await;
    }

    let display = best_notification_display_tag(
        raw_tag,
        &[
            existing_resolved_tag,
            existing_display_tag,
            inferred.as_deref(),
        ],
    );
    let pending =
        matches!(in_flight_reason.as_deref(), Some("new_version")) && stable_display == raw_tag;
    Ok((display, pending))
}

async fn infer_notification_display_tag_from_snapshot(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
    raw_tag: &str,
) -> anyhow::Result<Option<String>> {
    let Some((snapshot_json, checked_at, _updated_at)) = state
        .db
        .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
        .await?
    else {
        return Ok(None);
    };

    let mut snapshot =
        match serde_json::from_str::<ServiceDigestTagsSnapshotResponse>(&snapshot_json) {
            Ok(snapshot) => snapshot,
            Err(_) => return Ok(None),
        };
    if snapshot.checked_at.trim().is_empty() {
        snapshot.checked_at = checked_at;
    }
    Ok(infer_notification_semver_tag_from_snapshot(
        &snapshot, raw_tag,
    ))
}

fn infer_notification_semver_tag_from_snapshot(
    snapshot: &ServiceDigestTagsSnapshotResponse,
    raw_tag: &str,
) -> Option<String> {
    let mut semver_tags = snapshot
        .tags
        .iter()
        .filter_map(|tag| crate::ignore::parse_version(tag).map(|version| (version, tag.clone())))
        .collect::<Vec<_>>();
    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let raw_tag = raw_tag.trim();
    semver_tags
        .into_iter()
        .map(|(_, tag)| tag)
        .find(|tag| tag != raw_tag)
}

fn best_notification_display_tag(raw_tag: &str, improved_candidates: &[Option<&str>]) -> String {
    let raw_tag = raw_tag.trim();
    for candidate in improved_candidates {
        if let Some(candidate) = candidate
            .map(str::trim)
            .filter(|candidate| !candidate.is_empty() && *candidate != raw_tag)
        {
            return candidate.to_string();
        }
    }
    raw_tag.to_string()
}

async fn log_new_version_notification_skip(
    state: &AppState,
    check_job_id: &str,
    now_rfc3339: &str,
    msg: String,
) {
    let _ = state
        .db
        .insert_job_log(
            check_job_id,
            &JobLogLine {
                ts: now_rfc3339.to_string(),
                level: "info".to_string(),
                msg,
            },
        )
        .await;
}

fn has_enabled_delivery_channel(settings: &NotificationSettings) -> bool {
    settings.webhook_enabled
        || settings.telegram_enabled
        || settings.email_enabled
        || settings.webpush_enabled
}

fn successful_delivery_channels(results: &Value) -> Vec<String> {
    let Some(map) = results.as_object() else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(channel, result)| {
            result
                .get("ok")
                .and_then(|value| value.as_bool())
                .filter(|ok| *ok)
                .map(|_| channel.clone())
        })
        .collect()
}

fn failed_delivery_error(results: &Value) -> Option<String> {
    let map = results.as_object()?;
    let failures = map
        .iter()
        .filter_map(|(channel, result)| {
            let ok = result
                .get("ok")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if ok {
                return None;
            }
            let error = result
                .get("error")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unknown delivery error");
            Some(format!("{channel}: {error}"))
        })
        .collect::<Vec<_>>();
    if failures.is_empty() {
        None
    } else {
        Some(failures.join("; "))
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobNotificationPayloadV2 {
    schema: &'static str,
    kind: &'static str,
    sent_at: String,
    channel: &'static str,
    job: JobNotificationJobV2,
    links: JobNotificationLinksV2,
    human: JobNotificationHumanV2,
    debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobNotificationJobV2 {
    id: String,
    #[serde(rename = "type")]
    r#type: String,
    scope: String,
    status: String,
    reason: String,
    created_by: String,
    created_at: String,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    finished_at: Option<String>,
    #[serde(default)]
    stack_id: Option<String>,
    #[serde(default)]
    service_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobNotificationLinksV2 {
    primary_url: String,
    job_url: String,
    service_urls: Vec<JobNotificationServiceUrlV2>,
    truncated: JobNotificationTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobNotificationServiceUrlV2 {
    stack_id: String,
    stack_name: String,
    service_id: String,
    service_name: String,
    url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobNotificationTruncatedV2 {
    service_urls_omitted: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobNotificationHumanV2 {
    title: String,
    summary: String,
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobNotificationDebugV2 {
    app_version: String,
    source: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewVersionNotificationPayloadV2 {
    schema: &'static str,
    kind: &'static str,
    sent_at: String,
    channel: &'static str,
    check: NewVersionNotificationCheckV2,
    links: NewVersionNotificationLinksV2,
    human: JobNotificationHumanV2,
    debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewVersionNotificationCheckV2 {
    job_id: String,
    status: String,
    scope: String,
    services_checked: u32,
    new_versions: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewVersionNotificationLinksV2 {
    primary_url: String,
    job_url: String,
    service_urls: Vec<NewVersionNotificationServiceUrlV2>,
    truncated: JobNotificationTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewVersionNotificationServiceUrlV2 {
    stack_id: String,
    stack_name: String,
    service_id: String,
    service_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_display_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_display_tag: Option<String>,
    url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GhcrWebhookAnomalyPayloadV2 {
    schema: &'static str,
    kind: &'static str,
    sent_at: String,
    channel: &'static str,
    job: GhcrWebhookAnomalyJobV2,
    links: GhcrWebhookAnomalyLinksV2,
    human: JobNotificationHumanV2,
    debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GhcrWebhookAnomalyJobV2 {
    id: String,
    status: String,
    missing: u32,
    conflict: u32,
    error: u32,
    total_anomalies: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GhcrWebhookAnomalyLinksV2 {
    primary_url: String,
    job_url: String,
    settings_url: String,
    repos: Vec<GhcrWebhookAnomalyRepoV2>,
    truncated: GhcrWebhookAnomalyTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GhcrWebhookAnomalyRepoV2 {
    owner: String,
    repo: String,
    full_name: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GhcrWebhookAnomalyTruncatedV2 {
    repos_omitted: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestNotificationPayloadV2 {
    schema: &'static str,
    kind: &'static str,
    sent_at: String,
    channel: &'static str,
    url: String,
    human: TestNotificationHuman,
    debug: TestNotificationDebug,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestNotificationHuman {
    title: String,
    summary: String,
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestNotificationDebug {
    requested_channel: Option<&'static str>,
    app_version: String,
    source: &'static str,
    raw_message: String,
}

fn notification_channel_key(channel: NotificationTestChannel) -> &'static str {
    match channel {
        NotificationTestChannel::Email => "email",
        NotificationTestChannel::Webhook => "webhook",
        NotificationTestChannel::Telegram => "telegram",
        NotificationTestChannel::WebPush => "webPush",
    }
}

fn notification_channel_label(channel: NotificationTestChannel) -> &'static str {
    match channel {
        NotificationTestChannel::Email => "Email",
        NotificationTestChannel::Webhook => "Webhook",
        NotificationTestChannel::Telegram => "Telegram",
        NotificationTestChannel::WebPush => "Web Push",
    }
}

fn is_absolute_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn best_effort_url(public_base_url: Option<&str>, path_no_leading_slash: &str) -> String {
    if let Some(base) = public_base_url
        && let Ok(base) = Url::parse(base)
        && let Ok(joined) = base.join(path_no_leading_slash)
    {
        return joined.to_string();
    }
    format!("/{path_no_leading_slash}")
}

fn update_job_status_label_zh(status: &str) -> Cow<'_, str> {
    match status {
        "success" => Cow::Borrowed("成功"),
        "failed" => Cow::Borrowed("失败"),
        "rolled_back" => Cow::Borrowed("已回滚"),
        _ => Cow::Borrowed(status),
    }
}

fn normalize_test_message(raw_message: &str) -> String {
    let trimmed = raw_message.trim();
    let normalized = if trimmed.is_empty() {
        "dockrev test"
    } else {
        trimmed
    };
    truncate_chars(normalized, MAX_TEST_SUMMARY_CHARS)
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut chars = input.chars();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else { return out };
        out.push(ch);
    }
    if chars.next().is_some() {
        out.push_str("... [truncated]");
    }
    out
}

struct NotificationTagDisplay<'a> {
    label: &'a str,
    readable: bool,
}

fn notification_tag_display<'a>(
    display_tag: Option<&'a str>,
    raw_tag: Option<&'a str>,
) -> Option<NotificationTagDisplay<'a>> {
    let display_tag = display_tag.map(str::trim).filter(|tag| !tag.is_empty());
    let raw_tag = raw_tag.map(str::trim).filter(|tag| !tag.is_empty());
    match (display_tag, raw_tag) {
        (Some(display_tag), Some(raw_tag)) if display_tag != raw_tag => {
            Some(NotificationTagDisplay {
                label: display_tag,
                readable: true,
            })
        }
        (_, Some(raw_tag)) if crate::ignore::parse_version(raw_tag).is_some() => {
            Some(NotificationTagDisplay {
                label: raw_tag,
                readable: true,
            })
        }
        (_, Some(raw_tag)) => Some(NotificationTagDisplay {
            label: raw_tag,
            readable: false,
        }),
        (Some(display_tag), None) => Some(NotificationTagDisplay {
            label: display_tag,
            readable: true,
        }),
        (None, None) => None,
    }
}

fn render_tag_transition(
    current_display_tag: Option<&str>,
    candidate_display_tag: Option<&str>,
    current_tag: Option<&str>,
    candidate_tag: Option<&str>,
) -> Option<String> {
    let current = notification_tag_display(current_display_tag, current_tag);
    let candidate = notification_tag_display(candidate_display_tag, candidate_tag);
    match (current, candidate) {
        (Some(current), Some(candidate)) if !current.readable && !candidate.readable => None,
        (Some(current), Some(candidate)) => {
            Some(format!("{} -> {}", current.label, candidate.label))
        }
        (None, Some(candidate)) if candidate.readable => Some(format!("-> {}", candidate.label)),
        _ => None,
    }
}

fn render_new_version_service_label(svc: &NewVersionNotificationServiceUrlV2) -> String {
    let mut label = format!("{} / {}", svc.stack_name, svc.service_name);
    if let Some(transition) = render_tag_transition(
        svc.current_display_tag.as_deref(),
        svc.candidate_display_tag.as_deref(),
        svc.current_tag.as_deref(),
        svc.candidate_tag.as_deref(),
    ) {
        label.push_str(&format!(" ({transition})"));
    }
    label
}

fn summarize_new_version_services(
    total_new_versions: usize,
    visible_services: &[NewVersionNotificationServiceUrlV2],
    omitted: u32,
) -> String {
    if total_new_versions == 0 {
        return "发现新版本服务数为 0。".to_string();
    }

    if total_new_versions == 1 {
        if let Some(svc) = visible_services.first() {
            if let Some(transition) = render_tag_transition(
                svc.current_display_tag.as_deref(),
                svc.candidate_display_tag.as_deref(),
                svc.current_tag.as_deref(),
                svc.candidate_tag.as_deref(),
            ) {
                return format!(
                    "{} / {} 服务有新版本（{transition}）。",
                    svc.stack_name, svc.service_name
                );
            }
            return format!("{} / {} 服务有新版本。", svc.stack_name, svc.service_name);
        }
        return "发现 1 个服务有新版本。".to_string();
    }

    let preview = visible_services
        .iter()
        .map(render_new_version_service_label)
        .collect::<Vec<_>>()
        .join("、");
    if preview.is_empty() {
        return format!("发现 {total_new_versions} 个服务有新版本。");
    }

    if omitted > 0 {
        return format!(
            "发现 {total_new_versions} 个服务有新版本：{preview}（通知正文仅展示前 {} 条）。",
            visible_services.len()
        );
    }

    format!("发现 {total_new_versions} 个服务有新版本：{preview}。")
}

fn summarize_ghcr_anomaly_repos(
    total_anomalies: u32,
    visible_repos: &[GhcrWebhookAnomalyRepoV2],
    omitted: u32,
) -> String {
    if visible_repos.is_empty() {
        return format!("巡检发现 {total_anomalies} 个异常仓库。");
    }

    let preview = visible_repos
        .iter()
        .map(|repo| format!("{} [{}]", repo.full_name, repo.state))
        .collect::<Vec<_>>()
        .join("、");

    if omitted > 0 {
        return format!(
            "巡检发现 {total_anomalies} 个异常仓库：{preview}（通知正文仅展示前 {} 条）。",
            visible_repos.len()
        );
    }

    format!("巡检发现 {total_anomalies} 个异常仓库：{preview}。")
}

fn summarize_updated_services(
    visible_services: &[JobNotificationServiceUrlV2],
    omitted: u32,
) -> String {
    let total_changed = visible_services.len() + omitted as usize;
    if total_changed == 0 {
        return "变更 0 个服务。".to_string();
    }

    if total_changed == 1 {
        if let Some(svc) = visible_services.first() {
            return format!(
                "变更 1 个服务（{} / {}）。",
                svc.stack_name, svc.service_name
            );
        }
        return "变更 1 个服务。".to_string();
    }

    let preview = visible_services
        .iter()
        .map(|svc| format!("{} / {}", svc.stack_name, svc.service_name))
        .collect::<Vec<_>>()
        .join("、");

    if preview.is_empty() {
        return format!("变更 {total_changed} 个服务。");
    }

    if omitted > 0 {
        return format!(
            "变更 {total_changed} 个服务：{preview}（通知正文仅展示前 {} 条）。",
            visible_services.len()
        );
    }

    format!("变更 {total_changed} 个服务：{preview}。")
}

fn extract_changed_service_ids(update: &Value) -> Vec<String> {
    let obj = update
        .get("newDigests")
        .and_then(|v| v.as_object())
        .or_else(|| update.get("oldDigests").and_then(|v| v.as_object()));
    match obj {
        Some(map) => map.keys().cloned().collect(),
        None => Vec::new(),
    }
}

fn extract_changed_services_by_stack(summary: &Value) -> Vec<(String, String)> {
    let Some(stacks) = summary.get("stacks").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut out: Vec<(String, String)> = Vec::new();
    for s in stacks {
        let Some(stack_id) = s.get("stackId").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(update) = s.get("update") else {
            continue;
        };
        for service_id in extract_changed_service_ids(update) {
            out.push((stack_id.to_string(), service_id));
        }
    }
    out
}

fn extract_error_excerpt(summary: &Value) -> Option<String> {
    if let Some(err) = summary.get("error").and_then(|v| v.as_str()) {
        let trimmed = err.trim();
        if !trimmed.is_empty() {
            return Some(truncate_chars(trimmed, MAX_JOB_ERROR_CHARS));
        }
    }

    let stacks = summary.get("stacks").and_then(|v| v.as_array())?;
    for s in stacks {
        let Some(update) = s.get("update") else {
            continue;
        };
        if let Some(err) = update.get("lastError").and_then(|v| v.as_str()) {
            let trimmed = err.trim();
            if !trimmed.is_empty() {
                return Some(truncate_chars(trimmed, MAX_JOB_ERROR_CHARS));
            }
        }
        if let Some(err) = update.get("error").and_then(|v| v.as_str()) {
            let trimmed = err.trim();
            if !trimmed.is_empty() {
                return Some(truncate_chars(trimmed, MAX_JOB_ERROR_CHARS));
            }
        }
    }
    None
}

fn build_test_payload_v2(
    now_rfc3339: &str,
    raw_message: &str,
    requested_channel: Option<NotificationTestChannel>,
    channel: NotificationTestChannel,
    app_version: &str,
    url: &str,
) -> TestNotificationPayloadV2 {
    let channel_label = notification_channel_label(channel);
    let summary = normalize_test_message(raw_message);
    TestNotificationPayloadV2 {
        schema: "dockrev.notification.test.v2",
        kind: "notification_test",
        sent_at: now_rfc3339.to_string(),
        channel: notification_channel_key(channel),
        url: url.to_string(),
        human: TestNotificationHuman {
            title: format!("Dockrev test notification ({channel_label})"),
            summary,
            detail: format!(
                "This is a test notification for {channel_label}. Sent at {now_rfc3339}."
            ),
        },
        debug: TestNotificationDebug {
            requested_channel: requested_channel.map(notification_channel_key),
            app_version: app_version.to_string(),
            source: "dockrev-api",
            raw_message: truncate_chars(raw_message, MAX_TEST_DEBUG_RAW_MESSAGE_CHARS),
        },
    }
}

fn to_value(payload: &TestNotificationPayloadV2) -> anyhow::Result<Value> {
    serde_json::to_value(payload).context("serialize test notification payload v2")
}

fn render_debug_json(payload: &TestNotificationPayloadV2) -> anyhow::Result<String> {
    serde_json::to_string_pretty(&payload.debug).context("serialize debug payload")
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn render_telegram_test_html(payload: &TestNotificationPayloadV2) -> anyhow::Result<String> {
    let debug = render_debug_json(payload)?;
    Ok(format!(
        "<b>{}</b>\n{}\n{}\n\n<b>Debug</b>\n<pre>{}</pre>",
        escape_html(&payload.human.title),
        escape_html(&payload.human.summary),
        escape_html(&payload.human.detail),
        escape_html(&debug)
    ))
}

fn render_telegram_test_plain(payload: &TestNotificationPayloadV2) -> anyhow::Result<String> {
    let debug = render_debug_json(payload)?;
    Ok(format!(
        "{}\n{}\n{}\n\nDebug\n{}",
        payload.human.title, payload.human.summary, payload.human.detail, debug
    ))
}

fn render_email_test_plain(payload: &TestNotificationPayloadV2) -> anyhow::Result<String> {
    let debug = render_debug_json(payload)?;
    Ok(format!(
        "{}\n\n{}\n\n{}\n\nDebug JSON\n```json\n{}\n```",
        payload.human.title, payload.human.summary, payload.human.detail, debug
    ))
}

fn render_email_test_html(payload: &TestNotificationPayloadV2) -> anyhow::Result<String> {
    let debug = render_debug_json(payload)?;
    Ok(format!(
        "<h2>{}</h2><p>{}</p><p>{}</p><h3>Debug JSON</h3><pre><code>{}</code></pre>",
        escape_html(&payload.human.title),
        escape_html(&payload.human.summary),
        escape_html(&payload.human.detail),
        escape_html(&debug)
    ))
}

fn render_web_push_body(payload: &TestNotificationPayloadV2) -> String {
    format!(
        "{}\n{}\nrequestedChannel: {}\nappVersion: {}",
        payload.human.summary,
        payload.human.detail,
        payload.debug.requested_channel.unwrap_or("all"),
        payload.debug.app_version
    )
}

fn to_web_push_value(payload: &TestNotificationPayloadV2) -> anyhow::Result<Value> {
    let mut value = to_value(payload)?;
    if let Value::Object(map) = &mut value {
        map.insert(
            "title".to_string(),
            Value::String(payload.human.title.clone()),
        );
        map.insert(
            "body".to_string(),
            Value::String(render_web_push_body(payload)),
        );
        map.insert("url".to_string(), Value::String(payload.url.clone()));
    }
    Ok(value)
}

fn should_retry_telegram_plain_text(status: reqwest::StatusCode, body: &str) -> bool {
    if status != reqwest::StatusCode::BAD_REQUEST {
        return false;
    }
    let body = body.to_ascii_lowercase();
    body.contains("parse entities")
        || body.contains("can't parse entities")
        || body.contains("parse_mode")
}

fn render_telegram_plain_for_send(payload: &TestNotificationPayloadV2) -> anyhow::Result<String> {
    let plain = render_telegram_test_plain(payload)?;
    Ok(truncate_chars(
        &plain,
        TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32),
    ))
}

fn to_job_value(payload: &JobNotificationPayloadV2) -> anyhow::Result<Value> {
    serde_json::to_value(payload).context("serialize job notification payload v2")
}

fn to_new_version_value(payload: &NewVersionNotificationPayloadV2) -> anyhow::Result<Value> {
    serde_json::to_value(payload).context("serialize new version notification payload v2")
}

fn to_ghcr_webhook_anomaly_value(payload: &GhcrWebhookAnomalyPayloadV2) -> anyhow::Result<Value> {
    serde_json::to_value(payload).context("serialize ghcr webhook anomaly payload v2")
}

fn to_web_push_job_value(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> anyhow::Result<Value> {
    let mut value = to_job_value(payload)?;
    if let Value::Object(map) = &mut value {
        map.insert(
            "title".to_string(),
            Value::String(payload.human.title.clone()),
        );

        let mut body = format!("{}\n点击通知查看详情", payload.human.summary);
        if let Some(err) = error_excerpt {
            body.push_str("\n错误：");
            body.push_str(err);
        }
        map.insert("body".to_string(), Value::String(body));
        map.insert(
            "url".to_string(),
            Value::String(payload.links.primary_url.clone()),
        );
    }
    Ok(value)
}

fn to_web_push_new_version_value(
    payload: &NewVersionNotificationPayloadV2,
) -> anyhow::Result<Value> {
    let mut value = to_new_version_value(payload)?;
    if let Value::Object(map) = &mut value {
        map.insert(
            "title".to_string(),
            Value::String(payload.human.title.clone()),
        );
        map.insert(
            "body".to_string(),
            Value::String(payload.human.summary.clone()),
        );
        map.insert(
            "url".to_string(),
            Value::String(payload.links.primary_url.clone()),
        );
    }
    Ok(value)
}

fn to_web_push_ghcr_webhook_anomaly_value(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> anyhow::Result<Value> {
    let mut value = to_ghcr_webhook_anomaly_value(payload)?;
    if let Value::Object(map) = &mut value {
        map.insert(
            "title".to_string(),
            Value::String(payload.human.title.clone()),
        );
        map.insert(
            "body".to_string(),
            Value::String(format!("{}\n点击通知查看详情", payload.human.summary)),
        );
        map.insert(
            "url".to_string(),
            Value::String(payload.links.primary_url.clone()),
        );
    }
    Ok(value)
}

fn render_open_link_html(url: &str, label: &str) -> String {
    if is_absolute_http_url(url) {
        format!(
            "<a href=\"{}\">{}</a>",
            escape_html(url),
            escape_html(label)
        )
    } else {
        // Telegram cannot resolve relative links. Show the path so operators can copy it.
        format!("<code>{}</code>", escape_html(url))
    }
}

fn render_telegram_job_html(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "<b>{}</b> {}",
        escape_html(&payload.human.title),
        render_open_link_html(&payload.links.primary_url, "详情")
    ));
    lines.push(escape_html(&payload.human.summary));

    if !is_absolute_http_url(&payload.links.primary_url) {
        lines.push("提示：未配置实例 Public Base URL（系统设置），以下为站内路径。".to_string());
    }

    if !payload.links.service_urls.is_empty() {
        lines.push(String::new());
        lines.push("<b>服务清单</b>".to_string());
        for svc in &payload.links.service_urls {
            lines.push(format!(
                "- {} / {}：{}",
                escape_html(&svc.stack_name),
                escape_html(&svc.service_name),
                render_open_link_html(&svc.url, "服务详情"),
            ));
        }
        if payload.links.truncated.service_urls_omitted > 0 {
            lines.push(format!(
                "... 以及其他 {} 个服务（已省略）",
                payload.links.truncated.service_urls_omitted
            ));
        }
    }

    if let Some(err) = error_excerpt {
        lines.push(String::new());
        lines.push("<b>错误</b>".to_string());
        lines.push(format!("<pre>{}</pre>", escape_html(err)));
    }

    lines.join("\n")
}

fn render_telegram_job_plain(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "{} 详情：{}",
        payload.human.title, payload.links.primary_url
    ));
    lines.push(payload.human.summary.clone());

    if !is_absolute_http_url(&payload.links.primary_url) {
        lines.push("提示：未配置实例 Public Base URL（系统设置），以下为站内路径。".to_string());
    }

    if !payload.links.service_urls.is_empty() {
        lines.push(String::new());
        lines.push("服务清单".to_string());
        for svc in &payload.links.service_urls {
            lines.push(format!(
                "- {} / {}: {}",
                svc.stack_name, svc.service_name, svc.url
            ));
        }
        if payload.links.truncated.service_urls_omitted > 0 {
            lines.push(format!(
                "... 以及其他 {} 个服务（已省略）",
                payload.links.truncated.service_urls_omitted
            ));
        }
    }

    if let Some(err) = error_excerpt {
        lines.push(String::new());
        lines.push("错误".to_string());
        lines.push(err.to_string());
    }

    lines.join("\n")
}

fn render_telegram_job_plain_for_send(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> String {
    let plain = render_telegram_job_plain(payload, error_excerpt);
    truncate_chars(&plain, TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32))
}

fn render_email_job_plain(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> String {
    render_telegram_job_plain(payload, error_excerpt)
}

fn render_email_job_html(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> String {
    let title = escape_html(&payload.human.title);
    let summary = escape_html(&payload.human.summary);

    let mut items = String::new();
    if !payload.links.service_urls.is_empty() {
        items.push_str("<ul>");
        for svc in &payload.links.service_urls {
            let label = format!("{} / {}", svc.stack_name, svc.service_name);
            let label = escape_html(&label);
            if is_absolute_http_url(&svc.url) {
                items.push_str(&format!(
                    "<li>{label}: <a href=\"{}\">服务详情</a></li>",
                    escape_html(&svc.url)
                ));
            } else {
                items.push_str(&format!(
                    "<li>{label}: <code>{}</code></li>",
                    escape_html(&svc.url)
                ));
            }
        }
        if payload.links.truncated.service_urls_omitted > 0 {
            items.push_str(&format!(
                "<li>... 以及其他 {} 个服务（已省略）</li>",
                payload.links.truncated.service_urls_omitted
            ));
        }
        items.push_str("</ul>");
    }

    let job_link = if is_absolute_http_url(&payload.links.job_url) {
        format!(
            "<a href=\"{}\">{}</a>",
            escape_html(&payload.links.job_url),
            "查看任务详情"
        )
    } else {
        format!("<code>{}</code>", escape_html(&payload.links.job_url))
    };

    let open_primary = if is_absolute_http_url(&payload.links.primary_url) {
        format!(
            "<a href=\"{}\">{}</a>",
            escape_html(&payload.links.primary_url),
            escape_html(&payload.links.primary_url)
        )
    } else {
        format!("<code>{}</code>", escape_html(&payload.links.primary_url))
    };

    let mut note = String::new();
    if !is_absolute_http_url(&payload.links.job_url) {
        note = "<p><em>提示：未配置实例 Public Base URL（系统设置），以下链接可能仅为站内路径。</em></p>".to_string();
    }

    let mut err_block = String::new();
    if let Some(err) = error_excerpt {
        err_block = format!("<h3>错误</h3><pre><code>{}</code></pre>", escape_html(err));
    }

    format!(
        "<h2>{title}</h2><p>{summary}</p>{note}<p>任务详情：{job_link}</p><p>打开：{open_primary}</p>{items}{err_block}",
    )
}

async fn send_telegram_job(
    client: &reqwest::Client,
    bot_token: Option<&str>,
    chat_id: Option<&str>,
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> anyhow::Result<()> {
    let token = bot_token.context("telegram.botToken missing")?;
    let chat_id = chat_id.context("telegram.chatId missing")?;
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");

    let html_text = render_telegram_job_html(payload, error_excerpt);
    if html_text.chars().count() > TELEGRAM_MAX_MESSAGE_CHARS {
        let plain_text = render_telegram_job_plain_for_send(payload, error_excerpt);
        let retry = client
            .post(&url)
            .json(&json!({ "chat_id": chat_id, "text": plain_text }))
            .send()
            .await?;
        if retry.status().is_success() {
            return Ok(());
        }
        let retry_status = retry.status();
        let retry_body = retry.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "telegram http {}: {}",
            retry_status,
            retry_body
        ));
    }

    let resp = client
        .post(&url)
        .json(&json!({ "chat_id": chat_id, "text": html_text, "parse_mode": "HTML" }))
        .send()
        .await?;
    if resp.status().is_success() {
        return Ok(());
    }

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if should_retry_telegram_plain_text(status, &body) {
        let plain_text = render_telegram_job_plain_for_send(payload, error_excerpt);
        let retry = client
            .post(&url)
            .json(&json!({ "chat_id": chat_id, "text": plain_text }))
            .send()
            .await?;
        if retry.status().is_success() {
            return Ok(());
        }
        let retry_status = retry.status();
        let retry_body = retry.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "telegram http {}: {} (fallback http {}: {})",
            status,
            body,
            retry_status,
            retry_body
        ));
    }

    Err(anyhow::anyhow!("telegram http {}: {}", status, body))
}

async fn send_email_job(
    smtp_url: Option<&str>,
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> anyhow::Result<()> {
    let smtp_url = smtp_url.context("email.smtpUrl missing")?;
    let (dsn, from, to) = parse_smtp_dsn(smtp_url)?;

    let status_zh = update_job_status_label_zh(&payload.job.status);
    let subject = format!("[dockrev] 更新完成（{status_zh}）");

    let plain_text = render_email_job_plain(payload, error_excerpt);
    let html_text = render_email_job_html(payload, error_excerpt);

    let mut builder = Message::builder().from(from).subject(subject);
    for addr in to {
        builder = builder.to(addr);
    }

    let email = builder.multipart(
        MultiPart::alternative()
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(plain_text),
            )
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html_text),
            ),
    )?;

    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::from_url(&dsn)?.build();
    mailer.send(email).await?;
    Ok(())
}

fn is_single_new_version_payload(payload: &NewVersionNotificationPayloadV2) -> bool {
    payload.links.service_urls.len() == 1 && payload.links.truncated.service_urls_omitted == 0
}

fn render_service_detail_action_html(url: &str) -> String {
    render_open_link_html(url, "服务详情")
}

fn render_service_detail_action_plain(url: &str) -> String {
    format!("服务详情：{url}")
}

fn render_telegram_new_version_html(payload: &NewVersionNotificationPayloadV2) -> String {
    let mut lines: Vec<String> = Vec::new();
    let single = is_single_new_version_payload(payload);
    if single {
        lines.push(format!("<b>{}</b>", escape_html(&payload.human.title)));
    } else {
        lines.push(format!(
            "<b>{}</b> {}",
            escape_html(&payload.human.title),
            render_open_link_html(&payload.links.primary_url, "详情")
        ));
    }
    lines.push(escape_html(&payload.human.summary));

    if !is_absolute_http_url(&payload.links.primary_url) {
        lines.push("提示：未配置实例 Public Base URL（系统设置），以下为站内路径。".to_string());
    }

    if single {
        if let Some(svc) = payload.links.service_urls.first() {
            lines.push(render_service_detail_action_html(&svc.url));
        }
        return lines.join("\n");
    }

    if !payload.links.service_urls.is_empty() {
        lines.push(String::new());
        lines.push("<b>服务清单</b>".to_string());
        for svc in &payload.links.service_urls {
            lines.push(format!(
                "- {}：{}",
                escape_html(&render_new_version_service_label(svc)),
                render_open_link_html(&svc.url, "服务详情"),
            ));
        }
        if payload.links.truncated.service_urls_omitted > 0 {
            lines.push(format!(
                "... 以及其他 {} 个服务（已省略）",
                payload.links.truncated.service_urls_omitted
            ));
        }
    }

    lines.join("\n")
}

fn render_telegram_new_version_plain(payload: &NewVersionNotificationPayloadV2) -> String {
    let mut lines: Vec<String> = Vec::new();
    let single = is_single_new_version_payload(payload);
    if single {
        lines.push(payload.human.title.clone());
    } else {
        lines.push(format!(
            "{} 详情：{}",
            payload.human.title, payload.links.primary_url
        ));
    }
    lines.push(payload.human.summary.clone());

    if !is_absolute_http_url(&payload.links.primary_url) {
        lines.push("提示：未配置实例 Public Base URL（系统设置），以下为站内路径。".to_string());
    }

    if single {
        if let Some(svc) = payload.links.service_urls.first() {
            lines.push(render_service_detail_action_plain(&svc.url));
        }
        return lines.join("\n");
    }

    if !payload.links.service_urls.is_empty() {
        lines.push(String::new());
        lines.push("服务清单".to_string());
        for svc in &payload.links.service_urls {
            lines.push(format!(
                "- {}: {}",
                render_new_version_service_label(svc),
                svc.url
            ));
        }
        if payload.links.truncated.service_urls_omitted > 0 {
            lines.push(format!(
                "... 以及其他 {} 个服务（已省略）",
                payload.links.truncated.service_urls_omitted
            ));
        }
    }

    lines.join("\n")
}

fn render_telegram_new_version_plain_for_send(payload: &NewVersionNotificationPayloadV2) -> String {
    let plain = render_telegram_new_version_plain(payload);
    truncate_chars(&plain, TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32))
}

fn render_email_new_version_plain(payload: &NewVersionNotificationPayloadV2) -> String {
    render_telegram_new_version_plain(payload)
}

fn render_email_new_version_html(payload: &NewVersionNotificationPayloadV2) -> String {
    let title = escape_html(&payload.human.title);
    let summary = escape_html(&payload.human.summary);
    let single = is_single_new_version_payload(payload);

    let mut note = String::new();
    if !is_absolute_http_url(&payload.links.job_url) {
        note = "<p><em>提示：未配置实例 Public Base URL（系统设置），以下链接可能仅为站内路径。</em></p>".to_string();
    }

    if single {
        let action = payload
            .links
            .service_urls
            .first()
            .map(|svc| render_service_detail_action_html(&svc.url))
            .unwrap_or_else(|| render_service_detail_action_html(&payload.links.primary_url));
        return format!("<h2>{title}</h2><p>{summary}</p>{note}<p>{action}</p>");
    }

    let mut items = String::new();
    if !payload.links.service_urls.is_empty() {
        items.push_str("<ul>");
        for svc in &payload.links.service_urls {
            let label = escape_html(&render_new_version_service_label(svc));
            if is_absolute_http_url(&svc.url) {
                items.push_str(&format!(
                    "<li>{label}: <a href=\"{}\">服务详情</a></li>",
                    escape_html(&svc.url)
                ));
            } else {
                items.push_str(&format!(
                    "<li>{label}: <code>{}</code></li>",
                    escape_html(&svc.url)
                ));
            }
        }
        if payload.links.truncated.service_urls_omitted > 0 {
            items.push_str(&format!(
                "<li>... 以及其他 {} 个服务（已省略）</li>",
                payload.links.truncated.service_urls_omitted
            ));
        }
        items.push_str("</ul>");
    }

    let check_link = if is_absolute_http_url(&payload.links.job_url) {
        format!(
            "<a href=\"{}\">{}</a>",
            escape_html(&payload.links.job_url),
            "查看检查任务"
        )
    } else {
        format!("<code>{}</code>", escape_html(&payload.links.job_url))
    };

    let open_primary = if is_absolute_http_url(&payload.links.primary_url) {
        format!(
            "<a href=\"{}\">{}</a>",
            escape_html(&payload.links.primary_url),
            escape_html(&payload.links.primary_url)
        )
    } else {
        format!("<code>{}</code>", escape_html(&payload.links.primary_url))
    };

    format!(
        "<h2>{title}</h2><p>{summary}</p>{note}<p>检查任务：{check_link}</p><p>打开：{open_primary}</p>{items}",
    )
}

fn render_telegram_ghcr_webhook_anomaly_html(payload: &GhcrWebhookAnomalyPayloadV2) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "<b>{}</b> {}",
        escape_html(&payload.human.title),
        render_open_link_html(&payload.links.job_url, "任务")
    ));
    lines.push(escape_html(&payload.human.summary));

    if !is_absolute_http_url(&payload.links.job_url) {
        lines.push("提示：未配置实例 Public Base URL（系统设置），以下为站内路径。".to_string());
    }

    if !payload.links.repos.is_empty() {
        lines.push(String::new());
        lines.push("<b>异常仓库</b>".to_string());
        for repo in &payload.links.repos {
            let mut detail = format!("{} [{}]", repo.full_name, repo.state);
            if let Some(err) = repo.last_error.as_deref() {
                detail.push_str(" - ");
                detail.push_str(err);
            }
            lines.push(format!("- {}", escape_html(&detail)));
        }
        if payload.links.truncated.repos_omitted > 0 {
            lines.push(format!(
                "... 以及其他 {} 个仓库（已省略）",
                payload.links.truncated.repos_omitted
            ));
        }
    }

    lines.join("\n")
}

fn render_telegram_ghcr_webhook_anomaly_plain(payload: &GhcrWebhookAnomalyPayloadV2) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "{} 任务：{}",
        payload.human.title, payload.links.job_url
    ));
    lines.push(payload.human.summary.clone());

    if !is_absolute_http_url(&payload.links.job_url) {
        lines.push("提示：未配置实例 Public Base URL（系统设置），以下为站内路径。".to_string());
    }

    if !payload.links.repos.is_empty() {
        lines.push(String::new());
        lines.push("异常仓库".to_string());
        for repo in &payload.links.repos {
            let mut detail = format!("{} [{}]", repo.full_name, repo.state);
            if let Some(err) = repo.last_error.as_deref() {
                detail.push_str(" - ");
                detail.push_str(err);
            }
            lines.push(format!("- {detail}"));
        }
        if payload.links.truncated.repos_omitted > 0 {
            lines.push(format!(
                "... 以及其他 {} 个仓库（已省略）",
                payload.links.truncated.repos_omitted
            ));
        }
    }

    lines.join("\n")
}

fn render_telegram_ghcr_webhook_anomaly_plain_for_send(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> String {
    let plain = render_telegram_ghcr_webhook_anomaly_plain(payload);
    truncate_chars(&plain, TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32))
}

fn render_email_ghcr_webhook_anomaly_plain(payload: &GhcrWebhookAnomalyPayloadV2) -> String {
    render_telegram_ghcr_webhook_anomaly_plain(payload)
}

fn render_email_ghcr_webhook_anomaly_html(payload: &GhcrWebhookAnomalyPayloadV2) -> String {
    let title = escape_html(&payload.human.title);
    let summary = escape_html(&payload.human.summary);

    let mut items = String::new();
    if !payload.links.repos.is_empty() {
        items.push_str("<ul>");
        for repo in &payload.links.repos {
            let mut detail = format!("{} [{}]", repo.full_name, repo.state);
            if let Some(err) = repo.last_error.as_deref() {
                detail.push_str(" - ");
                detail.push_str(err);
            }
            items.push_str(&format!("<li>{}</li>", escape_html(&detail)));
        }
        if payload.links.truncated.repos_omitted > 0 {
            items.push_str(&format!(
                "<li>... 以及其他 {} 个仓库（已省略）</li>",
                payload.links.truncated.repos_omitted
            ));
        }
        items.push_str("</ul>");
    }

    let job_link = if is_absolute_http_url(&payload.links.job_url) {
        format!(
            "<a href=\"{}\">{}</a>",
            escape_html(&payload.links.job_url),
            "查看巡检任务"
        )
    } else {
        format!("<code>{}</code>", escape_html(&payload.links.job_url))
    };

    let mut note = String::new();
    if !is_absolute_http_url(&payload.links.job_url) {
        note = "<p><em>提示：未配置实例 Public Base URL（系统设置），以下链接可能仅为站内路径。</em></p>".to_string();
    }

    format!("<h2>{title}</h2><p>{summary}</p>{note}<p>巡检任务：{job_link}</p>{items}",)
}

async fn send_telegram_new_version(
    client: &reqwest::Client,
    bot_token: Option<&str>,
    chat_id: Option<&str>,
    payload: &NewVersionNotificationPayloadV2,
) -> anyhow::Result<()> {
    let token = bot_token.context("telegram.botToken missing")?;
    let chat_id = chat_id.context("telegram.chatId missing")?;
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let html_text = render_telegram_new_version_html(payload);
    if html_text.chars().count() > TELEGRAM_MAX_MESSAGE_CHARS {
        let plain_text = render_telegram_new_version_plain_for_send(payload);
        let retry = client
            .post(&url)
            .json(&json!({ "chat_id": chat_id, "text": plain_text }))
            .send()
            .await?;
        if retry.status().is_success() {
            return Ok(());
        }
        let retry_status = retry.status();
        let retry_body = retry.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "telegram http {}: {}",
            retry_status,
            retry_body
        ));
    }

    let resp = client
        .post(&url)
        .json(&json!({ "chat_id": chat_id, "text": html_text, "parse_mode": "HTML" }))
        .send()
        .await?;
    if resp.status().is_success() {
        return Ok(());
    }

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if should_retry_telegram_plain_text(status, &body) {
        let plain_text = render_telegram_new_version_plain_for_send(payload);
        let retry = client
            .post(&url)
            .json(&json!({ "chat_id": chat_id, "text": plain_text }))
            .send()
            .await?;
        if retry.status().is_success() {
            return Ok(());
        }
        let retry_status = retry.status();
        let retry_body = retry.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "telegram http {}: {} (fallback http {}: {})",
            status,
            body,
            retry_status,
            retry_body
        ));
    }

    Err(anyhow::anyhow!("telegram http {}: {}", status, body))
}

async fn send_email_new_version(
    smtp_url: Option<&str>,
    payload: &NewVersionNotificationPayloadV2,
) -> anyhow::Result<()> {
    let smtp_url = smtp_url.context("email.smtpUrl missing")?;
    let (dsn, from, to) = parse_smtp_dsn(smtp_url)?;

    let subject = "[dockrev] 发现新版本".to_string();
    let plain_text = render_email_new_version_plain(payload);
    let html_text = render_email_new_version_html(payload);

    let mut builder = Message::builder().from(from).subject(subject);
    for addr in to {
        builder = builder.to(addr);
    }

    let email = builder.multipart(
        MultiPart::alternative()
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(plain_text),
            )
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html_text),
            ),
    )?;

    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::from_url(&dsn)?.build();
    mailer.send(email).await?;
    Ok(())
}

async fn send_telegram_ghcr_webhook_anomaly(
    client: &reqwest::Client,
    bot_token: Option<&str>,
    chat_id: Option<&str>,
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> anyhow::Result<()> {
    let token = bot_token.context("telegram.botToken missing")?;
    let chat_id = chat_id.context("telegram.chatId missing")?;
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let html_text = render_telegram_ghcr_webhook_anomaly_html(payload);
    if html_text.chars().count() > TELEGRAM_MAX_MESSAGE_CHARS {
        let plain_text = render_telegram_ghcr_webhook_anomaly_plain_for_send(payload);
        let retry = client
            .post(&url)
            .json(&json!({ "chat_id": chat_id, "text": plain_text }))
            .send()
            .await?;
        if retry.status().is_success() {
            return Ok(());
        }
        let retry_status = retry.status();
        let retry_body = retry.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "telegram http {}: {}",
            retry_status,
            retry_body
        ));
    }

    let resp = client
        .post(&url)
        .json(&json!({ "chat_id": chat_id, "text": html_text, "parse_mode": "HTML" }))
        .send()
        .await?;
    if resp.status().is_success() {
        return Ok(());
    }

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if should_retry_telegram_plain_text(status, &body) {
        let plain_text = render_telegram_ghcr_webhook_anomaly_plain_for_send(payload);
        let retry = client
            .post(&url)
            .json(&json!({ "chat_id": chat_id, "text": plain_text }))
            .send()
            .await?;
        if retry.status().is_success() {
            return Ok(());
        }
        let retry_status = retry.status();
        let retry_body = retry.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "telegram http {}: {} (fallback http {}: {})",
            status,
            body,
            retry_status,
            retry_body
        ));
    }

    Err(anyhow::anyhow!("telegram http {}: {}", status, body))
}

async fn send_email_ghcr_webhook_anomaly(
    smtp_url: Option<&str>,
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> anyhow::Result<()> {
    let smtp_url = smtp_url.context("email.smtpUrl missing")?;
    let (dsn, from, to) = parse_smtp_dsn(smtp_url)?;

    let subject = "[dockrev] GHCR Webhook 巡检异常".to_string();
    let plain_text = render_email_ghcr_webhook_anomaly_plain(payload);
    let html_text = render_email_ghcr_webhook_anomaly_html(payload);

    let mut builder = Message::builder().from(from).subject(subject);
    for addr in to {
        builder = builder.to(addr);
    }

    let email = builder.multipart(
        MultiPart::alternative()
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(plain_text),
            )
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html_text),
            ),
    )?;

    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::from_url(&dsn)?.build();
    mailer.send(email).await?;
    Ok(())
}

fn finalize_job_links(
    job_url: String,
    mut service_urls_full: Vec<JobNotificationServiceUrlV2>,
    job_scope_is_service: bool,
    job_service_id: Option<&str>,
) -> JobNotificationLinksV2 {
    // Keep service ordering stable across channels.
    service_urls_full.sort_by(|a, b| {
        (
            a.stack_name.as_str(),
            a.service_name.as_str(),
            a.service_id.as_str(),
        )
            .cmp(&(
                b.stack_name.as_str(),
                b.service_name.as_str(),
                b.service_id.as_str(),
            ))
    });

    let unique_service_url = if job_scope_is_service && let Some(target) = job_service_id {
        service_urls_full
            .iter()
            .find(|s| s.service_id == target)
            .map(|s| s.url.clone())
    } else if service_urls_full.len() == 1 {
        service_urls_full.first().map(|s| s.url.clone())
    } else {
        None
    };

    let primary_url = unique_service_url.unwrap_or_else(|| job_url.clone());

    let omitted = service_urls_full.len().saturating_sub(MAX_JOB_SERVICE_URLS) as u32;
    service_urls_full.truncate(MAX_JOB_SERVICE_URLS);

    JobNotificationLinksV2 {
        primary_url,
        job_url,
        service_urls: service_urls_full,
        truncated: JobNotificationTruncatedV2 {
            service_urls_omitted: omitted,
        },
    }
}

async fn build_job_payload_v2(
    state: &AppState,
    now_rfc3339: &str,
    public_base_url: Option<&str>,
    channel: &'static str,
    job_id: &str,
    status: &str,
    summary: &Value,
) -> anyhow::Result<JobNotificationPayloadV2> {
    let job_opt = state.db.get_job(job_id).await?;

    let job = match &job_opt {
        Some(job) => JobNotificationJobV2 {
            id: job.id.clone(),
            r#type: job.r#type.as_str().to_string(),
            scope: job.scope.as_str().to_string(),
            status: status.to_string(),
            reason: job.reason.clone(),
            created_by: job.created_by.clone(),
            created_at: job.created_at.clone(),
            started_at: job.started_at.clone(),
            finished_at: job.finished_at.clone(),
            stack_id: job.stack_id.clone(),
            service_id: job.service_id.clone(),
        },
        None => JobNotificationJobV2 {
            id: job_id.to_string(),
            r#type: "update".to_string(),
            scope: "unknown".to_string(),
            status: status.to_string(),
            reason: "unknown".to_string(),
            created_by: "unknown".to_string(),
            created_at: now_rfc3339.to_string(),
            started_at: None,
            finished_at: Some(now_rfc3339.to_string()),
            stack_id: None,
            service_id: None,
        },
    };

    let job_url = best_effort_url(public_base_url, &format!("queue/{job_id}"));

    let mut pairs: Vec<(String, String)> = Vec::new();
    let job_scope_is_service = job_opt
        .as_ref()
        .is_some_and(|j| j.scope.as_str() == "service" && j.service_id.is_some());
    if job_scope_is_service
        && let (Some(stack_id), Some(service_id)) = (job.stack_id.clone(), job.service_id.clone())
    {
        pairs.push((stack_id, service_id));
    }
    pairs.extend(extract_changed_services_by_stack(summary));

    let mut seen = std::collections::HashSet::<String>::new();
    let mut unique_pairs: Vec<(String, String)> = Vec::new();
    for (stack_id, service_id) in pairs {
        if seen.insert(service_id.clone()) {
            unique_pairs.push((stack_id, service_id));
        }
    }

    let mut service_urls_full: Vec<JobNotificationServiceUrlV2> = Vec::new();
    for (stack_id, service_id) in unique_pairs {
        let stack = state.db.get_stack(&stack_id).await?;
        let stack_name = stack
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| stack_id.clone());
        let service_name = stack
            .as_ref()
            .and_then(|s| {
                s.services
                    .iter()
                    .find(|svc| svc.id == service_id)
                    .map(|svc| svc.name.clone())
            })
            .unwrap_or_else(|| service_id.clone());
        let url = best_effort_url(
            public_base_url,
            &format!("services/{stack_id}/{service_id}"),
        );
        service_urls_full.push(JobNotificationServiceUrlV2 {
            stack_id,
            stack_name,
            service_id,
            service_name,
            url,
        });
    }
    let links = finalize_job_links(
        job_url.clone(),
        service_urls_full,
        job_scope_is_service,
        job.service_id.as_deref(),
    );

    let status_zh = update_job_status_label_zh(status);
    let title = if status == "failed" {
        "Dockrev：更新失败".to_string()
    } else {
        format!("Dockrev：更新完成（{status_zh}）")
    };

    let summary = if links.service_urls.is_empty() {
        format!("状态：{status_zh}。")
    } else {
        summarize_updated_services(&links.service_urls, links.truncated.service_urls_omitted)
    };

    let mut detail_lines = Vec::new();
    detail_lines.push(format!("任务：{job_id}"));
    detail_lines.push(format!("打开：{}", links.primary_url));
    detail_lines.push(format!("发送：{now_rfc3339}"));
    if !is_absolute_http_url(&links.job_url) {
        detail_lines.push(
            "提示：未配置实例 Public Base URL（系统设置），Telegram/Email 无法生成可点击链接。"
                .to_string(),
        );
    }
    let detail = detail_lines.join("\n");

    Ok(JobNotificationPayloadV2 {
        schema: "dockrev.notification.job.v2",
        kind: "job_finished",
        sent_at: now_rfc3339.to_string(),
        channel,
        job,
        links,
        human: JobNotificationHumanV2 {
            title,
            summary,
            detail,
        },
        debug: JobNotificationDebugV2 {
            app_version: state.config.app_effective_version.clone(),
            source: "dockrev-api",
        },
    })
}

async fn build_new_version_payload_v2(
    state: &AppState,
    now_rfc3339: &str,
    public_base_url: Option<&str>,
    channel: &'static str,
    check_job_id: &str,
    services_checked: u32,
    discovered_services: &[NewVersionDiscoveredService],
) -> anyhow::Result<NewVersionNotificationPayloadV2> {
    let job_opt = state.db.get_job(check_job_id).await?;
    let status = job_opt
        .as_ref()
        .map(|job| job.status.clone())
        .unwrap_or_else(|| "success".to_string());
    let scope = job_opt
        .as_ref()
        .map(|job| job.scope.as_str().to_string())
        .unwrap_or_else(|| "all".to_string());

    let job_url = best_effort_url(public_base_url, &format!("queue/{check_job_id}"));

    let mut seen = std::collections::HashSet::<String>::new();
    let mut service_urls_full: Vec<NewVersionNotificationServiceUrlV2> = Vec::new();
    for item in discovered_services {
        let key = format!("{}/{}", item.stack_id, item.service_id);
        if !seen.insert(key) {
            continue;
        }

        let stack = state.db.get_stack(&item.stack_id).await?;
        let stack_name = stack
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| item.stack_id.clone());
        let service_name = stack
            .as_ref()
            .and_then(|s| {
                s.services
                    .iter()
                    .find(|svc| svc.id == item.service_id)
                    .map(|svc| svc.name.clone())
            })
            .unwrap_or_else(|| item.service_id.clone());

        let url = best_effort_url(
            public_base_url,
            &format!("services/{}/{}", item.stack_id, item.service_id),
        );
        service_urls_full.push(NewVersionNotificationServiceUrlV2 {
            stack_id: item.stack_id.clone(),
            stack_name,
            service_id: item.service_id.clone(),
            service_name,
            current_tag: Some(item.current_tag.clone()),
            current_display_tag: Some(item.current_display_tag.clone()),
            candidate_tag: Some(item.candidate_tag.clone()),
            candidate_display_tag: Some(item.candidate_display_tag.clone()),
            url,
        });
    }

    service_urls_full.sort_by(|a, b| {
        (
            a.stack_name.as_str(),
            a.service_name.as_str(),
            a.service_id.as_str(),
        )
            .cmp(&(
                b.stack_name.as_str(),
                b.service_name.as_str(),
                b.service_id.as_str(),
            ))
    });

    let total_new_versions = service_urls_full.len();
    let omitted = service_urls_full
        .len()
        .saturating_sub(MAX_NEW_VERSION_SERVICE_URLS) as u32;
    service_urls_full.truncate(MAX_NEW_VERSION_SERVICE_URLS);

    let primary_url = if service_urls_full.len() == 1 {
        service_urls_full
            .first()
            .map(|svc| svc.url.clone())
            .unwrap_or_else(|| job_url.clone())
    } else {
        job_url.clone()
    };

    let summary = summarize_new_version_services(total_new_versions, &service_urls_full, omitted);

    let mut detail_lines = vec![
        format!("检查任务：{check_job_id}"),
        format!("打开：{primary_url}"),
        format!("发送：{now_rfc3339}"),
    ];
    if !is_absolute_http_url(&job_url) {
        detail_lines.push(
            "提示：未配置实例 Public Base URL（系统设置），Telegram/Email 无法生成可点击链接。"
                .to_string(),
        );
    }

    Ok(NewVersionNotificationPayloadV2 {
        schema: "dockrev.notification.new_version_discovered.v2",
        kind: "new_version_discovered",
        sent_at: now_rfc3339.to_string(),
        channel,
        check: NewVersionNotificationCheckV2 {
            job_id: check_job_id.to_string(),
            status,
            scope,
            services_checked,
            new_versions: total_new_versions as u32,
        },
        links: NewVersionNotificationLinksV2 {
            primary_url,
            job_url,
            service_urls: service_urls_full,
            truncated: JobNotificationTruncatedV2 {
                service_urls_omitted: omitted,
            },
        },
        human: JobNotificationHumanV2 {
            title: "Dockrev：发现新版本".to_string(),
            summary,
            detail: detail_lines.join("\n"),
        },
        debug: JobNotificationDebugV2 {
            app_version: state.config.app_effective_version.clone(),
            source: "dockrev-api",
        },
    })
}

async fn build_ghcr_webhook_anomaly_payload_v2(
    state: &AppState,
    now_rfc3339: &str,
    public_base_url: Option<&str>,
    channel: &'static str,
    event: GhcrWebhookAnomalyEvent<'_>,
) -> anyhow::Result<GhcrWebhookAnomalyPayloadV2> {
    let job_url = best_effort_url(public_base_url, &format!("queue/{}", event.job_id));
    let settings_url = best_effort_url(public_base_url, "settings");
    let primary_url = job_url.clone();
    let total_anomalies = event.counts.total();

    let mut seen = std::collections::HashSet::<String>::new();
    let mut repo_items: Vec<GhcrWebhookAnomalyRepoV2> = Vec::new();
    for repo in event.repos {
        let full_name = format!("{}/{}", repo.owner, repo.repo);
        if !seen.insert(full_name.to_ascii_lowercase()) {
            continue;
        }

        let last_error = repo
            .last_error
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| truncate_chars(v, MAX_GHCR_REPO_ERROR_CHARS));

        repo_items.push(GhcrWebhookAnomalyRepoV2 {
            owner: repo.owner.clone(),
            repo: repo.repo.clone(),
            full_name,
            state: repo.state.clone(),
            last_error,
        });
    }

    repo_items.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    let omitted = repo_items.len().saturating_sub(MAX_GHCR_REPOS) as u32;
    repo_items.truncate(MAX_GHCR_REPOS);
    let summary = summarize_ghcr_anomaly_repos(total_anomalies, &repo_items, omitted);

    let mut detail_lines = vec![
        format!("任务：{}", event.job_id),
        format!("打开：{primary_url}"),
        format!("发送：{now_rfc3339}"),
    ];
    if !is_absolute_http_url(&settings_url) {
        detail_lines.push(
            "提示：未配置实例 Public Base URL（系统设置），Telegram/Email 无法生成可点击链接。"
                .to_string(),
        );
    }

    Ok(GhcrWebhookAnomalyPayloadV2 {
        schema: "dockrev.notification.ghcr_webhook_anomaly.v2",
        kind: "ghcr_webhook_anomaly",
        sent_at: now_rfc3339.to_string(),
        channel,
        job: GhcrWebhookAnomalyJobV2 {
            id: event.job_id.to_string(),
            status: event.status.to_string(),
            missing: event.counts.missing,
            conflict: event.counts.conflict,
            error: event.counts.error,
            total_anomalies,
        },
        links: GhcrWebhookAnomalyLinksV2 {
            primary_url,
            job_url,
            settings_url,
            repos: repo_items,
            truncated: GhcrWebhookAnomalyTruncatedV2 {
                repos_omitted: omitted,
            },
        },
        human: JobNotificationHumanV2 {
            title: "Dockrev：GitHub Webhook 巡检异常".to_string(),
            summary,
            detail: detail_lines.join("\n"),
        },
        debug: JobNotificationDebugV2 {
            app_version: state.config.app_effective_version.clone(),
            source: "dockrev-api",
        },
    })
}

async fn send_new_versions(
    state: &AppState,
    check_job_id: &str,
    now_rfc3339: &str,
    services_checked: u32,
    discovered_services: &[NewVersionDiscoveredService],
) -> anyhow::Result<Value> {
    let settings = state.db.get_notification_settings().await?;
    if !is_event_enabled(&settings, NotificationEventKind::NewVersionDiscovered) {
        return Ok(Value::Object(serde_json::Map::new()));
    }

    let public_base_url = state.db.get_instance_public_base_url().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .context("build reqwest client")?;

    let mut results = serde_json::Map::new();

    if settings.webhook_enabled {
        let r = async {
            let payload = build_new_version_payload_v2(
                state,
                now_rfc3339,
                public_base_url.as_deref(),
                "webhook",
                check_job_id,
                services_checked,
                discovered_services,
            )
            .await?;
            let value = to_new_version_value(&payload)?;
            send_webhook(&client, settings.webhook_url.as_deref(), &value).await
        }
        .await;
        log_result(state, Some(check_job_id), now_rfc3339, "webhook", &r).await;
        results.insert("webhook".to_string(), result_value(r));
    }

    if settings.telegram_enabled {
        let r = async {
            let payload = build_new_version_payload_v2(
                state,
                now_rfc3339,
                public_base_url.as_deref(),
                "telegram",
                check_job_id,
                services_checked,
                discovered_services,
            )
            .await?;
            send_telegram_new_version(
                &client,
                settings.telegram_bot_token.as_deref(),
                settings.telegram_chat_id.as_deref(),
                &payload,
            )
            .await
        }
        .await;
        log_result(state, Some(check_job_id), now_rfc3339, "telegram", &r).await;
        results.insert("telegram".to_string(), result_value(r));
    }

    if settings.email_enabled {
        let r = async {
            let payload = build_new_version_payload_v2(
                state,
                now_rfc3339,
                public_base_url.as_deref(),
                "email",
                check_job_id,
                services_checked,
                discovered_services,
            )
            .await?;
            send_email_new_version(settings.email_smtp_url.as_deref(), &payload).await
        }
        .await;
        log_result(state, Some(check_job_id), now_rfc3339, "email", &r).await;
        results.insert("email".to_string(), result_value(r));
    }

    if settings.webpush_enabled {
        let r = async {
            let payload = build_new_version_payload_v2(
                state,
                now_rfc3339,
                public_base_url.as_deref(),
                "webPush",
                check_job_id,
                services_checked,
                discovered_services,
            )
            .await?;
            let web_push_payload = to_web_push_new_version_value(&payload)?;
            send_web_push(
                state,
                settings.webpush_vapid_private_key.as_deref(),
                settings.webpush_vapid_subject.as_deref(),
                &web_push_payload,
            )
            .await
        }
        .await;
        log_result(state, Some(check_job_id), now_rfc3339, "webPush", &r).await;
        results.insert("webPush".to_string(), result_value(r));
    }

    Ok(Value::Object(results))
}

async fn send_ghcr_webhook_anomaly(
    state: &AppState,
    now_rfc3339: &str,
    event: GhcrWebhookAnomalyEvent<'_>,
) -> anyhow::Result<Value> {
    let settings = state.db.get_notification_settings().await?;
    if !is_event_enabled(&settings, NotificationEventKind::GhcrWebhookAnomaly) {
        return Ok(Value::Object(serde_json::Map::new()));
    }

    let public_base_url = state.db.get_instance_public_base_url().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .context("build reqwest client")?;

    let mut results = serde_json::Map::new();

    if settings.webhook_enabled {
        let r = async {
            let payload = build_ghcr_webhook_anomaly_payload_v2(
                state,
                now_rfc3339,
                public_base_url.as_deref(),
                "webhook",
                event,
            )
            .await?;
            let value = to_ghcr_webhook_anomaly_value(&payload)?;
            send_webhook(&client, settings.webhook_url.as_deref(), &value).await
        }
        .await;
        log_result(state, Some(event.job_id), now_rfc3339, "webhook", &r).await;
        results.insert("webhook".to_string(), result_value(r));
    }

    if settings.telegram_enabled {
        let r = async {
            let payload = build_ghcr_webhook_anomaly_payload_v2(
                state,
                now_rfc3339,
                public_base_url.as_deref(),
                "telegram",
                event,
            )
            .await?;
            send_telegram_ghcr_webhook_anomaly(
                &client,
                settings.telegram_bot_token.as_deref(),
                settings.telegram_chat_id.as_deref(),
                &payload,
            )
            .await
        }
        .await;
        log_result(state, Some(event.job_id), now_rfc3339, "telegram", &r).await;
        results.insert("telegram".to_string(), result_value(r));
    }

    if settings.email_enabled {
        let r = async {
            let payload = build_ghcr_webhook_anomaly_payload_v2(
                state,
                now_rfc3339,
                public_base_url.as_deref(),
                "email",
                event,
            )
            .await?;
            send_email_ghcr_webhook_anomaly(settings.email_smtp_url.as_deref(), &payload).await
        }
        .await;
        log_result(state, Some(event.job_id), now_rfc3339, "email", &r).await;
        results.insert("email".to_string(), result_value(r));
    }

    if settings.webpush_enabled {
        let r = async {
            let payload = build_ghcr_webhook_anomaly_payload_v2(
                state,
                now_rfc3339,
                public_base_url.as_deref(),
                "webPush",
                event,
            )
            .await?;
            let web_push_payload = to_web_push_ghcr_webhook_anomaly_value(&payload)?;
            send_web_push(
                state,
                settings.webpush_vapid_private_key.as_deref(),
                settings.webpush_vapid_subject.as_deref(),
                &web_push_payload,
            )
            .await
        }
        .await;
        log_result(state, Some(event.job_id), now_rfc3339, "webPush", &r).await;
        results.insert("webPush".to_string(), result_value(r));
    }

    Ok(Value::Object(results))
}

async fn send_all(
    state: &AppState,
    job_id: Option<&str>,
    now_rfc3339: &str,
    payload: Option<&Value>,
    mode: NotifySendMode,
) -> anyhow::Result<Value> {
    let settings = state.db.get_notification_settings().await?;
    if matches!(mode, NotifySendMode::Default)
        && !is_event_enabled(&settings, NotificationEventKind::Update)
    {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    let public_base_url = state.db.get_instance_public_base_url().await?;
    let test_url = best_effort_url(public_base_url.as_deref(), "settings");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .context("build reqwest client")?;

    let mut results = serde_json::Map::new();

    if should_send_channel(
        &mode,
        settings.webhook_enabled,
        NotificationTestChannel::Webhook,
    ) {
        let r = match &mode {
            NotifySendMode::Default => {
                let envelope = payload.context("notify payload missing for default mode")?;
                let status = envelope
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let summary = envelope
                    .get("summary")
                    .context("notify summary missing for default mode")?;
                let job_id = job_id.context("notify jobId missing for default mode")?;
                let job_payload = build_job_payload_v2(
                    state,
                    now_rfc3339,
                    public_base_url.as_deref(),
                    "webhook",
                    job_id,
                    status,
                    summary,
                )
                .await?;
                let job_value = to_job_value(&job_payload)?;
                send_webhook(&client, settings.webhook_url.as_deref(), &job_value).await
            }
            NotifySendMode::Test { channel, message } => {
                let test_payload = build_test_payload_v2(
                    now_rfc3339,
                    message,
                    *channel,
                    NotificationTestChannel::Webhook,
                    &state.config.app_effective_version,
                    &test_url,
                );
                let test_payload = to_value(&test_payload)?;
                send_webhook(&client, settings.webhook_url.as_deref(), &test_payload).await
            }
        };
        log_result(state, job_id, now_rfc3339, "webhook", &r).await;
        results.insert("webhook".to_string(), result_value(r));
    }

    if should_send_channel(
        &mode,
        settings.telegram_enabled,
        NotificationTestChannel::Telegram,
    ) {
        let r = match &mode {
            NotifySendMode::Default => {
                let envelope = payload.context("notify payload missing for default mode")?;
                let status = envelope
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let summary = envelope
                    .get("summary")
                    .context("notify summary missing for default mode")?;
                let error_excerpt = extract_error_excerpt(summary);
                let job_id = job_id.context("notify jobId missing for default mode")?;
                let job_payload = build_job_payload_v2(
                    state,
                    now_rfc3339,
                    public_base_url.as_deref(),
                    "telegram",
                    job_id,
                    status,
                    summary,
                )
                .await?;
                send_telegram_job(
                    &client,
                    settings.telegram_bot_token.as_deref(),
                    settings.telegram_chat_id.as_deref(),
                    &job_payload,
                    error_excerpt.as_deref(),
                )
                .await
            }
            NotifySendMode::Test { channel, message } => {
                let test_payload = build_test_payload_v2(
                    now_rfc3339,
                    message,
                    *channel,
                    NotificationTestChannel::Telegram,
                    &state.config.app_effective_version,
                    &test_url,
                );
                send_telegram_test(
                    &client,
                    settings.telegram_bot_token.as_deref(),
                    settings.telegram_chat_id.as_deref(),
                    &test_payload,
                )
                .await
            }
        };
        log_result(state, job_id, now_rfc3339, "telegram", &r).await;
        results.insert("telegram".to_string(), result_value(r));
    }

    if should_send_channel(
        &mode,
        settings.email_enabled,
        NotificationTestChannel::Email,
    ) {
        let r = match &mode {
            NotifySendMode::Default => {
                let envelope = payload.context("notify payload missing for default mode")?;
                let status = envelope
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let summary = envelope
                    .get("summary")
                    .context("notify summary missing for default mode")?;
                let error_excerpt = extract_error_excerpt(summary);
                let job_id = job_id.context("notify jobId missing for default mode")?;
                let job_payload = build_job_payload_v2(
                    state,
                    now_rfc3339,
                    public_base_url.as_deref(),
                    "email",
                    job_id,
                    status,
                    summary,
                )
                .await?;
                send_email_job(
                    settings.email_smtp_url.as_deref(),
                    &job_payload,
                    error_excerpt.as_deref(),
                )
                .await
            }
            NotifySendMode::Test { channel, message } => {
                let test_payload = build_test_payload_v2(
                    now_rfc3339,
                    message,
                    *channel,
                    NotificationTestChannel::Email,
                    &state.config.app_effective_version,
                    &test_url,
                );
                send_email_test(settings.email_smtp_url.as_deref(), &test_payload).await
            }
        };
        log_result(state, job_id, now_rfc3339, "email", &r).await;
        results.insert("email".to_string(), result_value(r));
    }

    if should_send_channel(
        &mode,
        settings.webpush_enabled,
        NotificationTestChannel::WebPush,
    ) {
        let r = match &mode {
            NotifySendMode::Default => {
                let envelope = payload.context("notify payload missing for default mode")?;
                let status = envelope
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let summary = envelope
                    .get("summary")
                    .context("notify summary missing for default mode")?;
                let error_excerpt = extract_error_excerpt(summary);
                let job_id = job_id.context("notify jobId missing for default mode")?;
                let job_payload = build_job_payload_v2(
                    state,
                    now_rfc3339,
                    public_base_url.as_deref(),
                    "webPush",
                    job_id,
                    status,
                    summary,
                )
                .await?;
                let web_push_payload =
                    to_web_push_job_value(&job_payload, error_excerpt.as_deref())?;
                send_web_push(
                    state,
                    settings.webpush_vapid_private_key.as_deref(),
                    settings.webpush_vapid_subject.as_deref(),
                    &web_push_payload,
                )
                .await
            }
            NotifySendMode::Test { channel, message } => {
                let test_payload = build_test_payload_v2(
                    now_rfc3339,
                    message,
                    *channel,
                    NotificationTestChannel::WebPush,
                    &state.config.app_effective_version,
                    &test_url,
                );
                let web_push_payload = to_web_push_value(&test_payload)?;
                send_web_push(
                    state,
                    settings.webpush_vapid_private_key.as_deref(),
                    settings.webpush_vapid_subject.as_deref(),
                    &web_push_payload,
                )
                .await
            }
        };
        log_result(state, job_id, now_rfc3339, "webPush", &r).await;
        results.insert("webPush".to_string(), result_value(r));
    }

    Ok(Value::Object(results))
}

async fn log_result(
    state: &AppState,
    job_id: Option<&str>,
    now_rfc3339: &str,
    channel: &str,
    result: &anyhow::Result<()>,
) {
    let Some(job_id) = job_id else { return };
    let (level, msg) = match result {
        Ok(()) => ("info", format!("notify: {channel}=ok")),
        Err(e) => ("warn", format!("notify: {channel}=failed error={e}")),
    };
    let _ = state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: now_rfc3339.to_string(),
                level: level.to_string(),
                msg,
            },
        )
        .await;
}

fn result_value(result: anyhow::Result<()>) -> Value {
    match result {
        Ok(()) => json!({"ok": true}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

async fn send_webhook(
    client: &reqwest::Client,
    url: Option<&str>,
    payload: &Value,
) -> anyhow::Result<()> {
    let url = url.context("webhook.url missing")?;
    let resp = client.post(url).json(payload).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("webhook http {}: {}", status, body));
    }
    Ok(())
}

async fn send_telegram_test(
    client: &reqwest::Client,
    bot_token: Option<&str>,
    chat_id: Option<&str>,
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<()> {
    let token = bot_token.context("telegram.botToken missing")?;
    let chat_id = chat_id.context("telegram.chatId missing")?;
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let html_text = render_telegram_test_html(payload)?;
    if html_text.chars().count() > TELEGRAM_MAX_MESSAGE_CHARS {
        let plain_text = render_telegram_plain_for_send(payload)?;
        let retry = client
            .post(&url)
            .json(&json!({ "chat_id": chat_id, "text": plain_text }))
            .send()
            .await?;
        if retry.status().is_success() {
            return Ok(());
        }
        let retry_status = retry.status();
        let retry_body = retry.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "telegram http {}: {}",
            retry_status,
            retry_body
        ));
    }

    let resp = client
        .post(&url)
        .json(&json!({ "chat_id": chat_id, "text": html_text, "parse_mode": "HTML" }))
        .send()
        .await?;
    if resp.status().is_success() {
        return Ok(());
    }

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if should_retry_telegram_plain_text(status, &body) {
        let plain_text = render_telegram_plain_for_send(payload)?;
        let retry = client
            .post(&url)
            .json(&json!({ "chat_id": chat_id, "text": plain_text }))
            .send()
            .await?;
        if retry.status().is_success() {
            return Ok(());
        }
        let retry_status = retry.status();
        let retry_body = retry.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "telegram http {}: {} (fallback http {}: {})",
            status,
            body,
            retry_status,
            retry_body
        ));
    }

    Err(anyhow::anyhow!("telegram http {}: {}", status, body))
}

async fn send_email_test(
    smtp_url: Option<&str>,
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<()> {
    let smtp_url = smtp_url.context("email.smtpUrl missing")?;
    let (dsn, from, to) = parse_smtp_dsn(smtp_url)?;

    let subject = "[dockrev] test notification";
    let plain_text = render_email_test_plain(payload)?;
    let html_text = render_email_test_html(payload)?;

    let mut builder = Message::builder().from(from).subject(subject);
    for addr in to {
        builder = builder.to(addr);
    }

    let email = builder.multipart(
        MultiPart::alternative()
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(plain_text),
            )
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html_text),
            ),
    )?;

    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::from_url(&dsn)?.build();
    mailer.send(email).await?;
    Ok(())
}

fn parse_smtp_dsn(smtp_url: &str) -> anyhow::Result<(String, Mailbox, Vec<Mailbox>)> {
    let mut url = Url::parse(smtp_url).context("invalid smtpUrl")?;
    let mut to = Vec::new();
    let mut from: Option<Mailbox> = None;

    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "to" => {
                for part in v.split(',') {
                    let part = part.trim();
                    if !part.is_empty() {
                        to.push(part.parse::<Mailbox>().context("invalid to address")?);
                    }
                }
            }
            "from" => {
                if from.is_none() {
                    from = Some(v.parse::<Mailbox>().context("invalid from address")?);
                }
            }
            _ => {}
        }
    }

    url.set_query(None);

    let from = match from {
        Some(v) => v,
        None => {
            let host = url.host_str().unwrap_or("localhost");
            format!("Dockrev <dockrev@{host}>")
                .parse::<Mailbox>()
                .context("invalid default from mailbox")?
        }
    };

    if to.is_empty() {
        return Err(anyhow::anyhow!("email to missing (set ?to= on smtpUrl)"));
    }

    Ok((url.to_string(), from, to))
}

async fn send_web_push(
    state: &AppState,
    vapid_private_key: Option<&str>,
    vapid_subject: Option<&str>,
    payload: &Value,
) -> anyhow::Result<()> {
    let private_key = vapid_private_key.context("webPush.vapidPrivateKey missing")?;
    let subject = vapid_subject.unwrap_or("mailto:dockrev@localhost");

    let subs = state.db.list_web_push_subscriptions().await?;
    if subs.is_empty() {
        return Err(anyhow::anyhow!("no web push subscriptions"));
    }

    let client = HyperWebPushClient::new();
    let content = serde_json::to_vec(payload)?;

    let mut sent = 0u32;
    for (endpoint, p256dh, auth) in subs {
        let subscription = SubscriptionInfo::new(endpoint, p256dh, auth);
        let mut sig_builder =
            VapidSignatureBuilder::from_base64(private_key, &subscription).context("vapid key")?;
        sig_builder.add_claim("sub", subject);
        let signature = sig_builder.build().context("build vapid signature")?;

        let mut builder = WebPushMessageBuilder::new(&subscription);
        builder.set_payload(ContentEncoding::Aes128Gcm, &content);
        builder.set_urgency(Urgency::Normal);
        builder.set_ttl(60);
        builder.set_vapid_signature(signature);

        match client.send(builder.build()?).await {
            Ok(()) => sent += 1,
            Err(WebPushError::EndpointNotValid(_)) | Err(WebPushError::EndpointNotFound(_)) => {
                let _ = state
                    .db
                    .delete_web_push_subscription(&subscription.endpoint)
                    .await;
            }
            Err(e) => {
                return Err(anyhow::anyhow!("web push send failed: {}", e));
            }
        }
    }

    if sent == 0 {
        return Err(anyhow::anyhow!("web push: no successful sends"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smtp_dsn_parsing_requires_to() {
        let err = parse_smtp_dsn("smtp://user:pass@smtp.example.com:587").unwrap_err();
        assert!(err.to_string().contains("to missing"));
    }

    #[test]
    fn smtp_dsn_parsing_accepts_query_from_to() {
        let (dsn, _from, to) = parse_smtp_dsn(
            "smtp://user@example.com:pass@smtp.example.com:587?from=Dockrev%20<noreply@example.com>&to=a@example.com,b@example.com",
        )
        .unwrap();
        assert!(!dsn.contains("?"));
        assert_eq!(to.len(), 2);
    }

    #[test]
    fn test_payload_v2_shape_is_breaking() {
        let payload = build_test_payload_v2(
            "2026-03-05T04:44:59.673686721Z",
            "dockrev: test notification",
            Some(NotificationTestChannel::Webhook),
            NotificationTestChannel::Telegram,
            "0.1.0",
            "https://dockrev.example.com/settings",
        );
        let value = to_value(&payload).unwrap();

        assert_eq!(
            value["schema"].as_str(),
            Some("dockrev.notification.test.v2")
        );
        assert_eq!(value["kind"].as_str(), Some("notification_test"));
        assert_eq!(value["channel"].as_str(), Some("telegram"));
        assert_eq!(
            value["url"].as_str(),
            Some("https://dockrev.example.com/settings")
        );
        assert_eq!(
            value["human"]["summary"].as_str(),
            Some("dockrev: test notification")
        );
        assert_eq!(value["debug"]["requestedChannel"].as_str(), Some("webhook"));
        assert!(value.get("type").is_none());
        assert!(value.get("ts").is_none());
        assert!(value.get("message").is_none());
    }

    #[test]
    fn telegram_test_message_contains_html_code_block() {
        let payload = build_test_payload_v2(
            "2026-03-05T04:44:59.673686721Z",
            "dockrev: test notification",
            None,
            NotificationTestChannel::Telegram,
            "0.1.0",
            "https://dockrev.example.com/settings",
        );
        let html = render_telegram_test_html(&payload).unwrap();
        assert!(html.contains("<pre>"));
        assert!(html.contains("<b>Debug</b>"));
    }

    #[test]
    fn web_push_body_is_plain_text_without_code_blocks() {
        let payload = build_test_payload_v2(
            "2026-03-05T04:44:59.673686721Z",
            "dockrev: test notification",
            None,
            NotificationTestChannel::WebPush,
            "0.1.0",
            "https://dockrev.example.com/settings",
        );
        let value = to_web_push_value(&payload).unwrap();
        let body = value["body"].as_str().unwrap_or_default();
        assert!(!body.contains("```"));
        assert!(!body.contains("<pre>"));
        assert_eq!(
            value["url"].as_str(),
            Some("https://dockrev.example.com/settings")
        );
    }

    #[test]
    fn truncate_chars_marks_overflow() {
        assert_eq!(truncate_chars("abcdef", 4), "abcd... [truncated]");
        assert_eq!(truncate_chars("abc", 4), "abc");
    }

    #[test]
    fn telegram_plain_text_retry_only_on_parse_errors() {
        assert!(should_retry_telegram_plain_text(
            reqwest::StatusCode::BAD_REQUEST,
            "{\"description\":\"Bad Request: can't parse entities\"}"
        ));
        assert!(!should_retry_telegram_plain_text(
            reqwest::StatusCode::BAD_REQUEST,
            "{\"description\":\"Bad Request: chat not found\"}"
        ));
        assert!(!should_retry_telegram_plain_text(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{\"description\":\"Bad Request: can't parse entities\"}"
        ));
    }

    #[test]
    fn telegram_plain_payload_is_capped_for_send() {
        let payload = build_test_payload_v2(
            "2026-03-05T04:44:59.673686721Z",
            &"&".repeat(5000),
            None,
            NotificationTestChannel::Telegram,
            "0.1.0",
            "https://dockrev.example.com/settings",
        );
        let plain = render_telegram_plain_for_send(&payload).unwrap();
        assert!(plain.chars().count() <= TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32));
    }

    fn sample_job_payload(links: JobNotificationLinksV2) -> JobNotificationPayloadV2 {
        JobNotificationPayloadV2 {
            schema: "dockrev.notification.job.v2",
            kind: "job_finished",
            sent_at: "2026-03-05T04:44:59Z".to_string(),
            channel: "telegram",
            job: JobNotificationJobV2 {
                id: "job_123".to_string(),
                r#type: "update".to_string(),
                scope: "all".to_string(),
                status: "success".to_string(),
                reason: "manual".to_string(),
                created_by: "test".to_string(),
                created_at: "2026-03-05T04:40:00Z".to_string(),
                started_at: Some("2026-03-05T04:41:00Z".to_string()),
                finished_at: Some("2026-03-05T04:44:59Z".to_string()),
                stack_id: None,
                service_id: None,
            },
            links,
            human: JobNotificationHumanV2 {
                title: "Dockrev：更新完成（成功）".to_string(),
                summary: "变更 1 个服务（blog / api）。".to_string(),
                detail: "test".to_string(),
            },
            debug: JobNotificationDebugV2 {
                app_version: "0.1.0".to_string(),
                source: "dockrev-api",
            },
        }
    }

    fn make_service_url(i: usize) -> JobNotificationServiceUrlV2 {
        JobNotificationServiceUrlV2 {
            stack_id: format!("stk_{i}"),
            stack_name: format!("stack-{i}"),
            service_id: format!("svc_{i}"),
            service_name: format!("service-{i}"),
            url: format!("https://dockrev.example.com/services/stk_{i}/svc_{i}"),
        }
    }

    #[test]
    fn job_notification_links_single_service_prefers_service_url() {
        let job_url = "https://dockrev.example.com/queue/job_123".to_string();
        let links = finalize_job_links(job_url.clone(), vec![make_service_url(1)], false, None);
        assert_eq!(links.primary_url, links.service_urls[0].url);
        assert_ne!(links.primary_url, job_url);
    }

    #[test]
    fn job_notification_links_multi_service_prefers_job_url() {
        let job_url = "https://dockrev.example.com/queue/job_123".to_string();
        let links = finalize_job_links(
            job_url.clone(),
            vec![make_service_url(1), make_service_url(2)],
            false,
            None,
        );
        assert_eq!(links.primary_url, job_url);
    }

    #[test]
    fn service_urls_truncation_sets_omitted_count() {
        let job_url = "https://dockrev.example.com/queue/job_123".to_string();
        let service_urls = (0..(MAX_JOB_SERVICE_URLS + 3))
            .map(make_service_url)
            .collect::<Vec<_>>();
        let links = finalize_job_links(job_url, service_urls, false, None);
        assert_eq!(links.service_urls.len(), MAX_JOB_SERVICE_URLS);
        assert_eq!(links.truncated.service_urls_omitted, 3);
    }

    #[test]
    fn update_summary_includes_service_names_for_multi() {
        let services = vec![
            make_service_url(1),
            make_service_url(2),
            make_service_url(3),
        ];
        let summary = summarize_updated_services(&services, 0);
        assert!(summary.starts_with("变更 3 个服务："));
        assert!(summary.contains("stack-1 / service-1"));
        assert!(summary.contains("stack-2 / service-2"));
        assert!(summary.contains("stack-3 / service-3"));
    }

    #[test]
    fn update_summary_marks_omitted_and_visible_limit() {
        let services = vec![
            make_service_url(1),
            make_service_url(2),
            make_service_url(3),
        ];
        let summary = summarize_updated_services(&services, 9);
        assert!(summary.contains("变更 12 个服务"));
        assert!(summary.contains("stack-1 / service-1"));
        assert!(summary.contains("stack-2 / service-2"));
        assert!(summary.contains("stack-3 / service-3"));
        assert!(summary.contains("仅展示前 3 条"));
    }

    #[test]
    fn telegram_render_contains_clickable_service_links() {
        let job_url = "https://dockrev.example.com/queue/job_123".to_string();
        let links = finalize_job_links(job_url, vec![make_service_url(1)], false, None);
        let payload = sample_job_payload(links);
        let html = render_telegram_job_html(&payload, None);
        assert!(html.contains(
            "<b>Dockrev：更新完成（成功）</b> <a href=\"https://dockrev.example.com/services/stk_1/svc_1\">详情</a>"
        ));
        assert!(html.contains("<a href=\"https://dockrev.example.com/services/stk_1/svc_1\">"));
        assert!(html.contains("<b>服务清单</b>"));
        assert!(!html.contains("任务详情："));
        assert!(!html.contains("打开服务详情："));
        assert!(!html.contains("Dockrev notification:"));
    }

    #[test]
    fn telegram_job_title_line_keeps_detail_suffix_without_base_url() {
        let links = JobNotificationLinksV2 {
            primary_url: "services/stk_1/svc_1".to_string(),
            job_url: "queue/job_123".to_string(),
            service_urls: vec![JobNotificationServiceUrlV2 {
                stack_id: "stk_1".to_string(),
                stack_name: "blog".to_string(),
                service_id: "svc_1".to_string(),
                service_name: "api".to_string(),
                url: "services/stk_1/svc_1".to_string(),
            }],
            truncated: JobNotificationTruncatedV2 {
                service_urls_omitted: 0,
            },
        };
        let payload = sample_job_payload(links);
        let html = render_telegram_job_html(&payload, None);
        assert!(
            html.contains("<b>Dockrev：更新完成（成功）</b> <code>services/stk_1/svc_1</code>")
        );
        assert!(!html.contains("\n详情："));
    }

    fn sample_new_version_payload() -> NewVersionNotificationPayloadV2 {
        NewVersionNotificationPayloadV2 {
            schema: "dockrev.notification.new_version_discovered.v2",
            kind: "new_version_discovered",
            sent_at: "2026-03-05T04:44:59Z".to_string(),
            channel: "telegram",
            check: NewVersionNotificationCheckV2 {
                job_id: "job_check_123".to_string(),
                status: "success".to_string(),
                scope: "all".to_string(),
                services_checked: 12,
                new_versions: 1,
            },
            links: NewVersionNotificationLinksV2 {
                primary_url: "https://dockrev.example.com/services/stk_1/svc_1".to_string(),
                job_url: "https://dockrev.example.com/queue/job_check_123".to_string(),
                service_urls: vec![NewVersionNotificationServiceUrlV2 {
                    stack_id: "stk_1".to_string(),
                    stack_name: "blog".to_string(),
                    service_id: "svc_1".to_string(),
                    service_name: "api".to_string(),
                    current_tag: Some("latest".to_string()),
                    current_display_tag: Some("1.0.0".to_string()),
                    candidate_tag: Some("latest".to_string()),
                    candidate_display_tag: Some("1.1.0".to_string()),
                    url: "https://dockrev.example.com/services/stk_1/svc_1".to_string(),
                }],
                truncated: JobNotificationTruncatedV2 {
                    service_urls_omitted: 0,
                },
            },
            human: JobNotificationHumanV2 {
                title: "Dockrev：发现新版本".to_string(),
                summary: "blog / api 服务有新版本（1.0.0 -> 1.1.0）。".to_string(),
                detail: "test".to_string(),
            },
            debug: JobNotificationDebugV2 {
                app_version: "0.1.0".to_string(),
                source: "dockrev-api",
            },
        }
    }

    fn make_new_version_service(
        stack_name: &str,
        service_name: &str,
    ) -> NewVersionNotificationServiceUrlV2 {
        NewVersionNotificationServiceUrlV2 {
            stack_id: format!("stk_{stack_name}"),
            stack_name: stack_name.to_string(),
            service_id: format!("svc_{service_name}"),
            service_name: service_name.to_string(),
            current_tag: Some("v1.0.0".to_string()),
            current_display_tag: Some("1.0.0".to_string()),
            candidate_tag: Some("v1.1.0".to_string()),
            candidate_display_tag: Some("1.1.0".to_string()),
            url: format!(
                "https://dockrev.example.com/services/stk_{stack_name}/svc_{service_name}"
            ),
        }
    }

    #[test]
    fn new_version_summary_includes_service_names_for_multi() {
        let services = vec![
            make_new_version_service("blog", "api"),
            make_new_version_service("blog", "worker"),
            make_new_version_service("shop", "gateway"),
        ];
        let summary = summarize_new_version_services(3, &services, 0);
        assert!(summary.contains("blog / api"));
        assert!(summary.contains("blog / worker"));
        assert!(summary.contains("shop / gateway"));
        assert!(summary.starts_with("发现 3 个服务有新版本："));
    }

    #[test]
    fn new_version_summary_marks_omitted_and_preview() {
        let services = vec![
            make_new_version_service("blog", "api"),
            make_new_version_service("blog", "worker"),
            make_new_version_service("shop", "gateway"),
            make_new_version_service("shop", "sync"),
        ];
        let summary = summarize_new_version_services(14, &services, 10);
        assert!(summary.contains("blog / api"));
        assert!(summary.contains("blog / worker"));
        assert!(summary.contains("shop / gateway"));
        assert!(summary.contains("shop / sync"));
        assert!(summary.contains("仅展示前 4 条"));
    }

    #[test]
    fn new_version_summary_single_service_omits_raw_only_transition() {
        let services = vec![NewVersionNotificationServiceUrlV2 {
            stack_id: "stk_blog".to_string(),
            stack_name: "blog".to_string(),
            service_id: "svc_api".to_string(),
            service_name: "api".to_string(),
            current_tag: Some("latest".to_string()),
            current_display_tag: Some("latest".to_string()),
            candidate_tag: Some("latest".to_string()),
            candidate_display_tag: Some("latest".to_string()),
            url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
        }];
        let summary = summarize_new_version_services(1, &services, 0);
        assert_eq!(summary, "blog / api 服务有新版本。");
    }

    #[test]
    fn new_version_summary_single_service_allows_resolved_and_raw_mix() {
        let services = vec![NewVersionNotificationServiceUrlV2 {
            stack_id: "stk_blog".to_string(),
            stack_name: "blog".to_string(),
            service_id: "svc_api".to_string(),
            service_name: "api".to_string(),
            current_tag: Some("latest".to_string()),
            current_display_tag: Some("latest".to_string()),
            candidate_tag: Some("latest".to_string()),
            candidate_display_tag: Some("1.1.0".to_string()),
            url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
        }];
        let summary = summarize_new_version_services(1, &services, 0);
        assert_eq!(summary, "blog / api 服务有新版本（latest -> 1.1.0）。");
    }

    #[test]
    fn new_version_summary_keeps_parseable_non_strict_transitions() {
        let services = vec![NewVersionNotificationServiceUrlV2 {
            stack_id: "stk_blog".to_string(),
            stack_name: "blog".to_string(),
            service_id: "svc_api".to_string(),
            service_name: "api".to_string(),
            current_tag: Some("15-alpine".to_string()),
            current_display_tag: Some("15-alpine".to_string()),
            candidate_tag: Some("16-alpine".to_string()),
            candidate_display_tag: Some("16-alpine".to_string()),
            url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
        }];
        let summary = summarize_new_version_services(1, &services, 0);
        assert_eq!(
            summary,
            "blog / api 服务有新版本（15-alpine -> 16-alpine）。"
        );
    }

    #[test]
    fn best_notification_display_tag_keeps_existing_resolved_before_stale_snapshot() {
        let display =
            best_notification_display_tag("latest", &[Some("5.2.0"), Some("5.2.0"), Some("5.1.0")]);
        assert_eq!(display, "5.2.0");
    }

    #[test]
    fn new_version_summary_multi_service_omits_raw_only_transition_per_item() {
        let services = vec![
            NewVersionNotificationServiceUrlV2 {
                stack_id: "stk_blog".to_string(),
                stack_name: "blog".to_string(),
                service_id: "svc_api".to_string(),
                service_name: "api".to_string(),
                current_tag: Some("latest".to_string()),
                current_display_tag: Some("latest".to_string()),
                candidate_tag: Some("latest".to_string()),
                candidate_display_tag: Some("latest".to_string()),
                url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
            },
            make_new_version_service("shop", "gateway"),
        ];
        let summary = summarize_new_version_services(2, &services, 0);
        assert!(summary.contains("blog / api"));
        assert!(!summary.contains("blog / api（"));
        assert!(summary.contains("shop / gateway (1.0.0 -> 1.1.0)"));
    }

    fn sample_ghcr_anomaly_payload() -> GhcrWebhookAnomalyPayloadV2 {
        GhcrWebhookAnomalyPayloadV2 {
            schema: "dockrev.notification.ghcr_webhook_anomaly.v2",
            kind: "ghcr_webhook_anomaly",
            sent_at: "2026-03-05T04:44:59Z".to_string(),
            channel: "telegram",
            job: GhcrWebhookAnomalyJobV2 {
                id: "job_ghcr_123".to_string(),
                status: "failed".to_string(),
                missing: 1,
                conflict: 0,
                error: 1,
                total_anomalies: 2,
            },
            links: GhcrWebhookAnomalyLinksV2 {
                primary_url: "https://dockrev.example.com/queue/job_ghcr_123".to_string(),
                job_url: "https://dockrev.example.com/queue/job_ghcr_123".to_string(),
                settings_url: "https://dockrev.example.com/settings".to_string(),
                repos: vec![
                    GhcrWebhookAnomalyRepoV2 {
                        owner: "acme".to_string(),
                        repo: "api".to_string(),
                        full_name: "acme/api".to_string(),
                        state: "missing".to_string(),
                        last_error: Some("webhook missing".to_string()),
                    },
                    GhcrWebhookAnomalyRepoV2 {
                        owner: "acme".to_string(),
                        repo: "worker".to_string(),
                        full_name: "acme/worker".to_string(),
                        state: "error".to_string(),
                        last_error: Some("github api timeout".to_string()),
                    },
                ],
                truncated: GhcrWebhookAnomalyTruncatedV2 { repos_omitted: 0 },
            },
            human: JobNotificationHumanV2 {
                title: "Dockrev：GitHub Webhook 巡检异常".to_string(),
                summary: "巡检发现 2 个异常仓库：acme/api [missing]、acme/worker [error]。"
                    .to_string(),
                detail: "test".to_string(),
            },
            debug: JobNotificationDebugV2 {
                app_version: "0.1.0".to_string(),
                source: "dockrev-api",
            },
        }
    }

    #[test]
    fn new_version_telegram_render_uses_single_service_action_copy() {
        let payload = sample_new_version_payload();
        let html = render_telegram_new_version_html(&payload);
        assert!(html.contains("<b>Dockrev：发现新版本</b>"));
        assert!(html.contains("blog / api 服务有新版本（1.0.0 -&gt; 1.1.0）。"));
        assert!(
            html.contains(
                "<a href=\"https://dockrev.example.com/services/stk_1/svc_1\">服务详情</a>"
            )
        );
        assert!(!html.contains("<b>服务清单</b>"));
        assert!(!html.contains(">详情</a>"));
    }

    #[test]
    fn new_version_single_service_without_base_url_keeps_service_action() {
        let mut payload = sample_new_version_payload();
        payload.links.primary_url = "services/stk_1/svc_1".to_string();
        payload.links.job_url = "queue/job_check_123".to_string();
        payload.links.service_urls[0].url = "services/stk_1/svc_1".to_string();

        let html = render_telegram_new_version_html(&payload);
        assert!(html.contains("<b>Dockrev：发现新版本</b>"));
        assert!(html.contains("<code>services/stk_1/svc_1</code>"));
        assert!(!html.contains("\n详情："));
        assert!(!html.contains("<b>服务清单</b>"));
    }

    #[test]
    fn new_version_email_render_uses_single_service_action_copy() {
        let payload = sample_new_version_payload();
        let plain = render_email_new_version_plain(&payload);
        let html = render_email_new_version_html(&payload);

        assert!(plain.contains("Dockrev：发现新版本"));
        assert!(plain.contains("blog / api 服务有新版本（1.0.0 -> 1.1.0）。"));
        assert!(plain.contains("服务详情：https://dockrev.example.com/services/stk_1/svc_1"));
        assert!(!plain.contains("服务清单"));
        assert!(!plain.contains("检查任务："));

        assert!(html.contains("<h2>Dockrev：发现新版本</h2>"));
        assert!(html.contains("blog / api 服务有新版本（1.0.0 -&gt; 1.1.0）。"));
        assert!(
            html.contains(
                "<a href=\"https://dockrev.example.com/services/stk_1/svc_1\">服务详情</a>"
            )
        );
        assert!(!html.contains("服务清单"));
        assert!(!html.contains("检查任务："));
    }

    #[test]
    fn ghcr_anomaly_telegram_render_contains_repo_state() {
        let payload = sample_ghcr_anomaly_payload();
        let html = render_telegram_ghcr_webhook_anomaly_html(&payload);
        assert!(html.contains(
            "<b>Dockrev：GitHub Webhook 巡检异常</b> <a href=\"https://dockrev.example.com/queue/job_ghcr_123\">任务</a>"
        ));
        assert!(html.contains("acme/api"));
        assert!(html.contains("acme/worker"));
        assert!(html.contains("missing"));
        assert!(html.contains("webhook missing"));
        assert!(!html.contains("巡检任务："));
        assert!(!html.contains("打开设置"));
    }

    #[test]
    fn ghcr_anomaly_title_line_keeps_task_suffix_without_base_url() {
        let mut payload = sample_ghcr_anomaly_payload();
        payload.links.primary_url = "queue/job_ghcr_123".to_string();
        payload.links.job_url = "queue/job_ghcr_123".to_string();
        payload.links.settings_url = "settings".to_string();

        let html = render_telegram_ghcr_webhook_anomaly_html(&payload);
        assert!(
            html.contains(
                "<b>Dockrev：GitHub Webhook 巡检异常</b> <code>queue/job_ghcr_123</code>"
            )
        );
        assert!(!html.contains("\n任务："));
    }

    #[test]
    fn ghcr_anomaly_summary_includes_repo_names() {
        let repos = vec![
            GhcrWebhookAnomalyRepoV2 {
                owner: "acme".to_string(),
                repo: "api".to_string(),
                full_name: "acme/api".to_string(),
                state: "missing".to_string(),
                last_error: None,
            },
            GhcrWebhookAnomalyRepoV2 {
                owner: "acme".to_string(),
                repo: "worker".to_string(),
                full_name: "acme/worker".to_string(),
                state: "error".to_string(),
                last_error: None,
            },
        ];
        let summary = summarize_ghcr_anomaly_repos(2, &repos, 0);
        assert!(summary.contains("acme/api [missing]"));
        assert!(summary.contains("acme/worker [error]"));
        assert!(!summary.contains("missing="));
    }

    #[test]
    fn ghcr_anomaly_summary_marks_omitted_and_visible_limit() {
        let repos = vec![
            GhcrWebhookAnomalyRepoV2 {
                owner: "acme".to_string(),
                repo: "api".to_string(),
                full_name: "acme/api".to_string(),
                state: "missing".to_string(),
                last_error: None,
            },
            GhcrWebhookAnomalyRepoV2 {
                owner: "acme".to_string(),
                repo: "worker".to_string(),
                full_name: "acme/worker".to_string(),
                state: "error".to_string(),
                last_error: None,
            },
            GhcrWebhookAnomalyRepoV2 {
                owner: "acme".to_string(),
                repo: "sync".to_string(),
                full_name: "acme/sync".to_string(),
                state: "conflict".to_string(),
                last_error: None,
            },
        ];
        let summary = summarize_ghcr_anomaly_repos(14, &repos, 11);
        assert!(summary.contains("acme/api [missing]"));
        assert!(summary.contains("acme/worker [error]"));
        assert!(summary.contains("acme/sync [conflict]"));
        assert!(summary.contains("仅展示前 3 条"));
    }

    #[test]
    fn web_push_payload_contains_url_for_new_notifications() {
        let new_version_payload = sample_new_version_payload();
        let new_version_value = to_web_push_new_version_value(&new_version_payload).unwrap();
        assert_eq!(
            new_version_value["url"].as_str(),
            Some("https://dockrev.example.com/services/stk_1/svc_1")
        );
        assert_eq!(
            new_version_value["body"].as_str(),
            Some("blog / api 服务有新版本（1.0.0 -> 1.1.0）。")
        );

        let ghcr_payload = sample_ghcr_anomaly_payload();
        let ghcr_value = to_web_push_ghcr_webhook_anomaly_value(&ghcr_payload).unwrap();
        assert_eq!(
            ghcr_value["url"].as_str(),
            Some("https://dockrev.example.com/queue/job_ghcr_123")
        );
        assert_eq!(
            ghcr_value["body"].as_str(),
            Some(
                "巡检发现 2 个异常仓库：acme/api [missing]、acme/worker [error]。
点击通知查看详情"
            )
        );
    }

    #[test]
    fn event_toggle_flags_are_checked_per_type() {
        let settings = NotificationSettings {
            email_enabled: false,
            email_smtp_url: None,
            webhook_enabled: false,
            webhook_url: None,
            telegram_enabled: false,
            telegram_bot_token: None,
            telegram_chat_id: None,
            webpush_enabled: false,
            webpush_vapid_public_key: None,
            webpush_vapid_private_key: None,
            webpush_vapid_subject: None,
            event_update_enabled: true,
            event_new_version_enabled: false,
            event_ghcr_webhook_anomaly_enabled: true,
        };
        assert!(is_event_enabled(&settings, NotificationEventKind::Update));
        assert!(!is_event_enabled(
            &settings,
            NotificationEventKind::NewVersionDiscovered
        ));
        assert!(is_event_enabled(
            &settings,
            NotificationEventKind::GhcrWebhookAnomaly
        ));
    }

    #[test]
    fn error_excerpt_skips_stacks_without_update_block() {
        let summary = json!({
            "stacks": [
                { "stackId": "stk_empty" },
                { "stackId": "stk_err", "update": { "error": "registry timeout" } }
            ]
        });
        let excerpt = extract_error_excerpt(&summary);
        assert_eq!(excerpt.as_deref(), Some("registry timeout"));
    }
}
