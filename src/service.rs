//! Service types stored by an application context.

/// Marks a type that can be registered as a service.
///
/// Services may contain thread-local state; thread safety is determined by each type.
/// Storage and lookup are responsibilities of the context that owns the service.
///
/// ```
/// use cxstd::Service;
///
/// struct Theme {
///     font_size: f32,
/// }
///
/// impl Service for Theme {}
/// ```
pub trait Service: 'static {}
