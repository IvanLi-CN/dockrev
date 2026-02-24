use std::{collections::BTreeSet, fs, net::Ipv4Addr, path::Path, time::Duration};

use anyhow::Context as _;
use serde_json::Value;
use url::Url;

use crate::{
    api::types::{
        DeployCheckGroup, DeployCheckItem, DeployCheckNaReason, DeployCheckOverall,
        DeployCheckReportResponse, DeployCheckResult, DeployCheckStatus,
    },
    db::ArchivedFilter,
    registry,
    runner::CommandSpec,
    state::AppState,
};

const LOCAL_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn build_report(state: &AppState) -> anyhow::Result<DeployCheckReportResponse> {
    let context = collect_context(state).await?;

    let mut checks = Vec::new();
    checks.push(check_docker_engine(state).await);
    checks.push(check_compose_access(&context));
    checks.push(check_service_image_ref_valid(&context));
    checks.push(check_update_executor_ready(state).await);

    checks.push(check_registry_auth(state, &context));
    checks.extend(check_notification_features(state).await?);
    checks.push(check_github_packages_feature(state).await?);

    let blocking_check_ids: Vec<String> = checks
        .iter()
        .filter(|item| item.required && item.status == DeployCheckStatus::Fail)
        .map(|item| item.id.clone())
        .collect();

    let overall = if blocking_check_ids.is_empty() {
        DeployCheckOverall {
            result: DeployCheckResult::Pass,
            blocking_check_ids,
            summary: "All required capabilities are available".to_string(),
        }
    } else {
        DeployCheckOverall {
            result: DeployCheckResult::Fail,
            blocking_check_ids,
            summary: "At least one required capability is unavailable".to_string(),
        }
    };

    Ok(DeployCheckReportResponse {
        overall,
        generated_at: now_rfc3339()?,
        checks,
    })
}

#[derive(Default)]
struct PreflightContext {
    compose_paths: Vec<String>,
    active_services_total: usize,
    invalid_image_refs: Vec<String>,
    parsed_images: Vec<registry::ImageRef>,
}

async fn collect_context(state: &AppState) -> anyhow::Result<PreflightContext> {
    let stack_ids = state.db.list_stack_ids().await?;
    let mut compose_paths = BTreeSet::<String>::new();
    let mut invalid_image_refs = Vec::<String>::new();
    let mut parsed_images = Vec::<registry::ImageRef>::new();
    let mut active_services_total = 0usize;

    for stack_id in stack_ids {
        let Some(stack) = state.db.get_stack(&stack_id).await? else {
            continue;
        };
        if stack.archived {
            continue;
        }

        for file in stack.compose.compose_files {
            if !file.trim().is_empty() {
                compose_paths.insert(file);
            }
        }
        if let Some(env_file) = stack.compose.env_file
            && !env_file.trim().is_empty()
        {
            compose_paths.insert(env_file);
        }

        for service in stack.services {
            if service.archived.unwrap_or(false) {
                continue;
            }
            active_services_total += 1;
            match registry::ImageRef::parse(&service.image.reference) {
                Ok(parsed) => parsed_images.push(parsed),
                Err(err) => {
                    invalid_image_refs.push(format!("{}/{} ({})", stack.name, service.name, err))
                }
            }
        }
    }

    // Also include the latest discovered compose files when stacks are not yet created.
    let discovered = state
        .db
        .list_discovered_compose_projects(ArchivedFilter::Exclude)
        .await
        .unwrap_or_default();
    for project in discovered {
        if let Some(files) = project.config_files {
            for file in files {
                if !file.trim().is_empty() {
                    compose_paths.insert(file);
                }
            }
        }
    }

    Ok(PreflightContext {
        compose_paths: compose_paths.into_iter().collect(),
        active_services_total,
        invalid_image_refs,
        parsed_images,
    })
}

async fn check_docker_engine(state: &AppState) -> DeployCheckItem {
    let spec = CommandSpec {
        program: "docker".to_string(),
        args: vec![
            "info".to_string(),
            "--format".to_string(),
            "{{.ServerVersion}}".to_string(),
        ],
        env: Vec::new(),
    };

    match state.runner.run(spec, LOCAL_COMMAND_TIMEOUT).await {
        Ok(output) if output.status == 0 => {
            let version = output.stdout.trim();
            let evidence = if version.is_empty() {
                "docker info ok".to_string()
            } else {
                format!("docker info ok (server {version})")
            };
            pass_core(
                "core.docker_engine",
                "Docker 引擎可用",
                "docker daemon reachable",
                "不可用时无法执行更新与运行时检查",
                evidence,
            )
        }
        Ok(output) => fail_core(
            "core.docker_engine",
            "Docker 引擎可用",
            "docker daemon unreachable",
            "不可用时无法执行更新与运行时检查",
            summarize_command_failure(output.status, &output.stderr),
            "检查 Docker 服务是否运行，并确认 dockrev 进程有权访问 Docker socket",
        ),
        Err(err) => fail_core(
            "core.docker_engine",
            "Docker 引擎可用",
            "docker daemon probe failed",
            "不可用时无法执行更新与运行时检查",
            err.to_string(),
            "检查 Docker 命令可执行路径与 socket 挂载",
        ),
    }
}

fn check_compose_access(context: &PreflightContext) -> DeployCheckItem {
    if context.compose_paths.is_empty() {
        return fail_core(
            "core.compose_access",
            "Compose 配置可访问",
            "no compose files discovered",
            "服务解析不完整，更新目标不可信",
            "未发现任何 compose 路径（active stack / discovered project）".to_string(),
            "先完成 compose 项目发现，确认服务已被 dockrev 管理",
        );
    }

    let mut non_absolute = Vec::new();
    let mut unreadable = Vec::new();

    for path in &context.compose_paths {
        let p = Path::new(path);
        if !p.is_absolute() {
            non_absolute.push(path.clone());
            continue;
        }
        if fs::metadata(p)
            .and_then(|m| {
                if m.is_file() {
                    Ok(())
                } else {
                    Err(std::io::Error::other("not a file"))
                }
            })
            .is_err()
        {
            unreadable.push(path.clone());
        }
    }

    if non_absolute.is_empty() && unreadable.is_empty() {
        return pass_core(
            "core.compose_access",
            "Compose 配置可访问",
            "compose paths are absolute and readable",
            "服务解析不完整，更新目标不可信",
            format!("validated {} path(s)", context.compose_paths.len()),
        );
    }

    let mut reasons = Vec::new();
    if !non_absolute.is_empty() {
        reasons.push(format!("non-absolute: {}", join_limited(&non_absolute, 4)));
    }
    if !unreadable.is_empty() {
        reasons.push(format!("unreadable: {}", join_limited(&unreadable, 4)));
    }

    fail_core(
        "core.compose_access",
        "Compose 配置可访问",
        "compose path validation failed",
        "服务解析不完整，更新目标不可信",
        reasons.join("; "),
        "确保 compose / env 文件使用绝对路径并且在容器内可读",
    )
}

fn check_service_image_ref_valid(context: &PreflightContext) -> DeployCheckItem {
    if context.active_services_total == 0 {
        return pass_core(
            "core.service_image_ref_valid",
            "服务镜像引用可解析",
            "no active services found; nothing to validate",
            "不可解析时对应服务会被跳过，导致功能不完整",
            "active services: 0".to_string(),
        );
    }

    if context.invalid_image_refs.is_empty() {
        return pass_core(
            "core.service_image_ref_valid",
            "服务镜像引用可解析",
            "all managed service image refs are valid",
            "不可解析时对应服务会被跳过，导致功能不完整",
            format!(
                "validated {} active service(s)",
                context.active_services_total
            ),
        );
    }

    fail_core(
        "core.service_image_ref_valid",
        "服务镜像引用可解析",
        "invalid image ref found",
        "不可解析时对应服务会被跳过，导致功能不完整",
        join_limited(&context.invalid_image_refs, 4),
        "修复 compose 里的 image 字段，使用 repo/name:tag 格式",
    )
}

async fn check_update_executor_ready(state: &AppState) -> DeployCheckItem {
    let args = compose_version_args(&state.config.compose_bin);
    let spec = CommandSpec {
        program: state.config.compose_bin.clone(),
        args,
        env: Vec::new(),
    };

    match state.runner.run(spec, LOCAL_COMMAND_TIMEOUT).await {
        Ok(output) if output.status == 0 => {
            let stdout = output.stdout.trim();
            let evidence = if stdout.is_empty() {
                format!("{} version ok", state.config.compose_bin)
            } else {
                first_line(stdout)
            };
            pass_core(
                "core.update_executor_ready",
                "更新执行器可用",
                "compose executor is callable",
                "发现到更新也无法执行 pull/up",
                evidence,
            )
        }
        Ok(output) => fail_core(
            "core.update_executor_ready",
            "更新执行器可用",
            "compose executor is not ready",
            "发现到更新也无法执行 pull/up",
            summarize_command_failure(output.status, &output.stderr),
            "确认 DOCKREV_COMPOSE_BIN 指向可执行命令（docker 或 docker-compose）",
        ),
        Err(err) => fail_core(
            "core.update_executor_ready",
            "更新执行器可用",
            "compose executor probe failed",
            "发现到更新也无法执行 pull/up",
            err.to_string(),
            "确认 DOCKREV_COMPOSE_BIN 可执行且包含 compose version 子命令",
        ),
    }
}

fn check_registry_auth(state: &AppState, context: &PreflightContext) -> DeployCheckItem {
    let mut required_hosts = BTreeSet::<String>::new();
    for image in &context.parsed_images {
        if likely_requires_registry_auth(image) {
            required_hosts.insert(normalize_registry_host(&image.registry));
        }
    }

    if required_hosts.is_empty() {
        let na_reason = if context.compose_paths.is_empty() {
            DeployCheckNaReason::MissingPrerequisite
        } else {
            DeployCheckNaReason::NotApplicable
        };
        return na_feature(
            "feature.registry_auth",
            "私有镜像仓库鉴权配置",
            "no deterministic private-registry target detected",
            "该功能未启用；不纳入阻塞判定",
            "targets: none",
            na_reason,
        );
    }

    let Some(path) = state.config.docker_config_path.clone() else {
        return fail_feature(
            "feature.registry_auth",
            "私有镜像仓库鉴权配置",
            "private registry auth is required but DOCKREV_DOCKER_CONFIG is missing",
            "对应服务可能出现 401，无法发现候选更新",
            format!("required hosts: {}", join_limited_set(&required_hosts, 6)),
            "设置 DOCKREV_DOCKER_CONFIG 并提供对应 registry 凭据",
        );
    };

    let auth = match load_docker_auth_inventory(&path) {
        Ok(v) => v,
        Err(err) => {
            return fail_feature(
                "feature.registry_auth",
                "私有镜像仓库鉴权配置",
                "failed to parse docker auth config",
                "对应服务可能出现 401，无法发现候选更新",
                format!("{}: {}", path.display(), err),
                "检查 DOCKREV_DOCKER_CONFIG 路径和 JSON 内容",
            );
        }
    };

    let mut missing_hosts = Vec::new();
    for host in &required_hosts {
        if auth.has_global_creds_store
            || auth.auth_hosts.contains(host)
            || auth.cred_helper_hosts.contains(host)
        {
            continue;
        }
        missing_hosts.push(host.clone());
    }

    if missing_hosts.is_empty() {
        return pass_feature(
            "feature.registry_auth",
            "私有镜像仓库鉴权配置",
            "required registry credentials are configured",
            "对应服务可能出现 401，无法发现候选更新",
            format!(
                "required hosts: {}; config: {}",
                join_limited_set(&required_hosts, 6),
                path.display()
            ),
        );
    }

    fail_feature(
        "feature.registry_auth",
        "私有镜像仓库鉴权配置",
        "missing credential entries for required registry hosts",
        "对应服务可能出现 401，无法发现候选更新",
        format!(
            "missing hosts: {}; config: {}",
            join_limited(&missing_hosts, 6),
            path.display()
        ),
        "在 Docker config 的 auths/credHelpers 中补齐对应 registry host 的凭据",
    )
}

async fn check_notification_features(state: &AppState) -> anyhow::Result<Vec<DeployCheckItem>> {
    let settings = state.db.get_notification_settings().await?;
    let mut checks = Vec::new();

    checks.push(if !settings.email_enabled {
        na_feature(
            "feature.notifications.email",
            "通知能力：Email",
            "email notification is disabled",
            "该功能未启用；不纳入阻塞判定",
            "enabled=false",
            DeployCheckNaReason::DisabledBySwitch,
        )
    } else if is_non_empty(settings.email_smtp_url.as_deref()) {
        pass_feature(
            "feature.notifications.email",
            "通知能力：Email",
            "email notification config is complete",
            "启用后若配置缺失，邮件通知不可用",
            "smtpUrl configured",
        )
    } else {
        fail_feature(
            "feature.notifications.email",
            "通知能力：Email",
            "email notification config is incomplete",
            "启用后若配置缺失，邮件通知不可用",
            "missing smtpUrl",
            "补齐通知设置中的 SMTP URL",
        )
    });

    checks.push(if !settings.webhook_enabled {
        na_feature(
            "feature.notifications.webhook",
            "通知能力：Webhook",
            "webhook notification is disabled",
            "该功能未启用；不纳入阻塞判定",
            "enabled=false",
            DeployCheckNaReason::DisabledBySwitch,
        )
    } else if let Some(url) = settings.webhook_url.as_deref() {
        match Url::parse(url) {
            Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => pass_feature(
                "feature.notifications.webhook",
                "通知能力：Webhook",
                "webhook notification config is complete",
                "启用后若配置缺失，Webhook 通知不可用",
                "webhook URL configured",
            ),
            Ok(parsed) => fail_feature(
                "feature.notifications.webhook",
                "通知能力：Webhook",
                "webhook URL scheme is not supported",
                "启用后若配置缺失，Webhook 通知不可用",
                format!("unsupported scheme: {}", parsed.scheme()),
                "提供合法的 webhook URL（http/https）",
            ),
            Err(_) => fail_feature(
                "feature.notifications.webhook",
                "通知能力：Webhook",
                "webhook URL is invalid",
                "启用后若配置缺失，Webhook 通知不可用",
                "invalid webhook URL",
                "提供合法的 webhook URL（http/https）",
            ),
        }
    } else {
        fail_feature(
            "feature.notifications.webhook",
            "通知能力：Webhook",
            "webhook notification config is incomplete",
            "启用后若配置缺失，Webhook 通知不可用",
            "missing webhook URL",
            "补齐通知设置中的 webhook URL",
        )
    });

    checks.push(if !settings.telegram_enabled {
        na_feature(
            "feature.notifications.telegram",
            "通知能力：Telegram",
            "telegram notification is disabled",
            "该功能未启用；不纳入阻塞判定",
            "enabled=false",
            DeployCheckNaReason::DisabledBySwitch,
        )
    } else {
        let has_token = is_non_empty(settings.telegram_bot_token.as_deref());
        let has_chat = is_non_empty(settings.telegram_chat_id.as_deref());
        if has_token && has_chat {
            pass_feature(
                "feature.notifications.telegram",
                "通知能力：Telegram",
                "telegram notification config is complete",
                "启用后若配置缺失，Telegram 通知不可用",
                "botToken + chatId configured",
            )
        } else {
            let mut missing = Vec::new();
            if !has_token {
                missing.push("botToken");
            }
            if !has_chat {
                missing.push("chatId");
            }
            fail_feature(
                "feature.notifications.telegram",
                "通知能力：Telegram",
                "telegram notification config is incomplete",
                "启用后若配置缺失，Telegram 通知不可用",
                format!("missing {}", missing.join(", ")),
                "补齐 Telegram botToken 与 chatId",
            )
        }
    });

    checks.push(if !settings.webpush_enabled {
        na_feature(
            "feature.notifications.web_push",
            "通知能力：Web Push",
            "web push notification is disabled",
            "该功能未启用；不纳入阻塞判定",
            "enabled=false",
            DeployCheckNaReason::DisabledBySwitch,
        )
    } else {
        let has_pub = is_non_empty(settings.webpush_vapid_public_key.as_deref());
        let has_priv = is_non_empty(settings.webpush_vapid_private_key.as_deref());
        let has_subject = is_non_empty(settings.webpush_vapid_subject.as_deref());
        if has_pub && has_priv && has_subject {
            pass_feature(
                "feature.notifications.web_push",
                "通知能力：Web Push",
                "web push notification config is complete",
                "启用后若配置缺失，Web Push 通知不可用",
                "vapidPublicKey + vapidPrivateKey + vapidSubject configured",
            )
        } else {
            let mut missing = Vec::new();
            if !has_pub {
                missing.push("vapidPublicKey");
            }
            if !has_priv {
                missing.push("vapidPrivateKey");
            }
            if !has_subject {
                missing.push("vapidSubject");
            }
            fail_feature(
                "feature.notifications.web_push",
                "通知能力：Web Push",
                "web push notification config is incomplete",
                "启用后若配置缺失，Web Push 通知不可用",
                format!("missing {}", missing.join(", ")),
                "补齐 Web Push VAPID 配置",
            )
        }
    });

    Ok(checks)
}

async fn check_github_packages_feature(state: &AppState) -> anyhow::Result<DeployCheckItem> {
    let settings = state.db.get_github_packages_settings().await?;

    if !settings.enabled {
        return Ok(na_feature(
            "feature.github_packages",
            "GitHub Packages 功能配置",
            "github packages integration is disabled",
            "该功能未启用；不纳入阻塞判定",
            "enabled=false",
            DeployCheckNaReason::DisabledBySwitch,
        ));
    }

    let mut missing = Vec::new();
    if !is_non_empty(settings.pat.as_deref()) {
        missing.push("pat");
    }
    if !is_non_empty(Some(settings.callback_url.as_str())) {
        missing.push("callbackUrl");
    } else if Url::parse(settings.callback_url.as_str()).is_err() {
        missing.push("callbackUrl(invalid)");
    }
    if !is_non_empty(settings.webhook_secret.as_deref()) {
        missing.push("secret");
    }

    if missing.is_empty() {
        Ok(pass_feature(
            "feature.github_packages",
            "GitHub Packages 功能配置",
            "github packages integration config is complete",
            "启用后若配置缺失，包发布触发链路不可用",
            "pat + callbackUrl + secret configured",
        ))
    } else {
        Ok(fail_feature(
            "feature.github_packages",
            "GitHub Packages 功能配置",
            "github packages integration config is incomplete",
            "启用后若配置缺失，包发布触发链路不可用",
            format!("missing {}", missing.join(", ")),
            "补齐 GitHub Packages 设置中的 PAT、回调地址与 secret",
        ))
    }
}

fn pass_core(
    id: &str,
    title: &str,
    summary: &str,
    impact: &str,
    evidence: String,
) -> DeployCheckItem {
    DeployCheckItem {
        id: id.to_string(),
        title: title.to_string(),
        group: DeployCheckGroup::Core,
        required: true,
        status: DeployCheckStatus::Pass,
        na_reason: None,
        summary: summary.to_string(),
        impact: impact.to_string(),
        evidence,
        recommendation: String::new(),
    }
}

fn fail_core(
    id: &str,
    title: &str,
    summary: &str,
    impact: &str,
    evidence: String,
    recommendation: &str,
) -> DeployCheckItem {
    DeployCheckItem {
        id: id.to_string(),
        title: title.to_string(),
        group: DeployCheckGroup::Core,
        required: true,
        status: DeployCheckStatus::Fail,
        na_reason: None,
        summary: summary.to_string(),
        impact: impact.to_string(),
        evidence,
        recommendation: recommendation.to_string(),
    }
}

fn pass_feature(
    id: &str,
    title: &str,
    summary: &str,
    impact: &str,
    evidence: impl Into<String>,
) -> DeployCheckItem {
    DeployCheckItem {
        id: id.to_string(),
        title: title.to_string(),
        group: DeployCheckGroup::Feature,
        required: true,
        status: DeployCheckStatus::Pass,
        na_reason: None,
        summary: summary.to_string(),
        impact: impact.to_string(),
        evidence: evidence.into(),
        recommendation: String::new(),
    }
}

fn fail_feature(
    id: &str,
    title: &str,
    summary: &str,
    impact: &str,
    evidence: impl Into<String>,
    recommendation: &str,
) -> DeployCheckItem {
    DeployCheckItem {
        id: id.to_string(),
        title: title.to_string(),
        group: DeployCheckGroup::Feature,
        required: true,
        status: DeployCheckStatus::Fail,
        na_reason: None,
        summary: summary.to_string(),
        impact: impact.to_string(),
        evidence: evidence.into(),
        recommendation: recommendation.to_string(),
    }
}

fn na_feature(
    id: &str,
    title: &str,
    summary: &str,
    impact: &str,
    evidence: &str,
    na_reason: DeployCheckNaReason,
) -> DeployCheckItem {
    DeployCheckItem {
        id: id.to_string(),
        title: title.to_string(),
        group: DeployCheckGroup::Feature,
        required: false,
        status: DeployCheckStatus::Na,
        na_reason: Some(na_reason),
        summary: summary.to_string(),
        impact: impact.to_string(),
        evidence: evidence.to_string(),
        recommendation: String::new(),
    }
}

fn is_non_empty(input: Option<&str>) -> bool {
    input.map(|v| !v.trim().is_empty()).unwrap_or(false)
}

fn compose_version_args(compose_bin: &str) -> Vec<String> {
    if is_docker_plugin(compose_bin) {
        vec!["compose".to_string(), "version".to_string()]
    } else {
        vec!["version".to_string()]
    }
}

fn is_docker_plugin(compose_bin: &str) -> bool {
    let lower = compose_bin.to_ascii_lowercase();
    lower == "docker" || lower.ends_with("/docker") || lower.ends_with("\\docker")
}

fn summarize_command_failure(status: i32, stderr: &str) -> String {
    let detail = stderr.trim();
    if detail.is_empty() {
        format!("command exit status {status}")
    } else {
        format!("command exit status {status}: {}", first_line(detail))
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).trim().to_string()
}

fn join_limited(items: &[String], max_items: usize) -> String {
    if items.is_empty() {
        return String::new();
    }
    let shown = items.iter().take(max_items).cloned().collect::<Vec<_>>();
    if items.len() > max_items {
        format!("{} (+{} more)", shown.join(", "), items.len() - max_items)
    } else {
        shown.join(", ")
    }
}

fn join_limited_set(items: &BTreeSet<String>, max_items: usize) -> String {
    let list = items.iter().cloned().collect::<Vec<_>>();
    join_limited(&list, max_items)
}

#[derive(Default)]
struct DockerAuthInventory {
    auth_hosts: BTreeSet<String>,
    cred_helper_hosts: BTreeSet<String>,
    has_global_creds_store: bool,
}

fn load_docker_auth_inventory(path: &Path) -> anyhow::Result<DockerAuthInventory> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read docker config {}", path.display()))?;
    let value: Value = serde_json::from_str(&text).context("parse docker config JSON")?;
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("docker config root must be an object"))?;

    let mut inventory = DockerAuthInventory::default();

    if let Some(creds_store) = obj.get("credsStore").and_then(|v| v.as_str())
        && !creds_store.trim().is_empty()
    {
        inventory.has_global_creds_store = true;
    }

    if let Some(auths) = obj.get("auths").and_then(|v| v.as_object()) {
        for (key, entry) in auths {
            let host = normalize_registry_host(key);
            let Some(entry_obj) = entry.as_object() else {
                continue;
            };
            let has_auth = entry_obj
                .get("auth")
                .and_then(|v| v.as_str())
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            let has_identity = entry_obj
                .get("identitytoken")
                .and_then(|v| v.as_str())
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if has_auth || has_identity {
                inventory.auth_hosts.insert(host);
            }
        }
    }

    if let Some(helpers) = obj.get("credHelpers").and_then(|v| v.as_object()) {
        for (key, helper) in helpers {
            if helper
                .as_str()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            {
                inventory
                    .cred_helper_hosts
                    .insert(normalize_registry_host(key));
            }
        }
    }

    Ok(inventory)
}

fn likely_requires_registry_auth(image: &registry::ImageRef) -> bool {
    let host = normalize_registry_host(&image.registry);
    if host == "docker.io" {
        let name = image.name.to_ascii_lowercase();
        return name.starts_with("local/")
            || name.starts_with("private/")
            || name.starts_with("internal/");
    }

    // Public registries should not force auth for minimal deployment.
    if matches!(
        host.as_str(),
        "ghcr.io" | "quay.io" | "gcr.io" | "public.ecr.aws" | "mcr.microsoft.com"
    ) || host.ends_with(".gcr.io")
    {
        return false;
    }

    is_private_registry_host(&host)
}

fn normalize_registry_host(input: &str) -> String {
    if let Ok(url) = Url::parse(input)
        && let Some(host) = url.host_str()
    {
        return normalize_registry_host(host);
    }

    let mut host = input
        .trim()
        .trim_end_matches('/')
        .trim_end_matches("/v1/")
        .trim_end_matches("/v2/")
        .trim_end_matches("/v1")
        .trim_end_matches("/v2")
        .to_ascii_lowercase();

    if host == "index.docker.io" || host == "registry-1.docker.io" {
        host = "docker.io".to_string();
    }

    host
}

fn is_private_registry_host(host: &str) -> bool {
    let host_no_port = split_host_port(host);
    if host_no_port == "localhost"
        || host_no_port.ends_with(".local")
        || host_no_port.ends_with(".internal")
        || host_no_port.ends_with(".lan")
    {
        return true;
    }

    if let Ok(ip) = host_no_port.parse::<Ipv4Addr>() {
        let [a, b, _, _] = ip.octets();
        return a == 10
            || a == 127
            || (a == 192 && b == 168)
            || (a == 172 && (16..=31).contains(&b));
    }

    false
}

fn split_host_port(host: &str) -> String {
    if host.starts_with('[')
        && let Some(end) = host.find(']')
    {
        return host[1..end].to_string();
    }

    host.rsplit_once(':')
        .and_then(|(left, right)| {
            if right.chars().all(|c| c.is_ascii_digit()) {
                Some(left.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| host.to_string())
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests {
    use super::{is_private_registry_host, normalize_registry_host, split_host_port};

    #[test]
    fn normalize_registry_host_maps_docker_variants() {
        assert_eq!(
            normalize_registry_host("https://index.docker.io/v1/"),
            "docker.io"
        );
        assert_eq!(normalize_registry_host("registry-1.docker.io"), "docker.io");
        assert_eq!(
            normalize_registry_host("Harbor.Local:5000/v2"),
            "harbor.local:5000"
        );
    }

    #[test]
    fn split_host_port_keeps_hostname() {
        assert_eq!(split_host_port("registry.local:5000"), "registry.local");
        assert_eq!(split_host_port("docker.io"), "docker.io");
    }

    #[test]
    fn private_host_detection_works() {
        assert!(is_private_registry_host("registry.local"));
        assert!(is_private_registry_host("192.168.1.20"));
        assert!(is_private_registry_host("172.20.0.5"));
        assert!(!is_private_registry_host("ghcr.io"));
    }
}
