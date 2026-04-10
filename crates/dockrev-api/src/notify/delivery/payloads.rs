use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobNotificationPayloadV2 {
    pub(crate) schema: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) sent_at: String,
    pub(crate) channel: &'static str,
    pub(crate) job: JobNotificationJobV2,
    pub(crate) links: JobNotificationLinksV2,
    pub(crate) human: JobNotificationHumanV2,
    pub(crate) debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobNotificationJobV2 {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) r#type: String,
    pub(crate) scope: String,
    pub(crate) status: String,
    pub(crate) reason: String,
    pub(crate) created_by: String,
    pub(crate) created_at: String,
    #[serde(default)]
    pub(crate) started_at: Option<String>,
    #[serde(default)]
    pub(crate) finished_at: Option<String>,
    #[serde(default)]
    pub(crate) stack_id: Option<String>,
    #[serde(default)]
    pub(crate) service_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobNotificationLinksV2 {
    pub(crate) primary_url: String,
    pub(crate) job_url: String,
    pub(crate) service_urls: Vec<JobNotificationServiceUrlV2>,
    pub(crate) truncated: JobNotificationTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobNotificationServiceUrlV2 {
    pub(crate) stack_id: String,
    pub(crate) stack_name: String,
    pub(crate) service_id: String,
    pub(crate) service_name: String,
    pub(crate) url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobNotificationTruncatedV2 {
    pub(crate) service_urls_omitted: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobNotificationHumanV2 {
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobNotificationDebugV2 {
    pub(crate) app_version: String,
    pub(crate) source: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewVersionNotificationPayloadV2 {
    pub(crate) schema: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) sent_at: String,
    pub(crate) channel: &'static str,
    pub(crate) check: NewVersionNotificationCheckV2,
    pub(crate) links: NewVersionNotificationLinksV2,
    pub(crate) human: JobNotificationHumanV2,
    pub(crate) debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewVersionNotificationCheckV2 {
    pub(crate) job_id: String,
    pub(crate) status: String,
    pub(crate) scope: String,
    pub(crate) services_checked: u32,
    pub(crate) new_versions: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewVersionNotificationLinksV2 {
    pub(crate) primary_url: String,
    pub(crate) job_url: String,
    pub(crate) service_urls: Vec<NewVersionNotificationServiceUrlV2>,
    pub(crate) truncated: JobNotificationTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewVersionNotificationServiceUrlV2 {
    pub(crate) stack_id: String,
    pub(crate) stack_name: String,
    pub(crate) service_id: String,
    pub(crate) service_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) current_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) current_display_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) candidate_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) candidate_display_tag: Option<String>,
    pub(crate) url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GhcrWebhookAnomalyPayloadV2 {
    pub(crate) schema: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) sent_at: String,
    pub(crate) channel: &'static str,
    pub(crate) job: GhcrWebhookAnomalyJobV2,
    pub(crate) links: GhcrWebhookAnomalyLinksV2,
    pub(crate) human: JobNotificationHumanV2,
    pub(crate) debug: JobNotificationDebugV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GhcrWebhookAnomalyJobV2 {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) missing: u32,
    pub(crate) conflict: u32,
    pub(crate) error: u32,
    pub(crate) total_anomalies: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GhcrWebhookAnomalyLinksV2 {
    pub(crate) primary_url: String,
    pub(crate) job_url: String,
    pub(crate) settings_url: String,
    pub(crate) repos: Vec<GhcrWebhookAnomalyRepoV2>,
    pub(crate) truncated: GhcrWebhookAnomalyTruncatedV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GhcrWebhookAnomalyRepoV2 {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) full_name: String,
    pub(crate) state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GhcrWebhookAnomalyTruncatedV2 {
    pub(crate) repos_omitted: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestNotificationPayloadV2 {
    pub(crate) schema: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) sent_at: String,
    pub(crate) channel: &'static str,
    pub(crate) url: String,
    pub(crate) human: TestNotificationHuman,
    pub(crate) debug: TestNotificationDebug,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestNotificationHuman {
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestNotificationDebug {
    pub(crate) requested_channel: Option<&'static str>,
    pub(crate) app_version: String,
    pub(crate) source: &'static str,
    pub(crate) raw_message: String,
}

pub(crate) fn notification_channel_key(channel: NotificationTestChannel) -> &'static str {
    match channel {
        NotificationTestChannel::Email => "email",
        NotificationTestChannel::Webhook => "webhook",
        NotificationTestChannel::Telegram => "telegram",
        NotificationTestChannel::WebPush => "webPush",
    }
}

pub(crate) fn notification_channel_label(channel: NotificationTestChannel) -> &'static str {
    match channel {
        NotificationTestChannel::Email => "Email",
        NotificationTestChannel::Webhook => "Webhook",
        NotificationTestChannel::Telegram => "Telegram",
        NotificationTestChannel::WebPush => "Web Push",
    }
}

pub(crate) fn is_absolute_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub(crate) fn best_effort_url(
    public_base_url: Option<&str>,
    path_no_leading_slash: &str,
) -> String {
    if let Some(base) = public_base_url
        && let Ok(base) = Url::parse(base)
        && let Ok(joined) = base.join(path_no_leading_slash)
    {
        return joined.to_string();
    }
    format!("/{path_no_leading_slash}")
}

pub(crate) fn update_job_status_label_zh(status: &str) -> Cow<'_, str> {
    match status {
        "success" => Cow::Borrowed("成功"),
        "failed" => Cow::Borrowed("失败"),
        "rolled_back" => Cow::Borrowed("已回滚"),
        _ => Cow::Borrowed(status),
    }
}

pub(crate) fn normalize_test_message(raw_message: &str) -> String {
    let trimmed = raw_message.trim();
    let normalized = if trimmed.is_empty() {
        "dockrev test"
    } else {
        trimmed
    };
    truncate_chars(normalized, MAX_TEST_SUMMARY_CHARS)
}

pub(crate) fn truncate_chars(input: &str, max_chars: usize) -> String {
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

pub(crate) struct NotificationTagDisplay<'a> {
    label: &'a str,
    readable: bool,
    raw_non_strict: bool,
}

pub(crate) fn notification_tag_display<'a>(
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

pub(crate) fn render_tag_transition(
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

pub(crate) fn render_new_version_service_label(svc: &NewVersionNotificationServiceUrlV2) -> String {
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

pub(crate) fn headline_new_version_services(
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

pub(crate) fn summarize_new_version_services(
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

pub(crate) fn summarize_ghcr_anomaly_repos(
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
pub(crate) fn summarize_updated_services(
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

pub(crate) fn summarize_transition_services(
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

pub(crate) fn extract_changed_service_ids(update: &Value) -> Vec<String> {
    let obj = update
        .get("newDigests")
        .and_then(|v| v.as_object())
        .or_else(|| update.get("oldDigests").and_then(|v| v.as_object()));
    match obj {
        Some(map) => map.keys().cloned().collect(),
        None => Vec::new(),
    }
}

pub(crate) fn extract_changed_services_by_stack(summary: &Value) -> Vec<(String, String)> {
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

pub(crate) fn extract_error_excerpt(summary: &Value) -> Option<String> {
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

pub(crate) fn build_test_payload_v2(
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

pub(crate) fn to_value(payload: &TestNotificationPayloadV2) -> anyhow::Result<Value> {
    serde_json::to_value(payload).context("serialize test notification payload v2")
}

pub(crate) fn render_debug_json(payload: &TestNotificationPayloadV2) -> anyhow::Result<String> {
    serde_json::to_string_pretty(&payload.debug).context("serialize debug payload")
}

pub(crate) fn escape_html(input: &str) -> String {
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

pub(crate) fn render_telegram_test_html(
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

pub(crate) fn render_telegram_test_plain(
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<String> {
    let debug = render_debug_json(payload)?;
    Ok(format!(
        "{}\n{}\n{}\n\nDebug\n{}",
        payload.human.title, payload.human.summary, payload.human.detail, debug
    ))
}

pub(crate) fn render_email_test_plain(
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<String> {
    let debug = render_debug_json(payload)?;
    Ok(format!(
        "{}\n\n{}\n\n{}\n\nDebug JSON\n```json\n{}\n```",
        payload.human.title, payload.human.summary, payload.human.detail, debug
    ))
}

pub(crate) fn render_email_test_html(
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<String> {
    let debug = render_debug_json(payload)?;
    Ok(format!(
        "<h2>{}</h2><p>{}</p><p>{}</p><h3>Debug JSON</h3><pre><code>{}</code></pre>",
        escape_html(&payload.human.title),
        escape_html(&payload.human.summary),
        escape_html(&payload.human.detail),
        escape_html(&debug)
    ))
}

pub(crate) fn render_web_push_body(payload: &TestNotificationPayloadV2) -> String {
    format!(
        "{}\n{}\nrequestedChannel: {}\nappVersion: {}",
        payload.human.summary,
        payload.human.detail,
        payload.debug.requested_channel.unwrap_or("all"),
        payload.debug.app_version
    )
}

pub(crate) fn to_web_push_value(payload: &TestNotificationPayloadV2) -> anyhow::Result<Value> {
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

pub(crate) fn should_retry_telegram_plain_text(status: reqwest::StatusCode, body: &str) -> bool {
    if status != reqwest::StatusCode::BAD_REQUEST {
        return false;
    }
    let body = body.to_ascii_lowercase();
    body.contains("parse entities")
        || body.contains("can't parse entities")
        || body.contains("parse_mode")
}

pub(crate) fn render_telegram_plain_for_send(
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<String> {
    let plain = render_telegram_test_plain(payload)?;
    Ok(truncate_chars(
        &plain,
        TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32),
    ))
}

pub(crate) fn to_job_value(payload: &JobNotificationPayloadV2) -> anyhow::Result<Value> {
    serde_json::to_value(payload).context("serialize job notification payload v2")
}

pub(crate) fn to_new_version_value(
    payload: &NewVersionNotificationPayloadV2,
) -> anyhow::Result<Value> {
    serde_json::to_value(payload).context("serialize new version notification payload v2")
}

pub(crate) fn to_ghcr_webhook_anomaly_value(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> anyhow::Result<Value> {
    serde_json::to_value(payload).context("serialize ghcr webhook anomaly payload v2")
}

pub(crate) fn to_web_push_job_value(
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

pub(crate) fn to_web_push_new_version_value(
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

pub(crate) fn to_web_push_ghcr_webhook_anomaly_value(
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
