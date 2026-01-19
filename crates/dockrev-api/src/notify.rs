use std::time::Duration;

use anyhow::Context as _;
use lettre::{
    AsyncSmtpTransport, AsyncTransport as _, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};
use serde_json::{Value, json};
use url::Url;
use web_push::{
    ContentEncoding, HyperWebPushClient, SubscriptionInfo, Urgency, VapidSignatureBuilder,
    WebPushClient as _, WebPushError, WebPushMessageBuilder,
};

use crate::{api::types::JobLogLine, state::AppState};

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
    send_all(state, Some(job_id), now_rfc3339, &payload).await?;
    Ok(())
}

pub async fn send_test(
    state: &AppState,
    now_rfc3339: &str,
    message: &str,
) -> anyhow::Result<Value> {
    let payload = json!({
        "type": "test",
        "ts": now_rfc3339,
        "message": message,
    });
    let results = send_all(state, None, now_rfc3339, &payload).await?;
    Ok(results)
}

async fn send_all(
    state: &AppState,
    job_id: Option<&str>,
    now_rfc3339: &str,
    payload: &Value,
) -> anyhow::Result<Value> {
    let settings = state.db.get_notification_settings().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .context("build reqwest client")?;

    let mut results = serde_json::Map::new();

    if settings.webhook_enabled {
        let r = send_webhook(&client, settings.webhook_url.as_deref(), payload).await;
        log_result(state, job_id, now_rfc3339, "webhook", &r).await;
        results.insert("webhook".to_string(), result_value(r));
    }

    if settings.telegram_enabled {
        let r = send_telegram(
            &client,
            settings.telegram_bot_token.as_deref(),
            settings.telegram_chat_id.as_deref(),
            payload,
        )
        .await;
        log_result(state, job_id, now_rfc3339, "telegram", &r).await;
        results.insert("telegram".to_string(), result_value(r));
    }

    if settings.email_enabled {
        let r = send_email(settings.email_smtp_url.as_deref(), payload).await;
        log_result(state, job_id, now_rfc3339, "email", &r).await;
        results.insert("email".to_string(), result_value(r));
    }

    if settings.webpush_enabled {
        let r = send_web_push(
            state,
            settings.webpush_vapid_private_key.as_deref(),
            settings.webpush_vapid_subject.as_deref(),
            payload,
        )
        .await;
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
}
