ENTRY(_start)
SECTIONS
{
    . = 0x100000;
    __kernel_start = .;
    .multiboot : { KEEP(*(.multiboot)) }
    .text.boot : { KEEP(*(.text.boot)) }
    .text : ALIGN(16) { *(.text .text.*) }
    .rodata : ALIGN(16) { *(.rodata .rodata.*) }
    .data : ALIGN(16) { *(.data .data.*) *(.got*) }
    .bss (NOLOAD) : ALIGN(4096) {
        __bss_start = .;
        *(.bss .bss.* COMMON)
        . = ALIGN(16);
        __bss_end = .;
    }
    . = ALIGN(4096);
    __kernel_end = .;
    /DISCARD/ : { *(.comment) *(.eh_frame*) *(.note*) *(.gcc_except_table*) }
}
