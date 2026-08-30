use std::{collections::BTreeMap, path::Path, time::Duration};

use crate::{
    compose,
    compose_runner::{ComposeRunnerConfig, ComposeStack},
    runner::CommandRunner,
};

pub(crate) const COMPOSE_CONFIG_UNRESOLVED: &str = "compose_config_unresolved";
const MAX_EFFECTIVE_CONFIG_STDOUT_BYTES: usize = 4 * 1024 * 1024;
const HOMEPAGE_LABEL_KEYS: [&str; 5] = [
    "homepage.group",
    "homepage.name",
    "homepage.icon",
    "homepage.href",
    "homepage.description",
];

pub(crate) enum PersistedComposeFilesState {
    Stopped {
        compose_files: Vec<String>,
    },
    Missing {
        compose_files: Vec<String>,
    },
    Invalid {
        compose_files: Option<Vec<String>>,
        reason: String,
    },
}

pub(crate) async fn classify_persisted_compose_files(
    compose_files: Option<Vec<String>>,
) -> PersistedComposeFilesState {
    let Some(compose_files) = compose_files else {
        return PersistedComposeFilesState::Invalid {
            compose_files: None,
            reason: "compose_files_not_recorded".to_string(),
        };
    };
    if compose_files.is_empty() {
        return PersistedComposeFilesState::Invalid {
            compose_files: Some(compose_files),
            reason: "compose_files_not_recorded".to_string(),
        };
    }

    let mut missing_files = 0usize;

    for path in &compose_files {
        if path.trim().is_empty() || !Path::new(path).is_absolute() {
            return PersistedComposeFilesState::Invalid {
                compose_files: Some(compose_files.clone()),
                reason: format!("compose_file_path_invalid: {path}"),
            };
        }

        let contents = match tokio::fs::read_to_string(path).await {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing_files = missing_files.saturating_add(1);
                continue;
            }
            Err(error) => {
                return PersistedComposeFilesState::Invalid {
                    compose_files: Some(compose_files.clone()),
                    reason: format!("compose_file_unreadable: {path} ({error})"),
                };
            }
        };

        if let Err(error) = compose::parse_services(&contents) {
            return PersistedComposeFilesState::Invalid {
                compose_files: Some(compose_files.clone()),
                reason: format!("compose_file_invalid: {path} ({error})"),
            };
        }
    }

    if missing_files == compose_files.len() {
        return PersistedComposeFilesState::Missing { compose_files };
    }
    if missing_files > 0 {
        return PersistedComposeFilesState::Invalid {
            compose_files: Some(compose_files),
            reason: "compose_files_partially_missing".to_string(),
        };
    }
    PersistedComposeFilesState::Stopped { compose_files }
}

pub(crate) async fn read_effective_compose_services(
    stack: &ComposeStack,
    runner: &dyn CommandRunner,
    config: &ComposeRunnerConfig,
    timeout: Duration,
) -> Result<BTreeMap<String, crate::compose::ServiceFromCompose>, String> {
    let output = runner
        .run_raw_bounded(
            stack.config_json(config),
            timeout,
            MAX_EFFECTIVE_CONFIG_STDOUT_BYTES,
        )
        .await
        .map_err(|_| COMPOSE_CONFIG_UNRESOLVED.to_string())?;

    if output.status != 0 || output.stdout.len() > MAX_EFFECTIVE_CONFIG_STDOUT_BYTES {
        return Err(COMPOSE_CONFIG_UNRESOLVED.to_string());
    }

    parse_effective_compose_services(&output.stdout)
        .map_err(|_| COMPOSE_CONFIG_UNRESOLVED.to_string())
}

fn parse_effective_compose_services(
    output: &[u8],
) -> anyhow::Result<BTreeMap<String, crate::compose::ServiceFromCompose>> {
    let root: serde_json::Value = serde_json::from_slice(output)?;
    let services = root
        .get("services")
        .and_then(serde_json::Value::as_object)
        .filter(|services| !services.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing or empty services"))?;

    // Compose's JSON output may contain networks, secrets, and rendered environment values. Keep
    // only fields that the existing discovery parser consumes before converting back to its input
    // shape.
    let services = services
        .iter()
        .filter_map(|(name, value)| {
            let service = value.as_object()?;
            let mut filtered = serde_json::Map::new();
            if let Some(image) = service.get("image") {
                filtered.insert("image".to_string(), image.clone());
            }
            if let Some(labels) = service.get("labels").and_then(serde_json::Value::as_object) {
                let filtered_labels = HOMEPAGE_LABEL_KEYS
                    .iter()
                    .filter_map(|key| {
                        labels
                            .get(*key)
                            .map(|value| ((*key).to_string(), value.clone()))
                    })
                    .collect::<serde_json::Map<_, _>>();
                if !filtered_labels.is_empty() {
                    filtered.insert(
                        "labels".to_string(),
                        serde_json::Value::Object(filtered_labels),
                    );
                }
            }
            Some((name.clone(), serde_json::Value::Object(filtered)))
        })
        .collect::<serde_json::Map<_, _>>();
    let services_json = serde_json::to_string(&serde_json::json!({ "services": services }))?;
    let parsed = compose::parse_services(&services_json)?;
    let merged = compose::merge_services(BTreeMap::new(), parsed);
    if merged.is_empty() {
        return Err(anyhow::anyhow!("no observable services"));
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct FakeRunner {
        output: crate::runner::CommandOutput,
    }

    #[async_trait]
    impl CommandRunner for FakeRunner {
        async fn run(
            &self,
            _spec: crate::runner::CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<crate::runner::CommandOutput> {
            Ok(self.output.clone())
        }
    }

    fn stack() -> ComposeStack {
        ComposeStack {
            project_name: "xp".to_string(),
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/xp/docker-compose.yml".to_string()],
                env_file: None,
            },
        }
    }

    fn config() -> ComposeRunnerConfig {
        ComposeRunnerConfig {
            compose_bin: "docker-compose".to_string(),
            env: Vec::new(),
        }
    }

    #[tokio::test]
    async fn effective_config_resolves_interpolated_image() {
        let runner = FakeRunner {
            output: crate::runner::CommandOutput {
                status: 0,
                stdout: r#"{"services":{"xp-101":{"image":"ghcr.io/ivanli-cn/xp:v3.34.8","environment":{"SECRET":"must stay transient"},"labels":{"homepage.href":"https://example.com/xp"}}}}"#
                    .to_string(),
                stderr: "should never be persisted".to_string(),
            },
        };

        let services =
            read_effective_compose_services(&stack(), &runner, &config(), Duration::from_secs(8))
                .await
                .unwrap();
        let service = services.get("xp-101").unwrap();
        assert_eq!(service.image_ref, "ghcr.io/ivanli-cn/xp:v3.34.8");
        assert_eq!(service.image_tag, "v3.34.8");
        assert_eq!(
            service
                .homepage
                .as_ref()
                .and_then(|homepage| homepage.href.as_deref()),
            Some("https://example.com/xp")
        );
    }

    #[tokio::test]
    async fn effective_config_failures_have_stable_reason() {
        for output in [
            crate::runner::CommandOutput {
                status: 1,
                stdout: "secret rendered values".to_string(),
                stderr: "secret error details".to_string(),
            },
            crate::runner::CommandOutput {
                status: 0,
                stdout: "not json".to_string(),
                stderr: String::new(),
            },
        ] {
            let runner = FakeRunner { output };
            let error = read_effective_compose_services(
                &stack(),
                &runner,
                &config(),
                Duration::from_secs(8),
            )
            .await
            .unwrap_err();
            assert_eq!(error, COMPOSE_CONFIG_UNRESOLVED);
        }
    }

    #[tokio::test]
    async fn effective_config_output_limit_has_stable_reason() {
        let runner = FakeRunner {
            output: crate::runner::CommandOutput {
                status: 0,
                stdout: "x".repeat(MAX_EFFECTIVE_CONFIG_STDOUT_BYTES + 1),
                stderr: String::new(),
            },
        };
        let error =
            read_effective_compose_services(&stack(), &runner, &config(), Duration::from_secs(8))
                .await
                .unwrap_err();
        assert_eq!(error, COMPOSE_CONFIG_UNRESOLVED);
    }
}
