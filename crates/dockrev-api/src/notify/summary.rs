use super::NewVersionDiscoveredService;

pub(super) fn notification_version_entries(
    items: &[NewVersionDiscoveredService],
    service_names: &std::collections::HashMap<String, String>,
) -> Vec<crate::db::NotificationVersionSummaryEntry> {
    let mut seen = std::collections::BTreeSet::new();
    items
        .iter()
        .filter(|item| seen.insert(item.service_id.as_str()))
        .map(|item| crate::db::NotificationVersionSummaryEntry {
            service_name: service_names.get(&item.service_id).cloned(),
            current_tag: item.current_display_tag.clone(),
            candidate_tag: item.candidate_display_tag.clone(),
        })
        .collect()
}
