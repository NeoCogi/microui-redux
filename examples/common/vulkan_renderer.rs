// Vulkan renderer skeleton for microui-redux
// Copyright 2024-Present (c) Raja Lehtihet & Wael El Oraiby
//
// This file provides a Vulkan-based implementation of the Renderer trait.

use std::sync::Arc;
use ash::{vk, Entry, Instance, Device};
use ash::vk::Handle;
use super::*;
use std::ffi::CString;
use ash::util::read_spv;
use std::io::Cursor;
use rs_math3d::transforms::ortho4;

pub struct VulkanRenderer {
    pub entry: Entry,
    pub instance: Instance,
    pub surface: vk::SurfaceKHR,
    pub atlas: AtlasHandle,
    pub device: Device,
    pub physical_device: vk::PhysicalDevice,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub queue: vk::Queue,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub surface_format: vk::SurfaceFormatKHR,
    pub present_mode: vk::PresentModeKHR,
    pub extent: vk::Extent2D,
    // UI batching
    verts: Vec<Vertex>,
    indices: Vec<u16>,
    pub vertex_buffer: vk::Buffer,
    pub vertex_buffer_memory: vk::DeviceMemory,
    pub index_buffer: vk::Buffer,
    pub index_buffer_memory: vk::DeviceMemory,
    pub render_pass: vk::RenderPass,
    pub pipeline_layout: vk::PipelineLayout,
    pub pipeline: vk::Pipeline,
    pub framebuffers: Vec<vk::Framebuffer>,
    // Texture support
    pub texture_image: vk::Image,
    pub texture_image_memory: vk::DeviceMemory,
    pub texture_image_view: vk::ImageView,
    pub texture_sampler: vk::Sampler,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set: vk::DescriptorSet,
    // Projection matrix
    projection_matrix: [f32; 16],
}

impl VulkanRenderer {
    fn find_memory_type(type_filter: u32, properties: vk::MemoryPropertyFlags, mem_properties: &vk::PhysicalDeviceMemoryProperties) -> u32 {
        for i in 0..mem_properties.memory_type_count {
            if (type_filter & (1 << i)) != 0 && (mem_properties.memory_types[i as usize].property_flags & properties) == properties {
                return i;
            }
        }
        panic!("Failed to find suitable memory type!");
    }

    fn create_texture_image(device: &Device, instance: &Instance, pdevice: vk::PhysicalDevice, width: u32, height: u32) -> (vk::Image, vk::DeviceMemory) {
        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .format(vk::Format::R8G8B8A8_UNORM)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1);

        let image = unsafe { device.create_image(&image_info, None).unwrap() };
        let mem_requirements = unsafe { device.get_image_memory_requirements(image) };
        let mem_type = Self::find_memory_type(mem_requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL, unsafe {
            &instance.get_physical_device_memory_properties(pdevice)
        });
        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(mem_requirements.size)
            .memory_type_index(mem_type);
        let memory = unsafe { device.allocate_memory(&alloc_info, None).unwrap() };
        unsafe { device.bind_image_memory(image, memory, 0).unwrap() };
        (image, memory)
    }

    fn create_texture_image_view(device: &Device, image: vk::Image) -> vk::ImageView {
        let view_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        unsafe { device.create_image_view(&view_info, None).unwrap() }
    }

    fn create_texture_sampler(device: &Device) -> vk::Sampler {
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .anisotropy_enable(false)
            .max_anisotropy(1.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);
        unsafe { device.create_sampler(&sampler_info, None).unwrap() }
    }

    fn create_descriptor_set_layout(device: &Device) -> vk::DescriptorSetLayout {
        let sampler_layout_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build();
        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(std::slice::from_ref(&sampler_layout_binding));
        unsafe { device.create_descriptor_set_layout(&layout_info, None).unwrap() }
    }

    fn create_descriptor_pool(device: &Device) -> vk::DescriptorPool {
        let pool_sizes = [vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .build()];
        let pool_info = vk::DescriptorPoolCreateInfo::builder().pool_sizes(&pool_sizes).max_sets(1);
        unsafe { device.create_descriptor_pool(&pool_info, None).unwrap() }
    }

    fn create_descriptor_set(
        device: &Device,
        pool: vk::DescriptorPool,
        layout: vk::DescriptorSetLayout,
        image_view: vk::ImageView,
        sampler: vk::Sampler,
    ) -> vk::DescriptorSet {
        let layouts = [layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::builder().descriptor_pool(pool).set_layouts(&layouts);
        let descriptor_set = unsafe { device.allocate_descriptor_sets(&alloc_info).unwrap()[0] };

        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(image_view)
            .sampler(sampler)
            .build();
        let write_descriptor_set = vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info))
            .build();
        unsafe { device.update_descriptor_sets(std::slice::from_ref(&write_descriptor_set), &[]) };
        descriptor_set
    }

    pub fn new_with_surface(atlas: AtlasHandle, entry: Entry, instance: Instance, surface: vk::SurfaceKHR) -> Self {
        // 1. Pick a physical device
        let pdevices = unsafe { instance.enumerate_physical_devices().unwrap() };
        let pdevice = pdevices[0];
        // Find a queue family that supports both graphics and present
        let surface_loader = ash::extensions::khr::Surface::new(&entry, &instance);
        let queue_families = unsafe { instance.get_physical_device_queue_family_properties(pdevice) };
        let mut queue_family_index = None;
        for (i, qf) in queue_families.iter().enumerate() {
            let supports_graphics = qf.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            let supports_present = unsafe { surface_loader.get_physical_device_surface_support(pdevice, i as u32, surface).unwrap() };
            if supports_graphics && supports_present {
                queue_family_index = Some(i as u32);
                break;
            }
        }
        let queue_family_index = queue_family_index.expect("No suitable queue family found");

        // 2. Create logical device and queue
        let device_extensions = [ash::extensions::khr::Swapchain::name().as_ptr()];
        let queue_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&[1.0]);
        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&device_extensions);
        let device = unsafe { instance.create_device(pdevice, &device_create_info, None).unwrap() };
        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        // 3. Surface format, present mode, extent
        let surface_loader = ash::extensions::khr::Surface::new(&entry, &instance);
        let formats = unsafe { surface_loader.get_physical_device_surface_formats(pdevice, surface).unwrap() };
        let surface_format = formats.iter().find(|f| f.format == vk::Format::B8G8R8A8_UNORM).cloned().unwrap_or(formats[0]);
        let present_modes = unsafe { surface_loader.get_physical_device_surface_present_modes(pdevice, surface).unwrap() };
        let present_mode = present_modes
            .iter()
            .cloned()
            .find(|&m| m == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);
        let caps = unsafe { surface_loader.get_physical_device_surface_capabilities(pdevice, surface).unwrap() };
        let extent = caps.current_extent;

        // 4. Create swapchain
        let swapchain_loader = ash::extensions::khr::Swapchain::new(&instance, &device);
        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface)
            .min_image_count(caps.min_image_count + 1)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(vk::SwapchainKHR::null());
        let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None).unwrap() };
        let swapchain_images: Vec<vk::Image> = unsafe { swapchain_loader.get_swapchain_images(swapchain).unwrap() };
        let swapchain_image_views: Vec<vk::ImageView> = swapchain_images
            .iter()
            .map(|&image| {
                let create_info = vk::ImageViewCreateInfo::builder()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(surface_format.format)
                    .components(vk::ComponentMapping::default())
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });
                unsafe { device.create_image_view(&create_info, None).unwrap() }
            })
            .collect();

        // 5. Command pool and buffers
        let command_pool_info = vk::CommandPoolCreateInfo::builder().queue_family_index(queue_family_index);
        let command_pool = unsafe { device.create_command_pool(&command_pool_info, None).unwrap() };
        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);
        let command_buffers = unsafe { device.allocate_command_buffers(&command_buffer_allocate_info).unwrap() };

        // 6. Create vertex and index buffers (host visible for now)
        let vertex_buffer_size = 65536 * std::mem::size_of::<Vertex>() as vk::DeviceSize;
        let index_buffer_size = 65536 * std::mem::size_of::<u16>() as vk::DeviceSize;
        let buffer_info = |size, usage| vk::BufferCreateInfo::builder().size(size).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);
        let vertex_buffer = unsafe {
            device
                .create_buffer(&buffer_info(vertex_buffer_size, vk::BufferUsageFlags::VERTEX_BUFFER), None)
                .unwrap()
        };
        let index_buffer = unsafe {
            device
                .create_buffer(&buffer_info(index_buffer_size, vk::BufferUsageFlags::INDEX_BUFFER), None)
                .unwrap()
        };
        // Allocate memory (host visible)
        let mem_properties = unsafe { instance.get_physical_device_memory_properties(pdevice) };
        let vmem_reqs = unsafe { device.get_buffer_memory_requirements(vertex_buffer) };
        let imem_reqs = unsafe { device.get_buffer_memory_requirements(index_buffer) };
        let vmem_type_index = Self::find_memory_type(
            vmem_reqs.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            &mem_properties,
        );
        let imem_type_index = Self::find_memory_type(
            imem_reqs.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            &mem_properties,
        );
        let valloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(vmem_reqs.size)
            .memory_type_index(vmem_type_index);
        let vertex_buffer_memory = unsafe { device.allocate_memory(&valloc_info, None).unwrap() };
        unsafe { device.bind_buffer_memory(vertex_buffer, vertex_buffer_memory, 0).unwrap() };
        let ialloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(imem_reqs.size)
            .memory_type_index(imem_type_index);
        let index_buffer_memory = unsafe { device.allocate_memory(&ialloc_info, None).unwrap() };
        unsafe { device.bind_buffer_memory(index_buffer, index_buffer_memory, 0).unwrap() };

        // 7. Create render pass
        let color_attachment = vk::AttachmentDescription::builder()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build();
        let color_attachment_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();
        let subpass = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&color_attachment_ref))
            .build();
        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(std::slice::from_ref(&color_attachment))
            .subpasses(std::slice::from_ref(&subpass));
        let render_pass = unsafe { device.create_render_pass(&render_pass_info, None).unwrap() };
        // 8. Create texture and descriptor set layout
        let descriptor_set_layout = Self::create_descriptor_set_layout(&device);
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(std::mem::size_of::<[f32; 16]>() as u32)
            .build();
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(std::slice::from_ref(&descriptor_set_layout))
            .push_constant_ranges(std::slice::from_ref(&push_constant_range));
        let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None).unwrap() };
        // 9. Create shaders (hardcoded SPIR-V for now)
        let vert_spv = include_bytes!("ui.vert.spv");
        let frag_spv = include_bytes!("ui.frag.spv");
        let vert_code = read_spv(&mut Cursor::new(&vert_spv[..])).unwrap();
        let frag_code = read_spv(&mut Cursor::new(&frag_spv[..])).unwrap();
        let vert_shader_module = unsafe {
            let info = vk::ShaderModuleCreateInfo::builder().code(&vert_code);
            device.create_shader_module(&info, None).unwrap()
        };
        let frag_shader_module = unsafe {
            let info = vk::ShaderModuleCreateInfo::builder().code(&frag_code);
            device.create_shader_module(&info, None).unwrap()
        };
        let entry_point = std::ffi::CString::new("main").unwrap();
        let shader_stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_shader_module)
                .name(&entry_point)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_shader_module)
                .name(&entry_point)
                .build(),
        ];
        // Vertex input
        let vertex_binding = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        let vertex_attributes = [
            vk::VertexInputAttributeDescription::builder()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0)
                .build(), // pos
            vk::VertexInputAttributeDescription::builder()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(8)
                .build(), // tex
            vk::VertexInputAttributeDescription::builder()
                .binding(0)
                .location(2)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(16)
                .build(), // color
        ];
        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(std::slice::from_ref(&vertex_binding))
            .vertex_attribute_descriptions(&vertex_attributes);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);
        let viewport = vk::Viewport::builder()
            .x(0.0)
            .y(0.0)
            .width(extent.width as f32)
            .height(extent.height as f32)
            .min_depth(0.0)
            .max_depth(1.0)
            .build();
        let scissor = vk::Rect2D::builder().offset(vk::Offset2D { x: 0, y: 0 }).extent(extent).build();
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(std::slice::from_ref(&viewport))
            .scissors(std::slice::from_ref(&scissor));
        let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false);
        let multisampling = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B | vk::ColorComponentFlags::A)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build();
        let color_blending = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .attachments(std::slice::from_ref(&color_blend_attachment));
        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);
        let pipeline = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pipeline_info), None)
                .unwrap()[0]
        };

        // Create framebuffers for each swapchain image view
        let framebuffers: Vec<vk::Framebuffer> = swapchain_image_views
            .iter()
            .map(|&view| {
                let attachments = [view];
                let framebuffer_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(render_pass)
                    .attachments(&attachments)
                    .width(extent.width)
                    .height(extent.height)
                    .layers(1);
                unsafe { device.create_framebuffer(&framebuffer_info, None).unwrap() }
            })
            .collect();

        // Create texture and descriptor set
        let (texture_image, texture_image_memory) = Self::create_texture_image(&device, &instance, pdevice, atlas.width() as u32, atlas.height() as u32);
        let texture_image_view = Self::create_texture_image_view(&device, texture_image);
        let texture_sampler = Self::create_texture_sampler(&device);
        let descriptor_pool = Self::create_descriptor_pool(&device);
        let descriptor_set = Self::create_descriptor_set(&device, descriptor_pool, descriptor_set_layout, texture_image_view, texture_sampler);

        // Create orthographic projection matrix (flip Y for Vulkan)
        let proj_matrix = ortho4(0.0, extent.width as f32, 0.0, extent.height as f32, -1.0, 1.0);
        let projection_matrix = [
            proj_matrix.col[0].x,
            proj_matrix.col[0].y,
            proj_matrix.col[0].z,
            proj_matrix.col[0].w,
            proj_matrix.col[1].x,
            proj_matrix.col[1].y,
            proj_matrix.col[1].z,
            proj_matrix.col[1].w,
            proj_matrix.col[2].x,
            proj_matrix.col[2].y,
            proj_matrix.col[2].z,
            proj_matrix.col[2].w,
            proj_matrix.col[3].x,
            proj_matrix.col[3].y,
            proj_matrix.col[3].z,
            proj_matrix.col[3].w,
        ];

        Self {
            entry,
            instance,
            surface,
            atlas,
            device,
            physical_device: pdevice,
            swapchain,
            swapchain_images,
            swapchain_image_views,
            queue,
            command_pool,
            command_buffers,
            surface_format,
            present_mode,
            extent,
            verts: Vec::new(),
            indices: Vec::new(),
            vertex_buffer,
            vertex_buffer_memory,
            index_buffer,
            index_buffer_memory,
            render_pass,
            pipeline_layout,
            pipeline,
            framebuffers,
            // Texture support
            texture_image,
            texture_image_memory,
            texture_image_view,
            texture_sampler,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            projection_matrix,
        }
    }

    fn update_atlas_texture(&mut self) {
        // Check if atlas has been updated
        static mut LAST_UPDATE_ID: usize = usize::MAX;
        unsafe {
            if LAST_UPDATE_ID != self.atlas.get_last_update_id() {
                // Create a staging buffer to upload texture data
                let width = self.atlas.width() as u32;
                let height = self.atlas.height() as u32;
                let image_size = (width * height * 4) as vk::DeviceSize;

                let staging_buffer_info = vk::BufferCreateInfo::builder()
                    .size(image_size)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE);
                let staging_buffer = self.device.create_buffer(&staging_buffer_info, None).unwrap();

                let mem_requirements = self.device.get_buffer_memory_requirements(staging_buffer);
                let mem_type = Self::find_memory_type(
                    mem_requirements.memory_type_bits,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                    unsafe { &self.instance.get_physical_device_memory_properties(self.physical_device) },
                );
                let alloc_info = vk::MemoryAllocateInfo::builder()
                    .allocation_size(mem_requirements.size)
                    .memory_type_index(mem_type);
                let staging_memory = self.device.allocate_memory(&alloc_info, None).unwrap();
                self.device.bind_buffer_memory(staging_buffer, staging_memory, 0).unwrap();

                // Copy atlas data to staging buffer
                let data = self.device.map_memory(staging_memory, 0, image_size, vk::MemoryMapFlags::empty()).unwrap();
                self.atlas.apply_pixels(|w, h, pixels| {
                    let data_slice = std::slice::from_raw_parts_mut(data as *mut u8, (w * h * 4) as usize);
                    for (i, pixel) in pixels.iter().enumerate() {
                        let offset = i * 4;
                        data_slice[offset] = pixel.x; // R
                        data_slice[offset + 1] = pixel.y; // G
                        data_slice[offset + 2] = pixel.z; // B
                        data_slice[offset + 3] = pixel.w; // A
                    }
                });
                self.device.unmap_memory(staging_memory);

                // Create command buffer for texture upload
                let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                    .command_pool(self.command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1);
                let upload_command_buffer = self.device.allocate_command_buffers(&command_buffer_allocate_info).unwrap()[0];

                let begin_info = vk::CommandBufferBeginInfo::builder().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
                self.device.begin_command_buffer(upload_command_buffer, &begin_info).unwrap();

                // Transition texture to transfer destination
                let barrier1 = vk::ImageMemoryBarrier::builder()
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .image(self.texture_image)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .build();

                self.device.cmd_pipeline_barrier(
                    upload_command_buffer,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier1],
                );

                // Copy buffer to image
                let buffer_image_copy = vk::BufferImageCopy::builder()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D { width, height, depth: 1 })
                    .build();

                self.device.cmd_copy_buffer_to_image(
                    upload_command_buffer,
                    staging_buffer,
                    self.texture_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[buffer_image_copy],
                );

                // Transition texture to shader read
                let barrier2 = vk::ImageMemoryBarrier::builder()
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image(self.texture_image)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .build();

                self.device.cmd_pipeline_barrier(
                    upload_command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier2],
                );

                self.device.end_command_buffer(upload_command_buffer).unwrap();

                // Submit upload command
                let submit_info = vk::SubmitInfo::builder().command_buffers(&[upload_command_buffer]).build();
                self.device.queue_submit(self.queue, &[submit_info], vk::Fence::null()).unwrap();
                self.device.queue_wait_idle(self.queue).unwrap();

                // Cleanup
                self.device.free_command_buffers(self.command_pool, &[upload_command_buffer]);
                self.device.destroy_buffer(staging_buffer, None);
                self.device.free_memory(staging_memory, None);

                LAST_UPDATE_ID = self.atlas.get_last_update_id();
            }
        }
    }

    pub fn clear_color(&mut self, color: [f32; 4]) {
        // Record and submit a command buffer to clear the first swapchain image
        // (for demo, not robust)
        let image_index = 0;
        let command_buffer = self.command_buffers[image_index];
        let begin_info = vk::CommandBufferBeginInfo::builder();
        unsafe {
            self.device.begin_command_buffer(command_buffer, &begin_info).unwrap();
            let clear_value = vk::ClearValue {
                color: vk::ClearColorValue { float32: color },
            };
            let image_subresource_range = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };
            self.device.cmd_clear_color_image(
                command_buffer,
                self.swapchain_images[image_index],
                vk::ImageLayout::GENERAL,
                &clear_value.color,
                &[image_subresource_range],
            );
            self.device.end_command_buffer(command_buffer).unwrap();
        }
        // TODO: Submit and present
    }

    fn record_draw(&mut self, command_buffer: vk::CommandBuffer, image_index: usize) {
        // Reset and begin command buffer
        unsafe {
            self.device.reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty()).unwrap();
            let begin_info = vk::CommandBufferBeginInfo::builder();
            self.device.begin_command_buffer(command_buffer, &begin_info).unwrap();
        }
        // Transition swapchain image to COLOR_ATTACHMENT_OPTIMAL
        let image = self.swapchain_images[image_index];
        let barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image(image)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .build();
        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }
        self.update_atlas_texture();
        self.upload_buffers();
        // Begin render pass
        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue { float32: [0.5, 0.5, 0.5, 1.0] }, // Same as OpenGL: color(0x7F, 0x7F, 0x7F, 255)
        };
        let render_pass_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index])
            .clear_values(std::slice::from_ref(&clear_value))
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            });
        unsafe {
            self.device
                .cmd_begin_render_pass(command_buffer, &render_pass_info, vk::SubpassContents::INLINE);
            self.device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            self.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );
            self.device.cmd_bind_vertex_buffers(command_buffer, 0, &[self.vertex_buffer], &[0]);
            self.device.cmd_bind_index_buffer(command_buffer, self.index_buffer, 0, vk::IndexType::UINT16);
            // Set push constant for orthographic projection
            let proj_bytes = std::slice::from_raw_parts(self.projection_matrix.as_ptr() as *const u8, 64);
            self.device
                .cmd_push_constants(command_buffer, self.pipeline_layout, vk::ShaderStageFlags::VERTEX, 0, proj_bytes);
            if self.indices.len() > 0 {
                self.device.cmd_draw_indexed(command_buffer, self.indices.len() as u32, 1, 0, 0, 0);
            }
            self.device.cmd_end_render_pass(command_buffer);
        }
        self.verts.clear();
        self.indices.clear();
        unsafe {
            self.device.end_command_buffer(command_buffer).unwrap();
        }
    }

    pub fn present(&mut self) {
        let swapchain_loader = ash::extensions::khr::Swapchain::new(&self.instance, &self.device);

        // Create synchronization objects (one-shot for demo)
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let image_available = unsafe { self.device.create_semaphore(&semaphore_info, None).unwrap() };
        let render_finished = unsafe { self.device.create_semaphore(&semaphore_info, None).unwrap() };
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight_fence = unsafe { self.device.create_fence(&fence_info, None).unwrap() };

        // Acquire next image
        let (image_index, _) = unsafe {
            swapchain_loader
                .acquire_next_image(self.swapchain, std::u64::MAX, image_available, vk::Fence::null())
                .unwrap()
        };
        let command_buffer = self.command_buffers[image_index as usize];
        // End all uses of device before this point
        // Record draw after device is no longer borrowed immutably
        self.record_draw(command_buffer, image_index as usize);

        // Submit command buffer
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&[image_available])
            .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
            .command_buffers(&[command_buffer])
            .signal_semaphores(&[render_finished])
            .build();
        unsafe {
            self.device.queue_submit(self.queue, &[submit_info], in_flight_fence).unwrap();
        }

        // Present
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&[render_finished])
            .swapchains(&[self.swapchain])
            .image_indices(&[image_index])
            .build();
        unsafe {
            swapchain_loader.queue_present(self.queue, &present_info).unwrap();
            self.device.wait_for_fences(&[in_flight_fence], true, std::u64::MAX).unwrap();
            self.device.destroy_semaphore(image_available, None);
            self.device.destroy_semaphore(render_finished, None);
            self.device.destroy_fence(in_flight_fence, None);
        }
    }

    fn upload_buffers(&mut self) {
        // Upload verts
        let vsize = self.verts.len() * std::mem::size_of::<Vertex>();
        if vsize > 0 {
            unsafe {
                let data = self
                    .device
                    .map_memory(self.vertex_buffer_memory, 0, vsize as u64, vk::MemoryMapFlags::empty())
                    .unwrap();
                std::ptr::copy_nonoverlapping(self.verts.as_ptr() as *const u8, data as *mut u8, vsize);
                self.device.unmap_memory(self.vertex_buffer_memory);
            }
        }
        // Upload indices
        let isize = self.indices.len() * std::mem::size_of::<u16>();
        if isize > 0 {
            unsafe {
                let data = self
                    .device
                    .map_memory(self.index_buffer_memory, 0, isize as u64, vk::MemoryMapFlags::empty())
                    .unwrap();
                std::ptr::copy_nonoverlapping(self.indices.as_ptr() as *const u8, data as *mut u8, isize);
                self.device.unmap_memory(self.index_buffer_memory);
            }
        }
    }
}

impl Renderer for VulkanRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {
        self.verts.clear();
        self.indices.clear();
        // TODO: Set up frame, clear, etc.
    }

    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        if self.verts.len() + 4 >= 65536 || self.indices.len() + 6 >= 65536 {
            self.flush();
        }
        let is = self.verts.len() as u16;
        self.indices.push(is + 0);
        self.indices.push(is + 1);
        self.indices.push(is + 2);
        self.indices.push(is + 2);
        self.indices.push(is + 3);
        self.indices.push(is + 0);
        self.verts.push(v0.clone());
        self.verts.push(v1.clone());
        self.verts.push(v2.clone());
        self.verts.push(v3.clone());
    }

    fn flush(&mut self) {
        // No-op for now, upload happens in record_draw
        // Do not clear verts/indices here; clear after drawing in record_draw
    }

    fn end(&mut self) {
        self.flush();
    }
}
