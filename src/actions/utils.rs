// src/actions/mod.rs
use std::process::Command;

pub fn run_command(cmd: &str, args: &[&str]) {
    tracing::debug!("Running: {} {}", cmd, args.join(" "));
    match Command::new(cmd).args(args).spawn() {
        Ok(_) => {}
        Err(e) => tracing::warn!("Command '{}' failed: {} (is it installed?)", cmd, e),
    }
}
