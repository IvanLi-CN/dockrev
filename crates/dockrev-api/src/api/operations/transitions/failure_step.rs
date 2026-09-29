use super::*;

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
