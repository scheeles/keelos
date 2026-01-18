use std::process::{Command, Stdio};
use std::{thread, time};

fn main() {
    println!(">>> Welcome to MaticOS v0.1 <<<");
    println!("Init process started (PID 1).");

    // Phase 2: Spawn the Matic Agent
    // The agent is responsible for the gRPC API and node management.
    println!("Starting Matic Agent...");
    
    match Command::new("/usr/bin/matic-agent")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn() 
    {
        Ok(mut child) => {
            println!("Matic Agent spawned with PID {}", child.id());
            
            // Simple supervision loop: just wait for it for now.
            // In Phase 3, we'll add a proper event loop and reconciliation.
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        println!("Matic Agent exited with {}. Restarting...", status);
                        // TODO: Restart logic
                        break;
                    }
                    Ok(None) => {
                        // Still running, sleep and continue
                        thread::sleep(time::Duration::from_secs(5));
                    }
                    Err(e) => {
                        println!("Error waiting for Agent: {}", e);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            println!("FAILED to start Matic Agent: {}", e);
        }
    }

    println!("Init process entering maintenance loop...");
    loop {
    }
}
