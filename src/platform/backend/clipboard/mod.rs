use std::{collections::BTreeMap, io::Cursor, path::PathBuf};

use url::Url;

use crate::platform::clipboard::{ClipboardError, ClipboardItem};

#[cfg(not(windows))]
mod portable;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(in crate::platform) use portable::Clipboard;
#[cfg(windows)]
pub(in crate::platform) use windows::Clipboard;

const TEXT: &str = "text/plain";
const UTF8_TEXT: &str = "text/plain;charset=utf-8";
const HTML: &str = "text/html";
const PNG: &str = "image/png";
const FILE_URIS: &str = "text/uri-list";

fn snapshot(formats: BTreeMap<String, Vec<u8>>) -> Vec<ClipboardItem> {
    if formats.is_empty() {
        Vec::new()
    } else {
        vec![ClipboardItem { formats }]
    }
}

fn plain_text(item: &ClipboardItem) -> Result<Option<&str>, ClipboardError> {
    item.formats
        .get(TEXT)
        .or_else(|| item.formats.get(UTF8_TEXT))
        .map(|bytes| {
            if bytes.contains(&0) {
                return Err(ClipboardError::InvalidPlainText);
            }
            std::str::from_utf8(bytes).map_err(|_| ClipboardError::InvalidPlainText)
        })
        .transpose()
}

fn native_error(error: impl std::fmt::Display) -> ClipboardError {
    ClipboardError::Native(error.to_string())
}

fn encode_png(image: image::DynamicImage) -> Result<Vec<u8>, ClipboardError> {
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(native_error)?;
    Ok(bytes.into_inner())
}

fn decode_image(mime_type: &str, bytes: &[u8]) -> Result<image::DynamicImage, ClipboardError> {
    let format = image::ImageFormat::from_mime_type(mime_type)
        .ok_or_else(|| ClipboardError::UnsupportedMimeType(mime_type.to_owned()))?;
    image::load_from_memory_with_format(bytes, format).map_err(native_error)
}

fn file_uris(paths: &[PathBuf]) -> Result<Vec<u8>, ClipboardError> {
    let mut data = String::new();
    for path in paths {
        let url = Url::from_file_path(path)
            .map_err(|()| native_error("clipboard file path could not be encoded as a file URL"))?;
        data.push_str(url.as_str());
        data.push_str("\r\n");
    }
    Ok(data.into_bytes())
}
