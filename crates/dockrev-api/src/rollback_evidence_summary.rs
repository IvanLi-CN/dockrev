use serde_json::Value;

use super::{
    MAX_SUMMARY_CAPTURE_ERRORS, MAX_SUMMARY_FIELD_CHARS, MAX_SUMMARY_SERVICES,
    bounded_summary_errors, truncate_summary_text,
};

pub(crate) fn sanitize_summary_for_api(summary: &Value) -> Value {
    let mut summary = summary.clone();
    if let Some(metadata) = summary.get_mut("rollbackEvidence") {
        sanitize_evidence_metadata(metadata);
    }
    summary
}

pub(crate) fn sanitize_evidence_metadata(metadata: &mut Value) {
    let Some(metadata) = metadata.as_object_mut() else {
        return;
    };
    let mut truncated = false;
    if let Some(services) = metadata.get_mut("services").and_then(Value::as_array_mut) {
        if services.len() > MAX_SUMMARY_SERVICES {
            services.truncate(MAX_SUMMARY_SERVICES);
            truncated = true;
        }
        for service in services {
            let Some(service) = service.as_object_mut() else {
                continue;
            };
            service.remove("stateError");
            service.remove("state_error");
            for field in ["serviceId", "candidateId", "healthStatus", "stateStatus"] {
                if let Some(Value::String(value)) = service.get_mut(field) {
                    truncated |= truncate_summary_text(value, MAX_SUMMARY_FIELD_CHARS);
                }
            }
            if let Some(capture_errors) = service
                .get_mut("captureErrors")
                .and_then(Value::as_array_mut)
            {
                if capture_errors.len() > MAX_SUMMARY_CAPTURE_ERRORS {
                    capture_errors.truncate(MAX_SUMMARY_CAPTURE_ERRORS);
                    truncated = true;
                }
                for error in capture_errors {
                    if let Value::String(error) = error {
                        truncated |= truncate_summary_text(error, MAX_SUMMARY_FIELD_CHARS);
                    }
                }
            }
        }
    }

    let errors_present = metadata.contains_key("errors");
    let errors = metadata
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if errors_present || truncated {
        metadata.insert(
            "errors".to_string(),
            Value::Array(
                bounded_summary_errors(errors, truncated)
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
}
