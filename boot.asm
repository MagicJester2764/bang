; boot.asm — Long mode (x86_64) to 32-bit protected mode trampoline
; Assembled with: nasm -f win64 boot.asm -o boot.o
;
; C prototype:
;   void boot_jump_to_kernel(uint32_t entry, uint32_t mbi_addr);
;
; Microsoft x64 calling convention:
;   ECX = entry point (32-bit address)
;   EDX = multiboot info pointer (32-bit address)
;
; Strategy: We copy a position-independent trampoline blob to a fixed
; low address (0x1000) so that 32-bit far jumps use known absolute
; addresses, avoiding PE/COFF relocation problems.

bits 64
section .text

global boot_jump_to_kernel

TRAMP_ADDR equ 0x1000

boot_jump_to_kernel:
    cli

    ; Save kernel params in r8/r9 (callee-clobbered, but we never return)
    mov r8d, ecx            ; r8d = kernel entry point
    mov r9d, edx            ; r9d = multiboot info address

    ; Copy trampoline blob to TRAMP_ADDR
    lea rsi, [rel _tramp_start]
    mov rdi, TRAMP_ADDR
    mov rcx, _tramp_end - _tramp_start
    rep movsb

    ; Write kernel params into the blob's data slots at known offsets
    mov dword [TRAMP_ADDR + (_slot_entry - _tramp_start)], r8d
    mov dword [TRAMP_ADDR + (_slot_mbi   - _tramp_start)], r9d

    ; Fix up GDT base pointer — must write full 8 bytes for 64-bit lgdt
    mov eax, TRAMP_ADDR + (_gdt - _tramp_start)
    mov qword [TRAMP_ADDR + (_gdt_ptr_base - _tramp_start)], rax

    ; Load the GDT from the copied blob
    lgdt [TRAMP_ADDR + (_gdt_ptr - _tramp_start)]

    ; Far return to 32-bit compatibility mode code in the copied blob
    push qword 0x08         ; 32-bit code segment selector
    push qword TRAMP_ADDR   ; Target: start of trampoline blob
    retfq

; =========================================================================
; Trampoline blob — copied to TRAMP_ADDR and executed from there.
; Must be fully position-independent with respect to TRAMP_ADDR.
; =========================================================================

bits 32
_tramp_start:
    ; Now in 32-bit compatibility mode (long mode paging still active)

    ; Load 32-bit data segments
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Disable paging (clear CR0.PG bit 31)
    mov eax, cr0
    and eax, 0x7FFFFFFF
    mov cr0, eax

    ; Flush TLB
    xor eax, eax
    mov cr3, eax

    ; Disable long mode (clear EFER.LME bit 8)
    mov ecx, 0xC0000080
    rdmsr
    and eax, 0xFFFFFEFF
    wrmsr

    ; Enable protected mode (PE=1, PG=0)
    mov eax, cr0
    or eax, 1
    and eax, 0x7FFFFFFF
    mov cr0, eax

    ; Far jump to flush pipeline — absolute address known at copy time
    db 0xEA                              ; far jmp opcode
    dd TRAMP_ADDR + (_pm32 - _tramp_start)  ; 32-bit offset
    dw 0x08                              ; segment selector

_pm32:
    ; Pure 32-bit protected mode

    ; Reload data segments after far jump
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Set up a valid stack
    mov esp, 0x7C00

    ; Set up multiboot registers
    mov eax, 0x2BADB002     ; Multiboot magic number

    ; Load kernel params from data slots
    mov ebx, dword [TRAMP_ADDR + (_slot_mbi - _tramp_start)]
    mov ecx, dword [TRAMP_ADDR + (_slot_entry - _tramp_start)]

    ; Jump to kernel
    jmp ecx

.hang:
    hlt
    jmp .hang

; ---- Data slots (patched by 64-bit code before jumping here) ----
_slot_entry: dd 0
_slot_mbi:   dd 0

; ---- GDT ----
align 16
_gdt:
    dq 0                    ; Null descriptor

    ; 32-bit code segment (selector 0x08)
    dw 0xFFFF, 0x0000
    db 0x00, 0x9A, 0xCF, 0x00

    ; 32-bit data segment (selector 0x10)
    dw 0xFFFF, 0x0000
    db 0x00, 0x92, 0xCF, 0x00
_gdt_end:

_gdt_ptr:
    dw _gdt_end - _gdt - 1  ; limit (2 bytes)
_gdt_ptr_base:
    dq 0                     ; base (8 bytes — required for 64-bit lgdt)

_tramp_end:
