# Configuration Schema

These schemas define the data structures used in API requests and configuration files.

## Update Schedule Object

Used when listing or creating update schedules.

| Field | Type | Description |
| :--- | :--- | :--- |
| `id` | UUID | Unique ID of the schedule. |
| `source_url` | URL | Location of the update image. |
| `status` | Enum | `pending`, `running`, `completed`, `failed`. |
| `enable_auto_rollback` | Bool | Whether rollback is active for this update. |
| `health_check_timeout_secs` | Int | Seconds to wait for healthy signal after reboot. |

## Health Check Result

Represents the result of a single system check.

| Field | Type | Description |
| :--- | :--- | :--- |
| `name` | String | Identifier (e.g., `boot`, `network`). |
| `status` | Enum | `pass`, `fail`, `unknown`. |
| `message` | String | Human-readable error or success message. |
| `duration_ms` | Int | Time taken to run the check. |

## Rollback Event

Audit record for a rollback.

| Field | Type | Description |
| :--- | :--- | :--- |
| `timestamp` | ISO8601 | When the rollback occurred. |
| `reason` | String | Why the rollback happened. |
| `from_partition` | String | The partition that failed. |
| `to_partition` | String | The partition rolled back to. |
| `automatic` | Bool | `true` if triggered by watchdog, `false` if manual. |
