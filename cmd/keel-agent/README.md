# keel-agent

The API endpoint for KeelOS node management.

**Status**: Alpha
**Language**: Rust
**Communication**: gRPC (mTLS)

## Overview

`keel-agent` runs as a daemon (supervised by `keel-init`) and exposes a gRPC interface. Since KeelOS has no SSH or shell, this agent is the *only* way to mutate the system state or retrieve debug information remotely.

## Features

*   **Install/Update**: Accepting new OS images and writing them to the passive partition.
*   **Reboot**: Safe reboot handling.
*   **Network Configuration**: Applying IP addresses, routes, and DNS settings.
*   **Observability**: Streaming logs from system components (`containerd`, `kubelet`) and providing kernel metrics.

## API Specs

The Protocol Buffer definitions are located in `crates/matic-proto`.

## Usage

Started automatically by `keel-init`.
Listens on `0.0.0.0:50051`.

Debug locally (if built natively):
```bash
cargo run --package keel-agent
```
