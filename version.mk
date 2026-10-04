# User-visible version. Keep in sync with [workspace.package] version in Cargo.toml.
VERSION_STRING := 0.0.1

# DFU application version. The resident bootloader refuses anything not strictly
# greater than what's installed (the display is at ≥ 200), so it's date-based:
# YYMMDDNN. Override on the command line for a second build on the same day.
VERSION_NUM ?= $(shell date -u +%y%m%d)00
