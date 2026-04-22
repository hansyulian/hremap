use anyhow::Result;
use evdev::{AbsInfo, AbsoluteAxisType, EventType, InputEvent, Key, UinputAbsSetup};
use futures_util::StreamExt;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use crate::actions;

use crate::config::{
    Action, Config, KeyCombo, LayerMode, MacroMode, ResolvedLayer,
    compute_modifier_index, is_modifier_key,
};
use crate::watcher::gnome::WindowInfo;

type SharedOutput = Arc<Mutex<evdev::uinput::VirtualDevice>>;

pub struct InputState<'a> {
    pub current_layer: &'a ResolvedLayer,
    pub shift_layer: Option<&'a ResolvedLayer>,
    pub held_modifiers: HashSet<u16>,
    pub macro_cancel: Option<CancellationToken>,
}

impl<'a> InputState<'a> {
    pub fn new(default_layer: &'a ResolvedLayer) -> Self {
        Self {
            current_layer: default_layer,
            shift_layer: None,
            held_modifiers: HashSet::new(),
            macro_cancel: None,
        }
    }

    pub fn active_layer(&self) -> &ResolvedLayer {
        self.shift_layer.unwrap_or(self.current_layer)
    }
}

async fn emit_combo(output: &SharedOutput, combo: &KeyCombo, value: i32) -> Result<()> {
    let mut out = output.lock().await;
    if value == 1 {
        for modifier in &combo.modifiers {
            out.emit(&[InputEvent::new(EventType::KEY, modifier.code(), 1)])?;
        }
        out.emit(&[InputEvent::new(EventType::KEY, combo.key.code(), 1)])?;
    } else {
        out.emit(&[InputEvent::new(EventType::KEY, combo.key.code(), 0)])?;
        for modifier in &combo.modifiers {
            out.emit(&[InputEvent::new(EventType::KEY, modifier.code(), 0)])?;
        }
    }
    Ok(())
}

async fn run_macro_once(
    output: &SharedOutput,
    steps: &[crate::config::MacroStep],
    cancel: &CancellationToken,
) {
    let mut pressed: Vec<KeyCombo> = vec![];

    for step in steps {
        if cancel.is_cancelled() {
            break;
        }
        if step.up {
            if let Err(e) = emit_combo(output, &step.combo, 0).await {
                tracing::error!("Macro emit error: {}", e);
            }
            pressed.retain(|k| k.key != step.combo.key);
        } else {
            if let Err(e) = emit_combo(output, &step.combo, 1).await {
                tracing::error!("Macro emit error: {}", e);
            }
            pressed.push(step.combo.clone());
        }
        if step.delay_ms > 0 {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(step.delay_ms)) => {}
                _ = cancel.cancelled() => { break; }
            }
        }
    }

    for combo in pressed.iter().rev() {
        if let Err(e) = emit_combo(output, combo, 0).await {
            tracing::error!("Macro release error: {}", e);
        }
    }
}

fn stop_macro(state: &mut InputState) {
    if let Some(token) = state.macro_cancel.take() {
        token.cancel();
    }
}

fn spawn_macro(
    output: SharedOutput,
    steps: Vec<crate::config::MacroStep>,
    mode: MacroMode,
) -> CancellationToken {
    let token = CancellationToken::new();
    let token_clone = token.clone();
    tokio::spawn(async move {
        match mode {
            MacroMode::Once => {
                run_macro_once(&output, &steps, &token_clone).await;
            }
            MacroMode::Hold | MacroMode::Toggle => {
                loop {
                    if token_clone.is_cancelled() {
                        break;
                    }
                    run_macro_once(&output, &steps, &token_clone).await;
                }
            }
        }
    });
    token
}

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

pub async fn run(
    mut window_rx: watch::Receiver<Option<WindowInfo>>,
    config: Config,
) -> Result<()> {
    let devices = evdev::enumerate()
        .filter_map(|(_, device)| {
            let supported = device.supported_keys();
            match supported {
                Some(keys) => {
                    if keys.contains(Key::KEY_A) || keys.contains(Key::BTN_LEFT) {
                        Some(device)
                    } else {
                        None
                    }
                }
                None => None,
            }
        })
        .collect::<Vec<_>>();

    if devices.is_empty() {
        anyhow::bail!("No keyboard or mouse devices found");
    }

    let output = evdev::uinput::VirtualDeviceBuilder::new()?
        .name("hremap-virtual")
        .with_keys(&evdev::AttributeSet::from_iter([
            Key::KEY_A, Key::KEY_B, Key::KEY_C, Key::KEY_D, Key::KEY_E,
            Key::KEY_F, Key::KEY_G, Key::KEY_H, Key::KEY_I, Key::KEY_J,
            Key::KEY_K, Key::KEY_L, Key::KEY_M, Key::KEY_N, Key::KEY_O,
            Key::KEY_P, Key::KEY_Q, Key::KEY_R, Key::KEY_S, Key::KEY_T,
            Key::KEY_U, Key::KEY_V, Key::KEY_W, Key::KEY_X, Key::KEY_Y,
            Key::KEY_Z, Key::KEY_1, Key::KEY_2, Key::KEY_3, Key::KEY_4,
            Key::KEY_5, Key::KEY_6, Key::KEY_7, Key::KEY_8, Key::KEY_9,
            Key::KEY_0, Key::KEY_ENTER, Key::KEY_ESC, Key::KEY_BACKSPACE,
            Key::KEY_TAB, Key::KEY_SPACE, Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT,
            Key::KEY_LEFTALT, Key::KEY_RIGHTCTRL, Key::KEY_RIGHTSHIFT,
            Key::KEY_RIGHTALT, Key::KEY_LEFTMETA, Key::KEY_F1, Key::KEY_F2,
            Key::KEY_F3, Key::KEY_F4, Key::KEY_F5, Key::KEY_F6, Key::KEY_F7,
            Key::KEY_F8, Key::KEY_F9, Key::KEY_F10, Key::KEY_F11, Key::KEY_F12,
            Key::KEY_F13, Key::KEY_F14, Key::KEY_F15, Key::KEY_F16,
            Key::KEY_F17, Key::KEY_F18, Key::KEY_F19, Key::KEY_F20,
            Key::KEY_UP, Key::KEY_DOWN, Key::KEY_LEFT, Key::KEY_RIGHT,
            Key::KEY_HOME, Key::KEY_END, Key::KEY_DELETE, Key::KEY_INSERT,
            Key::KEY_PAGEUP, Key::KEY_PAGEDOWN, Key::KEY_CAPSLOCK,
            Key::KEY_MINUS, Key::KEY_EQUAL, Key::KEY_LEFTBRACE,
            Key::KEY_RIGHTBRACE, Key::KEY_BACKSLASH, Key::KEY_SEMICOLON,
            Key::KEY_APOSTROPHE, Key::KEY_GRAVE, Key::KEY_COMMA,
            Key::KEY_DOT, Key::KEY_SLASH,
            Key::KEY_PLAYPAUSE, Key::KEY_NEXTSONG, Key::KEY_PREVIOUSSONG,
            Key::KEY_VOLUMEUP, Key::KEY_VOLUMEDOWN, Key::KEY_MUTE,
            Key::BTN_LEFT, Key::BTN_RIGHT, Key::BTN_MIDDLE,
            Key::BTN_SIDE, Key::BTN_EXTRA,
        ]))?
        .with_relative_axes(&evdev::AttributeSet::from_iter([
            evdev::RelativeAxisType::REL_X,
            evdev::RelativeAxisType::REL_Y,
            evdev::RelativeAxisType::REL_WHEEL,
            evdev::RelativeAxisType::REL_HWHEEL,
            evdev::RelativeAxisType::REL_WHEEL_HI_RES,
            evdev::RelativeAxisType::REL_HWHEEL_HI_RES,
        ]))?
        .with_absolute_axis(&UinputAbsSetup::new(
            AbsoluteAxisType::ABS_X,
            AbsInfo::new(0, 0, 65535, 0, 0, 1),
        ))?
        .with_absolute_axis(&UinputAbsSetup::new(
            AbsoluteAxisType::ABS_Y,
            AbsInfo::new(0, 0, 65535, 0, 0, 1),
        ))?
        .build()?;

    let output = Arc::new(Mutex::new(output));

    let mut streams = devices
        .into_iter()
        .filter_map(|mut device| {
            let name = device.name().unwrap_or("unknown").to_string();
            match device.grab() {
                Ok(_) => {
                    tracing::info!("Grabbed device: {}", name);
                    match device.into_event_stream() {
                        Ok(stream) => Some(stream),
                        Err(e) => {
                            tracing::warn!("Failed to stream {}: {}", name, e);
                            None
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to grab {}: {}", name, e);
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    tracing::info!("Grabbed {} devices", streams.len());

    let default_layer = config.layers.get(&config.default_layer)
        .ok_or_else(|| anyhow::anyhow!("Default layer '{}' not found", config.default_layer))?;

    let mut state = InputState::new(default_layer);

    loop {
        let event = {
            let futures = streams.iter_mut().map(|s| Box::pin(s.next()));
            let (result, _, _) = futures_util::future::select_all(futures).await;
            result
        };

        match event {
            Some(Ok(ev)) => {
                if ev.event_type() != EventType::KEY {
                    let mut out = output.lock().await;
                    out.emit(&[ev])?;
                    continue;
                }

                let key = Key::new(ev.code());
                let value = ev.value();

                // track held modifiers
                if is_modifier_key(key) {
                    if value == 1 {
                        state.held_modifiers.insert(key.code());
                    } else if value == 0 {
                        state.held_modifiers.remove(&key.code());
                    }
                    // always pass through modifier keys
                    let mut out = output.lock().await;
                    out.emit(&[ev])?;
                    continue;
                }

                // skip repeat events
                if value == 2 {
                    let mut out = output.lock().await;
                    out.emit(&[ev])?;
                    continue;
                }

                // update base layer when window changes
                if window_rx.has_changed()? {
                    let active_window = window_rx.borrow_and_update().clone();
                    update_base_layer(&config, &mut state, &active_window);
                }

                let modifier_index = compute_modifier_index(&state.held_modifiers);
                let layer = state.active_layer();

                match layer.lookup(key.code(), modifier_index) {
                    Some(action) => match action.clone() {
                        Action::Key(combo) => {
                            emit_combo(&output, &combo, value).await?;
                        }
                        Action::Layer { layer: layer_name, mode:layer_mode } => match layer_mode {
                            LayerMode::Shift => {
                                if value == 1 {
                                    if let Some(l) = config.layers.get(&layer_name) {
                                        tracing::info!("Shift layer on: {}", layer_name);
                                        state.shift_layer = Some(l);
                                    }
                                } else if value == 0 {
                                    tracing::info!("Shift layer off");
                                    state.shift_layer = None;
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
                        Action::Macro { mode:macro_mode, steps } => match macro_mode {
                            MacroMode::Once => {
                                if value == 1 {
                                    stop_macro(&mut state);
                                    let token = spawn_macro(output.clone(), steps, MacroMode::Once);
                                    state.macro_cancel = Some(token);
                                }
                            }
                            MacroMode::Hold => {
                                if value == 1 {
                                    stop_macro(&mut state);
                                    let token = spawn_macro(output.clone(), steps, MacroMode::Hold);
                                    state.macro_cancel = Some(token);
                                } else if value == 0 {
                                    stop_macro(&mut state);
                                }
                            }
                            MacroMode::Toggle => {
                                if value == 1 {
                                    if state.macro_cancel.is_some() {
                                        stop_macro(&mut state);
                                    } else {
                                        let token = spawn_macro(output.clone(), steps, MacroMode::Toggle);
                                        state.macro_cancel = Some(token);
                                    }
                                }
                            }
                        },
                    },
                    None => {
                        let mut out = output.lock().await;
                        out.emit(&[ev])?;
                    }
                }
            }
            Some(Err(e)) => {
                tracing::error!("Input error: {}", e);
            }
            None => {}
        }
    }
}