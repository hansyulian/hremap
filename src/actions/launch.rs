use std::process::Command;

pub fn launch(command: &str) {
    tracing::info!("Launching: {}", command);
    match Command::new("sh").arg("-c").arg(command).spawn() {
        Ok(_) => tracing::info!("Launched: {}", command),
        Err(e) => tracing::error!("Failed to launch '{}': {}", command, e),
    }
}
