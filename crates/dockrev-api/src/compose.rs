use std::collections::BTreeMap;

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
        let update_guard = parse_update_guard_labels(&label_values);
        let image_ref = svc_val
            .get("image")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if image_ref.is_empty() && homepage.is_none() && update_guard.is_none() {
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
            update_guard,
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

fn extract_tag(image_ref: &str) -> Option<String> {
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

fn parse_update_guard_labels(values: &BTreeMap<String, String>) -> Option<ServiceUpdateGuard> {
    values
        .iter()
        .any(|(key, value)| is_traefik_router_rule_label(key) && !value.trim().is_empty())
        .then(ServiceUpdateGuard::traefik_online_service)
}

fn is_traefik_router_rule_label(key: &str) -> bool {
    (key.starts_with("traefik.http.routers.") && key.ends_with(".rule"))
        || (key.starts_with("traefik.tcp.routers.") && key.ends_with(".rule"))
        || (key.starts_with("traefik.udp.routers.") && key.ends_with(".rule"))
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
    fn parse_services_extracts_traefik_router_guard_from_list_labels() {
        let yaml = r#"
services:
  whoami:
    image: traefik/whoami:latest
    labels:
      - traefik.http.routers.whoami.rule=Host(`whoami.example.com`)
"#;
        let services = parse_services(yaml).unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(
            services[0].update_guard,
            Some(ServiceUpdateGuard::traefik_online_service())
        );
    }

    #[test]
    fn parse_services_extracts_traefik_router_guard_from_map_labels() {
        let yaml = r#"
services:
  tcp-app:
    image: ghcr.io/acme/tcp-app:1.0
    labels:
      traefik.tcp.routers.tcp-app.rule: HostSNI(`tcp.example.com`)
"#;
        let services = parse_services(yaml).unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(
            services[0].update_guard,
            Some(ServiceUpdateGuard::traefik_online_service())
        );
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
    fn merge_services_preserves_base_update_guard_when_override_only_changes_homepage() {
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

        assert_eq!(
            edge.update_guard,
            Some(ServiceUpdateGuard::traefik_online_service())
        );
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
}
