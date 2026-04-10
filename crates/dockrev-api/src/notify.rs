use std::{borrow::Cow, time::Duration};

use anyhow::Context as _;
use dockrev_common::normalized_semver_from_oci_version;
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

mod delivery;

#[cfg(test)]
use delivery::*;
use delivery::{send_all, send_ghcr_webhook_anomaly, send_new_versions};

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
const NEW_VERSION_NOTIFY_SETTLE_TIMEOUT: Duration = Duration::from_secs(10);

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
    let dispatch_now_rfc3339 = notification_now_rfc3339(now_rfc3339);

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
            created_at: dispatch_now_rfc3339.clone(),
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
            &dispatch_now_rfc3339,
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
                .finalize_new_version_notification(
                    &item.record_id,
                    &[],
                    None,
                    &dispatch_now_rfc3339,
                )
                .await?;
        }
    }

    if sendable_reserved.is_empty() {
        log_new_version_notification_skip(
            state,
            check_job_id,
            &dispatch_now_rfc3339,
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
        &dispatch_now_rfc3339,
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
                        &dispatch_now_rfc3339,
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
                &dispatch_now_rfc3339,
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
    pub current_digest: Option<String>,
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
        let current_digest = item
            .get("currentDigest")
            .and_then(|v| v.as_str())
            .map(str::to_string);
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
            current_digest,
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
    current_snapshot_ready: bool,
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
        load_new_version_notification_settle_targets(state, discovered_services, &host_platform)
            .await?;
    let after_id = state.snapshot_worker.latest_event_id().await;
    let (settled, pending_keys) = settle_new_version_discovered_services_once(
        state,
        discovered_services,
        &settle_targets,
        &host_platform,
    )
    .await?;
    if pending_keys.is_empty() {
        return Ok(settled);
    }

    let pending_keys = pending_keys.into_iter().collect::<Vec<_>>();
    let _outcomes = state
        .snapshot_worker
        .wait_for_task_finished_keys_since(
            after_id,
            &pending_keys,
            NEW_VERSION_NOTIFY_SETTLE_TIMEOUT,
        )
        .await;

    settle_new_version_discovered_services_once(
        state,
        discovered_services,
        &settle_targets,
        &host_platform,
    )
    .await
    .map(|(settled, _pending_keys)| settled)
}

async fn load_notification_snapshot_ready(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
) -> anyhow::Result<Option<bool>> {
    let Some((snapshot_json, checked_at, _updated_at)) = state
        .db
        .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some(
        notification_snapshot_is_ready_from_row(&snapshot_json, &checked_at).unwrap_or(false),
    ))
}

async fn load_new_version_notification_settle_targets(
    state: &AppState,
    discovered_services: &[NewVersionDiscoveredService],
    host_platform: &str,
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
        let target = if let Some(service) = stack
            .as_ref()
            .and_then(|stack| stack.services.iter().find(|svc| svc.id == item.service_id))
        {
            let image_repo = image_repo.clone().or_else(|| {
                crate::snapshot_worker::image_repo_from_image_ref(&service.image.reference)
            });
            let live_current_digest = service
                .image
                .digest
                .as_deref()
                .and_then(crate::snapshot_worker::normalize_digest);
            let frozen_current_digest = item
                .current_digest
                .as_deref()
                .and_then(crate::snapshot_worker::normalize_digest)
                .or_else(|| live_current_digest.clone());
            let current_snapshot_ready = if let (Some(image_repo), Some(current_digest)) =
                (image_repo.as_deref(), frozen_current_digest.as_deref())
            {
                load_notification_snapshot_ready(state, image_repo, current_digest, host_platform)
                    .await?
                    .unwrap_or(false)
            } else {
                false
            };
            let current_digest_matches_live = live_current_digest == frozen_current_digest;
            NewVersionNotificationSettleTarget {
                image_repo,
                current_digest: frozen_current_digest,
                current_snapshot_ready,
                current_resolved_tag: service
                    .image
                    .resolved_tag
                    .clone()
                    .filter(|_| current_snapshot_ready)
                    .filter(|_| current_digest_matches_live),
                candidate_resolved_tag: service.candidate.as_ref().and_then(|candidate| {
                    crate::snapshot_worker::normalize_digest(&candidate.digest)
                        .filter(|digest| digest == item.candidate_digest.as_str())
                        .and_then(|_| candidate.resolved_tag.clone())
                }),
            }
        } else {
            NewVersionNotificationSettleTarget {
                image_repo,
                ..NewVersionNotificationSettleTarget::default()
            }
        };
        out.insert(item.service_id.clone(), target);
    }

    Ok(out)
}

async fn settle_new_version_discovered_services_once(
    state: &AppState,
    discovered_services: &[NewVersionDiscoveredService],
    settle_targets: &std::collections::HashMap<String, NewVersionNotificationSettleTarget>,
    host_platform: &str,
) -> anyhow::Result<(
    Vec<NewVersionDiscoveredService>,
    std::collections::BTreeSet<String>,
)> {
    let mut pending_keys = std::collections::BTreeSet::<String>::new();
    let mut settled = Vec::with_capacity(discovered_services.len());

    for item in discovered_services {
        let target = settle_targets.get(&item.service_id);
        let image_repo = target.and_then(|target| target.image_repo.as_deref());
        let (current_display_tag, current_pending_key) = settle_new_version_display_tag(
            state,
            image_repo,
            &item.current_tag,
            target.and_then(|target| target.current_resolved_tag.as_deref()),
            target.and_then(|target| {
                target
                    .current_snapshot_ready
                    .then_some(item.current_display_tag.as_str())
            }),
            target.and_then(|target| target.current_digest.as_deref()),
            host_platform,
        )
        .await?;
        let (candidate_display_tag, candidate_pending_key) = settle_new_version_display_tag(
            state,
            image_repo,
            &item.candidate_tag,
            target.and_then(|target| target.candidate_resolved_tag.as_deref()),
            Some(item.candidate_display_tag.as_str()),
            Some(item.candidate_digest.as_str()),
            host_platform,
        )
        .await?;
        if let Some(key) = current_pending_key {
            pending_keys.insert(key);
        }
        if let Some(key) = candidate_pending_key {
            pending_keys.insert(key);
        }
        settled.push(NewVersionDiscoveredService {
            current_display_tag,
            candidate_display_tag,
            ..item.clone()
        });
    }

    Ok((settled, pending_keys))
}

async fn settle_new_version_display_tag(
    state: &AppState,
    image_repo: Option<&str>,
    raw_tag: &str,
    existing_resolved_tag: Option<&str>,
    existing_display_tag: Option<&str>,
    digest: Option<&str>,
    host_platform: &str,
) -> anyhow::Result<(String, Option<String>)> {
    let raw_tag = raw_tag.trim();
    let digest = digest.map(str::trim).filter(|digest| !digest.is_empty());
    let image_repo = image_repo.map(str::trim).filter(|repo| !repo.is_empty());
    let stable_display = preferred_notification_display_tag(
        raw_tag,
        existing_display_tag,
        None,
        existing_resolved_tag,
        None,
    );
    let needs_inference = notification_tag_requires_settle(raw_tag, &stable_display);
    if !needs_inference {
        return Ok((stable_display, None));
    }

    let mut inferred = None;
    let mut explicit_version = None;
    let mut snapshot_ready = false;
    if needs_inference && let (Some(image_repo), Some(digest)) = (image_repo, digest) {
        let snapshot_result = infer_notification_display_tag_from_snapshot(
            state,
            image_repo,
            digest,
            host_platform,
            raw_tag,
        )
        .await?;
        inferred = snapshot_result.display_tag;
        snapshot_ready = snapshot_result.ready;
        if snapshot_ready && inferred.is_none() {
            explicit_version = infer_notification_explicit_version_from_registry(
                state,
                image_repo,
                digest,
                host_platform,
            )
            .await;
        }
    }

    let display = preferred_notification_display_tag(
        raw_tag,
        existing_display_tag,
        inferred.as_deref(),
        existing_resolved_tag,
        explicit_version.as_deref(),
    );
    let pending_key = if !snapshot_ready {
        match (image_repo, digest) {
            (Some(image_repo), Some(digest))
                if state
                    .snapshot_worker
                    .in_flight_reason(image_repo, digest, host_platform)
                    .await
                    .is_some() =>
            {
                crate::snapshot_worker::snapshot_task_key(image_repo, digest, host_platform)
            }
            _ => None,
        }
    } else {
        None
    };
    Ok((display, pending_key))
}

#[derive(Default)]
struct NotificationSnapshotDisplayResult {
    display_tag: Option<String>,
    ready: bool,
}

async fn infer_notification_display_tag_from_snapshot(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
    raw_tag: &str,
) -> anyhow::Result<NotificationSnapshotDisplayResult> {
    let Some((snapshot_json, checked_at, _updated_at)) = state
        .db
        .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
        .await?
    else {
        return Ok(NotificationSnapshotDisplayResult::default());
    };

    let mut snapshot =
        match serde_json::from_str::<ServiceDigestTagsSnapshotResponse>(&snapshot_json) {
            Ok(snapshot) => snapshot,
            Err(_) => return Ok(NotificationSnapshotDisplayResult::default()),
        };
    if snapshot.checked_at.trim().is_empty() {
        snapshot.checked_at = checked_at;
    }
    let ready = notification_snapshot_is_ready(&snapshot, snapshot.checked_at.as_str());
    Ok(NotificationSnapshotDisplayResult {
        display_tag: ready
            .then(|| infer_notification_semver_tag_from_snapshot(&snapshot, raw_tag))
            .flatten(),
        ready,
    })
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

fn preferred_notification_display_tag(
    raw_tag: &str,
    frozen_display_tag: Option<&str>,
    inferred_display_tag: Option<&str>,
    live_resolved_tag: Option<&str>,
    explicit_version_tag: Option<&str>,
) -> String {
    best_notification_display_tag(
        raw_tag,
        &[
            inferred_display_tag,
            frozen_display_tag,
            live_resolved_tag,
            explicit_version_tag,
        ],
    )
}

async fn infer_notification_explicit_version_from_registry(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
) -> Option<String> {
    let image = crate::snapshot_worker::image_ref_from_repo(image_repo)?;
    match state
        .registry
        .get_oci_version(&image, digest, host_platform)
        .await
    {
        Ok(Some(raw_version)) => normalized_semver_from_oci_version(&raw_version),
        Ok(None) => None,
        Err(error) => {
            tracing::debug!(
                image_repo,
                digest,
                host_platform,
                error = %error,
                "notification explicit version lookup failed"
            );
            None
        }
    }
}

fn notification_tag_supports_settle(raw_tag: &str) -> bool {
    crate::api::needs_version_inference_for_tags(raw_tag.trim(), None)
}

pub(crate) fn notification_tag_requires_settle(raw_tag: &str, display_tag: &str) -> bool {
    let raw_tag = raw_tag.trim();
    let display_tag = display_tag.trim();
    if raw_tag.is_empty() || display_tag != raw_tag {
        return false;
    }
    // Only wait for aliases we can plausibly collapse back into a semver-like label.
    notification_tag_supports_settle(raw_tag)
}

fn notification_now_rfc3339(fallback: &str) -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| fallback.to_string())
}

fn notification_snapshot_checked_at_is_older_than(
    checked_at: &str,
    min_age: time::Duration,
) -> bool {
    let parsed =
        time::OffsetDateTime::parse(checked_at, &time::format_description::well_known::Rfc3339);
    let now = time::OffsetDateTime::now_utc();
    match parsed {
        Ok(ts) => now - ts > min_age,
        Err(_) => true,
    }
}

pub(crate) fn notification_snapshot_is_ready(
    snapshot: &ServiceDigestTagsSnapshotResponse,
    checked_at: &str,
) -> bool {
    if notification_snapshot_checked_at_is_older_than(
        checked_at,
        time::Duration::days(crate::snapshot_worker::SNAPSHOT_CACHE_TTL_DAYS),
    ) {
        return false;
    }
    let retryable_all_failed = crate::snapshot_worker::snapshot_is_all_failed(snapshot)
        && notification_snapshot_checked_at_is_older_than(
            checked_at,
            time::Duration::minutes(crate::snapshot_worker::SNAPSHOT_ALL_FAILED_RETRY_MINUTES),
        );
    !retryable_all_failed
}

pub(crate) fn notification_snapshot_is_ready_from_row(
    snapshot_json: &str,
    checked_at: &str,
) -> Option<bool> {
    let mut snapshot =
        serde_json::from_str::<ServiceDigestTagsSnapshotResponse>(snapshot_json).ok()?;
    if snapshot.checked_at.trim().is_empty() {
        snapshot.checked_at = checked_at.to_string();
    }
    Some(notification_snapshot_is_ready(
        &snapshot,
        snapshot.checked_at.as_str(),
    ))
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

#[cfg(test)]
mod tests;
