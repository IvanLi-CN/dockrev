use std::time::Duration;

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
    api::types::{JobLogLine, NotificationTestChannel},
    state::AppState,
};

const MAX_TEST_SUMMARY_CHARS: usize = 512;
const MAX_TEST_DEBUG_RAW_MESSAGE_CHARS: usize = 1024;
const TELEGRAM_MAX_MESSAGE_CHARS: usize = 4096;

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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestNotificationPayloadV2 {
    schema: &'static str,
    kind: &'static str,
    sent_at: String,
    channel: &'static str,
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

fn build_test_payload_v2(
    now_rfc3339: &str,
    raw_message: &str,
    requested_channel: Option<NotificationTestChannel>,
    channel: NotificationTestChannel,
    app_version: &str,
) -> TestNotificationPayloadV2 {
    let channel_label = notification_channel_label(channel);
    let summary = normalize_test_message(raw_message);
    TestNotificationPayloadV2 {
        schema: "dockrev.notification.test.v2",
        kind: "notification_test",
        sent_at: now_rfc3339.to_string(),
        channel: notification_channel_key(channel),
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

async fn send_all(
    state: &AppState,
    job_id: Option<&str>,
    now_rfc3339: &str,
    payload: Option<&Value>,
    mode: NotifySendMode,
) -> anyhow::Result<Value> {
    let settings = state.db.get_notification_settings().await?;
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
                let payload = payload.context("notify payload missing for default mode")?;
                send_webhook(&client, settings.webhook_url.as_deref(), payload).await
            }
            NotifySendMode::Test { channel, message } => {
                let test_payload = build_test_payload_v2(
                    now_rfc3339,
                    message,
                    *channel,
                    NotificationTestChannel::Webhook,
                    &state.config.app_effective_version,
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
                let payload = payload.context("notify payload missing for default mode")?;
                send_telegram(
                    &client,
                    settings.telegram_bot_token.as_deref(),
                    settings.telegram_chat_id.as_deref(),
                    payload,
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
                let payload = payload.context("notify payload missing for default mode")?;
                send_email(settings.email_smtp_url.as_deref(), payload).await
            }
            NotifySendMode::Test { channel, message } => {
                let test_payload = build_test_payload_v2(
                    now_rfc3339,
                    message,
                    *channel,
                    NotificationTestChannel::Email,
                    &state.config.app_effective_version,
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
                let payload = payload.context("notify payload missing for default mode")?;
                send_web_push(
                    state,
                    settings.webpush_vapid_private_key.as_deref(),
                    settings.webpush_vapid_subject.as_deref(),
                    payload,
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

async fn send_telegram(
    client: &reqwest::Client,
    bot_token: Option<&str>,
    chat_id: Option<&str>,
    payload: &Value,
) -> anyhow::Result<()> {
    let token = bot_token.context("telegram.botToken missing")?;
    let chat_id = chat_id.context("telegram.chatId missing")?;
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let text = format!("Dockrev notification: {}", serde_json::to_string(payload)?);
    let resp = client
        .post(url)
        .json(&json!({ "chat_id": chat_id, "text": text }))
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("telegram http {}: {}", status, body));
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

async fn send_email(smtp_url: Option<&str>, payload: &Value) -> anyhow::Result<()> {
    let smtp_url = smtp_url.context("email.smtpUrl missing")?;
    let (dsn, from, to) = parse_smtp_dsn(smtp_url)?;

    let subject = "[dockrev] notification";
    let body = serde_json::to_string_pretty(payload)?;

    let mut builder = Message::builder()
        .from(from)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN);
    for addr in to {
        builder = builder.to(addr);
    }
    let email = builder.body(body)?;

    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::from_url(&dsn)?.build();
    mailer.send(email).await?;
    Ok(())
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
        );
        let value = to_value(&payload).unwrap();

        assert_eq!(
            value["schema"].as_str(),
            Some("dockrev.notification.test.v2")
        );
        assert_eq!(value["kind"].as_str(), Some("notification_test"));
        assert_eq!(value["channel"].as_str(), Some("telegram"));
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
        );
        let value = to_web_push_value(&payload).unwrap();
        let body = value["body"].as_str().unwrap_or_default();
        assert!(!body.contains("```"));
        assert!(!body.contains("<pre>"));
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
        );
        let plain = render_telegram_plain_for_send(&payload).unwrap();
        assert!(plain.chars().count() <= TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32));
    }
}
