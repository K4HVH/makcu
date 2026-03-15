# MAKCU API Reference

Firmware under test: **v3.2 left chip / v3.7 right chip**
Interface: serial over USB at **4 Mbaud** on `/dev/ttyACM0` (Linux) or `COMx` (Windows)

All findings are physically verified. See `discovery/` for raw results and methodology.

---

## Hardware Overview

```
[USB 1 — Left ]  →  Gaming PC / Console        (left chip,  v3.2)
[USB 2 — COM  ]  →  Library connects here      (serial command interface)
[USB 3 — Right]  →  Mouse                      (right chip, v3.7)
```

Two independent ESP32-S3 microcontrollers. The left chip relays HID input to the
PC and runs the command interpreter. The right chip manages the physical mouse.

Constraints:
- Only one process may hold the COM port at a time.
- A mouse must be connected to USB 3 or the device will not function.

---

## Transport

| Property    | Value                                    |
|-------------|------------------------------------------|
| USB VID     | `0x1A86`                                 |
| USB PID     | `0x55D3`                                 |
| Device name | `USB-Enhanced-SERIAL CH343`              |
| Default baud | 115200                                  |
| Operating baud | 4,000,000 (4 Mbaud)                  |
| Line terminator | `\r\n`                               |

---

## Connection Sequence

Always attempt 4 Mbaud first — the device retains its baud rate within a session.

```
1. Open port at 4 Mbaud
2. Send: km.version()\r\n
3. If response contains "km.MAKCU" → connected, done.
4. Otherwise:
   a. Close port
   b. Open port at 115200
   c. Send binary baud-change frame (9 bytes, no \r\n):
        DE AD 05 00 A5 00 09 3D 00
   d. Wait 100 ms
   e. Close port
   f. Open port at 4 Mbaud
   g. Wait 50 ms, flush input buffer
   h. Send: km.version()\r\n
   i. Verify response contains "km.MAKCU"
```

### Baud-change frame layout

```
Offset  Len  Value        Description
0       2    DE AD        Magic header
2       2    05 00        Payload length (little-endian u16) = 5
4       1    A5           Command: set baud rate
5       4    00 09 3D 00  Target baud (little-endian u32) = 4,000,000
```

The baud rate is **not persistent** — it resets to 115200 on power cycle.
There is no software mechanism to make it persistent.

---

## Command Protocol

### Request format

```
km.<command>(<args>)\r\n
```

- No spaces anywhere in the command string.
- No ID or tracking suffix — the device ignores everything after `)`.

### Response format

All responses end with the prompt `>>> ` (4 bytes: `3E 3E 3E 20`).

| Pattern | Meaning |
|---------|---------|
| `<echo>\r\n>>> ` | Command executed, no return value (`EXECUTED`) |
| `<echo>\r\n<value>\r\n>>> ` | Command executed, returned a value (`RESPONDED`) |
| *(timeout, no `>>> `)* | Command not recognised (`SILENT`) |

`km.version()` is the exception: it returns `km.MAKCU\r\n>>> ` with no preceding echo line.

### Reading responses

Read bytes until `>>> ` is seen or a timeout expires. A timeout of 500 ms is
sufficient for all known commands. There is no framing or length prefix in the
ASCII protocol — the `>>> ` terminator is the only delimiter.

---

## Commands

### `km.version()`

Returns the firmware identifier.

```
→  km.version()\r\n
←  km.MAKCU\r\n>>>
```

Use this to verify the connection is live.

---

### Mouse buttons

Five buttons: `left`, `right`, `middle`, `ms1` (side 1), `ms2` (side 2).

**Do not use `side1` / `side2` — the firmware does not recognise these names.**

#### Query state

```
km.left()\r\n
km.right()\r\n
km.middle()\r\n
km.ms1()\r\n
km.ms2()\r\n
```

Returns `0` (released) or `1` (pressed). Reflects the combined physical + software
state — if the button is held down by software, it returns `1`.

**Zero-arg form is a query, not a click.**

#### Set state

```
km.left(1)\r\n    →  force button down   (EXECUTED)
km.left(0)\r\n    →  release (soft)      (EXECUTED)
km.left(2)\r\n    →  force release       (EXECUTED)
```

| Arg | Behaviour |
|-----|-----------|
| `1` | Force the button into the pressed state. Overrides physical state. |
| `0` | Release the software-held state. Does not override an active physical press. |
| `2` | Force release regardless of physical state. |

Same args apply to `right`, `middle`, `ms1`, `ms2`.

#### Click (press + release)

The device has no working click shorthand (`km.click()` is broken). Implement
clicks as two commands:

```
km.left(1)\r\n
km.left(0)\r\n
```

Insert a delay between them if needed for application compatibility.

---

### Mouse movement

```
km.move(x, y)\r\n
```

Moves the cursor by `(x, y)` relative to its current position.

| Parameter | Type | Range    |
|-----------|------|----------|
| `x`       | int  | −32767 to +32767 |
| `y`       | int  | −32767 to +32767 |

Response: `EXECUTED` (no return value).

**Smooth and bezier variants (`km.move(x,y,steps)`, `km.move(x,y,steps,cx,cy)`)
are broken in current firmware.** They are accepted without error but only execute
in certain directions — diagonal moves silently do nothing. Use `km.move(x,y)` only.

If smooth or human-like movement is required, send multiple `km.move(x,y)` calls
from the library with appropriate timing.

---

### Silent move

```
km.silent(x, y)\r\n
```

Performs: left-button-down → move(x,y) → left-button-up in two HID frames.
Useful for drag operations. Response: `EXECUTED`.

---

### Scroll wheel

```
km.wheel(n)\r\n
```

Scrolls by `n` units. Accepts any integer; there is no firmware-level clamping.
Positive values scroll up, negative scroll down (direction depends on OS settings).
Response: `EXECUTED`.

---

### Input locks

Locks block the corresponding input from reaching the PC while the lock is active.
Physical input is still received by the device — it is intercepted and dropped.

#### Set lock

```
km.lock_mx(1)\r\n    →  lock X-axis movement
km.lock_mx(0)\r\n    →  unlock X-axis movement
```

Axes: `mx` (X), `my` (Y)
Buttons: `ml` (left), `mr` (right), `mm` (middle), `ms1` (side 1), `ms2` (side 2)

All set commands return `EXECUTED`.

#### Query lock state

```
km.lock_mx()\r\n   →  returns 0 or 1
```

Returns `0` (unlocked) or `1` (locked). Same suffix options as above.

All locks are independent. Locks do not persist across power cycles.

#### Full lock command reference

| Command             | Description                  |
|---------------------|------------------------------|
| `km.lock_mx(1/0)`  | X-axis movement lock         |
| `km.lock_my(1/0)`  | Y-axis movement lock         |
| `km.lock_ml(1/0)`  | Left button lock             |
| `km.lock_mr(1/0)`  | Right button lock            |
| `km.lock_mm(1/0)`  | Middle button lock           |
| `km.lock_ms1(1/0)` | Side button 1 lock           |
| `km.lock_ms2(1/0)` | Side button 2 lock           |
| `km.lock_mx()`     | Query X-axis lock state      |
| `km.lock_my()`     | Query Y-axis lock state      |
| `km.lock_ml()`     | Query left button lock state |
| `km.lock_mr()`     | Query right button lock      |
| `km.lock_mm()`     | Query middle button lock     |
| `km.lock_ms1()`    | Query side 1 lock state      |
| `km.lock_ms2()`    | Query side 2 lock state      |

---

### Button event stream

Enables asynchronous reporting of button state changes as raw bytes.

```
km.buttons(1)\r\n    →  enable stream   (EXECUTED)
km.buttons(0)\r\n    →  disable stream  (EXECUTED)
km.buttons()\r\n     →  query enabled   (RESPONDED → 0 or 1)
```

When enabled, the device emits a single raw byte whenever any button state changes.
The byte is a bitmask and is always `< 32` (i.e. never contains `\r` or `\n`),
so it will not be confused with an ASCII response.

#### Button bitmask

| Bit | Mask   | Button        |
|-----|--------|---------------|
| 0   | `0x01` | Left          |
| 1   | `0x02` | Right         |
| 2   | `0x04` | Middle        |
| 3   | `0x08` | Side 1 (ms1)  |
| 4   | `0x10` | Side 2 (ms2)  |

`0x00` means all buttons released. Each byte represents the **complete current
state** of all buttons, not a delta.

#### Parsing

Stream bytes arrive interleaved with normal command responses on the same serial
stream. Detection rule: **read the byte immediately after the `km.` prefix.**

The device prefixes each event with the literal string `km.` followed by the raw
mask byte. Do **not** detect events by watching for bare bytes `< 32` — certain
button combinations produce mask values that equal `\r` (`0x0D`) or `\n` (`0x0A`),
which would be silently dropped or misinterpreted:

| Combination           | Mask   | Byte value |
|-----------------------|--------|------------|
| RIGHT + SIDE1         | `0x0A` | `\n` — dropped by naive parsers |
| LEFT + MIDDLE + SIDE1 | `0x0D` | `\r` — dropped by naive parsers |

The `km.` prefix approach sidesteps this entirely: match the 3-byte sequence `km.`
then take the very next byte as the mask value unconditionally.

The mask byte represents the **full current state** of all buttons — treat it as a
snapshot, not a delta. Diff against the previous value if you need to detect
individual button transitions.

---

### Serial number spoofing

Reads or sets the serial number the device presents for the connected mouse.

```
km.serial()\r\n            →  read current serial
km.serial('MYSERIAL')\r\n  →  set serial to string
km.serial(0)\r\n           →  reset to original
```

All three forms return a message string (RESPONDED).

**This feature is hardware-dependent.** If the connected mouse has no serial number
slot in its firmware, all forms return:

```
km.Mouse does not have a serial number
```

The command is processed correctly regardless — it is the mouse hardware that
determines whether spoofing takes effect.

---

## What Does Not Work

The following commands were tested and produce no effect. Do not expose them in
the library.

| Command(s) | Status | Notes |
|------------|--------|-------|
| `km.click(...)` | SILENT | No click produced. Use press + release. |
| `km.move(x,y,steps)` | Broken | Accepted but only works in some directions. |
| `km.move(x,y,steps,cx,cy)` | Broken | Same. Confirmed broken by firmware dev. |
| `km.catch_ml()` etc. | Returns 0 | Parses correctly, counter never increments. Firmware bug. |
| `km.screen()`, `moveto()`, `getpos()` | SILENT | Absolute positioning not in firmware. |
| `km.axis()` | SILENT | Axis streaming not in firmware. |
| `km.mo(...)` | SILENT | Raw HID frame injection not in firmware. |
| `km.remap_button()`, `km.invert_x()` etc. | SILENT | Not in firmware. |
| `km.turbo()` | SILENT | Not in firmware. |
| `km.pan()`, `km.tilt()` | SILENT | Not in firmware. |
| `km.mouse()` | SILENT | Mouse streaming not in firmware. |
| `km.down()`, `km.up()`, `km.press()` etc. | SILENT | Keyboard not in firmware. |
| `km.info()`, `km.device()`, `km.fault()` | SILENT | Device management not in firmware. |
| `km.baud()`, `km.echo()`, `km.log()` etc. | SILENT | Not in firmware. |
| `km.lock_mw()`, `km.lock_mx+()` etc. | SILENT | Extended locks not in firmware. |
| `km.side1()`, `km.side2()` | SILENT | Wrong names — use `km.ms1()` / `km.ms2()`. |

---

## Behaviour Notes

- **No command ID tracking.** Anything after `)` is silently ignored by the firmware.
  Do not append `#id` suffixes.
- **Lock state is not cached in hardware.** Always query the device if you need
  the current state — do not rely on a software-side cache that may diverge.
- **The button stream and command responses share the same byte stream.** If the
  stream is enabled, the library must handle interleaved stream bytes when reading
  command responses.
- **`km.buttons(1)` should not be auto-enabled on connect.** Only enable it when
  the caller explicitly requests button events.
- **`km.left(0)` does not override a physical press.** Use `km.left(2)` if you
  need to force-release regardless of physical state.
