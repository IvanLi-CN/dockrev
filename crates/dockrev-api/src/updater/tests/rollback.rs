use super::*;

#[test]
fn managed_override_prepare_and_apply_share_one_rollback_snapshot() {
    let root = std::env::temp_dir().join(format!(
        "dockrev-override-transaction-{}",
        ulid::Ulid::new()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let stack = single_service_stack(
        "ghcr.io/acme/web:1.0",
        Some(crate::api::types::Candidate {
            tag: "1.1".to_string(),
            resolved_tag: None,
            digest: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            arch_match: crate::api::types::ArchMatch::Match,
            arch: Vec::new(),
        }),
    );
    let old = "services:\n  web:\n    image: ghcr.io/acme/web@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n";
    let path = crate::managed_override::managed_override_path(&root, &stack.id);
    crate::managed_override::commit_with_snapshot(&path, old).unwrap();
    crate::managed_override::discard_snapshot(&path).unwrap();

    let targets = explicit_targets(
        "svc_1",
        "1.1",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        &[],
    );
    let first = build_override_file(
        &stack,
        &[&stack.services[0]],
        &HashMap::from([("svc_1".to_string(), targets[0].clone())]),
        Some(&root),
        false,
        &["web".to_string()],
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&first).unwrap(),
        "services:\n  web:\n    image: ghcr.io/acme/web:1.1@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
    );
    let previous = std::fs::read_to_string(format!("{}.previous", first.display())).unwrap();
    let _second = build_override_file(
        &stack,
        &[&stack.services[0]],
        &HashMap::from([("svc_1".to_string(), targets[0].clone())]),
        Some(&root),
        true,
        &["web".to_string()],
    )
    .unwrap()
    .unwrap();
    assert_eq!(previous, old);

    let _new_operation = build_override_file(
        &stack,
        &[&stack.services[0]],
        &HashMap::from([("svc_1".to_string(), targets[0].clone())]),
        Some(&root),
        false,
        &["web".to_string()],
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(format!("{}.previous", first.display())).unwrap(),
        std::fs::read_to_string(&first).unwrap()
    );

    restore_managed_override_snapshot(&first).unwrap();
    assert!(
        std::fs::read_to_string(&first)
            .unwrap()
            .contains("@sha256:aaaaaaaa")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn managed_override_keeps_candidate_tag_with_its_digest() {
    let root = std::env::temp_dir().join(format!("dockrev-override-tag-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&root).unwrap();
    let stack = single_service_stack(
        "docker.io/valkey/valkey:9.1",
        Some(crate::api::types::Candidate {
            tag: "9.1".to_string(),
            resolved_tag: Some("9.1.2".to_string()),
            digest: "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                .to_string(),
            arch_match: crate::api::types::ArchMatch::Match,
            arch: Vec::new(),
        }),
    );

    let path = build_override_file(
        &stack,
        &[&stack.services[0]],
        &HashMap::new(),
        Some(&root),
        false,
        &[],
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "services:\n  web:\n    image: docker.io/valkey/valkey:9.1@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\n"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[derive(Default)]
struct HealthRollbackRunner {
    step: Mutex<usize>,
    events: Mutex<Vec<&'static str>>,
    timed_out: bool,
    fail_log_write: bool,
    fail_rollback_image_inspect: bool,
    expected_managed_override: Option<String>,
}

#[async_trait::async_trait]
impl CommandRunner for HealthRollbackRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        if let Some(path) = &self.expected_managed_override
            && *step > 0
            && spec.program == "docker-compose"
        {
            assert!(
                spec.args
                    .windows(2)
                    .any(|window| { window[0] == "-f" && window[1] == *path }),
                "compose command did not use managed override: {:?}",
                spec.args
            );
        }
        let out = match *step {
            0 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "old_container\n".to_string(),
                    stderr: String::new(),
                }
            }
            1 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "old_container"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                }
            }
            2 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["pull", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            3 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["up", "-d", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            4 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "new_container\n".to_string(),
                    stderr: String::new(),
                }
            }
            5 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec![
                        "inspect",
                        "--format",
                        "{{if .State.Health}}1{{else}}0{{end}}",
                        "new_container"
                    ]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "1\n".to_string(),
                    stderr: String::new(),
                }
            }
            6 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec![
                        "inspect",
                        "--format",
                        "{{json .Config.Healthcheck}}",
                        "new_container"
                    ]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "null\n".to_string(),
                    stderr: String::new(),
                }
            }
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "new_container"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                }
            }
            8 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec![
                        "inspect",
                        "--format",
                        "{{.State.Health.Status}}",
                        "new_container"
                    ]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "unhealthy\n".to_string(),
                    stderr: String::new(),
                }
            }
            9 => {
                self.events.lock().unwrap().push("rollback");
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["image", "tag", "sha256:old", "ghcr.io/org/web:1.0"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            10 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(
                    &spec.args,
                    &["up", "-d", "--pull", "never", "--no-deps", "web"]
                ));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            11 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "rollback_container\n".to_string(),
                    stderr: String::new(),
                }
            }
            12 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec![
                        "inspect",
                        "--format",
                        "{{.State.Health.Status}}",
                        "rollback_container"
                    ]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "healthy\n".to_string(),
                    stderr: String::new(),
                }
            }
            13 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "rollback_container"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                if self.fail_rollback_image_inspect {
                    *step += 1;
                    return Err(anyhow::anyhow!("injected post-healthcheck inspect failure"));
                }
                CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                }
            }
            _ => panic!(
                "unexpected extra command: program={} args={:?}",
                spec.program, spec.args
            ),
        };
        *step += 1;
        Ok(out)
    }

    async fn run_raw(
        &self,
        _spec: CommandSpec,
        _timeout: Duration,
    ) -> anyhow::Result<crate::runner::RawCommandOutput> {
        Ok(crate::runner::RawCommandOutput {
            status: 0,
            stdout: br#"{"Status":"running","Error":"","ExitCode":0,"RestartCount":0,"Health":{"Status":"unhealthy","Log":[]}}"#.to_vec(),
            stderr: Vec::new(),
        })
    }

    async fn run_raw_to_file(
        &self,
        _spec: CommandSpec,
        _timeout: Duration,
        output_path: &std::path::Path,
    ) -> anyhow::Result<crate::runner::RawFileCommandOutput> {
        let bytes = b"candidate logs before rollback";
        if self.fail_log_write {
            tokio::fs::write(output_path, b"candidate logs before").await?;
            self.events.lock().unwrap().push("logs-write-failed");
            return Err(anyhow::anyhow!("injected file write failure"));
        }
        tokio::fs::write(output_path, bytes).await?;
        self.events.lock().unwrap().push("logs-captured");
        Ok(crate::runner::RawFileCommandOutput {
            status: if self.timed_out { -1 } else { 0 },
            bytes_written: bytes.len() as u64,
            stderr: Vec::new(),
            eof_reached: !self.timed_out,
            timed_out: self.timed_out,
        })
    }
}

#[tokio::test]
async fn healthcheck_failure_count_survives_a_later_update_error() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = HealthRollbackRunner {
        fail_rollback_image_inspect: true,
        ..Default::default()
    };
    let failure_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let result = run_update_job_with_gate_using_root_unlocked(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        None,
        false,
        &[],
        None,
        None,
        None,
        Some(failure_count.clone()),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(failure_count.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(*runner.events.lock().unwrap(), vec!["rollback"]);
}

#[tokio::test]
async fn healthcheck_failure_rolls_back_with_attempted_and_final_digests() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = HealthRollbackRunner {
        timed_out: true,
        ..Default::default()
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UpdateProgressEvent>();
    let root = std::env::temp_dir().join(format!("dockrev-health-evidence-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&root).unwrap();
    let healthcheck_failure_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let evidence = crate::rollback_evidence::RollbackEvidenceContext::new(
        "job-health-failure",
        &root.join("dockrev.sqlite"),
    )
    .unwrap();

    let outcome = run_update_job_with_gate_using_root_unlocked(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        Some(tx),
        false,
        &[],
        None,
        None,
        Some(evidence.clone()),
        Some(healthcheck_failure_count.clone()),
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "rolled_back");
    assert_eq!(
        healthcheck_failure_count.load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(
        outcome.summary_json["newDigests"]["svc_1"],
        json!("sha256:new")
    );
    assert_eq!(
        outcome.summary_json["finalDigests"]["svc_1"],
        json!("sha256:old")
    );
    assert_eq!(
        outcome.summary_json["failureStep"].as_str(),
        Some("healthcheck")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["trigger"],
        json!("healthcheck")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["toDigests"]["svc_1"],
        json!("sha256:old")
    );

    let mut steps = Vec::new();
    let mut messages = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        steps.push(evt.step);
        messages.push(evt.message);
    }
    assert!(steps.contains(&UpdateProgressStep::HealthStart));
    assert!(steps.contains(&UpdateProgressStep::HealthFailed));
    assert!(!steps.contains(&UpdateProgressStep::HealthDone));
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("healthcheck failed"))
    );
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("rolled back after healthcheck failure"))
    );
    assert_eq!(*runner.step.lock().unwrap(), 14);
    assert_eq!(
        *runner.events.lock().unwrap(),
        vec!["logs-captured", "rollback"]
    );
    assert!(evidence.metadata()[0].logs_truncated);
    assert!(
        evidence.metadata()[0]
            .capture_errors
            .iter()
            .any(|error| error.contains("timed out"))
    );
    assert_eq!(
        tokio::fs::read(
            evidence
                .job_spool_path()
                .join("svc_1/new_container/container.log")
        )
        .await
        .unwrap(),
        b"candidate logs before rollback"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn log_file_write_failure_preserves_partial_evidence_and_rolls_back() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = HealthRollbackRunner {
        fail_log_write: true,
        ..Default::default()
    };
    let root =
        std::env::temp_dir().join(format!("dockrev-health-write-error-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&root).unwrap();
    let evidence = crate::rollback_evidence::RollbackEvidenceContext::new(
        "job-health-write-error",
        &root.join("dockrev.sqlite"),
    )
    .unwrap();

    let outcome = run_update_job_with_gate_using_root_unlocked(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        None,
        false,
        &[],
        None,
        None,
        Some(evidence.clone()),
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "rolled_back");
    assert_eq!(
        *runner.events.lock().unwrap(),
        vec!["logs-write-failed", "rollback"]
    );
    let metadata = &evidence.metadata()[0];
    assert!(metadata.logs_truncated);
    assert!(
        metadata
            .capture_errors
            .iter()
            .any(|error| error.contains("injected file write failure"))
    );
    assert_eq!(metadata.logs_bytes, b"candidate logs before".len() as u64);
    assert_eq!(
        tokio::fs::read(
            evidence
                .job_spool_path()
                .join("svc_1/new_container/container.log")
        )
        .await
        .unwrap(),
        b"candidate logs before"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn managed_root_is_used_for_update_and_rollback_compose_commands() {
    let root = std::env::temp_dir().join(format!("dockrev-managed-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&root).unwrap();
    let _cleanup = TempDirCleanup(root.clone());
    let stack = single_service_stack(
        "ghcr.io/org/web:1.0",
        Some(Candidate {
            tag: "1.0".to_string(),
            resolved_tag: Some("1.0".to_string()),
            digest: "sha256:new".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let managed_path = crate::managed_override::managed_override_path(&root, &stack.id);
    let runner = HealthRollbackRunner {
        expected_managed_override: Some(managed_path.to_string_lossy().to_string()),
        ..Default::default()
    };

    let outcome = run_update_job_with_gate_using_root(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        None,
        false,
        &[],
        None,
        Some(root.as_path()),
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "rolled_back");
    assert!(managed_path.is_file());
    assert!(!managed_path.with_extension("yml.previous").is_file());
    assert!(!crate::managed_override::has_pending_snapshot(
        &managed_path
    ));
}

#[derive(Default)]
struct SyncTagRollbackRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for SyncTagRollbackRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container\n".to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old\n".to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            },
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "cannot sync tag".to_string(),
                }
            }
            8 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["image", "tag", "sha256:old", "ghcr.io/org/web:1.0"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            9 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(
                    &spec.args,
                    &["up", "-d", "--pull", "never", "--no-deps", "web"]
                ));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            10 => CommandOutput {
                status: 0,
                stdout: "rollback_container\n".to_string(),
                stderr: String::new(),
            },
            11 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "rollback_container"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                }
            }
            _ => panic!(
                "unexpected extra command: program={} args={:?}",
                spec.program, spec.args
            ),
        };
        *step += 1;
        Ok(out)
    }
}

#[derive(Default)]
struct DigestPinnedRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for DigestPinnedRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container\n".to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old\n".to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            },
            _ => panic!(
                "unexpected extra command: program={} args={:?}",
                spec.program, spec.args
            ),
        };
        *step += 1;
        Ok(out)
    }
}

#[derive(Default)]
struct ExplicitTargetDigestSyncRunner {
    step: Mutex<usize>,
    skip_target_tag_pull: bool,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

#[async_trait::async_trait]
impl CommandRunner for ExplicitTargetDigestSyncRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls
            .lock()
            .unwrap()
            .push((spec.program.clone(), spec.args.clone()));
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container\n".to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old\n".to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            },
            7 => {
                assert_eq!(spec.program, "docker");
                let expected = if self.skip_target_tag_pull {
                    vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"]
                } else {
                    vec!["pull", "ghcr.io/org/web:1.0"]
                };
                assert_eq!(
                    spec.args,
                    expected.into_iter().map(str::to_string).collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            8 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            _ => panic!(
                "unexpected extra command: program={} args={:?}",
                spec.program, spec.args
            ),
        };
        *step += 1;
        Ok(out)
    }
}

#[derive(Default)]
struct SyncBeforeSemverRunner {
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

#[async_trait::async_trait]
impl CommandRunner for SyncBeforeSemverRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls
            .lock()
            .unwrap()
            .push((spec.program.clone(), spec.args.clone()));
        let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
        let out = if spec.program == "docker-compose"
            && args_end_with(&spec.args, &["ps", "-q", "web"])
        {
            CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            }
        } else if spec.program == "docker"
            && args == vec!["inspect", "--format", "{{.Image}}", "new_container"]
        {
            CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            }
        } else if spec.program == "docker"
            && args
                == vec![
                    "inspect",
                    "--format",
                    "{{if .State.Health}}1{{else}}0{{end}}",
                    "new_container",
                ]
        {
            CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            }
        } else {
            CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }
        };
        Ok(out)
    }
}

#[tokio::test]
async fn sync_tag_failure_rolls_back_instead_of_reporting_success() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = SyncTagRollbackRunner::default();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "rolled_back");
    assert_eq!(
        outcome.summary_json["newDigests"]["svc_1"],
        json!("sha256:new")
    );
    assert_eq!(
        outcome.summary_json["finalDigests"]["svc_1"],
        json!("sha256:old")
    );
    assert_eq!(
        outcome.summary_json["failureStep"].as_str(),
        Some("sync_configured_tag")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["trigger"],
        json!("sync_configured_tag")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["toDigests"]["svc_1"],
        json!("sha256:old")
    );
    assert_eq!(*runner.step.lock().unwrap(), 12);
}

#[tokio::test]
async fn explicit_target_digest_still_syncs_tag_based_service() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = ExplicitTargetDigestSyncRunner::default();
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:explicit", &[]);

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(
        outcome.summary_json["targetTagsPulled"],
        json!(["ghcr.io/org/web:1.0"])
    );
    assert_eq!(*runner.step.lock().unwrap(), 9);
}

#[tokio::test]
async fn selected_version_skips_target_tag_pull_but_syncs_local_configured_tag() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let override_root = std::env::temp_dir().join(format!(
        "dockrev-selected-version-override-{}",
        ulid::Ulid::new()
    ));
    std::fs::create_dir_all(&override_root).unwrap();
    let _override_cleanup = TempDirCleanup(override_root.clone());
    let runner = ExplicitTargetDigestSyncRunner {
        skip_target_tag_pull: true,
        ..Default::default()
    };
    let target_digest = format!("sha256:{}", "e".repeat(64));
    let mut targets = explicit_targets("svc_1", "1.0", &target_digest, &[]);
    targets[0].skip_target_tag_pull = true;

    let outcome = run_update_job_with_gate_using_root(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        Some(targets.as_slice()),
        false,
        "ui",
        None,
        None,
        false,
        &[],
        None,
        Some(&override_root),
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(outcome.summary_json["targetTagsPulled"], json!([]));
    assert_eq!(*runner.step.lock().unwrap(), 8);

    let override_path = crate::managed_override::managed_override_path(&override_root, &stack.id);
    let override_path_str = override_path.to_string_lossy().to_string();
    let override_contents = std::fs::read_to_string(&override_path).unwrap();
    assert!(override_contents.contains(&format!("ghcr.io/org/web:1.0@{target_digest}")));
    let commands = runner.calls.lock().unwrap();
    assert!(commands.iter().any(|(program, args)| {
        program == "docker-compose"
            && args.iter().any(|arg| arg == "up")
            && args.iter().any(|arg| arg == &override_path_str)
    }));
}

#[tokio::test]
async fn digest_pinned_service_skips_local_tag_sync() {
    let stack = single_service_stack("ghcr.io/org/web@sha256:old", None);
    let runner = DigestPinnedRunner::default();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(outcome.summary_json["targetTagsPulled"], json!([]));
    assert_eq!(*runner.step.lock().unwrap(), 7);
}

#[tokio::test]
async fn explicit_targets_must_cover_selected_services_at_execution_time() {
    let mut stack = single_service_stack(
        "ghcr.io/org/web:1.0",
        Some(Candidate {
            tag: "1.0".to_string(),
            resolved_tag: Some("1.0".to_string()),
            digest: "sha256:new1".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let mut worker = stack.services[0].clone();
    worker.id = "svc_2".to_string();
    worker.name = "worker".to_string();
    worker.image.reference = "ghcr.io/org/worker:2.0".to_string();
    worker.image.tag = "2.0".to_string();
    worker.candidate = Some(Candidate {
        tag: "2.0".to_string(),
        resolved_tag: Some("2.0".to_string()),
        digest: "sha256:new2".to_string(),
        arch_match: ArchMatch::Match,
        arch: vec!["linux/amd64".to_string()],
    });
    stack.services.push(worker);

    let runner = FakeRunner::default();
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new1", &[]);

    let err = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Stack,
        None,
        "live",
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .expect_err("missing explicit target should fail before executing update commands");

    assert!(
        err.to_string()
            .contains("explicit update targets no longer cover selected services: svc_2")
    );
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn stack_update_pulls_target_tag_before_sync_and_compatibility_tags_afterwards() {
    let stack = single_service_stack(
        "ghcr.io/org/web:1.0",
        Some(Candidate {
            tag: "1.0".to_string(),
            resolved_tag: Some("0.7.7".to_string()),
            digest: "sha256:candidate".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let runner = SyncBeforeSemverRunner::default();
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:candidate", &["v0.7.7"]);

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Stack,
        None,
        "live",
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(
        outcome.summary_json["targetTagsPulled"],
        json!(["ghcr.io/org/web:1.0"])
    );
    assert_eq!(
        outcome.summary_json["pullTagsPulled"],
        json!(["ghcr.io/org/web:v0.7.7"])
    );
    let calls = runner.calls.lock().unwrap();
    let target_idx = calls
        .iter()
        .position(|(program, args)| {
            program == "docker"
                && args == &vec!["pull".to_string(), "ghcr.io/org/web:1.0".to_string()]
        })
        .expect("target tag pull should exist");
    let sync_idx = calls
        .iter()
        .position(|(program, args)| {
            program == "docker"
                && args
                    == &vec![
                        "image".to_string(),
                        "tag".to_string(),
                        "sha256:new".to_string(),
                        "ghcr.io/org/web:1.0".to_string(),
                    ]
        })
        .expect("sync tag command should exist");
    let compat_idx = calls
        .iter()
        .position(|(program, args)| {
            program == "docker"
                && args == &vec!["pull".to_string(), "ghcr.io/org/web:v0.7.7".to_string()]
        })
        .expect("compatibility tag pull should exist");
    assert!(target_idx < sync_idx);
    assert!(sync_idx < compat_idx);
}
