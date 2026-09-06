use wgpu::{Device, ShaderModule, ShaderModuleDescriptor, ShaderSource};

use crate::types::{BeamStep, DrawCommand};

pub const DISPLAY_RESOLUTION: usize = 1024;
pub const SUPERSAMPLE_MULTIPLIER: usize = 4;
pub const RENDER_RESOLUTION: usize = DISPLAY_RESOLUTION * SUPERSAMPLE_MULTIPLIER;

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

pub struct Shader;

impl Shader {
    pub fn new(device: &Device, contents: &str) -> ShaderModule {
        let injected_constants = format!(
            "const DISPLAY_RESOLUTION: f32 = {DISPLAY_RESOLUTION}.0;
            const SUPERSAMPLE_MULTIPLIER: i32 = {SUPERSAMPLE_MULTIPLIER};"
        );

        // WGSL doesn't care about declaration order, and prepending would mess up line numbers
        let contents = format!("{contents}\n{injected_constants}");

        device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(contents.into()),
        })
    }
}
