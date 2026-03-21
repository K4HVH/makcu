use std::sync::mpsc;

use crate::error::Result;
use crate::protocol::constants;
use crate::types::{Button, CatchEvent};

use super::{Device, FireAndForget};

impl Device {
    /// Enable the catch stream for a button.
    ///
    /// The button **must be locked** via `set_lock()` before calling this.
    /// Catch produces no events without an active lock.
    ///
    /// There is no explicit catch disable command — unlocking the button
    /// (via `set_lock(target, false)`) is the only way to stop the stream.
    /// This means catch and lock are coupled: you cannot keep a button
    /// locked while disabling its catch stream.
    pub fn enable_catch(&self, button: Button) -> Result<()> {
        self.exec(constants::catch_enable_cmd(button))
    }

    /// Subscribe to catch events. Returns a receiver that yields `CatchEvent`
    /// values whenever a locked button with catch enabled is physically
    /// pressed or released.
    ///
    /// You must call `set_lock()` then `enable_catch()` first for events to flow.
    pub fn catch_events(&self) -> mpsc::Receiver<CatchEvent> {
        self.transport().subscribe_catch()
    }
}

impl FireAndForget<'_> {
    pub fn enable_catch(&self, button: Button) -> Result<()> {
        self.send(constants::catch_enable_cmd(button))
    }
}

// -- Async --

#[cfg(feature = "async")]
use super::{AsyncDevice, AsyncFireAndForget};

#[cfg(feature = "async")]
impl AsyncDevice {
    /// Enable the catch stream for a button.
    ///
    /// The button **must be locked** via `set_lock()` before calling this.
    /// Catch produces no events without an active lock. Unlocking is the
    /// only way to stop the stream.
    pub async fn enable_catch(&self, button: Button) -> Result<()> {
        self.exec(constants::catch_enable_cmd(button)).await
    }

    /// Subscribe to catch events.
    pub fn catch_events(&self) -> mpsc::Receiver<CatchEvent> {
        self.transport().subscribe_catch()
    }
}

#[cfg(feature = "async")]
impl AsyncFireAndForget<'_> {
    pub fn enable_catch(&self, button: Button) -> Result<()> {
        self.send(constants::catch_enable_cmd(button))
    }
}
