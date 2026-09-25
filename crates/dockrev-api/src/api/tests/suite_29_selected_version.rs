#[derive(Clone, Default)]
struct SelectedVersionRegistry;

#[async_trait::async_trait]
impl RegistryClient for SelectedVersionRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec![
            "latest".to_string(),
            "v2.71.37".to_string(),
            "v2.71.38".to_string(),
        ])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = if reference.starts_with("sha256:") {
            reference.to_string()
        } else {
            let byte = match reference {
                "v2.71.37" => '7',
                "v2.71.38" => '8',
                _ => '9',
            };
            format!("sha256:{}", byte.to_string().repeat(64))
        };
        Ok(ManifestInfo {
            digest: Some(digest),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

fn selected_version_digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

#[test]
fn configured_tag_observation_ignores_unrelated_runtime_version() {
    assert_eq!(
        crate::api::operations::configured_tag_observation_version(
            "latest",
            &selected_version_digest('7'),
            Some(&selected_version_digest('4')),
            Some("v2.71.34"),
        ),
        None
    );
    assert_eq!(
        crate::api::operations::configured_tag_observation_version(
            "latest",
            &selected_version_digest('7'),
            Some(&selected_version_digest('7')),
            Some("v2.71.37"),
        ),
        Some("v2.71.37".to_string())
    );
    assert_eq!(
        crate::api::operations::configured_tag_observation_version(
            "v2.71.37",
            &selected_version_digest('7'),
            Some(&selected_version_digest('4')),
            Some("v2.71.34"),
        ),
        Some("v2.71.37".to_string())
    );
}

#[tokio::test]
async fn snapshot_version_inference_binds_matching_observations_after_check_completion() {
    let state = test_state_with(
        ":memory:",
        Arc::new(SelectedVersionRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let (_, service_id, _) = selected_version_seed_service(&state).await;
    let digest = selected_version_digest('7');
    record_selected_version_observation(&state, &service_id, &digest, None).await;

    let checked_at = test_now_rfc3339();
    let snapshot = serde_json::json!({
        "digest": digest,
        "tags": ["latest", "v2.71.37"],
        "checkedAt": checked_at,
        "scan": {
            "repoTagsTotal": 2,
            "repoTagsConsidered": 2,
            "manifestsOk": 2,
            "manifestsTimeout": 0,
            "manifestsError": 0,
        },
    });
    let snapshot_json = serde_json::to_string(&snapshot).unwrap();
    state
        .db
        .upsert_image_digest_tags_snapshot(
            "ghcr.io/acme/web",
            &selected_version_digest('7'),
            "linux/amd64",
            &snapshot_json,
            &checked_at,
            &checked_at,
        )
        .await
        .unwrap();

    let observations = state
        .db
        .list_service_version_tag_observations(&service_id, "ghcr.io/acme/web", "latest")
        .await
        .unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].digest, selected_version_digest('7'));
    assert_eq!(observations[0].version.as_deref(), Some("v2.71.37"));
}

#[tokio::test]
async fn successful_check_observation_binds_from_an_existing_snapshot() {
    let state = test_state_with(
        ":memory:",
        Arc::new(SelectedVersionRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let (_, service_id, _) = selected_version_seed_service(&state).await;
    let digest = selected_version_digest('7');
    let checked_at = test_now_rfc3339();
    let snapshot_json = serde_json::json!({
        "digest": digest,
        "tags": ["latest", "v2.71.37"],
        "checkedAt": checked_at,
        "scan": {
            "repoTagsTotal": 2,
            "repoTagsConsidered": 2,
            "manifestsOk": 2,
            "manifestsTimeout": 0,
            "manifestsError": 0,
        },
    })
    .to_string();
    state
        .db
        .upsert_image_digest_tags_snapshot(
            "ghcr.io/acme/web",
            &digest,
            "linux/amd64",
            &snapshot_json,
            &checked_at,
            &checked_at,
        )
        .await
        .unwrap();

    record_selected_version_observation(&state, &service_id, &digest, None).await;

    let observations = state
        .db
        .list_service_version_tag_observations(&service_id, "ghcr.io/acme/web", "latest")
        .await
        .unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].version.as_deref(), Some("v2.71.37"));
}

#[derive(Clone, Default)]
struct PendingSelectedVersionRunner;

#[async_trait::async_trait]
impl CommandRunner for PendingSelectedVersionRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        _timeout: std::time::Duration,
    ) -> anyhow::Result<CommandOutput> {
        if spec.args.as_slice() == ["version"] {
            return Ok(CommandOutput {
                status: 0,
                stdout: "Docker Compose version v2.40.0\n".to_string(),
                stderr: String::new(),
            });
        }
        std::future::pending().await
    }
}

async fn selected_version_seed_service(state: &Arc<AppState>) -> (String, String, String) {
    let compose_path = format!(
        "/tmp/dockrev-selected-version-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(state, "selected-version", &compose_path).await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some(selected_version_digest('4')),
            Some("v2.71.34".to_string()),
            Some("[\"v2.71.34\"]".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    (stack_id, service_id, compose_path)
}

async fn record_selected_version_observation(
    state: &Arc<AppState>,
    service_id: &str,
    digest: &str,
    version: Option<&str>,
) {
    record_selected_version_observation_for_key(
        state,
        service_id,
        "ghcr.io/acme/web",
        "latest",
        digest,
        version,
    )
    .await;
}

async fn record_selected_version_observation_for_key(
    state: &Arc<AppState>,
    service_id: &str,
    image_repo: &str,
    configured_tag: &str,
    digest: &str,
    version: Option<&str>,
) {
    let now = test_now_rfc3339();
    let job_id = insert_check_job(state, "ui", &now).await;
    state
        .db
        .finish_job(
            &job_id,
            "success",
            &now,
            &serde_json::json!({
                "configuredTagObservations": [{
                    "serviceId": service_id,
                    "imageRepo": image_repo,
                    "configuredTag": configured_tag,
                    "digest": digest,
                    "version": version,
                    "observedAt": now,
                }],
            }),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn successful_check_observations_are_deduplicated_and_inference_binds_to_seen_tag() {
    let state = test_state_with(
        ":memory:",
        Arc::new(SelectedVersionRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let (stack_id, service_id, _) = selected_version_seed_service(&state).await;
    let target_digest = selected_version_digest('7');

    let latest_before_observation = state
        .db
        .list_service_version_tag_observations(&service_id, "ghcr.io/acme/web", "latest")
        .await
        .unwrap();
    assert!(latest_before_observation.is_empty(), "migration must not backfill");

    record_selected_version_observation(&state, &service_id, &target_digest, None).await;
    record_selected_version_observation(&state, &service_id, &target_digest, None).await;
    record_selected_version_observation_for_key(
        &state,
        &service_id,
        "ghcr.io/acme/web",
        "stable",
        &target_digest,
        None,
    )
    .await;
    record_selected_version_observation_for_key(
        &state,
        &service_id,
        "ghcr.io/acme/other",
        "latest",
        &target_digest,
        None,
    )
    .await;
    let pending = state
        .db
        .upsert_auto_update_candidate(
            &crate::db::AutoUpdateCandidateInput {
                id: format!("{service_id}:{target_digest}"),
                stack_id,
                service_id: service_id.clone(),
                image_ref: "ghcr.io/acme/web:latest".to_string(),
                raw_tag: "latest".to_string(),
                candidate_digest: target_digest.clone(),
                resolved_version: None,
                status: "awaiting_inference".to_string(),
                reason: Some("version_inference_pending".to_string()),
                attempts: 1,
                retry_at: None,
                discovered_at: test_now_rfc3339(),
                source_job_id: "check-source".to_string(),
                source: "ui".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "v2.71.34".to_string(),
                current_digest: Some(selected_version_digest('4')),
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    let settled_at = test_now_rfc3339();
    state
        .db
        .settle_auto_update_candidate(&crate::db::AutoUpdateCandidateSettlementInput {
            service_id: service_id.clone(),
            candidate_digest: target_digest.clone(),
            status: "ready".to_string(),
            resolved_version: Some("v2.71.37".to_string()),
            resolved_tags: None,
            reason: Some("digest_bound_version".to_string()),
            last_error: None,
            attempts: pending.attempts,
            evidence_generation: pending.evidence_generation,
            retry_at: None,
            settled_at: Some(settled_at.clone()),
            now: settled_at,
        })
        .await
        .unwrap();

    let latest = state
        .db
        .list_service_version_tag_observations(&service_id, "ghcr.io/acme/web", "latest")
        .await
        .unwrap();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].version.as_deref(), Some("v2.71.37"));
    assert_eq!(latest[0].digest, target_digest);
    let switched_tag = state
        .db
        .list_service_version_tag_observations(&service_id, "ghcr.io/acme/web", "stable")
        .await
        .unwrap();
    assert_eq!(switched_tag.len(), 1);
    assert_eq!(switched_tag[0].version, None);
    let switched_repository = state
        .db
        .list_service_version_tag_observations(&service_id, "ghcr.io/acme/other", "latest")
        .await
        .unwrap();
    assert_eq!(switched_repository.len(), 1);
    assert_eq!(switched_repository[0].version, None);
}

#[tokio::test]
async fn service_version_preview_classifies_observed_and_unobserved_releases_and_rejects_stale_submit() {
    let state = test_state_with(
        ":memory:",
        Arc::new(SelectedVersionRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let (_, service_id, _) = selected_version_seed_service(&state).await;
    let observed_digest = selected_version_digest('7');
    record_selected_version_observation(&state, &service_id, &observed_digest, Some("v2.71.37"))
        .await;
    let app = api::router(state.clone());

    let observed_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/version-update/preview"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"releaseTag":"v2.71.37"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(observed_response.status(), 200);
    let observed = response_json(observed_response).await;
    assert_eq!(observed["classification"], "normal");
    assert_eq!(observed["targetDigest"], observed_digest);
    assert_eq!(observed["configuredTag"], "latest");

    let unknown_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/version-update/preview"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"releaseTag":"v2.71.38"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown_response.status(), 200);
    let unknown = response_json(unknown_response).await;
    assert_eq!(unknown["classification"], "forced");
    assert_eq!(unknown["targetDigest"], selected_version_digest('8'));

    let older_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/version-update/preview"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"releaseTag":"v2.71.33"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(older_response.status(), 400);

    for release_tag in ["v2.71.34", "not-a-version"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/services/{service_id}/version-update/preview"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "releaseTag": release_tag }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "release tag {release_tag} must not be actionable");
    }

    let force_not_confirmed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/version-update"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "releaseTag": unknown["releaseTag"],
                        "classification": unknown["classification"],
                        "targetDigest": unknown["targetDigest"],
                        "currentDigest": unknown["currentDigest"],
                        "imageReference": unknown["imageReference"],
                        "imageRepo": unknown["imageRepo"],
                        "configuredTag": unknown["configuredTag"],
                        "forceConfirmed": false,
                        "backupMode": "inherit",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(force_not_confirmed.status(), 400);
    assert!(state
        .db
        .list_jobs()
        .await
        .unwrap()
        .iter()
        .all(|job| job.r#type.as_str() != "update"));

    let generic_skip_pull = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "scope": "service",
                        "serviceId": service_id.clone(),
                        "targets": [{
                            "serviceId": service_id.clone(),
                            "targetTag": "latest",
                            "targetDigest": selected_version_digest('8'),
                            "pullTags": [],
                            "skipTargetTagPull": true,
                        }],
                        "mode": "apply",
                        "allowArchMismatch": false,
                        "backupMode": "inherit",
                        "reason": "ui",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(generic_skip_pull.status(), 400);
    assert!(state
        .db
        .list_jobs()
        .await
        .unwrap()
        .iter()
        .all(|job| job.r#type.as_str() != "update"));

    let changed_at = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some(selected_version_digest('5')),
            Some("v2.71.35".to_string()),
            Some("[\"v2.71.35\"]".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &changed_at,
            &changed_at,
        )
        .await
        .unwrap();
    let stale_submit = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/version-update"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "releaseTag": unknown["releaseTag"],
                        "classification": unknown["classification"],
                        "targetDigest": unknown["targetDigest"],
                        "currentDigest": unknown["currentDigest"],
                        "imageReference": unknown["imageReference"],
                        "imageRepo": unknown["imageRepo"],
                        "configuredTag": unknown["configuredTag"],
                        "forceConfirmed": true,
                        "backupMode": "inherit",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_submit.status(), 409);
    assert!(state
        .db
        .list_jobs()
        .await
        .unwrap()
        .iter()
        .all(|job| job.r#type.as_str() != "update"));
}

#[tokio::test]
async fn service_version_preview_uses_current_digest_observation_when_tag_is_unresolved() {
    let state = test_state_with(
        ":memory:",
        Arc::new(SelectedVersionRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let (_, service_id, _) = selected_version_seed_service(&state).await;
    let current_digest = selected_version_digest('4');
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some(current_digest.clone()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    record_selected_version_observation(
        &state,
        &service_id,
        &current_digest,
        Some("v2.71.34"),
    )
    .await;

    let response = api::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/version-update/preview"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"releaseTag":"v2.71.37"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let preview = response_json(response).await;
    assert_eq!(preview["currentVersion"], "v2.71.34");
    assert_eq!(preview["currentDigest"], current_digest);
    assert_eq!(preview["classification"], "forced");
}

#[tokio::test]
async fn normal_selected_version_submit_persists_exact_digest_and_tag_pull_policy() {
    let state = test_state_with(
        ":memory:",
        Arc::new(SelectedVersionRegistry),
        Arc::new(PendingSelectedVersionRunner),
    )
    .await;
    let (_, service_id, _) = selected_version_seed_service(&state).await;
    let target_digest = selected_version_digest('7');
    record_selected_version_observation(&state, &service_id, &target_digest, Some("v2.71.37"))
        .await;
    let app = api::router(state.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/version-update/preview"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"releaseTag":"v2.71.37"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let preview = response_json(response).await;
    let submit = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/version-update"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "releaseTag": preview["releaseTag"],
                        "classification": preview["classification"],
                        "targetDigest": preview["targetDigest"],
                        "currentDigest": preview["currentDigest"],
                        "imageReference": preview["imageReference"],
                        "imageRepo": preview["imageRepo"],
                        "configuredTag": preview["configuredTag"],
                        "forceConfirmed": false,
                        "backupMode": "inherit",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(submit.status(), 200);
    let submitted = response_json(submit).await;
    let job = state
        .db
        .get_job(submitted["jobId"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    let target = &job.summary_json["targets"][0];
    assert_eq!(target["targetTag"], "latest");
    assert_eq!(target["targetDigest"], target_digest);
    assert_eq!(target["skipTargetTagPull"], true);
    let recovered_target: crate::api::types::UpdateServiceTarget =
        serde_json::from_value(target.clone()).unwrap();
    assert!(recovered_target.skip_target_tag_pull);
}
