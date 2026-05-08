use crate::utils::is_modifier_key;

use super::raw::{RawAction, RawConfig, RawLayer};
use super::resolved::{Action, Config, KeyCombo, LayerMappings, MacroStep, ResolvedLayer};
use super::utils::parse_key;
use anyhow::{bail, Result};
use evdev::Key;
use std::collections::HashMap;

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
