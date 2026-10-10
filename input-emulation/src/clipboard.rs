use arboard::{Clipboard, ImageData};
use input_event::{clipboard_key_image, ClipboardEvent};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::task::spawn_blocking;

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("Failed to access clipboard: {0}")]
    Access(String),
    #[error("Failed to set clipboard: {0}")]
    Set(String),
}

/// Clipboard emulation that sets clipboard content
#[derive(Clone)]
pub struct ClipboardEmulation {
    // Use Arc<Mutex<>> to share clipboard across threads
    clipboard: Arc<Mutex<Option<Clipboard>>>,
}

impl ClipboardEmulation {
    pub fn new() -> Result<Self, ClipboardError> {
        // Try to create initial clipboard instance
        let clipboard = match Clipboard::new() {
            Ok(c) => Some(c),
            Err(e) => {
                log::warn!("Failed to create clipboard instance: {e}");
                None
            }
        };

        Ok(Self {
            clipboard: Arc::new(Mutex::new(clipboard)),
        })
    }

    /// Set clipboard content from a clipboard event
    pub async fn set(&self, event: ClipboardEvent) -> Result<(), ClipboardError> {
        match event {
            ClipboardEvent::Text(text) => {
                let clipboard_arc = self.clipboard.clone();

                spawn_blocking(move || {
                    let mut clipboard_guard = clipboard_arc.lock().unwrap();

                    // Try to get or create clipboard
                    let clipboard = match clipboard_guard.as_mut() {
                        Some(c) => c,
                        None => {
                            // Try to create a new clipboard instance
                            match Clipboard::new() {
                                Ok(c) => {
                                    *clipboard_guard = Some(c);
                                    clipboard_guard.as_mut().unwrap()
                                }
                                Err(e) => {
                                    return Err(ClipboardError::Access(format!("{e}")));
                                }
                            }
                        }
                    };

                    // Set clipboard text
                    clipboard
                        .set_text(text.clone())
                        .map_err(|e| ClipboardError::Set(format!("{e}")))?;

                    log::debug!("Clipboard set, length: {} bytes", text.len());
                    Ok(())
                })
                .await
                .map_err(|e| ClipboardError::Access(format!("Task join error: {e}")))?
            }
        }
    }

    /// Get current clipboard content (for testing/verification)
    pub async fn get(&self) -> Result<String, ClipboardError> {
        let clipboard_arc = self.clipboard.clone();

        spawn_blocking(move || {
            let mut clipboard_guard = clipboard_arc.lock().unwrap();

            let clipboard = match clipboard_guard.as_mut() {
                Some(c) => c,
                None => match Clipboard::new() {
                    Ok(c) => {
                        *clipboard_guard = Some(c);
                        clipboard_guard.as_mut().unwrap()
                    }
                    Err(e) => {
                        return Err(ClipboardError::Access(format!("{e}")));
                    }
                },
            };

            clipboard
                .get_text()
                .map_err(|e| ClipboardError::Access(format!("{e}")))
        })
        .await
        .map_err(|e| ClipboardError::Access(format!("Task join error: {e}")))?
    }

    /// Apply a PNG image to the local clipboard, returning the
    /// [`clipboard_key_image`] key of the pixels applied so the caller
    /// can tell the local poll loop not to re-broadcast them.
    pub async fn set_image_png(&self, png: Vec<u8>) -> Result<Vec<u8>, ClipboardError> {
        let clipboard_arc = self.clipboard.clone();

        spawn_blocking(move || {
            let rgba = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
                .map_err(|e| ClipboardError::Set(format!("image decode failed: {e}")))?
                .to_rgba8();
            let (width, height) = (rgba.width(), rgba.height());
            let pixels = rgba.into_raw();
            let key = clipboard_key_image(width, height, &pixels);

            let mut clipboard_guard = clipboard_arc.lock().unwrap();
            let clipboard = get_or_create(&mut clipboard_guard)?;
            clipboard
                .set_image(ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: pixels.into(),
                })
                .map_err(|e| ClipboardError::Set(format!("{e}")))?;

            log::debug!("Clipboard image set, {width}x{height}");
            Ok(key)
        })
        .await
        .map_err(|e| ClipboardError::Access(format!("Task join error: {e}")))?
    }
}

/// Get the shared clipboard handle, opening one on first use.
fn get_or_create(clipboard: &mut Option<Clipboard>) -> Result<&mut Clipboard, ClipboardError> {
    if clipboard.is_none() {
        *clipboard =
            Some(Clipboard::new().map_err(|e| ClipboardError::Access(format!("{e}")))?);
    }
    Ok(clipboard.as_mut().expect("clipboard just opened"))
}
