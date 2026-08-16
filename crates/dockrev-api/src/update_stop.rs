use std::{collections::HashMap, sync::Mutex};

#[derive(Default)]
pub struct UpdateStopHub {
    senders: Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>,
}

impl UpdateStopHub {
    pub fn subscribe(&self, job_id: &str) -> tokio::sync::watch::Receiver<bool> {
        let mut senders = self.senders.lock().expect("update stop hub lock poisoned");
        senders
            .entry(job_id.to_string())
            .or_insert_with(|| tokio::sync::watch::channel(false).0)
            .subscribe()
    }

    pub fn request(&self, job_id: &str) {
        let mut senders = self.senders.lock().expect("update stop hub lock poisoned");
        let sender = senders
            .entry(job_id.to_string())
            .or_insert_with(|| tokio::sync::watch::channel(false).0);
        let _ = sender.send(true);
    }

    pub fn remove(&self, job_id: &str) {
        self.senders
            .lock()
            .expect("update stop hub lock poisoned")
            .remove(job_id);
    }
}

#[derive(Debug)]
pub struct UpdateStopRequested;

impl std::fmt::Display for UpdateStopRequested {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("update stop requested")
    }
}

impl std::error::Error for UpdateStopRequested {}

pub fn requested_error() -> anyhow::Error {
    anyhow::Error::new(UpdateStopRequested)
}

pub fn is_requested(error: &anyhow::Error) -> bool {
    error.downcast_ref::<UpdateStopRequested>().is_some()
}
