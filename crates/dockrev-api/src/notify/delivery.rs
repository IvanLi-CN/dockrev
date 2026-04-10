use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobNotificationPayloadV2 {
    pub(super) schema: &'static str,
    pub(super) kind: &'static str,
    pub(super) sent_at: String,
    pub(super) channel: &'static str,
    pub(super) job: JobNotificationJobV2,
    pub(super) links: JobNotificationLinksV2,
    pub(super) human: JobNotificationHumanV2,
    pub(super) debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobNotificationJobV2 {
    pub(super) id: String,
    #[serde(rename = "type")]
    pub(super) r#type: String,
    pub(super) scope: String,
    pub(super) status: String,
    pub(super) reason: String,
    pub(super) created_by: String,
    pub(super) created_at: String,
    #[serde(default)]
    pub(super) started_at: Option<String>,
    #[serde(default)]
    pub(super) finished_at: Option<String>,
    #[serde(default)]
    pub(super) stack_id: Option<String>,
    #[serde(default)]
    pub(super) service_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobNotificationLinksV2 {
    pub(super) primary_url: String,
    pub(super) job_url: String,
    pub(super) service_urls: Vec<JobNotificationServiceUrlV2>,
    pub(super) truncated: JobNotificationTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobNotificationServiceUrlV2 {
    pub(super) stack_id: String,
    pub(super) stack_name: String,
    pub(super) service_id: String,
    pub(super) service_name: String,
    pub(super) url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobNotificationTruncatedV2 {
    pub(super) service_urls_omitted: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobNotificationHumanV2 {
    pub(super) title: String,
    pub(super) summary: String,
    pub(super) detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobNotificationDebugV2 {
    pub(super) app_version: String,
    pub(super) source: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NewVersionNotificationPayloadV2 {
    pub(super) schema: &'static str,
    pub(super) kind: &'static str,
    pub(super) sent_at: String,
    pub(super) channel: &'static str,
    pub(super) check: NewVersionNotificationCheckV2,
    pub(super) links: NewVersionNotificationLinksV2,
    pub(super) human: JobNotificationHumanV2,
    pub(super) debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NewVersionNotificationCheckV2 {
    pub(super) job_id: String,
    pub(super) status: String,
    pub(super) scope: String,
    pub(super) services_checked: u32,
    pub(super) new_versions: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NewVersionNotificationLinksV2 {
    pub(super) primary_url: String,
    pub(super) job_url: String,
    pub(super) service_urls: Vec<NewVersionNotificationServiceUrlV2>,
    pub(super) truncated: JobNotificationTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NewVersionNotificationServiceUrlV2 {
    pub(super) stack_id: String,
    pub(super) stack_name: String,
    pub(super) service_id: String,
    pub(super) service_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) current_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) current_display_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) candidate_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) candidate_display_tag: Option<String>,
    pub(super) url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GhcrWebhookAnomalyPayloadV2 {
    pub(super) schema: &'static str,
    pub(super) kind: &'static str,
    pub(super) sent_at: String,
    pub(super) channel: &'static str,
    pub(super) job: GhcrWebhookAnomalyJobV2,
    pub(super) links: GhcrWebhookAnomalyLinksV2,
    pub(super) human: JobNotificationHumanV2,
    pub(super) debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GhcrWebhookAnomalyJobV2 {
    pub(super) id: String,
    pub(super) status: String,
    pub(super) missing: u32,
    pub(super) conflict: u32,
    pub(super) error: u32,
    pub(super) total_anomalies: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GhcrWebhookAnomalyLinksV2 {
    pub(super) primary_url: String,
    pub(super) job_url: String,
    pub(super) settings_url: String,
    pub(super) repos: Vec<GhcrWebhookAnomalyRepoV2>,
    pub(super) truncated: GhcrWebhookAnomalyTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GhcrWebhookAnomalyRepoV2 {
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) full_name: String,
    pub(super) state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GhcrWebhookAnomalyTruncatedV2 {
    pub(super) repos_omitted: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TestNotificationPayloadV2 {
    pub(super) schema: &'static str,
    pub(super) kind: &'static str,
    pub(super) sent_at: String,
    pub(super) channel: &'static str,
    pub(super) url: String,
    pub(super) human: TestNotificationHuman,
    pub(super) debug: TestNotificationDebug,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TestNotificationHuman {
    pub(super) title: String,
    pub(super) summary: String,
    pub(super) detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TestNotificationDebug {
    pub(super) requested_channel: Option<&'static str>,
    pub(super) app_version: String,
    pub(super) source: &'static str,
    pub(super) raw_message: String,
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

pub(super) fn truncate_chars(input: &str, max_chars: usize) -> String {
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
    raw_non_strict: bool,
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
                raw_non_strict: false,
            })
        }
        (_, Some(raw_tag)) if crate::ignore::parse_version(raw_tag).is_some() => {
            Some(NotificationTagDisplay {
                label: raw_tag,
                readable: true,
                raw_non_strict: !crate::ignore::is_strict_semver(raw_tag),
            })
        }
        (_, Some(raw_tag)) => Some(NotificationTagDisplay {
            label: raw_tag,
            readable: false,
            raw_non_strict: !crate::ignore::is_strict_semver(raw_tag),
        }),
        (Some(display_tag), None) => Some(NotificationTagDisplay {
            label: display_tag,
            readable: true,
            raw_non_strict: false,
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
        (Some(current), Some(candidate))
            if current.label == candidate.label
                && current.raw_non_strict
                && candidate.raw_non_strict =>
        {
            None
        }
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

fn headline_new_version_services(
    total_new_versions: usize,
    visible_services: &[NewVersionNotificationServiceUrlV2],
) -> String {
    if total_new_versions == 0 {
        return "发现新版本服务数为 0".to_string();
    }

    if total_new_versions == 1 {
        if let Some(svc) = visible_services.first() {
            return format!("{} / {} 服务有新版本", svc.stack_name, svc.service_name);
        }
        return "发现 1 个服务有新版本".to_string();
    }

    format!("发现 {total_new_versions} 个服务有新版本")
}

pub(super) fn summarize_new_version_services(
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

    if visible_services.is_empty() {
        return format!("发现 {total_new_versions} 个服务有新版本。");
    }

    let mut lines = vec![format!("发现 {total_new_versions} 个服务有新版本：")];
    lines.extend(
        visible_services
            .iter()
            .map(|svc| format!("- {}", render_new_version_service_label(svc))),
    );
    if omitted > 0 {
        lines.push(format!("... 以及其他 {omitted} 个服务（已省略）"));
    }
    lines.join("\n")
}

pub(super) fn summarize_ghcr_anomaly_repos(
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

#[cfg(test)]
pub(super) fn summarize_updated_services(
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

fn summarize_transition_services(
    verb: &str,
    visible_services: &[JobNotificationServiceUrlV2],
    omitted: u32,
) -> String {
    let total_changed = visible_services.len() + omitted as usize;
    if total_changed == 0 {
        return format!("{verb} 0 个服务。");
    }

    if total_changed == 1 {
        if let Some(svc) = visible_services.first() {
            return format!(
                "{verb} 1 个服务（{} / {}）。",
                svc.stack_name, svc.service_name
            );
        }
        return format!("{verb} 1 个服务。");
    }

    let preview = visible_services
        .iter()
        .map(|svc| format!("{} / {}", svc.stack_name, svc.service_name))
        .collect::<Vec<_>>()
        .join("、");

    if preview.is_empty() {
        return format!("{verb} {total_changed} 个服务。");
    }

    if omitted > 0 {
        return format!(
            "{verb} {total_changed} 个服务：{preview}（通知正文仅展示前 {} 条）。",
            visible_services.len()
        );
    }

    format!("{verb} {total_changed} 个服务：{preview}。")
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
        let Some(update) = s.get("update").or_else(|| s.get("rollback")) else {
            continue;
        };
        for service_id in extract_changed_service_ids(update) {
            out.push((stack_id.to_string(), service_id));
        }
    }
    out
}

pub(super) fn extract_error_excerpt(summary: &Value) -> Option<String> {
    if let Some(err) = summary.get("error").and_then(|v| v.as_str()) {
        let trimmed = err.trim();
        if !trimmed.is_empty() {
            return Some(truncate_chars(trimmed, MAX_JOB_ERROR_CHARS));
        }
    }

    let stacks = summary.get("stacks").and_then(|v| v.as_array())?;
    for s in stacks {
        let Some(update) = s.get("update").or_else(|| s.get("rollback")) else {
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

pub(super) fn build_test_payload_v2(
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

pub(super) fn to_value(payload: &TestNotificationPayloadV2) -> anyhow::Result<Value> {
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

pub(super) fn render_telegram_test_html(
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<String> {
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

pub(super) fn to_web_push_value(payload: &TestNotificationPayloadV2) -> anyhow::Result<Value> {
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

pub(super) fn should_retry_telegram_plain_text(status: reqwest::StatusCode, body: &str) -> bool {
    if status != reqwest::StatusCode::BAD_REQUEST {
        return false;
    }
    let body = body.to_ascii_lowercase();
    body.contains("parse entities")
        || body.contains("can't parse entities")
        || body.contains("parse_mode")
}

pub(super) fn render_telegram_plain_for_send(
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<String> {
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

pub(super) fn to_web_push_new_version_value(
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

pub(super) fn to_web_push_ghcr_webhook_anomaly_value(
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

pub(super) fn render_telegram_job_html(
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

fn render_check_job_action_html(url: &str) -> String {
    if is_absolute_http_url(url) {
        format!("检查任务：<a href=\"{}\">检查任务</a>", escape_html(url))
    } else {
        format!("检查任务：<code>{}</code>", escape_html(url))
    }
}

fn render_check_job_action_plain(url: &str) -> String {
    format!("检查任务：{url}")
}

pub(super) fn render_telegram_new_version_html(
    payload: &NewVersionNotificationPayloadV2,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    let single = is_single_new_version_payload(payload);
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

    lines.push(render_check_job_action_html(&payload.links.primary_url));
    lines.join("\n")
}

pub(super) fn render_telegram_new_version_plain(
    payload: &NewVersionNotificationPayloadV2,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    let single = is_single_new_version_payload(payload);
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

    lines.push(render_check_job_action_plain(&payload.links.primary_url));
    lines.join("\n")
}

fn render_telegram_new_version_plain_for_send(payload: &NewVersionNotificationPayloadV2) -> String {
    let plain = render_telegram_new_version_plain(payload);
    truncate_chars(&plain, TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32))
}

pub(super) fn render_email_new_version_plain(payload: &NewVersionNotificationPayloadV2) -> String {
    render_telegram_new_version_plain(payload)
}

pub(super) fn render_email_new_version_html(payload: &NewVersionNotificationPayloadV2) -> String {
    let summary = escape_html(&payload.human.summary).replace('\n', "<br>");
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
        return format!("<p>{summary}</p>{note}<p>{action}</p>");
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

    format!("<p>{summary}</p>{note}<p>检查任务：{check_link}</p>")
}

pub(super) fn render_telegram_ghcr_webhook_anomaly_html(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> String {
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

    let subject = format!("[dockrev] {}", payload.human.title);
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

pub(super) fn finalize_job_links(
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
    let action_noun = if job.r#type == "rollback" {
        "回滚"
    } else {
        "更新"
    };
    let action_verb = if job.r#type == "rollback" {
        "回滚"
    } else {
        "变更"
    };
    let title = if status == "failed" {
        format!("Dockrev：{action_noun}失败")
    } else {
        format!("Dockrev：{action_noun}完成（{status_zh}）")
    };

    let summary = if links.service_urls.is_empty() {
        format!("状态：{status_zh}。")
    } else {
        summarize_transition_services(
            action_verb,
            &links.service_urls,
            links.truncated.service_urls_omitted,
        )
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

    let title = headline_new_version_services(total_new_versions, &service_urls_full);
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
            title,
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

pub(super) async fn send_new_versions(
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

pub(super) async fn send_ghcr_webhook_anomaly(
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

pub(super) async fn send_all(
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

pub(super) fn parse_smtp_dsn(smtp_url: &str) -> anyhow::Result<(String, Mailbox, Vec<Mailbox>)> {
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
