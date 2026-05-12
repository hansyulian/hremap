use super::output::build_output_device;
use super::types::InputState;
use crate::io::input::{resolve_key, resolve_value, should_passthrough};
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

// ─── Main event loop ──────────────────────────────────────────────────────────
pub async fn run(
    mut window_rx: watch::Receiver<Option<WindowInfo>>,
    config: RuntimeConfig,
) -> Result<()> {
    let mut output = build_output_device()?;
    let (macro_tx, mut macro_rx) = mpsc::unbounded_channel::<InputEvent>();

    let mut streams = grab_devices(&config.device_names);

    if streams.is_empty() {
        anyhow::bail!("No keyboard or mouse devices found");
    }

    tracing::info!("Grabbed {} devices", streams.len());

    let default_layer = config
        .layers
        .get(&config.default_layer)
        .ok_or_else(|| anyhow::anyhow!("Default layer '{}' not found", config.default_layer))?;

    let mut state = InputState::new(default_layer);

    loop {
        tokio::select! {
            biased;

            // ─── Macro events come first (biased) so they drain quickly ───
            Some(ev) = macro_rx.recv() => {
                output.emit(&[ev])?;
            }

            // ─── Real input events ────────────────────────────────────────
            Some((_, event)) = streams.next() => {
                let ev = match event {
                    Ok(ev) => ev,
                    Err(e) => {
                        tracing::error!("Input error: {}", e);
                        continue;
                    }
                };

                // tracing::info!("Event type: {:?}, code: {}, value: {}", ev.event_type(), ev.code(), ev.value());
                // bypassing some events
                if should_passthrough(&ev) {
                    if ev.event_type() != EventType::ABSOLUTE{
                        output.emit(&[ev])?;
                    }
                    continue;
                }

                let key = resolve_key(&ev);
                let value = resolve_value(&ev);

                if is_modifier_key(key) {
                    match value {
                        1 => { state.held_modifiers.write().await.insert(key.code()); }
                        0 => { state.held_modifiers.write().await.remove(&key.code()); }
                        _ => {}
                    }
                    output.emit(&[ev])?;
                    continue;
                }

                // what is this for?
                // if value == 2 {
                //     output.emit(&[ev])?;
                //     continue;
                // }

                // i dont think this should be here
                if window_rx.has_changed()? {
                    let active_window = window_rx.borrow_and_update().clone();
                    update_base_layer(&config, &mut state, &active_window);
                }

                if state.shift_trigger_key == Some(key.code()){
                    match value {
                        0 => {
                            tracing::info!("Shift trigger released");
                            state.shift_layer = None;
                            state.shift_trigger_key = None;
                        }
                        _ => {
                            continue;
                        }
                    }
                }

                // ─── Key up ───────────────────────────────────────────────
                if value == 0 {
                    if let Some(action) = state.pending_releases.remove(&key.code()) {
                        handle_action(
                            action, value, key, &mut output, &macro_tx,
                            &mut state, &config, &window_rx, None,
                        ).await?;
                    } else {
                        output.emit(&[ev])?;
                    }
                    continue;
                }

                // ─── Key down ─────────────────────────────────────────────
                let modifier_index = {
                    let held = state.held_modifiers.read().await;
                    compute_modifier_index(&held)
                };

                let action = state.active_layer().lookup(key.code(), modifier_index);
                let skipped_event = should_skip_event_on_action(&ev);

                if skipped_event && action.is_some() {
                    continue;
                }

                match action {
                    Some(LookupResult::Exact(action)) => {
                        let action = action.clone();
                        state.pending_releases.insert(key.code(), action.clone());
                        handle_action(
                            action, value, key, &mut output, &macro_tx,
                            &mut state, &config, &window_rx, None,
                        ).await?;
                    }
                    Some(LookupResult::Fallback(action)) => {
                        let action = action.clone();
                        let held = state.held_modifiers.read().await.clone();
                        state.pending_releases.insert(key.code(), action.clone());
                        handle_action(
                            action, value, key, &mut output, &macro_tx,
                            &mut state, &config, &window_rx, Some(&held),
                        ).await?;
                    }
                    None => {
                        output.emit(&[ev])?;
                    }
                }
            }

        }
    }
}

// ─── Layer helpers ────────────────────────────────────────────────────────────
fn update_base_layer<'a>(
    config: &'a RuntimeConfig,
    state: &mut InputState<'a>,
    window: &Option<WindowInfo>,
) {
    let layer_name = if let Some(win) = window {
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

fn grab_devices(device_names: &[String]) -> StreamMap<String, evdev::EventStream> {
    let mut map = StreamMap::new();
    for (_, raw_device) in evdev::enumerate() {
        if !should_grab(&raw_device, device_names) {
            continue;
        }
        let name = raw_device.name().unwrap_or("unknown").to_string();

        // convert to sync device so SYN_DROPPED is handled automatically
        let mut device: evdev::Device = raw_device.into();

        match device.grab() {
            Ok(_) => match device.into_event_stream() {
                Ok(stream) => {
                    tracing::info!("Grabbed device: {}", name);
                    map.insert(name, stream);
                }
                Err(e) => tracing::warn!("Failed to stream {}: {}", name, e),
            },
            Err(e) => tracing::warn!("Failed to grab {}: {}", name, e),
        }
    }
    map
}
