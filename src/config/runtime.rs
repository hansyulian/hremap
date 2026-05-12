use crate::{
    config::{LayerMode, MacroMode, VolumeDirection},
    utils::compute_modifier_index::MODIFIER_COUNT,
};

use evdev::Key;
use std::collections::HashMap;
// ─── Resolved structs ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RuntimeKeyCombo {
    pub modifiers: Vec<Key>,
    pub key: Key,
}

#[derive(Debug, Clone)]
pub struct RuntimeMacroStep {
    pub combo: RuntimeKeyCombo,
    pub delay_ms: u64,
    pub up: bool,
}

#[derive(Debug, Clone)]
pub enum RuntimeAction {
    Key(RuntimeKeyCombo),
    RuntimeActionMacro {
        mode: MacroMode,
        steps: Vec<RuntimeMacroStep>,
    },
    RuntimeActionLayer {
        layer: String,
        mode: LayerMode,
    },
    RuntimeActionVolume {
        direction: VolumeDirection,
        amount: f32,
    },
    RuntimeActionAppVolume {
        direction: VolumeDirection,
        amount: f32,
    },
    RuntimeActionLaunch {
        command: String,
    },
}

// key code → [Option<Action>; 8] (indexed by modifier bitmask)
pub type RuntimeMappings = HashMap<u16, [Option<RuntimeAction>; MODIFIER_COUNT]>;

#[derive(Debug)]
pub struct RuntimeLayer {
    pub name: String,
    pub mappings: RuntimeMappings,
}
pub enum LookupResult<'a> {
    Exact(&'a RuntimeAction),
    Fallback(&'a RuntimeAction),
}

impl RuntimeLayer {
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
pub struct RuntimeConfig {
    pub layers: HashMap<String, RuntimeLayer>,
    pub profile_map: HashMap<String, String>,
    pub default_layer: String,
    pub device_names: Vec<String>,
}
