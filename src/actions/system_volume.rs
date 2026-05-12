use super::utils::run_command;
use crate::config::VolumeDirection;

pub fn system_volume(direction: &VolumeDirection, amount: f32) {
    let arg = match direction {
        VolumeDirection::Up => {
            format!("{}%+", (amount * 100.0) as u32)
        }
        VolumeDirection::Down => format!("{}%-", (amount * 100.0) as u32),
        VolumeDirection::Mute => "toggle".to_string(),
    };

    let (cmd, args): (&str, Vec<&str>) = match direction {
        VolumeDirection::Mute => ("wpctl", vec!["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]),
        _ => (
            "wpctl",
            vec!["set-volume", "--limit", "1.0", "@DEFAULT_AUDIO_SINK@", &arg],
        ),
    };

    run_command(cmd, &args);
}
