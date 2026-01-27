# Cloud Platforms

KeelOS creates a unified operating model across bare metal and cloud environments. While official machine images (AMIs) are coming soon, you can run KeelOS on cloud providers today using custom image imports.

## Status

| Provider | Status | Notes |
| :--- | :--- | :--- |
| **AWS** | 🚧 Planned | Native AMI generation pipeline in progress. |
| **GCP** | 🚧 Planned | |
| **Azure** | 🚧 Planned | |
| **OpenStack** | ✅ Supported | Via QCOW2 image upload. |
| **Hetzner** | ✅ Supported | Via ISO mount or Rescue System install. |

## Feature Roadmap

### Cloud-Init Support
We are implementing a lightweight equivalent to `cloud-init` to handle:
*   **Metadata Service**: Fetching keys and configuration from the cloud provider's metadata service (IMDS).
*   **UserData**: Injecting initial configuration (e.g., joining a cluster token) via UserData.

### Pre-Built AMIs
We intend to publish public AMIs for every release in all major AWS regions.

## Generic Cloud Install

For providers allowing custom ISO uploads (like Vultr, DigitalOcean, or Hetzner Cloud):

1.  **Upload ISO**: Upload `keelos-amd64.iso` to your provider.
2.  **Create Instance**: Boot a new instance from the ISO.
3.  **Install**: Access the VNC console and run `osctl install-local` to write to the virtual disk.
4.  **Detach ISO**: Detach the ISO and reboot.

This creates a persistent KeelOS VM managed exactly like a bare-metal node.
