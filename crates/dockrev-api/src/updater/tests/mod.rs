use super::*;
use crate::{
    api::types::{
        ArchMatch, BackupTargetOverrides, Candidate, ComposeRef, Service, ServiceSettings,
        TernaryChoice,
    },
    runner::{CommandOutput, CommandRunner},
};
use std::{collections::BTreeMap, fs, sync::Mutex};

mod support;
use support::*;
mod auth_env;
mod lifecycle;
mod retry_and_failures;
mod rollback;
mod selection;
