use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{Json, Router, body::Body, http::Request, response::IntoResponse as _, routing::post};
use http_body_util::BodyExt as _;
use serde_json::json;
use tower::ServiceExt as _;

use crate::{
    api, compose,
    config::Config,
    db::Db,
    ids,
    registry::{ImageRef, ManifestInfo, RegistryClient},
    runner::{CommandOutput, CommandRunner, CommandSpec},
    state::AppState,
};

include!("support_01.rs");
include!("support_02.rs");
include!("support_03.rs");
include!("support_04.rs");
include!("suite_01.rs");
include!("suite_02.rs");
include!("suite_03.rs");
include!("suite_04.rs");
include!("suite_05.rs");
include!("suite_06.rs");
include!("suite_07.rs");
include!("suite_08.rs");
include!("suite_09.rs");
include!("suite_10.rs");
include!("suite_11.rs");
include!("suite_12.rs");
include!("suite_13.rs");
include!("suite_14.rs");
include!("suite_15.rs");
include!("suite_16.rs");
include!("suite_17.rs");
include!("suite_18.rs");
include!("suite_19.rs");
include!("suite_20.rs");
include!("suite_21.rs");
include!("suite_22.rs");
include!("suite_23.rs");
include!("suite_24.rs");
include!("suite_25.rs");
include!("suite_26.rs");
