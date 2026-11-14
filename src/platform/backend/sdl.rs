//! Shared SDL context, native events, windows and GPU presentation.

use std::cell::RefMut;

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
    mem::MaybeUninit,
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::Mutex,
    thread::{self, ThreadId},
    time::Duration,
};

use sdl3_sys as sys;
use sys::video::SDL_WindowFlags;

use crate::platform::events::*;
use crate::window::*;
use crate::{KeyCode, KeyLocation};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct WindowPixelFormat(pub i32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct WindowPropertiesId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowDisplayMode {
    pub display_id: u32,
    pub format: WindowPixelFormat,
    pub width: i32,
    pub height: i32,
    pub pixel_density: f32,
    pub refresh_rate: f32,
    pub refresh_rate_numerator: i32,
    pub refresh_rate_denominator: i32,
    pub(crate) internal: *mut c_void,
}

pub(crate) struct WindowSurfaceRef<'a> {
    pub(crate) raw: NonNull<c_void>,
    pub(crate) marker: PhantomData<&'a mut ()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WindowFlashOperation {
    Cancel,
    Briefly,
    UntilFocused,
    Unknown(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WindowHitTestResult {
    Normal,
    Draggable,
    ResizeTopLeft,
    ResizeTop,
    ResizeTopRight,
    ResizeRight,
    ResizeBottomRight,
    ResizeBottom,
    ResizeBottomLeft,
    ResizeLeft,
    Unknown(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WindowProgressState {
    Invalid,
    None,
    Indeterminate,
    Normal,
    Paused,
    Error,
    Unknown(i32),
}

// Native conversions stay private instead of adding backend-specific From impls
// or constructors to public window/event types.
trait FromNative<T> {
    fn from_native(value: T) -> Self;
}
trait IntoNative<T>: Sized {
    fn into_native(self) -> T;
}
impl<S, T: FromNative<S>> IntoNative<T> for S {
    fn into_native(self) -> T {
        T::from_native(self)
    }
}

static SDL_CONTEXTS: Mutex<Option<(ThreadId, usize)>> = Mutex::new(None);

thread_local! {
    static PLATFORM_CONTEXT: RefCell<Weak<SdlContext>> = const { RefCell::new(Weak::new()) };
    static STANDALONE_GAMEPAD_CONTEXT: RefCell<Option<Rc<SdlContext>>> = const { RefCell::new(None) };
}

pub(crate) fn poll_event() -> Option<PlatformEventMessage> {
    let context = PLATFORM_CONTEXT.with(|slot| slot.borrow().upgrade())?;
    if context.event_subsystem_taken.get() {
        return None;
    }
    SdlInstance { context }.poll_event()
}

pub(crate) fn get_gamepads() -> anyhow::Result<Vec<Option<crate::gamepad::Gamepad>>> {
    let context = init()?.context;
    let gamepads = gamepads_for_context(&context)?;
    // Keep the standalone enumeration entry point monitoring hotplug events.
    STANDALONE_GAMEPAD_CONTEXT.with(|slot| {
        slot.borrow_mut().get_or_insert_with(|| context.clone());
    });
    Ok(gamepads)
}

pub(crate) fn gamepads_for_context(
    context: &Rc<SdlContext>,
) -> anyhow::Result<Vec<Option<crate::gamepad::Gamepad>>> {
    let states = {
        let mut gamepads = context.gamepads.borrow_mut();
        if gamepads.is_none() {
            *gamepads = Some(super::gamepad::Gamepads::new()?);
        }
        gamepads.as_ref().unwrap().states()
    };
    Ok(states
        .into_iter()
        .map(|state| state.map(|state| crate::gamepad::Gamepad::new(state, context.clone())))
        .collect())
}

pub(crate) fn gamepad_for_index(
    context: &Rc<SdlContext>,
    index: u32,
) -> Option<crate::gamepad::Gamepad> {
    let state = context.gamepads.borrow().as_ref()?.state(index)?;
    Some(crate::gamepad::Gamepad::new(state, context.clone()))
}

const REDRAW_REQUEST: i32 = 0;
const CLOSE_REQUEST: i32 = 1;

pub(crate) struct SdlContext {
    video_initialized: Cell<bool>,
    pub(crate) event_subsystem_taken: Cell<bool>,
    gamepads: RefCell<Option<super::gamepad::Gamepads>>,
    request_event: u32,
    resize_sizes: RefCell<HashMap<WindowId, (f32, f32)>>,
    composing: RefCell<HashSet<WindowId>>,
    mouse_buttons: Cell<u32>,
    mouse_positions: RefCell<HashMap<WindowId, (f32, f32)>>,
    modifiers: Cell<sys::keycode::SDL_Keymod>,
    pending_events: RefCell<VecDeque<PlatformEventMessage>>,
    cursors: RefCell<HashMap<WindowCursor, NonNull<sys::mouse::SDL_Cursor>>>,
}

impl SdlContext {
    fn end_composition(
        &self,
        window_id: WindowId,
        time_stamp: Duration,
        text: String,
    ) -> Option<PlatformEventMessage> {
        self.composing
            .borrow_mut()
            .remove(&window_id)
            .then(|| PlatformEventMessage {
                time_stamp,
                window_id,
                event: PlatformEvent::CompositionEnd(CompositionEndEvent { data: text }),
            })
    }
}

struct SdlWindowOwner {
    raw: NonNull<sys::video::SDL_Window>,
    context: Rc<SdlContext>,
}

pub(crate) struct SdlInstance {
    pub(crate) context: Rc<SdlContext>,
}

pub(crate) struct SdlWindow<const GPU: bool = false> {
    id: WindowId,
    inner_width: f32,
    inner_height: f32,
    inner_physical_width: u32,
    inner_physical_height: u32,
    screen_x: f32,
    screen_y: f32,
    screen_physical_x: i32,
    screen_physical_y: i32,
    device_pixel_ratio: f32,
    is_minimized: bool,
    is_maximized: bool,
    is_borderless: bool,
    is_fullscreen: bool,
    is_focused: bool,
    frame: RefCell<Option<FrameResources>>,

    gpu: Option<WgpuContext>,
    hit_test: Box<Cell<ResizeHitTest>>,
    hit_test_installed: bool,
    owner: Rc<SdlWindowOwner>,
}

pub(crate) type SdlWgpuWindow = SdlWindow<true>;

impl<const GPU: bool> SdlWindow<GPU> {
    fn flags(&self) -> SDL_WindowFlags {
        unsafe { sys::video::SDL_GetWindowFlags(self.owner.raw.as_ptr()) }
    }

    pub(crate) fn window_handle_provider(&self) -> Rc<dyn raw_window_handle::HasWindowHandle> {
        self.owner.clone()
    }
}

#[derive(Clone, Copy, Default)]
struct ResizeHitTest {
    width: f32,
    height: f32,
    coordinate_scale: f32,
    enabled: bool,
}

impl ResizeHitTest {
    fn test(&self, x: f32, y: f32) -> WindowHitTestResult {
        if !self.enabled || x < 0.0 || y < 0.0 || x >= self.width || y >= self.height {
            return WindowHitTestResult::Normal;
        }
        // Keep the resize margin in logical pixels, including on high-DPI displays.
        let horizontal_margin = 6.0_f32.min(self.width / 2.0);
        let vertical_margin = 6.0_f32.min(self.height / 2.0);
        let left = x < horizontal_margin;
        let right = x >= self.width - horizontal_margin;
        let top = y < vertical_margin;
        let bottom = y >= self.height - vertical_margin;
        match (left, right, top, bottom) {
            (true, _, true, _) => WindowHitTestResult::ResizeTopLeft,
            (_, true, true, _) => WindowHitTestResult::ResizeTopRight,
            (true, _, _, true) => WindowHitTestResult::ResizeBottomLeft,
            (_, true, _, true) => WindowHitTestResult::ResizeBottomRight,
            (true, _, _, _) => WindowHitTestResult::ResizeLeft,
            (_, true, _, _) => WindowHitTestResult::ResizeRight,
            (_, _, true, _) => WindowHitTestResult::ResizeTop,
            (_, _, _, true) => WindowHitTestResult::ResizeBottom,
            _ => WindowHitTestResult::Normal,
        }
    }
}

pub(crate) fn init() -> Result<SdlInstance, WindowError> {
    if let Some(context) = PLATFORM_CONTEXT.with(|slot| slot.borrow().upgrade()) {
        return Ok(SdlInstance { context });
    }
    let mut contexts = SDL_CONTEXTS.lock().map_err(|_| WindowError::Unknown)?;
    let current_thread = thread::current().id();
    if let Some((owner, _)) = contexts.as_ref() {
        if *owner != current_thread {
            return Err(WindowError::Nul(
                "platform context must be initialized on the owning thread".into(),
            ));
        }
    } else {
        #[cfg(target_vendor = "apple")]
        if !unsafe { sys::init::SDL_IsMainThread() } {
            return Err(WindowError::Nul(
                "platform context must be initialized on the main thread".into(),
            ));
        }
        unsafe { sys::main::SDL_SetMainReady() };
    }

    if !unsafe { sys::init::SDL_Init(sys::init::SDL_INIT_EVENTS) } {
        let error = SdlInstance::error();
        if contexts.is_none() {
            unsafe { sys::init::SDL_Quit() };
        }
        return Err(error);
    }

    let request_event = unsafe { sys::events::SDL_RegisterEvents(1) };
    if request_event == 0 {
        let error = SdlInstance::error();
        if contexts.is_none() {
            unsafe { sys::init::SDL_Quit() };
        }
        return Err(error);
    }
    let (_, count) = contexts.get_or_insert((current_thread, 0));
    *count += 1;
    let context = Rc::new(SdlContext {
        video_initialized: Cell::new(false),
        event_subsystem_taken: Cell::new(false),
        gamepads: RefCell::new(None),
        request_event,
        resize_sizes: RefCell::new(HashMap::new()),
        composing: RefCell::new(HashSet::new()),
        mouse_buttons: Cell::new(0),
        mouse_positions: RefCell::new(HashMap::new()),
        modifiers: Cell::new(unsafe { sys::keyboard::SDL_GetModState() }),
        pending_events: RefCell::new(VecDeque::new()),
        cursors: RefCell::new(HashMap::new()),
    });
    PLATFORM_CONTEXT.with(|slot| *slot.borrow_mut() = Rc::downgrade(&context));
    Ok(SdlInstance { context })
}

impl SdlInstance {
    pub(crate) fn is_pressed(&self, code: KeyCode) -> bool {
        let mouse_mask = match code {
            KeyCode::MouseLeft => Some(1),
            KeyCode::MouseMiddle => Some(2),
            KeyCode::MouseRight => Some(4),
            KeyCode::MouseBack => Some(8),
            KeyCode::MouseForward => Some(16),
            _ => None,
        };
        if let Some(mask) = mouse_mask {
            return self.context.mouse_buttons.get() & mask != 0;
        }
        if code == KeyCode::Unidentified || !self.context.video_initialized.get() {
            return false;
        }
        let mut count = 0;
        let keys = unsafe { sys::keyboard::SDL_GetKeyboardState(&mut count) };
        if keys.is_null() || count <= 0 {
            return false;
        }
        unsafe { std::slice::from_raw_parts(keys, count as usize) }
            .iter()
            .enumerate()
            .any(|(scancode, pressed)| *pressed && conversions::web_code(scancode as i32) == code)
    }

    pub(crate) fn poll_event(&mut self) -> Option<PlatformEventMessage> {
        if let Some(event) = self.context.pending_events.borrow_mut().pop_front() {
            return Some(event);
        }
        let mut event = MaybeUninit::<sys::events::SDL_Event>::zeroed();
        while unsafe { sys::events::SDL_PollEvent(event.as_mut_ptr()) } {
            let native = unsafe { event.assume_init_ref() };
            let gamepad_change = self
                .context
                .gamepads
                .borrow_mut()
                .as_mut()
                .and_then(|gamepads| gamepads.handle_event(native));
            if let Some((connected, gamepad)) = gamepad_change {
                return Some(PlatformEventMessage {
                    time_stamp: Duration::from_nanos(unsafe { native.common.timestamp }),
                    window_id: WindowId(0),
                    event: if connected {
                        PlatformEvent::GamepadConnected(GamepadEvent { gamepad })
                    } else {
                        PlatformEvent::GamepadDisconnected(GamepadEvent { gamepad })
                    },
                });
            }
            if let Some(event) = unsafe { PlatformEventMessage::from_sdl(native) } {
                if let PlatformEvent::Resize(resize) = &event.event {
                    let size = (resize.inner_width, resize.inner_height);
                    let previous = self
                        .context
                        .resize_sizes
                        .borrow_mut()
                        .insert(event.window_id, size);
                    if previous == Some(size) {
                        continue;
                    }
                }
                match &event.event {
                    PlatformEvent::CompositionUpdate(edit) if edit.data.is_empty() => {
                        if let Some(end) = self.context.end_composition(
                            event.window_id,
                            event.time_stamp,
                            String::new(),
                        ) {
                            return Some(end);
                        }
                        continue;
                    }
                    PlatformEvent::CompositionUpdate(_) => {
                        if self.context.composing.borrow_mut().insert(event.window_id) {
                            let start = PlatformEventMessage {
                                time_stamp: event.time_stamp,
                                window_id: event.window_id,
                                event: PlatformEvent::CompositionStart(
                                    CompositionStartEvent::default(),
                                ),
                            };
                            self.context.pending_events.borrow_mut().push_back(event);
                            return Some(start);
                        }
                    }
                    PlatformEvent::TextInput(input) => {
                        if let Some(end) = self.context.end_composition(
                            event.window_id,
                            event.time_stamp,
                            input.data.clone(),
                        ) {
                            self.context.pending_events.borrow_mut().push_back(end);
                        }
                    }
                    PlatformEvent::Blur => {
                        if let Some(end) = self.context.end_composition(
                            event.window_id,
                            event.time_stamp,
                            String::new(),
                        ) {
                            self.context.pending_events.borrow_mut().push_back(event);
                            return Some(end);
                        }
                    }
                    _ => {}
                }
                return Some(event);
            }
        }
        None
    }
}

impl SdlInstance {
    fn create_native_window<const GPU: bool>(
        &self,
        title: &str,
        (x, y): (i32, i32),
        (width, height): (i32, i32),
        flags: SDL_WindowFlags,
    ) -> Result<SdlWindow<GPU>, WindowError> {
        if !self.context.video_initialized.get() {
            if !unsafe { sys::init::SDL_InitSubSystem(sys::init::SDL_INIT_VIDEO) } {
                return Err(Self::error());
            }
            self.context.video_initialized.set(true);
            self.context
                .modifiers
                .set(unsafe { sys::keyboard::SDL_GetModState() });
        }
        if flags & (SDL_WindowFlags::MODAL | SDL_WindowFlags::TOOLTIP | SDL_WindowFlags::POPUP_MENU)
            != 0
        {
            return Err(WindowError::Nul("child windows are not supported".into()));
        }
        let title = CString::new(title).map_err(|error| WindowError::Nul(error.to_string()))?;
        let raw = unsafe {
            sys::video::SDL_CreateWindow(
                title.as_ptr(),
                width,
                height,
                flags | SDL_WindowFlags::HIGH_PIXEL_DENSITY,
            )
        };
        let raw = NonNull::new(raw).ok_or_else(Self::error)?;
        let mut window = SdlWindow {
            id: unsafe { sys::video::SDL_GetWindowID(raw.as_ptr()) }.into_native(),
            inner_width: 0.0,
            inner_height: 0.0,
            inner_physical_width: 0,
            inner_physical_height: 0,
            screen_x: 0.0,
            screen_y: 0.0,
            screen_physical_x: 0,
            screen_physical_y: 0,
            device_pixel_ratio: 1.0,
            is_minimized: false,
            is_maximized: false,
            is_borderless: false,
            is_fullscreen: false,
            is_focused: false,
            frame: RefCell::new(None),

            gpu: None,
            hit_test: Box::new(Cell::new(ResizeHitTest::default())),
            hit_test_installed: false,
            owner: Rc::new(SdlWindowOwner {
                raw,
                context: Rc::clone(&self.context),
            }),
        };
        window.move_to(x, y)?;
        window.resize_to(width, height)?;
        window.sync()?;
        self.context
            .resize_sizes
            .borrow_mut()
            .insert(window.id, (window.inner_width, window.inner_height));
        Ok(window)
    }
}

impl SdlInstance {
    fn error() -> WindowError {
        unsafe { copy_string(sys::error::SDL_GetError()) }
            .map(WindowError::Nul)
            .unwrap_or(WindowError::Unknown)
    }
}

impl raw_window_handle::HasWindowHandle for SdlWindowOwner {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = self
            .raw_window_handle()
            .map_err(|_| raw_window_handle::HandleError::Unavailable)?;
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl<const GPU: bool> Window for SdlWindow<GPU> {
    fn id(&self) -> WindowId {
        self.id
    }
    fn inner_width(&self) -> f32 {
        self.inner_width
    }

    fn inner_height(&self) -> f32 {
        self.inner_height
    }

    fn inner_physical_width(&self) -> u32 {
        self.inner_physical_width
    }

    fn inner_physical_height(&self) -> u32 {
        self.inner_physical_height
    }

    fn screen_x(&self) -> f32 {
        self.screen_x
    }

    fn screen_y(&self) -> f32 {
        self.screen_y
    }

    fn screen_physical_x(&self) -> i32 {
        self.screen_physical_x
    }

    fn screen_physical_y(&self) -> i32 {
        self.screen_physical_y
    }

    fn device_pixel_ratio(&self) -> f32 {
        self.device_pixel_ratio
    }

    fn resize_to(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        let factor = self.get_display_scale()? / self.get_pixel_density()?;
        let width = (width as f32 * factor).round() as i32;
        let height = (height as f32 * factor).round() as i32;
        Self::result(unsafe {
            sys::video::SDL_SetWindowSize(self.owner.raw.as_ptr(), width, height)
        })?;
        self.refresh_metrics()
    }

    fn resize_by(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        let (current_width, current_height) = self.get_size()?;
        let factor = self.get_display_scale()? / self.get_pixel_density()?;
        let width = current_width
            .checked_add((width as f32 * factor).round() as i32)
            .ok_or_else(|| WindowError::Nul("window width overflow".into()))?;
        let height = current_height
            .checked_add((height as f32 * factor).round() as i32)
            .ok_or_else(|| WindowError::Nul("window height overflow".into()))?;
        Self::result(unsafe {
            sys::video::SDL_SetWindowSize(self.owner.raw.as_ptr(), width, height)
        })?;
        self.refresh_metrics()
    }

    fn move_to(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowPosition(self.owner.raw.as_ptr(), x, y) })?;
        self.refresh_metrics()
    }

    fn move_by(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        let (current_x, current_y) = self.get_position()?;
        let x = current_x
            .checked_add(x)
            .ok_or_else(|| WindowError::Nul("window x position overflow".into()))?;
        let y = current_y
            .checked_add(y)
            .ok_or_else(|| WindowError::Nul("window y position overflow".into()))?;
        self.move_to(x, y)
    }

    fn maximize(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_MaximizeWindow(self.owner.raw.as_ptr()) })?;
        self.refresh_metrics()
    }

    fn is_maximized(&self) -> bool {
        self.is_maximized
    }

    fn minimize(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_MinimizeWindow(self.owner.raw.as_ptr()) })?;
        self.refresh_metrics()
    }

    fn is_minimized(&self) -> bool {
        self.is_minimized
    }

    fn restore(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_RestoreWindow(self.owner.raw.as_ptr()) })?;
        self.refresh_metrics()
    }

    fn focus(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_RaiseWindow(self.owner.raw.as_ptr()) })?;
        self.refresh_metrics()
    }

    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn request_redraw(&mut self) -> Result<(), WindowError> {
        self.push_request(REDRAW_REQUEST)
    }

    fn request_close(&mut self) -> Result<(), WindowError> {
        self.push_request(CLOSE_REQUEST)
    }

    fn set_cursor(&mut self, cursor: WindowCursor) -> Result<(), WindowError> {
        if cursor == WindowCursor::None {
            return Self::result(unsafe { sys::mouse::SDL_HideCursor() });
        }
        let mut cursors = self.owner.context.cursors.borrow_mut();
        let raw = match cursors.get(&cursor) {
            Some(raw) => *raw,
            None => {
                let raw = unsafe { sys::mouse::SDL_CreateSystemCursor(cursor.into_native()) };
                let raw = NonNull::new(raw).ok_or_else(SdlInstance::error)?;
                cursors.insert(cursor, raw);
                raw
            }
        };
        if unsafe { sys::mouse::SDL_GetCursor() } != raw.as_ptr() {
            Self::result(unsafe { sys::mouse::SDL_SetCursor(raw.as_ptr()) })?;
        }
        Self::result(unsafe { sys::mouse::SDL_ShowCursor() })
    }

    fn start_text_input(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::keyboard::SDL_StartTextInput(self.owner.raw.as_ptr()) })
    }

    fn stop_text_input(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::keyboard::SDL_StopTextInput(self.owner.raw.as_ptr()) })?;
        if let Some(end) = self.owner.context.end_composition(
            self.id,
            Duration::from_nanos(unsafe { sys::timer::SDL_GetTicksNS() }),
            String::new(),
        ) {
            self.owner
                .context
                .pending_events
                .borrow_mut()
                .push_back(end);
        }
        Ok(())
    }

    fn fullscreen(&mut self, fullscreen: bool) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowFullscreen(self.owner.raw.as_ptr(), fullscreen)
        })?;
        self.refresh_metrics()
    }

    fn is_fullscreen(&self) -> bool {
        self.is_fullscreen
    }

    fn borderless(&mut self, borderless: bool) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowBordered(self.owner.raw.as_ptr(), !borderless)
        })?;
        self.refresh_metrics()
    }

    fn is_borderless(&self) -> bool {
        self.is_borderless
    }

    fn raw_window_handle(&self) -> Result<WindowHandle<'_>, WindowError> {
        let raw = self.owner.raw_window_handle()?;
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

// Back the retained private window operations.
#[allow(dead_code)]
impl<const GPU: bool> SdlWindow<GPU> {
    pub(crate) fn show_system_menu(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        let factor = self.get_display_scale()? / self.get_pixel_density()?;
        let x = (x as f32 * factor).round() as i32;
        let y = (y as f32 * factor).round() as i32;
        Self::result(unsafe { sys::video::SDL_ShowWindowSystemMenu(self.owner.raw.as_ptr(), x, y) })
    }

    pub(crate) fn get_display(&self) -> Result<u32, WindowError> {
        let value = unsafe { sys::video::SDL_GetDisplayForWindow(self.owner.raw.as_ptr()) };
        if value.0 == 0 {
            Err(SdlInstance::error())
        } else {
            Ok(value.0)
        }
    }

    pub(crate) fn set_fullscreen_mode(
        &mut self,
        mode: Option<&WindowDisplayMode>,
    ) -> Result<(), WindowError> {
        let Some(mode) = mode else {
            return Self::result(unsafe {
                sys::video::SDL_SetWindowFullscreenMode(self.owner.raw.as_ptr(), std::ptr::null())
            });
        };
        let mut count = 0;
        let modes = unsafe {
            sys::video::SDL_GetFullscreenDisplayModes(
                sys::video::SDL_DisplayID(mode.display_id),
                &mut count,
            )
        };
        if modes.is_null() {
            return Err(SdlInstance::error());
        }
        let mut matched = std::ptr::null();
        for index in 0..count {
            let candidate = unsafe { *modes.add(index as usize) };
            if !candidate.is_null()
                && WindowDisplayMode::from_native(unsafe { &*candidate }) == *mode
            {
                matched = candidate;
                break;
            }
        }
        unsafe { sys::stdinc::SDL_free(modes.cast()) };
        if matched.is_null() {
            return Err(WindowError::Nul(
                "fullscreen mode is not available on the display".into(),
            ));
        }
        Self::result(unsafe {
            sys::video::SDL_SetWindowFullscreenMode(self.owner.raw.as_ptr(), matched)
        })
    }

    pub(crate) fn get_fullscreen_mode(&self) -> Option<WindowDisplayMode> {
        let mode = unsafe { sys::video::SDL_GetWindowFullscreenMode(self.owner.raw.as_ptr()) };
        unsafe { mode.as_ref() }.map(WindowDisplayMode::from_native)
    }

    pub(crate) fn get_icc_profile(&self) -> Result<Vec<u8>, WindowError> {
        let mut size = 0;
        let profile =
            unsafe { sys::video::SDL_GetWindowICCProfile(self.owner.raw.as_ptr(), &mut size) };
        if profile.is_null() {
            return Err(SdlInstance::error());
        }
        let bytes = unsafe { std::slice::from_raw_parts(profile.cast::<u8>(), size) }.to_vec();
        unsafe { sys::stdinc::SDL_free(profile) };
        Ok(bytes)
    }

    pub(crate) fn get_pixel_format(&self) -> Result<WindowPixelFormat, WindowError> {
        let format = unsafe { sys::video::SDL_GetWindowPixelFormat(self.owner.raw.as_ptr()) };
        if format == sys::pixels::SDL_PixelFormat::UNKNOWN {
            Err(SdlInstance::error())
        } else {
            Ok(format.into_native())
        }
    }

    pub(crate) fn get_properties(&self) -> Result<WindowPropertiesId, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowProperties(self.owner.raw.as_ptr()) };
        if value.0 == 0 {
            Err(SdlInstance::error())
        } else {
            Ok(value.into_native())
        }
    }

    pub(crate) fn is_hidden(&self) -> bool {
        self.flags() & SDL_WindowFlags::HIDDEN != 0
    }

    pub(crate) fn is_resizable(&self) -> bool {
        self.flags() & SDL_WindowFlags::RESIZABLE != 0
    }

    pub(crate) fn set_title(&mut self, title: &str) -> Result<(), WindowError> {
        let title = CString::new(title).map_err(|error| WindowError::Nul(error.to_string()))?;
        Self::result(unsafe {
            sys::video::SDL_SetWindowTitle(self.owner.raw.as_ptr(), title.as_ptr())
        })
    }

    pub(crate) fn get_title(&self) -> String {
        unsafe { copy_string(sys::video::SDL_GetWindowTitle(self.owner.raw.as_ptr())) }
            .unwrap_or_default()
    }

    pub(crate) fn set_icon(&mut self, icon: &WindowSurfaceRef<'_>) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowIcon(self.owner.raw.as_ptr(), icon.into_native())
        })
    }

    pub(crate) fn get_position(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowPosition(self.owner.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    pub(crate) fn get_size(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowSize(self.owner.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    pub(crate) fn get_safe_area(&self) -> Result<WindowRect, WindowError> {
        let mut rect = sys::rect::SDL_Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        Self::result(unsafe {
            sys::video::SDL_GetWindowSafeArea(self.owner.raw.as_ptr(), &mut rect)
        })?;
        Ok(rect.into_native())
    }

    pub(crate) fn set_aspect_ratio(
        &mut self,
        min_aspect: f32,
        max_aspect: f32,
    ) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowAspectRatio(self.owner.raw.as_ptr(), min_aspect, max_aspect)
        })
    }

    pub(crate) fn get_aspect_ratio(&self) -> Result<(f32, f32), WindowError> {
        let (mut first, mut second) = (0.0, 0.0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowAspectRatio(self.owner.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    pub(crate) fn get_borders_size(&self) -> Result<(i32, i32, i32, i32), WindowError> {
        let (mut top, mut left, mut bottom, mut right) = (0, 0, 0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowBordersSize(
                self.owner.raw.as_ptr(),
                &mut top,
                &mut left,
                &mut bottom,
                &mut right,
            )
        })?;
        Ok((top, left, bottom, right))
    }

    pub(crate) fn get_size_in_pixels(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowSizeInPixels(self.owner.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    pub(crate) fn set_minimum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowMinimumSize(self.owner.raw.as_ptr(), width, height)
        })
    }

    pub(crate) fn get_minimum_size(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowMinimumSize(self.owner.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    pub(crate) fn set_maximum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowMaximumSize(self.owner.raw.as_ptr(), width, height)
        })
    }

    pub(crate) fn get_maximum_size(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowMaximumSize(self.owner.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    pub(crate) fn set_resizable(&mut self, resizable: bool) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowResizable(self.owner.raw.as_ptr(), resizable)
        })?;
        self.refresh_metrics()
    }

    pub(crate) fn set_always_on_top(&mut self, on_top: bool) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowAlwaysOnTop(self.owner.raw.as_ptr(), on_top)
        })
    }

    pub(crate) fn set_fill_document(&mut self, fill: bool) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowFillDocument(self.owner.raw.as_ptr(), fill)
        })
    }

    pub(crate) fn show(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_ShowWindow(self.owner.raw.as_ptr()) })
    }

    pub(crate) fn hide(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_HideWindow(self.owner.raw.as_ptr()) })
    }

    pub(crate) fn sync(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SyncWindow(self.owner.raw.as_ptr()) })?;
        self.refresh_metrics()
    }

    pub(crate) fn has_surface(&self) -> bool {
        !GPU && unsafe { sys::video::SDL_WindowHasSurface(self.owner.raw.as_ptr()) }
    }

    pub(crate) fn get_surface(&mut self) -> Result<WindowSurfaceRef<'_>, WindowError> {
        if GPU {
            return Err(WindowError::Nul(
                "SDL software surfaces are not available on a wgpu window".into(),
            ));
        }
        let surface = unsafe { sys::video::SDL_GetWindowSurface(self.owner.raw.as_ptr()) };
        Ok(WindowSurfaceRef {
            raw: NonNull::new(surface).ok_or_else(SdlInstance::error)?.cast(),
            marker: PhantomData,
        })
    }

    pub(crate) fn set_surface_vsync(&mut self, vsync: i32) -> Result<(), WindowError> {
        if GPU {
            return Err(WindowError::Nul(
                "SDL software surfaces are not available on a wgpu window".into(),
            ));
        }
        Self::result(unsafe {
            sys::video::SDL_SetWindowSurfaceVSync(self.owner.raw.as_ptr(), vsync)
        })
    }

    pub(crate) fn get_surface_vsync(&self) -> Result<i32, WindowError> {
        if GPU {
            return Err(WindowError::Nul(
                "SDL software surfaces are not available on a wgpu window".into(),
            ));
        }
        let mut vsync = 0;
        Self::result(unsafe {
            sys::video::SDL_GetWindowSurfaceVSync(self.owner.raw.as_ptr(), &mut vsync)
        })?;
        Ok(vsync)
    }

    pub(crate) fn update_surface(&mut self) -> Result<(), WindowError> {
        if GPU {
            return Err(WindowError::Nul(
                "SDL software surfaces are not available on a wgpu window".into(),
            ));
        }
        Self::result(unsafe { sys::video::SDL_UpdateWindowSurface(self.owner.raw.as_ptr()) })
    }

    pub(crate) fn update_surface_rects(&mut self, rects: &[WindowRect]) -> Result<(), WindowError> {
        if GPU {
            return Err(WindowError::Nul(
                "SDL software surfaces are not available on a wgpu window".into(),
            ));
        }
        let count =
            i32::try_from(rects.len()).map_err(|error| WindowError::Nul(error.to_string()))?;
        let rects: Vec<sys::rect::SDL_Rect> =
            rects.iter().copied().map(IntoNative::into_native).collect();
        Self::result(unsafe {
            sys::video::SDL_UpdateWindowSurfaceRects(self.owner.raw.as_ptr(), rects.as_ptr(), count)
        })
    }

    pub(crate) fn destroy_surface(&mut self) -> Result<(), WindowError> {
        if GPU {
            return Err(WindowError::Nul(
                "SDL software surfaces are not available on a wgpu window".into(),
            ));
        }
        Self::result(unsafe { sys::video::SDL_DestroyWindowSurface(self.owner.raw.as_ptr()) })
    }

    pub(crate) fn set_keyboard_grab(&mut self, grabbed: bool) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowKeyboardGrab(self.owner.raw.as_ptr(), grabbed)
        })
    }

    pub(crate) fn set_mouse_grab(&mut self, grabbed: bool) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowMouseGrab(self.owner.raw.as_ptr(), grabbed)
        })
    }

    pub(crate) fn get_keyboard_grab(&self) -> bool {
        unsafe { sys::video::SDL_GetWindowKeyboardGrab(self.owner.raw.as_ptr()) }
    }

    pub(crate) fn get_mouse_grab(&self) -> bool {
        unsafe { sys::video::SDL_GetWindowMouseGrab(self.owner.raw.as_ptr()) }
    }

    pub(crate) fn set_mouse_rect(&mut self, rect: Option<&WindowRect>) -> Result<(), WindowError> {
        let rect = rect.copied().map(sys::rect::SDL_Rect::from_native);
        Self::result(unsafe {
            sys::video::SDL_SetWindowMouseRect(
                self.owner.raw.as_ptr(),
                rect.as_ref().map_or(std::ptr::null(), |rect| rect),
            )
        })
    }

    pub(crate) fn get_mouse_rect(&self) -> Option<WindowRect> {
        let rect = unsafe { sys::video::SDL_GetWindowMouseRect(self.owner.raw.as_ptr()) };
        unsafe { rect.as_ref() }
            .copied()
            .map(IntoNative::into_native)
    }

    pub(crate) fn set_opacity(&mut self, opacity: f32) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowOpacity(self.owner.raw.as_ptr(), opacity) })
    }

    pub(crate) fn get_opacity(&self) -> Result<f32, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowOpacity(self.owner.raw.as_ptr()) };
        if value < 0.0 {
            Err(SdlInstance::error())
        } else {
            Ok(value)
        }
    }

    pub(crate) fn set_focusable(&mut self, focusable: bool) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowFocusable(self.owner.raw.as_ptr(), focusable)
        })
    }

    pub(crate) fn set_shape(
        &mut self,
        shape: Option<&WindowSurfaceRef<'_>>,
    ) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowShape(
                self.owner.raw.as_ptr(),
                shape.map_or(std::ptr::null_mut(), IntoNative::into_native),
            )
        })
    }

    pub(crate) fn flash(&mut self, operation: WindowFlashOperation) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_FlashWindow(self.owner.raw.as_ptr(), operation.into_native())
        })
    }

    pub(crate) fn set_progress_state(
        &mut self,
        state: WindowProgressState,
    ) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowProgressState(self.owner.raw.as_ptr(), state.into_native())
        })
    }

    pub(crate) fn get_progress_state(&self) -> Result<WindowProgressState, WindowError> {
        let state = unsafe { sys::video::SDL_GetWindowProgressState(self.owner.raw.as_ptr()) };
        if state == sys::video::SDL_ProgressState::INVALID {
            Err(SdlInstance::error())
        } else {
            Ok(state.into_native())
        }
    }

    pub(crate) fn set_progress_value(&mut self, value: f32) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowProgressValue(self.owner.raw.as_ptr(), value)
        })
    }

    pub(crate) fn get_progress_value(&self) -> Result<f32, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowProgressValue(self.owner.raw.as_ptr()) };
        if value < 0.0 {
            Err(SdlInstance::error())
        } else {
            Ok(value)
        }
    }

    pub(crate) fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, WindowError> {
        let raw = self.owner.raw_display_handle()?;
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(raw) })
    }
}

impl SdlWindowOwner {
    fn raw_display_handle(&self) -> Result<raw_window_handle::RawDisplayHandle, WindowError> {
        use raw_window_handle as rwh;

        let driver = unsafe { copy_string(sys::video::SDL_GetCurrentVideoDriver()) }
            .ok_or_else(SdlInstance::error)?;
        match driver.as_str() {
            "windows" => Ok(rwh::RawDisplayHandle::Windows(
                rwh::WindowsDisplayHandle::new(),
            )),
            "cocoa" => Ok(rwh::RawDisplayHandle::AppKit(
                rwh::AppKitDisplayHandle::new(),
            )),
            "uikit" => Ok(rwh::RawDisplayHandle::UiKit(rwh::UiKitDisplayHandle::new())),
            "android" => Ok(rwh::RawDisplayHandle::Android(
                rwh::AndroidDisplayHandle::new(),
            )),
            "wayland" => {
                let display =
                    self.native_pointer(sys::video::SDL_PROP_WINDOW_WAYLAND_DISPLAY_POINTER)?;
                Ok(rwh::RawDisplayHandle::Wayland(
                    rwh::WaylandDisplayHandle::new(display),
                ))
            }
            "x11" => {
                let display =
                    self.native_pointer(sys::video::SDL_PROP_WINDOW_X11_DISPLAY_POINTER)?;
                let screen = unsafe {
                    sys::properties::SDL_GetNumberProperty(
                        self.get_properties()?.into_native(),
                        sys::video::SDL_PROP_WINDOW_X11_SCREEN_NUMBER,
                        0,
                    )
                };
                Ok(rwh::RawDisplayHandle::Xlib(rwh::XlibDisplayHandle::new(
                    Some(display),
                    screen as i32,
                )))
            }
            _ => Err(WindowError::Nul(format!(
                "native display handles are not supported by SDL video driver {driver}"
            ))),
        }
    }

    fn raw_window_handle(&self) -> Result<raw_window_handle::RawWindowHandle, WindowError> {
        use raw_window_handle as rwh;

        let driver = unsafe { copy_string(sys::video::SDL_GetCurrentVideoDriver()) }
            .ok_or_else(SdlInstance::error)?;
        match driver.as_str() {
            "windows" => {
                let hwnd = self.native_pointer(sys::video::SDL_PROP_WINDOW_WIN32_HWND_POINTER)?;
                let mut handle = rwh::Win32WindowHandle::new(
                    std::num::NonZeroIsize::new(hwnd.as_ptr() as isize)
                        .ok_or(WindowError::Unknown)?,
                );
                let instance = unsafe {
                    sys::properties::SDL_GetPointerProperty(
                        self.get_properties()?.into_native(),
                        sys::video::SDL_PROP_WINDOW_WIN32_INSTANCE_POINTER,
                        std::ptr::null_mut(),
                    )
                };
                handle.hinstance = std::num::NonZeroIsize::new(instance as isize);
                Ok(rwh::RawWindowHandle::Win32(handle))
            }
            "wayland" => {
                let surface =
                    self.native_pointer(sys::video::SDL_PROP_WINDOW_WAYLAND_SURFACE_POINTER)?;
                Ok(rwh::RawWindowHandle::Wayland(
                    rwh::WaylandWindowHandle::new(surface),
                ))
            }
            "x11" => {
                let window = unsafe {
                    sys::properties::SDL_GetNumberProperty(
                        self.get_properties()?.into_native(),
                        sys::video::SDL_PROP_WINDOW_X11_WINDOW_NUMBER,
                        0,
                    )
                };
                if window == 0 {
                    return Err(WindowError::Nul("X11 window handle is unavailable".into()));
                }
                Ok(rwh::RawWindowHandle::Xlib(rwh::XlibWindowHandle::new(
                    window as _,
                )))
            }
            "android" => {
                let window =
                    self.native_pointer(sys::video::SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER)?;
                Ok(rwh::RawWindowHandle::AndroidNdk(
                    rwh::AndroidNdkWindowHandle::new(window),
                ))
            }
            #[cfg(target_os = "macos")]
            "cocoa" => {
                let window =
                    self.native_pointer(sys::video::SDL_PROP_WINDOW_COCOA_WINDOW_POINTER)?;
                let view = unsafe { Self::objc_view(window, c"contentView") }?;
                Ok(rwh::RawWindowHandle::AppKit(rwh::AppKitWindowHandle::new(
                    view,
                )))
            }
            #[cfg(target_os = "ios")]
            "uikit" => {
                let window =
                    self.native_pointer(sys::video::SDL_PROP_WINDOW_UIKIT_WINDOW_POINTER)?;
                let controller = unsafe { Self::objc_view(window, c"rootViewController") }?;
                let view = unsafe { Self::objc_view(controller, c"view") }?;
                let mut handle = rwh::UiKitWindowHandle::new(view);
                handle.ui_view_controller = Some(controller);
                Ok(rwh::RawWindowHandle::UiKit(handle))
            }
            _ => Err(WindowError::Nul(format!(
                "native window handles are not supported by SDL video driver {driver}"
            ))),
        }
    }

    fn native_pointer(&self, name: *const c_char) -> Result<NonNull<c_void>, WindowError> {
        let pointer = unsafe {
            sys::properties::SDL_GetPointerProperty(
                self.get_properties()?.into_native(),
                name,
                std::ptr::null_mut(),
            )
        };
        NonNull::new(pointer)
            .ok_or_else(|| WindowError::Nul("native window property is unavailable".into()))
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    unsafe fn objc_view(
        object: NonNull<c_void>,
        selector: &CStr,
    ) -> Result<NonNull<c_void>, WindowError> {
        #[link(name = "objc")]
        unsafe extern "C" {
            fn sel_registerName(name: *const c_char) -> *mut c_void;
            fn objc_msgSend(object: *mut c_void, selector: *mut c_void) -> *mut c_void;
        }
        let pointer = unsafe { objc_msgSend(object.as_ptr(), sel_registerName(selector.as_ptr())) };
        NonNull::new(pointer)
            .ok_or_else(|| WindowError::Nul("native window view is unavailable".into()))
    }

    fn get_properties(&self) -> Result<WindowPropertiesId, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowProperties(self.raw.as_ptr()) };
        if value.0 == 0 {
            Err(SdlInstance::error())
        } else {
            Ok(value.into_native())
        }
    }
}

impl<const GPU: bool> SdlWindow<GPU> {
    pub(crate) fn refresh_metrics(&mut self) -> Result<(), WindowError> {
        let (width, height) = self.get_size_in_pixels()?;
        let width = u32::try_from(width).map_err(|error| WindowError::Nul(error.to_string()))?;
        let height = u32::try_from(height).map_err(|error| WindowError::Nul(error.to_string()))?;
        let pixel_ratio = self.get_display_scale()?;
        let pixel_density = self.get_pixel_density()?;
        let coordinate_scale = pixel_density / pixel_ratio;
        let (x, y) = self.get_position()?;
        self.screen_x = x as f32 * coordinate_scale;
        self.screen_y = y as f32 * coordinate_scale;
        self.screen_physical_x = (x as f64 * pixel_density as f64).round() as i32;
        self.screen_physical_y = (y as f64 * pixel_density as f64).round() as i32;
        self.inner_width = width as f32 / pixel_ratio;
        self.inner_height = height as f32 / pixel_ratio;
        self.inner_physical_width = width;
        self.inner_physical_height = height;
        self.device_pixel_ratio = pixel_ratio;
        let flags = self.flags();
        self.is_minimized = flags & SDL_WindowFlags::MINIMIZED != 0;
        self.is_maximized = flags & SDL_WindowFlags::MAXIMIZED != 0;
        self.is_borderless = flags & SDL_WindowFlags::BORDERLESS != 0;
        self.is_fullscreen = flags & SDL_WindowFlags::FULLSCREEN != 0;
        self.is_focused = flags & SDL_WindowFlags::INPUT_FOCUS != 0;
        let resize_border = self.is_borderless && flags & SDL_WindowFlags::RESIZABLE != 0;
        self.hit_test.set(ResizeHitTest {
            width: self.inner_width,
            height: self.inner_height,
            coordinate_scale,
            enabled: resize_border
                && !self.is_minimized
                && !self.is_maximized
                && !self.is_fullscreen,
        });
        if resize_border != self.hit_test_installed {
            let callback = if resize_border {
                Some(Self::hit_test_callback as _)
            } else {
                None
            };
            let data = if resize_border {
                (self.hit_test.as_ref() as *const Cell<ResizeHitTest>)
                    .cast_mut()
                    .cast()
            } else {
                std::ptr::null_mut()
            };
            Self::result(unsafe {
                sys::video::SDL_SetWindowHitTest(self.owner.raw.as_ptr(), callback, data)
            })?;
            self.hit_test_installed = resize_border;
        }
        Ok(())
    }

    pub(crate) fn get_pixel_density(&self) -> Result<f32, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowPixelDensity(self.owner.raw.as_ptr()) };
        if value > 0.0 {
            Ok(value)
        } else {
            Err(SdlInstance::error())
        }
    }

    pub(crate) fn get_display_scale(&self) -> Result<f32, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowDisplayScale(self.owner.raw.as_ptr()) };
        if value > 0.0 {
            Ok(value)
        } else {
            Err(SdlInstance::error())
        }
    }
    fn push_request(&self, code: i32) -> Result<(), WindowError> {
        let mut event = sys::events::SDL_Event {
            user: sys::events::SDL_UserEvent {
                r#type: self.owner.context.request_event,
                windowID: sys::video::SDL_WindowID(self.id().0),
                code,
                ..Default::default()
            },
        };
        Self::result(unsafe { sys::events::SDL_PushEvent(&mut event) })
    }

    fn result(success: bool) -> Result<(), WindowError> {
        if success {
            Ok(())
        } else {
            Err(SdlInstance::error())
        }
    }

    unsafe extern "C" fn hit_test_callback(
        _window: *mut sys::video::SDL_Window,
        point: *const sys::rect::SDL_Point,
        data: *mut c_void,
    ) -> sys::video::SDL_HitTestResult {
        let state = unsafe { &*data.cast::<Cell<ResizeHitTest>>() }.get();
        let point = unsafe { *point };
        state
            .test(
                point.x as f32 * state.coordinate_scale,
                point.y as f32 * state.coordinate_scale,
            )
            .into_native()
    }
}

impl<const GPU: bool> Drop for SdlWindow<GPU> {
    fn drop(&mut self) {
        self.owner
            .context
            .resize_sizes
            .borrow_mut()
            .remove(&self.id);
        self.owner.context.composing.borrow_mut().remove(&self.id);
        self.owner
            .context
            .mouse_positions
            .borrow_mut()
            .remove(&self.id);
        self.owner
            .context
            .pending_events
            .borrow_mut()
            .retain(|event| event.window_id != self.id);
        if self.hit_test_installed {
            unsafe {
                sys::video::SDL_SetWindowHitTest(
                    self.owner.raw.as_ptr(),
                    None,
                    std::ptr::null_mut(),
                )
            };
        }
    }
}

impl Drop for SdlWindowOwner {
    fn drop(&mut self) {
        unsafe { sys::video::SDL_DestroyWindow(self.raw.as_ptr()) };
    }
}

impl Drop for SdlContext {
    fn drop(&mut self) {
        drop(self.gamepads.get_mut().take());
        for (_, cursor) in self.cursors.get_mut().drain() {
            unsafe { sys::mouse::SDL_DestroyCursor(cursor.as_ptr()) };
        }
        let mut contexts = SDL_CONTEXTS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.video_initialized.get() {
            unsafe { sys::init::SDL_QuitSubSystem(sys::init::SDL_INIT_VIDEO) };
        }
        unsafe { sys::init::SDL_QuitSubSystem(sys::init::SDL_INIT_EVENTS) };
        if let Some((_, count)) = contexts.as_mut() {
            *count -= 1;
            if *count == 0 {
                unsafe { sys::init::SDL_Quit() };
                *contexts = None;
            }
        }
    }
}

unsafe fn copy_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

pub(crate) struct SdlWgpuFrame<'a> {
    resources: RefMut<'a, FrameResources>,
}

struct FrameResources {
    texture_view: wgpu::TextureView,
    command_encoder: wgpu::CommandEncoder,
    surface_texture: wgpu::SurfaceTexture,
}

struct WgpuContext {
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    clear_color: wgpu::Color,
    suspended: bool,
    instance: wgpu::Instance,
}

impl SdlInstance {
    pub(crate) fn create_wgpu_window(
        &self,
        title: &str,
        position: (i32, i32),
        size: (i32, i32),
        resizable: bool,
        hidden: bool,
    ) -> Result<SdlWgpuWindow, WindowError> {
        let mut flags = SDL_WindowFlags::HIDDEN;
        if resizable {
            flags |= SDL_WindowFlags::RESIZABLE;
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            flags |= SDL_WindowFlags::METAL;
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
        {
            flags |= SDL_WindowFlags::VULKAN;
        }
        let mut window = self.create_native_window::<true>(title, position, size, flags)?;
        window.gpu = Some(WgpuContext::new(&window)?);
        if !hidden {
            window.show()?;
            window.refresh_metrics()?;
        }
        Ok(window)
    }
}

impl SdlWindow<true> {
    pub(crate) fn begin_frame(&self) -> Result<bool, WindowError> {
        if self.gpu().suspended {
            return Ok(false);
        }
        if self
            .frame
            .try_borrow()
            .map_err(|error| WindowError::Nul(error.to_string()))?
            .is_some()
        {
            return Err(WindowError::Nul("a wgpu frame is already active".into()));
        }
        let Some(frame) = self.acquire_frame()? else {
            return Ok(false);
        };
        *self.frame.borrow_mut() = Some(frame);
        Ok(true)
    }

    pub(crate) fn finish_frame(&self, present: bool) -> Result<(), WindowError> {
        let frame = self
            .frame
            .try_borrow_mut()
            .map_err(|error| WindowError::Nul(error.to_string()))?
            .take()
            .ok_or_else(|| {
                WindowError::Nul("the render callback replaced the active window".into())
            })?;
        if present {
            self.gpu().queue.submit([frame.command_encoder.finish()]);
            self.gpu().queue.present(frame.surface_texture);
        }
        Ok(())
    }
}

impl WgpuContext {
    fn new(window: &SdlWgpuWindow) -> Result<Self, WindowError> {
        if unsafe { sys::video::SDL_WindowHasSurface(window.owner.raw.as_ptr()) }
            || window.flags() & SDL_WindowFlags::OPENGL != 0
        {
            return Err(WindowError::Nul(
                "wgpu windows cannot use an SDL software surface or OpenGL context".into(),
            ));
        }
        let (width, height) = window.get_size_in_pixels()?;
        let width = u32::try_from(width).map_err(|error| WindowError::Nul(error.to_string()))?;
        let height = u32::try_from(height).map_err(|error| WindowError::Nul(error.to_string()))?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: if cfg!(target_os = "windows") {
                wgpu::Backends::DX12
            } else {
                wgpu::Backends::METAL | wgpu::Backends::VULKAN
            },
            flags: wgpu::InstanceFlags::empty().with_env(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let raw_display_handle = window.display_handle()?.as_raw();
        let raw_window_handle = window.raw_window_handle()?.as_raw();
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })
        }
        .map_err(|error| WindowError::Nul(format!("failed to create wgpu surface: {error}")))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| WindowError::Nul(format!("failed to request wgpu adapter: {error}")))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("cxstd window device"),
            ..Default::default()
        }))
        .map_err(|error| WindowError::Nul(format!("failed to request wgpu device: {error}")))?;
        let mut surface_config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or_else(|| {
                WindowError::Nul("wgpu adapter is incompatible with the surface".into())
            })?;
        surface_config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .ok_or_else(|| {
                WindowError::Nul("wgpu surface does not support an UNORM format".into())
            })?;
        if width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            return Err(WindowError::Nul(
                "window size exceeds the wgpu device limits".into(),
            ));
        }
        let suspended = width == 0 || height == 0;
        if !suspended {
            surface.configure(&device, &surface_config);
        }
        Ok(Self {
            surface,
            adapter,
            device,
            queue,
            surface_config,
            clear_color: wgpu::Color {
                r: 17.0 / 255.0,
                g: 24.0 / 255.0,
                b: 39.0 / 255.0,
                a: 1.0,
            },
            suspended,
            instance,
        })
    }
}

impl SdlWindow<true> {
    fn gpu(&self) -> &WgpuContext {
        self.gpu
            .as_ref()
            .expect("GPU windows are created with a GPU context")
    }

    fn gpu_mut(&mut self) -> &mut WgpuContext {
        self.gpu
            .as_mut()
            .expect("GPU windows are created with a GPU context")
    }

    fn acquire_frame(&self) -> Result<Option<FrameResources>, WindowError> {
        let surface_texture = match self.gpu().surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.gpu()
                    .surface
                    .configure(&self.gpu().device, &self.gpu().surface_config);
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Err(WindowError::Nul("wgpu surface lost".into()));
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(WindowError::Nul("wgpu surface validation failed".into()));
            }
        };
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut clear = self
            .gpu()
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cxstd window clear encoder"),
            });
        {
            let _pass = clear.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cxstd window clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.gpu().clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self.gpu().queue.submit([clear.finish()]);
        let command_encoder =
            self.gpu()
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cxstd window frame encoder"),
                });
        Ok(Some(FrameResources {
            texture_view,
            command_encoder,
            surface_texture,
        }))
    }
}

impl SdlWgpuFrame<'_> {
    pub(crate) fn texture_view(&self) -> &wgpu::TextureView {
        &self.resources.texture_view
    }

    pub(crate) fn command_encoder(&mut self) -> &mut wgpu::CommandEncoder {
        &mut self.resources.command_encoder
    }
}

impl SdlWindow<true> {
    pub(crate) fn instance(&self) -> &wgpu::Instance {
        &self.gpu().instance
    }

    pub(crate) fn adapter(&self) -> &wgpu::Adapter {
        &self.gpu().adapter
    }

    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.gpu().device
    }

    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.gpu().queue
    }

    pub(crate) fn surface_config(&self) -> &wgpu::SurfaceConfiguration {
        &self.gpu().surface_config
    }

    pub(crate) fn clear_color(&self) -> wgpu::Color {
        self.gpu().clear_color
    }

    pub(crate) fn set_clear_color(&mut self, color: wgpu::Color) {
        self.gpu_mut().clear_color = color;
    }

    pub(crate) fn resize_surface(&mut self, width: u32, height: u32) -> Result<(), WindowError> {
        if self
            .frame
            .try_borrow()
            .map_err(|error| WindowError::Nul(error.to_string()))?
            .is_some()
        {
            return Err(WindowError::Nul(
                "cannot resize the surface while a frame is active".into(),
            ));
        }
        if width == 0 || height == 0 {
            self.gpu_mut().suspended = true;
            return Ok(());
        }
        let limit = self.gpu().device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return Err(WindowError::Nul(
                "window size exceeds the wgpu device limits".into(),
            ));
        }
        self.gpu_mut().surface_config.width = width;
        self.gpu_mut().surface_config.height = height;
        self.gpu()
            .surface
            .configure(&self.gpu().device, &self.gpu().surface_config);
        self.gpu_mut().suspended = false;
        Ok(())
    }

    pub(crate) fn next_frame(&self) -> Result<SdlWgpuFrame<'_>, WindowError> {
        let slot = self
            .frame
            .try_borrow_mut()
            .map_err(|error| WindowError::Nul(error.to_string()))?;
        let resources = RefMut::filter_map(slot, |slot| slot.as_mut())
            .map_err(|_| WindowError::Nul("a frame is only available during render".into()))?;
        Ok(SdlWgpuFrame { resources })
    }
}

mod conversions {
    use super::*;

    impl FromNative<sys::video::SDL_WindowID> for WindowId {
        fn from_native(value: sys::video::SDL_WindowID) -> Self {
            Self(value.0)
        }
    }

    impl FromNative<WindowId> for sys::video::SDL_WindowID {
        fn from_native(value: WindowId) -> Self {
            Self(value.0)
        }
    }

    impl FromNative<sys::properties::SDL_PropertiesID> for WindowPropertiesId {
        fn from_native(value: sys::properties::SDL_PropertiesID) -> Self {
            Self(value.0)
        }
    }

    impl FromNative<WindowPropertiesId> for sys::properties::SDL_PropertiesID {
        fn from_native(value: WindowPropertiesId) -> Self {
            Self(value.0)
        }
    }

    impl FromNative<sys::pixels::SDL_PixelFormat> for WindowPixelFormat {
        fn from_native(value: sys::pixels::SDL_PixelFormat) -> Self {
            Self(value.0)
        }
    }

    impl FromNative<WindowPixelFormat> for sys::pixels::SDL_PixelFormat {
        fn from_native(value: WindowPixelFormat) -> Self {
            Self(value.0)
        }
    }

    impl FromNative<u32> for WindowId {
        fn from_native(value: u32) -> Self {
            Self(value)
        }
    }

    impl FromNative<&sys::video::SDL_DisplayMode> for WindowDisplayMode {
        fn from_native(value: &sys::video::SDL_DisplayMode) -> Self {
            Self {
                display_id: value.displayID.0,
                format: value.format.into_native(),
                width: value.w,
                height: value.h,
                pixel_density: value.pixel_density,
                refresh_rate: value.refresh_rate,
                refresh_rate_numerator: value.refresh_rate_numerator,
                refresh_rate_denominator: value.refresh_rate_denominator,
                internal: value.internal.cast(),
            }
        }
    }

    impl FromNative<sys::video::SDL_DisplayMode> for WindowDisplayMode {
        fn from_native(value: sys::video::SDL_DisplayMode) -> Self {
            Self::from_native(&value)
        }
    }

    impl<'a> FromNative<&'a mut sys::surface::SDL_Surface> for WindowSurfaceRef<'a> {
        fn from_native(value: &'a mut sys::surface::SDL_Surface) -> Self {
            Self {
                raw: NonNull::from(value).cast(),
                marker: PhantomData,
            }
        }
    }

    impl FromNative<&WindowSurfaceRef<'_>> for *mut sys::surface::SDL_Surface {
        fn from_native(value: &WindowSurfaceRef<'_>) -> Self {
            value.raw.cast().as_ptr()
        }
    }

    impl FromNative<sys::rect::SDL_Rect> for WindowRect {
        fn from_native(value: sys::rect::SDL_Rect) -> Self {
            Self {
                x: value.x,
                y: value.y,
                width: value.w,
                height: value.h,
            }
        }
    }

    impl FromNative<WindowRect> for sys::rect::SDL_Rect {
        fn from_native(value: WindowRect) -> Self {
            Self {
                x: value.x,
                y: value.y,
                w: value.width,
                h: value.height,
            }
        }
    }

    impl FromNative<sys::video::SDL_FlashOperation> for WindowFlashOperation {
        fn from_native(value: sys::video::SDL_FlashOperation) -> Self {
            match value {
                sys::video::SDL_FlashOperation::CANCEL => Self::Cancel,
                sys::video::SDL_FlashOperation::BRIEFLY => Self::Briefly,
                sys::video::SDL_FlashOperation::UNTIL_FOCUSED => Self::UntilFocused,
                value => Self::Unknown(value.0),
            }
        }
    }

    impl FromNative<WindowFlashOperation> for sys::video::SDL_FlashOperation {
        fn from_native(value: WindowFlashOperation) -> Self {
            match value {
                WindowFlashOperation::Cancel => Self::CANCEL,
                WindowFlashOperation::Briefly => Self::BRIEFLY,
                WindowFlashOperation::UntilFocused => Self::UNTIL_FOCUSED,
                WindowFlashOperation::Unknown(value) => Self(value),
            }
        }
    }

    impl FromNative<sys::video::SDL_HitTestResult> for WindowHitTestResult {
        fn from_native(value: sys::video::SDL_HitTestResult) -> Self {
            match value {
                sys::video::SDL_HitTestResult::NORMAL => Self::Normal,
                sys::video::SDL_HitTestResult::DRAGGABLE => Self::Draggable,
                sys::video::SDL_HitTestResult::RESIZE_TOPLEFT => Self::ResizeTopLeft,
                sys::video::SDL_HitTestResult::RESIZE_TOP => Self::ResizeTop,
                sys::video::SDL_HitTestResult::RESIZE_TOPRIGHT => Self::ResizeTopRight,
                sys::video::SDL_HitTestResult::RESIZE_RIGHT => Self::ResizeRight,
                sys::video::SDL_HitTestResult::RESIZE_BOTTOMRIGHT => Self::ResizeBottomRight,
                sys::video::SDL_HitTestResult::RESIZE_BOTTOM => Self::ResizeBottom,
                sys::video::SDL_HitTestResult::RESIZE_BOTTOMLEFT => Self::ResizeBottomLeft,
                sys::video::SDL_HitTestResult::RESIZE_LEFT => Self::ResizeLeft,
                value => Self::Unknown(value.0),
            }
        }
    }

    impl FromNative<WindowHitTestResult> for sys::video::SDL_HitTestResult {
        fn from_native(value: WindowHitTestResult) -> Self {
            match value {
                WindowHitTestResult::Normal => Self::NORMAL,
                WindowHitTestResult::Draggable => Self::DRAGGABLE,
                WindowHitTestResult::ResizeTopLeft => Self::RESIZE_TOPLEFT,
                WindowHitTestResult::ResizeTop => Self::RESIZE_TOP,
                WindowHitTestResult::ResizeTopRight => Self::RESIZE_TOPRIGHT,
                WindowHitTestResult::ResizeRight => Self::RESIZE_RIGHT,
                WindowHitTestResult::ResizeBottomRight => Self::RESIZE_BOTTOMRIGHT,
                WindowHitTestResult::ResizeBottom => Self::RESIZE_BOTTOM,
                WindowHitTestResult::ResizeBottomLeft => Self::RESIZE_BOTTOMLEFT,
                WindowHitTestResult::ResizeLeft => Self::RESIZE_LEFT,
                WindowHitTestResult::Unknown(value) => Self(value),
            }
        }
    }

    impl FromNative<sys::video::SDL_ProgressState> for WindowProgressState {
        fn from_native(value: sys::video::SDL_ProgressState) -> Self {
            match value {
                sys::video::SDL_ProgressState::INVALID => Self::Invalid,
                sys::video::SDL_ProgressState::NONE => Self::None,
                sys::video::SDL_ProgressState::INDETERMINATE => Self::Indeterminate,
                sys::video::SDL_ProgressState::NORMAL => Self::Normal,
                sys::video::SDL_ProgressState::PAUSED => Self::Paused,
                sys::video::SDL_ProgressState::ERROR => Self::Error,
                value => Self::Unknown(value.0),
            }
        }
    }

    impl FromNative<WindowProgressState> for sys::video::SDL_ProgressState {
        fn from_native(value: WindowProgressState) -> Self {
            match value {
                WindowProgressState::Invalid => Self::INVALID,
                WindowProgressState::None => Self::NONE,
                WindowProgressState::Indeterminate => Self::INDETERMINATE,
                WindowProgressState::Normal => Self::NORMAL,
                WindowProgressState::Paused => Self::PAUSED,
                WindowProgressState::Error => Self::ERROR,
                WindowProgressState::Unknown(value) => Self(value),
            }
        }
    }

    fn keyboard_snapshot(
        value: sys::events::SDL_KeyboardEvent,
    ) -> (String, KeyCode, KeyLocation, u32) {
        let modifiers = value.r#mod.0;
        let is_composing = PLATFORM_CONTEXT.with(|slot| {
            slot.borrow().upgrade().is_some_and(|context| {
                context.modifiers.set(value.r#mod);
                context
                    .composing
                    .borrow()
                    .contains(&WindowId(value.windowID.0))
            })
        });
        // Bypass SDL key-event options that intentionally hide layout/modifier changes.
        let key =
            unsafe { sys::keyboard::SDL_GetKeyFromScancode(value.scancode, value.r#mod, false) };
        let mut key = web_key(if key.0 == 0 { value.key.0 } else { key.0 });
        if modifiers & 0x4000 != 0 && value.scancode.0 == 230 {
            key = "AltGraph".into();
        }
        if modifiers & 0x1000 == 0 {
            key = match value.scancode.0 {
                89 => "End",
                90 => "ArrowDown",
                91 => "PageDown",
                92 => "ArrowLeft",
                93 => "Clear",
                94 => "ArrowRight",
                95 => "Home",
                96 => "ArrowUp",
                97 => "PageUp",
                98 => "Insert",
                99 => "Delete",
                _ => &key,
            }
            .to_string();
        }
        (
            key,
            web_code(value.scancode.0),
            match value.scancode.0 {
                224..=227 => KeyLocation::Left,
                228..=231 => KeyLocation::Right,
                83..=99 | 103 | 133 | 134 | 176..=221 => KeyLocation::Numpad,
                _ => KeyLocation::Standard,
            },
            u32::from(value.r#mod.0)
                | if value.repeat { 1 << 16 } else { 0 }
                | if is_composing { 1 << 17 } else { 0 },
        )
    }

    impl FromNative<sys::events::SDL_KeyboardEvent> for KeyboardEvent {
        fn from_native(value: sys::events::SDL_KeyboardEvent) -> Self {
            let (key, code, location, flags) = keyboard_snapshot(value);
            Self {
                key,
                code,
                location,
                flags,
            }
        }
    }

    pub(super) fn web_code(scancode: i32) -> KeyCode {
        match scancode {
            4 => KeyCode::KeyA,
            5 => KeyCode::KeyB,
            6 => KeyCode::KeyC,
            7 => KeyCode::KeyD,
            8 => KeyCode::KeyE,
            9 => KeyCode::KeyF,
            10 => KeyCode::KeyG,
            11 => KeyCode::KeyH,
            12 => KeyCode::KeyI,
            13 => KeyCode::KeyJ,
            14 => KeyCode::KeyK,
            15 => KeyCode::KeyL,
            16 => KeyCode::KeyM,
            17 => KeyCode::KeyN,
            18 => KeyCode::KeyO,
            19 => KeyCode::KeyP,
            20 => KeyCode::KeyQ,
            21 => KeyCode::KeyR,
            22 => KeyCode::KeyS,
            23 => KeyCode::KeyT,
            24 => KeyCode::KeyU,
            25 => KeyCode::KeyV,
            26 => KeyCode::KeyW,
            27 => KeyCode::KeyX,
            28 => KeyCode::KeyY,
            29 => KeyCode::KeyZ,
            30 => KeyCode::Digit1,
            31 => KeyCode::Digit2,
            32 => KeyCode::Digit3,
            33 => KeyCode::Digit4,
            34 => KeyCode::Digit5,
            35 => KeyCode::Digit6,
            36 => KeyCode::Digit7,
            37 => KeyCode::Digit8,
            38 => KeyCode::Digit9,
            39 => KeyCode::Digit0,
            40 => KeyCode::Enter,
            41 => KeyCode::Escape,
            42 => KeyCode::Backspace,
            43 => KeyCode::Tab,
            44 => KeyCode::Space,
            45 => KeyCode::Minus,
            46 => KeyCode::Equal,
            47 => KeyCode::BracketLeft,
            48 => KeyCode::BracketRight,
            49 => KeyCode::Backslash,
            50 => KeyCode::Backslash,
            51 => KeyCode::Semicolon,
            52 => KeyCode::Quote,
            53 => KeyCode::Backquote,
            54 => KeyCode::Comma,
            55 => KeyCode::Period,
            56 => KeyCode::Slash,
            57 => KeyCode::CapsLock,
            58 => KeyCode::F1,
            59 => KeyCode::F2,
            60 => KeyCode::F3,
            61 => KeyCode::F4,
            62 => KeyCode::F5,
            63 => KeyCode::F6,
            64 => KeyCode::F7,
            65 => KeyCode::F8,
            66 => KeyCode::F9,
            67 => KeyCode::F10,
            68 => KeyCode::F11,
            69 => KeyCode::F12,
            70 => KeyCode::PrintScreen,
            71 => KeyCode::ScrollLock,
            72 => KeyCode::Pause,
            73 => KeyCode::Insert,
            74 => KeyCode::Home,
            75 => KeyCode::PageUp,
            76 => KeyCode::Delete,
            77 => KeyCode::End,
            78 => KeyCode::PageDown,
            79 => KeyCode::ArrowRight,
            80 => KeyCode::ArrowLeft,
            81 => KeyCode::ArrowDown,
            82 => KeyCode::ArrowUp,
            83 => KeyCode::NumLock,
            84 => KeyCode::NumpadDivide,
            85 => KeyCode::NumpadMultiply,
            86 => KeyCode::NumpadSubtract,
            87 => KeyCode::NumpadAdd,
            88 => KeyCode::NumpadEnter,
            89 => KeyCode::Numpad1,
            90 => KeyCode::Numpad2,
            91 => KeyCode::Numpad3,
            92 => KeyCode::Numpad4,
            93 => KeyCode::Numpad5,
            94 => KeyCode::Numpad6,
            95 => KeyCode::Numpad7,
            96 => KeyCode::Numpad8,
            97 => KeyCode::Numpad9,
            98 => KeyCode::Numpad0,
            99 => KeyCode::NumpadDecimal,
            100 => KeyCode::IntlBackslash,
            101 => KeyCode::ContextMenu,
            104 => KeyCode::F13,
            105 => KeyCode::F14,
            106 => KeyCode::F15,
            107 => KeyCode::F16,
            108 => KeyCode::F17,
            109 => KeyCode::F18,
            110 => KeyCode::F19,
            111 => KeyCode::F20,
            112 => KeyCode::F21,
            113 => KeyCode::F22,
            114 => KeyCode::F23,
            115 => KeyCode::F24,
            117 => KeyCode::Help,
            118 => KeyCode::ContextMenu,
            135 => KeyCode::IntlRo,
            136 => KeyCode::KanaMode,
            137 => KeyCode::IntlYen,
            138 => KeyCode::Convert,
            139 => KeyCode::NonConvert,
            158 => KeyCode::Enter,
            224 => KeyCode::ControlLeft,
            225 => KeyCode::ShiftLeft,
            226 => KeyCode::AltLeft,
            227 => KeyCode::MetaLeft,
            228 => KeyCode::ControlRight,
            229 => KeyCode::ShiftRight,
            230 => KeyCode::AltRight,
            231 => KeyCode::MetaRight,
            _ => KeyCode::Unidentified,
        }
    }

    fn web_key(key: u32) -> String {
        match key {
            8 => return "Backspace".into(),
            9 | 0x2000_0001 => return "Tab".into(),
            13 => return "Enter".into(),
            27 => return "Escape".into(),
            127 => return "Delete".into(),
            _ => {}
        }
        if key & 0x6000_0000 == 0 {
            return char::from_u32(key)
                .filter(|c| !c.is_control())
                .map(|c| c.to_string())
                .unwrap_or_else(|| "Unidentified".into());
        }
        let scancode = (key & !0x4000_0000) as i32;
        match scancode {
            57 => "CapsLock",
            58..=69 => return format!("F{}", scancode - 57),
            70 => "PrintScreen",
            71 => "ScrollLock",
            72 => "Pause",
            73 => "Insert",
            74 => "Home",
            75 => "PageUp",
            76 => "Delete",
            77 => "End",
            78 => "PageDown",
            79 => "ArrowRight",
            80 => "ArrowLeft",
            81 => "ArrowDown",
            82 => "ArrowUp",
            83 => "NumLock",
            84 => "/",
            85 => "*",
            86 => "-",
            87 => "+",
            88 | 158 => "Enter",
            89..=97 => return (scancode - 88).to_string(),
            98 => "0",
            99 => ".",
            101 | 118 => "ContextMenu",
            102 => "Power",
            103 | 134 => "=",
            104..=115 => return format!("F{}", scancode - 91),
            116 => "Open",
            117 => "Help",
            119 => "Select",
            120 => "Stop",
            121 => "Again",
            122 => "Undo",
            123 => "Cut",
            124 => "Copy",
            125 => "Paste",
            126 => "Find",
            127 => "AudioVolumeMute",
            128 => "AudioVolumeUp",
            129 => "AudioVolumeDown",
            133 => ",",
            136 => "KanaMode",
            138 => "Convert",
            139 => "NonConvert",
            144 => "HangulMode",
            145 => "HanjaMode",
            146 => "Katakana",
            147 => "Hiragana",
            148 => "ZenkakuHankaku",
            154 => "PrintScreen",
            155 => "Cancel",
            156 | 162 | 216 | 217 => "Clear",
            157 => "PageUp",
            159 => "Separator",
            187 => "Backspace",
            224 | 228 => "Control",
            225 | 229 => "Shift",
            226 | 230 => "Alt",
            227 | 231 => "Meta",
            257 => "AltGraph",
            258 => "Standby",
            259 => "WakeUp",
            260 => "ChannelUp",
            261 => "ChannelDown",
            262 => "MediaPlay",
            263 => "MediaPause",
            264 => "MediaRecord",
            265 => "MediaFastForward",
            266 => "MediaRewind",
            267 => "MediaTrackNext",
            268 => "MediaTrackPrevious",
            269 => "MediaStop",
            270 => "Eject",
            271 => "MediaPlayPause",
            272 => "LaunchMediaPlayer",
            273 => "New",
            274 => "Open",
            275 => "Close",
            276 => "Exit",
            277 => "Save",
            278 => "Print",
            279 => "Props",
            280 => "BrowserSearch",
            281 => "BrowserHome",
            282 => "BrowserBack",
            283 => "BrowserForward",
            284 => "BrowserStop",
            285 => "BrowserRefresh",
            286 => "BrowserFavorites",
            287 => "Soft1",
            288 => "Soft2",
            289 => "Call",
            290 => "EndCall",
            _ => "Unidentified",
        }
        .into()
    }

    impl CompositionUpdateEvent {
        /// # Safety
        /// The native text pointer must be valid and NUL-terminated during this call.
        pub(super) unsafe fn from_sdl(value: &sys::events::SDL_TextEditingEvent) -> Self {
            let data = unsafe { copy_string(value.text) }.unwrap_or_default();
            let selection = usize::try_from(value.start)
                .ok()
                .zip(usize::try_from(value.length).ok());
            let offset = |index| data.chars().take(index).map(|c| c.len_utf16() as u32).sum();
            let (selection_start, selection_end) = match selection {
                Some((start, length)) => (Some(offset(start)), Some(offset(start + length))),
                None => (None, None),
            };
            Self {
                data,
                selection_start,
                selection_end,
            }
        }
    }

    impl TextInputEvent {
        /// # Safety
        /// The native text pointer must be valid and NUL-terminated during this call.
        pub(super) unsafe fn from_sdl(value: &sys::events::SDL_TextInputEvent) -> Self {
            Self {
                data: unsafe { copy_string(value.text) }.unwrap_or_default(),
            }
        }
    }

    struct MouseSnapshot {
        screen_x: f64,
        screen_y: f64,
        client_x: f64,
        client_y: f64,
        flags: u32,
        factor: f64,
    }

    fn mouse_snapshot(id: sys::video::SDL_WindowID, x: f32, y: f32) -> MouseSnapshot {
        PLATFORM_CONTEXT.with(|slot| {
            if let Some(context) = slot.borrow().upgrade() {
                context
                    .mouse_positions
                    .borrow_mut()
                    .insert(WindowId(id.0), (x, y));
            }
        });
        let modifiers = PLATFORM_CONTEXT.with(|slot| {
            slot.borrow()
                .upgrade()
                .map(|context| context.modifiers.get().0)
                .unwrap_or_default()
        });
        let (density, scale) = window_scales(id).unwrap_or((1.0, 1.0));
        let factor = f64::from(density) / f64::from(scale);
        let (mut screen_x, mut screen_y) = (0, 0);
        if let Some(window) = window_pointer(id) {
            unsafe {
                sys::video::SDL_GetWindowPosition(window.as_ptr(), &mut screen_x, &mut screen_y)
            };
        }
        let buttons = PLATFORM_CONTEXT.with(|slot| {
            slot.borrow()
                .upgrade()
                .map(|context| context.mouse_buttons.get())
                .unwrap_or_default()
        });
        MouseSnapshot {
            client_x: f64::from(x) * factor,
            client_y: f64::from(y) * factor,
            screen_x: (f64::from(screen_x) + f64::from(x)) * factor,
            screen_y: (f64::from(screen_y) + f64::from(y)) * factor,
            flags: u32::from(modifiers) | ((buttons & 0x1f) << 18),
            factor,
        }
    }

    impl FromNative<sys::events::SDL_MouseMotionEvent> for MouseEvent {
        fn from_native(value: sys::events::SDL_MouseMotionEvent) -> Self {
            PLATFORM_CONTEXT.with(|slot| {
                if let Some(context) = slot.borrow().upgrade() {
                    context.mouse_buttons.set(value.state.0);
                }
            });
            let mouse = mouse_snapshot(value.windowID, value.x, value.y);
            Self {
                screen_x: mouse.screen_x,
                screen_y: mouse.screen_y,
                client_x: mouse.client_x,
                client_y: mouse.client_y,
                flags: mouse.flags,
                movement_x: f64::from(value.xrel) * mouse.factor,
                movement_y: f64::from(value.yrel) * mouse.factor,
                ..Self::default()
            }
        }
    }

    fn mouse_button_code(button: u8) -> KeyCode {
        match button {
            1 => KeyCode::MouseLeft,
            2 => KeyCode::MouseMiddle,
            3 => KeyCode::MouseRight,
            4 => KeyCode::MouseBack,
            5 => KeyCode::MouseForward,
            _ => KeyCode::Unidentified,
        }
    }

    impl FromNative<sys::events::SDL_MouseButtonEvent> for MouseEvent {
        fn from_native(value: sys::events::SDL_MouseButtonEvent) -> Self {
            let mask = match value.button {
                1..=32 => 1u32 << (value.button - 1),
                _ => 0,
            };
            PLATFORM_CONTEXT.with(|slot| {
                if let Some(context) = slot.borrow().upgrade() {
                    let buttons = context.mouse_buttons.get();
                    context.mouse_buttons.set(if value.down {
                        buttons | mask
                    } else {
                        buttons & !mask
                    });
                }
            });
            let mouse = mouse_snapshot(value.windowID, value.x, value.y);
            Self {
                screen_x: mouse.screen_x,
                screen_y: mouse.screen_y,
                client_x: mouse.client_x,
                client_y: mouse.client_y,
                flags: mouse.flags,
                button: mouse_button_code(value.button),
                // SDL already includes the current press in its click count.
                detail: i32::from(value.clicks),
                ..Self::default()
            }
        }
    }

    impl FromNative<sys::events::SDL_MouseWheelEvent> for WheelEvent {
        fn from_native(value: sys::events::SDL_MouseWheelEvent) -> Self {
            let mouse = mouse_snapshot(value.windowID, value.mouse_x, value.mouse_y);
            let (mut delta_x, mut delta_y) = (f64::from(value.x), -f64::from(value.y));
            if value.direction == sys::mouse::SDL_MOUSEWHEEL_FLIPPED {
                delta_x = -delta_x;
                delta_y = -delta_y;
            }
            Self {
                screen_x: mouse.screen_x,
                screen_y: mouse.screen_y,
                client_x: mouse.client_x,
                client_y: mouse.client_y,
                flags: mouse.flags,
                delta_x,
                delta_y,
                delta_z: 0.0,
            }
        }
    }

    fn mouse_boundary_snapshot(id: sys::video::SDL_WindowID) -> MouseSnapshot {
        let position = PLATFORM_CONTEXT.with(|slot| {
            slot.borrow().upgrade().and_then(|context| {
                context
                    .mouse_positions
                    .borrow()
                    .get(&WindowId(id.0))
                    .copied()
            })
        });
        let (x, y) = position.unwrap_or_else(|| {
            let (mut x, mut y) = (0.0, 0.0);
            unsafe { sys::mouse::SDL_GetMouseState(&mut x, &mut y) };
            (x, y)
        });
        mouse_snapshot(id, x, y)
    }

    impl PlatformEventMessage {
        /// Returns None for unsupported SDL events and unregistered user events.
        ///
        /// # Safety
        /// The tag must identify an initialized SDL event payload.
        /// Referenced strings and counted arrays must be valid for reading during this call.
        /// State-dependent events require initialized SDL on its video thread.
        /// Events whose required window or display state cannot be queried return None.
        pub(super) unsafe fn from_sdl(value: &sys::events::SDL_Event) -> Option<Self> {
            let event_type = sys::events::SDL_EventType(unsafe { value.r#type });
            let time_stamp = Duration::from_nanos(unsafe { value.common.timestamp });
            let (window_id, mut kind) = match event_type {
                sys::events::SDL_EVENT_QUIT | sys::events::SDL_EVENT_TERMINATING => {
                    (WindowId(0), PlatformEvent::Quit)
                }
                sys::events::SDL_EVENT_KEY_DOWN => (
                    unsafe { value.key.windowID }.into_native(),
                    PlatformEvent::KeyDown(unsafe { value.key }.into_native()),
                ),
                sys::events::SDL_EVENT_KEY_UP => (
                    unsafe { value.key.windowID }.into_native(),
                    PlatformEvent::KeyUp(unsafe { value.key }.into_native()),
                ),
                sys::events::SDL_EVENT_TEXT_EDITING => (
                    unsafe { value.edit.windowID }.into_native(),
                    PlatformEvent::CompositionUpdate(unsafe {
                        CompositionUpdateEvent::from_sdl(&value.edit)
                    }),
                ),
                sys::events::SDL_EVENT_TEXT_INPUT => (
                    unsafe { value.text.windowID }.into_native(),
                    PlatformEvent::TextInput(unsafe { TextInputEvent::from_sdl(&value.text) }),
                ),
                sys::events::SDL_EVENT_MOUSE_MOTION => (
                    unsafe { value.motion.windowID }.into_native(),
                    PlatformEvent::MouseMove(unsafe { value.motion }.into_native()),
                ),
                sys::events::SDL_EVENT_MOUSE_BUTTON_DOWN => (
                    unsafe { value.button.windowID }.into_native(),
                    PlatformEvent::MouseDown(unsafe { value.button }.into_native()),
                ),
                sys::events::SDL_EVENT_MOUSE_BUTTON_UP => (
                    unsafe { value.button.windowID }.into_native(),
                    PlatformEvent::MouseUp(unsafe { value.button }.into_native()),
                ),
                sys::events::SDL_EVENT_MOUSE_WHEEL => (
                    unsafe { value.wheel.windowID }.into_native(),
                    PlatformEvent::Wheel(unsafe { value.wheel }.into_native()),
                ),
                sys::events::SDL_EVENT_WINDOW_SHOWN | sys::events::SDL_EVENT_WINDOW_EXPOSED => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::Redraw,
                ),
                sys::events::SDL_EVENT_WINDOW_MOVED => {
                    (unsafe { value.window.windowID }.into_native(), {
                        let window = unsafe { value.window };
                        PlatformEvent::Move(MoveEvent {
                            screen_x: window.data1 as f32,
                            screen_y: window.data2 as f32,
                        })
                    })
                }
                sys::events::SDL_EVENT_WINDOW_RESIZED
                | sys::events::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => {
                    let id = unsafe { value.window.windowID };
                    let window = window_pointer(id)?;
                    let (mut width, mut height) = (0, 0);
                    if !unsafe {
                        sys::video::SDL_GetWindowSizeInPixels(
                            window.as_ptr(),
                            &mut width,
                            &mut height,
                        )
                    } {
                        return None;
                    }
                    let inner_physical_width = u32::try_from(width).ok()?;
                    let inner_physical_height = u32::try_from(height).ok()?;
                    let (_, display_scale) = window_scales(id)?;
                    (
                        id.into_native(),
                        PlatformEvent::Resize(ResizeEvent {
                            inner_width: inner_physical_width as f32 / display_scale,
                            inner_height: inner_physical_height as f32 / display_scale,
                            inner_physical_width,
                            inner_physical_height,
                        }),
                    )
                }
                sys::events::SDL_EVENT_WINDOW_MINIMIZED => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::Minimize,
                ),
                sys::events::SDL_EVENT_WINDOW_MAXIMIZED => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::Maximize,
                ),
                sys::events::SDL_EVENT_WINDOW_RESTORED => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::Restore,
                ),
                sys::events::SDL_EVENT_WINDOW_MOUSE_ENTER => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::MouseEnter({
                        let mouse = mouse_boundary_snapshot(unsafe { value.window.windowID });
                        MouseEvent {
                            screen_x: mouse.screen_x,
                            screen_y: mouse.screen_y,
                            client_x: mouse.client_x,
                            client_y: mouse.client_y,
                            flags: mouse.flags,
                            ..MouseEvent::default()
                        }
                    }),
                ),
                sys::events::SDL_EVENT_WINDOW_MOUSE_LEAVE => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::MouseLeave({
                        let mouse = mouse_boundary_snapshot(unsafe { value.window.windowID });
                        MouseEvent {
                            screen_x: mouse.screen_x,
                            screen_y: mouse.screen_y,
                            client_x: mouse.client_x,
                            client_y: mouse.client_y,
                            flags: mouse.flags,
                            ..MouseEvent::default()
                        }
                    }),
                ),
                sys::events::SDL_EVENT_WINDOW_FOCUS_GAINED => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::Focus,
                ),
                sys::events::SDL_EVENT_WINDOW_FOCUS_LOST => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::Blur,
                ),
                sys::events::SDL_EVENT_WINDOW_CLOSE_REQUESTED => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::Close,
                ),
                sys::events::SDL_EVENT_WINDOW_DISPLAY_SCALE_CHANGED => {
                    let window_id = unsafe { value.window.windowID };
                    let window = window_pointer(window_id)?;
                    let (_, device_pixel_ratio) = window_scales(window_id)?;
                    let (mut width, mut height) = (0, 0);
                    if !unsafe {
                        sys::video::SDL_GetWindowSizeInPixels(
                            window.as_ptr(),
                            &mut width,
                            &mut height,
                        )
                    } {
                        return None;
                    }
                    (
                        window_id.into_native(),
                        PlatformEvent::DevicePixelRatioChange(DevicePixelRatioChangeEvent {
                            device_pixel_ratio,
                            inner_physical_width: u32::try_from(width).ok()?,
                            inner_physical_height: u32::try_from(height).ok()?,
                        }),
                    )
                }
                sys::events::SDL_EVENT_WINDOW_ENTER_FULLSCREEN => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::FullscreenChange(true),
                ),
                sys::events::SDL_EVENT_WINDOW_LEAVE_FULLSCREEN => (
                    unsafe { value.window.windowID }.into_native(),
                    PlatformEvent::FullscreenChange(false),
                ),

                _ => {
                    let context = PLATFORM_CONTEXT.with(|slot| slot.borrow().upgrade())?;
                    if event_type.0 != context.request_event {
                        return None;
                    }
                    let event = unsafe { value.user };
                    window_pointer(event.windowID)?;
                    let kind = match event.code {
                        REDRAW_REQUEST => PlatformEvent::Redraw,
                        CLOSE_REQUEST => PlatformEvent::Close,
                        _ => return None,
                    };
                    (event.windowID.into_native(), kind)
                }
            };
            if uses_logical_coordinates(&kind) {
                let (density, scale) = window_scales(sys::video::SDL_WindowID(window_id.0))?;
                normalize_logical_coordinates(&mut kind, density / scale);
            }
            Some(Self {
                time_stamp,
                window_id,
                event: kind,
            })
        }
    }

    impl FromNative<WindowCursor> for sys::mouse::SDL_SystemCursor {
        fn from_native(value: WindowCursor) -> Self {
            match value {
                WindowCursor::Auto
                | WindowCursor::Default
                | WindowCursor::None
                | WindowCursor::ContextMenu
                | WindowCursor::Help
                | WindowCursor::Alias
                | WindowCursor::Copy
                | WindowCursor::ZoomIn
                | WindowCursor::ZoomOut => Self::DEFAULT,
                WindowCursor::Pointer => Self::POINTER,
                WindowCursor::Progress => Self::PROGRESS,
                WindowCursor::Wait => Self::WAIT,
                WindowCursor::Cell | WindowCursor::Crosshair => Self::CROSSHAIR,
                WindowCursor::Text | WindowCursor::VerticalText => Self::TEXT,
                WindowCursor::Move
                | WindowCursor::Grab
                | WindowCursor::Grabbing
                | WindowCursor::AllScroll => Self::MOVE,
                WindowCursor::NoDrop | WindowCursor::NotAllowed => Self::NOT_ALLOWED,
                WindowCursor::ColResize | WindowCursor::EwResize => Self::EW_RESIZE,
                WindowCursor::RowResize | WindowCursor::NsResize => Self::NS_RESIZE,
                WindowCursor::NResize => Self::N_RESIZE,
                WindowCursor::EResize => Self::E_RESIZE,
                WindowCursor::SResize => Self::S_RESIZE,
                WindowCursor::WResize => Self::W_RESIZE,
                WindowCursor::NeResize => Self::NE_RESIZE,
                WindowCursor::NwResize => Self::NW_RESIZE,
                WindowCursor::SeResize => Self::SE_RESIZE,
                WindowCursor::SwResize => Self::SW_RESIZE,
                WindowCursor::NeswResize => Self::NESW_RESIZE,
                WindowCursor::NwseResize => Self::NWSE_RESIZE,
            }
        }
    }

    fn uses_logical_coordinates(kind: &PlatformEvent) -> bool {
        matches!(kind, PlatformEvent::Move(_))
    }

    fn normalize_logical_coordinates(kind: &mut PlatformEvent, factor: f32) {
        if let PlatformEvent::Move(event) = kind {
            event.screen_x *= factor;
            event.screen_y *= factor;
        }
    }

    fn window_pointer(id: sys::video::SDL_WindowID) -> Option<NonNull<sys::video::SDL_Window>> {
        NonNull::new(unsafe { sys::video::SDL_GetWindowFromID(id) })
    }

    fn window_scales(id: sys::video::SDL_WindowID) -> Option<(f32, f32)> {
        let window = window_pointer(id)?;
        let pixel_density = unsafe { sys::video::SDL_GetWindowPixelDensity(window.as_ptr()) };
        let display_scale = unsafe { sys::video::SDL_GetWindowDisplayScale(window.as_ptr()) };
        (pixel_density > 0.0 && display_scale > 0.0).then_some((pixel_density, display_scale))
    }
}
