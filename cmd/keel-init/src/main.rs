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
use std::path::Path;
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

    // Initialize boot phase tracker
    let mut boot_tracker = telemetry::BootPhaseTracker::new();

    // Mount essential filesystems
    boot_tracker.start_phase("filesystem");
    setup_filesystems()?;

    // Set up cgroups
    boot_tracker.start_phase("cgroups");
    setup_cgroups();

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

    info!("Networking initialized");
}

/// Configure loopback interface
fn configure_loopback() {
    // Using ip command instead of busybox ifconfig for modern networking
    match Command::new("/bin/ip")
        .args(["link", "set", "lo", "up"])
        .status()
    {
        Ok(status) if status.success() => {
            match Command::new("/bin/ip")
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
    match Command::new("/bin/ip")
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
            // Add IP address
            match Command::new("/bin/ip")
                .args(["addr", "add", &cfg.ipv4_address, "dev", &iface.name])
                .status()
            {
                Ok(status) if status.success() => {
                    info!(interface = %iface.name, ip = %cfg.ipv4_address, "Static IP configured");
                }
                Ok(status) => {
                    warn!(interface = %iface.name, exit_code = ?status.code(), "Failed to set IP address");
                }
                Err(e) => {
                    warn!(interface = %iface.name, error = %e, "Failed to set IP address");
                }
            }

            // Set gateway if present
            if let Some(ref gateway) = cfg.gateway {
                match Command::new("/bin/ip")
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
                        debug!(interface = %iface.name, gateway = %gateway, "Default route configured");
                    }
                    Ok(status) => {
                        warn!(exit_code = ?status.code(), "Failed to set default route");
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to set default route");
                    }
                }
            }

            // Set MTU if non-default
            if cfg.mtu != 1500 {
                match Command::new("/bin/ip")
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

    match Command::new("/bin/ip").args(&args).status() {
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
    match Command::new("/bin/ip")
        .args(["link", "set", "eth0", "up"])
        .status()
    {
        Ok(status) if status.success() => {
            // Use QEMU's default network: 10.0.2.0/24
            match Command::new("/bin/ip")
                .args(["addr", "add", "10.0.2.15/24", "dev", "eth0"])
                .status()
            {
                Ok(status) if status.success() => {
                    debug!("Set eth0 IP to 10.0.2.15/24");
                    // Add default route
                    let _ = Command::new("/bin/ip")
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
        Err(e) => warn!(error = %e, "Failed to mount cgroup v2"),
    }
}

/// Spawn a process with graceful error handling
fn spawn_service(name: &str, path: &str, args: &[&str]) -> Option<Child> {
    // Check if binary exists
    if let Err(e) = fs::metadata(path) {
        error!(service = name, path = path, error = %e, "Binary not found");
        return None;
    }

    let mut cmd = Command::new(path);
    cmd.args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match cmd.spawn() {
        Ok(child) => {
            info!(service = name, pid = child.id(), "Service started");
            Some(child)
        }
        Err(e) => {
            error!(service = name, error = %e, "Failed to spawn service");
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

    // Check if kubeconfig exists (set during bootstrap)
    let kubeconfig_path = "/var/lib/keel/kubernetes/kubelet.kubeconfig";
    let mut args = vec!["--config=/etc/kubernetes/kubelet-config.yaml", "--v=2"];

    if std::path::Path::new(kubeconfig_path).exists() {
        info!(path = kubeconfig_path, "Using kubeconfig for kubelet");
        args.push("--kubeconfig=/var/lib/keel/kubernetes/kubelet.kubeconfig");
    } else {
        debug!("No kubeconfig found - kubelet will run in standalone mode");
    }

    spawn_service("kubelet", kubelet_path, &args)
}

/// Main supervision loop for system services
fn supervise_services() -> Result<(), InitError> {
    // Start containerd
    info!("Starting containerd");
    let mut containerd = spawn_service("containerd", "/usr/bin/containerd", &[]);

    // Start keel-agent
    info!("Starting keel-agent");
    let mut agent = spawn_service("keel-agent", "/usr/bin/keel-agent", &[]);

    // Start kubelet (with override support and kubeconfig)
    info!("Starting kubelet");
    let mut kubelet = spawn_kubelet();

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

        // Check for kubelet restart signal (from bootstrap)
        if Path::new("/run/keel/restart-kubelet").exists() {
            info!("Kubelet restart signal detected");
            // Kill existing kubelet if running
            if let Some(mut child) = kubelet {
                info!("Stopping kubelet for restart");
                let _ = child.kill();
                let _ = child.wait(); // Clean up zombie
            }
            // Remove signal file
            let _ = fs::remove_file("/run/keel/restart-kubelet");
            // Restart kubelet with new configuration
            kubelet = spawn_kubelet();
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
