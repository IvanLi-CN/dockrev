use super::*;

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

#[test]
fn telegram_new_version_card_erases_reference_text_before_drawing_live_values() {
    let mut payload = sample_new_version_payload();
    payload.sent_at = "2026-07-07T04:01:13Z".to_string();
    payload.check.job_id = "chk_01KWXBQ5GXXR2QPHEWT3GATNPB".to_string();
    payload.check.services_checked = 74;
    payload.links.job_url =
        "https://dockrev.example.com/queue/chk_01KWXBQ5GXXR2QPHEWT3GATNPB".to_string();
    payload.links.primary_url =
        "https://dockrev.example.com/services/docker-mod/dockrev-supervisor".to_string();
    let service = payload.links.service_urls.first_mut().unwrap();
    service.stack_id = "docker-mod".to_string();
    service.stack_name = "docker-mod".to_string();
    service.service_id = "dockrev-supervisor".to_string();
    service.service_name = "dockrev-supervisor".to_string();
    service.current_display_tag = Some("0.48.0".to_string());
    service.candidate_display_tag = Some("0.55.0".to_string());
    service.url = payload.links.primary_url.clone();
    payload.human.title = "docker-mod / dockrev-supervisor 服务有新版本".to_string();
    payload.human.summary =
        "docker-mod / dockrev-supervisor 服务有新版本（0.48.0 -> 0.55.0）。".to_string();

    let png = render_new_version_telegram_card_png(&payload).unwrap();
    assert_png(&png);
    write_debug_card("new-version-live-regression-card.png", &png);
    let image = image::load_from_memory(&png).unwrap().to_rgba8();

    assert_no_text_ink(&image, (292, 208, 12, 70), "title-left-reference-remnant");
    assert_no_text_ink(&image, (292, 292, 12, 42), "subject-left-reference-remnant");
    assert_no_text_ink(
        &image,
        (342, 398, 30, 50),
        "metric-1-left-reference-remnant",
    );
}

fn assert_no_text_ink(image: &image::RgbaImage, rect: (u32, u32, u32, u32), label: &'static str) {
    let (x, y, w, h) = rect;
    let ink_pixels = (y..y + h)
        .flat_map(|yy| (x..x + w).map(move |xx| image.get_pixel(xx, yy)))
        .filter(|pixel| is_text_ink(pixel.0))
        .count();
    assert_eq!(
        ink_pixels, 0,
        "{label} should not contain old template text ink"
    );
}

fn is_text_ink([r, g, b, a]: [u8; 4]) -> bool {
    if a == 0 {
        return false;
    }
    (r < 80 && g < 110 && b < 140) || (r < 80 && g < 150 && b > 150)
}

fn write_debug_card(file_name: &str, bytes: &[u8]) {
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
    std::fs::write(dir.join(file_name), bytes).unwrap();
}
