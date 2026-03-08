use std::collections::BTreeMap;

use crate::api::types::{ComposeConfig, Service, StackBackupConfig, TernaryChoice};

#[derive(Clone, Debug)]
pub struct StackRecord {
    pub id: String,
    pub name: String,
    pub archived: bool,
    pub compose: ComposeConfig,
    pub backup: StackBackupConfig,
    pub services: Vec<Service>,
}

#[derive(Clone, Debug)]
pub struct ServiceSeed {
    pub id: String,
    pub name: String,
    pub image_ref: String,
    pub image_tag: String,
    pub auto_rollback: bool,
    pub backup_bind_paths: BTreeMap<String, TernaryChoice>,
    pub backup_volume_names: BTreeMap<String, TernaryChoice>,
}
