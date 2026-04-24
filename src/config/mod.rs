use anyhow::{bail, Result};
use evdev::Key;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Raw config structs ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct RawConfig {
    pub device_names: Vec<String>,
    pub default_layer: String,
    pub layers: HashMap<String, RawLayer>,
    #[serde(default)]
    pub profiles: Vec<RawProfile>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawLayer {
    pub parent: Option<String>,
    #[serde(default)]
    pub mappings: Vec<RawMapping>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawMapping {
    pub trigger: Vec<String>,
    pub action: RawAction,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawAction {
    Key {
        keys: Vec<String>,
    },
    Macro {
        mode: MacroMode,
        steps: Vec<RawMacroStep>,
    },
    Layer {
        layer: String,
        mode: LayerMode,
    },
    Volume {
        direction: VolumeDirection,
        #[serde(default = "default_volume_amount")]
        amount: f32,
    },
    AppVolume {
        direction: VolumeDirection,
        #[serde(default = "default_volume_amount")]
        amount: f32,
    },
    Launch {
        command: String,
    },
}

fn default_volume_amount() -> f32 {
    0.1
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum VolumeDirection {
    Up,
    Down,
    Mute,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawMacroStep {
    pub keys: Vec<String>,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub up: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MacroMode {
    Once,
    Hold,
    Toggle,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LayerMode {
    Shift,
    Toggle,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawProfile {
    pub wm_classes: Vec<String>,
    pub layer: String,
}

// ─── Resolved structs ─────────────────────────────────────────────────────────

pub const MODIFIER_COUNT: usize = 8; // 3 bits: SHIFT | CTRL | ALT

pub fn compute_modifier_index(held: &std::collections::HashSet<u16>) -> usize {
    let mut index = 0usize;
    if held.contains(&Key::KEY_LEFTSHIFT.code()) || held.contains(&Key::KEY_RIGHTSHIFT.code()) {
        index |= 1;
    }
    if held.contains(&Key::KEY_LEFTCTRL.code()) || held.contains(&Key::KEY_RIGHTCTRL.code()) {
        index |= 2;
    }
    if held.contains(&Key::KEY_LEFTALT.code()) || held.contains(&Key::KEY_RIGHTALT.code()) {
        index |= 4;
    }
    index
}

pub fn is_modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::KEY_LEFTCTRL
            | Key::KEY_RIGHTCTRL
            | Key::KEY_LEFTSHIFT
            | Key::KEY_RIGHTSHIFT
            | Key::KEY_LEFTALT
            | Key::KEY_RIGHTALT
            | Key::KEY_LEFTMETA
            | Key::KEY_RIGHTMETA
    )
}

#[derive(Debug, Clone)]
pub struct KeyCombo {
    pub modifiers: Vec<Key>,
    pub key: Key,
}

#[derive(Debug, Clone)]
pub struct MacroStep {
    pub combo: KeyCombo,
    pub delay_ms: u64,
    pub up: bool,
}

#[derive(Debug, Clone)]
pub enum Action {
    Key(KeyCombo),
    Macro {
        mode: MacroMode,
        steps: Vec<MacroStep>,
    },
    Layer {
        layer: String,
        mode: LayerMode,
    },
    Volume {
        direction: VolumeDirection,
        amount: f32,
    },
    AppVolume {
        direction: VolumeDirection,
        amount: f32,
    },
    Launch {
        command: String,
    },
}

// key code → [Option<Action>; 8] (indexed by modifier bitmask)
pub type LayerMappings = HashMap<u16, [Option<Action>; MODIFIER_COUNT]>;

#[derive(Debug)]
pub struct ResolvedLayer {
    pub name: String,
    pub mappings: LayerMappings,
}

impl ResolvedLayer {
    pub fn lookup(&self, key_code: u16, modifier_index: usize) -> Option<&Action> {
        self.mappings
            .get(&key_code)
            .and_then(|arr| arr[modifier_index].as_ref())
    }
}

#[derive(Debug)]
pub struct Config {
    pub layers: HashMap<String, ResolvedLayer>,
    pub profile_map: HashMap<String, String>,
    pub default_layer: String,
    pub device_names: Vec<String>,
}

// ─── Key parsing ──────────────────────────────────────────────────────────────

pub fn parse_key(name: &str) -> Result<Key> {
    let key = match name.to_uppercase().as_str() {
        "A" => Key::KEY_A,
        "B" => Key::KEY_B,
        "C" => Key::KEY_C,
        "D" => Key::KEY_D,
        "E" => Key::KEY_E,
        "F" => Key::KEY_F,
        "G" => Key::KEY_G,
        "H" => Key::KEY_H,
        "I" => Key::KEY_I,
        "J" => Key::KEY_J,
        "K" => Key::KEY_K,
        "L" => Key::KEY_L,
        "M" => Key::KEY_M,
        "N" => Key::KEY_N,
        "O" => Key::KEY_O,
        "P" => Key::KEY_P,
        "Q" => Key::KEY_Q,
        "R" => Key::KEY_R,
        "S" => Key::KEY_S,
        "T" => Key::KEY_T,
        "U" => Key::KEY_U,
        "V" => Key::KEY_V,
        "W" => Key::KEY_W,
        "X" => Key::KEY_X,
        "Y" => Key::KEY_Y,
        "Z" => Key::KEY_Z,
        "0" => Key::KEY_0,
        "1" => Key::KEY_1,
        "2" => Key::KEY_2,
        "3" => Key::KEY_3,
        "4" => Key::KEY_4,
        "5" => Key::KEY_5,
        "6" => Key::KEY_6,
        "7" => Key::KEY_7,
        "8" => Key::KEY_8,
        "9" => Key::KEY_9,
        "F1" => Key::KEY_F1,
        "F2" => Key::KEY_F2,
        "F3" => Key::KEY_F3,
        "F4" => Key::KEY_F4,
        "F5" => Key::KEY_F5,
        "F6" => Key::KEY_F6,
        "F7" => Key::KEY_F7,
        "F8" => Key::KEY_F8,
        "F9" => Key::KEY_F9,
        "F10" => Key::KEY_F10,
        "F11" => Key::KEY_F11,
        "F12" => Key::KEY_F12,
        "F13" => Key::KEY_F13,
        "F14" => Key::KEY_F14,
        "F15" => Key::KEY_F15,
        "F16" => Key::KEY_F16,
        "F17" => Key::KEY_F17,
        "F18" => Key::KEY_F18,
        "F19" => Key::KEY_F19,
        "F20" => Key::KEY_F20,
        "F21" => Key::KEY_F21,
        "F22" => Key::KEY_F22,
        "F23" => Key::KEY_F23,
        "F24" => Key::KEY_F24,
        "CTRL" | "CONTROL" | "LCTRL" => Key::KEY_LEFTCTRL,
        "RCTRL" => Key::KEY_RIGHTCTRL,
        "SHIFT" | "LSHIFT" => Key::KEY_LEFTSHIFT,
        "RSHIFT" => Key::KEY_RIGHTSHIFT,
        "ALT" | "LALT" => Key::KEY_LEFTALT,
        "RALT" => Key::KEY_RIGHTALT,
        "SUPER" | "WIN" | "META" => Key::KEY_LEFTMETA,
        "ENTER" | "RETURN" => Key::KEY_ENTER,
        "ESC" | "ESCAPE" => Key::KEY_ESC,
        "BACKSPACE" => Key::KEY_BACKSPACE,
        "TAB" => Key::KEY_TAB,
        "SPACE" => Key::KEY_SPACE,
        "DELETE" | "DEL" => Key::KEY_DELETE,
        "HOME" => Key::KEY_HOME,
        "END" => Key::KEY_END,
        "PAGEUP" => Key::KEY_PAGEUP,
        "PAGEDOWN" => Key::KEY_PAGEDOWN,
        "UP" => Key::KEY_UP,
        "DOWN" => Key::KEY_DOWN,
        "LEFT" => Key::KEY_LEFT,
        "RIGHT" => Key::KEY_RIGHT,
        "CAPSLOCK" => Key::KEY_CAPSLOCK,
        "PAUSE" => Key::KEY_PAUSE,
        "SCROLLLOCK" => Key::KEY_SCROLLLOCK,
        "INSERT" => Key::KEY_INSERT,
        "MINUS" => Key::KEY_MINUS,
        "EQUAL" => Key::KEY_EQUAL,
        "LEFTBRACKET" => Key::KEY_LEFTBRACE,
        "RIGHTBRACKET" => Key::KEY_RIGHTBRACE,
        "BACKSLASH" => Key::KEY_BACKSLASH,
        "SEMICOLON" => Key::KEY_SEMICOLON,
        "APOSTROPHE" => Key::KEY_APOSTROPHE,
        "GRAVE" | "BACKTICK" => Key::KEY_GRAVE,
        "COMMA" => Key::KEY_COMMA,
        "PERIOD" => Key::KEY_DOT,
        "SLASH" => Key::KEY_SLASH,
        "PLAY_PAUSE" => Key::KEY_PLAYPAUSE,
        "NEXT_TRACK" => Key::KEY_NEXTSONG,
        "PREV_TRACK" => Key::KEY_PREVIOUSSONG,
        "VOLUME_UP" => Key::KEY_VOLUMEUP,
        "VOLUME_DOWN" => Key::KEY_VOLUMEDOWN,
        "MUTE" => Key::KEY_MUTE,
        "BTN_LEFT" | "LEFT_CLICK" => Key::BTN_LEFT,
        "BTN_RIGHT" | "RIGHT_CLICK" => Key::BTN_RIGHT,
        "BTN_MIDDLE" | "MIDDLE_CLICK" => Key::BTN_MIDDLE,
        "BTN_SIDE" | "MBACK" => Key::BTN_SIDE,
        "BTN_EXTRA" | "MFORWARD" => Key::BTN_EXTRA,
        "NUMPAD_0" | "KP_0" => Key::KEY_KP0,
        "NUMPAD_1" | "KP_1" => Key::KEY_KP1,
        "NUMPAD_2" | "KP_2" => Key::KEY_KP2,
        "NUMPAD_3" | "KP_3" => Key::KEY_KP3,
        "NUMPAD_4" | "KP_4" => Key::KEY_KP4,
        "NUMPAD_5" | "KP_5" => Key::KEY_KP5,
        "NUMPAD_6" | "KP_6" => Key::KEY_KP6,
        "NUMPAD_7" | "KP_7" => Key::KEY_KP7,
        "NUMPAD_8" | "KP_8" => Key::KEY_KP8,
        "NUMPAD_9" | "KP_9" => Key::KEY_KP9,
        "NUMPAD_MULTIPLY" => Key::KEY_KPASTERISK,
        "NUMPAD_PLUS" => Key::KEY_KPPLUS,
        "NUMPAD_MINUS" => Key::KEY_KPMINUS,
        "NUMPAD_DOT" => Key::KEY_KPDOT,
        "NUMPAD_ENTER" => Key::KEY_KPENTER,
        "NUMPAD_SLASH" => Key::KEY_KPSLASH,
        _ => bail!("Unknown key name: {}", name),
    };
    Ok(key)
}

fn parse_combo(keys: &[String]) -> Result<KeyCombo> {
    if keys.is_empty() {
        bail!("Empty key combo");
    }
    let mut modifiers = vec![];
    let mut main_key = None;
    for name in keys {
        let key = parse_key(name)?;
        if is_modifier_key(key) {
            modifiers.push(key);
        } else {
            if main_key.is_some() {
                bail!("Multiple non-modifier keys in combo: {:?}", keys);
            }
            main_key = Some(key);
        }
    }
    let key = match main_key {
        Some(k) => k,
        None => bail!("No main key in combo: {:?}", keys),
    };
    Ok(KeyCombo { modifiers, key })
}

fn parse_trigger(keys: &[String]) -> Result<(u16, usize)> {
    if keys.is_empty() {
        bail!("Empty trigger");
    }

    let mut modifier_index = 0usize;
    let mut main_key = None;

    for name in keys {
        let key = parse_key(name)?;
        match key {
            Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT => modifier_index |= 1,
            Key::KEY_LEFTCTRL | Key::KEY_RIGHTCTRL => modifier_index |= 2,
            Key::KEY_LEFTALT | Key::KEY_RIGHTALT => modifier_index |= 4,
            other => {
                if main_key.is_some() {
                    bail!("Multiple non-modifier keys in trigger: {:?}", keys);
                }
                main_key = Some(other);
            }
        }
    }

    let key = match main_key {
        Some(k) => k,
        None => bail!("Modifier-only triggers are not supported: {:?}", keys),
    };

    Ok((key.code(), modifier_index))
}

fn resolve_action(raw: &RawAction) -> Result<Action> {
    match raw {
        RawAction::Key { keys } => Ok(Action::Key(parse_combo(keys)?)),
        RawAction::Macro { mode, steps } => {
            let resolved_steps = steps
                .iter()
                .map(|s| {
                    Ok(MacroStep {
                        combo: parse_combo(&s.keys)?,
                        delay_ms: s.delay_ms,
                        up: s.up,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(Action::Macro {
                mode: mode.clone(),
                steps: resolved_steps,
            })
        }
        RawAction::Layer { layer, mode } => Ok(Action::Layer {
            layer: layer.clone(),
            mode: mode.clone(),
        }),
        RawAction::Volume { direction, amount } => Ok(Action::Volume {
            direction: direction.clone(),
            amount: *amount,
        }),
        RawAction::AppVolume { direction, amount } => Ok(Action::AppVolume {
            direction: direction.clone(),
            amount: *amount,
        }),
        RawAction::Launch { command } => Ok(Action::Launch {
            command: command.clone(),
        }),
    }
}

fn resolve_layer(
    name: &str,
    raw_layers: &HashMap<String, RawLayer>,
    cache: &mut HashMap<String, LayerMappings>,
) -> Result<LayerMappings> {
    if let Some(cached) = cache.get(name) {
        return Ok(cached.clone());
    }

    let raw = match raw_layers.get(name) {
        Some(l) => l,
        None => bail!("Layer not found: {}", name),
    };

    // start with parent mappings
    let mut mappings: LayerMappings = if let Some(parent) = &raw.parent {
        resolve_layer(parent, raw_layers, cache)?
    } else {
        HashMap::new()
    };

    // child mappings override parent
    for raw_mapping in &raw.mappings {
        let (key_code, modifier_index) = parse_trigger(&raw_mapping.trigger)?;
        let action = resolve_action(&raw_mapping.action)?;

        let slot = mappings
            .entry(key_code)
            .or_insert_with(|| std::array::from_fn(|_| None));
        slot[modifier_index] = Some(action.clone());

        // Auto-expand Action::Key with no modifier trigger into all modifier combinations
        // that aren't already explicitly mapped, prepending held modifiers to the output.
        if modifier_index == 0 {
            if let Action::Key(ref combo) = action {
                for mod_bits in 1..MODIFIER_COUNT {
                    if slot[mod_bits].is_none() {
                        // build the extra modifiers from the bitmask
                        let mut extra: Vec<Key> = vec![];
                        if mod_bits & 1 != 0 {
                            extra.push(Key::KEY_LEFTSHIFT);
                        }
                        if mod_bits & 2 != 0 {
                            extra.push(Key::KEY_LEFTCTRL);
                        }
                        if mod_bits & 4 != 0 {
                            extra.push(Key::KEY_LEFTALT);
                        }

                        // prepend extra mods, then the combo's own modifiers, then the key
                        let mut new_modifiers = extra;
                        new_modifiers.extend_from_slice(&combo.modifiers);

                        slot[mod_bits] = Some(Action::Key(KeyCombo {
                            modifiers: new_modifiers,
                            key: combo.key,
                        }));
                    }
                }
            }
        }
    }

    cache.insert(name.to_string(), mappings.clone());
    Ok(mappings)
}

pub fn load(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)?;
    let raw: RawConfig = serde_yaml::from_str(&content)?;

    let mut cache: HashMap<String, LayerMappings> = HashMap::new();
    let mut layers: HashMap<String, ResolvedLayer> = HashMap::new();

    for name in raw.layers.keys() {
        let mappings = resolve_layer(name, &raw.layers, &mut cache)?;
        layers.insert(
            name.clone(),
            ResolvedLayer {
                name: name.clone(),
                mappings,
            },
        );
    }

    let mut profile_map: HashMap<String, String> = HashMap::new();
    for profile in &raw.profiles {
        for wm_class in &profile.wm_classes {
            profile_map.insert(wm_class.clone(), profile.layer.clone());
        }
    }

    Ok(Config {
        layers,
        profile_map,
        default_layer: raw.default_layer,
        device_names: raw.device_names,
    })
}
