# Security

Security is the primary driver behind KeelOS's architecture. By removing the userspace and enforcing immutability, KeelOS provides a drastic reduction in attack surface compared to traditional Linux distributions.

## Core Security Features

### 1. Read-Only Filesystem
The root filesystem is mounted as read-only.
*   **Malware Resistance**: Attackers cannot write persistent malware to `/bin`, `/usr`, or `/etc`. Any attempt to modify system files will fail with `Read-only file system`.
*   **Integrity**: The system state is guaranteed to be identical to the build artifact.

### 2. No Shell, No SSH
KeelOS does not contain `bash`, `sh`, or an SSH server (`sshd`).
*   **Exploit Mitigation**: Many common exploits rely on shelling out (`/bin/sh -c ...`) to execute payloads. These attacks fail immediately on KeelOS.
*   **Access Control**: Access is strictly controlled via the gRPC API, which requires mutual TLS authentication. There is no "root password" to brute-force.

### 3. No Interpreters (LOTL)
"Living off the Land" (LOTL) attacks use existing system tools like Python, Perl, or Ruby to conduct malicious activities.
*   **Zero Interpreters**: KeelOS ships with **no** dynamic language interpreters.
*   **Minimal Binaries**: The only binaries present are those strictly required for Kubernetes (`kubelet`, `containerd`, `runc`) and the KeelOS management tools.

### 4. Kernel Lockdown
KeelOS enables the Linux Kernel Lockdown feature in "integrity" mode.
*   **Memory Protection**: Prevents even the root user from modifying kernel memory or loading unsigned kernel modules.
*   **Kexec Restrictions**: Prevents replacing the running kernel.

### 5. Mutual TLS (mTLS)
All communication between the `osctl` CLI and the `keel-agent` on the node is encrypted and mutually authenticated.
*   **Client Certificates**: You cannot talk to the API without a valid client certificate signed by the cluster CA.
*   **Encryption**: All traffic is encrypted in transit using TLS 1.3.

## Automated Updates & Rollback

Security updates are critical. KeelOS makes them safe and easy.
*   **Atomic Updates**: You don't patch a running system; you replace it. This ensures that security patches are applied cleanly and consistently across the entire cluster.
*   **Automatic Rollback**: If a security update causes a boot failure or connectivity issue, the node automatically rolls back to the previous known-good version, preventing cluster outages.
