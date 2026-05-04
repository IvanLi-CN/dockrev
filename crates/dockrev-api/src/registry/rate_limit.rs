use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::Duration,
};

use reqwest::header::RETRY_AFTER;

pub(super) fn parse_retry_after_delay(
    headers: &reqwest::header::HeaderMap,
    max_ms: u64,
) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if raw.is_empty() {
        return None;
    }

    let cap_ms = max_ms.max(1);

    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_millis(
            seconds.saturating_mul(1000).min(cap_ms),
        ));
    }

    if let Ok(at) = time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc2822)
    {
        let now = time::OffsetDateTime::now_utc();
        let delta = at - now;
        let millis = if delta.is_negative() {
            0
        } else {
            delta.whole_milliseconds().try_into().unwrap_or(u64::MAX)
        };
        return Some(Duration::from_millis(millis.min(cap_ms)));
    }

    None
}

pub(super) fn parse_ratelimit_remaining(
    headers: &reqwest::header::HeaderMap,
) -> Option<(i64, Option<i64>)> {
    fn parse_first_number(raw: &str) -> Option<i64> {
        raw.split(|ch: char| !(ch.is_ascii_digit() || ch == '-'))
            .find(|part| !part.is_empty() && *part != "-")
            .and_then(|part| part.parse::<i64>().ok())
    }

    let remaining = headers
        .get("ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_first_number)?;
    let limit = headers
        .get("ratelimit-limit")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_first_number);
    Some((remaining, limit))
}

pub(super) fn is_registry_rate_limit_error_text(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    lower.contains("too many requests")
        || lower.contains("toomanyrequests")
        || lower.contains("pull rate limit")
        || lower.contains("rate limit")
        || lower.contains("429")
}

pub(super) fn retry_backoff_with_jitter(
    base_ms: u64,
    max_ms: u64,
    attempt: usize,
    host: &str,
    request_seed: u64,
) -> Duration {
    let cap_ms = max_ms.max(1);
    let factor = 1u64
        .checked_shl((attempt as u32).min(16))
        .unwrap_or(u64::MAX);
    let raw_ms = base_ms.saturating_mul(factor).min(cap_ms);

    let jitter_cap = (base_ms / 2).max(1);
    let mut hasher = DefaultHasher::new();
    host.hash(&mut hasher);
    attempt.hash(&mut hasher);
    request_seed.hash(&mut hasher);
    let jitter_ms = hasher.finish() % (jitter_cap + 1);

    Duration::from_millis(raw_ms.saturating_add(jitter_ms).min(cap_ms))
}
