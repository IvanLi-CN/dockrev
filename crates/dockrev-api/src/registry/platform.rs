use super::*;

pub(super) fn parse_manifest_value(
    value: &serde_json::Value,
    digest: Option<String>,
    host_platform: &str,
) -> anyhow::Result<ManifestInfo> {
    let mut arch = Vec::new();
    let mut host_platform_digest_exact: Option<String> = None;
    let mut host_platform_digest_base_matches = Vec::new();
    let host_base = host_platform
        .split('/')
        .take(2)
        .collect::<Vec<_>>()
        .join("/");
    if let Some(manifests) = value.get("manifests").and_then(|v| v.as_array()) {
        for manifest in manifests {
            let os = manifest
                .get("platform")
                .and_then(|platform| platform.get("os"))
                .and_then(|value| value.as_str());
            let architecture = manifest
                .get("platform")
                .and_then(|platform| platform.get("architecture"))
                .and_then(|value| value.as_str());
            let variant = manifest
                .get("platform")
                .and_then(|platform| platform.get("variant"))
                .and_then(|value| value.as_str());

            if let (Some(os), Some(architecture)) = (os, architecture) {
                let platform = variant.map_or_else(
                    || format!("{os}/{architecture}"),
                    |variant| format!("{os}/{architecture}/{variant}"),
                );
                arch.push(platform);
            }

            let Some(digest) = manifest.get("digest").and_then(|value| value.as_str()) else {
                continue;
            };
            if host_platform_digest_exact.is_none()
                && platform_matches(host_platform, os, architecture, variant)
            {
                host_platform_digest_exact = Some(digest.to_string());
                continue;
            }

            // Only use a base-platform digest when it is unambiguous across variants.
            if let (Some(os), Some(architecture)) = (os, architecture)
                && format!("{os}/{architecture}") == host_base
            {
                host_platform_digest_base_matches.push(digest.to_string());
            }
        }
    } else if let (Some(os), Some(architecture)) = (
        value.get("os").and_then(|value| value.as_str()),
        value.get("architecture").and_then(|value| value.as_str()),
    ) {
        arch.push(format!("{os}/{architecture}"));
    }

    arch.sort();
    arch.dedup();
    let platform_digest = if host_platform_digest_exact.is_some() {
        host_platform_digest_exact
    } else {
        host_platform_digest_base_matches.sort();
        host_platform_digest_base_matches.dedup();
        (host_platform_digest_base_matches.len() == 1)
            .then(|| host_platform_digest_base_matches.remove(0))
    };
    Ok(ManifestInfo {
        digest: digest.or(platform_digest.clone()),
        platform_digest,
        arch,
    })
}

pub(super) fn merge_config_blob_platform(
    info: &mut ManifestInfo,
    manifest: &serde_json::Value,
    config_blob: &serde_json::Value,
    host_platform: &str,
) {
    if !info.arch.is_empty()
        || manifest.get("manifests").is_some()
        || manifest_config_digest(manifest).is_none()
    {
        return;
    }

    let Some((platform, variant)) = config_blob_platform(config_blob) else {
        return;
    };
    let full_platform = variant.as_deref().map_or_else(
        || platform.clone(),
        |variant| format!("{platform}/{variant}"),
    );
    info.arch.push(full_platform);
    if variant.as_deref() == Some("v8")
        && platform == "linux/arm64"
        && host_platform == "linux/arm64"
    {
        info.arch.push(platform);
    }
    info.arch.sort();
    info.arch.dedup();
}

fn config_blob_platform(value: &serde_json::Value) -> Option<(String, Option<String>)> {
    let os = value.get("os").and_then(|value| value.as_str())?.trim();
    let architecture = value
        .get("architecture")
        .and_then(|value| value.as_str())?
        .trim();
    if os.is_empty() || architecture.is_empty() {
        return None;
    }

    let variant = value
        .get("variant")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|variant| !variant.is_empty())
        .map(str::to_string);
    Some((format!("{os}/{architecture}"), variant))
}

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
