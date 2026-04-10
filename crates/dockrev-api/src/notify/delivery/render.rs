use super::*;

pub(crate) fn render_open_link_html(url: &str, label: &str) -> String {
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

pub(crate) fn render_telegram_job_html(
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

pub(crate) fn render_telegram_job_plain(
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

pub(crate) fn render_telegram_job_plain_for_send(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> String {
    let plain = render_telegram_job_plain(payload, error_excerpt);
    truncate_chars(&plain, TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32))
}

pub(crate) fn render_email_job_plain(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> String {
    render_telegram_job_plain(payload, error_excerpt)
}

pub(crate) fn render_email_job_html(
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

pub(crate) fn is_single_new_version_payload(payload: &NewVersionNotificationPayloadV2) -> bool {
    payload.links.service_urls.len() == 1 && payload.links.truncated.service_urls_omitted == 0
}

pub(crate) fn render_service_detail_action_html(url: &str) -> String {
    render_open_link_html(url, "服务详情")
}

pub(crate) fn render_service_detail_action_plain(url: &str) -> String {
    format!("服务详情：{url}")
}

pub(crate) fn render_check_job_action_html(url: &str) -> String {
    if is_absolute_http_url(url) {
        format!("检查任务：<a href=\"{}\">检查任务</a>", escape_html(url))
    } else {
        format!("检查任务：<code>{}</code>", escape_html(url))
    }
}

pub(crate) fn render_check_job_action_plain(url: &str) -> String {
    format!("检查任务：{url}")
}

pub(crate) fn render_telegram_new_version_html(
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

pub(crate) fn render_telegram_new_version_plain(
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

pub(crate) fn render_telegram_new_version_plain_for_send(
    payload: &NewVersionNotificationPayloadV2,
) -> String {
    let plain = render_telegram_new_version_plain(payload);
    truncate_chars(&plain, TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32))
}

pub(crate) fn render_email_new_version_plain(payload: &NewVersionNotificationPayloadV2) -> String {
    render_telegram_new_version_plain(payload)
}

pub(crate) fn render_email_new_version_html(payload: &NewVersionNotificationPayloadV2) -> String {
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

pub(crate) fn render_telegram_ghcr_webhook_anomaly_html(
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

pub(crate) fn render_telegram_ghcr_webhook_anomaly_plain(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> String {
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

pub(crate) fn render_telegram_ghcr_webhook_anomaly_plain_for_send(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> String {
    let plain = render_telegram_ghcr_webhook_anomaly_plain(payload);
    truncate_chars(&plain, TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32))
}

pub(crate) fn render_email_ghcr_webhook_anomaly_plain(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> String {
    render_telegram_ghcr_webhook_anomaly_plain(payload)
}

pub(crate) fn render_email_ghcr_webhook_anomaly_html(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> String {
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
