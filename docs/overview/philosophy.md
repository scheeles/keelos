# Philosophy

KeelOS is built upon four core philosophical pillars. These principles guide every design decision, from the kernel configuration to the API structure.

## 1. Immutable

In traditional systems, servers are "pets." They are nursed, patched, and tweaked over time. This leads to configuration drift, where no two servers are exactly alike, making debugging a nightmare.

KeelOS treats servers as "cattle."
*   **Read-Only Filesystem**: The root filesystem is a read-only SquashFS image. It cannot be modified by the user or by running processes.
*   **Ephemeral by Default**: Any changes made to the system (outside of designated persistent partitions) are lost on reboot.
*   **Atomic Updates**: You don't update packages; you replace the entire OS. This guarantees that if it works in testing, it works in production.

## 2. API-Oriented

We believe that **infrastructure is code**, and code should be managed by APIs, not by humans typing commands into a shell.

*   **No SSH**: SSH is a backdoor for humans to make undocumented changes. We removed it.
*   **Structured API**: Every action—checking health, scheduling updates, viewing logs—is an API call.
*   **mTLS Authentication**: Security is not an afterthought. Every request is authenticated with mutual TLS, ensuring that only authorized clients (like `osctl`) can talk to the node.

## 3. Minimal

Complexity is the enemy of security and reliability. General-purpose operating systems are bloated with legacy support and unused features.

*   **Single-Purpose**: KeelOS does one thing: host Kubernetes. Anything not required for that task is removed.
*   **No Userspace Bloat**: We don't have Systemd, Udev, D-Bus, or a package manager. We have `keel-init`, which does exactly what we need and nothing more.
*   **Small Footprint**: A smaller OS means faster boot times, less disk usage, and a smaller attack surface.

## 4. Secure

Security is not a feature; it's the default state.

*   **Attack Surface Reduction**: You can't exploit a shell vulnerability if there is no shell. You can't run a Python script payload if there is no Python.
*   **Kernel Lockdown**: We utilize the Linux kernel's lockdown mode to prevent even the root user from modifying kernel memory.
*   **Verified Integrity**: Because the OS is immutable, we can cryptographically verify that the code running on your server is exactly what you built.
