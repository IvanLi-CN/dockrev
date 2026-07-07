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
