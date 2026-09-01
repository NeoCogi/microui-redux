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
// Vulkan renderer overview:
// - UI and custom geometry uploads go through per-frame staging/device buffers that grow with
//   demand, stay resident, and are referenced via ring offsets to avoid reallocations.
// - Staging buffers remain persistently mapped; command submission uses reusable per-frame
//   transfer command buffers, synchronized with semaphores/barriers instead of queue_wait_idle.
// - Custom draw uploads share the same staging path as UI vertices so we issue one batched
//   transfer stream per frame.
// - Mesh vertex/index data lives in device-local memory and is refreshed via staging uploads.
// - Texture descriptors are recreated automatically after swapchain rebuilds. The immutable atlas
//   image survives those rebuilds and is rebound without another pixel upload.
//
//! Vulkan renderer backend used by examples.
//!
//! This module implements the `RendererBackend` trait, texture uploads, UI batching, swapchain handling,
//! and optional custom mesh rendering for the demo application.

use std::{collections::HashMap, convert::TryFrom, io::Cursor, mem, ptr};

use ash::{khr, util::read_spv, vk, Entry};
use microui_redux::{
    prelude::*,
    render::{TextureError, Vertex},
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use sdl2::video::Window;

use super::{
    mesh::{CustomRenderArea, MeshSubmission, MeshVertex},
    resource_guard::ResourceGuard,
};

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

/// Creates exactly one graphics pipeline and destroys any handles returned alongside an error.
///
/// Vulkan may return a successful prefix in `Err((pipelines, error))`; simply mapping that error
/// drops copyable handles without destroying them. Centralizing this unusual result shape keeps
/// both example pipelines leak-free and also handles a non-conforming empty success defensively.
fn create_single_graphics_pipeline(device: &ash::Device, info: vk::GraphicsPipelineCreateInfo<'_>) -> Result<vk::Pipeline> {
    let infos = [info];
    let pipelines = match unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &infos, None) } {
        Ok(pipelines) => pipelines,
        Err((partial, err)) => {
            for pipeline in partial {
                if pipeline != vk::Pipeline::null() {
                    unsafe { device.destroy_pipeline(pipeline, None) };
                }
            }
            return Err(format!("create_graphics_pipelines failed: {err:?}"));
        }
    };

    let mut pipelines = pipelines.into_iter();
    let pipeline = pipelines.next().ok_or_else(|| "create_graphics_pipelines returned no pipeline".to_string())?;
    for unexpected in pipelines {
        if unexpected != vk::Pipeline::null() {
            unsafe { device.destroy_pipeline(unexpected, None) };
        }
    }
    if pipeline == vk::Pipeline::null() {
        return Err("create_graphics_pipelines returned a null pipeline".into());
    }
    Ok(pipeline)
}

/// Native swapchain image and synchronization slot acquired before display-list execution.
struct AcquiredVulkanFrame {
    /// Frame-in-flight slot whose acquire semaphore was signaled for this image.
    frame: usize,
    /// Swapchain image that must either be submitted and presented or make the context fatal.
    image_index: u32,
    /// Whether acquisition reported that the swapchain should be rebuilt after presentation.
    suboptimal: bool,
}

/// Lifecycle of the single swapchain image currently owned by the high-level frame boundary.
///
/// A successful acquire signals a binary semaphore and removes one image from the presentation
/// engine. If any later record, submit, or present step fails, neither that semaphore nor that image
/// can be blindly reused. The fatal state deliberately disables later frames instead of attempting
/// a partial synchronization repair whose correctness would depend on how far the driver progressed.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum AcquiredFrameState {
    /// No acquired image or signaled acquire semaphore is outstanding.
    #[default]
    Ready,
    /// One [`AcquiredVulkanFrame`] must complete through graphics submission and presentation.
    Acquired,
    /// A post-acquire failure abandoned synchronization state; this context cannot render again.
    Fatal,
}

impl AcquiredFrameState {
    /// Rejects acquisition unless every earlier image and semaphore completed normally.
    fn ensure_ready(self) -> Result<()> {
        match self {
            Self::Ready => Ok(()),
            Self::Acquired => Err(String::from("cannot acquire a second Vulkan frame while one is outstanding")),
            Self::Fatal => Err(String::from("cannot acquire a Vulkan frame after an earlier acquired frame failed")),
        }
    }

    /// Records that swapchain acquisition succeeded and its synchronization objects are now live.
    fn acquire_succeeded(&mut self) -> Result<()> {
        match self {
            Self::Ready => {
                // Only a ready context can publish a new acquired-frame token.
                *self = Self::Acquired;
                Ok(())
            }
            Self::Acquired => Err(String::from("cannot acquire a second Vulkan frame while one is outstanding")),
            Self::Fatal => Err(String::from("cannot acquire a Vulkan frame after an earlier acquired frame failed")),
        }
    }

    /// Commits a successful acquired frame or permanently latches any post-acquire failure.
    fn finish<T>(&mut self, result: Result<T>) -> Result<T> {
        debug_assert_eq!(*self, Self::Acquired, "only an acquired frame may be finalized");
        if result.is_ok() {
            // Submission and presentation consumed both the image and its acquire semaphore.
            *self = Self::Ready;
        } else {
            // The exact driver progress is intentionally irrelevant after failure: no later frame
            // may wait on, reset, or reuse synchronization objects from this abandoned transaction.
            *self = Self::Fatal;
        }
        result
    }

    /// Latches a panic or other non-Result escape from acquired-frame finalization.
    fn abandon(&mut self) {
        // A panic can occur after native commands have been recorded. Treat it exactly like an
        // explicit error so unwinding cannot make the same slot available again.
        if *self == Self::Acquired {
            *self = Self::Fatal;
        }
    }

    /// Returns whether later rendering must remain disabled.
    fn is_fatal(self) -> bool {
        self == Self::Fatal
    }
}

pub struct VulkanRenderer {
    // `VulkanRenderer` is the microui `RendererBackend` implementation. It batches UI quads
    // into `vertices`, records custom render jobs into `commands`, and delegates all Vulkan object
    // lifetime and frame submission concerns to `VulkanContext`.
    atlas: AtlasHandle,
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
    /// Permanent high-level latch set after device loss or an abandoned acquired frame.
    disabled: bool,
}

trait VulkanFrameOps {
    fn begin(&mut self, width: i32, height: i32, clr: Color) -> Result<()>;
    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex);
    fn push_triangle_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex);
    fn flush(&mut self);
    fn finish(&mut self, acquired: AcquiredVulkanFrame);
    /// Uploads pixels using the immutable dimensions carried by the renderer-issued ID.
    fn create_texture(&mut self, id: TextureId, pixels: &[u8]) -> std::result::Result<(), TextureError>;
    fn destroy_texture(&mut self, id: TextureId);
    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]);
}

impl VulkanRenderer {
    /// Closes the current UI vertex batch so later commands preserve draw ordering.
    fn flush_ui_batch(&mut self) {
        // Custom render jobs must preserve ordering relative to the UI that came before them. The
        // command list therefore stores UI draws as explicit "draw up to this vertex index" cuts.
        let end = self.vertices.len();
        if end > self.current_batch_end {
            self.commands.push(FrameCommand::DrawTo(end));
            self.current_batch_end = end;
        }
    }

    /// Creates the high-level Vulkan renderer and its backing Vulkan context.
    pub fn new(window: &Window, atlas: AtlasHandle, width: u32, height: u32) -> Result<Self> {
        let mut context = VulkanContext::new(window, width, height)?;
        context.upload_atlas(&atlas)?;
        let swapchain_generation = context.swapchain_generation();

        Ok(Self {
            atlas,
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
            disabled: false,
        })
    }

    /// Recreates the swapchain when the SDL window size diverges from the cached Vulkan extent.
    fn ensure_swapchain_extent(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Ok(()); // Minimized window; skip until it has a size.
        }

        // The SDL window size is the source of truth; the Vulkan context rebuilds the swapchain on
        // demand when the cached extent diverges from it.
        let extent = self.context.extent();
        if extent.width != width || extent.height != height {
            self.context.recreate_swapchain(width, height)?;
        }

        Ok(())
    }

    /// Rebinds backend-owned texture descriptors after a swapchain/UI resource rebuild.
    fn handle_swapchain_updates(&mut self) -> Result<()> {
        let generation = self.context.swapchain_generation();
        if self.last_swapchain_generation != generation {
            // Texture descriptor sets belong to the UI descriptor pool, so a swapchain/UI rebuild
            // invalidates them even though the logical texture map stays the same. Publish the new
            // generation only after every replacement descriptor has been allocated successfully.
            self.rebind_texture_descriptors()?;
            self.last_swapchain_generation = generation;
        }
        Ok(())
    }

    /// Allocates fresh descriptors for every texture, committing none until the batch succeeds.
    fn rebind_texture_descriptors(&mut self) -> Result<()> {
        let mut replacements = Vec::new();
        replacements
            .try_reserve(self.textures.len())
            .map_err(|err| format!("failed to reserve descriptor replacement transaction: {err}"))?;
        for (&id, texture) in &self.textures {
            let descriptor = self.context.allocate_texture_descriptor(&texture.image)?;
            replacements.push((id, descriptor.guarded(&self.context.device)));
        }

        // No fallible operations remain. Old-generation descriptors refer to an already-destroyed
        // pool and are invalidated without a Vulkan free; current-generation retry state is freed.
        for (id, descriptor) in replacements {
            let texture = self.textures.get_mut(&id).expect("rebind keys remain stable during the transaction");
            self.context.free_texture_descriptor(&mut texture.descriptor);
            texture.descriptor = descriptor.into_inner();
        }
        Ok(())
    }

    /// Queues a backend-specific custom render job after flushing earlier UI geometry.
    pub(crate) fn enqueue_custom_render<C: VulkanCustomRenderer + 'static>(&mut self, area: CustomRenderArea, cmd: C) {
        self.flush_ui_batch();
        self.commands.push(FrameCommand::Custom(CustomRenderJob {
            area,
            kind: "custom",
            callback: Box::new(cmd),
            budget: FrameResourceBudget::default(),
        }));
    }

    /// Converts a microui clear color into the Vulkan clear value used for the render pass.
    fn color_to_vk_clear(color: Color) -> vk::ClearValue {
        let to_float = |c: u8| c as f32 / 255.0;
        vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [to_float(color.r), to_float(color.g), to_float(color.b), to_float(color.a)],
            },
        }
    }
}

impl VulkanFrameOps for VulkanRenderer {
    /// Starts a new frame, syncing window size and swapchain generation.
    fn begin(&mut self, width: i32, height: i32, clr: Color) -> Result<()> {
        // `begin` only resets CPU-side batching state and keeps the GPU-side context synchronized
        // with window size changes. Actual command buffer recording happens in `end`.
        self.frame_index = self.frame_index.wrapping_add(1);
        self.width = width as u32;
        self.height = height as u32;
        self.clear_color = clr;
        self.vertices.clear();
        self.commands.clear();
        self.current_batch_end = 0;
        if self.disabled {
            return Err(String::from("Vulkan renderer is disabled after an unrecoverable frame failure"));
        }

        self.ensure_swapchain_extent(self.width, self.height)?;
        self.handle_swapchain_updates()?;
        Ok(())
    }

    /// Appends a quad to the buffered UI vertex stream.
    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        self.vertices.extend_from_slice(&[*v0, *v1, *v2, *v0, *v2, *v3]);
    }

    /// Appends one triangle to the buffered UI vertex stream without forcing a separate draw job.
    fn push_triangle_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex) {
        self.vertices.extend_from_slice(&[*v0, *v1, *v2]);
    }

    /// Turns the currently buffered UI vertices into an explicit draw command boundary.
    fn flush(&mut self) {
        // Match the GL renderer expectation: turn buffered UI vertices into a draw command before
        // custom rendering happens.
        self.flush_ui_batch();
    }

    /// Finalizes the frame by submitting the queued UI and custom commands to Vulkan.
    fn finish(&mut self, acquired: AcquiredVulkanFrame) {
        self.flush_ui_batch();
        if self.disabled {
            self.commands.clear();
            self.vertices.clear();
            self.current_batch_end = 0;
            return;
        }
        let mut commands = std::mem::take(&mut self.commands);
        // `draw_frame` consumes the queued UI/custom jobs and may drain `commands`; taking the vec
        // avoids reallocating a fresh command buffer every frame.
        if let Err(err) = self.context.draw_frame(
            acquired,
            Self::color_to_vk_clear(self.clear_color),
            &self.vertices,
            self.width,
            self.height,
            self.frame_index,
            &mut commands,
        ) {
            eprintln!("[microui-redux][vulkan] draw_frame failed: {err}");
            if self.context.is_unusable() {
                self.disabled = true;
                eprintln!("[microui-redux][vulkan] acquired frame became unrecoverable; disabling Vulkan rendering for the rest of this run");
            }
        }
        commands.clear();
        self.commands = commands;
        self.vertices.clear();
        self.current_batch_end = 0;
    }

    /// Creates a backend-owned sampled texture and tracks it by `TextureId`.
    fn create_texture(&mut self, id: TextureId, pixels: &[u8]) -> std::result::Result<(), TextureError> {
        // Texture creation is the public backend boundary, so convert Vulkan's string-based
        // operational failures into the renderer's typed texture error without changing the
        // file-wide `Result<T>` used by unrelated swapchain and resource internals.
        if self.disabled {
            return Err(TextureError::backend("Vulkan renderer is disabled after an unrecoverable frame failure"));
        }
        self.textures
            .try_reserve(1)
            .map_err(|err| TextureError::backend(format!("failed to reserve texture ownership entry: {err}")))?;
        // The opaque capability is the sole dimension source validated by the Context executor.
        let dimensions = id.size();
        let texture = self
            .context
            .create_texture_resource(dimensions.width, dimensions.height, pixels)
            .map_err(TextureError::backend)?;
        // A replacement is committed only after the new image is complete. Destroying the old
        // image after insertion also keeps the map leak-free if an ID is retried unexpectedly.
        if let Some(mut previous) = self.textures.insert(id, texture) {
            previous.destroy(&self.context);
        }
        Ok(())
    }

    /// Destroys a backend-owned sampled texture if it is still tracked.
    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(mut texture) = self.textures.remove(&id) {
            // Texture destruction is rare in the examples; a simple idle boundary guarantees no
            // submitted frame still samples the image before its view/memory are released.
            unsafe {
                self.context.device.device_wait_idle().ok();
            }
            texture.destroy(&self.context);
        }
    }

    /// Queues a pre-clipped textured custom draw that samples from a backend-owned texture.
    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        if self.disabled {
            return;
        }
        let descriptor = match self.textures.get(&id).map(|tex| tex.descriptor.set) {
            Some(desc) => desc,
            None => return,
        };

        let mut quad = Vec::with_capacity(6);
        quad.extend_from_slice(&[vertices[0], vertices[1], vertices[2], vertices[0], vertices[2], vertices[3]]);

        // The Context executor already clipped the quad and adjusted UVs, so the texture command's draw area
        // is just the submitted geometry bounds used to preserve ordering.
        let area_rect = rect_from_vertices(&vertices);
        let area = CustomRenderArea { rect: area_rect, clip: area_rect };

        // The Context executor owns the pre-texture ordering boundary.
        self.commands.push(FrameCommand::Custom(CustomRenderJob {
            area,
            kind: "texture",
            callback: Box::new(TextureDrawCommand {
                vertices: quad,
                descriptor_set: descriptor,
            }),
            budget: FrameResourceBudget {
                ui_vertices: 6,
                mesh_vertices: 0,
                mesh_indices: 0,
            },
        }));
    }
}

impl Drop for VulkanRenderer {
    /// Destroys user images while the context/device they depend on are still alive.
    fn drop(&mut self) {
        // `VulkanContext::drop` also waits, but renderer-owned images must be released before the
        // context field itself is dropped. Establish the idle boundary here first.
        unsafe {
            self.context.device.device_wait_idle().ok();
        }
        for (_, mut texture) in self.textures.drain() {
            texture.destroy(&self.context);
        }
    }
}

#[must_use = "the Vulkan frame is finalized when dropped"]
pub struct VulkanFrame<'a> {
    backend: &'a mut VulkanRenderer,
    acquired: Option<AcquiredVulkanFrame>,
}

impl VulkanFrame<'_> {
    pub fn enqueue_colored_vertices(&mut self, area: CustomRenderArea, vertices: Vec<Vertex>) {
        self.backend.enqueue_colored_vertices(area, vertices);
    }

    pub fn enqueue_mesh_draw(&mut self, area: CustomRenderArea, submission: MeshSubmission) {
        self.backend.enqueue_mesh_draw(area, submission);
    }
}

impl RendererFrame for VulkanFrame<'_> {
    fn push_quad(&mut self, vertices: [Vertex; 4]) {
        VulkanFrameOps::push_quad_vertices(self.backend, &vertices[0], &vertices[1], &vertices[2], &vertices[3]);
    }

    fn push_triangle(&mut self, vertices: [Vertex; 3]) {
        VulkanFrameOps::push_triangle_vertices(self.backend, &vertices[0], &vertices[1], &vertices[2]);
    }

    fn flush(&mut self) {
        VulkanFrameOps::flush(self.backend);
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        VulkanFrameOps::draw_texture(self.backend, id, vertices);
    }
}

impl Drop for VulkanFrame<'_> {
    fn drop(&mut self) {
        let Some(acquired) = self.acquired.take() else {
            return;
        };
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            VulkanFrameOps::finish(self.backend, acquired);
        }))
        .is_err()
        {
            // A panic can occur after acquire or native recording. Latch both layers before
            // unwinding is swallowed so no later frame reuses the outstanding image or sync slot.
            self.backend.context.abandon_acquired_frame();
            self.backend.disabled = true;
            eprintln!("[microui-redux][vulkan] frame finalization panicked");
        }
    }
}

impl RendererBackend for VulkanRenderer {
    type Frame<'a> = VulkanFrame<'a>;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn replace_atlas(&mut self, atlas: AtlasHandle) -> std::result::Result<(), AtlasUploadError> {
        // VulkanContext's atlas upload retains the old image until the candidate image, transfer,
        // and descriptor update all succeed. Mirror that commit by replacing the CPU handle last.
        self.context.upload_atlas(&atlas).map_err(AtlasUploadError::new)?;
        self.atlas = atlas;
        Ok(())
    }

    fn frame(&mut self, info: FrameInfo) -> std::result::Result<Self::Frame<'_>, FrameError> {
        let dimensions = info.dimensions();
        VulkanFrameOps::begin(self, dimensions.width, dimensions.height, info.clear()).map_err(FrameError::new)?;
        let acquired = self
            .context
            .acquire_frame(dimensions.width as u32, dimensions.height as u32)
            .map_err(FrameError::new)?;
        Ok(VulkanFrame { backend: self, acquired: Some(acquired) })
    }

    fn create_texture(&mut self, id: TextureId, pixels: &[u8]) -> std::result::Result<(), TextureError> {
        // Preserve the typed texture failure exposed by the frame-ops boundary verbatim.
        VulkanFrameOps::create_texture(self, id, pixels)
    }

    fn destroy_texture(&mut self, id: TextureId) {
        VulkanFrameOps::destroy_texture(self, id);
    }
}

impl VulkanRenderer {
    pub fn enqueue_colored_vertices(&mut self, area: CustomRenderArea, vertices: Vec<Vertex>) {
        if self.disabled {
            return;
        }
        if vertices.is_empty() {
            return;
        }
        let descriptor_set = match self.context.ui_descriptor_set() {
            Some(set) => set,
            None => return,
        };
        self.flush_ui_batch();
        let ui_vertices = vertices.len();
        self.commands.push(FrameCommand::Custom(CustomRenderJob {
            area,
            kind: "colored",
            callback: Box::new(ColoredVerticesCommand { vertices, descriptor_set }),
            budget: FrameResourceBudget {
                ui_vertices,
                mesh_vertices: 0,
                mesh_indices: 0,
            },
        }));
    }

    pub fn enqueue_mesh_draw(&mut self, area: CustomRenderArea, submission: MeshSubmission) {
        if self.disabled {
            return;
        }
        if submission.mesh.is_empty() {
            return;
        }
        let mesh_vertices = submission.mesh.vertices().len();
        let mesh_indices = submission.mesh.indices().len();
        self.flush_ui_batch();
        self.commands.push(FrameCommand::Custom(CustomRenderJob {
            area,
            kind: "mesh",
            callback: Box::new(MeshDrawCommand { submission }),
            budget: FrameResourceBudget {
                ui_vertices: 0,
                mesh_vertices,
                mesh_indices,
            },
        }));
    }
}

struct VulkanTexture {
    image: ImageResource,
    descriptor: TextureDescriptor,
}

impl VulkanTexture {
    /// Releases the individually allocated descriptor before destroying its referenced image.
    fn destroy(&mut self, context: &VulkanContext) {
        context.free_texture_descriptor(&mut self.descriptor);
        self.image.destroy(&context.device);
    }
}

/// Identifies both a descriptor set and the exact pool generation that owns its allocation.
#[derive(Clone, Copy)]
struct TextureDescriptor {
    /// Individually allocated descriptor-set handle used for one external image.
    set: vk::DescriptorSet,
    /// Descriptor pool that must receive `set` when its texture is destroyed.
    pool: vk::DescriptorPool,
    /// Swapchain/UI generation in which `pool` and `set` were allocated.
    generation: u64,
}

impl TextureDescriptor {
    /// Arms reclamation for a newly allocated set until its texture or rebind batch commits.
    fn guarded(self, device: &ash::Device) -> ResourceGuard<Self, impl FnOnce(Self) + use<>> {
        let device = device.clone();
        ResourceGuard::new(self, move |descriptor| unsafe {
            let _ = device.free_descriptor_sets(descriptor.pool, &[descriptor.set]);
        })
    }

    /// Requires generation as well as pool equality because Vulkan may reuse a destroyed handle.
    fn belongs_to(self, generation: u64, pool: vk::DescriptorPool) -> bool {
        self.generation == generation && self.pool == pool
    }
}

struct Buffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
}

impl Buffer {
    /// Arms destruction for a complete buffer/memory pair until a parent owner commits it.
    fn guarded(self, device: &ash::Device) -> ResourceGuard<Self, impl FnOnce(Self) + use<>> {
        let device = device.clone();
        ResourceGuard::new(self, move |mut buffer| buffer.destroy(&device))
    }

    /// Destroys the Vulkan buffer before releasing the memory bound to it.
    fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            if self.buffer != vk::Buffer::null() {
                device.destroy_buffer(self.buffer, None);
            }
            if self.memory != vk::DeviceMemory::null() {
                device.free_memory(self.memory, None);
            }
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
        // Mapping is the final fallible step, but the passed buffer is not yet owned by any
        // persistent structure. Guard it so a map failure releases both handle and memory.
        let buffer = buffer.guarded(device);
        let ptr = unsafe {
            device
                .map_memory(buffer.get().memory, 0, buffer.get().size, vk::MemoryMapFlags::empty())
                .map_err(|err| format!("map_memory (staging) failed: {err:?}"))?
        } as *mut u8;
        Ok(Self { buffer: buffer.into_inner(), ptr })
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

pub(crate) trait VulkanCustomRenderer: Send {
    fn record(&mut self, ctx: &mut VulkanContext, command_buffer: vk::CommandBuffer, extent: vk::Extent2D, area: &CustomRenderArea);
}

#[derive(Clone, Copy, Default)]
struct FrameResourceBudget {
    ui_vertices: usize,
    mesh_vertices: usize,
    mesh_indices: usize,
}

struct CustomRenderJob {
    area: CustomRenderArea,
    kind: &'static str,
    callback: Box<dyn VulkanCustomRenderer>,
    budget: FrameResourceBudget,
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
    /// Arms reverse-order destruction for a complete image/memory/view aggregate.
    fn guarded(self, device: &ash::Device) -> ResourceGuard<Self, impl FnOnce(Self) + use<>> {
        let device = device.clone();
        ResourceGuard::new(self, move |mut image| image.destroy(&device))
    }

    /// Destroys the dependent view first, then the image and its backing allocation.
    fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            if self.view != vk::ImageView::null() {
                device.destroy_image_view(self.view, None);
            }
            if self.image != vk::Image::null() {
                device.destroy_image(self.image, None);
            }
            if self.memory != vk::DeviceMemory::null() {
                device.free_memory(self.memory, None);
            }
        }
        self.image = vk::Image::null();
        self.memory = vk::DeviceMemory::null();
        self.view = vk::ImageView::null();
    }
}

const MAX_DESCRIPTOR_SETS: u32 = 128;
/// Individual external textures must return descriptor capacity when their capabilities die.
const DESCRIPTOR_POOL_FLAGS: vk::DescriptorPoolCreateFlags = vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET;

/// Publishes a completely built resource aggregate and returns its previous owner for destruction.
///
/// `replacement` is evaluated by the caller before this function receives the authoritative slot.
/// An error therefore leaves `current` byte-for-byte untouched. On success, `transfer` may move
/// swapchain-independent ownership from the old aggregate to the new one before publication; no
/// fallible work remains after that ownership movement begins.
fn commit_resource_replacement<T, E>(
    current: &mut Option<T>,
    replacement: std::result::Result<T, E>,
    transfer: impl FnOnce(&mut T, &mut T),
) -> std::result::Result<Option<T>, E> {
    // Resolve construction first. `?` returns while `current` still owns every installed resource.
    let mut replacement = replacement?;
    let mut previous = current.take();
    if let Some(previous) = previous.as_mut() {
        // Ownership transfer is deliberately infallible, making publication the only remaining
        // mutation after the previous aggregate leaves its slot.
        transfer(previous, &mut replacement);
    }
    *current = Some(replacement);
    Ok(previous)
}

struct UiResources {
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    vertex_buffer: Option<Buffer>,              // GPU-local
    staging_buffers: Vec<Option<MappedBuffer>>, // CPU-visible, one slot per frame-in-flight
    atlas: Option<ImageResource>,
    vertex_offset: vk::DeviceSize,
    staging_offsets: Vec<vk::DeviceSize>,
    retired_vertex_buffers: Vec<Vec<Buffer>>,
    retired_staging_buffers: Vec<Vec<MappedBuffer>>,
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
        // UI setup is a six-stage transaction. Each independently destroyable handle remains
        // guarded until descriptor allocation proves the complete resource set is usable.
        let descriptor_set_layout = ResourceGuard::new(Self::create_descriptor_set_layout(device)?, |layout| unsafe {
            device.destroy_descriptor_set_layout(layout, None)
        });
        let pipeline_layout = ResourceGuard::new(Self::create_pipeline_layout(device, *descriptor_set_layout.get())?, |layout| unsafe {
            device.destroy_pipeline_layout(layout, None)
        });
        let pipeline = ResourceGuard::new(Self::create_pipeline(ctx, *pipeline_layout.get())?, |pipeline| unsafe {
            device.destroy_pipeline(pipeline, None)
        });
        let sampler = ResourceGuard::new(Self::create_sampler(device)?, |sampler| unsafe { device.destroy_sampler(sampler, None) });
        let descriptor_pool = ResourceGuard::new(Self::create_descriptor_pool(device)?, |pool| unsafe {
            device.destroy_descriptor_pool(pool, None)
        });
        let descriptor_set = Self::allocate_descriptor_set(device, *descriptor_pool.get(), *descriptor_set_layout.get())?;

        // Finish all potentially allocating CPU bookkeeping while the driver handles are still
        // guarded. The struct literal below then only moves completed state into its final owner.
        let staging_buffers = (0..ctx.max_frames_in_flight).map(|_| None).collect();
        let staging_offsets = vec![0; ctx.max_frames_in_flight];
        let retired_vertex_buffers = (0..ctx.max_frames_in_flight).map(|_| Vec::new()).collect();
        let retired_staging_buffers = (0..ctx.max_frames_in_flight).map(|_| Vec::new()).collect();
        Ok(Self {
            descriptor_set_layout: descriptor_set_layout.into_inner(),
            pipeline_layout: pipeline_layout.into_inner(),
            pipeline: pipeline.into_inner(),
            sampler: sampler.into_inner(),
            descriptor_pool: descriptor_pool.into_inner(),
            descriptor_set,
            vertex_buffer: None,
            staging_buffers,
            atlas: None,
            vertex_offset: 0,
            staging_offsets,
            retired_vertex_buffers,
            retired_staging_buffers,
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
        for frame in &mut self.retired_vertex_buffers {
            for mut buffer in frame.drain(..) {
                buffer.destroy(device);
            }
        }
        for frame in &mut self.retired_staging_buffers {
            for mut buffer in frame.drain(..) {
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
        // Shader modules are construction-only objects. Their guards intentionally remain armed
        // on success, so they are destroyed as soon as pipeline creation returns.
        let vert_module = ResourceGuard::new(Self::create_shader_module(device, UI_VERT_SPV)?, |module| unsafe {
            device.destroy_shader_module(module, None)
        });
        let frag_module = ResourceGuard::new(Self::create_shader_module(device, UI_FRAG_SPV)?, |module| unsafe {
            device.destroy_shader_module(module, None)
        });
        let entry = c"main";
        let stage_infos = [
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(*vert_module.get())
                .name(entry)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(*frag_module.get())
                .name(entry)
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
        let pipeline = create_single_graphics_pipeline(device, pipeline_info.build())?;

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
        // External texture sets have independent lifetimes, so the pool must permit reclaiming
        // each set instead of monotonically consuming `MAX_DESCRIPTOR_SETS` until recreation.
        let info = vk::DescriptorPoolCreateInfo::builder()
            .flags(DESCRIPTOR_POOL_FLAGS)
            .pool_sizes(&pool_sizes)
            .max_sets(MAX_DESCRIPTOR_SETS);
        unsafe { device.create_descriptor_pool(&info, None) }.map_err(|err| format!("create_descriptor_pool failed: {err:?}"))
    }

    fn allocate_descriptor_set(device: &ash::Device, pool: vk::DescriptorPool, layout: vk::DescriptorSetLayout) -> Result<vk::DescriptorSet> {
        let layouts = [layout];
        let info = vk::DescriptorSetAllocateInfo::builder().descriptor_pool(pool).set_layouts(&layouts);
        let sets = unsafe { device.allocate_descriptor_sets(&info) }.map_err(|err| format!("allocate_descriptor_sets failed: {err:?}"))?;
        Ok(sets[0])
    }

    fn allocate_texture_descriptor(&mut self, ctx: &VulkanContext, image: &ImageResource) -> Result<TextureDescriptor> {
        let descriptor_set = Self::allocate_descriptor_set(&ctx.device, self.descriptor_pool, self.descriptor_set_layout)?;
        self.update_descriptor(&ctx.device, descriptor_set, image);
        Ok(TextureDescriptor {
            set: descriptor_set,
            pool: self.descriptor_pool,
            generation: ctx.swapchain_generation,
        })
    }

    fn create_shader_module(device: &ash::Device, code: &[u8]) -> Result<vk::ShaderModule> {
        let mut cursor = Cursor::new(code);
        let spv = read_spv(&mut cursor).map_err(|err| format!("read_spv failed: {err:?}"))?;
        let info = vk::ShaderModuleCreateInfo::builder().code(&spv);
        unsafe { device.create_shader_module(&info, None) }.map_err(|err| format!("create_shader_module failed: {err:?}"))
    }

    /// Creates and uploads the immutable atlas image during renderer construction.
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

        // Keep both upload resources pending until the transfer and descriptor update complete;
        // an error leaves any previously installed atlas untouched.
        let mut atlas_image = ctx.create_image_resource(width_u32, height_u32)?.guarded(&ctx.device);
        let staging = ctx
            .create_buffer(
                data.len() as u64,
                vk::BufferUsageFlags::TRANSFER_SRC,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?
            .guarded(&ctx.device);
        ctx.write_buffer(staging.get(), &data)?;
        ctx.copy_buffer_to_image(staging.get(), atlas_image.get_mut())?;
        self.update_descriptor(&ctx.device, self.descriptor_set, atlas_image.get());

        if let Some(mut previous) = self.atlas.replace(atlas_image.into_inner()) {
            previous.destroy(&ctx.device);
        }
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

    fn ensure_vertex_buffer(&mut self, ctx: &VulkanContext, frame: usize, required: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.vertex_buffer.as_ref().map(|buf| buf.size);
        let needs_realloc = current_capacity.map(|cap| cap < required).unwrap_or(true);
        if needs_realloc {
            if frame >= self.retired_vertex_buffers.len() {
                return Err(format!("invalid frame index for UI retired vertex buffers: {}", frame));
            }
            let new_capacity = Self::grow_capacity(current_capacity, required, Self::MIN_VERTEX_CAPACITY);
            // Allocate first; only a successful replacement retires the still-usable old buffer.
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;
            if let Some(previous) = self.vertex_buffer.replace(buffer) {
                self.retired_vertex_buffers[frame].push(previous);
            }
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
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::TRANSFER_SRC,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let mapped = MappedBuffer::new(buffer, &ctx.device)?;
            if let Some(previous) = self.staging_buffers[frame].replace(mapped) {
                self.retired_staging_buffers[frame].push(previous);
            }
            self.staging_offsets[frame] = 0;
        }
        Ok(())
    }

    fn cleanup_retired(&mut self, frame: usize, device: &ash::Device) {
        if let Some(bins) = self.retired_vertex_buffers.get_mut(frame) {
            for mut buffer in bins.drain(..) {
                buffer.destroy(device);
            }
        }
        if let Some(bins) = self.retired_staging_buffers.get_mut(frame) {
            for mut buffer in bins.drain(..) {
                buffer.destroy(device);
            }
        }
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

    #[allow(clippy::too_many_arguments)] // Mirrors the explicit Vulkan draw state passed to record.
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

    #[allow(clippy::too_many_arguments)] // Mirrors the explicit Vulkan draw state passed to record.
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
        let vertex_bytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr() as *const u8, std::mem::size_of_val(vertices)) };
        let copy_size = vertex_bytes.len() as u64;
        let dst_offset = self.vertex_offset;
        self.ensure_staging_buffer(ctx, frame, frame_staging_offset + copy_size)?;
        self.ensure_vertex_buffer(ctx, frame, dst_offset + copy_size)?;
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

    fn prepare_frame(&mut self, ctx: &VulkanContext, frame: usize, ui_vertex_bytes: vk::DeviceSize) -> Result<()> {
        self.ensure_staging_buffer(ctx, frame, ui_vertex_bytes)?;
        self.ensure_vertex_buffer(ctx, frame, ui_vertex_bytes)?;
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
    retired_vertex_buffers: Vec<Vec<Buffer>>,
    retired_index_buffers: Vec<Vec<Buffer>>,
    retired_vertex_staging: Vec<Vec<MappedBuffer>>,
    retired_index_staging: Vec<Vec<MappedBuffer>>,
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
        // Keep the layout and temporary shader modules owned until the graphics pipeline is
        // complete. Any intermediate driver failure then unwinds through the exact destructors.
        let pipeline_layout = ResourceGuard::new(
            unsafe { device.create_pipeline_layout(&layout_info, None) }.map_err(|err| format!("create_pipeline_layout failed: {err:?}"))?,
            |layout| unsafe { device.destroy_pipeline_layout(layout, None) },
        );

        let vert_module = ResourceGuard::new(Self::create_shader_module(device, MESH_VERT_SPV)?, |module| unsafe {
            device.destroy_shader_module(module, None)
        });
        let frag_module = ResourceGuard::new(Self::create_shader_module(device, MESH_FRAG_SPV)?, |module| unsafe {
            device.destroy_shader_module(module, None)
        });
        let entry = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(*vert_module.get())
                .name(entry)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(*frag_module.get())
                .name(entry)
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
            .layout(*pipeline_layout.get())
            .render_pass(ctx.render_pass)
            .subpass(0);

        let pipeline = ResourceGuard::new(create_single_graphics_pipeline(device, pipeline_info.build())?, |pipeline| unsafe {
            device.destroy_pipeline(pipeline, None)
        });

        // Allocate frame bins before committing either driver handle, preserving unwind safety
        // even if host allocation panics while the aggregate is being assembled.
        let retired_vertex_buffers = (0..ctx.max_frames_in_flight).map(|_| Vec::new()).collect();
        let retired_index_buffers = (0..ctx.max_frames_in_flight).map(|_| Vec::new()).collect();
        let retired_vertex_staging = (0..ctx.max_frames_in_flight).map(|_| Vec::new()).collect();
        let retired_index_staging = (0..ctx.max_frames_in_flight).map(|_| Vec::new()).collect();

        Ok(Self {
            pipeline_layout: pipeline_layout.into_inner(),
            pipeline: pipeline.into_inner(),
            vertex_buffer: None,
            index_buffer: None,
            vertex_staging: None,
            index_staging: None,
            vertex_offset: 0,
            vertex_staging_offset: 0,
            index_offset: 0,
            index_staging_offset: 0,
            retired_vertex_buffers,
            retired_index_buffers,
            retired_vertex_staging,
            retired_index_staging,
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
        for frame in &mut self.retired_vertex_buffers {
            for mut buffer in frame.drain(..) {
                buffer.destroy(device);
            }
        }
        for frame in &mut self.retired_index_buffers {
            for mut buffer in frame.drain(..) {
                buffer.destroy(device);
            }
        }
        for frame in &mut self.retired_vertex_staging {
            for mut buffer in frame.drain(..) {
                buffer.destroy(device);
            }
        }
        for frame in &mut self.retired_index_staging {
            for mut buffer in frame.drain(..) {
                buffer.destroy(device);
            }
        }
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
        }
        self.pipeline = vk::Pipeline::null();
        self.pipeline_layout = vk::PipelineLayout::null();
    }

    fn ensure_vertex_buffer(&mut self, ctx: &VulkanContext, frame: usize, required_total: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.vertex_buffer.as_ref().map(|buf| buf.size);
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            if frame >= self.retired_vertex_buffers.len() {
                return Err(format!("invalid frame index for mesh retired vertex buffers: {}", frame));
            }
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_VERTEX_CAPACITY);
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;
            if let Some(previous) = self.vertex_buffer.replace(buffer) {
                self.retired_vertex_buffers[frame].push(previous);
            }
            self.vertex_offset = 0;
        }
        Ok(())
    }

    fn ensure_index_buffer(&mut self, ctx: &VulkanContext, frame: usize, required_total: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.index_buffer.as_ref().map(|buf| buf.size);
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            if frame >= self.retired_index_buffers.len() {
                return Err(format!("invalid frame index for mesh retired index buffers: {}", frame));
            }
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_INDEX_CAPACITY);
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;
            if let Some(previous) = self.index_buffer.replace(buffer) {
                self.retired_index_buffers[frame].push(previous);
            }
            self.index_offset = 0;
        }
        Ok(())
    }

    fn ensure_vertex_staging_buffer(&mut self, ctx: &VulkanContext, frame: usize, required_total: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.vertex_staging.as_ref().map(|buf| buf.size());
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            if frame >= self.retired_vertex_staging.len() {
                return Err(format!("invalid frame index for mesh retired vertex staging buffers: {}", frame));
            }
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_VERTEX_STAGING_CAPACITY);
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::TRANSFER_SRC,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let mapped = MappedBuffer::new(buffer, &ctx.device)?;
            if let Some(previous) = self.vertex_staging.replace(mapped) {
                self.retired_vertex_staging[frame].push(previous);
            }
            self.vertex_staging_offset = 0;
        }
        Ok(())
    }

    fn ensure_index_staging_buffer(&mut self, ctx: &VulkanContext, frame: usize, required_total: vk::DeviceSize) -> Result<()> {
        let current_capacity = self.index_staging.as_ref().map(|buf| buf.size());
        let needs_realloc = current_capacity.map(|cap| cap < required_total).unwrap_or(true);
        if needs_realloc {
            if frame >= self.retired_index_staging.len() {
                return Err(format!("invalid frame index for mesh retired index staging buffers: {}", frame));
            }
            let new_capacity = Self::grow_capacity(current_capacity, required_total, Self::MIN_INDEX_STAGING_CAPACITY);
            let buffer = ctx.create_buffer(
                new_capacity,
                vk::BufferUsageFlags::TRANSFER_SRC,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let mapped = MappedBuffer::new(buffer, &ctx.device)?;
            if let Some(previous) = self.index_staging.replace(mapped) {
                self.retired_index_staging[frame].push(previous);
            }
            self.index_staging_offset = 0;
        }
        Ok(())
    }

    fn cleanup_retired(&mut self, frame: usize, device: &ash::Device) {
        if let Some(buffers) = self.retired_vertex_buffers.get_mut(frame) {
            for mut buffer in buffers.drain(..) {
                buffer.destroy(device);
            }
        }
        if let Some(buffers) = self.retired_index_buffers.get_mut(frame) {
            for mut buffer in buffers.drain(..) {
                buffer.destroy(device);
            }
        }
        if let Some(buffers) = self.retired_vertex_staging.get_mut(frame) {
            for mut buffer in buffers.drain(..) {
                buffer.destroy(device);
            }
        }
        if let Some(buffers) = self.retired_index_staging.get_mut(frame) {
            for mut buffer in buffers.drain(..) {
                buffer.destroy(device);
            }
        }
    }

    fn reset_upload_state(&mut self, frame: usize, device: &ash::Device) {
        self.vertex_offset = 0;
        self.vertex_staging_offset = 0;
        self.index_offset = 0;
        self.index_staging_offset = 0;
        self.cleanup_retired(frame, device);
    }

    fn prepare_frame(&mut self, ctx: &VulkanContext, frame: usize, required_vertex_bytes: vk::DeviceSize, required_index_bytes: vk::DeviceSize) -> Result<()> {
        self.ensure_vertex_buffer(ctx, frame, required_vertex_bytes)?;
        self.ensure_index_buffer(ctx, frame, required_index_bytes)?;
        self.ensure_vertex_staging_buffer(ctx, frame, required_vertex_bytes)?;
        self.ensure_index_staging_buffer(ctx, frame, required_index_bytes)?;
        Ok(())
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
                std::mem::size_of_val(submission.mesh.vertices()),
            )
        };
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                submission.mesh.indices().as_ptr() as *const u8,
                std::mem::size_of_val(submission.mesh.indices()),
            )
        };
        let frame = ctx.current_frame;
        let vertex_copy_size = vertex_bytes.len() as u64;
        let index_copy_size = index_bytes.len() as u64;
        let vertex_dst_offset = self.vertex_offset;
        let index_dst_offset = self.index_offset;
        self.ensure_vertex_buffer(ctx, frame, vertex_dst_offset + vertex_copy_size)?;
        self.ensure_index_buffer(ctx, frame, index_dst_offset + index_copy_size)?;
        self.ensure_vertex_staging_buffer(ctx, frame, self.vertex_staging_offset + vertex_copy_size)?;
        self.ensure_index_staging_buffer(ctx, frame, self.index_staging_offset + index_copy_size)?;

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
/// Owns the native objects acquired before a full [`VulkanContext`] can exist.
///
/// `VulkanContext::drop` cannot help while instance/surface/device creation is only partially
/// complete. This bootstrap owner mirrors their dependency order and is disarmed only after the
/// last fallible discovery step succeeds.
struct VulkanBootstrap {
    /// Root Vulkan instance retained until the completed context takes ownership.
    instance: Option<ash::Instance>,
    /// Instance-level surface dispatch table required to destroy `surface`.
    surface_loader: Option<Surface>,
    /// Window surface owned after creation and nulled when committed or destroyed.
    surface: vk::SurfaceKHR,
    /// Logical device retained only after device creation succeeds.
    device: Option<ash::Device>,
}

impl VulkanBootstrap {
    /// Starts a partial native ownership chain with the already-created instance.
    fn new(instance: ash::Instance) -> Self {
        // Later stages remain absent/null so Drop can distinguish exactly which prefix exists.
        Self {
            instance: Some(instance),
            surface_loader: None,
            surface: vk::SurfaceKHR::null(),
            device: None,
        }
    }

    /// Borrows the instance required by surface and physical-device discovery.
    fn instance(&self) -> &ash::Instance {
        // Bootstrap construction installs the instance before any helper can borrow it.
        self.instance.as_ref().expect("bootstrap instance is present before commit")
    }

    /// Borrows the surface loader after it has been paired with the created surface.
    fn surface_loader(&self) -> &Surface {
        // Callers reach this helper only after installing both loader and surface during startup.
        self.surface_loader.as_ref().expect("bootstrap surface loader is installed with the surface")
    }

    /// Borrows the logical device during the final queue/bootstrap stages.
    fn device(&self) -> &ash::Device {
        // Device-dependent discovery runs only after logical-device creation stored this owner.
        self.device.as_ref().expect("bootstrap device is present after logical-device creation")
    }

    /// Transfers the completed native ownership chain into `VulkanContext`.
    fn commit(mut self) -> (ash::Instance, Surface, vk::SurfaceKHR, ash::Device) {
        // Null/take every field before Drop runs, preserving dependency order while disarming all
        // bootstrap cleanup paths in one infallible ownership transfer.
        let surface = std::mem::replace(&mut self.surface, vk::SurfaceKHR::null());
        (
            self.instance.take().expect("bootstrap instance cannot be committed twice"),
            self.surface_loader.take().expect("bootstrap surface loader cannot be committed twice"),
            surface,
            self.device.take().expect("bootstrap device cannot be committed twice"),
        )
    }
}

impl Drop for VulkanBootstrap {
    /// Tears down only the prefix that was acquired, in reverse dependency order.
    fn drop(&mut self) {
        unsafe {
            if let Some(device) = self.device.take() {
                device.destroy_device(None);
            }
            if self.surface != vk::SurfaceKHR::null() {
                if let Some(loader) = self.surface_loader.as_ref() {
                    loader.destroy_surface(self.surface, None);
                }
                self.surface = vk::SurfaceKHR::null();
            }
            if let Some(instance) = self.instance.take() {
                instance.destroy_instance(None);
            }
        }
    }
}

pub(crate) struct VulkanContext {
    // `VulkanContext` owns the actual Vulkan objects and implements the frame graph used by the
    // example renderer: acquire -> upload/record -> submit -> present -> recreate on demand.
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
    /// Ownership state for the acquire semaphore and swapchain image handed to the active frame.
    acquired_frame_state: AcquiredFrameState,
    ui: Option<UiResources>,
    depth_images: Vec<ImageResource>,
    mesh: Option<MeshResources>,
    swapchain_generation: u64,
    device_lost: bool,
}

impl VulkanContext {
    /// Creates the Vulkan instance/device state and the first set of swapchain-dependent resources.
    fn new(window: &Window, width: u32, height: u32) -> Result<Self> {
        // Context creation front-loads the permanent objects: instance/device/queues, the shared
        // command pool, and the initial swapchain-dependent resources.
        let entry = Entry::linked();
        let app_name = c"microui-redux-examples";
        let engine_name = c"microui-redux";

        let display_handle = window.display_handle().map_err(|err| format!("failed to get display handle: {err:?}"))?;
        let window_handle = window.window_handle().map_err(|err| format!("failed to get window handle: {err:?}"))?;
        let raw_display_handle = display_handle.into();
        let raw_window_handle = window_handle.into();

        let app_info = vk::ApplicationInfo::builder()
            .application_name(app_name)
            .engine_name(engine_name)
            .api_version(vk::API_VERSION_1_1)
            .build();

        let mut extension_names = ash_window_handle::enumerate_required_extensions(raw_display_handle)
            .map_err(|err| format!("enumerate_required_extensions failed: {err:?}"))?
            .to_vec();
        let surface_extension = khr::surface::NAME.as_ptr();
        if !extension_names.contains(&surface_extension) {
            extension_names.push(surface_extension);
        }

        let instance_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .build();
        let instance = unsafe { entry.create_instance(&instance_info, None) }.map_err(|err| format!("create_instance failed: {err:?}"))?;
        let mut bootstrap = VulkanBootstrap::new(instance);

        let surface_loader = Surface::new(&entry, bootstrap.instance());
        bootstrap.surface_loader = Some(surface_loader);
        let surface = unsafe { ash_window_handle::create_surface(&entry, bootstrap.instance(), raw_display_handle, raw_window_handle, None) }
            .map_err(|err| format!("create_surface failed: {err:?}"))?;
        bootstrap.surface = surface;

        let (physical_device, queue_indices) = Self::select_physical_device(bootstrap.instance(), bootstrap.surface_loader(), surface)?;

        let device = Self::create_logical_device(bootstrap.instance(), physical_device, &queue_indices)?;
        bootstrap.device = Some(device);
        let graphics_queue = unsafe { bootstrap.device().get_device_queue(queue_indices.graphics_family, 0) };
        let present_queue = unsafe { bootstrap.device().get_device_queue(queue_indices.present_family, 0) };

        let depth_format = Self::find_depth_format(bootstrap.instance(), physical_device)?;
        let (instance, surface_loader, surface, device) = bootstrap.commit();
        let swapchain_loader = Swapchain::new(&instance, &device);

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
            acquired_frame_state: AcquiredFrameState::Ready,
            ui: None,
            depth_images: Vec::new(),
            mesh: None,
            logical_width: width,
            logical_height: height,
            swapchain_generation: 0,
            device_lost: false,
        };

        ctx.command_pool = ctx.create_command_pool()?;
        ctx.recreate_swapchain(width, height)?;
        ctx.create_sync_objects()?;
        ctx.allocate_transfer_command_buffers()?;

        Ok(ctx)
    }

    /// Picks the first physical device that exposes both graphics and present queue families.
    fn select_physical_device(instance: &ash::Instance, surface_loader: &Surface, surface: vk::SurfaceKHR) -> Result<(vk::PhysicalDevice, QueueFamilyIndices)> {
        let devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| format!("enumerate_physical_devices failed: {err:?}"))?;

        for device in devices {
            if let Some(indices) = Self::find_queue_families(instance, surface_loader, surface, device) {
                return Ok((device, indices));
            }
        }

        Err("no suitable Vulkan physical device found".into())
    }

    /// Finds queue families that can render and present to the supplied surface.
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

    /// Creates the logical device and queues needed by the example renderer.
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

    /// Rebuilds all swapchain-dependent resources for a new window extent.
    fn recreate_swapchain(&mut self, width: u32, height: u32) -> Result<()> {
        if self.device_lost {
            return Err("cannot recreate swapchain: device is lost".into());
        }
        if width == 0 || height == 0 {
            return Ok(());
        }

        unsafe {
            self.device.device_wait_idle().map_err(|err| self.handle_vk_error("device_wait_idle", err))?;
        }

        // Rebuild all resources tied to the window surface size or swapchain format. Permanent
        // objects like the device, queues, and command pool stay alive across this boundary.
        self.cleanup_swapchain();
        self.create_swapchain(width, height)?;
        self.create_image_views()?;
        self.create_depth_images()?;
        self.render_pass = self.create_render_pass()?;
        self.framebuffers = self.create_framebuffers()?;
        self.allocate_command_buffers()?;
        // Construct the complete replacement while the installed UI aggregate still owns the
        // immutable atlas. If pipeline, sampler, pool, or descriptor creation fails, `self.ui`
        // remains the exact owner needed by a later rebuild retry instead of dropping the atlas.
        let replacement = UiResources::new(self);
        let device = &self.device;
        let previous = commit_resource_replacement(&mut self.ui, replacement, |previous, replacement| {
            if let Some(atlas) = previous.atlas.take() {
                // Descriptor writes are infallible Vulkan commands. Rebind before moving the image
                // into its new aggregate, leaving no fallible step after ownership transfer starts.
                replacement.update_descriptor(device, replacement.descriptor_set, &atlas);
                replacement.atlas = Some(atlas);
            }
        })?;
        if let Some(mut previous) = previous {
            // The transferred atlas is no longer present in `previous`; destroy only resources
            // tied to the obsolete render pass and descriptor pool after publication succeeds.
            previous.destroy(&self.device);
        }
        self.swapchain_generation = self.swapchain_generation.wrapping_add(1);

        Ok(())
    }

    /// Creates the Vulkan swapchain that backs presentation to the SDL window.
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

        // Do not publish a swapchain until its image list can also be queried. The guard closes
        // the otherwise easy-to-miss leak when `get_swapchain_images` fails.
        let swapchain = ResourceGuard::new(
            unsafe { self.swapchain_loader.create_swapchain(&create_info, None) }.map_err(|err| format!("create_swapchain failed: {err:?}"))?,
            |swapchain| unsafe { self.swapchain_loader.destroy_swapchain(swapchain, None) },
        );
        let images = unsafe { self.swapchain_loader.get_swapchain_images(*swapchain.get()) }.map_err(|err| format!("get_swapchain_images failed: {err:?}"))?;
        let previous = std::mem::replace(&mut self.swapchain, swapchain.into_inner());
        if previous != vk::SwapchainKHR::null() {
            unsafe { self.swapchain_loader.destroy_swapchain(previous, None) };
        }
        self.swapchain_images = images;
        self.swapchain_format = surface_format.format;
        self.extent = extent;

        Ok(())
    }

    /// Creates image views for every swapchain image.
    fn create_image_views(&mut self) -> Result<()> {
        // Keep every newly created view guarded until the full image set succeeds; `collect` over
        // raw Vulkan handles would leak the successful prefix on the first error.
        let mut pending = Vec::with_capacity(self.swapchain_images.len());
        for &image in &self.swapchain_images {
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
            let view = unsafe { self.device.create_image_view(&view_info, None) }.map_err(|err| format!("create_image_view failed: {err:?}"))?;
            pending.push(ResourceGuard::new(view, |view| unsafe { self.device.destroy_image_view(view, None) }));
        }
        let views = pending.into_iter().map(|view| view.into_inner()).collect();
        for previous in std::mem::replace(&mut self.swapchain_image_views, views) {
            unsafe { self.device.destroy_image_view(previous, None) };
        }
        Ok(())
    }

    /// Builds the render pass used for UI and mesh drawing.
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

    /// Creates one framebuffer per swapchain image view.
    fn create_framebuffers(&self) -> Result<Vec<vk::Framebuffer>> {
        let mut pending = Vec::with_capacity(self.swapchain_image_views.len());
        for (index, &view) in self.swapchain_image_views.iter().enumerate() {
            let depth_view = self.depth_images.get(index).map(|image| image.view).unwrap_or(vk::ImageView::null());
            let attachments = [view, depth_view];
            let framebuffer_info = vk::FramebufferCreateInfo::builder()
                .render_pass(self.render_pass)
                .attachments(&attachments)
                .width(self.extent.width)
                .height(self.extent.height)
                .layers(1);
            let framebuffer =
                unsafe { self.device.create_framebuffer(&framebuffer_info, None) }.map_err(|err| format!("create_framebuffer failed: {err:?}"))?;
            pending.push(ResourceGuard::new(framebuffer, |framebuffer| unsafe {
                self.device.destroy_framebuffer(framebuffer, None)
            }));
        }
        Ok(pending.into_iter().map(|framebuffer| framebuffer.into_inner()).collect())
    }

    /// Creates the shared command pool used for graphics and transient copy work.
    fn create_command_pool(&self) -> Result<vk::CommandPool> {
        let pool_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(self.queue_indices.graphics_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        unsafe { self.device.create_command_pool(&pool_info, None) }.map_err(|err| format!("create_command_pool failed: {err:?}"))
    }

    /// Allocates one graphics command buffer per swapchain image.
    fn allocate_command_buffers(&mut self) -> Result<()> {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(self.framebuffers.len() as u32);
        // Allocate replacement buffers before releasing the currently valid set.
        let buffers = unsafe { self.device.allocate_command_buffers(&alloc_info) }.map_err(|err| format!("allocate_command_buffers failed: {err:?}"))?;
        let previous = std::mem::replace(&mut self.command_buffers, buffers);
        if !previous.is_empty() {
            unsafe { self.device.free_command_buffers(self.command_pool, &previous) };
        }
        Ok(())
    }

    /// Allocates reusable transfer command buffers, one per frame-in-flight slot.
    fn allocate_transfer_command_buffers(&mut self) -> Result<()> {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(self.max_frames_in_flight as u32);
        let buffers =
            unsafe { self.device.allocate_command_buffers(&alloc_info) }.map_err(|err| format!("allocate_command_buffers (transfer) failed: {err:?}"))?;
        let previous = std::mem::replace(&mut self.transfer_command_buffers, buffers);
        if !previous.is_empty() {
            unsafe { self.device.free_command_buffers(self.command_pool, &previous) };
        }
        self.transfer_recording = vec![false; self.max_frames_in_flight];
        self.transfer_has_work = vec![false; self.max_frames_in_flight];
        Ok(())
    }

    /// Creates per-frame semaphores and fences used by acquire, transfer, and present.
    fn create_sync_objects(&mut self) -> Result<()> {
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED).build();

        // Build a complete replacement set under guards. A failure in any frame slot destroys all
        // semaphores/fences created for earlier slots and preserves the installed set.
        let mut image_available = Vec::with_capacity(self.max_frames_in_flight);
        let mut render_finished = Vec::with_capacity(self.max_frames_in_flight);
        let mut transfer_complete = Vec::with_capacity(self.max_frames_in_flight);
        let mut fences = Vec::with_capacity(self.max_frames_in_flight);

        for _ in 0..self.max_frames_in_flight {
            unsafe {
                let image_available_handle = self
                    .device
                    .create_semaphore(&semaphore_info, None)
                    .map_err(|err| format!("create_semaphore failed: {err:?}"))?;
                let image_available_guard = ResourceGuard::new(image_available_handle, |semaphore| self.device.destroy_semaphore(semaphore, None));
                let render_finished_handle = self
                    .device
                    .create_semaphore(&semaphore_info, None)
                    .map_err(|err| format!("create_semaphore failed: {err:?}"))?;
                let render_finished_guard = ResourceGuard::new(render_finished_handle, |semaphore| self.device.destroy_semaphore(semaphore, None));
                let transfer_complete_handle = self
                    .device
                    .create_semaphore(&semaphore_info, None)
                    .map_err(|err| format!("create_semaphore failed: {err:?}"))?;
                let transfer_complete_guard = ResourceGuard::new(transfer_complete_handle, |semaphore| self.device.destroy_semaphore(semaphore, None));
                let fence_handle = self
                    .device
                    .create_fence(&fence_info, None)
                    .map_err(|err| format!("create_fence failed: {err:?}"))?;
                let fence_guard = ResourceGuard::new(fence_handle, |fence| self.device.destroy_fence(fence, None));

                image_available.push(image_available_guard);
                render_finished.push(render_finished_guard);
                transfer_complete.push(transfer_complete_guard);
                fences.push(fence_guard);
            }
        }

        let image_available = image_available.into_iter().map(|guard| guard.into_inner()).collect();
        let render_finished = render_finished.into_iter().map(|guard| guard.into_inner()).collect();
        let transfer_complete = transfer_complete.into_iter().map(|guard| guard.into_inner()).collect();
        let fences = fences.into_iter().map(|guard| guard.into_inner()).collect();
        for semaphore in std::mem::replace(&mut self.image_available_semaphores, image_available) {
            unsafe { self.device.destroy_semaphore(semaphore, None) };
        }
        for semaphore in std::mem::replace(&mut self.render_finished_semaphores, render_finished) {
            unsafe { self.device.destroy_semaphore(semaphore, None) };
        }
        for semaphore in std::mem::replace(&mut self.transfer_complete_semaphores, transfer_complete) {
            unsafe { self.device.destroy_semaphore(semaphore, None) };
        }
        for fence in std::mem::replace(&mut self.in_flight_fences, fences) {
            unsafe { self.device.destroy_fence(fence, None) };
        }

        Ok(())
    }

    /// Destroys resources that depend on the current swapchain and framebuffer set.
    fn cleanup_swapchain(&mut self) {
        unsafe {
            // Only swapchain-dependent resources are destroyed here. The permanent command pool,
            // device, queues, and synchronization objects live until `Drop`.
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

    /// Acquires one swapchain image before higher-level display-list execution begins.
    fn acquire_frame(&mut self, width: u32, height: u32) -> Result<AcquiredVulkanFrame> {
        if self.device_lost {
            return Err("device is lost; VulkanContext is disabled".into());
        }
        // Never reuse a binary acquire semaphore or swapchain image left outstanding by an earlier
        // post-acquire failure. The fatal policy makes this check finite instead of waiting on an
        // unsignaled fence that was reset but never submitted.
        self.acquired_frame_state.ensure_ready()?;
        self.logical_width = width;
        self.logical_height = height;
        let frame = self.current_frame;
        let fence = self.in_flight_fences[frame];
        unsafe {
            self.device
                .wait_for_fences(&[fence], true, u64::MAX)
                .map_err(|err| self.handle_vk_error("wait_for_fences", err))?;
        }

        match unsafe {
            self.swapchain_loader
                .acquire_next_image(self.swapchain, u64::MAX, self.image_available_semaphores[frame], vk::Fence::null())
        } {
            Ok((image_index, suboptimal)) => {
                // Publish the ownership transition only after Vulkan has returned a real image and
                // signaled this frame slot's acquire semaphore.
                self.acquired_frame_state.acquire_succeeded()?;
                Ok(AcquiredVulkanFrame { frame, image_index, suboptimal })
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {
                self.recreate_swapchain(width, height)?;
                Err(String::from("Vulkan swapchain changed during frame acquisition"))
            }
            Err(err) => Err(self.handle_vk_error("acquire_next_image", err)),
        }
    }

    /// Executes one complete Vulkan frame from an acquired image through submit and present.
    #[allow(clippy::too_many_arguments)] // Frame recording keeps Vulkan state explicit at the call boundary.
    fn draw_frame(
        &mut self,
        acquired: AcquiredVulkanFrame,
        clear_value: vk::ClearValue,
        vertices: &[Vertex],
        width: u32,
        height: u32,
        _frame_index: u64,
        commands: &mut Vec<FrameCommand>,
    ) -> Result<()> {
        // Every exit after successful acquisition passes through the lifecycle commit below. Any
        // error permanently latches the context before the caller can attempt another frame with a
        // signaled acquire semaphore, an unpresented image, or an unsignaled unsubmitted fence.
        let result = self.draw_acquired_frame(acquired, clear_value, vertices, width, height, _frame_index, commands);

        // Success proves graphics submission and presentation consumed the acquired resources.
        // Every error latches Fatal, including record, transfer, fence-reset, submit, present, and
        // swapchain-rebuild failures.
        self.acquired_frame_state.finish(result)
    }

    /// Performs the fallible native work for one already-acquired swapchain image.
    #[allow(clippy::too_many_arguments)] // Frame recording keeps Vulkan state explicit at the call boundary.
    fn draw_acquired_frame(
        &mut self,
        acquired: AcquiredVulkanFrame,
        clear_value: vk::ClearValue,
        vertices: &[Vertex],
        width: u32,
        height: u32,
        _frame_index: u64,
        commands: &mut Vec<FrameCommand>,
    ) -> Result<()> {
        if self.device_lost {
            return Err("device is lost; VulkanContext is disabled".into());
        }
        // `draw_frame` receives the image acquired by `RendererBackend::frame`; by the time it
        // runs, the higher-level renderer has collected UI vertices plus ordered custom jobs.
        let AcquiredVulkanFrame { frame, image_index, suboptimal } = acquired;
        debug_assert_eq!(frame, self.current_frame);
        let fence = self.in_flight_fences[frame];

        // Reset per-frame upload cursors only after the frame fence signaled. Retired GPU buffers
        // are likewise only reclaimed once this frame slot is no longer in flight.
        self.reset_transfer_state(frame)?;
        self.reset_ui_offset(frame);
        if let Some(ref mut mesh) = self.mesh {
            mesh.reset_upload_state(frame, &self.device);
        }
        self.preflight_frame_resources(vertices, commands.as_slice())?;

        let mut swapchain_needs_recreate = suboptimal;

        // UI drawing and custom rendering share one graphics command buffer per acquired image.
        let command_buffer = self.command_buffers[image_index as usize];
        self.record_command_buffer(command_buffer, image_index, clear_value, vertices, width, height, _frame_index, commands)?;
        let transfer_semaphore = self.submit_transfer_commands()?;

        let mut wait_semaphores = vec![self.image_available_semaphores[frame]];
        let mut wait_stages = vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        if let Some(semaphore) = transfer_semaphore {
            wait_semaphores.push(semaphore);
            wait_stages.push(vk::PipelineStageFlags::VERTEX_INPUT);
        }
        let signal_semaphores = [self.render_finished_semaphores[frame]];

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(std::slice::from_ref(&command_buffer))
            .signal_semaphores(&signal_semaphores);
        let submit_infos = [submit_info.build()];

        unsafe {
            // Keep the fence signaled throughout every fallible CPU recording and transfer-submit
            // step. Reset it only when the graphics submission is fully assembled. If reset or
            // queue submission fails, the outer acquired-frame transaction becomes fatal and no
            // later frame can wait on or reuse this slot.
            self.device.reset_fences(&[fence]).map_err(|err| self.handle_vk_error("reset_fences", err))?;
            self.device
                .queue_submit(self.graphics_queue, &submit_infos, fence)
                .map_err(|err| self.handle_vk_error("queue_submit", err))?;
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
            Err(err) => return Err(self.handle_vk_error("queue_present", err)),
        }

        if swapchain_needs_recreate {
            self.recreate_swapchain(width, height)?;
        }

        self.current_frame = (self.current_frame + 1) % self.max_frames_in_flight;

        Ok(())
    }

    /// Estimates this frame's upload budget and grows GPU/staging buffers before recording begins.
    fn preflight_frame_resources(&mut self, vertices: &[Vertex], commands: &[FrameCommand]) -> Result<()> {
        // Pre-compute the worst-case upload budget for this frame before any command buffer
        // recording begins. That keeps the actual record path free of buffer growth decisions.
        let mut budget = FrameResourceBudget::default();
        let mut cursor = 0usize;
        for command in commands {
            match command {
                FrameCommand::DrawTo(end_index) => {
                    let end = (*end_index).min(vertices.len());
                    if end > cursor {
                        budget.ui_vertices = budget.ui_vertices.saturating_add(end - cursor);
                    }
                    cursor = end;
                }
                FrameCommand::Custom(job) => {
                    budget.ui_vertices = budget.ui_vertices.saturating_add(job.budget.ui_vertices);
                    budget.mesh_vertices = budget.mesh_vertices.saturating_add(job.budget.mesh_vertices);
                    budget.mesh_indices = budget.mesh_indices.saturating_add(job.budget.mesh_indices);
                }
            }
        }
        if cursor < vertices.len() {
            budget.ui_vertices = budget.ui_vertices.saturating_add(vertices.len() - cursor);
        }

        let to_bytes = |count: usize, element_size: usize| -> Result<vk::DeviceSize> {
            let bytes = count.checked_mul(element_size).ok_or_else(|| "frame upload size overflow".to_string())?;
            u64::try_from(bytes).map_err(|_| "frame upload size exceeds device address range".to_string())
        };

        let frame = self.current_frame;
        let ui_bytes = to_bytes(budget.ui_vertices, std::mem::size_of::<Vertex>())?;
        if let Some(mut ui) = self.ui.take() {
            let result = ui.prepare_frame(self, frame, ui_bytes);
            self.ui = Some(ui);
            result?;
        }

        if budget.mesh_vertices > 0 && budget.mesh_indices > 0 {
            let mesh_vertex_bytes = to_bytes(budget.mesh_vertices, std::mem::size_of::<MeshVertex>())?;
            let mesh_index_bytes = to_bytes(budget.mesh_indices, std::mem::size_of::<u32>())?;
            let mut mesh = match self.mesh.take() {
                Some(existing) => existing,
                None => MeshResources::new(self)?,
            };
            let result = mesh.prepare_frame(self, frame, mesh_vertex_bytes, mesh_index_bytes);
            self.mesh = Some(mesh);
            result?;
        }

        Ok(())
    }

    /// Records the graphics command buffer by replaying queued UI and custom commands in order.
    #[allow(clippy::too_many_arguments)] // Command recording keeps Vulkan state explicit at the call boundary.
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
        // GPU command recording is intentionally linear: begin render pass, replay the queued UI /
        // custom jobs in order, then end the pass. The `commands` vec is drained here so the
        // higher-level renderer can reuse its allocation next frame.
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
        // UI resources must be moved out to satisfy the callback borrow pattern. Run recording in
        // a local transaction, then restore the aggregate before propagating any error; destroying
        // it here would also be unsafe while another frame may still reference its pipeline.
        let mut ui = self.ui.take();
        let record_result: Result<()> = (|| {
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
                        // Custom callbacks may need mutable access to the context and its helper
                        // recorders. Temporarily returning UI resources to `self` avoids nested
                        // mutable borrows while keeping command ordering intact.
                        if let Some(ui_resources) = ui.take() {
                            self.ui = Some(ui_resources);
                        }
                        job.callback.record(self, command_buffer, self.extent, &job.area);
                        ui = self.ui.take();
                    }
                }
            }

            if let Some(ref mut ui) = ui
                && cursor < vertices.len()
            {
                ui.record(self, command_buffer, &vertices[cursor..], frame_width, frame_height)?;
            }
            Ok(())
        })();
        if let Some(ui) = ui {
            self.ui = Some(ui);
        }
        record_result?;

        unsafe {
            self.device.cmd_end_render_pass(command_buffer);
            self.device
                .end_command_buffer(command_buffer)
                .map_err(|err| format!("end_command_buffer failed: {err:?}"))?;
        }

        Ok(())
    }

    /// Chooses the preferred swapchain surface format, falling back to the first available one.
    fn choose_surface_format(available_formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
        available_formats
            .iter()
            .cloned()
            .find(|format| format.format == vk::Format::B8G8R8A8_UNORM)
            .unwrap_or_else(|| available_formats[0])
    }

    /// Finds a depth format supported for depth-stencil attachments on the selected device.
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

    /// Prefers mailbox presentation when available, otherwise falls back to FIFO.
    fn choose_present_mode(available_present_modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
        if available_present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        }
    }

    /// Resolves the swapchain extent from surface caps and the requested window size.
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

    /// Converts a Vulkan error into a user-facing string and latches device-lost state.
    fn handle_vk_error(&mut self, op: &str, err: vk::Result) -> String {
        if err == vk::Result::ERROR_DEVICE_LOST {
            self.device_lost = true;
            format!("{op} failed: {err:?} (device lost)")
        } else {
            format!("{op} failed: {err:?}")
        }
    }

    /// Latches an acquired frame that escaped normal Result-based finalization.
    fn abandon_acquired_frame(&mut self) {
        // VulkanFrame's panic guard calls this before allowing another backend operation.
        self.acquired_frame_state.abandon();
    }

    /// Returns whether native rendering must remain disabled for the rest of this context's life.
    fn is_unusable(&self) -> bool {
        // Actual device loss and an abandoned acquired-frame transaction are both permanent here;
        // this example deliberately chooses a simple fatal policy instead of partial sync repair.
        self.device_lost || self.acquired_frame_state.is_fatal()
    }

    /// Returns the current swapchain extent.
    fn extent(&self) -> vk::Extent2D {
        self.extent
    }
    /// Returns the generation counter that increments on every swapchain rebuild.
    fn swapchain_generation(&self) -> u64 {
        self.swapchain_generation
    }
    /// Uploads the shared immutable microui atlas into the current UI resources.
    fn upload_atlas(&mut self, atlas: &AtlasHandle) -> Result<()> {
        if let Some(mut ui) = self.ui.take() {
            let result = ui.upload_atlas(self, atlas);
            self.ui = Some(ui);
            result
        } else {
            Ok(())
        }
    }

    /// Returns the descriptor set used for atlas-backed UI draws, if initialized.
    fn ui_descriptor_set(&self) -> Option<vk::DescriptorSet> {
        self.ui.as_ref().map(|ui| ui.descriptor_set)
    }

    /// Records a custom UI draw using an explicit descriptor set instead of the shared atlas.
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

    /// Resets per-frame UI upload cursors and retires buffers for the completed frame slot.
    fn reset_ui_offset(&mut self, frame: usize) {
        if let Some(ref mut ui) = self.ui {
            ui.reset_frame_offsets(frame);
            ui.cleanup_retired(frame, &self.device);
        }
    }

    /// Records a UI draw using the provided descriptor set and optional custom clip area.
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

    /// Records a 3D mesh submission against the shared mesh pipeline resources.
    fn record_mesh(&mut self, command_buffer: vk::CommandBuffer, submission: &MeshSubmission, area: &CustomRenderArea) -> Result<()> {
        let mut resources = match self.mesh.take() {
            Some(resources) => resources,
            None => MeshResources::new(self)?,
        };
        let result = resources.record(self, command_buffer, submission, area);
        self.mesh = Some(resources);
        result
    }

    /// Creates a Vulkan buffer and allocates/binds memory matching the requested usage.
    fn create_buffer(&self, size: vk::DeviceSize, usage: vk::BufferUsageFlags, properties: vk::MemoryPropertyFlags) -> Result<Buffer> {
        let info = vk::BufferCreateInfo::builder().size(size).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);
        // Vulkan buffer creation is not atomic: the raw buffer and its allocation are separate
        // objects. Guard each immediately, and commit neither until binding succeeds.
        let buffer = ResourceGuard::new(
            unsafe { self.device.create_buffer(&info, None) }.map_err(|err| format!("create_buffer failed: {err:?}"))?,
            |buffer| unsafe { self.device.destroy_buffer(buffer, None) },
        );
        let requirements = unsafe { self.device.get_buffer_memory_requirements(*buffer.get()) };
        let memory_type = self.find_memory_type(requirements.memory_type_bits, properties)?;
        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = ResourceGuard::new(
            unsafe { self.device.allocate_memory(&alloc_info, None) }.map_err(|err| format!("allocate_memory failed: {err:?}"))?,
            |memory| unsafe { self.device.free_memory(memory, None) },
        );
        unsafe { self.device.bind_buffer_memory(*buffer.get(), *memory.get(), 0) }.map_err(|err| format!("bind_buffer_memory failed: {err:?}"))?;
        Ok(Buffer {
            buffer: buffer.into_inner(),
            memory: memory.into_inner(),
            size,
        })
    }

    /// Writes data into a buffer starting at offset zero.
    fn write_buffer(&self, buffer: &Buffer, data: &[u8]) -> Result<()> {
        self.write_buffer_offset(buffer, 0, data)
    }

    /// Maps buffer memory, copies `data` into it at `offset`, and unmaps it again.
    fn write_buffer_offset(&self, buffer: &Buffer, offset: vk::DeviceSize, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        unsafe {
            let mapped = self
                .device
                .map_memory(buffer.memory, offset, data.len() as u64, vk::MemoryMapFlags::empty())
                .map_err(|err| format!("map_memory failed: {err:?}"))?;
            // Treat a successful mapping as a scoped resource as well: future edits can add a
            // fallible validation/copy step without accidentally skipping `unmap_memory`.
            let mapped = ResourceGuard::new(mapped, |_| self.device.unmap_memory(buffer.memory));
            ptr::copy_nonoverlapping(data.as_ptr(), *mapped.get() as *mut u8, data.len());
        }
        Ok(())
    }

    /// Creates a sampled 2D image resource suitable for atlas or texture uploads.
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
        // Image, memory, and view are guarded separately because every edge between those
        // allocations can fail. The aggregate becomes visible only after all three exist.
        let image = ResourceGuard::new(
            unsafe { self.device.create_image(&image_info, None) }.map_err(|err| format!("create_image failed: {err:?}"))?,
            |image| unsafe { self.device.destroy_image(image, None) },
        );
        let requirements = unsafe { self.device.get_image_memory_requirements(*image.get()) };
        let memory_type = self.find_memory_type(requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        let alloc = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = ResourceGuard::new(
            unsafe { self.device.allocate_memory(&alloc, None) }.map_err(|err| format!("allocate_memory failed: {err:?}"))?,
            |memory| unsafe { self.device.free_memory(memory, None) },
        );
        unsafe { self.device.bind_image_memory(*image.get(), *memory.get(), 0) }.map_err(|err| format!("bind_image_memory failed: {err:?}"))?;

        let view_info = vk::ImageViewCreateInfo::builder()
            .image(*image.get())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1)
                    .build(),
            );
        let view = ResourceGuard::new(
            unsafe { self.device.create_image_view(&view_info, None) }.map_err(|err| format!("create_image_view failed: {err:?}"))?,
            |view| unsafe { self.device.destroy_image_view(view, None) },
        );

        Ok(ImageResource {
            image: image.into_inner(),
            memory: memory.into_inner(),
            view: view.into_inner(),
            extent: vk::Extent2D { width, height },
            format,
            layout: vk::ImageLayout::UNDEFINED,
        })
    }

    /// Recreates the per-swapchain-image depth attachments for the current extent.
    fn create_depth_images(&mut self) -> Result<()> {
        // A failed nth attachment must clean the first n-1 and leave the installed set alone.
        let mut attachments = Vec::with_capacity(self.swapchain_images.len());
        for _ in &self.swapchain_images {
            attachments.push(self.create_depth_attachment(self.extent)?.guarded(&self.device));
        }
        let attachments = attachments.into_iter().map(ResourceGuard::into_inner).collect();
        for mut previous in std::mem::replace(&mut self.depth_images, attachments) {
            previous.destroy(&self.device);
        }
        Ok(())
    }

    /// Creates one depth attachment image and transitions it into attachment layout.
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
        let image = ResourceGuard::new(
            unsafe { self.device.create_image(&image_info, None) }.map_err(|err| format!("create_image failed: {err:?}"))?,
            |image| unsafe { self.device.destroy_image(image, None) },
        );
        let requirements = unsafe { self.device.get_image_memory_requirements(*image.get()) };
        let memory_type = self.find_memory_type(requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        let alloc = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let memory = ResourceGuard::new(
            unsafe { self.device.allocate_memory(&alloc, None) }.map_err(|err| format!("allocate_memory failed: {err:?}"))?,
            |memory| unsafe { self.device.free_memory(memory, None) },
        );
        unsafe { self.device.bind_image_memory(*image.get(), *memory.get(), 0) }.map_err(|err| format!("bind_image_memory failed: {err:?}"))?;

        let aspect = if Self::has_stencil_component(format) {
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        } else {
            vk::ImageAspectFlags::DEPTH
        };

        self.single_time_commands(|cmd| {
            self.transition_image_layout(
                cmd,
                *image.get(),
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                aspect,
            );
        })?;

        let view_info = vk::ImageViewCreateInfo::builder()
            .image(*image.get())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange::builder().aspect_mask(aspect).level_count(1).layer_count(1).build());
        let view = ResourceGuard::new(
            unsafe { self.device.create_image_view(&view_info, None) }.map_err(|err| format!("create_image_view failed: {err:?}"))?,
            |view| unsafe { self.device.destroy_image_view(view, None) },
        );

        Ok(ImageResource {
            image: image.into_inner(),
            memory: memory.into_inner(),
            view: view.into_inner(),
            extent,
            format,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        })
    }

    /// Returns whether the chosen depth format also carries a stencil component.
    fn has_stencil_component(format: vk::Format) -> bool {
        matches!(format, vk::Format::D32_SFLOAT_S8_UINT | vk::Format::D24_UNORM_S8_UINT)
    }

    /// Uploads a staging buffer into an image and transitions it to shader-read layout.
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

    /// Begins or reuses the current frame's transfer command buffer.
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

    /// Queues a buffer-to-buffer transfer plus the barrier required for later GPU reads.
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

    /// Ends the current frame's transfer command buffer if recording started.
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

    /// Submits the current frame's transfer work and returns the semaphore it signals, if any.
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

    /// Resets transfer bookkeeping for the current frame slot before new uploads are recorded.
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

    /// Inserts the buffer memory barrier that makes transfer writes visible to later pipeline stages.
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

    /// Creates a sampled texture image from raw RGBA pixels and allocates its descriptor set.
    fn create_texture_resource(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<VulkanTexture> {
        let width_u32 = u32::try_from(width).map_err(|_| "texture width out of range".to_string())?;
        let height_u32 = u32::try_from(height).map_err(|_| "texture height out of range".to_string())?;
        // Image and staging buffer stay pending through upload and descriptor allocation. This
        // covers map/copy/command-buffer/pool failures without hand-written cleanup branches.
        let mut image = self.create_image_resource(width_u32, height_u32)?.guarded(&self.device);

        let staging = self
            .create_buffer(
                pixels.len() as u64,
                vk::BufferUsageFlags::TRANSFER_SRC,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?
            .guarded(&self.device);
        self.write_buffer(staging.get(), pixels)?;
        self.copy_buffer_to_image(staging.get(), image.get_mut())?;

        let descriptor = self.allocate_texture_descriptor(image.get())?.guarded(&self.device);
        Ok(VulkanTexture {
            image: image.into_inner(),
            descriptor: descriptor.into_inner(),
        })
    }

    /// Allocates a texture descriptor set from the UI descriptor pool for the supplied image.
    fn allocate_texture_descriptor(&mut self, image: &ImageResource) -> Result<TextureDescriptor> {
        let mut ui = self.ui.take().ok_or_else(|| "UI resources not initialized".to_string())?;
        // Always restore the aggregate owner before propagating descriptor-pool exhaustion.
        let descriptor_set = ui.allocate_texture_descriptor(self, image);
        self.ui = Some(ui);
        descriptor_set
    }

    /// Frees a texture descriptor only while the pool generation that allocated it is live.
    fn free_texture_descriptor(&self, descriptor: &mut TextureDescriptor) {
        if descriptor.set == vk::DescriptorSet::null() {
            return;
        }
        if let Some(ui) = self.ui.as_ref()
            && descriptor.belongs_to(self.swapchain_generation, ui.descriptor_pool)
        {
            unsafe {
                // The pool was created with `FREE_DESCRIPTOR_SET`; a valid set normally cannot
                // fail to free. On device loss the enclosing pool still reclaims it during Drop.
                let _ = self.device.free_descriptor_sets(descriptor.pool, &[descriptor.set]);
            }
        }
        descriptor.set = vk::DescriptorSet::null();
        descriptor.pool = vk::DescriptorPool::null();
    }

    /// Runs a one-off command buffer for setup work like image layout transitions.
    fn single_time_commands<F: FnOnce(vk::CommandBuffer)>(&self, f: F) -> Result<()> {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // Free the transient command buffer on begin/end/submit/wait errors as well as success.
        let command_buffer =
            unsafe { self.device.allocate_command_buffers(&alloc_info) }.map_err(|err| format!("allocate_command_buffers failed: {err:?}"))?[0];
        let command_buffer = ResourceGuard::new(command_buffer, |command_buffer| unsafe {
            self.device.free_command_buffers(self.command_pool, &[command_buffer])
        });
        let begin_info = vk::CommandBufferBeginInfo::builder().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(*command_buffer.get(), &begin_info)
                .map_err(|err| format!("begin_command_buffer failed: {err:?}"))?;
        }
        f(*command_buffer.get());
        unsafe {
            self.device
                .end_command_buffer(*command_buffer.get())
                .map_err(|err| format!("end_command_buffer failed: {err:?}"))?;
        }
        let command_buffers = [*command_buffer.get()];
        let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);
        let submit_infos = [submit_info.build()];
        unsafe {
            self.device
                .queue_submit(self.graphics_queue, &submit_infos, vk::Fence::null())
                .map_err(|err| format!("queue_submit failed: {err:?}"))?;
            self.device
                .queue_wait_idle(self.graphics_queue)
                .map_err(|err| format!("queue_wait_idle failed: {err:?}"))?;
        }
        Ok(())
    }

    /// Records an image layout transition barrier for the supplied image/subresource range.
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

    /// Finds a device memory type that satisfies the allocation bitmask and property flags.
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
            // The drop order mirrors creation order in reverse: wait for the device to go idle,
            // destroy per-frame sync/swapchain resources, then tear down the command pool, surface,
            // device, and finally the instance.
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
            for &semaphore in &self.transfer_complete_semaphores {
                self.device.destroy_semaphore(semaphore, None);
            }

            if let Some(mut ui) = self.ui.take() {
                ui.destroy(&self.device);
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

#[cfg(test)]
mod tests {
    use ash::vk::Handle;
    use std::cell::Cell;

    use super::*;

    /// Minimal aggregate used to fault-inject rebuild construction without a Vulkan device.
    #[derive(Debug, Eq, PartialEq)]
    struct TestResources {
        /// Distinguishes the installed and replacement generations.
        generation: u8,
        /// Stand-in for the uniquely owned immutable atlas image.
        atlas: Option<u8>,
    }

    /// Verifies failed construction retains the installed owner and successful commit moves it.
    #[test]
    fn resource_replacement_preserves_owner_on_failure_then_transfers_it_on_commit() {
        let mut current = Some(TestResources { generation: 1, atlas: Some(42) });
        let transfer_called = Cell::new(false);

        // Inject a constructor failure. The transfer callback must not run and the authoritative
        // aggregate must retain the exact atlas owner for a later rebuild attempt.
        let failed = commit_resource_replacement(&mut current, Err::<TestResources, _>("injected construction failure"), |_, _| {
            transfer_called.set(true)
        });
        assert_eq!(failed.unwrap_err(), "injected construction failure");
        assert!(!transfer_called.get());
        assert_eq!(current, Some(TestResources { generation: 1, atlas: Some(42) }));

        let previous = commit_resource_replacement(
            &mut current,
            Ok::<_, &str>(TestResources { generation: 2, atlas: None }),
            |previous, replacement| replacement.atlas = previous.atlas.take(),
        )
        .unwrap()
        .unwrap();

        assert_eq!(previous.atlas, None, "the obsolete aggregate must no longer own the atlas");
        assert_eq!(current.unwrap().atlas, Some(42), "the published replacement must own the same atlas");
    }

    /// Verifies every post-acquire failure and panic permanently closes the frame lifecycle.
    #[test]
    fn acquired_frame_failure_is_fatal_while_success_returns_to_ready() {
        let mut state = AcquiredFrameState::Ready;
        state.ensure_ready().unwrap();
        state.acquire_succeeded().unwrap();
        assert_eq!(state, AcquiredFrameState::Acquired);

        let error = state.finish::<()>(Err(String::from("injected record failure"))).unwrap_err();
        assert_eq!(error, "injected record failure");
        assert!(state.is_fatal());
        assert!(state.ensure_ready().is_err(), "a failed acquired frame must reject all later acquisition");

        let mut successful = AcquiredFrameState::Ready;
        successful.acquire_succeeded().unwrap();
        successful.finish(Ok(())).unwrap();
        assert_eq!(successful, AcquiredFrameState::Ready);

        successful.acquire_succeeded().unwrap();
        successful.abandon();
        assert!(successful.is_fatal(), "panic-style abandonment must use the same fatal latch");
    }

    #[test]
    fn external_texture_pool_supports_individual_descriptor_reclamation() {
        assert!(DESCRIPTOR_POOL_FLAGS.contains(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET));
    }

    #[test]
    fn descriptor_reclamation_requires_the_exact_live_pool_generation() {
        let pool = vk::DescriptorPool::from_raw(7);
        let descriptor = TextureDescriptor {
            set: vk::DescriptorSet::from_raw(11),
            pool,
            generation: 3,
        };

        assert!(descriptor.belongs_to(3, pool));
        // Generation prevents a numerically reused pool handle from accepting a stale set.
        assert!(!descriptor.belongs_to(4, pool));
        assert!(!descriptor.belongs_to(3, vk::DescriptorPool::from_raw(8)));
    }
}
