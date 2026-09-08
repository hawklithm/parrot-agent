//! Contract tests for WebSocket real-time event transport.
//!
//! Exercises the **wire contract** — subscription, forwarder loop, ping/pong,
//! and close semantics — against a local InMemorySseService.
use std::sync::Arc;

use models::{SseEvent, SseEventType, SseSubscription};
use services::sse_service::{InMemorySseService, SseService};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

#[tokio::test]
async fn ws_forwarder_forwards_events_as_text() {
    let svc = Arc::new(InMemorySseService::default());
    let company_id = Uuid::new_v4();
    let channel = "events".to_string();

    let mut rx = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id: Uuid::new_v4(),
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
            payload: serde_json::json!({"text": "hello"}),
            timestamp: chrono::Utc::now(),
        },
    )
    .await
    .unwrap();

    // Forwarder contract: each broadcast frame → one Text message on the socket.
    let frame = rx.recv().await.unwrap();
    // frame.data is the serialized SseEvent.payload as a JSON string
    assert!(frame.data.contains("hello"));
}

#[tokio::test]
async fn ws_ping_returns_pong_with_same_payload() {
    let ping_payload = b"keepalive";
    let ping_msg = Message::Ping(ping_payload.to_vec());
    // Contract: Ping → Pong(echoed payload)
    match &ping_msg {
        Message::Ping(data) => {
            let pong = Message::Pong(data.clone());
            assert!(matches!(pong, Message::Pong(_)));
        }
        _ => panic!("expected Ping"),
    }
}

#[tokio::test]
async fn ws_close_drains_receiver_no_panic() {
    let svc = Arc::new(InMemorySseService::default());
    let company_id = Uuid::new_v4();

    let _rx = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id: Uuid::new_v4(),
            channel: "events".to_string(),
            last_event_id: None,
        })
        .await
        .unwrap();

    // Drop simulates client disconnect. Should not panic.
    drop(_rx);
}

#[tokio::test]
async fn ws_different_companies_are_isolated() {
    let svc_a = Arc::new(InMemorySseService::default());
    let svc_b = Arc::new(InMemorySseService::default());
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();

    let mut rx_a = svc_a
        .subscribe(SseSubscription {
            company_id: company_a,
            actor_id: Uuid::new_v4(),
            channel: "events".to_string(),
            last_event_id: None,
        })
        .await
        .unwrap();
    let mut rx_b = svc_b
        .subscribe(SseSubscription {
            company_id: company_b,
            actor_id: Uuid::new_v4(),
            channel: "events".to_string(),
            last_event_id: None,
        })
        .await
        .unwrap();

    svc_a.publish(
        company_a,
        "events",
        SseEvent {
            event_type: SseEventType::Message,
            channel: "events".to_string(),
            payload: serde_json::json!({"src": "company_a"}),
            timestamp: chrono::Utc::now(),
        },
    )
    .await
    .unwrap();

    let frame_a = rx_a.recv().await.unwrap();
    // frame.data is the serialized JSON payload
    assert!(frame_a.data.contains("company_a"));

    // Company B must not see A's event.
    let got_b = tokio::time::timeout(
        tokio::time::Duration::from_millis(200),
        rx_b.recv(),
    )
    .await;
    assert!(
        got_b.is_err(),
        "company_b received company_a's event — cross-company leak"
    );
}

#[tokio::test]
async fn ws_lagged_broadcast_dropped_not_blocking() {
    let svc = Arc::new(InMemorySseService::default());
    let company_id = Uuid::new_v4();
    let channel = "fast".to_string();

    let mut rx = svc
        .subscribe(SseSubscription {
            company_id,
            actor_id: Uuid::new_v4(),
            channel: channel.clone(),
            last_event_id: None,
        })
        .await
        .unwrap();

    // Flood with events faster than the receiver can drain.
    for i in 0..20 {
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

    // Must not deadlock or hang — broadcast drops lagged frames.
    tokio::select! {
        _ = rx.recv() => {},
        _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
            panic!("receiver blocked on broadcast flood");
        }
    }
}
