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

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

*   `feat: add network interface configuration`
*   `fix(init): correctly reap zombie processes`
*   `docs: update architecture spec`
*   `chore: bump kernel version`
