# Changelog

All notable changes to this project will be documented in this file.

## [v0.1.0] - 2026-01-27

### Added
- **Immutable OS Architecture**: Read-only SquashFS root filesystem.
- **gRPC Management API**: `keel-agent` for secure, API-driven management.
- **Update System**:
    - Atomic A/B partition updates.
    - **Delta Updates**: Binary diff updates using `bsdiff` for bandwidth efficiency.
    - Automatic rollback on boot failure.
- **CLI**: `osctl` command-line tool for managing the OS.
- **Documentation**: Comprehensive guides for architecture, lifecycle management, and installation.
- **License**: Apache License 2.0.

### Changed
- Renamed project from `maticos` to `keelos`.
