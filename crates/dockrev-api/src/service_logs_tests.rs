use super::{
    LineChunkBuffer, MAX_PENDING_SERVICE_LOG_BYTES, ServiceLogFrameParser, parse_service_log_lines,
    take_bounded_utf8_prefix,
};
use crate::api::types::ServiceLogMetaFormat;
use tokio::sync::mpsc;

#[test]
fn parse_service_log_lines_drops_truncated_leading_continuation() {
    let lines = parse_service_log_lines(
        "2026-07-01T08:12:51.833074000Z Caused by:\n\
         2026-07-01T08:12:51.833081000Z     (code: 5) database is locked\n\
         2026-07-01T08:12:53.763043000Z worker ready\n",
    );

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].raw, "worker ready");
    assert_eq!(lines[0].ts, "2026-07-01T08:12:53.763043000Z");
}

#[test]
fn parse_service_log_lines_preserves_leading_indented_entry() {
    let lines = parse_service_log_lines(
        "2026-07-01T08:12:51.833081000Z     standalone indented output\n\
         2026-07-01T08:12:53.763043000Z worker ready\n",
    );

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].raw, "    standalone indented output");
    assert_eq!(lines[1].raw, "worker ready");
}

#[test]
fn line_chunk_buffer_bounds_newline_free_output_without_loss() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut buffer = LineChunkBuffer::default();
    buffer.push(&tx, &vec![b'x'; MAX_PENDING_SERVICE_LOG_BYTES + 1]);

    let emitted = rx
        .try_recv()
        .expect("oversized partial output should flush");
    assert_eq!(emitted.text.len(), MAX_PENDING_SERVICE_LOG_BYTES);
    assert!(emitted.forced_fragment);
    buffer.finish(&tx);
    assert_eq!(
        rx.try_recv().expect("tail should be preserved").text.len(),
        1
    );
    assert!(buffer.pending.is_empty());
}

#[test]
fn line_chunk_buffer_keeps_utf8_code_points_intact_at_forced_boundary() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut buffer = LineChunkBuffer::default();
    let mut chunk = vec![b'x'; MAX_PENDING_SERVICE_LOG_BYTES - 1];
    chunk.extend_from_slice("界".as_bytes());
    chunk.extend_from_slice("尾".as_bytes());
    buffer.push(&tx, &chunk);

    let emitted = rx
        .try_recv()
        .expect("oversized partial output should flush");
    assert!(emitted.forced_fragment);
    assert!(emitted.text.ends_with('x'));
    assert!(!emitted.text.contains('\u{fffd}'));
    buffer.finish(&tx);
    let tail = rx.try_recv().expect("utf8 tail should be preserved");
    assert_eq!(tail.text, "界尾");
    assert!(!tail.forced_fragment);
}

#[test]
fn bounded_utf8_prefix_leaves_incomplete_code_point_for_next_chunk() {
    let mut bytes = "x界".as_bytes().to_vec();
    let prefix = take_bounded_utf8_prefix(&mut bytes, 2);
    assert_eq!(prefix, b"x");
    assert_eq!(bytes, "界".as_bytes());
}

#[test]
fn service_log_parser_rejoins_bounded_continuation_chunks() {
    let timestamp = "2026-07-01T08:12:51.833074000Z ";
    let first_chunk = format!("{timestamp}{}", "x".repeat(MAX_PENDING_SERVICE_LOG_BYTES));
    let mut parser = ServiceLogFrameParser::default();

    assert!(parser.push_physical_line(&first_chunk, false).is_none());
    assert!(parser.push_physical_line("tail", true).is_none());

    let line = parser
        .finish()
        .expect("continuation should complete the line");
    assert!(line.raw.ends_with("tail"));
}

#[test]
fn line_chunk_buffer_splits_carriage_return_progress() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut buffer = LineChunkBuffer::default();
    buffer.push(&tx, b"first\rsecond");

    assert_eq!(
        rx.try_recv().expect("carriage return should flush").text,
        "first"
    );
    buffer.finish(&tx);
    assert_eq!(
        rx.try_recv().expect("final partial line should flush").text,
        "second"
    );
}

#[test]
fn service_log_parser_separates_live_unstamped_continuations() {
    let mut parser = ServiceLogFrameParser::default();
    assert!(
        parser
            .push_physical_line("2026-07-01T08:12:51.833074000Z worker failed", false)
            .is_none()
    );
    assert!(
        parser
            .push_physical_line("    database is locked", false)
            .is_none()
    );
    let line = parser
        .finish()
        .expect("continuation should complete the line");
    assert_eq!(line.raw, "worker failed\n    database is locked");
}

#[test]
fn parse_service_log_lines_removes_only_docker_separator_space() {
    let lines = parse_service_log_lines(
        "2026-07-01T08:12:51.833063000Z worker ready\n\
         2026-07-01T08:12:51.833070000Z \n\
         2026-07-01T08:12:51.833081000Z     standalone indented output\n",
    );

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].raw, "worker ready\n");
    assert_eq!(lines[1].raw, "    standalone indented output");
}

#[test]
fn parse_service_log_lines_adds_json_metadata() {
    let lines = parse_service_log_lines(
        "2026-07-06T16:15:16.433978000Z {\"timestamp\":\"2026-07-06T16:15:16.433978Z\",\"level\":\"INFO\",\"message\":\"runtime perf\",\"component\":\"admin_read\",\"event\":\"dashboard_overview_phase\",\"elapsed_ms\":24,\"route\":\"/api/dashboard/overview\"}\n",
    );

    assert_eq!(lines.len(), 1);
    let meta = lines[0].meta.as_ref().expect("json metadata");
    assert_eq!(meta.format, ServiceLogMetaFormat::Json);
    assert_eq!(meta.level.as_deref(), Some("info"));
    assert_eq!(
        meta.timestamp.as_deref(),
        Some("2026-07-06T16:15:16.433978Z")
    );
    assert_eq!(meta.message.as_deref(), Some("runtime perf"));
    assert_eq!(meta.attributes["component"].as_str(), Some("admin_read"));
    assert_eq!(
        meta.attributes["event"].as_str(),
        Some("dashboard_overview_phase")
    );
    assert_eq!(meta.attributes["elapsed_ms"].as_i64(), Some(24));
    assert!(meta.highlights.contains(&"component".to_string()));
    assert!(meta.highlights.contains(&"event".to_string()));
}

#[test]
fn parse_service_log_lines_adds_logfmt_metadata() {
    let lines = parse_service_log_lines(
        "2026-07-06T16:15:16.433978000Z level=warn msg=\"slow query\" route=/api/services elapsed_ms=242 degraded=true\n",
    );

    let meta = lines[0].meta.as_ref().expect("logfmt metadata");
    assert_eq!(meta.format, ServiceLogMetaFormat::Logfmt);
    assert_eq!(meta.level.as_deref(), Some("warn"));
    assert_eq!(meta.message.as_deref(), Some("slow query"));
    assert_eq!(meta.attributes["route"].as_str(), Some("/api/services"));
    assert_eq!(meta.attributes["elapsed_ms"].as_i64(), Some(242));
    assert_eq!(meta.attributes["degraded"].as_bool(), Some(true));
}

#[test]
fn parse_service_log_lines_falls_back_to_text_metadata() {
    let lines = parse_service_log_lines("2026-07-06T16:15:16.433978000Z worker ready\n");

    let meta = lines[0].meta.as_ref().expect("text metadata");
    assert_eq!(meta.format, ServiceLogMetaFormat::Text);
    assert_eq!(meta.message.as_deref(), Some("worker ready"));
    assert!(meta.attributes.is_empty());
}
