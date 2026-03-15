#!/usr/bin/env python3
"""
Button mask verification test.

Enables km.buttons(1) and listens for stream bytes using the km. prefix
detection method — reads the mask byte positionally after seeing "km." in
the stream, so byte values like 0x0A (RIGHT+SIDE1) and 0x0D (LEFT+MID+SIDE1)
are not confused with \n / \r and silently dropped.

Press buttons on the mouse. Ctrl-C or q to quit.

Usage:
    python discovery/test_button_mask.py
    python discovery/test_button_mask.py --port /dev/ttyACM0
"""

import argparse
import collections
import select
import sys
import termios
import time
import tty

import serial
import serial.tools.list_ports

VID = 0x1A86
PID = 0x55D3
BAUD_4M = 4_000_000
BAUD_DEFAULT = 115_200
BAUD_FRAME = bytes([0xDE, 0xAD, 0x05, 0x00, 0xA5, 0x00, 0x09, 0x3D, 0x00])
TERMINATOR = b">>> "

BUTTONS = [
    (0x01, "LEFT  "),
    (0x02, "RIGHT "),
    (0x04, "MIDDLE"),
    (0x08, "SIDE1 "),
    (0x10, "SIDE2 "),
]


def find_port():
    for p in serial.tools.list_ports.comports():
        if p.vid == VID and p.pid == PID:
            return p.device
    raise RuntimeError("MAKCU not found")


def read_prompt(ser, timeout=0.5):
    buf = b""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        chunk = ser.read(64)
        if chunk:
            buf += chunk
            if TERMINATOR in buf:
                return buf
    return buf


def send(ser, cmd, timeout=0.5):
    ser.reset_input_buffer()
    ser.write(cmd.encode() + b"\r\n")
    ser.flush()
    return read_prompt(ser, timeout)


def connect(port_name=None):
    name = port_name or find_port()
    try:
        ser = serial.Serial(name, BAUD_4M, timeout=0.05)
        ser.write(b"km.version()\r\n")
        ser.flush()
        if b"km.MAKCU" in read_prompt(ser, 0.5):
            return ser
        ser.close()
    except Exception:
        pass
    ser = serial.Serial(name, BAUD_DEFAULT, timeout=0.05)
    ser.write(BAUD_FRAME)
    ser.flush()
    time.sleep(0.1)
    ser.close()
    ser = serial.Serial(name, BAUD_4M, timeout=0.05)
    time.sleep(0.05)
    ser.reset_input_buffer()
    ser.write(b"km.version()\r\n")
    ser.flush()
    if b"km.MAKCU" not in read_prompt(ser, 0.5):
        raise RuntimeError("version check failed")
    return ser


def decode_mask(byte):
    active = [name.strip() for mask, name in BUTTONS if byte & mask]
    return " + ".join(active) if active else "all released"


def format_mask(byte):
    bits = "".join(("1" if byte & mask else "0") for mask, _ in reversed(BUTTONS))
    return f"0x{byte:02X}  [{bits}]  {decode_mask(byte)}"


def println(msg=""):
    """Print with \r\n for raw terminal mode."""
    sys.stdout.write(msg + "\r\n")
    sys.stdout.flush()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port")
    args = ap.parse_args()

    ser = connect(args.port)

    fd = sys.stdin.fileno()
    old_settings = termios.tcgetattr(fd)

    try:
        tty.setraw(fd)

        println()
        println("\033[1mButton mask test\033[0m  (q / Ctrl-C to quit)")
        println()
        println("  Bit layout:  [SIDE2 SIDE1 MID RIGHT LEFT]")
        println("  Detection:   km. prefix method (handles 0x0A / 0x0D combinations)")
        println()

        send(ser, "km.buttons(1)")
        println("  Stream enabled. Press buttons on the mouse...")
        println()

        # State machine: match "km." prefix then take next byte as mask
        # Problematic combos with bare-byte detection:
        #   RIGHT + SIDE1        = 0x0A = \\n  (silently dropped without prefix method)
        #   LEFT + MIDDLE + SIDE1 = 0x0D = \\r  (same)
        KM_PREFIX = b"km."
        prefix_buf = collections.deque(maxlen=3)
        expect_mask = False
        prev_mask = None

        while True:
            # Quit key
            if select.select([sys.stdin], [], [], 0)[0]:
                ch = sys.stdin.read(1)
                if ch in ("\x03", "\x04", "q"):
                    break

            data = ser.read(ser.in_waiting or 1)
            if not data:
                continue

            for byte in data:
                if expect_mask:
                    expect_mask = False
                    if byte != prev_mask:
                        prev_mask = byte
                        label = format_mask(byte)
                        if byte == 0x00:
                            println(f"  \033[90m\u2191 released    {label}\033[0m")
                        else:
                            println(f"  \033[92m\u2193 pressed     {label}\033[0m")
                    continue

                prefix_buf.append(byte)
                if bytes(prefix_buf) == KM_PREFIX:
                    expect_mask = True
                    prefix_buf.clear()

    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old_settings)
        send(ser, "km.buttons(0)")
        ser.close()
        print("\r\nStream disabled. Done.\r\n")


if __name__ == "__main__":
    main()
