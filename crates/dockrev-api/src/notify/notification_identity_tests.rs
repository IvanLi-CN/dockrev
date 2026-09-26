use super::*;

#[test]
fn notification_identities_are_stable_across_job_retries() {
    let first = std::collections::BTreeSet::from(["check-1".to_string()]);
    let reordered = std::collections::BTreeSet::from(["check-1".to_string()]);
    let second = std::collections::BTreeSet::from(["check-2".to_string()]);

    assert_eq!(
        new_version_notification_identity(&first),
        new_version_notification_identity(&reordered)
    );
    assert_ne!(
        new_version_notification_identity(&first),
        new_version_notification_identity(&second)
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
    let first_batch =
        std::collections::HashMap::from([("acme/api".to_string(), "batch-1".to_string())]);
    let recurring_batch =
        std::collections::HashMap::from([("acme/api".to_string(), "batch-2".to_string())]);
    assert_eq!(
        ghcr_anomaly_notification_identity(&first_anomaly, &first_batch),
        ghcr_anomaly_notification_identity(&retried_anomaly, &first_batch)
    );
    assert_ne!(
        ghcr_anomaly_notification_identity(&first_anomaly, &first_batch),
        ghcr_anomaly_notification_identity(&recurring_anomaly, &recurring_batch)
    );
}
