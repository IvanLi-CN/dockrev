use semver::Version;

pub fn normalized_semver_from_oci_version(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "<no value>" {
        return None;
    }
    let normalized = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let version = Version::parse(normalized).ok()?;
    if !version.build.is_empty() {
        return None;
    }
    Some(version.to_string())
}

#[cfg(test)]
mod tests {
    use super::normalized_semver_from_oci_version;

    #[test]
    fn normalizes_v_prefix_and_rejects_build_metadata() {
        assert_eq!(
            normalized_semver_from_oci_version(" v0.7.7\n"),
            Some("0.7.7".to_string())
        );
        assert_eq!(
            normalized_semver_from_oci_version("0.7.7+build.1"),
            None,
            "docker tags cannot include '+' build metadata"
        );
    }
}
