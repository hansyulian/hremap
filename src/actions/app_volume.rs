use super::utils::run_command;
use crate::config::VolumeDirection;
use std::process::Command;

pub fn app_volume(direction: &VolumeDirection, amount: f32, pid: Option<u32>) {
    let pid = match pid {
        Some(p) => p,
        None => {
            tracing::warn!("App volume: no active window PID");
            return;
        }
    };

    // find sink input index matching the PID
    let sink_index = match find_sink_input_by_pid(pid) {
        Some(index) => index,
        None => {
            tracing::warn!("App volume: no sink input found for PID {}", pid);
            return;
        }
    };

    let index_str = sink_index.to_string();
    let amount_str = match direction {
        VolumeDirection::Up => format!("{}%+", (amount * 100.0) as u32),
        VolumeDirection::Down => format!("{}%-", (amount * 100.0) as u32),
        VolumeDirection::Mute => "toggle".to_string(),
    };

    let args: Vec<&str> = match direction {
        VolumeDirection::Mute => vec!["set-sink-input-mute", &index_str, "toggle"],
        _ => vec!["set-sink-input-volume", &index_str, &amount_str],
    };

    run_command("pactl", &args);
}

fn find_sink_input_by_pid(pid: u32) -> Option<u32> {
    let output = match Command::new("pactl")
        .arg("list")
        .arg("sink-inputs")
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("pactl not found or failed: {}", e);
            return None;
        }
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut current_index: Option<u32> = None;

    for line in text.lines() {
        let line = line.trim();

        // new sink input block
        if line.starts_with("Sink Input #") {
            let index_str = line.trim_start_matches("Sink Input #");
            current_index = index_str.parse().ok();
        }

        // check for matching PID
        if line.starts_with("application.process.id") {
            let value = line
                .split('=')
                .nth(1)
                .map(|s| s.trim().trim_matches('"'))
                .and_then(|s| s.parse::<u32>().ok());

            if value == Some(pid) {
                return current_index;
            }
        }
    }

    None
}
