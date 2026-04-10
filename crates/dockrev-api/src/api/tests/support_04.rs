#[async_trait::async_trait]
impl CommandRunner for CleanupRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
        let out = match self.mode {
            CleanupRunnerMode::StaleOnSecondScan => {
                if args == vec!["container", "ls", "-aq"] {
                    let generation = self.scan_generation.fetch_add(1, Ordering::SeqCst);
                    if generation == 0 {
                        CommandOutput {
                            status: 0,
                            stdout: "ctr_web_old\n".to_string(),
                            stderr: String::new(),
                        }
                    } else {
                        CommandOutput {
                            status: 0,
                            stdout: "ctr_web_old\nctr_misc\n".to_string(),
                            stderr: String::new(),
                        }
                    }
                } else if args == vec!["inspect", "--size", "--format", "{{json .}}", "ctr_web_old"]
                {
                    CommandOutput {
                        status: 0,
                        stdout: serde_json::json!({
                            "Id": "ctr_web_old",
                            "Name": "/demo-web-old",
                            "Image": "sha256:used-web",
                            "SizeRw": 4096,
                            "Config": {
                                "Image": "ghcr.io/acme/web:5.2",
                                "Labels": {
                                    "com.docker.compose.project": "demo",
                                    "com.docker.compose.service": "web"
                                }
                            },
                            "State": { "Status": "exited" },
                            "Mounts": [],
                            "NetworkSettings": { "Networks": {} }
                        })
                        .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["inspect", "--size", "--format", "{{json .}}", "ctr_misc"] {
                    CommandOutput {
                        status: 0,
                        stdout: serde_json::json!({
                            "Id": "ctr_misc",
                            "Name": "/misc",
                            "Image": "sha256:used-misc",
                            "SizeRw": 512,
                            "Config": {
                                "Image": "alpine:3.18",
                                "Labels": {}
                            },
                            "State": { "Status": "exited" },
                            "Mounts": [],
                            "NetworkSettings": { "Networks": {} }
                        })
                        .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["image", "ls", "-aq", "--no-trunc"] {
                    CommandOutput {
                        status: 0,
                        stdout: "sha256:img-web-unused\n".to_string(),
                        stderr: String::new(),
                    }
                } else if args
                    == vec![
                        "image",
                        "inspect",
                        "--format",
                        "{{json .}}",
                        "sha256:img-web-unused",
                    ]
                {
                    CommandOutput {
                        status: 0,
                        stdout: serde_json::json!({
                            "Id": "sha256:img-web-unused",
                            "RepoTags": ["ghcr.io/acme/web:5.1"],
                            "RepoDigests": [],
                            "Size": 2048,
                            "Config": { "Labels": {} }
                        })
                        .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["volume", "ls", "-q"] || args == vec!["network", "ls", "-q"]
                {
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du", "--format=json"] {
                    CommandOutput {
                        status: 0,
                        stdout: r#"{"Reclaimable":true,"Shared":false,"Size":"2147483648"}"#
                            .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du"] {
                    CommandOutput {
                        status: 0,
                        stdout: "Reclaimable:  2.0GB\nTotal:  2.0GB\n".to_string(),
                        stderr: String::new(),
                    }
                } else {
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: format!("unexpected cleanup stale args: {:?}", args),
                    }
                }
            }
            CleanupRunnerMode::VolumeInUse => {
                if args == vec!["container", "ls", "-aq"]
                    || args == vec!["image", "ls", "-aq", "--no-trunc"]
                    || args == vec!["network", "ls", "-q"]
                {
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                } else if args == vec!["volume", "ls", "-q"] {
                    CommandOutput {
                        status: 0,
                        stdout: "demo_named\n".to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["volume", "inspect", "--format", "{{json .}}", "demo_named"]
                {
                    CommandOutput {
                        status: 0,
                        stdout: serde_json::json!({
                            "Name": "demo_named",
                            "Labels": {
                                "com.docker.compose.project": "demo"
                            },
                            "UsageData": {
                                "Size": 8192
                            }
                        })
                        .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du", "--format=json"] {
                    CommandOutput {
                        status: 0,
                        stdout: r#"{"Reclaimable":true,"Shared":false,"Size":"268435456"}"#
                            .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["system", "df", "-v"] {
                    CommandOutput {
                        status: 0,
                        stdout: r#"Images space usage:
REPOSITORY          TAG                 IMAGE ID            CREATED             SIZE                SHARED SIZE         UNIQUE SIZE         CONTAINERS
Local Volumes space usage:
NAME                LINKS               SIZE
demo_named          1                   8 KB
"#
                        .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du"] {
                    CommandOutput {
                        status: 0,
                        stdout: "Reclaimable:  256MB\nTotal:  256MB\n".to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["volume", "rm", "demo_named"] {
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: "Error response from daemon: remove demo_named: volume is in use"
                            .to_string(),
                    }
                } else {
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: format!("unexpected cleanup volume args: {:?}", args),
                    }
                }
            }
            CleanupRunnerMode::VolumeEstimateFallback => {
                if args == vec!["container", "ls", "-aq"]
                    || args == vec!["image", "ls", "-aq", "--no-trunc"]
                    || args == vec!["network", "ls", "-q"]
                {
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                } else if args == vec!["volume", "ls", "-q"] {
                    CommandOutput {
                        status: 0,
                        stdout: "demo_named\n".to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["volume", "inspect", "--format", "{{json .}}", "demo_named"]
                {
                    CommandOutput {
                        status: 0,
                        stdout: serde_json::json!({
                            "Name": "demo_named",
                            "Labels": {
                                "com.docker.compose.project": "demo",
                                "com.docker.compose.service": "web"
                            }
                        })
                        .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["system", "df", "-v"] {
                    CommandOutput {
                        status: 0,
                        stdout: r#"Images space usage:
REPOSITORY          TAG                 IMAGE ID            CREATED             SIZE                SHARED SIZE         UNIQUE SIZE         CONTAINERS
Local Volumes space usage:
NAME                LINKS               SIZE
demo_named          0                   128 MB
"#
                        .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du", "--format=json"] {
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                } else {
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: format!("unexpected cleanup volume fallback args: {:?}", args),
                    }
                }
            }
            CleanupRunnerMode::BuilderCacheTextFallback => {
                if args == vec!["container", "ls", "-aq"]
                    || args == vec!["image", "ls", "-aq", "--no-trunc"]
                    || args == vec!["network", "ls", "-q"]
                    || args == vec!["volume", "ls", "-q"]
                {
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du", "--format=json"] {
                    CommandOutput {
                        status: 0,
                        stdout: "{not-json}\n".to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du"] {
                    CommandOutput {
                        status: 0,
                        stdout: "Reclaimable:  384MB\nTotal:  512MB\n".to_string(),
                        stderr: String::new(),
                    }
                } else {
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: format!(
                            "unexpected cleanup builder text fallback args: {:?}",
                            args
                        ),
                    }
                }
            }
            CleanupRunnerMode::BuilderCacheSharedLowerBound => {
                if args == vec!["container", "ls", "-aq"]
                    || args == vec!["image", "ls", "-aq", "--no-trunc"]
                    || args == vec!["network", "ls", "-q"]
                    || args == vec!["volume", "ls", "-q"]
                {
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du", "--format=json"] {
                    CommandOutput {
                        status: 0,
                        stdout: r#"{"Reclaimable":true,"Shared":false,"Size":"256MB"}
{"Reclaimable":true,"Shared":true,"Size":"128MB"}"#
                            .to_string(),
                        stderr: String::new(),
                    }
                } else if args == vec!["buildx", "du"] {
                    CommandOutput {
                        status: 0,
                        stdout: "Reclaimable:  384MB\nTotal:  512MB\n".to_string(),
                        stderr: String::new(),
                    }
                } else {
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: format!("unexpected cleanup builder fallback args: {:?}", args),
                    }
                }
            }
        };
        Ok(out)
    }
}

async fn seed_cleanup_stack(
    state: &Arc<AppState>,
    project: &str,
    compose_body: &str,
) -> (String, String, String) {
    let compose_path = format!("/tmp/dockrev-cleanup-test-{}.yml", ulid::Ulid::new());
    std::fs::write(&compose_path, compose_body).unwrap();
    let stack_id = seed_stack_from_compose(state, project, &compose_path).await;
    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let service_id = stack.services.first().unwrap().id.clone();
    let now = test_now_rfc3339();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: project.to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();
    (stack_id, service_id, compose_path)
}

