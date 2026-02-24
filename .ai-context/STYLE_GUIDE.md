# KeelOS Coding Standards

This document defines the rules for contributing to KeelOS.

## General Principles

1.  **Memory Safety**: All system components (PID 1, Agent) are written in **Rust**. Tooling may use Go.
2.  **No Panic**: PID 1 must never panic. Use `Result<T, E>` and handle all errors gracefully.
3.  **Minimal Dependencies**: Audit every crate. Prefer `std` where possible.
4.  **Static Linking**: All binaries must be statically linked (target `x86_64-unknown-linux-musl`).

## Build Environment

1.  **Containerized Builds**: All builds must run inside a container to ensure reproducibility and a consistent build environment across different machines.

## Rust Guidelines

*   **Formatter**: Use `rustfmt` with default settings.
*   **Lints**: The project uses comprehensive linting configured in:
    *   `clippy.toml` - Project-specific clippy configuration
    *   `Cargo.toml` - Workspace-level lint rules (`[workspace.lints]`)
    *   Enabled lint groups:
        *   `clippy::all` (deny) - All clippy lints
        *   `clippy::pedantic` (warn) - Opinionated but helpful lints
        *   `clippy::nursery` (warn) - Experimental but useful checks
        *   `clippy::cargo` (warn) - Cargo manifest lints
    *   Critical denials for PID 1 safety:
        *   `unwrap_used` - Prevents `.unwrap()` calls
        *   `expect_used` - Prevents `.expect()` calls
        *   `panic` - Prevents explicit `panic!()` macros
        *   These are allowed in test code only
*   **Error Handling**:
    *   ❌ `unwrap()` or `expect()` in runtime code (enforced by lints).
    *   ✅ `?` operator or explicit matching.
    *   ✅ Proper error documentation with `/// # Errors`
*   **Running Lints Locally**:
    ```bash
    # Check all lints (same as CI)
    cargo clippy --workspace -- -D warnings
    
    # Auto-fix some issues
    cargo clippy --workspace --fix
    
    # Check formatting
    cargo fmt --all -- --check
    ```
*   **Async**: Use `tokio` for the Agent. `keel-init` should remain synchronous where possible for simplicity, or use a minimal executor if needed.

## Testing Guidelines

1.  **Unit Tests**: Mandatory for all new logic. In Rust, use `#[cfg(test)]` modules within the same file for unit tests.
2.  **Verification Scripts**: New features affecting system boot or artifact creation must be covered by scripts in `tools/testing/`.
3.  **E2E Tests**: Significant system-wide changes (e.g., networking, container lifecycle) require end-to-end tests using the project's QEMU-based testing framework.
4.  **No Regressions**: All existing tests and verification scripts must pass before merging.

## GitHub Interactions

- **Always use the GitHub CLI (`gh`) for all GitHub operations**
- Use `gh pr`, `gh run`, `gh api` instead of browser interactions
- Examples:
  - View PR checks: `gh pr checks <pr-number>`
  - View run logs: `gh run view <run-id> --log`
  - Check run status: `gh run list --branch <branch>`
- Only use the browser for creating PRs when the CLI prompt is insufficient

## Commit Strategy

1.  **Feature Branches**: All work must be performed in feature branches. Never commit directly to `main`.
2.  **Atomic Changes**: Each commit must represent a single, logical unit of work.
3.  **Linear History**: Maintain a linear git history. Rebase your changes on top of `main`.
4.  **PR Squashing**: Feature branches should be squashed into descriptive, single commits before merging

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

*   `feat: add network interface configuration`
*   `fix(init): correctly reap zombie processes`
*   `docs: update architecture spec`
*   `chore: bump kernel version`

## Documentation Strategy

1.  **Code-Level Documentation**:
    *   **Rust**: Public items (`pub`) generally require `///` doc comments. Complex internal logic needs `//` comments explaining *why*, not *what*.
    *   **Shell**: Functions require a header comment block explaining usage and arguments.
2.  **Component Documentation**:
    *   Every crate in `crates/` and significant tool in `tools/` must have a `README.md`.
    *   Include: **Overview**, **Build Instructions**, and **Usage Examples**.
3.  **CLI User Experience**:
    *   All CLI tools must implement `--help`.
    *   Use distinct 'long' vs 'short' help where applicable.
4.  **Architecture & Design**:
    *   Significant architectural decisions should be recorded in `docs/` (or `.ai-context` for meta-rules).
5.  **End-User Documentation**:
    *   **Guides & Tutorials**: Focus on "How-To" articles for common tasks.
    *   **Installation**: Clear, step-by-step getting started guides.
    *   **Release Notes**: Maintain a `CHANGELOG.md` or release notes for every version.
