use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::Db;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationAnomalyObservation {
    pub owner: String,
    pub repo: String,
    pub state: String,
    pub last_error: Option<String>,
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
        }
    }

    #[tokio::test]
    async fn anomaly_state_only_emits_new_and_changed_active_states() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let scope = vec!["acme/api".to_string()];

        assert_eq!(
            db.reconcile_notification_anomaly_states(
                &scope,
                &[observation("missing")],
                "2026-09-26T00:00:00Z",
            )
            .await
            .unwrap()
            .len(),
            1
        );
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
        assert_eq!(
            db.reconcile_notification_anomaly_states(
                &scope,
                &[observation("conflict")],
                "2026-09-26T00:02:00Z",
            )
            .await
            .unwrap()
            .len(),
            1
        );
        assert!(
            db.reconcile_notification_anomaly_states(&scope, &[], "2026-09-26T00:03:00Z",)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.reconcile_notification_anomaly_states(
                &scope,
                &[observation("conflict")],
                "2026-09-26T00:04:00Z",
            )
            .await
            .unwrap()
            .len(),
            1
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
                        "SELECT state, active FROM notification_anomaly_states WHERE owner = ?1 AND repo = ?2",
                        params![observation.owner, observation.repo],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0)),
                    )
                    .optional()?;

                match previous {
                    None => {
                        tx.execute(
                            "INSERT INTO notification_anomaly_states (owner, repo, state, active, occurrence_count, last_error, last_seen_at) VALUES (?1, ?2, ?3, 1, 1, ?4, ?5)",
                            params![observation.owner, observation.repo, observation.state, observation.last_error, now],
                        )?;
                        newly_active.push(observation.clone());
                    }
                    Some((previous_state, was_active)) => {
                        let changed = !was_active || previous_state != observation.state;
                        tx.execute(
                            "UPDATE notification_anomaly_states SET state = ?3, active = 1, occurrence_count = occurrence_count + CASE WHEN ?4 THEN 1 ELSE 0 END, last_error = ?5, last_seen_at = ?6 WHERE owner = ?1 AND repo = ?2",
                            params![
                                observation.owner,
                                observation.repo,
                                observation.state,
                                changed,
                                observation.last_error,
                                now,
                            ],
                        )?;
                        if changed {
                            newly_active.push(observation.clone());
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
                        "UPDATE notification_anomaly_states SET active = 0, last_seen_at = ?3 WHERE owner = ?1 AND repo = ?2",
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
}
