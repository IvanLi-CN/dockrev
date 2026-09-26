use super::*;

#[test]
fn notification_identities_are_stable_across_job_retries() {
    let service = |service_id: &str, candidate_digest: &str| NewVersionDiscoveredService {
        stack_id: "stack".to_string(),
        service_id: service_id.to_string(),
        image_ref: format!("ghcr.io/acme/{service_id}"),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "1.0.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.1.0".to_string(),
        candidate_digest: candidate_digest.to_string(),
    };
    let first = vec![service("web", "sha256:web"), service("api", "sha256:api")];
    let reordered = vec![service("api", "sha256:api"), service("web", "sha256:web")];

    assert_eq!(
        new_version_notification_identity(&first),
        new_version_notification_identity(&reordered)
    );
    assert_ne!(
        new_version_notification_identity(&first),
        new_version_notification_identity(&[service("web", "sha256:next")])
    );

    let first_anomaly = vec![GhcrWebhookAnomalyRepo {
        owner: "acme".to_string(),
        repo: "web".to_string(),
        state: "missing".to_string(),
        last_error: None,
        occurrence_count: 1,
    }];
    let retried_anomaly = vec![GhcrWebhookAnomalyRepo {
        occurrence_count: 1,
        ..first_anomaly[0].clone()
    }];
    let recurring_anomaly = vec![GhcrWebhookAnomalyRepo {
        occurrence_count: 2,
        ..first_anomaly[0].clone()
    }];
    assert_eq!(
        ghcr_anomaly_notification_identity(&first_anomaly),
        ghcr_anomaly_notification_identity(&retried_anomaly)
    );
    assert_ne!(
        ghcr_anomaly_notification_identity(&first_anomaly),
        ghcr_anomaly_notification_identity(&recurring_anomaly)
    );
}
