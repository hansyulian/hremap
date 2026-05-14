use super::types::InputState;
use crate::io::input::{resolve_key, resolve_value, should_passthrough};
use crate::io::output::VirtualOutputDevice;
use crate::utils::{compute_modifier_index, is_modifier_key};
use anyhow::Result;
use evdev::{EventType, InputEvent};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio_stream::StreamMap;

use super::handle_action::handle_action;
use super::input::should_grab;
use super::utils::should_skip_event_on_action;
use crate::config::{LookupResult, RuntimeConfig};
use crate::watcher::WindowInfo;

// ─── Device type hint for splitting streams ───────────────────────────────────
enum DeviceKind {
    Keyboard,
    Mouse,
}

fn device_kind(device: &evdev::Device) -> DeviceKind {
    // evdev exposes supported event types; mice have REL_X/REL_Y
    use evdev::RelativeAxisType;
    if let Some(rel) = device.supported_relative_axes() {
        if rel.contains(RelativeAxisType::REL_X) {
            return DeviceKind::Mouse;
        }
    }
    DeviceKind::Keyboard
}

// ─── Main event loop ──────────────────────────────────────────────────────────
pub async fn run(
    mut window_rx: watch::Receiver<Option<WindowInfo>>,
    config: RuntimeConfig,
) -> Result<()> {
    let mut virtual_output = VirtualOutputDevice::new()?;

    // bounded channel — macro tasks apply backpressure instead of flooding
    let (macro_tx, mut macro_rx) = mpsc::channel::<InputEvent>(32);

    let (mut keyboard_streams, mut mouse_streams) = grab_devices(&config.device_names);

    if keyboard_streams.is_empty() && mouse_streams.is_empty() {
        anyhow::bail!("No keyboard or mouse devices found");
    }

    tracing::debug!(
        "Grabbed {} keyboard(s), {} mouse(s)",
        keyboard_streams.len(),
        mouse_streams.len()
    );

    let default_layer = config
        .layers
        .get(&config.default_layer)
        .ok_or_else(|| anyhow::anyhow!("Default layer '{}' not found", config.default_layer))?;

    let mut state = InputState::new(default_layer);

    loop {
        tokio::select! {
            // ─── Macro events ─────────────────────────────────────────────
            Some(ev) = macro_rx.recv() => {
                virtual_output.emit(&[ev])?;
            }

            // ─── Window change ────────────────────────────────────────────
            _ = window_rx.changed() => {
                let active_window = window_rx.borrow_and_update().clone();
                update_base_layer(&config, &mut state, &active_window);
            }

            // ─── Keyboard events ──────────────────────────────────────────
            // keyboard gets its own branch — never buried behind 1000Hz mouse events
            Some((_, event)) = keyboard_streams.next() => {
                let ev = match event {
                    Ok(ev) => ev,
                    Err(e) => {
                        tracing::error!("Keyboard input error: {}", e);
                        continue;
                    }
                };

                if should_passthrough(&ev) {
                    if ev.event_type() != EventType::ABSOLUTE {
                        virtual_output.emit(&[ev])?;
                    }
                    continue;
                }

                process_key_event(ev, &mut virtual_output, &macro_tx, &mut state, &config, &window_rx).await?;
            }

            // ─── Mouse events ─────────────────────────────────────────────
            // movement (REL_X/REL_Y) fast-pathed immediately,
            // buttons/scroll go through full pipeline
            Some((_, event)) = mouse_streams.next() => {
                let ev = match event {
                    Ok(ev) => ev,
                    Err(e) => {
                        tracing::error!("Mouse input error: {}", e);
                        continue;
                    }
                };

                if should_passthrough(&ev) {
                    if ev.event_type() != EventType::ABSOLUTE {
                        virtual_output.emit(&[ev])?;
                    }
                    continue;
                }

                process_key_event(ev, &mut virtual_output, &macro_tx, &mut state, &config, &window_rx).await?;
            }
        }
    }
}

// ─── Shared key event processing ─────────────────────────────────────────────
async fn process_key_event<'a>(
    ev: InputEvent,
    output: &mut VirtualOutputDevice,
    macro_tx: &mpsc::Sender<InputEvent>,
    state: &mut InputState<'a>,
    config: &'a RuntimeConfig,
    window_rx: &watch::Receiver<Option<WindowInfo>>,
) -> Result<()> {
    let key = resolve_key(&ev);
    let value = resolve_value(&ev);

    // ─── Modifier tracking ────────────────────────────────────────────────
    if is_modifier_key(key) {
        match value {
            1 => {
                state.held_modifiers.insert(key.code());
            }
            0 => {
                state.held_modifiers.remove(&key.code());
            }
            _ => {}
        }
        output.emit(&[ev])?;
        return Ok(());
    }

    // ─── Shift trigger release ────────────────────────────────────────────
    if state.shift_trigger_key == Some(key.code()) {
        match value {
            0 => {
                tracing::debug!("Shift trigger released");
                state.shift_layer = None;
                state.shift_trigger_key = None;
            }
            _ => {
                return Ok(());
            }
        }
    }

    // ─── Key repeat ───────────────────────────────────────────────────────
    // reuse whatever action value==1 decided — don't re-evaluate layer/mapping
    // this prevents mismatch if layer switches mid-hold (e.g. W→UP repeat
    // should stay UP, not revert to raw W)
    if value == 2 {
        if let Some(action) = state.pending_releases.get(&key.code()) {
            handle_action(
                action.clone(),
                value,
                key,
                output,
                macro_tx,
                state,
                config,
                window_rx,
                None,
            )
            .await?;
        } else {
            output.emit(&[ev])?;
        }
        return Ok(());
    }

    // ─── Key up ───────────────────────────────────────────────────────────
    if value == 0 {
        if let Some(action) = state.pending_releases.remove(&key.code()) {
            handle_action(
                action, value, key, output, macro_tx, state, config, window_rx, None,
            )
            .await?;
        } else {
            output.emit(&[ev])?;
        }
        return Ok(());
    }

    // ─── Key down ─────────────────────────────────────────────────────────
    let modifier_index = compute_modifier_index(&state.held_modifiers);

    let action = state.active_layer().lookup(key.code(), modifier_index);
    let skipped_event = should_skip_event_on_action(&ev);

    if skipped_event && action.is_some() {
        return Ok(());
    }

    match action {
        Some(LookupResult::Exact(action)) => {
            let action = action.clone();
            state.pending_releases.insert(key.code(), action.clone());
            handle_action(
                action, value, key, output, macro_tx, state, config, window_rx, None,
            )
            .await?;
        }
        Some(LookupResult::Fallback(action)) => {
            let action = action.clone();
            let held = state.held_modifiers.clone();
            state.pending_releases.insert(key.code(), action.clone());
            handle_action(
                action,
                value,
                key,
                output,
                macro_tx,
                state,
                config,
                window_rx,
                Some(&held),
            )
            .await?;
        }
        None => {
            output.emit(&[ev])?;
        }
    }

    Ok(())
}

// ─── Layer helpers ────────────────────────────────────────────────────────────
fn update_base_layer<'a>(
    config: &'a RuntimeConfig,
    state: &mut InputState<'a>,
    window: &Option<WindowInfo>,
) {
    let layer_name = if let Some(win) = window {
        tracing::info!(
            "Window = title: '{}' wm_class: '{}' pid: '{}'",
            win.title,
            win.wm_class,
            win.pid
        );
        config
            .profile_map
            .get(&win.wm_class)
            .and_then(|l| config.layers.contains_key(l).then(|| l.as_str()))
            .unwrap_or(&config.default_layer)
    } else {
        &config.default_layer
    };

    if let Some(layer) = config.layers.get(layer_name) {
        state.current_layer = layer;
    }
}

// ─── Grab input devices ───────────────────────────────────────────────────────
fn grab_devices(
    device_names: &[String],
) -> (
    StreamMap<String, evdev::EventStream>,
    StreamMap<String, evdev::EventStream>,
) {
    let mut keyboards = StreamMap::new();
    let mut mice = StreamMap::new();

    for (_, raw_device) in evdev::enumerate() {
        if !should_grab(&raw_device, device_names) {
            continue;
        }

        let name = raw_device.name().unwrap_or("unknown").to_string();
        let kind = device_kind(&raw_device);

        let mut device: evdev::Device = raw_device.into();

        match device.grab() {
            Ok(_) => match device.into_event_stream() {
                Ok(stream) => {
                    tracing::debug!("Grabbed device: {}", name);
                    match kind {
                        DeviceKind::Keyboard => keyboards.insert(name, stream),
                        DeviceKind::Mouse => mice.insert(name, stream),
                    };
                }
                Err(e) => tracing::warn!("Failed to stream {}: {}", name, e),
            },
            Err(e) => tracing::warn!("Failed to grab {}: {}", name, e),
        }
    }

    (keyboards, mice)
}
