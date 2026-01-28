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

use nix::sys::stat::{umask, Mode};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::{thread, time};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

mod bootstrap_certs;
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

    // Generate bootstrap certificates (first boot)
    boot_tracker.start_phase("bootstrap_certs");
    if let Err(e) = bootstrap_certs::generate_bootstrap_certificates() {
        warn!(error = %e, "Failed to generate bootstrap certificates");
    }

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
    let ret = unsafe {
        libc::mount(
            b"proc\0".as_ptr() as *const i8,
            b"/proc\0".as_ptr() as *const i8,
            0,
            std::ptr::null_mut(),
        )
    };
    if ret != 0 {
        warn!("Failed to mount /proc");
    } else {
        debug!("Mounted /proc");
    }

    // Mount sysfs
    let ret = unsafe {
        libc::mount(
            b"sysfs\0".as_ptr() as *const i8,
            b"/sys\0".as_ptr() as *const i8,
            0,
            std::ptr::null_mut(),
        )
    };
    if ret != 0 {
        warn!("Failed to mount /sys");
    } else {
        debug!("Mounted /sys");
    }

    // Mount devtmpfs - critical for device access
    let ret = unsafe {
        libc::mount(
            b"devtmpfs\0".as_ptr() as *const i8,
            b"/dev\0".as_ptr() as *const i8,
            0,
            std::ptr::null_mut(),
        )
    };
    if ret != 0 {
        warn!("Failed to mount /dev");
    } else {
        debug!("Mounted /dev");
    }

    // Mount tmpfs
    let ret = unsafe {
        libc::mount(
            b"tmpfs\0".as_ptr() as *const i8,
            b"/tmp\0".as_ptr() as *const i8,
            0,
            std::ptr::null_mut(),
        )
    };
    if ret != 0 {
        warn!("Failed to mount /tmp");
    } else {
        debug!("Mounted /tmp");
    }

    info!("API filesystems mounted");
    Ok(())
}

/// Configure basic networking (loopback and primary interface)
fn setup_networking() {
    info!("Initializing networking");

    // Check if busybox exists
    if fs::metadata("/bin/busybox").is_err() {
        warn!("/bin/busybox not found - networking configuration may fail");
        return;
    }

    // Configure loopback
    match Command::new("/bin/busybox")
        .args(["ifconfig", "lo", "127.0.0.1", "up"])
        .status()
    {
        Ok(status) if status.success() => debug!("Configured loopback interface"),
        Ok(status) => warn!(exit_code = ?status.code(), "ifconfig lo failed"),
        Err(e) => warn!(error = %e, "Failed to configure loopback"),
    }

    // Configure eth0 (QEMU default)
    match Command::new("/bin/busybox")
        .args([
            "ifconfig",
            "eth0",
            "10.0.2.15",
            "netmask",
            "255.255.255.0",
            "up",
        ])
        .status()
    {
        Ok(status) if status.success() => debug!(
            interface = "eth0",
            ip = "10.0.2.15",
            "Configured network interface"
        ),
        Ok(status) => warn!(exit_code = ?status.code(), "ifconfig eth0 failed"),
        Err(e) => warn!(error = %e, "Failed to configure eth0"),
    }

    // Add default route
    match Command::new("/bin/busybox")
        .args(["route", "add", "default", "gw", "10.0.2.2"])
        .status()
    {
        Ok(status) if status.success() => debug!(gateway = "10.0.2.2", "Added default route"),
        Ok(status) => warn!(exit_code = ?status.code(), "route add failed"),
        Err(e) => warn!(error = %e, "Failed to add default route"),
    }

    info!("Networking initialized");
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
    let ret = unsafe {
        libc::mount(
            b"cgroup2\0".as_ptr() as *const i8,
            b"/sys/fs/cgroup\0".as_ptr() as *const i8,
            0,
            std::ptr::null_mut(),
        )
    };
    if ret == 0 {
        debug!("Mounted cgroup v2 at /sys/fs/cgroup");
    } else {
        warn!("Failed to mount cgroup v2");
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
