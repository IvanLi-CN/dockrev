use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod cleanup;
mod core;
mod deploy;
mod discovery;
mod github_packages;
mod ignores;
mod jobs;
mod notifications;
mod service_logs;
mod services;
mod settings;

pub use cleanup::*;
pub use core::*;
pub use deploy::*;
pub use discovery::*;
pub use github_packages::*;
pub use ignores::*;
pub use jobs::*;
pub use notifications::*;
pub use service_logs::*;
pub use services::*;
pub use settings::*;

fn mask_if_some(input: Option<String>) -> Option<String> {
    input.map(|_| "******".to_string())
}

fn mask_to_bullets(input: Option<&str>) -> Option<String> {
    input.map(|value| "•".repeat(value.chars().count()))
}

fn is_non_empty(input: Option<&str>) -> bool {
    input.map(|value| !value.trim().is_empty()).unwrap_or(false)
}

fn default_true() -> bool {
    true
}
