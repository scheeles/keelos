# Quick Start (QEMU)

The fastest way to try KeelOS is to build and run it locally using QEMU. This guide walks you through building the OS image from source and booting it in a virtual machine.

## Prerequisites

*   **Linux or macOS** host machine.
*   **Docker** (for building the OS image reproducibly).
*   **QEMU** (`qemu-system-x86_64`) for running the VM.

## 1. Build the OS Image

KeelOS uses a containerized build process to ensure that the build is reproducible regardless of your host OS.

Run the builder script:

```bash
./tools/builder/build.sh
```

This will:
1.  Pull the KeelOS build container.
2.  Compile the kernel, `keel-init`, `keel-agent`, and `osctl`.
3.  Assemble the `initramfs` and kernel image.
4.  Output artifacts to the `build/` directory.

## 2. Boot in QEMU

Once the build is complete, you can boot the image directly using the provided test script:

```bash
./tools/testing/run-qemu.sh
```

You should see output similar to:

```text
>>> Booting KeelOS in QEMU...
[    0.000000] Linux version 6.x.x ...
...
[   OK  ] Initialized keel-init (PID 1)
[   OK  ] Mounting API filesystem
[   OK  ] Starting network...
[   OK  ] Starting keel-agent...
[   OK  ] Ready.
```

## 3. Interact with the Node

Since KeelOS does not have a shell, you cannot "log in" to the VM. Instead, you verify connectivity using the `osctl` CLI.

The QEMU script forwards port **50051** (gRPC) inside the VM to port **50052** on your localhost.

Check the health of the node:

```bash
# Point osctl to the forwarded port
./target/debug/osctl health --endpoint http://localhost:50052
```

*(Note: You may need to build `osctl` locally first if it wasn't built by the builder script, via `cargo build -p osctl`)*.

## Troubleshooting

*   **"Connection Refused"**: Ensure the VM is fully booted. The `keel-agent` takes a few seconds to start.
*   **"QEMU not found"**: Install QEMU via your package manager (`brew install qemu` on macOS, `apt install qemu-system-x86` on Debian/Ubuntu).
