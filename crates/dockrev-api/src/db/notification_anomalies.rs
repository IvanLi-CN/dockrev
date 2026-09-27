use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};
use std::collections::{BTreeSet, HashMap};

use super::Db;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationAnomalyObservation {
    pub owner: String,
    pub repo: String,
    pub state: String,
    pub last_error: Option<String>,
    pub occurrence_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationAnomalyOccurrence {
    pub observation: NotificationAnomalyObservation,
    pub batch_id: String,
    pub notification_item_id: Option<String>,
    pub sent_channels: Vec<String>,
    pub source_job_id: Option<String>,
    pub source_status: String,
}

const ANOMALY_DELIVERY_CLAIM_TTL_MINUTES: i64 = 5;

fn anomaly_delivery_claim_expiry(now: &str) -> anyhow::Result<String> {
    Ok(
        (time::OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339)?
            + time::Duration::minutes(ANOMALY_DELIVERY_CLAIM_TTL_MINUTES))
        .format(&time::format_description::well_known::Rfc3339)?,
    )
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    fn observation(state: &str) -> NotificationAnomalyObservation {
        NotificationAnomalyObservation {
            owner: "acme".to_string(),
            repo: "api".to_string(),
            state: state.to_string(),
            last_error: None,
            occurrence_count: 0,
        }
    }

    #[tokio::test]
    async fn anomaly_state_only_emits_new_and_changed_active_states() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];

        let first = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:00:00Z",
            )
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        mark_notified(&db, &first).await;
        assert!(
            db.reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:01:00Z",
            )
            .await
            .unwrap()
            .is_empty()
        );
        let changed = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("conflict")],
                "2026-09-26T00:02:00Z",
            )
            .await
            .unwrap();
        assert_eq!(changed.len(), 1);
        mark_notified(&db, &changed).await;
        assert!(
            db.reconcile_notification_anomaly_states(&scope, &[], "2026-09-26T00:03:00Z",)
                .await
                .unwrap()
                .is_empty()
        );
        let recurring = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("conflict")],
                "2026-09-26T00:04:00Z",
            )
            .await
            .unwrap();
        assert_eq!(recurring.len(), 1);
        mark_notified(&db, &recurring).await;
    }

    async fn mark_notified(db: &Db, observations: &[NotificationAnomalyObservation]) {
        let keys = observations
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();
        db.mark_notification_anomaly_states_notified(&keys)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pending_anomaly_is_replayed_with_the_same_occurrence_identity() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];

        let first = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:00:00Z",
            )
            .await
            .unwrap();
        let retry = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:01:00Z",
            )
            .await
            .unwrap();

        assert_eq!(first[0].occurrence_count, 1);
        assert_eq!(retry, first);
        mark_notified(&db, &retry).await;
        assert!(
            db.reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:02:00Z",
            )
            .await
            .unwrap()
            .is_empty()
        );
    }

    #[tokio::test]
    async fn concurrent_anomaly_delivery_claim_allows_only_one_owner() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        db.reconcile_notification_anomaly_states(
            &["acme/api".to_string()],
            &[observation("missing")],
            "2026-09-26T00:00:00Z",
        )
        .await
        .unwrap();
        let occurrence = db
            .list_all_pending_notification_anomaly_occurrences()
            .await
            .unwrap()
            .pop()
            .unwrap();
        let first_db = db.clone();
        let second_db = db.clone();
        let (first, second) = tokio::join!(
            first_db.try_claim_notification_anomaly_batch(
                &occurrence.batch_id,
                "claim-a",
                "2026-09-26T00:01:00Z",
            ),
            second_db.try_claim_notification_anomaly_batch(
                &occurrence.batch_id,
                "claim-b",
                "2026-09-26T00:01:00Z",
            )
        );

        assert_eq!(first.unwrap() as u8 + second.unwrap() as u8, 1);
    }

    #[tokio::test]
    async fn disabled_anomaly_notifications_are_not_replayed_after_reenable() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];

        db.reconcile_notification_anomaly_states_with_enabled(
            &scope,
            &[observation("missing")],
            "2026-09-26T00:00:00Z",
            Some(false),
            None,
            "success",
        )
        .await
        .unwrap();
        assert!(
            db.list_all_pending_notification_anomaly_occurrences()
                .await
                .unwrap()
                .is_empty()
        );

        let reenabled = db
            .reconcile_notification_anomaly_states_with_enabled(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:01:00Z",
                Some(true),
                None,
                "success",
            )
            .await
            .unwrap();
        assert!(reenabled.is_empty());
        assert!(
            db.list_all_pending_notification_anomaly_occurrences()
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn disabling_anomaly_notifications_preserves_accepted_pending_occurrences() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];

        db.reconcile_notification_anomaly_states_with_enabled(
            &scope,
            &[observation("missing")],
            "2026-09-26T00:00:00Z",
            Some(true),
            None,
            "success",
        )
        .await
        .unwrap();
        db.reconcile_notification_anomaly_states_with_enabled(
            &scope,
            &[observation("missing")],
            "2026-09-26T00:01:00Z",
            Some(false),
            None,
            "success",
        )
        .await
        .unwrap();

        let pending = db
            .list_all_pending_notification_anomaly_occurrences()
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].observation.state, "missing");
        assert_eq!(pending[0].observation.occurrence_count, 1);
    }

    #[tokio::test]
    async fn new_anomaly_in_a_later_audit_gets_a_new_batch() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let first = db
            .reconcile_notification_anomaly_states(
                &["acme/api".to_string(), "acme/web".to_string()],
                &[observation("missing")],
                "2026-09-26T00:00:00Z",
            )
            .await
            .unwrap();
        let first_keys = first
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();
        let first_batch = db
            .list_notification_anomaly_batch_ids(&first_keys)
            .await
            .unwrap();

        let second = db
            .reconcile_notification_anomaly_states(
                &["acme/api".to_string(), "acme/web".to_string()],
                &[
                    observation("missing"),
                    NotificationAnomalyObservation {
                        owner: "acme".to_string(),
                        repo: "web".to_string(),
                        state: "conflict".to_string(),
                        last_error: None,
                        occurrence_count: 0,
                    },
                ],
                "2026-09-26T00:01:00Z",
            )
            .await
            .unwrap();
        let second_keys = second
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();
        let second_batch = db
            .list_notification_anomaly_batch_ids(&second_keys)
            .await
            .unwrap();

        assert_eq!(first_batch.get("acme/api"), second_batch.get("acme/api"));
        assert_ne!(first_batch.get("acme/api"), second_batch.get("acme/web"));
    }

    #[tokio::test]
    async fn changed_pending_anomaly_gets_a_new_occurrence_identity() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];
        let first = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:00:00Z",
            )
            .await
            .unwrap();
        let first_keys = first
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();
        let first_batch = db
            .list_notification_anomaly_batch_ids(&first_keys)
            .await
            .unwrap();

        let changed = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("conflict")],
                "2026-09-26T00:01:00Z",
            )
            .await
            .unwrap();
        let changed_keys = changed
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();
        let changed_batch = db
            .list_notification_anomaly_batch_ids(&changed_keys)
            .await
            .unwrap();

        assert_ne!(first_batch.get("acme/api"), changed_batch.get("acme/api"));
    }

    #[tokio::test]
    async fn rapid_anomaly_changes_keep_each_pending_occurrence() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];
        db.reconcile_notification_anomaly_states(
            &scope,
            &[observation("missing")],
            "2026-09-26T00:00:00Z",
        )
        .await
        .unwrap();
        let pending = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("conflict")],
                "2026-09-26T00:01:00Z",
            )
            .await
            .unwrap();
        let keys = pending
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();
        let occurrences = db
            .list_pending_notification_anomaly_occurrences(&keys)
            .await
            .unwrap();

        assert_eq!(occurrences.len(), 2);
        assert_ne!(occurrences[0].batch_id, occurrences[1].batch_id);

        db.mark_notification_anomaly_occurrences_item_persisted(&keys, "item-1")
            .await
            .unwrap();
        let persisted = db
            .list_pending_notification_anomaly_occurrences(&keys)
            .await
            .unwrap();
        assert!(
            persisted
                .iter()
                .all(|item| item.notification_item_id.as_deref() == Some("item-1"))
        );
    }

    #[tokio::test]
    async fn recovery_ends_pending_batch_before_a_new_occurrence() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];
        let first = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:00:00Z",
            )
            .await
            .unwrap();
        let first_keys = first
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();
        let first_batch = db
            .list_notification_anomaly_batch_ids(&first_keys)
            .await
            .unwrap();

        let recovered = db
            .reconcile_notification_anomaly_states(&scope, &[], "2026-09-26T00:01:00Z")
            .await
            .unwrap();
        assert_eq!(recovered, first);
        mark_notified(&db, &recovered).await;
        let recurring = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:02:00Z",
            )
            .await
            .unwrap();
        let recurring_keys = recurring
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();
        let recurring_batch = db
            .list_notification_anomaly_batch_ids(&recurring_keys)
            .await
            .unwrap();

        assert_ne!(first_batch.get("acme/api"), recurring_batch.get("acme/api"));
    }

    #[tokio::test]
    async fn anomaly_delivery_ledger_skips_successful_channels_on_retry() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];
        let active = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:00:00Z",
            )
            .await
            .unwrap();
        let keys = active
            .iter()
            .map(|item| {
                (
                    item.owner.clone(),
                    item.repo.clone(),
                    item.state.clone(),
                    item.occurrence_count,
                )
            })
            .collect::<Vec<_>>();

        db.record_notification_anomaly_delivery(&keys, &["webhook".to_string()], false)
            .await
            .unwrap();
        let channels = db
            .list_notification_anomaly_sent_channels(&keys)
            .await
            .unwrap();
        assert_eq!(channels.get("acme/api"), Some(&vec!["webhook".to_string()]));

        db.record_notification_anomaly_delivery(&keys, &[], true)
            .await
            .unwrap();
        assert_eq!(
            db.list_notification_anomaly_sent_channels(&keys)
                .await
                .unwrap()
                .get("acme/api"),
            Some(&Vec::new())
        );
    }

    #[tokio::test]
    async fn stale_audit_does_not_regress_a_newer_anomaly_state() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];

        let first = db
            .reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:02:00Z",
            )
            .await
            .unwrap();
        mark_notified(&db, &first).await;

        assert!(
            db.reconcile_notification_anomaly_states(
                &scope,
                &[observation("conflict")],
                "2026-09-26T00:01:00Z",
            )
            .await
            .unwrap()
            .is_empty()
        );
        assert!(
            db.reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:03:00Z",
            )
            .await
            .unwrap()
            .is_empty()
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_occurrence(
    tx: &rusqlite::Transaction<'_>,
    owner: &str,
    repo: &str,
    state: &str,
    last_error: Option<&str>,
    occurrence_count: i64,
    batch_id: &str,
    notification_pending: bool,
    source_job_id: Option<&str>,
    source_status: &str,
    created_at: &str,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO notification_anomaly_occurrences (id, owner, repo, state, last_error, occurrence_count, batch_id, notification_pending, source_job_id, source_status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            crate::ids::new_notification_id(),
            owner,
            repo,
            state,
            last_error,
            occurrence_count,
            batch_id,
            notification_pending,
            source_job_id,
            source_status,
            created_at,
        ],
    )?;
    Ok(())
}

impl Db {
    #[allow(dead_code)]
    pub(crate) async fn reconcile_notification_anomaly_states(
        &self,
        scope_keys: &[String],
        observations: &[NotificationAnomalyObservation],
        now: &str,
    ) -> anyhow::Result<Vec<NotificationAnomalyObservation>> {
        self.reconcile_notification_anomaly_states_with_enabled(
            scope_keys,
            observations,
            now,
            Some(true),
            None,
            "success",
        )
        .await
    }

    pub(crate) async fn reconcile_notification_anomaly_states_with_enabled(
        &self,
        scope_keys: &[String],
        observations: &[NotificationAnomalyObservation],
        now: &str,
        notification_enabled: Option<bool>,
        source_job_id: Option<&str>,
        source_status: &str,
    ) -> anyhow::Result<Vec<NotificationAnomalyObservation>> {
        self.reconcile_notification_anomaly_states_with_enabled_and_decision(
            scope_keys,
            observations,
            now,
            notification_enabled,
            source_job_id,
            source_status,
        )
        .await
        .map(|(_, pending)| pending)
    }

    pub(crate) async fn reconcile_notification_anomaly_states_with_enabled_and_decision(
        &self,
        scope_keys: &[String],
        observations: &[NotificationAnomalyObservation],
        now: &str,
        notification_enabled: Option<bool>,
        source_job_id: Option<&str>,
        source_status: &str,
    ) -> anyhow::Result<(bool, Vec<NotificationAnomalyObservation>)> {
        let scope_keys = scope_keys.to_vec();
        let observations = observations.to_vec();
        let now = now.to_string();
        let source_job_id = source_job_id.map(ToString::to_string);
        let source_status = source_status.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let notification_enabled = match notification_enabled {
                Some(enabled) => enabled,
                None => tx
                    .query_row(
                        "SELECT event_ghcr_webhook_anomaly_enabled FROM notification_settings LIMIT 1",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                    .unwrap_or(1)
                    != 0,
            };
            let notification_pending = notification_enabled;
            // One audit invocation owns one batch. Existing pending occurrences
            // keep their original batch so retries do not mutate an earlier item.
            let audit_batch_id = crate::ids::new_notification_id();
            let mut requested_keys = scope_keys.iter().cloned().collect::<BTreeSet<_>>();
            requested_keys.extend(
                observations
                    .iter()
                    .map(|item| format!("{}/{}", item.owner, item.repo)),
            );

            for observation in &observations {
                let previous = tx
                    .query_row(
                        "SELECT state, active, occurrence_count, notification_pending, notification_batch_id, last_seen_at FROM notification_anomaly_states WHERE owner = ?1 AND repo = ?2",
                        params![observation.owner, observation.repo],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, i64>(1)? != 0,
                                row.get::<_, i64>(2)?,
                                row.get::<_, i64>(3)? != 0,
                                row.get::<_, Option<String>>(4)?,
                                row.get::<_, String>(5)?,
                            ))
                        },
                    )
                    .optional()?;

                let (occurrence_count, occurrence_batch_id) = match previous {
                    None => {
                        let batch_id = audit_batch_id.clone();
                        insert_occurrence(
                            &tx,
                            &observation.owner,
                            &observation.repo,
                            &observation.state,
                            observation.last_error.as_deref(),
                            1,
                            &batch_id,
                            notification_pending,
                            source_job_id.as_deref(),
                            &source_status,
                            &now,
                        )?;
                        tx.execute(
                            "INSERT INTO notification_anomaly_states (owner, repo, state, active, occurrence_count, last_error, last_seen_at, notification_pending, notification_batch_id) VALUES (?1, ?2, ?3, 1, 1, ?4, ?5, ?6, ?7)",
                            params![observation.owner, observation.repo, observation.state, observation.last_error, now, notification_pending, batch_id],
                        )?;
                        (1, batch_id)
                    }
                    Some((previous_state, was_active, previous_count, pending, previous_batch_id, last_seen_at)) => {
                        if last_seen_at > now {
                            continue;
                        }
                        let changed = !was_active || previous_state != observation.state;
                        let occurrence_count = if changed {
                            previous_count.saturating_add(1)
                        } else {
                            previous_count
                        };
                        let occurrence_batch_id = if changed {
                            audit_batch_id.clone()
                        } else if pending {
                            previous_batch_id
                                .unwrap_or_else(|| audit_batch_id.clone())
                        } else {
                            previous_batch_id.unwrap_or_else(|| audit_batch_id.clone())
                        };
                        let pending_occurrence_exists = tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 AND notification_pending = 1)",
                            params![observation.owner, observation.repo, observation.state, occurrence_count],
                            |row| row.get::<_, i64>(0),
                        )? != 0;
                        if changed || (pending && !pending_occurrence_exists) {
                            insert_occurrence(
                                &tx,
                                &observation.owner,
                                &observation.repo,
                                &observation.state,
                                observation.last_error.as_deref(),
                                occurrence_count,
                                &occurrence_batch_id,
                                notification_pending,
                                source_job_id.as_deref(),
                                &source_status,
                                &now,
                            )?;
                        }
                        tx.execute(
                            "UPDATE notification_anomaly_states SET state = ?3, active = 1, occurrence_count = ?4, last_error = ?5, last_seen_at = ?6, notification_pending = EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1), notification_batch_id = (SELECT batch_id FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1 ORDER BY created_at DESC, id DESC LIMIT 1) WHERE owner = ?1 AND repo = ?2 AND last_seen_at <= ?6",
                            params![
                                observation.owner,
                                observation.repo,
                                observation.state,
                                occurrence_count,
                                observation.last_error,
                                now,
                            ],
                        )?;
                        (occurrence_count, occurrence_batch_id)
                    }
                };
                let _ = (occurrence_count, occurrence_batch_id);
            }

            for key in &requested_keys {
                let Some((owner, repo)) = key.split_once('/') else {
                    continue;
                };
                if !observations
                    .iter()
                    .any(|item| item.owner == owner && item.repo == repo)
                {
                    if let Some((state, occurrence_count, last_error, pending, batch_id)) = tx
                        .query_row(
                            "SELECT state, occurrence_count, last_error, notification_pending, notification_batch_id FROM notification_anomaly_states WHERE owner = ?1 AND repo = ?2",
                            params![owner, repo],
                            |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, i64>(1)?,
                                    row.get::<_, Option<String>>(2)?,
                                    row.get::<_, i64>(3)? != 0,
                                    row.get::<_, Option<String>>(4)?,
                                ))
                            },
                        )
                        .optional()?
                    {
                        let pending_occurrence_exists = tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND occurrence_count = ?3 AND notification_pending = 1)",
                            params![owner, repo, occurrence_count],
                            |row| row.get::<_, i64>(0),
                        )? != 0;
                        if pending && !pending_occurrence_exists {
                            let batch_id = batch_id.unwrap_or_else(|| {
                                audit_batch_id.clone()
                            });
                            insert_occurrence(
                                &tx,
                                owner,
                                repo,
                                &state,
                                last_error.as_deref(),
                                occurrence_count,
                                &batch_id,
                                notification_pending,
                                source_job_id.as_deref(),
                                &source_status,
                                &now,
                            )?;
                        }
                    }
                    tx.execute(
                        "UPDATE notification_anomaly_states SET active = 0, last_seen_at = ?3, notification_pending = EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1), notification_batch_id = CASE WHEN EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1) THEN notification_batch_id ELSE NULL END WHERE owner = ?1 AND repo = ?2 AND last_seen_at <= ?3",
                        params![owner, repo, now],
                    )?;
                }
            }

            let mut pending = Vec::new();
            for key in requested_keys {
                let Some((owner, repo)) = key.split_once('/') else {
                    continue;
                };
                let mut stmt = tx.prepare(
                    "SELECT state, last_error, occurrence_count FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1 ORDER BY created_at, id",
                )?;
                let rows = stmt.query_map(params![owner, repo], |row| {
                    Ok(NotificationAnomalyObservation {
                        owner: owner.to_string(),
                        repo: repo.to_string(),
                        state: row.get(0)?,
                        last_error: row.get(1)?,
                        occurrence_count: row.get(2)?,
                    })
                })?;
                pending.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
            }

            tx.commit()?;
            Ok((notification_enabled, pending))
        })
        .await
        .context("reconcile notification anomaly states")
    }

    #[allow(dead_code)]
    pub(crate) async fn mark_notification_anomaly_states_notified(
        &self,
        items: &[(String, String, String, i64)],
    ) -> anyhow::Result<()> {
        let items = items.to_vec();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (owner, repo, state, occurrence_count) in items {
                tx.execute(
                    "UPDATE notification_anomaly_occurrences SET notification_pending = 0, sent_channels_json = '[]', delivery_claim_token = NULL, delivery_claim_expires_at = NULL WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4",
                    params![owner, repo, state, occurrence_count],
                )?;
                tx.execute(
                    "UPDATE notification_anomaly_states SET notification_pending = EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1), notification_batch_id = NULL, notification_sent_channels_json = '[]' WHERE owner = ?1 AND repo = ?2",
                    params![owner, repo],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .context("mark notification anomaly states notified")
    }

    pub(crate) async fn mark_notification_anomaly_occurrences_item_persisted(
        &self,
        items: &[(String, String, String, i64)],
        notification_item_id: &str,
    ) -> anyhow::Result<()> {
        let items = items.to_vec();
        let notification_item_id = notification_item_id.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (owner, repo, state, occurrence_count) in items {
                tx.execute(
                    "UPDATE notification_anomaly_occurrences SET notification_item_id = ?5 WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 AND notification_pending = 1",
                    params![owner, repo, state, occurrence_count, notification_item_id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .context("mark notification anomaly occurrence item persisted")
    }

    #[allow(dead_code)]
    pub(crate) async fn list_pending_notification_anomaly_occurrences(
        &self,
        items: &[(String, String, String, i64)],
    ) -> anyhow::Result<Vec<NotificationAnomalyOccurrence>> {
        let items = items.to_vec();
        self.call(move |conn| {
            let mut result = Vec::new();
            for (owner, repo, state, occurrence_count) in items {
                let mut stmt = conn.prepare(
                    "SELECT state, last_error, occurrence_count, batch_id, notification_item_id, sent_channels_json, source_job_id, source_status FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 AND notification_pending = 1 ORDER BY created_at, id",
                )?;
                let rows = stmt.query_map(
                    params![owner, repo, state, occurrence_count],
                    |row| {
                        let raw_channels: String = row.get(5)?;
                        Ok(NotificationAnomalyOccurrence {
                            observation: NotificationAnomalyObservation {
                                owner: owner.clone(),
                                repo: repo.clone(),
                                state: row.get(0)?,
                                last_error: row.get(1)?,
                                occurrence_count: row.get(2)?,
                            },
                            batch_id: row.get(3)?,
                            notification_item_id: row.get(4)?,
                            sent_channels: serde_json::from_str(&raw_channels).unwrap_or_default(),
                            source_job_id: row.get(6)?,
                            source_status: row.get(7)?,
                        })
                    },
                )?;
                result.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
            }
            Ok(result)
        })
        .await
        .context("list pending notification anomaly occurrences")
    }

    pub(crate) async fn list_all_pending_notification_anomaly_occurrences(
        &self,
    ) -> anyhow::Result<Vec<NotificationAnomalyOccurrence>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT owner, repo, state, last_error, occurrence_count, batch_id, notification_item_id, sent_channels_json, source_job_id, source_status FROM notification_anomaly_occurrences WHERE notification_pending = 1 ORDER BY created_at, id LIMIT 256",
            )?;
            let rows = stmt.query_map([], |row| {
                let raw_channels: String = row.get(7)?;
                Ok(NotificationAnomalyOccurrence {
                    observation: NotificationAnomalyObservation {
                        owner: row.get(0)?,
                        repo: row.get(1)?,
                        state: row.get(2)?,
                        last_error: row.get(3)?,
                        occurrence_count: row.get(4)?,
                    },
                    batch_id: row.get(5)?,
                    notification_item_id: row.get(6)?,
                    sent_channels: serde_json::from_str(&raw_channels).unwrap_or_default(),
                    source_job_id: row.get(8)?,
                    source_status: row.get(9)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
        .context("list all pending notification anomaly occurrences")
    }

    pub(crate) async fn list_pending_notification_anomaly_occurrences_for_batch(
        &self,
        batch_id: &str,
    ) -> anyhow::Result<Vec<NotificationAnomalyOccurrence>> {
        let batch_id = batch_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT owner, repo, state, last_error, occurrence_count, batch_id, notification_item_id, sent_channels_json, source_job_id, source_status FROM notification_anomaly_occurrences WHERE batch_id = ?1 AND notification_pending = 1 ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map(params![batch_id], |row| {
                let raw_channels: String = row.get(7)?;
                Ok(NotificationAnomalyOccurrence {
                    observation: NotificationAnomalyObservation {
                        owner: row.get(0)?,
                        repo: row.get(1)?,
                        state: row.get(2)?,
                        last_error: row.get(3)?,
                        occurrence_count: row.get(4)?,
                    },
                    batch_id: row.get(5)?,
                    notification_item_id: row.get(6)?,
                    sent_channels: serde_json::from_str(&raw_channels).unwrap_or_default(),
                    source_job_id: row.get(8)?,
                    source_status: row.get(9)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
        .context("list pending notification anomaly occurrences for batch")
    }

    pub(crate) async fn try_claim_notification_anomaly_batch(
        &self,
        batch_id: &str,
        claim_token: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        let batch_id = batch_id.to_string();
        let claim_token = claim_token.to_string();
        let now = now.to_string();
        let claim_expires_at = anomaly_delivery_claim_expiry(&now)?;
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let total: i64 = tx.query_row(
                "SELECT COUNT(*) FROM notification_anomaly_occurrences WHERE batch_id = ?1 AND notification_pending = 1",
                params![batch_id.as_str()],
                |row| row.get(0),
            )?;
            if total == 0 {
                return Ok(false);
            }
            let available: i64 = tx.query_row(
                "SELECT COUNT(*) FROM notification_anomaly_occurrences WHERE batch_id = ?1 AND notification_pending = 1 AND (delivery_claim_token IS NULL OR delivery_claim_expires_at <= ?2)",
                params![batch_id.as_str(), now.as_str()],
                |row| row.get(0),
            )?;
            if available != total {
                return Ok(false);
            }
            let changed = tx.execute(
                "UPDATE notification_anomaly_occurrences SET delivery_claim_token = ?2, delivery_claim_expires_at = ?3 WHERE batch_id = ?1 AND notification_pending = 1",
                params![batch_id.as_str(), claim_token.as_str(), claim_expires_at.as_str()],
            )?;
            tx.commit()?;
            Ok(changed as i64 == total)
        })
        .await
        .context("claim notification anomaly batch")
    }

    #[allow(dead_code)]
    pub(crate) async fn list_notification_anomaly_sent_channels(
        &self,
        items: &[(String, String, String, i64)],
    ) -> anyhow::Result<HashMap<String, Vec<String>>> {
        let items = items.to_vec();
        self.call(move |conn| {
            let mut result = HashMap::new();
            for (owner, repo, state, occurrence_count) in items {
                let Some(raw) = conn
                    .query_row(
                        "SELECT sent_channels_json FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 ORDER BY created_at DESC, id DESC LIMIT 1",
                        params![owner, repo, state, occurrence_count],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                else {
                    continue;
                };
                result.insert(
                    format!("{owner}/{repo}"),
                    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default(),
                );
            }
            Ok(result)
        })
        .await
        .context("list notification anomaly sent channels")
    }

    #[allow(dead_code)]
    pub(crate) async fn record_notification_anomaly_delivery(
        &self,
        items: &[(String, String, String, i64)],
        successful_channels: &[String],
        complete: bool,
    ) -> anyhow::Result<()> {
        self.record_notification_anomaly_delivery_with_claim(
            items,
            successful_channels,
            complete,
            None,
        )
        .await
    }

    pub(crate) async fn record_notification_anomaly_delivery_with_claim(
        &self,
        items: &[(String, String, String, i64)],
        successful_channels: &[String],
        complete: bool,
        claim_token: Option<&str>,
    ) -> anyhow::Result<()> {
        let items = items.to_vec();
        let successful_channels = successful_channels.to_vec();
        let claim_token = claim_token.map(ToString::to_string);
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (owner, repo, state, occurrence_count) in items {
                let existing = tx
                    .query_row(
                        "SELECT sent_channels_json FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 AND notification_pending = 1 AND (?5 IS NULL OR delivery_claim_token = ?5) ORDER BY created_at DESC, id DESC LIMIT 1",
                        params![owner, repo, state, occurrence_count, claim_token.as_deref()],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                let Some(existing) = existing else {
                    continue;
                };
                let mut channels = serde_json::from_str::<BTreeSet<String>>(&existing)
                    .ok()
                    .unwrap_or_default();
                channels.extend(successful_channels.iter().cloned());
                if complete {
                    tx.execute(
                        "UPDATE notification_anomaly_occurrences SET notification_pending = 0, sent_channels_json = '[]', delivery_claim_token = NULL, delivery_claim_expires_at = NULL WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 AND notification_pending = 1 AND (?5 IS NULL OR delivery_claim_token = ?5)",
                        params![owner, repo, state, occurrence_count, claim_token.as_deref()],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE notification_anomaly_occurrences SET sent_channels_json = ?5, delivery_claim_token = NULL, delivery_claim_expires_at = NULL WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 AND notification_pending = 1 AND (?6 IS NULL OR delivery_claim_token = ?6)",
                        params![
                            owner,
                            repo,
                            state,
                            occurrence_count,
                            serde_json::to_string(&channels)?,
                            claim_token.as_deref(),
                        ],
                    )?;
                }
                tx.execute(
                    "UPDATE notification_anomaly_states SET notification_pending = EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1), notification_batch_id = CASE WHEN EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1) THEN (SELECT batch_id FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1 ORDER BY created_at DESC, id DESC LIMIT 1) ELSE NULL END, notification_sent_channels_json = '[]' WHERE owner = ?1 AND repo = ?2",
                    params![owner, repo],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .context("record notification anomaly delivery")
    }

    #[allow(dead_code)]
    pub(crate) async fn list_notification_anomaly_batch_ids(
        &self,
        items: &[(String, String, String, i64)],
    ) -> anyhow::Result<HashMap<String, String>> {
        let keys = items.to_vec();
        self.call(move |conn| {
            let mut result = HashMap::new();
            let mut stmt = conn.prepare(
                "SELECT owner, repo, batch_id FROM notification_anomaly_occurrences WHERE notification_pending = 1 ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (owner, repo, batch_id) = row?;
                if keys
                    .iter()
                    .any(|(key_owner, key_repo, _, _)| key_owner == &owner && key_repo == &repo)
                {
                    result.insert(format!("{owner}/{repo}"), batch_id);
                }
            }
            Ok(result)
        })
        .await
        .context("list notification anomaly batch ids")
    }
}
