# User-visible version. Keep in sync with [workspace.package] version in Cargo.toml.
VERSION_STRING := 0.0.1

# DFU application version. The resident bootloader refuses anything not strictly
# greater than what's installed — including other firmwares flashed the same day
# (the Swang Stodva HW probe went on as 26100401). Default: YYMMDDHH in UTC, so
# every build is newer than anything built in an earlier hour. Override on the
# command line for a second build within the same hour: make dfu VERSION_NUM=...
VERSION_NUM ?= $(shell date -u +%y%m%d%H)
