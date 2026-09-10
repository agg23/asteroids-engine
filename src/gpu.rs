use std::sync::Arc;

use wgpu::{
    Adapter, BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, BlendState, Buffer,
    BufferDescriptor, BufferUsages, Color, ColorTargetState, ColorWrites, CommandEncoder,
    CurrentSurfaceTexture, Device, Extent3d, FragmentState, Instance, InstanceDescriptor, LoadOp,
    Operations, PresentMode, PrimitiveState, PrimitiveTopology, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, StoreOp,
    Surface, SurfaceColorSpace, SurfaceColorSpaces, SurfaceConfiguration, SurfaceTexture, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    VertexBufferLayout, VertexState, VertexStepMode, vertex_attr_array,
};
use winit::window::Window;

use crate::{
    shader::{BeamStepInstance, RENDER_RESOLUTION, Shader, SharedUniforms},
    types::BeamStep,
};

// #[repr(C)]
// #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
// struct Vertex {
//     // CRT emitter space, [0, 1024)
//     position: [u16; 2],
//     // DVG 4 bit intensity
//     intensity: u32,
// }

// const MAX_LINES: usize = 8192;
const MAX_STEPS: usize = 32768;

pub struct GpuRenderer {
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    gpu_adapter: Adapter,

    device: Device,
    queue: Queue,
    emu_input_pipeline: RenderPipeline,
    decay_pipeline: RenderPipeline,
    render_pipeline: RenderPipeline,

    active_texture_index: usize,

    decay_bind_group: [BindGroup; 2],
    render_bind_group: [BindGroup; 2],

    // Double buffered energy calculations
    phosphor_texture: [Texture; 2],
    phosphor_view: [TextureView; 2],

    uniform_buffer: Buffer,

    vertex_buffer: Buffer,

    instances: Vec<BeamStepInstance>,

    last_hdr_headroom: f32,
}

impl GpuRenderer {
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

        // let shader = device.create_shader_module(include_wgsl!("shader/line.wgsl"));
        let beam_tracing_shader = Shader::new(&device, include_str!("shader/beam_tracing.wgsl"));
        let decay_shader = Shader::new(&device, include_str!("shader/decay.wgsl"));
        let output_shader = Shader::new(&device, include_str!("shader/output.wgsl"));

        let emu_input_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Emu input pipeline"),
            layout: None,
            vertex: VertexState {
                module: &beam_tracing_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VertexBufferLayout {
                    // Bytes for each line segment struct
                    array_stride: size_of::<BeamStepInstance>() as u64,
                    step_mode: VertexStepMode::Instance,
                    // Map fields onto WGSL @location(n) inputs
                    attributes: &vertex_attr_array![
                        // active_ticks
                        0 => Uint32,
                        // ticks_remaining_in_frame
                        1 => Uint32,
                        // dest
                        2 => Uint16x2,
                        // intensity
                        3 => Uint32,
                    ],
                })],
            },
            // For now, state that every 2 vectors form a line that we directly draw
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            fragment: Some(FragmentState {
                module: &beam_tracing_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                // This must match the texture def
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::R16Float,
                    blend: Some(BlendState::ADDITIVE),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: Default::default(),
            cache: None,
        });

        let decay_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Decay pipeline"),
            layout: None,
            vertex: VertexState {
                module: &decay_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            fragment: Some(FragmentState {
                module: &decay_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                // This must match the texture def
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::R16Float,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: Default::default(),
            cache: None,
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render pipeline"),
            layout: None,
            vertex: VertexState {
                module: &output_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            // For now, state that every 2 vectors form a line that we directly draw
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            fragment: Some(FragmentState {
                module: &output_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                // This must match the surface we present to
                targets: &[Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: Default::default(),
            cache: None,
        });

        let phosphor_texture = ["a", "b"].map(|label| {
            device.create_texture(&TextureDescriptor {
                label: Some(&format!("Phosphor energy {label}")),
                size: Extent3d {
                    width: RENDER_RESOLUTION as u32,
                    height: RENDER_RESOLUTION as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::R16Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });

        let phosphor_view = [
            phosphor_texture[0].create_view(&Default::default()),
            phosphor_texture[1].create_view(&Default::default()),
        ];

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Shared uniforms"),
            size: size_of::<SharedUniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Connect phosphor texture to decay shader
        let decay_bind_group = [0, 1].map(|i| {
            device.create_bind_group(&BindGroupDescriptor {
                label: Some("Decay bind group"),
                layout: &decay_pipeline.get_bind_group_layout(0),
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&phosphor_view[i]),
                    },
                    // TODO: Enable when used
                    // BindGroupEntry {
                    //     binding: 1,
                    //     resource: uniform_buffer.as_entire_binding(),
                    // },
                ],
            })
        });

        // Connect phosphor texture to render shader
        let render_bind_group = [0, 1].map(|i| {
            device.create_bind_group(&BindGroupDescriptor {
                label: Some("Render bind group"),
                layout: &render_pipeline.get_bind_group_layout(0),
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&phosphor_view[i]),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                ],
            })
        });

        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Beam events"),
            size: (MAX_STEPS * size_of::<BeamStepInstance>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            surface,
            surface_config,
            gpu_adapter,
            device,
            queue,
            emu_input_pipeline,
            decay_pipeline,
            render_pipeline,
            active_texture_index: 0,
            decay_bind_group,
            render_bind_group,
            phosphor_texture,
            phosphor_view,
            uniform_buffer,
            vertex_buffer,
            instances: Vec::new(),
            last_hdr_headroom: 0.0,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Gets the surface texture for use
    fn acquire_surface(&mut self) -> Option<SurfaceTexture> {
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

    fn output_pass(&self, encoder: &mut CommandEncoder, view: &TextureView, bind_index: usize) {
        // This is broken out so we can rerun it without the rest of the pipeline
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Render"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    // Clear texture
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        });

        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.render_bind_group[bind_index], &[]);
        pass.draw(0..3, 0..1);
    }

    /// Rerender the last known phosphor state
    pub fn present_last(&mut self) {
        let Some(frame) = self.acquire_surface() else {
            return;
        };

        let frame_view = frame.texture.create_view(&Default::default());

        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.output_pass(&mut encoder, &frame_view, self.active_texture_index);

        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }

    pub fn render(&mut self, commands: impl Iterator<Item = BeamStep>, next_frame_tick_count: u64) {
        let Some(frame) = self.acquire_surface() else {
            return;
        };
        let frame_view = frame.texture.create_view(&Default::default());

        let hdr_headroom = self
            .surface
            .display_hdr_info(&self.gpu_adapter)
            .tone_map_headroom()
            .unwrap_or(1.0);

        if hdr_headroom != self.last_hdr_headroom {
            self.last_hdr_headroom = hdr_headroom;
            println!("HDR headroom {hdr_headroom:.1}");
        }

        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&SharedUniforms::new(hdr_headroom)),
        );

        self.instances.clear();

        self.instances
            .extend(commands.map(|command| BeamStepInstance::new(command, next_frame_tick_count)));

        // Upload
        self.queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.instances),
        );

        let read_index = self.active_texture_index;
        let write_index = 1 - read_index;

        // Build encoder and submit GPU task
        let mut encoder = self.device.create_command_encoder(&Default::default());

        // Run decay as the first part of the frame, rather than as the last (right before display), as we don't want
        // to decay the newly deposited energy
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Decay"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &self.phosphor_view[write_index],
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        // Keep what we produce from the shader
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.decay_pipeline);
            pass.set_bind_group(0, &self.decay_bind_group[read_index], &[]);
            pass.draw(0..3, 0..1);

            // Drop pass
        }

        // Apply commands from the emulator, depositing the energy
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Emu data"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &self.phosphor_view[write_index],
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        // Keep what decay just wrote
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.emu_input_pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            // Run across all instances with six vertices (a quad) per instance
            pass.draw(0..6, 0..self.instances.len() as u32);

            // Drop pass
        }

        self.output_pass(&mut encoder, &frame_view, write_index);

        self.queue.submit([encoder.finish()]);

        // Flip the buffers
        self.active_texture_index = write_index;

        self.queue.present(frame);
    }
}
