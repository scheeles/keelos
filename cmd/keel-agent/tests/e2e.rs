//! End-to-end tests for keel-agent gRPC service
//!
//! These tests start a real gRPC server and connect a client, exercising the
//! full request/response lifecycle over the transport layer.

use keel_api::node::node_service_client::NodeServiceClient;
use keel_api::node::node_service_server::NodeServiceServer;
use keel_api::node::{
    CancelScheduledUpdateRequest, GetHealthRequest, GetRollbackHistoryRequest, GetStatusRequest,
    GetUpdateScheduleRequest, RebootRequest, ScheduleUpdateRequest,
};
use std::net::SocketAddr;
use tonic::transport::{Channel, Server};

/// Start a gRPC server on an ephemeral port and return the address.
///
/// The server runs in a background task. The returned address can be used to
/// connect a client.
async fn start_test_server() -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let scheduler = std::sync::Arc::new(keel_agent::UpdateScheduler::new(format!(
        "/tmp/keel-e2e-{}.json",
        addr.port()
    )));
    let health_checker = std::sync::Arc::new(keel_agent::HealthChecker::new(
        keel_agent::HealthCheckerConfig::default(),
    ));

    let service = keel_agent::HelperNodeService {
        scheduler,
        health_checker,
    };

    tokio::spawn(async move {
        Server::builder()
            .add_service(NodeServiceServer::new(service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .ok();
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    Ok(addr)
}

/// Connect a gRPC client to the given address.
async fn connect_client(
    addr: SocketAddr,
) -> Result<NodeServiceClient<Channel>, Box<dyn std::error::Error>> {
    let channel = Channel::from_shared(format!("http://{addr}"))?
        .connect()
        .await?;
    Ok(NodeServiceClient::new(channel))
}

/// Cleanup helper that removes the schedule file used by a test server.
fn cleanup_schedule_file(port: u16) {
    let _ = std::fs::remove_file(format!("/tmp/keel-e2e-{port}.json"));
}

// ---- Tests ----

#[tokio::test]
async fn e2e_get_status() -> Result<(), Box<dyn std::error::Error>> {
    let addr = start_test_server().await?;
    let mut client = connect_client(addr).await?;

    let response = client.get_status(GetStatusRequest {}).await?;
    let status = response.into_inner();

    assert_eq!(status.hostname, "keel-node");
    assert_eq!(status.os_version, "0.1.0");
    assert!(!status.kernel_version.is_empty());

    cleanup_schedule_file(addr.port());
    Ok(())
}

#[tokio::test]
async fn e2e_reboot() -> Result<(), Box<dyn std::error::Error>> {
    let addr = start_test_server().await?;
    let mut client = connect_client(addr).await?;

    let response = client
        .reboot(RebootRequest {
            reason: "e2e test reboot".to_string(),
        })
        .await?;

    assert!(response.into_inner().scheduled);

    cleanup_schedule_file(addr.port());
    Ok(())
}

#[tokio::test]
async fn e2e_get_health() -> Result<(), Box<dyn std::error::Error>> {
    let addr = start_test_server().await?;
    let mut client = connect_client(addr).await?;

    let response = client.get_health(GetHealthRequest {}).await?;
    let health = response.into_inner();

    // Status should be one of the valid values
    assert!(
        health.status == "healthy" || health.status == "degraded" || health.status == "unhealthy"
    );
    // Should have at least one health check result
    assert!(!health.checks.is_empty());
    // last_update_time should be populated
    assert!(!health.last_update_time.is_empty());

    cleanup_schedule_file(addr.port());
    Ok(())
}

#[tokio::test]
async fn e2e_schedule_update_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let addr = start_test_server().await?;
    let mut client = connect_client(addr).await?;

    // 1. Schedule an update
    let schedule_resp = client
        .schedule_update(ScheduleUpdateRequest {
            source_url: "http://example.com/update.squashfs".to_string(),
            expected_sha256: "abc123def456".to_string(),
            scheduled_at: "2099-01-01T00:00:00Z".to_string(),
            maintenance_window_secs: 3600,
            enable_auto_rollback: true,
            health_check_timeout_secs: 120,
            pre_update_hook: String::new(),
            post_update_hook: String::new(),
            is_delta: false,
            fallback_to_full: false,
            full_image_url: String::new(),
        })
        .await?;

    let schedule = schedule_resp.into_inner();
    assert!(!schedule.schedule_id.is_empty());
    assert_eq!(schedule.status, "pending");
    assert!(!schedule.scheduled_at.is_empty());

    let schedule_id = schedule.schedule_id.clone();

    // 2. Verify the schedule appears in the list
    let list_resp = client
        .get_update_schedule(GetUpdateScheduleRequest {})
        .await?;
    let schedules = list_resp.into_inner().schedules;
    assert_eq!(schedules.len(), 1);

    let listed = &schedules[0];
    assert_eq!(listed.id, schedule_id);
    assert_eq!(listed.source_url, "http://example.com/update.squashfs");
    assert_eq!(listed.status, "pending");
    assert!(listed.enable_auto_rollback);

    // 3. Cancel the schedule
    let cancel_resp = client
        .cancel_scheduled_update(CancelScheduledUpdateRequest {
            schedule_id: schedule_id.clone(),
        })
        .await?;
    let cancel = cancel_resp.into_inner();
    assert!(cancel.success);

    // 4. Verify the schedule shows as cancelled
    let list_resp2 = client
        .get_update_schedule(GetUpdateScheduleRequest {})
        .await?;
    let schedules2 = list_resp2.into_inner().schedules;
    let cancelled = schedules2
        .iter()
        .find(|s| s.id == schedule_id)
        .ok_or("Cancelled schedule not found")?;
    assert_eq!(cancelled.status, "cancelled");

    cleanup_schedule_file(addr.port());
    Ok(())
}

#[tokio::test]
async fn e2e_cancel_nonexistent_schedule() -> Result<(), Box<dyn std::error::Error>> {
    let addr = start_test_server().await?;
    let mut client = connect_client(addr).await?;

    let resp = client
        .cancel_scheduled_update(CancelScheduledUpdateRequest {
            schedule_id: "nonexistent-id".to_string(),
        })
        .await?;

    let cancel = resp.into_inner();
    assert!(!cancel.success);
    assert!(!cancel.message.is_empty());

    cleanup_schedule_file(addr.port());
    Ok(())
}

#[tokio::test]
async fn e2e_get_rollback_history_empty() -> Result<(), Box<dyn std::error::Error>> {
    let addr = start_test_server().await?;
    let mut client = connect_client(addr).await?;

    let resp = client
        .get_rollback_history(GetRollbackHistoryRequest {})
        .await?;
    let history = resp.into_inner();

    // Fresh server should have no rollback events
    assert!(history.events.is_empty());

    cleanup_schedule_file(addr.port());
    Ok(())
}

#[tokio::test]
async fn e2e_schedule_update_invalid_time() -> Result<(), Box<dyn std::error::Error>> {
    let addr = start_test_server().await?;
    let mut client = connect_client(addr).await?;

    let result = client
        .schedule_update(ScheduleUpdateRequest {
            source_url: "http://example.com/update.squashfs".to_string(),
            expected_sha256: String::new(),
            scheduled_at: "not-a-valid-timestamp".to_string(),
            maintenance_window_secs: 0,
            enable_auto_rollback: false,
            health_check_timeout_secs: 0,
            pre_update_hook: String::new(),
            post_update_hook: String::new(),
            is_delta: false,
            fallback_to_full: false,
            full_image_url: String::new(),
        })
        .await;

    // Should fail with INVALID_ARGUMENT
    assert!(result.is_err());
    let status = result.err().ok_or("Expected error")?;
    assert_eq!(status.code(), tonic::Code::InvalidArgument);

    cleanup_schedule_file(addr.port());
    Ok(())
}

#[tokio::test]
async fn e2e_schedule_multiple_updates() -> Result<(), Box<dyn std::error::Error>> {
    let addr = start_test_server().await?;
    let mut client = connect_client(addr).await?;

    // Schedule two updates
    let resp1 = client
        .schedule_update(ScheduleUpdateRequest {
            source_url: "http://example.com/v1.squashfs".to_string(),
            expected_sha256: String::new(),
            scheduled_at: "2099-06-01T00:00:00Z".to_string(),
            maintenance_window_secs: 1800,
            enable_auto_rollback: true,
            health_check_timeout_secs: 0,
            pre_update_hook: String::new(),
            post_update_hook: String::new(),
            is_delta: false,
            fallback_to_full: false,
            full_image_url: String::new(),
        })
        .await?;

    let resp2 = client
        .schedule_update(ScheduleUpdateRequest {
            source_url: "http://example.com/v2.squashfs".to_string(),
            expected_sha256: String::new(),
            scheduled_at: "2099-07-01T00:00:00Z".to_string(),
            maintenance_window_secs: 0,
            enable_auto_rollback: false,
            health_check_timeout_secs: 60,
            pre_update_hook: String::new(),
            post_update_hook: String::new(),
            is_delta: true,
            fallback_to_full: true,
            full_image_url: "http://example.com/v2-full.squashfs".to_string(),
        })
        .await?;

    let id1 = resp1.into_inner().schedule_id;
    let id2 = resp2.into_inner().schedule_id;
    assert_ne!(id1, id2);

    // Verify both appear in the list
    let list = client
        .get_update_schedule(GetUpdateScheduleRequest {})
        .await?
        .into_inner()
        .schedules;
    assert_eq!(list.len(), 2);

    // Cancel the first one, verify the second is still pending
    client
        .cancel_scheduled_update(CancelScheduledUpdateRequest {
            schedule_id: id1.clone(),
        })
        .await?;

    let list2 = client
        .get_update_schedule(GetUpdateScheduleRequest {})
        .await?
        .into_inner()
        .schedules;

    let s1 = list2
        .iter()
        .find(|s| s.id == id1)
        .ok_or("Schedule 1 not found")?;
    let s2 = list2
        .iter()
        .find(|s| s.id == id2)
        .ok_or("Schedule 2 not found")?;
    assert_eq!(s1.status, "cancelled");
    assert_eq!(s2.status, "pending");

    cleanup_schedule_file(addr.port());
    Ok(())
}
