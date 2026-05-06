use std::{collections::BTreeMap, fs, path::Path};

use anyhow::Context as _;

use crate::api::types::{ServiceHomepage, ServiceUpdateGuard};

#[derive(Clone, Debug)]
pub struct ServiceFromCompose {
    pub name: String,
    pub image_ref: String,
    pub image_tag: String,
    pub homepage: Option<ServiceHomepage>,
    pub update_guard: Option<ServiceUpdateGuard>,
}

pub fn parse_services(compose_yaml: &str) -> anyhow::Result<Vec<ServiceFromCompose>> {
    let root: serde_yaml_ng::Value = serde_yaml_ng::from_str(compose_yaml).context("parse yaml")?;

    let services = root
        .get("services")
        .and_then(|v| v.as_mapping())
        .ok_or_else(|| anyhow::anyhow!("missing or invalid 'services' section"))?;

    let mut out = Vec::new();
    for (name_key, svc_val) in services {
        let Some(name) = name_key.as_str() else {
            continue;
        };
        let label_values = svc_val
            .get("labels")
            .map(collect_labels)
            .unwrap_or_default();
        let homepage = parse_homepage_labels(&label_values);
        let image_ref = svc_val
            .get("image")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if image_ref.is_empty() && homepage.is_none() {
            continue;
        }

        let image_tag = if image_ref.is_empty() {
            String::new()
        } else {
            extract_tag(&image_ref).unwrap_or_else(|| "latest".to_string())
        };

        out.push(ServiceFromCompose {
            name: name.to_string(),
            image_ref,
            image_tag,
            homepage,
            update_guard: None,
        });
    }

    Ok(out)
}

pub fn merge_services(
    mut base: BTreeMap<String, ServiceFromCompose>,
    add: Vec<ServiceFromCompose>,
) -> BTreeMap<String, ServiceFromCompose> {
    for svc in add {
        if let Some(existing) = base.get_mut(&svc.name) {
            merge_service(existing, svc);
            continue;
        }

        if svc.image_ref.is_empty() {
            continue;
        }

        base.insert(svc.name.clone(), svc);
    }
    base
}

fn merge_service(existing: &mut ServiceFromCompose, incoming: ServiceFromCompose) {
    if !incoming.image_ref.is_empty() {
        existing.image_ref = incoming.image_ref;
        existing.image_tag = incoming.image_tag;
    }

    existing.homepage = merge_homepage(existing.homepage.take(), incoming.homepage);
    existing.update_guard = merge_update_guard(existing.update_guard.take(), incoming.update_guard);
}

fn merge_homepage(
    base: Option<ServiceHomepage>,
    incoming: Option<ServiceHomepage>,
) -> Option<ServiceHomepage> {
    match (base, incoming) {
        (None, None) => None,
        (Some(homepage), None) | (None, Some(homepage)) => {
            (!homepage.is_empty()).then_some(homepage)
        }
        (Some(base), Some(incoming)) => {
            let merged = ServiceHomepage {
                group: incoming.group.or(base.group),
                name: incoming.name.or(base.name),
                icon: incoming.icon.or(base.icon),
                href: incoming.href.or(base.href),
                description: incoming.description.or(base.description),
            };
            (!merged.is_empty()).then_some(merged)
        }
    }
}

fn merge_update_guard(
    base: Option<ServiceUpdateGuard>,
    incoming: Option<ServiceUpdateGuard>,
) -> Option<ServiceUpdateGuard> {
    incoming.or(base)
}

pub fn extract_tag(image_ref: &str) -> Option<String> {
    // Strip digest first so refs like `repo:tag@sha256:...` still yield the tag, while
    // digest-only refs like `repo@sha256:...` return None.
    let (without_digest, _) = image_ref.split_once('@').unwrap_or((image_ref, ""));
    let (left, right) = without_digest.rsplit_once(':')?;
    if right.is_empty() {
        return None;
    }
    // If the part after the last ':' contains a '/', it is likely "registry:port/path", not a tag.
    if right.contains('/') {
        return None;
    }
    if left.is_empty() {
        return None;
    }
    Some(right.to_string())
}

pub fn validate_docker_tag(tag: &str) -> anyhow::Result<String> {
    let tag = tag.trim();
    if tag.is_empty() {
        return Err(anyhow::anyhow!("tag is required"));
    }
    if tag.len() > 128 {
        return Err(anyhow::anyhow!("tag must be at most 128 characters"));
    }
    let mut chars = tag.chars();
    let Some(first) = chars.next() else {
        return Err(anyhow::anyhow!("tag is required"));
    };
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return Err(anyhow::anyhow!(
            "tag must start with an ASCII letter, digit, or underscore"
        ));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-') {
        return Err(anyhow::anyhow!(
            "tag may only contain ASCII letters, digits, underscore, period, and dash"
        ));
    }
    Ok(tag.to_string())
}

pub fn image_repo_from_tagged_ref(image_ref: &str) -> Option<String> {
    let image_ref = image_ref.trim();
    let (without_digest, digest) = image_ref.split_once('@').unwrap_or((image_ref, ""));
    if !digest.is_empty() {
        return None;
    }
    let without_digest = without_digest.trim();
    if without_digest.is_empty() {
        return None;
    }
    match without_digest.rsplit_once(':') {
        Some((left, right)) if !right.is_empty() && !right.contains('/') && !left.is_empty() => {
            Some(left.to_string())
        }
        _ => Some(without_digest.to_string()),
    }
}

pub fn image_ref_with_tag(image_ref: &str, tag: &str) -> anyhow::Result<String> {
    let tag = validate_docker_tag(tag)?;
    let repo = image_repo_from_tagged_ref(image_ref)
        .ok_or_else(|| anyhow::anyhow!("only tag-based image refs are supported"))?;
    Ok(format!("{repo}:{tag}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposeTagPatchResult {
    pub image_ref: String,
    pub tag: String,
}

#[derive(Clone, Debug)]
struct ImageValueParts {
    prefix: String,
    image_ref: String,
    suffix: String,
}

pub fn patch_service_image_tag_in_file(
    path: &Path,
    service_name: &str,
    tag: &str,
) -> anyhow::Result<ComposeTagPatchResult> {
    let tag = validate_docker_tag(tag)?;
    let original =
        fs::read_to_string(path).with_context(|| format!("read compose file {path:?}"))?;
    let patched = patch_service_image_tag_text(&original, service_name, &tag)
        .with_context(|| format!("patch service {service_name} image tag in {path:?}"))?;
    let tmp_path = path.with_file_name(format!(
        ".{}.dockrev-tag-{}.tmp",
        path.file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("compose"),
        ulid::Ulid::new()
    ));
    fs::write(&tmp_path, patched.text.as_bytes())
        .with_context(|| format!("write temporary compose file {tmp_path:?}"))?;
    fs::rename(&tmp_path, path).with_context(|| format!("replace compose file {path:?}"))?;
    Ok(ComposeTagPatchResult {
        image_ref: patched.image_ref,
        tag,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComposeTagPatchTextResult {
    text: String,
    image_ref: String,
}

fn patch_service_image_tag_text(
    text: &str,
    service_name: &str,
    tag: &str,
) -> anyhow::Result<ComposeTagPatchTextResult> {
    let service_name = service_name.trim();
    if service_name.is_empty() {
        return Err(anyhow::anyhow!("service name is required"));
    }
    let lines = split_lines_preserve_endings(text);
    let services_idx = find_top_level_mapping_key_line(&lines, "services")
        .ok_or_else(|| anyhow::anyhow!("missing services mapping"))?;
    let service_idx = find_direct_child_mapping_key_line(&lines, service_name, services_idx)
        .ok_or_else(|| anyhow::anyhow!("service image definition not found"))?;
    let image_idx = find_direct_child_mapping_key_line(&lines, "image", service_idx)
        .ok_or_else(|| anyhow::anyhow!("service image definition not found"))?;

    let raw_line = lines[image_idx].0;
    let newline = lines[image_idx].1;
    let colon = raw_line
        .find(':')
        .ok_or_else(|| anyhow::anyhow!("invalid image line"))?;
    let key_prefix = &raw_line[..=colon];
    let value = &raw_line[colon + 1..];
    let parts = parse_image_value(value)?;
    if parts.image_ref.contains('$') {
        return Err(anyhow::anyhow!(
            "image uses variable interpolation and cannot be edited safely"
        ));
    }
    if parts.image_ref.contains('@') {
        return Err(anyhow::anyhow!(
            "digest-pinned image refs cannot be edited safely"
        ));
    }
    let next_ref = image_ref_with_tag(&parts.image_ref, tag)?;
    let next_line = format!(
        "{key_prefix}{}{}{}{}",
        parts.prefix, next_ref, parts.suffix, newline
    );

    let mut out = String::new();
    for (idx, (line, ending)) in lines.iter().enumerate() {
        if idx == image_idx {
            out.push_str(&next_line);
        } else {
            out.push_str(line);
            out.push_str(ending);
        }
    }
    Ok(ComposeTagPatchTextResult {
        text: out,
        image_ref: next_ref,
    })
}

fn split_lines_preserve_endings(text: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            let end = idx + ch.len_utf8();
            let raw = &text[start..idx];
            if let Some(stripped) = raw.strip_suffix('\r') {
                out.push((stripped, "\r\n"));
            } else {
                out.push((raw, "\n"));
            }
            start = end;
        }
    }
    if start < text.len() {
        out.push((&text[start..], ""));
    }
    out
}

fn line_indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn find_top_level_mapping_key_line(lines: &[(&str, &str)], key: &str) -> Option<usize> {
    for (idx, (line, _)) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || line_indent(line) != 0 {
            continue;
        }
        let Some((candidate, rest)) = trimmed.split_once(':') else {
            continue;
        };
        if rest.trim_start().starts_with('&') || rest.trim_start().starts_with('*') {
            continue;
        }
        if unquote_yaml_key(candidate.trim()) == Some(key) {
            return Some(idx);
        }
    }
    None
}

fn find_direct_child_mapping_key_line(
    lines: &[(&str, &str)],
    key: &str,
    parent_idx: usize,
) -> Option<usize> {
    let parent_indent = line_indent(lines.get(parent_idx)?.0);
    let mut child_indent: Option<usize> = None;
    for (idx, (line, _)) in lines.iter().enumerate().skip(parent_idx + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line_indent(line);
        if indent <= parent_indent {
            break;
        }
        let Some((candidate, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let expected_indent = *child_indent.get_or_insert(indent);
        if indent != expected_indent {
            continue;
        }
        if rest.trim_start().starts_with('&') || rest.trim_start().starts_with('*') {
            continue;
        }
        if unquote_yaml_key(candidate.trim()) == Some(key) {
            return Some(idx);
        }
    }
    None
}

fn unquote_yaml_key(input: &str) -> Option<&str> {
    if input.is_empty() {
        return None;
    }
    if (input.starts_with('"') && input.ends_with('"'))
        || (input.starts_with('\'') && input.ends_with('\''))
    {
        return input.get(1..input.len().saturating_sub(1));
    }
    Some(input)
}

fn parse_image_value(value: &str) -> anyhow::Result<ImageValueParts> {
    let leading_len = value.len() - value.trim_start().len();
    let leading = &value[..leading_len];
    let rest = &value[leading_len..];
    if rest.is_empty() {
        return Err(anyhow::anyhow!("image must be a single-line scalar"));
    }
    if rest.starts_with('*') || rest.starts_with('&') {
        return Err(anyhow::anyhow!(
            "image aliases and anchors are not supported"
        ));
    }
    if rest.starts_with('"') || rest.starts_with('\'') {
        let quote = rest.as_bytes()[0] as char;
        let mut escaped = false;
        for (idx, ch) in rest.char_indices().skip(1) {
            if quote == '"' && ch == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if ch == quote && !escaped {
                let image_ref = &rest[1..idx];
                let suffix = &rest[idx..];
                return Ok(ImageValueParts {
                    prefix: format!("{leading}{quote}"),
                    image_ref: image_ref.to_string(),
                    suffix: suffix.to_string(),
                });
            }
            escaped = false;
        }
        return Err(anyhow::anyhow!("unterminated quoted image scalar"));
    }

    let mut end = rest.len();
    for (idx, ch) in rest.char_indices() {
        if ch == '#' && (idx == 0 || rest[..idx].ends_with(char::is_whitespace)) {
            end = idx;
            break;
        }
    }
    let image_raw = &rest[..end];
    let image_trimmed = image_raw.trim_end();
    if image_trimmed.is_empty() {
        return Err(anyhow::anyhow!("image must be a single-line scalar"));
    }
    Ok(ImageValueParts {
        prefix: leading.to_string(),
        image_ref: image_trimmed.to_string(),
        suffix: rest[image_trimmed.len()..].to_string(),
    })
}

fn parse_homepage_labels(values: &BTreeMap<String, String>) -> Option<ServiceHomepage> {
    if values.is_empty() {
        return None;
    }

    let homepage = ServiceHomepage {
        group: normalize_homepage_value(values.get("homepage.group")),
        name: normalize_homepage_value(values.get("homepage.name")),
        icon: normalize_homepage_value(values.get("homepage.icon")),
        href: normalize_homepage_value(values.get("homepage.href")),
        description: normalize_homepage_value(values.get("homepage.description")),
    };

    (!homepage.is_empty()).then_some(homepage)
}

fn collect_labels(value: &serde_yaml_ng::Value) -> BTreeMap<String, String> {
    if let Some(mapping) = value.as_mapping() {
        let mut out = BTreeMap::new();
        for (key, value) in mapping {
            let Some(key) = key.as_str() else {
                continue;
            };
            let Some(value) = yaml_scalar_to_string(value) else {
                continue;
            };
            out.insert(key.to_string(), value);
        }
        return out;
    }

    if let Some(sequence) = value.as_sequence() {
        let mut out = BTreeMap::new();
        for item in sequence {
            let Some(item) = item.as_str() else {
                continue;
            };
            let Some((key, value)) = item.split_once('=') else {
                continue;
            };
            out.insert(key.trim().to_string(), value.to_string());
        }
        return out;
    }

    BTreeMap::new()
}

fn yaml_scalar_to_string(value: &serde_yaml_ng::Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }
    if let Some(value) = value.as_bool() {
        return Some(value.to_string());
    }
    if let Some(value) = value.as_i64() {
        return Some(value.to_string());
    }
    if let Some(value) = value.as_u64() {
        return Some(value.to_string());
    }
    value.as_f64().map(|value| value.to_string())
}

fn normalize_homepage_value(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_services_basic() {
        let yaml = r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
  db:
    image: postgres:16
"#;
        let services = parse_services(yaml).unwrap();
        assert_eq!(services.len(), 2);
        assert!(
            services
                .iter()
                .any(|s| s.name == "web" && s.image_tag == "5.2")
        );
        assert!(
            services
                .iter()
                .any(|s| s.name == "db" && s.image_tag == "16")
        );
    }

    #[test]
    fn extract_tag_registry_port() {
        assert_eq!(
            extract_tag("localhost:5000/repo/app:1.2.3").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(extract_tag("localhost:5000/repo/app").as_deref(), None);
    }

    #[test]
    fn extract_tag_digest() {
        assert_eq!(
            extract_tag("ghcr.io/acme/web@sha256:deadbeef").as_deref(),
            None
        );
    }

    #[test]
    fn extract_tag_tag_plus_digest() {
        assert_eq!(
            extract_tag("valkey/valkey:8-alpine@sha256:deadbeef").as_deref(),
            Some("8-alpine")
        );
        assert_eq!(
            extract_tag("localhost:5000/repo/app:1.2.3@sha256:deadbeef").as_deref(),
            Some("1.2.3")
        );
    }

    #[test]
    fn parse_services_extracts_homepage_labels_from_list() {
        let yaml = r#"
services:
  gitea:
    image: docker.gitea.com/gitea:1.23
    labels:
      - homepage.group=Developer
      - homepage.name=Gitea
      - homepage.icon=si-gitea
      - homepage.href=https://git.example.com
      - homepage.description=Git forge
      - homepage.widget.type=gitea
"#;
        let services = parse_services(yaml).unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(
            services[0].homepage,
            Some(ServiceHomepage {
                group: Some("Developer".to_string()),
                name: Some("Gitea".to_string()),
                icon: Some("si-gitea".to_string()),
                href: Some("https://git.example.com".to_string()),
                description: Some("Git forge".to_string()),
            })
        );
    }

    #[test]
    fn parse_services_extracts_homepage_labels_from_map() {
        let yaml = r#"
services:
  grafana:
    image: grafana/grafana:11
    labels:
      homepage.group: Monitoring
      homepage.name: Grafana
      homepage.icon: mdi-chart-timeline-variant
      homepage.href: https://grafana.example.com
      homepage.description: Dashboards
"#;
        let services = parse_services(yaml).unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(
            services[0].homepage,
            Some(ServiceHomepage {
                group: Some("Monitoring".to_string()),
                name: Some("Grafana".to_string()),
                icon: Some("mdi-chart-timeline-variant".to_string()),
                href: Some("https://grafana.example.com".to_string()),
                description: Some("Dashboards".to_string()),
            })
        );
    }

    #[test]
    fn parse_services_ignores_traefik_router_labels_for_update_blocking() {
        let yaml = r#"
services:
  whoami:
    image: traefik/whoami:latest
    labels:
      - traefik.http.routers.whoami.rule=Host(`whoami.example.com`)
"#;
        let services = parse_services(yaml).unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].update_guard, None);
    }

    #[test]
    fn merge_services_preserves_base_homepage_when_override_omits_labels() {
        let base = parse_services(
            r#"
services:
  gitea:
    image: docker.gitea.com/gitea:1.23
    labels:
      - homepage.group=Developer
      - homepage.name=Gitea
      - homepage.icon=si-gitea
"#,
        )
        .unwrap();
        let override_services = parse_services(
            r#"
services:
  gitea:
    image: docker.gitea.com/gitea:1.24
"#,
        )
        .unwrap();

        let merged = merge_services(
            base.into_iter()
                .map(|svc| (svc.name.clone(), svc))
                .collect::<BTreeMap<_, _>>(),
            override_services,
        );
        let gitea = merged.get("gitea").unwrap();

        assert_eq!(gitea.image_ref, "docker.gitea.com/gitea:1.24");
        assert_eq!(gitea.image_tag, "1.24");
        assert_eq!(
            gitea.homepage,
            Some(ServiceHomepage {
                group: Some("Developer".to_string()),
                name: Some("Gitea".to_string()),
                icon: Some("si-gitea".to_string()),
                href: None,
                description: None,
            })
        );
    }

    #[test]
    fn merge_services_applies_homepage_override_from_label_only_file() {
        let base = parse_services(
            r#"
services:
  grafana:
    image: grafana/grafana:11
    labels:
      homepage.group: Monitoring
      homepage.name: Grafana
"#,
        )
        .unwrap();
        let override_services = parse_services(
            r#"
services:
  grafana:
    labels:
      homepage.href: https://grafana.example.com
      homepage.description: Dashboards
"#,
        )
        .unwrap();

        let merged = merge_services(
            base.into_iter()
                .map(|svc| (svc.name.clone(), svc))
                .collect::<BTreeMap<_, _>>(),
            override_services,
        );
        let grafana = merged.get("grafana").unwrap();

        assert_eq!(grafana.image_ref, "grafana/grafana:11");
        assert_eq!(grafana.image_tag, "11");
        assert_eq!(
            grafana.homepage,
            Some(ServiceHomepage {
                group: Some("Monitoring".to_string()),
                name: Some("Grafana".to_string()),
                icon: None,
                href: Some("https://grafana.example.com".to_string()),
                description: Some("Dashboards".to_string()),
            })
        );
    }

    #[test]
    fn merge_services_keeps_update_guard_empty_when_override_only_changes_homepage() {
        let base = parse_services(
            r#"
services:
  edge:
    image: ghcr.io/acme/edge:1.0
    labels:
      - traefik.http.routers.edge.rule=Host(`edge.example.com`)
"#,
        )
        .unwrap();
        let override_services = parse_services(
            r#"
services:
  edge:
    labels:
      homepage.name: Edge
"#,
        )
        .unwrap();

        let merged = merge_services(
            base.into_iter()
                .map(|svc| (svc.name.clone(), svc))
                .collect::<BTreeMap<_, _>>(),
            override_services,
        );
        let edge = merged.get("edge").unwrap();

        assert_eq!(edge.update_guard, None);
        assert_eq!(
            edge.homepage,
            Some(ServiceHomepage {
                group: None,
                name: Some("Edge".to_string()),
                icon: None,
                href: None,
                description: None,
            })
        );
    }

    #[test]
    fn patch_service_image_tag_preserves_comment() {
        let yaml = "services:\n  api:\n    image: ghcr.io/acme/api:5.2 # prod\n";
        let patched = patch_service_image_tag_text(yaml, "api", "5.3").unwrap();
        assert_eq!(
            patched.text,
            "services:\n  api:\n    image: ghcr.io/acme/api:5.3 # prod\n"
        );
        assert_eq!(patched.image_ref, "ghcr.io/acme/api:5.3");
    }

    #[test]
    fn patch_service_image_tag_preserves_double_quotes() {
        let yaml = "services:\n  api:\n    image: \"localhost:5000/acme/api:5.2\"\n";
        let patched = patch_service_image_tag_text(yaml, "api", "5.3").unwrap();
        assert_eq!(
            patched.text,
            "services:\n  api:\n    image: \"localhost:5000/acme/api:5.3\"\n"
        );
    }

    #[test]
    fn patch_service_image_tag_adds_tag_to_implicit_latest() {
        let yaml = "services:\n  api:\n    image: nginx\n";
        let patched = patch_service_image_tag_text(yaml, "api", "1.27").unwrap();
        assert_eq!(patched.text, "services:\n  api:\n    image: nginx:1.27\n");
        assert_eq!(patched.image_ref, "nginx:1.27");
    }

    #[test]
    fn patch_service_image_tag_adds_tag_after_registry_port() {
        let yaml = "services:\n  api:\n    image: localhost:5000/acme/api\n";
        let patched = patch_service_image_tag_text(yaml, "api", "5.3").unwrap();
        assert_eq!(
            patched.text,
            "services:\n  api:\n    image: localhost:5000/acme/api:5.3\n"
        );
        assert_eq!(patched.image_ref, "localhost:5000/acme/api:5.3");
    }

    #[test]
    fn patch_service_image_tag_ignores_nested_image_keys() {
        let yaml = "services:\n  api:\n    labels:\n      image: not-a-service-image:1\n    image: ghcr.io/acme/api:5.2\n";
        let patched = patch_service_image_tag_text(yaml, "api", "5.3").unwrap();
        assert_eq!(
            patched.text,
            "services:\n  api:\n    labels:\n      image: not-a-service-image:1\n    image: ghcr.io/acme/api:5.3\n"
        );
    }

    #[test]
    fn patch_service_image_tag_uses_root_services_mapping() {
        let yaml = "x-template:\n  services:\n    api:\n      image: ghcr.io/acme/template:1\nservices:\n  api:\n    image: ghcr.io/acme/api:5.2\n";
        let patched = patch_service_image_tag_text(yaml, "api", "5.3").unwrap();
        assert_eq!(
            patched.text,
            "x-template:\n  services:\n    api:\n      image: ghcr.io/acme/template:1\nservices:\n  api:\n    image: ghcr.io/acme/api:5.3\n"
        );
        assert_eq!(patched.image_ref, "ghcr.io/acme/api:5.3");
    }

    #[test]
    fn patch_service_image_tag_rejects_interpolation() {
        let yaml = "services:\n  api:\n    image: ghcr.io/acme/api:${TAG}\n";
        let err = patch_service_image_tag_text(yaml, "api", "5.3").unwrap_err();
        assert!(err.to_string().contains("variable interpolation"));
    }

    #[test]
    fn patch_service_image_tag_rejects_dollar_interpolation() {
        let yaml = "services:\n  api:\n    image: ghcr.io/acme/api:$TAG\n";
        let err = patch_service_image_tag_text(yaml, "api", "5.3").unwrap_err();
        assert!(err.to_string().contains("variable interpolation"));
    }

    #[test]
    fn patch_service_image_tag_rejects_digest_pin() {
        let yaml = "services:\n  api:\n    image: ghcr.io/acme/api@sha256:deadbeef\n";
        let err = patch_service_image_tag_text(yaml, "api", "5.3").unwrap_err();
        assert!(err.to_string().contains("digest-pinned"));
    }

    #[test]
    fn validate_docker_tag_rejects_slash() {
        assert!(validate_docker_tag("release/5").is_err());
    }
}
