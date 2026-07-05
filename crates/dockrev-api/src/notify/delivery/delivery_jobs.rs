use super::*;

const TELEGRAM_MAX_CAPTION_CHARS: usize = 1024;
const TELEGRAM_CAPTION_SAFE_CHARS: usize = 900;

#[derive(Clone, Debug, Default)]
pub(crate) struct TelegramDeliveryReport {
    pub(crate) photo_fallback_error: Option<String>,
    pub(crate) supplemental_sent: bool,
}

pub(crate) fn telegram_text_message_json(
    chat_id: &str,
    text: &str,
    parse_mode: Option<&str>,
) -> Value {
    let mut value = json!({
        "chat_id": chat_id,
        "text": text,
        "link_preview_options": { "is_disabled": true },
    });
    if let Some(parse_mode) = parse_mode {
        value["parse_mode"] = Value::String(parse_mode.to_string());
    }
    value
}

pub(crate) fn render_telegram_photo_caption_html(
    title: &str,
    summary: &str,
    action: Option<String>,
) -> String {
    let caption = build_telegram_photo_caption_html(title, summary, action.as_deref());
    if caption.chars().count() <= TELEGRAM_MAX_CAPTION_CHARS {
        return caption;
    }

    let title = truncate_for_escaped_html(title, 160);
    let title_html_chars = format!("<b>{}</b>", escape_html(&title)).chars().count();
    let action_chars = action
        .as_ref()
        .map(|value| value.chars().count() + 1)
        .unwrap_or(0);
    let fixed_chars = title_html_chars + 1 + action_chars;
    let (summary_budget, action) = if fixed_chars < TELEGRAM_CAPTION_SAFE_CHARS {
        (TELEGRAM_CAPTION_SAFE_CHARS - fixed_chars, action)
    } else {
        (
            TELEGRAM_CAPTION_SAFE_CHARS.saturating_sub(title_html_chars + 1),
            None,
        )
    };
    let summary = truncate_for_escaped_html(summary, summary_budget);
    build_telegram_photo_caption_html(&title, &summary, action.as_deref())
}

fn build_telegram_photo_caption_html(title: &str, summary: &str, action: Option<&str>) -> String {
    let mut lines = vec![
        format!("<b>{}</b>", escape_html(title)),
        escape_html(summary),
    ];
    if let Some(action) = action {
        lines.push(action.to_string());
    }
    lines.join("\n")
}

fn truncate_for_escaped_html(input: &str, max_escaped_chars: usize) -> String {
    if escape_html(input).chars().count() <= max_escaped_chars {
        return input.to_string();
    }

    let suffix = "... [truncated]";
    let budget = max_escaped_chars.saturating_sub(suffix.chars().count());
    let mut out = String::new();
    let mut escaped_chars = 0usize;
    for ch in input.chars() {
        let escaped = escape_html(&ch.to_string());
        let next_chars = escaped.chars().count();
        if escaped_chars + next_chars > budget {
            break;
        }
        out.push(ch);
        escaped_chars += next_chars;
    }
    out.push_str(suffix);
    out
}

pub(crate) async fn send_telegram_html_or_plain_message(
    client: &reqwest::Client,
    token: &str,
    chat_id: &str,
    html_text: &str,
    plain_text: &str,
) -> anyhow::Result<()> {
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    if html_text.chars().count() > TELEGRAM_MAX_MESSAGE_CHARS {
        return send_telegram_plain_message(client, &url, chat_id, plain_text).await;
    }

    let resp = client
        .post(&url)
        .json(&telegram_text_message_json(
            chat_id,
            html_text,
            Some("HTML"),
        ))
        .send()
        .await?;
    if resp.status().is_success() {
        return Ok(());
    }

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if should_retry_telegram_plain_text(status, &body) {
        return send_telegram_plain_message(client, &url, chat_id, plain_text)
            .await
            .map_err(|err| {
                anyhow::anyhow!(
                    "telegram http {}: {} (fallback error: {})",
                    status,
                    body,
                    err
                )
            });
    }

    Err(anyhow::anyhow!("telegram http {}: {}", status, body))
}

async fn send_telegram_plain_message(
    client: &reqwest::Client,
    url: &str,
    chat_id: &str,
    plain_text: &str,
) -> anyhow::Result<()> {
    let retry = client
        .post(url)
        .json(&telegram_text_message_json(chat_id, plain_text, None))
        .send()
        .await?;
    if retry.status().is_success() {
        return Ok(());
    }
    let retry_status = retry.status();
    let retry_body = retry.text().await.unwrap_or_default();
    Err(anyhow::anyhow!(
        "telegram http {}: {}",
        retry_status,
        retry_body
    ))
}

pub(crate) async fn send_telegram_card_or_text(
    client: &reqwest::Client,
    token: &str,
    chat_id: &str,
    card_png: Vec<u8>,
    caption_html: String,
    detail_html: String,
    detail_plain: String,
) -> anyhow::Result<TelegramDeliveryReport> {
    let photo_result = send_telegram_photo(client, token, chat_id, card_png, &caption_html).await;
    match photo_result {
        Ok(()) => {
            let mut report = TelegramDeliveryReport::default();
            if should_send_supplemental_message(&caption_html, &detail_html) {
                send_telegram_html_or_plain_message(
                    client,
                    token,
                    chat_id,
                    &detail_html,
                    &detail_plain,
                )
                .await?;
                report.supplemental_sent = true;
            }
            Ok(report)
        }
        Err(photo_err) => {
            let photo_error = photo_err.to_string();
            send_telegram_html_or_plain_message(
                client,
                token,
                chat_id,
                &detail_html,
                &detail_plain,
            )
            .await
            .map_err(|text_err| {
                anyhow::anyhow!(
                    "telegram photo failed: {}; text fallback failed: {}",
                    photo_error,
                    text_err
                )
            })?;
            Ok(TelegramDeliveryReport {
                photo_fallback_error: Some(photo_error),
                supplemental_sent: false,
            })
        }
    }
}

async fn send_telegram_photo(
    client: &reqwest::Client,
    token: &str,
    chat_id: &str,
    card_png: Vec<u8>,
    caption_html: &str,
) -> anyhow::Result<()> {
    let url = format!("https://api.telegram.org/bot{token}/sendPhoto");
    let part = reqwest::multipart::Part::bytes(card_png)
        .file_name("dockrev-notification.png")
        .mime_str("image/png")?;
    let form = reqwest::multipart::Form::new()
        .text("chat_id", chat_id.to_string())
        .text("caption", caption_html.to_string())
        .text("parse_mode", "HTML")
        .part("photo", part);

    let resp = client.post(&url).multipart(form).send().await?;
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(anyhow::anyhow!("telegram photo http {}: {}", status, body))
}

fn should_send_supplemental_message(caption_html: &str, detail_html: &str) -> bool {
    detail_html.chars().count() > caption_html.chars().count() + 160
        || detail_html.contains("<pre>")
        || detail_html.contains("<b>服务清单</b>")
        || detail_html.contains("<b>异常仓库</b>")
}

pub(crate) async fn send_telegram_job(
    client: &reqwest::Client,
    bot_token: Option<&str>,
    chat_id: Option<&str>,
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> anyhow::Result<TelegramDeliveryReport> {
    let token = bot_token.context("telegram.botToken missing")?;
    let chat_id = chat_id.context("telegram.chatId missing")?;
    let html_text = render_telegram_job_html(payload, error_excerpt);
    let plain_text = render_telegram_job_plain_for_send(payload, error_excerpt);
    let caption = render_telegram_photo_caption_html(
        &payload.human.title,
        &payload.human.summary,
        Some(render_open_link_html(&payload.links.primary_url, "详情")),
    );
    let card_png = render_job_telegram_card_png(payload, error_excerpt)?;
    send_telegram_card_or_text(
        client, token, chat_id, card_png, caption, html_text, plain_text,
    )
    .await
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
) -> anyhow::Result<TelegramDeliveryReport> {
    let token = bot_token.context("telegram.botToken missing")?;
    let chat_id = chat_id.context("telegram.chatId missing")?;
    let html_text = render_telegram_new_version_html(payload);
    let plain_text = render_telegram_new_version_plain_for_send(payload);
    let action = if is_single_new_version_payload(payload) {
        payload
            .links
            .service_urls
            .first()
            .map(|svc| render_service_detail_action_html(&svc.url))
    } else {
        Some(render_check_job_action_html(&payload.links.primary_url))
    };
    let caption =
        render_telegram_photo_caption_html(&payload.human.title, &payload.human.summary, action);
    let card_png = render_new_version_telegram_card_png(payload)?;
    send_telegram_card_or_text(
        client, token, chat_id, card_png, caption, html_text, plain_text,
    )
    .await
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
) -> anyhow::Result<TelegramDeliveryReport> {
    let token = bot_token.context("telegram.botToken missing")?;
    let chat_id = chat_id.context("telegram.chatId missing")?;
    let html_text = render_telegram_ghcr_webhook_anomaly_html(payload);
    let plain_text = render_telegram_ghcr_webhook_anomaly_plain_for_send(payload);
    let caption = render_telegram_photo_caption_html(
        &payload.human.title,
        &payload.human.summary,
        Some(render_open_link_html(&payload.links.job_url, "任务")),
    );
    let card_png = render_ghcr_webhook_anomaly_telegram_card_png(payload)?;
    send_telegram_card_or_text(
        client, token, chat_id, card_png, caption, html_text, plain_text,
    )
    .await
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
