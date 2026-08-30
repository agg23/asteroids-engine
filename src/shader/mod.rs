use crate::types::{BeamStep, DrawCommand};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineInstance {
    // DVG 12-bit coordinates
    pub start: [u16; 2],
    pub dest: [u16; 2],

    // 4 bit
    pub intensity: u32,

    // Bitfield
    // [0]: delta X negative
    // [1]: delta Y negative
    pub flags: u32,
}

impl From<DrawCommand> for LineInstance {
    fn from(value: DrawCommand) -> Self {
        let mut flags = 0;

        if value.moving_negative_x {
            flags |= 0x1;
        }

        if value.moving_negative_y {
            flags |= 0x2;
        }

        LineInstance {
            start: [value.start_x, value.start_y],
            dest: [value.dest_x, value.dest_y],
            intensity: value.intensity as u32,
            flags,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BeamStepInstance {
    // pub tick: u64,
    pub active_ticks: u32,

    // DVG 12-bit coordinates
    pub dest: [u16; 2],

    // 4 bit
    pub intensity: u32,
    // pub padding: u32,
}

impl From<BeamStep> for BeamStepInstance {
    fn from(value: BeamStep) -> Self {
        Self {
            // tick: value.tick,
            active_ticks: value.active_ticks,
            dest: [value.x, value.y],
            intensity: value.intensity as u32,
            // padding: 0,
        }
    }
}
