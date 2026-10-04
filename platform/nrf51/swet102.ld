/* Swet102 memory map (TECH_DESIGN §10.1), verified with the HW probe:
 * UICR.BOOTLOADERADDR = 0x3AC00, 4 × 8 KB RAM blocks.
 *
 *   0x00000  MBR + S130 2.0.1           108 KB
 *   0x1B000  Swet102 app                115 KB   <- FLASH below
 *   0x37C00  FDS pages (3 × 1 KB)         3 KB   reserved: not in FLASH. fstorage
 *                                                puts them right below the bootloader.
 *   0x3AC00  casainho bootloader         20 KB
 *   0x3FC00  bootloader settings          1 KB
 *
 * RAM: 32 KB part. S130 needs up to 0x20001FE8 (probe); we keep SS's 0x20002C00.
 */
SEARCH_DIR(.)
GROUP(-lgcc -lc -lnosys)

MEMORY
{
  FLASH (rx)  : ORIGIN = 0x1B000,    LENGTH = 0x37C00 - 0x1B000
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
