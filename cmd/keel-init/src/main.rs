//! KeelOS Init Process (PID 1)
//!
//! This is the first process started by the kernel. As PID 1, it has special
//! responsibilities:
//! - It must NEVER panic or exit unexpectedly
//! - It must reap zombie processes
//! - It must supervise critical system services
//! - It tracks boot phase metrics for observability
//!
//! All errors are handled gracefully - the system will continue running
//! in a degraded/maintenance mode rather than crashing.

use nix::mount::{mount, MsFlags};
use nix::sys::stat::{umask, Mode};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::fs;
use std::process::{Child, Command, Stdio};
use std::{thread, time};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

mod telemetry;

/// Entry point - wraps run() to ensure PID 1 never exits unexpectedly
fn main() {
    // Initialize tracing subscriber for structured logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_ansi(false) // No ANSI colors for serial console
        .compact()
        .finish();

    // Ignore errors if subscriber is already set (shouldn't happen for PID 1)
    let _ = tracing::subscriber::set_global_default(subscriber);

    info!("Welcome to KeelOS v0.1");
    info!("Init process started (PID 1)");

    if let Err(e) = run() {
        error!(error = %e, "Init encountered a fatal error");
        error!("System entering maintenance mode");
    }

    // PID 1 must never exit - enter infinite maintenance loop
    maintenance_loop();
}

/// Main init logic - all errors are propagated but never cause a panic
fn run() -> Result<(), InitError> {
    // Set safe umask
    umask(Mode::from_bits(0o077).unwrap());

    // Set PATH - as PID 1, we have no inherited PATH from a parent process.
    // Child processes (kubelet, containerd, etc.) need this to find binaries like mount.
    std::env::set_var(
        "PATH",
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    );

    // Initialize boot phase tracker
    let mut boot_tracker = telemetry::BootPhaseTracker::new();

    // Mount essential filesystems
    boot_tracker.start_phase("filesystem");
    setup_filesystems()?;

    // Mount persistent storage for container data
    boot_tracker.start_phase("storage");
    setup_persistent_storage();

    // Set up cgroups
    boot_tracker.start_phase("cgroups");
    setup_cgroups();

    // Set hostname
    setup_hostname();

    // Configure networking
    boot_tracker.start_phase("network");
    setup_networking();

    // Check for test mode
    check_test_mode();

    // Supervise core services
    boot_tracker.start_phase("services");
    supervise_services()?;

    // Export boot metrics
    boot_tracker.end_current_phase();
    if let Err(e) = boot_tracker.export_to_file("/run/keel/boot-metrics.json") {
        warn!(error = %e, "Failed to export boot metrics");
    }

    Ok(())
}

/// Custom error type for init operations
#[derive(Debug)]
#[allow(dead_code)] // Variants reserved for future structured error handling
enum InitError {
    Mount(String),
    Spawn(String),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::Mount(msg) => write!(f, "Mount error: {}", msg),
            InitError::Spawn(msg) => write!(f, "Process spawn error: {}", msg),
        }
    }
}

impl std::error::Error for InitError {}

/// Mount essential API filesystems (/proc, /sys, /dev, /tmp)
fn setup_filesystems() -> Result<(), InitError> {
    info!("Mounting API filesystems");

    // Ensure directories exist (ignore errors - they may already exist)
    let _ = fs::create_dir_all("/proc");
    let _ = fs::create_dir_all("/sys");
    let _ = fs::create_dir_all("/dev");
    let _ = fs::create_dir_all("/tmp");

    // Mount the root filesystem as shared/remount to support pivot_root (required by runc)
    // When running from initramfs, root is not a mount point, which breaks pivot_root
    if let Err(e) = mount::<str, str, str, str>(
        Some("/"),
        "/",
        None,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None,
    ) {
        warn!(error = %e, "Failed to bind mount / to /");
    } else {
        debug!("Bind mounted / to /");
    }

    // Make the mount private to avoid propagation issues
    if let Err(e) =
        mount::<str, str, str, str>(None, "/", None, MsFlags::MS_PRIVATE | MsFlags::MS_REC, None)
    {
        warn!(error = %e, "Failed to make / private");
    } else {
        debug!("Made / mount private");
    }

    // Mount proc - critical for process management
    if let Err(e) =
        mount::<str, str, str, str>(Some("none"), "/proc", Some("proc"), MsFlags::empty(), None)
    {
        warn!(error = %e, "Failed to mount /proc");
    } else {
        debug!("Mounted /proc");
    }

    // Mount sysfs
    if let Err(e) =
        mount::<str, str, str, str>(Some("none"), "/sys", Some("sysfs"), MsFlags::empty(), None)
    {
        warn!(error = %e, "Failed to mount /sys");
    } else {
        debug!("Mounted /sys");
    }

    // Mount devtmpfs - critical for device access
    if let Err(e) = mount::<str, str, str, str>(
        Some("none"),
        "/dev",
        Some("devtmpfs"),
        MsFlags::empty(),
        None,
    ) {
        warn!(error = %e, "Failed to mount /dev");
    } else {
        debug!("Mounted /dev");
    }

    // Mount devpts - needed by runc/containerd for container PTY allocation
    let _ = fs::create_dir_all("/dev/pts");
    if let Err(e) = mount::<str, str, str, str>(
        Some("devpts"),
        "/dev/pts",
        Some("devpts"),
        MsFlags::empty(),
        Some("newinstance,ptmxmode=0666,mode=0620"),
    ) {
        warn!(error = %e, "Failed to mount /dev/pts");
    } else {
        debug!("Mounted /dev/pts");
    }

    // Mount /dev/shm - needed for POSIX shared memory in containers
    let _ = fs::create_dir_all("/dev/shm");
    if let Err(e) = mount::<str, str, str, str>(
        Some("tmpfs"),
        "/dev/shm",
        Some("tmpfs"),
        MsFlags::empty(),
        Some("size=64m"),
    ) {
        warn!(error = %e, "Failed to mount /dev/shm");
    } else {
        debug!("Mounted /dev/shm");
    }

    // Mount /dev/mqueue - needed for POSIX message queues
    let _ = fs::create_dir_all("/dev/mqueue");
    if let Err(e) = mount::<str, str, str, str>(
        Some("mqueue"),
        "/dev/mqueue",
        Some("mqueue"),
        MsFlags::empty(),
        None,
    ) {
        warn!(error = %e, "Failed to mount /dev/mqueue");
    } else {
        debug!("Mounted /dev/mqueue");
    }

    // Mount tmpfs
    if let Err(e) =
        mount::<str, str, str, str>(Some("none"), "/tmp", Some("tmpfs"), MsFlags::empty(), None)
    {
        warn!(error = %e, "Failed to mount /tmp");
    } else {
        debug!("Mounted /tmp");
    }

    info!("API filesystems mounted");
    Ok(())
}

/// Mount persistent storage disk for container and kubelet data.
/// Without this, all container images and state live on the rootfs (RAM),
/// which quickly fills up and causes DiskPressure.
fn setup_persistent_storage() {
    use std::process::Command;

    let data_mount = "/data";

    // Create mount point
    let _ = std::fs::create_dir_all(data_mount);

    // Try candidate devices in order of preference
    let candidates = ["/dev/sda4", "/dev/sda"];
    let mut mounted = false;

    for dev in &candidates {
        if !std::path::Path::new(dev).exists() {
            debug!(device = dev, "Device not found, skipping");
            continue;
        }

        info!(device = dev, "Trying to mount persistent storage");

        // First, try mounting directly (already formatted)
        match mount::<str, str, str, str>(
            Some(dev),
            data_mount,
            Some("ext4"),
            MsFlags::empty(),
            None,
        ) {
            Ok(()) => {
                info!(
                    device = dev,
                    mount = data_mount,
                    "Mounted persistent storage"
                );
                mounted = true;
                break;
            }
            Err(_) => {
                // Mount failed, try formatting first
                info!(device = dev, "Formatting device with ext4");
                let format_result = Command::new("/sbin/mkfs.ext4").args(["-F", dev]).output();

                match format_result {
                    Ok(output) if output.status.success() => {
                        info!(device = dev, "Formatted successfully");
                        // Try mount again after formatting
                        match mount::<str, str, str, str>(
                            Some(dev),
                            data_mount,
                            Some("ext4"),
                            MsFlags::empty(),
                            None,
                        ) {
                            Ok(()) => {
                                info!(
                                    device = dev,
                                    mount = data_mount,
                                    "Mounted persistent storage after formatting"
                                );
                                mounted = true;
                                break;
                            }
                            Err(e) => {
                                warn!(device = dev, error = %e, "Failed to mount after formatting")
                            }
                        }
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        warn!(device = dev, stderr = %stderr, "mkfs.ext4 failed");
                    }
                    Err(e) => warn!(device = dev, error = %e, "Failed to run mkfs.ext4"),
                }
            }
        }
    }

    if !mounted {
        warn!("No persistent storage available. Container data will use rootfs (RAM).");
        return;
    }

    // Bind-mount key directories to persistent storage
    let bind_dirs = [
        ("containerd", "/var/lib/containerd"),
        ("kubelet", "/var/lib/kubelet"),
        ("keel", "/var/lib/keel"),
    ];

    for (subdir, target) in &bind_dirs {
        let source = format!("{}/{}", data_mount, subdir);
        let _ = std::fs::create_dir_all(&source);
        let _ = std::fs::create_dir_all(target);

        match mount::<str, str, str, str>(
            Some(source.as_str()),
            *target,
            None,
            MsFlags::MS_BIND,
            None,
        ) {
            Ok(()) => info!(source = %source, target = target, "Bind-mounted persistent storage"),
            Err(e) => warn!(source = %source, target = target, error = %e, "Failed to bind-mount"),
        }
    }

    info!("Persistent storage setup complete");
}

/// Configure networking based on saved configuration or DHCP fallback
fn setup_networking() {
    info!("Initializing networking");

    // Configure loopback first (always needed)
    configure_loopback();

    // Try to load network configuration
    match keel_config::network::NetworkConfig::load() {
        Ok(config) => {
            info!("Loading network configuration from file");
            apply_network_config(&config);
        }
        Err(e) => {
            debug!(error = %e, "No network configuration found, using DHCP fallback");
            // Fallback to DHCP on eth0 (QEMU default primary interface)
            configure_dhcp_fallback();
        }
    }

    // Ensure /etc/resolv.conf exists - kubelet and other services need it for DNS.
    // If configure_dns or configure_dhcp_fallback already created it, this is a no-op.
    if !std::path::Path::new("/etc/resolv.conf").exists() {
        info!("No /etc/resolv.conf found, creating default");
        // Use common public DNS resolvers as a safe default
        let default_resolv =
            "# Generated by keel-init (default fallback)\nnameserver 8.8.8.8\nnameserver 1.1.1.1\n";
        if let Err(e) = fs::write("/etc/resolv.conf", default_resolv) {
            warn!(error = %e, "Failed to create /etc/resolv.conf");
        }
    }

    // Ensure /etc/hosts exists - containerd needs it to generate pod sandbox hosts files
    if !std::path::Path::new("/etc/hosts").exists() {
        let hostname = nix::unistd::gethostname()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "keelos".to_string());
        let hosts_content = format!(
            "# Generated by keel-init\n127.0.0.1\tlocalhost\n::1\t\tlocalhost\n127.0.1.1\t{}\n",
            hostname
        );
        if let Err(e) = fs::write("/etc/hosts", hosts_content) {
            warn!(error = %e, "Failed to create /etc/hosts");
        }
    }

    info!("Networking initialized");
}

/// Configure loopback interface
fn configure_loopback() {
    // Using ip command instead of busybox ifconfig for modern networking
    match Command::new("/sbin/ip")
        .args(["link", "set", "lo", "up"])
        .status()
    {
        Ok(status) if status.success() => {
            match Command::new("/sbin/ip")
                .args(["addr", "add", "127.0.0.1/8", "dev", "lo"])
                .status()
            {
                Ok(status) if status.success() => debug!("Configured loopback interface"),
                Ok(status) => warn!(exit_code = ?status.code(), "Failed to set loopback address"),
                Err(e) => warn!(error = %e, "Failed to configure loopback address"),
            }
        }
        Ok(status) => warn!(exit_code = ?status.code(), "Failed to bring up loopback"),
        Err(e) => warn!(error = %e, "Failed to bring up loopback"),
    }
}

/// Apply network configuration from config file
fn apply_network_config(config: &keel_config::network::NetworkConfig) {
    // Configure each interface
    for iface in &config.interfaces {
        configure_interface(iface);
    }

    // Configure DNS if present
    if let Some(ref dns) = config.dns {
        configure_dns(dns);
    }

    // Configure custom routes
    for route in &config.routes {
        configure_route(route);
    }
}

/// Configure a single network interface
fn configure_interface(iface: &keel_config::network::InterfaceConfig) {
    use keel_config::network::InterfaceType;

    info!(interface = %iface.name, "Configuring network interface");

    // Bring interface up
    match Command::new("/sbin/ip")
        .args(["link", "set", &iface.name, "up"])
        .status()
    {
        Ok(status) if status.success() => debug!(interface = %iface.name, "Interface up"),
        Ok(status) => {
            warn!(interface = %iface.name, exit_code = ?status.code(), "Failed to bring up interface");
            return;
        }
        Err(e) => {
            warn!(interface = %iface.name, error = %e, "Failed to bring up interface");
            return;
        }
    }

    // Configure based on interface type
    match &iface.config {
        InterfaceType::Dhcp => {
            // For DHCP, we just need to bring the interface up
            // In a real system, you'd start a DHCP client here
            debug!(interface = %iface.name, "DHCP configuration (client not implemented)");
        }
        InterfaceType::Static(cfg) => {
            // Add IPv4 address if present
            if !cfg.ipv4_address.is_empty() {
                match Command::new("/sbin/ip")
                    .args(["addr", "add", &cfg.ipv4_address, "dev", &iface.name])
                    .status()
                {
                    Ok(status) if status.success() => {
                        info!(interface = %iface.name, ip = %cfg.ipv4_address, "Static IPv4 configured");
                    }
                    Ok(status) => {
                        warn!(interface = %iface.name, exit_code = ?status.code(), "Failed to set IPv4 address");
                    }
                    Err(e) => {
                        warn!(interface = %iface.name, error = %e, "Failed to set IPv4 address");
                    }
                }

                // Set IPv4 gateway if present
                if let Some(ref gateway) = cfg.gateway {
                    match Command::new("/sbin/ip")
                        .args([
                            "route",
                            "add",
                            "default",
                            "via",
                            gateway,
                            "dev",
                            &iface.name,
                        ])
                        .status()
                    {
                        Ok(status) if status.success() => {
                            debug!(interface = %iface.name, gateway = %gateway, "IPv4 default route configured");
                        }
                        Ok(status) => {
                            warn!(exit_code = ?status.code(), "Failed to set IPv4 default route");
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to set IPv4 default route");
                        }
                    }
                }
            }

            // Add IPv6 addresses
            for ipv6_addr in &cfg.ipv6_addresses {
                match Command::new("/sbin/ip")
                    .args(["-6", "addr", "add", ipv6_addr, "dev", &iface.name])
                    .status()
                {
                    Ok(status) if status.success() => {
                        info!(interface = %iface.name, ip = %ipv6_addr, "Static IPv6 configured");
                    }
                    Ok(status) => {
                        warn!(interface = %iface.name, exit_code = ?status.code(), "Failed to set IPv6 address");
                    }
                    Err(e) => {
                        warn!(interface = %iface.name, error = %e, "Failed to set IPv6 address");
                    }
                }
            }

            // Set IPv6 gateway if present
            if let Some(ref gateway6) = cfg.ipv6_gateway {
                match Command::new("/sbin/ip")
                    .args([
                        "-6",
                        "route",
                        "add",
                        "default",
                        "via",
                        gateway6,
                        "dev",
                        &iface.name,
                    ])
                    .status()
                {
                    Ok(status) if status.success() => {
                        debug!(interface = %iface.name, gateway = %gateway6, "IPv6 default route configured");
                    }
                    Ok(status) => {
                        warn!(exit_code = ?status.code(), "Failed to set IPv6 default route");
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to set IPv6 default route");
                    }
                }
            }

            // Set MTU if non-default
            if cfg.mtu != 1500 {
                match Command::new("/sbin/ip")
                    .args(["link", "set", &iface.name, "mtu", &cfg.mtu.to_string()])
                    .status()
                {
                    Ok(status) if status.success() => {
                        debug!(interface = %iface.name, mtu = cfg.mtu, "MTU configured");
                    }
                    Ok(status) => {
                        warn!(exit_code = ?status.code(), "Failed to set MTU");
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to set MTU");
                    }
                }
            }
        }
        InterfaceType::Vlan(_) => {
            warn!(interface = %iface.name, "VLAN configuration not yet implemented");
        }
        InterfaceType::Bond(_) => {
            warn!(interface = %iface.name, "Bond configuration not yet implemented");
        }
    }
}

/// Configure DNS resolvers
fn configure_dns(dns: &keel_config::network::DnsConfig) {
    let mut resolv_conf = String::from("# Generated by keel-init\n");

    for ns in &dns.nameservers {
        resolv_conf.push_str(&format!("nameserver {}\n", ns));
    }

    if !dns.search_domains.is_empty() {
        resolv_conf.push_str(&format!("search {}\n", dns.search_domains.join(" ")));
    }

    match fs::write("/etc/resolv.conf", resolv_conf) {
        Ok(_) => info!("DNS configuration written to /etc/resolv.conf"),
        Err(e) => warn!(error = %e, "Failed to write /etc/resolv.conf"),
    }
}

/// Configure a custom route
fn configure_route(route: &keel_config::network::RouteConfig) {
    let mut args = vec!["route", "add", &route.destination, "via", &route.gateway];

    let metric_str;
    if let Some(metric) = route.metric {
        metric_str = metric.to_string();
        args.extend_from_slice(&["metric", &metric_str]);
    }

    match Command::new("/sbin/ip").args(&args).status() {
        Ok(status) if status.success() => {
            info!(destination = %route.destination, gateway = %route.gateway, "Route configured");
        }
        Ok(status) => {
            warn!(exit_code = ?status.code(), "Failed to configure route");
        }
        Err(e) => {
            warn!(error = %e, "Failed to configure route");
        }
    }
}

/// Fallback DHCP configuration for QEMU testing
fn configure_dhcp_fallback() {
    info!("Using DHCP fallback for eth0");

    // For QEMU testing, use static IP that matches QEMU's default network
    // In production, this would start a proper DHCP client
    match Command::new("/sbin/ip")
        .args(["link", "set", "eth0", "up"])
        .status()
    {
        Ok(status) if status.success() => {
            // Use QEMU's default network: 10.0.2.0/24
            match Command::new("/sbin/ip")
                .args(["addr", "add", "10.0.2.15/24", "dev", "eth0"])
                .status()
            {
                Ok(status) if status.success() => {
                    debug!("Set eth0 IP to 10.0.2.15/24");
                    // Add default route
                    let _ = Command::new("/sbin/ip")
                        .args(["route", "add", "default", "via", "10.0.2.2", "dev", "eth0"])
                        .status();
                }
                Ok(status) => warn!(exit_code = ?status.code(), "Failed to set eth0 address"),
                Err(e) => warn!(error = %e, "Failed to set eth0 address"),
            }
        }
        Ok(status) => warn!(exit_code = ?status.code(), "Failed to bring up eth0"),
        Err(e) => warn!(error = %e, "Failed to bring up eth0"),
    }
}

/// Set up the system hostname
fn setup_hostname() {
    // Try /etc/hostname first
    if let Ok(hostname) = fs::read_to_string("/etc/hostname") {
        let hostname = hostname.trim().to_string();
        if !hostname.is_empty() {
            if let Err(e) = nix::unistd::sethostname(&hostname) {
                warn!(error = %e, hostname = %hostname, "Failed to set hostname");
            } else {
                info!(hostname = %hostname, "Hostname set from /etc/hostname");
                return;
            }
        }
    }

    // Try kernel command line (hostname=xxx)
    if let Ok(cmdline) = fs::read_to_string("/proc/cmdline") {
        for param in cmdline.split_whitespace() {
            if let Some(hostname) = param.strip_prefix("hostname=") {
                let hostname = hostname.trim().to_string();
                if !hostname.is_empty() {
                    if let Err(e) = nix::unistd::sethostname(&hostname) {
                        warn!(error = %e, hostname = %hostname, "Failed to set hostname from cmdline");
                    } else {
                        info!(hostname = %hostname, "Hostname set from kernel cmdline");
                        return;
                    }
                }
            }
        }
    }

    // Generate a hostname from machine-id or random
    let hostname = format!("keelos-{}", &uuid_like_id()[..8]);
    if let Err(e) = nix::unistd::sethostname(&hostname) {
        warn!(error = %e, hostname = %hostname, "Failed to set generated hostname");
    } else {
        info!(hostname = %hostname, "Hostname set (generated)");
    }
}

/// Generate a simple unique ID string from machine-id or /dev/urandom
fn uuid_like_id() -> String {
    // Try machine-id first
    if let Ok(id) = fs::read_to_string("/etc/machine-id") {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }

    // Fall back to reading a small number of random bytes
    // NOTE: Do NOT use fs::read("/dev/urandom") - it tries to read the entire
    // infinite device and causes an OOM kernel panic!
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let mut buf = [0u8; 16];
        if f.read_exact(&mut buf).is_ok() {
            return buf.iter().map(|b| format!("{:02x}", b)).collect();
        }
    }

    // Last resort
    "00000000".to_string()
}

/// Check for test mode flags in kernel cmdline
fn check_test_mode() {
    let cmdline = match fs::read_to_string("/proc/cmdline") {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Could not read /proc/cmdline");
            return;
        }
    };

    debug!(cmdline = %cmdline.trim(), "Kernel command line");

    if cmdline.contains("test_cni=1") {
        info!("TEST MODE: Installing static bridge CNI config");
        // In test environments (QEMU SLIRP), kindnet can't route between Docker and
        // SLIRP networks. A static bridge CNI gives kubelet a working local CNI.
        let cni_config = r#"{
  "cniVersion": "0.3.1",
  "name": "bridge",
  "plugins": [
    {
      "type": "bridge",
      "bridge": "cni0",
      "isGateway": true,
      "ipMasq": true,
      "ipam": {
        "type": "host-local",
        "subnet": "10.244.1.0/24",
        "routes": [{"dst": "0.0.0.0/0"}]
      }
    },
    {
      "type": "portmap",
      "capabilities": {"portMappings": true}
    }
  ]
}
"#;
        if let Err(e) = fs::create_dir_all("/etc/cni/net.d") {
            warn!(error = %e, "Failed to create /etc/cni/net.d");
        }
        match fs::write("/etc/cni/net.d/10-bridge.conflist", cni_config) {
            Ok(_) => info!("Static bridge CNI config installed"),
            Err(e) => warn!(error = %e, "Failed to write CNI config"),
        }
    }

    if cmdline.contains("test_update=1") {
        info!("TEST MODE: Triggering self-update in 15 seconds");
        thread::spawn(|| {
            thread::sleep(time::Duration::from_secs(15));
            info!("Executing in-VM update test");
            let status = Command::new("/usr/bin/osctl")
                .args([
                    "--endpoint",
                    "http://127.0.0.1:50051",
                    "update",
                    "--source",
                    "http://10.0.2.2:8080/update.squashfs",
                ])
                .status();
            info!(result = ?status, "In-VM update test finished");
        });
    }
}

/// Setup cgroup v2 filesystem
fn setup_cgroups() {
    let _ = fs::create_dir_all("/sys/fs/cgroup");
    match mount::<str, str, str, str>(
        Some("cgroup2"),
        "/sys/fs/cgroup",
        Some("cgroup2"),
        MsFlags::empty(),
        None,
    ) {
        Ok(_) => debug!("Mounted cgroup v2 at /sys/fs/cgroup"),
        Err(e) => {
            warn!(error = %e, "Failed to mount cgroup v2");
            return;
        }
    }

    // Enable cgroup v2 controllers in the root cgroup.
    // Without this, sub-cgroups (e.g. kubepods/) won't have controller interface files
    // like cpu.max, memory.max, etc., causing runc container creation to fail.
    let controllers = "+cpu +memory +io +pids +cpuset";
    match fs::write("/sys/fs/cgroup/cgroup.subtree_control", controllers) {
        Ok(_) => info!("Enabled cgroup v2 controllers: {}", controllers),
        Err(e) => warn!(error = %e, "Failed to enable cgroup v2 controllers"),
    }
}

/// Spawn a process with graceful error handling
fn spawn_service(name: &str, path: &str, args: &[&str]) -> Option<Child> {
    // Check if binary exists
    if let Err(e) = fs::metadata(path) {
        error!(service = name, path = path, error = %e, "Binary not found");
        return None;
    }

    info!(service = name, path = path, args = ?args, "Spawning service");

    let mut cmd = Command::new(path);
    cmd.args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match cmd.spawn() {
        Ok(child) => {
            info!(
                service = name,
                pid = child.id(),
                "✅ Service started successfully"
            );
            Some(child)
        }
        Err(e) => {
            error!(service = name, path = path, args = ?args, error = %e, "❌ Failed to spawn service");
            None
        }
    }
}

/// Reap any zombie processes (critical for PID 1)
fn reap_zombies() {
    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => break,
            Ok(status) => {
                // Successfully reaped a zombie
                if let WaitStatus::Exited(pid, code) = status {
                    debug!(
                        pid = pid.as_raw(),
                        exit_code = code,
                        "Reaped zombie process"
                    );
                }
            }
            Err(nix::errno::Errno::ECHILD) => break, // No children to reap
            Err(e) => {
                warn!(error = %e, "waitpid error");
                break;
            }
        }
    }
}

/// Spawn kubelet with appropriate configuration
/// Checks for kubeconfig and adds --kubeconfig argument if available
fn spawn_kubelet() -> Option<Child> {
    let kubelet_path = if std::path::Path::new("/var/lib/keel/bin/kubelet").exists() {
        info!("Using override kubelet from /var/lib/keel/bin/kubelet");
        "/var/lib/keel/bin/kubelet"
    } else {
        "/usr/bin/kubelet"
    };

    // Ensure kubelet directories exist
    let _ = fs::create_dir_all("/var/lib/kubelet/pki");
    let _ = fs::create_dir_all("/var/lib/kubelet");

    // Check if kubeconfig exists (set during bootstrap)
    let bootstrap_kubeconfig = "/var/lib/keel/kubernetes/kubelet.kubeconfig";
    let kubeconfig_path = "/var/lib/kubelet/kubeconfig"; // Permanent kubeconfig after CSR
    let mut args = vec![
        "--config=/etc/kubernetes/kubelet-config.yaml",
        "--cert-dir=/var/lib/kubelet/pki",
        "--v=2",
    ];

    // Set hostname override to ensure kubelet uses valid node name
    // Priority:
    // 1. Node name from bootstrap config (if bootstrapped)
    // 2. System hostname (if set)
    // 3. Generated fallback
    let bootstrap_config_path = "/var/lib/keel/kubernetes/bootstrap.json";
    let hostname =
        if let Ok(config) = keel_config::bootstrap::BootstrapConfig::load(bootstrap_config_path) {
            info!(node_name = %config.node_name, "Using node name from bootstrap configuration");
            config.node_name
        } else {
            nix::unistd::gethostname()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_default()
        };

    let hostname_arg = if !hostname.is_empty() && hostname != "(none)" && hostname != "localhost" {
        Some(format!("--hostname-override={}", hostname))
    } else {
        // Generate a fallback node name
        let fallback = format!("keelos-node-{}", &uuid_like_id()[..8]);
        Some(format!("--hostname-override={}", fallback))
    };
    if let Some(ref arg) = hostname_arg {
        args.push(arg);
    }

    // Bootstrap flow:
    // 1. If bootstrap kubeconfig exists but permanent doesn't -> initial bootstrap
    // 2. If permanent kubeconfig exists -> already joined, use permanent
    // 3. If neither exists -> standalone mode
    if std::path::Path::new(bootstrap_kubeconfig).exists() {
        if !std::path::Path::new(kubeconfig_path).exists() {
            // Initial bootstrap - kubelet will use bootstrap token to generate CSR
            info!(
                bootstrap_path = bootstrap_kubeconfig,
                target_path = kubeconfig_path,
                "Using bootstrap kubeconfig for initial cluster join"
            );
            args.push("--bootstrap-kubeconfig=/var/lib/keel/kubernetes/kubelet.kubeconfig");
            args.push("--kubeconfig=/var/lib/kubelet/kubeconfig");
        } else {
            // Already bootstrapped - use permanent kubeconfig
            info!(path = kubeconfig_path, "Using permanent kubeconfig");
            args.push("--kubeconfig=/var/lib/kubelet/kubeconfig");
        }
    } else if std::path::Path::new(kubeconfig_path).exists() {
        // Only permanent kubeconfig exists
        info!(path = kubeconfig_path, "Using permanent kubeconfig");
        args.push("--kubeconfig=/var/lib/kubelet/kubeconfig");
    } else {
        debug!("No kubeconfig found - kubelet will run in standalone mode");
    }

    spawn_service("kubelet", kubelet_path, &args)
}

/// Import pre-loaded container images from known locations into containerd
/// This ensures images like the pause container are available without network access
fn import_preloaded_images() {
    // Check multiple locations for pre-loaded images:
    // 1. /usr/share/keel/images/ - bundled in the initramfs (e.g., pause image)
    // 2. /data/images/ - pre-populated on the data partition (e.g., kube-proxy, kindnet)
    let image_dirs = ["/usr/share/keel/images", "/data/images"];

    for images_dir in &image_dirs {
        info!(dir = images_dir, "Scanning for pre-loaded container images");
        let entries = match fs::read_dir(images_dir) {
            Ok(entries) => entries,
            Err(e) => {
                info!(dir = images_dir, error = %e, "Pre-loaded images directory not found");
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("tar") {
                let path_str = path.to_string_lossy();
                let file_size = fs::metadata(&*path_str).map(|m| m.len()).unwrap_or(0);
                info!(image = %path_str, size_bytes = file_size, "Importing pre-loaded container image");
                match Command::new("/usr/bin/ctr")
                    .args(["-n", "k8s.io", "images", "import", &path_str])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        info!(image = %path_str, stdout = %stdout, "Successfully imported container image");
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        warn!(image = %path_str, stderr = %stderr, stdout = %stdout, "Failed to import container image");
                    }
                    Err(e) => {
                        warn!(image = %path_str, error = %e, "Failed to run ctr images import");
                    }
                }
            }
        }
    }
}

/// Main supervision loop for system services
fn supervise_services() -> Result<(), InitError> {
    // Start keel-agent first - it handles bootstrap
    info!("Starting keel-agent");
    let mut agent = spawn_service("keel-agent", "/usr/bin/keel-agent", &[]);

    // Start container services immediately
    // If bootstrap kubeconfig exists, kubelet will use it for cluster join
    // If not, kubelet runs in standalone mode and will be restarted when bootstrap completes
    info!("Starting containerd");
    let mut containerd: Option<Child> = spawn_service("containerd", "/usr/bin/containerd", &[]);

    // Give containerd a moment to initialize its socket
    thread::sleep(time::Duration::from_secs(2));

    // Import pre-loaded container images (e.g., pause image for pod sandboxes)
    import_preloaded_images();

    info!("Starting kubelet");
    let mut kubelet: Option<Child> = spawn_kubelet();

    // Track restart counts for backoff
    let mut agent_restart_count: u32 = 0;
    let max_restart_delay_secs: u64 = 60;

    // Supervision loop
    loop {
        // Reap any zombie processes first
        reap_zombies();

        // Check containerd - critical service
        if let Some(ref mut child) = containerd {
            if let Ok(Some(status)) = child.try_wait() {
                error!(service = "containerd", exit_status = %status, "Critical service exited");
                containerd = spawn_service("containerd", "/usr/bin/containerd", &[]);
                if containerd.is_none() {
                    error!("containerd restart failed - system degraded");
                }
            }
        }

        // Check keel-agent - restart with backoff
        if let Some(ref mut child) = agent {
            if let Ok(Some(status)) = child.try_wait() {
                let delay = std::cmp::min(1u64 << agent_restart_count, max_restart_delay_secs);
                warn!(
                    service = "keel-agent",
                    exit_status = %status,
                    attempt = agent_restart_count + 1,
                    backoff_secs = delay,
                    "Service exited, restarting with backoff"
                );

                thread::sleep(time::Duration::from_secs(delay));

                agent = spawn_service("keel-agent", "/usr/bin/keel-agent", &[]);
                if agent.is_some() {
                    agent_restart_count = agent_restart_count.saturating_add(1);
                }
            }
        }

        // Check kubelet - log but continue (maintenance mode)
        if let Some(ref mut child) = kubelet {
            if let Ok(Some(status)) = child.try_wait() {
                warn!(service = "kubelet", exit_status = %status, "Kubelet exited - node in maintenance mode");
                kubelet = None; // Don't restart automatically
            }
        }

        // Check for bootstrap kubeconfig to start or restart services
        let bootstrap_kubeconfig = "/var/lib/keel/kubernetes/kubelet.kubeconfig";
        let permanent_kubeconfig = "/var/lib/kubelet/kubeconfig";
        let bootstrap_exists = std::path::Path::new(bootstrap_kubeconfig).exists();
        let permanent_exists = std::path::Path::new(permanent_kubeconfig).exists();

        // If bootstrap kubeconfig has just appeared and kubelet isn't configured for it,
        // restart kubelet to pick up the bootstrap configuration
        if bootstrap_exists && kubelet.is_none() {
            info!("Bootstrap kubeconfig detected - restarting kubelet with cluster config");
            kubelet = spawn_kubelet();
        }

        // Handle explicit restart signal or permanent kubeconfig appearing
        let should_restart = if std::path::Path::new("/run/keel/restart-kubelet").exists() {
            // Explicit restart signal from keel-agent
            info!("Kubelet restart signal detected");
            true // Restart regardless of whether kubelet is running
        } else if bootstrap_exists && permanent_exists && kubelet.is_some() {
            // Permanent kubeconfig appeared - kubelet successfully bootstrapped
            // Restart to switch from bootstrap to permanent kubeconfig
            static SWITCHED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !SWITCHED.load(std::sync::atomic::Ordering::Relaxed) {
                info!("Permanent kubeconfig detected - restarting kubelet to use it");
                SWITCHED.store(true, std::sync::atomic::Ordering::Relaxed);
                true
            } else {
                false
            }
        } else {
            false
        };

        // Restart kubelet if signal detected or kubeconfig changed
        if should_restart {
            if let Some(ref mut child) = kubelet {
                info!(pid = child.id(), "Stopping kubelet for restart");
                let _ = child.kill();
                let _ = child.wait();
                info!("Kubelet process stopped, preparing to respawn");
            }
            let _ = fs::remove_file("/run/keel/restart-kubelet");
            // Restart kubelet with new configuration
            info!("Calling spawn_kubelet() to restart with updated config");
            kubelet = spawn_kubelet();
            if let Some(ref child) = kubelet {
                info!(pid = child.id(), "✅ Kubelet successfully restarted");
            } else {
                error!("⚠️  CRITICAL: spawn_kubelet() returned None - kubelet failed to restart!");
                error!("This means kubelet will not join the cluster. Check logs above for spawn errors.");
            }
        }

        thread::sleep(time::Duration::from_secs(5));
    }
}

/// Infinite maintenance loop - PID 1 must never exit
fn maintenance_loop() -> ! {
    info!("Init process entering maintenance loop");
    loop {
        // Continue reaping zombies even in maintenance mode
        reap_zombies();
        thread::sleep(time::Duration::from_secs(60));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_error_display() {
        let mount_err = InitError::Mount("test mount error".to_string());
        assert!(format!("{}", mount_err).contains("Mount error"));

        let spawn_err = InitError::Spawn("test spawn error".to_string());
        assert!(format!("{}", spawn_err).contains("Process spawn error"));
    }
}
