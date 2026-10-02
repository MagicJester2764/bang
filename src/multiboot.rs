use uefi::boot::{self, AllocateType};
use uefi::mem::memory_map::{MemoryMap, MemoryMapOwned, MemoryType};
use uefi::table::cfg::ConfigTableEntry;

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
const MB2_TAG_TYPE_ACPI_OLD: u32 = 14;
const MB2_TAG_TYPE_ACPI_NEW: u32 = 15;

/// The ACPI root pointer, as the firmware has it: twenty bytes of the first
/// revision, thirty-six of the second.
const RSDP_V1: usize = 20;
const RSDP_V2: usize = 36;

/// A copy of the firmware's ACPI root pointer, taken while there was a
/// firmware to ask.
///
/// On a machine started by UEFI this is the only way a kernel learns where
/// the ACPI tables are: the pointer is in the firmware's configuration
/// table and nowhere a kernel could look for it afterwards. (A BIOS leaves
/// it in memory to be searched for; UEFI does not have to.) Without it a
/// kernel does not know how many processors there are.
#[derive(Clone, Copy)]
pub struct Rsdp {
    bytes: [u8; RSDP_V2],
    len: usize,
}

/// Find the ACPI root pointer: the second revision's if the firmware has
/// one, the first's if not.
///
/// Before boot services are exited. What the table says about itself is not
/// believed further than its signature and its length: checking it is the
/// kernel's, which is who will follow it.
pub fn find_rsdp() -> Option<Rsdp> {
    let at = uefi::system::with_config_table(|entries| {
        let find = |guid| entries.iter().find(|e| e.guid == guid).map(|e| e.address as *const u8);
        find(ConfigTableEntry::ACPI2_GUID).or_else(|| find(ConfigTableEntry::ACPI_GUID))
    })?;
    if at.is_null() {
        return None;
    }
    // SAFETY: the firmware published this address as an RSDP, which is at
    // least twenty bytes; the rest is read only if those bytes say it is
    // there.
    unsafe {
        if core::slice::from_raw_parts(at, 8) != b"RSD PTR " {
            return None;
        }
        let revision = *at.add(15);
        let len = if revision >= 2 {
            let said = core::ptr::read_unaligned(at.add(20) as *const u32) as usize;
            if said < RSDP_V2 { RSDP_V1 } else { RSDP_V2 }
        } else {
            RSDP_V1
        };
        let mut bytes = [0u8; RSDP_V2];
        core::ptr::copy_nonoverlapping(at, bytes.as_mut_ptr(), len);
        Some(Rsdp { bytes, len })
    }
}

/// Maximum number of modules supported.
const MAX_MODULES: usize = 32;

/// What the kernel is told is written here: four pages below four
/// gigabytes, asked for while there is a firmware to ask ([`reserve`]).
///
/// It used to be written into this program's own memory, and the kernel
/// handed its address — which a Multiboot kernel is handed in a register
/// thirty-two bits wide. Firmware loads a program wherever it has room, and
/// on a machine with more than four gigabytes that is above them: the
/// address lost its top half on the way, and the kernel read what it was
/// told out of memory nothing had written to. It found no memory map, and
/// so no memory.
static mut INFO_AT: usize = 0;
const INFO_PAGES: usize = 4;
const INFO_SIZE: usize = INFO_PAGES * 4096;

/// Where each part of a Multiboot1 structure goes in that memory: the
/// structure itself, the modules, their names, and the memory map last.
const MB1_MODULES_AT: usize = 64;
const MB1_NAMES_AT: usize = MB1_MODULES_AT + MAX_MODULES * core::mem::size_of::<MultibootModule>();
const MB1_NAME_MAX: usize = 64;
const MB1_MMAP_AT: usize = MB1_NAMES_AT + MAX_MODULES * MB1_NAME_MAX;
const MB1_MMAP_MAX: usize = (INFO_SIZE - MB1_MMAP_AT) / core::mem::size_of::<MultibootMmapEntry>();

/// Set aside the memory a kernel's boot information is written to. False if
/// the firmware has none below four gigabytes to give, in which case there
/// is nothing a Multiboot kernel can be told.
///
/// Before boot services are exited.
pub fn reserve() -> bool {
    match boot::allocate_pages(AllocateType::MaxAddress(0xFFFF_FFFF), MemoryType::LOADER_DATA, INFO_PAGES) {
        Ok(at) => {
            unsafe { INFO_AT = at.as_ptr() as usize };
            true
        }
        Err(_) => false,
    }
}

/// Multiboot1 module entries (16 bytes each).
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct MultibootModule {
    mod_start: u32,
    mod_end: u32,
    string: u32,
    reserved: u32,
}

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
/// Must be called after [`reserve`] has succeeded and after
/// `exit_boot_services` — writes into the memory that set aside.
pub unsafe fn build_mb1_info(memory_map: &MemoryMapOwned, modules: &[ModuleInfo]) -> u32 {
    let base_at = INFO_AT as *mut u8;
    core::ptr::write_bytes(base_at, 0, INFO_SIZE);
    let mbi = base_at as *mut MultibootInfo;
    let mods = base_at.add(MB1_MODULES_AT) as *mut MultibootModule;
    let names = base_at.add(MB1_NAMES_AT);
    let mmap = base_at.add(MB1_MMAP_AT) as *mut MultibootMmapEntry;

    let mut mem_lower: u64 = 0;
    let mut mem_upper: u64 = 0;
    let mut mmap_count: usize = 0;

    for desc in memory_map.entries() {
        if mmap_count >= MB1_MMAP_MAX {
            break;
        }

        let base = desc.phys_start;
        let length = desc.page_count * 4096;
        let mb_type = efi_memtype_to_mb(desc.ty);

        mmap.add(mmap_count).write_unaligned(MultibootMmapEntry {
            size: (core::mem::size_of::<MultibootMmapEntry>() - 4) as u32,
            base_addr: base,
            length,
            entry_type: mb_type,
        });
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
            // A module's name is kept in this program's own memory, which
            // may be above four gigabytes; the kernel is handed a copy that
            // is not.
            let name = names.add(i * MB1_NAME_MAX);
            let from = m.name_ptr as *const u8;
            let mut len = 0;
            while len < MB1_NAME_MAX - 1 && *from.add(len) != 0 {
                *name.add(len) = *from.add(len);
                len += 1;
            }
            // Multiboot1 fixes these fields at 32 bits, so the narrowing is
            // mandated by the format rather than accidental; a module is
            // loaded below four gigabytes for that reason (`modules.rs`).
            mods.add(i).write_unaligned(MultibootModule {
                mod_start: m.phys_start as u32,
                mod_end: (m.phys_start + m.size) as u32,
                string: name as u32,
                reserved: 0,
            });
        }
        flags |= MB_INFO_MODS;
    }

    mbi.write_unaligned(MultibootInfo {
        flags,
        mem_lower: mem_lower as u32,
        mem_upper: mem_upper as u32,
        boot_device: 0,
        cmdline: 0,
        mods_count: mod_count as u32,
        mods_addr: if mod_count > 0 { mods as u32 } else { 0 },
        syms: [0; 4],
        mmap_length: (mmap_count * core::mem::size_of::<MultibootMmapEntry>()) as u32,
        mmap_addr: mmap as u32,
    });

    mbi as u32
}

/// Build Multiboot2 boot info from the post-ExitBootServices memory map.
/// Returns the physical address of the MB2 info buffer.
///
/// # Safety
/// Must be called after [`reserve`] has succeeded and after
/// `exit_boot_services` — writes into the memory that set aside.
pub unsafe fn build_mb2_info(
    memory_map: &MemoryMapOwned,
    fb: Option<&FbInfo>,
    modules: &[ModuleInfo],
    rsdp: Option<&Rsdp>,
) -> u32 {
    let out = INFO_AT as *mut u8;
    let buf_size = INFO_SIZE;

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

    // The ACPI root pointer (type 15 for the second revision's, 14 for the
    // first's): a copy of the structure itself, as the specification has it.
    if let Some(rsdp) = rsdp {
        let tag_size = 8 + rsdp.len;
        if pos + tag_size + 8 <= buf_size {
            let kind = if rsdp.len == RSDP_V2 { MB2_TAG_TYPE_ACPI_NEW } else { MB2_TAG_TYPE_ACPI_OLD };
            write_u32(out, pos, kind);
            write_u32(out, pos + 4, tag_size as u32);
            core::ptr::copy_nonoverlapping(rsdp.bytes.as_ptr(), out.add(pos + 8), rsdp.len);
            pos += tag_size;
            pos = (pos + 7) & !7;
        }
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
