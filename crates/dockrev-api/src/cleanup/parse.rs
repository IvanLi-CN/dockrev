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
        .collect::<Vec<_>>();
    lines.sort_unstable();
    let normalized = lines.join("\n");
    if normalized.is_empty() {
        return None;
    }
    let hashed = digest(&SHA256, normalized.as_bytes());
    Some(hex::encode(hashed.as_ref()))
}

pub(super) fn volume_fingerprint_key(volume: &DockerVolumeInspect) -> Option<String> {
    volume
        .created_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|created_at| format!("volume:{}:{created_at}", volume.name))
}

pub(super) async fn scan_volume_sizes_from_system_df(state: &AppState) -> BTreeMap<String, u64> {
    let out = match state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec!["system".to_string(), "df".to_string(), "-v".to_string()],
                env: Vec::new(),
            },
            DOCKER_TIMEOUT,
        )
        .await
    {
        Ok(out) if out.status == 0 => out,
        _ => return BTreeMap::new(),
    };
    parse_volume_sizes_from_system_df_verbose(&out.stdout)
}

pub(super) async fn scan_volume_size_from_mountpoint(
    state: &AppState,
    mountpoint: &str,
) -> Option<u64> {
    let mountpoint = mountpoint.trim();
    if mountpoint.is_empty() {
        return None;
    }
    let out = state
        .runner
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
