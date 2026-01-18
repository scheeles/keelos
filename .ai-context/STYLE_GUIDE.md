# MaticOS Coding Standards

This document defines the rules for contributing to MaticOS.

## General Principles

1.  **Memory Safety**: All system components (PID 1, Agent) are written in **Rust**. Tooling may use Go.
2.  **No Panic**: PID 1 must never panic. Use `Result<T, E>` and handle all errors gracefully.
3.  **Minimal Dependencies**: Audit every crate. Prefer `std` where possible.
4.  **Static Linking**: All binaries must be statically linked (target `x86_64-unknown-linux-musl`).

## Rust Guidelines

*   **Formatter**: Use `rustfmt` with default settings.
*   **Lints**: Code must pass `clippy::pedantic`.
*   **Error Handling**:
    *   ❌ `unwrap()` or `expect()` in runtime code.
    *   ✅ `?` operator or explicit matching.
*   **Async**: Use `tokio` for the Agent. `matic-init` should remain synchronous where possible for simplicity, or use a minimal executor if needed.

## Testing Guidelines

1.  **Unit Tests**: Mandatory for all new logic. In Rust, use `#[cfg(test)]` modules within the same file for unit tests.
2.  **Verification Scripts**: New features affecting system boot or artifact creation must be covered by scripts in `tools/testing/`.
3.  **E2E Tests**: Significant system-wide changes (e.g., networking, container lifecycle) require end-to-end tests using the project's QEMU-based testing framework.
4.  **No Regressions**: All existing tests and verification scripts must pass before merging.

## Commit Strategy

1.  **Atomic Changes**: Each commit must represent a single, logical unit of work.
2.  **Linear History**: Maintain a linear git history. Rebase your changes on top of `main`.
3.  **PR Squashing**: Feature branches should be squashed into descriptive, single commits upon merging to `main`.

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

*   `feat: add network interface configuration`
*   `fix(init): correctly reap zombie processes`
*   `docs: update architecture spec`
*   `chore: bump kernel version`
