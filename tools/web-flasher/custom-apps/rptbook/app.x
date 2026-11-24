/* Oversized segment: the loader extends the arena into the bottom of
 * the stack, so LENGTH here is ARENA_MAX (12288), not ARENA_SIZE. */
MEMORY
{
  RAM : ORIGIN = 0x200001D0, LENGTH = 12288
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
