use std::thread;
use std::time::Duration;

fn main() {
    println!(">>> Welcome to MaticOS v0.1 <<<");
    println!("Init process started (PID 1).");

    // In a real implementation, we would:
    // 1. Mount filesystems (/proc, /sys)
    // 2. Start the Agent
    // 3. Reap zombies

    println!("Entering infinite loop to keep system alive...");
    loop {
        // Sleep to prevent CPU spin
        thread::sleep(Duration::from_secs(60));
    }
}
