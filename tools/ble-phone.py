#! /usr/bin/env nix
#! nix shell --impure --expr ``(builtins.getFlake "nixpkgs").legacyPackages.${builtins.currentSystem}.python3.withPackages (p: [ p.bleak ])`` --command python3
"""Bench stand-in for the companion app (TECH_DESIGN §9): connect to the
display, subscribe to everything and print what it sends.

    tools/ble-phone.py                 scan for "swet102", connect, print
    tools/ble-phone.py --address XX:.. connect to a known address
    tools/ble-phone.py --time 19:05 06:40
                                       also send the PC's local time with that
                                       sunset and sunrise (lights AUTO)

No pairing: like the real app, it just connects. Ctrl-C to quit.
"""

import argparse
import asyncio
import struct
import sys
import time

from bleak import BleakClient, BleakScanner

BASE = "8f03{:04x}-da4c-453d-a163-41a592a0e9fd"
TELEMETRY, COMMAND, CONTROL, TRIPS = (BASE.format(n) for n in (2, 3, 4, 5))
FW_REV = "00002a26-0000-1000-8000-00805f9b34fb"
SW_REV = "00002a28-0000-1000-8000-00805f9b34fb"  # build number (VERSION_NUM)

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
    if len(data) != 16 or data[0] != 3:
        print(f"telemetry ?? {data.hex()}")
        return
    _, speed, power, soc, pas, limit, flags, err, odo_hm, pack_cv = struct.unpack(
        "<BHHBBBBBIH", data
    )
    soc_s = "--" if soc == 0xFF else f"{soc}%"
    volts = f" {pack_cv / 100:.1f} V" if pack_cv else ""
    err_s = "" if err == 0 else (" LINK LOST" if err == 0xFF else f" ERROR {err:02X}")
    lights = " lights" if flags & 1 else ""
    walk = " walk" if flags & 2 else ""
    link = "" if flags & 4 else " (no motor)"
    print(
        f"tel  {speed / 10:5.1f} km/h {power:4d} W  bat {soc_s:>4}{volts}  PAS {pas}"
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


def hhmm(s: str) -> int:
    h, m = (int(x) for x in s.split(":"))
    if not (0 <= h < 24 and 0 <= m < 60):
        raise argparse.ArgumentTypeError(f"not a time of day: {s}")
    return h * 60 + m


def time_sync(sunset_min: int, sunrise_min: int) -> bytes:
    """The control message the app sends on connect (TECH_DESIGN §9.5)."""
    t = time.localtime()
    secs = t.tm_hour * 3600 + t.tm_min * 60 + t.tm_sec
    return struct.pack("<BIHH", ord("T"), secs, sunset_min, sunrise_min)


async def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--address", help="display address (skip the scan)")
    ap.add_argument("--time", nargs=2, type=hhmm, metavar=("SUNSET", "SUNRISE"),
                    help="send the local time with today's sunset and sunrise (HH:MM)")
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
        build = await c.read_gatt_char(SW_REV)
        print(f"connected; firmware {rev.decode(errors='replace')}, build {build.decode(errors='replace')}")
        await c.start_notify(TELEMETRY, telemetry)
        await c.start_notify(TRIPS, trips)
        await c.start_notify(COMMAND, command)
        if args.time:
            await c.write_gatt_char(CONTROL, time_sync(*args.time), response=True)
            print(f"sent the time ({time.strftime('%H:%M:%S')}); the first sync sets lights to AUTO")
        while c.is_connected:
            await asyncio.sleep(1)
        print("disconnected")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
