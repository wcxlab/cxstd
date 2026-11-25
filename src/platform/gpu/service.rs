//! Shared WGPU resources for window surfaces and offscreen rendering.

use std::sync::{Mutex, OnceLock};

use anyhow::Context as _;

/// The GPU handles shared by windows in the process.
pub struct GpuService {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuService {
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

// Keep device and queue ownership out of TLS because WGPU uses TLS during teardown.
static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();
static GPU: OnceLock<GpuService> = OnceLock::new();
static GPU_INIT: Mutex<()> = Mutex::new(());

pub(in crate::platform) fn instance() -> &'static wgpu::Instance {
    INSTANCE.get_or_init(|| {
        wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: if cfg!(target_os = "windows") {
                wgpu::Backends::DX12
            } else {
                wgpu::Backends::METAL | wgpu::Backends::VULKAN
            },
            flags: wgpu::InstanceFlags::empty().with_env(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        })
    })
}

pub(in crate::platform) fn initialize(
    compatible_surface: Option<&wgpu::Surface<'_>>,
) -> anyhow::Result<&'static GpuService> {
    if let Some(service) = GPU.get() {
        return Ok(service);
    }
    let _init = GPU_INIT
        .lock()
        .map_err(|_| anyhow::anyhow!("platform GPU initialization lock is poisoned"))?;
    if let Some(service) = GPU.get() {
        return Ok(service);
    }

    let instance = instance().clone();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .context("failed to request wgpu adapter")?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("cxstd platform device"),
        ..Default::default()
    }))
    .context("failed to request wgpu device")?;
    Ok(GPU.get_or_init(|| GpuService {
        instance,
        adapter,
        device,
        queue,
    }))
}
