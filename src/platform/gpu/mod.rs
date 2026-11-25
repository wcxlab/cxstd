mod service;

pub use service::GpuService;
pub(super) use service::{initialize, instance};
