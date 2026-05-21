//! Tests for interview room ownership, stopping, and replay data.

use anyhow::Result;
use common::*;
use operational_transform::OperationSeq;
use rustpad_server::{database::Database, server, ServerConfig};
use serde_json::json;
use tempfile::NamedTempFile;

pub mod common;

fn temp_sqlite_uri() -> Result<String> {
    Ok(format!(
        "sqlite://{}",
        NamedTempFile::new()?
            .into_temp_path()
            .as_os_str()
            .to_str()
            .expect("failed to get name of tempfile as &str")
    ))
}

#[tokio::test]
async fn test_host_can_stop_room() -> Result<()> {
    pretty_env_logger::try_init().ok();
    let filter = server(ServerConfig::default());

    let mut host = connect(&filter, "interview").await?;
    assert_eq!(host.recv().await?, json!({ "Identity": 0 }));
    host.send(&json!({
        "Join": {
            "info": { "name": "Host", "hue": 42 },
            "host_token": "secret"
        }
    }))
    .await;
    let room_state = host.recv().await?;
    assert_eq!(room_state["RoomState"]["is_host"], true);
    assert_eq!(room_state["RoomState"]["host_token"], "secret");
    host.recv().await?; // Own UserInfo broadcast.

    let mut guest = connect(&filter, "interview").await?;
    assert_eq!(guest.recv().await?, json!({ "Identity": 1 }));
    guest.recv().await?; // Existing host UserInfo.
    guest
        .send(&json!({
            "Join": {
                "info": { "name": "Guest", "hue": 96 },
                "host_token": null
            }
        }))
        .await;
    let guest_state = guest.recv().await?;
    assert_eq!(guest_state["RoomState"]["is_host"], false);
    assert_eq!(
        guest_state["RoomState"]["host_token"],
        serde_json::Value::Null
    );
    guest.recv().await?; // Own UserInfo broadcast.
    host.recv().await?; // Guest UserInfo broadcast.

    guest
        .send(&json!({ "StopRoom": { "host_token": "wrong" } }))
        .await;
    guest.recv_closed().await?;
    host.recv().await?; // Guest disconnect broadcast.

    host.send(&json!({ "StopRoom": { "host_token": "secret" } }))
        .await;
    let closed = host.recv().await?;
    assert!(closed.get("RoomClosed").is_some());

    let mut operation = OperationSeq::default();
    operation.insert("after stop");
    host.send(&json!({
        "Edit": {
            "revision": 0,
            "operation": operation
        }
    }))
    .await;
    expect_text(&filter, "interview", "").await;

    let replay = replay(&filter, "interview").await;
    assert!(replay["events"]
        .as_array()
        .expect("events should be an array")
        .iter()
        .any(|event| event["type"] == "Closed"));

    Ok(())
}

#[tokio::test]
async fn test_replay_survives_restart() -> Result<()> {
    pretty_env_logger::try_init().ok();
    let database = Database::new(&temp_sqlite_uri()?).await?;
    let filter = server(ServerConfig {
        expiry_days: 2,
        database: Some(database.clone()),
    });

    let mut host = connect(&filter, "persist-replay").await?;
    assert_eq!(host.recv().await?, json!({ "Identity": 0 }));
    host.send(&json!({
        "Join": {
            "info": { "name": "Host", "hue": 42 },
            "host_token": "secret"
        }
    }))
    .await;
    host.recv().await?; // RoomState.
    host.recv().await?; // UserInfo.

    let mut operation = OperationSeq::default();
    operation.insert("hello");
    host.send(&json!({
        "Edit": {
            "revision": 0,
            "operation": operation
        }
    }))
    .await;
    host.recv().await?; // History.
    host.send(&json!({ "StopRoom": { "host_token": "secret" } }))
        .await;
    host.recv().await?; // RoomClosed.

    let restarted = server(ServerConfig {
        expiry_days: 2,
        database: Some(database),
    });
    expect_text(&restarted, "persist-replay", "hello").await;
    let replay = replay(&restarted, "persist-replay").await;
    assert_eq!(replay["final_text"], "hello");
    assert!(replay["closed_at"].is_number());
    assert!(replay["events"]
        .as_array()
        .expect("events should be an array")
        .iter()
        .any(|event| event["type"] == "Edit"));

    Ok(())
}

#[tokio::test]
async fn test_live_history_survives_restart() -> Result<()> {
    pretty_env_logger::try_init().ok();
    let database = Database::new(&temp_sqlite_uri()?).await?;
    let filter = server(ServerConfig {
        expiry_days: 2,
        database: Some(database.clone()),
    });

    let mut client = connect(&filter, "persist-live").await?;
    assert_eq!(client.recv().await?, json!({ "Identity": 0 }));
    client
        .send(&json!({
            "Join": {
                "info": { "name": "Host", "hue": 42 },
                "host_token": "secret"
            }
        }))
        .await;
    client.recv().await?; // RoomState.
    client.recv().await?; // UserInfo.

    let mut operation = OperationSeq::default();
    operation.insert("hello");
    client
        .send(&json!({
            "Edit": {
                "revision": 0,
                "operation": operation
            }
        }))
        .await;
    client.recv().await?; // History.

    let mut operation = OperationSeq::default();
    operation.retain(5);
    operation.insert(" world");
    client
        .send(&json!({
            "Edit": {
                "revision": 1,
                "operation": operation
            }
        }))
        .await;
    client.recv().await?; // History.

    let restarted = server(ServerConfig {
        expiry_days: 2,
        database: Some(database),
    });
    expect_text(&restarted, "persist-live", "hello world").await;

    let mut reconnected = connect(&restarted, "persist-live").await?;
    assert_eq!(reconnected.recv().await?, json!({ "Identity": 0 }));
    let history = reconnected.recv().await?;
    assert_eq!(
        history["History"]["operations"].as_array().unwrap().len(),
        2
    );

    let mut operation = OperationSeq::default();
    operation.retain(11);
    operation.insert("!");
    reconnected
        .send(&json!({
            "Edit": {
                "revision": 2,
                "operation": operation
            }
        }))
        .await;
    reconnected.recv().await?; // History.
    expect_text(&restarted, "persist-live", "hello world!").await;

    Ok(())
}
