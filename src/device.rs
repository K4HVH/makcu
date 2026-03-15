use std::io::Read;
use std::time::{Duration, Instant};

use serialport::SerialPort;

use crate::{Button, ButtonAction, ButtonMask, Error, Response, Result};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const USB_VID: u16 = 0x1A86;
const USB_PID: u16 = 0x55D3;

const BAUD_4M: u32 = 4_000_000;
const BAUD_DEFAULT: u32 = 115_200;

/// Binary frame that switches the device to 4 Mbaud.
/// Format: `DE AD [len_lo len_hi] [cmd] [rate_le_u32]`
const BAUD_CHANGE_FRAME: &[u8] = &[0xDE, 0xAD, 0x05, 0x00, 0xA5, 0x00, 0x09, 0x3D, 0x00];

const TERMINATOR: &[u8] = b">>> ";

/// Timeout for a normal command response.
const CMD_TIMEOUT: Duration = Duration::from_millis(300);

// ---------------------------------------------------------------------------
// Lock
// ---------------------------------------------------------------------------

/// A mouse input axis or button that can be locked.
///
/// Locking prevents the corresponding physical input from being forwarded to
/// the host PC while still allowing software-injected movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lock {
    /// X-axis movement.
    X,
    /// Y-axis movement.
    Y,
    /// Left mouse button.
    Left,
    /// Right mouse button.
    Right,
    /// Middle mouse button.
    Middle,
    /// Side button 1.
    Side1,
    /// Side button 2.
    Side2,
}

fn lock_cmd(lock: Lock) -> &'static str {
    match lock {
        Lock::X => "lock_mx",
        Lock::Y => "lock_my",
        Lock::Left => "lock_ml",
        Lock::Right => "lock_mr",
        Lock::Middle => "lock_mm",
        Lock::Side1 => "lock_ms1",
        Lock::Side2 => "lock_ms2",
    }
}

// ---------------------------------------------------------------------------
// Device
// ---------------------------------------------------------------------------

/// An open connection to a MAKCU device.
///
/// All methods are synchronous / blocking. Construct with [`Device::open`] or
/// [`Device::open_port`].
pub struct Device {
    port: Box<dyn SerialPort>,
}

impl Device {
    // -----------------------------------------------------------------------
    // Connection
    // -----------------------------------------------------------------------

    /// Find and connect to the first available MAKCU device.
    ///
    /// Scans all serial ports for one matching USB VID `0x1A86` / PID `0x55D3`,
    /// tries 4 Mbaud first (device may already be in high-speed mode from a
    /// prior session), then falls back to the baud-rate change sequence.
    pub fn open() -> Result<Self> {
        let port_name = find_port()?;
        Self::open_port(&port_name)
    }

    /// Connect to a specific port (e.g. `"/dev/ttyACM0"` or `"COM3"`).
    ///
    /// Uses the same 4M-first / fallback logic as [`Device::open`].
    pub fn open_port(port_name: &str) -> Result<Self> {
        // Try 4 Mbaud first — device may already be there from a prior session.
        if let Ok(dev) = try_connect(port_name, BAUD_4M) {
            return Ok(dev);
        }
        // Fall back: open at 115200, send the binary baud-change frame, then
        // reopen at 4 Mbaud.
        {
            let mut port = serialport::new(port_name, BAUD_DEFAULT)
                .timeout(Duration::from_millis(100))
                .open()
                .map_err(Error::Port)?;
            port.write_all(BAUD_CHANGE_FRAME)?;
            port.flush()?;
            // Give the device a moment to switch (official guide recommends 100ms).
            std::thread::sleep(Duration::from_millis(100));
        }
        try_connect(port_name, BAUD_4M)
    }

    // -----------------------------------------------------------------------
    // Raw send
    // -----------------------------------------------------------------------

    /// Send a raw command string and return the parsed response.
    ///
    /// The `\r\n` terminator is appended automatically. Most callers should use
    /// the higher-level typed methods instead.
    pub fn send_raw(&mut self, cmd: &str) -> Result<Response> {
        self.port
            .clear(serialport::ClearBuffer::Input)
            .map_err(Error::Port)?;
        let line = format!("{}\r\n", cmd);
        self.port.write_all(line.as_bytes())?;
        self.port.flush()?;
        match read_until_prompt(&mut *self.port, CMD_TIMEOUT) {
            Ok(raw) => parse_response(&raw, cmd),
            Err(Error::Timeout) => Ok(Response::Silent),
            Err(e) => Err(e),
        }
    }

    // -----------------------------------------------------------------------
    // Device info
    // -----------------------------------------------------------------------

    /// Query the firmware version string. Expect `"km.MAKCU"`.
    pub fn version(&mut self) -> Result<String> {
        match self.send_raw("km.version()")? {
            Response::Responded(v) => Ok(v),
            other => Err(Error::Protocol(format!(
                "version() expected a value, got {:?}",
                other
            ))),
        }
    }

    // -----------------------------------------------------------------------
    // Mouse buttons
    // -----------------------------------------------------------------------

    /// Set a button to the given action state.
    ///
    /// - [`ButtonAction::Down`] — force button down (held)
    /// - [`ButtonAction::SilentUp`] — release without overriding physical press
    /// - [`ButtonAction::ForcedUp`] — force release even if physically held
    pub fn button_set(&mut self, button: Button, action: ButtonAction) -> Result<()> {
        self.exec(format!(
            "km.{}({})",
            button_cmd(button),
            action as u8
        ))
    }

    /// Query whether a button is currently pressed. Returns `true` if pressed.
    pub fn button_state(&mut self, button: Button) -> Result<bool> {
        let cmd = format!("km.{}()", button_cmd(button));
        match self.send_raw(&cmd)? {
            Response::Responded(v) => Ok(v.trim() == "1"),
            other => Err(Error::Protocol(format!(
                "button_state() expected a value, got {:?}",
                other
            ))),
        }
    }

    /// Press a button down (forced).
    pub fn press(&mut self, button: Button) -> Result<()> {
        self.button_set(button, ButtonAction::Down)
    }

    /// Silent release — does not override a physically held button.
    pub fn release(&mut self, button: Button) -> Result<()> {
        self.button_set(button, ButtonAction::SilentUp)
    }

    /// Force release a button even if the user is physically holding it.
    pub fn force_release(&mut self, button: Button) -> Result<()> {
        self.button_set(button, ButtonAction::ForcedUp)
    }

    /// Click: press then silent-release.
    pub fn click(&mut self, button: Button) -> Result<()> {
        self.press(button)?;
        self.release(button)
    }

    // -----------------------------------------------------------------------
    // Mouse movement
    // -----------------------------------------------------------------------

    /// Relative mouse move. Coordinates are in HID units, range ±32767.
    pub fn move_xy(&mut self, x: i32, y: i32) -> Result<()> {
        self.exec(format!("km.move({},{})", x, y))
    }

    /// Smooth relative move over `duration_ms` milliseconds.
    ///
    /// The device generates random curve segments with jitter, and always
    /// ends exactly at (x, y).
    pub fn move_smooth(&mut self, x: i32, y: i32, duration_ms: u32) -> Result<()> {
        self.exec(format!("km.move({},{},{})", x, y, duration_ms))
    }

    /// Smooth move with random overshoot.
    ///
    /// `overshoot_pct` is a percentage, e.g. `20` for ±20% overshoot. The
    /// device always ends exactly at (x, y).
    pub fn move_overshoot(
        &mut self,
        x: i32,
        y: i32,
        duration_ms: u32,
        overshoot_pct: u32,
    ) -> Result<()> {
        self.exec(format!(
            "km.move({},{},{},{})",
            x, y, duration_ms, overshoot_pct
        ))
    }

    /// Smooth move with Bezier curve path and random overshoot.
    pub fn move_bezier(
        &mut self,
        x: i32,
        y: i32,
        duration_ms: u32,
        overshoot_pct: u32,
        bezier: u32,
    ) -> Result<()> {
        self.exec(format!(
            "km.move({},{},{},{},{})",
            x, y, duration_ms, overshoot_pct, bezier
        ))
    }

    /// Silent click-move: left-button-down → move → left-button-up, in two
    /// HID frames. Useful for drag operations that should not appear as a
    /// normal click in software.
    pub fn silent_move(&mut self, x: i32, y: i32) -> Result<()> {
        self.exec(format!("km.silent({},{})", x, y))
    }

    // -----------------------------------------------------------------------
    // Scroll wheel
    // -----------------------------------------------------------------------

    /// Scroll the wheel. Positive = up, negative = down (OS convention).
    pub fn wheel(&mut self, steps: i32) -> Result<()> {
        self.exec(format!("km.wheel({})", steps))
    }

    // -----------------------------------------------------------------------
    // Input locks
    // -----------------------------------------------------------------------

    /// Lock or unlock a mouse input. When locked, physical input is suppressed
    /// on the host side while software-injected input continues to work.
    pub fn lock_set(&mut self, lock: Lock, enabled: bool) -> Result<()> {
        let cmd = format!("km.{}({})", lock_cmd(lock), if enabled { 1 } else { 0 });
        self.exec(cmd)
    }

    /// Query whether a lock is currently active.
    pub fn lock_state(&mut self, lock: Lock) -> Result<bool> {
        let cmd = format!("km.{}()", lock_cmd(lock));
        match self.send_raw(&cmd)? {
            Response::Responded(v) => Ok(v.trim() == "1"),
            other => Err(Error::Protocol(format!(
                "lock_state() expected a value, got {:?}",
                other
            ))),
        }
    }

    // -----------------------------------------------------------------------
    // Button monitoring stream
    // -----------------------------------------------------------------------

    /// Enable or disable the button-state-change stream.
    ///
    /// When enabled the device emits a raw bitmask byte (value < 32) each time
    /// any button changes state. Read events with [`Device::read_button_event`].
    pub fn set_button_stream(&mut self, enable: bool) -> Result<()> {
        self.exec(format!("km.buttons({})", if enable { 1 } else { 0 }))
    }

    /// Query whether the button stream is currently enabled.
    pub fn button_stream_enabled(&mut self) -> Result<bool> {
        match self.send_raw("km.buttons()")? {
            Response::Responded(v) => Ok(v.trim() == "1"),
            other => Err(Error::Protocol(format!(
                "buttons() expected a value, got {:?}",
                other
            ))),
        }
    }

    /// Read one button-state-change event from the stream.
    ///
    /// Blocks until a raw bitmask byte (< 32) arrives or `timeout` elapses.
    /// You must call [`Device::set_button_stream`]`(true)` before reading events.
    pub fn read_button_event(&mut self, timeout: Duration) -> Result<ButtonMask> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 1];
        loop {
            if Instant::now() > deadline {
                return Err(Error::Timeout);
            }
            match self.port.read(&mut buf) {
                // Raw bitmask byte — this is what we're waiting for.
                Ok(1) if buf[0] < 32 => return Ok(ButtonMask::from_byte(buf[0])),
                // Prompt or echo bytes — skip.
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Catch (click counter)
    // -----------------------------------------------------------------------

    /// Return the number of physical clicks counted since the last call.
    ///
    /// Returns `0` when no clicks have occurred. Useful for detecting
    /// user input while locks are active.
    pub fn catch(&mut self, button: Button) -> Result<u32> {
        let suffix = match button {
            Button::Left => "ml",
            Button::Right => "mr",
            Button::Middle => "mm",
            Button::Side1 => "ms1",
            Button::Side2 => "ms2",
        };
        let cmd = format!("km.catch_{}()", suffix);
        match self.send_raw(&cmd)? {
            Response::Responded(v) => v.trim().parse::<u32>().map_err(|_| {
                Error::Protocol(format!("catch returned non-integer: {:?}", v))
            }),
            other => Err(Error::Protocol(format!(
                "catch() expected a value, got {:?}",
                other
            ))),
        }
    }

    // -----------------------------------------------------------------------
    // Serial spoofing (hardware-dependent)
    // -----------------------------------------------------------------------

    /// Query the current serial number reported by the connected mouse.
    ///
    /// If the mouse has no serial number, returns a message string from the
    /// device rather than an error.
    pub fn serial_get(&mut self) -> Result<String> {
        match self.send_raw("km.serial()")? {
            Response::Responded(v) => Ok(v),
            other => Err(Error::Protocol(format!(
                "serial() expected a value, got {:?}",
                other
            ))),
        }
    }

    /// Spoof the mouse serial number. Requires hardware support from the
    /// connected mouse — not all mice support serial spoofing.
    pub fn serial_set(&mut self, serial: &str) -> Result<String> {
        let cmd = format!("km.serial('{}')", serial);
        match self.send_raw(&cmd)? {
            Response::Responded(v) => Ok(v),
            other => Err(Error::Protocol(format!(
                "serial('str') expected a value, got {:?}",
                other
            ))),
        }
    }

    /// Reset the spoofed serial back to the factory value.
    pub fn serial_reset(&mut self) -> Result<String> {
        match self.send_raw("km.serial(0)")? {
            Response::Responded(v) => Ok(v),
            other => Err(Error::Protocol(format!(
                "serial(0) expected a value, got {:?}",
                other
            ))),
        }
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Send a command and assert it returns EXECUTED.
    fn exec(&mut self, cmd: String) -> Result<()> {
        match self.send_raw(&cmd)? {
            Response::Executed => Ok(()),
            other => Err(Error::Protocol(format!(
                "command {:?} expected EXECUTED, got {:?}",
                cmd, other
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn button_cmd(button: Button) -> &'static str {
    match button {
        Button::Left => "left",
        Button::Right => "right",
        Button::Middle => "middle",
        Button::Side1 => "ms1",
        Button::Side2 => "ms2",
    }
}

fn find_port() -> Result<String> {
    let ports = serialport::available_ports().map_err(Error::Port)?;
    for port in ports {
        if let serialport::SerialPortType::UsbPort(info) = port.port_type
            && info.vid == USB_VID
            && info.pid == USB_PID
        {
            return Ok(port.port_name);
        }
    }
    Err(Error::NotFound)
}

fn try_connect(port_name: &str, baud: u32) -> Result<Device> {
    let port = serialport::new(port_name, baud)
        .timeout(Duration::from_millis(200))
        .open()
        .map_err(Error::Port)?;
    let mut dev = Device { port };
    // Verify the connection with a version query.
    match dev.send_raw("km.version()") {
        Ok(Response::Responded(v)) if v.contains("km.MAKCU") => Ok(dev),
        Ok(other) => Err(Error::Protocol(format!(
            "unexpected version response: {:?}",
            other
        ))),
        Err(Error::Timeout) => Err(Error::Timeout),
        Err(e) => Err(e),
    }
}

/// Read from `port` until the `>>> ` prompt terminator is found or `timeout` elapses.
fn read_until_prompt(port: &mut dyn SerialPort, timeout: Duration) -> Result<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 64];
    loop {
        if Instant::now() > deadline {
            return Err(Error::Timeout);
        }
        match port.read(&mut tmp) {
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(TERMINATOR.len()).any(|w| w == TERMINATOR) {
                    return Ok(buf);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // Serial port timeout — keep looping until our deadline.
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Parse a raw response buffer into a [`Response`].
fn parse_response(raw: &[u8], sent_cmd: &str) -> Result<Response> {
    // Find the terminator.
    let term_pos = match raw.windows(TERMINATOR.len()).position(|w| w == TERMINATOR) {
        Some(p) => p,
        None => return Ok(Response::Silent),
    };
    let body = &raw[..term_pos];
    // Strip leading/trailing CR, LF, space.
    let body = trim_bytes(body);

    if body.is_empty() {
        return Ok(Response::Executed);
    }

    let text = String::from_utf8_lossy(body);

    // If there's a newline: first line is the echo, second is the return value.
    if let Some(nl) = body.iter().position(|&b| b == b'\n') {
        let value = String::from_utf8_lossy(&body[nl + 1..])
            .trim()
            .to_string();
        return Ok(Response::Responded(value));
    }

    // Single line: if it matches the sent command → EXECUTED (just an echo).
    // Otherwise it's a returned value (e.g. km.version() returns "km.MAKCU"
    // without a separate echo line).
    if text.trim() == sent_cmd.trim() {
        Ok(Response::Executed)
    } else {
        Ok(Response::Responded(text.trim().to_string()))
    }
}

fn trim_bytes(b: &[u8]) -> &[u8] {
    let is_ws = |&x: &u8| x == b'\r' || x == b'\n' || x == b' ';
    let start = b.iter().position(|x| !is_ws(x)).unwrap_or(b.len());
    let end = b.iter().rposition(|x| !is_ws(x)).map(|i| i + 1).unwrap_or(0);
    if start >= end {
        &[]
    } else {
        &b[start..end]
    }
}
