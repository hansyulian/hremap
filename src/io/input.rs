use evdev::Key;

pub fn should_grab(device: &evdev::Device, device_names: &[String]) -> bool {
    let name = device.name().unwrap_or("").to_lowercase();

    if !device_names.is_empty() {
        if !device_names
            .iter()
            .any(|n| name.contains(&n.to_lowercase()))
        {
            return false;
        }
    }

    let supported = match device.supported_keys() {
        Some(keys) => keys,
        None => return false,
    };

    if !supported.contains(Key::KEY_A) && !supported.contains(Key::BTN_LEFT) {
        return false;
    }

    if let Some(abs) = device.supported_absolute_axes() {
        if abs.contains(evdev::AbsoluteAxisType::ABS_MT_POSITION_X) {
            tracing::info!("Skipping touchpad: {}", device.name().unwrap_or("unknown"));
            return false;
        }
    }

    true
}
