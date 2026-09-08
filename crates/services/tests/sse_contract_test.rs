//! Contract tests for SSE/WS real-time event transport.
//!
//! Exercises the **wire contract** — frame format, channel routing,
//! publisher/subscriber lifecycle, lag handling — rather than the full
//! application story. Each test targets one invariant from the SSE/WS
//! feature spec that must hold for any client (browser, CLI, integration runner).
use std::sync::Arc;

use models::{SseEvent, SseEventType, SseFrame, SseSubscription};
use services::sse_service::{InMemorySseService, SseService};
use uuid::Uuid;

#[test]
fn sse_frame_serializes_with_event_and_trailing_double_newline() {
    let frame = SseFrame::with_event("issue.updated".to_string(), "hello".to_string());
    let raw = frame.to_sse_text();
    assert!(raw.starts_with("event: issue.updated\n"));
    assert!(raw.contains("data: hello\n"));
    assert!(raw.ends_with("\n\n"));
}

#[test]
fn sse_frame_escapes_newlines_in_data() {
    let frame = SseFrame::new("line1\nline2".to_string());
    let raw = frame.to_sse_text();
    let lines: Vec<&str> = raw.lines().collect();
    assert!(lines.iter().any(|l| *l == "data: line1"));
    assert!(lines.iter().any(|l| *l == "data: line2"));
}

#[test]
fn parse_sse_frames_splits_on_double_newline() {
    let input = "event: a\ndata: one\n\ndata: two\n\n";
    let (frames, remaining) =
        services::sse_service::parse_sse_frames(input);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].event.as_deref(), Some("a"));
    assert_eq!(frames[0].data, "one");
    assert_eq!(frames[1].data, "two");
    assert_eq!(remaining, "");
}

#[test]
fn parse_sse_frames_returns_remaining_when_incomplete() {
    let input = "event: a\ndata: one"; // missing trailing \n\n
    let (frames, remaining) =
        services::sse_service::parse_sse_frames(input);
    assert_eq!(frames.len(), 0);
    assert!(!remaining.is_empty());
}

#[tokio::test]
async fn sse_service_subscriber_receives_published_events() {
    let svc = Arc::new(InMemorySseService::default());
    let company_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let channel = "issues".to_string();

    let mut rx = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id,
            channel: channel.clone(),
            last_event_id: None,
        })
        .await
        .unwrap();

    svc.publish(
        company_id,
        &channel,
        SseEvent {
            event_type: SseEventType::Message,
            channel: channel.clone(),
            payload: serde_json::json!({"text": "issue created"}),
            timestamp: chrono::Utc::now(),
        },
    )
    .await
    .unwrap();

    let frame = rx.recv().await.unwrap();
    assert_eq!(frame.event, Some("message".to_string()));
}

#[tokio::test]
async fn sse_service_lagged_messages_are_dropped_not_blocked() {
    let svc = Arc::new(InMemorySseService::default());
    let company_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let channel = "fast".to_string();

    let mut rx = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id,
            channel: channel.clone(),
            last_event_id: None,
        })
        .await
        .unwrap();

    for i in 0..10 {
        svc.publish(
            company_id,
            &channel,
            SseEvent {
                event_type: SseEventType::Message,
                channel: channel.clone(),
                payload: serde_json::json!({"i": i}),
                timestamp: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();
    }

    tokio::select! {
        _ = rx.recv() => {},
        _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
            panic!("subscriber blocked on lagged events");
        }
    }
}

#[tokio::test]
async fn sse_service_channel_isolation() {
    let svc = Arc::new(InMemorySseService::default());
    let company_id = Uuid::new_v4();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    let mut rx_a = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id: a,
            channel: "alpha".to_string(),
            last_event_id: None,
        })
        .await
        .unwrap();
    let mut rx_b = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id: b,
            channel: "beta".to_string(),
            last_event_id: None,
        })
        .await
        .unwrap();

    svc.publish(
        company_id,
        "alpha",
        SseEvent {
            event_type: SseEventType::Message,
            channel: "alpha".to_string(),
            payload: serde_json::json!({"text": "only-alpha"}),
            timestamp: chrono::Utc::now(),
        },
    )
    .await
    .unwrap();

    let frame = rx_a.recv().await.unwrap();
    assert_eq!(frame.event, Some("message".to_string()));

    let got_beta = tokio::time::timeout(
        tokio::time::Duration::from_millis(200),
        rx_b.recv(),
    )
    .await;
    assert!(
        got_beta.is_err(),
        "beta received alpha's event — channels are not isolated"
    );
}

#[tokio::test]
async fn sse_service_subscriber_count_reflects_active_channels() {
    let svc = InMemorySseService::default();
    let company_id = Uuid::new_v4();

    assert_eq!(svc.subscriber_count(company_id, "x").await, 0);

    let _rx = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id: Uuid::new_v4(),
            channel: "x".to_string(),
            last_event_id: None,
        })
        .await
        .unwrap();

    assert_eq!(svc.subscriber_count(company_id, "x").await, 1);
    assert_eq!(svc.subscriber_count(company_id, "y").await, 0);
}

#[tokio::test]
async fn sse_service_heartbeat_propagates() {
    let svc = Arc::new(InMemorySseService::default());
    let company_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let channel = "health".to_string();

    let mut rx = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id,
            channel: channel.clone(),
            last_event_id: None,
        })
        .await
        .unwrap();

    for _ in 0..5 {
        svc.publish(
            company_id,
            &channel,
            SseEvent {
                event_type: SseEventType::Heartbeat,
                channel: channel.clone(),
                payload: serde_json::json!({"alive": true}),
                timestamp: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();
    }

    let frame = rx.recv().await.unwrap();
    assert_eq!(frame.event, Some("heartbeat".to_string()));
}

#[test]
fn sse_stream_event_types_all_map_to_frames() {
    // Log event
    let log_event = services::sse_service::SseStreamEvent::Log {
        stream: "stdout".to_string(),
        chunk: "task output".to_string(),
    };
    let frame = log_event.to_frame();
    assert_eq!(frame.event, Some("log".to_string()));
    assert!(frame.data.contains("stdout"));
    assert!(frame.data.contains("task output"));

    // Terminal event — maps to "run.complete" per implementation
    let term_event = services::sse_service::SseStreamEvent::Terminal {
        status: "done".to_string(),
        exit_code: Some(0),
        message: Some("ok".to_string()),
    };
    let frame = term_event.to_frame();
    assert_eq!(frame.event, Some("run.complete".to_string()));
    assert!(frame.data.contains("done"));

    // Delta event — maps to "message.delta"
    let delta_event = services::sse_service::SseStreamEvent::Delta {
        text: "partial".to_string(),
        index: None,
    };
    let frame = delta_event.to_frame();
    assert_eq!(frame.event, Some("message.delta".to_string()));
    assert!(frame.data.contains("partial"));
}
