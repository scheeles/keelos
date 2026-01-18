//! MaticOS Init Process (PID 1)
//!
//! This is the first process started by the kernel. As PID 1, it has special
//! responsibilities:
//! - It must NEVER panic or exit unexpectedly
//! - It must reap zombie processes
//! - It must supervise critical system services
//!
//! All errors are handled gracefully - the system will continue running
//! in a degraded/maintenance mode rather than crashing.

use std::process::{Child, Command, Stdio};
use std::{thread, time, fs};
use nix::mount::{mount, MsFlags};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;

/// Entry point - wraps run() to ensure PID 1 never exits unexpectedly
fn main() {
    println!(">>> Welcome to MaticOS v0.1 <<<");
    println!("Init process started (PID 1).");

    if let Err(e) = run() {
        eprintln!("FATAL: Init encountered an error: {}", e);
        eprintln!("System entering maintenance mode...");
    }

    // PID 1 must never exit - enter infinite maintenance loop
    maintenance_loop();
}

/// Main init logic - all errors are propagated but never cause a panic
fn run() -> Result<(), InitError> {
    // Phase 1: Mount essential filesystems
    setup_filesystems()?;

    // Phase 2: Setup networking
    setup_networking();

    // Phase 3: Check for test mode
    check_test_mode();

    // Phase 4: Setup cgroups
    setup_cgroups();

    // Phase 5: Start and supervise services
    supervise_services()?;

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
    println!("Mounting API filesystems...");

    // Ensure directories exist (ignore errors - they may already exist)
    let _ = fs::create_dir_all("/proc");
    let _ = fs::create_dir_all("/sys");
    let _ = fs::create_dir_all("/dev");
    let _ = fs::create_dir_all("/tmp");

    // Mount proc - critical for process management
    if let Err(e) = mount::<str, str, str, str>(Some("none"), "/proc", Some("proc"), MsFlags::empty(), None) {
        eprintln!("WARNING: Failed to mount /proc: {}", e);
        // proc is critical but we continue - some functionality will be degraded
    } else {
        println!("Mounted /proc");
    }

    // Mount sysfs
    if let Err(e) = mount::<str, str, str, str>(Some("none"), "/sys", Some("sysfs"), MsFlags::empty(), None) {
        eprintln!("WARNING: Failed to mount /sys: {}", e);
    } else {
        println!("Mounted /sys");
    }

    // Mount devtmpfs - critical for device access
    if let Err(e) = mount::<str, str, str, str>(Some("none"), "/dev", Some("devtmpfs"), MsFlags::empty(), None) {
        eprintln!("WARNING: Failed to mount /dev: {}", e);
    } else {
        println!("Mounted /dev");
    }

    // Mount tmpfs
    if let Err(e) = mount::<str, str, str, str>(Some("none"), "/tmp", Some("tmpfs"), MsFlags::empty(), None) {
        eprintln!("WARNING: Failed to mount /tmp: {}", e);
    } else {
        println!("Mounted /tmp");
    }

    Ok(())
}

/// Configure basic networking (loopback and primary interface)
fn setup_networking() {
    println!("Initializing networking...");

    // Check if busybox exists
    if fs::metadata("/bin/busybox").is_err() {
        eprintln!("WARNING: /bin/busybox not found. Networking configuration may fail.");
        return;
    }

    // Configure loopback
    match Command::new("/bin/busybox").args(["ifconfig", "lo", "127.0.0.1", "up"]).status() {
        Ok(status) if status.success() => println!("Configured loopback interface"),
        Ok(status) => eprintln!("WARNING: ifconfig lo exited with: {}", status),
        Err(e) => eprintln!("WARNING: Failed to configure loopback: {}", e),
    }

    // Configure eth0 (QEMU default)
    match Command::new("/bin/busybox").args(["ifconfig", "eth0", "10.0.2.15", "netmask", "255.255.255.0", "up"]).status() {
        Ok(status) if status.success() => println!("Configured eth0 interface"),
        Ok(status) => eprintln!("WARNING: ifconfig eth0 exited with: {}", status),
        Err(e) => eprintln!("WARNING: Failed to configure eth0: {}", e),
    }

    // Add default route
    match Command::new("/bin/busybox").args(["route", "add", "default", "gw", "10.0.2.2"]).status() {
        Ok(status) if status.success() => println!("Added default route"),
        Ok(status) => eprintln!("WARNING: route add exited with: {}", status),
        Err(e) => eprintln!("WARNING: Failed to add default route: {}", e),
    }
}

/// Check for test mode flags in kernel cmdline
fn check_test_mode() {
    let cmdline = match fs::read_to_string("/proc/cmdline") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("WARNING: Could not read /proc/cmdline: {}", e);
            return;
        }
    };

    println!("Kernel cmdline: {}", cmdline.trim());

    if cmdline.contains("test_update=1") {
        println!(">>> TEST MODE: Triggering self-update in 15 seconds...");
        thread::spawn(|| {
            thread::sleep(time::Duration::from_secs(15));
            println!(">>> Executing in-VM update test...");
            let status = Command::new("/usr/bin/osctl")
                .args(["--endpoint", "http://127.0.0.1:50051", "update", "--source", "http://10.0.2.2:8080/update.squashfs"])
                .status();
            println!(">>> in-VM update test finished with: {:?}", status);
        });
    }
}

/// Setup cgroup v2 filesystem
fn setup_cgroups() {
    let _ = fs::create_dir_all("/sys/fs/cgroup");
    match mount::<str, str, str, str>(Some("cgroup2"), "/sys/fs/cgroup", Some("cgroup2"), MsFlags::empty(), None) {
        Ok(_) => println!("Mounted cgroup v2 at /sys/fs/cgroup"),
        Err(e) => eprintln!("WARNING: Failed to mount cgroup v2: {}", e),
    }
}

/// Spawn a process with graceful error handling
fn spawn_service(name: &str, path: &str, args: &[&str]) -> Option<Child> {
    // Check if binary exists
    if let Err(e) = fs::metadata(path) {
        eprintln!("ERROR: {} binary not found at {}: {}", name, path, e);
        return None;
    }

    let mut cmd = Command::new(path);
    cmd.args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match cmd.spawn() {
        Ok(child) => {
            println!("{} spawned with PID {}", name, child.id());
            Some(child)
        }
        Err(e) => {
            eprintln!("ERROR: Failed to spawn {}: {}", name, e);
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
                    println!("Reaped zombie process {} (exit code: {})", pid, code);
                }
            }
            Err(nix::errno::Errno::ECHILD) => break, // No children to reap
            Err(e) => {
                eprintln!("WARNING: waitpid error: {}", e);
                break;
            }
        }
    }
}

/// Main supervision loop for system services
fn supervise_services() -> Result<(), InitError> {
    // Start containerd
    println!("Starting containerd...");
    let mut containerd = spawn_service("containerd", "/usr/bin/containerd", &[]);

    // Start matic-agent
    println!("Starting matic-agent...");
    let mut agent = spawn_service("matic-agent", "/usr/bin/matic-agent", &[]);

    // Start kubelet (with override support)
    println!("Starting kubelet...");
    let kubelet_path = if std::path::Path::new("/var/lib/matic/bin/kubelet").exists() {
        println!("*** USING OVERRIDE KUBELET ***");
        "/var/lib/matic/bin/kubelet"
    } else {
        "/usr/bin/kubelet"
    };
    let mut kubelet = spawn_service("kubelet", kubelet_path, &["--config=/etc/kubernetes/kubelet-config.yaml", "--v=2"]);

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
                eprintln!("CRITICAL: containerd exited with {}. Attempting restart...", status);
                containerd = spawn_service("containerd", "/usr/bin/containerd", &[]);
                if containerd.is_none() {
                    eprintln!("CRITICAL: containerd restart failed. System degraded.");
                }
            }
        }

        // Check matic-agent - restart with backoff
        if let Some(ref mut child) = agent {
            if let Ok(Some(status)) = child.try_wait() {
                eprintln!("matic-agent exited with {}. Restarting (attempt {})...", status, agent_restart_count + 1);
                
                // Exponential backoff: 1s, 2s, 4s, 8s, ... up to max
                let delay = std::cmp::min(1u64 << agent_restart_count, max_restart_delay_secs);
                thread::sleep(time::Duration::from_secs(delay));
                
                agent = spawn_service("matic-agent", "/usr/bin/matic-agent", &[]);
                if agent.is_some() {
                    agent_restart_count = agent_restart_count.saturating_add(1);
                }
            }
        }

        // Check kubelet - log but continue (maintenance mode)
        if let Some(ref mut child) = kubelet {
            if let Ok(Some(status)) = child.try_wait() {
                eprintln!("kubelet exited with {}. Node in maintenance mode.", status);
                kubelet = None; // Don't restart automatically
            }
        }

        thread::sleep(time::Duration::from_secs(5));
    }
}

/// Infinite maintenance loop - PID 1 must never exit
fn maintenance_loop() -> ! {
    println!("Init process entering maintenance loop...");
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
