#! /usr/bin/env nix
#! nix shell --impure --expr ``(builtins.getFlake "nixpkgs").legacyPackages.${builtins.currentSystem}.python3.withPackages (p: [ p.bleak ])`` --command python3
"""Bench stand-in for the companion app (TECH_DESIGN §9): connect to the
display, subscribe to everything and print what it sends.

    tools/ble-phone.py                 scan for "swet102", connect, print
    tools/ble-phone.py --address XX:.. connect to a known address
    tools/ble-phone.py --dfu           also write "DFU!" to the control characteristic

No pairing: like the real app, it just connects. Ctrl-C to quit.
"""

import argparse
import asyncio
import struct
import sys

from bleak import BleakClient, BleakScanner

BASE = "8f03{:04x}-da4c-453d-a163-41a592a0e9fd"
TELEMETRY, COMMAND, CONTROL, TRIPS = (BASE.format(n) for n in (2, 3, 4, 5))
FW_REV = "00002a26-0000-1000-8000-00805f9b34fb"

COMMANDS = {
    0x01: "volume +",
    0x02: "volume -",
    0x03: "next track",
    0x04: "previous track",
    0x05: "play / pause",
    0x10: "gate A",
    0x11: "gate B",
}
TRIP_NAMES = ["trip", "battery", "ride", "odometer"]


def telemetry(_, data: bytearray) -> None:
    if len(data) != 14 or data[0] != 2:
        print(f"telemetry ?? {data.hex()}")
        return
    _, speed, power, soc, pas, limit, flags, err, odo_hm = struct.unpack("<BHHBBBBBI", data)
    soc_s = "--" if soc == 0xFF else f"{soc}%"
    err_s = "" if err == 0 else (" LINK LOST" if err == 0xFF else f" ERROR {err:02X}")
    lights = " lights" if flags & 1 else ""
    walk = " walk" if flags & 2 else ""
    link = "" if flags & 4 else " (no motor)"
    print(
        f"tel  {speed / 10:5.1f} km/h {power:4d} W  bat {soc_s:>4}  PAS {pas}"
        f"  limit {limit}  odo {odo_hm / 10:.1f} km{lights}{walk}{link}{err_s}"
    )


def trips(_, data: bytearray) -> None:
    if len(data) != 18 or data[0] != 1:
        print(f"trips ?? {data.hex()}")
        return
    _, tid, m, mx, avg, mah, moving_s = struct.unpack("<BBIHHII", data)
    name = TRIP_NAMES[tid] if tid < 4 else f"id {tid}"
    print(
        f"trip {name:>8}: {m / 1000:7.2f} km  max {mx / 10:.1f}  avg {avg / 10:.1f}"
        f"  {mah / 1000:.2f} Ah  moving {moving_s // 60} min"
    )


def command(_, data: bytearray) -> None:
    seq, code = data[0], data[1]
    print(f"CMD  #{seq:3d} {COMMANDS.get(code, f'code {code:02X}')}")


async def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--address", help="display address (skip the scan)")
    ap.add_argument("--dfu", action="store_true", help='write "DFU!" (wheel must be stopped 5 s)')
    args = ap.parse_args()

    target = args.address
    if not target:
        print("scanning for swet102 ...")
        dev = await BleakScanner.find_device_by_name("swet102", timeout=20)
        if dev is None:
            sys.exit("swet102 not found")
        target = dev.address
    print(f"connecting to {target}")
    async with BleakClient(target) as c:
        rev = await c.read_gatt_char(FW_REV)
        print(f"connected; firmware {rev.decode(errors='replace')}")
        await c.start_notify(TELEMETRY, telemetry)
        await c.start_notify(TRIPS, trips)
        await c.start_notify(COMMAND, command)
        if args.dfu:
            await c.write_gatt_char(CONTROL, b"DFU!", response=True)
            print('wrote "DFU!": the display should reboot into SW102_DFU')
        while c.is_connected:
            await asyncio.sleep(1)
        print("disconnected")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
