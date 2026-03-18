use crate::error::Result;
use crate::protocol::builder;
use crate::timed;

use super::{Device, FireAndForget};

impl Device {
    /// Relative mouse move. Coordinates are in HID units, range ±32767.
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        timed!("move_xy", {
            self.exec_dynamic(builder::build_move(x, y)?.as_bytes())
        })
    }

    /// Left-down → move(x,y) → left-up in two HID frames.
    /// Useful for drag-like repositioning without a visible click.
    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        timed!("silent_move", {
            self.exec_dynamic(builder::build_silent_move(x, y)?.as_bytes())
        })
    }

    /// Scroll wheel. Range ±127. Positive = up, negative = down.
    pub fn wheel(&self, delta: i32) -> Result<()> {
        timed!("wheel", {
            self.exec_dynamic(builder::build_wheel(delta)?.as_bytes())
        })
    }
}

impl FireAndForget<'_> {
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        self.send_dynamic(builder::build_move(x, y)?.as_bytes())
    }

    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        self.send_dynamic(builder::build_silent_move(x, y)?.as_bytes())
    }

    pub fn wheel(&self, delta: i32) -> Result<()> {
        self.send_dynamic(builder::build_wheel(delta)?.as_bytes())
    }
}

// -- Async --

#[cfg(feature = "async")]
use super::{AsyncDevice, AsyncFireAndForget};

#[cfg(feature = "async")]
impl AsyncDevice {
    pub async fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        self.exec_dynamic(builder::build_move(x, y)?.as_bytes())
            .await
    }

    /// Left-down → move(x,y) → left-up in two HID frames.
    pub async fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        self.exec_dynamic(builder::build_silent_move(x, y)?.as_bytes())
            .await
    }

    /// Scroll wheel. Range ±127.
    pub async fn wheel(&self, delta: i32) -> Result<()> {
        self.exec_dynamic(builder::build_wheel(delta)?.as_bytes())
            .await
    }
}

#[cfg(feature = "async")]
impl AsyncFireAndForget<'_> {
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        self.send_dynamic(builder::build_move(x, y)?.as_bytes())
    }

    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        self.send_dynamic(builder::build_silent_move(x, y)?.as_bytes())
    }

    pub fn wheel(&self, delta: i32) -> Result<()> {
        self.send_dynamic(builder::build_wheel(delta)?.as_bytes())
    }
}
