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

#[test]
fn telegram_text_payload_disables_link_preview() {
    let value = telegram_text_message_json(
        "-100123",
        "服务详情：https://dockrev.example.com/services/stk/svc",
        Some("HTML"),
    );
    assert_eq!(value["chat_id"].as_str(), Some("-100123"));
    assert_eq!(value["parse_mode"].as_str(), Some("HTML"));
    assert_eq!(
        value["link_preview_options"]["is_disabled"].as_bool(),
        Some(true)
    );
}

#[test]
fn telegram_card_render_error_is_classified_as_photo_fallback() {
    let err = telegram_card_png_or_render_fallback(Err(anyhow::anyhow!(
        "decode telegram card reference png"
    )))
    .unwrap_err();
    assert_eq!(err, "decode telegram card reference png");
}

#[test]
fn telegram_card_item_summary_includes_multiple_items_and_omitted_count() {
    let items = vec![
        "blog / api".to_string(),
        "blog / worker".to_string(),
        "shop / gateway".to_string(),
        "shop / sync".to_string(),
    ];
    assert_eq!(
        summarize_card_items(&items, 2, "4 个服务".to_string()),
        "blog / api、blog / worker、shop / gateway，另 3 项"
    );
    assert_eq!(
        summarize_card_items(&items[..2], 0, "2 个服务".to_string()),
        "blog / api、blog / worker"
    );
    assert_eq!(
        summarize_card_items(&[], 0, "无仓库详情".to_string()),
        "无仓库详情"
    );
}

#[test]
fn telegram_photo_caption_is_capped() {
    let caption = render_telegram_photo_caption_html(
        "Dockrev：更新完成（成功）",
        &"服务有新版本。".repeat(300),
        Some(render_open_link_html(
            "https://dockrev.example.com/services/stk/svc",
            "详情",
        )),
    );
    assert!(caption.chars().count() <= 920);
}

#[test]
fn telegram_photo_caption_truncates_before_html_escape() {
    let caption = render_telegram_photo_caption_html(
        "Dockrev：更新完成（成功）",
        &"服务 & 版本 <latest> 有变化。".repeat(240),
        Some(render_open_link_html(
            "https://dockrev.example.com/services/stk/svc",
            "详情",
        )),
    );
    assert!(caption.chars().count() <= 920);
    assert!(!caption.contains("&amp..."));
    assert!(!caption.contains("&lt..."));
    assert!(!caption.contains("&quot..."));
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

fn assert_png(bytes: &[u8]) {
    assert!(bytes.len() > 4096, "png should not be tiny/empty");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[derive(Clone, Debug)]
struct PixelParityRow {
    name: &'static str,
    width: u32,
    height: u32,
    byte_equal: bool,
    diff_pixels: u64,
    max_channel_delta: u8,
}

#[derive(Clone, Debug)]
struct DynamicParityRow {
    name: &'static str,
    output_file: &'static str,
    diff_file: &'static str,
    width: u32,
    height: u32,
    diff_pixels: u64,
    max_channel_delta: u8,
}

fn assert_png_pixel_equal(name: &'static str, actual: &[u8], expected: &[u8]) -> PixelParityRow {
    let actual_img = image::load_from_memory(actual)
        .unwrap_or_else(|err| panic!("{name} actual png decodes: {err}"))
        .to_rgba8();
    let expected_img = image::load_from_memory(expected)
        .unwrap_or_else(|err| panic!("{name} expected png decodes: {err}"))
        .to_rgba8();

    assert_eq!(
        actual_img.dimensions(),
        expected_img.dimensions(),
        "{name} dimensions must match"
    );

    let mut diff_pixels = 0u64;
    let mut max_channel_delta = 0u8;
    for (actual_pixel, expected_pixel) in actual_img.pixels().zip(expected_img.pixels()) {
        if actual_pixel.0 != expected_pixel.0 {
            diff_pixels += 1;
        }
        for channel in 0..4 {
            max_channel_delta =
                max_channel_delta.max(actual_pixel.0[channel].abs_diff(expected_pixel.0[channel]));
        }
    }

    assert_eq!(diff_pixels, 0, "{name} pixel diff count must be 0");
    assert_eq!(max_channel_delta, 0, "{name} max channel delta must be 0");

    let (width, height) = actual_img.dimensions();
    PixelParityRow {
        name,
        width,
        height,
        byte_equal: actual == expected,
        diff_pixels,
        max_channel_delta,
    }
}

fn dynamic_parity_row(
    name: &'static str,
    output_file: &'static str,
    diff_file: &'static str,
    actual: &[u8],
    expected: &[u8],
) -> (DynamicParityRow, image::RgbaImage) {
    let actual_img = image::load_from_memory(actual)
        .unwrap_or_else(|err| panic!("{name} actual png decodes: {err}"))
        .to_rgba8();
    let expected_img = image::load_from_memory(expected)
        .unwrap_or_else(|err| panic!("{name} expected png decodes: {err}"))
        .to_rgba8();
    assert_eq!(
        actual_img.dimensions(),
        expected_img.dimensions(),
        "{name} dimensions must match"
    );

    let (width, height) = actual_img.dimensions();
    let mut heatmap =
        image::RgbaImage::from_pixel(width, height, image::Rgba([255, 255, 255, 255]));
    let mut diff_pixels = 0u64;
    let mut max_channel_delta = 0u8;

    for y in 0..height {
        for x in 0..width {
            let actual_pixel = actual_img.get_pixel(x, y);
            let expected_pixel = expected_img.get_pixel(x, y);
            let mut pixel_delta = 0u8;
            for channel in 0..4 {
                let delta = actual_pixel.0[channel].abs_diff(expected_pixel.0[channel]);
                pixel_delta = pixel_delta.max(delta);
                max_channel_delta = max_channel_delta.max(delta);
            }
            if pixel_delta > 0 {
                diff_pixels += 1;
                heatmap.put_pixel(x, y, image::Rgba([255, 0, 64, 255]));
            }
        }
    }

    assert_eq!(diff_pixels, 0, "{name} dynamic parity diff count must be 0");
    assert_eq!(
        max_channel_delta, 0,
        "{name} dynamic parity max channel delta must be 0"
    );

    (
        DynamicParityRow {
            name,
            output_file,
            diff_file,
            width,
            height,
            diff_pixels,
            max_channel_delta,
        },
        heatmap,
    )
}

fn diff_pixels(actual: &image::RgbaImage, expected: &image::RgbaImage) -> u64 {
    assert_eq!(actual.dimensions(), expected.dimensions());
    actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(lhs, rhs)| lhs.0 != rhs.0)
        .count() as u64
}

fn assert_dynamic_card_changes_are_slot_bounded(name: &'static str, card: &TelegramCard) {
    let slots = telegram_card_debug_dynamic_text_slots(card).unwrap();
    assert!(!slots.is_empty(), "{name} should have dynamic text slots");

    for slot in &slots {
        assert!(
            slot.rendered_width <= slot.draw.2,
            "{name} {} rendered width {} must fit draw width {}",
            slot.name,
            slot.rendered_width,
            slot.draw.2
        );
    }
    assert!(
        slots.iter().any(|slot| slot.rendered_text.ends_with("...")),
        "{name} should truncate at least one long dynamic slot"
    );

    let actual = image::load_from_memory(&render_telegram_card_png(card).unwrap())
        .unwrap()
        .to_rgba8();
    let reference =
        image::load_from_memory(&render_telegram_card_static_template_png(card.template).unwrap())
            .unwrap()
            .to_rgba8();
    assert_eq!(actual.dimensions(), reference.dimensions());

    let mut outside_slot_diffs = Vec::new();
    for y in 0..actual.height() {
        for x in 0..actual.width() {
            if actual.get_pixel(x, y).0 == reference.get_pixel(x, y).0 {
                continue;
            }
            if !slots
                .iter()
                .any(|slot| rect_contains(slot.draw, x, y) || rect_contains(slot.erase, x, y))
            {
                outside_slot_diffs.push((x, y));
                if outside_slot_diffs.len() >= 8 {
                    break;
                }
            }
        }
        if outside_slot_diffs.len() >= 8 {
            break;
        }
    }

    assert!(
        outside_slot_diffs.is_empty(),
        "{name} changed pixels outside declared slots: {outside_slot_diffs:?}"
    );
}

fn rect_contains(rect: (i32, i32, u32, u32), x: u32, y: u32) -> bool {
    let (rx, ry, rw, rh) = rect;
    let x = x as i32;
    let y = y as i32;
    x >= rx && y >= ry && x < rx + rw as i32 && y < ry + rh as i32
}

fn write_pixel_parity_report(rows: &[PixelParityRow]) {
    let Ok(dir) = std::env::var("DOCKREV_WRITE_TELEGRAM_CARD_EVIDENCE_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let dir = if dir.is_absolute() {
        dir
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(dir)
    };
    std::fs::create_dir_all(&dir).unwrap();

    let mut report = String::from(
        "# Telegram Card Pixel Parity Report\n\n\
         Template assets are compared against the accepted imagegen design reference PNGs after PNG decoding.\n\n\
         | Card | Size | Byte Equal | Diff Pixels | Max Channel Delta |\n\
         | --- | ---: | ---: | ---: | ---: |\n",
    );
    for row in rows {
        report.push_str(&format!(
            "| {} | {}x{} | {} | {} | {} |\n",
            row.name, row.width, row.height, row.byte_equal, row.diff_pixels, row.max_channel_delta
        ));
    }
    std::fs::write(dir.join("pixel-parity-report.md"), report).unwrap();
}

fn write_dynamic_parity_report(rows: &[DynamicParityRow]) {
    let Ok(dir) = std::env::var("DOCKREV_WRITE_TELEGRAM_CARD_EVIDENCE_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let dir = if dir.is_absolute() {
        dir
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(dir)
    };
    std::fs::create_dir_all(&dir).unwrap();

    let mut report = String::from(
        "# Telegram Dynamic Parity Report\n\n\
         Accepted fixture cards are rendered through the dynamic slot renderer, then compared against the original design references after PNG decoding.\n\n\
         | Card | Size | Dynamic Output | Diff Heatmap | Diff Pixels | Max Channel Delta |\n\
         | --- | ---: | --- | --- | ---: | ---: |\n",
    );
    for row in rows {
        report.push_str(&format!(
            "| {} | {}x{} | [{}]({}) | [{}]({}) | {} | {} |\n",
            row.name,
            row.width,
            row.height,
            row.output_file,
            row.output_file,
            row.diff_file,
            row.diff_file,
            row.diff_pixels,
            row.max_channel_delta
        ));
    }
    std::fs::write(dir.join("dynamic-parity-report.md"), report).unwrap();
}

#[test]
fn telegram_card_renderer_generates_all_notification_cards() {
    let links = finalize_job_links(
        "https://dockrev.example.com/queue/job_123".to_string(),
        vec![make_service_url(1)],
        false,
        None,
    );
    let job = sample_job_payload(links);
    assert_png(&render_job_telegram_card_png(&job, None).unwrap());

    let new_version = sample_new_version_payload();
    assert_png(&render_new_version_telegram_card_png(&new_version).unwrap());

    let ghcr = sample_ghcr_anomaly_payload();
    assert_png(&render_ghcr_webhook_anomaly_telegram_card_png(&ghcr).unwrap());

    let test = build_test_payload_v2(
        "2026-03-05T04:44:59.673686721Z",
        "dockrev: test notification",
        Some(NotificationTestChannel::Telegram),
        NotificationTestChannel::Telegram,
        "0.1.0",
        "https://dockrev.example.com/settings",
    );
    let test_png = render_test_telegram_card_png(&test).unwrap();
    assert_png(&test_png);

    if let Ok(dir) = std::env::var("DOCKREV_WRITE_TELEGRAM_CARD_EVIDENCE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let dir = if dir.is_absolute() {
            dir
        } else {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(dir)
        };
        std::fs::create_dir_all(&dir).unwrap();
        let fixture_job = accepted_design_fixture_card(CardTemplate::JobFinished);
        let fixture_new_version = accepted_design_fixture_card(CardTemplate::NewVersion);
        let fixture_ghcr = accepted_design_fixture_card(CardTemplate::GhcrAnomaly);
        let fixture_test = accepted_design_fixture_card(CardTemplate::TestNotification);
        std::fs::write(
            dir.join("job-finished-card.png"),
            render_telegram_card_png(&fixture_job).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("new-version-card.png"),
            render_telegram_card_png(&fixture_new_version).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("ghcr-anomaly-card.png"),
            render_telegram_card_png(&fixture_ghcr).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("test-notification-card.png"),
            render_telegram_card_png(&fixture_test).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("job-finished-dynamic-card.png"),
            render_job_telegram_card_png(&job, None).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("new-version-dynamic-card.png"),
            render_new_version_telegram_card_png(&new_version).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("ghcr-anomaly-dynamic-card.png"),
            render_ghcr_webhook_anomaly_telegram_card_png(&ghcr).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("test-notification-dynamic-card.png"),
            render_test_telegram_card_png(&test).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn telegram_card_template_assets_match_accepted_design_templates() {
    assert_eq!(
        include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/job-finished-design-reference.png")
            .as_slice(),
        include_bytes!("../../assets/telegram-card-template-job-finished.png").as_slice()
    );

    assert_eq!(
        include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/imagegen-design-reference.png")
            .as_slice(),
        include_bytes!("../../assets/telegram-card-template-new-version.png").as_slice()
    );

    assert_eq!(
        include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/ghcr-anomaly-design-reference.png")
            .as_slice(),
        include_bytes!("../../assets/telegram-card-template-ghcr-anomaly.png").as_slice()
    );

    assert_eq!(
        include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/test-notification-design-reference.png")
            .as_slice(),
        include_bytes!("../../assets/telegram-card-template-test.png").as_slice()
    );
}

#[test]
fn telegram_card_template_assets_pixel_match_accepted_design_templates() {
    let rows = vec![
        assert_png_pixel_equal(
            "job_finished",
            include_bytes!("../../assets/telegram-card-template-job-finished.png").as_slice(),
            include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/job-finished-design-reference.png")
                .as_slice(),
        ),
        assert_png_pixel_equal(
            "new_version",
            include_bytes!("../../assets/telegram-card-template-new-version.png").as_slice(),
            include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/imagegen-design-reference.png")
                .as_slice(),
        ),
        assert_png_pixel_equal(
            "ghcr_anomaly",
            include_bytes!("../../assets/telegram-card-template-ghcr-anomaly.png").as_slice(),
            include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/ghcr-anomaly-design-reference.png")
                .as_slice(),
        ),
        assert_png_pixel_equal(
            "notification_test",
            include_bytes!("../../assets/telegram-card-template-test.png").as_slice(),
            include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/test-notification-design-reference.png")
                .as_slice(),
        ),
    ];

    for row in &rows {
        println!(
            "{}: size={}x{}, byte_equal={}, diff_pixels={}, max_channel_delta={}",
            row.name, row.width, row.height, row.byte_equal, row.diff_pixels, row.max_channel_delta
        );
    }
    write_pixel_parity_report(&rows);
}

#[test]
fn telegram_card_renderer_replaces_dynamic_metadata_slots() {
    let links = finalize_job_links(
        "https://dockrev.example.com/queue/job_123".to_string(),
        vec![make_service_url(1)],
        false,
        None,
    );
    let job = sample_job_payload(links);
    let new_version = sample_new_version_payload();
    let ghcr = sample_ghcr_anomaly_payload();
    let test = build_test_payload_v2(
        "2026-03-05T04:44:59.673686721Z",
        "dockrev: test notification",
        Some(NotificationTestChannel::Telegram),
        NotificationTestChannel::Telegram,
        "0.1.0",
        "https://dockrev.example.com/settings",
    );

    let cases = [
        (
            "job_finished",
            render_job_telegram_card_png(&job, None).unwrap(),
            CardTemplate::JobFinished,
        ),
        (
            "new_version",
            render_new_version_telegram_card_png(&new_version).unwrap(),
            CardTemplate::NewVersion,
        ),
        (
            "ghcr_anomaly",
            render_ghcr_webhook_anomaly_telegram_card_png(&ghcr).unwrap(),
            CardTemplate::GhcrAnomaly,
        ),
        (
            "notification_test",
            render_test_telegram_card_png(&test).unwrap(),
            CardTemplate::TestNotification,
        ),
    ];

    for (name, actual, template) in cases {
        assert_png(&actual);
        assert_ne!(
            actual,
            render_telegram_card_static_template_png(template).unwrap(),
            "{name} should replace at least one text slot for non-fixture metadata"
        );
    }
}

#[test]
fn telegram_card_design_fixture_cards_render_accepted_designs() {
    let cases = [
        (
            "job_finished",
            CardTemplate::JobFinished,
            "job-finished-dynamic-parity-card.png",
            "job-finished-dynamic-parity-diff.png",
            include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/job-finished-design-reference.png")
                .as_slice(),
        ),
        (
            "new_version",
            CardTemplate::NewVersion,
            "new-version-dynamic-parity-card.png",
            "new-version-dynamic-parity-diff.png",
            include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/imagegen-design-reference.png")
                .as_slice(),
        ),
        (
            "ghcr_anomaly",
            CardTemplate::GhcrAnomaly,
            "ghcr-anomaly-dynamic-parity-card.png",
            "ghcr-anomaly-dynamic-parity-diff.png",
            include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/ghcr-anomaly-design-reference.png")
                .as_slice(),
        ),
        (
            "notification_test",
            CardTemplate::TestNotification,
            "test-notification-dynamic-parity-card.png",
            "test-notification-dynamic-parity-diff.png",
            include_bytes!("../../../../docs/specs/tgc9m-telegram-dynamic-notification-cards/assets/test-notification-design-reference.png")
                .as_slice(),
        ),
    ];
    let mut rows = Vec::new();

    let evidence_dir = std::env::var("DOCKREV_WRITE_TELEGRAM_CARD_EVIDENCE_DIR")
        .ok()
        .map(|dir| {
            let dir = std::path::PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(dir)
            }
        });
    if let Some(dir) = &evidence_dir {
        std::fs::create_dir_all(dir).unwrap();
    }

    for (name, template, output_file, diff_file, expected) in cases {
        let card = accepted_design_fixture_card(template);
        let actual = render_telegram_card_png(&card).unwrap();
        assert_png_pixel_equal(name, &actual, expected);
        let debug_slots = telegram_card_debug_dynamic_text_slots(&card).unwrap();
        assert!(
            !debug_slots.is_empty(),
            "{name} parity fixture must still traverse text slots"
        );
        let (row, heatmap) = dynamic_parity_row(name, output_file, diff_file, &actual, expected);
        if let Some(dir) = &evidence_dir {
            std::fs::write(dir.join(output_file), &actual).unwrap();
            heatmap.save(dir.join(diff_file)).unwrap();
        }
        rows.push(row);
    }

    write_dynamic_parity_report(&rows);
}

#[test]
fn telegram_card_static_fixture_requires_exact_design_metadata() {
    let mut card = accepted_design_fixture_card(CardTemplate::NewVersion);
    card.subject = "1.0.0 -> latest".to_string();
    let err = render_static_design_fixture_card_png(&card).unwrap_err();
    assert!(
        err.to_string()
            .contains("does not match the accepted static design fixture")
    );
}

#[test]
fn telegram_card_dynamic_renderer_handles_long_text_without_overflowing_slots() {
    let mut job = accepted_design_fixture_card(CardTemplate::JobFinished);
    job.title =
        "enterprise-platform-with-long-stack-name / api-gateway-worker-with-long-service-name 更新完成"
            .to_string();
    job.subject =
        "enterprise-platform-with-long-stack-name / api-gateway-worker-with-long-service-name"
            .to_string();
    job.metrics[0] = pair(
        "变更",
        "enterprise-platform-with-long-stack-name / api-gateway-worker-with-long-service-name",
    );
    job.metrics[2] = pair("", "job_update_with_a_very_long_identifier_1234567890");
    job.rows[1] = pair(
        "服务",
        "enterprise-platform-with-long-stack-name / api-gateway-worker-with-long-service-name",
    );

    let mut new_version = accepted_design_fixture_card(CardTemplate::NewVersion);
    new_version.title =
        "blog / api-with-a-very-long-service-name-that-should-not-overflow 服务有新版本"
            .to_string();
    new_version.subject =
        "v2026.07.06-super-long-current-build-tag -> v2026.07.06-even-longer-candidate-build-tag"
            .to_string();
    new_version.metrics[0] = pair("检查", "123456789 个服务");
    new_version.metrics[2] = pair("", "job_check_with_a_very_long_identifier_1234567890");
    new_version.rows[1] = pair(
        "服务",
        "enterprise-platform-with-long-stack-name / api-gateway-worker-with-long-service-name",
    );
    new_version.rows[2] = pair(
        "版本变更",
        "v2026.07.06-super-long-current-build-tag -> v2026.07.06-even-longer-candidate-build-tag",
    );

    let mut ghcr = accepted_design_fixture_card(CardTemplate::GhcrAnomaly);
    ghcr.subject = "123 missing / 456 conflict / 789 error".to_string();
    ghcr.metrics[0] = pair("缺失", "123456789 个");
    ghcr.metrics[1] = pair("冲突", "456789123 个");
    ghcr.metrics[2] = pair("", "job_ghcr_with_a_very_long_identifier_1234567890");
    ghcr.rows[1] = pair("异常摘要", "123 missing / 456 conflict / 789 error");
    ghcr.rows[2] = pair(
        "仓库",
        "organization-with-long-name/service-with-long-name [missing]",
    );

    let mut test = accepted_design_fixture_card(CardTemplate::TestNotification);
    test.metrics[0] = pair("请求", "telegram-with-a-very-long-request-channel");
    test.metrics[2] = pair("", "2026.07.06-super-long-version-tag");
    test.rows[2] = pair("应用版本", "2026.07.06-super-long-version-tag");
    test.rows[3] = pair("发送时间", "2026-03-05 04:44:59");

    for (name, card) in [
        ("job_finished", job),
        ("new_version", new_version),
        ("ghcr_anomaly", ghcr),
        ("notification_test", test),
    ] {
        let png = render_telegram_card_png(&card).unwrap();
        assert_png(&png);
        assert_ne!(
            png,
            render_telegram_card_static_template_png(card.template).unwrap()
        );
        assert_dynamic_card_changes_are_slot_bounded(name, &card);
    }
}

#[test]
fn telegram_card_static_template_rebuilds_accepted_design_from_base_and_layers() {
    let cases = [
        ("job_finished", CardTemplate::JobFinished),
        ("new_version", CardTemplate::NewVersion),
        ("ghcr_anomaly", CardTemplate::GhcrAnomaly),
        ("notification_test", CardTemplate::TestNotification),
    ];
    for (name, template) in cases {
        let static_template = build_static_telegram_card_template(template).unwrap();
        let restored = compose_static_template(&static_template).unwrap();
        let base_diff_pixels = diff_pixels(&static_template.base, &static_template.reference);
        let restored_diff_pixels = diff_pixels(&restored, &static_template.reference);

        assert_eq!(static_template.template, template);
        assert!(
            static_template.layers.len() >= 8,
            "{name} should expose multiple template layers"
        );
        assert!(
            static_template
                .layers
                .iter()
                .all(|layer| !layer.name.trim().is_empty()),
            "{name} template layers should carry stable names"
        );
        assert!(
            base_diff_pixels > 0,
            "{name} base must differ from the reference after blanking content layers"
        );
        assert_eq!(
            restored_diff_pixels, 0,
            "{name} recomposed output must restore the accepted reference exactly"
        );
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
