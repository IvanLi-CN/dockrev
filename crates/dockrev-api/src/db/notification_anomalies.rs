use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::Db;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationAnomalyObservation {
    pub owner: String,
    pub repo: String,
    pub state: String,
    pub last_error: Option<String>,
    pub occurrence_count: i64,
}

#[cfg(test)]
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

impl Db {
    pub(crate) async fn reconcile_notification_anomaly_states(
        &self,
        scope_keys: &[String],
        observations: &[NotificationAnomalyObservation],
        now: &str,
    ) -> anyhow::Result<Vec<NotificationAnomalyObservation>> {
        let scope_keys = scope_keys.to_vec();
        let observations = observations.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut newly_active = Vec::new();

            for observation in &observations {
                let previous = tx
                    .query_row(
                        "SELECT state, active, occurrence_count, notification_pending, last_seen_at FROM notification_anomaly_states WHERE owner = ?1 AND repo = ?2",
                        params![observation.owner, observation.repo],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, i64>(1)? != 0,
                                row.get::<_, i64>(2)?,
                                row.get::<_, i64>(3)? != 0,
                                row.get::<_, String>(4)?,
                            ))
                        },
                    )
                    .optional()?;

                match previous {
                    None => {
                        tx.execute(
                            "INSERT INTO notification_anomaly_states (owner, repo, state, active, occurrence_count, last_error, last_seen_at, notification_pending) VALUES (?1, ?2, ?3, 1, 1, ?4, ?5, 1)",
                            params![observation.owner, observation.repo, observation.state, observation.last_error, now],
                        )?;
                        let mut current = observation.clone();
                        current.occurrence_count = 1;
                        newly_active.push(current);
                    }
                    Some((previous_state, was_active, previous_occurrence_count, pending, last_seen_at)) => {
                        if last_seen_at > now {
                            continue;
                        }
                        let changed = !was_active || previous_state != observation.state;
                        let occurrence_count = if changed {
                            previous_occurrence_count.saturating_add(1)
                        } else {
                            previous_occurrence_count
                        };
                        let notification_pending = changed || pending;
                        tx.execute(
                            "UPDATE notification_anomaly_states SET state = ?3, active = 1, occurrence_count = ?4, last_error = ?5, last_seen_at = ?6, notification_pending = ?7 WHERE owner = ?1 AND repo = ?2 AND last_seen_at <= ?6",
                            params![
                                observation.owner,
                                observation.repo,
                                observation.state,
                                occurrence_count,
                                observation.last_error,
                                now,
                                notification_pending,
                            ],
                        )?;
                        if changed || pending {
                            let mut current = observation.clone();
                            current.occurrence_count = occurrence_count;
                            newly_active.push(current);
                        }
                    }
                }
            }

            for key in scope_keys {
                let Some((owner, repo)) = key.split_once('/') else {
                    continue;
                };
                if !observations
                    .iter()
                    .any(|item| item.owner == owner && item.repo == repo)
                {
                    tx.execute(
                        "UPDATE notification_anomaly_states SET active = 0, last_seen_at = ?3 WHERE owner = ?1 AND repo = ?2 AND last_seen_at <= ?3",
                        params![owner, repo, now],
                    )?;
                }
            }

            tx.commit()?;
            Ok(newly_active)
        })
        .await
        .context("reconcile notification anomaly states")
    }

    pub(crate) async fn mark_notification_anomaly_states_notified(
        &self,
        items: &[(String, String, String, i64)],
    ) -> anyhow::Result<()> {
        let items = items.to_vec();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (owner, repo, state, occurrence_count) in items {
                tx.execute(
                    "UPDATE notification_anomaly_states SET notification_pending = 0 WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4",
                    params![owner, repo, state, occurrence_count],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .context("mark notification anomaly states notified")
    }
}
