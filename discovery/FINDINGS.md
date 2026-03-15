# MAKCU API Findings

**Sources of truth (in priority order):**
1. `discovery/results/verify_20260315_100500.json` — manual physical verification, 86 tests
2. `discovery/results/probe_20260315_003455.json` — automated protocol probe
3. Discord firmware changelog (`sources/discord/v3-firmware_channel.md`, `makcu-api_channel.md`)
4. makcu-cpp source (`/home/wardy/Documents/makcu-stuff/makcu-cpp/`)
5. makcu-rs source (unmaintained, several wrong assumptions — see notes)
6. v3.9 API page (`sources/api-page-analysis.md`) — cross-reference only, not our firmware

Firmware under test: v3.2 (left chip) / v3.7 (right chip) at 4 Mbaud on `/dev/ttyACM0`.

---

## Hardware Architecture

MAKCU is a USB passthrough device with **two independent ESP32-S3 microcontrollers**
and three USB ports.

```
[USB 1 — Left ]  →  Gaming PC / Console        (left chip,  v3.2)
[USB 2 — COM  ]  →  Our software               (serial command interface)
[USB 3 — Right]  →  Mouse or Keyboard          (right chip, v3.7)
```

- **COM port** (USB 2) is the only port our library connects to.
- Only one application can hold the COM port at a time.
- The device cannot function without a peripheral connected to USB 3.

**LED indicators:**
- Solid ON: side functioning correctly
- Solid OFF: no peripheral detected
- Slow blink (500ms): reconfiguring / warning
- Left LED at startup: 1 flash = 115200 baud, 4 flashes = 4 Mbps

---

## Transport

| Property | Value |
|----------|-------|
| Interface | Serial over USB (CDC / virtual COM port) |
| USB chip | WCH CH340 / CH343 |
| Device name | `USB-Enhanced-SERIAL CH343` / `USB Single Serial` |
| USB VID | `0x1A86` |
| USB PID | `0x55D3` |
| Default baud | 115200 |
| High-speed baud | 4000000 (4 Mbaud) |
| Line terminator | `\r\n` |

---

## Connection Sequence

1. **Try 4 Mbaud first** — device may already be there from a previous session
2. If that fails: open at 115200, send binary baud change frame, wait 100ms, close, reopen at 4 Mbaud
3. Verify with `km.version()` — expect `km.MAKCU` in response

### Baud rate change frame (binary, no CRLF)

```
DE AD 05 00 A5 00 09 3D 00
```

```
Offset  Len  Bytes        Description
0       2    DE AD        Magic header
2       2    05 00        Payload length (LE u16) = 5
4       1    A5           Command: baud rate change
5       4    00 09 3D 00  Target baud rate (LE u32) = 4,000,000
```

Baud rate is **not persistent** by default — resets to 115200 on power cycle.
(Persistence requires a physical button press on the device, not software-controllable.)

---

## ASCII Command Protocol

Format: `km.<name>(<args>)\r\n`

**There is no `#id` tracking mechanism.** The device ignores anything appended after
the closing parenthesis. Do not use tracking IDs.

### Response format

All responses terminate with `>>> ` (the interactive prompt). Three cases:

| Response | Meaning | Library status |
|----------|---------|----------------|
| `<echo>\r\n>>> ` | Command executed, no return value | `EXECUTED` |
| `<echo>\n<value>\r\n>>> ` | Command returned a value | `RESPONDED` |
| *(nothing within timeout)* | Command not recognised / not supported | `SILENT` |

For `km.version()` specifically: response is `km.MAKCU\r\n>>> ` (value only, no separate echo line).

---

## CONFIRMED WORKING COMMANDS

### Device info

| Command | Response | Notes |
|---------|----------|-------|
| `km.version()` | `km.MAKCU` | Identifies firmware |

### Mouse buttons

Zero-arg form **queries** button state (returns `0` or `1`). It does NOT click.

| Command | Type | Notes |
|---------|------|-------|
| `km.left()` | RESPONDED → `0`/`1` | Query physical+software state |
| `km.left(1)` | EXECUTED | Forced down |
| `km.left(0)` | EXECUTED | Silent up (does not override user's physical press) |
| `km.left(2)` | EXECUTED | Forced up (overrides physical state) — physically verified |

Same for `km.right()`, `km.middle()`, `km.ms1()`, `km.ms2()`.

**`km.side1()` / `km.side2()` are NOT valid names.** The firmware does not
recognise them at all. Use `km.ms1()` / `km.ms2()`.

### Mouse movement

| Command | Type | Notes |
|---------|------|-------|
| `km.move(x,y)` | EXECUTED | Relative move, range ±32767 — **use this** |

### Silent move

| Command | Type | Notes |
|---------|------|-------|
| `km.silent(x,y)` | EXECUTED | left-down → move → left-up in two HID frames |

### Scroll wheel

| Command | Type | Notes |
|---------|------|-------|
| `km.wheel(n)` | EXECUTED | Any integer accepted; no clamping up to ±127 |

### Input locks

All lock set/clear commands are EXECUTED. All lock queries (no-arg) are RESPONDED.
Lock query returns `0` (unlocked) or `1` (locked). All locks physically verified.

| Command | Type | Return |
|---------|------|--------|
| `km.lock_mx(1/0)` | EXECUTED | — |
| `km.lock_mx()` | RESPONDED | `0` or `1` |
| `km.lock_my(1/0)` | EXECUTED | — |
| `km.lock_my()` | RESPONDED | `0` or `1` |
| `km.lock_ml(1/0)` | EXECUTED | — |
| `km.lock_ml()` | RESPONDED | `0` or `1` |
| `km.lock_mr(1/0)` | EXECUTED | — |
| `km.lock_mr()` | RESPONDED | `0` or `1` |
| `km.lock_mm(1/0)` | EXECUTED | — |
| `km.lock_mm()` | RESPONDED | `0` or `1` |
| `km.lock_ms1(1/0)` | EXECUTED | — |
| `km.lock_ms1()` | RESPONDED | `0` or `1` |
| `km.lock_ms2(1/0)` | EXECUTED | — |
| `km.lock_ms2()` | RESPONDED | `0` or `1` |

Lock query returns `0`/`1` only — not `0`–`3` as the v3.9 docs suggest.

### Button monitoring stream

| Command | Type | Return |
|---------|------|--------|
| `km.buttons(1)` | EXECUTED | Enable stream |
| `km.buttons(0)` | EXECUTED | Disable stream |
| `km.buttons()` | RESPONDED | `0` or `1` (enabled state) |

When enabled, device sends button state changes as raw bytes (bitmask < 32, not CR/LF).
Physically verified — events captured for left, side1, side2 buttons with correct masks.

**Button bitmask:**
```
Bit 0  0x01  Left
Bit 1  0x02  Right
Bit 2  0x04  Middle
Bit 3  0x08  Side1 (ms1)
Bit 4  0x10  Side2 (ms2)
Bit pattern 0x00 = all released
```

### Serial spoofing

| Command | Type | Return |
|---------|------|--------|
| `km.serial()` | RESPONDED | Current serial or `km.Mouse does not have a serial number` |
| `km.serial('string')` | RESPONDED | Message (spoofing has no effect if mouse doesn't support it) |
| `km.serial(0)` | RESPONDED | Message (same) |

**Serial spoofing requires hardware support from the connected mouse.**
If the mouse has no serial number slot, always returns `km.Mouse does not have a serial number`.

---

## NOT IN CURRENT FIRMWARE

These commands produced no response (SILENT) on v3.2/v3.7 in physical testing,
or are confirmed broken by the firmware dev.

- **Smooth / bezier movement** — `km.move(x,y,segments)`, `km.move(x,y,segments,ctrl_x,ctrl_y)` — EXECUTED but broken; only work in some directions, fail silently on others (e.g. diagonals). Confirmed broken by firmware developer. Use `km.move(x,y)` only.
- **Click shorthand** — `km.click(button)` and all variants — SILENT, produces no click
- **Absolute positioning** — `km.screen()`, `moveto()`, `getpos()` — all SILENT
- **Axis streaming** — `km.axis()` and variants — SILENT
- **Raw mouse frame** — `km.mo(...)` — SILENT
- **Remap / invert / swap** — `km.remap_button()`, `km.invert_x()`, `km.swap_xy()` etc. — all SILENT
- **Turbo** — `km.turbo()` and variants — SILENT
- **Keyboard** — `km.down()`, `km.up()`, `km.press()`, `km.string()` etc. — not in this firmware
- **Scroll extras** — `km.pan()`, `km.tilt()` — SILENT
- **Mouse streaming** — `km.mouse()` and variants — SILENT
- **Device management** — `km.info()`, `km.device()`, `km.fault()`, `km.baud()`, `km.echo()`, `km.log()`, `km.bypass()`, `km.led()`, `km.release()` — all SILENT
- **Extended locks** (v3.6+) — `km.lock_mw()`, `km.lock_mx+()`, `km.lock_my-()` etc. — SILENT on v3.2/v3.7
- **Catch / click counter** — `km.catch_ml()` etc. — RESPONDED but always returns `0`. Commands parse correctly but the counter never increments regardless of lock state or click count. Confirmed broken via both raw serial (Python) and makcu-cpp. Broken in current firmware with no fix available.

---

## Cross-Reference: Other Libraries

### makcu-cpp

**What it gets right:**
- USB VID/PID detection
- Binary baud change frame (correct format)
- Button command names: uses `ms1`/`ms2` correctly
- Implements `click()` as separate press+release (knows `km.click()` doesn't work)
- `km.catch_*()`, `km.serial()`, `km.buttons()` all present

**Issues to avoid in our library:**
- Always sends baud change frame — never tries 4M first. Fails if device is already at 4M from a prior session.
- Auto-enables `km.buttons(1)` on every connect without caller asking.
- Lock state cached in library rather than queried from device — can get out of sync.

### makcu-rs

Third-party, unmaintained. Does not implement `km.catch_*`.

**What it gets wrong:**
- `#id` tracking is completely broken — device ignores suffix after `)`.
- No baud rate switch — stays at 115200.
- Zero-arg button form treated as click — it's actually a state query returning `0`/`1`.

---

## Key Corrections (vs Prior Assumptions)

1. **Zero-arg `km.left()` QUERIES state, does NOT click.** Returns `0` or `1`.
2. **No `#id` response tracking.** Device ignores any suffix after `)`.
3. **`km.click(...)` is physically non-functional** — not just silent, produces no click.
4. **`moveto` / `getpos` / `km.screen` are silent** and not in current firmware.
5. **Lock queries return `0` or `1` only** (not 0-3 as v3.9 docs suggest).
6. **`km.wheel` is NOT clamped** — ±127 physically scrolled further than ±1.
7. **Device may already be at 4 Mbaud** — always try 4M first before sending baud frame.
8. **Serial spoofing is mouse-hardware-dependent** — command works but has no effect on mice without a serial number slot.
9. **Smooth and bezier movement are broken** — confirmed by firmware dev; only `km.move(x,y)` is reliable.
10. **`km.catch_*` is broken in v3.2** — always returns 0 regardless of lock state. Confirmed via raw serial and makcu-cpp. v3.2 is the latest left chip firmware; no fix available.
11. **`side1`/`side2` are completely unrecognised** — not aliases, just wrong names.

---

## Open Questions

None. All known commands have been physically verified against v3.2 (left) / v3.7 (right).
