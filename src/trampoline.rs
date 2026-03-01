//! Assembly trampoline: drops from 64-bit long mode to 32-bit protected mode
//! and jumps to a Multiboot kernel.
//!
//! Ported from boot.asm (NASM win64) to GAS .intel_syntax inside global_asm!.
//! The trampoline blob is copied to physical address 0x1000 at runtime, so
//! all addresses within it must be computed relative to that base.
//!
//! Layout of the blob at TRAMP_ADDR:
//!   +0x00: jmp over data slots (2 bytes)
//!   +0x02: _slot_entry (4 bytes)
//!   +0x06: _slot_mbi   (4 bytes)
//!   +0x0A: _slot_magic (4 bytes)
//!   +0x0E: code starts

use core::arch::global_asm;

global_asm!(
    r#"
.intel_syntax noprefix

.set TRAMP_ADDR, 0x1000
// Data slot absolute addresses
.set SLOT_ENTRY, TRAMP_ADDR + 0x02
.set SLOT_MBI,   TRAMP_ADDR + 0x06
.set SLOT_MAGIC, TRAMP_ADDR + 0x0A

.section .text

.global boot_jump_to_kernel
boot_jump_to_kernel:
    cli

    // Microsoft x64 ABI: ECX = entry, EDX = mbi_addr, R8D = magic
    mov r10d, ecx
    mov r11d, edx
    // r8d already has magic

    // Compute blob size into rcx
    lea rsi, [rip + _tramp_start]
    lea rcx, [rip + _tramp_end]
    sub rcx, rsi

    // Copy trampoline blob to TRAMP_ADDR
    mov rdi, TRAMP_ADDR
    rep movsb

    // Write kernel params into the blob's data slots (absolute addresses)
    mov dword ptr [SLOT_ENTRY], r10d
    mov dword ptr [SLOT_MBI],   r11d
    mov dword ptr [SLOT_MAGIC], r8d

    // Fix up GDT base pointer — compute address of _gdt in copied blob
    lea r12, [rip + _tramp_start]

    lea rax, [rip + _gdt]
    sub rax, r12
    add rax, TRAMP_ADDR
    // rax = address of _gdt in copied blob

    // Write GDT base into _gdt_ptr_base in copied blob
    lea rdx, [rip + _gdt_ptr_base]
    sub rdx, r12
    add rdx, TRAMP_ADDR
    mov qword ptr [rdx], rax

    // Load the GDT from the copied blob
    lea rax, [rip + _gdt_ptr]
    sub rax, r12
    add rax, TRAMP_ADDR
    lgdt [rax]

    // Far return to 32-bit compatibility mode code in the copied blob
    // Target = TRAMP_ADDR + offset of _tramp_code (past the data slots)
    lea rax, [rip + _tramp_code]
    sub rax, r12
    add rax, TRAMP_ADDR
    push 0x08           // 32-bit code segment selector
    push rax            // target address in copied blob
    retfq

// =========================================================================
// Trampoline blob — copied to TRAMP_ADDR and executed from there.
// Data slots are placed first at known offsets, then code follows.
// =========================================================================

.code32
_tramp_start:
    // +0x00: Jump over data slots to the real code
    jmp _tramp_code

    // +0x02: Data slots (patched by 64-bit code before jumping here)
    // These MUST be at the offsets defined in SLOT_ENTRY/SLOT_MBI/SLOT_MAGIC
_slot_entry: .long 0    // +0x02
_slot_mbi:   .long 0    // +0x06
_slot_magic: .long 0    // +0x0A

_tramp_code:
    // Now in 32-bit compatibility mode (long mode paging still active)

    // Load 32-bit data segments
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    // Disable paging (clear CR0.PG bit 31)
    mov eax, cr0
    and eax, 0x7FFFFFFF
    mov cr0, eax

    // Flush TLB
    xor eax, eax
    mov cr3, eax

    // Disable long mode (clear EFER.LME bit 8)
    mov ecx, 0xC0000080
    rdmsr
    and eax, 0xFFFFFEFF
    wrmsr

    // Enable protected mode (PE=1, PG=0)
    mov eax, cr0
    or eax, 1
    and eax, 0x7FFFFFFF
    mov cr0, eax

    // Far jump to flush pipeline — manually encoded with absolute address.
    // Target = TRAMP_ADDR + offset of _pm32 from _tramp_start
    .byte 0xEA                              // far jmp opcode
    .long TRAMP_ADDR + (_pm32 - _tramp_start) // 32-bit offset
    .short 0x08                             // code segment selector

_pm32:
    // Pure 32-bit protected mode

    // Reload data segments after far jump
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    // Set up a valid stack
    mov esp, 0x7C00

    // Load kernel params from data slots using absolute addresses
    mov eax, dword ptr [SLOT_MAGIC]
    mov ebx, dword ptr [SLOT_MBI]
    mov ecx, dword ptr [SLOT_ENTRY]

    // Jump to kernel
    jmp ecx

.hang32:
    hlt
    jmp .hang32

// ---- GDT ----
.balign 16
_gdt:
    .quad 0                 // Null descriptor
    // 32-bit code segment (selector 0x08)
    .short 0xFFFF, 0x0000
    .byte 0x00, 0x9A, 0xCF, 0x00
    // 32-bit data segment (selector 0x10)
    .short 0xFFFF, 0x0000
    .byte 0x00, 0x92, 0xCF, 0x00
_gdt_end:

_gdt_ptr:
    .short _gdt_end - _gdt - 1   // limit (2 bytes)
_gdt_ptr_base:
    .quad 0                       // base (8 bytes — required for 64-bit lgdt)

_tramp_end:

.code64
.att_syntax prefix
"#
);

extern "C" {
    /// Jump to a 32-bit kernel via the trampoline.
    ///
    /// Microsoft x64 ABI: entry in RCX, mbi_addr in RDX, magic in R8.
    /// Does not return.
    pub fn boot_jump_to_kernel(entry: u32, mbi_addr: u32, magic: u32) -> !;
}
