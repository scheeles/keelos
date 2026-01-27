# Changelog

All notable changes to KeelOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial `keel-init` implementation (PID 1 with process supervision).
- Initial `keel-agent` gRPC server with status and install endpoints.
- Initial `osctl` CLI client.
- Docker-based build environment (`tools/builder/`).
- Kernel build script with minimal x86_64 configuration.
- Initramfs build script bundling all components.
- QEMU-based testing harness (`tools/testing/`).
- A/B partition update mechanism prototype.

### Changed
- N/A

### Fixed
- N/A

### Security
- N/A

---

## [0.1.0] - YYYY-MM-DD (Planned)

Initial alpha release.
