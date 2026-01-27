# Local Installation (QEMU)

Running KeelOS in QEMU is the primary method for development and testing. It allows you to simulate a full KeelOS node on your local machine.

## Prerequisites

*   **QEMU**: `qemu-system-x86_64` must be installed.
    *   **macOS**: `brew install qemu`
    *   **Ubuntu/Debian**: `sudo apt install qemu-system-x86`
    *   **Fedora**: `sudo dnf install qemu-system-x86`

## Running the Instance

We provide a helper script to launch QEMU with the correct flags.

```bash
./tools/testing/run-qemu.sh
```

### Customizing the VM

You can customize the QEMU instance by modifying environment variables or the script itself.

**Environment Variables:**

| Variable | Default | Description |
| :--- | :--- | :--- |
| `EXTRA_APPEND` | *(empty)* | Additional kernel command-line arguments. |

**Example: Enabling debug logging**

```bash
EXTRA_APPEND="debug" ./tools/testing/run-qemu.sh
```

### Resource Allocation

By default, the script allocates:
*   **RAM**: 1GB (`-m 1G`)
*   **CPU**: 1 vCPU (implicit default)

To increase resources, edit `tools/testing/run-qemu.sh` to change `-m 1G` to `-m 4G` or add `-smp 4`.

## Networking

QEMU uses User Networking (`-net user`), which isolates the VM from your local network but allows outbound access.

### Port Forwarding

To access services inside the VM, we forward ports to localhost.

| VM Port | Localhost Port | Service |
| :--- | :--- | :--- |
| `50051` | `50052` | `keel-agent` (gRPC Management) |
| `10250` | *(Not forwarded)* | `kubelet` API |

**Accessing from `osctl`:**

Because of this forwarding, you must target port `50052` when running `osctl` on your host:

```bash
osctl --addr 127.0.0.1:50052 status
```

## Storage

The script creates a raw disk image `build/sda.img` if it doesn't exist. This acts as the persistent storage for the VM.

*   **Resetting State**: To wipe the VM and start fresh, delete this file:
    ```bash
    rm build/sda.img
    ```

## Troubleshooting

### "Glb: command not found" or "Exec format error"
Ensure you are running checking out the correct architecture branch or that your `qemu-system-x86_64` matches the kernel architecture.

### "Connection Refused"
The agent takes a few seconds to start. Watch the QEMU console output for:
`[   OK  ] Starting keel-agent...`
Before trying to connect with `osctl`.
