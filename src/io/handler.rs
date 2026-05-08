use super::output::build_output_device;
use super::types::InputState;
use crate::actions;
use crate::utils::{compute_modifier_index, is_modifier_key};
use anyhow::Result;
use evdev::{EventType, InputEvent, Key};
use futures_util::StreamExt;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio_stream::StreamMap;
use tokio_util::sync::CancellationToken;

use super::emit::emit_combo;
use super::global::{MacroTx, SharedModifiers};
use super::input::should_grab;
use crate::config::{Action, Config, KeyCombo, LayerMode, LookupResult, MacroMode};
use crate::watcher::WindowInfo;

// ─── Macro helpers ────────────────────────────────────────────────────────────
async fn run_macro_once(
    tx: &MacroTx,
    steps: &[crate::config::MacroStep],
    cancel: &CancellationToken,
    held_modifiers: &SharedModifiers,
) {
    let mut pressed: Vec<KeyCombo> = vec![];

    for step in steps {
        if cancel.is_cancelled() {
            break;
        }

        let held_snapshot = held_modifiers.read().await.clone();
        let held = if held_snapshot.is_empty() {
            None
        } else {
            Some(held_snapshot)
        };

        let events = build_combo_events(&step.combo, if step.up { 0 } else { 1 }, held.as_ref());

        for ev in events {
            if tx.send(ev).is_err() {
                return;
            }
        }

        if step.up {
            pressed.retain(|k| k.key != step.combo.key);
        } else {
            pressed.push(step.combo.clone());
        }

        if step.delay_ms > 0 {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(step.delay_ms)) => {}
                _ = cancel.cancelled() => { break; }
            }
        }
    }

    let held_snapshot = held_modifiers.read().await.clone();
    let held = if held_snapshot.is_empty() {
        None
    } else {
        Some(held_snapshot)
    };
    for combo in pressed.iter().rev() {
        let events = build_combo_events(combo, 0, held.as_ref());
        for ev in events {
            if tx.send(ev).is_err() {
                return;
            }
        }
    }
}

/// Build the list of InputEvents for a combo without needing the device directly.
fn build_combo_events(
    combo: &KeyCombo,
    value: i32,
    held_modifiers: Option<&HashSet<u16>>,
) -> Vec<InputEvent> {
    let mut events = vec![];
    if value == 1 {
        if let Some(held) = held_modifiers {
            for code in held {
                let already = combo.modifiers.iter().any(|m| m.code() == *code);
                if !already {
                    events.push(InputEvent::new(EventType::KEY, *code, 1));
                }
            }
        }
        for modifier in &combo.modifiers {
            events.push(InputEvent::new(EventType::KEY, modifier.code(), 1));
        }
        events.push(InputEvent::new(EventType::KEY, combo.key.code(), 1));
    } else {
        events.push(InputEvent::new(EventType::KEY, combo.key.code(), 0));
        for modifier in combo.modifiers.iter().rev() {
            events.push(InputEvent::new(EventType::KEY, modifier.code(), 0));
        }
        if let Some(held) = held_modifiers {
            for code in held {
                let already = combo.modifiers.iter().any(|m| m.code() == *code);
                if !already {
                    events.push(InputEvent::new(EventType::KEY, *code, 0));
                }
            }
        }
    }
    events
}

fn stop_macro_for_key(state: &mut InputState, key_code: u16) {
    if let Some(token) = state.macro_cancels.remove(&key_code) {
        token.cancel();
    }
}

fn spawn_macro(
    tx: MacroTx,
    steps: Vec<crate::config::MacroStep>,
    mode: MacroMode,
    held_modifiers: SharedModifiers,
    key_code: u16,
    cancels: &mut HashMap<u16, CancellationToken>,
) {
    // cancel existing for this key if any
    if let Some(existing) = cancels.remove(&key_code) {
        existing.cancel();
    }
    let token = CancellationToken::new();
    let token_clone = token.clone();
    tokio::spawn(async move {
        match mode {
            MacroMode::Once => {
                run_macro_once(&tx, &steps, &token_clone, &held_modifiers).await;
            }
            MacroMode::Hold | MacroMode::Toggle => loop {
                if token_clone.is_cancelled() {
                    break;
                }
                run_macro_once(&tx, &steps, &token_clone, &held_modifiers).await;
            },
        }
    });
    cancels.insert(key_code, token);
}

// ─── Layer helpers ────────────────────────────────────────────────────────────

fn update_base_layer<'a>(
    config: &'a Config,
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
    for (_, mut device) in evdev::enumerate() {
        if !should_grab(&device, device_names) {
            continue;
        }
        let name = device.name().unwrap_or("unknown").to_string();
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

// ─── Action handler ───────────────────────────────────────────────────────────

async fn handle_action<'a>(
    action: Action,
    value: i32,
    key: Key,
    output: &mut evdev::uinput::VirtualDevice,
    tx: &MacroTx,
    state: &mut InputState<'a>,
    config: &'a Config,
    window_rx: &watch::Receiver<Option<WindowInfo>>,
    held_modifiers: Option<&HashSet<u16>>,
) -> Result<()> {
    match action {
        Action::Key(combo) => {
            emit_combo(output, &combo, value, held_modifiers)?;
        }
        Action::Layer {
            layer: layer_name,
            mode,
        } => match mode {
            LayerMode::Shift => {
                if value == 1 {
                    if let Some(l) = config.layers.get(&layer_name) {
                        tracing::info!("Shift layer on: {}", layer_name);
                        state.shift_layer = Some(l);
                        state.shift_trigger_key = Some(key.code());
                    }
                } else if value == 0 {
                    tracing::info!("Shift layer off");
                    state.shift_layer = None;
                    state.shift_trigger_key = None;
                }
            }
            LayerMode::Toggle => {
                if value == 1 {
                    if state.shift_layer.map(|l| l.name.as_str()) == Some(layer_name.as_str()) {
                        tracing::info!("Toggle layer off: {}", layer_name);
                        state.shift_layer = None;
                    } else if let Some(l) = config.layers.get(&layer_name) {
                        tracing::info!("Toggle layer on: {}", layer_name);
                        state.shift_layer = Some(l);
                    }
                }
            }
        },
        Action::Volume { direction, amount } => {
            if value == 1 {
                actions::system_volume(&direction, amount);
            }
        }
        Action::AppVolume { direction, amount } => {
            if value == 1 {
                let pid = window_rx.borrow().as_ref().map(|w| w.pid);
                actions::app_volume(&direction, amount, pid);
            }
        }
        Action::Launch { command } => {
            if value == 1 {
                actions::launch(&command);
            }
        }
        Action::Macro { mode, steps } => match mode {
            MacroMode::Once => {
                if value == 1 {
                    spawn_macro(
                        tx.clone(),
                        steps,
                        MacroMode::Once,
                        state.held_modifiers.clone(),
                        key.code(),
                        &mut state.macro_cancels,
                    );
                }
            }
            MacroMode::Hold => {
                if value == 1 {
                    spawn_macro(
                        tx.clone(),
                        steps,
                        MacroMode::Hold,
                        state.held_modifiers.clone(),
                        key.code(),
                        &mut state.macro_cancels,
                    );
                } else if value == 0 {
                    stop_macro_for_key(state, key.code());
                }
            }
            MacroMode::Toggle => {
                if value == 1 {
                    if state.macro_cancels.contains_key(&key.code()) {
                        stop_macro_for_key(state, key.code());
                    } else {
                        spawn_macro(
                            tx.clone(),
                            steps,
                            MacroMode::Toggle,
                            state.held_modifiers.clone(),
                            key.code(),
                            &mut state.macro_cancels,
                        );
                    }
                }
            }
        },
    }
    Ok(())
}

// ─── Main event loop ──────────────────────────────────────────────────────────
pub async fn run(mut window_rx: watch::Receiver<Option<WindowInfo>>, config: Config) -> Result<()> {
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

                // prune finished macros
                state.macro_cancels.retain(|_, t| !t.is_cancelled());

                if ev.event_type() != EventType::KEY {
                    if ev.event_type() != EventType::ABSOLUTE {
                        output.emit(&[ev])?;
                    }
                    continue;
                }

                let key = Key::new(ev.code());
                let value = ev.value();

                if is_modifier_key(key) {
                    match value {
                        1 => { state.held_modifiers.write().await.insert(key.code()); }
                        0 => { state.held_modifiers.write().await.remove(&key.code()); }
                        _ => {}
                    }
                    output.emit(&[ev])?;
                    continue;
                }

                if value == 2 {
                    output.emit(&[ev])?;
                    continue;
                }

                if window_rx.has_changed()? {
                    let active_window = window_rx.borrow_and_update().clone();
                    update_base_layer(&config, &mut state, &active_window);
                }

                if value == 0 && state.shift_trigger_key == Some(key.code()) {
                    tracing::info!("Shift trigger released");
                    state.shift_layer = None;
                    state.shift_trigger_key = None;
                    continue;
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
