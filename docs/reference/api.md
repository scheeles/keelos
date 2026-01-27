# API Reference

The KeelOS API is defined using Protocol Buffers (v3). Services are exposed on port `50051` by default.

## Service: `NodeService`

The core service for node management.

### RPC Methods

#### `GetStatus`
Returns static information about the node.
*   **Request**: `GetStatusRequest` (Empty)
*   **Response**: `GetStatusResponse`
    *   `hostname` (string)
    *   `kernel_version` (string)
    *   `os_version` (string)
    *   `uptime_seconds` (float)

#### `GetHealth`
Returns dynamic health status.
*   **Request**: `GetHealthRequest` (Empty)
*   **Response**: `GetHealthResponse`
    *   `status` (string): "healthy", "degraded", "unhealthy"
    *   `checks` (repeated `HealthCheckResult`): List of specific checks.

#### `InstallUpdate`
Streams the installation of an update.
*   **Request**: `InstallUpdateRequest`
    *   `source_url` (string): URL/Path to image.
    *   `expected_sha256` (string): Checksum for verification.
*   **Response**: (Stream) `UpdateProgress`
    *   `percentage` (int): 0-100.
    *   `message` (string): Current step description.

#### `ScheduleUpdate`
Schedules an update operation.
*   **Request**: `ScheduleUpdateRequest`
    *   `source_url` (string)
    *   `scheduled_at` (string): RFC3339 timestamp.
    *   `enable_auto_rollback` (bool): If true, enables watchdog fallback.
    *   `health_check_timeout_secs` (int): Time to wait for health before rolling back.

#### `Reboot`
Safely reboots the machine.
*   **Request**: `RebootRequest`
    *   `reason` (string): Audit log reason.

#### `TriggerRollback`
Manually reverts to the previous partition.
*   **Request**: `TriggerRollbackRequest`
    *   `reason` (string)
*   **Response**: `TriggerRollbackResponse`
    *   `success` (bool)
    *   `message` (string)

#### `GetRollbackHistory`
Returns a list of past rollback events.
*   **Response**: `GetRollbackHistoryResponse`
    *   `events` (repeated `RollbackEvent`)

---

### Certificate Management

#### `RenewCertificate`
Manually triggers server certificate rotation.
*   **Request**: `RenewCertificateRequest` (Empty)
*   **Response**: `RenewCertificateResponse`
    *   `success` (bool): Whether rotation succeeded
    *   `message` (string): Status message or error description
    *   `not_after` (string): New certificate expiry (RFC3339 format)

**Use Cases:**
- Emergency rotation after security incident
- Testing certificate rotation mechanism
- Proactive renewal before maintenance

**Example Request:**
```bash
osctl cert renew
```

**Example Response:**
```json
{
  "success": true,
  "message": "Certificate renewed successfully",
  "not_after": "2026-04-27T20:14:00Z"
}
```

#### `GetCertificateInfo`
Retrieves information about the current server certificate.
*   **Request**: `GetCertificateInfoRequest` (Empty)
*   **Response**: `GetCertificateInfoResponse`
    *   `common_name` (string): Certificate CN (e.g., "keel-agent")
    *   `not_before` (string): Valid from (RFC3339)
    *   `not_after` (string): Valid until (RFC3339)
    *   `days_until_expiry` (int64): Days remaining until expiry
    *   `is_expiring_soon` (bool): True if < 30 days until expiry

**Use Cases:**
- Monitoring certificate expiry
- Verifying automatic rotation is working
- Alerting before expiry

**Example Request:**
```bash
osctl cert status
```

**Example Response:**
```json
{
  "common_name": "keel-agent",
  "not_before": "2026-01-27T20:14:00Z",
  "not_after": "2026-04-27T20:14:00Z",
  "days_until_expiry": 89,
  "is_expiring_soon": false
}
```

#### `GetCACertificate`
Returns the Certificate Authority (CA) certificate in PEM format.
*   **Request**: `GetCACertificateRequest` (Empty)
*   **Response**: `GetCACertificateResponse`
    *   `ca_cert_pem` (string): CA certificate in PEM format

**Use Cases:**
- Client enrollment (distribute CA to new clients)
- Certificate verification
- Trust establishment

**Example Request:**
```bash
osctl cert get-ca --output ca.pem
```

**Example Response:**
```json
{
  "ca_cert_pem": "-----BEGIN CERTIFICATE-----\nMIIBkTCCAT...\n-----END CERTIFICATE-----"
}
```

---

## Security

All API endpoints require mTLS authentication. See the [mTLS Certificate Rotation Guide](../security/mtls-certificate-rotation.md) for details on certificate management.
