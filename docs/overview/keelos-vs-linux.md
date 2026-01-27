# KeelOS vs. Traditional Linux

KeelOS is not just a "stripped-down" Linux; it's a rethinking of what a server OS should be in the age of Kubernetes. Here is how it compares to traditional distributions like Ubuntu, Debian, or RHEL.

## Comparison overview

| Feature | Traditional Linux (Ubuntu/RHEL) | KeelOS |
| :--- | :--- | :--- |
| **Primary Interface** | SSH / Shell / Console | gRPC API / `osctl` |
| **Init System** | Systemd | `keel-init` (Custom, minimal) |
| **Update Mechanism** | Package Manager (`apt`, `yum`) | Atomic Image Swap (A/B) |
| **Filesystem state** | Mutable (Read/Write) | Immutable (Read-Only) |
| **Configuration** | File-based (`/etc/*`) | API-driven (Runtime & Boot) |
| **Security Model** | User/Password, Sudo | mTLS, Kernel Lockdown |
| **Typical Size** | 500MB - 2GB+ | < 100MB |

## Key Differences Explained

### 1. No Systemd
Traditional Linux uses Systemd to manage services. Systemd is powerful but complex, managing everything from logging to network naming.
**KeelOS** uses `keel-init`, a single-binary init system written in Rust. It has one job: bootstrap the system and start the `keel-agent`, `containerd`, and `kubelet`. It doesn't need to support complex dependency trees for desktop services.

### 2. No Package Manager
On Ubuntu, you run `apt-get upgrade` to update software. This changes libraries and binaries in place, potentially breaking running applications or leaving the system in an inconsistent state if the update is interrupted.
**KeelOS** has no package manager. To update, you push a completely new OS image to a designated partition and reboot. If the new image fails, the system reverts to the old one.

### 3. No SSH
SSH is the standard way to manage Linux servers. It's also the standard vector for attacks and configuration drift.
**KeelOS** removes SSH entirely. Instead, you use `osctl` to interact with the `keel-agent` API. This allows you to inspect logs, check status, and reboot nodes, but in a structured, audited, and secure way.

### 4. No Interpreters
Traditional Linux comes with Python, Perl, and often Ruby to support system tools. Attackers love these because they facilitate running complex exploit scripts.
**KeelOS** ships with **zero** interpreters. It's just the kernel and a few statically linked Go/Rust binaries.
