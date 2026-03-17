# makcu

A Rust library for controlling MAKCU USB HID interceptor devices.

MAKCU devices are small USB dongles that sit between a mouse and a host PC, allowing programmatic mouse control — cursor movement, button presses, scroll, input locking, and more — via serial commands over USB.

## Quick start

```rust
use makcu::{Device, Button, Result};

fn main() -> Result<()> {
    let device = Device::connect()?;

    device.move_xy(100, 50)?;
    device.button_down(Button::Left)?;
    device.button_up(Button::Left)?;
    device.wheel(-3)?;

    device.disconnect();
    Ok(())
}
```

## Features

The base crate exposes only firmware-native commands. Optional features add higher-level functionality:

| Feature | Description |
|---------|-------------|
| `async` | `AsyncDevice` with full async parity (requires tokio) |
| `batch` | `BatchBuilder` — fluent command sequencing with coalesced writes |
| `extras` | Software-implemented click, smooth movement, drag, and event callbacks |
| `profile` | Per-command timing profiler (zero overhead when disabled) |
| `mock` | In-process mock transport for testing without hardware |

```toml
[dependencies]
makcu = { version = "0.1", features = ["batch", "extras"] }
```

## API overview

### Connection

```rust
// Auto-detect by USB VID/PID
let device = Device::connect()?;

// Specific port
let device = Device::connect_port("/dev/ttyUSB0")?;

// Custom config
let device = Device::with_config(DeviceConfig {
    fire_and_forget: true,
    reconnect: true,
    ..Default::default()
})?;
```

### Mouse control

```rust
device.move_xy(100, -50)?;           // relative move
device.silent_move(10, 10)?;         // click-move (two HID frames)
device.wheel(3)?;                    // scroll up

device.button_down(Button::Left)?;   // press
device.button_up(Button::Left)?;     // release
device.button_up_force(Button::Left)?; // force-release (unstick)
device.button_state(Button::Left)?;  // query: true/false
```

### Input locks

```rust
device.set_lock(LockTarget::X, true)?;   // lock X axis
device.lock_state(LockTarget::X)?;       // query lock state
device.lock_states_all()?;               // all 7 locks at once
```

### Device info

```rust
device.version()?;       // firmware version string
device.serial()?;        // current serial number
device.set_serial("custom")?;  // spoof serial
device.reset_serial()?;  // restore factory serial
```

### Fire-and-forget

By default, every command waits for the device's `>>> ` response prompt (~1ms round trip). For maximum throughput, use fire-and-forget:

```rust
let ff = device.ff();
ff.move_xy(10, 0)?;   // returns immediately after serial write
ff.wheel(1)?;
```

### Button stream

```rust
device.enable_button_stream()?;
let rx = device.button_events();

// rx.try_recv() returns ButtonMask with per-button accessors
if let Ok(mask) = rx.try_recv() {
    println!("left={} right={}", mask.left(), mask.right());
}

device.disable_button_stream()?;
```

### Batch (feature = `batch`)

Coalesces multiple commands into a single `write_all()` call:

```rust
device.batch()
    .move_xy(10, 0)
    .move_xy(0, 10)
    .button_down(Button::Left)
    .button_up(Button::Left)
    .wheel(1)
    .execute()?;
```

The firmware processes commands sequentially from its serial buffer — batching doesn't skip or merge commands. It just eliminates the inter-command gap on the host side by delivering all the bytes in one write, so the firmware always has the next command ready to read immediately.

### Extras (feature = `extras`)

Software-implemented operations with timing control:

```rust
use std::time::Duration;

device.click(Button::Left, Duration::from_millis(50))?;
device.click_sequence(Button::Left, Duration::from_millis(50), 3, Duration::from_millis(100))?;
device.move_smooth(200, 0, 20, Duration::from_millis(10))?;
device.drag(Button::Left, 100, 0, 10, Duration::from_millis(15))?;
device.move_pattern(&[(100, 0), (0, 100), (-100, 0), (0, -100)], 10, Duration::from_millis(10))?;
```

Event callbacks:

```rust
let _handle = device.on_button_press(Button::Left, |pressed| {
    println!("left button: {}", if pressed { "down" } else { "up" });
});
// Callback unregisters when handle is dropped
```

### Async (feature = `async`)

Full async parity — every sync method has an async equivalent:

```rust
let device = AsyncDevice::connect().await?;
device.move_xy(100, 50).await?;
device.click(Button::Left, Duration::from_millis(50)).await?;
device.batch().move_xy(10, 0).wheel(1).execute().await?;
```

### Profiler (feature = `profile`)

Zero-cost when disabled. Records timing for every command:

```rust
use makcu::profiler;

device.move_xy(100, 0)?;
device.move_xy(-100, 0)?;

for (name, stat) in profiler::stats() {
    println!("{}: {}x avg={}us min={}us max={}us",
        name, stat.count, stat.avg_us as u64, stat.min_us, stat.max_us);
}
profiler::reset();
```

### Mock (feature = `mock`)

Test without hardware:

```rust
let (device, mock) = Device::mock();

// Register responses for query commands
mock.on_command(b"km.version()\r\n", b"km.version()\r\nkm.MAKCU_L_V3.2\r\n>>> ");

let version = device.version()?;
assert_eq!(version, "MAKCU_L_V3.2");

// Inspect what was sent
assert!(mock.sent_commands().iter().any(|c| c == b"km.version()\r\n"));
```

### Raw commands

Escape hatch for firmware commands the library doesn't wrap:

```rust
let response = device.send_raw(b"km.version()\r\n")?;
```

## Examples

```bash
# Basic usage with real hardware
cargo run --example basic

# All features demonstrated
cargo run --example comprehensive --features "async,batch,extras,profile"

# Mock transport (no hardware)
cargo run --example mock --features "mock"

# Performance benchmark
cargo run --example benchmark --release --features "batch,extras"
```

## Architecture

The library uses a multi-threaded transport layer:

- **Writer thread** coalesces pending commands into single `write_all()` calls
- **Reader thread** runs a `StreamParser` state machine that routes responses and fans out button events
- **Monitor thread** handles automatic reconnection with exponential backoff

All `Device` methods take `&self` — I/O goes through channels. `Device` is `Send + Sync` and can be shared via `Arc`.

Communication is at 4 Mbaud over USB-serial (CH340/CH343 chip). The library auto-detects devices by USB VID/PID and handles the baud rate upgrade sequence automatically.

## Performance

All numbers are averages of 3 runs on the same device (Linux, CH340 USB-serial).

| Metric | makcu | makcu-cpp | makcu-rs |
| --- | --- | --- | --- |
| **Baud rate** | 4 Mbaud | 4 Mbaud | 115,200 |
| **What is measured** | Real serial I/O | Serial write+flush | Channel enqueue\* |
| **Confirmed round-trip (move)** | 999 us | N/A | N/A |
| **100 rapid F&F moves** | 1333 us total | 4647 us total | 27 us total\* |
| **Batch 10 cmds** | 16 us total | 470 us total | 3 us total\* |
| **Batch 50 moves** | 12 us total | 2635 us total | 6 us total\* |

\*makcu-rs measures channel enqueue time, not serial I/O — the timer stops before bytes reach the serial port, producing sub-microsecond figures that don't reflect actual device latency.

Run `cargo run --example benchmark --release --features "batch,extras"` to reproduce.

## License

MIT
