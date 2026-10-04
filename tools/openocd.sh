#!/usr/bin/env bash
# OpenOCD over CMSIS-DAP (WCH-LinkE in DAP mode: `wlink mode-switch --dap`) for
# the nRF51. Runs in a privileged container because raw USB access needs root
# on this NixOS host; the openocd binary comes from the Nix store.
set -euo pipefail
OPENOCD_BIN=$(command -v openocd)
OPENOCD_BIN=$(readlink -f "$OPENOCD_BIN")
SCRIPTS=$(dirname "$OPENOCD_BIN")/../share/openocd/scripts
IMAGE=${OPENOCD_IMAGE:-swet102-nrfutil}  # any local image works; it only provides a rootfs
exec docker run --rm --privileged -v /nix:/nix:ro -v /dev/bus/usb:/dev/bus/usb \
  -v "$PWD:$PWD" -w "$PWD" --entrypoint "$OPENOCD_BIN" "$IMAGE" \
  -s "$SCRIPTS" -f interface/cmsis-dap.cfg -f target/nrf51.cfg "$@"
