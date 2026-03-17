//! makcu — Rust library for controlling MAKCU devices.
//!
//! # Quick start
//!
//! ```no_run
//! use makcu::{Device, Button};
//!
//! let dev = Device::connect().expect("MAKCU not found");
//!
//! println!("{}", dev.version().unwrap());
//!
//! dev.button_down(Button::Left).unwrap();
//! dev.move_xy(100, 50).unwrap();
//! dev.button_up(Button::Left).unwrap();
//! ```

mod device;
pub mod error;
pub(crate) mod protocol;
pub(crate) mod transport;
pub mod types;

#[cfg(feature = "batch")]
pub mod batch;

#[cfg(feature = "extras")]
pub mod extras;

/// Per-command timing profiler. Zero-cost when the `profile` feature is disabled.
pub mod profiler;

// -- Public re-exports --

pub use device::{Device, DeviceConfig, FireAndForget};
pub use error::{MakcuError, Result};
pub use types::{Button, ButtonMask, ConnectionState, DeviceInfo, LockStates, LockTarget};

#[cfg(feature = "async")]
pub use device::{AsyncDevice, AsyncFireAndForget};

#[cfg(feature = "batch")]
pub use batch::BatchBuilder;

#[cfg(all(feature = "batch", feature = "async"))]
pub use batch::AsyncBatchBuilder;

#[cfg(feature = "extras")]
pub use extras::EventHandle;

#[cfg(feature = "mock")]
pub use transport::mock::MockTransport;

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
