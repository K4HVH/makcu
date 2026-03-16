use crate::error::Result;
use crate::protocol::builder;
use crate::timed;

use super::{Device, FireAndForget};

impl Device {
    /// Relative mouse move. Coordinates are in HID units, range ±32767.
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        timed!("move_xy", {
            let cmd = builder::build_move(x, y);
            self.exec_dynamic(cmd.as_bytes())
        })
    }

    /// Silent click-move: left-down → move → left-up in two HID frames.
    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        timed!("silent_move", {
            let cmd = builder::build_silent_move(x, y);
            self.exec_dynamic(cmd.as_bytes())
        })
    }

    /// Scroll wheel. Positive = up, negative = down.
    pub fn wheel(&self, delta: i32) -> Result<()> {
        timed!("wheel", {
            let cmd = builder::build_wheel(delta);
            self.exec_dynamic(cmd.as_bytes())
        })
    }
}

impl FireAndForget<'_> {
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        let cmd = builder::build_move(x, y);
        self.send_dynamic(cmd.as_bytes())
    }

    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        let cmd = builder::build_silent_move(x, y);
        self.send_dynamic(cmd.as_bytes())
    }

    pub fn wheel(&self, delta: i32) -> Result<()> {
        let cmd = builder::build_wheel(delta);
        self.send_dynamic(cmd.as_bytes())
    }
}

// -- Async --

#[cfg(feature = "async")]
use super::{AsyncDevice, AsyncFireAndForget};

#[cfg(feature = "async")]
impl AsyncDevice {
    pub async fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        let cmd = builder::build_move(x, y);
        self.exec_dynamic(cmd.as_bytes()).await
    }

    pub async fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        let cmd = builder::build_silent_move(x, y);
        self.exec_dynamic(cmd.as_bytes()).await
    }

    pub async fn wheel(&self, delta: i32) -> Result<()> {
        let cmd = builder::build_wheel(delta);
        self.exec_dynamic(cmd.as_bytes()).await
    }
}

#[cfg(feature = "async")]
impl AsyncFireAndForget<'_> {
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()> {
        let cmd = builder::build_move(x, y);
        self.send_dynamic(cmd.as_bytes())
    }

    pub fn silent_move(&self, x: i32, y: i32) -> Result<()> {
        let cmd = builder::build_silent_move(x, y);
        self.send_dynamic(cmd.as_bytes())
    }

    pub fn wheel(&self, delta: i32) -> Result<()> {
        let cmd = builder::build_wheel(delta);
        self.send_dynamic(cmd.as_bytes())
    }
}
