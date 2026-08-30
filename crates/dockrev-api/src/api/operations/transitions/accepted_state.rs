use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) fn accepted_state_from_check(
    service: &crate::db::ServiceForCheck,
    outcome: &service_check::ServiceCheckOutcome,
    checked_at: &str,
) -> crate::db::ServiceAcceptedState {
    crate::db::ServiceAcceptedState {
        image_ref: service.image_ref.clone(),
        image_tag: service.image_tag.clone(),
        current_digest: outcome.current_digest.clone(),
        current_runtime_started_at: outcome.current_runtime_started_at.clone(),
        current_resolved_tag: outcome.current_resolved_tag.clone(),
        current_resolved_tags_json: outcome.current_resolved_tags_json.clone(),
        candidate_tag: outcome.candidate_tag.clone(),
        candidate_resolved_tag: outcome.candidate_resolved_tag.clone(),
        candidate_digest: outcome.candidate_digest.clone(),
        candidate_arch_match: outcome.candidate_arch_match.clone(),
        candidate_arch_json: outcome.candidate_arch_json.clone(),
        ignore_rule_id: outcome.ignore_rule_id.clone(),
        ignore_reason: outcome.ignore_reason.clone(),
        checked_at: Some(checked_at.to_string()),
    }
}

pub(crate) fn combined_backup_pull_percent(
    processed_stacks: u32,
    total_stacks: u32,
    pull_branch_percent: &AtomicU32,
    backup_branch_percent: &AtomicU32,
) -> u32 {
    if total_stacks == 0 {
        return 0;
    }
    let pull = pull_branch_percent.load(Ordering::Relaxed).min(10_000) as f64 / 10_000.0;
    let backup = backup_branch_percent.load(Ordering::Relaxed).min(10_000) as f64 / 10_000.0;
    (((processed_stacks as f64 + pull * 0.35 + backup * 0.40) / total_stacks as f64) * 100.0)
        .floor() as u32
}
