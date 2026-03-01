use uefi::mem::memory_map::{MemoryMap, MemoryMapOwned, MemoryType};

use crate::gop::FbInfo;
use crate::modules::ModuleInfo;

// Multiboot1 constants
const MB_INFO_MEMORY: u32 = 0x0000_0001;
const MB_INFO_MODS: u32 = 0x0000_0008;
const MB_INFO_MEM_MAP: u32 = 0x0000_0040;

// Multiboot2 tag types
const MB2_TAG_TYPE_END: u32 = 0;
const MB2_TAG_TYPE_MODULE: u32 = 3;
const MB2_TAG_TYPE_MMAP: u32 = 6;
const MB2_TAG_TYPE_FRAMEBUFFER: u32 = 8;

/// Maximum number of modules supported.
const MAX_MODULES: usize = 32;

/// Static buffer for Multiboot1 info — must survive ExitBootServices.
static mut MB1_INFO: MultibootInfo = MultibootInfo {
    flags: 0,
    mem_lower: 0,
    mem_upper: 0,
    boot_device: 0,
    cmdline: 0,
    mods_count: 0,
    mods_addr: 0,
    syms: [0; 4],
    mmap_length: 0,
    mmap_addr: 0,
};

/// Static buffer for Multiboot1 memory map entries.
static mut MB1_MMAP: [MultibootMmapEntry; 256] = [MultibootMmapEntry {
    size: 0,
    base_addr: 0,
    length: 0,
    entry_type: 0,
}; 256];

/// Static buffer for Multiboot2 boot info (8192 bytes, 8-byte aligned).
#[repr(align(8))]
struct Mb2Buffer([u8; 8192]);

static mut MB2_INFO: Mb2Buffer = Mb2Buffer([0u8; 8192]);

/// Static buffer for Multiboot1 module entries (16 bytes each).
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct MultibootModule {
    mod_start: u32,
    mod_end: u32,
    string: u32,
    reserved: u32,
}

static mut MB1_MODULES: [MultibootModule; MAX_MODULES] = [MultibootModule {
    mod_start: 0,
    mod_end: 0,
    string: 0,
    reserved: 0,
}; MAX_MODULES];

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct MultibootInfo {
    flags: u32,
    mem_lower: u32,
    mem_upper: u32,
    boot_device: u32,
    cmdline: u32,
    mods_count: u32,
    mods_addr: u32,
    syms: [u32; 4],
    mmap_length: u32,
    mmap_addr: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct MultibootMmapEntry {
    size: u32,
    base_addr: u64,
    length: u64,
    entry_type: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Mb2MmapEntry {
    base_addr: u64,
    length: u64,
    entry_type: u32,
    reserved: u32,
}

/// Convert EFI memory type to Multiboot memory type.
fn efi_memtype_to_mb(efi_type: MemoryType) -> u32 {
    match efi_type {
        MemoryType::CONVENTIONAL
        | MemoryType::LOADER_CODE
        | MemoryType::LOADER_DATA
        | MemoryType::BOOT_SERVICES_CODE
        | MemoryType::BOOT_SERVICES_DATA => 1, // available
        MemoryType::ACPI_RECLAIM => 3,
        MemoryType::ACPI_NON_VOLATILE => 4,
        _ => 2, // reserved
    }
}

/// Build Multiboot1 boot info from the post-ExitBootServices memory map.
/// Returns the physical address of the MBI structure.
///
/// # Safety
/// Must be called after `exit_boot_services` — writes into static buffers.
pub unsafe fn build_mb1_info(memory_map: &MemoryMapOwned, modules: &[ModuleInfo]) -> u32 {
    let mbi = &raw mut MB1_INFO;
    let mmap = &raw mut MB1_MMAP;
    let mods = &raw mut MB1_MODULES;

    // Zero the structures
    core::ptr::write_bytes(mbi, 0, 1);
    core::ptr::write_bytes(mmap, 0, 1);

    let mut mem_lower: u64 = 0;
    let mut mem_upper: u64 = 0;
    let mut mmap_count: usize = 0;

    for desc in memory_map.entries() {
        if mmap_count >= 256 {
            break;
        }

        let base = desc.phys_start;
        let length = desc.page_count * 4096;
        let mb_type = efi_memtype_to_mb(desc.ty);

        let ent = &mut (*mmap)[mmap_count];
        ent.size = (core::mem::size_of::<MultibootMmapEntry>() - 4) as u32;
        ent.base_addr = base;
        ent.length = length;
        ent.entry_type = mb_type;
        mmap_count += 1;

        if mb_type == 1 {
            if base < 0x10_0000 {
                mem_lower += length / 1024;
            } else if base == 0x10_0000 || (base > 0x10_0000 && mem_upper > 0) {
                mem_upper += length / 1024;
            }
        }
    }

    if mem_lower > 640 {
        mem_lower = 640;
    }

    let mut flags = MB_INFO_MEMORY | MB_INFO_MEM_MAP;

    // Populate module entries
    let mod_count = modules.len().min(MAX_MODULES);
    if mod_count > 0 {
        for i in 0..mod_count {
            let m = &modules[i];
            (*mods)[i].mod_start = m.phys_start as u32;
            (*mods)[i].mod_end = (m.phys_start + m.size) as u32;
            (*mods)[i].string = m.name_ptr;
            (*mods)[i].reserved = 0;
        }
        flags |= MB_INFO_MODS;
    }

    (*mbi).flags = flags;
    (*mbi).mem_lower = mem_lower as u32;
    (*mbi).mem_upper = mem_upper as u32;
    (*mbi).mods_count = mod_count as u32;
    (*mbi).mods_addr = if mod_count > 0 {
        (*mods).as_ptr() as u32
    } else {
        0
    };
    (*mbi).mmap_length = (mmap_count * core::mem::size_of::<MultibootMmapEntry>()) as u32;
    (*mbi).mmap_addr = (*mmap).as_ptr() as u32;

    mbi as u32
}

/// Build Multiboot2 boot info from the post-ExitBootServices memory map.
/// Returns the physical address of the MB2 info buffer.
///
/// # Safety
/// Must be called after `exit_boot_services` — writes into static buffers.
pub unsafe fn build_mb2_info(
    memory_map: &MemoryMapOwned,
    fb: Option<&FbInfo>,
    modules: &[ModuleInfo],
) -> u32 {
    let buf = &raw mut MB2_INFO;
    let out = (*buf).0.as_mut_ptr();
    let buf_size = 8192usize;

    core::ptr::write_bytes(out, 0, buf_size);

    // MB2 boot info fixed header: { u32 total_size, u32 reserved }
    let mut pos: usize = 8;

    // Memory map tag (type=6)
    let mmap_tag_start = pos;
    let entry_size = core::mem::size_of::<Mb2MmapEntry>() as u32;

    // Tag type
    write_u32(out, pos, MB2_TAG_TYPE_MMAP);
    pos += 4;
    // Placeholder for tag size
    let tag_size_offset = pos;
    pos += 4;
    // entry_size
    write_u32(out, pos, entry_size);
    pos += 4;
    // entry_version
    write_u32(out, pos, 0);
    pos += 4;

    // Write mmap entries
    for desc in memory_map.entries() {
        if pos + core::mem::size_of::<Mb2MmapEntry>() > buf_size {
            break;
        }

        write_u64(out, pos, desc.phys_start);
        write_u64(out, pos + 8, desc.page_count * 4096);
        write_u32(out, pos + 16, efi_memtype_to_mb(desc.ty));
        write_u32(out, pos + 20, 0); // reserved

        pos += core::mem::size_of::<Mb2MmapEntry>();
    }

    // Fill in mmap tag size
    write_u32(out, tag_size_offset, (pos - mmap_tag_start) as u32);

    // Align to 8 bytes
    pos = (pos + 7) & !7;

    // Module tags (type=3) — one per loaded module
    for m in modules {
        // Compute name length from the null-terminated string at name_ptr
        let name_addr = m.name_ptr as *const u8;
        let mut name_len: usize = 0;
        while *name_addr.add(name_len) != 0 {
            name_len += 1;
        }
        // Tag: type(4) + size(4) + mod_start(4) + mod_end(4) + string(name_len+1)
        let tag_size = 4 + 4 + 4 + 4 + name_len + 1;
        if pos + tag_size > buf_size {
            break;
        }

        write_u32(out, pos, MB2_TAG_TYPE_MODULE);
        write_u32(out, pos + 4, tag_size as u32);
        write_u32(out, pos + 8, m.phys_start as u32);
        write_u32(out, pos + 12, (m.phys_start + m.size) as u32);

        // Copy name string (including null terminator)
        core::ptr::copy_nonoverlapping(name_addr, out.add(pos + 16), name_len + 1);

        pos += tag_size;
        // Align to 8 bytes
        pos = (pos + 7) & !7;
    }

    // Framebuffer tag (type=8) if GOP info is available
    if let Some(fb) = fb {
        let fb_tag_size: usize = 37;
        if pos + fb_tag_size <= buf_size {
            write_u32(out, pos + 0x00, MB2_TAG_TYPE_FRAMEBUFFER);
            write_u32(out, pos + 0x04, fb_tag_size as u32);
            write_u64(out, pos + 0x08, fb.addr);
            write_u32(out, pos + 0x10, fb.pitch);
            write_u32(out, pos + 0x14, fb.width);
            write_u32(out, pos + 0x18, fb.height);
            *out.add(pos + 0x1C) = fb.bpp;
            *out.add(pos + 0x1D) = fb.fb_type;
            *out.add(pos + 0x1E) = 0; // reserved
            *out.add(pos + 0x1F) = fb.red_pos;
            *out.add(pos + 0x20) = fb.red_size;
            *out.add(pos + 0x21) = fb.green_pos;
            *out.add(pos + 0x22) = fb.green_size;
            *out.add(pos + 0x23) = fb.blue_pos;
            *out.add(pos + 0x24) = fb.blue_size;
            pos += fb_tag_size;
        }

        // Align to 8 bytes
        pos = (pos + 7) & !7;
    }

    // Terminating tag (type=0, size=8)
    if pos + 8 <= buf_size {
        write_u32(out, pos, MB2_TAG_TYPE_END);
        write_u32(out, pos + 4, 8);
        pos += 8;
    }

    // Fill in the fixed header
    write_u32(out, 0, pos as u32); // total_size
    write_u32(out, 4, 0); // reserved

    out as u32
}

/// Write a u32 at a byte offset (unaligned).
unsafe fn write_u32(base: *mut u8, offset: usize, val: u32) {
    core::ptr::write_unaligned(base.add(offset) as *mut u32, val);
}

/// Write a u64 at a byte offset (unaligned).
unsafe fn write_u64(base: *mut u8, offset: usize, val: u64) {
    core::ptr::write_unaligned(base.add(offset) as *mut u64, val);
}
