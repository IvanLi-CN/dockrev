use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use std::sync::Mutex;

use tokio::sync::broadcast;

const JOB_LIVE_LOG_BROADCAST_CAPACITY: usize = 512;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobLiveTerminalSegment {
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bg: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) dim: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) underline: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobLiveTerminalLine {
    pub(crate) segments: Vec<JobLiveTerminalSegment>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobLiveTerminal {
    pub(crate) ts: String,
    pub(crate) command_seq: u64,
    pub(crate) lines: Vec<JobLiveTerminalLine>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobLiveCommandComplete {
    pub(crate) command_seq: u64,
    pub(crate) had_output: bool,
    pub(crate) summary_persisted: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum JobLiveEvent {
    Terminal(JobLiveTerminal),
    CommandComplete(JobLiveCommandComplete),
}

pub(crate) struct JobLiveLogSubscription {
    receiver: broadcast::Receiver<JobLiveEvent>,
    hub: Arc<JobLiveLogHub>,
    job_id: String,
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

impl Drop for JobLiveLogSubscription {
    fn drop(&mut self) {
        self.hub.remove_if_unsubscribed(&self.job_id);
    }
}

#[derive(Clone)]
struct JobLiveEntry {
    sender: broadcast::Sender<JobLiveEvent>,
    next_command_seq: u64,
}

#[derive(Clone, Default)]
pub(crate) struct JobLiveLogHub {
    entries: Arc<Mutex<BTreeMap<String, JobLiveEntry>>>,
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
                .or_insert_with(|| JobLiveEntry {
                    sender: broadcast::channel(JOB_LIVE_LOG_BROADCAST_CAPACITY).0,
                    next_command_seq: 0,
                })
                .sender
                .clone()
        };
        JobLiveLogSubscription {
            receiver: sender.subscribe(),
            hub: Arc::new(self.clone()),
            job_id: job_id.to_string(),
        }
    }

    pub(crate) fn begin_command(&self, job_id: &str) -> u64 {
        let mut entries = self.entries.lock().expect("job live log hub lock poisoned");
        let entry = entries
            .entry(job_id.to_string())
            .or_insert_with(|| JobLiveEntry {
                sender: broadcast::channel(JOB_LIVE_LOG_BROADCAST_CAPACITY).0,
                next_command_seq: 0,
            });
        entry.next_command_seq = entry.next_command_seq.saturating_add(1);
        entry.next_command_seq
    }

    pub(crate) fn publish_terminal(&self, job_id: &str, terminal: JobLiveTerminal) {
        if let Ok(entries) = self.entries.lock()
            && let Some(entry) = entries.get(job_id)
        {
            let _ = entry.sender.send(JobLiveEvent::Terminal(terminal));
        }
    }

    pub(crate) fn publish_command_complete(
        &self,
        job_id: &str,
        command_seq: u64,
        had_output: bool,
        summary_persisted: bool,
    ) {
        if let Ok(entries) = self.entries.lock()
            && let Some(entry) = entries.get(job_id)
        {
            let _ = entry
                .sender
                .send(JobLiveEvent::CommandComplete(JobLiveCommandComplete {
                    command_seq,
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

    fn remove_if_unsubscribed(&self, job_id: &str) {
        let mut entries = self.entries.lock().expect("job live log hub lock poisoned");
        if entries
            .get(job_id)
            .is_some_and(|entry| entry.sender.receiver_count() <= 1)
        {
            entries.remove(job_id);
        }
    }
}

pub(crate) fn terminal_snapshot(
    parser: &vt100::Parser,
    ts: String,
    command_seq: u64,
) -> JobLiveTerminal {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let mut lines = Vec::with_capacity(rows as usize);

    for row in 0..rows {
        let last_col = (0..cols).rev().find(|&col| {
            screen
                .cell(row, col)
                .is_some_and(|cell| cell.has_contents())
        });
        let Some(last_col) = last_col else {
            lines.push(JobLiveTerminalLine {
                segments: Vec::new(),
            });
            continue;
        };

        let mut segments: Vec<JobLiveTerminalSegment> = Vec::new();
        for col in 0..=last_col {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let text = if cell.has_contents() {
                cell.contents().to_string()
            } else {
                " ".to_string()
            };
            if text.is_empty() {
                continue;
            }
            let segment = JobLiveTerminalSegment {
                text,
                fg: color_to_css(cell.fgcolor()),
                bg: color_to_css(cell.bgcolor()),
                bold: cell.bold(),
                dim: cell.dim(),
                underline: cell.underline(),
            };
            if let Some(previous) = segments.last_mut()
                && previous.fg == segment.fg
                && previous.bg == segment.bg
                && previous.bold == segment.bold
                && previous.dim == segment.dim
                && previous.underline == segment.underline
            {
                previous.text.push_str(&segment.text);
            } else {
                segments.push(segment);
            }
        }
        lines.push(JobLiveTerminalLine { segments });
    }

    while lines.last().is_some_and(|line| line.segments.is_empty()) {
        lines.pop();
    }

    JobLiveTerminal {
        ts,
        command_seq,
        lines,
    }
}

fn color_to_css(color: vt100::Color) -> Option<String> {
    match color {
        vt100::Color::Default => None,
        vt100::Color::Rgb(r, g, b) => Some(format!("rgb({r}, {g}, {b})")),
        vt100::Color::Idx(index) => {
            let (r, g, b) = xterm_color(index);
            Some(format!("rgb({r}, {g}, {b})"))
        }
    }
}

fn xterm_color(index: u8) -> (u8, u8, u8) {
    const BASIC: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    if index < 16 {
        return BASIC[index as usize];
    }
    if index < 232 {
        let value = index - 16;
        let component = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
        return (
            component(value / 36),
            component((value / 6) % 6),
            component(value % 6),
        );
    }
    let gray = 8 + (index - 232) * 10;
    (gray, gray, gray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn close_releases_live_entry_without_replay_buffer() {
        let hub = JobLiveLogHub::new();
        let mut subscription = hub.subscribe("job-1").await;
        let sequence = hub.begin_command("job-1");
        hub.publish_terminal(
            "job-1",
            JobLiveTerminal {
                ts: "2026-08-04T00:00:00Z".to_string(),
                command_seq: sequence,
                lines: vec![JobLiveTerminalLine {
                    segments: vec![JobLiveTerminalSegment {
                        text: "line".to_string(),
                        fg: None,
                        bg: None,
                        bold: false,
                        dim: false,
                        underline: false,
                    }],
                }],
            },
        );
        assert!(matches!(
            subscription.recv().await,
            Ok(JobLiveEvent::Terminal(_))
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

    #[tokio::test]
    async fn dropping_last_subscription_releases_live_entry() {
        let hub = JobLiveLogHub::new();
        let subscription = hub.subscribe("job-1").await;
        drop(subscription);

        assert_eq!(hub.begin_command("job-1"), 1);
    }

    #[test]
    fn terminal_snapshot_preserves_vt100_styles_and_trims_blank_rows() {
        let mut parser = vt100::Parser::new(4, 20, 2000);
        parser.process(b"\x1b[1;31mred\x1b[0m plain\x1b[4m under");
        let snapshot = terminal_snapshot(&parser, "ts".to_string(), 1);
        assert_eq!(snapshot.lines.len(), 1);
        assert_eq!(snapshot.lines[0].segments.len(), 3);
        assert_eq!(snapshot.lines[0].segments[0].text, "red");
        assert_eq!(
            snapshot.lines[0].segments[0].fg.as_deref(),
            Some("rgb(205, 0, 0)")
        );
        assert!(snapshot.lines[0].segments[0].bold);
        assert!(snapshot.lines[0].segments[2].underline);
    }

    #[test]
    fn terminal_snapshot_applies_carriage_return_instead_of_stacking_progress() {
        let mut parser = vt100::Parser::new(4, 40, 2000);
        parser.process(b"Downloading 1MB\rDownloading 2MB");
        let snapshot = terminal_snapshot(&parser, "ts".to_string(), 1);
        let text = snapshot.lines[0]
            .segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<String>();
        assert_eq!(text, "Downloading 2MB");
    }
}
