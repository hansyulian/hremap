use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub wm_class: String,
    pub wm_class_instance: String,
    pub pid: u32,
}
