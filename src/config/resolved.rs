use crate::{
    config::{LayerMode, MacroMode, VolumeDirection},
    utils::compute_modifier_index::MODIFIER_COUNT,
};

use evdev::Key;
use std::collections::HashMap;
// ─── Resolved structs ─────────────────────────────────────────────────────────

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
