use super::*;

use std::io::Cursor;

use ab_glyph::{FontArc, PxScale};
use image::{ImageBuffer, ImageFormat, Rgba, RgbaImage};
use imageproc::{
    drawing::{
        draw_filled_circle_mut, draw_filled_rect_mut, draw_line_segment_mut, draw_text_mut,
        text_size,
    },
    rect::Rect,
};

const CARD_WIDTH: u32 = 1280;
const CARD_HEIGHT: u32 = 640;
const FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/NotoSansCJKsc-Regular.otf");

#[derive(Clone, Debug)]
pub(crate) struct CardPair {
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TelegramCard {
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

#[derive(Clone, Copy, Debug)]
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
        kind,
        status: status_label,
        title: payload.human.title.clone(),
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
    let service_label = payload
        .links
        .service_urls
        .first()
        .map(|svc| format!("{} / {}", svc.stack_name, svc.service_name))
        .unwrap_or_else(|| format!("{} 个服务", payload.check.new_versions));
    let version_change = payload
        .links
        .service_urls
        .first()
        .and_then(new_version_transition)
        .unwrap_or_else(|| format!("{} 个新版本", payload.check.new_versions));
    let rows = vec![
        pair("通知类型", "发现新版本"),
        pair("服务", &service_label),
        pair("版本变更", &version_change),
        pair("发现时间", &format_card_time(&payload.sent_at)),
    ];

    render_telegram_card_png(&TelegramCard {
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
    let repo_summary = payload
        .links
        .repos
        .first()
        .map(|repo| format!("{} [{}]", repo.full_name, repo.state))
        .unwrap_or_else(|| "无仓库详情".to_string());
    rows.push(pair("仓库", &repo_summary));
    rows.push(pair("巡检时间", &format_card_time(&payload.sent_at)));

    render_telegram_card_png(&TelegramCard {
        kind: "Webhook 巡检".to_string(),
        status: "异常".to_string(),
        title: payload.human.title.clone(),
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
        kind: "通知测试".to_string(),
        status: notification_channel_label(NotificationTestChannel::Telegram).to_string(),
        title: payload.human.title.clone(),
        subject: payload.human.summary.clone(),
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
    let font = FontArc::try_from_slice(FONT_BYTES).context("load telegram card font")?;
    let mut img: RgbaImage = ImageBuffer::from_pixel(CARD_WIDTH, CARD_HEIGHT, rgba(241, 247, 255));
    let accent = card.accent;

    draw_background(&mut img);
    draw_filled_round_rect_mut(&mut img, 38, 34, 1204, 574, 24, rgba(215, 225, 239));
    draw_filled_round_rect_mut(&mut img, 32, 24, 1216, 584, 24, rgba(255, 255, 255));

    draw_brand_mark(&mut img, 63, 60, accent);
    draw_text_fit_bold(
        &mut img,
        &font,
        42.0,
        137,
        63,
        170,
        rgba(15, 23, 42),
        "Dockrev",
    );
    draw_line_segment_mut(&mut img, (315.0, 69.0), (315.0, 107.0), rgba(194, 207, 225));
    draw_small_icon_badge(&mut img, &font, 349, 64, accent, CardIcon::Bell);
    draw_text_fit(&mut img, &font, 32.0, 405, 66, 360, accent, &card.kind);
    draw_status_pill(&mut img, &font, 1104, 66, 110, 42, &card.status, accent);

    draw_panel(&mut img, 64, 126, 1152, 238, 10);
    draw_filled_round_rect_mut(&mut img, 65, 128, 7, 136, 4, accent);
    draw_icon_tile(&mut img, 95, 154, 82, accent, card.icon);

    draw_text_fit_bold(
        &mut img,
        &font,
        41.0,
        207,
        155,
        790,
        rgba(15, 23, 42),
        &card.title,
    );
    draw_text_fit_bold(&mut img, &font, 33.0, 208, 213, 560, accent, &card.subject);

    let metric_width = 350u32;
    for (index, metric) in card.metrics.iter().take(3).enumerate() {
        let x = 96 + (index as i32 * 376);
        draw_metric_tile(&mut img, &font, x, 263, metric_width, metric, accent, index);
    }

    draw_panel(&mut img, 64, 378, 1152, 203, 10);
    let visible_rows = if card.rows.is_empty() {
        vec![pair("详情", "见文字消息")]
    } else {
        card.rows.iter().take(4).cloned().collect::<Vec<_>>()
    };
    let mut y = 398;
    for (index, row) in visible_rows.iter().enumerate() {
        draw_reference_table_row(&mut img, &font, y, accent, row, index);
        y += 51;
    }
    if card.omitted > 0 {
        draw_text_fit(
            &mut img,
            &font,
            21.0,
            154,
            y + 4,
            920,
            rgba(100, 116, 139),
            &format!("以及其他 {} 条，完整详情见消息正文", card.omitted),
        );
    }

    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;
    Ok(out)
}

fn draw_background(img: &mut RgbaImage) {
    for y in 0..CARD_HEIGHT {
        for x in 0..CARD_WIDTH {
            let nx = x as f32 / CARD_WIDTH as f32;
            let ny = y as f32 / CARD_HEIGHT as f32;
            let shade = 250.0 - 7.0 * nx - 5.0 * ny;
            let green = 252.0 - 5.0 * ny;
            let blue = 255.0 - 4.0 * nx;
            *img.get_pixel_mut(x, y) = Rgba([shade as u8, green as u8, blue as u8, 255]);
        }
    }
    for offset in (-260..1280).step_by(76) {
        draw_line_segment_mut(
            img,
            (offset as f32, 640.0),
            ((offset + 420) as f32, 0.0),
            rgba(235, 242, 251),
        );
    }
    for x in (1160..1270).step_by(16) {
        for y in (10..132).step_by(16) {
            draw_filled_circle_mut(img, (x, y), 2, rgba(226, 236, 248));
        }
    }
    for x in (4..64).step_by(18) {
        for y in (490..626).step_by(18) {
            draw_filled_circle_mut(img, (x, y), 2, rgba(226, 236, 248));
        }
    }
}

fn draw_panel(img: &mut RgbaImage, x: i32, y: i32, w: u32, h: u32, r: i32) {
    draw_filled_round_rect_mut(img, x + 2, y + 3, w, h, r, rgba(230, 236, 246));
    draw_filled_round_rect_mut(img, x, y, w, h, r, rgba(207, 220, 237));
    draw_filled_round_rect_mut(
        img,
        x + 1,
        y + 1,
        w.saturating_sub(2),
        h.saturating_sub(2),
        r,
        rgba(255, 255, 255),
    );
}

fn draw_metric_tile(
    img: &mut RgbaImage,
    font: &FontArc,
    x: i32,
    y: i32,
    w: u32,
    metric: &CardPair,
    accent: Rgba<u8>,
    index: usize,
) {
    draw_panel(img, x, y, w, 84, 10);
    draw_metric_icon(img, x + 18, y + 20, accent, index);
    let text_x = x + 94;
    if metric.label.is_empty() {
        draw_text_fit(
            img,
            font,
            25.0,
            text_x,
            y + 27,
            w - 118,
            rgba(15, 23, 42),
            &metric.value,
        );
    } else {
        draw_text_fit(
            img,
            font,
            24.0,
            text_x,
            y + 27,
            98,
            rgba(15, 23, 42),
            &metric.label,
        );
        draw_metric_value_inline(
            img,
            font,
            text_x + 62,
            y + 27,
            w - 160,
            accent,
            &metric.value,
        );
    }
}

fn draw_reference_table_row(
    img: &mut RgbaImage,
    font: &FontArc,
    y: i32,
    accent: Rgba<u8>,
    row: &CardPair,
    index: usize,
) {
    if index > 0 {
        draw_line_segment_mut(
            img,
            (77.0, (y - 8) as f32),
            (1204.0, (y - 8) as f32),
            rgba(213, 225, 240),
        );
    }
    draw_table_icon_badge(img, 96, y - 15, accent, index);
    draw_text_fit_bold(
        img,
        font,
        24.0,
        155,
        y - 4,
        260,
        rgba(15, 23, 42),
        &row.label,
    );
    if index == 2 {
        draw_value_badge(img, font, 1012, y - 11, 142, 34, accent, &row.value);
    } else {
        draw_text_right_fit(
            img,
            font,
            24.0,
            1135,
            y - 4,
            410,
            rgba(15, 23, 42),
            &row.value,
        );
        if index == 3 {
            draw_check_icon(img, 1164, y - 8, 27, rgba(34, 197, 94));
        }
    }
}

fn pair(label: &str, value: &str) -> CardPair {
    CardPair {
        label: label.to_string(),
        value: value.to_string(),
    }
}

fn draw_brand_mark(img: &mut RgbaImage, x: i32, y: i32, accent: Rgba<u8>) {
    draw_filled_circle_mut(img, (x + 24, y + 24), 26, tint(accent, 0.9));
    draw_circle_outline(img, x + 24, y + 24, 25, accent);
    draw_cube_icon(img, x + 10, y + 9, 28, accent);
    draw_thick_line_segment_mut(
        img,
        ((x + 38) as f32, (y + 18) as f32),
        ((x + 45) as f32, (y + 25) as f32),
        rgba(38, 203, 124),
        2,
    );
    draw_thick_line_segment_mut(
        img,
        ((x + 45) as f32, (y + 25) as f32),
        ((x + 56) as f32, (y + 11) as f32),
        rgba(38, 203, 124),
        2,
    );
}

fn draw_icon_tile(
    img: &mut RgbaImage,
    x: i32,
    y: i32,
    size: u32,
    accent: Rgba<u8>,
    icon: CardIcon,
) {
    draw_filled_round_rect_mut(img, x + 3, y + 5, size, size, 14, rgba(218, 229, 244));
    draw_filled_round_rect_mut(img, x + 1, y + 1, size, size, 14, rgba(207, 220, 237));
    draw_filled_round_rect_mut(img, x, y, size, size, 14, tint(accent, 0.9));
    draw_card_icon(img, x + 22, y + 22, 38, accent, icon);
}

fn draw_small_icon_badge(
    img: &mut RgbaImage,
    _font: &FontArc,
    x: i32,
    y: i32,
    accent: Rgba<u8>,
    icon: CardIcon,
) {
    draw_filled_round_rect_mut(img, x, y, 42, 42, 12, tint(accent, 0.9));
    draw_card_icon(img, x + 11, y + 11, 20, accent, icon);
}

fn draw_metric_icon(img: &mut RgbaImage, x: i32, y: i32, accent: Rgba<u8>, index: usize) {
    draw_filled_round_rect_mut(img, x, y, 58, 58, 12, tint(accent, 0.9));
    match index {
        0 => draw_magnifier_icon(img, x + 16, y + 15, 26, accent),
        1 => draw_arrow_up_icon(img, x + 17, y + 15, 25, accent),
        _ => draw_calendar_icon(img, x + 16, y + 15, 26, accent),
    }
}

fn draw_metric_value_inline(
    img: &mut RgbaImage,
    font: &FontArc,
    x: i32,
    y: i32,
    max_width: u32,
    accent: Rgba<u8>,
    text: &str,
) {
    let scale = PxScale::from(24.0);
    let clipped = truncate_to_width(font, scale, text, max_width);
    let split_at = clipped
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_digit())
        .map(|(idx, _)| idx)
        .unwrap_or(clipped.len());
    let (leading, rest) = clipped.split_at(split_at);
    if leading.is_empty() {
        draw_text_mut(img, rgba(15, 23, 42), x, y, scale, font, &clipped);
        return;
    }
    draw_text_mut(img, accent, x, y, scale, font, leading);
    draw_text_mut(
        img,
        rgba(15, 23, 42),
        x + text_size(scale, font, leading).0 as i32 + 7,
        y,
        scale,
        font,
        rest.trim_start(),
    );
}

fn draw_table_icon_badge(img: &mut RgbaImage, x: i32, y: i32, accent: Rgba<u8>, index: usize) {
    draw_filled_circle_mut(img, (x + 21, y + 21), 21, tint(accent, 0.92));
    match index {
        0 => draw_tag_icon(img, x + 11, y + 11, 20, accent),
        1 => draw_cube_icon(img, x + 11, y + 10, 21, accent),
        2 => draw_arrow_up_icon(img, x + 9, y + 9, 25, accent),
        _ => draw_clock_icon(img, x + 9, y + 9, 25, accent),
    }
}

fn draw_card_icon(
    img: &mut RgbaImage,
    x: i32,
    y: i32,
    size: u32,
    accent: Rgba<u8>,
    icon: CardIcon,
) {
    match icon {
        CardIcon::Package => draw_cube_icon(img, x, y, size, accent),
        CardIcon::Check => draw_check_icon(img, x, y, size, accent),
        CardIcon::Alert => draw_alert_icon(img, x, y, size, accent),
        CardIcon::Bell => draw_bell_icon(img, x, y, size, accent),
    }
}

fn draw_cube_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    let s = size as f32;
    let points = [
        (x as f32 + s * 0.5, y as f32),
        (x as f32 + s, y as f32 + s * 0.28),
        (x as f32 + s, y as f32 + s * 0.74),
        (x as f32 + s * 0.5, y as f32 + s),
        (x as f32, y as f32 + s * 0.74),
        (x as f32, y as f32 + s * 0.28),
    ];
    draw_polyline(img, &points, color);
    draw_thick_line_segment_mut(
        img,
        points[0],
        (x as f32 + s * 0.5, y as f32 + s * 0.52),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        points[5],
        (x as f32 + s * 0.5, y as f32 + s * 0.52),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        points[1],
        (x as f32 + s * 0.5, y as f32 + s * 0.52),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        (x as f32 + s * 0.5, y as f32 + s * 0.52),
        points[3],
        color,
        2,
    );
}

fn draw_check_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    let cx = x + (size / 2) as i32;
    let cy = y + (size / 2) as i32;
    draw_circle_outline(img, cx, cy, (size as i32 / 2).saturating_sub(1), color);
    draw_thick_line_segment_mut(
        img,
        (x as f32 + size as f32 * 0.26, y as f32 + size as f32 * 0.52),
        (x as f32 + size as f32 * 0.42, y as f32 + size as f32 * 0.68),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        (x as f32 + size as f32 * 0.42, y as f32 + size as f32 * 0.68),
        (x as f32 + size as f32 * 0.76, y as f32 + size as f32 * 0.32),
        color,
        2,
    );
}

fn draw_alert_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    let s = size as f32;
    let points = [
        (x as f32 + s * 0.5, y as f32 + 1.0),
        (x as f32 + s - 1.0, y as f32 + s - 1.0),
        (x as f32 + 1.0, y as f32 + s - 1.0),
    ];
    draw_polyline(img, &points, color);
    draw_thick_line_segment_mut(
        img,
        (x as f32 + s * 0.5, y as f32 + s * 0.33),
        (x as f32 + s * 0.5, y as f32 + s * 0.64),
        color,
        2,
    );
    draw_filled_circle_mut(
        img,
        (x + (size / 2) as i32, y + (size as i32 * 4 / 5)),
        2,
        color,
    );
}

fn draw_bell_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    let s = size as f32;
    draw_circle_outline(
        img,
        x + (size / 2) as i32,
        y + (size / 2) as i32,
        size as i32 / 3,
        color,
    );
    draw_thick_line_segment_mut(
        img,
        (x as f32 + s * 0.2, y as f32 + s * 0.72),
        (x as f32 + s * 0.8, y as f32 + s * 0.72),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        (x as f32 + s * 0.5, y as f32 + s * 0.06),
        (x as f32 + s * 0.5, y as f32 + s * 0.18),
        color,
        2,
    );
    draw_filled_circle_mut(
        img,
        (x + (size / 2) as i32, y + (size as i32 * 4 / 5)),
        2,
        color,
    );
}

fn draw_magnifier_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    draw_circle_outline(
        img,
        x + (size as i32 / 2) - 3,
        y + (size as i32 / 2) - 3,
        size as i32 / 3,
        color,
    );
    draw_thick_line_segment_mut(
        img,
        (x as f32 + size as f32 * 0.62, y as f32 + size as f32 * 0.62),
        (x as f32 + size as f32 * 0.9, y as f32 + size as f32 * 0.9),
        color,
        2,
    );
}

fn draw_arrow_up_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    draw_circle_outline(
        img,
        x + (size / 2) as i32,
        y + (size / 2) as i32,
        size as i32 / 2,
        color,
    );
    draw_thick_line_segment_mut(
        img,
        (x as f32 + size as f32 * 0.5, y as f32 + size as f32 * 0.72),
        (x as f32 + size as f32 * 0.5, y as f32 + size as f32 * 0.26),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        (x as f32 + size as f32 * 0.5, y as f32 + size as f32 * 0.26),
        (x as f32 + size as f32 * 0.32, y as f32 + size as f32 * 0.44),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        (x as f32 + size as f32 * 0.5, y as f32 + size as f32 * 0.26),
        (x as f32 + size as f32 * 0.68, y as f32 + size as f32 * 0.44),
        color,
        2,
    );
}

fn draw_calendar_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    draw_line_round_rect(img, x, y + 4, size, size - 6, color);
    draw_thick_line_segment_mut(
        img,
        (x as f32, (y + 12) as f32),
        ((x + size as i32) as f32, (y + 12) as f32),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        ((x + 7) as f32, y as f32),
        ((x + 7) as f32, (y + 8) as f32),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        ((x + size as i32 - 7) as f32, y as f32),
        ((x + size as i32 - 7) as f32, (y + 8) as f32),
        color,
        2,
    );
}

fn draw_tag_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    let s = size as f32;
    let points = [
        (x as f32 + s * 0.08, y as f32 + s * 0.42),
        (x as f32 + s * 0.42, y as f32 + s * 0.08),
        (x as f32 + s * 0.88, y as f32 + s * 0.08),
        (x as f32 + s * 0.88, y as f32 + s * 0.54),
        (x as f32 + s * 0.52, y as f32 + s * 0.9),
    ];
    draw_polyline(img, &points, color);
    draw_filled_circle_mut(
        img,
        (x + (size as i32 * 2 / 3), y + (size as i32 / 3)),
        2,
        color,
    );
}

fn draw_clock_icon(img: &mut RgbaImage, x: i32, y: i32, size: u32, color: Rgba<u8>) {
    let cx = x + (size / 2) as i32;
    let cy = y + (size / 2) as i32;
    draw_circle_outline(img, cx, cy, (size as i32 / 2).saturating_sub(1), color);
    draw_thick_line_segment_mut(
        img,
        (cx as f32, (cy - 8) as f32),
        (cx as f32, cy as f32),
        color,
        2,
    );
    draw_thick_line_segment_mut(
        img,
        (cx as f32, cy as f32),
        ((cx + 7) as f32, (cy + 5) as f32),
        color,
        2,
    );
}

fn draw_circle_outline(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, color: Rgba<u8>) {
    let steps = 40;
    let mut prev = (cx as f32 + r as f32, cy as f32);
    for i in 1..=steps {
        let theta = std::f32::consts::TAU * (i as f32 / steps as f32);
        let next = (
            cx as f32 + r as f32 * theta.cos(),
            cy as f32 + r as f32 * theta.sin(),
        );
        draw_thick_line_segment_mut(img, prev, next, color, 2);
        prev = next;
    }
}

fn draw_line_round_rect(img: &mut RgbaImage, x: i32, y: i32, w: u32, h: u32, color: Rgba<u8>) {
    let x2 = x + w as i32;
    let y2 = y + h as i32;
    draw_line_segment_mut(img, (x as f32, y as f32), (x2 as f32, y as f32), color);
    draw_line_segment_mut(img, (x2 as f32, y as f32), (x2 as f32, y2 as f32), color);
    draw_line_segment_mut(img, (x2 as f32, y2 as f32), (x as f32, y2 as f32), color);
    draw_line_segment_mut(img, (x as f32, y2 as f32), (x as f32, y as f32), color);
}

fn draw_polyline(img: &mut RgbaImage, points: &[(f32, f32)], color: Rgba<u8>) {
    for pair in points.windows(2) {
        draw_thick_line_segment_mut(img, pair[0], pair[1], color, 2);
    }
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        draw_thick_line_segment_mut(img, *last, *first, color, 2);
    }
}

fn draw_thick_line_segment_mut(
    img: &mut RgbaImage,
    start: (f32, f32),
    end: (f32, f32),
    color: Rgba<u8>,
    width: i32,
) {
    for dx in -width / 2..=width / 2 {
        for dy in -width / 2..=width / 2 {
            draw_line_segment_mut(
                img,
                (start.0 + dx as f32, start.1 + dy as f32),
                (end.0 + dx as f32, end.1 + dy as f32),
                color,
            );
        }
    }
}

fn draw_status_pill(
    img: &mut RgbaImage,
    font: &FontArc,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    text: &str,
    accent: Rgba<u8>,
) {
    draw_filled_round_rect_mut(img, x + 2, y + 3, w, h, 10, rgba(226, 235, 247));
    draw_filled_round_rect_mut(img, x, y, w, h, 10, rgba(207, 220, 237));
    draw_filled_round_rect_mut(
        img,
        x + 1,
        y + 1,
        w.saturating_sub(2),
        h.saturating_sub(2),
        10,
        tint(accent, 0.92),
    );
    draw_filled_circle_mut(img, (x + 22, y + 21), 4, accent);
    draw_text_fit_bold(
        img,
        font,
        22.0,
        x + 43,
        y + 8,
        w.saturating_sub(52),
        accent,
        text,
    );
}

fn draw_value_badge(
    img: &mut RgbaImage,
    font: &FontArc,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    accent: Rgba<u8>,
    text: &str,
) {
    draw_filled_round_rect_mut(img, x, y, w, h, 9, tint(accent, 0.88));
    draw_text_right_fit(
        img,
        font,
        19.0,
        x + w as i32 - 13,
        y + 5,
        w.saturating_sub(24),
        accent,
        text,
    );
}

fn draw_text_fit(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    x: i32,
    y: i32,
    max_width: u32,
    color: Rgba<u8>,
    text: &str,
) {
    let scale = PxScale::from(size);
    let clipped = truncate_to_width(font, scale, text, max_width);
    draw_text_mut(img, color, x, y, scale, font, &clipped);
}

fn draw_text_fit_bold(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    x: i32,
    y: i32,
    max_width: u32,
    color: Rgba<u8>,
    text: &str,
) {
    let scale = PxScale::from(size);
    let clipped = truncate_to_width(font, scale, text, max_width);
    draw_text_mut(img, color, x, y, scale, font, &clipped);
    draw_text_mut(img, color, x + 1, y, scale, font, &clipped);
}

fn draw_text_right_fit(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    right: i32,
    y: i32,
    max_width: u32,
    color: Rgba<u8>,
    text: &str,
) {
    let scale = PxScale::from(size);
    let clipped = truncate_to_width(font, scale, text, max_width);
    let width = text_size(scale, font, &clipped).0 as i32;
    draw_text_mut(img, color, right - width, y, scale, font, &clipped);
}

fn truncate_to_width(font: &FontArc, scale: PxScale, text: &str, max_width: u32) -> String {
    let clean = text.replace(['\n', '\r', '\t'], " ");
    if text_size(scale, font, &clean).0 <= max_width {
        return clean;
    }

    let ellipsis = "...";
    let mut out = String::new();
    for ch in clean.chars() {
        let candidate = format!("{out}{ch}{ellipsis}");
        if text_size(scale, font, &candidate).0 > max_width {
            break;
        }
        out.push(ch);
    }
    if out.is_empty() {
        ellipsis.to_string()
    } else {
        out.push_str(ellipsis);
        out
    }
}

fn draw_filled_round_rect_mut(
    img: &mut RgbaImage,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    r: i32,
    color: Rgba<u8>,
) {
    let r = r.max(1).min((w as i32) / 2).min((h as i32) / 2);
    let inner_w = w.saturating_sub((r * 2) as u32);
    let inner_h = h.saturating_sub((r * 2) as u32);
    if inner_w > 0 {
        draw_filled_rect_mut(img, Rect::at(x + r, y).of_size(inner_w, h), color);
    }
    if inner_h > 0 {
        draw_filled_rect_mut(img, Rect::at(x, y + r).of_size(w, inner_h), color);
    }
    draw_filled_circle_mut(img, (x + r, y + r), r, color);
    draw_filled_circle_mut(img, (x + w as i32 - r, y + r), r, color);
    draw_filled_circle_mut(img, (x + r, y + h as i32 - r), r, color);
    draw_filled_circle_mut(img, (x + w as i32 - r, y + h as i32 - r), r, color);
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

fn tint(color: Rgba<u8>, white_mix: f32) -> Rgba<u8> {
    let mix = white_mix.clamp(0.0, 1.0);
    let channels = color.0;
    Rgba([
        mix_channel(channels[0], 255, mix),
        mix_channel(channels[1], 255, mix),
        mix_channel(channels[2], 255, mix),
        255,
    ])
}

fn mix_channel(source: u8, target: u8, amount: f32) -> u8 {
    ((source as f32 * (1.0 - amount)) + (target as f32 * amount)).round() as u8
}
