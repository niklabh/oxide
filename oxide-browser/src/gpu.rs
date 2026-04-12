//! WebGPU-style GPU resource management for guest wasm modules.
//!
//! This module implements a sandboxed GPU API inspired by the WebGPU specification.
//! Guest modules can create buffers, textures, shaders, and pipelines, then submit
//! draw calls or compute dispatches — all mediated through the host capability system.
//!
//! Resources are identified by opaque `u32` handles; the host owns the actual `wgpu`
//! objects and enforces limits (maximum buffer size, texture dimensions, shader
//! compilation timeouts, etc.).

use std::collections::HashMap;
use std::sync::Arc;
use wgpu;

/// Opaque handle type for GPU resources visible to the guest.
pub type GpuHandle = u32;

/// Per-module GPU state: device, queue, and resource tables.
pub struct GpuState {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    next_handle: GpuHandle,
    buffers: HashMap<GpuHandle, wgpu::Buffer>,
    textures: HashMap<GpuHandle, GpuTexture>,
    shaders: HashMap<GpuHandle, wgpu::ShaderModule>,
    pipelines: HashMap<GpuHandle, GpuPipeline>,
    /// RGBA output surface that gets composited into the canvas (reserved for GPU readback).
    #[allow(dead_code)]
    readback_buffer: Option<ReadbackBuffer>,
}

#[allow(dead_code)]
struct GpuTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

enum GpuPipeline {
    Render(wgpu::RenderPipeline),
    Compute(wgpu::ComputePipeline),
}

#[allow(dead_code)]
struct ReadbackBuffer {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
}

/// Maximum buffer size a guest may allocate (64 MB).
const MAX_BUFFER_SIZE: u64 = 64 * 1024 * 1024;

/// Maximum texture dimension (4096).
const MAX_TEXTURE_DIM: u32 = 4096;

impl GpuState {
    fn alloc_handle(&mut self) -> GpuHandle {
        let h = self.next_handle;
        self.next_handle += 1;
        h
    }

    /// Create a GPU buffer with the given size and usage flags.
    /// Returns a handle, or 0 on failure.
    pub fn create_buffer(&mut self, size: u64, usage_bits: u32) -> GpuHandle {
        if size == 0 || size > MAX_BUFFER_SIZE {
            return 0;
        }
        let usage = wgpu::BufferUsages::from_bits_truncate(usage_bits)
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide_guest_buffer"),
            size,
            usage,
            mapped_at_creation: false,
        });
        let h = self.alloc_handle();
        self.buffers.insert(h, buffer);
        h
    }

    /// Create a 2D RGBA8 texture. Returns a handle, or 0 on failure.
    pub fn create_texture(&mut self, width: u32, height: u32) -> GpuHandle {
        if width == 0 || height == 0 || width > MAX_TEXTURE_DIM || height > MAX_TEXTURE_DIM {
            return 0;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxide_guest_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let h = self.alloc_handle();
        self.textures.insert(
            h,
            GpuTexture {
                texture,
                view,
                width,
                height,
            },
        );
        h
    }

    /// Compile a WGSL shader module. Returns a handle, or 0 on failure.
    pub fn create_shader(&mut self, source: &str) -> GpuHandle {
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("oxide_guest_shader"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let h = self.alloc_handle();
        self.shaders.insert(h, module);
        h
    }

    /// Create a render pipeline from a shader handle.
    /// `vertex_entry` and `fragment_entry` name the WGSL entry points.
    /// Returns a handle, or 0 if the shader handle is invalid.
    pub fn create_render_pipeline(
        &mut self,
        shader_handle: GpuHandle,
        vertex_entry: &str,
        fragment_entry: &str,
    ) -> GpuHandle {
        let shader = match self.shaders.get(&shader_handle) {
            Some(s) => s,
            None => return 0,
        };
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("oxide_guest_render_pipeline"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some(vertex_entry),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
        let h = self.alloc_handle();
        self.pipelines.insert(h, GpuPipeline::Render(pipeline));
        h
    }

    /// Create a compute pipeline from a shader handle.
    pub fn create_compute_pipeline(
        &mut self,
        shader_handle: GpuHandle,
        entry_point: &str,
    ) -> GpuHandle {
        let shader = match self.shaders.get(&shader_handle) {
            Some(s) => s,
            None => return 0,
        };
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("oxide_guest_compute_pipeline"),
                layout: None,
                module: shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            });
        let h = self.alloc_handle();
        self.pipelines.insert(h, GpuPipeline::Compute(pipeline));
        h
    }

    /// Write data to a GPU buffer from guest memory.
    pub fn write_buffer(&self, handle: GpuHandle, offset: u64, data: &[u8]) -> bool {
        match self.buffers.get(&handle) {
            Some(buf) => {
                self.queue.write_buffer(buf, offset, data);
                true
            }
            None => false,
        }
    }

    /// Submit a render pass that draws `vertex_count` vertices using the given pipeline,
    /// targeting a texture.
    pub fn draw(
        &self,
        pipeline_handle: GpuHandle,
        target_texture: GpuHandle,
        vertex_count: u32,
        instance_count: u32,
    ) -> bool {
        let pipeline = match self.pipelines.get(&pipeline_handle) {
            Some(GpuPipeline::Render(p)) => p,
            _ => return false,
        };
        let target = match self.textures.get(&target_texture) {
            Some(t) => t,
            None => return false,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("oxide_guest_draw"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oxide_guest_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.draw(0..vertex_count, 0..instance_count.max(1));
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        true
    }

    /// Submit a compute dispatch.
    pub fn dispatch_compute(
        &self,
        pipeline_handle: GpuHandle,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) -> bool {
        let pipeline = match self.pipelines.get(&pipeline_handle) {
            Some(GpuPipeline::Compute(p)) => p,
            _ => return false,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("oxide_guest_compute"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("oxide_guest_compute_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(workgroups_x, workgroups_y, workgroups_z);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        true
    }

    /// Destroy a buffer resource.
    pub fn destroy_buffer(&mut self, handle: GpuHandle) -> bool {
        if let Some(buf) = self.buffers.remove(&handle) {
            buf.destroy();
            true
        } else {
            false
        }
    }

    /// Destroy a texture resource.
    pub fn destroy_texture(&mut self, handle: GpuHandle) -> bool {
        if let Some(tex) = self.textures.remove(&handle) {
            tex.texture.destroy();
            true
        } else {
            false
        }
    }
}

/// Initialise the wgpu device and queue, returning a ready-to-use [`GpuState`].
///
/// Uses the default backend (Vulkan, Metal, DX12) with low power preference.
/// Returns `None` if no suitable adapter is found.
pub fn init_gpu() -> Option<GpuState> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok()?;

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("oxide_gpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
        },
    ))
    .ok()?;

    Some(GpuState {
        device: Arc::new(device),
        queue: Arc::new(queue),
        next_handle: 1,
        buffers: HashMap::new(),
        textures: HashMap::new(),
        shaders: HashMap::new(),
        pipelines: HashMap::new(),
        readback_buffer: None,
    })
}
