use crate::types::DrawCommand;

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
