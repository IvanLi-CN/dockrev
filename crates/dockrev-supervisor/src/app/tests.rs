use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
};
use tower::util::ServiceExt;

use crate::{
    config::Config,
    state_store::{LogLine, Progress, RequestParams, StateFile, load_or_idle, store_atomic},
};

use super::{
    App, StartKey,
    auth::require_user,
    meta::{
        DEFAULT_DEVELOPER_NAME, DEFAULT_DEVELOPER_URL, DEFAULT_REPOSITORY_URL, SupervisorMeta,
        build_supervisor_meta,
    },
    orchestration::rollback_image_ref,
    routes::{
        HttpPrevious, HttpRequestParams, HttpTarget, RollbackRequest, SelfUpgradeResponse,
        StartSelfUpgradeRequest, StartTarget, build_response_operations, get_self_upgrade,
        post_self_upgrade_rollback,
    },
    state_helpers::{
        MAX_LOG_OPERATION_GROUPS, build_operation_groups, mark_failed_if_running,
        retain_recent_operation_logs,
    },
    ui::render_ui,
};

#[test]
fn rollback_image_ref_handles_full_refs_and_plain_tags() {
    let repo = "dockrev";

    let p = crate::state_store::PreviousRef {
        tag: "local".to_string(),
        digest: None,
    };
    assert_eq!(rollback_image_ref(repo, &p).unwrap(), "dockrev:local");

    let p = crate::state_store::PreviousRef {
        tag: "dockrev:local".to_string(),
        digest: None,
    };
    assert_eq!(rollback_image_ref(repo, &p).unwrap(), "dockrev:local");

    let p = crate::state_store::PreviousRef {
        tag: "dockrev".to_string(),
        digest: None,
    };
    assert_eq!(rollback_image_ref(repo, &p).unwrap(), "dockrev");

    let p = crate::state_store::PreviousRef {
        tag: "v0.1.0".to_string(),
        digest: Some("sha256:abc".to_string()),
    };
    assert_eq!(rollback_image_ref(repo, &p).unwrap(), "dockrev@sha256:abc");

    let p = crate::state_store::PreviousRef {
        tag: "unknown".to_string(),
        digest: None,
    };
    assert!(rollback_image_ref(repo, &p).is_err());
}

#[test]
fn render_ui_renders_logs_as_semantic_lines() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("function renderLogEntries(logs)"));
    assert!(html.contains("row.className = `logLine logLine-${level.toLowerCase()}`;"));
    assert!(html.contains("logsEl.replaceChildren(fragment);"));
}

#[tokio::test]
async fn supervisor_ui_accepts_base_path_with_or_without_trailing_slash() {
    let app = test_app_for_authz(None, None, true).await;
    let router = app.clone().router();

    let no_slash = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/supervisor")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_slash.status(), StatusCode::OK);

    let with_slash = router
        .oneshot(
            Request::builder()
                .uri("/supervisor/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(with_slash.status(), StatusCode::OK);
}

fn test_meta() -> SupervisorMeta {
    build_supervisor_meta(
        Some("0.9.0"),
        "0.1.0",
        Some("https://github.com/IvanLi-CN/dockrev"),
        Some("Ivan Li"),
        Some("https://github.com/IvanLi-CN"),
    )
}

fn test_log(ts: &str, level: &str, msg: &str, op_id: Option<&str>) -> LogLine {
    LogLine {
        ts: ts.to_string(),
        level: level.to_string(),
        msg: msg.to_string(),
        op_id: op_id.map(|v| v.to_string()),
    }
}

#[test]
fn build_operation_groups_handles_legacy_and_opid_logs() {
    let logs = vec![
        test_log(
            "2026-02-01T00:00:01Z",
            "INFO",
            "self-upgrade requested",
            None,
        ),
        test_log("2026-02-01T00:00:02Z", "INFO", "pull image", None),
        test_log("2026-02-01T00:00:03Z", "INFO", "dry-run done", None),
        test_log(
            "2026-02-01T00:10:01Z",
            "INFO",
            "self-upgrade requested",
            Some("sup_a"),
        ),
        test_log(
            "2026-02-01T00:10:02Z",
            "ERROR",
            "compose failed",
            Some("sup_a"),
        ),
        test_log(
            "2026-02-01T00:20:01Z",
            "INFO",
            "self-upgrade requested",
            None,
        ),
        test_log("2026-02-01T00:20:02Z", "INFO", "pull image", None),
    ];

    let groups = build_operation_groups(&logs);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].op_id, "legacy-2026_02_01t00_00_01z");
    assert_eq!(groups[0].logs.len(), 3);
    assert_eq!(groups[1].op_id, "sup_a");
    assert_eq!(groups[1].logs.len(), 2);
    assert_eq!(groups[2].op_id, "legacy-2026_02_01t00_20_01z");
    assert_eq!(groups[2].logs.len(), 2);
}

#[test]
fn retain_recent_operation_logs_caps_groups_to_thirty() {
    let now = crate::state_store::now_rfc3339().unwrap();
    let mut st = StateFile::idle(&now);
    for i in 0..35 {
        st.logs.push(test_log(
            &format!("2026-02-01T01:{i:02}:00Z"),
            "INFO",
            &format!("self-upgrade requested op{i}"),
            None,
        ));
        st.logs.push(test_log(
            &format!("2026-02-01T01:{i:02}:10Z"),
            "INFO",
            &format!("succeeded op{i}"),
            None,
        ));
    }

    retain_recent_operation_logs(&mut st, MAX_LOG_OPERATION_GROUPS);
    let groups = build_operation_groups(&st.logs);
    assert_eq!(groups.len(), 30);
    assert!(groups[0].logs[0].msg.contains("op5"));
    assert!(groups[29].logs[0].msg.contains("op34"));
}

#[test]
fn build_response_operations_returns_newest_first_with_state() {
    let now = crate::state_store::now_rfc3339().unwrap();
    let mut st = StateFile::idle(&now);
    st.state = "running".to_string();
    st.op_id = "sup_live".to_string();
    st.logs = vec![
        test_log(
            "2026-02-01T02:00:00Z",
            "INFO",
            "self-upgrade requested",
            None,
        ),
        test_log("2026-02-01T02:00:01Z", "ERROR", "compose failed", None),
        test_log(
            "2026-02-01T02:05:00Z",
            "INFO",
            "self-upgrade requested",
            Some("sup_live"),
        ),
        test_log(
            "2026-02-01T02:05:01Z",
            "INFO",
            "pull image",
            Some("sup_live"),
        ),
    ];

    let ops = build_response_operations(&st);
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].op_id, "sup_live");
    assert_eq!(ops[0].state, "running");
    assert_eq!(ops[1].state, "failed");
}

#[test]
fn infer_operation_state_prefers_current_state_for_active_op() {
    let now = crate::state_store::now_rfc3339().unwrap();
    let mut st = StateFile::idle(&now);
    st.state = "failed".to_string();
    st.op_id = "sup_same".to_string();
    st.logs = vec![
        test_log(
            "2026-02-01T02:00:00Z",
            "INFO",
            "self-upgrade requested",
            Some("sup_same"),
        ),
        test_log(
            "2026-02-01T02:00:01Z",
            "INFO",
            "succeeded",
            Some("sup_same"),
        ),
        test_log(
            "2026-02-01T02:00:02Z",
            "ERROR",
            "rollback failed: boom",
            Some("sup_same"),
        ),
    ];

    let ops = build_response_operations(&st);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].state, "failed");
}

#[test]
fn infer_operation_state_prefers_latest_terminal_marker_for_history() {
    let now = crate::state_store::now_rfc3339().unwrap();
    let mut st = StateFile::idle(&now);
    st.logs = vec![
        test_log(
            "2026-02-01T02:00:00Z",
            "INFO",
            "self-upgrade requested",
            Some("sup_old"),
        ),
        test_log(
            "2026-02-01T02:00:01Z",
            "ERROR",
            "pull failed once",
            Some("sup_old"),
        ),
        test_log("2026-02-01T02:00:02Z", "INFO", "succeeded", Some("sup_old")),
        test_log(
            "2026-02-01T02:01:00Z",
            "INFO",
            "self-upgrade requested",
            Some("sup_new"),
        ),
        test_log("2026-02-01T02:01:01Z", "INFO", "succeeded", Some("sup_new")),
        test_log(
            "2026-02-01T02:01:02Z",
            "ERROR",
            "rollback failed: boom",
            Some("sup_new"),
        ),
    ];

    let ops = build_response_operations(&st);
    assert_eq!(
        ops.iter().find(|o| o.op_id == "sup_old").unwrap().state,
        "succeeded"
    );
    assert_eq!(
        ops.iter().find(|o| o.op_id == "sup_new").unwrap().state,
        "failed"
    );
}

#[test]
fn legacy_group_ids_stay_stable_after_retention() {
    let now = crate::state_store::now_rfc3339().unwrap();
    let mut st = StateFile::idle(&now);
    for i in 0..31 {
        st.logs.push(test_log(
            &format!("2026-02-01T03:{i:02}:00Z"),
            "INFO",
            "self-upgrade requested",
            None,
        ));
        st.logs.push(test_log(
            &format!("2026-02-01T03:{i:02}:10Z"),
            "INFO",
            "dry-run done",
            None,
        ));
    }

    let before = build_operation_groups(&st.logs);
    let expected_first_id = before[1].op_id.clone();
    retain_recent_operation_logs(&mut st, MAX_LOG_OPERATION_GROUPS);
    let after = build_operation_groups(&st.logs);
    assert_eq!(after.len(), 30);
    assert_eq!(after[0].op_id, expected_first_id);
}

#[test]
fn render_ui_contains_roomier_four_panel_markup() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains(r#"data-panel="masthead""#));
    assert!(html.contains(r#"data-panel="action-deck""#));
    assert!(html.contains(r#"data-panel="status-grid""#));
    assert!(html.contains(r#"data-panel="workspace""#));
    assert!(html.contains("id=\"historyList\""));
    assert!(!html.contains("id=\"opTabs\""));
    assert!(!html.contains("id=\"tabsToggle\""));
}

#[test]
fn render_ui_contains_status_tiles_and_textual_history_markup() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("id=\"statusTarget\""));
    assert!(html.contains("id=\"statusPrevious\""));
    assert!(html.contains("id=\"statusProgressMessage\""));
    assert!(html.contains("function renderStatus(st)"));
    assert!(html.contains("function createStateBadge(state)"));
    assert!(html.contains("function operationMarker(op)"));
    assert!(html.contains("stateBadge stateBadge-"));
    assert!(html.contains("latestHasNewer"));
}

#[test]
fn render_ui_contains_semantic_log_highlighting() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("const LOG_TOKEN_PATTERN"));
    assert!(html.contains("function appendHighlightedMessage(parent, message)"));
    assert!(html.contains("function renderLogEntries(logs)"));
    assert!(html.contains("logLine-warn"));
    assert!(html.contains("logLine-error"));
    assert!(html.contains("logToken-level logLevel-"));
    assert!(html.contains("logToken-ref"));
    assert!(html.contains("logToken-opid"));
    assert!(html.contains("logToken-digest"));
    assert!(html.contains("renderLogEntries(active.logs || [])"));
}

#[test]
fn render_ui_contains_copy_buttons_for_key_refs() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("id=\"copyOpId\""));
    assert!(html.contains("id=\"copyTarget\""));
    assert!(html.contains("id=\"copyPrevious\""));
    assert!(html.contains("name: 'mdi:content-copy'"));
    assert!(html.contains("name: 'mdi:check-bold'"));
    assert!(html.contains("content: attr(data-tooltip);"));
    assert!(html.contains("function setCopyButtonTooltip(button, text)"));
    assert!(
        html.contains("button.dataset.defaultTooltip = button.getAttribute('aria-label') || '';")
    );
    assert!(html.contains("setCopyButtonTooltip(button, '已复制');"));
    assert!(html.contains(".copyButton.copied::after"));
    assert!(html.contains(".copyButton.failed::after"));
    assert!(html.contains("button.dataset.icon = icon.name;"));
    assert!(html.contains("function setCopyButtonValue(button, value)"));
    assert!(html.contains("function writeClipboardText(text)"));
    assert!(html.contains("const copied = document.execCommand('copy');"));
    assert!(html.contains("if (!copied) throw new Error('execCommand copy failed');"));
    assert!(html.contains("function hasPreviousRollbackTarget(previous)"));
    assert!(html.contains("tag !== 'unknown'"));
    assert!(html.contains("hasPreviousRollbackTarget(st?.previous)"));
    assert!(html.contains("bindCopyButton(copyOpIdBtn);"));
    assert!(html.contains("renderCopyButtonIcon(button, 'copied');"));
}

#[test]
fn render_ui_distinguishes_cached_and_uncached_offline_states() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("const cached = lastKnownSelfUpgradeState;"));
    assert!(html.contains("const hasCachedState = !!cached;"));
    assert!(html.contains("if (!hasCachedState) {"));
    assert!(html.contains("statusOpIdEl.textContent = 'unavailable';"));
    assert!(html.contains("statusTargetEl.textContent = 'unavailable while offline';"));
    assert!(
        html.contains(
            "const cachedTargetText = cached.target ? formatTargetRef(cached.target) : '';"
        )
    );
    assert!(html.contains("statusOpIdEl.textContent = cached.opId"));
    assert!(html.contains("statusTargetEl.textContent = cachedTargetText"));
    assert!(html.contains("setCopyButtonValue(copyTargetBtn, cachedTargetText);"));
}

#[test]
fn render_ui_constrains_history_rail_height() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("max-height: min(560px, calc(100vh - 190px));"));
    assert!(html.contains("overscroll-behavior: contain;"));
    assert!(html.contains("max-height: min(360px, 44vh);"));
}

#[test]
fn render_ui_collapses_history_rail_when_operation_groups_are_absent() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("id=\"workspaceGrid\""));
    assert!(html.contains("id=\"historyRail\""));
    assert!(
        html.contains(
            "workspaceGridEl.classList.toggle('workspaceGrid-logsOnly', !hasOperations);"
        )
    );
    assert!(html.contains("historyRailEl.hidden = !hasOperations;"));
    assert!(html.contains("暂无 operation 历史，已回退到扁平日志视图。"));
}

#[test]
fn render_ui_marks_latest_history_card_when_same_operation_gets_new_logs() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("let latestOpMarker = null;"));
    assert!(html.contains("const previousLatestMarker = latestOpMarker;"));
    assert!(html.contains("const nextLatestMarker = operationMarker(nextLatestOp);"));
    assert!(html.contains("previousLatestMarker !== nextLatestMarker"));
    assert!(html.contains("latestOpMarker = nextLatestMarker;"));
}

#[test]
fn render_ui_disables_dry_and_apply_during_running_operation() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("function syncUpgradeActionState(st)"));
    assert!(html.contains("dryBtn.disabled = running;"));
    assert!(html.contains("applyBtn.disabled = running;"));
}

#[test]
fn render_ui_shows_mode_specific_spinner_for_running_operation() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("button.btnRunning::before"));
    assert!(html.contains("mode === 'dry-run'"));
    assert!(html.contains("mode === 'apply'"));
    assert!(html.contains("st?.progress?.step !== 'rollback'"));
    assert!(html.contains("if (lastKnownSelfUpgradeState) {"));
    assert!(html.contains("syncUpgradeActionState(lastKnownSelfUpgradeState);"));
    assert!(html.contains("setRunningButton(dryBtn"));
    assert!(html.contains("setRunningButton(applyBtn"));
}

#[test]
fn self_upgrade_response_serializes_request_mode_when_present() {
    let now = "2026-03-03T13:23:00Z";
    let resp = SelfUpgradeResponse {
        state: "running".to_string(),
        op_id: "sup_test".to_string(),
        request: Some(HttpRequestParams {
            mode: "apply".to_string(),
            rollback_on_failure: true,
        }),
        target: HttpTarget {
            image: "ghcr.io/ivanli-cn/dockrev".to_string(),
            tag: "latest".to_string(),
            digest: None,
        },
        previous: HttpPrevious {
            tag: "prev".to_string(),
            digest: None,
        },
        started_at: now.to_string(),
        updated_at: now.to_string(),
        progress: Progress {
            step: "apply".to_string(),
            message: "docker compose up".to_string(),
        },
        logs: Vec::new(),
        operations: Vec::new(),
    };

    let value = serde_json::to_value(resp).unwrap();
    assert_eq!(value["request"]["mode"], "apply");
    assert_eq!(value["request"]["rollbackOnFailure"], true);
}

#[test]
fn self_upgrade_response_omits_request_when_absent() {
    let now = "2026-03-03T13:23:00Z";
    let resp = SelfUpgradeResponse {
        state: "idle".to_string(),
        op_id: String::new(),
        request: None,
        target: HttpTarget {
            image: "ghcr.io/ivanli-cn/dockrev".to_string(),
            tag: "latest".to_string(),
            digest: None,
        },
        previous: HttpPrevious {
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
        operations: Vec::new(),
    };

    let value = serde_json::to_value(resp).unwrap();
    assert!(value.get("request").is_none());
}

async fn test_app_for_authz(
    allowed_user: Option<&str>,
    allowed_group: Option<&str>,
    allow_anonymous_in_dev: bool,
) -> Arc<App> {
    let dir = std::env::temp_dir().join(format!(
        "dockrev-supervisor-test-{}-authz-{}",
        std::process::id(),
        ulid::Ulid::new()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();

    Arc::new(
        App::new(Config {
            http_addr: "127.0.0.1:0".to_string(),
            base_path: "/supervisor".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            auth_group_header_name: "Remote-Groups".parse().unwrap(),
            auth_allowed_user: allowed_user.map(ToString::to_string),
            auth_allowed_group: allowed_group.map(ToString::to_string),
            auth_allow_anonymous_in_dev: allow_anonymous_in_dev,
            target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            target_container_id: Some("ctr".to_string()),
            target_compose_project: Some("p".to_string()),
            target_compose_service: Some("dockrev".to_string()),
            target_compose_files: vec!["/abs/compose.yml".to_string()],
            docker_bin: "docker".to_string(),
            docker_host: None,
            compose_bin: "docker-compose".to_string(),
            state_path: dir.join("state.json"),
        })
        .await
        .unwrap(),
    )
}

#[tokio::test]
async fn supervisor_auth_allows_matching_group() {
    let app = test_app_for_authz(None, Some("ops"), false).await;
    let mut headers = HeaderMap::new();
    headers.insert("Remote-Groups", "dev, ops".parse().unwrap());

    let user = require_user(app.as_ref(), &headers).unwrap();
    assert_eq!(user, "group:ops");
}

#[tokio::test]
async fn supervisor_auth_allows_matching_user() {
    let app = test_app_for_authz(Some("alice"), None, false).await;
    let mut headers = HeaderMap::new();
    headers.insert("X-Forwarded-User", "alice".parse().unwrap());

    let user = require_user(app.as_ref(), &headers).unwrap();
    assert_eq!(user, "alice");
}

#[tokio::test]
async fn supervisor_auth_allows_anonymous_in_dev_without_identity() {
    let app = test_app_for_authz(None, None, true).await;
    let headers = HeaderMap::new();

    let user = require_user(app.as_ref(), &headers).unwrap();
    assert_eq!(user, "anonymous");
}

#[tokio::test]
async fn supervisor_auth_requires_identity_once_allowlist_is_configured() {
    let app = test_app_for_authz(Some("alice"), None, true).await;
    let headers = HeaderMap::new();

    assert!(require_user(app.as_ref(), &headers).is_err());
}

#[tokio::test]
async fn supervisor_health_requires_authorized_request() {
    let app = test_app_for_authz(Some("alice"), None, false).await;
    let router = app.clone().router();

    let resp = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/supervisor/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn supervisor_version_allows_matching_identity() {
    let app = test_app_for_authz(Some("alice"), None, false).await;
    let router = app.clone().router();

    let resp = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/supervisor/version")
                .header("X-Forwarded-User", "alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_self_upgrade_returns_request_only_while_running() {
    let dir = std::env::temp_dir().join(format!(
        "dockrev-supervisor-test-{}-request-visibility",
        std::process::id()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();

    let app = Arc::new(
        App::new(Config {
            http_addr: "127.0.0.1:0".to_string(),
            base_path: "/supervisor".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            auth_group_header_name: "Remote-Groups".parse().unwrap(),
            auth_allowed_user: None,
            auth_allowed_group: None,
            auth_allow_anonymous_in_dev: true,
            target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            target_container_id: Some("ctr".to_string()),
            target_compose_project: Some("p".to_string()),
            target_compose_service: Some("dockrev".to_string()),
            target_compose_files: vec!["/abs/compose.yml".to_string()],
            docker_bin: "docker".to_string(),
            docker_host: None,
            compose_bin: "docker-compose".to_string(),
            state_path: dir.join("state.json"),
        })
        .await
        .unwrap(),
    );

    let mut headers = HeaderMap::new();
    headers.insert("X-Forwarded-User", "ops".parse().unwrap());

    {
        let mut rt = app.runtime.lock().await;
        rt.state.state = "succeeded".to_string();
        rt.state.request = Some(RequestParams {
            mode: "apply".to_string(),
            rollback_on_failure: true,
        });
    }
    let Json(done_resp) = get_self_upgrade(State(app.clone()), headers.clone())
        .await
        .unwrap();
    assert!(done_resp.request.is_none());

    {
        let mut rt = app.runtime.lock().await;
        rt.state.state = "running".to_string();
        rt.state.request = Some(RequestParams {
            mode: "dry-run".to_string(),
            rollback_on_failure: false,
        });
    }
    let Json(running_resp) = get_self_upgrade(State(app.clone()), headers).await.unwrap();
    let req = running_resp
        .request
        .expect("running response should expose request");
    assert_eq!(req.mode, "dry-run");
    assert!(!req.rollback_on_failure);
}

#[test]
fn render_ui_contains_rollback_popconfirm_elements() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains(r#"id="rollbackPop""#));
    assert!(html.contains(r#"id="rollbackConfirm""#));
    assert!(html.contains(r#"id="rollbackCancel""#));
    assert!(html.contains("let rollbackPendingOpId = null;"));
    assert!(html.contains(r"document.addEventListener('keydown'"));
}

#[test]
fn render_ui_requires_confirm_before_rollback_post() {
    let html = render_ui("/supervisor", &test_meta());

    let rollback_click_idx = html
        .find("document.getElementById('rollback').onclick = async (evt) =>")
        .unwrap();
    let open_pop_idx = html.find("setRollbackPopOpen(true);").unwrap();
    let confirm_click_idx = html
        .find("document.getElementById('rollbackConfirm').onclick = async () =>")
        .unwrap();
    let rollback_post_idx = html.find("fetchJson('self-upgrade/rollback'").unwrap();

    assert!(rollback_click_idx < open_pop_idx);
    assert!(open_pop_idx < confirm_click_idx);
    assert!(confirm_click_idx < rollback_post_idx);
    assert!(html.contains("st.opId !== rollbackPendingOpId"));
    assert!(html.contains("JSON.stringify({ opId: rollbackPendingOpId })"));
}

#[test]
fn render_ui_contains_supervisor_meta_links() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("Supervisor 版本"));
    assert!(html.contains("开源仓库"));
    assert!(html.contains("开发者"));
    assert!(html.contains("releases/tag/0.9.0"));
    assert!(html.contains("https://github.com/IvanLi-CN/dockrev"));
    assert!(html.contains("https://github.com/IvanLi-CN"));
}

#[test]
fn render_ui_bootstraps_theme_before_styles() {
    let html = render_ui("/supervisor", &test_meta());
    let theme_bootstrap_idx = html.find("window.__dockrevSupervisorTheme").unwrap();
    let style_idx = html.find("<style>").unwrap();

    assert!(theme_bootstrap_idx < style_idx);
    assert!(html.contains("root.dataset.theme = theme;"));
    assert!(html.contains("root.style.colorScheme = theme;"));
    assert!(html.contains("const theme = storedTheme || preferredTheme();"));
}

#[test]
fn render_ui_reuses_dockrev_theme_storage_contract() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains(r#"const STORAGE_KEY = "dockrev:theme";"#));
    assert!(html.contains("window.localStorage.getItem(STORAGE_KEY)"));
    assert!(html.contains("return value === 'light' || value === 'dark' ? value : null;"));
}

#[test]
fn render_ui_syncs_theme_on_storage_and_system_changes() {
    let html = render_ui("/supervisor", &test_meta());
    assert!(html.contains("const themeController = window.__dockrevSupervisorTheme;"));
    assert!(html.contains(r#"window.addEventListener('storage'"#));
    assert!(html.contains("themeMedia.addEventListener('change', handleSystemThemeChange);"));
    assert!(html.contains("themeMedia.addListener(handleSystemThemeChange);"));
    assert!(html.contains(
        "if (themeController?.hasStoredTheme && themeController.hasStoredTheme()) return;"
    ));
}

#[test]
fn build_supervisor_meta_uses_fallbacks_when_metadata_missing() {
    let meta = build_supervisor_meta(None, "0.3.0", None, None, None);
    assert_eq!(meta.version, "0.3.0");
    assert_eq!(meta.repository, DEFAULT_REPOSITORY_URL);
    assert_eq!(meta.developer_name, DEFAULT_DEVELOPER_NAME);
    assert_eq!(meta.developer_url, DEFAULT_DEVELOPER_URL);
    assert_eq!(
        meta.release_url.as_deref(),
        Some("https://github.com/IvanLi-CN/dockrev/releases/tag/0.3.0")
    );
}

#[test]
fn build_supervisor_meta_prefers_provided_runtime_and_package_metadata() {
    let meta = build_supervisor_meta(
        Some("9.9.9"),
        "0.3.0",
        Some("https://github.com/acme/dockrev-fork"),
        Some("Alice <alice@example.com>:Bob"),
        Some("https://acme.example/dev"),
    );
    assert_eq!(meta.version, "9.9.9");
    assert_eq!(meta.repository, "https://github.com/acme/dockrev-fork");
    assert_eq!(meta.developer_name, "Alice");
    assert_eq!(meta.developer_url, "https://acme.example/dev");
    assert_eq!(
        meta.release_url.as_deref(),
        Some("https://github.com/acme/dockrev-fork/releases/tag/9.9.9")
    );
}

#[test]
fn build_supervisor_meta_normalizes_github_repo_release_link() {
    let with_dot_git = build_supervisor_meta(
        Some("1.2.3"),
        "0.3.0",
        Some("https://github.com/acme/dockrev-fork.git"),
        Some("Alice"),
        None,
    );
    assert_eq!(
        with_dot_git.release_url.as_deref(),
        Some("https://github.com/acme/dockrev-fork/releases/tag/1.2.3")
    );

    let with_trailing_slash = build_supervisor_meta(
        Some("1.2.3"),
        "0.3.0",
        Some("https://github.com/acme/dockrev-fork/"),
        Some("Alice"),
        None,
    );
    assert_eq!(
        with_trailing_slash.release_url.as_deref(),
        Some("https://github.com/acme/dockrev-fork/releases/tag/1.2.3")
    );
}

#[tokio::test]
async fn start_is_idempotent_while_running() {
    let dir = std::env::temp_dir().join(format!(
        "dockrev-supervisor-test-{}-idem",
        std::process::id()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();

    let cfg = Config {
        http_addr: "127.0.0.1:0".to_string(),
        base_path: "/supervisor".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_group_header_name: "Remote-Groups".parse().unwrap(),
        auth_allowed_user: None,
        auth_allowed_group: None,
        auth_allow_anonymous_in_dev: true,
        target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        target_container_id: Some("ctr".to_string()),
        target_compose_project: Some("p".to_string()),
        target_compose_service: Some("dockrev".to_string()),
        target_compose_files: vec!["/abs/compose.yml".to_string()],
        docker_bin: "docker".to_string(),
        docker_host: None,
        compose_bin: "docker-compose".to_string(),
        state_path: dir.join("state.json"),
    };

    let app = App::new(cfg).await.unwrap();

    // Force state to running and set running key.
    {
        let mut rt = app.runtime.lock().await;
        rt.state.state = "running".to_string();
        rt.state.op_id = "sup_1".to_string();
        rt.running_key = Some(StartKey {
            tag: "latest".to_string(),
            digest: None,
            mode: "apply".to_string(),
            rollback_on_failure: true,
        });
    }

    let op1 = app
        .start_op(StartSelfUpgradeRequest {
            target: StartTarget {
                tag: "latest".to_string(),
                digest: None,
            },
            mode: "apply".to_string(),
            rollback_on_failure: true,
        })
        .await
        .unwrap();
    assert_eq!(op1, "sup_1");

    let err = app
        .start_op(StartSelfUpgradeRequest {
            target: StartTarget {
                tag: "v1.2.3".to_string(),
                digest: None,
            },
            mode: "apply".to_string(),
            rollback_on_failure: true,
        })
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn mark_failed_if_running_transitions_state_and_clears_running_key() {
    let dir = std::env::temp_dir().join(format!(
        "dockrev-supervisor-test-{}-fail",
        std::process::id()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();

    let cfg = Config {
        http_addr: "127.0.0.1:0".to_string(),
        base_path: "/supervisor".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_group_header_name: "Remote-Groups".parse().unwrap(),
        auth_allowed_user: None,
        auth_allowed_group: None,
        auth_allow_anonymous_in_dev: true,
        target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        target_container_id: Some("ctr".to_string()),
        target_compose_project: Some("p".to_string()),
        target_compose_service: Some("dockrev".to_string()),
        target_compose_files: vec!["/abs/compose.yml".to_string()],
        docker_bin: "docker".to_string(),
        docker_host: None,
        compose_bin: "docker-compose".to_string(),
        state_path: dir.join("state.json"),
    };

    let app = App::new(cfg).await.unwrap();
    {
        let mut rt = app.runtime.lock().await;
        rt.state.state = "running".to_string();
        rt.state.progress = Progress {
            step: "precheck".to_string(),
            message: "starting".to_string(),
        };
        rt.running_key = Some(StartKey {
            tag: "latest".to_string(),
            digest: None,
            mode: "apply".to_string(),
            rollback_on_failure: true,
        });
    }

    mark_failed_if_running(&app, anyhow::anyhow!("boom")).await;

    let rt = app.runtime.lock().await;
    assert_eq!(rt.state.state, "failed");
    assert!(rt.running_key.is_none());
}

#[tokio::test]
async fn app_new_trims_legacy_large_logs_on_boot() {
    let dir = std::env::temp_dir().join(format!(
        "dockrev-supervisor-test-{}-boottrim",
        std::process::id()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();

    let cfg = Config {
        http_addr: "127.0.0.1:0".to_string(),
        base_path: "/supervisor".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_group_header_name: "Remote-Groups".parse().unwrap(),
        auth_allowed_user: None,
        auth_allowed_group: None,
        auth_allow_anonymous_in_dev: true,
        target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        target_container_id: Some("ctr".to_string()),
        target_compose_project: Some("p".to_string()),
        target_compose_service: Some("dockrev".to_string()),
        target_compose_files: vec!["/abs/compose.yml".to_string()],
        docker_bin: "docker".to_string(),
        docker_host: None,
        compose_bin: "docker-compose".to_string(),
        state_path: dir.join("state.json"),
    };

    let now = crate::state_store::now_rfc3339().unwrap();
    let mut st = StateFile::idle(&now);
    for i in 0..31 {
        let op_id = format!("sup_boot_{i}");
        st.logs.push(test_log(
            &format!("2026-02-01T05:{i:02}:00Z"),
            "INFO",
            "self-upgrade requested",
            Some(&op_id),
        ));
        st.logs.push(test_log(
            &format!("2026-02-01T05:{i:02}:10Z"),
            "INFO",
            "dry-run done",
            Some(&op_id),
        ));
    }
    store_atomic(&cfg.state_path, &st).await.unwrap();

    let app = App::new(cfg.clone()).await.unwrap();
    let rt = app.runtime.lock().await;
    assert_eq!(build_operation_groups(&rt.state.logs).len(), 30);
    drop(rt);

    let persisted = load_or_idle(&cfg.state_path).await.unwrap();
    assert_eq!(build_operation_groups(&persisted.logs).len(), 30);
}

#[tokio::test]
async fn post_self_upgrade_rollback_clears_request_for_running_rollback() {
    let dir = std::env::temp_dir().join(format!(
        "dockrev-supervisor-test-{}-rollback-request",
        std::process::id()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();

    let app = Arc::new(
        App::new(Config {
            http_addr: "127.0.0.1:0".to_string(),
            base_path: "/supervisor".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            auth_group_header_name: "Remote-Groups".parse().unwrap(),
            auth_allowed_user: None,
            auth_allowed_group: None,
            auth_allow_anonymous_in_dev: true,
            target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            target_container_id: Some("ctr".to_string()),
            target_compose_project: Some("p".to_string()),
            target_compose_service: Some("dockrev".to_string()),
            target_compose_files: vec!["/abs/compose.yml".to_string()],
            docker_bin: "docker".to_string(),
            docker_host: None,
            compose_bin: "docker-compose".to_string(),
            state_path: dir.join("state.json"),
        })
        .await
        .unwrap(),
    );

    {
        let mut rt = app.runtime.lock().await;
        rt.state.state = "failed".to_string();
        rt.state.op_id = "sup_test".to_string();
        rt.state.request = Some(RequestParams {
            mode: "apply".to_string(),
            rollback_on_failure: true,
        });
        rt.state.previous.tag = "prev".to_string();
        rt.state.previous.digest = Some("sha256:abc".to_string());
    }

    let mut headers = HeaderMap::new();
    headers.insert("X-Forwarded-User", "ops".parse().unwrap());

    let result = post_self_upgrade_rollback(
        State(app.clone()),
        headers,
        Json(RollbackRequest {
            op_id: "sup_test".to_string(),
        }),
    )
    .await;
    assert!(result.is_ok());

    let rt = app.runtime.lock().await;
    assert!(rt.state.request.is_none());
}
