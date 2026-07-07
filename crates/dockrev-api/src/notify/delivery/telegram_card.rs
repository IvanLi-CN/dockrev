use super::*;

use std::io::Cursor;

use ab_glyph::{FontArc, PxScale};
use image::{ImageFormat, Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};

const FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/NotoSansCJKsc-Regular.otf");
const JOB_FINISHED_TEMPLATE_BYTES: &[u8] =
    include_bytes!("../../../assets/telegram-card-template-job-finished.png");
const NEW_VERSION_TEMPLATE_BYTES: &[u8] =
    include_bytes!("../../../assets/telegram-card-template-new-version.png");
const GHCR_ANOMALY_TEMPLATE_BYTES: &[u8] =
    include_bytes!("../../../assets/telegram-card-template-ghcr-anomaly.png");
const TEST_TEMPLATE_BYTES: &[u8] =
    include_bytes!("../../../assets/telegram-card-template-test.png");

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CardPair {
    pub(crate) label: String,
    pub(crate) value: String,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TelegramCard {
    pub(crate) template: CardTemplate,
    pub(crate) kind: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) subject: String,
    pub(crate) metrics: Vec<CardPair>,
    pub(crate) rows: Vec<CardPair>,
    pub(crate) omitted: u32,
    pub(crate) accent: Rgba<u8>,
    pub(crate) icon: CardIcon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardTemplate {
    JobFinished,
    NewVersion,
    GhcrAnomaly,
    TestNotification,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardIcon {
    Package,
    Check,
    Alert,
    Bell,
}

pub(crate) fn render_job_telegram_card_png(
    payload: &JobNotificationPayloadV2,
    error_excerpt: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let status_label = update_job_status_label_zh(&payload.job.status).to_string();
    let accent = match payload.job.status.as_str() {
        "success" => rgba(38, 203, 124),
        "failed" => rgba(255, 91, 116),
        "rolled_back" => rgba(255, 181, 67),
        _ => rgba(80, 166, 255),
    };
    let subject = if payload.links.service_urls.len() == 1 {
        let svc = &payload.links.service_urls[0];
        format!("{} / {}", svc.stack_name, svc.service_name)
    } else if payload.links.service_urls.is_empty() {
        format!("任务 {}", payload.job.id)
    } else {
        format!(
            "{} 个服务",
            payload.links.service_urls.len()
                + payload.links.truncated.service_urls_omitted as usize
        )
    };

    let kind = if payload.job.r#type == "rollback" {
        "回滚任务".to_string()
    } else {
        "更新任务".to_string()
    };

    let metrics = vec![
        pair("变更", &subject),
        pair("结果", &status_label),
        pair("", &payload.job.id),
    ];
    let mut rows = vec![
        pair("通知类型", &kind),
        pair("服务", &subject),
        pair("更新结果", &status_label),
        pair(
            "完成时间",
            &format_card_time(
                payload
                    .job
                    .finished_at
                    .as_deref()
                    .unwrap_or(&payload.sent_at),
            ),
        ),
    ];
    if error_excerpt.is_some() {
        rows[2] = pair("错误", "完整错误详情见文字消息");
    }

    render_telegram_card_png(&TelegramCard {
        template: CardTemplate::JobFinished,
        kind,
        status: status_label,
        title: format!("{subject} 更新完成"),
        subject,
        metrics,
        rows,
        omitted: payload.links.truncated.service_urls_omitted,
        accent,
        icon: if payload.job.status == "success" {
            CardIcon::Check
        } else {
            CardIcon::Package
        },
    })
}

pub(crate) fn render_new_version_telegram_card_png(
    payload: &NewVersionNotificationPayloadV2,
) -> anyhow::Result<Vec<u8>> {
    let subject = if payload.links.service_urls.len() == 1 {
        let svc = &payload.links.service_urls[0];
        new_version_transition(svc)
            .unwrap_or_else(|| format!("{} / {}", svc.stack_name, svc.service_name))
    } else {
        format!("{} 个服务发现新版本", payload.check.new_versions)
    };
    let metrics = vec![
        pair(
            "检查",
            &format!("{} 个服务", payload.check.services_checked),
        ),
        pair("发现", &format!("{} 个新版本", payload.check.new_versions)),
        pair("", &payload.check.job_id),
    ];
    let service_labels = payload
        .links
        .service_urls
        .iter()
        .map(|svc| format!("{} / {}", svc.stack_name, svc.service_name))
        .collect::<Vec<_>>();
    let service_label = summarize_card_items(
        &service_labels,
        payload.links.truncated.service_urls_omitted,
        format!("{} 个服务", payload.check.new_versions),
    );
    let version_changes = payload
        .links
        .service_urls
        .iter()
        .filter_map(new_version_transition)
        .collect::<Vec<_>>();
    let version_change = summarize_card_items(
        &version_changes,
        payload.links.truncated.service_urls_omitted,
        format!("{} 个新版本", payload.check.new_versions),
    );
    let rows = vec![
        pair("通知类型", "发现新版本"),
        pair("服务", &service_label),
        pair("版本变更", &version_change),
        pair("发现时间", &format_card_time(&payload.sent_at)),
    ];

    render_telegram_card_png(&TelegramCard {
        template: CardTemplate::NewVersion,
        kind: "发现新版本".to_string(),
        status: "NEW".to_string(),
        title: payload.human.title.clone(),
        subject,
        metrics,
        rows,
        omitted: payload.links.truncated.service_urls_omitted,
        accent: rgba(15, 103, 217),
        icon: CardIcon::Package,
    })
}

pub(crate) fn render_ghcr_webhook_anomaly_telegram_card_png(
    payload: &GhcrWebhookAnomalyPayloadV2,
) -> anyhow::Result<Vec<u8>> {
    let anomaly_summary = format!(
        "{} missing / {} conflict / {} error",
        payload.job.missing, payload.job.conflict, payload.job.error
    );
    let metrics = vec![
        pair("缺失", &format!("{} 个", payload.job.missing)),
        pair("冲突", &format!("{} 个", payload.job.conflict)),
        pair("", &payload.job.id),
    ];
    let mut rows = vec![
        pair("通知类型", "Webhook 巡检"),
        pair("异常摘要", &anomaly_summary),
    ];
    let repo_summaries = payload
        .links
        .repos
        .iter()
        .map(|repo| format!("{} [{}]", repo.full_name, repo.state))
        .collect::<Vec<_>>();
    let repo_summary = summarize_card_items(
        &repo_summaries,
        payload.links.truncated.repos_omitted,
        "无仓库详情".to_string(),
    );
    rows.push(pair("仓库", &repo_summary));
    rows.push(pair("巡检时间", &format_card_time(&payload.sent_at)));

    render_telegram_card_png(&TelegramCard {
        template: CardTemplate::GhcrAnomaly,
        kind: "Webhook 巡检".to_string(),
        status: "异常".to_string(),
        title: "GHCR Webhook 巡检异常".to_string(),
        subject: anomaly_summary,
        metrics,
        rows,
        omitted: payload.links.truncated.repos_omitted,
        accent: rgba(255, 91, 116),
        icon: CardIcon::Alert,
    })
}

pub(crate) fn render_test_telegram_card_png(
    payload: &TestNotificationPayloadV2,
) -> anyhow::Result<Vec<u8>> {
    render_telegram_card_png(&TelegramCard {
        template: CardTemplate::TestNotification,
        kind: "通知测试".to_string(),
        status: notification_channel_label(NotificationTestChannel::Telegram).to_string(),
        title: "Telegram 测试通知".to_string(),
        subject: "通知渠道可用".to_string(),
        metrics: vec![
            pair("请求", payload.debug.requested_channel.unwrap_or("all")),
            pair("目标", "Telegram"),
            pair("", &payload.debug.app_version),
        ],
        rows: vec![
            pair("通知类型", "通知测试"),
            pair("目标渠道", "Telegram"),
            pair("应用版本", &payload.debug.app_version),
            pair("发送时间", &format_card_time(&payload.sent_at)),
        ],
        omitted: 0,
        accent: rgba(126, 95, 255),
        icon: CardIcon::Bell,
    })
}

pub(crate) fn render_telegram_card_png(card: &TelegramCard) -> anyhow::Result<Vec<u8>> {
    render_dynamic_template_card_png(card)
}

pub(crate) fn render_telegram_card_static_template_png(
    template: CardTemplate,
) -> anyhow::Result<Vec<u8>> {
    let template = build_static_telegram_card_template(template)?;
    encode_card_png(compose_static_template(&template)?)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn render_static_design_fixture_card_png(
    card: &TelegramCard,
) -> anyhow::Result<Vec<u8>> {
    let expected = accepted_design_fixture_card(card.template);
    anyhow::ensure!(
        *card == expected,
        "card metadata does not match the accepted static design fixture for {:?}",
        card.template
    );
    render_telegram_card_static_template_png(card.template)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn accepted_design_fixture_card(template: CardTemplate) -> TelegramCard {
    match template {
        CardTemplate::JobFinished => TelegramCard {
            template,
            kind: "更新完成".to_string(),
            status: "成功".to_string(),
            title: "blog / api 更新完成".to_string(),
            subject: "1.0.0 -> 1.1.0".to_string(),
            metrics: vec![
                pair("变更", "1 个服务"),
                pair("结果", "成功"),
                pair("", "job_update_123"),
            ],
            rows: vec![
                pair("通知类型", "更新完成"),
                pair("服务", "blog / api"),
                pair("更新结果", "成功"),
                pair("完成时间", "2024-05-26 14:32:10"),
            ],
            omitted: 0,
            accent: rgba(38, 203, 124),
            icon: CardIcon::Package,
        },
        CardTemplate::NewVersion => TelegramCard {
            template,
            kind: "发现新版本".to_string(),
            status: "NEW".to_string(),
            title: "blog / api 服务有新版本".to_string(),
            subject: "1.0.0 -> 1.1.0".to_string(),
            metrics: vec![
                pair("检查", "12 个服务"),
                pair("发现", "1 个新版本"),
                pair("", "job_check_123"),
            ],
            rows: vec![
                pair("通知类型", "发现新版本"),
                pair("服务", "blog / api"),
                pair("版本变更", "1.0.0 -> 1.1.0"),
                pair("发现时间", "2024-05-26 14:32:10"),
            ],
            omitted: 0,
            accent: rgba(15, 103, 217),
            icon: CardIcon::Package,
        },
        CardTemplate::GhcrAnomaly => TelegramCard {
            template,
            kind: "Webhook 巡检".to_string(),
            status: "异常".to_string(),
            title: "GHCR Webhook 巡检异常".to_string(),
            subject: "2 missing / 1 conflict / 1 error".to_string(),
            metrics: vec![
                pair("缺失", "2 个"),
                pair("冲突", "1 个"),
                pair("", "job_ghcr_123"),
            ],
            rows: vec![
                pair("通知类型", "Webhook 巡检"),
                pair("异常摘要", "2 missing / 1 conflict / 1 error"),
                pair("仓库", "ghcr.io/acme/api [missing]"),
                pair("巡检时间", "2024-05-26 14:32:10"),
            ],
            omitted: 0,
            accent: rgba(255, 91, 116),
            icon: CardIcon::Alert,
        },
        CardTemplate::TestNotification => TelegramCard {
            template,
            kind: "通知测试".to_string(),
            status: "Telegram".to_string(),
            title: "Telegram 测试通知".to_string(),
            subject: "通知渠道可用".to_string(),
            metrics: vec![
                pair("请求", "all"),
                pair("目标", "Telegram"),
                pair("", "0.1.0"),
            ],
            rows: vec![
                pair("通知类型", "通知测试"),
                pair("目标渠道", "Telegram"),
                pair("应用版本", "0.1.0"),
                pair("发送时间", "2024-05-26 14:32:10"),
            ],
            omitted: 0,
            accent: rgba(126, 95, 255),
            icon: CardIcon::Bell,
        },
    }
}

fn render_dynamic_template_card_png(card: &TelegramCard) -> anyhow::Result<Vec<u8>> {
    let font = FontArc::try_from_slice(FONT_BYTES).context("load telegram card font")?;
    let reference = decode_template_reference(card.template)?;
    let mut img = reference.clone();
    let profile = card_profile(card.template);
    let reference_card = accepted_design_fixture_card(card.template);

    for slot in scaled_text_slots(card.template, profile) {
        let text = text_for_slot(card, slot.kind);
        let reference_text = text_for_slot(&reference_card, slot.kind);

        if text.is_none() && reference_text.is_none() {
            continue;
        }
        erase_text_slot(&mut img, &slot);

        match text {
            Some(text) if reference_text.as_deref() == Some(text.as_str()) => {
                restore_reference_text_slot(&mut img, &reference, &slot);
            }
            Some(text) => {
                draw_text_slot(&mut img, &font, &slot, &text, card);
            }
            None => {}
        }

        if matches!(
            (card.template, slot.kind),
            (CardTemplate::NewVersion, TextSlotKind::MetricValue(0))
        ) {
            restore_reference_patch(&mut img, &reference, rect(276, 408, 74, 44));
        }
    }

    encode_card_png(img)
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct TelegramCardDebugTextSlot {
    pub(crate) name: &'static str,
    pub(crate) draw: (i32, i32, u32, u32),
    pub(crate) erase: (i32, i32, u32, u32),
    pub(crate) rendered_text: String,
    pub(crate) rendered_width: u32,
}

#[cfg(test)]
pub(crate) fn telegram_card_debug_dynamic_text_slots(
    card: &TelegramCard,
) -> anyhow::Result<Vec<TelegramCardDebugTextSlot>> {
    let font = FontArc::try_from_slice(FONT_BYTES).context("load telegram card font")?;
    let reference_card = accepted_design_fixture_card(card.template);
    let profile = card_profile(card.template);
    let mut debug_slots = Vec::new();

    for slot in scaled_text_slots(card.template, profile) {
        let text = text_for_slot(card, slot.kind);
        let reference_text = text_for_slot(&reference_card, slot.kind);

        if text.is_none() && reference_text.is_none() {
            continue;
        }
        let Some(text) = text else {
            continue;
        };

        let scale = fit_text_scale(&font, &text, slot.draw.w, slot.size, slot.min_size);
        let rendered_text = truncate_text_to_width(&font, scale, &text, slot.draw.w);
        let (rendered_width, _) = text_size(scale, &font, &rendered_text);
        debug_slots.push(TelegramCardDebugTextSlot {
            name: text_slot_name(slot.kind),
            draw: rect_tuple(slot.draw),
            erase: rect_tuple(slot.erase),
            rendered_text,
            rendered_width,
        });
    }

    Ok(debug_slots)
}

fn card_template_bytes(template: CardTemplate) -> &'static [u8] {
    match template {
        CardTemplate::JobFinished => JOB_FINISHED_TEMPLATE_BYTES,
        CardTemplate::NewVersion => NEW_VERSION_TEMPLATE_BYTES,
        CardTemplate::GhcrAnomaly => GHCR_ANOMALY_TEMPLATE_BYTES,
        CardTemplate::TestNotification => TEST_TEMPLATE_BYTES,
    }
}

#[derive(Clone, Copy, Debug)]
struct TemplateBlockSpec {
    name: &'static str,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    #[allow(dead_code)]
    sample_x: i32,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug)]
pub(crate) struct StaticTelegramCardLayer {
    pub(crate) name: &'static str,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) image: RgbaImage,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug)]
pub(crate) struct StaticTelegramCardTemplate {
    pub(crate) template: CardTemplate,
    pub(crate) reference: RgbaImage,
    pub(crate) base: RgbaImage,
    pub(crate) layers: Vec<StaticTelegramCardLayer>,
}

const GENERIC_TEMPLATE_BLOCKS: &[TemplateBlockSpec] = &[
    TemplateBlockSpec {
        name: "header_kind",
        x: 454,
        y: 54,
        w: 332,
        h: 76,
        sample_x: 850,
    },
    TemplateBlockSpec {
        name: "status_pill",
        x: 1432,
        y: 58,
        w: 172,
        h: 62,
        sample_x: 1518,
    },
    TemplateBlockSpec {
        name: "hero",
        x: 118,
        y: 214,
        w: 768,
        h: 140,
        sample_x: 1210,
    },
    TemplateBlockSpec {
        name: "metric_1",
        x: 114,
        y: 400,
        w: 468,
        h: 112,
        sample_x: 508,
    },
    TemplateBlockSpec {
        name: "metric_2",
        x: 604,
        y: 400,
        w: 468,
        h: 112,
        sample_x: 998,
    },
    TemplateBlockSpec {
        name: "metric_3",
        x: 1096,
        y: 400,
        w: 468,
        h: 112,
        sample_x: 1488,
    },
    TemplateBlockSpec {
        name: "fact_1",
        x: 80,
        y: 596,
        w: 1510,
        h: 64,
        sample_x: 1010,
    },
    TemplateBlockSpec {
        name: "fact_2",
        x: 80,
        y: 670,
        w: 1510,
        h: 64,
        sample_x: 1010,
    },
    TemplateBlockSpec {
        name: "fact_3",
        x: 80,
        y: 744,
        w: 1510,
        h: 64,
        sample_x: 1010,
    },
    TemplateBlockSpec {
        name: "fact_4",
        x: 80,
        y: 818,
        w: 1510,
        h: 64,
        sample_x: 1010,
    },
];

#[derive(Clone, Copy, Debug)]
struct TextSlotSpec {
    kind: TextSlotKind,
    draw: TextRect,
    erase: TextRect,
    fill_x: i32,
    size: f32,
    min_size: f32,
    align: TextAlign,
}

#[derive(Clone, Copy, Debug)]
struct TextRect {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

#[derive(Clone, Copy, Debug)]
enum TextSlotKind {
    HeaderKind,
    Status,
    Title,
    Subject,
    MetricLabel(usize),
    MetricValue(usize),
    RowLabel(usize),
    RowValue(usize),
}

#[derive(Clone, Copy, Debug)]
enum TextAlign {
    Left,
    Center,
    Right,
}

const GENERIC_TEXT_SLOTS: &[TextSlotSpec] = &[
    TextSlotSpec {
        kind: TextSlotKind::HeaderKind,
        draw: rect(608, 100, 360, 44),
        erase: rect(596, 84, 255, 64),
        fill_x: 940,
        size: 32.0,
        min_size: 22.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::Status,
        draw: rect(1472, 92, 120, 42),
        erase: rect(1484, 82, 104, 48),
        fill_x: 1446,
        size: 31.0,
        min_size: 19.0,
        align: TextAlign::Center,
    },
    TextSlotSpec {
        kind: TextSlotKind::Title,
        draw: rect(304, 246, 930, 56),
        erase: rect(286, 230, 900, 80),
        fill_x: 1280,
        size: 46.0,
        min_size: 30.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::Subject,
        draw: rect(304, 324, 800, 42),
        erase: rect(286, 306, 760, 58),
        fill_x: 1120,
        size: 34.0,
        min_size: 23.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::MetricLabel(0),
        draw: rect(288, 420, 220, 34),
        erase: rect(280, 414, 190, 44),
        fill_x: 520,
        size: 25.0,
        min_size: 18.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::MetricValue(0),
        draw: rect(288, 462, 280, 42),
        erase: rect(286, 448, 270, 58),
        fill_x: 535,
        size: 32.0,
        min_size: 22.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::MetricLabel(1),
        draw: rect(762, 420, 220, 34),
        erase: rect(752, 414, 190, 44),
        fill_x: 1000,
        size: 25.0,
        min_size: 18.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::MetricValue(1),
        draw: rect(762, 462, 280, 42),
        erase: rect(760, 448, 270, 58),
        fill_x: 1015,
        size: 32.0,
        min_size: 22.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::MetricLabel(2),
        draw: rect(1242, 420, 310, 34),
        erase: rect(1232, 414, 306, 44),
        fill_x: 1212,
        size: 25.0,
        min_size: 18.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::MetricValue(2),
        draw: rect(1242, 450, 320, 42),
        erase: rect(1232, 436, 280, 58),
        fill_x: 1212,
        size: 30.0,
        min_size: 20.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::RowLabel(0),
        draw: rect(220, 586, 390, 36),
        erase: rect(210, 576, 320, 48),
        fill_x: 620,
        size: 27.0,
        min_size: 19.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::RowValue(0),
        draw: rect(900, 568, 620, 36),
        erase: rect(820, 540, 730, 92),
        fill_x: 860,
        size: 25.0,
        min_size: 18.0,
        align: TextAlign::Right,
    },
    TextSlotSpec {
        kind: TextSlotKind::RowLabel(1),
        draw: rect(220, 660, 390, 36),
        erase: rect(210, 650, 320, 48),
        fill_x: 620,
        size: 27.0,
        min_size: 19.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::RowValue(1),
        draw: rect(900, 642, 620, 36),
        erase: rect(820, 614, 730, 92),
        fill_x: 860,
        size: 25.0,
        min_size: 18.0,
        align: TextAlign::Right,
    },
    TextSlotSpec {
        kind: TextSlotKind::RowLabel(2),
        draw: rect(220, 734, 390, 36),
        erase: rect(210, 724, 320, 48),
        fill_x: 620,
        size: 27.0,
        min_size: 19.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::RowValue(2),
        draw: rect(900, 716, 620, 36),
        erase: rect(820, 688, 730, 92),
        fill_x: 860,
        size: 25.0,
        min_size: 18.0,
        align: TextAlign::Right,
    },
    TextSlotSpec {
        kind: TextSlotKind::RowLabel(3),
        draw: rect(220, 808, 390, 36),
        erase: rect(210, 798, 320, 48),
        fill_x: 620,
        size: 27.0,
        min_size: 19.0,
        align: TextAlign::Left,
    },
    TextSlotSpec {
        kind: TextSlotKind::RowValue(3),
        draw: rect(900, 790, 540, 36),
        erase: rect(820, 752, 640, 94),
        fill_x: 860,
        size: 25.0,
        min_size: 18.0,
        align: TextAlign::Right,
    },
];

const fn rect(x: i32, y: i32, w: u32, h: u32) -> TextRect {
    TextRect { x, y, w, h }
}

#[cfg(test)]
fn rect_tuple(rect: TextRect) -> (i32, i32, u32, u32) {
    (rect.x, rect.y, rect.w, rect.h)
}

#[cfg(test)]
fn text_slot_name(kind: TextSlotKind) -> &'static str {
    match kind {
        TextSlotKind::HeaderKind => "header_kind",
        TextSlotKind::Status => "status",
        TextSlotKind::Title => "title",
        TextSlotKind::Subject => "subject",
        TextSlotKind::MetricLabel(0) => "metric_1_label",
        TextSlotKind::MetricValue(0) => "metric_1_value",
        TextSlotKind::MetricLabel(1) => "metric_2_label",
        TextSlotKind::MetricValue(1) => "metric_2_value",
        TextSlotKind::MetricLabel(2) => "metric_3_label",
        TextSlotKind::MetricValue(2) => "metric_3_value",
        TextSlotKind::MetricLabel(_) => "metric_label",
        TextSlotKind::MetricValue(_) => "metric_value",
        TextSlotKind::RowLabel(0) => "row_1_label",
        TextSlotKind::RowValue(0) => "row_1_value",
        TextSlotKind::RowLabel(1) => "row_2_label",
        TextSlotKind::RowValue(1) => "row_2_value",
        TextSlotKind::RowLabel(2) => "row_3_label",
        TextSlotKind::RowValue(2) => "row_3_value",
        TextSlotKind::RowLabel(3) => "row_4_label",
        TextSlotKind::RowValue(3) => "row_4_value",
        TextSlotKind::RowLabel(_) => "row_label",
        TextSlotKind::RowValue(_) => "row_value",
    }
}

pub(crate) fn build_static_telegram_card_template(
    template: CardTemplate,
) -> anyhow::Result<StaticTelegramCardTemplate> {
    let reference = decode_template_reference(template)?;
    let profile = card_profile(template);
    let mut base = reference.clone();
    let mut layers = Vec::new();

    for block in scaled_template_blocks(profile) {
        erase_template_block(&mut base, &block);
        layers.push(StaticTelegramCardLayer {
            name: block.name,
            x: block.x,
            y: block.y,
            image: crop_rect(&reference, block.x, block.y, block.w, block.h),
        });
    }

    Ok(StaticTelegramCardTemplate {
        template,
        reference,
        base,
        layers,
    })
}

pub(crate) fn compose_static_template(
    template: &StaticTelegramCardTemplate,
) -> anyhow::Result<RgbaImage> {
    let mut out = template.base.clone();
    for layer in &template.layers {
        paste_rect(&mut out, layer.x, layer.y, &layer.image);
    }
    Ok(out)
}

fn decode_template_reference(template: CardTemplate) -> anyhow::Result<RgbaImage> {
    Ok(image::load_from_memory(card_template_bytes(template))
        .context("decode telegram card reference png")?
        .to_rgba8())
}

fn scaled_template_blocks(profile: CardProfile) -> Vec<TemplateBlockSpec> {
    let sx = profile.width as f32 / 1672.0;
    let sy = profile.height as f32 / 941.0;
    GENERIC_TEMPLATE_BLOCKS
        .iter()
        .map(|block| TemplateBlockSpec {
            name: block.name,
            x: sc(block.x as f32, sx),
            y: sc(block.y as f32, sy),
            w: su(block.w as f32, sx),
            h: su(block.h as f32, sy),
            sample_x: sc(block.sample_x as f32, sx),
        })
        .collect()
}

fn scaled_text_slots(template: CardTemplate, profile: CardProfile) -> Vec<TextSlotSpec> {
    let sx = profile.width as f32 / 1672.0;
    let sy = profile.height as f32 / 941.0;
    let font_scale = ((sx + sy) / 2.0).clamp(0.86, 1.08);
    GENERIC_TEXT_SLOTS
        .iter()
        .map(|slot| {
            let slot = template_text_slot(template, *slot);
            TextSlotSpec {
                kind: slot.kind,
                draw: scale_rect(slot.draw, sx, sy),
                erase: scale_rect(slot.erase, sx, sy),
                fill_x: sc(slot.fill_x as f32, sx),
                size: slot.size * font_scale,
                min_size: slot.min_size * font_scale,
                align: slot.align,
            }
        })
        .collect()
}

fn template_text_slot(template: CardTemplate, mut slot: TextSlotSpec) -> TextSlotSpec {
    match (template, slot.kind) {
        (CardTemplate::NewVersion, TextSlotKind::MetricValue(0)) => {
            slot.draw = rect(356, 424, 210, 36);
            slot.erase = rect(326, 408, 250, 66);
            slot.fill_x = 525;
        }
        (CardTemplate::NewVersion, TextSlotKind::MetricValue(1)) => {
            slot.draw = rect(846, 424, 210, 36);
            slot.erase = rect(820, 408, 250, 66);
            slot.fill_x = 1015;
        }
        (CardTemplate::NewVersion, TextSlotKind::MetricValue(2)) => {
            slot.draw = rect(1240, 424, 320, 38);
            slot.erase = rect(1200, 406, 380, 72);
            slot.fill_x = 1212;
        }
        (
            CardTemplate::GhcrAnomaly | CardTemplate::TestNotification,
            TextSlotKind::MetricValue(0),
        ) => {
            slot.draw = rect(356, 424, 210, 36);
            slot.erase = rect(350, 414, 210, 52);
            slot.fill_x = 525;
        }
        (
            CardTemplate::GhcrAnomaly | CardTemplate::TestNotification,
            TextSlotKind::MetricValue(1),
        ) => {
            slot.draw = rect(846, 424, 210, 36);
            slot.erase = rect(840, 414, 210, 52);
            slot.fill_x = 1015;
        }
        (
            CardTemplate::GhcrAnomaly | CardTemplate::TestNotification,
            TextSlotKind::MetricValue(2),
        ) => {
            slot.draw = rect(1240, 424, 320, 38);
            slot.erase = rect(1230, 412, 330, 58);
            slot.fill_x = 1212;
        }
        (CardTemplate::NewVersion, TextSlotKind::Title) => {
            slot.draw = rect(288, 226, 980, 56);
            slot.erase = rect(260, 210, 940, 92);
            slot.fill_x = 1280;
        }
        (CardTemplate::NewVersion, TextSlotKind::Subject) => {
            slot.draw = rect(288, 306, 820, 38);
            slot.erase = rect(260, 290, 800, 70);
            slot.fill_x = 1120;
        }
        (CardTemplate::GhcrAnomaly, TextSlotKind::Title) => {
            slot.draw = rect(304, 242, 900, 56);
            slot.erase = rect(300, 235, 860, 72);
            slot.fill_x = 1280;
        }
        (CardTemplate::GhcrAnomaly, TextSlotKind::Subject) => {
            slot.draw = rect(304, 316, 780, 38);
            slot.erase = rect(300, 310, 760, 54);
            slot.fill_x = 1120;
        }
        (CardTemplate::TestNotification, TextSlotKind::Title) => {
            slot.draw = rect(324, 228, 900, 56);
            slot.erase = rect(320, 222, 840, 76);
            slot.fill_x = 1280;
        }
        (CardTemplate::TestNotification, TextSlotKind::Subject) => {
            slot.draw = rect(324, 306, 780, 38);
            slot.erase = rect(320, 302, 700, 52);
            slot.fill_x = 1120;
        }
        (CardTemplate::GhcrAnomaly, TextSlotKind::RowValue(3)) => {
            slot.draw = rect(900, 790, 590, 36);
            slot.erase = rect(820, 752, 690, 94);
            slot.fill_x = 860;
        }
        (CardTemplate::NewVersion, TextSlotKind::RowValue(3)) => {
            slot.draw = rect(900, 790, 590, 36);
            slot.erase = rect(820, 752, 730, 94);
            slot.fill_x = 860;
        }
        _ => {}
    }
    slot
}

fn scale_rect(rect: TextRect, sx: f32, sy: f32) -> TextRect {
    TextRect {
        x: sc(rect.x as f32, sx),
        y: sc(rect.y as f32, sy),
        w: su(rect.w as f32, sx),
        h: su(rect.h as f32, sy),
    }
}

fn text_for_slot(card: &TelegramCard, kind: TextSlotKind) -> Option<String> {
    match kind {
        TextSlotKind::HeaderKind => Some(card.kind.clone()),
        TextSlotKind::Status => Some(card.status.clone()),
        TextSlotKind::Title => Some(card.title.clone()),
        TextSlotKind::Subject => Some(card.subject.clone()),
        TextSlotKind::MetricLabel(index) => card
            .metrics
            .get(index)
            .and_then(|pair| non_empty_text(&pair.label)),
        TextSlotKind::MetricValue(index) => card.metrics.get(index).map(|pair| pair.value.clone()),
        TextSlotKind::RowLabel(index) => card
            .rows
            .get(index)
            .and_then(|pair| non_empty_text(&pair.label)),
        TextSlotKind::RowValue(index) => card.rows.get(index).map(|pair| pair.value.clone()),
    }
}

fn non_empty_text(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn erase_text_slot(img: &mut RgbaImage, slot: &TextSlotSpec) {
    fill_rect_from_column(
        img,
        slot.erase.x,
        slot.erase.y,
        slot.erase.w,
        slot.erase.h,
        slot.fill_x,
    );
}

fn restore_reference_text_slot(img: &mut RgbaImage, reference: &RgbaImage, slot: &TextSlotSpec) {
    restore_reference_patch(img, reference, slot.erase);
}

fn restore_reference_patch(img: &mut RgbaImage, reference: &RgbaImage, rect: TextRect) {
    paste_rect(
        img,
        rect.x,
        rect.y,
        &crop_rect(reference, rect.x, rect.y, rect.w, rect.h),
    );
}

fn draw_text_slot(
    img: &mut RgbaImage,
    font: &FontArc,
    slot: &TextSlotSpec,
    text: &str,
    card: &TelegramCard,
) {
    let scale = fit_text_scale(font, text, slot.draw.w, slot.size, slot.min_size);
    let clipped = truncate_text_to_width(font, scale, text, slot.draw.w);
    let (text_w, _text_h) = text_size(scale, font, &clipped);
    let x = match slot.align {
        TextAlign::Left => slot.draw.x,
        TextAlign::Center => slot.draw.x + ((slot.draw.w.saturating_sub(text_w) / 2) as i32),
        TextAlign::Right => slot.draw.x + slot.draw.w.saturating_sub(text_w) as i32,
    };
    let y = slot.draw.y;
    draw_text_mut(
        img,
        color_for_slot(card, slot.kind),
        x,
        y,
        scale,
        font,
        &clipped,
    );
}

fn fit_text_scale(font: &FontArc, text: &str, max_width: u32, size: f32, min_size: f32) -> PxScale {
    let mut current = size;
    while current > min_size {
        let scale = PxScale::from(current);
        if text_size(scale, font, text).0 <= max_width {
            return scale;
        }
        current -= 1.0;
    }
    PxScale::from(min_size)
}

fn truncate_text_to_width(font: &FontArc, scale: PxScale, text: &str, max_width: u32) -> String {
    if text_size(scale, font, text).0 <= max_width {
        return text.to_string();
    }

    let ellipsis = "...";
    let ellipsis_width = text_size(scale, font, ellipsis).0;
    let mut buf = String::new();
    for ch in text.chars() {
        let next = format!("{buf}{ch}");
        if text_size(scale, font, &next)
            .0
            .saturating_add(ellipsis_width)
            > max_width
        {
            break;
        }
        buf.push(ch);
    }
    buf.push_str(ellipsis);
    buf
}

fn color_for_slot(card: &TelegramCard, kind: TextSlotKind) -> Rgba<u8> {
    match kind {
        TextSlotKind::HeaderKind | TextSlotKind::Status | TextSlotKind::Subject => card.accent,
        TextSlotKind::MetricValue(index) => match card.metrics.get(index) {
            Some(metric) if !metric.label.trim().is_empty() => card.accent,
            _ => rgba(15, 23, 42),
        },
        TextSlotKind::RowValue(index) => match card.rows.get(index).map(|row| row.label.as_str()) {
            Some("版本变更") | Some("更新结果") | Some("应用版本") => card.accent,
            _ => rgba(15, 23, 42),
        },
        TextSlotKind::Title | TextSlotKind::MetricLabel(_) | TextSlotKind::RowLabel(_) => {
            rgba(15, 23, 42)
        }
    }
}

fn erase_template_block(img: &mut RgbaImage, block: &TemplateBlockSpec) {
    fill_rect_from_column(img, block.x, block.y, block.w, block.h, block.sample_x);
}

fn fill_rect_from_column(img: &mut RgbaImage, x: i32, y: i32, w: u32, h: u32, src_x: i32) {
    for yy in 0..h as i32 {
        let color = sample_pixel(img, src_x, y + yy);
        for xx in 0..w as i32 {
            put_pixel_checked(img, x + xx, y + yy, color);
        }
    }
}

fn crop_rect(img: &RgbaImage, x: i32, y: i32, w: u32, h: u32) -> RgbaImage {
    let mut out = RgbaImage::new(w, h);
    for yy in 0..h as i32 {
        for xx in 0..w as i32 {
            out.put_pixel(xx as u32, yy as u32, sample_pixel(img, x + xx, y + yy));
        }
    }
    out
}

fn paste_rect(img: &mut RgbaImage, x: i32, y: i32, patch: &RgbaImage) {
    for yy in 0..patch.height() as i32 {
        for xx in 0..patch.width() as i32 {
            put_pixel_checked(img, x + xx, y + yy, *patch.get_pixel(xx as u32, yy as u32));
        }
    }
}

fn sample_pixel(img: &RgbaImage, x: i32, y: i32) -> Rgba<u8> {
    let clamped_x = x.clamp(0, img.width().saturating_sub(1) as i32) as u32;
    let clamped_y = y.clamp(0, img.height().saturating_sub(1) as i32) as u32;
    *img.get_pixel(clamped_x, clamped_y)
}

fn put_pixel_checked(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && x < img.width() as i32 && y < img.height() as i32 {
        img.put_pixel(x as u32, y as u32, color);
    }
}

fn encode_card_png(img: RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;
    Ok(out)
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
struct CardProfile {
    width: u32,
    height: u32,
}

#[allow(dead_code)]
fn card_profile(template: CardTemplate) -> CardProfile {
    match template {
        CardTemplate::NewVersion => CardProfile {
            width: 1774,
            height: 887,
        },
        CardTemplate::JobFinished | CardTemplate::GhcrAnomaly | CardTemplate::TestNotification => {
            CardProfile {
                width: 1672,
                height: 941,
            }
        }
    }
}

fn sc(value: f32, scale: f32) -> i32 {
    (value * scale).round() as i32
}

fn su(value: f32, scale: f32) -> u32 {
    (value * scale).round().max(1.0) as u32
}

pub(crate) fn pair(label: &str, value: &str) -> CardPair {
    CardPair {
        label: label.to_string(),
        value: value.to_string(),
    }
}

fn new_version_transition(svc: &NewVersionNotificationServiceUrlV2) -> Option<String> {
    match (
        svc.current_display_tag
            .as_deref()
            .or(svc.current_tag.as_deref()),
        svc.candidate_display_tag
            .as_deref()
            .or(svc.candidate_tag.as_deref()),
    ) {
        (Some(current), Some(candidate)) if !current.is_empty() && !candidate.is_empty() => {
            Some(format!("{current} -> {candidate}"))
        }
        _ => None,
    }
}

pub(crate) fn summarize_card_items(items: &[String], omitted: u32, fallback: String) -> String {
    if items.is_empty() {
        return fallback;
    }

    const MAX_CARD_ITEMS: usize = 3;
    let visible = items
        .iter()
        .take(MAX_CARD_ITEMS)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("、");
    let hidden = items.len().saturating_sub(MAX_CARD_ITEMS) as u32 + omitted;
    if hidden > 0 {
        format!("{visible}，另 {hidden} 项")
    } else {
        visible
    }
}

fn format_card_time(value: &str) -> String {
    value
        .trim_end_matches('Z')
        .replace('T', " ")
        .split('.')
        .next()
        .unwrap_or(value)
        .to_string()
}

fn rgba(r: u8, g: u8, b: u8) -> Rgba<u8> {
    Rgba([r, g, b, 255])
}
