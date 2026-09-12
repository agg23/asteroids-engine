#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GamepadInputs {
    pub p1_start: bool,
    pub p2_start: bool,

    pub rotate_left: bool,
    pub rotate_right: bool,

    pub thrust: bool,
    pub fire: bool,

    pub hyperspace: bool,
}
