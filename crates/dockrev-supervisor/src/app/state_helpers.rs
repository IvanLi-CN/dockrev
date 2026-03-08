use crate::state_store::{LogLine, Progress, StateFile, now_rfc3339, store_atomic};

use super::App;

pub(crate) const MAX_LOG_OPERATION_GROUPS: usize = 30;

#[derive(Clone, Debug)]
pub(crate) struct OperationLogsGroup {
    pub(crate) op_id: String,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
    pub(crate) logs: Vec<LogLine>,
}

pub(crate) fn normalize_digest(input: String) -> String {
    let t = input.trim().to_string();
    if t.starts_with("sha256:") {
        t
    } else {
        format!("sha256:{t}")
    }
}

fn log_boundary_starts_operation(msg: &str) -> bool {
    msg.contains("self-upgrade requested")
}

fn normalized_log_op_id(log: &LogLine) -> Option<String> {
    log.op_id
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn append_log_line(st: &mut StateFile, now: &str, level: &str, msg: impl Into<String>) {
    let op_id = st.op_id.trim();
    let op_id = if op_id.is_empty() {
        None
    } else {
        Some(op_id.to_string())
    };
    st.logs.push(LogLine {
        ts: now.to_string(),
        level: level.to_string(),
        msg: msg.into(),
        op_id,
    });
}

fn stable_legacy_group_id(log: &LogLine) -> String {
    let source = if !log.ts.trim().is_empty() {
        log.ts.as_str()
    } else if !log.msg.trim().is_empty() {
        log.msg.as_str()
    } else {
        "legacy"
    };
    let mut out = String::with_capacity(source.len());
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    format!("legacy-{out}")
}

pub(crate) fn build_operation_groups(logs: &[LogLine]) -> Vec<OperationLogsGroup> {
    let mut groups: Vec<OperationLogsGroup> = Vec::new();
    let mut legacy_active: Option<String> = None;

    for log in logs {
        let group_id = if let Some(op_id) = normalized_log_op_id(log) {
            legacy_active = None;
            op_id
        } else if log_boundary_starts_operation(&log.msg) {
            let id = stable_legacy_group_id(log);
            legacy_active = Some(id.clone());
            id
        } else if let Some(id) = legacy_active.clone() {
            id
        } else {
            let id = stable_legacy_group_id(log);
            legacy_active = Some(id.clone());
            id
        };

        let should_append = groups.last().map(|g| g.op_id.as_str()) == Some(group_id.as_str());
        if should_append {
            if let Some(last) = groups.last_mut() {
                last.updated_at = log.ts.clone();
                last.logs.push(log.clone());
            }
            continue;
        }

        groups.push(OperationLogsGroup {
            op_id: group_id,
            started_at: log.ts.clone(),
            updated_at: log.ts.clone(),
            logs: vec![log.clone()],
        });
    }

    groups
}

pub(crate) fn infer_operation_state(group: &OperationLogsGroup, st: &StateFile) -> String {
    if group.op_id == st.op_id
        && matches!(
            st.state.as_str(),
            "running" | "succeeded" | "failed" | "rolled_back"
        )
    {
        return st.state.clone();
    }

    // For historical groups, prefer the latest terminal marker in log order.
    for line in group.logs.iter().rev() {
        if line.msg.contains("rolled back") {
            return "rolled_back".to_string();
        }
        if line.level.eq_ignore_ascii_case("ERROR") {
            return "failed".to_string();
        }
        if line.msg.contains("dry-run done") || line.msg.contains("succeeded") {
            return "succeeded".to_string();
        }
    }

    "unknown".to_string()
}

pub(crate) fn retain_recent_operation_logs(st: &mut StateFile, max_groups: usize) {
    if st.logs.is_empty() {
        return;
    }
    let groups = build_operation_groups(&st.logs);
    if groups.len() <= max_groups {
        return;
    }
    let keep_from = groups.len().saturating_sub(max_groups);
    let mut kept: Vec<LogLine> = Vec::new();
    for group in groups.into_iter().skip(keep_from) {
        kept.extend(group.logs.into_iter());
    }
    st.logs = kept;
}

pub(crate) async fn mark_failed_if_running(app: &App, err: anyhow::Error) {
    let now = now_rfc3339()
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc().unix_timestamp().to_string());
    let mut rt = app.runtime.lock().await;
    if rt.state.state == "running" {
        let step = rt.state.progress.step.clone();
        rt.state.state = "failed".to_string();
        rt.state.progress = Progress {
            step,
            message: format!("failed: {err}"),
        };
        rt.state.updated_at = now.clone();
        append_log_line(&mut rt.state, &now, "ERROR", err.to_string());
    }
    retain_recent_operation_logs(&mut rt.state, MAX_LOG_OPERATION_GROUPS);
    rt.running_key = None;

    if let Err(e) = store_atomic(&app.cfg.state_path, &rt.state).await {
        tracing::error!(error = %e, "failed to persist supervisor state after background failure");
    }
}
