use super::types::InputState;
use crate::actions;
use crate::io::global::SharedModifiers;
use anyhow::Result;
use evdev::{EventType, InputEvent, Key};
use std::collections::{HashMap, HashSet};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::emit::emit_combo;
use super::global::MacroTx;
use crate::config::{Action, Config, KeyCombo, LayerMode, MacroMode, MacroStep};
use crate::watcher::WindowInfo;

pub async fn handle_action<'a>(
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

fn spawn_macro(
    tx: MacroTx,
    steps: Vec<MacroStep>,
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

fn stop_macro_for_key(state: &mut InputState, key_code: u16) {
    if let Some(token) = state.macro_cancels.remove(&key_code) {
        token.cancel();
    }
    // prune finished macros
    state.macro_cancels.retain(|_, t| !t.is_cancelled());
}

// ─── Macro helpers ────────────────────────────────────────────────────────────
async fn run_macro_once(
    tx: &MacroTx,
    steps: &[MacroStep],
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
