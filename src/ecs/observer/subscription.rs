/// A local registration that is cancelled when its handle is dropped.
#[must_use = "dropping the subscription immediately cancels it"]
pub struct Subscription {
    cancel: Option<Box<dyn FnOnce()>>,
}

impl Subscription {
    /// Creates a handle that runs `cancel` when dropped.
    pub fn new(cancel: impl FnOnce() + 'static) -> Self {
        Self {
            cancel: Some(Box::new(cancel)),
        }
    }

    /// Leaves the registration active without retaining its handle.
    ///
    /// The host must arrange later cleanup, for example with `Observers::remove_key()`.
    pub fn detach(mut self) {
        self.cancel.take();
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}

impl std::fmt::Debug for Subscription {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Subscription").finish()
    }
}
