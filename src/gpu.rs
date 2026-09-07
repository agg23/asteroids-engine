use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, BlendState, Buffer,
    BufferDescriptor, BufferUsages, Color, ColorTargetState, ColorWrites, Device, Extent3d,
    FragmentState, Instance, InstanceDescriptor, LoadOp, MapMode, Operations, PollType,
    PrimitiveState, PrimitiveTopology, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, StoreOp, TexelCopyBufferInfo, TexelCopyBufferLayout,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    VertexBufferLayout, VertexState, VertexStepMode, vertex_attr_array,
};

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
    device: Device,
    queue: Queue,
    emu_input_pipeline: RenderPipeline,
    decay_pipeline: RenderPipeline,
    render_pipeline: RenderPipeline,

    active_texture_index: usize,

    decay_bind_group: [BindGroup; 2],
    render_bind_group: [BindGroup; 2],

    // GPU render target
    target: Texture,

    // Double buffered energy calculations
    phosphor_texture: [Texture; 2],
    phosphor_view: [TextureView; 2],

    uniform_buffer: Buffer,

    vertex_buffer: Buffer,
    // Buffer for reading texture on CPU for display
    // TODO: Remove and convert to GPU direct rendering
    readback_buffer: Buffer,

    size: u32,
    instances: Vec<BeamStepInstance>,
}

impl GpuRenderer {
    pub fn new(output_size: u32) -> Self {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let gpu_adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("no GPU adapter found");
        let (device, queue) = pollster::block_on(gpu_adapter.request_device(&Default::default()))
            .expect("failed to create GPU device");

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
                // This must match the texture def
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: Default::default(),
            cache: None,
        });

        let target = device.create_texture(&TextureDescriptor {
            label: Some("Render texture"),
            size: Extent3d {
                width: output_size,
                height: output_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            // Draw into it and read it out
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
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
                    // BindGroupEntry {
                    //     binding: 1,
                    //     resource: uniform_buffer.as_entire_binding(),
                    // },
                ],
            })
        });

        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Beam events"),
            size: (MAX_STEPS * size_of::<BeamStepInstance>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        assert_eq!((output_size * 4) % 256, 0, "Rows must be 256 byte aligned");
        let readback_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Readback"),
            size: (output_size * output_size * 4) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            device,
            queue,
            emu_input_pipeline,
            decay_pipeline,
            render_pipeline,
            active_texture_index: 0,
            decay_bind_group,
            render_bind_group,
            target,
            phosphor_texture,
            phosphor_view,
            uniform_buffer,
            vertex_buffer,
            readback_buffer,
            size: output_size,
            instances: Vec::new(),
        }
    }

    pub fn render(
        &mut self,
        commands: impl Iterator<Item = BeamStep>,
        next_frame_tick_count: u64,
        out: &mut [u32],
    ) {
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&SharedUniforms::new(next_frame_tick_count)),
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

        // Draw to our output textuer
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &self.target.create_view(&Default::default()),
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
            pass.set_bind_group(0, &self.render_bind_group[write_index], &[]);
            pass.draw(0..3, 0..1);

            // Drop pass
        }

        // After the render path, copy the texture to our output buffer
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &self.readback_buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    // RGBA
                    // TODO: This won't work with HDR
                    bytes_per_row: Some(self.size * 4),
                    rows_per_image: None,
                },
            },
            self.target.size(),
        );

        self.queue.submit([encoder.finish()]);

        // Flip the buffers
        self.active_texture_index = write_index;

        // Poll GPU until we have the result
        let slice = self.readback_buffer.slice(..);

        slice.map_async(MapMode::Read, |result| result.unwrap());

        self.device.poll(PollType::wait_indefinitely()).unwrap();

        {
            let pixels = slice.get_mapped_range().unwrap();

            // Rearrange pixels and copy to output
            for (px_out, px_in) in out.iter_mut().zip(pixels.chunks_exact(4)) {
                // RGBA bytes to minifb 0x00RRGGBB
                let [r, g, b, _a] = px_in.try_into().unwrap();
                *px_out = u32::from_be_bytes([0, r, g, b]);
            }

            // If you don't drop here before the unmap, WGPU will crash
        }

        // Remove buffer from VRAM
        self.readback_buffer.unmap();
    }
}
