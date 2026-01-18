use std::process::{Command, Stdio};
use std::{thread, time, fs};
use nix::mount::{mount, MsFlags};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(">>> Welcome to MaticOS v0.1 <<<");
    println!("Init process started (PID 1).");

    // Phase 1.5: Setup API filesystems
    println!("Mounting api filesystems...");
    
    // Ensure directories exist
    let _ = fs::create_dir_all("/proc");
    let _ = fs::create_dir_all("/sys");
    let _ = fs::create_dir_all("/dev");
    let _ = fs::create_dir_all("/tmp");

    mount::<str, str, str, str>(Some("none"), "/proc", Some("proc"), MsFlags::empty(), None)?;
    mount::<str, str, str, str>(Some("none"), "/sys", Some("sysfs"), MsFlags::empty(), None)?;
    mount::<str, str, str, str>(Some("none"), "/dev", Some("devtmpfs"), MsFlags::empty(), None)?;
    mount::<str, str, str, str>(Some("none"), "/tmp", Some("tmpfs"), MsFlags::empty(), None)?;
    // Phase 1.6: Setup basic networking
    println!("Initializing networking...");

    // Check if busybox exists
    if std::fs::metadata("/bin/busybox").is_err() {
        println!("Warning: /bin/busybox not found. Networking validation may fail.");
    }

    match Command::new("/bin/busybox").args(&["ifconfig", "lo", "127.0.0.1", "up"]).status() {
        Ok(_) => println!("ifconfig lo up: OK"),
        Err(e) => println!("ifconfig lo failed: {}", e),
    }
    match Command::new("/bin/busybox").args(&["ifconfig", "eth0", "10.0.2.15", "netmask", "255.255.255.0", "up"]).status() {
        Ok(_) => println!("ifconfig eth0 up: OK"),
        Err(e) => println!("ifconfig eth0 failed: {}", e),
    }
    match Command::new("/bin/busybox").args(&["route", "add", "default", "gw", "10.0.2.2"]).status() {
        Ok(_) => println!("route add default: OK"),
        Err(e) => println!("route add default failed: {}", e),
    }

    if let Ok(cmdline) = fs::read_to_string("/proc/cmdline") {
        println!("Kernel cmdline: {}", cmdline);
        if cmdline.contains("test_update=1") {
            println!(">>> TEST MODE: Triggering self-update in 15 seconds...");
            thread::spawn(|| {
                thread::sleep(time::Duration::from_secs(15));
                println!(">>> Executing in-VM update test...");
                let status = Command::new("/usr/bin/osctl")
                    .arg("--endpoint")
                    .arg("http://127.0.0.1:50051")
                    .arg("update")
                    .arg("--source")
                    .arg("http://10.0.2.2:8080/update.squashfs")
                    .status();
                println!(">>> in-VM update test finished with: {:?}", status);
            });
        }
    }

    // Setup cgroup v2
    let _ = fs::create_dir_all("/sys/fs/cgroup");
    match mount::<str, str, str, str>(Some("cgroup2"), "/sys/fs/cgroup", Some("cgroup2"), MsFlags::empty(), None) {
        Ok(_) => println!("Mounted cgroup v2 at /sys/fs/cgroup"),
        Err(e) => println!("Warning: Failed to mount cgroup v2: {}", e),
    }

    // Phase 3: Spawn containerd
    println!("Starting containerd...");
    let containerd_path = "/usr/bin/containerd";
    // ... (rest of the code)
    match fs::metadata(containerd_path) {
        Ok(meta) => {
            println!("{} found: file={}, symlink={}", containerd_path, meta.is_file(), meta.file_type().is_symlink());
        }
        Err(e) => {
            println!("ERROR: metadata for {} failed: {}", containerd_path, e);
            // List /usr/bin to see what's there
            if let Ok(entries) = std::fs::read_dir("/usr/bin") {
                println!("Contents of /usr/bin:");
                for entry in entries {
                    if let Ok(e) = entry {
                        println!("  {:?}", e.file_name());
                    }
                }
            }
        }
    }

    let mut containerd = Command::new(containerd_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    println!("containerd spawned with PID {}", containerd.id());

    // Phase 2: Spawn the Matic Agent
    println!("Starting Matic Agent...");
    let mut agent = Command::new("/usr/bin/matic-agent")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    println!("Matic Agent spawned with PID {}", agent.id());

    // Phase 4: Spawn Kubelet
    println!("Starting kubelet...");
    
    // Runtime Strategy: Check for local override first
    let override_path = "/var/lib/matic/bin/kubelet";
    let system_path = "/usr/bin/kubelet";
    
    let kubelet_path = if std::path::Path::new(override_path).exists() {
        println!("*** USING OVERRIDE KUBELET: {} ***", override_path);
        override_path
    } else {
        println!("Using system kubelet: {}", system_path);
        system_path
    };

    let mut kubelet = Command::new(kubelet_path)
        .arg("--config=/etc/kubernetes/kubelet-config.yaml")
        .arg("--v=2") // Verbose logging for debugging
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    println!("kubelet spawned with PID {}", kubelet.id());

    // Supervision loop
    loop {
        // Simple check: if either exits, we log and eventually restart
        // In this minimal version, we'll just wait and exit if any fails
        if let Ok(Some(status)) = containerd.try_wait() {
            println!("containerd exited with {}. System halted.", status);
            break;
        }
        if let Ok(Some(status)) = agent.try_wait() {
            println!("Matic Agent exited with {}. Restarting...", status);
            // restart logic for agent
            agent = Command::new("/usr/bin/matic-agent")
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?;
        }
        if let Ok(Some(status)) = kubelet.try_wait() {
            println!("kubelet exited with {}. Continuing (maintenance mode)...", status);
        }
        thread::sleep(time::Duration::from_secs(5));
    }

    println!("Init process entering maintenance loop...");
    loop {
        thread::sleep(time::Duration::from_secs(3600));
    }
    
    Ok(())
}

