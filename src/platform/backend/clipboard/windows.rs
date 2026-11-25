use std::{
    borrow::Cow,
    collections::BTreeMap,
    hash::{Hash, Hasher},
    ptr,
};

use clipboard_win::{
    Clipboard as OpenClipboard,
    formats::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT},
    options::NoClear,
    raw,
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE},
};

use super::{
    ClipboardError, ClipboardItem, FILE_URIS, HTML, PNG, TEXT, UTF8_TEXT, decode_image, encode_png,
    file_uris, native_error, plain_text, snapshot,
};

const BMP: &str = "image/bmp";
const METADATA: &str = "application/x-gpui-text-metadata";
const GPUI_HASH: &str = "GPUI internal text hash";
const GPUI_METADATA: &str = "GPUI internal metadata";

#[derive(Default)]
pub(in crate::platform) struct Clipboard;

impl Clipboard {
    pub(in crate::platform) fn read(&self) -> Result<Vec<ClipboardItem>, ClipboardError> {
        let _clipboard = OpenClipboard::new_attempts(5).map_err(native_error)?;
        let mut formats = BTreeMap::new();
        for format in raw::EnumFormats::new() {
            if let Some((mime_type, bytes)) = read_format(format)? {
                formats.entry(mime_type).or_insert(bytes);
            }
        }
        if let Some(text) = formats.get(TEXT)
            && let Some(metadata) = read_metadata(text)?
        {
            formats.insert(METADATA.to_owned(), metadata);
        }
        Ok(snapshot(formats))
    }

    pub(in crate::platform) fn write(&self, item: &ClipboardItem) -> Result<(), ClipboardError> {
        let text = plain_text(item)?;
        let mut formats = Vec::new();
        if let Some(text) = text {
            formats.push(Representation::Text(text));
        }
        for (mime_type, bytes) in &item.formats {
            match mime_type.as_str() {
                TEXT | UTF8_TEXT => {}
                HTML => {
                    let html = std::str::from_utf8(bytes).map_err(native_error)?;
                    formats.push(Representation::Html(
                        registered_format("HTML Format")?,
                        html,
                    ));
                }
                METADATA => {
                    let text =
                        text.ok_or_else(|| ClipboardError::UnsupportedMimeType(mime_type.clone()))?;
                    let metadata = std::str::from_utf8(bytes).map_err(native_error)?;
                    if metadata.contains('\0') {
                        return Err(native_error(
                            "clipboard metadata contains an interior NUL byte",
                        ));
                    }
                    let metadata = metadata
                        .encode_utf16()
                        .chain(Some(0))
                        .flat_map(u16::to_le_bytes)
                        .collect();
                    formats.push(Representation::Bytes(
                        registered_format(GPUI_HASH)?,
                        Cow::Owned(text_hash(text).to_le_bytes().to_vec()),
                    ));
                    formats.push(Representation::Bytes(
                        registered_format(GPUI_METADATA)?,
                        Cow::Owned(metadata),
                    ));
                }
                mime_type => {
                    let format = registered_format(windows_format_name(mime_type))?;
                    formats.push(Representation::Bytes(format, Cow::Borrowed(bytes)));
                }
            }
        }
        if !item.formats.contains_key(PNG) {
            for format in ["image/jpeg", "image/webp", "image/gif", BMP, "image/tiff"] {
                if let Some(bytes) = item.formats.get(format)
                    && let Ok(image) = decode_image(format, bytes)
                {
                    formats.push(Representation::Bytes(
                        registered_format("PNG")?,
                        Cow::Owned(encode_png(image)?),
                    ));
                    break;
                }
            }
        }
        write_formats(&formats)
    }

    pub(in crate::platform) fn read_text(&self) -> Result<String, ClipboardError> {
        let _clipboard = OpenClipboard::new_attempts(5).map_err(native_error)?;
        if !raw::is_format_avail(CF_UNICODETEXT) {
            return Err(ClipboardError::NoText);
        }
        let mut bytes = Vec::new();
        raw::get_string(&mut bytes).map_err(native_error)?;
        if bytes.is_empty() {
            return Err(ClipboardError::NoText);
        }
        String::from_utf8(bytes).map_err(|_| ClipboardError::InvalidPlainText)
    }

    pub(in crate::platform) fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        if text.is_empty() {
            write_formats(&[])
        } else {
            write_formats(&[Representation::Text(text)])
        }
    }
}

fn read_format(format: u32) -> Result<Option<(String, Vec<u8>)>, ClipboardError> {
    let mut bytes = Vec::new();
    let mime_type = match format {
        CF_UNICODETEXT => {
            raw::get_string(&mut bytes).map_err(native_error)?;
            TEXT.to_owned()
        }
        CF_DIB | CF_DIBV5 => {
            raw::get_vec(format, &mut bytes).map_err(native_error)?;
            let Some(bmp) = dib_to_bmp(&bytes) else {
                return Ok(None);
            };
            bytes = bmp;
            BMP.to_owned()
        }
        CF_HDROP => {
            let mut paths = Vec::new();
            raw::get_file_list_path(&mut paths).map_err(native_error)?;
            bytes = file_uris(&paths)?;
            FILE_URIS.to_owned()
        }
        _ => {
            let Some(name) = raw::format_name_big(format) else {
                return Ok(None);
            };
            let known_name = match name.to_ascii_lowercase().as_str() {
                "png" => PNG,
                "gif" => "image/gif",
                "jfif" => "image/jpeg",
                "html format" => HTML,
                _ if name.contains('/') => &name,
                _ => return Ok(None),
            };
            if known_name == HTML && name.eq_ignore_ascii_case("HTML Format") {
                raw::get_html(format, &mut bytes).map_err(native_error)?;
            } else {
                raw::get_vec(format, &mut bytes).map_err(native_error)?;
            }
            known_name.to_owned()
        }
    };
    Ok(Some((mime_type, bytes)))
}

fn windows_format_name(mime_type: &str) -> &str {
    match mime_type {
        PNG => "PNG",
        "image/gif" => "GIF",
        "image/jpeg" => "JFIF",
        mime_type => mime_type,
    }
}

fn registered_format(name: &str) -> Result<u32, ClipboardError> {
    raw::register_format(name)
        .map(std::num::NonZeroU32::get)
        .ok_or_else(|| native_error(format!("could not register clipboard format {name}")))
}

enum Representation<'a> {
    Text(&'a str),
    Html(u32, &'a str),
    Bytes(u32, Cow<'a, [u8]>),
}

fn write_formats(formats: &[Representation<'_>]) -> Result<(), ClipboardError> {
    let owner = ClipboardOwner::new()?;
    let _clipboard = OpenClipboard::new_attempts_for(owner.0, 5).map_err(native_error)?;
    raw::empty().map_err(native_error)?;
    for format in formats {
        match format {
            Representation::Text(text) => raw::set_string_with(text, NoClear),
            Representation::Html(format, text) => raw::set_html_with(*format, text, NoClear),
            Representation::Bytes(format, bytes) => raw::set_without_clear(*format, bytes),
        }
        .map_err(native_error)?;
    }
    Ok(())
}

struct ClipboardOwner(HWND);

impl ClipboardOwner {
    fn new() -> Result<Self, ClipboardError> {
        // A message-only window supplies the Win32 owner even before a render window exists.
        let window = unsafe {
            CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if window.is_null() {
            Err(native_error(std::io::Error::last_os_error()))
        } else {
            Ok(Self(window))
        }
    }
}

impl Drop for ClipboardOwner {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) };
    }
}

fn text_hash(text: &str) -> u64 {
    let mut hasher = seahash::SeaHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn read_metadata(text: &[u8]) -> Result<Option<Vec<u8>>, ClipboardError> {
    let hash_format = registered_format(GPUI_HASH)?;
    let metadata_format = registered_format(GPUI_METADATA)?;
    if !raw::is_format_avail(hash_format) || !raw::is_format_avail(metadata_format) {
        return Ok(None);
    }
    let text = std::str::from_utf8(text).map_err(|_| ClipboardError::InvalidPlainText)?;
    let mut hash = Vec::new();
    raw::get_vec(hash_format, &mut hash).map_err(native_error)?;
    if !hash.starts_with(&text_hash(text).to_le_bytes()) {
        return Ok(None);
    }
    let mut metadata = Vec::new();
    raw::get_vec(metadata_format, &mut metadata).map_err(native_error)?;
    let words = metadata
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|word| *word != 0)
        .collect::<Vec<_>>();
    Ok(Some(String::from_utf16_lossy(&words).into_bytes()))
}

fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let header_size = u32::from_le_bytes(dib.get(..4)?.try_into().ok()?);
    if header_size < 40 || header_size as usize > dib.len() {
        return None;
    }
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    let colors = u32::from_le_bytes(dib[32..36].try_into().ok()?);
    let colors = if colors == 0 && bit_count <= 8 {
        1u32 << bit_count
    } else {
        colors
    };
    let masks = match (header_size, compression) {
        (40, 3) => 12,
        (40, 6) => 16,
        _ => 0,
    };
    let pixel_offset = 14u32
        .checked_add(header_size)?
        .checked_add(masks)?
        .checked_add(colors.checked_mul(4)?)?;
    let file_size = 14u32.checked_add(u32::try_from(dib.len()).ok()?)?;
    if pixel_offset > file_size {
        return None;
    }
    let mut bmp = Vec::with_capacity(file_size as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&pixel_offset.to_le_bytes());
    bmp.extend_from_slice(dib);
    Some(bmp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn restores_the_bmp_file_header_for_native_dib_pixels() -> anyhow::Result<()> {
        let image = image::DynamicImage::new_rgba8(2, 1);
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Bmp)?;
        let bmp = bytes.into_inner();
        assert_eq!(dib_to_bmp(&bmp[14..]).as_deref(), Some(bmp.as_slice()));
        assert!(dib_to_bmp(&[0; 39]).is_none());
        Ok(())
    }
}
