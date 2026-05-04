use crate::actions;
use anyhow::Result;
use evdev::{EventType, InputEvent, Key};
use futures_util::StreamExt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::sync::RwLock;
use tokio_stream::StreamMap;
use tokio_util::sync::CancellationToken;
mod utils;

use crate::config::{
    compute_modifier_index, is_modifier_key, Action, Config, KeyCombo, LayerMode, LookupResult,
    MacroMode, ResolvedLayer,
};
use crate::watcher::WindowInfo;

type SharedModifiers = Arc<RwLock<HashSet<u16>>>;
type MacroTx = mpsc::UnboundedSender<InputEvent>;

// ─── Input State ─────────────────────────────────────────────────────────────

pub struct InputState<'a> {
    pub current_layer: &'a ResolvedLayer,
    pub shift_layer: Option<&'a ResolvedLayer>,
    pub shift_trigger_key: Option<u16>,
    pub held_modifiers: SharedModifiers,
    pub macro_cancel: Option<CancellationToken>,
    pub pending_releases: HashMap<u16, Action>,
}

impl<'a> InputState<'a> {
    pub fn new(default_layer: &'a ResolvedLayer) -> Self {
        Self {
            current_layer: default_layer,
            shift_layer: None,
            shift_trigger_key: None,
            held_modifiers: Arc::new(RwLock::new(HashSet::new())),
            macro_cancel: None,
            pending_releases: HashMap::new(),
        }
    }

    pub fn active_layer(&self) -> &ResolvedLayer {
        self.shift_layer.unwrap_or(self.current_layer)
    }
}

// ─── Device filtering ────────────────────────────────────────────────────────

fn should_grab(device: &evdev::Device, device_names: &[String]) -> bool {
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

// ─── Output helpers ───────────────────────────────────────────────────────────

fn emit_key(output: &mut evdev::uinput::VirtualDevice, code: u16, value: i32) -> Result<()> {
    output.emit(&[InputEvent::new(EventType::KEY, code, value)])?;
    Ok(())
}

fn emit_combo(
    output: &mut evdev::uinput::VirtualDevice,
    combo: &KeyCombo,
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

fn stop_macro(state: &mut InputState) {
    if let Some(token) = state.macro_cancel.take() {
        token.cancel();
    }
}

fn spawn_macro(
    tx: MacroTx,
    steps: Vec<crate::config::MacroStep>,
    mode: MacroMode,
    held_modifiers: SharedModifiers,
) -> CancellationToken {
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
    token
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
                    stop_macro(state);
                    let token = spawn_macro(
                        tx.clone(),
                        steps,
                        MacroMode::Once,
                        state.held_modifiers.clone(),
                    );
                    state.macro_cancel = Some(token);
                }
            }
            MacroMode::Hold => {
                if value == 1 {
                    stop_macro(state);
                    let token = spawn_macro(
                        tx.clone(),
                        steps,
                        MacroMode::Hold,
                        state.held_modifiers.clone(),
                    );
                    state.macro_cancel = Some(token);
                } else if value == 0 {
                    stop_macro(state);
                }
            }
            MacroMode::Toggle => {
                if value == 1 {
                    if state.macro_cancel.is_some() {
                        stop_macro(state);
                    } else {
                        let token = spawn_macro(
                            tx.clone(),
                            steps,
                            MacroMode::Toggle,
                            state.held_modifiers.clone(),
                        );
                        state.macro_cancel = Some(token);
                    }
                }
            }
        },
    }
    Ok(())
}

// ─── Main event loop ──────────────────────────────────────────────────────────

pub async fn run(mut window_rx: watch::Receiver<Option<WindowInfo>>, config: Config) -> Result<()> {
    let mut output = utils::build_output_device()?;
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
