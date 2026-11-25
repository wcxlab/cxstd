use std::sync::Arc;

use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::platform::backend::clipboard;

use super::{ClipboardError, ClipboardItem, ClipboardTrait};

/// Access the system clipboard through the application's Tokio runtime.
#[derive(Clone)]
pub struct ClipboardService {
    clipboard: Arc<Mutex<clipboard::Clipboard>>,
}

impl ClipboardService {
    pub(in crate::platform) fn new() -> Self {
        Self {
            clipboard: Arc::default(),
        }
    }

    async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&clipboard::Clipboard) -> Result<T, ClipboardError> + Send + 'static,
    ) -> anyhow::Result<T> {
        let clipboard = self.clipboard.clone().lock_owned().await;
        // A dispatched operation keeps the lock until the native call finishes.
        let result = tokio::task::spawn_blocking(move || operation(&clipboard))
            .await
            .map_err(ClipboardError::Worker)?;
        Ok(result?)
    }
}

impl ClipboardTrait for ClipboardService {
    fn read(&self) -> BoxFuture<'_, anyhow::Result<Vec<ClipboardItem>>> {
        Box::pin(self.run(clipboard::Clipboard::read))
    }

    fn write<'a>(&'a self, items: &'a [ClipboardItem]) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            let Some(item) = items.first().cloned() else {
                return Ok(());
            };
            self.run(move |clipboard| clipboard.write(&item)).await
        })
    }

    fn read_text(&self) -> BoxFuture<'_, anyhow::Result<String>> {
        Box::pin(self.run(clipboard::Clipboard::read_text))
    }

    fn write_text<'a>(&'a self, text: &'a str) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            let text = text.to_owned();
            self.run(move |clipboard| {
                std::ffi::CString::new(text.as_str())?;
                clipboard.write_text(&text)
            })
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread};

    use tokio::sync::oneshot;

    use super::*;

    #[tokio::test]
    async fn blocking_operation_keeps_its_lock_when_the_caller_cancels() -> anyhow::Result<()> {
        let clipboard = ClipboardService::new();
        let worker_clipboard = clipboard.clone();
        let (started, running) = oneshot::channel();
        let (release, wait_for_release) = mpsc::channel();
        let task = tokio::spawn(async move {
            worker_clipboard
                .run(move |_| {
                    let _ = started.send(thread::current().id());
                    let _ = wait_for_release.recv();
                    Ok(())
                })
                .await
        });

        assert_ne!(running.await?, thread::current().id());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(clipboard.clipboard.try_lock().is_err());
        release.send(())?;

        let provider: &dyn ClipboardTrait = &clipboard;
        let error = provider.write_text("invalid\0text").await.unwrap_err();
        assert!(matches!(
            error.downcast_ref::<ClipboardError>(),
            Some(ClipboardError::InteriorNul(_))
        ));
        Ok(())
    }
}
