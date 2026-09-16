impl Db {
    async fn publish_stale_job_termination(&self, job: &JobListItem) {
        let mut entities = vec![crate::management_events::ManagementEventEntity {
            entity_type: "job".to_string(),
            id: job.id.clone(),
        }];
        if let Some(stack_id) = job.stack_id.as_ref() {
            entities.push(crate::management_events::ManagementEventEntity {
                entity_type: "stack".to_string(),
                id: stack_id.clone(),
            });
        }
        if let Some(service_id) = job.service_id.as_ref() {
            entities.push(crate::management_events::ManagementEventEntity {
                entity_type: "service".to_string(),
                id: service_id.clone(),
            });
        }
        self.management_events
            .publish_immediate(
                "jobs",
                entities,
                serde_json::json!({
                    "jobId": job.id,
                    "status": "failed",
                    "jobType": job.r#type.as_str(),
                    "scope": job.scope.as_str(),
                    "stackId": job.stack_id,
                    "serviceId": job.service_id,
                    "terminal": true,
                    "reason": "stale_check",
                }),
            )
            .await;
    }
}
