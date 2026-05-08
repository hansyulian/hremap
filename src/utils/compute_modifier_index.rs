use evdev::Key;

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
