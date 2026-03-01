#![no_main]
#![no_std]
#![allow(bad_asm_style)]

extern crate alloc;

#[macro_use]
extern crate uefi;

mod console;
mod elf;
mod gop;
mod handoff;
mod modules;
mod multiboot;
mod trampoline;

use uefi::boot;
use uefi::mem::memory_map::MemoryType;
use uefi::prelude::*;
use uefi::println;

use elf::BootMode;

/// Multiboot magic values passed to kernel in EAX.
const MB1_BOOT_MAGIC: u32 = 0x2BAD_B002;
const MB2_BOOT_MAGIC: u32 = 0x36D7_6289;

#[entry]
fn main() -> Status {
    uefi::helpers::init().expect("Failed to initialize UEFI helpers");

    console::print_banner();

    // Load kernel ELF into memory
    let kernel = elf::load_kernel();

    println!(
        "[+] Entry: {:#x}, MB version: {}, mode: {:?}",
        kernel.entry_point, kernel.mb_version, kernel.boot_mode
    );

    // Load boot modules from \drivers\ directory (must be before ExitBootServices)
    let mods = modules::load_modules();

    // Query GOP for framebuffer info (must be before ExitBootServices)
    let fb = if kernel.mb_version == 2 {
        gop::query_gop()
    } else {
        None
    };

    println!("[+] Exiting boot services...");

    // Exit boot services — the uefi crate handles the retry internally
    let memory_map = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };

    // Build multiboot info from the final memory map (no allocation needed)
    match kernel.boot_mode {
        BootMode::Protected32 => {
            let (mbi_addr, magic) = if kernel.mb_version == 1 {
                let addr = unsafe { multiboot::build_mb1_info(&memory_map, &mods) };
                (addr, MB1_BOOT_MAGIC)
            } else {
                let addr =
                    unsafe { multiboot::build_mb2_info(&memory_map, fb.as_ref(), &mods) };
                (addr, MB2_BOOT_MAGIC)
            };

            unsafe {
                trampoline::boot_jump_to_kernel(
                    kernel.entry_point as u32,
                    mbi_addr,
                    magic,
                );
            }
        }
        BootMode::Long64 => {
            // 64-bit direct handoff — pass MB2 info if available, otherwise 0
            let boot_info_ptr = if kernel.mb_version == 2 {
                unsafe { multiboot::build_mb2_info(&memory_map, fb.as_ref(), &mods) as u64 }
            } else {
                0u64
            };

            unsafe {
                handoff::boot_handoff_64(kernel.entry_point, boot_info_ptr);
            }
        }
    }
}
