//! Access to the system clipboard through `cxstd::platform::clipboard()`.
//!
//! On Windows, files copied in Explorer are read as `text/uri-list`. Compatible
//! GPUI text metadata is represented as `application/x-gpui-text-metadata` with
//! UTF-8 bytes alongside `text/plain`.

use std::collections::BTreeMap;

use futures::future::BoxFuture;

mod service;

pub use service::ClipboardService;

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("the clipboard contains no plain text")]
    NoText,
    #[error("a clipboard item must contain at least one representation")]
    EmptyItem,
    #[error("invalid clipboard MIME type: {0}")]
    InvalidMimeType(String),
    #[error("unsupported clipboard MIME type: {0}")]
    UnsupportedMimeType(String),
    #[error("clipboard item has no representation for {0}")]
    MissingMimeType(String),
    #[error("clipboard text contains an interior NUL byte")]
    InteriorNul(#[from] std::ffi::NulError),
    #[error("plain clipboard text must be UTF-8 without interior NUL bytes")]
    InvalidPlainText,
    #[error("clipboard operation failed: {0}")]
    Native(String),
    #[error("clipboard worker failed: {0}")]
    Worker(#[source] tokio::task::JoinError),
}

/// One clipboard item with an owned byte representation for each MIME type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardItem {
    pub(in crate::platform) formats: BTreeMap<String, Vec<u8>>,
}

impl ClipboardItem {
    pub fn new(
        formats: impl IntoIterator<Item = (impl Into<String>, Vec<u8>)>,
    ) -> anyhow::Result<Self> {
        let mut item = BTreeMap::new();
        for (mime_type, bytes) in formats {
            let mime_type = mime_type.into();
            if !mime_type.contains('/') || mime_type.contains('\0') {
                return Err(ClipboardError::InvalidMimeType(mime_type).into());
            }
            item.insert(mime_type, bytes);
        }
        if item.is_empty() {
            return Err(ClipboardError::EmptyItem.into());
        }
        Ok(Self { formats: item })
    }

    pub fn types(&self) -> impl ExactSizeIterator<Item = &str> + '_ {
        self.formats.keys().map(String::as_str)
    }

    pub fn get_type(&self, mime_type: &str) -> anyhow::Result<&[u8]> {
        self.formats
            .get(mime_type)
            .map(Vec::as_slice)
            .ok_or_else(|| ClipboardError::MissingMimeType(mime_type.to_owned()).into())
    }
}

/// Clipboard operations to be awaited on the application's Tokio runtime.
pub trait ClipboardTrait {
    /// Return an owned snapshot of the current item, or an empty list if there is none.
    fn read(&self) -> BoxFuture<'_, anyhow::Result<Vec<ClipboardItem>>>;

    /// Write the first item. An unsupported representation returns an error.
    /// An empty slice has no effect.
    fn write<'a>(&'a self, items: &'a [ClipboardItem]) -> BoxFuture<'a, anyhow::Result<()>>;

    /// Returns `ClipboardError::NoText` when there is no plain text to read.
    fn read_text(&self) -> BoxFuture<'_, anyhow::Result<String>>;

    /// Replaces the clipboard with UTF-8 text. An empty string clears it.
    fn write_text<'a>(&'a self, text: &'a str) -> BoxFuture<'a, anyhow::Result<()>>;
}

impl crate::ecs::Resource for dyn ClipboardTrait {}
