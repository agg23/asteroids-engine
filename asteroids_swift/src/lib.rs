use std::sync::atomic::{AtomicU32, Ordering};

use asteroids::{GamepadInputs, GpuRenderer, Machine, ROM, shader::EMULATION_OUTPUT_RESOLUTION};
use wgpu::{
    Backends, Device, DeviceDescriptor, Instance, InstanceDescriptor, Queue, RequestAdapterOptions,
    Texture, TextureFormat, TextureView,
};

use crate::ffi::{FFIGamepadInputs, InitError};

mod metal;

pub const OUTPUT_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

#[swift_bridge::bridge]
mod ffi {
    #[swift_bridge(swift_repr = "struct")]
    struct FFIGamepadInputs {
        p1_start: bool,
        p2_start: bool,

        rotate_left: bool,
        rotate_right: bool,

        thrust: bool,
        fire: bool,

        hyperspace: bool,
    }

    extern "Rust" {
        fn output_resolution() -> u32;

        // Max HDR brightness as multiple of SDR 1.0
        fn set_hdr_headroom(headroom: f32);
    }

    // For some reason swift-bridge doesn't support string as an init error type, so we wrap it in a newtype
    #[swift_bridge(swift_repr = "struct")]
    struct InitError(String);

    extern "Rust" {
        type Asteroids;

        #[swift_bridge(init)]
        fn new(rom_data: &[u8], io_surfaces: &[usize]) -> Result<Asteroids, InitError>;

        fn render_frame(&mut self, inputs: FFIGamepadInputs) -> usize;

        // Redraw the current phosphor state without advancing the simulation
        fn redraw(&mut self) -> usize;
    }
}

fn output_resolution() -> u32 {
    EMULATION_OUTPUT_RESOLUTION as u32
}

/// Shared with the emulator thread, so it is read fresh on every frame rather than cached
static HDR_HEADROOM: AtomicU32 = AtomicU32::new(1.0f32.to_bits());

fn set_hdr_headroom(headroom: f32) {
    HDR_HEADROOM.store(headroom.max(1.0).to_bits(), Ordering::Relaxed);
}

fn hdr_headroom() -> f32 {
    f32::from_bits(HDR_HEADROOM.load(Ordering::Relaxed))
}

struct OutputSurface {
    // Keep the texture alive
    _texture: Texture,
    view: TextureView,
}

pub struct Asteroids {
    machine: Machine,
    renderer: GpuRenderer,

    device: Device,

    surfaces: Vec<OutputSurface>,
    next_surface: usize,
}

impl From<String> for InitError {
    fn from(value: String) -> Self {
        InitError(value)
    }
}

impl Asteroids {
    fn new(rom_data: &[u8], io_surfaces: &[usize]) -> Result<Asteroids, InitError> {
        if io_surfaces.is_empty() {
            return Err(InitError("No output surfaces provided".into()));
        }

        let rom = ROM::load_mame_bytes(rom_data)
            .map_err(|error| format!("Could not load ROM: {error}").to_string())?;

        let (device, queue) = create_metal_device()?;

        let mut surfaces = Vec::with_capacity(io_surfaces.len());
        for (index, address) in io_surfaces.iter().enumerate() {
            let texture =
                unsafe { metal::texture_from_io_surface(&device, *address, output_resolution()) }
                    .map_err(|error| format!("Output surface {index}: {error}").to_string())?;

            surfaces.push(OutputSurface {
                view: texture.create_view(&Default::default()),
                _texture: texture,
            });
        }

        let renderer = GpuRenderer::new(&device, &queue, OUTPUT_TEXTURE_FORMAT);

        Ok(Asteroids {
            machine: Machine::new(rom),
            renderer,
            device,
            surfaces,
            next_surface: 0,
        })
    }

    fn render_frame(&mut self, inputs: FFIGamepadInputs) -> usize {
        let frame = self.machine.run_until_frame_now(inputs.into());

        let index = self.next_surface;
        self.next_surface = (index + 1) % self.surfaces.len();

        self.renderer.render(
            &self.surfaces[index].view,
            frame.commands,
            frame.frame_tick_count,
            hdr_headroom(),
        );

        index
    }

    fn redraw(&mut self) -> usize {
        let index = self.next_surface;
        self.next_surface = (index + 1) % self.surfaces.len();

        self.renderer.present_last(&self.surfaces[index].view);

        index
    }
}

impl From<FFIGamepadInputs> for GamepadInputs {
    fn from(value: FFIGamepadInputs) -> Self {
        GamepadInputs {
            p1_start: value.p1_start,
            p2_start: value.p2_start,

            rotate_left: value.rotate_left,
            rotate_right: value.rotate_right,

            thrust: value.thrust,
            fire: value.fire,

            hyperspace: value.hyperspace,
        }
    }
}

fn create_metal_device() -> Result<(Device, Queue), String> {
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::METAL;

    let instance = Instance::new(descriptor);

    let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions::default()))
        .map_err(|error| format!("No Metal adapter: {error}").to_string())?;

    let descriptor = DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    };

    pollster::block_on(adapter.request_device(&descriptor))
        .map_err(|error| format!("No Metal device: {error}").into())
}
