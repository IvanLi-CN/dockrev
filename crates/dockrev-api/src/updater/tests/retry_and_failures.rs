use super::*;

#[test]
fn parse_pull_fraction_supports_size_ratio_tokens() {
    let line = "d2cad1f9f7c9 Downloading [==================> ] 3.146MB/5.89MB";
    let frac = parse_pull_fraction_from_line(line).unwrap();
    assert!(frac > 0.50 && frac < 0.60);

    let full =
        "9b4e5f7f3558 Downloading [==================================================>] 443B/443B";
    let full_frac = parse_pull_fraction_from_line(full).unwrap();
    assert!((full_frac - 1.0).abs() < f64::EPSILON);
}

#[derive(Default)]
struct FlakyInspectRunner {
    calls: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for FlakyInspectRunner {
    async fn run(&self, _spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls < 3 {
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "transient".to_string(),
            })
        } else {
            Ok(CommandOutput {
                status: 0,
                stdout: "ok\n".to_string(),
                stderr: String::new(),
            })
        }
    }
}

#[tokio::test]
async fn run_to_string_with_retry_succeeds_after_transient_failures() {
    let runner = FlakyInspectRunner::default();
    let got = run_to_string_with_retry(
        &runner,
        CommandSpec {
            program: "docker".to_string(),
            args: vec!["inspect".to_string()],
            env: Vec::new(),
        },
        Duration::from_millis(100),
        "inspect_image_id",
        IdempotentRetryPolicy {
            max_attempts: 3,
            base_ms: 1,
            max_ms: 2,
        },
    )
    .await
    .expect("third attempt should succeed");
    assert_eq!(got.trim(), "ok");
    assert_eq!(*runner.calls.lock().unwrap(), 3);
}

#[derive(Default)]
struct FailUpRunner {
    up_calls: Mutex<usize>,
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for FailUpRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "c_before\n".to_string(),
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
            3 => {
                if args_end_with(&spec.args, &["up", "-d", "web"]) {
                    *self.up_calls.lock().unwrap() += 1;
                }
                CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "up failed".to_string(),
                }
            }
            _ => CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "unexpected extra command".to_string(),
            },
        };
        *step += 1;
        Ok(out)
    }
}

#[tokio::test]
async fn up_command_is_not_retried_when_it_fails() {
    let stack = StackRecord {
        id: "stk_1".to_string(),
        name: "App".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![Service {
            id: "svc_1".to_string(),
            name: "web".to_string(),
            image: ComposeRef {
                reference: "ghcr.io/org/web:1.0".to_string(),
                tag: "1.0".to_string(),
                digest: None,
                resolved_tag: None,
                resolved_tags: None,
            },
            candidate: None,
            ignore: None,
            version_inference: None,
            new_version_discovery_count: None,
            settings: ServiceSettings {
                auto_rollback: true,
                backup_targets: BackupTargetOverrides {
                    bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                    volume_names: BTreeMap::<String, TernaryChoice>::new(),
                },
                repo_url: None,
            },
            archived: None,
        }],
    };

    let runner = FailUpRunner::default();
    let err = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 5,
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
    .expect_err("up -d failure should abort immediately without retries");
    assert!(err.to_string().contains("command failed"));
    assert_eq!(*runner.up_calls.lock().unwrap(), 1);
}

#[derive(Default)]
struct CompatibilityTagWarningRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for CompatibilityTagWarningRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container
"
                .to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old
"
                .to_string(),
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
                stdout: "new_container
"
                .to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0
"
                .to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new
"
                .to_string(),
                stderr: String::new(),
            },
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["pull", "ghcr.io/org/web:1.0"]
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
            9 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["pull", "ghcr.io/org/web:v0.7.7"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "compat tag missing".to_string(),
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

#[tokio::test]
async fn compatibility_tag_pull_failures_only_record_warnings() {
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
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new", &["v0.7.7"]);
    let runner = CompatibilityTagWarningRunner::default();

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
    assert_eq!(outcome.summary_json["pullTagsPulled"], json!([]));
    assert_eq!(outcome.summary_json["semverPulled"], json!([]));
    let warnings = outcome.summary_json["pullTagWarnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["tagRef"], json!("ghcr.io/org/web:v0.7.7"));
    assert_eq!(warnings[0]["step"], json!("pull_tag"));
}

#[derive(Default)]
struct TargetTagPullRollbackRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for TargetTagPullRollbackRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container
"
                .to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old
"
                .to_string(),
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
                stdout: "new_container
"
                .to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0
"
                .to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new
"
                .to_string(),
                stderr: String::new(),
            },
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["pull", "ghcr.io/org/web:1.0"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "target tag missing".to_string(),
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
                    &["up", "-d", "--pull", "never", "web"]
                ));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            10 => CommandOutput {
                status: 0,
                stdout: "rollback_container
"
                .to_string(),
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
                    stdout: "sha256:old
"
                    .to_string(),
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

#[tokio::test]
async fn target_tag_pull_failure_rolls_back_with_explicit_failure_step() {
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
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new", &[]);
    let runner = TargetTagPullRollbackRunner::default();

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
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "rolled_back");
    assert_eq!(
        outcome.summary_json["failureStep"],
        json!("pull_target_tag")
    );
    assert_eq!(outcome.summary_json["targetTagsPulled"], json!([]));
    assert_eq!(
        outcome.summary_json["newDigests"]["svc_1"],
        json!("sha256:new")
    );
    assert_eq!(
        outcome.summary_json["finalDigests"]["svc_1"],
        json!("sha256:old")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["trigger"],
        json!("pull_target_tag")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["toDigests"]["svc_1"],
        json!("sha256:old")
    );
}

#[test]
fn strip_tag_and_digest_handles_digest_only_refs() {
    assert_eq!(
        strip_tag_and_digest("alpine@sha256:deadbeef"),
        Some("alpine".to_string())
    );
    assert_eq!(
        strip_tag_and_digest("ghcr.io/org/web@sha256:deadbeef"),
        Some("ghcr.io/org/web".to_string())
    );
}
