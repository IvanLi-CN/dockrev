use super::*;

pub(crate) fn finalize_job_links(
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

pub(crate) async fn build_job_payload_v2(
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

pub(crate) async fn build_new_version_payload_v2(
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

pub(crate) async fn build_ghcr_webhook_anomaly_payload_v2(
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
