use super::*;

impl Db {
    pub async fn sync_stack_from_compose(
        &self,
        stack_id: &str,
        compose_files: &[String],
        services: &[ComposeServiceSpec],
        now: &str,
    ) -> anyhow::Result<()> {
        self.sync_stack_from_compose_guarded(stack_id, compose_files, services, now, None)
            .await
            .map(|_| ())
    }
}
