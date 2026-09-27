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
    async fn pending_anomaly_batch_is_reused_when_audit_adds_another_repo() {
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
        assert_eq!(second_batch.get("acme/api"), second_batch.get("acme/web"));
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
    created_at: &str,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO notification_anomaly_occurrences (id, owner, repo, state, last_error, occurrence_count, batch_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            crate::ids::new_notification_id(),
            owner,
            repo,
            state,
            last_error,
            occurrence_count,
            batch_id,
            created_at,
        ],
    )?;
    Ok(())
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
            let reusable_batch_id = tx
                .query_row(
                    "SELECT batch_id FROM notification_anomaly_occurrences WHERE notification_pending = 1 ORDER BY created_at, id LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let mut new_batch_id = None;
            let mut changed_batch_id = None;
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
                        let batch_id = reusable_batch_id.clone().unwrap_or_else(|| {
                            new_batch_id
                                .get_or_insert_with(crate::ids::new_notification_id)
                                .clone()
                        });
                        insert_occurrence(
                            &tx,
                            &observation.owner,
                            &observation.repo,
                            &observation.state,
                            observation.last_error.as_deref(),
                            1,
                            &batch_id,
                            &now,
                        )?;
                        tx.execute(
                            "INSERT INTO notification_anomaly_states (owner, repo, state, active, occurrence_count, last_error, last_seen_at, notification_pending, notification_batch_id) VALUES (?1, ?2, ?3, 1, 1, ?4, ?5, 1, ?6)",
                            params![observation.owner, observation.repo, observation.state, observation.last_error, now, batch_id],
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
                            changed_batch_id
                                .get_or_insert_with(crate::ids::new_notification_id)
                                .clone()
                        } else if pending {
                            previous_batch_id
                                .or_else(|| reusable_batch_id.clone())
                                .unwrap_or_else(|| {
                                    new_batch_id
                                        .get_or_insert_with(crate::ids::new_notification_id)
                                        .clone()
                                })
                        } else {
                            previous_batch_id.unwrap_or_else(|| {
                                new_batch_id
                                    .get_or_insert_with(crate::ids::new_notification_id)
                                    .clone()
                            })
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
                                &now,
                            )?;
                        }
                        tx.execute(
                            "UPDATE notification_anomaly_states SET state = ?3, active = 1, occurrence_count = ?4, last_error = ?5, last_seen_at = ?6, notification_pending = EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1), notification_batch_id = ?7 WHERE owner = ?1 AND repo = ?2 AND last_seen_at <= ?6",
                            params![
                                observation.owner,
                                observation.repo,
                                observation.state,
                                occurrence_count,
                                observation.last_error,
                                now,
                                occurrence_batch_id,
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
                                new_batch_id
                                    .get_or_insert_with(crate::ids::new_notification_id)
                                    .clone()
                            });
                            insert_occurrence(
                                &tx,
                                owner,
                                repo,
                                &state,
                                last_error.as_deref(),
                                occurrence_count,
                                &batch_id,
                                &now,
                            )?;
                        }
                    }
                    tx.execute(
                        "UPDATE notification_anomaly_states SET active = 0, last_seen_at = ?3, notification_pending = EXISTS(SELECT 1 FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND notification_pending = 1) WHERE owner = ?1 AND repo = ?2 AND last_seen_at <= ?3",
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
            Ok(pending)
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
                    "UPDATE notification_anomaly_occurrences SET notification_pending = 0, sent_channels_json = '[]' WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4",
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

    pub(crate) async fn list_pending_notification_anomaly_occurrences(
        &self,
        items: &[(String, String, String, i64)],
    ) -> anyhow::Result<Vec<NotificationAnomalyOccurrence>> {
        let items = items.to_vec();
        self.call(move |conn| {
            let mut result = Vec::new();
            for (owner, repo, state, occurrence_count) in items {
                let mut stmt = conn.prepare(
                    "SELECT state, last_error, occurrence_count, batch_id, notification_item_id, sent_channels_json FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 AND notification_pending = 1 ORDER BY created_at, id",
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

    pub(crate) async fn record_notification_anomaly_delivery(
        &self,
        items: &[(String, String, String, i64)],
        successful_channels: &[String],
        complete: bool,
    ) -> anyhow::Result<()> {
        let items = items.to_vec();
        let successful_channels = successful_channels.to_vec();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (owner, repo, state, occurrence_count) in items {
                let existing = tx
                    .query_row(
                        "SELECT sent_channels_json FROM notification_anomaly_occurrences WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 ORDER BY created_at DESC, id DESC LIMIT 1",
                        params![owner, repo, state, occurrence_count],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                let mut channels = existing
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<BTreeSet<String>>(raw).ok())
                    .unwrap_or_default();
                channels.extend(successful_channels.iter().cloned());
                if complete {
                    tx.execute(
                        "UPDATE notification_anomaly_occurrences SET notification_pending = 0, sent_channels_json = '[]' WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4",
                        params![owner, repo, state, occurrence_count],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE notification_anomaly_occurrences SET sent_channels_json = ?5 WHERE owner = ?1 AND repo = ?2 AND state = ?3 AND occurrence_count = ?4 AND notification_pending = 1",
                        params![
                            owner,
                            repo,
                            state,
                            occurrence_count,
                            serde_json::to_string(&channels)?,
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
