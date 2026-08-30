use wgpu::{
    Buffer, BufferDescriptor, BufferUsages, Color, ColorTargetState, ColorWrites, Device, Extent3d,
    FragmentState, Instance, InstanceDescriptor, LoadOp, MapMode, Operations, PollType,
    PrimitiveState, PrimitiveTopology, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, StoreOp, TexelCopyBufferInfo, TexelCopyBufferLayout,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, VertexBufferLayout,
    VertexState, VertexStepMode, include_wgsl, vertex_attr_array,
};

use crate::{shader::LineInstance, types::DrawCommand};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    // CRT emitter space, [0, 1024)
    position: [u16; 2],
    // DVG 4 bit intensity
    intensity: u32,
}

const MAX_LINES: usize = 8192;

pub struct GpuRenderer {
    device: Device,
    queue: Queue,
    pipeline: RenderPipeline,

    // GPU render target
    target: Texture,

    vertex_buffer: Buffer,
    // Buffer for reading texture on CPU for display
    // TODO: Remove and convert to GPU direct rendering
    readback_buffer: Buffer,

    size: u32,
    instances: Vec<LineInstance>,
}

impl GpuRenderer {
    pub fn new(size: u32) -> Self {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let gpu_adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("no GPU adapter found");
        let (device, queue) = pollster::block_on(gpu_adapter.request_device(&Default::default()))
            .expect("failed to create GPU device");

        let shader = device.create_shader_module(include_wgsl!("shader/line.wgsl"));

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Line pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VertexBufferLayout {
                    // Bytes for each line segment struct
                    array_stride: size_of::<LineInstance>() as u64,
                    step_mode: VertexStepMode::Instance,
                    // Map fields onto WGSL @location(n) inputs
                    attributes: &vertex_attr_array![
                        // start
                        0 => Uint16x2,
                        // dest
                        1 => Uint16x2,
                        // intensity
                        2 => Uint32,
                        // flags
                        3 => Uint32
                    ],
                })],
            },
            // For now, state that every 2 vectors form a line that we directly draw
            primitive: PrimitiveState {
                topology: PrimitiveTopology::LineList,
                ..Default::default()
            },
            fragment: Some(FragmentState {
                module: &shader,
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
                width: size,
                height: size,
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

        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Vertices"),
            size: (MAX_LINES * 2 * size_of::<Vertex>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        assert_eq!((size * 4) % 256, 0, "Rows must be 256 byte aligned");
        let readback_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Readback"),
            size: (size * size * 4) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            device,
            queue,
            pipeline,
            target,
            vertex_buffer,
            readback_buffer,
            size,
            instances: Vec::new(),
        }
    }

    pub fn render(&mut self, commands: impl Iterator<Item = DrawCommand>, out: &mut [u32]) {
        self.instances.clear();

        self.instances
            .extend(commands.map(|c| -> LineInstance { c.into() }));

        // Upload
        self.queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.instances),
        );

        // Build encoder and submit GPU task
        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Actual lines"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &self.target.create_view(&Default::default()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        // Clear texture
                        load: LoadOp::Clear(Color::BLACK),
                        // Keep what we produce from the shader
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            // Run across all instances with two vertices per instance
            pass.draw(0..2, 0..self.instances.len() as u32);

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
