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
        "https://dockrev.example.com/settings",
    );
    let value = to_value(&payload).unwrap();

    assert_eq!(
        value["schema"].as_str(),
        Some("dockrev.notification.test.v2")
    );
    assert_eq!(value["kind"].as_str(), Some("notification_test"));
    assert_eq!(value["channel"].as_str(), Some("telegram"));
    assert_eq!(
        value["url"].as_str(),
        Some("https://dockrev.example.com/settings")
    );
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
        "https://dockrev.example.com/settings",
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
        "https://dockrev.example.com/settings",
    );
    let value = to_web_push_value(&payload).unwrap();
    let body = value["body"].as_str().unwrap_or_default();
    assert!(!body.contains("```"));
    assert!(!body.contains("<pre>"));
    assert_eq!(
        value["url"].as_str(),
        Some("https://dockrev.example.com/settings")
    );
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
        "https://dockrev.example.com/settings",
    );
    let plain = render_telegram_plain_for_send(&payload).unwrap();
    assert!(plain.chars().count() <= TELEGRAM_MAX_MESSAGE_CHARS.saturating_sub(32));
}

fn sample_job_payload(links: JobNotificationLinksV2) -> JobNotificationPayloadV2 {
    JobNotificationPayloadV2 {
        schema: "dockrev.notification.job.v2",
        kind: "job_finished",
        sent_at: "2026-03-05T04:44:59Z".to_string(),
        channel: "telegram",
        job: JobNotificationJobV2 {
            id: "job_123".to_string(),
            r#type: "update".to_string(),
            scope: "all".to_string(),
            status: "success".to_string(),
            reason: "manual".to_string(),
            created_by: "test".to_string(),
            created_at: "2026-03-05T04:40:00Z".to_string(),
            started_at: Some("2026-03-05T04:41:00Z".to_string()),
            finished_at: Some("2026-03-05T04:44:59Z".to_string()),
            stack_id: None,
            service_id: None,
        },
        links,
        human: JobNotificationHumanV2 {
            title: "Dockrev：更新完成（成功）".to_string(),
            summary: "变更 1 个服务（blog / api）。".to_string(),
            detail: "test".to_string(),
        },
        debug: JobNotificationDebugV2 {
            app_version: "0.1.0".to_string(),
            source: "dockrev-api",
        },
    }
}

fn make_service_url(i: usize) -> JobNotificationServiceUrlV2 {
    JobNotificationServiceUrlV2 {
        stack_id: format!("stk_{i}"),
        stack_name: format!("stack-{i}"),
        service_id: format!("svc_{i}"),
        service_name: format!("service-{i}"),
        url: format!("https://dockrev.example.com/services/stk_{i}/svc_{i}"),
    }
}

#[test]
fn job_notification_links_single_service_prefers_service_url() {
    let job_url = "https://dockrev.example.com/queue/job_123".to_string();
    let links = finalize_job_links(job_url.clone(), vec![make_service_url(1)], false, None);
    assert_eq!(links.primary_url, links.service_urls[0].url);
    assert_ne!(links.primary_url, job_url);
}

#[test]
fn job_notification_links_multi_service_prefers_job_url() {
    let job_url = "https://dockrev.example.com/queue/job_123".to_string();
    let links = finalize_job_links(
        job_url.clone(),
        vec![make_service_url(1), make_service_url(2)],
        false,
        None,
    );
    assert_eq!(links.primary_url, job_url);
}

#[test]
fn service_urls_truncation_sets_omitted_count() {
    let job_url = "https://dockrev.example.com/queue/job_123".to_string();
    let service_urls = (0..(MAX_JOB_SERVICE_URLS + 3))
        .map(make_service_url)
        .collect::<Vec<_>>();
    let links = finalize_job_links(job_url, service_urls, false, None);
    assert_eq!(links.service_urls.len(), MAX_JOB_SERVICE_URLS);
    assert_eq!(links.truncated.service_urls_omitted, 3);
}

#[test]
fn update_summary_includes_service_names_for_multi() {
    let services = vec![
        make_service_url(1),
        make_service_url(2),
        make_service_url(3),
    ];
    let summary = summarize_updated_services(&services, 0);
    assert!(summary.starts_with("变更 3 个服务："));
    assert!(summary.contains("stack-1 / service-1"));
    assert!(summary.contains("stack-2 / service-2"));
    assert!(summary.contains("stack-3 / service-3"));
}

#[test]
fn update_summary_marks_omitted_and_visible_limit() {
    let services = vec![
        make_service_url(1),
        make_service_url(2),
        make_service_url(3),
    ];
    let summary = summarize_updated_services(&services, 9);
    assert!(summary.contains("变更 12 个服务"));
    assert!(summary.contains("stack-1 / service-1"));
    assert!(summary.contains("stack-2 / service-2"));
    assert!(summary.contains("stack-3 / service-3"));
    assert!(summary.contains("仅展示前 3 条"));
}

#[test]
fn telegram_render_contains_clickable_service_links() {
    let job_url = "https://dockrev.example.com/queue/job_123".to_string();
    let links = finalize_job_links(job_url, vec![make_service_url(1)], false, None);
    let payload = sample_job_payload(links);
    let html = render_telegram_job_html(&payload, None);
    assert!(html.contains(
            "<b>Dockrev：更新完成（成功）</b> <a href=\"https://dockrev.example.com/services/stk_1/svc_1\">详情</a>"
        ));
    assert!(html.contains("<a href=\"https://dockrev.example.com/services/stk_1/svc_1\">"));
    assert!(html.contains("<b>服务清单</b>"));
    assert!(!html.contains("任务详情："));
    assert!(!html.contains("打开服务详情："));
    assert!(!html.contains("Dockrev notification:"));
}

#[test]
fn telegram_job_title_line_keeps_detail_suffix_without_base_url() {
    let links = JobNotificationLinksV2 {
        primary_url: "services/stk_1/svc_1".to_string(),
        job_url: "queue/job_123".to_string(),
        service_urls: vec![JobNotificationServiceUrlV2 {
            stack_id: "stk_1".to_string(),
            stack_name: "blog".to_string(),
            service_id: "svc_1".to_string(),
            service_name: "api".to_string(),
            url: "services/stk_1/svc_1".to_string(),
        }],
        truncated: JobNotificationTruncatedV2 {
            service_urls_omitted: 0,
        },
    };
    let payload = sample_job_payload(links);
    let html = render_telegram_job_html(&payload, None);
    assert!(html.contains("<b>Dockrev：更新完成（成功）</b> <code>services/stk_1/svc_1</code>"));
    assert!(!html.contains("\n详情："));
}

fn sample_new_version_payload() -> NewVersionNotificationPayloadV2 {
    NewVersionNotificationPayloadV2 {
        schema: "dockrev.notification.new_version_discovered.v2",
        kind: "new_version_discovered",
        sent_at: "2026-03-05T04:44:59Z".to_string(),
        channel: "telegram",
        check: NewVersionNotificationCheckV2 {
            job_id: "job_check_123".to_string(),
            status: "success".to_string(),
            scope: "all".to_string(),
            services_checked: 12,
            new_versions: 1,
        },
        links: NewVersionNotificationLinksV2 {
            primary_url: "https://dockrev.example.com/services/stk_1/svc_1".to_string(),
            job_url: "https://dockrev.example.com/queue/job_check_123".to_string(),
            service_urls: vec![NewVersionNotificationServiceUrlV2 {
                stack_id: "stk_1".to_string(),
                stack_name: "blog".to_string(),
                service_id: "svc_1".to_string(),
                service_name: "api".to_string(),
                current_tag: Some("latest".to_string()),
                current_display_tag: Some("1.0.0".to_string()),
                candidate_tag: Some("latest".to_string()),
                candidate_display_tag: Some("1.1.0".to_string()),
                url: "https://dockrev.example.com/services/stk_1/svc_1".to_string(),
            }],
            truncated: JobNotificationTruncatedV2 {
                service_urls_omitted: 0,
            },
        },
        human: JobNotificationHumanV2 {
            title: "blog / api 服务有新版本".to_string(),
            summary: "blog / api 服务有新版本（1.0.0 -> 1.1.0）。".to_string(),
            detail: "test".to_string(),
        },
        debug: JobNotificationDebugV2 {
            app_version: "0.1.0".to_string(),
            source: "dockrev-api",
        },
    }
}

fn sample_multi_new_version_payload() -> NewVersionNotificationPayloadV2 {
    let mut payload = sample_new_version_payload();
    payload.check.new_versions = 2;
    payload.links.primary_url = "https://dockrev.example.com/queue/job_check_123".to_string();
    payload
        .links
        .service_urls
        .push(make_new_version_service("shop", "gateway"));
    payload.human.title = "发现 2 个服务有新版本".to_string();
    payload.human.summary =
        "发现 2 个服务有新版本：\n- blog / api (1.0.0 -> 1.1.0)\n- shop / gateway (1.0.0 -> 1.1.0)"
            .to_string();
    payload
}

fn make_new_version_service(
    stack_name: &str,
    service_name: &str,
) -> NewVersionNotificationServiceUrlV2 {
    NewVersionNotificationServiceUrlV2 {
        stack_id: format!("stk_{stack_name}"),
        stack_name: stack_name.to_string(),
        service_id: format!("svc_{service_name}"),
        service_name: service_name.to_string(),
        current_tag: Some("v1.0.0".to_string()),
        current_display_tag: Some("1.0.0".to_string()),
        candidate_tag: Some("v1.1.0".to_string()),
        candidate_display_tag: Some("1.1.0".to_string()),
        url: format!("https://dockrev.example.com/services/stk_{stack_name}/svc_{service_name}"),
    }
}

#[test]
fn new_version_summary_includes_service_names_for_multi() {
    let services = vec![
        make_new_version_service("blog", "api"),
        make_new_version_service("blog", "worker"),
        make_new_version_service("shop", "gateway"),
    ];
    let summary = summarize_new_version_services(3, &services, 0);
    assert!(summary.starts_with("发现 3 个服务有新版本：\n"));
    assert!(summary.contains("\n- blog / api (1.0.0 -> 1.1.0)"));
    assert!(summary.contains("\n- blog / worker (1.0.0 -> 1.1.0)"));
    assert!(summary.contains("\n- shop / gateway (1.0.0 -> 1.1.0)"));
}

#[test]
fn new_version_summary_marks_omitted_and_preview() {
    let services = vec![
        make_new_version_service("blog", "api"),
        make_new_version_service("blog", "worker"),
        make_new_version_service("shop", "gateway"),
        make_new_version_service("shop", "sync"),
    ];
    let summary = summarize_new_version_services(14, &services, 10);
    assert!(summary.starts_with("发现 14 个服务有新版本：\n"));
    assert!(summary.contains("\n- blog / api (1.0.0 -> 1.1.0)"));
    assert!(summary.contains("\n- blog / worker (1.0.0 -> 1.1.0)"));
    assert!(summary.contains("\n- shop / gateway (1.0.0 -> 1.1.0)"));
    assert!(summary.contains("\n- shop / sync (1.0.0 -> 1.1.0)"));
    assert!(summary.contains("\n... 以及其他 10 个服务（已省略）"));
}

#[test]
fn new_version_summary_single_service_omits_raw_only_transition() {
    let services = vec![NewVersionNotificationServiceUrlV2 {
        stack_id: "stk_blog".to_string(),
        stack_name: "blog".to_string(),
        service_id: "svc_api".to_string(),
        service_name: "api".to_string(),
        current_tag: Some("latest".to_string()),
        current_display_tag: Some("latest".to_string()),
        candidate_tag: Some("latest".to_string()),
        candidate_display_tag: Some("latest".to_string()),
        url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
    }];
    let summary = summarize_new_version_services(1, &services, 0);
    assert_eq!(summary, "blog / api 服务有新版本。");
}

#[test]
fn new_version_summary_single_service_keeps_same_strict_semver_transition() {
    let services = vec![NewVersionNotificationServiceUrlV2 {
        stack_id: "stk_blog".to_string(),
        stack_name: "blog".to_string(),
        service_id: "svc_api".to_string(),
        service_name: "api".to_string(),
        current_tag: Some("1.2.3".to_string()),
        current_display_tag: Some("1.2.3".to_string()),
        candidate_tag: Some("1.2.3".to_string()),
        candidate_display_tag: Some("1.2.3".to_string()),
        url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
    }];
    let summary = summarize_new_version_services(1, &services, 0);
    assert_eq!(summary, "blog / api 服务有新版本（1.2.3 -> 1.2.3）。");
}

#[test]
fn new_version_summary_single_service_omits_same_alias_transition() {
    let services = vec![NewVersionNotificationServiceUrlV2 {
        stack_id: "stk_blog".to_string(),
        stack_name: "blog".to_string(),
        service_id: "svc_api".to_string(),
        service_name: "api".to_string(),
        current_tag: Some("5.2".to_string()),
        current_display_tag: Some("5.2".to_string()),
        candidate_tag: Some("5.2".to_string()),
        candidate_display_tag: Some("5.2".to_string()),
        url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
    }];
    let summary = summarize_new_version_services(1, &services, 0);
    assert_eq!(summary, "blog / api 服务有新版本。");
}

#[test]
fn new_version_summary_single_service_allows_resolved_and_raw_mix() {
    let services = vec![NewVersionNotificationServiceUrlV2 {
        stack_id: "stk_blog".to_string(),
        stack_name: "blog".to_string(),
        service_id: "svc_api".to_string(),
        service_name: "api".to_string(),
        current_tag: Some("latest".to_string()),
        current_display_tag: Some("latest".to_string()),
        candidate_tag: Some("latest".to_string()),
        candidate_display_tag: Some("1.1.0".to_string()),
        url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
    }];
    let summary = summarize_new_version_services(1, &services, 0);
    assert_eq!(summary, "blog / api 服务有新版本（latest -> 1.1.0）。");
}

#[test]
fn new_version_summary_keeps_parseable_non_strict_transitions() {
    let services = vec![NewVersionNotificationServiceUrlV2 {
        stack_id: "stk_blog".to_string(),
        stack_name: "blog".to_string(),
        service_id: "svc_api".to_string(),
        service_name: "api".to_string(),
        current_tag: Some("15-alpine".to_string()),
        current_display_tag: Some("15-alpine".to_string()),
        candidate_tag: Some("16-alpine".to_string()),
        candidate_display_tag: Some("16-alpine".to_string()),
        url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
    }];
    let summary = summarize_new_version_services(1, &services, 0);
    assert_eq!(
        summary,
        "blog / api 服务有新版本（15-alpine -> 16-alpine）。"
    );
}

#[test]
fn notification_tag_requires_settle_reuses_shared_non_strict_semver_rules() {
    assert!(notification_tag_requires_settle("main", "main"));
    assert!(notification_tag_requires_settle("nightly", "nightly"));
    assert!(!notification_tag_requires_settle("1.2.3", "1.2.3"));
    assert!(!notification_tag_requires_settle("main", "5.3.0"));
}

#[test]
fn preferred_notification_display_keeps_frozen_summary_before_live_resolved() {
    let display =
        preferred_notification_display_tag("latest", Some("5.2.0"), None, Some("5.1.0"), None);
    assert_eq!(display, "5.2.0");
}

#[test]
fn preferred_notification_display_keeps_fresh_snapshot_before_live_resolved() {
    let display = preferred_notification_display_tag(
        "latest",
        Some("latest"),
        Some("5.2.0"),
        Some("5.1.0"),
        None,
    );
    assert_eq!(display, "5.2.0");
}

#[test]
fn notification_tag_requires_settle_only_for_unresolved_non_strict_tags() {
    assert!(notification_tag_requires_settle("latest", "latest"));
    assert!(notification_tag_requires_settle("15-alpine", "15-alpine"));
    assert!(!notification_tag_requires_settle("15-alpine", "15.0.2"));
    assert!(!notification_tag_requires_settle("1.2.3", "1.2.3"));
    assert!(notification_tag_requires_settle("main", "main"));
    assert!(notification_tag_requires_settle(
        "sha-abcdef0",
        "sha-abcdef0"
    ));
}

#[test]
fn new_version_summary_multi_service_omits_raw_only_transition_per_item() {
    let services = vec![
        NewVersionNotificationServiceUrlV2 {
            stack_id: "stk_blog".to_string(),
            stack_name: "blog".to_string(),
            service_id: "svc_api".to_string(),
            service_name: "api".to_string(),
            current_tag: Some("latest".to_string()),
            current_display_tag: Some("latest".to_string()),
            candidate_tag: Some("latest".to_string()),
            candidate_display_tag: Some("latest".to_string()),
            url: "https://dockrev.example.com/services/stk_blog/svc_api".to_string(),
        },
        make_new_version_service("shop", "gateway"),
    ];
    let summary = summarize_new_version_services(2, &services, 0);
    assert!(summary.contains("blog / api"));
    assert!(!summary.contains("blog / api（"));
    assert!(summary.contains("shop / gateway (1.0.0 -> 1.1.0)"));
}

fn sample_ghcr_anomaly_payload() -> GhcrWebhookAnomalyPayloadV2 {
    GhcrWebhookAnomalyPayloadV2 {
        schema: "dockrev.notification.ghcr_webhook_anomaly.v2",
        kind: "ghcr_webhook_anomaly",
        sent_at: "2026-03-05T04:44:59Z".to_string(),
        channel: "telegram",
        job: GhcrWebhookAnomalyJobV2 {
            id: "job_ghcr_123".to_string(),
            status: "failed".to_string(),
            missing: 1,
            conflict: 0,
            error: 1,
            total_anomalies: 2,
        },
        links: GhcrWebhookAnomalyLinksV2 {
            primary_url: "https://dockrev.example.com/queue/job_ghcr_123".to_string(),
            job_url: "https://dockrev.example.com/queue/job_ghcr_123".to_string(),
            settings_url: "https://dockrev.example.com/settings".to_string(),
            repos: vec![
                GhcrWebhookAnomalyRepoV2 {
                    owner: "acme".to_string(),
                    repo: "api".to_string(),
                    full_name: "acme/api".to_string(),
                    state: "missing".to_string(),
                    last_error: Some("webhook missing".to_string()),
                },
                GhcrWebhookAnomalyRepoV2 {
                    owner: "acme".to_string(),
                    repo: "worker".to_string(),
                    full_name: "acme/worker".to_string(),
                    state: "error".to_string(),
                    last_error: Some("github api timeout".to_string()),
                },
            ],
            truncated: GhcrWebhookAnomalyTruncatedV2 { repos_omitted: 0 },
        },
        human: JobNotificationHumanV2 {
            title: "Dockrev：GitHub Webhook 巡检异常".to_string(),
            summary: "巡检发现 2 个异常仓库：acme/api [missing]、acme/worker [error]。".to_string(),
            detail: "test".to_string(),
        },
        debug: JobNotificationDebugV2 {
            app_version: "0.1.0".to_string(),
            source: "dockrev-api",
        },
    }
}

#[test]
fn new_version_telegram_render_uses_single_service_action_copy() {
    let payload = sample_new_version_payload();
    let html = render_telegram_new_version_html(&payload);
    assert!(!html.contains("Dockrev：发现新版本"));
    assert!(html.contains("blog / api 服务有新版本（1.0.0 -&gt; 1.1.0）。"));
    assert!(
        html.contains("<a href=\"https://dockrev.example.com/services/stk_1/svc_1\">服务详情</a>")
    );
    assert!(!html.contains("<b>服务清单</b>"));
    assert!(!html.contains(">详情</a>"));
}

#[test]
fn new_version_single_service_without_base_url_keeps_service_action() {
    let mut payload = sample_new_version_payload();
    payload.links.primary_url = "services/stk_1/svc_1".to_string();
    payload.links.job_url = "queue/job_check_123".to_string();
    payload.links.service_urls[0].url = "services/stk_1/svc_1".to_string();

    let html = render_telegram_new_version_html(&payload);
    assert!(!html.contains("Dockrev：发现新版本"));
    assert!(html.contains("<code>services/stk_1/svc_1</code>"));
    assert!(!html.contains("\n详情："));
    assert!(!html.contains("<b>服务清单</b>"));
}

#[test]
fn new_version_email_render_uses_single_service_action_copy() {
    let payload = sample_new_version_payload();
    let plain = render_email_new_version_plain(&payload);
    let html = render_email_new_version_html(&payload);

    assert!(!plain.contains("Dockrev：发现新版本"));
    assert!(plain.contains("blog / api 服务有新版本（1.0.0 -> 1.1.0）。"));
    assert!(plain.contains("服务详情：https://dockrev.example.com/services/stk_1/svc_1"));
    assert!(!plain.contains("服务清单"));
    assert!(!plain.contains("检查任务："));

    assert!(!html.contains("<h2>"));
    assert!(html.contains("blog / api 服务有新版本（1.0.0 -&gt; 1.1.0）。"));
    assert!(
        html.contains("<a href=\"https://dockrev.example.com/services/stk_1/svc_1\">服务详情</a>")
    );
    assert!(!html.contains("服务清单"));
    assert!(!html.contains("检查任务："));
}

#[test]
fn new_version_multi_service_render_puts_each_service_on_its_own_line() {
    let payload = sample_multi_new_version_payload();
    let telegram_html = render_telegram_new_version_html(&payload);
    let telegram_plain = render_telegram_new_version_plain(&payload);
    let email_html = render_email_new_version_html(&payload);

    assert!(!telegram_html.contains("<b>发现 2 个服务有新版本</b>"));
    assert!(telegram_html.starts_with(
            "发现 2 个服务有新版本：\n- blog / api (1.0.0 -&gt; 1.1.0)\n- shop / gateway (1.0.0 -&gt; 1.1.0)"
        ));
    assert!(telegram_html.contains(
        r#"检查任务：<a href="https://dockrev.example.com/queue/job_check_123">检查任务</a>"#
    ));
    assert!(!telegram_html.contains("<b>服务清单</b>"));

    assert!(telegram_plain.starts_with(
        "发现 2 个服务有新版本：\n- blog / api (1.0.0 -> 1.1.0)\n- shop / gateway (1.0.0 -> 1.1.0)"
    ));
    assert!(telegram_plain.contains("\n检查任务：https://dockrev.example.com/queue/job_check_123"));
    assert!(!telegram_plain.contains("\n服务清单\n"));

    assert!(!email_html.contains("<h2>"));
    assert!(email_html.starts_with(
            "<p>发现 2 个服务有新版本：<br>- blog / api (1.0.0 -&gt; 1.1.0)<br>- shop / gateway (1.0.0 -&gt; 1.1.0)</p>"
        ));
    assert!(email_html.contains(
        r#"检查任务：<a href="https://dockrev.example.com/queue/job_check_123">查看检查任务</a>"#
    ));
    assert!(!email_html.contains("<ul>"));
}

#[test]
fn ghcr_anomaly_telegram_render_contains_repo_state() {
    let payload = sample_ghcr_anomaly_payload();
    let html = render_telegram_ghcr_webhook_anomaly_html(&payload);
    assert!(html.contains(
            "<b>Dockrev：GitHub Webhook 巡检异常</b> <a href=\"https://dockrev.example.com/queue/job_ghcr_123\">任务</a>"
        ));
    assert!(html.contains("acme/api"));
    assert!(html.contains("acme/worker"));
    assert!(html.contains("missing"));
    assert!(html.contains("webhook missing"));
    assert!(!html.contains("巡检任务："));
    assert!(!html.contains("打开设置"));
}

#[test]
fn ghcr_anomaly_title_line_keeps_task_suffix_without_base_url() {
    let mut payload = sample_ghcr_anomaly_payload();
    payload.links.primary_url = "queue/job_ghcr_123".to_string();
    payload.links.job_url = "queue/job_ghcr_123".to_string();
    payload.links.settings_url = "settings".to_string();

    let html = render_telegram_ghcr_webhook_anomaly_html(&payload);
    assert!(
        html.contains("<b>Dockrev：GitHub Webhook 巡检异常</b> <code>queue/job_ghcr_123</code>")
    );
    assert!(!html.contains("\n任务："));
}

#[test]
fn ghcr_anomaly_summary_includes_repo_names() {
    let repos = vec![
        GhcrWebhookAnomalyRepoV2 {
            owner: "acme".to_string(),
            repo: "api".to_string(),
            full_name: "acme/api".to_string(),
            state: "missing".to_string(),
            last_error: None,
        },
        GhcrWebhookAnomalyRepoV2 {
            owner: "acme".to_string(),
            repo: "worker".to_string(),
            full_name: "acme/worker".to_string(),
            state: "error".to_string(),
            last_error: None,
        },
    ];
    let summary = summarize_ghcr_anomaly_repos(2, &repos, 0);
    assert!(summary.contains("acme/api [missing]"));
    assert!(summary.contains("acme/worker [error]"));
    assert!(!summary.contains("missing="));
}

#[test]
fn ghcr_anomaly_summary_marks_omitted_and_visible_limit() {
    let repos = vec![
        GhcrWebhookAnomalyRepoV2 {
            owner: "acme".to_string(),
            repo: "api".to_string(),
            full_name: "acme/api".to_string(),
            state: "missing".to_string(),
            last_error: None,
        },
        GhcrWebhookAnomalyRepoV2 {
            owner: "acme".to_string(),
            repo: "worker".to_string(),
            full_name: "acme/worker".to_string(),
            state: "error".to_string(),
            last_error: None,
        },
        GhcrWebhookAnomalyRepoV2 {
            owner: "acme".to_string(),
            repo: "sync".to_string(),
            full_name: "acme/sync".to_string(),
            state: "conflict".to_string(),
            last_error: None,
        },
    ];
    let summary = summarize_ghcr_anomaly_repos(14, &repos, 11);
    assert!(summary.contains("acme/api [missing]"));
    assert!(summary.contains("acme/worker [error]"));
    assert!(summary.contains("acme/sync [conflict]"));
    assert!(summary.contains("仅展示前 3 条"));
}

#[test]
fn web_push_payload_contains_url_for_new_notifications() {
    let new_version_payload = sample_new_version_payload();
    let new_version_value = to_web_push_new_version_value(&new_version_payload).unwrap();
    assert_eq!(
        new_version_value["url"].as_str(),
        Some("https://dockrev.example.com/services/stk_1/svc_1")
    );
    assert_eq!(
        new_version_value["title"].as_str(),
        Some("blog / api 服务有新版本")
    );
    assert_eq!(
        new_version_value["body"].as_str(),
        Some("blog / api 服务有新版本（1.0.0 -> 1.1.0）。")
    );

    let ghcr_payload = sample_ghcr_anomaly_payload();
    let ghcr_value = to_web_push_ghcr_webhook_anomaly_value(&ghcr_payload).unwrap();
    assert_eq!(
        ghcr_value["url"].as_str(),
        Some("https://dockrev.example.com/queue/job_ghcr_123")
    );
    assert_eq!(
        ghcr_value["body"].as_str(),
        Some(
            "巡检发现 2 个异常仓库：acme/api [missing]、acme/worker [error]。
点击通知查看详情"
        )
    );
}

#[test]
fn event_toggle_flags_are_checked_per_type() {
    let settings = NotificationSettings {
        email_enabled: false,
        email_smtp_url: None,
        webhook_enabled: false,
        webhook_url: None,
        telegram_enabled: false,
        telegram_bot_token: None,
        telegram_chat_id: None,
        webpush_enabled: false,
        webpush_vapid_public_key: None,
        webpush_vapid_private_key: None,
        webpush_vapid_subject: None,
        event_update_enabled: true,
        event_new_version_enabled: false,
        event_ghcr_webhook_anomaly_enabled: true,
    };
    assert!(is_event_enabled(&settings, NotificationEventKind::Update));
    assert!(!is_event_enabled(
        &settings,
        NotificationEventKind::NewVersionDiscovered
    ));
    assert!(is_event_enabled(
        &settings,
        NotificationEventKind::GhcrWebhookAnomaly
    ));
}

#[test]
fn error_excerpt_skips_stacks_without_update_block() {
    let summary = json!({
        "stacks": [
            { "stackId": "stk_empty" },
            { "stackId": "stk_err", "update": { "error": "registry timeout" } }
        ]
    });
    let excerpt = extract_error_excerpt(&summary);
    assert_eq!(excerpt.as_deref(), Some("registry timeout"));
}
