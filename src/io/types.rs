use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::global::SharedModifiers;
use crate::config::{Action, ResolvedLayer};

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
