use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateFile {
    pub schema_version: u32,
    pub op_id: String,
    pub state: String, // idle|running|succeeded|failed|rolled_back
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<RequestParams>,
    pub target: TargetRef,
    pub previous: PreviousRef,
    pub started_at: String,
    pub updated_at: String,
    pub progress: Progress,
    pub logs: Vec<LogLine>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestParams {
    pub mode: String,
    pub rollback_on_failure: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRef {
    pub tag: String,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousRef {
    pub tag: String,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub step: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub ts: String,
    pub level: String,
    pub msg: String,
}

impl StateFile {
    pub fn idle(now: &str) -> Self {
        Self {
            schema_version: 1,
            op_id: String::new(),
            state: "idle".to_string(),
            request: None,
            target: TargetRef {
                tag: "latest".to_string(),
                digest: None,
            },
            previous: PreviousRef {
                tag: "unknown".to_string(),
                digest: None,
            },
            started_at: now.to_string(),
            updated_at: now.to_string(),
            progress: Progress {
                step: "done".to_string(),
                message: "idle".to_string(),
            },
            logs: Vec::new(),
        }
    }
}

pub async fn load_or_idle(path: &Path) -> anyhow::Result<StateFile> {
    let now = now_rfc3339()?;
    let data = match tokio::fs::read(path).await {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(StateFile::idle(&now)),
        Err(e) => return Err(e.into()),
    };

    let parsed = serde_json::from_slice::<StateFile>(&data)
        .map_err(|e| anyhow::anyhow!("failed to parse state file {}: {e}", path.display()))?;
    Ok(parsed)
}

pub async fn store_atomic(path: &Path, state: &StateFile) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let data = serde_json::to_vec_pretty(state)?;
    let tmp = tmp_path(path);
    tokio::fs::write(&tmp, data).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut out = path.to_path_buf();
    let name = out
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("self-upgrade.json");
    out.set_file_name(format!(".{name}.tmp"));
    out
}

pub fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn atomic_store_and_load_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("dockrev-supervisor-test-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("state.json");

        let now = now_rfc3339().unwrap();
        let mut st = StateFile::idle(&now);
        st.op_id = "sup_test".to_string();
        st.state = "running".to_string();
        st.progress = Progress {
            step: "precheck".to_string(),
            message: "ok".to_string(),
        };
        st.logs.push(LogLine {
            ts: now.clone(),
            level: "INFO".to_string(),
            msg: "hello".to_string(),
        });

        store_atomic(&path, &st).await.unwrap();
        let loaded = load_or_idle(&path).await.unwrap();
        assert_eq!(loaded.op_id, "sup_test");
        assert_eq!(loaded.state, "running");
        assert_eq!(loaded.progress.step, "precheck");
        assert_eq!(loaded.logs.len(), 1);

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
