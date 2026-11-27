// Avvio di kernel-ai su Raspberry Pi (ARM64).
// Il firmware carica l'immagine a 0x80000 e salta qui sul core 0, di solito a EL2,
// con x0 = indirizzo del device tree. Gli altri core restano nello spin-table del firmware;
// se qualcuno arriva qui lo parcheggiamo.

.section .text.boot
.global _start
_start:
    mov     x19, x0                 // device tree (0 se assente)
    mrs     x1, mpidr_el1
    and     x1, x1, #0xffff         // Aff0 + Aff1 (su Cortex-A76 il numero del core sta in Aff1)
    cbz     x1, 1f
0:  wfe
    b       0b

1:  mrs     x0, CurrentEL
    lsr     x0, x0, #2
    cmp     x0, #3
    b.ne    2f
    // EL3 -> EL2 (capita sotto alcuni emulatori)
    mov     x0, #0x5b1              // NS, SMD, HCE, RW
    msr     scr_el3, x0
    mov     x0, #0x3c9              // EL2h, interruzioni mascherate
    msr     spsr_el3, x0
    adr     x0, 2f
    msr     elr_el3, x0
    eret

2:  mrs     x0, CurrentEL
    lsr     x0, x0, #2
    cmp     x0, #2
    b.ne    3f
    // EL2 -> EL1
    mov     x0, #3
    msr     cnthctl_el2, x0         // contatore e timer accessibili da EL1
    msr     cntvoff_el2, xzr
    mov     x0, #(1 << 31)          // EL1 a 64 bit
    msr     hcr_el2, x0
    mov     x0, #0x33ff
    msr     cptr_el2, x0            // niente trap su FP/SIMD
    msr     hstr_el2, xzr
    mov     x0, #0x0800
    movk    x0, #0x30d0, lsl #16    // SCTLR_EL1: bit RES1, MMU e cache spente
    msr     sctlr_el1, x0
    mov     x0, #0x3c5              // EL1h
    msr     spsr_el2, x0
    adr     x0, 3f
    msr     elr_el2, x0
    eret

3:  mov     x0, #(3 << 20)
    msr     cpacr_el1, x0           // FP e NEON a EL1
    isb
    ldr     x0, =__stack_top
    mov     sp, x0
    ldr     x0, =__bss_start
    ldr     x1, =__bss_end
4:  cmp     x0, x1
    b.hs    5f
    str     xzr, [x0], #8
    b       4b
5:  adr     x0, vectors
    msr     vbar_el1, x0
    isb
    mov     x0, x19
    bl      kmain
6:  wfe
    b       6b

// Tabella delle eccezioni: ogni voce passa tipo, ESR, ELR e FAR al gestore in Rust.
.macro VEC n
.balign 0x80
    mov     x0, #\n
    mrs     x1, esr_el1
    mrs     x2, elr_el1
    mrs     x3, far_el1
    b       exception
.endm

.balign 2048
vectors:
    VEC 0
    VEC 1
    VEC 2
    VEC 3
    VEC 4
    VEC 5
    VEC 6
    VEC 7
    VEC 8
    VEC 9
    VEC 10
    VEC 11
    VEC 12
    VEC 13
    VEC 14
    VEC 15
