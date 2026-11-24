/* Resident segment: this app is interactive and stays loaded, so it
 * must fit ARENA_SIZE (8192). Oversized segments (up to ARENA_MAX)
 * are transient-only — the loader faults them if Enter returns
 * Continue (FAULT_OVERSIZE_CONTINUE) — so LENGTH here also acts as a
 * build-time guard against regressing into an oversized build. */
MEMORY
{
  RAM : ORIGIN = 0x200001D0, LENGTH = 8192
}

ENTRY(app_entry);

SECTIONS
{
  .text : {
    *(.text .text.*)
  } > RAM

  .rodata : {
    *(.rodata .rodata.*)
  } > RAM

  .data : {
    *(.data .data.*)
  } > RAM

  .bss (NOLOAD) : {
    *(.bss .bss.*)
    *(COMMON)
  } > RAM

  /DISCARD/ : {
    *(.ARM.exidx*)
    *(.ARM.extab*)
    *(.comment)
    *(.debug*)
    *(.eh_frame*)
  }
}
