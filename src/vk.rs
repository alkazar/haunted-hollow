//! Vulkan renderer (ash). Two pipelines share one texture atlas:
//! - `world`: depth-tested voxel geometry (static world mesh + per-frame dynamic mesh)
//! - `overlay`: alpha-blended 2D HUD quads
//! Everything is drawn as quads with a shared quad index buffer.

use ash::khr::{surface, swapchain};
use ash::{vk, Device, Entry, Instance};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::ffi::CStr;
use std::io::Cursor;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PushConstants {
    pub viewproj: [[f32; 4]; 4],
    pub cam: [f32; 4],
    pub fog: [f32; 4],
    pub params: [f32; 4],
}

const FRAMES: usize = 2;
pub const MAX_DYN_QUADS: usize = 160_000;
pub const MAX_UI_QUADS: usize = 40_000;

struct Buffer {
    buf: vk::Buffer,
    mem: vk::DeviceMemory,
    ptr: *mut u8,
    size: u64,
}

struct Img {
    image: vk::Image,
    mem: vk::DeviceMemory,
    view: vk::ImageView,
}

pub struct Renderer {
    _entry: Entry,
    instance: Instance,
    surface_fn: surface::Instance,
    surface: vk::SurfaceKHR,
    pdev: vk::PhysicalDevice,
    mem_props: vk::PhysicalDeviceMemoryProperties,
    device: Device,
    queue: vk::Queue,
    swap_fn: swapchain::Device,
    swapchain: vk::SwapchainKHR,
    format: vk::Format,
    pub extent: vk::Extent2D,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    depth: Option<Img>,
    render_pass: vk::RenderPass,
    set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    world_pipe: vk::Pipeline,
    ui_pipe: vk::Pipeline,
    desc_pool: vk::DescriptorPool,
    desc_set: vk::DescriptorSet,
    atlas: Img,
    sampler: vk::Sampler,
    cmd_pool: vk::CommandPool,
    cmds: Vec<vk::CommandBuffer>,
    img_avail: Vec<vk::Semaphore>,
    render_done: Vec<vk::Semaphore>,
    fences: Vec<vk::Fence>,
    frame: usize,
    static_vb: Option<Buffer>,
    static_quads: u32,
    index_buf: Option<Buffer>,
    index_quads: usize,
    dyn_vb: Vec<Buffer>,
    ui_vb: Vec<Buffer>,
    capture_supported: bool,
    pub needs_resize: bool,
    pub device_name: String,
}

pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Renderer {
    pub fn new(
        window: &(impl HasDisplayHandle + HasWindowHandle),
        width: u32,
        height: u32,
        atlas_rgba: &[u8],
        atlas_size: u32,
    ) -> Result<Self, String> {
        unsafe { Self::init(window, width, height, atlas_rgba, atlas_size) }
    }

    unsafe fn init(
        window: &(impl HasDisplayHandle + HasWindowHandle),
        width: u32,
        height: u32,
        atlas_rgba: &[u8],
        atlas_size: u32,
    ) -> Result<Self, String> {
        let entry = Entry::load().map_err(|e| format!("Could not load Vulkan: {e}"))?;
        let display = window.display_handle().map_err(|e| e.to_string())?.as_raw();
        let win = window.window_handle().map_err(|e| e.to_string())?.as_raw();

        let app_name = c"Haunted Hollow";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .engine_name(app_name)
            .api_version(vk::API_VERSION_1_1);
        let exts = ash_window::enumerate_required_extensions(display).map_err(|e| e.to_string())?;
        let ici = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(exts);
        let instance = entry.create_instance(&ici, None).map_err(|e| format!("create_instance: {e}"))?;
        let surface_fn = surface::Instance::new(&entry, &instance);
        let surface = ash_window::create_surface(&entry, &instance, display, win, None)
            .map_err(|e| format!("create_surface: {e}"))?;

        // Pick a GPU (prefer discrete) with a queue that does graphics + present.
        let pdevs = instance.enumerate_physical_devices().map_err(|e| e.to_string())?;
        let mut best: Option<(i32, vk::PhysicalDevice, u32)> = None;
        for pd in pdevs {
            let props = instance.get_physical_device_properties(pd);
            let qprops = instance.get_physical_device_queue_family_properties(pd);
            for (i, q) in qprops.iter().enumerate() {
                let present = surface_fn
                    .get_physical_device_surface_support(pd, i as u32, surface)
                    .unwrap_or(false);
                if q.queue_flags.contains(vk::QueueFlags::GRAPHICS) && present {
                    let score = match props.device_type {
                        vk::PhysicalDeviceType::DISCRETE_GPU => 3,
                        vk::PhysicalDeviceType::INTEGRATED_GPU => 2,
                        vk::PhysicalDeviceType::VIRTUAL_GPU => 1,
                        _ => 0,
                    };
                    if best.map_or(true, |b| score > b.0) {
                        best = Some((score, pd, i as u32));
                    }
                    break;
                }
            }
        }
        let (_, pdev, qfi) = best.ok_or("No Vulkan GPU with graphics+present support found")?;
        let props = instance.get_physical_device_properties(pdev);
        let device_name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy().into_owned();
        let mem_props = instance.get_physical_device_memory_properties(pdev);

        let prio = [1.0f32];
        let qci = [vk::DeviceQueueCreateInfo::default().queue_family_index(qfi).queue_priorities(&prio)];
        let dev_exts = [swapchain::NAME.as_ptr()];
        let dci = vk::DeviceCreateInfo::default()
            .queue_create_infos(&qci)
            .enabled_extension_names(&dev_exts);
        let device = instance.create_device(pdev, &dci, None).map_err(|e| format!("create_device: {e}"))?;
        let queue = device.get_device_queue(qfi, 0);
        let swap_fn = swapchain::Device::new(&instance, &device);

        // Surface format: prefer UNORM so our sRGB-authored colours pass straight through.
        let formats = surface_fn
            .get_physical_device_surface_formats(pdev, surface)
            .map_err(|e| e.to_string())?;
        let fmt = formats
            .iter()
            .find(|f| f.format == vk::Format::B8G8R8A8_UNORM)
            .or_else(|| formats.iter().find(|f| f.format == vk::Format::R8G8B8A8_UNORM))
            .unwrap_or(&formats[0]);
        let format = fmt.format;

        let caps = surface_fn
            .get_physical_device_surface_capabilities(pdev, surface)
            .map_err(|e| e.to_string())?;
        let capture_supported = caps.supported_usage_flags.contains(vk::ImageUsageFlags::TRANSFER_SRC);

        // Render pass: colour + depth.
        let attachments = [
            vk::AttachmentDescription::default()
                .format(format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR),
            vk::AttachmentDescription::default()
                .format(vk::Format::D32_SFLOAT)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
        ];
        let color_ref = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let depth_ref = vk::AttachmentReference::default()
            .attachment(1)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let subpass = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_ref)
            .depth_stencil_attachment(&depth_ref)];
        let deps = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            )
            .dst_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            )
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            )];
        let rpci = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpass)
            .dependencies(&deps);
        let render_pass = device.create_render_pass(&rpci, None).map_err(|e| e.to_string())?;

        // Descriptors: sampled image + sampler (naga-friendly, no combined sampler).
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        let set_layout = device
            .create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None)
            .map_err(|e| e.to_string())?;
        let pc_range = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<PushConstants>() as u32)];
        let layouts = [set_layout];
        let pipeline_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&layouts)
                    .push_constant_ranges(&pc_range),
                None,
            )
            .map_err(|e| e.to_string())?;

        let world_spv = include_bytes!(concat!(env!("OUT_DIR"), "/world.spv"));
        let ui_spv = include_bytes!(concat!(env!("OUT_DIR"), "/overlay.spv"));
        let world_pipe = make_pipeline(&device, render_pass, pipeline_layout, world_spv, true)?;
        let ui_pipe = make_pipeline(&device, render_pass, pipeline_layout, ui_spv, false)?;

        let cmd_pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(qfi)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
            .map_err(|e| e.to_string())?;
        let cmds = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(cmd_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(FRAMES as u32),
            )
            .map_err(|e| e.to_string())?;

        let mut r = Renderer {
            _entry: entry,
            instance,
            surface_fn,
            surface,
            pdev,
            mem_props,
            device,
            queue,
            swap_fn,
            swapchain: vk::SwapchainKHR::null(),
            format,
            extent: vk::Extent2D { width, height },
            images: vec![],
            views: vec![],
            framebuffers: vec![],
            depth: None,
            render_pass,
            set_layout,
            pipeline_layout,
            world_pipe,
            ui_pipe,
            desc_pool: vk::DescriptorPool::null(),
            desc_set: vk::DescriptorSet::null(),
            atlas: Img { image: vk::Image::null(), mem: vk::DeviceMemory::null(), view: vk::ImageView::null() },
            sampler: vk::Sampler::null(),
            cmd_pool,
            cmds,
            img_avail: vec![],
            render_done: vec![],
            fences: vec![],
            frame: 0,
            static_vb: None,
            static_quads: 0,
            index_buf: None,
            index_quads: 0,
            dyn_vb: vec![],
            ui_vb: vec![],
            capture_supported,
            needs_resize: false,
            device_name,
        };
        r.create_swapchain(width, height)?;
        r.upload_atlas(atlas_rgba, atlas_size)?;

        // Descriptor set.
        let sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLED_IMAGE, descriptor_count: 1 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLER, descriptor_count: 1 },
        ];
        r.desc_pool = r
            .device
            .create_descriptor_pool(&vk::DescriptorPoolCreateInfo::default().pool_sizes(&sizes).max_sets(1), None)
            .map_err(|e| e.to_string())?;
        r.desc_set = r
            .device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(r.desc_pool)
                    .set_layouts(&layouts),
            )
            .map_err(|e| e.to_string())?[0];
        let img_info = [vk::DescriptorImageInfo::default()
            .image_view(r.atlas.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let samp_info = [vk::DescriptorImageInfo::default().sampler(r.sampler)];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(r.desc_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&img_info),
            vk::WriteDescriptorSet::default()
                .dst_set(r.desc_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&samp_info),
        ];
        r.device.update_descriptor_sets(&writes, &[]);

        for _ in 0..FRAMES {
            let s = r.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).map_err(|e| e.to_string())?;
            r.img_avail.push(s);
            let f = r
                .device
                .create_fence(&vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED), None)
                .map_err(|e| e.to_string())?;
            r.fences.push(f);
            let dvb = r.create_buffer(
                (MAX_DYN_QUADS * 4 * std::mem::size_of::<Vertex>()) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                true,
            )?;
            r.dyn_vb.push(dvb);
            let uvb = r.create_buffer(
                (MAX_UI_QUADS * 4 * std::mem::size_of::<Vertex>()) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                true,
            )?;
            r.ui_vb.push(uvb);
        }
        r.ensure_index_buffer(MAX_DYN_QUADS.max(MAX_UI_QUADS))?;
        Ok(r)
    }

    fn find_mem(&self, bits: u32, flags: vk::MemoryPropertyFlags) -> Option<u32> {
        (0..self.mem_props.memory_type_count).find(|&i| {
            bits & (1 << i) != 0 && self.mem_props.memory_types[i as usize].property_flags.contains(flags)
        })
    }

    fn create_buffer(&self, size: u64, usage: vk::BufferUsageFlags, host: bool) -> Result<Buffer, String> {
        unsafe {
            let buf = self
                .device
                .create_buffer(
                    &vk::BufferCreateInfo::default()
                        .size(size.max(16))
                        .usage(usage)
                        .sharing_mode(vk::SharingMode::EXCLUSIVE),
                    None,
                )
                .map_err(|e| e.to_string())?;
            let req = self.device.get_buffer_memory_requirements(buf);
            let flags = if host {
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
            } else {
                vk::MemoryPropertyFlags::DEVICE_LOCAL
            };
            let mt = self
                .find_mem(req.memory_type_bits, flags)
                .or_else(|| self.find_mem(req.memory_type_bits, vk::MemoryPropertyFlags::empty()))
                .ok_or("no memory type")?;
            let mem = self
                .device
                .allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(mt), None)
                .map_err(|e| format!("allocate {size} bytes: {e}"))?;
            self.device.bind_buffer_memory(buf, mem, 0).map_err(|e| e.to_string())?;
            let ptr = if host {
                self.device
                    .map_memory(mem, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                    .map_err(|e| e.to_string())? as *mut u8
            } else {
                std::ptr::null_mut()
            };
            Ok(Buffer { buf, mem, ptr, size })
        }
    }

    fn destroy_buffer(&self, b: Buffer) {
        unsafe {
            if !b.ptr.is_null() {
                self.device.unmap_memory(b.mem);
            }
            self.device.destroy_buffer(b.buf, None);
            self.device.free_memory(b.mem, None);
        }
    }

    fn one_shot<F: FnOnce(vk::CommandBuffer)>(&self, f: F) -> Result<(), String> {
        unsafe {
            let cb = self
                .device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(self.cmd_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .map_err(|e| e.to_string())?[0];
            self.device
                .begin_command_buffer(
                    cb,
                    &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(|e| e.to_string())?;
            f(cb);
            self.device.end_command_buffer(cb).map_err(|e| e.to_string())?;
            let cbs = [cb];
            let submit = [vk::SubmitInfo::default().command_buffers(&cbs)];
            self.device.queue_submit(self.queue, &submit, vk::Fence::null()).map_err(|e| e.to_string())?;
            self.device.queue_wait_idle(self.queue).map_err(|e| e.to_string())?;
            self.device.free_command_buffers(self.cmd_pool, &cbs);
            Ok(())
        }
    }

    /// Uploads data into a new device-local buffer through a staging buffer.
    fn device_local_buffer(&self, data: &[u8], usage: vk::BufferUsageFlags) -> Result<Buffer, String> {
        let staging = self.create_buffer(data.len() as u64, vk::BufferUsageFlags::TRANSFER_SRC, true)?;
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), staging.ptr, data.len()) };
        let dst = self.create_buffer(data.len() as u64, usage | vk::BufferUsageFlags::TRANSFER_DST, false)?;
        self.one_shot(|cb| unsafe {
            let region = [vk::BufferCopy { src_offset: 0, dst_offset: 0, size: data.len() as u64 }];
            self.device.cmd_copy_buffer(cb, staging.buf, dst.buf, &region);
        })?;
        self.destroy_buffer(staging);
        Ok(dst)
    }

    fn ensure_index_buffer(&mut self, quads: usize) -> Result<(), String> {
        if quads <= self.index_quads {
            return Ok(());
        }
        unsafe { self.device.device_wait_idle().ok() };
        if let Some(b) = self.index_buf.take() {
            self.destroy_buffer(b);
        }
        let mut idx: Vec<u32> = Vec::with_capacity(quads * 6);
        for q in 0..quads as u32 {
            let b = q * 4;
            idx.extend_from_slice(&[b, b + 1, b + 2, b + 2, b + 3, b]);
        }
        let bytes = unsafe { std::slice::from_raw_parts(idx.as_ptr() as *const u8, idx.len() * 4) };
        self.index_buf = Some(self.device_local_buffer(bytes, vk::BufferUsageFlags::INDEX_BUFFER)?);
        self.index_quads = quads;
        Ok(())
    }

    /// Uploads the static world mesh (vertices in groups of 4 = quads).
    pub fn set_static_mesh(&mut self, verts: &[Vertex]) -> Result<(), String> {
        unsafe { self.device.device_wait_idle().ok() };
        if let Some(b) = self.static_vb.take() {
            self.destroy_buffer(b);
        }
        let quads = verts.len() / 4;
        self.ensure_index_buffer(quads)?;
        let bytes = unsafe {
            std::slice::from_raw_parts(verts.as_ptr() as *const u8, quads * 4 * std::mem::size_of::<Vertex>())
        };
        self.static_vb = Some(self.device_local_buffer(bytes, vk::BufferUsageFlags::VERTEX_BUFFER)?);
        self.static_quads = quads as u32;
        Ok(())
    }

    fn upload_atlas(&mut self, rgba: &[u8], size: u32) -> Result<(), String> {
        unsafe {
            let image = self
                .device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
                        .extent(vk::Extent3D { width: size, height: size, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .map_err(|e| e.to_string())?;
            let req = self.device.get_image_memory_requirements(image);
            let mt = self
                .find_mem(req.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
                .ok_or("no image memory")?;
            let mem = self
                .device
                .allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(mt), None)
                .map_err(|e| e.to_string())?;
            self.device.bind_image_memory(image, mem, 0).map_err(|e| e.to_string())?;

            let staging = self.create_buffer(rgba.len() as u64, vk::BufferUsageFlags::TRANSFER_SRC, true)?;
            std::ptr::copy_nonoverlapping(rgba.as_ptr(), staging.ptr, rgba.len());
            let range = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };
            self.one_shot(|cb| {
                let b1 = [vk::ImageMemoryBarrier::default()
                    .image(image)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .subresource_range(range)];
                self.device.cmd_pipeline_barrier(
                    cb,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &b1,
                );
                let region = [vk::BufferImageCopy::default()
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image_extent(vk::Extent3D { width: size, height: size, depth: 1 })];
                self.device.cmd_copy_buffer_to_image(
                    cb,
                    staging.buf,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &region,
                );
                let b2 = [vk::ImageMemoryBarrier::default()
                    .image(image)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .subresource_range(range)];
                self.device.cmd_pipeline_barrier(
                    cb,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &b2,
                );
            })?;
            self.destroy_buffer(staging);
            let view = self
                .device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
                        .subresource_range(range),
                    None,
                )
                .map_err(|e| e.to_string())?;
            self.atlas = Img { image, mem, view };
            // Nearest filtering = crisp Minecraft-style pixels.
            self.sampler = self
                .device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .max_lod(0.0),
                    None,
                )
                .map_err(|e| e.to_string())?;
            Ok(())
        }
    }

    fn create_swapchain(&mut self, width: u32, height: u32) -> Result<(), String> {
        unsafe {
            let caps = self
                .surface_fn
                .get_physical_device_surface_capabilities(self.pdev, self.surface)
                .map_err(|e| e.to_string())?;
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                    height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
                }
            };
            if extent.width == 0 || extent.height == 0 {
                self.extent = extent;
                return Ok(());
            }
            let mut count = caps.min_image_count + 1;
            if caps.max_image_count > 0 {
                count = count.min(caps.max_image_count);
            }
            let alpha = [
                vk::CompositeAlphaFlagsKHR::OPAQUE,
                vk::CompositeAlphaFlagsKHR::INHERIT,
                vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
            ]
            .into_iter()
            .find(|a| caps.supported_composite_alpha.contains(*a))
            .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);
            let mut usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;
            if self.capture_supported {
                usage |= vk::ImageUsageFlags::TRANSFER_SRC;
            }
            let old = self.swapchain;
            let sci = vk::SwapchainCreateInfoKHR::default()
                .surface(self.surface)
                .min_image_count(count)
                .image_format(self.format)
                .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(usage)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(alpha)
                .present_mode(vk::PresentModeKHR::FIFO)
                .clipped(true)
                .old_swapchain(old);
            self.swapchain = self.swap_fn.create_swapchain(&sci, None).map_err(|e| format!("swapchain: {e}"))?;
            if old != vk::SwapchainKHR::null() {
                self.swap_fn.destroy_swapchain(old, None);
            }
            self.extent = extent;
            self.images = self.swap_fn.get_swapchain_images(self.swapchain).map_err(|e| e.to_string())?;
            let range = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };
            self.views = self
                .images
                .iter()
                .map(|&img| {
                    self.device
                        .create_image_view(
                            &vk::ImageViewCreateInfo::default()
                                .image(img)
                                .view_type(vk::ImageViewType::TYPE_2D)
                                .format(self.format)
                                .subresource_range(range),
                            None,
                        )
                        .unwrap()
                })
                .collect();

            // Depth buffer.
            let dimg = self
                .device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::D32_SFLOAT)
                        .extent(vk::Extent3D { width: extent.width, height: extent.height, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT),
                    None,
                )
                .map_err(|e| e.to_string())?;
            let req = self.device.get_image_memory_requirements(dimg);
            let mt = self
                .find_mem(req.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
                .ok_or("depth mem")?;
            let dmem = self
                .device
                .allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(mt), None)
                .map_err(|e| e.to_string())?;
            self.device.bind_image_memory(dimg, dmem, 0).map_err(|e| e.to_string())?;
            let dview = self
                .device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(dimg)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::D32_SFLOAT)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::DEPTH,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .map_err(|e| e.to_string())?;
            self.depth = Some(Img { image: dimg, mem: dmem, view: dview });

            self.framebuffers = self
                .views
                .iter()
                .map(|&v| {
                    let att = [v, dview];
                    self.device
                        .create_framebuffer(
                            &vk::FramebufferCreateInfo::default()
                                .render_pass(self.render_pass)
                                .attachments(&att)
                                .width(extent.width)
                                .height(extent.height)
                                .layers(1),
                            None,
                        )
                        .unwrap()
                })
                .collect();
            // One "render finished" semaphore per swapchain image.
            for s in self.render_done.drain(..) {
                self.device.destroy_semaphore(s, None);
            }
            for _ in 0..self.images.len() {
                self.render_done
                    .push(self.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).unwrap());
            }
            Ok(())
        }
    }

    fn destroy_swapchain_resources(&mut self) {
        unsafe {
            for fb in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(fb, None);
            }
            for v in self.views.drain(..) {
                self.device.destroy_image_view(v, None);
            }
            if let Some(d) = self.depth.take() {
                self.device.destroy_image_view(d.view, None);
                self.device.destroy_image(d.image, None);
                self.device.free_memory(d.mem, None);
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        unsafe { self.device.device_wait_idle().ok() };
        self.destroy_swapchain_resources();
        if let Err(e) = self.create_swapchain(width, height) {
            eprintln!("swapchain recreate failed: {e}");
        }
        self.needs_resize = false;
    }

    /// Draws one frame. Returns captured pixels if `capture` was requested.
    pub fn draw(
        &mut self,
        world_pc: &PushConstants,
        dyn_verts: &[Vertex],
        ui_pc: &PushConstants,
        ui_verts: &[Vertex],
        clear: [f32; 3],
        capture: bool,
        win_size: (u32, u32),
    ) -> Option<Capture> {
        if self.needs_resize {
            self.resize(win_size.0, win_size.1);
        }
        if self.extent.width == 0 || self.extent.height == 0 || self.framebuffers.is_empty() {
            return None;
        }
        unsafe {
            let f = self.frame;
            self.device.wait_for_fences(&[self.fences[f]], true, u64::MAX).ok();
            let (img_index, _) = match self.swap_fn.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.img_avail[f],
                vk::Fence::null(),
            ) {
                Ok(r) => r,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.needs_resize = true;
                    return None;
                }
                Err(e) => {
                    eprintln!("acquire failed: {e}");
                    return None;
                }
            };
            self.device.reset_fences(&[self.fences[f]]).ok();

            // Upload dynamic geometry.
            let dyn_quads = (dyn_verts.len() / 4).min(MAX_DYN_QUADS);
            std::ptr::copy_nonoverlapping(dyn_verts.as_ptr(), self.dyn_vb[f].ptr as *mut Vertex, dyn_quads * 4);
            let ui_quads = (ui_verts.len() / 4).min(MAX_UI_QUADS);
            std::ptr::copy_nonoverlapping(ui_verts.as_ptr(), self.ui_vb[f].ptr as *mut Vertex, ui_quads * 4);

            let cb = self.cmds[f];
            self.device.reset_command_buffer(cb, vk::CommandBufferResetFlags::empty()).ok();
            self.device.begin_command_buffer(cb, &vk::CommandBufferBeginInfo::default()).ok();
            let clears = [
                vk::ClearValue { color: vk::ClearColorValue { float32: [clear[0], clear[1], clear[2], 1.0] } },
                vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } },
            ];
            let rpbi = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[img_index as usize])
                .render_area(vk::Rect2D { offset: vk::Offset2D::default(), extent: self.extent })
                .clear_values(&clears);
            self.device.cmd_begin_render_pass(cb, &rpbi, vk::SubpassContents::INLINE);
            let vp = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.extent.width as f32,
                height: self.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            let sc = [vk::Rect2D { offset: vk::Offset2D::default(), extent: self.extent }];
            self.device.cmd_set_viewport(cb, 0, &vp);
            self.device.cmd_set_scissor(cb, 0, &sc);
            let ib = self.index_buf.as_ref().unwrap().buf;
            self.device.cmd_bind_index_buffer(cb, ib, 0, vk::IndexType::UINT32);
            self.device.cmd_bind_descriptor_sets(
                cb,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.desc_set],
                &[],
            );

            // World.
            self.device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, self.world_pipe);
            self.push(cb, world_pc);
            if let Some(svb) = &self.static_vb {
                if self.static_quads > 0 {
                    self.device.cmd_bind_vertex_buffers(cb, 0, &[svb.buf], &[0]);
                    self.device.cmd_draw_indexed(cb, self.static_quads * 6, 1, 0, 0, 0);
                }
            }
            if dyn_quads > 0 {
                self.device.cmd_bind_vertex_buffers(cb, 0, &[self.dyn_vb[f].buf], &[0]);
                self.device.cmd_draw_indexed(cb, dyn_quads as u32 * 6, 1, 0, 0, 0);
            }
            // HUD.
            if ui_quads > 0 {
                self.device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, self.ui_pipe);
                self.push(cb, ui_pc);
                self.device.cmd_bind_vertex_buffers(cb, 0, &[self.ui_vb[f].buf], &[0]);
                self.device.cmd_draw_indexed(cb, ui_quads as u32 * 6, 1, 0, 0, 0);
            }
            self.device.cmd_end_render_pass(cb);

            let image = self.images[img_index as usize];
            let mut cap_buf = None;
            if capture && self.capture_supported {
                let size = (self.extent.width * self.extent.height * 4) as u64;
                if let Ok(b) = self.create_buffer(size, vk::BufferUsageFlags::TRANSFER_DST, true) {
                    let range = vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    };
                    let to_src = [vk::ImageMemoryBarrier::default()
                        .image(image)
                        .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .subresource_range(range)];
                    self.device.cmd_pipeline_barrier(
                        cb,
                        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &to_src,
                    );
                    let region = [vk::BufferImageCopy::default()
                        .image_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: 0,
                            base_array_layer: 0,
                            layer_count: 1,
                        })
                        .image_extent(vk::Extent3D { width: self.extent.width, height: self.extent.height, depth: 1 })];
                    self.device.cmd_copy_image_to_buffer(
                        cb,
                        image,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        b.buf,
                        &region,
                    );
                    let back = [vk::ImageMemoryBarrier::default()
                        .image(image)
                        .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                        .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .subresource_range(range)];
                    self.device.cmd_pipeline_barrier(
                        cb,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &back,
                    );
                    cap_buf = Some(b);
                }
            }
            self.device.end_command_buffer(cb).ok();

            let wait = [self.img_avail[f]];
            let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signal = [self.render_done[img_index as usize]];
            let cbs = [cb];
            let submit = [vk::SubmitInfo::default()
                .wait_semaphores(&wait)
                .wait_dst_stage_mask(&stages)
                .command_buffers(&cbs)
                .signal_semaphores(&signal)];
            if let Err(e) = self.device.queue_submit(self.queue, &submit, self.fences[f]) {
                eprintln!("submit failed: {e}");
            }
            let swaps = [self.swapchain];
            let idxs = [img_index];
            let pi = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal)
                .swapchains(&swaps)
                .image_indices(&idxs);
            match self.swap_fn.queue_present(self.queue, &pi) {
                Ok(suboptimal) if suboptimal => self.needs_resize = true,
                Ok(_) => {}
                Err(_) => self.needs_resize = true,
            }
            self.frame = (self.frame + 1) % FRAMES;

            cap_buf.map(|b| {
                self.device.wait_for_fences(&[self.fences[f]], true, u64::MAX).ok();
                let n = (self.extent.width * self.extent.height * 4) as usize;
                let src = std::slice::from_raw_parts(b.ptr, n);
                let mut rgba = src.to_vec();
                if matches!(self.format, vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB) {
                    for px in rgba.chunks_exact_mut(4) {
                        px.swap(0, 2);
                    }
                }
                let cap = Capture { width: self.extent.width, height: self.extent.height, rgba };
                let _ = b.size;
                self.destroy_buffer(b);
                cap
            })
        }
    }

    unsafe fn push(&self, cb: vk::CommandBuffer, pc: &PushConstants) {
        let bytes = std::slice::from_raw_parts(pc as *const PushConstants as *const u8, std::mem::size_of::<PushConstants>());
        self.device.cmd_push_constants(
            cb,
            self.pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            bytes,
        );
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();
            for b in std::mem::take(&mut self.dyn_vb) {
                self.destroy_buffer(b);
            }
            for b in std::mem::take(&mut self.ui_vb) {
                self.destroy_buffer(b);
            }
            if let Some(b) = self.static_vb.take() {
                self.destroy_buffer(b);
            }
            if let Some(b) = self.index_buf.take() {
                self.destroy_buffer(b);
            }
            self.destroy_swapchain_resources();
            for s in self.render_done.drain(..).chain(self.img_avail.drain(..)) {
                self.device.destroy_semaphore(s, None);
            }
            for f in self.fences.drain(..) {
                self.device.destroy_fence(f, None);
            }
            self.device.destroy_command_pool(self.cmd_pool, None);
            self.device.destroy_sampler(self.sampler, None);
            self.device.destroy_image_view(self.atlas.view, None);
            self.device.destroy_image(self.atlas.image, None);
            self.device.free_memory(self.atlas.mem, None);
            self.device.destroy_descriptor_pool(self.desc_pool, None);
            self.device.destroy_pipeline(self.world_pipe, None);
            self.device.destroy_pipeline(self.ui_pipe, None);
            self.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_descriptor_set_layout(self.set_layout, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.swap_fn.destroy_swapchain(self.swapchain, None);
            self.device.destroy_device(None);
            self.surface_fn.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

fn make_pipeline(
    device: &Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    spv: &[u8],
    world: bool,
) -> Result<vk::Pipeline, String> {
    unsafe {
        let code = ash::util::read_spv(&mut Cursor::new(spv)).map_err(|e| e.to_string())?;
        let module = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)
            .map_err(|e| e.to_string())?;
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(module)
                .name(c"vs_main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(module)
                .name(c"fs_main"),
        ];
        let binding = [vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<Vertex>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let attrs = [
            vk::VertexInputAttributeDescription { location: 0, binding: 0, format: vk::Format::R32G32B32_SFLOAT, offset: 0 },
            vk::VertexInputAttributeDescription { location: 1, binding: 0, format: vk::Format::R32G32_SFLOAT, offset: 12 },
            vk::VertexInputAttributeDescription { location: 2, binding: 0, format: vk::Format::R32G32B32A32_SFLOAT, offset: 20 },
        ];
        let vi = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&binding)
            .vertex_attribute_descriptions(&attrs);
        let ia = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let vps = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let rs = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(if world { vk::CullModeFlags::BACK } else { vk::CullModeFlags::NONE })
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let ms = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let ds = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(world)
            .depth_write_enable(world)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
        let blend = [if world {
            vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA)
        } else {
            vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA)
        }];
        let cbs = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend);
        let dyn_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dyn_states);
        let pci = [vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vi)
            .input_assembly_state(&ia)
            .viewport_state(&vps)
            .rasterization_state(&rs)
            .multisample_state(&ms)
            .depth_stencil_state(&ds)
            .color_blend_state(&cbs)
            .dynamic_state(&dynamic)
            .layout(layout)
            .render_pass(render_pass)
            .subpass(0)];
        let p = device
            .create_graphics_pipelines(vk::PipelineCache::null(), &pci, None)
            .map_err(|(_, e)| format!("pipeline: {e}"))?[0];
        device.destroy_shader_module(module, None);
        Ok(p)
    }
}
