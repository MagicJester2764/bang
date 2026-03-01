//! 64-bit direct handoff for ELF64 kernels without Multiboot headers.
//!
//! Jumps directly to a 64-bit kernel entry point, converting from
//! the Microsoft x64 ABI (used by UEFI) to the System V AMD64 ABI.
//!
//! Convention: RDI = magic (0x42414E47 = "BANG"), RSI = boot_info_ptr

use core::arch::global_asm;

/// Magic value passed in RDI to identify a Bang direct handoff.
pub const BANG_HANDOFF_MAGIC: u64 = 0x4241_4E47; // "BANG"

global_asm!(
    r#"
.intel_syntax noprefix

.section .text

.global boot_handoff_64
boot_handoff_64:
    // Microsoft x64 ABI: RCX = entry, RDX = boot_info_ptr
    cli

    // Set up a clean stack at 0x7C00
    mov rsp, 0x7C00

    // Convert MS ABI → SysV ABI: RDI = magic, RSI = boot_info_ptr
    mov rdi, 0x42414E47     // BANG_HANDOFF_MAGIC
    mov rsi, rdx            // boot_info_ptr from RDX

    // Jump to kernel entry
    jmp rcx

.hang64:
    hlt
    jmp .hang64

.att_syntax prefix
"#
);

extern "C" {
    /// Jump directly to a 64-bit kernel entry point.
    ///
    /// Microsoft x64 ABI: entry in RCX, boot_info_ptr in RDX.
    /// Kernel receives: RDI = BANG_HANDOFF_MAGIC, RSI = boot_info_ptr.
    /// Does not return.
    pub fn boot_handoff_64(entry: u64, boot_info_ptr: u64) -> !;
}
