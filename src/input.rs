use minifb::{Key, Window};

#[derive(Clone)]
pub struct GamepadInputs {
    pub p1_start: bool,
    pub p2_start: bool,

    pub rotate_left: bool,
    pub rotate_right: bool,

    pub thrust: bool,
    pub fire: bool,

    pub hyperspace: bool,
}

impl GamepadInputs {
    pub fn new() -> Self {
        Self {
            p1_start: false,
            p2_start: false,

            rotate_left: false,
            rotate_right: false,

            thrust: false,
            fire: false,

            hyperspace: false,
        }
    }

    pub fn read_keyboard(window: &Window) -> Self {
        Self {
            thrust: window.is_key_down(Key::W),
            rotate_left: window.is_key_down(Key::A),
            rotate_right: window.is_key_down(Key::D),
            fire: window.is_key_down(Key::Space),
            hyperspace: window.is_key_down(Key::LeftShift),
            p1_start: window.is_key_down(Key::Key1),
            p2_start: window.is_key_down(Key::Key2),
        }
    }
}
