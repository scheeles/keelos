# First Boot & Bootstrapping

Understanding how KeelOS boots is helpful for troubleshooting and understanding its difference from traditional Linux.

## The Boot Sequence

### 1. Bootloader
The BIOS/UEFI loads the bootloader (Limine or GRUB), which loads the Linux Kernel (`bzImage`) and the Initial RAM Filesystem (`initramfs`) into memory.

### 2. Kernel Initialization
The Linux kernel initializes hardware, mounts the `initramfs`, and executes the init process. In typical Linux, this is `/sbin/init` (symlinked to Systemd). In KeelOS, this is `/init`, which acts as our custom PID 1.

### 3. keel-init (PID 1)
`keel-init` is the heart of the boot process. It is a statically linked Rust binary that performs the following steps sequentially:

1.  **Mount API Filesystems**: Mounts `/proc`, `/sys`, `/dev`, and `/run`.
2.  **Load Kernel Modules**: Loads necessary drivers for networking and storage.
3.  **Network Setup**: Brings up the loopback interface (`lo`) and attempts DHCP on the primary interface (`eth0`).
4.  **Partition Discovery**: Looks for the persistent data partition on the attached disk.
    *   *If found*: Mounts it to `/var/lib/keel`.
    *   *If not found*: Formats the disk and creates the partition structure (First Boot).
5.  **Service Startup**:
    *   Starts `containerd` to manage container lifecycles.
    *   Starts `keel-agent` to listen for API commands.
    *   Starts `kubelet` to join the Kubernetes cluster.

## Node Identity

On first boot, the node generates a unique Machine ID and a self-signed certificate (unless bootstrapped with a specific identity).

### Registration
In a managed cluster, the `keel-agent` will reach out to the control plane to register itself and download the cluster Certificate Authority (CA). This process is known as **bootstrapping**.

Once bootstrapped, the node will only accept API commands signed by the Cluster CA, ensuring that unauthorized users cannot take control of the machine.
