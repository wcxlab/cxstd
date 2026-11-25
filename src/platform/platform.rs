//! Initialize the native platform on the main thread with `cxstd::platform::init()?`.
//! Access windows, events and gamepads on that thread. The clipboard is shared
//! across threads and uses the application's Tokio runtime.
//!
//! ```
//! cxstd::platform::init()?;
//! let builder = cxstd::platform::create_window().size(800, 600);
//! # cxstd::platform::init()?;
//! let _ = cxstd::platform::poll_event();
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::LazyLock,
};

use super::backend::sdl::{self, SdlContext};
use super::clipboard::ClipboardService;
use super::event::EventService;
use super::gamepad::GamepadService;
use super::gpu::GpuService;
use super::window::{WindowBuilder, WindowId, WindowState};
use crate::platform::PlatformEventMessage;

thread_local! {
    static PLATFORM: OnceCell<Platform> = const { OnceCell::new() };
}

struct Platform {
    windows: RefCell<HashMap<WindowId, Rc<WindowState>>>,
    context: Rc<SdlContext>,
    event: EventService,
    gamepad: GamepadService,
    gpu: &'static GpuService,
    clipboard: &'static ClipboardService,
}

impl Platform {
    fn new() -> anyhow::Result<Self> {
        let native = sdl::init()?;
        native.context.init_gamepads()?;
        let context = native.context.clone();
        Ok(Self {
            windows: RefCell::default(),
            event: EventService::new(native),
            gamepad: GamepadService {
                context: context.clone(),
            },
            context,
            gpu: super::gpu::initialize(None)?,
            clipboard: shared_clipboard(),
        })
    }
}

/// Initialize native events, gamepads, GPU and clipboard services.
/// Repeated calls on the platform thread reuse the same context.
/// GPU adapter selection runs before a window surface exists.
pub fn init() -> anyhow::Result<()> {
    PLATFORM.with(|slot| {
        if slot.get().is_none() {
            let platform = Platform::new()?;
            slot.get_or_init(|| platform);
        }
        Ok(())
    })
}

fn with_platform<R>(access: impl FnOnce(&Platform) -> R) -> R {
    PLATFORM.with(|slot| {
        access(
            slot.get()
                .expect("call cxstd::platform::init() on the platform thread first"),
        )
    })
}

/// Start configuring a window on the initialized platform thread.
pub fn create_window() -> WindowBuilder {
    with_platform(|_| WindowBuilder::new())
}

pub(super) fn window_context() -> anyhow::Result<Rc<SdlContext>> {
    init()?;
    Ok(with_platform(|platform| platform.context.clone()))
}

pub(super) fn register_window(id: WindowId, window: Rc<WindowState>) {
    with_platform(|platform| platform.windows.borrow_mut().insert(id, window));
}

pub(super) fn unregister_window(id: WindowId) {
    let window = with_platform(|platform| platform.windows.borrow_mut().remove(&id));
    drop(window);
}

/// Access the process-wide GPU handles.
pub fn gpu() -> anyhow::Result<&'static GpuService> {
    PLATFORM
        .with(|slot| slot.get().map(|platform| platform.gpu))
        .map_or_else(|| super::gpu::initialize(None), Ok)
}

/// Access gamepads on the initialized platform thread.
pub fn gamepad() -> GamepadService {
    with_platform(|platform| platform.gamepad.clone())
}

/// Poll the shared native event queue on its owning thread.
/// Returns `None` when no events are available or the thread has no native context.
pub fn poll_event() -> Option<PlatformEventMessage> {
    PLATFORM.with(|slot| match slot.get() {
        Some(platform) => platform.event.poll_event(),
        None => EventService::current()?.poll_event(),
    })
}

/// Access the process-wide clipboard from any thread.
pub fn clipboard() -> &'static ClipboardService {
    PLATFORM
        .with(|slot| slot.get().map(|platform| platform.clipboard))
        .unwrap_or_else(shared_clipboard)
}

fn shared_clipboard() -> &'static ClipboardService {
    static CLIPBOARD: LazyLock<ClipboardService> = LazyLock::new(ClipboardService::new);
    &CLIPBOARD
}
