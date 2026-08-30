use std::collections::BTreeMap;

pub(crate) async fn read_and_merge_compose_files(
    config_files: &[String],
) -> Result<BTreeMap<String, crate::compose::ServiceFromCompose>, String> {
    let mut merged = BTreeMap::new();
    for path in config_files {
        let contents = tokio::fs::read_to_string(path).await.map_err(|error| {
            format!("compose_file_unreadable: {path} ({error}) (mount missing? ensure host path is mounted read-only at the same absolute path)")
        })?;
        let parsed = crate::compose::parse_services(&contents)
            .map_err(|error| format!("compose_file_invalid: {path} ({error})"))?;
        merged = crate::compose::merge_services(merged, parsed);
    }
    if merged.is_empty() {
        return Err("compose_no_services".to_string());
    }
    Ok(merged)
}
