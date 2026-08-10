use std::time::Duration;

use crate::{
    config::Config,
    error::ApiError,
    runner::{CommandRunner, CommandSpec},
};

pub const COMPOSE_V2_REQUIRED_REASON: &str = "compose_v2_required";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposeCapability {
    Supported { major: u64, evidence: String },
    Unsupported { evidence: String },
    Unavailable { evidence: String },
}

impl ComposeCapability {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported { .. })
    }

    pub fn evidence(&self) -> &str {
        match self {
            Self::Supported { evidence, .. }
            | Self::Unsupported { evidence }
            | Self::Unavailable { evidence } => evidence,
        }
    }
}

pub fn uses_docker_subcommand(compose_bin: &str) -> bool {
    let bin = compose_bin.to_ascii_lowercase();
    bin == "docker" || bin.ends_with("/docker") || bin.ends_with("\\docker")
}

pub fn version_command(compose_bin: &str) -> Vec<String> {
    if uses_docker_subcommand(compose_bin) {
        vec!["compose".to_string(), "version".to_string()]
    } else {
        vec!["version".to_string()]
    }
}

pub fn parse_major_version(output: &str) -> Option<u64> {
    for token in output.split_whitespace() {
        let candidate =
            token.trim_matches(|character: char| !character.is_ascii_digit() && character != '.');
        if candidate.is_empty()
            || !candidate
                .chars()
                .any(|character| character.is_ascii_digit())
        {
            continue;
        }
        let major = candidate.split('.').next()?.parse::<u64>().ok()?;
        if candidate.contains('.') || token.starts_with('v') || token.starts_with('V') {
            return Some(major);
        }
    }
    None
}

pub async fn probe(
    runner: &dyn CommandRunner,
    config: &Config,
) -> anyhow::Result<ComposeCapability> {
    let spec = CommandSpec {
        program: config.compose_bin.clone(),
        args: version_command(&config.compose_bin),
        env: Vec::new(),
    };
    let timeout = Duration::from_secs(config.deploy_check_local_command_timeout_seconds);
    let output = runner.run(spec, timeout).await?;
    Ok(classify_version_output(
        output.status,
        &output.stdout,
        &output.stderr,
    ))
}

pub fn classify_version_output(status: i32, stdout: &str, stderr: &str) -> ComposeCapability {
    let evidence = [stdout.trim(), stderr.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    let evidence = if evidence.is_empty() {
        format!("command exit status {status}")
    } else {
        evidence
    };
    if status != 0 {
        return ComposeCapability::Unavailable { evidence };
    }
    let Some(major) = parse_major_version(&evidence) else {
        return ComposeCapability::Unavailable { evidence };
    };
    if major < 2 {
        ComposeCapability::Unsupported { evidence }
    } else {
        ComposeCapability::Supported { major, evidence }
    }
}

pub async fn require_v2(runner: &dyn CommandRunner, config: &Config) -> anyhow::Result<()> {
    let capability = probe(runner, config).await?;
    if capability.is_supported() {
        Ok(())
    } else {
        anyhow::bail!("{COMPOSE_V2_REQUIRED_REASON}: {}", capability.evidence())
    }
}

pub async fn require_v2_api(runner: &dyn CommandRunner, config: &Config) -> Result<(), ApiError> {
    match probe(runner, config).await {
        Ok(capability) if capability.is_supported() => Ok(()),
        Ok(capability) => Err(ApiError::compose_v2_required(
            &config.compose_bin,
            capability.evidence(),
        )),
        Err(error) => Err(ApiError::compose_v2_required(
            &config.compose_bin,
            &error.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plugin_and_standalone_versions() {
        assert_eq!(
            parse_major_version("Docker Compose version v2.40.0"),
            Some(2)
        );
        assert_eq!(
            parse_major_version("docker-compose version 1.29.2, build 5becea4c"),
            Some(1)
        );
    }

    #[test]
    fn rejects_unparseable_version_output() {
        assert_eq!(parse_major_version("Docker Compose version unknown"), None);
        assert_eq!(parse_major_version("command completed"), None);
    }

    #[test]
    fn classifies_v1_and_probe_failures_as_unavailable_capability() {
        assert!(matches!(
            classify_version_output(0, "docker-compose version 1.29.2", ""),
            ComposeCapability::Unsupported { .. }
        ));
        assert!(matches!(
            classify_version_output(0, "Docker Compose version unknown", ""),
            ComposeCapability::Unavailable { .. }
        ));
        assert!(matches!(
            classify_version_output(127, "", "command not found"),
            ComposeCapability::Unavailable { .. }
        ));
    }

    #[test]
    fn selects_correct_version_command() {
        assert_eq!(version_command("docker"), ["compose", "version"]);
        assert_eq!(
            version_command("/usr/local/bin/docker"),
            ["compose", "version"]
        );
        assert_eq!(version_command("docker-compose"), ["version"]);
    }
}
