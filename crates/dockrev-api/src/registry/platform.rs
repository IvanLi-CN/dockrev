use super::*;

pub(super) fn platform_matches(
    host_platform: &str,
    os: Option<&str>,
    architecture: Option<&str>,
    variant: Option<&str>,
) -> bool {
    let (Some(os), Some(architecture)) = (os, architecture) else {
        return false;
    };
    let candidate = if let Some(variant) = variant {
        format!("{os}/{architecture}/{variant}")
    } else {
        format!("{os}/{architecture}")
    };
    candidate == host_platform
}

pub fn host_platform_override(config_value: Option<&str>) -> Option<String> {
    if let Some(v) = config_value {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };

    Some(format!("linux/{arch}"))
}

pub fn compute_arch_match(host_platform: &str, arch: &[String]) -> ArchMatch {
    if arch.is_empty() {
        return ArchMatch::Unknown;
    }
    if arch.iter().any(|p| p == host_platform) {
        return ArchMatch::Match;
    }
    // Best-effort: tolerate missing variant for arm64.
    let host_no_variant = host_platform
        .split('/')
        .take(2)
        .collect::<Vec<_>>()
        .join("/");
    if host_no_variant != host_platform && arch.iter().any(|p| p == &host_no_variant) {
        return ArchMatch::Match;
    }
    ArchMatch::Mismatch
}
