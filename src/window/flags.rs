bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct WindowFlags: u64 {
        const FULLSCREEN = 1 << 0;
        const OPENGL = 1 << 1;
        const OCCLUDED = 1 << 2;
        const HIDDEN = 1 << 3;
        const BORDERLESS = 1 << 4;
        const RESIZABLE = 1 << 5;
        const MINIMIZED = 1 << 6;
        const MAXIMIZED = 1 << 7;
        const MOUSE_GRABBED = 1 << 8;
        const INPUT_FOCUS = 1 << 9;
        const MOUSE_FOCUS = 1 << 10;
        const EXTERNAL = 1 << 11;
        const MODAL = 1 << 12;
        const HIGH_PIXEL_DENSITY = 1 << 13;
        const MOUSE_CAPTURE = 1 << 14;
        const MOUSE_RELATIVE_MODE = 1 << 15;
        const ALWAYS_ON_TOP = 1 << 16;
        const UTILITY = 1 << 17;
        const TOOLTIP = 1 << 18;
        const POPUP_MENU = 1 << 19;
        const KEYBOARD_GRABBED = 1 << 20;
        const FILL_DOCUMENT = 1 << 21;
        const VULKAN = 1 << 28;
        const METAL = 1 << 29;
        const TRANSPARENT = 1 << 30;
        const NOT_FOCUSABLE = 1 << 31;
        const _ = !0;
    }
}

impl WindowFlags {
    pub fn is_fullscreen(&self) -> bool {
        self.contains(Self::FULLSCREEN)
    }

    pub fn is_opengl(&self) -> bool {
        self.contains(Self::OPENGL)
    }

    pub fn is_occluded(&self) -> bool {
        self.contains(Self::OCCLUDED)
    }

    pub fn is_hidden(&self) -> bool {
        self.contains(Self::HIDDEN)
    }

    pub fn is_borderless(&self) -> bool {
        self.contains(Self::BORDERLESS)
    }

    pub fn is_resizable(&self) -> bool {
        self.contains(Self::RESIZABLE)
    }

    pub fn is_minimized(&self) -> bool {
        self.contains(Self::MINIMIZED)
    }

    pub fn is_maximized(&self) -> bool {
        self.contains(Self::MAXIMIZED)
    }

    pub fn is_mouse_grabbed(&self) -> bool {
        self.contains(Self::MOUSE_GRABBED)
    }

    pub fn is_input_focus(&self) -> bool {
        self.contains(Self::INPUT_FOCUS)
    }

    pub fn is_mouse_focus(&self) -> bool {
        self.contains(Self::MOUSE_FOCUS)
    }

    pub fn is_external(&self) -> bool {
        self.contains(Self::EXTERNAL)
    }

    pub fn is_modal(&self) -> bool {
        self.contains(Self::MODAL)
    }

    pub fn is_high_pixel_density(&self) -> bool {
        self.contains(Self::HIGH_PIXEL_DENSITY)
    }

    pub fn is_mouse_capture(&self) -> bool {
        self.contains(Self::MOUSE_CAPTURE)
    }

    pub fn is_mouse_relative_mode(&self) -> bool {
        self.contains(Self::MOUSE_RELATIVE_MODE)
    }

    pub fn is_always_on_top(&self) -> bool {
        self.contains(Self::ALWAYS_ON_TOP)
    }

    pub fn is_utility(&self) -> bool {
        self.contains(Self::UTILITY)
    }

    pub fn is_tooltip(&self) -> bool {
        self.contains(Self::TOOLTIP)
    }

    pub fn is_popup_menu(&self) -> bool {
        self.contains(Self::POPUP_MENU)
    }

    pub fn is_keyboard_grabbed(&self) -> bool {
        self.contains(Self::KEYBOARD_GRABBED)
    }

    pub fn is_fill_document(&self) -> bool {
        self.contains(Self::FILL_DOCUMENT)
    }

    pub fn is_vulkan(&self) -> bool {
        self.contains(Self::VULKAN)
    }

    pub fn is_metal(&self) -> bool {
        self.contains(Self::METAL)
    }

    pub fn is_transparent(&self) -> bool {
        self.contains(Self::TRANSPARENT)
    }

    pub fn is_focusable(&self) -> bool {
        !self.contains(Self::NOT_FOCUSABLE)
    }
}
