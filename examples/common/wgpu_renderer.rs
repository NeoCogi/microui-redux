#![allow(dead_code)]
//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.

use std::{collections::HashMap, mem, slice, sync::Arc};

use microui_redux::*;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rs_math3d::{Vec3f, Vec4f};
use sdl2::video::Window;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone)]
pub struct MeshBuffers {
    vertices: Arc<[MeshVertex]>,
    indices: Arc<[u32]>,
}

impl MeshBuffers {
    pub fn from_vecs(vertices: Vec<MeshVertex>, indices: Vec<u32>) -> Self {
        Self {
            vertices: vertices.into(),
            indices: indices.into(),
        }
    }

    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() || self.indices.is_empty()
    }
}

#[derive(Clone)]
pub struct MeshSubmission {
    pub mesh: MeshBuffers,
    pub pvm: Mat4f,
    pub view_model: Mat4f,
}

#[derive(Clone, Copy)]
pub struct CustomRenderArea {
    pub rect: Recti,
    pub clip: Recti,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GpuVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [u8; 4],
}

impl GpuVertex {
    fn from_vertex(v: &Vertex) -> Self {
        let pos = v.position();
        let uv = v.tex_coord();
        let c = v.color();
        Self {
            pos: [pos.x, pos.y],
            uv: [uv.x, uv.y],
            color: [c.x, c.y, c.z, c.w],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Uniforms {
    screen_size: [f32; 2],
    _pad: [f32; 2],
}

enum RenderCommand {
    DrawUiTo(usize),
    DrawTexture { id: TextureId, vertices: Vec<GpuVertex> },
    DrawColored { area: CustomRenderArea, vertices: Vec<GpuVertex> },
}

struct GpuTexture {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

pub struct WgpuRenderer {
    atlas: AtlasHandle,
    atlas_last_update_id: usize,
    textures: HashMap<TextureId, GpuTexture>,

    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    _adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    atlas_texture: GpuTexture,

    ui_vertices: Vec<GpuVertex>,
    commands: Vec<RenderCommand>,
    current_batch_end: usize,

    staging_vertex_buffer: wgpu::Buffer,
    staging_vertex_capacity: usize,

    width: u32,
    height: u32,
    clear_color: Color,
}

impl WgpuRenderer {
    fn white_uv_center(&self) -> Vec2f {
        let rect = self.atlas.get_icon_rect(WHITE_ICON);
        let dim = self.atlas.get_texture_dimension();
        Vec2f::new(
            (rect.x as f32 + rect.width as f32 * 0.5) / dim.width as f32,
            (rect.y as f32 + rect.height as f32 * 0.5) / dim.height as f32,
        )
    }

    fn as_bytes<T>(value: &T) -> &[u8] {
        unsafe { slice::from_raw_parts((value as *const T).cast::<u8>(), mem::size_of::<T>()) }
    }

    fn slice_as_bytes<T>(values: &[T]) -> &[u8] {
        unsafe { slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values)) }
    }

    fn create_gpu_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        uniform_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
        pixels: Option<&[u8]>,
    ) -> Result<GpuTexture, String> {
        if width == 0 || height == 0 {
            return Err("texture dimensions must be non-zero".into());
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("microui.texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        if let Some(bytes) = pixels {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(width.saturating_mul(4)),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("microui.texture.bind_group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });

        Ok(GpuTexture { _texture: texture, bind_group })
    }

    fn flush_ui_batch(&mut self) {
        let end = self.ui_vertices.len();
        if end > self.current_batch_end {
            self.commands.push(RenderCommand::DrawUiTo(end));
            self.current_batch_end = end;
        }
    }

    fn sync_atlas(&mut self) {
        if self.atlas_last_update_id == self.atlas.get_last_update_id() {
            return;
        }

        self.atlas.apply_pixels(|width, height, pixels| {
            let pixel_ptr = pixels.as_ptr() as *const u8;
            let pixel_slice: &[u8] = unsafe { slice::from_raw_parts(pixel_ptr, pixels.len() * 4) };
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &self.atlas_texture._texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixel_slice,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some((width as u32).saturating_mul(4)),
                    rows_per_image: Some(height as u32),
                },
                wgpu::Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                },
            );
        });

        self.atlas_last_update_id = self.atlas.get_last_update_id();
    }

    fn configure_surface(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn ensure_staging_capacity(&mut self, byte_count: usize) {
        if byte_count <= self.staging_vertex_capacity {
            return;
        }

        let target = byte_count.next_power_of_two().max(64 * 1024);
        self.staging_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("microui.staging.vertices"),
            size: target as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.staging_vertex_capacity = target;
    }

    fn clip_to_scissor(clip: Recti, surface_width: u32, surface_height: u32) -> Option<(u32, u32, u32, u32)> {
        if clip.width <= 0 || clip.height <= 0 {
            return None;
        }

        let x0 = clip.x.max(0).min(surface_width as i32);
        let y0 = clip.y.max(0).min(surface_height as i32);
        let x1 = (clip.x + clip.width).max(0).min(surface_width as i32);
        let y1 = (clip.y + clip.height).max(0).min(surface_height as i32);

        if x1 <= x0 || y1 <= y0 {
            return None;
        }

        Some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
    }

    fn append_quad(dst: &mut Vec<GpuVertex>, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        dst.extend_from_slice(&[
            GpuVertex::from_vertex(v0),
            GpuVertex::from_vertex(v1),
            GpuVertex::from_vertex(v2),
            GpuVertex::from_vertex(v0),
            GpuVertex::from_vertex(v2),
            GpuVertex::from_vertex(v3),
        ]);
    }

    fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("microui.wgpu.shader"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("microui.bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("microui.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("microui.pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("microui.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<GpuVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Unorm8x4,
                            offset: 16,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        (pipeline, bind_group_layout, sampler)
    }

    pub fn new(window: &Window, atlas: AtlasHandle, width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::default();

        let raw_window_handle = window
            .window_handle()
            .map_err(|err| format!("failed to get raw window handle: {err}"))?
            .as_raw();
        let raw_display_handle = window
            .display_handle()
            .map_err(|err| format!("failed to get raw display handle: {err}"))?
            .as_raw();

        let surface = unsafe {
            instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle { raw_display_handle, raw_window_handle })
                .map_err(|err| format!("failed to create wgpu surface: {err}"))?
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| "failed to request wgpu adapter".to_string())?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("microui.wgpu.device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        ))
        .map_err(|err| format!("failed to request wgpu device: {err}"))?;

        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or_else(|| "surface is not supported by adapter".to_string())?;
        if config.format.is_srgb() {
            config.format = config.format.remove_srgb_suffix();
        }
        surface.configure(&device, &config);

        let (pipeline, bind_group_layout, sampler) = Self::create_pipeline(&device, config.format);
        let uniforms = Uniforms {
            screen_size: [width.max(1) as f32, height.max(1) as f32],
            _pad: [0.0, 0.0],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("microui.uniforms"),
            contents: Self::as_bytes(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let atlas_texture = Self::create_gpu_texture(
            &device,
            &queue,
            &bind_group_layout,
            &sampler,
            &uniform_buffer,
            atlas.width() as u32,
            atlas.height() as u32,
            None,
        )?;

        let initial_capacity = 64 * 1024;
        let staging_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("microui.staging.vertices"),
            size: initial_capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut renderer = Self {
            atlas,
            atlas_last_update_id: usize::MAX,
            textures: HashMap::new(),

            _instance: instance,
            surface,
            _adapter: adapter,
            device,
            queue,
            config,

            pipeline,
            bind_group_layout,
            sampler,
            uniform_buffer,
            atlas_texture,

            ui_vertices: Vec::new(),
            commands: Vec::new(),
            current_batch_end: 0,

            staging_vertex_buffer,
            staging_vertex_capacity: initial_capacity,

            width,
            height,
            clear_color: color(0, 0, 0, 255),
        };
        renderer.sync_atlas();

        Ok(renderer)
    }

    pub fn enqueue_colored_vertices(&mut self, area: CustomRenderArea, vertices: Vec<Vertex>) {
        if vertices.is_empty() {
            return;
        }
        self.flush_ui_batch();
        let vertices = vertices.iter().map(GpuVertex::from_vertex).collect();
        self.commands.push(RenderCommand::DrawColored { area, vertices });
    }

    pub fn enqueue_mesh_draw(&mut self, area: CustomRenderArea, submission: MeshSubmission) {
        if submission.mesh.is_empty() {
            return;
        }
        if area.rect.width <= 0 || area.rect.height <= 0 {
            return;
        }

        let white_uv = self.white_uv_center();

        #[derive(Clone)]
        struct Tri {
            depth: f32,
            verts: [Vertex; 3],
        }

        let mut tris: Vec<Tri> = Vec::with_capacity(submission.mesh.indices().len() / 3);
        let mesh = &submission.mesh;
        let pvm = &submission.pvm;

        for tri in mesh.indices().chunks_exact(3) {
            let idxs = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let mut clip_space = [Vec4f::default(); 3];

            for (dst, src_idx) in clip_space.iter_mut().zip(&idxs) {
                let v = &mesh.vertices()[*src_idx];
                *dst = *pvm * Vec4f::new(v.position[0], v.position[1], v.position[2], 1.0);
            }

            let a = clip_space[0];
            let b = clip_space[1];
            let c = clip_space[2];
            let ab = Vec3f::new(b.x - a.x, b.y - a.y, b.z - a.z);
            let ac = Vec3f::new(c.x - a.x, c.y - a.y, c.z - a.z);
            let cross = Vec3f::cross(&ab, &ac);
            if cross.z <= 0.0 {
                continue;
            }

            let mut verts = [Vertex::new(Vec2f::default(), white_uv, color4b(0, 0, 0, 255)); 3];
            let mut depth_acc = 0.0;
            let mut valid = true;

            for ((clip, src_idx), out_v) in clip_space.iter().zip(&idxs).zip(verts.iter_mut()) {
                if clip.w.abs() < 1e-5 {
                    valid = false;
                    break;
                }
                let ndc = Vec3f::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
                depth_acc += ndc.z;
                let sx = area.rect.x as f32 + (ndc.x * 0.5 + 0.5) * area.rect.width as f32;
                let sy = area.rect.y as f32 + (-ndc.y * 0.5 + 0.5) * area.rect.height as f32;

                let v = &mesh.vertices()[*src_idx];
                let normal = Vec3f::new(v.normal[0], v.normal[1], v.normal[2]);
                let color = (normal * 0.5) + Vec3f::new(0.5, 0.5, 0.5);
                let r = (color.x.clamp(0.0, 1.0) * 255.0) as u8;
                let g = (color.y.clamp(0.0, 1.0) * 255.0) as u8;
                let b = (color.z.clamp(0.0, 1.0) * 255.0) as u8;

                *out_v = Vertex::new(Vec2f::new(sx, sy), white_uv, color4b(r, g, b, 255));
            }

            if valid {
                tris.push(Tri { depth: depth_acc / 3.0, verts });
            }
        }

        tris.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));

        let mut verts: Vec<Vertex> = Vec::with_capacity(tris.len() * 3);
        for tri in tris {
            verts.extend_from_slice(&tri.verts);
        }

        self.enqueue_colored_vertices(area, verts);
    }
}

impl Renderer for WgpuRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.width = width.max(0) as u32;
        self.height = height.max(0) as u32;
        self.clear_color = clr;
        self.ui_vertices.clear();
        self.commands.clear();
        self.current_batch_end = 0;

        self.configure_surface(self.width.max(1), self.height.max(1));
        self.sync_atlas();

        let uniforms = Uniforms {
            screen_size: [self.width.max(1) as f32, self.height.max(1) as f32],
            _pad: [0.0, 0.0],
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, Self::as_bytes(&uniforms));
    }

    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        Self::append_quad(&mut self.ui_vertices, v0, v1, v2, v3);
    }

    fn flush(&mut self) {
        self.flush_ui_batch();
    }

    fn end(&mut self) {
        self.flush_ui_batch();
        self.sync_atlas();

        if self.width == 0 || self.height == 0 {
            return;
        }

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(err) => {
                        eprintln!("[microui-redux][wgpu] failed to acquire frame after reconfigure: {err}");
                        return;
                    }
                }
            }
            Err(wgpu::SurfaceError::Timeout) => {
                return;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                eprintln!("[microui-redux][wgpu] surface out of memory");
                return;
            }
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("microui.encoder") });

        struct PreparedDraw {
            vertex_offset: u64,
            vertex_size: u64,
            vertex_count: u32,
            source: DrawSource,
            scissor: Option<(u32, u32, u32, u32)>,
        }
        #[derive(Clone, Copy)]
        enum DrawSource {
            Atlas,
            Texture(TextureId),
        }

        let mut ui_start = 0usize;
        let mut prepared_draws: Vec<PreparedDraw> = Vec::new();
        let mut packed_upload: Vec<u8> = Vec::new();

        for cmd in &self.commands {
            let (vertices, source, scissor) = match cmd {
                RenderCommand::DrawUiTo(end) => {
                    let end = (*end).min(self.ui_vertices.len());
                    if end <= ui_start {
                        continue;
                    }
                    let verts = &self.ui_vertices[ui_start..end];
                    ui_start = end;
                    (verts, DrawSource::Atlas, None)
                }
                RenderCommand::DrawTexture { id, vertices } => {
                    if !self.textures.contains_key(id) {
                        continue;
                    }
                    (vertices.as_slice(), DrawSource::Texture(*id), None)
                }
                RenderCommand::DrawColored { area, vertices } => {
                    let scissor = Self::clip_to_scissor(area.clip, self.config.width, self.config.height);
                    if scissor.is_none() {
                        continue;
                    }
                    (vertices.as_slice(), DrawSource::Atlas, scissor)
                }
            };

            if vertices.is_empty() {
                continue;
            }

            let bytes = Self::slice_as_bytes(vertices);
            let vertex_offset = packed_upload.len() as u64;
            let vertex_size = bytes.len() as u64;
            packed_upload.extend_from_slice(bytes);
            prepared_draws.push(PreparedDraw {
                vertex_offset,
                vertex_size,
                vertex_count: vertices.len() as u32,
                source,
                scissor,
            });
        }

        if !packed_upload.is_empty() {
            self.ensure_staging_capacity(packed_upload.len());
            self.queue.write_buffer(&self.staging_vertex_buffer, 0, packed_upload.as_slice());
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("microui.render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.clear_color.r as f64 / 255.0,
                            g: self.clear_color.g as f64 / 255.0,
                            b: self.clear_color.b as f64 / 255.0,
                            a: self.clear_color.a as f64 / 255.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_scissor_rect(0, 0, self.config.width, self.config.height);

            for draw in prepared_draws.iter() {
                if let Some((x, y, w, h)) = draw.scissor {
                    pass.set_scissor_rect(x, y, w, h);
                } else {
                    pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                }

                let bind_group = match draw.source {
                    DrawSource::Atlas => &self.atlas_texture.bind_group,
                    DrawSource::Texture(id) => {
                        let Some(texture) = self.textures.get(&id) else {
                            continue;
                        };
                        &texture.bind_group
                    }
                };

                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, self.staging_vertex_buffer.slice(draw.vertex_offset..(draw.vertex_offset + draw.vertex_size)));
                pass.draw(0..draw.vertex_count, 0..1);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
    }

    fn create_texture(&mut self, id: TextureId, width: i32, height: i32, pixels: &[u8]) {
        if width <= 0 || height <= 0 {
            return;
        }

        let texture = match Self::create_gpu_texture(
            &self.device,
            &self.queue,
            &self.bind_group_layout,
            &self.sampler,
            &self.uniform_buffer,
            width as u32,
            height as u32,
            Some(pixels),
        ) {
            Ok(texture) => texture,
            Err(err) => {
                eprintln!("[microui-redux][wgpu] create texture failed: {err}");
                return;
            }
        };
        self.textures.insert(id, texture);
    }

    fn destroy_texture(&mut self, id: TextureId) {
        self.textures.remove(&id);
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        if !self.textures.contains_key(&id) {
            return;
        }

        self.flush_ui_batch();
        let mut quad = Vec::with_capacity(6);
        Self::append_quad(&mut quad, &vertices[0], &vertices[1], &vertices[2], &vertices[3]);
        self.commands.push(RenderCommand::DrawTexture { id, vertices: quad });
    }
}

const UI_SHADER: &str = r#"
struct Uniforms {
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var atlas_sampler: sampler;

@group(0) @binding(2)
var atlas_texture: texture_2d<f32>;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let ndc_x = (input.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (input.position.y / uniforms.screen_size.y) * 2.0;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let texel = textureSample(atlas_texture, atlas_sampler, input.uv);
    return texel * input.color;
}
"#;
