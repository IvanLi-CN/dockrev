use super::*;

pub(crate) fn configured_tag_observation_version(
    configured_tag: &str,
    observed_digest: &str,
    candidate_digest: Option<&str>,
    candidate_resolved_tag: Option<&str>,
) -> Option<String> {
    let configured_tag = configured_tag.trim();
    if crate::ignore::is_strict_semver(configured_tag) {
        return Some(configured_tag.to_string());
    }
    let observed_digest = snapshot_worker::normalize_digest(observed_digest);
    let candidate_digest = candidate_digest.and_then(snapshot_worker::normalize_digest);
    if observed_digest.is_none() || observed_digest != candidate_digest {
        return None;
    }
    let candidate_resolved_tag = candidate_resolved_tag?.trim();
    crate::ignore::is_strict_semver(candidate_resolved_tag)
        .then(|| candidate_resolved_tag.to_string())
}

#[derive(Clone, Debug)]
pub(crate) struct CheckConfiguredTagObservation {
    pub(crate) service_id: String,
    pub(crate) image_repo: String,
    pub(crate) configured_tag: String,
    pub(crate) digest: String,
    pub(crate) version: Option<String>,
    pub(crate) observed_at: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CheckDiscoveredVersion {
    pub(crate) stack_id: String,
    pub(crate) service_id: String,
    pub(crate) service_name: String,
    pub(crate) image_ref: String,
    pub(crate) current_tag: String,
    pub(crate) current_digest: Option<String>,
    pub(crate) current_display_tag: String,
    pub(crate) candidate_tag: String,
    pub(crate) candidate_display_tag: String,
    pub(crate) candidate_digest: String,
}
