use std::collections::BTreeMap;

use anyhow::Context as _;

#[derive(Clone, Debug)]
pub struct ServiceFromCompose {
    pub name: String,
    pub image_ref: String,
    pub image_tag: String,
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
        let image_ref = svc_val
            .get("image")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if image_ref.is_empty() {
            continue;
        }

        let image_tag = extract_tag(&image_ref).unwrap_or_else(|| "latest".to_string());

        out.push(ServiceFromCompose {
            name: name.to_string(),
            image_ref,
            image_tag,
        });
    }

    Ok(out)
}

pub fn merge_services(
    mut base: BTreeMap<String, ServiceFromCompose>,
    add: Vec<ServiceFromCompose>,
) -> BTreeMap<String, ServiceFromCompose> {
    for svc in add {
        base.insert(svc.name.clone(), svc);
    }
    base
}

fn extract_tag(image_ref: &str) -> Option<String> {
    if image_ref.contains('@') {
        return None;
    }
    let (left, right) = image_ref.rsplit_once(':')?;
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
}
