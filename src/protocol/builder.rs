use std::io::Write;

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

/// Build `km.move(x,y)\r\n`. Returns `None` if the formatted command
/// exceeds the 64-byte stack buffer (only possible with extreme values).
pub fn build_move(x: i32, y: i32) -> Option<CommandBuf> {
    let mut cmd = CommandBuf::new();
    let _ = write!(&mut cmd.buf[..], "km.move({},{})\r\n", x, y);
    cmd.len = fmt_len(&cmd.buf);
    if cmd.len == 0 {
        return None;
    }
    Some(cmd)
}

/// Build `km.silent(x,y)\r\n`.
pub fn build_silent_move(x: i32, y: i32) -> Option<CommandBuf> {
    let mut cmd = CommandBuf::new();
    let _ = write!(&mut cmd.buf[..], "km.silent({},{})\r\n", x, y);
    cmd.len = fmt_len(&cmd.buf);
    if cmd.len == 0 {
        return None;
    }
    Some(cmd)
}

/// Build `km.wheel(delta)\r\n`.
pub fn build_wheel(delta: i32) -> Option<CommandBuf> {
    let mut cmd = CommandBuf::new();
    let _ = write!(&mut cmd.buf[..], "km.wheel({})\r\n", delta);
    cmd.len = fmt_len(&cmd.buf);
    if cmd.len == 0 {
        return None;
    }
    Some(cmd)
}

/// Build `km.serial('value')\r\n`
///
/// Returns `None` if the value is too long to fit in the 64-byte command buffer.
/// The maximum value length is ~45 characters.
pub fn build_serial_set(value: &str) -> Option<CommandBuf> {
    // km.serial('')\r\n = 16 bytes overhead, leaving ~48 chars for value
    if value.len() > 45 {
        return None;
    }
    let mut cmd = CommandBuf::new();
    let _ = write!(&mut cmd.buf[..], "km.serial('{}')\r\n", value);
    cmd.len = fmt_len(&cmd.buf);
    if cmd.len == 0 {
        return None;
    }
    Some(cmd)
}

/// Find the actual length of the formatted string in the buffer.
fn fmt_len(buf: &[u8; 64]) -> usize {
    // Find the \n that terminates our command
    buf.iter()
        .position(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0)
}
