//! Console logging through the application's `tracing` subscriber.
//!
//! ```
//! use cxstd::platform::console;
//!
//! console::log("Application started");
//! console::warn(format_args!("{} frames were skipped", 2));
//! ```

mod console;

pub use console::{assert, debug, dir, error, info, log, trace, warn};
