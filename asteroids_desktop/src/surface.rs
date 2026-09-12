use std::sync::Arc;

use wgpu::{
    Adapter, CurrentSurfaceTexture, Device, Instance, InstanceDescriptor, PresentMode, Queue,
    RequestAdapterOptions, Surface, SurfaceColorSpace, SurfaceColorSpaces, SurfaceConfiguration,
    SurfaceTexture, TextureFormat, TextureUsages,
};
use winit::window::Window;

pub struct WindowTarget {
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    gpu_adapter: Adapter,

    device: Device,
    queue: Queue,

    last_hdr_headroom: f32,
}

impl WindowTarget {
    pub fn new(window: Arc<Window>) -> Self {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());

        let surface = instance
            .create_surface(Arc::clone(&window))
            .expect("failed to create surface");

        let gpu_adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no GPU adapter found");
        let (device, queue) = pollster::block_on(gpu_adapter.request_device(&Default::default()))
            .expect("failed to create GPU device");

        let capabilities = surface.get_capabilities(&gpu_adapter);

        let use_hdr = capabilities
            // This is not sRGB (and thus isn't gamma corrected). It's linear
            .color_spaces(TextureFormat::Rgba16Float)
            // Pair with linear sRGB output, supporting HDR
            .contains(SurfaceColorSpaces::EXTENDED_SRGB_LINEAR);

        let (format, color_space) = if use_hdr {
            (
                TextureFormat::Rgba16Float,
                SurfaceColorSpace::ExtendedSrgbLinear,
            )
        } else {
            // Prefer sRGB
            let format = capabilities
                .formats
                .iter()
                .copied()
                .find(|format| format.is_srgb())
                .unwrap_or(capabilities.formats[0]);

            // Auto resolves to plain sRGB for an 8 bit format
            (format, SurfaceColorSpace::Auto)
        };

        println!("surface {format:?} / {color_space:?} (hdr: {use_hdr})");

        let physical = window.inner_size();
        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space,
            width: physical.width.max(1),
            height: physical.height.max(1),
            present_mode: PresentMode::AutoNoVsync,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Self {
            surface,
            surface_config,
            gpu_adapter,
            device,
            queue,
            last_hdr_headroom: 0.0,
        }
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn format(&self) -> TextureFormat {
        self.surface_config.format
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    pub fn hdr_headroom(&mut self) -> f32 {
        let hdr_headroom = self
            .surface
            .display_hdr_info(&self.gpu_adapter)
            .tone_map_headroom()
            .unwrap_or(1.0);

        if hdr_headroom != self.last_hdr_headroom {
            self.last_hdr_headroom = hdr_headroom;
            println!("HDR headroom {hdr_headroom:.1}");
        }

        hdr_headroom
    }

    /// Gets the surface texture for use
    pub fn acquire(&mut self) -> Option<SurfaceTexture> {
        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => Some(frame),
            CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.surface_config);
                Some(frame)
            }
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                // Skip this frame
                self.surface.configure(&self.device, &self.surface_config);
                None
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => None,
            other => panic!("failed to acquire surface texture: {other:?}"),
        }
    }

    pub fn present(&self, frame: SurfaceTexture) {
        self.queue.present(frame);
    }
}
