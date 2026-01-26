# AGENTS.md

> Guidance for AI agents contributing to MaticOS.

## Project Overview

MaticOS is an immutable, API-driven Linux distribution designed exclusively for hosting Kubernetes workloads. It replaces the traditional userspace (Shell, SSH, Systemd) with a single-binary PID 1 (`matic-init`) and a gRPC-based management API (`matic-agent`).

Key characteristics:
- **Immutable**: Read-only SquashFS image with atomic A/B partition updates
- **API-Driven**: No SSH or console access; all management via authenticated gRPC
- **Minimalist**: Under 100MB; only essential components (kernel, init, agent, containerd, kubelet)
- **Secure**: mTLS everywhere, kernel lockdown, no interpreters

## Repository Structure

| Directory | Purpose |
|-----------|---------|
| `/kernel` | Minimalist Linux kernel configuration and patches |
| `/pkg` | Shared Rust/Go libraries for OS components |
| `/cmd` | Binaries: `matic-init`, `matic-agent`, `osctl` |
| `/crates` | Rust workspace crates |
| `/system` | Static manifests and bootstrap configuration |
| `/tools` | Build systems and test harnesses |
| `/docs` | End-user documentation |
| `/.ai-context` | Developer guidelines and style rules |

## Critical Rules

1. **Read the Style Guide First**: Always review [`.ai-context/STYLE_GUIDE.md`](./.ai-context/STYLE_GUIDE.md) before making changes.

2. **No Panics in PID 1**: The `matic-init` binary is PID 1 and **must never panic**. Always use `Result<T, E>` with proper error handling.

3. **Static Linking**: All binaries must be statically linked using `x86_64-unknown-linux-musl`.

4. **Containerized Builds**: Run all builds inside the provided Docker container for reproducibility:
   ```bash
   ./tools/builder/build.sh
   ```

5. **Feature Branches**: Never commit directly to `main`. Create feature branches and submit PRs.

6. **Tests Required**: All new code requires unit tests. System changes require verification scripts in `tools/testing/`.

## Build & Test

```bash
# Build the OS image (runs in container)
./tools/builder/build.sh

# Run in QEMU for testing
./tools/testing/run-qemu.sh

# Run boot tests
./tools/testing/test-boot.sh
```

## Code Guidelines

### Rust
- Formatter: `rustfmt` with defaults
- Lints: Must pass `clippy::pedantic`
- No `unwrap()` or `expect()` in runtime code; use `?` operator
- Async: Use `tokio` for the Agent; keep `matic-init` synchronous

### Commit Messages
Follow [Conventional Commits](https://www.conventionalcommits.org/):
- `feat:` new features
- `fix:` bug fixes
- `docs:` documentation
- `chore:` maintenance

## Key Documentation

- [Style Guide](./.ai-context/STYLE_GUIDE.md) - Coding standards and rules
- [Architecture](./docs/architecture.md) - System design and boot sequence
- [Getting Started](./docs/getting-started.md) - Build from source
- [CHANGELOG](./CHANGELOG.md) - Release notes
