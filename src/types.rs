#[derive(Debug)]
pub struct DrawCommand {
    pub start_tick: u64,
    /// The duration of the draw command
    pub duration_ticks: u16,

    // Positions are global, [0, 1024)
    // 12 bits
    pub start_x: u16,
    pub start_y: u16,

    pub dest_x: u16,
    pub dest_y: u16,

    // 4 bit
    pub intensity: u8,
}
