use axum::http::{HeaderMap, HeaderName};

use super::{App, errors::ApiError};

pub(crate) fn require_user(app: &App, headers: &HeaderMap) -> Result<String, ApiError> {
    let user = read_header_value(headers, &app.cfg.auth_forward_header_name);
    let groups = read_groups(headers, &app.cfg.auth_group_header_name);
    let allowlist_configured =
        app.cfg.auth_allowed_user.is_some() || app.cfg.auth_allowed_group.is_some();

    if let Some(current_user) = user.as_deref()
        && app.cfg.auth_allowed_user.as_deref() == Some(current_user)
    {
        return Ok(current_user.to_string());
    }

    if let Some(allowed_group) = app.cfg.auth_allowed_group.as_deref()
        && groups.iter().any(|group| group == allowed_group)
    {
        return Ok(user.unwrap_or_else(|| format!("group:{allowed_group}")));
    }

    if app.cfg.auth_allow_anonymous_in_dev && !allowlist_configured {
        return Ok(user.unwrap_or_else(|| "anonymous".to_string()));
    }

    Err(ApiError::auth_required())
}

fn read_header_value(headers: &HeaderMap, name: &HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn read_groups(headers: &HeaderMap, name: &HeaderName) -> Vec<String> {
    let Some(raw) = read_header_value(headers, name) else {
        return Vec::new();
    };
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}
