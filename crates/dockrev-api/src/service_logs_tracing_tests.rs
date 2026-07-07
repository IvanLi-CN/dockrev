use super::parse_service_log_lines;
use crate::api::types::ServiceLogMetaFormat;

#[test]
fn parse_service_log_lines_adds_tracing_text_metadata() {
    let lines = parse_service_log_lines(
        "2026-07-07T05:54:01.126784508Z \u{1b}[2m2026-07-07T05:54:01.126674Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m openai proxy request started \u{1b}[3mproxy_request_id\u{1b}[0m\u{1b}[2m=\u{1b}[0m2722 \u{1b}[3mmethod\u{1b}[0m\u{1b}[2m=\u{1b}[0mPOST \u{1b}[3muri\u{1b}[0m\u{1b}[2m=\u{1b}[0m/v1/responses \u{1b}[3mproxy_request_started\u{1b}[0m\u{1b}[2m=\u{1b}[0mtrue \u{1b}[3mhas_body\u{1b}[0m\u{1b}[2m=\u{1b}[0mtrue \u{1b}[3mcontent_length\u{1b}[0m\u{1b}[2m=\u{1b}[0mSome(569164) \u{1b}[3mpeer_ip\u{1b}[0m\u{1b}[2m=\u{1b}[0mSome(172.24.0.176)\n",
    );

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].ts, "2026-07-07T05:54:01.126784508Z");
    let meta = lines[0].meta.as_ref().expect("tracing text metadata");
    assert_eq!(meta.format, ServiceLogMetaFormat::Text);
    assert_eq!(meta.level.as_deref(), Some("info"));
    assert_eq!(
        meta.timestamp.as_deref(),
        Some("2026-07-07T05:54:01.126674Z")
    );
    assert_eq!(
        meta.message.as_deref(),
        Some("openai proxy request started")
    );
    assert_eq!(meta.attributes["proxy_request_id"].as_i64(), Some(2722));
    assert_eq!(meta.attributes["method"].as_str(), Some("POST"));
    assert_eq!(meta.attributes["uri"].as_str(), Some("/v1/responses"));
    assert_eq!(
        meta.attributes["proxy_request_started"].as_bool(),
        Some(true)
    );
    assert_eq!(meta.attributes["has_body"].as_bool(), Some(true));
    assert_eq!(
        meta.attributes["content_length"].as_str(),
        Some("Some(569164)")
    );
    assert_eq!(
        meta.attributes["peer_ip"].as_str(),
        Some("Some(172.24.0.176)")
    );
    assert!(meta.highlights.contains(&"method".to_string()));
    assert!(meta.highlights.contains(&"uri".to_string()));
    assert!(meta.highlights.contains(&"proxy_request_id".to_string()));
}

#[test]
fn parse_service_log_lines_keeps_tracing_status_phrase() {
    let lines = parse_service_log_lines(
        "2026-07-07T05:54:00.521644238Z 2026-07-07T05:54:00.521559Z  INFO openai proxy response headers ready proxy_request_id=2719 method=POST uri=/v1/responses status=200 OK elapsed_ms=4548\n",
    );

    let meta = lines[0].meta.as_ref().expect("tracing text metadata");
    assert_eq!(meta.level.as_deref(), Some("info"));
    assert_eq!(
        meta.message.as_deref(),
        Some("openai proxy response headers ready")
    );
    assert_eq!(meta.attributes["status"].as_str(), Some("200 OK"));
    assert_eq!(meta.attributes["elapsed_ms"].as_i64(), Some(4548));
}

#[test]
fn parse_service_log_lines_handles_tracing_span_fields() {
    let lines = parse_service_log_lines(
        "2026-07-07T05:54:01.126784508Z 2026-07-07T05:54:01.126674Z INFO openai_proxy: request{method=POST uri=/v1/responses}: openai proxy request started proxy_request_id=2722 has_body=true\n",
    );

    let meta = lines[0].meta.as_ref().expect("tracing text metadata");
    assert_eq!(
        meta.message.as_deref(),
        Some("openai proxy request started")
    );
    assert_eq!(meta.attributes["target"].as_str(), Some("openai_proxy"));
    assert_eq!(meta.attributes["span"].as_str(), Some("request"));
    assert_eq!(meta.attributes["method"].as_str(), Some("POST"));
    assert_eq!(meta.attributes["uri"].as_str(), Some("/v1/responses"));
    assert_eq!(meta.attributes["proxy_request_id"].as_i64(), Some(2722));
    assert_eq!(meta.attributes["has_body"].as_bool(), Some(true));
}
