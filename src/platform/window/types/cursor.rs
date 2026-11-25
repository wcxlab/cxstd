//! CSS cursor keywords and native fallback semantics.

/// Predefined CSS `cursor` keywords, using Rust variant names.
///
/// Custom images and CSS cascade values are outside this enum. `Auto` requires
/// content-level resolution and uses `Default` at the native window boundary.
/// SDL approximates unsupported shapes: context-menu/help/alias/copy/zoom use
/// the default arrow; cell uses crosshair; vertical-text uses text; grab/grabbing
/// and all-scroll use move; no-drop uses not-allowed; col/row-resize use ew/ns-resize.
/// `None` hides the cursor; all other values make it visible.
///
/// See <https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/cursor>.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WindowCursor {
    #[default]
    Auto,
    Default,
    None,
    ContextMenu,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NoDrop,
    NotAllowed,
    Grab,
    Grabbing,
    AllScroll,
    ColResize,
    RowResize,
    NResize,
    EResize,
    SResize,
    WResize,
    NeResize,
    NwResize,
    SeResize,
    SwResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ZoomIn,
    ZoomOut,
}
