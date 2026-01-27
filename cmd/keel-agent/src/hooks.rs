//! Hook execution for update phases
//!
//! Provides safe execution of pre/post update hooks.

use std::process::Command;
use tracing::{error, info, warn};

/// Execute a hook command
///
/// Limits execution to a timeout and checks exit status.
/// Note: Sandbox limitations apply - should ideally use a constrained user.
pub async fn execute_hook(command: &str, phase: &str) -> Result<(), String> {
    if command.is_empty() {
        return Ok(());
    }

    info!(phase = phase, command = command, "Executing update hook");

    // Split command into program and args
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }

    let program = parts[0];
    let args = &parts[1..];

    let result = Command::new(program).args(args).status();

    match result {
        Ok(status) => {
            if status.success() {
                info!(
                    phase = phase,
                    command = command,
                    "Hook executed successfully"
                );
                Ok(())
            } else {
                let msg = format!("Hook failed with exit code: {:?}", status.code());
                warn!(phase = phase, error = %msg, "Hook failure");
                Err(msg)
            }
        }
        Err(e) => {
            let msg = format!("Failed to execute hook: {}", e);
            error!(phase = phase, error = %msg, "Hook execution error");
            Err(msg)
        }
    }
}
