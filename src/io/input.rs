use evdev::{EventType, InputEvent, Key, RelativeAxisType};

use crate::config::utils::{WHEEL_DOWN, WHEEL_UP};

pub fn should_passthrough(event: &InputEvent) -> bool {
    if event.event_type() == EventType::KEY {
        return false;
    }
    if event.event_type() == EventType::RELATIVE
        && (event.code() == RelativeAxisType::REL_WHEEL.0
            || event.code() == RelativeAxisType::REL_WHEEL_HI_RES.0)
    {
        return false;
    }

    return true;
}

pub fn resolve_key(event: &InputEvent) -> Key {
    if event.event_type() == EventType::RELATIVE
        && (event.code() == RelativeAxisType::REL_WHEEL.0
            || event.code() == RelativeAxisType::REL_WHEEL_HI_RES.0)
    {
        return if event.value() > 0 {
            WHEEL_UP
        } else {
            WHEEL_DOWN
        };
    }

    return Key::new(event.code());
}

pub fn resolve_value(event: &InputEvent) -> i32 {
    if event.event_type() == EventType::RELATIVE
        && (event.code() == RelativeAxisType::REL_WHEEL.0
            || event.code() == RelativeAxisType::REL_WHEEL_HI_RES.0)
    {
        return 1;
    }

    return event.value();
}

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
