/* Swet102 memory map (TECH_DESIGN §10.1).
 *
 *   0x00000  MBR + S130 2.0.1           108 KB
 *   0x1B000  Swet102 app                117 KB   <- FLASH below
 *   0x38400  FDS pages (3 × 1 KB)         3 KB   reserved: not in FLASH
 *   0x3B000  casainho bootloader         19 KB
 *   0x3FC00  bootloader settings          1 KB
 *
 * RAM: 32 KB part, first 11 KB (0x2C00) belong to S130. The SoftDevice
 * reports the minimum base it accepts; the M0 test pattern shows it.
 */
SEARCH_DIR(.)
GROUP(-lgcc -lc -lnosys)

MEMORY
{
  FLASH (rx)  : ORIGIN = 0x1B000,    LENGTH = 0x38400 - 0x1B000
  RAM   (xrw) : ORIGIN = 0x20002C00, LENGTH = 0x20008000 - 0x20002C00
}

SECTIONS
{
  .fs_data :
  {
    PROVIDE(__start_fs_data = .);
    KEEP(*(.fs_data))
    PROVIDE(__stop_fs_data = .);
  } > RAM
} INSERT AFTER .data;

INCLUDE "nrf5x_common.ld"
