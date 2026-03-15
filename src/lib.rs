//! makcu — Rust library for controlling MAKCU devices.
//!
//! # Quick start
//!
//! ```no_run
//! use makcu::{Device, Button, ButtonAction, Lock};
//!
//! let mut dev = Device::open().expect("MAKCU not found");
//!
//! println!("{}", dev.version().unwrap());
//!
//! dev.press(Button::Left).unwrap();
//! dev.move_xy(100, 50).unwrap();
//! dev.release(Button::Left).unwrap();
//! ```

mod device;

pub use device::{Device, Lock};

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Library error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No MAKCU device found on any available serial port.
    #[error("MAKCU device not found (USB VID=0x1A86 PID=0x55D3)")]
    NotFound,

    /// Device did not respond within the timeout — command not supported or
    /// device is unresponsive.
    #[error("device did not respond (timeout)")]
    Timeout,

    /// Serial port error.
    #[error("serial port error: {0}")]
    Port(#[from] serialport::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Unexpected or malformed response from the device.
    #[error("protocol error: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

/// The three response types the MAKCU protocol can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// Command executed, device echoed the command back with no return value.
    Executed,
    /// Command returned a value (echo + value line).
    Responded(String),
    /// No response within timeout — command is not supported in current firmware.
    Silent,
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
    /// Side button 1 (back).
    Side1,
    /// Side button 2 (forward).
    Side2,
}

// ---------------------------------------------------------------------------
// ButtonAction
// ---------------------------------------------------------------------------

/// The state to set a button to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ButtonAction {
    /// Silent release — does not override the user's own physical press.
    SilentUp = 0,
    /// Force the button down (held).
    Down = 1,
    /// Force the button up, even if the user is physically holding it.
    ForcedUp = 2,
}

// ---------------------------------------------------------------------------
// ButtonMask
// ---------------------------------------------------------------------------

/// Decoded bitmask from a button-stream notification byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ButtonMask {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub side1: bool,
    pub side2: bool,
}

impl ButtonMask {
    /// Decode from the raw bitmask byte sent by the device.
    ///
    /// Bit layout: `bit0`=left, `bit1`=right, `bit2`=middle, `bit3`=side1, `bit4`=side2.
    pub fn from_byte(b: u8) -> Self {
        Self {
            left: b & 0x01 != 0,
            right: b & 0x02 != 0,
            middle: b & 0x04 != 0,
            side1: b & 0x08 != 0,
            side2: b & 0x10 != 0,
        }
    }

    /// Re-encode to the raw bitmask byte.
    pub fn to_byte(self) -> u8 {
        (self.left as u8)
            | ((self.right as u8) << 1)
            | ((self.middle as u8) << 2)
            | ((self.side1 as u8) << 3)
            | ((self.side2 as u8) << 4)
    }
}
