# Swet102 firmware: Rust core (staticlib) + C platform layer + nRF5 SDK 12.3.
#
#   make            build/swet102.hex (app only) + size report
#   make check      size gates: fits the app region, no core::fmt in the image
#   make dfu        signed OTA zip (needs NRFUTIL; SWET_PIN_CITY/SWET_PIN_SPORT or DEV_PINS=1)
#   make full       bootloader + S130 + app + settings, for SWD   (needs NRFUTIL)
#   make flash-full mass-erase and write the full image over SWD  (needs OPENOCD)
#
# `nix develop` sets SDK_ROOT and puts the toolchains on PATH.

SDK_ROOT ?= $(error SDK_ROOT is not set: run inside `nix develop`, or point it at nRF5_SDK_12.3.0)
CROSS    ?= arm-none-eabi-
CC       := $(CROSS)gcc
OBJCOPY  := $(CROSS)objcopy
SIZE     := $(CROSS)size
NM       := $(CROSS)nm
NRFUTIL  ?= tools/nrfutil.sh
OPENOCD  ?= tools/openocd.sh
SREC_CAT ?= srec_cat

include version.mk

BUILD     := build
OUT       := $(BUILD)/swet102
RUST_LIB  := target/thumbv6m-none-eabi/release/libswet_fw.a
SOFTDEVICE := $(SDK_ROOT)/components/softdevice/s130/hex/s130_nrf51_2.0.1_softdevice.hex
BOOTLOADER := prebuilt/sw102_bootloader.hex
KEYFILE    := prebuilt/private.key

PLATFORM_SRC := $(wildcard platform/nrf51/*.c)

SDK_SRC := \
  components/libraries/util/app_error.c \
  components/libraries/util/app_error_weak.c \
  components/libraries/util/app_util_platform.c \
  components/libraries/util/nrf_assert.c \
  components/libraries/timer/app_timer.c \
  components/libraries/scheduler/app_scheduler.c \
  components/libraries/fifo/app_fifo.c \
  components/libraries/uart/app_uart_fifo.c \
  components/libraries/fds/fds.c \
  components/libraries/fstorage/fstorage.c \
  components/libraries/crc16/crc16.c \
  components/drivers_nrf/common/nrf_drv_common.c \
  components/drivers_nrf/clock/nrf_drv_clock.c \
  components/drivers_nrf/spi_master/nrf_drv_spi.c \
  components/drivers_nrf/uart/nrf_drv_uart.c \
  components/drivers_nrf/wdt/nrf_drv_wdt.c \
  components/softdevice/common/softdevice_handler/softdevice_handler.c \
  components/softdevice/common/softdevice_handler/softdevice_handler_appsh.c \
  components/toolchain/system_nrf51.c

SDK_ASM := components/toolchain/gcc/gcc_startup_nrf51.S

SDK_INC := \
  components/device \
  components/toolchain \
  components/toolchain/gcc \
  components/toolchain/cmsis/include \
  components/softdevice/s130/headers \
  components/softdevice/s130/headers/nrf51 \
  components/softdevice/common/softdevice_handler \
  components/drivers_nrf/common \
  components/drivers_nrf/clock \
  components/drivers_nrf/delay \
  components/drivers_nrf/hal \
  components/drivers_nrf/spi_master \
  components/drivers_nrf/uart \
  components/drivers_nrf/wdt \
  components/libraries/util \
  components/libraries/timer \
  components/libraries/scheduler \
  components/libraries/fifo \
  components/libraries/uart \
  components/libraries/fds \
  components/libraries/fstorage \
  components/libraries/crc16 \
  components/libraries/log \
  components/libraries/log/src \
  components/libraries/experimental_section_vars \
  components/ble/common

DEFS := -DNRF51 -DNRF51822 -DS130 -DSOFTDEVICE_PRESENT -DBLE_STACK_SUPPORT_REQD \
        -DNRF_SD_BLE_API_VERSION=2 -DSWI_DISABLE0 -DUSE_WITH_BOOTLOADER

ARCH   := -mcpu=cortex-m0 -mthumb -mabi=aapcs -mfloat-abi=soft
CFLAGS := -std=c99 $(ARCH) $(DEFS) -Os -g3 -ffunction-sections -fdata-sections \
          -fno-strict-aliasing -fno-builtin -fshort-enums \
          -Iplatform/nrf51 $(addprefix -isystem $(SDK_ROOT)/,$(SDK_INC))
# Our own C is held to a higher bar than the SDK.
PLATFORM_CFLAGS := -Wall -Wextra -Werror
# Reset_Handler zeroes .bss and jumps straight to main(): no newlib _start,
# so no exit()/stdio cleanup and none of the file-I/O syscall stubs get linked.
ASMFLAGS := -x assembler-with-cpp $(ARCH) $(DEFS) -D__STACK_SIZE=4096 -D__HEAP_SIZE=0 \
            -D__STARTUP_CLEAR_BSS -D__START=main
LDFLAGS  := $(ARCH) -Tplatform/nrf51/swet102.ld -L$(SDK_ROOT)/components/toolchain/gcc \
            -Wl,--gc-sections -Wl,--no-enum-size-warning -Wl,--no-warn-execstack -Wl,--print-memory-usage \
            -Wl,-Map=$(OUT).map --specs=nano.specs -nostartfiles

PLATFORM_OBJ := $(PLATFORM_SRC:%.c=$(BUILD)/%.o)
SDK_OBJ      := $(SDK_SRC:%.c=$(BUILD)/sdk/%.o) $(SDK_ASM:%.S=$(BUILD)/sdk/%.o)

.PHONY: all check check-pins dfu full flash-full flash-app clean FORCE
all: $(OUT).hex

$(RUST_LIB): FORCE
	SWET_BUILD_NUM=$(VERSION_NUM) cargo build -p swet-fw --release --target thumbv6m-none-eabi

$(BUILD)/platform/%.o: platform/%.c
	@mkdir -p $(dir $@)
	@echo "  CC      $<"
	@$(CC) $(CFLAGS) $(PLATFORM_CFLAGS) -MMD -c $< -o $@

$(BUILD)/sdk/%.o: $(SDK_ROOT)/%.c
	@mkdir -p $(dir $@)
	@echo "  CC      $(notdir $<)"
	@$(CC) $(CFLAGS) -MMD -c $< -o $@

$(BUILD)/sdk/%.o: $(SDK_ROOT)/%.S
	@mkdir -p $(dir $@)
	@echo "  AS      $(notdir $<)"
	@$(CC) $(ASMFLAGS) -c $< -o $@

$(OUT).elf: $(PLATFORM_OBJ) $(SDK_OBJ) $(RUST_LIB) platform/nrf51/swet102.ld
	@echo "  LD      $@"
	@$(CC) $(LDFLAGS) $(PLATFORM_OBJ) $(SDK_OBJ) $(RUST_LIB) -o $@
	$(SIZE) $@

$(OUT).hex: $(OUT).elf
	$(OBJCOPY) -O ihex $< $@

# Size gates (TECH_DESIGN §13). The linker already fails if FLASH overflows.
check: $(OUT).elf
	@if $(NM) -C $< | grep -q 'core::fmt'; then \
	  echo "error: core::fmt is linked into the firmware:"; $(NM) -C $< | grep 'core::fmt' | head; exit 1; fi
	@echo "ok: no core::fmt in $<"
	@# A non-zero default anywhere in App moves 1+ KB from .bss to .data (flash).
	@if ! $(NM) -C $< | grep -q " b swet_fw::APP$$"; then \
	  echo "error: swet_fw::APP is not zero-initialised (not in .bss):"; $(NM) -S -C $< | grep "swet_fw::APP"; exit 1; fi
	@echo "ok: App is zero-initialised (.bss)"

# OTA packages are what goes on the bike: refuse the public dev PINs (1111/2222)
# unless DEV_PINS=1 says it's a bench build on purpose.
check-pins:
ifndef DEV_PINS
	@if [ -z "$$SWET_PIN_CITY" ] || [ -z "$$SWET_PIN_SPORT" ]; then \
	  echo "error: set SWET_PIN_CITY and SWET_PIN_SPORT (4 digits each), or DEV_PINS=1 for a bench build"; exit 1; fi
endif

dfu: check-pins $(OUT).hex
	$(NRFUTIL) pkg generate --application $(OUT).hex --key-file $(KEYFILE) \
	  --application-version $(VERSION_NUM) --hw-version 51 --sd-req 0x87 $(OUT)-$(VERSION_NUM).zip

$(BUILD)/settings.hex: $(OUT).hex
	$(NRFUTIL) settings generate --no-backup --family NRF51 --application $< \
	  --application-version $(VERSION_NUM) --bootloader-version 0 --bl-settings-version 1 $@

full: $(BUILD)/settings.hex
	$(SREC_CAT) -MULTiple $(BOOTLOADER) -Intel $(SOFTDEVICE) -Intel $(OUT).hex -Intel \
	  $(BUILD)/settings.hex -Intel -Output $(OUT)-full.hex -Intel

flash-full: full
	$(OPENOCD) -c "init; halt; nrf51 mass_erase; reset halt; flash write_image $(OUT)-full.hex; verify_image $(OUT)-full.hex; reset run; shutdown"

# App + settings page only; keeps bootloader and SoftDevice.
flash-app: $(BUILD)/settings.hex
	$(OPENOCD) -c "init; halt; flash write_image erase $(OUT).hex; flash write_image erase $(BUILD)/settings.hex; reset run; shutdown"

clean:
	rm -rf $(BUILD)
	cargo clean -p swet-fw --release --target thumbv6m-none-eabi

# The SDK reaches sdk_config.h through -isystem headers, which -MMD doesn't
# track; without this a config change leaves stale SDK objects.
$(SDK_OBJ): platform/nrf51/sdk_config.h

-include $(PLATFORM_OBJ:.o=.d) $(SDK_OBJ:.o=.d)
