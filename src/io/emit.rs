use crate::config::RuntimeKeyCombo;
use anyhow::Result;
use evdev::{EventType, InputEvent};
use std::collections::HashSet;

fn emit_key(output: &mut evdev::uinput::VirtualDevice, code: u16, value: i32) -> Result<()> {
    output.emit(&[InputEvent::new(EventType::KEY, code, value)])?;
    Ok(())
}

pub fn emit_combo(
    output: &mut evdev::uinput::VirtualDevice,
    combo: &RuntimeKeyCombo,
    value: i32,
    held_modifiers: Option<&HashSet<u16>>,
) -> Result<()> {
    if value == 1 {
        if let Some(held) = held_modifiers {
            for code in held {
                if !combo.modifiers.iter().any(|m| m.code() == *code) {
                    emit_key(output, *code, 1)?;
                }
            }
        }
        for modifier in &combo.modifiers {
            emit_key(output, modifier.code(), 1)?;
        }
        emit_key(output, combo.key.code(), 1)?;
    } else {
        emit_key(output, combo.key.code(), 0)?;
        for modifier in combo.modifiers.iter().rev() {
            emit_key(output, modifier.code(), 0)?;
        }
        if let Some(held) = held_modifiers {
            for code in held {
                if !combo.modifiers.iter().any(|m| m.code() == *code) {
                    emit_key(output, *code, 0)?;
                }
            }
        }
    }
    Ok(())
}
