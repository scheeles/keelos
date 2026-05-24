//! Hook execution for update phases
//!
//! Provides safe execution of pre/post update hooks.

use tokio::process::Command;
use tokio::time::{timeout, Duration};
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

    const HOOK_TIMEOUT: Duration = Duration::from_secs(30);

    let result: Result<Result<std::process::ExitStatus, std::io::Error>, _> =
        timeout(HOOK_TIMEOUT, Command::new(program).args(args).status()).await;

    match result {
        Ok(Ok(status)) => {
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
        Ok(Err(e)) => {
            let msg = format!("Failed to execute hook: {}", e);
            error!(phase = phase, error = %msg, "Hook execution error");
            Err(msg)
        }
        Err(_) => {
            let msg = format!("Hook timed out after {}s", HOOK_TIMEOUT.as_secs());
            error!(phase = phase, error = %msg, "Hook timeout");
            Err(msg)
        }
    }
}
