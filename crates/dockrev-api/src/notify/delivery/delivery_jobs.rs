use super::*;

pub(crate) async fn send_telegram_job(
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

pub(crate) async fn send_email_job(
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

pub(crate) async fn send_telegram_new_version(
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

pub(crate) async fn send_email_new_version(
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

pub(crate) async fn send_telegram_ghcr_webhook_anomaly(
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

pub(crate) async fn send_email_ghcr_webhook_anomaly(
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
