use std::{cell::RefCell, collections::BTreeMap};

use super::{
    ClipboardError, ClipboardItem, FILE_URIS, HTML, PNG, TEXT, UTF8_TEXT, Url, decode_image,
    encode_png, file_uris, native_error, plain_text, snapshot,
};

#[derive(Default)]
pub(in crate::platform) struct Clipboard(RefCell<Option<arboard::Clipboard>>);

impl Clipboard {
    pub(in crate::platform) fn read(&self) -> Result<Vec<ClipboardItem>, ClipboardError> {
        self.with_clipboard(|clipboard| {
            let mut formats = BTreeMap::new();
            if let Some(text) = available(clipboard.get_text())? {
                formats.insert(TEXT.to_owned(), text.into_bytes());
            }
            if let Some(html) = available(clipboard.get().html())? {
                formats.insert(HTML.to_owned(), html.into_bytes());
            }
            if let Some(paths) = available(clipboard.get().file_list())? {
                formats.insert(FILE_URIS.to_owned(), file_uris(&paths)?);
            }
            if let Some(data) = available(clipboard.get_image())? {
                let width = u32::try_from(data.width).map_err(native_error)?;
                let height = u32::try_from(data.height).map_err(native_error)?;
                let image = image::RgbaImage::from_raw(width, height, data.bytes.into_owned())
                    .ok_or_else(|| native_error("invalid clipboard image dimensions"))?;
                formats.insert(PNG.to_owned(), encode_png(image.into())?);
            }
            Ok(snapshot(formats))
        })
    }

    pub(in crate::platform) fn write(&self, item: &ClipboardItem) -> Result<(), ClipboardError> {
        let text = plain_text(item)?;
        if item
            .types()
            .all(|format| matches!(format, TEXT | UTF8_TEXT | HTML))
        {
            if let Some(html) = item.formats.get(HTML) {
                let html = std::str::from_utf8(html).map_err(native_error)?;
                return self.with_clipboard(|clipboard| {
                    clipboard.set_html(html, text).map_err(native_error)
                });
            }
            if let Some(text) = text {
                return self.write_text(text);
            }
        }

        if item.formats.len() == 1 {
            let (mime_type, bytes) = item
                .formats
                .first_key_value()
                .ok_or(ClipboardError::EmptyItem)?;
            if mime_type == FILE_URIS {
                let data = std::str::from_utf8(bytes).map_err(native_error)?;
                let paths = data
                    .lines()
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(|line| {
                        Url::parse(line)
                            .map_err(native_error)?
                            .to_file_path()
                            .map_err(|()| {
                                native_error("clipboard file list contains a non-file URL")
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                return self.with_clipboard(|clipboard| {
                    clipboard.set().file_list(&paths).map_err(native_error)
                });
            }
            if mime_type.starts_with("image/") {
                let image = decode_image(mime_type, bytes)?.into_rgba8();
                let data = arboard::ImageData {
                    width: image.width() as usize,
                    height: image.height() as usize,
                    bytes: image.into_raw().into(),
                };
                return self
                    .with_clipboard(|clipboard| clipboard.set_image(data).map_err(native_error));
            }
        }
        let format = item
            .types()
            .find(|format| !matches!(*format, TEXT | UTF8_TEXT | HTML));
        Err(ClipboardError::UnsupportedMimeType(
            format.ok_or(ClipboardError::EmptyItem)?.to_owned(),
        ))
    }

    pub(in crate::platform) fn read_text(&self) -> Result<String, ClipboardError> {
        self.with_clipboard(|clipboard| {
            available(clipboard.get_text())?
                .filter(|text| !text.is_empty())
                .ok_or(ClipboardError::NoText)
        })
    }

    pub(in crate::platform) fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        self.with_clipboard(|clipboard| {
            if text.is_empty() {
                clipboard.clear().map_err(native_error)
            } else {
                clipboard.set_text(text).map_err(native_error)
            }
        })
    }

    fn with_clipboard<T>(
        &self,
        operation: impl FnOnce(&mut arboard::Clipboard) -> Result<T, ClipboardError>,
    ) -> Result<T, ClipboardError> {
        let mut clipboard = self.0.borrow_mut();
        if clipboard.is_none() {
            *clipboard = Some(arboard::Clipboard::new().map_err(native_error)?);
        }
        let clipboard = clipboard.as_mut().expect("clipboard has been initialized");
        operation(clipboard)
    }
}

fn available<T>(result: Result<T, arboard::Error>) -> Result<Option<T>, ClipboardError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(native_error(error)),
    }
}
