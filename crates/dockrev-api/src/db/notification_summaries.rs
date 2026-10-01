#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationVersionSummaryEntry {
    pub service_name: Option<String>,
    pub current_tag: String,
    pub candidate_tag: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationGhcrSummaryEntry {
    pub owner: String,
    pub repo: String,
    pub state: String,
}

pub(crate) fn format_job_notification_summary(
    job_type: Option<&str>,
    scope: Option<&str>,
    target_name: Option<&str>,
    status: &str,
) -> (String, String) {
    let task = job_type_label(job_type);
    let title = format!("{task}任务{}", job_status_title(status));
    let target = match scope {
        Some("service") => target_name
            .filter(|name| !name.trim().is_empty())
            .map(|name| format!("服务「{name}」"))
            .unwrap_or_else(|| "该服务".to_string()),
        Some("stack") => target_name
            .filter(|name| !name.trim().is_empty())
            .map(|name| format!("Stack「{name}」"))
            .unwrap_or_else(|| "该 Stack".to_string()),
        Some("all") => "多个服务".to_string(),
        _ => "任务目标".to_string(),
    };
    let outcome = match status {
        "success" | "succeeded" => "已完成",
        "failed" | "error" => "执行失败，请查看详情",
        "cancelled" | "canceled" => "已取消",
        "rolled_back" => "已回滚",
        "stopped" => "已停止",
        _ => "已结束",
    };
    (title, format!("{target}的{task}操作{outcome}。"))
}

pub(crate) fn format_new_version_notification_title(service_count: usize) -> String {
    format!("发现 {service_count} 个新版本")
}

pub(crate) fn format_new_version_notification_body(
    entries: &[NotificationVersionSummaryEntry],
) -> String {
    let changes = entries
        .iter()
        .take(2)
        .map(|entry| {
            let name = entry
                .service_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("服务");
            let current = notification_version_label(&entry.current_tag, "当前版本");
            let candidate = notification_version_label(&entry.candidate_tag, "新版本");
            format!("{name}：{current} -> {candidate}")
        })
        .collect::<Vec<_>>();
    let mut body = changes.join("\n");
    if entries.len() > changes.len() {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&format!(
            "另有 {} 个服务有新版本。",
            entries.len() - changes.len()
        ));
    }
    if body.is_empty() {
        "已发现新版本，可查看检查结果。".to_string()
    } else {
        body
    }
}

pub(crate) fn format_ghcr_anomaly_notification_title(anomaly_count: usize) -> String {
    format!("GHCR Webhook 有 {anomaly_count} 项异常")
}

pub(crate) fn format_ghcr_anomaly_notification_body(
    entries: &[NotificationGhcrSummaryEntry],
) -> String {
    let mut body = entries
        .iter()
        .take(5)
        .map(|entry| {
            let description = match entry.state.as_str() {
                "missing" => "未在 GHCR 找到关联仓库",
                "conflict" => "Webhook 仓库归属与配置不一致",
                "error" => "Webhook 检查失败",
                _ => "Webhook 状态需要检查",
            };
            format!("{}/{}：{description}", entry.owner, entry.repo)
        })
        .collect::<Vec<_>>()
        .join("\n");
    if entries.len() > 5 {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&format!("另有 {} 项异常。", entries.len() - 5));
    }
    if body.is_empty() {
        "GHCR Webhook 状态需要检查。".to_string()
    } else {
        body
    }
}

fn job_type_label(job_type: Option<&str>) -> &str {
    match job_type {
        Some("update") => "更新",
        Some("rollback") => "回滚",
        Some("check") => "检查",
        Some("discovery") => "发现",
        Some("cleanup_apply") => "清理",
        Some(
            "github_packages_webhook"
            | "github_packages_webhook_sync_all"
            | "github_packages_webhook_sync_repo",
        ) => "GHCR Webhook",
        Some("runtime_scan") => "运行时检查",
        Some("service_lifecycle" | "stack_lifecycle") => "资源管理",
        _ => "运维",
    }
}

fn job_status_title(status: &str) -> &str {
    match status {
        "success" | "succeeded" => "已成功",
        "failed" | "error" => "失败",
        "cancelled" | "canceled" => "已取消",
        "rolled_back" => "已回滚",
        "stopped" => "已停止",
        _ => "已完成",
    }
}

fn notification_version_label<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let value = value.trim();
    if !notification_version_tag_is_readable(value) {
        fallback
    } else {
        value
    }
}

pub(crate) fn notification_version_tag_is_readable(value: &str) -> bool {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 80
        || ["sha", "digest:"]
            .iter()
            .any(|prefix| lower.starts_with(prefix) && lower.contains(':'))
    {
        return false;
    }

    let bytes = value.as_bytes();
    let hexadecimal = bytes.iter().all(u8::is_ascii_hexdigit);
    let common_digest_length = matches!(bytes.len(), 32 | 40 | 64);
    let hash_like_tag =
        (7..=63).contains(&bytes.len()) && bytes.iter().any(u8::is_ascii_alphabetic);
    !(hexadecimal && (common_digest_length || hash_like_tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_summary_formatters_avoid_machine_payloads() {
        let (title, body) = format_job_notification_summary(
            Some("update"),
            Some("service"),
            Some("支付 API"),
            "success",
        );
        assert_eq!(title, "更新任务已成功");
        assert_eq!(body, "服务「支付 API」的更新操作已完成。");

        let (fallback_title, fallback_body) =
            format_job_notification_summary(None, None, None, "failed");
        assert_eq!(fallback_title, "运维任务失败");
        assert_eq!(fallback_body, "任务目标的运维操作执行失败，请查看详情。");
        for machine_value in ["{", "progress", "job_", "sha256:"] {
            assert!(!fallback_title.contains(machine_value));
            assert!(!fallback_body.contains(machine_value));
        }

        let versions = vec![
            NotificationVersionSummaryEntry {
                service_name: Some("支付 API".to_string()),
                current_tag: "2.4.0".to_string(),
                candidate_tag: "2.5.0".to_string(),
            },
            NotificationVersionSummaryEntry {
                service_name: Some("后台 Worker".to_string()),
                current_tag: "1.8.2".to_string(),
                candidate_tag: "1.9.0".to_string(),
            },
            NotificationVersionSummaryEntry {
                service_name: Some("前端".to_string()),
                current_tag: "5.1.0".to_string(),
                candidate_tag: "5.2.0".to_string(),
            },
        ];
        assert_eq!(
            format_new_version_notification_title(versions.len()),
            "发现 3 个新版本"
        );
        let version_body = format_new_version_notification_body(&versions);
        assert_eq!(
            version_body,
            "支付 API：2.4.0 -> 2.5.0\n后台 Worker：1.8.2 -> 1.9.0\n另有 1 个服务有新版本。"
        );
        for machine_value in ["service-internal", "sha256:"] {
            assert!(!version_body.contains(machine_value));
        }
        let digest_version = NotificationVersionSummaryEntry {
            service_name: Some("服务".to_string()),
            current_tag: "0123456789abcdef0123456789abcdef01234567".to_string(),
            candidate_tag: "a".repeat(64),
        };
        let digest_body = format_new_version_notification_body(&[digest_version]);
        assert_eq!(digest_body, "服务：当前版本 -> 新版本");
        assert!(!digest_body.contains("0123456789abcdef"));

        let anomalies = vec![
            NotificationGhcrSummaryEntry {
                owner: "acme".to_string(),
                repo: "api".to_string(),
                state: "missing".to_string(),
            },
            NotificationGhcrSummaryEntry {
                owner: "acme".to_string(),
                repo: "worker".to_string(),
                state: "conflict".to_string(),
            },
            NotificationGhcrSummaryEntry {
                owner: "acme".to_string(),
                repo: "web".to_string(),
                state: "error".to_string(),
            },
        ];
        let ghcr_body = format_ghcr_anomaly_notification_body(&anomalies);
        assert_eq!(
            ghcr_body,
            "acme/api：未在 GHCR 找到关联仓库\nacme/worker：Webhook 仓库归属与配置不一致\nacme/web：Webhook 检查失败"
        );
        for machine_value in ["missing", "conflict", "error"] {
            assert!(!ghcr_body.contains(machine_value));
        }
    }
}
