use std::collections::HashSet;

use asteroids::GamepadInputs;
use winit::keyboard::KeyCode;

pub struct KeyState {
    held: HashSet<KeyCode>,
}

impl KeyState {
    pub fn new() -> Self {
        Self {
            held: HashSet::new(),
        }
    }

    pub fn set(&mut self, key: KeyCode, pressed: bool) {
        if pressed {
            self.held.insert(key);
        } else {
            self.held.remove(&key);
        }
    }

    pub fn clear(&mut self) {
        self.held.clear();
    }

    pub fn is_key_down(&self, key: KeyCode) -> bool {
        self.held.contains(&key)
    }

    pub fn current_gamepad(&self) -> GamepadInputs {
        GamepadInputs {
            thrust: self.is_key_down(KeyCode::KeyW),
            rotate_left: self.is_key_down(KeyCode::KeyA),
            rotate_right: self.is_key_down(KeyCode::KeyD),
            fire: self.is_key_down(KeyCode::Space),
            hyperspace: self.is_key_down(KeyCode::ShiftLeft),
            p1_start: self.is_key_down(KeyCode::Digit1),
            p2_start: self.is_key_down(KeyCode::Digit2),
        }
    }
}
