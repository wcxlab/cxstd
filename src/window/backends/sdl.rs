//! `sdl_inst.create_wgpu_window(...)` pairs an owned SDL window with GPU presentation.
//! Route events through `window.window_event(&event, &mut handler)`, then call
//! `window.update(&mut handler)` and `window.render(&mut handler)` from the host loop.
//! Frame access is limited to the render callback; frames and surfaces drop before the native window.

#[cfg(feature = "wgpu")]
use std::{
    cell::RefMut,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
    mem::MaybeUninit,
    ptr::NonNull,
    rc::Rc,
    sync::Mutex,
    thread::{self, ThreadId},
};

use sdl3_sys as sys;

use super::super::*;

static SDL_CONTEXTS: Mutex<Option<(ThreadId, usize)>> = Mutex::new(None);

struct SdlContext {
    cursors: RefCell<HashMap<WindowSystemCursor, NonNull<sys::mouse::SDL_Cursor>>>,
}

struct SdlWindowOwner {
    raw: NonNull<sys::video::SDL_Window>,
    context: Rc<SdlContext>,
}

pub struct SdlInstance {
    context: Rc<SdlContext>,
}

pub struct SdlWindow {
    raw: NonNull<sys::video::SDL_Window>,
    hit_test: Box<Option<HitTestCallback>>,
    owner: Rc<SdlWindowOwner>,
}

type HitTestCallback = Box<dyn Fn(i32, i32) -> WindowHitTestResult>;

pub fn init() -> Result<SdlInstance, WindowError> {
    let mut contexts = SDL_CONTEXTS.lock().map_err(|_| WindowError::Unknown)?;
    let current_thread = thread::current().id();
    if let Some((owner, _)) = contexts.as_ref() {
        if *owner != current_thread {
            return Err(WindowError::Nul(
                "window context must be initialized on the video thread".into(),
            ));
        }
    } else {
        #[cfg(target_vendor = "apple")]
        if !unsafe { sys::init::SDL_IsMainThread() } {
            return Err(WindowError::Nul(
                "window context must be initialized on the main thread".into(),
            ));
        }
        unsafe { sys::main::SDL_SetMainReady() };
    }

    if !unsafe { sys::init::SDL_Init(sys::init::SDL_INIT_VIDEO) } {
        let error = SdlInstance::error();
        if contexts.is_none() {
            unsafe { sys::init::SDL_Quit() };
        }
        return Err(error);
    }

    let (_, count) = contexts.get_or_insert((current_thread, 0));
    *count += 1;
    Ok(SdlInstance {
        context: Rc::new(SdlContext {
            cursors: RefCell::new(HashMap::new()),
        }),
    })
}

impl SdlInstance {
    pub fn poll_event(&mut self) -> Option<WindowEvent> {
        let mut event = MaybeUninit::<sys::events::SDL_Event>::zeroed();
        if unsafe { sys::events::SDL_PollEvent(event.as_mut_ptr()) } {
            Some(unsafe { WindowEvent::from_sdl(event.assume_init_ref()) })
        } else {
            None
        }
    }

    pub fn create_window(
        &self,
        title: &str,
        width: i32,
        height: i32,
        flags: WindowFlags,
    ) -> Result<SdlWindow, WindowError> {
        if flags.intersects(WindowFlags::MODAL | WindowFlags::TOOLTIP | WindowFlags::POPUP_MENU) {
            return Err(WindowError::Nul("child windows are not supported".into()));
        }
        let title = CString::new(title).map_err(|error| WindowError::Nul(error.to_string()))?;
        let raw =
            unsafe { sys::video::SDL_CreateWindow(title.as_ptr(), width, height, flags.into()) };
        let raw = NonNull::new(raw).ok_or_else(Self::error)?;
        Ok(SdlWindow {
            raw,
            hit_test: Box::new(None),
            owner: Rc::new(SdlWindowOwner {
                raw,
                context: Rc::clone(&self.context),
            }),
        })
    }
}

impl SdlInstance {
    fn error() -> WindowError {
        unsafe { copy_string(sys::error::SDL_GetError()) }
            .map(WindowError::Nul)
            .unwrap_or(WindowError::Unknown)
    }
}

impl SdlWindow {
    pub fn window_handle_provider(&self) -> Rc<dyn raw_window_handle::HasWindowHandle> {
        self.owner.clone()
    }
}

impl raw_window_handle::HasWindowHandle for SdlWindowOwner {
    fn window_handle(&self) -> Result<WindowHandle<'_>, WindowHandleError> {
        let raw = self
            .raw_window_handle()
            .map_err(|_| WindowHandleError::Unavailable)?;
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl Window for SdlWindow {
    fn get_id(&self) -> Result<WindowId, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowID(self.raw.as_ptr()) };
        if value.0 == 0 {
            Err(SdlInstance::error())
        } else {
            Ok(value.into())
        }
    }

    fn get_display(&self) -> Result<WindowDisplayId, WindowError> {
        let value = unsafe { sys::video::SDL_GetDisplayForWindow(self.raw.as_ptr()) };
        if value.0 == 0 {
            Err(SdlInstance::error())
        } else {
            Ok(value.into())
        }
    }

    fn get_pixel_density(&self) -> Result<f32, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowPixelDensity(self.raw.as_ptr()) };
        if value > 0.0 {
            Ok(value)
        } else {
            Err(SdlInstance::error())
        }
    }

    fn get_display_scale(&self) -> Result<f32, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowDisplayScale(self.raw.as_ptr()) };
        if value > 0.0 {
            Ok(value)
        } else {
            Err(SdlInstance::error())
        }
    }

    fn set_fullscreen_mode(&mut self, mode: Option<&WindowDisplayMode>) -> Result<(), WindowError> {
        let Some(mode) = mode else {
            return Self::result(unsafe {
                sys::video::SDL_SetWindowFullscreenMode(self.raw.as_ptr(), std::ptr::null())
            });
        };
        let mut count = 0;
        let modes = unsafe {
            sys::video::SDL_GetFullscreenDisplayModes(mode.display_id.into(), &mut count)
        };
        if modes.is_null() {
            return Err(SdlInstance::error());
        }
        let mut matched = std::ptr::null();
        for index in 0..count {
            let candidate = unsafe { *modes.add(index as usize) };
            if !candidate.is_null() && WindowDisplayMode::from(unsafe { &*candidate }) == *mode {
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
        Self::result(unsafe { sys::video::SDL_SetWindowFullscreenMode(self.raw.as_ptr(), matched) })
    }

    fn get_fullscreen_mode(&self) -> Option<WindowDisplayMode> {
        let mode = unsafe { sys::video::SDL_GetWindowFullscreenMode(self.raw.as_ptr()) };
        unsafe { mode.as_ref() }.map(WindowDisplayMode::from)
    }

    fn get_icc_profile(&self) -> Result<Vec<u8>, WindowError> {
        let mut size = 0;
        let profile = unsafe { sys::video::SDL_GetWindowICCProfile(self.raw.as_ptr(), &mut size) };
        if profile.is_null() {
            return Err(SdlInstance::error());
        }
        let bytes = unsafe { std::slice::from_raw_parts(profile.cast::<u8>(), size) }.to_vec();
        unsafe { sys::stdinc::SDL_free(profile) };
        Ok(bytes)
    }

    fn get_pixel_format(&self) -> Result<WindowPixelFormat, WindowError> {
        let format = unsafe { sys::video::SDL_GetWindowPixelFormat(self.raw.as_ptr()) };
        if format == sys::pixels::SDL_PixelFormat::UNKNOWN {
            Err(SdlInstance::error())
        } else {
            Ok(format.into())
        }
    }

    fn get_properties(&self) -> Result<WindowPropertiesId, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowProperties(self.raw.as_ptr()) };
        if value.0 == 0 {
            Err(SdlInstance::error())
        } else {
            Ok(value.into())
        }
    }

    fn get_flags(&self) -> WindowFlags {
        unsafe { sys::video::SDL_GetWindowFlags(self.raw.as_ptr()) }.into()
    }

    fn set_cursor(&mut self, cursor: WindowSystemCursor) -> Result<(), WindowError> {
        let mut cursors = self.owner.context.cursors.borrow_mut();
        let raw = match cursors.get(&cursor) {
            Some(raw) => *raw,
            None => {
                let raw = unsafe { sys::mouse::SDL_CreateSystemCursor(cursor.into()) };
                let raw = NonNull::new(raw).ok_or_else(SdlInstance::error)?;
                cursors.insert(cursor, raw);
                raw
            }
        };
        if unsafe { sys::mouse::SDL_GetCursor() } == raw.as_ptr() {
            return Ok(());
        }
        Self::result(unsafe { sys::mouse::SDL_SetCursor(raw.as_ptr()) })
    }

    fn start_text_input(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::keyboard::SDL_StartTextInput(self.raw.as_ptr()) })
    }

    fn stop_text_input(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::keyboard::SDL_StopTextInput(self.raw.as_ptr()) })
    }

    fn set_title(&mut self, title: &str) -> Result<(), WindowError> {
        let title = CString::new(title).map_err(|error| WindowError::Nul(error.to_string()))?;
        Self::result(unsafe { sys::video::SDL_SetWindowTitle(self.raw.as_ptr(), title.as_ptr()) })
    }

    fn get_title(&self) -> String {
        unsafe { copy_string(sys::video::SDL_GetWindowTitle(self.raw.as_ptr())) }
            .unwrap_or_default()
    }

    fn set_icon(&mut self, icon: &WindowSurfaceRef<'_>) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowIcon(self.raw.as_ptr(), icon.into()) })
    }

    fn set_position(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowPosition(self.raw.as_ptr(), x, y) })
    }

    fn get_position(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowPosition(self.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    fn set_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowSize(self.raw.as_ptr(), width, height) })
    }

    fn get_size(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowSize(self.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    fn get_safe_area(&self) -> Result<WindowRect, WindowError> {
        let mut rect = sys::rect::SDL_Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        Self::result(unsafe { sys::video::SDL_GetWindowSafeArea(self.raw.as_ptr(), &mut rect) })?;
        Ok(rect.into())
    }

    fn set_aspect_ratio(&mut self, min_aspect: f32, max_aspect: f32) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowAspectRatio(self.raw.as_ptr(), min_aspect, max_aspect)
        })
    }

    fn get_aspect_ratio(&self) -> Result<(f32, f32), WindowError> {
        let (mut first, mut second) = (0.0, 0.0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowAspectRatio(self.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    fn get_borders_size(&self) -> Result<(i32, i32, i32, i32), WindowError> {
        let (mut top, mut left, mut bottom, mut right) = (0, 0, 0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowBordersSize(
                self.raw.as_ptr(),
                &mut top,
                &mut left,
                &mut bottom,
                &mut right,
            )
        })?;
        Ok((top, left, bottom, right))
    }

    fn get_size_in_pixels(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowSizeInPixels(self.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    fn set_minimum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowMinimumSize(self.raw.as_ptr(), width, height)
        })
    }

    fn get_minimum_size(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowMinimumSize(self.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    fn set_maximum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowMaximumSize(self.raw.as_ptr(), width, height)
        })
    }

    fn get_maximum_size(&self) -> Result<(i32, i32), WindowError> {
        let (mut first, mut second) = (0, 0);
        Self::result(unsafe {
            sys::video::SDL_GetWindowMaximumSize(self.raw.as_ptr(), &mut first, &mut second)
        })?;
        Ok((first, second))
    }

    fn set_bordered(&mut self, bordered: bool) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowBordered(self.raw.as_ptr(), bordered) })
    }

    fn set_resizable(&mut self, resizable: bool) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowResizable(self.raw.as_ptr(), resizable) })
    }

    fn set_always_on_top(&mut self, on_top: bool) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowAlwaysOnTop(self.raw.as_ptr(), on_top) })
    }

    fn set_fill_document(&mut self, fill: bool) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowFillDocument(self.raw.as_ptr(), fill) })
    }

    fn show(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_ShowWindow(self.raw.as_ptr()) })
    }

    fn hide(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_HideWindow(self.raw.as_ptr()) })
    }

    fn raise(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_RaiseWindow(self.raw.as_ptr()) })
    }

    fn maximize(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_MaximizeWindow(self.raw.as_ptr()) })
    }

    fn minimize(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_MinimizeWindow(self.raw.as_ptr()) })
    }

    fn restore(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_RestoreWindow(self.raw.as_ptr()) })
    }

    fn set_fullscreen(&mut self, fullscreen: bool) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowFullscreen(self.raw.as_ptr(), fullscreen) })
    }

    fn sync(&self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SyncWindow(self.raw.as_ptr()) })
    }

    fn has_surface(&self) -> bool {
        unsafe { sys::video::SDL_WindowHasSurface(self.raw.as_ptr()) }
    }

    fn get_surface(&mut self) -> Result<WindowSurfaceRef<'_>, WindowError> {
        let surface = unsafe { sys::video::SDL_GetWindowSurface(self.raw.as_ptr()) };
        Ok(WindowSurfaceRef {
            raw: NonNull::new(surface).ok_or_else(SdlInstance::error)?.cast(),
            marker: PhantomData,
        })
    }

    fn set_surface_vsync(&mut self, vsync: i32) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowSurfaceVSync(self.raw.as_ptr(), vsync) })
    }

    fn get_surface_vsync(&self) -> Result<i32, WindowError> {
        let mut vsync = 0;
        Self::result(unsafe {
            sys::video::SDL_GetWindowSurfaceVSync(self.raw.as_ptr(), &mut vsync)
        })?;
        Ok(vsync)
    }

    fn update_surface(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_UpdateWindowSurface(self.raw.as_ptr()) })
    }

    fn update_surface_rects(&mut self, rects: &[WindowRect]) -> Result<(), WindowError> {
        let count =
            i32::try_from(rects.len()).map_err(|error| WindowError::Nul(error.to_string()))?;
        let rects: Vec<sys::rect::SDL_Rect> = rects.iter().copied().map(Into::into).collect();
        Self::result(unsafe {
            sys::video::SDL_UpdateWindowSurfaceRects(self.raw.as_ptr(), rects.as_ptr(), count)
        })
    }

    fn destroy_surface(&mut self) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_DestroyWindowSurface(self.raw.as_ptr()) })
    }

    fn set_keyboard_grab(&mut self, grabbed: bool) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowKeyboardGrab(self.raw.as_ptr(), grabbed) })
    }

    fn set_mouse_grab(&mut self, grabbed: bool) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowMouseGrab(self.raw.as_ptr(), grabbed) })
    }

    fn get_keyboard_grab(&self) -> bool {
        unsafe { sys::video::SDL_GetWindowKeyboardGrab(self.raw.as_ptr()) }
    }

    fn get_mouse_grab(&self) -> bool {
        unsafe { sys::video::SDL_GetWindowMouseGrab(self.raw.as_ptr()) }
    }

    fn set_mouse_rect(&mut self, rect: Option<&WindowRect>) -> Result<(), WindowError> {
        let rect = rect.copied().map(sys::rect::SDL_Rect::from);
        Self::result(unsafe {
            sys::video::SDL_SetWindowMouseRect(
                self.raw.as_ptr(),
                rect.as_ref().map_or(std::ptr::null(), |rect| rect),
            )
        })
    }

    fn get_mouse_rect(&self) -> Option<WindowRect> {
        let rect = unsafe { sys::video::SDL_GetWindowMouseRect(self.raw.as_ptr()) };
        unsafe { rect.as_ref() }.copied().map(Into::into)
    }

    fn set_opacity(&mut self, opacity: f32) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowOpacity(self.raw.as_ptr(), opacity) })
    }

    fn get_opacity(&self) -> Result<f32, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowOpacity(self.raw.as_ptr()) };
        if value < 0.0 {
            Err(SdlInstance::error())
        } else {
            Ok(value)
        }
    }

    fn set_focusable(&mut self, focusable: bool) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowFocusable(self.raw.as_ptr(), focusable) })
    }

    fn show_system_menu(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_ShowWindowSystemMenu(self.raw.as_ptr(), x, y) })
    }

    fn set_hit_test(
        &mut self,
        hit_test: impl Fn(i32, i32) -> WindowHitTestResult + 'static,
    ) -> Result<(), WindowError>
    where
        Self: Sized,
    {
        let previous = self.hit_test.replace(Box::new(hit_test));
        let data = (&mut *self.hit_test as *mut Option<HitTestCallback>).cast();
        let result = Self::result(unsafe {
            sys::video::SDL_SetWindowHitTest(self.raw.as_ptr(), Some(Self::hit_test_callback), data)
        });
        drop(previous);
        result
    }

    fn set_shape(&mut self, shape: Option<&WindowSurfaceRef<'_>>) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowShape(
                self.raw.as_ptr(),
                shape.map_or(std::ptr::null_mut(), Into::into),
            )
        })
    }

    fn flash(&mut self, operation: WindowFlashOperation) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_FlashWindow(self.raw.as_ptr(), operation.into()) })
    }

    fn set_progress_state(&mut self, state: WindowProgressState) -> Result<(), WindowError> {
        Self::result(unsafe {
            sys::video::SDL_SetWindowProgressState(self.raw.as_ptr(), state.into())
        })
    }

    fn get_progress_state(&self) -> Result<WindowProgressState, WindowError> {
        let state = unsafe { sys::video::SDL_GetWindowProgressState(self.raw.as_ptr()) };
        if state == sys::video::SDL_ProgressState::INVALID {
            Err(SdlInstance::error())
        } else {
            Ok(state.into())
        }
    }

    fn set_progress_value(&mut self, value: f32) -> Result<(), WindowError> {
        Self::result(unsafe { sys::video::SDL_SetWindowProgressValue(self.raw.as_ptr(), value) })
    }

    fn get_progress_value(&self) -> Result<f32, WindowError> {
        let value = unsafe { sys::video::SDL_GetWindowProgressValue(self.raw.as_ptr()) };
        if value < 0.0 {
            Err(SdlInstance::error())
        } else {
            Ok(value)
        }
    }

    fn display_handle(&self) -> Result<WindowDisplayHandle<'_>, WindowError> {
        let raw = self.owner.raw_display_handle()?;
        Ok(unsafe { WindowDisplayHandle::borrow_raw(raw) })
    }

    fn window_handle(&self) -> Result<WindowHandle<'_>, WindowError> {
        let raw = self.owner.raw_window_handle()?;
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
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
                        self.get_properties()?.into(),
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
                        self.get_properties()?.into(),
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
                        self.get_properties()?.into(),
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
                self.get_properties()?.into(),
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
            Ok(value.into())
        }
    }
}

impl SdlWindow {
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
        let callback = unsafe { &*data.cast::<Option<HitTestCallback>>() };
        let point = unsafe { *point };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback
                .as_ref()
                .map_or(WindowHitTestResult::Normal, |callback| {
                    callback(point.x, point.y)
                })
        }))
        .unwrap_or(WindowHitTestResult::Normal)
        .into()
    }
}

impl Drop for SdlWindow {
    fn drop(&mut self) {
        if self.hit_test.is_some() {
            unsafe {
                sys::video::SDL_SetWindowHitTest(self.raw.as_ptr(), None, std::ptr::null_mut())
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
        for (_, cursor) in self.cursors.get_mut().drain() {
            unsafe { sys::mouse::SDL_DestroyCursor(cursor.as_ptr()) };
        }
        let mut contexts = SDL_CONTEXTS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe { sys::init::SDL_QuitSubSystem(sys::init::SDL_INIT_VIDEO) };
        if let Some((_, count)) = contexts.as_mut() {
            *count -= 1;
            if *count == 0 {
                unsafe { sys::init::SDL_Quit() };
                *contexts = None;
            }
        }
    }
}

impl From<sys::video::SDL_WindowID> for WindowId {
    fn from(value: sys::video::SDL_WindowID) -> Self {
        Self(value.0)
    }
}

impl From<WindowId> for sys::video::SDL_WindowID {
    fn from(value: WindowId) -> Self {
        Self(value.0)
    }
}

impl From<sys::video::SDL_DisplayID> for WindowDisplayId {
    fn from(value: sys::video::SDL_DisplayID) -> Self {
        Self(value.0)
    }
}

impl From<WindowDisplayId> for sys::video::SDL_DisplayID {
    fn from(value: WindowDisplayId) -> Self {
        Self(value.0)
    }
}

impl From<sys::properties::SDL_PropertiesID> for WindowPropertiesId {
    fn from(value: sys::properties::SDL_PropertiesID) -> Self {
        Self(value.0)
    }
}

impl From<WindowPropertiesId> for sys::properties::SDL_PropertiesID {
    fn from(value: WindowPropertiesId) -> Self {
        Self(value.0)
    }
}

impl From<sys::pixels::SDL_PixelFormat> for WindowPixelFormat {
    fn from(value: sys::pixels::SDL_PixelFormat) -> Self {
        Self(value.0)
    }
}

impl From<WindowPixelFormat> for sys::pixels::SDL_PixelFormat {
    fn from(value: WindowPixelFormat) -> Self {
        Self(value.0)
    }
}

impl From<u32> for WindowId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<&sys::video::SDL_DisplayMode> for WindowDisplayMode {
    fn from(value: &sys::video::SDL_DisplayMode) -> Self {
        Self {
            display_id: value.displayID.into(),
            format: value.format.into(),
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

impl From<sys::video::SDL_DisplayMode> for WindowDisplayMode {
    fn from(value: sys::video::SDL_DisplayMode) -> Self {
        Self::from(&value)
    }
}

impl<'a> From<&'a mut sys::surface::SDL_Surface> for WindowSurfaceRef<'a> {
    fn from(value: &'a mut sys::surface::SDL_Surface) -> Self {
        Self {
            raw: NonNull::from(value).cast(),
            marker: PhantomData,
        }
    }
}

impl From<&WindowSurfaceRef<'_>> for *mut sys::surface::SDL_Surface {
    fn from(value: &WindowSurfaceRef<'_>) -> Self {
        value.raw.cast().as_ptr()
    }
}

impl From<sys::rect::SDL_Rect> for WindowRect {
    fn from(value: sys::rect::SDL_Rect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.w,
            height: value.h,
        }
    }
}

impl From<WindowRect> for sys::rect::SDL_Rect {
    fn from(value: WindowRect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            w: value.width,
            h: value.height,
        }
    }
}

impl From<sys::video::SDL_WindowFlags> for WindowFlags {
    fn from(value: sys::video::SDL_WindowFlags) -> Self {
        Self::from_bits_retain(value.0)
    }
}

impl From<WindowFlags> for sys::video::SDL_WindowFlags {
    fn from(value: WindowFlags) -> Self {
        Self(value.bits())
    }
}

impl From<sys::video::SDL_FlashOperation> for WindowFlashOperation {
    fn from(value: sys::video::SDL_FlashOperation) -> Self {
        match value {
            sys::video::SDL_FlashOperation::CANCEL => Self::Cancel,
            sys::video::SDL_FlashOperation::BRIEFLY => Self::Briefly,
            sys::video::SDL_FlashOperation::UNTIL_FOCUSED => Self::UntilFocused,
            value => Self::Unknown(value.0),
        }
    }
}

impl From<WindowFlashOperation> for sys::video::SDL_FlashOperation {
    fn from(value: WindowFlashOperation) -> Self {
        match value {
            WindowFlashOperation::Cancel => Self::CANCEL,
            WindowFlashOperation::Briefly => Self::BRIEFLY,
            WindowFlashOperation::UntilFocused => Self::UNTIL_FOCUSED,
            WindowFlashOperation::Unknown(value) => Self(value),
        }
    }
}

impl From<sys::video::SDL_HitTestResult> for WindowHitTestResult {
    fn from(value: sys::video::SDL_HitTestResult) -> Self {
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

impl From<WindowHitTestResult> for sys::video::SDL_HitTestResult {
    fn from(value: WindowHitTestResult) -> Self {
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

impl From<sys::video::SDL_ProgressState> for WindowProgressState {
    fn from(value: sys::video::SDL_ProgressState) -> Self {
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

impl From<WindowProgressState> for sys::video::SDL_ProgressState {
    fn from(value: WindowProgressState) -> Self {
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

impl From<sys::events::SDL_EventType> for WindowEventCode {
    fn from(value: sys::events::SDL_EventType) -> Self {
        Self(value.0)
    }
}

impl From<sys::keyboard::SDL_KeyboardID> for WindowKeyboardId {
    fn from(value: sys::keyboard::SDL_KeyboardID) -> Self {
        Self(value.0)
    }
}

impl From<sys::mouse::SDL_MouseID> for WindowMouseId {
    fn from(value: sys::mouse::SDL_MouseID) -> Self {
        Self(value.0)
    }
}

impl From<sys::joystick::SDL_JoystickID> for WindowJoystickId {
    fn from(value: sys::joystick::SDL_JoystickID) -> Self {
        Self(value.0)
    }
}

impl From<sys::audio::SDL_AudioDeviceID> for WindowAudioDeviceId {
    fn from(value: sys::audio::SDL_AudioDeviceID) -> Self {
        Self(value.0)
    }
}

impl From<sys::camera::SDL_CameraID> for WindowCameraId {
    fn from(value: sys::camera::SDL_CameraID) -> Self {
        Self(value.0)
    }
}

impl From<sys::sensor::SDL_SensorID> for WindowSensorId {
    fn from(value: sys::sensor::SDL_SensorID) -> Self {
        Self(value.0)
    }
}

impl From<sys::touch::SDL_TouchID> for WindowTouchId {
    fn from(value: sys::touch::SDL_TouchID) -> Self {
        Self(value.0)
    }
}

impl From<sys::touch::SDL_FingerID> for WindowFingerId {
    fn from(value: sys::touch::SDL_FingerID) -> Self {
        Self(value.0)
    }
}

impl From<sys::pen::SDL_PenID> for WindowPenId {
    fn from(value: sys::pen::SDL_PenID) -> Self {
        Self(value.0)
    }
}

impl From<sys::scancode::SDL_Scancode> for WindowScancode {
    fn from(value: sys::scancode::SDL_Scancode) -> Self {
        Self(value.0)
    }
}

impl From<sys::keycode::SDL_Keycode> for WindowKeycode {
    fn from(value: sys::keycode::SDL_Keycode) -> Self {
        Self(value.0)
    }
}

impl From<sys::keycode::SDL_Keymod> for WindowKeymod {
    fn from(value: sys::keycode::SDL_Keymod) -> Self {
        Self(value.0)
    }
}

impl From<sys::mouse::SDL_MouseButtonFlags> for WindowMouseButtonFlags {
    fn from(value: sys::mouse::SDL_MouseButtonFlags) -> Self {
        Self(value.0)
    }
}

impl From<sys::mouse::SDL_MouseWheelDirection> for WindowMouseWheelDirection {
    fn from(value: sys::mouse::SDL_MouseWheelDirection) -> Self {
        Self(value.0)
    }
}

impl From<sys::power::SDL_PowerState> for WindowPowerState {
    fn from(value: sys::power::SDL_PowerState) -> Self {
        Self(value.0)
    }
}

impl From<sys::pen::SDL_PenInputFlags> for WindowPenInputFlags {
    fn from(value: sys::pen::SDL_PenInputFlags) -> Self {
        Self(value.0)
    }
}

impl From<sys::pen::SDL_PenDeviceType> for WindowPenDeviceType {
    fn from(value: sys::pen::SDL_PenDeviceType) -> Self {
        Self(value.0)
    }
}

impl From<sys::pen::SDL_PenAxis> for WindowPenAxis {
    fn from(value: sys::pen::SDL_PenAxis) -> Self {
        Self(value.0)
    }
}

impl From<sys::events::SDL_CommonEvent> for WindowCommonEvent {
    fn from(value: sys::events::SDL_CommonEvent) -> Self {
        Self {
            event_type: value.r#type,
            reserved: value.reserved,
            timestamp: value.timestamp,
        }
    }
}

impl From<sys::events::SDL_DisplayEvent> for WindowDisplayEvent {
    fn from(value: sys::events::SDL_DisplayEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            display_id: value.displayID.into(),
            data1: value.data1,
            data2: value.data2,
        }
    }
}

impl From<sys::events::SDL_KeyboardDeviceEvent> for WindowKeyboardDeviceEvent {
    fn from(value: sys::events::SDL_KeyboardDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_KeyboardEvent> for WindowKeyboardEvent {
    fn from(value: sys::events::SDL_KeyboardEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            scancode: value.scancode.into(),
            key: value.key.into(),
            modifiers: value.r#mod.into(),
            raw: value.raw,
            down: value.down,
            repeat: value.repeat,
        }
    }
}

impl WindowTextEditingEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_TextEditingEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            text: unsafe { copy_string(value.text) }.unwrap_or_default(),
            start: value.start,
            length: value.length,
        }
    }
}

impl WindowTextEditingCandidatesEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_TextEditingCandidatesEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            candidates: unsafe { copy_strings(value.candidates, value.num_candidates) },
            num_candidates: value.num_candidates,
            selected_candidate: value.selected_candidate,
            horizontal: value.horizontal,
        }
    }
}

impl WindowTextInputEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_TextInputEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            text: unsafe { copy_string(value.text) }.unwrap_or_default(),
        }
    }
}

impl From<sys::events::SDL_MouseDeviceEvent> for WindowMouseDeviceEvent {
    fn from(value: sys::events::SDL_MouseDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_MouseMotionEvent> for WindowMouseMotionEvent {
    fn from(value: sys::events::SDL_MouseMotionEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            state: value.state.into(),
            x: value.x,
            y: value.y,
            xrel: value.xrel,
            yrel: value.yrel,
        }
    }
}

impl From<sys::events::SDL_MouseButtonEvent> for WindowMouseButtonEvent {
    fn from(value: sys::events::SDL_MouseButtonEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            button: value.button,
            down: value.down,
            clicks: value.clicks,
            x: value.x,
            y: value.y,
        }
    }
}

impl From<sys::events::SDL_MouseWheelEvent> for WindowMouseWheelEvent {
    fn from(value: sys::events::SDL_MouseWheelEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            x: value.x,
            y: value.y,
            direction: value.direction.into(),
            mouse_x: value.mouse_x,
            mouse_y: value.mouse_y,
            integer_x: value.integer_x,
            integer_y: value.integer_y,
        }
    }
}

impl From<sys::events::SDL_JoyAxisEvent> for WindowJoyAxisEvent {
    fn from(value: sys::events::SDL_JoyAxisEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            axis: value.axis,
            value: value.value,
        }
    }
}

impl From<sys::events::SDL_JoyBallEvent> for WindowJoyBallEvent {
    fn from(value: sys::events::SDL_JoyBallEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            ball: value.ball,
            xrel: value.xrel,
            yrel: value.yrel,
        }
    }
}

impl From<sys::events::SDL_JoyHatEvent> for WindowJoyHatEvent {
    fn from(value: sys::events::SDL_JoyHatEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            hat: value.hat,
            value: value.value,
        }
    }
}

impl From<sys::events::SDL_JoyButtonEvent> for WindowJoyButtonEvent {
    fn from(value: sys::events::SDL_JoyButtonEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            button: value.button,
            down: value.down,
        }
    }
}

impl From<sys::events::SDL_JoyDeviceEvent> for WindowJoyDeviceEvent {
    fn from(value: sys::events::SDL_JoyDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_JoyBatteryEvent> for WindowJoyBatteryEvent {
    fn from(value: sys::events::SDL_JoyBatteryEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            state: value.state.into(),
            percent: value.percent,
        }
    }
}

impl From<sys::events::SDL_GamepadAxisEvent> for WindowGamepadAxisEvent {
    fn from(value: sys::events::SDL_GamepadAxisEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            axis: value.axis,
            value: value.value,
        }
    }
}

impl From<sys::events::SDL_GamepadButtonEvent> for WindowGamepadButtonEvent {
    fn from(value: sys::events::SDL_GamepadButtonEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            button: value.button,
            down: value.down,
        }
    }
}

impl From<sys::events::SDL_GamepadDeviceEvent> for WindowGamepadDeviceEvent {
    fn from(value: sys::events::SDL_GamepadDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_GamepadTouchpadEvent> for WindowGamepadTouchpadEvent {
    fn from(value: sys::events::SDL_GamepadTouchpadEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            touchpad: value.touchpad,
            finger: value.finger,
            x: value.x,
            y: value.y,
            pressure: value.pressure,
        }
    }
}

impl From<sys::events::SDL_GamepadSensorEvent> for WindowGamepadSensorEvent {
    fn from(value: sys::events::SDL_GamepadSensorEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            sensor: value.sensor,
            data: value.data,
            sensor_timestamp: value.sensor_timestamp,
        }
    }
}

impl From<sys::events::SDL_AudioDeviceEvent> for WindowAudioDeviceEvent {
    fn from(value: sys::events::SDL_AudioDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            recording: value.recording,
        }
    }
}

impl From<sys::events::SDL_CameraDeviceEvent> for WindowCameraDeviceEvent {
    fn from(value: sys::events::SDL_CameraDeviceEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
        }
    }
}

impl From<sys::events::SDL_RenderEvent> for WindowRenderEvent {
    fn from(value: sys::events::SDL_RenderEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
        }
    }
}

impl From<sys::events::SDL_TouchFingerEvent> for WindowTouchFingerEvent {
    fn from(value: sys::events::SDL_TouchFingerEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            touch_id: value.touchID.into(),
            finger_id: value.fingerID.into(),
            x: value.x,
            y: value.y,
            dx: value.dx,
            dy: value.dy,
            pressure: value.pressure,
            window_id: value.windowID.into(),
        }
    }
}

impl From<sys::events::SDL_PinchFingerEvent> for WindowPinchFingerEvent {
    fn from(value: sys::events::SDL_PinchFingerEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            scale: value.scale,
            window_id: value.windowID.into(),
        }
    }
}

impl From<sys::events::SDL_PenProximityEvent> for WindowPenProximityEvent {
    fn from(value: sys::events::SDL_PenProximityEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            device_type: value.device_type.into(),
        }
    }
}

impl From<sys::events::SDL_PenMotionEvent> for WindowPenMotionEvent {
    fn from(value: sys::events::SDL_PenMotionEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            x: value.x,
            y: value.y,
            device_type: value.device_type.into(),
        }
    }
}

impl From<sys::events::SDL_PenTouchEvent> for WindowPenTouchEvent {
    fn from(value: sys::events::SDL_PenTouchEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            x: value.x,
            y: value.y,
            eraser: value.eraser,
            down: value.down,
            device_type: value.device_type.into(),
        }
    }
}

impl From<sys::events::SDL_PenButtonEvent> for WindowPenButtonEvent {
    fn from(value: sys::events::SDL_PenButtonEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            x: value.x,
            y: value.y,
            button: value.button,
            down: value.down,
            device_type: value.device_type.into(),
        }
    }
}

impl From<sys::events::SDL_PenAxisEvent> for WindowPenAxisEvent {
    fn from(value: sys::events::SDL_PenAxisEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            which: value.which.into(),
            pen_state: value.pen_state.into(),
            x: value.x,
            y: value.y,
            axis: value.axis.into(),
            value: value.value,
            device_type: value.device_type.into(),
        }
    }
}

impl WindowDropEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_DropEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            x: value.x,
            y: value.y,
            source: unsafe { copy_string(value.source) },
            data: unsafe { copy_string(value.data) },
        }
    }
}

impl WindowClipboardEvent {
    /// # Safety
    /// All non-null string pointers and counted arrays must be valid for reading.
    /// Each string must be NUL-terminated and remain valid during this call.
    pub unsafe fn from_sdl(value: &sys::events::SDL_ClipboardEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            owner: value.owner,
            num_mime_types: value.num_mime_types,
            mime_types: unsafe { copy_strings(value.mime_types, value.num_mime_types) },
        }
    }
}

impl From<sys::events::SDL_SensorEvent> for WindowSensorEvent {
    fn from(value: sys::events::SDL_SensorEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
            which: value.which.into(),
            data: value.data,
            sensor_timestamp: value.sensor_timestamp,
        }
    }
}

impl From<sys::events::SDL_QuitEvent> for WindowQuitEvent {
    fn from(value: sys::events::SDL_QuitEvent) -> Self {
        Self {
            event_type: value.r#type.into(),
            reserved: value.reserved,
            timestamp: value.timestamp,
        }
    }
}

impl From<sys::events::SDL_UserEvent> for WindowUserEvent {
    fn from(value: sys::events::SDL_UserEvent) -> Self {
        Self {
            event_type: value.r#type,
            reserved: value.reserved,
            timestamp: value.timestamp,
            window_id: value.windowID.into(),
            code: value.code,
            data1: value.data1,
            data2: value.data2,
        }
    }
}

impl WindowEvent {
    /// # Safety
    /// The tag must identify an initialized SDL event payload.
    /// Referenced strings and counted arrays must be valid for reading during this call.
    /// User data pointers are copied without being read or taking ownership.
    pub unsafe fn from_sdl(value: &sys::events::SDL_Event) -> Self {
        let event_type = sys::events::SDL_EventType(unsafe { value.r#type });
        match event_type {
            sys::events::SDL_EVENT_QUIT => Self::Quit(unsafe { value.quit }.into()),
            sys::events::SDL_EVENT_TERMINATING => Self::Terminating(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_LOW_MEMORY => Self::LowMemory(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_WILL_ENTER_BACKGROUND => {
                Self::WillEnterBackground(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_DID_ENTER_BACKGROUND => {
                Self::DidEnterBackground(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_WILL_ENTER_FOREGROUND => {
                Self::WillEnterForeground(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_DID_ENTER_FOREGROUND => {
                Self::DidEnterForeground(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_LOCALE_CHANGED => {
                Self::LocaleChanged(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_SYSTEM_THEME_CHANGED => {
                Self::SystemThemeChanged(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_ORIENTATION => {
                Self::DisplayOrientation(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_ADDED => {
                Self::DisplayAdded(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_REMOVED => {
                Self::DisplayRemoved(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_MOVED => {
                Self::DisplayMoved(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_DESKTOP_MODE_CHANGED => {
                Self::DisplayDesktopModeChanged(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_CURRENT_MODE_CHANGED => {
                Self::DisplayCurrentModeChanged(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_CONTENT_SCALE_CHANGED => {
                Self::DisplayContentScaleChanged(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_DISPLAY_USABLE_BOUNDS_CHANGED => {
                Self::DisplayUsableBoundsChanged(unsafe { value.display }.into())
            }
            sys::events::SDL_EVENT_KEY_DOWN => Self::KeyDown(unsafe { value.key }.into()),
            sys::events::SDL_EVENT_KEY_UP => Self::KeyUp(unsafe { value.key }.into()),
            sys::events::SDL_EVENT_TEXT_EDITING => {
                Self::TextEditing(unsafe { WindowTextEditingEvent::from_sdl(&value.edit) })
            }
            sys::events::SDL_EVENT_TEXT_INPUT => {
                Self::TextInput(unsafe { WindowTextInputEvent::from_sdl(&value.text) })
            }
            sys::events::SDL_EVENT_KEYMAP_CHANGED => {
                Self::KeymapChanged(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_KEYBOARD_ADDED => {
                Self::KeyboardAdded(unsafe { value.kdevice }.into())
            }
            sys::events::SDL_EVENT_KEYBOARD_REMOVED => {
                Self::KeyboardRemoved(unsafe { value.kdevice }.into())
            }
            sys::events::SDL_EVENT_TEXT_EDITING_CANDIDATES => Self::TextEditingCandidates(unsafe {
                WindowTextEditingCandidatesEvent::from_sdl(&value.edit_candidates)
            }),
            sys::events::SDL_EVENT_SCREEN_KEYBOARD_SHOWN => {
                Self::ScreenKeyboardShown(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_SCREEN_KEYBOARD_HIDDEN => {
                Self::ScreenKeyboardHidden(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_MOUSE_MOTION => {
                Self::MouseMotion(unsafe { value.motion }.into())
            }
            sys::events::SDL_EVENT_MOUSE_BUTTON_DOWN => {
                Self::MouseButtonDown(unsafe { value.button }.into())
            }
            sys::events::SDL_EVENT_MOUSE_BUTTON_UP => {
                Self::MouseButtonUp(unsafe { value.button }.into())
            }
            sys::events::SDL_EVENT_MOUSE_WHEEL => Self::MouseWheel(unsafe { value.wheel }.into()),
            sys::events::SDL_EVENT_MOUSE_ADDED => Self::MouseAdded(unsafe { value.mdevice }.into()),
            sys::events::SDL_EVENT_MOUSE_REMOVED => {
                Self::MouseRemoved(unsafe { value.mdevice }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_AXIS_MOTION => {
                Self::JoystickAxisMotion(unsafe { value.jaxis }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_BALL_MOTION => {
                Self::JoystickBallMotion(unsafe { value.jball }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_HAT_MOTION => {
                Self::JoystickHatMotion(unsafe { value.jhat }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_BUTTON_DOWN => {
                Self::JoystickButtonDown(unsafe { value.jbutton }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_BUTTON_UP => {
                Self::JoystickButtonUp(unsafe { value.jbutton }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_ADDED => {
                Self::JoystickAdded(unsafe { value.jdevice }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_REMOVED => {
                Self::JoystickRemoved(unsafe { value.jdevice }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_BATTERY_UPDATED => {
                Self::JoystickBatteryUpdated(unsafe { value.jbattery }.into())
            }
            sys::events::SDL_EVENT_JOYSTICK_UPDATE_COMPLETE => {
                Self::JoystickUpdateComplete(unsafe { value.jdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_AXIS_MOTION => {
                Self::GamepadAxisMotion(unsafe { value.gaxis }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_BUTTON_DOWN => {
                Self::GamepadButtonDown(unsafe { value.gbutton }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_BUTTON_UP => {
                Self::GamepadButtonUp(unsafe { value.gbutton }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_ADDED => {
                Self::GamepadAdded(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_REMOVED => {
                Self::GamepadRemoved(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_REMAPPED => {
                Self::GamepadRemapped(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN => {
                Self::GamepadTouchpadDown(unsafe { value.gtouchpad }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION => {
                Self::GamepadTouchpadMotion(unsafe { value.gtouchpad }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_TOUCHPAD_UP => {
                Self::GamepadTouchpadUp(unsafe { value.gtouchpad }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_SENSOR_UPDATE => {
                Self::GamepadSensorUpdate(unsafe { value.gsensor }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_UPDATE_COMPLETE => {
                Self::GamepadUpdateComplete(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_GAMEPAD_STEAM_HANDLE_UPDATED => {
                Self::GamepadSteamHandleUpdated(unsafe { value.gdevice }.into())
            }
            sys::events::SDL_EVENT_FINGER_DOWN => Self::FingerDown(unsafe { value.tfinger }.into()),
            sys::events::SDL_EVENT_FINGER_UP => Self::FingerUp(unsafe { value.tfinger }.into()),
            sys::events::SDL_EVENT_FINGER_MOTION => {
                Self::FingerMotion(unsafe { value.tfinger }.into())
            }
            sys::events::SDL_EVENT_FINGER_CANCELED => {
                Self::FingerCanceled(unsafe { value.tfinger }.into())
            }
            sys::events::SDL_EVENT_PINCH_BEGIN => Self::PinchBegin(unsafe { value.pinch }.into()),
            sys::events::SDL_EVENT_PINCH_UPDATE => Self::PinchUpdate(unsafe { value.pinch }.into()),
            sys::events::SDL_EVENT_PINCH_END => Self::PinchEnd(unsafe { value.pinch }.into()),
            sys::events::SDL_EVENT_CLIPBOARD_UPDATE => {
                Self::ClipboardUpdate(unsafe { WindowClipboardEvent::from_sdl(&value.clipboard) })
            }
            sys::events::SDL_EVENT_DROP_FILE => {
                Self::DropFile(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_DROP_TEXT => {
                Self::DropText(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_DROP_BEGIN => {
                Self::DropBegin(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_DROP_COMPLETE => {
                Self::DropComplete(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_DROP_POSITION => {
                Self::DropPosition(unsafe { WindowDropEvent::from_sdl(&value.drop) })
            }
            sys::events::SDL_EVENT_AUDIO_DEVICE_ADDED => {
                Self::AudioDeviceAdded(unsafe { value.adevice }.into())
            }
            sys::events::SDL_EVENT_AUDIO_DEVICE_REMOVED => {
                Self::AudioDeviceRemoved(unsafe { value.adevice }.into())
            }
            sys::events::SDL_EVENT_AUDIO_DEVICE_FORMAT_CHANGED => {
                Self::AudioDeviceFormatChanged(unsafe { value.adevice }.into())
            }
            sys::events::SDL_EVENT_SENSOR_UPDATE => {
                Self::SensorUpdate(unsafe { value.sensor }.into())
            }
            sys::events::SDL_EVENT_PEN_PROXIMITY_IN => {
                Self::PenProximityIn(unsafe { value.pproximity }.into())
            }
            sys::events::SDL_EVENT_PEN_PROXIMITY_OUT => {
                Self::PenProximityOut(unsafe { value.pproximity }.into())
            }
            sys::events::SDL_EVENT_PEN_DOWN => Self::PenDown(unsafe { value.ptouch }.into()),
            sys::events::SDL_EVENT_PEN_UP => Self::PenUp(unsafe { value.ptouch }.into()),
            sys::events::SDL_EVENT_PEN_BUTTON_DOWN => {
                Self::PenButtonDown(unsafe { value.pbutton }.into())
            }
            sys::events::SDL_EVENT_PEN_BUTTON_UP => {
                Self::PenButtonUp(unsafe { value.pbutton }.into())
            }
            sys::events::SDL_EVENT_PEN_MOTION => Self::PenMotion(unsafe { value.pmotion }.into()),
            sys::events::SDL_EVENT_PEN_AXIS => Self::PenAxis(unsafe { value.paxis }.into()),
            sys::events::SDL_EVENT_CAMERA_DEVICE_ADDED => {
                Self::CameraDeviceAdded(unsafe { value.cdevice }.into())
            }
            sys::events::SDL_EVENT_CAMERA_DEVICE_REMOVED => {
                Self::CameraDeviceRemoved(unsafe { value.cdevice }.into())
            }
            sys::events::SDL_EVENT_CAMERA_DEVICE_APPROVED => {
                Self::CameraDeviceApproved(unsafe { value.cdevice }.into())
            }
            sys::events::SDL_EVENT_CAMERA_DEVICE_DENIED => {
                Self::CameraDeviceDenied(unsafe { value.cdevice }.into())
            }
            sys::events::SDL_EVENT_RENDER_TARGETS_RESET => {
                Self::RenderTargetsReset(unsafe { value.render }.into())
            }
            sys::events::SDL_EVENT_RENDER_DEVICE_RESET => {
                Self::RenderDeviceReset(unsafe { value.render }.into())
            }
            sys::events::SDL_EVENT_RENDER_DEVICE_LOST => {
                Self::RenderDeviceLost(unsafe { value.render }.into())
            }
            sys::events::SDL_EVENT_PRIVATE0 => Self::Private0(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_PRIVATE1 => Self::Private1(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_PRIVATE2 => Self::Private2(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_PRIVATE3 => Self::Private3(unsafe { value.common }.into()),
            sys::events::SDL_EVENT_POLL_SENTINEL => {
                Self::PollSentinel(unsafe { value.common }.into())
            }
            sys::events::SDL_EVENT_WINDOW_SHOWN => {
                let window = unsafe { value.window };
                Self::WindowShown {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_HIDDEN => {
                let window = unsafe { value.window };
                Self::WindowHidden {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_EXPOSED => {
                let window = unsafe { value.window };
                Self::WindowExposed {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    live_resize: window.data1 != 0,
                }
            }
            sys::events::SDL_EVENT_WINDOW_MOVED => {
                let window = unsafe { value.window };
                Self::WindowMoved {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    x: window.data1,
                    y: window.data2,
                }
            }
            sys::events::SDL_EVENT_WINDOW_RESIZED => {
                let window = unsafe { value.window };
                Self::WindowResized {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    width: window.data1,
                    height: window.data2,
                }
            }
            sys::events::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowPixelSizeChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    width: window.data1,
                    height: window.data2,
                }
            }
            sys::events::SDL_EVENT_WINDOW_METAL_VIEW_RESIZED => {
                let window = unsafe { value.window };
                Self::WindowMetalViewResized {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_MINIMIZED => {
                let window = unsafe { value.window };
                Self::WindowMinimized {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_MAXIMIZED => {
                let window = unsafe { value.window };
                Self::WindowMaximized {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_RESTORED => {
                let window = unsafe { value.window };
                Self::WindowRestored {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_MOUSE_ENTER => {
                let window = unsafe { value.window };
                Self::WindowMouseEnter {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_MOUSE_LEAVE => {
                let window = unsafe { value.window };
                Self::WindowMouseLeave {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_FOCUS_GAINED => {
                let window = unsafe { value.window };
                Self::WindowFocusGained {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_FOCUS_LOST => {
                let window = unsafe { value.window };
                Self::WindowFocusLost {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_CLOSE_REQUESTED => {
                let window = unsafe { value.window };
                Self::WindowCloseRequested {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_HIT_TEST => {
                let window = unsafe { value.window };
                Self::WindowHitTest {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    data1: window.data1,
                    data2: window.data2,
                }
            }
            sys::events::SDL_EVENT_WINDOW_ICCPROF_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowICCProfChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_DISPLAY_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowDisplayChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                    display_id: WindowDisplayId(window.data1 as u32),
                }
            }
            sys::events::SDL_EVENT_WINDOW_DISPLAY_SCALE_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowDisplayScaleChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_SAFE_AREA_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowSafeAreaChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_OCCLUDED => {
                let window = unsafe { value.window };
                Self::WindowOccluded {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_ENTER_FULLSCREEN => {
                let window = unsafe { value.window };
                Self::WindowEnterFullscreen {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_LEAVE_FULLSCREEN => {
                let window = unsafe { value.window };
                Self::WindowLeaveFullscreen {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_DESTROYED => {
                let window = unsafe { value.window };
                Self::WindowDestroyed {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            sys::events::SDL_EVENT_WINDOW_HDR_STATE_CHANGED => {
                let window = unsafe { value.window };
                Self::WindowHdrStateChanged {
                    timestamp: window.timestamp,
                    window_id: window.windowID.into(),
                }
            }
            event_type
                if (sys::events::SDL_EVENT_USER.0..=sys::events::SDL_EVENT_LAST.0)
                    .contains(&event_type.0) =>
            {
                Self::User(unsafe { value.user }.into())
            }
            _ => Self::Unknown(unsafe { value.common }.into()),
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

unsafe fn copy_strings(values: *const *const c_char, count: i32) -> Vec<String> {
    if count <= 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(values, count as usize) }
        .iter()
        .map(|value| unsafe { copy_string(*value) }.unwrap_or_default())
        .collect()
}

#[cfg(feature = "wgpu")]
pub struct SdlWgpuWindow {
    frame: RefCell<Option<FrameResources>>,
    gpu: WgpuContext,
    native: SdlWindow,
}

#[cfg(feature = "wgpu")]
pub struct SdlWgpuFrame<'a> {
    resources: RefMut<'a, FrameResources>,
}

#[cfg(feature = "wgpu")]
struct FrameResources {
    texture_view: wgpu::TextureView,
    command_encoder: wgpu::CommandEncoder,
    surface_texture: wgpu::SurfaceTexture,
}

#[cfg(feature = "wgpu")]
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

#[cfg(feature = "wgpu")]
impl SdlInstance {
    pub fn create_wgpu_window(
        &self,
        title: &str,
        width: i32,
        height: i32,
        flags: WindowFlags,
    ) -> Result<SdlWgpuWindow, WindowError> {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let flags = flags | WindowFlags::METAL;
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
        let flags = flags | WindowFlags::VULKAN;
        let window = self.create_window(title, width, height, flags)?;
        SdlWgpuWindow::new(window)
    }
}

#[cfg(feature = "wgpu")]
impl SdlWgpuWindow {
    pub fn window_handle_provider(&self) -> Rc<dyn raw_window_handle::HasWindowHandle> {
        self.native.window_handle_provider()
    }

    pub fn init(&mut self, handler: &mut impl WindowHandler<Self>) -> Result<(), WindowError> {
        let result = handler.init(self);
        result.or_else(|error| handler.error(error))
    }

    pub fn update(&mut self, handler: &mut impl WindowHandler<Self>) -> Result<(), WindowError> {
        let result = handler.update(self);
        result.or_else(|error| handler.error(error))
    }

    pub fn window_event(
        &mut self,
        event: &WindowEvent,
        handler: &mut impl WindowHandler<Self>,
    ) -> Result<(), WindowError> {
        let result = (|| {
            if let WindowEvent::WindowPixelSizeChanged {
                window_id,
                width,
                height,
                ..
            } = event
            {
                if *window_id != self.get_id()? {
                    return Ok(());
                }
                let width =
                    u32::try_from(*width).map_err(|error| WindowError::Nul(error.to_string()))?;
                let height =
                    u32::try_from(*height).map_err(|error| WindowError::Nul(error.to_string()))?;
                self.resize_surface(width, height)?;
            }
            handler.window_event(self, event)
        })();
        result.or_else(|error| handler.error(error))
    }

    pub fn render(&mut self, handler: &mut impl WindowHandler<Self>) -> Result<bool, WindowError> {
        if self.gpu.suspended {
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
        let frame = match self.acquire_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(false),
            Err(error) => {
                handler.error(error)?;
                return Ok(false);
            }
        };
        *self.frame.borrow_mut() = Some(frame);
        let result = catch_unwind(AssertUnwindSafe(|| handler.render(self)));
        let frame = self
            .frame
            .try_borrow_mut()
            .map_err(|error| WindowError::Nul(error.to_string()))
            .map(|mut slot| slot.take());
        let result = match result {
            Ok(result) => result,
            Err(payload) => {
                drop(frame);
                resume_unwind(payload);
            }
        };
        let frame = frame?.ok_or_else(|| {
            WindowError::Nul("the render callback replaced the active window".into())
        })?;
        if let Err(error) = result {
            drop(frame);
            handler.error(error)?;
            return Ok(false);
        }
        self.gpu.queue.submit([frame.command_encoder.finish()]);
        self.gpu.queue.present(frame.surface_texture);
        let result = handler.frame_ready(self);
        result.or_else(|error| handler.error(error))?;
        Ok(true)
    }
}

#[cfg(feature = "wgpu")]
impl SdlWgpuWindow {
    fn new(native: SdlWindow) -> Result<Self, WindowError> {
        if native.has_surface() || native.get_flags().is_opengl() {
            return Err(WindowError::Nul(
                "wgpu windows cannot use an SDL software surface or OpenGL context".into(),
            ));
        }
        let (width, height) = native.get_size_in_pixels()?;
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
        let raw_display_handle = native.display_handle()?.as_raw();
        let raw_window_handle = native.window_handle()?.as_raw();
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
            frame: RefCell::new(None),
            gpu: WgpuContext {
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
            },
            native,
        })
    }

    fn acquire_frame(&self) -> Result<Option<FrameResources>, WindowError> {
        let surface_texture = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.gpu
                    .surface
                    .configure(&self.gpu.device, &self.gpu.surface_config);
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
            .gpu
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
                        load: wgpu::LoadOp::Clear(self.gpu.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self.gpu.queue.submit([clear.finish()]);
        let command_encoder =
            self.gpu
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

    fn software_surface_error() -> WindowError {
        WindowError::Nul("SDL software surfaces are not available on a wgpu window".into())
    }
}

#[cfg(feature = "wgpu")]
impl WgpuFrame for SdlWgpuFrame<'_> {
    fn texture_view(&self) -> &wgpu::TextureView {
        &self.resources.texture_view
    }

    fn command_encoder(&mut self) -> &mut wgpu::CommandEncoder {
        &mut self.resources.command_encoder
    }
}

#[cfg(feature = "wgpu")]
impl WgpuWindow for SdlWgpuWindow {
    type Frame<'a> = SdlWgpuFrame<'a>;

    fn instance(&self) -> &wgpu::Instance {
        &self.gpu.instance
    }

    fn adapter(&self) -> &wgpu::Adapter {
        &self.gpu.adapter
    }

    fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    fn queue(&self) -> &wgpu::Queue {
        &self.gpu.queue
    }

    fn surface_config(&self) -> &wgpu::SurfaceConfiguration {
        &self.gpu.surface_config
    }

    fn clear_color(&self) -> wgpu::Color {
        self.gpu.clear_color
    }

    fn set_clear_color(&mut self, color: wgpu::Color) {
        self.gpu.clear_color = color;
    }

    fn resize_surface(&mut self, width: u32, height: u32) -> Result<(), WindowError> {
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
            self.gpu.suspended = true;
            return Ok(());
        }
        let limit = self.gpu.device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return Err(WindowError::Nul(
                "window size exceeds the wgpu device limits".into(),
            ));
        }
        self.gpu.surface_config.width = width;
        self.gpu.surface_config.height = height;
        self.gpu
            .surface
            .configure(&self.gpu.device, &self.gpu.surface_config);
        self.gpu.suspended = false;
        Ok(())
    }

    fn next_frame(&self) -> Result<Self::Frame<'_>, WindowError> {
        let slot = self
            .frame
            .try_borrow_mut()
            .map_err(|error| WindowError::Nul(error.to_string()))?;
        let resources = RefMut::filter_map(slot, |slot| slot.as_mut())
            .map_err(|_| WindowError::Nul("a frame is only available during render".into()))?;
        Ok(SdlWgpuFrame { resources })
    }
}

#[cfg(feature = "wgpu")]
impl Window for SdlWgpuWindow {
    fn get_id(&self) -> Result<WindowId, WindowError> {
        self.native.get_id()
    }

    fn get_display(&self) -> Result<WindowDisplayId, WindowError> {
        self.native.get_display()
    }

    fn get_pixel_density(&self) -> Result<f32, WindowError> {
        self.native.get_pixel_density()
    }

    fn get_display_scale(&self) -> Result<f32, WindowError> {
        self.native.get_display_scale()
    }

    fn set_fullscreen_mode(&mut self, mode: Option<&WindowDisplayMode>) -> Result<(), WindowError> {
        self.native.set_fullscreen_mode(mode)
    }

    fn get_fullscreen_mode(&self) -> Option<WindowDisplayMode> {
        self.native.get_fullscreen_mode()
    }

    fn get_icc_profile(&self) -> Result<Vec<u8>, WindowError> {
        self.native.get_icc_profile()
    }

    fn get_pixel_format(&self) -> Result<WindowPixelFormat, WindowError> {
        self.native.get_pixel_format()
    }

    fn get_properties(&self) -> Result<WindowPropertiesId, WindowError> {
        self.native.get_properties()
    }

    fn get_flags(&self) -> WindowFlags {
        self.native.get_flags()
    }

    fn set_cursor(&mut self, cursor: WindowSystemCursor) -> Result<(), WindowError> {
        self.native.set_cursor(cursor)
    }

    fn start_text_input(&mut self) -> Result<(), WindowError> {
        self.native.start_text_input()
    }

    fn stop_text_input(&mut self) -> Result<(), WindowError> {
        self.native.stop_text_input()
    }

    fn set_title(&mut self, title: &str) -> Result<(), WindowError> {
        self.native.set_title(title)
    }

    fn get_title(&self) -> String {
        self.native.get_title()
    }

    fn set_icon(&mut self, icon: &WindowSurfaceRef<'_>) -> Result<(), WindowError> {
        self.native.set_icon(icon)
    }

    fn set_position(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        self.native.set_position(x, y)
    }

    fn get_position(&self) -> Result<(i32, i32), WindowError> {
        self.native.get_position()
    }

    fn set_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        self.native.set_size(width, height)
    }

    fn get_size(&self) -> Result<(i32, i32), WindowError> {
        self.native.get_size()
    }

    fn get_safe_area(&self) -> Result<WindowRect, WindowError> {
        self.native.get_safe_area()
    }

    fn set_aspect_ratio(&mut self, min_aspect: f32, max_aspect: f32) -> Result<(), WindowError> {
        self.native.set_aspect_ratio(min_aspect, max_aspect)
    }

    fn get_aspect_ratio(&self) -> Result<(f32, f32), WindowError> {
        self.native.get_aspect_ratio()
    }

    fn get_borders_size(&self) -> Result<(i32, i32, i32, i32), WindowError> {
        self.native.get_borders_size()
    }

    fn get_size_in_pixels(&self) -> Result<(i32, i32), WindowError> {
        self.native.get_size_in_pixels()
    }

    fn set_minimum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        self.native.set_minimum_size(width, height)
    }

    fn get_minimum_size(&self) -> Result<(i32, i32), WindowError> {
        self.native.get_minimum_size()
    }

    fn set_maximum_size(&mut self, width: i32, height: i32) -> Result<(), WindowError> {
        self.native.set_maximum_size(width, height)
    }

    fn get_maximum_size(&self) -> Result<(i32, i32), WindowError> {
        self.native.get_maximum_size()
    }

    fn set_bordered(&mut self, bordered: bool) -> Result<(), WindowError> {
        self.native.set_bordered(bordered)
    }

    fn set_resizable(&mut self, resizable: bool) -> Result<(), WindowError> {
        self.native.set_resizable(resizable)
    }

    fn set_always_on_top(&mut self, on_top: bool) -> Result<(), WindowError> {
        self.native.set_always_on_top(on_top)
    }

    fn set_fill_document(&mut self, fill: bool) -> Result<(), WindowError> {
        self.native.set_fill_document(fill)
    }

    fn show(&mut self) -> Result<(), WindowError> {
        self.native.show()
    }

    fn hide(&mut self) -> Result<(), WindowError> {
        self.native.hide()
    }

    fn raise(&mut self) -> Result<(), WindowError> {
        self.native.raise()
    }

    fn maximize(&mut self) -> Result<(), WindowError> {
        self.native.maximize()
    }

    fn minimize(&mut self) -> Result<(), WindowError> {
        self.native.minimize()
    }

    fn restore(&mut self) -> Result<(), WindowError> {
        self.native.restore()
    }

    fn set_fullscreen(&mut self, fullscreen: bool) -> Result<(), WindowError> {
        self.native.set_fullscreen(fullscreen)
    }

    fn sync(&self) -> Result<(), WindowError> {
        self.native.sync()
    }

    fn has_surface(&self) -> bool {
        false
    }

    fn get_surface(&mut self) -> Result<WindowSurfaceRef<'_>, WindowError> {
        Err(Self::software_surface_error())
    }

    fn set_surface_vsync(&mut self, _vsync: i32) -> Result<(), WindowError> {
        Err(Self::software_surface_error())
    }

    fn get_surface_vsync(&self) -> Result<i32, WindowError> {
        Err(Self::software_surface_error())
    }

    fn update_surface(&mut self) -> Result<(), WindowError> {
        Err(Self::software_surface_error())
    }

    fn update_surface_rects(&mut self, _rects: &[WindowRect]) -> Result<(), WindowError> {
        Err(Self::software_surface_error())
    }

    fn destroy_surface(&mut self) -> Result<(), WindowError> {
        Err(Self::software_surface_error())
    }

    fn set_keyboard_grab(&mut self, grabbed: bool) -> Result<(), WindowError> {
        self.native.set_keyboard_grab(grabbed)
    }

    fn set_mouse_grab(&mut self, grabbed: bool) -> Result<(), WindowError> {
        self.native.set_mouse_grab(grabbed)
    }

    fn get_keyboard_grab(&self) -> bool {
        self.native.get_keyboard_grab()
    }

    fn get_mouse_grab(&self) -> bool {
        self.native.get_mouse_grab()
    }

    fn set_mouse_rect(&mut self, rect: Option<&WindowRect>) -> Result<(), WindowError> {
        self.native.set_mouse_rect(rect)
    }

    fn get_mouse_rect(&self) -> Option<WindowRect> {
        self.native.get_mouse_rect()
    }

    fn set_opacity(&mut self, opacity: f32) -> Result<(), WindowError> {
        self.native.set_opacity(opacity)
    }

    fn get_opacity(&self) -> Result<f32, WindowError> {
        self.native.get_opacity()
    }

    fn set_focusable(&mut self, focusable: bool) -> Result<(), WindowError> {
        self.native.set_focusable(focusable)
    }

    fn show_system_menu(&mut self, x: i32, y: i32) -> Result<(), WindowError> {
        self.native.show_system_menu(x, y)
    }

    fn set_hit_test(
        &mut self,
        hit_test: impl Fn(i32, i32) -> WindowHitTestResult + 'static,
    ) -> Result<(), WindowError>
    where
        Self: Sized,
    {
        self.native.set_hit_test(hit_test)
    }

    fn set_shape(&mut self, shape: Option<&WindowSurfaceRef<'_>>) -> Result<(), WindowError> {
        self.native.set_shape(shape)
    }

    fn flash(&mut self, operation: WindowFlashOperation) -> Result<(), WindowError> {
        self.native.flash(operation)
    }

    fn set_progress_state(&mut self, state: WindowProgressState) -> Result<(), WindowError> {
        self.native.set_progress_state(state)
    }

    fn get_progress_state(&self) -> Result<WindowProgressState, WindowError> {
        self.native.get_progress_state()
    }

    fn set_progress_value(&mut self, value: f32) -> Result<(), WindowError> {
        self.native.set_progress_value(value)
    }

    fn get_progress_value(&self) -> Result<f32, WindowError> {
        self.native.get_progress_value()
    }

    fn display_handle(&self) -> Result<WindowDisplayHandle<'_>, WindowError> {
        self.native.display_handle()
    }

    fn window_handle(&self) -> Result<WindowHandle<'_>, WindowError> {
        self.native.window_handle()
    }
}

impl From<WindowSystemCursor> for sys::mouse::SDL_SystemCursor {
    fn from(value: WindowSystemCursor) -> Self {
        Self(match value {
            WindowSystemCursor::Default => 0,
            WindowSystemCursor::Text => 1,
            WindowSystemCursor::Wait => 2,
            WindowSystemCursor::Crosshair => 3,
            WindowSystemCursor::Progress => 4,
            WindowSystemCursor::NwseResize => 5,
            WindowSystemCursor::NeswResize => 6,
            WindowSystemCursor::EwResize => 7,
            WindowSystemCursor::NsResize => 8,
            WindowSystemCursor::Move => 9,
            WindowSystemCursor::NotAllowed => 10,
            WindowSystemCursor::Pointer => 11,
            WindowSystemCursor::NwResize => 12,
            WindowSystemCursor::NResize => 13,
            WindowSystemCursor::NeResize => 14,
            WindowSystemCursor::EResize => 15,
            WindowSystemCursor::SeResize => 16,
            WindowSystemCursor::SResize => 17,
            WindowSystemCursor::SwResize => 18,
            WindowSystemCursor::WResize => 19,
        })
    }
}
