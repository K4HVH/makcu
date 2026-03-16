use std::sync::mpsc;

use crate::error::Result;
use crate::protocol::constants;
use crate::types::ButtonMask;

use super::{Device, FireAndForget};

impl Device {
    /// Enable the button-state-change stream on the device.
    pub fn enable_button_stream(&self) -> Result<()> {
        self.exec(constants::CMD_BUTTONS_ON)
    }

    /// Disable the button-state-change stream.
    pub fn disable_button_stream(&self) -> Result<()> {
        self.exec(constants::CMD_BUTTONS_OFF)
    }

    /// Subscribe to button events. Returns a receiver that yields `ButtonMask`
    /// values whenever the device reports a button state change.
    ///
    /// You must call `enable_button_stream()` first for events to flow.
    pub fn button_events(&self) -> mpsc::Receiver<ButtonMask> {
        self.transport().subscribe_buttons()
    }
}

impl FireAndForget<'_> {
    pub fn enable_button_stream(&self) -> Result<()> {
        self.send(constants::CMD_BUTTONS_ON)
    }

    pub fn disable_button_stream(&self) -> Result<()> {
        self.send(constants::CMD_BUTTONS_OFF)
    }
}

// -- Async --

#[cfg(feature = "async")]
use super::{AsyncDevice, AsyncFireAndForget};

#[cfg(feature = "async")]
impl AsyncDevice {
    pub async fn enable_button_stream(&self) -> Result<()> {
        self.exec(constants::CMD_BUTTONS_ON).await
    }

    pub async fn disable_button_stream(&self) -> Result<()> {
        self.exec(constants::CMD_BUTTONS_OFF).await
    }

    pub fn button_events(&self) -> mpsc::Receiver<ButtonMask> {
        self.transport().subscribe_buttons()
    }
}

#[cfg(feature = "async")]
impl AsyncFireAndForget<'_> {
    pub fn enable_button_stream(&self) -> Result<()> {
        self.send(constants::CMD_BUTTONS_ON)
    }

    pub fn disable_button_stream(&self) -> Result<()> {
        self.send(constants::CMD_BUTTONS_OFF)
    }
}
