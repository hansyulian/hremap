use anyhow::{bail, Result};
use evdev::Key;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
mod utils;

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
pub enum LookupResult<'a> {
    Exact(&'a Action),
    Fallback(&'a Action),
}

impl ResolvedLayer {
    pub fn lookup(&self, key_code: u16, modifier_index: usize) -> Option<LookupResult> {
        self.mappings.get(&key_code).and_then(|arr| {
            if let Some(action) = arr[modifier_index].as_ref() {
                Some(LookupResult::Exact(action))
            } else if modifier_index != 0 {
                arr[0].as_ref().map(LookupResult::Fallback)
            } else {
                None
            }
        })
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

fn parse_combo(keys: &[String]) -> Result<KeyCombo> {
    if keys.is_empty() {
        bail!("Empty key combo");
    }
    let mut modifiers = vec![];
    let mut main_key = None;
    for name in keys {
        let key = utils::parse_key(name)?;
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
        let key = utils::parse_key(name)?;
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
