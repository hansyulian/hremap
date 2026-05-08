use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
