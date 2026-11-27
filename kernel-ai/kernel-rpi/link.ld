ENTRY(_start)
SECTIONS
{
    . = 0x80000;
    __kernel_start = .;
    .text.boot : { KEEP(*(.text.boot)) }
    .text : { *(.text .text.*) }
    .rodata : ALIGN(16) { *(.rodata .rodata.*) }
    .data : ALIGN(16) { *(.data .data.*) }
    .bss (NOLOAD) : ALIGN(16) {
        __bss_start = .;
        *(.bss .bss.* COMMON)
        . = ALIGN(16);
        __bss_end = .;
    }
    . = ALIGN(4096);
    __stack_bottom = .;
    . += 0x80000;
    __stack_top = .;
    . = ALIGN(4096);
    __kernel_end = .;
    /DISCARD/ : { *(.comment) *(.eh_frame*) *(.note*) }
}
