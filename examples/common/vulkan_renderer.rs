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
//
// Vulkan renderer optimizations (current status):
// - UI and custom geometry uploads go through per-frame staging/device buffers that grow with
//   demand, stay resident, and are referenced via ring offsets to avoid reallocations.
// - Staging buffers remain persistently mapped; command submission uses reusable per-frame
//   transfer command buffers, synchronized with semaphores/barriers instead of queue_wait_idle.
// - Custom draw uploads share the same staging path as UI vertices so we issue one batched
//   transfer stream per frame.
// - Mesh vertex/index data lives in device-local memory and is refreshed via staging uploads.
// - Texture descriptors are recreated automatically after swapchain rebuilds, and atlas uploads
//   are retriggered whenever UI resources lose their backing image, keeping rendering seamless
//   across window resizes.
//

use std::{collections::HashMap, convert::TryFrom, ffi::CString, io::Cursor, mem, ptr, sync::Arc};

use ash::{khr, util::read_spv, vk, Entry};
use microui_redux::*;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use sdl2::video::Window;

type Result<T> = std::result::Result<T, String>;
type Surface = khr::surface::Instance;
type Swapchain = khr::swapchain::Device;

mod vk_builder {
    use super::vk;

    pub trait BuilderExt: Sized {
        fn builder() -> Self;
        fn build(self) -> Self {
            self
        }
    }

    macro_rules! impl_lifetime {
        ($($ty:ident),+ $(,)?) => {
            $(
                impl<'a> BuilderExt for vk::$ty<'a> {
                    fn builder() -> Self { Self::default() }
                }
            )+
        };
    }

    macro_rules! impl_plain {
        ($($ty:ty),+ $(,)?) => {
            $(
                impl BuilderExt for $ty {
                    fn builder() -> Self { Self::default() }
                }
            )+
        };
    }

    impl_lifetime!(
        ApplicationInfo,
        InstanceCreateInfo,
        DeviceQueueCreateInfo,
        DeviceCreateInfo,
        SwapchainCreateInfoKHR,
        ImageViewCreateInfo,
        SubpassDescription,
        RenderPassCreateInfo,
        FramebufferCreateInfo,
        CommandPoolCreateInfo,
        CommandBufferAllocateInfo,
        FenceCreateInfo,
        SubmitInfo,
        PresentInfoKHR,
        CommandBufferBeginInfo,
        RenderPassBeginInfo,
        DescriptorSetLayoutBinding,
        DescriptorSetLayoutCreateInfo,
        PipelineLayoutCreateInfo,
        BufferMemoryBarrier,
        PipelineShaderStageCreateInfo,
        PipelineVertexInputStateCreateInfo,
        PipelineInputAssemblyStateCreateInfo,
        PipelineRasterizationStateCreateInfo,
        PipelineMultisampleStateCreateInfo,
        PipelineColorBlendStateCreateInfo,
        PipelineDynamicStateCreateInfo,
        PipelineViewportStateCreateInfo,
        PipelineDepthStencilStateCreateInfo,
        GraphicsPipelineCreateInfo,
        SamplerCreateInfo,
        DescriptorPoolCreateInfo,
        DescriptorSetAllocateInfo,
        ShaderModuleCreateInfo,
        BufferCreateInfo,
        MemoryAllocateInfo,
        ImageCreateInfo,
        WriteDescriptorSet,
        ImageMemoryBarrier,
    );

    impl_plain!(
        vk::PhysicalDeviceFeatures,
        vk::AttachmentDescription,
        vk::SubpassDependency,
        vk::PushConstantRange,
        vk::VertexInputBindingDescription,
        vk::VertexInputAttributeDescription,
        vk::PipelineColorBlendAttachmentState,
        vk::DescriptorPoolSize,
        vk::DescriptorImageInfo,
        vk::Viewport,
        vk::Rect2D,
        vk::BufferImageCopy,
        vk::ImageSubresourceLayers,
        vk::ImageSubresourceRange,
    );
}
use vk_builder::BuilderExt;

const UI_VERT_SPV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/vulkan/ui.vert.spv"));
const UI_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/vulkan/ui.frag.spv"));
const MESH_VERT_SPV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/vulkan/mesh.vert.spv"));
const MESH_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/vulkan/mesh.frag.spv"));

pub struct VulkanRenderer {
    atlas: AtlasHandle,
    last_atlas_update_id: usize,
    textures: HashMap<TextureId, VulkanTexture>,
    context: VulkanContext,
    last_swapchain_generation: u64,
    vertices: Vec<Vertex>,
    commands: Vec<FrameCommand>,
    current_batch_end: usize,
    clear_color: Color,
    width: u32,
    height: u32,
    frame_index: u64,
}

impl VulkanRenderer {
    fn flush_ui_batch(&mut self) {
        let end = self.vertices.len();
        if end > self.current_batch_end {
            self.commands.push(FrameCommand::DrawTo(end));
            self.current_batch_end = end;
        }
    }

    pub fn new(window: &Window, atlas: AtlasHandle, width: u32, height: u32) -> Result<Self> {
        let context = VulkanContext::new(window, width, height)?;
        let swapchain_generation = context.swapchain_generation();

        Ok(Self {
            atlas,
            last_atlas_update_id: usize::MAX,
            textures: HashMap::new(),
            context,
            last_swapchain_generation: swapchain_generation,
            vertices: Vec::new(),
            commands: Vec::new(),
            current_batch_end: 0,
            clear_color: color(0, 0, 0, 255),
            width,
            height,
            frame_index: 0,
        })
    }

    fn ensure_swapchain_extent(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Ok(()); // Minimized window; skip until it has a size.
        }

        let extent = self.context.extent();
        if extent.width != width || extent.height != height {
            self.context.recreate_swapchain(width, height)?;
        }

        Ok(())
    }

    fn handle_swapchain_updates(&mut self) {
        let generation = self.context.swapchain_generation();
        if self.last_swapchain_generation != generation {
            self.last_swapchain_generation = generation;
            self.last_atlas_update_id = usize::MAX;
            if let Err(err) = self.rebind_texture_descriptors() {
                eprintln!("[microui-redux][vulkan] failed to rebind texture descriptors: {err}");
            }
        }
    }

    fn rebind_texture_descriptors(&mut self) -> Result<()> {
        for texture in self.textures.values_mut() {
            let descriptor = self.context.allocate_texture_descriptor(&texture.image)?;
            texture.descriptor_set = descriptor;
        }
        Ok(())
    }

    pub(crate) fn enqueue_custom_render<C: VulkanCustomRenderer + 'static>(&mut self, area: CustomRenderArea, cmd: C) {
        self.flush_ui_batch();
        self.commands.push(FrameCommand::Custom(CustomRenderJob {
            area,
            kind: "custom",
            callback: Box::new(cmd),
        }));
    }

    fn color_to_vk_clear(color: Color) -> vk::ClearValue {
        let to_float = |c: u8| c as f32 / 255.0;
        vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [to_float(color.r), to_float(color.g), to_float(color.b), to_float(color.a)],
            },
        }
    }

    fn sync_atlas(&mut self) {
        let needs_upload = self.last_atlas_update_id != self.atlas.get_last_update_id() || !self.context.ui_has_atlas();
        if needs_upload {
            if let Err(err) = self.context.upload_atlas(&self.atlas) {
                eprintln!("[microui-redux][vulkan] failed to upload atlas: {err}");
                return;
            }
            self.last_atlas_update_id = self.atlas.get_last_update_id();
        }
    }
}

impl Renderer for VulkanRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.frame_index = self.frame_index.wrapping_add(1);
        self.width = width as u32;
        self.height = height as u32;
        self.clear_color = clr;
        self.vertices.clear();
        self.commands.clear();
        self.current_batch_end = 0;

        if let Err(err) = self.ensure_swapchain_extent(self.width, self.height) {
            eprintln!("[microui-redux][vulkan] failed to resize swapchain: {err}");
        }
        self.handle_swapchain_updates();
        self.sync_atlas();
    }

    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        self.vertices.extend_from_slice(&[*v0, *v1, *v2, *v0, *v2, *v3]);
    }

    fn flush(&mut self) {
        // Match the GL renderer expectation: turn buffered UI vertices into a draw command before
        // custom rendering happens.
        self.flush_ui_batch();
    }

    fn end(&mut self) {
        self.flush_ui_batch();
        let mut commands = std::mem::take(&mut self.commands);
        if let Err(err) = self.context.draw_frame(
            Self::color_to_vk_clear(self.clear_color),
            &self.vertices,
            self.width,
            self.height,
            self.frame_index,
            &mut commands,
        ) {
            eprintln!("[microui-redux][vulkan] draw_frame failed: {err}");
        }
        commands.clear();
        self.commands = commands;
        self.vertices.clear();
        self.current_batch_end = 0;
    }

    fn create_texture(&mut self, id: TextureId, width: i32, height: i32, pixels: &[u8]) {
        match self.context.create_texture_resource(width, height, pixels) {
            Ok(texture) => {
                self.textures.insert(id, texture);
            }
            Err(err) => eprintln!("[microui-redux][vulkan] create_texture failed: {err}"),
        }
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(mut texture) = self.textures.remove(&id) {
            texture.image.destroy(&self.context.device);
        }
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        let descriptor = match self.textures.get(&id).map(|tex| tex.descriptor_set) {
            Some(desc) => desc,
            None => return,
        };

        let mut quad = Vec::with_capacity(6);
        quad.extend_from_slice(&[vertices[0], vertices[1], vertices[2], vertices[0], vertices[2], vertices[3]]);

        let area_rect = rect_from_vertices(&vertices);
        let area = CustomRenderArea { rect: area_rect, clip: area_rect };

        self.flush_ui_batch();
        self.commands.push(FrameCommand::Custom(CustomRenderJob {
            area,
            kind: "texture",
            callback: Box::new(TextureDrawCommand {
                vertices: quad,
                descriptor_set: descriptor,
            }),
        }));
    }
}

impl VulkanRenderer {
    pub fn enqueue_colored_vertices(&mut self, area: CustomRenderArea, vertices: Vec<Vertex>) {
        if vertices.is_empty() {
            return;
        }
        let descriptor_set = match self.context.ui_descriptor_set() {
            Some(set) => set,
            None => return,
        };
        self.flush_ui_batch();
        self.commands.push(FrameCommand::Custom(CustomRenderJob {
            area,
            kind: "colored",
            callback: Box::new(ColoredVerticesCommand { vertices, descriptor_set }),
        }));
    }

    pub fn enqueue_mesh_draw(&mut self, area: CustomRenderArea, submission: MeshSubmission) {
        if submission.mesh.is_empty() {
            return;
        }
        self.flush_ui_batch();
        self.commands.push(FrameCommand::Custom(CustomRenderJob {
            area,
            kind: "mesh",
            callback: Box::new(MeshDrawCommand { submission }),
        }));
    }
}

struct VulkanTexture {
    image: ImageResource,
    descriptor_set: vk::DescriptorSet,
}

struct Buffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
}

impl Buffer {
    fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
        self.buffer = vk::Buffer::null();
        self.memory = vk::DeviceMemory::null();
        self.size = 0;
    }
}

struct MappedBuffer {
    buffer: Buffer,
    ptr: *mut u8,
}

impl MappedBuffer {
    fn new(buffer: Buffer, device: &ash::Device) -> Result<Self> {
        let ptr = unsafe {
            device
                .map_memory(buffer.memory, 0, buffer.size, vk::MemoryMapFlags::empty())
                .map_err(|err| format!("map_memory (staging) failed: {err:?}"))?
        } as *mut u8;
        Ok(Self { buffer, ptr })
    }

    fn size(&self) -> vk::DeviceSize {
        self.buffer.size
    }

    fn vk_buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    fn write(&self, offset: vk::DeviceSize, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let len = u64::try_from(data.len()).map_err(|_| "staging data too large".to_string())?;
        let end = offset.checked_add(len).ok_or_else(|| "staging write overflow".to_string())?;
        if end > self.buffer.size {
            return Err("staging write exceeds buffer size".into());
        }
        let offset_usize = usize::try_from(offset).map_err(|_| "staging offset exceeds address space".to_string())?;
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.add(offset_usize), data.len());
        }
        Ok(())
    }

    fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            if !self.ptr.is_null() {
                device.unmap_memory(self.buffer.memory);
            }
        }
        self.ptr = ptr::null_mut();
        self.buffer.destroy(device);
    }
}

struct PendingCopy {
    src: vk::Buffer,
    dst: vk::Buffer,
    src_offset: vk::DeviceSize,
    dst_offset: vk::DeviceSize,
    size: vk::DeviceSize,
}

struct ImageResource {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    extent: vk::Extent2D,
    format: vk::Format,
    layout: vk::ImageLayout,
}

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

pub(crate) trait VulkanCustomRenderer: Send {
    fn record(&mut self, ctx: &mut VulkanContext, command_buffer: vk::CommandBuffer, extent: vk::Extent2D, area: &CustomRenderArea);
}

struct CustomRenderJob {
    area: CustomRenderArea,
    kind: &'static str,
    callback: Box<dyn VulkanCustomRenderer>,
}

fn vk_trace_enabled() -> bool {
    false
}

fn vk_dump_enabled() -> bool {
    false
}

// No-op toggles removed; keep environment helpers minimal.

macro_rules! vk_trace {
    ($($arg:tt)*) => {
        if vk_trace_enabled() {
            eprintln!($($arg)*);
        }
    };
}

enum FrameCommand {
    DrawTo(usize),
    Custom(CustomRenderJob),
}

struct TextureDrawCommand {
    vertices: Vec<Vertex>,
    descriptor_set: vk::DescriptorSet,
}

struct ColoredVerticesCommand {
    vertices: Vec<Vertex>,
    descriptor_set: vk::DescriptorSet,
}

struct MeshDrawCommand {
    submission: MeshSubmission,
}

impl VulkanCustomRenderer for TextureDrawCommand {
    fn record(&mut self, ctx: &mut VulkanContext, command_buffer: vk::CommandBuffer, _extent: vk::Extent2D, area: &CustomRenderArea) {
        if let Err(err) = ctx.draw_vertices_with_descriptor(
            command_buffer,
            &self.vertices,
            ctx.logical_width.max(1),
            ctx.logical_height.max(1),
            self.descriptor_set,
            Some(area),
        ) {
            eprintln!("[microui-redux][vulkan] texture draw failed: {err}");
        }
    }
}

impl VulkanCustomRenderer for ColoredVerticesCommand {
    fn record(&mut self, ctx: &mut VulkanContext, command_buffer: vk::CommandBuffer, _extent: vk::Extent2D, area: &CustomRenderArea) {
        if let Err(err) = ctx.draw_custom_vertices(command_buffer, &self.vertices, area, self.descriptor_set) {
            eprintln!("[microui-redux][vulkan] solid draw failed: {err}");
        }
    }
}

impl VulkanCustomRenderer for MeshDrawCommand {
    fn record(&mut self, ctx: &mut VulkanContext, command_buffer: vk::CommandBuffer, _extent: vk::Extent2D, area: &CustomRenderArea) {
        if let Err(err) = ctx.record_mesh(command_buffer, &self.submission, area) {
            eprintln!("[microui-redux][vulkan] mesh draw failed: {err}");
        }
    }
}

fn rect_from_vertices(vertices: &[Vertex]) -> Recti {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for v in vertices {
        let pos = v.position();
        min_x = min_x.min(pos.x);
        min_y = min_y.min(pos.y);
        max_x = max_x.max(pos.x);
        max_y = max_y.max(pos.y);
    }
    let x = min_x.floor() as i32;
    let y = min_y.floor() as i32;
    let width = (max_x - min_x).ceil().max(0.0) as i32;
    let height = (max_y - min_y).ceil().max(0.0) as i32;
    Recti::new(x, y, width, height)
}

fn clamp_rect_to_surface(region: Recti, surface_width: u32, surface_height: u32) -> Recti {
    let surface = rect(0, 0, surface_width as i32, surface_height as i32);
    region.intersect(&surface).unwrap_or_else(|| rect(0, 0, 0, 0))
}

fn log_viewport_scissor(_stage: &str, _logical: Recti, _viewport: &vk::Viewport, _scissor: &vk::Rect2D) {}

fn rect_to_vk(rect: Recti, surface_width: u32, surface_height: u32) -> vk::Rect2D {
    let rect = clamp_rect_to_surface(rect, surface_width, surface_height);
    vk::Rect2D {
        offset: vk::Offset2D { x: rect.x.max(0), y: rect.y.max(0) },
        extent: vk::Extent2D {
            width: rect.width.max(0) as u32,
            height: rect.height.max(0) as u32,
        },
    }
}

fn scale_rect_to_surface(rect: Recti, logical_width: u32, logical_height: u32, surface_width: u32, surface_height: u32) -> Recti {
    let lw = logical_width.max(1) as f32;
    let lh = logical_height.max(1) as f32;
    let sx = surface_width as f32 / lw;
    let sy = surface_height as f32 / lh;
    Recti::new(
        (rect.x as f32 * sx).round() as i32,
        (rect.y as f32 * sy).round() as i32,
        (rect.width as f32 * sx).round() as i32,
        (rect.height as f32 * sy).round() as i32,
    )
}

fn ui_full_viewport(extent: vk::Extent2D) -> vk::Viewport {
    vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }
}

fn mesh_viewport_from_rect(rect: Recti, surface_width: u32, surface_height: u32) -> Option<vk::Viewport> {
    let clamped = clamp_rect_to_surface(rect, surface_width, surface_height);
    if clamped.width <= 0 || clamped.height <= 0 {
        return None;
    }
    let x = clamped.x.max(0) as f32;
    let y = clamped.y.max(0) as f32;
    Some(vk::Viewport {
        x,
        y,
        width: clamped.width as f32,
        height: clamped.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    })
}

fn opengl_to_vulkan_clip_matrix() -> Mat4f {
    // Converts OpenGL-style clip space (Y up, Z in [-1, 1]) to Vulkan clip space
    // (Y down, Z in [0, 1]).
    Mat4f::new(
        1.0, 0.0, 0.0, 0.0, //
        0.0, -1.0, 0.0, 0.0, //
        0.0, 0.0, 0.5, 0.0, //
        0.0, 0.0, 0.5, 1.0, //
    )
}

impl ImageResource {
    fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
        self.image = vk::Image::null();
        self.memory = vk::DeviceMemory::null();
        self.view = vk::ImageView::null();
    }
}

const MAX_DESCRIPTOR_SETS: u32 = 128;

struct UiResources {
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    vertex_buffer: Option<Buffer>,        // GPU-local
    staging_buffers: Vec<Option<MappedBuffer>>, // CPU-visible, one slot per frame-in-flight
    atlas: Option<ImageResource>,
    vertex_offset: vk::DeviceSize,
    staging_offsets: Vec<vk::DeviceSize>,
}

impl UiResources {
    const MIN_VERTEX_CAPACITY: vk::DeviceSize = 1_u64 << 20; // 1 MB default
    const MIN_STAGING_CAPACITY: vk::DeviceSize = 64_u64 << 10; // 64 KB default

    fn grow_capacity(current: Option<vk::DeviceSize>, required: vk::DeviceSize, min: vk::DeviceSize) -> vk::DeviceSize {
        if required == 0 {
            return min.max(1);
        }
        let mut size = current.unwrap_or(min).max(min).max(1);
        while size < required {
            size = match size.checked_mul(2) {
                Some(next) => next,
                None => return required,
            };
        }
        size
    }

    fn new(ctx: &VulkanContext) -> Result<Self> {
        let device = &ctx.device;
        let descriptor_set_layout = Self::create_descriptor_set_layout(device)?;
        let pipeline_layout = Self::create_pipeline_layout(device, descriptor_set_layout)?;
        let pipeline = Self::create_pipeline(ctx, pipeline_layout)?;
        let sampler = Self::create_sampler(device)?;
        let descriptor_pool = Self::create_descriptor_pool(device)?;
        let descriptor_set = Self::allocate_descriptor_set(device, descriptor_pool, descriptor_set_layout)?;
        Ok(Self {
            descriptor_set_layout,
            pipeline_layout,
            pipeline,
            sampler,
            descriptor_pool,
            descriptor_set,
            vertex_buffer: None,
            staging_buffers: (0..ctx.max_frames_in_flight).map(|_| None).collect(),
            atlas: None,
            vertex_offset: 0,
            staging_offsets: vec![0; ctx.max_frames_in_flight],
        })
    }

    fn destroy(&mut self, device: &ash::Device) {
        if let Some(mut buffer) = self.vertex_buffer.take() {
            buffer.destroy(device);
        }
        for slot in &mut self.staging_buffers {
            if let Some(mut buffer) = slot.take() {
                buffer.destroy(device);
            }
        }
        if let Some(mut atlas) = self.atlas.take() {
            atlas.destroy(device);
        }
        unsafe {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_sampler(self.sampler, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }

    fn create_descriptor_set_layout(device: &ash::Device) -> Result<vk::DescriptorSetLayout> {
        let binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let bindings = [binding];
        let info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        unsafe { device.create_descriptor_set_layout(&info, None) }.map_err(|err| format!("create_descriptor_set_layout failed: {err:?}"))
    }

    fn create_pipeline_layout(device: &ash::Device, descriptor_set_layout: vk::DescriptorSetLayout) -> Result<vk::PipelineLayout> {
        let set_layouts = [descriptor_set_layout];
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size((std::mem::size_of::<f32>() * 16) as u32)
            .build();
        let push_ranges = [push_constant_range];
        let info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_ranges);
        unsafe { device.create_pipeline_layout(&info, None) }.map_err(|err| format!("create_pipeline_layout failed: {err:?}"))
    }

    fn create_pipeline(ctx: &VulkanContext, pipeline_layout: vk::PipelineLayout) -> Result<vk::Pipeline> {
        let device = &ctx.device;
        let vert_module = Self::create_shader_module(device, UI_VERT_SPV)?;
        let frag_module = Self::create_shader_module(device, UI_FRAG_SPV)?;
        let entry = CString::new("main").unwrap();
        let stage_infos = [
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_module)
                .name(&entry)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_module)
                .name(&entry)
                .build(),
        ];

        let binding_desc = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        let binding_descs = [binding_desc];

        let attr_pos = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();
        let attr_tex = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(8)
            .build();
        let attr_color = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(2)
            .format(vk::Format::R8G8B8A8_UNORM)
            .offset(16)
            .build();
        let attribute_descs = [attr_pos, attr_tex, attr_color];

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&binding_descs)
            .vertex_attribute_descriptions(&attribute_descs);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .line_width(1.0);

        let multisample = vk::PipelineMultisampleStateCreateInfo::builder().rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B | vk::ColorComponentFlags::A)
            .build();
        let blend_attachments = [blend_attachment];
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .attachments(&blend_attachments)
            .logic_op_enable(false);

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(false)
            .depth_write_enable(false);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&dynamic_states);

        let viewport_state = vk::PipelineViewportStateCreateInfo::builder().viewport_count(1).scissor_count(1);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stage_infos)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
            .render_pass(ctx.render_pass)
            .subpass(0);
        let pipeline_infos = [pipeline_info.build()];
        let pipeline = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_infos, None) }
            .map_err(|(_, err)| format!("create_graphics_pipelines failed: {err:?}"))?[0];

        unsafe {
            device.destroy_shader_module(vert_module, None);
            device.destroy_shader_module(frag_module, None);
        }

        Ok(pipeline)
    }

    fn create_sampler(device: &ash::Device) -> Result<vk::Sampler> {
        let info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .border_color(vk::BorderColor::INT_OPAQUE_WHITE);
        unsafe { device.create_sampler(&info, None) }.map_err(|err| format!("create_sampler failed: {err:?}"))
    }

    fn create_descriptor_pool(device: &ash::Device) -> Result<vk::DescriptorPool> {
        let pool_size = vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(MAX_DESCRIPTOR_SETS)
            .build();
        let pool_sizes = [pool_size];
        let info = vk::DescriptorPoolCreateInfo::builder().pool_sizes(&pool_sizes).max_sets(MAX_DESCRIPTOR_SETS);
        unsafe { device.create_descriptor_pool(&info, None) }.map_err(|err| format!("create_descriptor_pool failed: {err:?}"))
    }

    fn allocate_descriptor_set(device: &ash::Device, pool: vk::DescriptorPool, layout: vk::DescriptorSetLayout) -> Result<vk::DescriptorSet> {
        let layouts = [layout];
        let info = vk::DescriptorSetAllocateInfo::builder().descriptor_pool(pool).set_layouts(&layouts);
        let sets = unsafe { device.allocate_descriptor_sets(&info) }.map_err(|err| format!("allocate_descriptor_sets failed: {err:?}"))?;
        Ok(sets[0])
    }

    fn allocate_texture_descriptor(&mut self, ctx: &VulkanContext, image: &ImageResource) -> Result<vk::DescriptorSet> {
        let descriptor_set = Self::allocate_descriptor_set(&ctx.device, self.descriptor_pool, self.descriptor_set_layout)?;
        self.update_descriptor(&ctx.device, descriptor_set, image);
        Ok(descriptor_set)
    }

    fn create_shader_module(device: &ash::Device, code: &[u8]) -> Result<vk::ShaderModule> {
        let mut cursor = Cursor::new(code);
        let spv = read_spv(&mut cursor).map_err(|err| format!("read_spv failed: {err:?}"))?;
        let info = vk::ShaderModuleCreateInfo::builder().code(&spv);
        unsafe { device.create_shader_module(&info, None) }.map_err(|err| format!("create_shader_module failed: {err:?}"))
    }

    fn upload_atlas(&mut self, ctx: &mut VulkanContext, atlas: &AtlasHandle) -> Result<()> {
        let mut width = 0;
        let mut height = 0;
        let mut data = Vec::new();
        atlas.apply_pixels(|w, h, pixels| {
            width = w;
            height = h;
            let bytes = unsafe { std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 4) };
            data.clear();
            data.extend_from_slice(bytes);
        });
        if width == 0 || height == 0 {
            return Ok(());
        }

        let width_u32 = u32::try_from(width).map_err(|_| "atlas width exceeds u32 range")?;
        let height_u32 = u32::try_from(height).map_err(|_| "atlas height exceeds u32 range")?;

        if self
            .atlas
            .as_ref()
            .map(|img| img.extent.width != width_u32 || img.extent.height != height_u32)
            .unwrap_or(true)
        {
            if let Some(mut old) = self.atlas.take() {
                old.destroy(&ctx.device);
            }
            self.atlas = Some(ctx.create_image_resource(width_u32, height_u32)?);
        }

        let staging = ctx.create_buffer(
            data.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        ctx.write_buffer(&staging, &data)?;

        if let Some(atlas_image) = self.atlas.as_mut() {
            ctx.copy_buffer_to_image(&staging, atlas_image)?;
        }
        if let Some(atlas_image) = self.atlas.as_ref() {
            self.update_descriptor(&ctx.device, self.descriptor_set, atlas_image);
        }

        let mut staging = staging;
        staging.destroy(&ctx.device);
        Ok(())
    }

    fn update_descriptor(&self, device: &ash::Device, descriptor_set: vk::DescriptorSet, atlas: &ImageResource) {
        let image_info = vk::DescriptorImageInfo::builder()
            .sampler(self.sampler)
            .image_view(atlas.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .build();
        let image_infos = [image_info];
        let writes = [vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build()];
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    fn ensure_vertex_buffer(&mut self, ctx: &VulkanContext, required: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.vertex_buffer.as_ref().map(|buf| buf.size);
        let needs_realloc = current_capacity.map(|cap| cap < required).unwrap_or(true);
        if needs_realloc {
            let new_capacity = Self::grow_capacity(current_capacity, required, Self::MIN_VERTEX_CAPACITY);
            if let Some(mut buffer) = self.vertex_buffer.take() {
                buffer.destroy(&ctx.device);
            }
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;
            self.vertex_buffer = Some(buffer);
            self.vertex_offset = 0;
        }
        Ok(())
    }

    fn ensure_staging_buffer(&mut self, ctx: &VulkanContext, frame: usize, required_total: vk::DeviceSize) -> Result<()> {
        if frame >= self.staging_buffers.len() {
            return Err(format!("invalid frame index for UI staging buffer: {}", frame));
        }
        let current_capacity = self.staging_buffers[frame].as_ref().map(|buf| buf.size());
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_STAGING_CAPACITY);
            if let Some(mut buffer) = self.staging_buffers[frame].take() {
                buffer.destroy(&ctx.device);
            }
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::TRANSFER_SRC,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let mapped = MappedBuffer::new(buffer, &ctx.device)?;
            self.staging_buffers[frame] = Some(mapped);
            self.staging_offsets[frame] = 0;
        }
        Ok(())
    }

    fn reset_frame_offsets(&mut self, frame: usize) {
        self.vertex_offset = 0;
        if let Some(offset) = self.staging_offsets.get_mut(frame) {
            *offset = 0;
        }
    }

    fn record(&mut self, ctx: &mut VulkanContext, command_buffer: vk::CommandBuffer, vertices: &[Vertex], width: u32, height: u32) -> Result<()> {
        if vertices.is_empty() {
            return Ok(());
        }
        if self.atlas.is_none() {
            return Ok(());
        }
        self.record_with_descriptor(ctx, command_buffer, vertices, width, height, self.descriptor_set, None)
    }

    fn record_custom(
        &mut self,
        ctx: &mut VulkanContext,
        command_buffer: vk::CommandBuffer,
        vertices: &[Vertex],
        width: u32,
        height: u32,
        descriptor_set: vk::DescriptorSet,
        area: Option<&CustomRenderArea>,
    ) -> Result<()> {
        self.record_with_descriptor(ctx, command_buffer, vertices, width, height, descriptor_set, area)
    }

    fn record_with_descriptor(
        &mut self,
        ctx: &mut VulkanContext,
        command_buffer: vk::CommandBuffer,
        vertices: &[Vertex],
        width: u32,
        height: u32,
        descriptor_set: vk::DescriptorSet,
        area: Option<&CustomRenderArea>,
    ) -> Result<()> {
        if vertices.is_empty() {
            return Ok(());
        }
        let frame = ctx.current_frame;
        let frame_staging_offset = match self.staging_offsets.get(frame).copied() {
            Some(offset) => offset,
            None => return Err(format!("invalid frame index for UI upload offset: {}", frame)),
        };
        let vertex_bytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr() as *const u8, vertices.len() * std::mem::size_of::<Vertex>()) };
        let copy_size = vertex_bytes.len() as u64;
        let dst_offset = self.vertex_offset;
        self.ensure_staging_buffer(ctx, frame, frame_staging_offset + copy_size)?;
        self.ensure_vertex_buffer(ctx, dst_offset + copy_size)?;
        let staging = self.staging_buffers.get(frame).and_then(|slot| slot.as_ref());
        if let (Some(staging), Some(buffer)) = (staging, self.vertex_buffer.as_ref()) {
            staging.write(frame_staging_offset, vertex_bytes)?;
            ctx.record_transfer_copy(
                staging.vk_buffer(),
                frame_staging_offset,
                buffer.buffer,
                dst_offset,
                copy_size,
                vk::AccessFlags::VERTEX_ATTRIBUTE_READ,
            )?;
            unsafe {
                ctx.device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
                ctx.device
                    .cmd_bind_descriptor_sets(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.pipeline_layout, 0, &[descriptor_set], &[]);
                ctx.device.cmd_bind_vertex_buffers(command_buffer, 0, &[buffer.buffer], &[dst_offset]);

                let viewport = ui_full_viewport(ctx.extent);
                ctx.device.cmd_set_viewport(command_buffer, 0, &[viewport]);

                let logical_clip = area.map(|a| a.clip).unwrap_or(rect(0, 0, width as i32, height as i32));
                let clip_rect = ctx.scale_rect(logical_clip);
                let clamped = clamp_rect_to_surface(clip_rect, ctx.extent.width, ctx.extent.height);
                let scissor = rect_to_vk(clamped, ctx.extent.width, ctx.extent.height);
                ctx.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                log_viewport_scissor("ui", logical_clip, &viewport, &scissor);
                vk_trace!(
                    "[microui-redux][vk-trace][ui] logical_clip={:?} scaled_clip={:?} viewport=({:.1},{:.1},{:.1},{:.1}) scissor=(offset=({}, {}), extent=({}, {}))",
                    logical_clip,
                    clip_rect,
                    viewport.x,
                    viewport.y,
                    viewport.width,
                    viewport.height,
                    scissor.offset.x,
                    scissor.offset.y,
                    scissor.extent.width,
                    scissor.extent.height
                );

                let ortho = Self::ortho_matrix(width as f32, height as f32);
                let bytes = Self::matrix_bytes(&ortho);
                vk_trace!(
                    "[microui-redux][vk-trace][ui] ortho_first_row=[{:.5}, {:.5}, {:.5}, {:.5}]",
                    ortho[0],
                    ortho[1],
                    ortho[2],
                    ortho[3]
                );
                ctx.device
                    .cmd_push_constants(command_buffer, self.pipeline_layout, vk::ShaderStageFlags::VERTEX, 0, bytes);
                ctx.device.cmd_draw(command_buffer, vertices.len() as u32, 1, 0, 0);
            }
            self.vertex_offset += copy_size;
            if let Some(offset) = self.staging_offsets.get_mut(frame) {
                *offset += copy_size;
            }
        }
        Ok(())
    }

    fn ortho_matrix(width: f32, height: f32) -> [f32; 16] {
        [
            2.0 / width,
            0.0,
            0.0,
            0.0,
            0.0,
            2.0 / height,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            -1.0,
            -1.0,
            0.0,
            1.0,
        ]
    }

    fn matrix_bytes(matrix: &[f32; 16]) -> &[u8] {
        unsafe { std::slice::from_raw_parts(matrix.as_ptr() as *const u8, mem::size_of::<[f32; 16]>()) }
    }
}

struct MeshResources {
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    vertex_buffer: Option<Buffer>,
    index_buffer: Option<Buffer>,
    vertex_staging: Option<MappedBuffer>,
    index_staging: Option<MappedBuffer>,
    vertex_offset: vk::DeviceSize,
    vertex_staging_offset: vk::DeviceSize,
    index_offset: vk::DeviceSize,
    index_staging_offset: vk::DeviceSize,
    retired_vertex_staging: Vec<MappedBuffer>,
    retired_index_staging: Vec<MappedBuffer>,
    depth_enabled: bool,
}

impl MeshResources {
    const MIN_VERTEX_CAPACITY: vk::DeviceSize = 1_u64 << 20;
    const MIN_INDEX_CAPACITY: vk::DeviceSize = 64_u64 << 10;
    const MIN_VERTEX_STAGING_CAPACITY: vk::DeviceSize = 64_u64 << 10;
    const MIN_INDEX_STAGING_CAPACITY: vk::DeviceSize = 32_u64 << 10;

    fn grow_capacity(current: Option<vk::DeviceSize>, required: vk::DeviceSize, min: vk::DeviceSize) -> vk::DeviceSize {
        if required == 0 {
            return min.max(1);
        }
        let mut size = current.unwrap_or(min).max(min).max(1);
        while size < required {
            size = match size.checked_mul(2) {
                Some(next) => next,
                None => return required,
            };
        }
        size
    }

    fn new(ctx: &VulkanContext) -> Result<Self> {
        let depth_enabled = true;
        let device = &ctx.device;
        let push_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size((std::mem::size_of::<f32>() * 32) as u32)
            .build();
        let layout_info = vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(std::slice::from_ref(&push_range));
        let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }.map_err(|err| format!("create_pipeline_layout failed: {err:?}"))?;

        let vert_module = Self::create_shader_module(device, MESH_VERT_SPV)?;
        let frag_module = Self::create_shader_module(device, MESH_FRAG_SPV)?;
        let entry = CString::new("main").unwrap();
        let stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_module)
                .name(&entry)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_module)
                .name(&entry)
                .build(),
        ];

        let binding = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(std::mem::size_of::<MeshVertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        let attributes = [
            vk::VertexInputAttributeDescription::builder()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0)
                .build(),
            vk::VertexInputAttributeDescription::builder()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(12)
                .build(),
            vk::VertexInputAttributeDescription::builder()
                .binding(0)
                .location(2)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(24)
                .build(),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(std::slice::from_ref(&binding))
            .vertex_attribute_descriptions(&attributes);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder().viewport_count(1).scissor_count(1);

        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0);

        let multisample = vk::PipelineMultisampleStateCreateInfo::builder().rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let depth_stencil = if depth_enabled {
            vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        } else {
            vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(false)
                .depth_write_enable(false)
        };

        let color_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B | vk::ColorComponentFlags::A)
            .blend_enable(false)
            .build();
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder().attachments(std::slice::from_ref(&color_attachment));
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&dynamic_states);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
            .render_pass(ctx.render_pass)
            .subpass(0);

        let pipeline_infos = [pipeline_info.build()];
        let pipeline = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_infos, None) }
            .map_err(|(_, err)| format!("create_graphics_pipelines failed: {err:?}"))?[0];

        unsafe {
            device.destroy_shader_module(vert_module, None);
            device.destroy_shader_module(frag_module, None);
        }

        Ok(Self {
            pipeline_layout,
            pipeline,
            vertex_buffer: None,
            index_buffer: None,
            vertex_staging: None,
            index_staging: None,
            vertex_offset: 0,
            vertex_staging_offset: 0,
            index_offset: 0,
            index_staging_offset: 0,
            retired_vertex_staging: Vec::new(),
            retired_index_staging: Vec::new(),
            depth_enabled,
        })
    }

    fn destroy(&mut self, device: &ash::Device) {
        if let Some(mut buffer) = self.vertex_buffer.take() {
            buffer.destroy(device);
        }
        if let Some(mut buffer) = self.index_buffer.take() {
            buffer.destroy(device);
        }
        if let Some(mut buffer) = self.vertex_staging.take() {
            buffer.destroy(device);
        }
        if let Some(mut buffer) = self.index_staging.take() {
            buffer.destroy(device);
        }
        for mut buffer in self.retired_vertex_staging.drain(..) {
            buffer.destroy(device);
        }
        for mut buffer in self.retired_index_staging.drain(..) {
            buffer.destroy(device);
        }
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
        }
        self.pipeline = vk::Pipeline::null();
        self.pipeline_layout = vk::PipelineLayout::null();
    }

    fn ensure_vertex_buffer(&mut self, ctx: &VulkanContext, required_total: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.vertex_buffer.as_ref().map(|buf| buf.size);
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_VERTEX_CAPACITY);
            if let Some(mut buffer) = self.vertex_buffer.take() {
                buffer.destroy(&ctx.device);
            }
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;
            self.vertex_buffer = Some(buffer);
            self.vertex_offset = 0;
        }
        Ok(())
    }

    fn ensure_index_buffer(&mut self, ctx: &VulkanContext, required_total: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.index_buffer.as_ref().map(|buf| buf.size);
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_INDEX_CAPACITY);
            if let Some(mut buffer) = self.index_buffer.take() {
                buffer.destroy(&ctx.device);
            }
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;
            self.index_buffer = Some(buffer);
            self.index_offset = 0;
        }
        Ok(())
    }

    fn ensure_vertex_staging_buffer(&mut self, ctx: &VulkanContext, required_total: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.vertex_staging.as_ref().map(|buf| buf.size());
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_VERTEX_STAGING_CAPACITY);
            if let Some(buffer) = self.vertex_staging.take() {
                self.retired_vertex_staging.push(buffer);
            }
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::TRANSFER_SRC,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let mapped = MappedBuffer::new(buffer, &ctx.device)?;
            self.vertex_staging = Some(mapped);
            self.vertex_staging_offset = 0;
        }
        Ok(())
    }

    fn ensure_index_staging_buffer(&mut self, ctx: &VulkanContext, required_total: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.index_staging.as_ref().map(|buf| buf.size());
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_INDEX_STAGING_CAPACITY);
            if let Some(buffer) = self.index_staging.take() {
                self.retired_index_staging.push(buffer);
            }
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::TRANSFER_SRC,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let mapped = MappedBuffer::new(buffer, &ctx.device)?;
            self.index_staging = Some(mapped);
            self.index_staging_offset = 0;
        }
        Ok(())
    }

    fn cleanup_retired_staging(&mut self, device: &ash::Device) {
        for mut buffer in self.retired_vertex_staging.drain(..) {
            buffer.destroy(device);
        }
        for mut buffer in self.retired_index_staging.drain(..) {
            buffer.destroy(device);
        }
    }

    fn reset_upload_state(&mut self, device: &ash::Device) {
        self.vertex_offset = 0;
        self.vertex_staging_offset = 0;
        self.index_offset = 0;
        self.index_staging_offset = 0;
        self.cleanup_retired_staging(device);
    }

    fn record(&mut self, ctx: &mut VulkanContext, command_buffer: vk::CommandBuffer, submission: &MeshSubmission, area: &CustomRenderArea) -> Result<()> {
        if submission.mesh.is_empty() {
            return Ok(());
        }

        let extent = ctx.extent;
        let clip_rect = scale_rect_to_surface(area.clip, ctx.logical_width, ctx.logical_height, extent.width, extent.height);
        let clip_rect = clamp_rect_to_surface(clip_rect, extent.width, extent.height);
        if clip_rect.width <= 0 || clip_rect.height <= 0 {
            return Ok(());
        }

        let vertex_bytes = unsafe {
            std::slice::from_raw_parts(
                submission.mesh.vertices().as_ptr() as *const u8,
                submission.mesh.vertices().len() * std::mem::size_of::<MeshVertex>(),
            )
        };
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                submission.mesh.indices().as_ptr() as *const u8,
                submission.mesh.indices().len() * std::mem::size_of::<u32>(),
            )
        };
        let vertex_copy_size = vertex_bytes.len() as u64;
        let index_copy_size = index_bytes.len() as u64;
        let vertex_dst_offset = self.vertex_offset;
        let index_dst_offset = self.index_offset;
        self.ensure_vertex_buffer(ctx, vertex_dst_offset + vertex_copy_size)?;
        self.ensure_index_buffer(ctx, index_dst_offset + index_copy_size)?;
        self.ensure_vertex_staging_buffer(ctx, self.vertex_staging_offset + vertex_copy_size)?;
        self.ensure_index_staging_buffer(ctx, self.index_staging_offset + index_copy_size)?;

        if let (Some(staging), Some(buffer)) = (self.vertex_staging.as_ref(), self.vertex_buffer.as_ref()) {
            staging.write(self.vertex_staging_offset, vertex_bytes)?;
            ctx.record_transfer_copy(
                staging.vk_buffer(),
                self.vertex_staging_offset,
                buffer.buffer,
                vertex_dst_offset,
                vertex_copy_size,
                vk::AccessFlags::VERTEX_ATTRIBUTE_READ,
            )?;
        }
        if let (Some(staging), Some(buffer)) = (self.index_staging.as_ref(), self.index_buffer.as_ref()) {
            staging.write(self.index_staging_offset, index_bytes)?;
            ctx.record_transfer_copy(
                staging.vk_buffer(),
                self.index_staging_offset,
                buffer.buffer,
                index_dst_offset,
                index_copy_size,
                vk::AccessFlags::INDEX_READ,
            )?;
        }

        self.vertex_offset += vertex_copy_size;
        self.vertex_staging_offset += vertex_copy_size;
        self.index_offset += index_copy_size;
        self.index_staging_offset += index_copy_size;

        let viewport_rect = scale_rect_to_surface(area.rect, ctx.logical_width, ctx.logical_height, extent.width, extent.height);
        let viewport = match mesh_viewport_from_rect(viewport_rect, extent.width, extent.height) {
            Some(vp) => vp,
            None => return Ok(()),
        };
        let scissor = rect_to_vk(clip_rect, extent.width, extent.height);
        vk_trace!(
            "[microui-redux][vk-trace][mesh] area.rect={:?} area.clip={:?} viewport=({:.1},{:.1},{:.1},{:.1}) scissor=(offset=({}, {}), extent=({}, {}))",
            area.rect,
            area.clip,
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            scissor.offset.x,
            scissor.offset.y,
            scissor.extent.width,
            scissor.extent.height
        );

        unsafe {
            ctx.device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            if let Some(buffer) = self.vertex_buffer.as_ref() {
                ctx.device.cmd_bind_vertex_buffers(command_buffer, 0, &[buffer.buffer], &[vertex_dst_offset]);
            }
            if let Some(buffer) = self.index_buffer.as_ref() {
                ctx.device
                    .cmd_bind_index_buffer(command_buffer, buffer.buffer, index_dst_offset, vk::IndexType::UINT32);
            }
            ctx.device.cmd_set_viewport(command_buffer, 0, &[viewport]);
            ctx.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
            log_viewport_scissor("mesh", area.rect, &viewport, &scissor);
        }

        let mut push_data = [0.0f32; 32];
        unsafe {
            let pvm_vk = Mat4f::mul_matrix_matrix(&opengl_to_vulkan_clip_matrix(), &submission.pvm);
            let pvm_slice = std::slice::from_raw_parts(pvm_vk.col.as_ptr() as *const f32, 16);
            let view_slice = std::slice::from_raw_parts(submission.view_model.col.as_ptr() as *const f32, 16);
            push_data[..16].copy_from_slice(pvm_slice);
            push_data[16..].copy_from_slice(view_slice);
        }
        let push_bytes = unsafe { std::slice::from_raw_parts(push_data.as_ptr() as *const u8, push_data.len() * std::mem::size_of::<f32>()) };

        unsafe {
            ctx.device
                .cmd_push_constants(command_buffer, self.pipeline_layout, vk::ShaderStageFlags::VERTEX, 0, push_bytes);
            ctx.device.cmd_draw_indexed(command_buffer, submission.mesh.indices().len() as u32, 1, 0, 0, 0);
        }

        Ok(())
    }

    fn create_shader_module(device: &ash::Device, code: &[u8]) -> Result<vk::ShaderModule> {
        let mut cursor = Cursor::new(code);
        let spv = read_spv(&mut cursor).map_err(|err| format!("read_spv failed: {err:?}"))?;
        let info = vk::ShaderModuleCreateInfo::builder().code(&spv);
        unsafe { device.create_shader_module(&info, None) }.map_err(|err| format!("create_shader_module failed: {err:?}"))
    }
}
pub(crate) struct VulkanContext {
    entry: Entry,
    instance: ash::Instance,
    surface_loader: Surface,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    queue_indices: QueueFamilyIndices,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    swapchain_loader: Swapchain,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    swapchain_format: vk::Format,
    depth_format: vk::Format,
    extent: vk::Extent2D,
    logical_width: u32,
    logical_height: u32,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    transfer_command_buffers: Vec<vk::CommandBuffer>,
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    transfer_complete_semaphores: Vec<vk::Semaphore>,
    in_flight_fences: Vec<vk::Fence>,
    transfer_recording: Vec<bool>,
    transfer_has_work: Vec<bool>,
    current_frame: usize,
    max_frames_in_flight: usize,
    ui: Option<UiResources>,
    depth_images: Vec<ImageResource>,
    mesh: Option<MeshResources>,
    swapchain_generation: u64,
}

impl VulkanContext {
    fn new(window: &Window, width: u32, height: u32) -> Result<Self> {
        let entry = Entry::linked();
        let app_name = CString::new("microui-redux-examples").unwrap();
        let engine_name = CString::new("microui-redux").unwrap();

        let display_handle = window.display_handle().map_err(|err| format!("failed to get display handle: {err:?}"))?;
        let window_handle = window.window_handle().map_err(|err| format!("failed to get window handle: {err:?}"))?;
        let raw_display_handle = display_handle.into();
        let raw_window_handle = window_handle.into();

        let app_info = vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .engine_name(&engine_name)
            .api_version(vk::API_VERSION_1_1)
            .build();

        let mut extension_names = ash_window_handle::enumerate_required_extensions(raw_display_handle)
            .map_err(|err| format!("enumerate_required_extensions failed: {err:?}"))?
            .to_vec();
        let surface_extension = khr::surface::NAME.as_ptr();
        if !extension_names.iter().any(|ext| *ext == surface_extension) {
            extension_names.push(surface_extension);
        }

        let instance_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .build();
        let instance = unsafe { entry.create_instance(&instance_info, None) }.map_err(|err| format!("create_instance failed: {err:?}"))?;

        let surface = unsafe { ash_window_handle::create_surface(&entry, &instance, raw_display_handle, raw_window_handle, None) }
            .map_err(|err| format!("create_surface failed: {err:?}"))?;
        let surface_loader = Surface::new(&entry, &instance);

        let (physical_device, queue_indices) = Self::select_physical_device(&instance, &surface_loader, surface)?;

        let device = Self::create_logical_device(&instance, physical_device, &queue_indices)?;
        let graphics_queue = unsafe { device.get_device_queue(queue_indices.graphics_family, 0) };
        let present_queue = unsafe { device.get_device_queue(queue_indices.present_family, 0) };

        let swapchain_loader = Swapchain::new(&instance, &device);
        let depth_format = Self::find_depth_format(&instance, physical_device)?;

        let mut ctx = Self {
            entry,
            instance,
            surface_loader,
            surface,
            physical_device,
            device,
            queue_indices,
            graphics_queue,
            present_queue,
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_images: Vec::new(),
            swapchain_image_views: Vec::new(),
            swapchain_format: vk::Format::UNDEFINED,
            depth_format,
            extent: vk::Extent2D { width, height },
            render_pass: vk::RenderPass::null(),
            framebuffers: Vec::new(),
            command_pool: vk::CommandPool::null(),
            command_buffers: Vec::new(),
            transfer_command_buffers: Vec::new(),
            image_available_semaphores: Vec::new(),
            render_finished_semaphores: Vec::new(),
            transfer_complete_semaphores: Vec::new(),
            in_flight_fences: Vec::new(),
            transfer_recording: Vec::new(),
            transfer_has_work: Vec::new(),
            current_frame: 0,
            max_frames_in_flight: 2,
            ui: None,
            depth_images: Vec::new(),
            mesh: None,
            logical_width: width,
            logical_height: height,
            swapchain_generation: 0,
        };

        ctx.command_pool = ctx.create_command_pool()?;
        ctx.recreate_swapchain(width, height)?;
        ctx.create_sync_objects()?;
        ctx.allocate_transfer_command_buffers()?;
        ctx.ui = Some(UiResources::new(&ctx)?);

        Ok(ctx)
    }

    fn select_physical_device(instance: &ash::Instance, surface_loader: &Surface, surface: vk::SurfaceKHR) -> Result<(vk::PhysicalDevice, QueueFamilyIndices)> {
        let devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| format!("enumerate_physical_devices failed: {err:?}"))?;

        for device in devices {
            if let Some(indices) = Self::find_queue_families(instance, surface_loader, surface, device) {
                return Ok((device, indices));
            }
        }

        Err("no suitable Vulkan physical device found".into())
    }

    fn find_queue_families(
        instance: &ash::Instance,
        surface_loader: &Surface,
        surface: vk::SurfaceKHR,
        device: vk::PhysicalDevice,
    ) -> Option<QueueFamilyIndices> {
        let queue_families = unsafe { instance.get_physical_device_queue_family_properties(device) };
        let mut graphics_family = None;
        let mut present_family = None;

        for (index, family) in queue_families.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                graphics_family = Some(index as u32);
            }

            let present_support = unsafe {
                surface_loader
                    .get_physical_device_surface_support(device, index as u32, surface)
                    .unwrap_or(false)
            };

            if present_support {
                present_family = Some(index as u32);
            }

            if graphics_family.is_some() && present_family.is_some() {
                break;
            }
        }

        match (graphics_family, present_family) {
            (Some(graphics_family), Some(present_family)) => Some(QueueFamilyIndices { graphics_family, present_family }),
            _ => None,
        }
    }

    fn create_logical_device(instance: &ash::Instance, physical_device: vk::PhysicalDevice, indices: &QueueFamilyIndices) -> Result<ash::Device> {
        let unique_indices = if indices.graphics_family == indices.present_family {
            vec![indices.graphics_family]
        } else {
            vec![indices.graphics_family, indices.present_family]
        };

        let queue_priority = 1.0f32;
        let queue_infos: Vec<_> = unique_indices
            .iter()
            .map(|index| {
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(*index)
                    .queue_priorities(std::slice::from_ref(&queue_priority))
                    .build()
            })
            .collect();

        let device_extensions = [khr::swapchain::NAME.as_ptr()];

        let device_features = vk::PhysicalDeviceFeatures::builder().build();
        let device_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_infos)
            .enabled_extension_names(&device_extensions)
            .enabled_features(&device_features)
            .build();

        unsafe { instance.create_device(physical_device, &device_info, None) }.map_err(|err| format!("create_device failed: {err:?}"))
    }

    fn recreate_swapchain(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }

        unsafe {
            self.device.device_wait_idle().map_err(|err| format!("device_wait_idle failed: {err:?}"))?;
        }

        self.cleanup_swapchain();
        self.create_swapchain(width, height)?;
        self.create_image_views()?;
        self.create_depth_images()?;
        self.render_pass = self.create_render_pass()?;
        self.framebuffers = self.create_framebuffers()?;
        self.allocate_command_buffers()?;
        if let Some(mut ui) = self.ui.take() {
            ui.destroy(&self.device);
        }
        self.ui = Some(UiResources::new(self)?);
        self.swapchain_generation = self.swapchain_generation.wrapping_add(1);

        Ok(())
    }

    fn create_swapchain(&mut self, width: u32, height: u32) -> Result<()> {
        let surface_caps = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, self.surface)
                .map_err(|err| format!("get_surface_capabilities failed: {err:?}"))?
        };

        let formats = unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(self.physical_device, self.surface)
                .map_err(|err| format!("get_surface_formats failed: {err:?}"))?
        };

        let present_modes = unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(self.physical_device, self.surface)
                .map_err(|err| format!("get_surface_present_modes failed: {err:?}"))?
        };

        let surface_format = Self::choose_surface_format(&formats);
        let present_mode = Self::choose_present_mode(&present_modes);
        let extent = Self::choose_extent(&surface_caps, width, height);
        let mut image_count = (surface_caps.min_image_count + 1).max(2);
        if surface_caps.max_image_count > 0 {
            image_count = image_count.min(surface_caps.max_image_count);
        }

        let queue_family_indices = [self.queue_indices.graphics_family, self.queue_indices.present_family];

        let mut create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .pre_transform(surface_caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE);

        if self.queue_indices.graphics_family != self.queue_indices.present_family {
            create_info = create_info
                .image_sharing_mode(vk::SharingMode::CONCURRENT)
                .queue_family_indices(&queue_family_indices);
        }

        self.swapchain = unsafe { self.swapchain_loader.create_swapchain(&create_info, None) }.map_err(|err| format!("create_swapchain failed: {err:?}"))?;
        self.swapchain_images =
            unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain) }.map_err(|err| format!("get_swapchain_images failed: {err:?}"))?;
        self.swapchain_format = surface_format.format;
        self.extent = extent;

        Ok(())
    }

    fn create_image_views(&mut self) -> Result<()> {
        self.swapchain_image_views = self
            .swapchain_images
            .iter()
            .map(|&image| {
                let components = vk::ComponentMapping {
                    r: vk::ComponentSwizzle::IDENTITY,
                    g: vk::ComponentSwizzle::IDENTITY,
                    b: vk::ComponentSwizzle::IDENTITY,
                    a: vk::ComponentSwizzle::IDENTITY,
                };
                let subresource_range = vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                };
                let view_info = vk::ImageViewCreateInfo::builder()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(self.swapchain_format)
                    .components(components)
                    .subresource_range(subresource_range);
                unsafe { self.device.create_image_view(&view_info, None) }.map_err(|err| format!("create_image_view failed: {err:?}"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(())
    }

    fn create_render_pass(&self) -> Result<vk::RenderPass> {
        let color_attachment = vk::AttachmentDescription::builder()
            .format(self.swapchain_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build();

        let color_attachment_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };

        let depth_attachment = vk::AttachmentDescription::builder()
            .format(self.depth_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .build();
        let depth_attachment_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };

        let subpass = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&color_attachment_ref))
            .depth_stencil_attachment(&depth_attachment_ref)
            .build();

        let dependency = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
            .build();

        let attachments = [color_attachment, depth_attachment];
        let subpasses = [subpass];
        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(std::slice::from_ref(&dependency));

        unsafe { self.device.create_render_pass(&render_pass_info, None) }.map_err(|err| format!("create_render_pass failed: {err:?}"))
    }

    fn create_framebuffers(&self) -> Result<Vec<vk::Framebuffer>> {
        self.swapchain_image_views
            .iter()
            .enumerate()
            .map(|(index, &view)| {
                let depth_view = self.depth_images.get(index).map(|image| image.view).unwrap_or(vk::ImageView::null());
                let attachments = [view, depth_view];
                let framebuffer_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(self.render_pass)
                    .attachments(&attachments)
                    .width(self.extent.width)
                    .height(self.extent.height)
                    .layers(1);

                unsafe { self.device.create_framebuffer(&framebuffer_info, None) }.map_err(|err| format!("create_framebuffer failed: {err:?}"))
            })
            .collect()
    }

    fn create_command_pool(&self) -> Result<vk::CommandPool> {
        let pool_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(self.queue_indices.graphics_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        unsafe { self.device.create_command_pool(&pool_info, None) }.map_err(|err| format!("create_command_pool failed: {err:?}"))
    }

    fn allocate_command_buffers(&mut self) -> Result<()> {
        if !self.command_buffers.is_empty() {
            unsafe {
                self.device.free_command_buffers(self.command_pool, &self.command_buffers);
            }
        }

        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(self.framebuffers.len() as u32);
        self.command_buffers =
            unsafe { self.device.allocate_command_buffers(&alloc_info) }.map_err(|err| format!("allocate_command_buffers failed: {err:?}"))?;
        Ok(())
    }

    fn allocate_transfer_command_buffers(&mut self) -> Result<()> {
        if !self.transfer_command_buffers.is_empty() {
            unsafe {
                self.device.free_command_buffers(self.command_pool, &self.transfer_command_buffers);
            }
        }

        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(self.max_frames_in_flight as u32);
        self.transfer_command_buffers =
            unsafe { self.device.allocate_command_buffers(&alloc_info) }.map_err(|err| format!("allocate_command_buffers (transfer) failed: {err:?}"))?;
        self.transfer_recording = vec![false; self.max_frames_in_flight];
        self.transfer_has_work = vec![false; self.max_frames_in_flight];
        Ok(())
    }

    fn create_sync_objects(&mut self) -> Result<()> {
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED).build();

        self.image_available_semaphores.clear();
        self.render_finished_semaphores.clear();
        self.in_flight_fences.clear();
        self.transfer_complete_semaphores.clear();

        for _ in 0..self.max_frames_in_flight {
            unsafe {
                let image_available = self
                    .device
                    .create_semaphore(&semaphore_info, None)
                    .map_err(|err| format!("create_semaphore failed: {err:?}"))?;
                let render_finished = self
                    .device
                    .create_semaphore(&semaphore_info, None)
                    .map_err(|err| format!("create_semaphore failed: {err:?}"))?;
                let transfer_complete = self
                    .device
                    .create_semaphore(&semaphore_info, None)
                    .map_err(|err| format!("create_semaphore failed: {err:?}"))?;
                let fence = self
                    .device
                    .create_fence(&fence_info, None)
                    .map_err(|err| format!("create_fence failed: {err:?}"))?;

                self.image_available_semaphores.push(image_available);
                self.render_finished_semaphores.push(render_finished);
                self.transfer_complete_semaphores.push(transfer_complete);
                self.in_flight_fences.push(fence);
            }
        }

        Ok(())
    }

    fn cleanup_swapchain(&mut self) {
        unsafe {
            if let Some(mut mesh) = self.mesh.take() {
                mesh.destroy(&self.device);
            }
            for &framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            self.framebuffers.clear();

            if !self.command_buffers.is_empty() {
                self.device.free_command_buffers(self.command_pool, &self.command_buffers);
                self.command_buffers.clear();
            }

            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }
            self.swapchain_image_views.clear();
            for mut depth in self.depth_images.drain(..) {
                depth.destroy(&self.device);
            }

            if self.render_pass != vk::RenderPass::null() {
                self.device.destroy_render_pass(self.render_pass, None);
                self.render_pass = vk::RenderPass::null();
            }

            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }
        }
    }

    fn draw_frame(
        &mut self,
        clear_value: vk::ClearValue,
        vertices: &[Vertex],
        width: u32,
        height: u32,
        _frame_index: u64,
        commands: &mut Vec<FrameCommand>,
    ) -> Result<()> {
        self.logical_width = width.max(1);
        self.logical_height = height.max(1);
        let fence = self.in_flight_fences[self.current_frame];
        unsafe {
            self.device
                .wait_for_fences(&[fence], true, u64::MAX)
                .map_err(|err| format!("wait_for_fences failed: {err:?}"))?;
        }

        self.reset_transfer_state(self.current_frame)?;
        self.reset_ui_offset();
        if let Some(ref mut mesh) = self.mesh {
            mesh.reset_upload_state(&self.device);
        }

        let mut swapchain_needs_recreate = false;

        let (image_index, suboptimal) = match unsafe {
            self.swapchain_loader
                .acquire_next_image(self.swapchain, u64::MAX, self.image_available_semaphores[self.current_frame], vk::Fence::null())
        } {
            Ok((index, suboptimal)) => (index, suboptimal),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate_swapchain(width, height)?;
                return Ok(());
            }
            Err(vk::Result::SUBOPTIMAL_KHR) => {
                self.recreate_swapchain(width, height)?;
                return Ok(());
            }
            Err(err) => return Err(format!("acquire_next_image failed: {err:?}")),
        };
        if suboptimal {
            swapchain_needs_recreate = true;
        }

        unsafe {
            self.device.reset_fences(&[fence]).map_err(|err| format!("reset_fences failed: {err:?}"))?;
        }

        let command_buffer = self.command_buffers[image_index as usize];
        self.record_command_buffer(command_buffer, image_index, clear_value, vertices, width, height, _frame_index, commands)?;
        let transfer_semaphore = self.submit_transfer_commands()?;

        let mut wait_semaphores = vec![self.image_available_semaphores[self.current_frame]];
        let mut wait_stages = vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        if let Some(semaphore) = transfer_semaphore {
            wait_semaphores.push(semaphore);
            wait_stages.push(vk::PipelineStageFlags::VERTEX_INPUT);
        }
        let signal_semaphores = [self.render_finished_semaphores[self.current_frame]];

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(std::slice::from_ref(&command_buffer))
            .signal_semaphores(&signal_semaphores);
        let submit_infos = [submit_info.build()];

        unsafe {
            self.device
                .queue_submit(self.graphics_queue, &submit_infos, fence)
                .map_err(|err| format!("queue_submit failed: {err:?}"))?;
        }

        let swapchains = [self.swapchain];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let present_info = present_info.build();
        let present_result = unsafe { self.swapchain_loader.queue_present(self.present_queue, &present_info) };
        match present_result {
            Ok(present_suboptimal) => {
                if present_suboptimal {
                    swapchain_needs_recreate = true;
                }
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate_swapchain(width, height)?;
                self.current_frame = (self.current_frame + 1) % self.max_frames_in_flight;
                return Ok(());
            }
            Err(vk::Result::SUBOPTIMAL_KHR) => {
                self.recreate_swapchain(width, height)?;
                self.current_frame = (self.current_frame + 1) % self.max_frames_in_flight;
                return Ok(());
            }
            Err(err) => return Err(format!("queue_present failed: {err:?}")),
        }

        if swapchain_needs_recreate {
            self.recreate_swapchain(width, height)?;
        }

        self.current_frame = (self.current_frame + 1) % self.max_frames_in_flight;

        Ok(())
    }

    fn record_command_buffer(
        &mut self,
        command_buffer: vk::CommandBuffer,
        image_index: u32,
        clear_value: vk::ClearValue,
        vertices: &[Vertex],
        width: u32,
        height: u32,
        _frame_index: u64,
        commands: &mut Vec<FrameCommand>,
    ) -> Result<()> {
        let begin_info = vk::CommandBufferBeginInfo::builder();
        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(|err| format!("begin_command_buffer failed: {err:?}"))?;
        }

        let depth_clear = vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
        };
        let clear_values = [clear_value, depth_clear];
        let render_pass_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index as usize])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            })
            .clear_values(&clear_values);

        unsafe {
            self.device
                .cmd_begin_render_pass(command_buffer, &render_pass_info, vk::SubpassContents::INLINE);
        }

        let frame_width = width.max(1);
        let frame_height = height.max(1);
        let mut cursor = 0;
        let mut ui = self.ui.take();

        for command in commands.drain(..) {
            match command {
                FrameCommand::DrawTo(end_index) => {
                    if let Some(ref mut ui) = ui {
                        let end = end_index.min(vertices.len());
                        if end <= cursor {
                            continue;
                        }
                        ui.record(self, command_buffer, &vertices[cursor..end], frame_width, frame_height)?;
                        cursor = end;
                    }
                }
                FrameCommand::Custom(mut job) => {
                    if let Some(ui_resources) = ui.take() {
                        self.ui = Some(ui_resources);
                    }
                    job.callback.record(self, command_buffer, self.extent, &job.area);
                    ui = self.ui.take();
                }
            }
        }

        if let Some(mut ui) = ui {
            if cursor < vertices.len() {
                ui.record(self, command_buffer, &vertices[cursor..], frame_width, frame_height)?;
            }
            self.ui = Some(ui);
        } else if self.ui.is_none() {
            // If no UI draws were recorded this frame, ensure we keep the context empty.
        }

        unsafe {
            self.device.cmd_end_render_pass(command_buffer);
            self.device
                .end_command_buffer(command_buffer)
                .map_err(|err| format!("end_command_buffer failed: {err:?}"))?;
        }

        Ok(())
    }

    fn choose_surface_format(available_formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
        available_formats
            .iter()
            .cloned()
            .find(|format| format.format == vk::Format::B8G8R8A8_UNORM)
            .unwrap_or_else(|| available_formats[0])
    }

    fn find_depth_format(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> Result<vk::Format> {
        let candidates = [vk::Format::D32_SFLOAT, vk::Format::D32_SFLOAT_S8_UINT, vk::Format::D24_UNORM_S8_UINT];
        for &format in &candidates {
            let props = unsafe { instance.get_physical_device_format_properties(physical_device, format) };
            if props.optimal_tiling_features.contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT) {
                return Ok(format);
            }
        }
        Err("no supported depth format found".into())
    }

    fn choose_present_mode(available_present_modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
        if available_present_modes.iter().any(|&mode| mode == vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        }
    }

    fn choose_extent(capabilities: &vk::SurfaceCapabilitiesKHR, width: u32, height: u32) -> vk::Extent2D {
        if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
        } else {
            vk::Extent2D {
                width: width.clamp(capabilities.min_image_extent.width, capabilities.max_image_extent.width),
                height: height.clamp(capabilities.min_image_extent.height, capabilities.max_image_extent.height),
            }
        }
    }

    fn extent(&self) -> vk::Extent2D {
        self.extent
    }
    fn swapchain_generation(&self) -> u64 {
        self.swapchain_generation
    }
    fn ui_has_atlas(&self) -> bool {
        self.ui.as_ref().map(|ui| ui.atlas.is_some()).unwrap_or(false)
    }

    fn upload_atlas(&mut self, atlas: &AtlasHandle) -> Result<()> {
        if let Some(mut ui) = self.ui.take() {
            let result = ui.upload_atlas(self, atlas);
            self.ui = Some(ui);
            result
        } else {
            Ok(())
        }
    }

    fn ui_descriptor_set(&self) -> Option<vk::DescriptorSet> {
        self.ui.as_ref().map(|ui| ui.descriptor_set)
    }

    fn draw_custom_vertices(
        &mut self,
        command_buffer: vk::CommandBuffer,
        vertices: &[Vertex],
        area: &CustomRenderArea,
        descriptor_set: vk::DescriptorSet,
    ) -> Result<()> {
        let mut ui = match self.ui.take() {
            Some(ui) => ui,
            None => return Ok(()),
        };
        let result = ui.record_custom(
            self,
            command_buffer,
            vertices,
            self.logical_width,
            self.logical_height,
            descriptor_set,
            Some(area),
        );
        self.ui = Some(ui);
        result
    }

    fn reset_ui_offset(&mut self) {
        if let Some(ref mut ui) = self.ui {
            ui.reset_frame_offsets(self.current_frame);
        }
    }

    fn draw_vertices_with_descriptor(
        &mut self,
        command_buffer: vk::CommandBuffer,
        vertices: &[Vertex],
        width: u32,
        height: u32,
        descriptor_set: vk::DescriptorSet,
        area: Option<&CustomRenderArea>,
    ) -> Result<()> {
        let mut ui = match self.ui.take() {
            Some(ui) => ui,
            None => return Ok(()),
        };
        let result = ui.record_with_descriptor(self, command_buffer, vertices, width, height, descriptor_set, area);
        self.ui = Some(ui);
        result
    }

    fn record_mesh(&mut self, command_buffer: vk::CommandBuffer, submission: &MeshSubmission, area: &CustomRenderArea) -> Result<()> {
        let mut resources = match self.mesh.take() {
            Some(resources) => resources,
            None => MeshResources::new(self)?,
        };
        let result = resources.record(self, command_buffer, submission, area);
        self.mesh = Some(resources);
        result
    }

    fn create_buffer(&self, size: vk::DeviceSize, usage: vk::BufferUsageFlags, properties: vk::MemoryPropertyFlags) -> Result<Buffer> {
        let info = vk::BufferCreateInfo::builder().size(size).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { self.device.create_buffer(&info, None) }.map_err(|err| format!("create_buffer failed: {err:?}"))?;
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let memory_type = self.find_memory_type(requirements.memory_type_bits, properties)?;
        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = unsafe { self.device.allocate_memory(&alloc_info, None) }.map_err(|err| format!("allocate_memory failed: {err:?}"))?;
        unsafe { self.device.bind_buffer_memory(buffer, memory, 0) }.map_err(|err| format!("bind_buffer_memory failed: {err:?}"))?;
        Ok(Buffer { buffer, memory, size })
    }

    fn write_buffer(&self, buffer: &Buffer, data: &[u8]) -> Result<()> {
        self.write_buffer_offset(buffer, 0, data)
    }

    fn write_buffer_offset(&self, buffer: &Buffer, offset: vk::DeviceSize, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        unsafe {
            let ptr = self
                .device
                .map_memory(buffer.memory, offset, data.len() as u64, vk::MemoryMapFlags::empty())
                .map_err(|err| format!("map_memory failed: {err:?}"))?;
            ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
            self.device.unmap_memory(buffer.memory);
        }
        Ok(())
    }

    fn create_image_resource(&self, width: u32, height: u32) -> Result<ImageResource> {
        let format = vk::Format::R8G8B8A8_UNORM;
        let extent3d = vk::Extent3D { width, height, depth: 1 };
        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(extent3d)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { self.device.create_image(&image_info, None) }.map_err(|err| format!("create_image failed: {err:?}"))?;
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let memory_type = self.find_memory_type(requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        let alloc = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = unsafe { self.device.allocate_memory(&alloc, None) }.map_err(|err| format!("allocate_memory failed: {err:?}"))?;
        unsafe { self.device.bind_image_memory(image, memory, 0) }.map_err(|err| format!("bind_image_memory failed: {err:?}"))?;

        let view_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1)
                    .build(),
            );
        let view = unsafe { self.device.create_image_view(&view_info, None) }.map_err(|err| format!("create_image_view failed: {err:?}"))?;

        Ok(ImageResource {
            image,
            memory,
            view,
            extent: vk::Extent2D { width, height },
            format,
            layout: vk::ImageLayout::UNDEFINED,
        })
    }

    fn create_depth_images(&mut self) -> Result<()> {
        self.depth_images.clear();
        let mut attachments = Vec::with_capacity(self.swapchain_images.len());
        for _ in &self.swapchain_images {
            attachments.push(self.create_depth_attachment(self.extent)?);
        }
        self.depth_images = attachments;
        Ok(())
    }

    fn create_depth_attachment(&self, extent: vk::Extent2D) -> Result<ImageResource> {
        let format = self.depth_format;
        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { self.device.create_image(&image_info, None) }.map_err(|err| format!("create_image failed: {err:?}"))?;
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let memory_type = self.find_memory_type(requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        let alloc = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = unsafe { self.device.allocate_memory(&alloc, None) }.map_err(|err| format!("allocate_memory failed: {err:?}"))?;
        unsafe { self.device.bind_image_memory(image, memory, 0) }.map_err(|err| format!("bind_image_memory failed: {err:?}"))?;

        let aspect = if Self::has_stencil_component(format) {
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        } else {
            vk::ImageAspectFlags::DEPTH
        };

        self.single_time_commands(|cmd| {
            self.transition_image_layout(
                cmd,
                image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                aspect,
            );
        })?;

        let view_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange::builder().aspect_mask(aspect).level_count(1).layer_count(1).build());
        let view = unsafe { self.device.create_image_view(&view_info, None) }.map_err(|err| format!("create_image_view failed: {err:?}"))?;

        Ok(ImageResource {
            image,
            memory,
            view,
            extent,
            format,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        })
    }

    fn has_stencil_component(format: vk::Format) -> bool {
        matches!(format, vk::Format::D32_SFLOAT_S8_UINT | vk::Format::D24_UNORM_S8_UINT)
    }

    fn copy_buffer_to_image(&self, buffer: &Buffer, image: &mut ImageResource) -> Result<()> {
        self.single_time_commands(|cmd| {
            self.transition_image_layout(
                cmd,
                image.image,
                image.layout,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
            );
            let region = vk::BufferImageCopy::builder()
                .image_subresource(
                    vk::ImageSubresourceLayers::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1)
                        .build(),
                )
                .image_extent(vk::Extent3D {
                    width: image.extent.width,
                    height: image.extent.height,
                    depth: 1,
                })
                .build();
            let regions = [region];
            unsafe {
                self.device
                    .cmd_copy_buffer_to_image(cmd, buffer.buffer, image.image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &regions);
            }
            self.transition_image_layout(
                cmd,
                image.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageAspectFlags::COLOR,
            );
            image.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        })
    }

    fn copy_buffer(&self, command_buffer: vk::CommandBuffer, src: &Buffer, dst: &Buffer, size: u64) {
        let regions = [vk::BufferCopy { src_offset: 0, dst_offset: 0, size }];
        unsafe {
            self.device.cmd_copy_buffer(command_buffer, src.buffer, dst.buffer, &regions);
        }
    }

    fn copy_buffer_with_offset(&self, command_buffer: vk::CommandBuffer, src: &Buffer, dst: &Buffer, dst_offset: u64, size: u64) {
        let regions = [vk::BufferCopy { src_offset: 0, dst_offset, size }];
        unsafe {
            self.device.cmd_copy_buffer(command_buffer, src.buffer, dst.buffer, &regions);
        }
    }

    fn begin_transfer_command_buffer(&mut self) -> Result<vk::CommandBuffer> {
        let frame = self.current_frame;
        if self.transfer_command_buffers.is_empty() {
            return Err("transfer command buffers not allocated".into());
        }
        let command_buffer = self.transfer_command_buffers[frame];
        if !self.transfer_recording.get(frame).copied().unwrap_or(false) {
            let begin_info = vk::CommandBufferBeginInfo::builder().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            unsafe {
                self.device
                    .begin_command_buffer(command_buffer, &begin_info)
                    .map_err(|err| format!("begin_command_buffer (transfer) failed: {err:?}"))?;
            }
            if let Some(state) = self.transfer_recording.get_mut(frame) {
                *state = true;
            }
        }
        Ok(command_buffer)
    }

    fn record_transfer_copy(
        &mut self,
        src: vk::Buffer,
        src_offset: vk::DeviceSize,
        dst: vk::Buffer,
        dst_offset: vk::DeviceSize,
        size: vk::DeviceSize,
        dst_access: vk::AccessFlags,
    ) -> Result<()> {
        if size == 0 {
            return Ok(());
        }
        let command_buffer = self.begin_transfer_command_buffer()?;
        let regions = [vk::BufferCopy { src_offset, dst_offset, size }];
        unsafe {
            self.device.cmd_copy_buffer(command_buffer, src, dst, &regions);
        }
        self.buffer_barrier_transfer(command_buffer, dst, dst_offset, size, dst_access);
        if let Some(flag) = self.transfer_has_work.get_mut(self.current_frame) {
            *flag = true;
        }
        Ok(())
    }

    fn end_transfer_recording_if_needed(&mut self, frame: usize) -> Result<()> {
        if self.transfer_recording.get(frame).copied().unwrap_or(false) {
            let command_buffer = self.transfer_command_buffers[frame];
            unsafe {
                self.device
                    .end_command_buffer(command_buffer)
                    .map_err(|err| format!("end_command_buffer (transfer) failed: {err:?}"))?;
            }
            self.transfer_recording[frame] = false;
        }
        Ok(())
    }

    fn submit_transfer_commands(&mut self) -> Result<Option<vk::Semaphore>> {
        let frame = self.current_frame;
        let has_work = self.transfer_has_work.get(frame).copied().unwrap_or(false);
        if !has_work {
            self.end_transfer_recording_if_needed(frame)?;
            return Ok(None);
        }
        self.end_transfer_recording_if_needed(frame)?;
        let command_buffer = self.transfer_command_buffers[frame];
        let semaphore = self.transfer_complete_semaphores.get(frame).copied().ok_or("missing transfer semaphore")?;
        let command_buffers = [command_buffer];
        let signal = [semaphore];
        let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers).signal_semaphores(&signal).build();
        unsafe {
            self.device
                .queue_submit(self.graphics_queue, &[submit_info], vk::Fence::null())
                .map_err(|err| format!("queue_submit (transfer) failed: {err:?}"))?;
        }
        self.transfer_has_work[frame] = false;
        Ok(Some(semaphore))
    }

    fn reset_transfer_state(&mut self, frame: usize) -> Result<()> {
        if self.transfer_command_buffers.is_empty() {
            return Ok(());
        }
        let command_buffer = self.transfer_command_buffers[frame];
        unsafe {
            self.device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|err| format!("reset_command_buffer (transfer) failed: {err:?}"))?;
        }
        if let Some(flag) = self.transfer_recording.get_mut(frame) {
            *flag = false;
        }
        if let Some(flag) = self.transfer_has_work.get_mut(frame) {
            *flag = false;
        }
        Ok(())
    }

    fn buffer_barrier_transfer(&self, command_buffer: vk::CommandBuffer, buffer: vk::Buffer, offset: u64, size: u64, dst_access: vk::AccessFlags) {
        if size == 0 {
            return;
        }
        let barrier = vk::BufferMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(dst_access)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(buffer)
            .offset(offset)
            .size(size)
            .build();
        let barriers = [barrier];
        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::VERTEX_INPUT,
                vk::DependencyFlags::empty(),
                &[],
                &barriers,
                &[],
            );
        }
    }

    fn create_texture_resource(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<VulkanTexture> {
        let width_u32 = u32::try_from(width).map_err(|_| "texture width out of range".to_string())?;
        let height_u32 = u32::try_from(height).map_err(|_| "texture height out of range".to_string())?;
        let mut image = self.create_image_resource(width_u32, height_u32)?;

        let staging = self.create_buffer(
            pixels.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        self.write_buffer(&staging, pixels)?;
        self.copy_buffer_to_image(&staging, &mut image)?;
        let mut staging = staging;
        staging.destroy(&self.device);

        let descriptor_set = self.allocate_texture_descriptor(&image)?;
        Ok(VulkanTexture { image, descriptor_set })
    }

    fn allocate_texture_descriptor(&mut self, image: &ImageResource) -> Result<vk::DescriptorSet> {
        let mut ui = self.ui.take().ok_or_else(|| "UI resources not initialized".to_string())?;
        let descriptor_set = ui.allocate_texture_descriptor(self, image)?;
        self.ui = Some(ui);
        Ok(descriptor_set)
    }

    fn single_time_commands<F: FnOnce(vk::CommandBuffer)>(&self, f: F) -> Result<()> {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer =
            unsafe { self.device.allocate_command_buffers(&alloc_info) }.map_err(|err| format!("allocate_command_buffers failed: {err:?}"))?[0];
        let begin_info = vk::CommandBufferBeginInfo::builder().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(|err| format!("begin_command_buffer failed: {err:?}"))?;
        }
        f(command_buffer);
        unsafe {
            self.device
                .end_command_buffer(command_buffer)
                .map_err(|err| format!("end_command_buffer failed: {err:?}"))?;
        }
        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);
        let submit_infos = [submit_info.build()];
        unsafe {
            self.device
                .queue_submit(self.graphics_queue, &submit_infos, vk::Fence::null())
                .map_err(|err| format!("queue_submit failed: {err:?}"))?;
            self.device
                .queue_wait_idle(self.graphics_queue)
                .map_err(|err| format!("queue_wait_idle failed: {err:?}"))?;
            self.device.free_command_buffers(self.command_pool, &command_buffers);
        }
        Ok(())
    }

    fn transition_image_layout(
        &self,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        aspect_mask: vk::ImageAspectFlags,
    ) {
        let (src_access, dst_access, src_stage, dst_stage) = match (old_layout, new_layout) {
            (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            ),
            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            ),
            _ => (
                vk::AccessFlags::empty(),
                vk::AccessFlags::empty(),
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            ),
        };

        let barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(aspect_mask)
                    .level_count(1)
                    .layer_count(1)
                    .build(),
            )
            .src_access_mask(src_access)
            .dst_access_mask(dst_access)
            .build();

        unsafe {
            self.device
                .cmd_pipeline_barrier(cmd, src_stage, dst_stage, vk::DependencyFlags::empty(), &[], &[], &[barrier]);
        }
    }

    fn find_memory_type(&self, type_filter: u32, properties: vk::MemoryPropertyFlags) -> Result<u32> {
        let mem_properties = unsafe { self.instance.get_physical_device_memory_properties(self.physical_device) };
        for (index, memory_type) in mem_properties.memory_types.iter().enumerate() {
            if (type_filter & (1 << index)) != 0 && memory_type.property_flags.contains(properties) {
                return Ok(index as u32);
            }
        }
        Err("Unable to find suitable memory type".into())
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();

            for &fence in &self.in_flight_fences {
                self.device.destroy_fence(fence, None);
            }
            for &semaphore in &self.image_available_semaphores {
                self.device.destroy_semaphore(semaphore, None);
            }
            for &semaphore in &self.render_finished_semaphores {
                self.device.destroy_semaphore(semaphore, None);
            }

            self.cleanup_swapchain();

            if self.command_pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.command_pool, None);
            }

            self.surface_loader.destroy_surface(self.surface, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

#[derive(Clone, Copy)]
struct QueueFamilyIndices {
    graphics_family: u32,
    present_family: u32,
}
impl VulkanContext {
    fn scale_rect(&self, rect: Recti) -> Recti {
        scale_rect_to_surface(rect, self.logical_width, self.logical_height, self.extent.width, self.extent.height)
    }
}
