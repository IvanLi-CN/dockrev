use std::collections::HashMap;

use crate::api::types::JobProgressDownload;

#[derive(Clone, Debug)]
pub(super) struct PullProgressSnapshot {
    pub(super) fraction: Option<f64>,
    pub(super) fraction_source: Option<PullProgressFractionSource>,
    pub(super) download: Option<JobProgressDownload>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PullProgressFractionSource {
    Bytes,
    Layers,
}

#[derive(Clone, Debug, Default)]
struct PullLayerState {
    status: String,
    current_bytes: Option<u64>,
    total_bytes: Option<u64>,
    complete: bool,
}

#[derive(Clone, Debug)]
struct PullLineObservation {
    layer_id: String,
    status: String,
    current_bytes: Option<u64>,
    total_bytes: Option<u64>,
    complete: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PullProgressTracker {
    layers: HashMap<String, PullLayerState>,
    last_byte_fraction: Option<f64>,
    last_layer_fraction: Option<f64>,
    last_fraction: Option<(PullProgressFractionSource, f64)>,
}

impl PullProgressTracker {
    pub(super) fn observe_line(&mut self, line: &str) -> Option<PullProgressSnapshot> {
        let observation = parse_pull_line_observation(line)?;
        let layer = self.layers.entry(observation.layer_id).or_default();
        layer.status = observation.status;
        layer.complete = layer.complete || observation.complete;
        if let Some(current) = observation.current_bytes {
            layer.current_bytes = Some(layer.current_bytes.unwrap_or(0).max(current));
        }
        if let Some(total) = observation.total_bytes {
            layer.total_bytes = Some(layer.total_bytes.unwrap_or(0).max(total));
        }
        if layer.complete
            && let Some(total) = layer.total_bytes
        {
            layer.current_bytes = Some(total);
        }
        let mut snapshot = self.snapshot();
        if let (Some(source), Some(fraction)) = (snapshot.fraction_source, snapshot.fraction) {
            let last_fraction = match source {
                PullProgressFractionSource::Bytes => &mut self.last_byte_fraction,
                PullProgressFractionSource::Layers => &mut self.last_layer_fraction,
            };
            let monotonic_fraction = last_fraction
                .map(|last| last.max(fraction))
                .unwrap_or(fraction);
            *last_fraction = Some(monotonic_fraction);
            snapshot.fraction = Some(monotonic_fraction);
            if let Some((last_source, last)) = self.last_fraction
                && last > monotonic_fraction
            {
                snapshot.fraction = Some(last);
                snapshot.fraction_source = Some(last_source);
                return Some(snapshot);
            }
            self.last_fraction = Some((source, monotonic_fraction));
        }
        Some(snapshot)
    }

    fn snapshot(&self) -> PullProgressSnapshot {
        let total_layers = self.layers.len() as u32;
        let completed_layers = self.layers.values().filter(|layer| layer.complete).count() as u32;
        let current_bytes = self
            .layers
            .values()
            .filter_map(|layer| layer.current_bytes)
            .sum::<u64>();
        let total_bytes = self
            .layers
            .values()
            .filter_map(|layer| layer.total_bytes)
            .sum::<u64>();
        let current_bytes_with_known_total = self
            .layers
            .values()
            .filter_map(|layer| {
                layer
                    .total_bytes
                    .map(|total| layer.current_bytes.unwrap_or(0).min(total))
            })
            .sum::<u64>();
        let active_layers = self
            .layers
            .iter()
            .filter(|(_, layer)| !layer.complete)
            .take(4)
            .map(|(id, layer)| format!("{} {}", short_layer_id(id), layer.status))
            .collect::<Vec<_>>();
        let status = if total_layers > 0 {
            Some(format!("layers {completed_layers}/{total_layers}"))
        } else {
            None
        };
        let download = JobProgressDownload {
            current_bytes: (current_bytes > 0).then_some(current_bytes),
            total_bytes: (total_bytes > 0).then_some(total_bytes),
            completed_layers: (total_layers > 0).then_some(completed_layers),
            total_layers: (total_layers > 0).then_some(total_layers),
            active_layers,
            status,
        };
        let (fraction, fraction_source) = if total_bytes > 0 {
            (
                Some((current_bytes_with_known_total as f64 / total_bytes as f64).clamp(0.0, 1.0)),
                Some(PullProgressFractionSource::Bytes),
            )
        } else if total_layers > 0 && completed_layers > 0 {
            (
                Some((completed_layers as f64 / total_layers as f64).clamp(0.0, 0.99)),
                Some(PullProgressFractionSource::Layers),
            )
        } else {
            (None, None)
        };
        PullProgressSnapshot {
            fraction,
            fraction_source,
            download: Some(download),
        }
    }
}

fn short_layer_id(layer_id: &str) -> String {
    layer_id.chars().take(12).collect()
}

fn parse_pull_line_observation(line: &str) -> Option<PullLineObservation> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    let layer_id = tokens.first()?.trim_matches(|c| matches!(c, ':' | ','));
    if layer_id.is_empty()
        || matches!(
            layer_id,
            "Image" | "Digest:" | "Status:" | "time=" | "level=warning"
        )
    {
        return None;
    }

    let (status, complete) = if trimmed.contains("Download complete") {
        ("Download complete".to_string(), true)
    } else if trimmed.contains("Pull complete") {
        ("Pull complete".to_string(), true)
    } else if trimmed.contains("Already exists") {
        ("Already exists".to_string(), true)
    } else if trimmed.contains("Downloading") {
        ("Downloading".to_string(), false)
    } else if trimmed.contains("Extracting") {
        ("Extracting".to_string(), false)
    } else if trimmed.contains("Verifying Checksum") {
        ("Verifying Checksum".to_string(), false)
    } else if trimmed.contains("Pulling fs layer") {
        ("Pulling fs layer".to_string(), false)
    } else if trimmed.contains("Waiting") {
        ("Waiting".to_string(), false)
    } else {
        return None;
    };

    let mut current_bytes = None;
    let mut total_bytes = None;
    for token in &tokens {
        let clean = token
            .trim()
            .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | ','));
        if let Some((current, total)) = clean.split_once('/') {
            if let (Some(current), Some(total)) =
                (parse_size_to_u64(current), parse_size_to_u64(total))
            {
                current_bytes = Some(current);
                total_bytes = Some(total);
            }
            continue;
        }
        if current_bytes.is_none()
            && let Some(value) = parse_size_to_u64(clean)
        {
            current_bytes = Some(value);
        }
    }

    Some(PullLineObservation {
        layer_id: layer_id.to_string(),
        status,
        current_bytes,
        total_bytes,
        complete,
    })
}

pub(super) fn parse_pull_fraction_from_line(line: &str) -> Option<f64> {
    let mut best: Option<f64> = None;
    for token in line.split_whitespace() {
        let clean = token
            .trim()
            .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | ','));
        let Some((current, total)) = clean.split_once('/') else {
            continue;
        };
        let Some(current_bytes) = parse_size_to_bytes(current) else {
            continue;
        };
        let Some(total_bytes) = parse_size_to_bytes(total) else {
            continue;
        };
        if total_bytes <= 0.0 {
            continue;
        }
        let ratio = (current_bytes / total_bytes).clamp(0.0, 1.0);
        if best.is_none_or(|v| ratio > v) {
            best = Some(ratio);
        }
    }
    best
}

fn parse_size_to_bytes(input: &str) -> Option<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut split_idx = None;
    for (idx, ch) in trimmed.char_indices() {
        if !(ch.is_ascii_digit() || ch == '.') {
            split_idx = Some(idx);
            break;
        }
    }
    let idx = split_idx.unwrap_or(trimmed.len());
    if idx == 0 {
        return None;
    }
    let num = trimmed[..idx].parse::<f64>().ok()?;
    let unit = trimmed[idx..].trim().to_ascii_uppercase();
    let factor = match unit.as_str() {
        "" | "B" => 1.0,
        "K" | "KB" | "KIB" => 1024.0,
        "M" | "MB" | "MIB" => 1024.0 * 1024.0,
        "G" | "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TB" | "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(num * factor)
}

fn parse_size_to_u64(input: &str) -> Option<u64> {
    parse_size_to_bytes(input).map(|value| value.max(0.0).round() as u64)
}

fn format_download_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1}GB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1}MB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1}KB", bytes / KIB)
    } else {
        format!("{}B", bytes as u64)
    }
}

fn pull_download_summary(download: &JobProgressDownload) -> String {
    let mut parts = Vec::new();
    if let Some(current) = download.current_bytes {
        let bytes = match download.total_bytes {
            Some(total) if total > 0 => {
                format!(
                    "{} / {}",
                    format_download_bytes(current),
                    format_download_bytes(total)
                )
            }
            _ => format!("downloaded {}", format_download_bytes(current)),
        };
        parts.push(bytes);
    }
    if let (Some(done), Some(total)) = (download.completed_layers, download.total_layers) {
        parts.push(format!("layers {done}/{total}"));
    }
    if let Some(active) = download.active_layers.first() {
        parts.push(active.clone());
    }
    if parts.is_empty() {
        download
            .status
            .clone()
            .unwrap_or_else(|| "downloading".to_string())
    } else {
        parts.join(" · ")
    }
}

pub(super) fn pull_progress_message(service_name: &str, snapshot: &PullProgressSnapshot) -> String {
    if snapshot.fraction_source == Some(PullProgressFractionSource::Bytes)
        && let Some(fraction) = snapshot.fraction
    {
        return format!(
            "pulling image for {} ({:.0}%)",
            service_name,
            fraction * 100.0
        );
    }
    if let Some(download) = snapshot.download.as_ref() {
        return format!(
            "pulling image for {} · {}",
            service_name,
            pull_download_summary(download)
        );
    }
    format!("pulling image for {service_name}")
}

pub(super) fn pull_progress_signature(snapshot: &PullProgressSnapshot) -> String {
    let Some(download) = snapshot.download.as_ref() else {
        return format!("fraction={:?}", snapshot.fraction);
    };
    format!(
        "fraction={:?};current={:?};total={:?};layers={:?}/{:?};active={}",
        snapshot.fraction,
        download.current_bytes,
        download.total_bytes,
        download.completed_layers,
        download.total_layers,
        download.active_layers.join("|")
    )
}
