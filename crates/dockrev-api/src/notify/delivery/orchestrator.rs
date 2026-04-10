use super::*;

pub(crate) async fn send_new_versions(
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

pub(crate) async fn send_ghcr_webhook_anomaly(
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

pub(crate) async fn send_all(
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

pub(crate) fn parse_smtp_dsn(smtp_url: &str) -> anyhow::Result<(String, Mailbox, Vec<Mailbox>)> {
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
