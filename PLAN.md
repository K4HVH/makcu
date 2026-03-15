# Library Plan — `makcu`

## Goals

- Idiomatic Rust: `Result`-based errors, builder patterns, feature flags, zero-cost abstractions
- Full sync and async support behind a feature flag — same API shape, different execution model
- No-lock architecture on hot paths — channels and atomics instead of mutexes
- High performance: pre-built command strings, batched writes, zero-copy parsing
- Automatic connection and reconnection
- Optional profiler — zero overhead when disabled
- Clear separation between firmware-native commands and library-implemented functionality

---

## Feature Flags

| Flag      | Default | Description                                                         |
|-----------|---------|---------------------------------------------------------------------|
| `async`   | off     | Enables `AsyncDevice` and async batch/extras, pulls in tokio        |
| `batch`   | off     | Enables `BatchBuilder`; adds `AsyncBatchBuilder` when `async` is also enabled |
| `extras`  | off     | Library-implemented functionality (click, smooth move, drag, etc.)  |
| `profile` | off     | Per-command timing profiler, zero-cost when disabled                |
| `mock`    | off     | Replaces serial transport with an in-process fake for unit testing  |

The base crate exposes only what the firmware natively supports.
`extras` adds software-implemented conveniences.
`batch` adds the sequenced command builder; it automatically includes extras
commands when `extras` is also enabled, and async variants when `async` is enabled.

---

## Crate Layout

`AsyncDevice` is a thin async wrapper over the same transport layer — it doesn't
warrant a mirrored directory. Instead, each file in `device/`, `batch/`, and
`extras/` contains both sync and async implementations, with the async side gated
by `#[cfg(feature = "async")]`. This gives full async parity everywhere with no
structural duplication.

```text
makcu/
├── Cargo.toml
├── API.md
├── PLAN.md
└── src/
    ├── lib.rs                    — public re-exports, feature gates
    │
    ├── types/                    — shared data types, no logic
    │   ├── mod.rs
    │   ├── button.rs             — Button enum, ButtonMask
    │   ├── lock.rs               — LockTarget enum, LockStates
    │   └── device_info.rs        — DeviceInfo, ConnectionState
    │
    ├── error/                    — MakcuError, MakcuResult
    │   └── mod.rs
    │
    ├── protocol/                 — wire format only, no I/O
    │   ├── mod.rs
    │   ├── constants.rs          — static command byte slices, BAUD_FRAME_4M
    │   ├── builder.rs            — stack-allocated parametric command formatting
    │   └── parser.rs             — response classifier, button event detection
    │
    ├── transport/                — internal only, never pub
    │   ├── mod.rs                — SerialTransport trait; real impl selected by feature
    │   ├── serial.rs             — real serialport backend (default)
    │   ├── mock.rs               — in-process fake backend  [feature = "mock"]
    │   ├── reader.rs             — reader thread + state machine
    │   ├── writer.rs             — writer thread, payload coalescing
    │   └── monitor.rs            — reconnection monitor, backoff logic
    │
    ├── device/                   — Device (sync) + AsyncDevice  [feature = "async"]
    │   ├── mod.rs                — both structs, DeviceConfig, connect/disconnect
    │   ├── buttons.rs            — button_down/up/force/state  [sync + async]
    │   ├── movement.rs           — move_xy, silent_move, wheel  [sync + async]
    │   ├── locks.rs              — set_lock, lock_state, lock_states_all  [sync + async]
    │   ├── stream.rs             — button stream enable/disable/subscribe  [sync + async]
    │   └── info.rs               — version, serial get/set/reset  [sync + async]
    │
    ├── batch/                    — feature = "batch"  [sync + async]
    │   ├── mod.rs
    │   └── builder.rs            — BatchBuilder + AsyncBatchBuilder  [cfg(async)]
    │                               Extras commands included when feature = "extras"
    │
    ├── extras/                   — feature = "extras"  [sync + async throughout]
    │   ├── mod.rs
    │   ├── click.rs              — click, click_sequence  [sync + async]
    │   ├── smooth.rs             — move_smooth, move_pattern, drag  [sync + async]
    │   └── events.rs             — on_button_press, on_button_event, EventHandle  [sync + async]
    │
    └── profiler/                 — feature = "profile"
        └── mod.rs                — CommandStat, record(), stats(), reset(), timed! macro
```

### How async parity works in practice

Each file in `device/` has two `impl` blocks — one unconditional, one cfg-gated:

```rust
// buttons.rs

impl Device {
    pub fn button_down(&self, button: Button) -> Result<()> { ... }
}

#[cfg(feature = "async")]
impl AsyncDevice {
    pub async fn button_down(&self, button: Button) -> Result<()> { ... }
}
```

Same pattern in `batch/builder.rs` and every file under `extras/`. The transport
layer is shared — `AsyncDevice` holds the same internal handle as `Device` and
just calls `tokio::time::sleep` instead of `std::thread::sleep` for timing, and
awaits channel operations instead of blocking on them.

---

## Types (`types.rs`)

```rust
pub enum Button { Left, Right, Middle, Side1, Side2 }

pub enum LockTarget { X, Y, Left, Right, Middle, Side1, Side2 }

/// Full button state snapshot emitted on every km. stream event.
pub struct ButtonMask(u8);

impl ButtonMask {
    pub fn left(&self) -> bool
    pub fn right(&self) -> bool
    pub fn middle(&self) -> bool
    pub fn side1(&self) -> bool
    pub fn side2(&self) -> bool
    pub fn is_pressed(&self, button: Button) -> bool
    pub fn raw(&self) -> u8
}

pub struct DeviceInfo {
    pub port: String,
    pub firmware: String,   // from km.version()
}

/// Snapshot of all seven lock states, returned by lock_states_all().
pub struct LockStates {
    pub x: bool, pub y: bool,
    pub left: bool, pub right: bool, pub middle: bool,
    pub side1: bool, pub side2: bool,
}
```

---

## Error Handling (`error.rs`)

```rust
#[derive(Debug, thiserror::Error)]
pub enum MakcuError {
    #[error("not connected")]
    NotConnected,
    #[error("port error: {0}")]
    Port(#[from] serialport::Error),
    #[error("command timed out")]
    Timeout,
    #[error("device not found")]
    NotFound,
    #[error("disconnected")]
    Disconnected,
}

pub type Result<T> = std::result::Result<T, MakcuError>;
```

---

## Protocol layer (`protocol/`)

All command strings are compile-time constants or minimal stack-allocated formatting.
No heap `String` allocation on hot paths.

```rust
// Static commands (no args)
pub const CMD_VERSION:    &[u8] = b"km.version()\r\n";
pub const CMD_BUTTONS_ON: &[u8] = b"km.buttons(1)\r\n";
// ... all no-arg commands

// Binary baud-change frame — sent as-is, no CRLF
pub const BAUD_FRAME_4M: &[u8] = &[0xDE, 0xAD, 0x05, 0x00, 0xA5, 0x00, 0x09, 0x3D, 0x00];

// Parametric commands use a small stack buffer (ArrayString or write! to [u8; N])
// to avoid heap allocation in the move/wheel hot path
```

### Response parsing

```text
Read bytes until ">>> " seen or timeout expires.
Button events: any byte immediately following "km." prefix in the stream,
               accepted unconditionally (handles 0x0A, 0x0D combinations).
Command response: strip echo line, extract value line if present.
```

---

## Transport layer (`transport/`)

Internal only. Two threads (or async tasks) per open device:

### Reader thread

- Tight read loop, feeds a state machine:
  - Tracks `km.` prefix in stream — next byte after it is always a button mask
  - Accumulates bytes until `>>>` + space — marks a complete command response
- Button events sent via `broadcast::Sender<ButtonMask>`
- Command responses routed to the waiting caller via a `oneshot` stored alongside
  the pending send

### Writer thread

- Receives byte payloads from an `mpsc` channel
- Drains all pending payloads each loop tick and coalesces into a single `write_all`
- Reduces syscall count under rapid-fire commands

### Connection state

- `AtomicU8` enum: `Disconnected | Connecting | Connected`
- No mutex on the hot read path

### Reconnection monitor

- Lightweight background thread that parks while connected
- On disconnection: exponential backoff reconnect attempts (100ms → 5s cap)
- Notifies subscribers via `watch::Sender<ConnectionState>`

---

## Fire-and-Forget vs Confirmed

The firmware developer recommends against fire-and-forget — commands can queue
faster than the device processes them and responses become misaligned.
**Confirmed is the default.** F&F is opt-in.

Two ways to opt in:

**Per-command:** a `.ff()` modifier on the device handle returns a wrapper that
sends without waiting for the `>>>` prompt.

```rust
device.move_xy(10, 20)?;          // confirmed (default) — waits for >>>
device.ff().move_xy(10, 20)?;     // fire-and-forget — returns immediately
```

**Global flag on DeviceConfig:** `fire_and_forget: bool` — when true, all
commands behave as F&F without needing the modifier. Useful for callers who
have profiled and decided they want maximum throughput and can tolerate the risk.

```rust
let device = Device::with_config(DeviceConfig {
    fire_and_forget: true,
    ..Default::default()
})?;
```

Under the hood, F&F simply skips registering a `oneshot` for the response and
returns `Ok(())` immediately after writing to the transport channel.

---

## Device API (`device.rs`)

```rust
pub struct Device { /* opaque */ }

impl Device {
    // ── Connection ──────────────────────────────────────────────────────────
    pub fn connect() -> Result<Self>
    pub fn connect_port(port: &str) -> Result<Self>
    pub fn with_config(cfg: DeviceConfig) -> Result<Self>
    pub fn disconnect(&mut self)
    pub fn is_connected(&self) -> bool
    pub fn connection_events(&self) -> watch::Receiver<ConnectionState>

    // Returns a wrapper where all commands are fire-and-forget
    pub fn ff(&self) -> FireAndForget<'_>

    // ── Device info ──────────────────────────────────────────────────────────
    pub fn version(&self) -> Result<String>
    pub fn serial(&self) -> Result<String>
    pub fn set_serial(&self, value: &str) -> Result<String>
    pub fn reset_serial(&self) -> Result<String>

    // ── Buttons (firmware-native) ─────────────────────────────────────────────
    pub fn button_down(&self, button: Button) -> Result<()>
    pub fn button_up(&self, button: Button) -> Result<()>
    pub fn button_up_force(&self, button: Button) -> Result<()>   // km.left(2)
    pub fn button_state(&self, button: Button) -> Result<bool>

    // ── Movement (firmware-native) ────────────────────────────────────────────
    pub fn move_xy(&self, x: i32, y: i32) -> Result<()>
    pub fn silent_move(&self, x: i32, y: i32) -> Result<()>
    pub fn wheel(&self, delta: i32) -> Result<()>

    // ── Locks (firmware-native) ───────────────────────────────────────────────
    pub fn set_lock(&self, target: LockTarget, locked: bool) -> Result<()>
    pub fn lock_state(&self, target: LockTarget) -> Result<bool>
    pub fn lock_states_all(&self) -> Result<LockStates>   // queries all 7 locks in one call

    // ── Button stream (firmware-native) ───────────────────────────────────────
    pub fn enable_button_stream(&self) -> Result<()>
    pub fn disable_button_stream(&self) -> Result<()>
    pub fn button_events(&self) -> broadcast::Receiver<ButtonMask>

    // ── Batching ─────────────────────────────── feature = "batch" ───────────
    pub fn batch(&self) -> BatchBuilder<'_>
}
```

### DeviceConfig

```rust
pub struct DeviceConfig {
    pub port: Option<String>,           // None = auto-detect by VID/PID
    pub try_4m_first: bool,             // default: true
    pub command_timeout: Duration,      // default: 500ms
    pub reconnect: bool,                // default: true
    pub reconnect_backoff: Duration,    // initial backoff, default: 100ms
    pub fire_and_forget: bool,          // default: false
}
```

---

## Async Device (feature = `async`)

`AsyncDevice` lives alongside `Device` in `device/` — same files, same transport,
just `async fn` on the surface. The only real differences:

- Channel `.recv()` / `.send()` are awaited rather than blocked on
- Timing in `extras` uses `tokio::time::sleep` instead of `std::thread::sleep`
- `tokio_serial` used for the underlying port in async mode

```rust
// defined in device/mod.rs, impls spread across device/*.rs
pub struct AsyncDevice { /* same internal handle as Device */ }

impl AsyncDevice {
    pub async fn connect() -> Result<Self>
    pub async fn disconnect(&mut self)
    pub fn ff(&self) -> AsyncFireAndForget<'_>
    pub fn batch(&self) -> AsyncBatchBuilder<'_>
    // all Device methods mirrored as async fn, defined in the same files
}
```

---

## Batch System (`batch/`, feature = `batch`)

A fluent command sequence builder. Collects commands and executes them in order
on `.execute()`. No per-command `Result` — one `Result<()>` at the end.

Queries (`button_state`, `lock_state`, `version`) are not supported — they return
values and must be handled individually.

### Execution model

Firmware-native commands (no timing) are coalesced into as few `write_all` calls
as possible. Extras commands that require timing (`click`, `move_smooth`, `drag`)
execute in-place when encountered in the sequence, breaking the coalesce window.
The builder transparently handles both — the caller just chains what they want.

### Commands available

Always available (firmware-native EXECUTED commands):

```rust
pub struct BatchBuilder<'d> { /* device ref + internal step list */ }

impl<'d> BatchBuilder<'d> {
    pub fn move_xy(self, x: i32, y: i32) -> Self
    pub fn silent_move(self, x: i32, y: i32) -> Self
    pub fn button_down(self, button: Button) -> Self
    pub fn button_up(self, button: Button) -> Self
    pub fn button_up_force(self, button: Button) -> Self
    pub fn wheel(self, delta: i32) -> Self
    pub fn set_lock(self, target: LockTarget, locked: bool) -> Self

    pub fn execute(self) -> Result<()>
}
```

Additional methods available when `extras` is also enabled:

```rust
// with feature = "extras"
impl<'d> BatchBuilder<'d> {
    pub fn click(self, button: Button, hold: Duration) -> Self
    pub fn click_sequence(self, button: Button, hold: Duration, count: u32, interval: Duration) -> Self
    pub fn move_smooth(self, x: i32, y: i32, steps: u32, interval: Duration) -> Self
    pub fn move_pattern(self, waypoints: Vec<(i32, i32)>, steps: u32, interval: Duration) -> Self
    pub fn drag(self, button: Button, x: i32, y: i32, steps: u32, interval: Duration) -> Self
}
```

When `async` is also enabled, `AsyncBatchBuilder` mirrors the above with
`async fn execute()`.

---

## Extras (`extras/`, feature = `extras`)

Library-implemented functionality — not native firmware commands. Behind `extras`
so the base crate stays minimal and the boundary is explicit.

### `extras/click.rs`

```rust
impl Device {
    /// Press + hold + release. hold is the delay between down and up.
    pub fn click(&self, button: Button, hold: Duration) -> Result<()>

    /// Repeated clicks with a delay between each press+release cycle.
    /// count: number of clicks. interval: delay between each click.
    pub fn click_sequence(&self, button: Button, hold: Duration, count: u32, interval: Duration) -> Result<()>
}
```

### `extras/smooth.rs`

```rust
impl Device {
    /// Smooth movement via repeated km.move calls.
    /// steps: number of individual moves sent.
    /// interval: delay between each move.
    /// Total distance is divided evenly across steps, remainder added to last step.
    pub fn move_smooth(&self, x: i32, y: i32, steps: u32, interval: Duration) -> Result<()>

    /// move_smooth with the given button held throughout (drag).
    pub fn drag(&self, button: Button, x: i32, y: i32, steps: u32, interval: Duration) -> Result<()>

    /// Navigate through a list of relative waypoints in sequence.
    /// Each waypoint is moved to with move_smooth using the given steps and interval.
    pub fn move_pattern(&self, waypoints: &[(i32, i32)], steps: u32, interval: Duration) -> Result<()>
}
```

### `extras/events.rs`

```rust
impl Device {
    /// Register a callback fired whenever the given button changes state.
    /// Spawns an internal listener thread that diffs the ButtonMask stream.
    /// Returns a handle that unregisters the callback when dropped.
    pub fn on_button_press<F>(&self, button: Button, f: F) -> EventHandle
    where
        F: Fn(bool) + Send + 'static

    /// Same but fires on any button state change with the full mask.
    pub fn on_button_event<F>(&self, f: F) -> EventHandle
    where
        F: Fn(ButtonMask) + Send + 'static
}

/// Dropping this handle unregisters the callback.
pub struct EventHandle { /* opaque */ }
```

Async equivalents are co-located in the same file, gated by `#[cfg(feature = "async")]`.

---

## Profiler (`profiler.rs`, feature = `profile`)

Zero-cost when disabled — all invocations are `#[inline(always)]` no-ops that
the compiler eliminates entirely.

```rust
pub struct CommandStat {
    pub count:    u64,
    pub total_us: u64,
    pub avg_us:   f64,
    pub min_us:   u64,
    pub max_us:   u64,
}

pub fn record(command: &'static str, elapsed: Duration);
pub fn stats() -> HashMap<&'static str, CommandStat>;
pub fn reset();

// Convenience macro — compiles to nothing when feature is off
// timed!("move_xy", device.move_xy(x, y))
```

---

## Connection Sequence

```text
1. Find port: scan for VID=0x1A86 / PID=0x55D3, or use config.port
2. Open at 4 Mbaud, send km.version()\r\n
3. Response contains "km.MAKCU" → connected, go to step 6
4. Open at 115200, write BAUD_FRAME_4M (9 bytes, no CRLF), wait 100ms, close
5. Open at 4 Mbaud, flush, send km.version()\r\n, verify response
6. Spawn reader + writer threads/tasks
7. If config.reconnect: start reconnection monitor thread/task
```

---

## What We Are NOT Doing

- No `#id` response tracking — firmware ignores suffixes after `)`
- No lock state cache — always query device to avoid stale state bugs
- No auto-enabling `km.buttons(1)` on connect — caller opts in explicitly
- No firmware smooth/bezier — broken; `extras` implements smooth move in software
- No `km.click()` — broken; `extras` implements click as press + delay + release
- No `km.catch_*()` — broken in current firmware

---

## Resolved Design Decisions

- **move_smooth API** — `steps` + `interval` explicitly. Caller has full control;
  the library divides total distance evenly across steps with the remainder on the last.

- **Button callbacks** — raw `broadcast::Receiver<ButtonMask>` on the base device.
  Higher-level `on_button_press(Button, fn)` / `on_button_event(fn)` in `extras/events.rs`.

- **Reconnect state** — do nothing. On reconnection the library fires a
  `ConnectionState::Reconnected` event and stops there. The application is
  responsible for re-enabling locks, the button stream, or anything else it needs.
  No hidden state restoration.

---

## Feature Comparison

Legend: ✓ correct  ~ partial/limited  ✗ absent  ⚠ present but wrong/broken

| Feature | makcu-cpp (C++) | makcu-rs (Rust, 3rd party) | **makcu** (ours) |
|---------|:-:|:-:|:-:|
| **Connection** |
| Auto-detect device by VID/PID | ✓ | ✗ manual port only | ✓ |
| Try 4 Mbaud first | ✗ always sends baud frame | ✗ stays at 115200 | ✓ |
| Auto-reconnect | ✓ callback | ✗ | ✓ watch channel |
| **Protocol correctness** |
| Response terminator (`>>>` + space) | ⚠ uses `#id` suffix (ignored by firmware) | ⚠ uses `#id` suffix (ignored by firmware) | ✓ |
| Zero-arg button = state query, not click | ✓ | ✗ treats as click | ✓ |
| Button wire names (ms1/ms2) | ✓ | ~ unverified | ✓ |
| Button stream: `km.` prefix parsing | ✓ | ~ unverified | ✓ handles 0x0A/0x0D |
| Auto-enable button stream on connect | ⚠ yes (should not) | ✗ | ✓ explicit opt-in only |
| **Firmware commands** |
| Button down / up / force-release | ✓ | ~ no force-release | ✓ |
| Button state query | ✓ | ✗ | ✓ |
| Relative move | ✓ | ✓ | ✓ |
| Silent move | ✗ | ✗ | ✓ |
| Scroll wheel | ✓ | ✓ | ✓ |
| Input locks (all 7) | ✓ | ✓ | ✓ |
| Lock state query | ✓ cached (can go stale) | ✗ | ✓ always queries device |
| Lock states — all at once | ✓ cached map | ✗ | ✓ 7 live queries |
| Button event stream | ✓ | ~ callback only | ✓ broadcast channel |
| Serial spoofing | ✓ get/set/reset | ~ set only | ✓ get/set/reset |
| `km.catch_*` | ⚠ exposed (broken firmware) | ✗ | ✗ not exposed |
| **Extras (software-implemented)** |
| Click (press + delay + release) | ✓ | ⚠ uses `km.click()` (broken) | ✓ extras feature |
| Click sequence (repeated clicks) | ✓ | ✗ | ✓ extras feature |
| Smooth movement (software) | ⚠ uses broken firmware smooth | ✗ | ✓ extras feature |
| Bezier movement | ⚠ uses broken firmware bezier | ✗ | ✗ not planned |
| Drag | ✓ | ✗ | ✓ extras feature |
| Move pattern (waypoint list) | ✓ | ✗ | ✓ extras feature |
| Button press/release callbacks | ✓ | ✓ | ✓ extras feature |
| **Architecture** |
| Language | C++ | Rust | Rust |
| Async support | ~ std::future (connect only) | ✓ tokio, full parity | ✓ tokio, full parity |
| Batch command builder | ✓ | ✓ sync + async | ✓ sync + async, feature flag |
| Fire-and-forget | ✓ default for move commands | ✓ default | ~ opt-in via `.ff()` |
| No-lock hot path | ✗ mutex-based | ~ | ✓ channels + atomics |
| Payload coalescing (batched writes) | ✗ | ✗ | ✓ writer thread drains queue |
| Performance profiler | ✓ static, always-on | ~ optional feature | ✓ zero-cost when off |
| Mock transport for testing | ✗ | ✓ mockserial feature | ✓ mock feature |
| C API | ✓ full wrapper | ✗ | ✗ not planned |
| **Error handling** |
| Style | Exceptions | `Result<T, MakcuError>` | `Result<T, MakcuError>` |
| Typed error variants | ~ exception hierarchy | ✓ | ✓ |
