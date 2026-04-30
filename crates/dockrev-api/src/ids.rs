use ulid::Ulid;

pub fn new_stack_id() -> String {
    format!("stk_{}", Ulid::new())
}

pub fn new_service_id() -> String {
    format!("svc_{}", Ulid::new())
}

pub fn new_ignore_id() -> String {
    format!("ign_{}", Ulid::new())
}

pub fn new_job_id() -> String {
    format!("job_{}", Ulid::new())
}

pub fn new_check_id() -> String {
    format!("chk_{}", Ulid::new())
}

pub fn new_discovery_id() -> String {
    format!("dsc_{}", Ulid::new())
}

pub fn new_backup_id() -> String {
    format!("bkp_{}", Ulid::new())
}

pub fn new_notification_id() -> String {
    format!("ntf_{}", Ulid::new())
}

pub fn new_auto_update_pending_id() -> String {
    format!("aup_{}", Ulid::new())
}
