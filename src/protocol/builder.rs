use std::io::Write;

use crate::error::{MakcuError, Result};

/// Maximum absolute value for move/silent_move coordinates (firmware limit).
pub const MOVE_RANGE: i32 = 32767;
/// Maximum absolute value for wheel scroll (firmware limit).
pub const WHEEL_RANGE: i32 = 127;

/// Stack-allocated command buffer for parametric commands.
/// Avoids heap allocation on the move/wheel hot path.
pub struct CommandBuf {
    buf: [u8; 64],
    len: usize,
}

impl CommandBuf {
    fn new() -> Self {
        Self {
            buf: [0u8; 64],
            len: 0,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

/// Build `km.move(x,y)\r\n`. Returns an error if coordinates exceed ±32767.
pub fn build_move(x: i32, y: i32) -> Result<CommandBuf> {
    check_move_range(x, "x")?;
    check_move_range(y, "y")?;
    build_cmd(|buf| write!(buf, "km.move({},{})\r\n", x, y))
}

/// Build `km.silent(x,y)\r\n`. Returns an error if coordinates exceed ±32767.
pub fn build_silent_move(x: i32, y: i32) -> Result<CommandBuf> {
    check_move_range(x, "x")?;
    check_move_range(y, "y")?;
    build_cmd(|buf| write!(buf, "km.silent({},{})\r\n", x, y))
}

/// Build `km.wheel(delta)\r\n`. Returns an error if delta exceeds ±127.
pub fn build_wheel(delta: i32) -> Result<CommandBuf> {
    if !(-WHEEL_RANGE..=WHEEL_RANGE).contains(&delta) {
        return Err(MakcuError::OutOfRange {
            value: delta as i64,
            min: -WHEEL_RANGE as i64,
            max: WHEEL_RANGE as i64,
        });
    }
    build_cmd(|buf| write!(buf, "km.wheel({})\r\n", delta))
}

fn check_move_range(v: i32, _axis: &str) -> Result<()> {
    if !(-MOVE_RANGE..=MOVE_RANGE).contains(&v) {
        return Err(MakcuError::OutOfRange {
            value: v as i64,
            min: -MOVE_RANGE as i64,
            max: MOVE_RANGE as i64,
        });
    }
    Ok(())
}

fn build_cmd(f: impl FnOnce(&mut &mut [u8]) -> std::io::Result<()>) -> Result<CommandBuf> {
    let mut cmd = CommandBuf::new();
    let mut buf: &mut [u8] = &mut cmd.buf[..];
    let _ = f(&mut buf);
    cmd.len = fmt_len(&cmd.buf);
    if cmd.len == 0 {
        return Err(MakcuError::Protocol("command too long for buffer".into()));
    }
    Ok(cmd)
}

/// Build `km.serial('value')\r\n`
///
/// Returns an error if the value is too long to fit in the 64-byte command buffer.
/// The maximum value length is ~45 characters.
pub fn build_serial_set(value: &str) -> Result<CommandBuf> {
    // km.serial('')\r\n = 16 bytes overhead, leaving ~48 chars for value
    if value.len() > 45 {
        return Err(MakcuError::Protocol("serial value too long".into()));
    }
    build_cmd(|buf| write!(buf, "km.serial('{}')\r\n", value))
}

/// Find the actual length of the formatted string in the buffer.
fn fmt_len(buf: &[u8; 64]) -> usize {
    // Find the \n that terminates our command
    buf.iter()
        .position(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0)
}
