use serde::Deserialize;

pub mod gnome;
pub mod kde;

#[derive(Debug, Clone, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub wm_class: String,
    pub wm_class_instance: String,
    pub pid: u32,
}
