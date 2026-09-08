#![no_main]
#![no_std]
#![allow(bad_asm_style)]

extern crate alloc;

#[macro_use]
extern crate uefi;

mod chain;
mod discover;
mod config;
mod console;
mod elf;
mod gop;
mod handoff;
mod linux;
mod menu;
mod modules;
mod multiboot;
mod trampoline;

use uefi::boot;
use uefi::mem::memory_map::MemoryType;
use uefi::prelude::*;
use uefi::println;

use elf::BootMode;

/// UCS-2 string length, in u16 units, excluding the terminator.
///
/// Nothing references this from Rust. LLVM recognises the `CStr16` scan loops
/// in the `uefi` crate and rewrites them into a `wcslen` libcall, and on a
/// freestanding UEFI target there is no libc to satisfy it — compiler_builtins
/// provides the `mem`/`str` family but not the wide-character ones. Newer
/// toolchains perform that rewrite where older ones did not, so the symbol has
/// to exist for the image to link.
///
/// # Safety
/// `s` must point to a NUL-terminated UCS-2 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcslen(s: *const u16) -> usize {
    let mut n = 0usize;
    // SAFETY: caller guarantees a NUL-terminated string.
    while unsafe { *s.add(n) } != 0 {
        n += 1;
    }
    n
}

/// Multiboot magic values passed to kernel in EAX.
const MB1_BOOT_MAGIC: u32 = 0x2BAD_B002;
const MB2_BOOT_MAGIC: u32 = 0x36D7_6289;

#[entry]
fn main() -> Status {
    uefi::helpers::init().expect("Failed to initialize UEFI helpers");

    console::print_banner();

    // What there is to boot, and which of it. Without a \bang.cfg this is the
    // single Multiboot2 entry Bang always had, so an image made before there
    // was a config still boots without one.
    let mut cfg = config::load().unwrap_or_else(config::Config::builtin);

    // What is installed, alongside what was written down. Added after the
    // config so that a configured entry stays where the user put it, and its
    // `default` keeps meaning what it meant.
    if cfg.autodetect {
        for found in discover::scan() {
            let already = cfg.entries.iter().any(|e| match &e.target {
                config::Target::Chainload { path, device } => {
                    *path == found.path && device.unwrap_or(found.device) == found.device
                }
                _ => false,
            });
            if already {
                continue;
            }
            cfg.entries.push(config::Entry {
                title: found.title,
                target: config::Target::Chainload {
                    path: found.path,
                    device: Some(found.device),
                },
            });
        }
    }

    let choice = menu::choose(&cfg);
    let entry = &cfg.entries[choice];

    let (kernel_path, modules_path) = match &entry.target {
        config::Target::Multiboot { kernel, modules } => (kernel, modules),
        config::Target::Chainload { path, device } => {
            // Boot services stay up: the image we start needs them, and
            // exiting them is its business rather than ours. If it comes back,
            // so do we — with nothing having been torn down.
            return chain::boot(*device, path);
        }
        config::Target::Linux { kernel, initrd, cmdline } => {
            // As with a chainload, boot services stay up: the handover
            // protocol has the kernel exit them itself.
            return linux::boot(kernel, initrd.as_deref(), cmdline);
        }
    };

    // Load kernel ELF into memory
    let kernel = elf::load_kernel(kernel_path);

    println!(
        "[+] Entry: {:#x}, MB version: {}, mode: {:?}",
        kernel.entry_point, kernel.mb_version, kernel.boot_mode
    );

    // Load boot modules (must be before ExitBootServices)
    let mods = match modules_path {
        Some(dir) => modules::load_modules(dir),
        None => alloc::vec::Vec::new(),
    };

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
