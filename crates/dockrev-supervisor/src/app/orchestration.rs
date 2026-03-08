use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{
    config::Config,
    docker_exec::{
        TargetRuntime, compose_up, docker_image_repo_digest, docker_image_semver_tag_ref_to_pull,
        docker_pull, resolve_target,
    },
    state_store::{Progress, StateFile, now_rfc3339, store_atomic},
};

use super::{
    App, StartKey,
    state_helpers::{MAX_LOG_OPERATION_GROUPS, append_log_line, retain_recent_operation_logs},
};

pub(crate) async fn run_operation(app: Arc<App>, key: StartKey) -> anyhow::Result<()> {
    let target = resolve_target(&app.cfg).await?;

    let image_ref = if let Some(d) = key.digest.as_deref() {
        format!("{}@{}", app.cfg.target_image_repo, d)
    } else {
        format!("{}:{}", app.cfg.target_image_repo, key.tag)
    };

    let current_digest = docker_image_repo_digest(
        &app.cfg,
        &target.current_image_id,
        &app.cfg.target_image_repo,
    )
    .await?;
    let previous_tag = if target.current_image_ref.trim().is_empty() {
        "unknown".to_string()
    } else {
        target.current_image_ref.clone()
    };

    update_state(&app, |st, now| {
        st.previous.tag = previous_tag;
        st.previous.digest = current_digest.clone();
        st.progress = Progress {
            step: "pull".to_string(),
            message: "pulling image".to_string(),
        };
        st.updated_at = now.to_string();
        append_log_line(st, now, "INFO", format!("pull {image_ref}"));
    })
    .await?;

    docker_pull(&app.cfg, &image_ref, Duration::from_secs(300)).await?;

    match docker_image_semver_tag_ref_to_pull(&app.cfg, &image_ref, &app.cfg.target_image_repo)
        .await
    {
        Ok(Some(tag_ref)) => {
            update_state(&app, |st, now| {
                append_log_line(
                    st,
                    now,
                    "INFO",
                    format!("best-effort pull semver tag {tag_ref}"),
                );
            })
            .await?;

            if let Err(e) = docker_pull(&app.cfg, &tag_ref, Duration::from_secs(300)).await {
                update_state(&app, |st, now| {
                    append_log_line(
                        st,
                        now,
                        "WARN",
                        format!("semver tag pull failed: {tag_ref}: {e}"),
                    );
                })
                .await?;
            }
        }
        Ok(None) => {}
        Err(e) => {
            update_state(&app, |st, now| {
                append_log_line(st, now, "WARN", format!("semver tag pull skipped: {e}"));
            })
            .await?;
        }
    }

    if key.mode == "dry-run" {
        update_state(&app, |st, now| {
            st.state = "succeeded".to_string();
            st.progress = Progress {
                step: "done".to_string(),
                message: "dry-run completed".to_string(),
            };
            st.updated_at = now.to_string();
            append_log_line(st, now, "INFO", "dry-run done");
        })
        .await?;
        clear_running(&app).await;
        return Ok(());
    }

    let override_path = override_file_path(&app.cfg.state_path)?;
    write_override(&override_path, &target.compose_service, &image_ref).await?;

    update_state(&app, |st, now| {
        st.progress = Progress {
            step: "apply".to_string(),
            message: "docker compose up".to_string(),
        };
        st.updated_at = now.to_string();
        append_log_line(st, now, "INFO", "compose up");
    })
    .await?;

    let apply_result =
        compose_up(&app.cfg, &target, &override_path, Duration::from_secs(600)).await;
    if let Err(e) = apply_result {
        return fail_and_maybe_rollback(app, target, key, current_digest, e).await;
    }

    update_state(&app, |st, now| {
        st.progress = Progress {
            step: "wait_healthy".to_string(),
            message: "waiting /api/health".to_string(),
        };
        st.updated_at = now.to_string();
    })
    .await?;

    let post_target = match wait_dockrev_health(&app.cfg, Duration::from_secs(180)).await {
        Ok(v) => v,
        Err(e) => return fail_and_maybe_rollback(app, target, key, current_digest, e).await,
    };

    update_state(&app, |st, now| {
        st.progress = Progress {
            step: "postcheck".to_string(),
            message: "fetching /api/version".to_string(),
        };
        st.updated_at = now.to_string();
    })
    .await?;

    let _ = fetch_dockrev_version(&post_target).await;

    update_state(&app, |st, now| {
        st.state = "succeeded".to_string();
        st.progress = Progress {
            step: "done".to_string(),
            message: "ok".to_string(),
        };
        st.updated_at = now.to_string();
        append_log_line(st, now, "INFO", "succeeded");
    })
    .await?;

    clear_running(&app).await;
    Ok(())
}

pub(crate) fn rollback_image_ref(
    target_image_repo: &str,
    previous: &crate::state_store::PreviousRef,
) -> anyhow::Result<String> {
    if let Some(d) = previous.digest.as_deref() {
        return Ok(format!("{target_image_repo}@{d}"));
    }

    let t = previous.tag.trim();
    if t.is_empty() || t == "unknown" {
        return Err(anyhow::anyhow!("no rollback target available"));
    }

    if t == target_image_repo
        || t.starts_with(&format!("{target_image_repo}:"))
        || t.starts_with(&format!("{target_image_repo}@"))
        || t.contains(['/', ':', '@'])
    {
        return Ok(t.to_string());
    }

    Ok(format!("{target_image_repo}:{t}"))
}

pub(crate) async fn run_rollback_only(
    app: Arc<App>,
    previous: crate::state_store::PreviousRef,
) -> anyhow::Result<()> {
    let result: anyhow::Result<()> = async {
        let target = resolve_target(&app.cfg).await?;
        let image_ref = rollback_image_ref(&app.cfg.target_image_repo, &previous)?;
        let override_path = override_file_path(&app.cfg.state_path)?;
        write_override(&override_path, &target.compose_service, &image_ref).await?;

        compose_up(&app.cfg, &target, &override_path, Duration::from_secs(600)).await?;
        let _ = wait_dockrev_health(&app.cfg, Duration::from_secs(180)).await?;

        update_state(&app, |st, now| {
            st.state = "rolled_back".to_string();
            st.progress = Progress {
                step: "done".to_string(),
                message: "rolled back".to_string(),
            };
            st.updated_at = now.to_string();
            append_log_line(st, now, "WARN", "rolled back");
        })
        .await?;

        Ok(())
    }
    .await;

    if let Err(err) = result {
        let _ = update_state(&app, |st, now| {
            st.state = "failed".to_string();
            st.progress = Progress {
                step: "rollback".to_string(),
                message: format!("rollback failed: {err}"),
            };
            st.updated_at = now.to_string();
            append_log_line(st, now, "ERROR", format!("rollback failed: {err}"));
        })
        .await;
    }

    clear_running(&app).await;
    Ok(())
}

async fn fail_and_maybe_rollback(
    app: Arc<App>,
    _target: TargetRuntime,
    key: StartKey,
    previous_digest: Option<String>,
    err: anyhow::Error,
) -> anyhow::Result<()> {
    update_state(&app, |st, now| {
        st.state = "failed".to_string();
        st.progress = Progress {
            step: "rollback".to_string(),
            message: format!("failed: {err}"),
        };
        st.updated_at = now.to_string();
        append_log_line(st, now, "ERROR", err.to_string());
    })
    .await?;

    if !key.rollback_on_failure {
        clear_running(&app).await;
        return Ok(());
    }

    let prev_tag = {
        let rt = app.runtime.lock().await;
        rt.state.previous.tag.clone()
    };
    let prev = crate::state_store::PreviousRef {
        tag: prev_tag,
        digest: previous_digest,
    };
    let _ = run_rollback_only(app.clone(), prev).await;
    Ok(())
}

async fn update_state(app: &App, f: impl FnOnce(&mut StateFile, &str)) -> anyhow::Result<()> {
    let now = now_rfc3339()?;
    let mut rt = app.runtime.lock().await;
    f(&mut rt.state, &now);
    retain_recent_operation_logs(&mut rt.state, MAX_LOG_OPERATION_GROUPS);
    store_atomic(&app.cfg.state_path, &rt.state).await?;
    Ok(())
}

async fn clear_running(app: &App) {
    let mut rt = app.runtime.lock().await;
    rt.running_key = None;
}

fn override_file_path(state_path: &Path) -> anyhow::Result<PathBuf> {
    let dir = state_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid state path"))?;
    Ok(dir.join("self-upgrade.override.yml"))
}

async fn write_override(path: &Path, service: &str, image: &str) -> anyhow::Result<()> {
    let body = format!(
        "services:\n  {service}:\n    image: {image}\n",
        service = service,
        image = image
    );
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, body).await?;
    Ok(())
}

async fn wait_dockrev_health(cfg: &Config, timeout: Duration) -> anyhow::Result<TargetRuntime> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()?;
    let started = std::time::Instant::now();
    let mut last_error: Option<String> = None;

    while started.elapsed() < timeout {
        match resolve_target(cfg).await {
            Ok(target) => {
                let url = format!(
                    "http://{}:{}/api/health",
                    target.container_ip, target.dockrev_http_port
                );
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => return Ok(target),
                    Ok(resp) => {
                        last_error = Some(format!("HTTP {} {}", resp.status().as_u16(), url))
                    }
                    Err(e) => last_error = Some(format!("{e} {url}")),
                }
            }
            Err(e) => last_error = Some(e.to_string()),
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }

    Err(anyhow::anyhow!(
        "timeout waiting for dockrev health; last_error={}",
        last_error.unwrap_or_else(|| "none".to_string())
    ))
}

async fn fetch_dockrev_version(target: &TargetRuntime) -> Option<String> {
    let url = format!(
        "http://{}:{}/api/version",
        target.container_ip, target.dockrev_http_port
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .ok()?;
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed = resp.json::<serde_json::Value>().await.ok()?;
    parsed
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
