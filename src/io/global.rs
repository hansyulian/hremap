use evdev::InputEvent;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

pub type SharedModifiers = Arc<RwLock<HashSet<u16>>>;
pub type MacroTx = mpsc::UnboundedSender<InputEvent>;
