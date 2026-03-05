pub fn canonicalize_for_store(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalize a cron expression for parsing.
///
/// Supported inputs:
/// - 5 fields: `min hour dom mon dow` (we prepend `0` seconds)
/// - 6 fields: `sec min hour dom mon dow`
/// - 7 fields: `sec min hour dom mon dow year`
pub fn normalize_cron(input: &str) -> anyhow::Result<String> {
    let fields = input.split_whitespace().collect::<Vec<_>>();
    match fields.len() {
        5 => Ok(format!("0 {}", fields.join(" "))),
        6 | 7 => Ok(fields.join(" ")),
        0 => Err(anyhow::anyhow!("cron expression is empty")),
        n => Err(anyhow::anyhow!(
            "cron expression must have 5, 6, or 7 fields; got {n}"
        )),
    }
}
