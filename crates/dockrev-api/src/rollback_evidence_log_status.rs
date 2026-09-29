use super::EvidenceMetadata;

pub(super) fn record_log_command_result(
    metadata: &mut EvidenceMetadata,
    output: &crate::runner::RawFileCommandOutput,
) -> bool {
    if output.timed_out {
        metadata.capture_errors.push(format!(
            "logs command timed out after {} seconds",
            super::LOG_CAPTURE_TIMEOUT_SECONDS
        ));
        return false;
    }
    if output.status != 0 {
        metadata
            .capture_errors
            .push(format!("logs command exited with {}", output.status));
        return false;
    }
    if !output.eof_reached {
        metadata
            .capture_errors
            .push("logs command did not reach EOF".to_string());
        return false;
    }
    true
}
