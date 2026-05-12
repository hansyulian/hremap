use evdev::{EventType, InputEvent, RelativeAxisType};

pub fn should_skip_event_on_action(ev: &InputEvent) -> bool {
    match ev.event_type() {
        EventType::RELATIVE => ev.code() == RelativeAxisType::REL_WHEEL_HI_RES.0,
        _ => false,
    }
}
