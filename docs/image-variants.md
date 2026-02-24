# OS Image Variants

KeelOS builds specialized OS images for different deployment environments. Each variant tailors the kernel configuration, initramfs contents, and output image formats for its target use case.

## Available Variants

| Variant | Target | Output Formats | Key Features |
|---------|--------|----------------|--------------|
| `cloud` | AWS, GCP, Azure | RAW, QCOW2, VHD, GCP tar.gz | Cloud metadata agent, virtio/Hyper-V drivers |
| `bare-metal` | Physical servers | ISO, PXE, RAW | Hardware drivers, IPMI, NIC support, PXE boot config |
| `edge` | IoT/edge devices | RAW, PXE | Minimal kernel, stripped CNI, no pre-loaded images |
| `dev` | Development/testing | ISO, QCOW2 | Debug tools, kernel debug info, unstripped binaries |

## Building Variants

All variant builds run inside the builder container. Enter the build environment first:

```bash
./tools/builder/build.sh
```

Then build a variant:

```bash
./tools/builder/build-variant.sh <variant> [version]
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SKIP_KERNEL` | `0` | Set to `1` to skip kernel build (reuse existing) |
| `SKIP_CARGO` | `0` | Set to `1` to skip cargo build (reuse existing) |
| `OUTPUT_DIR` | `build/variants/<name>` | Override output directory |

### Examples

```bash
# Build all from scratch
./tools/builder/build-variant.sh cloud v1.2.0

# Reuse kernel from a previous build
SKIP_KERNEL=1 ./tools/builder/build-variant.sh edge v1.2.0

# Custom output directory
OUTPUT_DIR=/tmp/images ./tools/builder/build-variant.sh bare-metal v1.2.0
```

## Variant Details

### Cloud (`cloud`)

Optimized for virtual machine deployment on major cloud providers.

**Kernel additions:**
- Virtio drivers (block, network, console, balloon, SCSI)
- Xen frontend drivers (for AWS)
- Hyper-V drivers (for Azure)
- EFI stub support

**Initramfs additions:**
- `keel-cloud-init` metadata agent that queries cloud IMDS endpoints at boot
- Automatic hostname configuration from instance metadata

**Output formats:**
- `raw.gz` — Compressed raw disk, suitable for AWS AMI import
- `qcow2` — For OpenStack or direct QEMU/KVM use
- `vhd` — Azure-compatible fixed VHD (1MB-aligned)
- `gcp.tar.gz` — GCP-compatible image (contains `disk.raw`)

**Cloud provider deployment:**

```bash
# AWS: Import raw image as AMI
aws ec2 import-image --disk-containers \
  "Format=raw,UserBucket={S3Bucket=my-bucket,S3Key=keelos-1.0.0-cloud.raw.gz}"

# GCP: Create image from tar.gz
gcloud compute images create keelos-1-0-0 \
  --source-uri=gs://my-bucket/keelos-1.0.0-cloud-gcp.tar.gz

# Azure: Upload VHD to managed disk
az disk create --name keelos-disk \
  --source https://mystorage.blob.core.windows.net/vhds/keelos-1.0.0-cloud.vhd
```

### Bare Metal (`bare-metal`)

Optimized for physical server deployment with PXE network booting.

**Kernel additions:**
- Storage drivers (AHCI, NVMe, MegaRAID, Fusion SAS)
- IPMI management interface
- Hardware monitoring (coretemp, EDAC)
- Network drivers (Intel e1000e/igb/ixgbe/i40e, Mellanox, Broadcom)
- PXE boot support (IP auto-config via DHCP, NFS root)

**Output formats:**
- `iso` — Bootable ISO for initial installation
- `pxe.tar.gz` — PXE bundle with vmlinuz, initramfs, and pxelinux config
- `raw.gz` — Compressed raw disk for imaging

**PXE boot setup:**

```bash
# Extract PXE bundle to TFTP server root
tar -xzf keelos-1.0.0-bare-metal-pxe.tar.gz -C /srv/tftp/

# Configure DHCP to point to TFTP server
# (see pxelinux.cfg in the bundle for boot parameters)
```

### Edge (`edge`)

Minimal image for resource-constrained environments (IoT gateways, edge nodes).

**Kernel changes:**
- Size-optimized (`CC_OPTIMIZE_FOR_SIZE`, XZ compression)
- Disabled: sound, DRM, USB, wireless, media, debug/tracing subsystems

**Initramfs changes:**
- Stripped CNI plugins (keeps only bridge, host-local, loopback, portmap)
- No pre-loaded container images (pulls on demand to save space)

**Output formats:**
- `raw.gz` — Compressed raw disk
- `pxe.tar.gz` — Minimal PXE bundle

### Development (`dev`)

**Not for production use.** Includes debug tools and symbols for development and testing.

**Kernel additions:**
- Full debug info (DWARF5)
- Hung task detection, lockdep, ftrace, kprobes
- Magic SysRq support
- Printk timestamps

**Initramfs additions:**
- Extended busybox symlinks (ls, cat, ps, top, dmesg, free, df, netstat, vi, ping, grep, etc.)
- Unstripped KeelOS binaries with debug symbols
- Verbose boot logging (`loglevel=7`)

**Output formats:**
- `iso` — For easy QEMU testing
- `qcow2` — For libvirt/virt-manager

**Testing with QEMU:**

```bash
# Boot dev variant in QEMU
qemu-system-x86_64 \
    -cdrom build/variants/dev/keelos-dev-dev.iso \
    -m 4G -smp 2 -nographic
```

## Custom Variants

Create a custom variant by adding a new `.conf` file in `tools/builder/variants/`:

```bash
cp tools/builder/variants/base.conf tools/builder/variants/my-variant.conf
# Edit my-variant.conf to customize
./tools/builder/build-variant.sh my-variant v1.0.0
```

Configuration options are documented in `tools/builder/variants/base.conf`.

## CI/CD Integration

The release pipeline (`release.yml`) builds all four variants in parallel using a GitHub Actions matrix strategy. Each variant produces its own set of artifacts, which are included in the GitHub Release alongside the base images.

Variant artifacts follow the naming convention:

```
keelos-<version>-<variant>.<format>
```

For example: `keelos-1.0.0-cloud.qcow2`, `keelos-1.0.0-bare-metal-pxe.tar.gz`.
