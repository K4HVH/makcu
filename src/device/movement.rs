use crate::error::{MakcuError, Result};
use crate::protocol::builder::{self, CommandBuf};
use crate::timed;

use super::{Device, FireAndForget};

/// Unwrap a builder result, converting `None` (buffer overflow) to a protocol error.
fn cmd(opt: Option<CommandBuf>) -> Result<CommandBuf> {
    opt.ok_or_else(|| MakcuError::Protocol("command too long for buffer".into()))
}

impl Device {
    /// Relative mouse move. Coordinates are in HID units, range ±32767.
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        timed!("move_xy", {
            self.exec_dynamic(cmd(builder::build_move(x, y))?.as_bytes())
        })
    }

    /// Silent click-move: left-down → move → left-up in two HID frames.
    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        timed!("silent_move", {
            self.exec_dynamic(cmd(builder::build_silent_move(x, y))?.as_bytes())
        })
    }

    /// Scroll wheel. Positive = up, negative = down.
    pub fn wheel(&self, delta: i32) -> Result<()> {
        timed!("wheel", {
            self.exec_dynamic(cmd(builder::build_wheel(delta))?.as_bytes())
        })
    }
}

impl FireAndForget<'_> {
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        self.send_dynamic(cmd(builder::build_move(x, y))?.as_bytes())
    }

    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        self.send_dynamic(cmd(builder::build_silent_move(x, y))?.as_bytes())
    }

    pub fn wheel(&self, delta: i32) -> Result<()> {
        self.send_dynamic(cmd(builder::build_wheel(delta))?.as_bytes())
    }
}

// -- Async --

#[cfg(feature = "async")]
use super::{AsyncDevice, AsyncFireAndForget};

#[cfg(feature = "async")]
impl AsyncDevice {
    pub async fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        self.exec_dynamic(cmd(builder::build_move(x, y))?.as_bytes())
            .await
    }

    pub async fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        self.exec_dynamic(cmd(builder::build_silent_move(x, y))?.as_bytes())
            .await
    }

    pub async fn wheel(&self, delta: i32) -> Result<()> {
        self.exec_dynamic(cmd(builder::build_wheel(delta))?.as_bytes())
            .await
    }
}

#[cfg(feature = "async")]
impl AsyncFireAndForget<'_> {
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        self.send_dynamic(cmd(builder::build_move(x, y))?.as_bytes())
    }

    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        self.send_dynamic(cmd(builder::build_silent_move(x, y))?.as_bytes())
    }

    pub fn wheel(&self, delta: i32) -> Result<()> {
        self.send_dynamic(cmd(builder::build_wheel(delta))?.as_bytes())
    }
}
