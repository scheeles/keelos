# KeelOS Documentation

Welcome to the KeelOS documentation. KeelOS is an immutable, API-driven Linux distribution designed exclusively for hosting Kubernetes workloads.

## Overview

Start here to understand what KeelOS is and why it exists.

| Document | Description |
|----------|-------------|
| [What is KeelOS?](./overview/what-is-keelos.md) | Introduction to KeelOS and how it differs from traditional Linux. |
| [Philosophy](./overview/philosophy.md) | The four pillars: Immutable, API-Oriented, Minimal, Secure. |
| [Security](./overview/security.md) | How KeelOS reduces attack surface and enforces integrity. |
| [KeelOS vs. Traditional Linux](./overview/keelos-vs-linux.md) | Side-by-side comparison with Ubuntu, RHEL, etc. |

## Getting Started

Get KeelOS running on your machine.

| Document | Description |
|----------|-------------|
| [System Requirements](./getting-started/system-requirements.md) | Hardware, network, and boot mode requirements. |
| [Quick Start (QEMU)](./getting-started/quickstart-qemu.md) | Build and boot KeelOS locally in under 10 minutes. |
| [Build from Source](./getting-started.md) | Full walkthrough: clone, build kernel, compile Rust, boot. |
| [First Boot & Bootstrapping](./getting-started/first-boot.md) | What happens at boot and how to join a Kubernetes cluster. |

## Installation

Install KeelOS on different platforms.

| Document | Description |
|----------|-------------|
| [Installation Guide](./installation.md) | Install from pre-built images (ISO, QCOW2, RAW, PXE). |
| [Local (QEMU)](./platform-installation/local-qemu.md) | Run in QEMU for development and testing. |
| [Bare Metal](./platform-installation/bare-metal.md) | Install on physical servers and PXE boot. |
| [Cloud Platforms](./platform-installation/cloud.md) | Deploy on AWS, GCP, Azure, OpenStack, and Hetzner. |

## Using KeelOS

Day-to-day operations and management.

| Document | Description |
|----------|-------------|
| [Using osctl](./using-osctl.md) | Introduction to the `osctl` CLI for node management. |
| [Lifecycle Management](./operational-guides/lifecycle-management.md) | Updates (full & delta), rollbacks, and disaster recovery. |
| [Configuration](./operational-guides/configuration.md) | Health checks, kubelet, and containerd configuration. |
| [Certificate Management](./certificate-management.md) | Bootstrap certs, K8s operational certs, auto-renewal, and monitoring. |
| [Certificate Quick Reference](./cert-quick-reference.md) | One-page cheat sheet for certificate commands and metrics. |
| [Troubleshooting](./troubleshooting.md) | Common issues and their solutions. |

## Guides

Step-by-step guides for specific tasks.

| Document | Description |
|----------|-------------|
| [Kubernetes Bootstrap](./guides/kubernetes-bootstrap.md) | Join a KeelOS node to a Kubernetes cluster (token & kubeconfig methods). |

## Learn More

Deep dives into how KeelOS works under the hood.

| Document | Description |
|----------|-------------|
| [Architecture](./learn-more/architecture.md) | PID 1, partition layout, component interaction, and security. |
| [API-Driven Management](./learn-more/api-management.md) | How the gRPC API replaces SSH and the shell. |
| [Immutability & Updates](./learn-more/immutability.md) | Read-only filesystems, A/B partitions, and persistence. |
| [Networking](./learn-more/networking.md) | Network configuration, CNI, firewalling, and proxy support. |

## Reference

Technical reference for APIs, CLI, and configuration schemas.

| Document | Description |
|----------|-------------|
| [osctl CLI Reference](./reference/osctl.md) | Complete command reference with flags and examples. |
| [gRPC API Reference](./reference/api.md) | `NodeService` RPC methods and message types. |
| [Network API Reference](./reference/network-api.md) | Network management RPC methods and configuration format. |
| [Health & Rollback API](./api/health-and-rollback.md) | Health check and rollback API with examples in Python and Go. |
| [Update Scheduling API](./api/update-scheduling.md) | Schedule, list, and cancel updates via the API. |
| [Configuration Schema](./reference/configuration.md) | Data structures for update schedules, health checks, and rollback events. |
| [Kernel Configuration](./reference/kernel.md) | Kernel version, build config, and command-line arguments. |
| [Architecture Diagram](./architecture.md) | Boot sequence diagram, component table, and partition layout. |

## Release Notes

See [CHANGELOG.md](../CHANGELOG.md) in the project root.

## Contributing Documentation

1.  **Format**: Use Markdown.
2.  **Location**: User-facing docs go in this directory. Internal/developer docs go in component `README.md` files.
3.  **Review**: All documentation changes require PR review.
