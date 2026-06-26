use ring::digest::{SHA256, digest};

use super::*;

pub(super) fn ensure_success(ctx: &str, out: &CommandOutput) -> anyhow::Result<()> {
    if out.status == 0 {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "{ctx} failed: status={} stderr={}",
        out.status,
        out.stderr.trim()
    ))
}

pub(super) fn parse_human_size(input: &str) -> Option<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut split_idx = 0usize;
    for (idx, ch) in trimmed.char_indices() {
        if !(ch.is_ascii_digit() || ch == '.') {
            split_idx = idx;
            break;
        }
    }
    if split_idx == 0 && !trimmed.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
        return None;
    }
    let (num, unit) = if split_idx == 0 {
        (trimmed, "")
    } else {
        trimmed.split_at(split_idx)
    };
    let value = num.trim().parse::<f64>().ok()?;
    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1_f64,
        "kb" | "k" => 1000_f64,
        "mb" | "m" => 1000_f64.powi(2),
        "gb" | "g" => 1000_f64.powi(3),
        "tb" | "t" => 1000_f64.powi(4),
        "kib" => 1024_f64,
        "mib" => 1024_f64.powi(2),
        "gib" => 1024_f64.powi(3),
        "tib" => 1024_f64.powi(4),
        _ => return None,
    };
    Some((value * multiplier).round() as u64)
}

pub(super) fn parse_buildx_du_json_lines(input: &str) -> Option<BuilderCacheEstimate> {
    let mut parsed_any = false;
    let mut reclaimable = 0_u64;
    let mut has_shared_reclaimable = false;

    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let record = serde_json::from_str::<BuildxDuRecord>(line).ok()?;
        parsed_any = true;
        if !record.reclaimable {
            continue;
        }
        if record.shared {
            has_shared_reclaimable = true;
            continue;
        }
        let size = parse_size_value(&record.size)?;
        reclaimable = reclaimable.saturating_add(size);
    }

    parsed_any.then_some(BuilderCacheEstimate {
        reclaimable_bytes: Some(reclaimable),
        estimate_unknown: has_shared_reclaimable,
        fingerprint_hint: None,
    })
}

pub(super) fn parse_buildx_du_text_summary(input: &str) -> Option<u64> {
    input
        .lines()
        .find_map(|line| line.strip_prefix("Reclaimable:"))
        .and_then(|raw| parse_human_size(raw.trim()))
}

pub(super) fn parse_du_kilobytes_output(input: &str) -> Option<u64> {
    let line = input.lines().find(|line| !line.trim().is_empty())?;
    let kib = line
        .split_whitespace()
        .next()
        .and_then(|raw| raw.parse::<u64>().ok())?;
    kib.checked_mul(1024)
}

pub(super) fn parse_df_bytes_output(input: &str) -> Option<(u64, u64)> {
    let mut wrapped_filesystem = false;
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Filesystem") {
            continue;
        }

        let columns = trimmed.split_whitespace().collect::<Vec<_>>();
        if wrapped_filesystem && columns.len() >= 2 {
            let total = columns[0].parse::<u64>().ok()?;
            let used = columns[1].parse::<u64>().ok()?;
            return (total > 0 && used <= total).then_some((used, total));
        }

        if columns.len() >= 3 {
            let total = columns[1].parse::<u64>().ok()?;
            let used = columns[2].parse::<u64>().ok()?;
            return (total > 0 && used <= total).then_some((used, total));
        }

        wrapped_filesystem = true;
    }
    None
}

pub(super) fn fingerprint_hint_from_buildx_text_output(raw: &str) -> Option<String> {
    let mut records = raw
        .lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with("Reclaimable:"))
        .filter(|line| !line.is_empty())
        .filter_map(parse_buildx_text_inventory_record)
        .collect::<Vec<_>>();
    if records.is_empty() {
        return None;
    }
    records.sort_unstable();
    let normalized = records.join("\n");
    let hashed = digest(&SHA256, normalized.as_bytes());
    Some(hex::encode(hashed.as_ref()))
}

fn parse_size_value(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Number(number) => number.as_u64(),
        serde_json::Value::String(text) => {
            text.parse::<u64>().ok().or_else(|| parse_human_size(text))
        }
        _ => None,
    }
}

pub(super) fn parse_volume_sizes_from_system_df_verbose(input: &str) -> BTreeMap<String, u64> {
    let mut in_volume_section = false;
    let mut volume_sizes = BTreeMap::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if !in_volume_section {
            if trimmed == "Local Volumes space usage:" {
                in_volume_section = true;
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with("NAME") {
            continue;
        }
        if trimmed.ends_with("space usage:") {
            break;
        }

        let columns = split_table_columns(line);
        if columns.len() < 3 {
            continue;
        }
        let Some(size) = parse_human_size(columns[2]) else {
            continue;
        };
        volume_sizes.insert(columns[0].to_string(), size);
    }

    volume_sizes
}

pub(super) fn fingerprint_hint_from_output(raw: &str) -> Option<String> {
    let mut lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(normalize_buildx_fingerprint_record)
        .collect::<Option<Vec<_>>>()?;
    lines.sort_unstable();
    let normalized = lines.join("\n");
    if normalized.is_empty() {
        return None;
    }
    let hashed = digest(&SHA256, normalized.as_bytes());
    Some(hex::encode(hashed.as_ref()))
}

fn normalize_buildx_fingerprint_record(line: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let object = value.as_object()?;
    let mut normalized = serde_json::Map::new();

    for key in [
        "ID",
        "Parents",
        "Description",
        "Mutable",
        "Reclaimable",
        "Shared",
        "Size",
        "InUse",
        "Type",
    ] {
        let Some(value) = object.get(key).filter(|value| !value.is_null()) else {
            continue;
        };
        let canonical = if key == "Parents" {
            normalize_parent_list(value)
        } else {
            value.clone()
        };
        normalized.insert(key.to_string(), canonical);
    }

    if normalized.is_empty() {
        return None;
    }

    serde_json::to_string(&serde_json::Value::Object(normalized)).ok()
}

fn normalize_parent_list(value: &serde_json::Value) -> serde_json::Value {
    let Some(items) = value.as_array() else {
        return value.clone();
    };
    let mut parents = items
        .iter()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    if parents.len() != items.len() {
        return value.clone();
    }
    parents.sort_unstable();
    serde_json::Value::Array(
        parents
            .into_iter()
            .map(serde_json::Value::String)
            .collect::<Vec<_>>(),
    )
}

pub(super) async fn resolve_volume_fingerprint_with_runner(
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
    volume: &DockerVolumeInspect,
) -> Option<String> {
    if let Some(labels) = volume.labels.as_ref() {
        let project = labels
            .get("com.docker.compose.project")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());
        let volume_name = labels
            .get("com.docker.compose.volume")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());
        if let (Some(project), Some(volume_name)) = (project, volume_name) {
            return Some(format!("volume:{project}:{volume_name}"));
        }
    }
    if let Some(created_at) = volume
        .created_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("volume:{}:{}", volume.name, created_at));
    }
    let mountpoint = volume.mountpoint.as_deref()?.trim();
    if mountpoint.is_empty() {
        return Some(format!("volume:{}", volume.name));
    }
    let size = scan_volume_size_from_mountpoint_with_runner(runner, mountpoint).await;
    Some(format!(
        "volume:{}:{}",
        volume.name,
        size.map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ))
}

pub(super) async fn scan_volume_sizes_from_system_df_with_runner(
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> BTreeMap<String, u64> {
    let out = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec!["system".to_string(), "df".to_string(), "-v".to_string()],
                env: Vec::new(),
            },
            DOCKER_TIMEOUT,
        )
        .await;
    match out {
        Ok(output) if output.status == 0 => {
            parse_volume_sizes_from_system_df_verbose(&output.stdout)
        }
        _ => BTreeMap::new(),
    }
}

pub(super) async fn scan_volume_size_from_mountpoint_with_runner(
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
    mountpoint: &str,
) -> Option<u64> {
    let out = runner
        .run(
            CommandSpec {
                program: "du".to_string(),
                args: vec!["-sk".to_string(), mountpoint.to_string()],
                env: Vec::new(),
            },
            DOCKER_TIMEOUT,
        )
        .await
        .ok()?;
    if out.status != 0 {
        return None;
    }
    parse_du_kilobytes_output(&out.stdout)
}

pub(super) async fn scan_server_disk_usage_with_runner(
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> Option<(u64, u64)> {
    let out = runner
        .run(
            CommandSpec {
                program: "df".to_string(),
                args: vec!["-B1".to_string(), ".".to_string()],
                env: Vec::new(),
            },
            DOCKER_TIMEOUT,
        )
        .await
        .ok()?;
    if out.status != 0 {
        return None;
    }
    parse_df_bytes_output(&out.stdout)
}

fn parse_buildx_text_inventory_record(line: &str) -> Option<String> {
    let mut columns = line.split_whitespace();
    let id = columns.next()?;
    if matches!(
        id,
        "ID" | "TYPE" | "NAME" | "Description" | "TOTAL" | "Total" | "SIZE"
    ) {
        return None;
    }
    let reclaimable = columns.next()?;
    let size = columns.next()?;
    Some(format!("{id}\t{reclaimable}\t{size}"))
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn volume_fingerprint_key(volume: &DockerVolumeInspect) -> Option<String> {
    volume
        .created_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|created_at| format!("volume:{}:created:{created_at}", volume.name))
}

fn split_table_columns(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut columns = Vec::new();
    let mut start = 0usize;
    let mut idx = 0usize;

    while idx < bytes.len() {
        if !bytes[idx].is_ascii_whitespace() {
            idx += 1;
            continue;
        }

        let mut end = idx;
        while end < bytes.len() && bytes[end].is_ascii_whitespace() {
            end += 1;
        }
        if end.saturating_sub(idx) < 2 {
            idx = end;
            continue;
        }

        let column = line[start..idx].trim();
        if !column.is_empty() {
            columns.push(column);
        }
        start = end;
        idx = end;
    }

    let tail = line[start..].trim();
    if !tail.is_empty() {
        columns.push(tail);
    }

    columns
}
