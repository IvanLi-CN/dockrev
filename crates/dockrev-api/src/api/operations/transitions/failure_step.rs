use super::*;

pub(crate) fn extract_stack_transition_summary(
    stack: &serde_json::Value,
    kind: TransitionJobKind,
) -> Option<&serde_json::Value> {
    stack.get(kind.summary_key())
}

pub(crate) fn transition_failure_step(
    kind: TransitionJobKind,
    stack_summaries: &[serde_json::Value],
) -> Option<&str> {
    stack_summaries.iter().find_map(|stack| {
        extract_stack_transition_summary(stack, kind)
            .and_then(|summary| summary.get("failureStep"))
            .and_then(|value| value.as_str())
    })
}

pub(crate) fn transition_terminal_message(
    kind: TransitionJobKind,
    final_status: &str,
    stack_summaries: &[serde_json::Value],
) -> String {
    match kind {
        TransitionJobKind::Update => {
            if final_status == "success" {
                return "update finished".to_string();
            }
            if final_status == "cancelled" {
                return "update cancelled".to_string();
            }
            if final_status == "rolled_back" {
                return match transition_failure_step(kind, stack_summaries) {
                    Some("healthcheck") => {
                        "update rolled back after healthcheck failure".to_string()
                    }
                    Some("pull_target_tag") => {
                        "update rolled back after target tag pull failure".to_string()
                    }
                    Some("sync_configured_tag") => {
                        "update rolled back after compose tag sync failure".to_string()
                    }
                    _ => "update rolled back".to_string(),
                };
            }
            "update finished with failures".to_string()
        }
        TransitionJobKind::Rollback => {
            if final_status == "rolled_back" {
                return "rollback finished".to_string();
            }
            match transition_failure_step(kind, stack_summaries) {
                Some("healthcheck") => "rollback failed after healthcheck failure".to_string(),
                Some("pull_target_tag") => {
                    "rollback failed after target tag pull failure".to_string()
                }
                Some("sync_configured_tag") => {
                    "rollback failed after compose tag sync failure".to_string()
                }
                _ => "rollback failed".to_string(),
            }
        }
    }
}

pub(crate) fn normalize_transition_outcome_status(
    kind: TransitionJobKind,
    outcome_status: &str,
) -> String {
    match kind {
        TransitionJobKind::Update => outcome_status.to_string(),
        TransitionJobKind::Rollback => match outcome_status {
            "success" => "rolled_back".to_string(),
            _ => "failed".to_string(),
        },
    }
}

pub(crate) fn transition_has_failure_step(
    kind: TransitionJobKind,
    stack_summaries: &[serde_json::Value],
    expected_step: &str,
) -> bool {
    stack_summaries.iter().any(|stack| {
        stack
            .get(kind.summary_key())
            .and_then(|summary| summary.get("failureStep"))
            .and_then(serde_json::Value::as_str)
            == Some(expected_step)
    })
}

pub(crate) fn transition_requires_evidence_failure_metadata(
    kind: TransitionJobKind,
    mode: &str,
    stack_summaries: &[serde_json::Value],
    healthcheck_failure_observed: bool,
) -> bool {
    kind == TransitionJobKind::Update
        && mode == "apply"
        && (healthcheck_failure_observed
            || transition_has_failure_step(kind, stack_summaries, "healthcheck"))
}
