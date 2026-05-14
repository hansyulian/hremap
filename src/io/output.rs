use anyhow::Result;
use evdev::{uinput::VirtualDevice, EventType, InputEvent, Key};

pub struct VirtualOutputDevice {
    keyboard: VirtualDevice,
    mouse: VirtualDevice,
}

impl VirtualOutputDevice {
    pub fn new() -> Result<Self> {
        Ok(Self {
            keyboard: build_virtual_keyboard_output_device()?,
            mouse: build_virtual_mouse_output_device()?,
        })
    }

    pub fn emit(&mut self, events: &[InputEvent]) -> Result<()> {
        let mut keyboard_events: Vec<InputEvent> = Vec::new();
        let mut mouse_events: Vec<InputEvent> = Vec::new();

        for ev in events {
            if is_mouse_event(ev) {
                mouse_events.push(*ev);
            } else {
                keyboard_events.push(*ev);
            }
        }

        if !keyboard_events.is_empty() {
            self.keyboard.emit(&keyboard_events)?;
        }
        if !mouse_events.is_empty() {
            self.mouse.emit(&mouse_events)?;
        }

        Ok(())
    }
}

fn is_mouse_event(ev: &InputEvent) -> bool {
    match ev.event_type() {
        EventType::RELATIVE => true,
        EventType::KEY => matches!(
            Key::new(ev.code()),
            Key::BTN_LEFT | Key::BTN_RIGHT | Key::BTN_MIDDLE | Key::BTN_SIDE | Key::BTN_EXTRA
        ),
        _ => false,
    }
}

pub fn build_virtual_keyboard_output_device() -> Result<evdev::uinput::VirtualDevice> {
    Ok(evdev::uinput::VirtualDeviceBuilder::new()?
        .name("HRemap Virtual Keyboard")
        .with_keys(&evdev::AttributeSet::from_iter([
            Key::KEY_A,
            Key::KEY_B,
            Key::KEY_C,
            Key::KEY_D,
            Key::KEY_E,
            Key::KEY_F,
            Key::KEY_G,
            Key::KEY_H,
            Key::KEY_I,
            Key::KEY_J,
            Key::KEY_K,
            Key::KEY_L,
            Key::KEY_M,
            Key::KEY_N,
            Key::KEY_O,
            Key::KEY_P,
            Key::KEY_Q,
            Key::KEY_R,
            Key::KEY_S,
            Key::KEY_T,
            Key::KEY_U,
            Key::KEY_V,
            Key::KEY_W,
            Key::KEY_X,
            Key::KEY_Y,
            Key::KEY_Z,
            Key::KEY_1,
            Key::KEY_2,
            Key::KEY_3,
            Key::KEY_4,
            Key::KEY_5,
            Key::KEY_6,
            Key::KEY_7,
            Key::KEY_8,
            Key::KEY_9,
            Key::KEY_0,
            Key::KEY_ENTER,
            Key::KEY_ESC,
            Key::KEY_BACKSPACE,
            Key::KEY_TAB,
            Key::KEY_SPACE,
            Key::KEY_LEFTCTRL,
            Key::KEY_LEFTSHIFT,
            Key::KEY_LEFTALT,
            Key::KEY_RIGHTCTRL,
            Key::KEY_RIGHTSHIFT,
            Key::KEY_RIGHTALT,
            Key::KEY_LEFTMETA,
            Key::KEY_F1,
            Key::KEY_F2,
            Key::KEY_F3,
            Key::KEY_F4,
            Key::KEY_F5,
            Key::KEY_F6,
            Key::KEY_F7,
            Key::KEY_F8,
            Key::KEY_F9,
            Key::KEY_F10,
            Key::KEY_F11,
            Key::KEY_F12,
            Key::KEY_F13,
            Key::KEY_F14,
            Key::KEY_F15,
            Key::KEY_F16,
            Key::KEY_F17,
            Key::KEY_F18,
            Key::KEY_F19,
            Key::KEY_F20,
            Key::KEY_F21,
            Key::KEY_F22,
            Key::KEY_F23,
            Key::KEY_F24,
            Key::KEY_UP,
            Key::KEY_DOWN,
            Key::KEY_LEFT,
            Key::KEY_RIGHT,
            Key::KEY_HOME,
            Key::KEY_END,
            Key::KEY_DELETE,
            Key::KEY_INSERT,
            Key::KEY_PAGEUP,
            Key::KEY_PAGEDOWN,
            Key::KEY_CAPSLOCK,
            Key::KEY_MINUS,
            Key::KEY_EQUAL,
            Key::KEY_LEFTBRACE,
            Key::KEY_RIGHTBRACE,
            Key::KEY_BACKSLASH,
            Key::KEY_SEMICOLON,
            Key::KEY_APOSTROPHE,
            Key::KEY_GRAVE,
            Key::KEY_COMMA,
            Key::KEY_DOT,
            Key::KEY_SLASH,
            Key::KEY_PLAYPAUSE,
            Key::KEY_NEXTSONG,
            Key::KEY_PREVIOUSSONG,
            Key::KEY_VOLUMEUP,
            Key::KEY_VOLUMEDOWN,
            Key::KEY_MUTE,
            Key::KEY_KP0,
            Key::KEY_KP1,
            Key::KEY_KP2,
            Key::KEY_KP3,
            Key::KEY_KP4,
            Key::KEY_KP5,
            Key::KEY_KP6,
            Key::KEY_KP7,
            Key::KEY_KP8,
            Key::KEY_KP9,
            Key::KEY_KPASTERISK,
            Key::KEY_KPPLUS,
            Key::KEY_KPMINUS,
            Key::KEY_KPDOT,
            Key::KEY_KPENTER,
            Key::KEY_KPSLASH,
            Key::KEY_NUMLOCK,
            Key::KEY_SCROLLLOCK,
            Key::KEY_RIGHTMETA,
        ]))?
        .build()?)
}

pub fn build_virtual_mouse_output_device() -> Result<evdev::uinput::VirtualDevice> {
    Ok(evdev::uinput::VirtualDeviceBuilder::new()?
        .name("HRemap Virtual Mouse")
        .with_keys(&evdev::AttributeSet::from_iter([
            Key::BTN_LEFT,
            Key::BTN_RIGHT,
            Key::BTN_MIDDLE,
            Key::BTN_SIDE,
            Key::BTN_EXTRA,
        ]))?
        .with_relative_axes(&evdev::AttributeSet::from_iter([
            evdev::RelativeAxisType::REL_X,
            evdev::RelativeAxisType::REL_Y,
            evdev::RelativeAxisType::REL_WHEEL,
            evdev::RelativeAxisType::REL_HWHEEL,
            evdev::RelativeAxisType::REL_WHEEL_HI_RES,
            evdev::RelativeAxisType::REL_HWHEEL_HI_RES,
        ]))?
        .build()?)
}
