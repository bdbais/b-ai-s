/* kernel-ai v0.3-pc: avvio multiboot2 (GRUB) -> modo lungo a 64 bit.
   GRUB entra in modo protetto a 32 bit con EAX = 0x36d76289 e EBX = info multiboot2. */


.section .multiboot, "a"
.balign 8
mb2_start:
    .long 0xE85250D6                 /* magic */
    .long 0                          /* architettura i386 */
    .long mb2_end - mb2_start
    .long -(0xE85250D6 + (mb2_end - mb2_start))
    /* tag framebuffer: 1024x768x32 (GRUB usa VBE) */
    .balign 8
    .short 5
    .short 0
    .long 20
    .long 1024
    .long 768
    .long 32
    /* tag dei moduli allineati alla pagina */
    .balign 8
    .short 6
    .short 0
    .long 8
    /* fine */
    .balign 8
    .short 0
    .short 0
    .long 8
mb2_end:

.section .bss
.balign 4096
pml4:   .skip 4096
pdpt:   .skip 4096
pds:    .skip 4096 * 4
.balign 16
stack_bottom: .skip 0x100000
stack_top:

.section .rodata
.balign 16
gdt64:
    .quad 0
    .quad 0x00AF9A000000FFFF         /* codice 64 bit */
    .quad 0x00CF92000000FFFF         /* dati */
gdt64_end:
gdt64_ptr:
    .short gdt64_end - gdt64 - 1
    .quad gdt64

.section .text.boot, "ax"
.code32
.global _start
_start:
    cli
    cld
    movl $stack_top, %esp
    movl %eax, %ebp                  /* magic */
    movl %ebx, %esi                  /* info multiboot2 */

    /* azzera bss (tabelle e stack stanno lì: niente push prima di questo punto) */
    movl $__bss_start, %edi
    movl $__bss_end, %ecx
    subl %edi, %ecx
    shrl $2, %ecx
    xorl %eax, %eax
    rep stosl
    movl %ebp, %edi

    /* PML4[0] -> PDPT; PDPT[0..3] -> 4 PD; ogni PD: 512 pagine da 2 MiB = mappa identità di 4 GiB */
    movl $pdpt, %eax
    orl $3, %eax
    movl %eax, pml4
    movl $pds, %eax
    orl $3, %eax
    movl %eax, pdpt
    addl $4096, %eax
    movl %eax, pdpt + 8
    addl $4096, %eax
    movl %eax, pdpt + 16
    addl $4096, %eax
    movl %eax, pdpt + 24

    movl $pds, %ebx
    xorl %ecx, %ecx                  /* indice pagina 0..2047 */
1:
    movl %ecx, %eax
    shll $21, %eax
    orl $0x83, %eax                  /* presente, scrivibile, 2 MiB */
    movl %eax, (%ebx, %ecx, 8)
    movl %ecx, %eax
    shrl $11, %eax                   /* bit 32+ dell'indirizzo */
    movl %eax, 4(%ebx, %ecx, 8)
    incl %ecx
    cmpl $2048, %ecx
    jne 1b

    movl $pml4, %eax
    movl %eax, %cr3
    movl %cr4, %eax
    orl $0x620, %eax                 /* PAE | OSFXSR | OSXMMEXCPT */
    movl %eax, %cr4
    movl $0xC0000080, %ecx           /* EFER */
    rdmsr
    orl $0x100, %eax                 /* LME */
    wrmsr
    movl %cr0, %eax
    andl $0xFFFFFFFB, %eax           /* niente EM */
    orl $0x80000002, %eax            /* PG | MP */
    movl %eax, %cr0

    lgdt gdt64_ptr
    ljmp $0x08, $long_mode

.code64
long_mode:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    xorw %ax, %ax
    movw %ax, %fs
    movw %ax, %gs
    movabsq $stack_top, %rsp
    fninit
    /* AVX per i nuclei di bais-model: se la CPU ha XSAVE (ECX[26]) e AVX (ECX[28])
       accende CR4.OSXSAVE e abilita in XCR0 gli stati x87, SSE e AVX. Senza, il
       rilevamento di simd.rs sceglie da solo il percorso scalare. */
    movl %edi, %r8d
    movl %esi, %r9d
    movl $1, %eax
    cpuid
    andl $0x14000000, %ecx
    cmpl $0x14000000, %ecx
    jne 3f
    movq %cr4, %rax
    orq $0x40000, %rax               /* OSXSAVE */
    movq %rax, %cr4
    xorl %ecx, %ecx
    xorl %edx, %edx
    movl $7, %eax                    /* x87 | SSE | AVX */
    xsetbv
3:
    movl %r8d, %edi
    movl %r9d, %esi
    /* edi = magic, esi = info: già nei registri degli argomenti */
    movl %edi, %edi
    movl %esi, %esi
    call kmain
2:  hlt
    jmp 2b

/* ------------------------------------------------ eccezioni della CPU */
.text
.macro ISR_NOERR n
.global isr\n
isr\n:
    pushq $0
    pushq $\n
    jmp isr_common
.endm
.macro ISR_ERR n
.global isr\n
isr\n:
    pushq $\n
    jmp isr_common
.endm
ISR_NOERR 0
ISR_NOERR 1
ISR_NOERR 2
ISR_NOERR 3
ISR_NOERR 4
ISR_NOERR 5
ISR_NOERR 6
ISR_NOERR 7
ISR_ERR 8
ISR_NOERR 9
ISR_ERR 10
ISR_ERR 11
ISR_ERR 12
ISR_ERR 13
ISR_ERR 14
ISR_NOERR 15
ISR_NOERR 16
ISR_ERR 17
ISR_NOERR 18
ISR_NOERR 19
ISR_NOERR 20

isr_common:
    /* stack: vettore, codice d'errore, RIP, CS, RFLAGS, RSP, SS */
    movq 0(%rsp), %rdi
    movq 8(%rsp), %rsi
    movq 16(%rsp), %rdx
    movq %cr2, %rcx
    andq $-16, %rsp
    call exception
3:  hlt
    jmp 3b

.section .rodata
.balign 8
.global isr_table
isr_table:
    .quad isr0, isr1, isr2, isr3, isr4, isr5, isr6, isr7, isr8, isr9
    .quad isr10, isr11, isr12, isr13, isr14, isr15, isr16, isr17, isr18, isr19, isr20
