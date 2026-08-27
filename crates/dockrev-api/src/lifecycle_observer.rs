use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{
    db::{Db, ServiceLifecycleEventInput},
    docker_engine::DockerEngineClient,
};

const EVENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DockerEngineEvent {
    #[serde(rename = "status", alias = "Action")]
    pub action: Option<String>,
    #[serde(rename = "time", alias = "Time")]
    pub time: Option<i64>,
    #[serde(rename = "timeNano", alias = "TimeNano")]
    pub time_nano: Option<i64>,
    #[serde(rename = "Actor", default)]
    pub actor: Option<DockerEventActor>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct DockerEventActor {
    #[serde(rename = "ID", alias = "id", default)]
    pub id: String,
    #[serde(rename = "Attributes", alias = "attributes", default)]
    pub attributes: std::collections::BTreeMap<String, String>,
}

#[derive(Clone)]
pub struct OperationScopedLifecycleObserver {
    db: Db,
    http: reqwest::Client,
    base_url: String,
}

pub struct OperationObservation {
    events: Arc<Mutex<Vec<DockerEngineEvent>>>,
    task: JoinHandle<()>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RuntimeContainer {
    #[serde(rename = "Id", alias = "id")]
    id: String,
    #[serde(rename = "State", alias = "state")]
    state: String,
    #[serde(rename = "Labels", alias = "labels", default)]
    labels: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
struct RuntimeInspect {
    #[serde(rename = "State")]
    state: Option<RuntimeInspectState>,
}

#[derive(Clone, Debug, Deserialize)]
struct RuntimeInspectState {
    #[serde(rename = "StartedAt")]
    started_at: Option<String>,
}

impl OperationScopedLifecycleObserver {
    pub fn from_env(db: Db) -> anyhow::Result<Self> {
        let client = DockerEngineClient::from_env()?;
        let (http, base_url) = client.event_transport();
        Ok(Self { db, http, base_url })
    }

    pub fn unavailable(db: Db) -> Self {
        let http = reqwest::Client::new();
        Self {
            db,
            http,
            base_url: "http://docker".to_string(),
        }
    }

    pub fn begin(&self, compose_project: &str, _service_names: &[String]) -> OperationObservation {
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let http = self.http.clone();
        let url = format!("{}/events", self.base_url.trim_end_matches('/'));
        let project = compose_project.to_string();
        let task = tokio::spawn(async move {
            let labels = vec![format!("com.docker.compose.project={project}")];
            let filters = serde_json::json!({"type": ["container"], "label": labels});
            let since = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let response = http
                .get(url)
                .query(&[
                    ("filters", filters.to_string()),
                    ("since", since.to_string()),
                ])
                .send()
                .await;
            let Ok(response) = response else { return };
            if !response.status().is_success() {
                return;
            }
            let mut response = response;
            let mut buffer = Vec::new();
            while let Ok(Some(chunk)) = response.chunk().await {
                buffer.extend_from_slice(&chunk);
                while let Some(pos) = buffer.iter().position(|byte| *byte == b'\n') {
                    let line = buffer.drain(..=pos).collect::<Vec<_>>();
                    if let Ok(event) = serde_json::from_slice::<DockerEngineEvent>(
                        &line[..line.len().saturating_sub(1)],
                    ) {
                        captured.lock().await.push(event);
                    }
                }
            }
        });
        OperationObservation { events, task }
    }

    pub async fn finish(&self, observation: OperationObservation) -> Vec<DockerEngineEvent> {
        observation.task.abort();
        let _ = observation.task.await;
        observation.events.lock().await.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_operation(
        &self,
        observation: Option<OperationObservation>,
        operation_group_id: &str,
        job_id: Option<&str>,
        stack_id: &str,
        compose_project: &str,
        service_names: &[String],
        action: &str,
        success: bool,
    ) {
        let events = match observation {
            Some(value) => self.finish(value).await,
            None => Vec::new(),
        };
        let mut transitions = Vec::new();
        if action == "stop" || action == "restart" {
            transitions.push("stopped");
        }
        if action == "start" || action == "restart" {
            transitions.push("started");
        }
        let event_actions: BTreeSet<&str> = events
            .iter()
            .filter_map(|event| event.action.as_deref())
            .collect();
        let runtime = self
            .inspect_runtime(compose_project)
            .await
            .unwrap_or_default();
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        for service_name in service_names {
            let Ok(Some(service_id)) = self
                .db
                .get_service_id_by_stack_and_name(stack_id, service_name)
                .await
            else {
                continue;
            };
            let service_containers = runtime
                .iter()
                .filter(|container| {
                    container
                        .labels
                        .get("com.docker.compose.service")
                        .map(|name| name == service_name)
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>();
            let running_containers = service_containers
                .iter()
                .filter(|container| container.state.eq_ignore_ascii_case("running"))
                .count();
            for transition in &transitions {
                let matching_events = events
                    .iter()
                    .filter(|event| {
                        let action_matches = match *transition {
                            "started" => {
                                matches!(event.action.as_deref(), Some("start" | "started"),)
                            }
                            "stopped" => matches!(
                                event.action.as_deref(),
                                Some("stop" | "die" | "kill" | "destroy")
                            ),
                            _ => false,
                        };
                        action_matches
                            && event
                                .actor
                                .as_ref()
                                .and_then(|actor| {
                                    actor.attributes.get("com.docker.compose.service")
                                })
                                .map(|name| name == service_name)
                                .unwrap_or(false)
                    })
                    .collect::<Vec<_>>();
                let matching = matching_events.first().copied();
                let mut runtime_started_at = None;
                if *transition == "started"
                    && !service_containers.is_empty()
                    && running_containers == service_containers.len()
                {
                    for container in &service_containers {
                        let Some(started_at) = self.inspect_started_at(&container.id).await else {
                            runtime_started_at = None;
                            break;
                        };
                        if runtime_started_at
                            .as_ref()
                            .is_none_or(|current: &String| started_at.as_str() > current.as_str())
                        {
                            runtime_started_at = Some(started_at);
                        }
                    }
                }
                let observed_at = if *transition == "started" {
                    runtime_started_at
                        .clone()
                        .or_else(|| matching.and_then(event_timestamp))
                } else {
                    matching
                        .and_then(event_timestamp)
                        .or_else(|| runtime_started_at.clone())
                }
                .unwrap_or_else(|| now.clone());
                let all_replicas_running = !service_containers.is_empty()
                    && running_containers == service_containers.len();
                let all_replicas_stopped =
                    !service_containers.is_empty() && running_containers == 0;
                let all_stop_events_seen = !service_containers.is_empty()
                    && matching_events.len() >= service_containers.len();
                let precise = if *transition == "started" {
                    success && all_replicas_running && runtime_started_at.is_some()
                } else {
                    success && all_replicas_stopped && all_stop_events_seen
                };
                let evidence = serde_json::json!({"engineEvent": matching, "engineEventCount": matching_events.len(), "engineEventActions": event_actions, "runtime": service_containers, "composeProject": compose_project, "success": success});
                let details = serde_json::json!({"serviceName": service_name, "observation": if precise { "engine_event_or_inspect" } else { "incomplete" }});
                let _ = self
                    .db
                    .insert_service_lifecycle_event(ServiceLifecycleEventInput {
                        service_id: service_id.clone(),
                        stack_id: Some(stack_id.to_string()),
                        operation_group_id: operation_group_id.to_string(),
                        job_id: job_id.map(ToOwned::to_owned),
                        origin: "compose".to_string(),
                        transition: (*transition).to_string(),
                        observed_at,
                        boundary_precision: if precise { "exact" } else { "incomplete" }
                            .to_string(),
                        evidence_json: evidence.to_string(),
                        details_json: details.to_string(),
                        created_at: now.clone(),
                    })
                    .await;
            }
        }
    }

    async fn inspect_runtime(
        &self,
        compose_project: &str,
    ) -> anyhow::Result<Vec<RuntimeContainer>> {
        let filters =
            serde_json::json!({"label": [format!("com.docker.compose.project={compose_project}")]});
        let response = self
            .http
            .get(format!(
                "{}/containers/json",
                self.base_url.trim_end_matches('/')
            ))
            .query(&[
                ("all", "true".to_string()),
                ("filters", filters.to_string()),
            ])
            .timeout(EVENT_CONNECT_TIMEOUT)
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }

    async fn inspect_started_at(&self, container_id: &str) -> Option<String> {
        let response = self
            .http
            .get(format!(
                "{}/containers/{container_id}/json",
                self.base_url.trim_end_matches('/')
            ))
            .timeout(EVENT_CONNECT_TIMEOUT)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        response
            .json::<RuntimeInspect>()
            .await
            .ok()?
            .state?
            .started_at
    }
}

fn event_timestamp(event: &DockerEngineEvent) -> Option<String> {
    if let Some(nanos) = event.time_nano.filter(|value| *value > 0) {
        return time::OffsetDateTime::from_unix_timestamp_nanos(nanos as i128)
            .ok()?
            .format(&time::format_description::well_known::Rfc3339)
            .ok();
    }
    event
        .time
        .filter(|value| *value > 0)
        .and_then(|seconds| time::OffsetDateTime::from_unix_timestamp(seconds).ok())
        .and_then(|value| {
            value
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
}

#[allow(dead_code)]
pub fn parse_engine_event_line(line: &[u8]) -> anyhow::Result<DockerEngineEvent> {
    serde_json::from_slice(line).context("decode Docker Engine event")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_engine_event_with_compose_attributes() {
        let event = parse_engine_event_line(br#"{"status":"die","timeNano":1700000000000000000,"Actor":{"ID":"abc","Attributes":{"com.docker.compose.service":"web"}}}"#).unwrap();
        assert_eq!(event.action.as_deref(), Some("die"));
        assert_eq!(
            event
                .actor
                .unwrap()
                .attributes
                .get("com.docker.compose.service")
                .map(String::as_str),
            Some("web")
        );
    }
}
