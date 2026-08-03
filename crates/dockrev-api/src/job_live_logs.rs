use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use std::sync::Mutex;

use tokio::sync::broadcast;

const JOB_LIVE_LOG_BROADCAST_CAPACITY: usize = 512;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobLiveLog {
    pub(crate) ts: String,
    pub(crate) stream: &'static str,
    pub(crate) msg: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobLiveCommandComplete {
    pub(crate) had_output: bool,
    pub(crate) summary_persisted: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum JobLiveEvent {
    Log(JobLiveLog),
    CommandComplete(JobLiveCommandComplete),
}

pub(crate) struct JobLiveLogSubscription {
    receiver: broadcast::Receiver<JobLiveEvent>,
}

pub(crate) struct JobLiveLogCleanupGuard {
    hub: Arc<JobLiveLogHub>,
    job_id: String,
}

impl JobLiveLogCleanupGuard {
    pub(crate) fn new(hub: Arc<JobLiveLogHub>, job_id: impl Into<String>) -> Self {
        Self {
            hub,
            job_id: job_id.into(),
        }
    }
}

impl Drop for JobLiveLogCleanupGuard {
    fn drop(&mut self) {
        self.hub.close(&self.job_id);
    }
}

impl JobLiveLogSubscription {
    pub(crate) async fn recv(&mut self) -> Result<JobLiveEvent, broadcast::error::RecvError> {
        self.receiver.recv().await
    }

    pub(crate) fn try_recv(&mut self) -> Result<JobLiveEvent, broadcast::error::TryRecvError> {
        self.receiver.try_recv()
    }
}

#[derive(Clone, Default)]
pub(crate) struct JobLiveLogHub {
    entries: Arc<Mutex<BTreeMap<String, broadcast::Sender<JobLiveEvent>>>>,
}

impl JobLiveLogHub {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn subscribe(&self, job_id: &str) -> JobLiveLogSubscription {
        let sender = {
            let mut entries = self.entries.lock().expect("job live log hub lock poisoned");
            entries
                .entry(job_id.to_string())
                .or_insert_with(|| broadcast::channel(JOB_LIVE_LOG_BROADCAST_CAPACITY).0)
                .clone()
        };
        JobLiveLogSubscription {
            receiver: sender.subscribe(),
        }
    }

    pub(crate) fn publish_log(&self, job_id: &str, log: JobLiveLog) {
        if let Ok(entries) = self.entries.lock()
            && let Some(sender) = entries.get(job_id)
        {
            let _ = sender.send(JobLiveEvent::Log(log));
        }
    }

    pub(crate) fn publish_command_complete(
        &self,
        job_id: &str,
        had_output: bool,
        summary_persisted: bool,
    ) {
        if let Ok(entries) = self.entries.lock()
            && let Some(sender) = entries.get(job_id)
        {
            let _ = sender.send(JobLiveEvent::CommandComplete(JobLiveCommandComplete {
                had_output,
                summary_persisted,
            }));
        }
    }

    pub(crate) fn close(&self, job_id: &str) {
        self.entries
            .lock()
            .expect("job live log hub lock poisoned")
            .remove(job_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn close_releases_live_entry_without_replay_buffer() {
        let hub = JobLiveLogHub::new();
        let mut subscription = hub.subscribe("job-1").await;
        hub.publish_log(
            "job-1",
            JobLiveLog {
                ts: "2026-08-03T00:00:00Z".to_string(),
                stream: "stdout",
                msg: "line".to_string(),
            },
        );
        assert!(matches!(
            subscription.recv().await,
            Ok(JobLiveEvent::Log(_))
        ));
        hub.close("job-1");
        assert!(matches!(
            subscription.recv().await,
            Err(broadcast::error::RecvError::Closed)
        ));

        let mut fresh_subscription = hub.subscribe("job-1").await;
        assert!(matches!(
            fresh_subscription.receiver.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }
}
